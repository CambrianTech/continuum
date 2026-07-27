//! `serving/pin` — FORCE this host to serve a specific base model (the hard pin).
//! The "future verb" [`serving/load`](super::load) names: promote/demote, made
//! explicit. Where `serving/load` only PERMITS (lifts an OFF pin and lets the
//! autonomic planner decide), `serving/pin` FORCES — it intersects the planner's
//! candidate set to exactly the named model, so the next reconcile swaps the live
//! `llama-server` to it. [`serving/unpin`](super::unpin) releases it back to
//! autonomic best-fit.
//!
//! ## Fail loud, never fall back
//!
//! The pin is fit-gated BEFORE it is set, so a force-serve that can't run is
//! refused at the call, never silently downgraded to a different model:
//!
//! - Unknown model id ⇒ [`CommandError::NotFound`].
//! - In the catalog but not downloaded (no GGUF on disk) ⇒ [`CommandError::Denied`]
//!   naming the fix (`models/pull`).
//! - On disk but won't fit a serving lane in the current budget ⇒
//!   [`CommandError::Denied`] naming the shortfall (needs ~XGB, host has ~YGB).
//!
//! Only a model that fits a lane at pin time is ever pinned. Budget can still
//! shift under a live pin (a game grabs VRAM); the daemon's plan then degrades
//! honestly (`fits_on_gpu = false`) rather than over-committing — it does not
//! silently un-pin.
//!
//! ## Single-serve honesty
//!
//! One host serves ONE base model (one supervised `llama-server`); per-persona
//! differentiation is the LoRA genome paged over that base, not a second base.
//! So on a single-serve host "pin a persona to model Y" honestly re-homes the
//! shared base for everyone on this host. Per-persona base divergence arrives
//! with multi-base serving; this verb is the host-level mechanism it will compose.
//!
//! ## Gating
//!
//! `Privileged` — it dictates what occupies GPU memory on this node.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::sync::watch;
use ts_rs::TS;

use crate::inference::llama_server::ServingSnapshot;
use crate::model_registry::live::{Availability, ModelCatalog};
use crate::modules::serving_daemon::PinFitChecker;
use crate::sdk_codegen::CommandError;

/// Which model to force-serve on this host.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/model_registry/ServingPinParams.ts")]
pub struct ServingPinParams {
    /// The model id as it appears in `models/list`. Must be downloaded and must
    /// fit a serving lane on this host — fails loud otherwise.
    pub model_id: String,
}

/// What `serving/pin` decided: the model now forced, what was serving before, and
/// the fit numbers it was gated on (so the caller sees the headroom, not just a
/// yes).
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/model_registry/PinReport.ts")]
pub struct PinReport {
    /// The model id now pinned — the daemon's next reconcile swaps the live
    /// server to it.
    pub pinned_model: String,
    /// What the host was serving at pin time (`None` if nothing was live). The
    /// promote/demote "from".
    pub previous_model: Option<String>,
    /// The host-fit served context window the planner will use for the pin.
    pub served_context_window: u32,
    /// The model's weight footprint, GB (what it needs).
    pub weights_gb: f32,
    /// The host's usable serving budget at pin time, GB (what it has).
    pub budget_gb: f32,
    /// Human-readable summary.
    pub detail: String,
}

fn gb(bytes: u64) -> f32 {
    (bytes as f64 / 1_000_000_000.0) as f32
}

