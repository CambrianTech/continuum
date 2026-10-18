//! May one more model be served here: the `serving/pin` fit-gate (a SWAP — the
//! incumbent's weights credit back) and the second-engine lane admission authority the
//! ephemeral spawn adapter consults (CO-RESIDENCE — the incumbent is spent). Both over
//! the same live budget and footprint estimator the planner uses.

use super::*;

/// The verdict for force-serving one specific model on this host RIGHT NOW —
/// what [`ServingDaemonModule::pin_fit_checker`] returns and `serving/pin` gates
/// on. Carries the numbers so the command can fail loud with a NAMED shortfall
/// ("needs ~XGB, host budget is ~YGB") rather than a bare refusal.
pub struct PinFit {
    /// The host-fit plan for the model alone. `None` when the model has no GGUF
    /// on disk (not downloaded → not servable). `Some` with `fits_on_gpu = false`
    /// when it is on disk but won't fit a lane in the current budget.
    pub plan: Option<ServingPlan>,
    /// The model's on-disk weight bytes (0 when not downloaded).
    pub weights_bytes: u64,
    /// The host's usable serving budget right now, in bytes.
    pub budget_bytes: u64,
}

/// The cloneable synchronous fit-gate the `serving/pin` command holds — given a
/// candidate [`Model`], can this host force-serve it right now. Built by
/// [`ServingDaemonModule::pin_fit_checker`] over the live budget so it agrees
/// with the autonomic planner by construction.
pub type PinFitChecker = Arc<dyn Fn(&Model) -> PinFit + Send + Sync>;

/// Pure pin fit-decision — split from the live-budget read so it is unit-testable.
/// A pin SWAPS: `serve()` kills the incumbent llama-server child, THEN launches the
/// candidate — never co-resident — so the candidate only needs to fit AFTER the
/// incumbent's VRAM is reclaimed. `base` is the live budget WITH the incumbent's
/// weights still counted as USED; crediting `incumbent`'s weights back models the
/// eviction. Without it, a swap DOWN to a model that fits alone but not alongside
/// the outgoing one is falsely denied (glass-boxed 2026-07-21: pin Devstral 14.3GB
/// refused at "budget ~12.1GB" while a 32B teacher was resident, though evicting it
/// frees ~20GB — the stronger-teacher swap-and-back the Academy needs). WEIGHTS
/// only (deterministic): the incumbent's variable KV is freed too, so this stays
/// conservative — a `fits_on_gpu` verdict here always holds in the real
/// post-eviction budget. `candidate = None` ⇒ no GGUF on disk ⇒ not servable.
/// Does another engine fit beside what is already on the device? Pure so the
/// arithmetic is assertable without a GPU, a daemon, or a live budget — the same
/// split `pin_fit_decision` makes one screen below.
///
/// CO-RESIDENCE, not a swap: an ephemeral lane joins the live lane rather than
/// replacing it, so the incumbent is SPENT, never credited back. That single sign flip
/// is the whole difference from a pin's fit check, and getting it backwards would
/// admit exactly the second engine this exists to refuse.
///
/// A refusal states every term. An operator who cannot see which number blocked them
/// deletes the check ([[stdout-is-never-a-transport]] — the fact belongs in the
/// message that carries the decision).
pub(super) fn lane_admission_decision(
    budget_bytes: u64,
    want_bytes: u64,
    live_bytes: u64,
    sibling_bytes: u64,
    sibling_count: usize,
) -> crate::inference::llama_server::LaneAdmission {
    use crate::inference::llama_server::LaneAdmission;
    let committed = live_bytes.saturating_add(sibling_bytes);
    let headroom = budget_bytes.saturating_sub(committed);
    if want_bytes <= headroom {
        return LaneAdmission::Admit;
    }
    let gb = |b: u64| b as f64 / 1e9;
    LaneAdmission::Refuse(format!(
        "another {:.1} GB engine does not fit: budget {:.1} GB, already committed {:.1} GB          (live lane {:.1} GB + {} ephemeral sibling(s) {:.1} GB), headroom {:.1} GB.          Co-residence would time-slice one device, not add capacity.",
        gb(want_bytes),
        gb(budget_bytes),
        gb(committed),
        gb(live_bytes),
        sibling_count,
        gb(sibling_bytes),
        gb(headroom),
    ))
}

pub(super) fn pin_fit_decision(
    mut base: HostBudget,
    candidate: Option<ModelFootprint>,
    incumbent: Option<&ModelFootprint>,
) -> PinFit {
    if let Some(inc) = incumbent {
        base.usable_bytes = base.usable_bytes.saturating_add(inc.weights_bytes);
    }
    let budget_bytes = base.usable_bytes;
    let weights_bytes = candidate.as_ref().map(|f| f.weights_bytes).unwrap_or(0);
    // Fit verdict at ONE lane — "can this model hold a lane at all"; the live plan
    // sizes lanes from demand separately. footprint None → not servable (plan None);
    // footprint Some but over budget → plan_serving degrades with fits_on_gpu=false,
    // which `serving/pin` reads to refuse loud.
    let plan = candidate
        .and_then(|f| plan_serving(&base, std::slice::from_ref(&f), &ServingDemand::new(1, None)));
    PinFit {
        plan,
        weights_bytes,
        budget_bytes,
    }
}

