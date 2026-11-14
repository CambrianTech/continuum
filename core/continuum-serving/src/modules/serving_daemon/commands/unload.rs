//! `serving/unload` — free a model's VRAM lane at runtime. The VRAM-axis "free",
//! counterpart to [`serving/load`](super::load) (and the sibling of the disk-axis
//! `models/remove`).
//!
//! ## What it does
//!
//! Pins a model OFF in the serving daemon's suppress-set. The daemon is
//! holistically in charge of VRAM, so unloading is not "kill a process" — it is an
//! edit to the exclude list the planner honors: once the id is suppressed, the
//! next plan recompute can no longer choose it, and the reconcile drops it
//! (relaunch to the next-best candidate, or publish empty) — the lane frees live,
//! no reboot. If the model was the one currently being served, this command waits
//! for the published [`ServingSnapshot`] to confirm the lane actually left it
//! before returning, so the caller knows the VRAM is free (e.g. before a
//! `models/remove`).
//!
//! ## The wait ends on an outcome, never a stopwatch
//!
//! The daemon's reconcile is the producer: the wait ends when its snapshot shows
//! the lane left the model ([`LaneRelease::Freed`]), when a later `serving/load`
//! takes the pin back ([`LaneRelease::Superseded`]), or when the daemon's channels
//! close ([`LaneRelease::DaemonGone`]). A caller that gives up first (the command
//! frame's own deadline) abandons only its wait: the pin is the daemon's intent
//! and stays applied (CONTINUUM-ENGINE.md RTOS rule 10).
//!
//! ## Fail loud
//!
//! Unknown model id ⇒ [`CommandError::NotFound`] (a typo'd pin that silently does
//! nothing is a lie). A superseded pin or a gone daemon ⇒ [`CommandError::Internal`]
//! naming which.
//!
//! ## Gating
//!
//! `Privileged` — it changes what occupies GPU memory on this node.

use crate::modules::serving_daemon::{ServingIntent, ServingIntentSnapshot};
use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::sync::watch;
use ts_rs::TS;

use crate::inference::llama_server::ServingSnapshot;
use crate::model_registry::live::ModelCatalog;
use crate::sdk_codegen::CommandError;

/// How the wait for a pinned-off model's lane ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LaneRelease {
    /// The published snapshot no longer serves the model: its VRAM is free.
    Freed,
    /// The model is no longer suppressed (a later `serving/load`): the lane may stay.
    Superseded,
    /// The daemon's snapshot or intent channel closed: no producer is left.
    DaemonGone,
}

/// Wait for the daemon to publish the model's release, or for the outcome that means it never will.
async fn lane_release(
    mut serving: watch::Receiver<ServingSnapshot>,
    mut intent: watch::Receiver<ServingIntentSnapshot>,
    model: &str,
) -> LaneRelease {
    tokio::select! {
        freed = serving.wait_for(|s| s.active_model.as_deref() != Some(model)) => {
            if freed.is_ok() { LaneRelease::Freed } else { LaneRelease::DaemonGone }
        }
        restored = intent.wait_for(|s| !s.suppressed.contains(model)) => {
            if restored.is_ok() { LaneRelease::Superseded } else { LaneRelease::DaemonGone }
        }
    }
}

/// Which model's VRAM lane to free.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/model_registry/ServingUnloadParams.ts"
)]
pub struct ServingUnloadParams {
    /// The model id as it appears in `models/list`. Fails loud if unknown. May be
    /// pinned off even if it is not the one currently served (a preemptive pin so
    /// the planner will not pick it); if it IS the served model, the command waits
    /// for the lane to free before returning.
    pub model_id: String,
}

/// What `serving/unload` did: whether the model was actually occupying a lane, and
/// what (if anything) the daemon is serving now that the lane is free.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/model_registry/UnloadReport.ts"
)]
pub struct UnloadReport {
    /// True if this model was the one being served when unload was called (its VRAM
    /// was actually reclaimed); false if it was pinned off preemptively.
    pub was_serving: bool,
    /// What the daemon is serving now that the lane freed — `None` if nothing is
    /// live (no other candidate fit the budget, or the node is intentionally idle).
    pub now_serving: Option<String>,
    /// Human-readable summary.
    pub detail: String,
}