crate::action_command! {
    /// Force this host to serve a specific base model — the hard pin behind
    /// promote/demote. Unlike serving/load (which only permits and lets the
    /// planner decide), this FORCES the named model: the planner's candidates are
    /// intersected to it and the next reconcile swaps the live server. Fit-gated:
    /// fails loud if the model is unknown, not downloaded, or won't fit a lane in
    /// the current budget — never silently serves a different model. Release with
    /// serving/unpin. On a single-serve host this re-homes the shared base for
    /// everyone on the node.
    pub struct ServingPin {
        pin: watch::Sender<Option<String>>,
        fit: PinFitChecker,
        catalog: Arc<ModelCatalog>,
        serving: watch::Receiver<ServingSnapshot>,
    }
    name: "serving/pin",
    access: Privileged,
    params: ServingPinParams,
    output: PinReport,
    run(this, _ctx, p) => {
        // 1. The model must exist in the live universe — never pin a typo.
        let snap = this.catalog.snapshot();
        let live = snap.get(&p.model_id).ok_or_else(|| {
            CommandError::NotFound(format!(
                "unknown model id '{}' — call models/list to see the live universe",
                p.model_id
            ))
        })?;

        // 2. Fit-gate against the live budget BEFORE pinning — the same math the
        //    autonomic tick uses, so this verdict agrees with what the reconcile
        //    would actually do. Refuse loud rather than pin something unservable.
        let verdict = (this.fit)(&live.model);
        let plan = match verdict.plan {
            Some(plan) if plan.fits_on_gpu => plan,
            Some(_) => {
                return Err(CommandError::Denied(format!(
                    "model '{}' needs ~{:.1}GB of weights but this host's serving budget is only \
                     ~{:.1}GB right now — it won't fit a lane. Pin a smaller model, free VRAM, or \
                     leave it to the autonomic planner.",
                    p.model_id,
                    gb(verdict.weights_bytes),
                    gb(verdict.budget_bytes),
                )));
            }
            None => {
                let why = if live.status.availability == Availability::Ready {
                    "has no servable GGUF on disk"
                } else {
                    "is in the catalog but not downloaded"
                };
                return Err(CommandError::Denied(format!(
                    "model '{}' {} — run models/pull '{}' first, then pin it",
                    p.model_id, why, p.model_id
                )));
            }
        };

        // 3. The promote/demote "from" — what is live at the moment we pin.
        let previous_model = this.serving.borrow().active_model.clone();

        // 4. Set the pin. live_candidates now intersects to this model; the
        //    daemon's next tick reconciles the live server to it (sub-second to a
        //    few seconds; observe readiness via serving/status).
        this.pin.send_replace(Some(p.model_id.clone()));

        let detail = match &previous_model {
            Some(prev) if prev == &p.model_id => format!(
                "model '{}' is already live; pin set so the planner can no longer move off it",
                p.model_id
            ),
            Some(prev) => format!(
                "pinned '{}' (was serving '{}'); the daemon will swap the live server on its next tick",
                p.model_id, prev
            ),
            None => format!(
                "pinned '{}' (nothing was serving); the daemon will bring it up on its next tick",
                p.model_id
            ),
        };

        Ok(PinReport {
            pinned_model: p.model_id,
            previous_model,
            served_context_window: plan.served_context_window,
            weights_gb: gb(verdict.weights_bytes),
            budget_gb: gb(verdict.budget_bytes),
            detail,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cognition::serving_plan::ServingPlan;
    use crate::modules::serving_daemon::{PinFit, PinFitChecker};
    use crate::sdk_codegen::{ActionCommand, Ctx};

    fn catalog_and_id() -> (Arc<ModelCatalog>, String) {
        let reg = crate::model_registry::catalog::registry().expect("Rust catalog must validate");
        let catalog = Arc::new(ModelCatalog::from_registry(&reg));
        let id = catalog
            .snapshot()
            .models
            .keys()
            .next()
            .expect("registry has at least one model")
            .clone();
        (catalog, id)
    }

    fn plan(fits: bool) -> ServingPlan {
        ServingPlan {
            base_model_id: "x".into(),
            served_context_window: 8192,
            lanes: 1,
            grid_overflow_lanes: 0,
            resident_models: 1,
            fits_on_gpu: fits,
            rationale: "test".into(),
        }
    }

    /// A fit-checker that returns a fixed verdict regardless of the model.
    fn fixed_fit(verdict: PinFit) -> PinFitChecker {
        // PinFit is not Clone (carries a ServingPlan); rebuild it per call from its parts.
        let plan = verdict.plan.clone();
        let weights = verdict.weights_bytes;
        let budget = verdict.budget_bytes;
        Arc::new(move |_m| PinFit {
            plan: plan.clone(),
            weights_bytes: weights,
            budget_bytes: budget,
        })
    }

    fn build(fit: PinFitChecker, catalog: Arc<ModelCatalog>) -> (ServingPin, watch::Receiver<Option<String>>) {
        let (pin, pin_rx) = watch::channel(None);
        let (_tx, serving) = watch::channel(ServingSnapshot::empty());
        (ServingPin { pin, fit, catalog, serving }, pin_rx)
    }

    // what this catches: the wire name mirrors the file path — the routing
    // contract that lets the typed registry dispatch `serving/pin` to this command.
    #[test]
    fn name_mirrors_path() {
        assert_eq!(ServingPin::NAME, "serving/pin");
    }

    // what this catches: an unknown model id is refused loud as NotFound BEFORE
    // any pin is set — a typo can never silently pin nothing.
    #[tokio::test]
    async fn unknown_model_is_not_found() {
        let (catalog, _id) = catalog_and_id();
        let (cmd, pin_rx) = build(fixed_fit(PinFit { plan: Some(plan(true)), weights_bytes: 0, budget_bytes: 0 }), catalog);
        let err = cmd
            .run(&Ctx::default(), ServingPinParams { model_id: "no-such-model".into() })
            .await
            .expect_err("unknown id must fail loud");
        assert!(matches!(err, CommandError::NotFound(_)));
        assert!(pin_rx.borrow().is_none(), "no pin set on a rejected request");
    }

    // what this catches: a real model that won't fit a lane is refused loud as
    // Denied and the pin is NOT set — the force-serve never silently falls back to
    // a different model.
    #[tokio::test]
    async fn model_that_wont_fit_is_denied() {
        let (catalog, id) = catalog_and_id();
        let (cmd, pin_rx) = build(
            fixed_fit(PinFit { plan: Some(plan(false)), weights_bytes: 30_000_000_000, budget_bytes: 8_000_000_000 }),
            catalog,
        );
        let err = cmd
            .run(&Ctx::default(), ServingPinParams { model_id: id })
            .await
            .expect_err("over-budget model must be denied");
        assert!(matches!(err, CommandError::Denied(_)));
        assert!(pin_rx.borrow().is_none(), "the pin must NOT be set when the model won't fit");
    }

    // what this catches: a model with no servable artifact (plan None) is denied
    // with the models/pull fix named — never pinned.
    #[tokio::test]
    async fn not_downloaded_is_denied() {
        let (catalog, id) = catalog_and_id();
        let (cmd, pin_rx) = build(
            fixed_fit(PinFit { plan: None, weights_bytes: 0, budget_bytes: 8_000_000_000 }),
            catalog,
        );
        let err = cmd
            .run(&Ctx::default(), ServingPinParams { model_id: id })
            .await
            .expect_err("undownloaded model must be denied");
        assert!(matches!(err, CommandError::Denied(_)));
        assert!(pin_rx.borrow().is_none());
    }

    // what this catches: a fitting model IS pinned — the watch flips to Some(id),
    // which is exactly what live_candidates intersects on, and the report carries
    // the planner's served window.
    #[tokio::test]
    async fn fitting_model_is_pinned() {
        let (catalog, id) = catalog_and_id();
        let (cmd, pin_rx) = build(
            fixed_fit(PinFit { plan: Some(plan(true)), weights_bytes: 4_000_000_000, budget_bytes: 40_000_000_000 }),
            catalog,
        );
        let report = cmd
            .run(&Ctx::default(), ServingPinParams { model_id: id.clone() })
            .await
            .expect("fitting model pins");
        assert_eq!(report.pinned_model, id);
        assert_eq!(report.served_context_window, 8192);
        assert_eq!(pin_rx.borrow().as_deref(), Some(id.as_str()), "the pin watch carries the forced model");
    }
}