impl ServingDaemonModule {
    /// The synchronous fit-gate `serving/pin` holds: given a candidate model,
    /// does it fit a serving lane on THIS host right now? Built from the same
    /// [`live_host_budget`] + [`footprint_for`] + [`plan_serving`] the autonomic
    /// tick uses, so the pin's "will it fit" verdict can never disagree with what
    /// the next reconcile would actually do. The command refuses loud BEFORE
    /// pinning when this says no — never a silent best-fit fallback.
    pub fn pin_fit_checker(&self) -> PinFitChecker {
        let system = self.system.clone();
        let resource_daemon = self.resource_daemon.clone();
        // Read the incumbent so a pin's fit-check models the SWAP it performs, not
        // co-residence (below).
        let serving_tx = self.serving_tx.clone();
        let model_resolver = self.model_resolver.clone();
        Arc::new(move |model: &Model| {
            let base = live_host_budget(&system, &resource_daemon);
            // The incumbent a pin would EVICT — its footprint credits back into the
            // budget (see [`pin_fit_decision`]). `live_host_budget` reads the live
            // system, so the eviction-crediting fit logic is split into a pure,
            // unit-testable helper below.
            let incumbent = serving_tx
                .borrow()
                .active_model
                .clone()
                .and_then(|id| (model_resolver)(&id))
                .and_then(|m| footprint_for(&m));
            pin_fit_decision(base, footprint_for(model), incumbent.as_ref())
        })
    }

    /// The device-budget authority the ephemeral spawn adapter consults before placing
    /// ANOTHER engine on this host.
    ///
    /// This is #56's `ResourceGovernor` for the one decision that was actually going
    /// unmade. That type never existed — its eight mentions in the tree are all doc
    /// comments promising a future owner — so `EphemeralServingLane::spawn` decided by
    /// `first_free_port` alone and three vision sidecars once shared one GPU at
    /// 3.1-8.6 tok/s apiece.
    ///
    /// Built over the SAME `live_host_budget` + `footprint_for` pair `pin_fit_checker`
    /// uses, so the gate and the planner cannot disagree about what fits. It models
    /// CO-RESIDENCE, not a swap: an ephemeral lane joins the live lane rather than
    /// replacing it, so the incumbent's footprint is spent, never credited back — the
    /// one real difference from a pin's arithmetic, and the reason this is a sibling
    /// function instead of a flag on that one.
    pub fn lane_admission_authority(&self) -> crate::inference::llama_server::LaneAdmissionAuthority {
        let system = self.system.clone();
        let resource_daemon = self.resource_daemon.clone();
        let serving_tx = self.serving_tx.clone();
        let model_resolver = self.model_resolver.clone();
        Arc::new(move |target: &crate::inference::llama_server::ServingTarget,
                       resident: &[crate::inference::lane_registry::LaneRecord]| {
            use crate::inference::llama_server::LaneAdmission;
            let budget = live_host_budget(&system, &resource_daemon).usable_bytes;
            let Some(model) = (model_resolver)(target.model_id()) else {
                // We cannot price a model we cannot resolve. Refusing is the honest
                // answer: admitting would be deciding by absence of information, which
                // is how the port check behaved and exactly what this replaces.
                return LaneAdmission::Refuse(format!(
                    "cannot admit an engine for '{}': the model does not resolve in the                      catalog, so its device footprint is unknown",
                    target.model_id()
                ));
            };
            // WEIGHTS as the cost, not weights+KV. The same choice `pin_fit_decision`
            // makes when it credits an incumbent back: weights dominate (measured on
            // this host, 18.97 GB of weights against a 24.63 GB budget) and they are the
            // term nothing can shrink, while KV is exactly what the planner sizes down
            // under pressure. A weights floor therefore under-states the true cost, so
            // this gate errs toward ADMITTING — it is a guard against a second ENGINE,
            // not a precise accountant.
            let Some(want) = footprint_for(&model).map(|f| f.weights_bytes) else {
                return LaneAdmission::Refuse(format!(
                    "cannot admit an engine for '{}': no on-disk footprint, so its device                      cost is unknown",
                    target.model_id()
                ));
            };
            // Everything already on the device: the live lane it joins, plus every
            // verified-live ephemeral sibling the caller's census found.
            let live_bytes = serving_tx
                .borrow()
                .active_model
                .clone()
                .and_then(|id| (model_resolver)(&id))
                .and_then(|m| footprint_for(&m))
                .map(|f| f.weights_bytes)
                .unwrap_or(0); // unwrap_or: nothing serving ⇒ the live lane costs nothing, which is the truth here
            let sibling_bytes: u64 = resident
                .iter()
                .filter_map(|rec| (model_resolver)(&rec.model))
                .filter_map(|m| footprint_for(&m))
                .map(|f| f.weights_bytes)
                .sum();
            lane_admission_decision(budget, want, live_bytes, sibling_bytes, resident.len())
        })
    }
}