crate::action_command! {
    /// Free a model's VRAM lane at runtime — no reboot. Pins the model OFF in the
    /// serving daemon's suppress-set so the planner drops it and the lane frees
    /// (relaunching to the next-best fit, or going idle). If the model was the one
    /// being served, waits for the lane to actually free before returning. The
    /// inverse of serving/load. Fails loud on an unknown model id. Returns whether
    /// VRAM was reclaimed and what is serving now.
    pub struct ServingUnload {
        intent: ServingIntent,
        serving: watch::Receiver<ServingSnapshot>,
        catalog: Arc<ModelCatalog>,
    }
    name: "serving/unload",
    access: Privileged,
    params: ServingUnloadParams,
    output: UnloadReport,
    run(this, _ctx, p) => {
        // 1. The model must exist in the live universe — never pin a typo.
        if this.catalog.snapshot().get(&p.model_id).is_none() {
            return Err(CommandError::NotFound(format!(
                "unknown model id '{}': it is not in the live model catalog",
                p.model_id
            )));
        }

        // 2. Was it the lane actually in use? (Determines whether we must wait for
        //    real VRAM to free, vs a preemptive pin of something not yet served.)
        let was_serving =
            this.serving.borrow().active_model.as_deref() == Some(p.model_id.as_str());

        // 3. Pin it OFF. Idempotent — re-unloading an already-pinned model is fine.
        this.intent.set_suppressed(&p.model_id, true, true).map_err(|error| CommandError::Denied(error.to_string()))?;

        // 4. If it held the lane, wait for the daemon's reconcile to actually free it
        //    before we claim success.
        if was_serving {
            match lane_release(this.serving.clone(), this.intent.subscribe(), &p.model_id).await {
                LaneRelease::Freed => {}
                LaneRelease::Superseded => {
                    return Err(CommandError::Internal(format!(
                        "model '{}' was pinned off, then loaded again before its lane freed",
                        p.model_id
                    )))
                }
                LaneRelease::DaemonGone => {
                    return Err(CommandError::Internal(format!(
                        "model '{}' pinned off, but the serving daemon stopped publishing before the lane freed; the pin remains applied",
                        p.model_id
                    )))
                }
            }
        }

        let now_serving = this.serving.borrow().active_model.clone();
        let detail = if was_serving {
            match &now_serving {
                Some(next) => format!(
                    "freed model '{}' from VRAM; daemon relaunched to '{next}'",
                    p.model_id
                ),
                None => format!(
                    "freed model '{}' from VRAM; no other candidate fit the budget — node is idle",
                    p.model_id
                ),
            }
        } else {
            format!(
                "model '{}' pinned off (it was not the served lane); planner will not pick it until serving/load",
                p.model_id
            )
        };

        Ok(UnloadReport {
            was_serving,
            now_serving,
            detail,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use futures::FutureExt;
    use std::collections::HashSet;

    // what this catches: the wire name mirrors the file path — the routing
    // contract that lets the typed registry dispatch `serving/unload` to this
    // command.
    #[test]
    fn name_mirrors_path() {
        use crate::sdk_codegen::ActionCommand;
        assert_eq!(ServingUnload::NAME, "serving/unload");
    }

    fn serving(model: Option<&str>) -> ServingSnapshot {
        ServingSnapshot { active_model: model.map(str::to_string), ..ServingSnapshot::empty() }
    }

    fn suppressing(models: &[&str]) -> ServingIntentSnapshot {
        ServingIntentSnapshot {
            revision: 1,
            pinned: None,
            suppressed: Arc::new(models.iter().map(|m| m.to_string()).collect::<HashSet<_>>()),
        }
    }

    // what this catches: a stopwatch deciding the unload. The wait stays open while the
    // daemon still serves the model, and the reconcile's publication of its release is
    // what completes it.
    #[tokio::test]
    async fn the_daemons_release_publication_completes_the_wait() {
        let (serving_tx, serving_rx) = watch::channel(serving(Some("m")));
        let (_intent_tx, intent_rx) = watch::channel(suppressing(&["m"]));
        let mut wait = Box::pin(lane_release(serving_rx, intent_rx, "m"));
        assert!((&mut wait).now_or_never().is_none(), "still served: no outcome yet");
        serving_tx.send_replace(serving(Some("next")));
        assert_eq!(wait.await, LaneRelease::Freed);
    }

    // what this catches: a pin taken back or a daemon gone read as a freed lane, or
    // waited on forever. Each is its own typed outcome.
    #[tokio::test]
    async fn a_reload_and_a_gone_daemon_are_typed_outcomes() {
        let (_serving_tx, serving_rx) = watch::channel(serving(Some("m")));
        let (intent_tx, intent_rx) = watch::channel(suppressing(&["m"]));
        let mut wait = Box::pin(lane_release(serving_rx.clone(), intent_rx, "m"));
        assert!((&mut wait).now_or_never().is_none());
        intent_tx.send_replace(suppressing(&[]));
        assert_eq!(wait.await, LaneRelease::Superseded);

        let (serving_tx, serving_rx) = watch::channel(serving(Some("m")));
        let (_intent_tx, intent_rx) = watch::channel(suppressing(&["m"]));
        let mut wait = Box::pin(lane_release(serving_rx, intent_rx, "m"));
        assert!((&mut wait).now_or_never().is_none());
        drop(serving_tx);
        assert_eq!(wait.await, LaneRelease::DaemonGone);
    }
}
