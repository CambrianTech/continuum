//! Intent ownership for the serving daemon.

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

/// A coherent process-local intent snapshot. Revision is not persisted across boots.
#[derive(Clone, Debug)]
pub struct ServingIntentSnapshot {
    pub revision: u64,
    pub pinned: Option<String>,
    pub suppressed: Arc<HashSet<String>>,
}

/// Mutation capability for the serving daemon's single intent watch.
#[derive(Clone)]
pub struct ServingIntent {
    pub(super) state: watch::Sender<ServingIntentSnapshot>,
}

impl ServingIntent {
    pub fn new(pinned: Option<String>) -> Self {
        let (state, _) = watch::channel(ServingIntentSnapshot {
            revision: 0,
            pinned,
            suppressed: Arc::new(HashSet::new()),
        });
        Self { state }
    }

    pub fn snapshot(&self) -> ServingIntentSnapshot {
        self.state.borrow().clone()
    }
    pub fn subscribe(&self) -> watch::Receiver<ServingIntentSnapshot> {
        self.state.subscribe()
    }

    /// Explicit accepted requests count even when reaffirming the current value.
    /// Pressure retries only count when their effective value changes.
    pub fn set_pin(&self, pinned: Option<String>, explicit: bool) -> Option<String> {
        let mut previous = None;
        self.state.send_if_modified(|state| {
            previous = state.pinned.clone();
            if !explicit && previous == pinned {
                return false;
            }
            state.revision = state
                .revision
                .checked_add(1)
                .expect("serving intent revision exhausted"); // JUSTIFIED: refuse overflow rather than authorize an old revision again.
            state.pinned = pinned;
            true
        });
        previous
    }

    pub fn set_suppressed(&self, model: &str, suppressed: bool, explicit: bool) -> bool {
        let mut previous = false;
        self.state.send_if_modified(|state| {
            previous = state.suppressed.contains(model);
            if !explicit && previous == suppressed {
                return false;
            }
            state.revision = state
                .revision
                .checked_add(1)
                .expect("serving intent revision exhausted"); // JUSTIFIED: refuse overflow rather than authorize an old revision again.
            if suppressed {
                Arc::make_mut(&mut state.suppressed).insert(model.to_string());
            } else {
                Arc::make_mut(&mut state.suppressed).remove(model);
            }
            true
        });
        previous
    }
}

/// Internal provenance for the plan. The command still returns only `plan`.
#[derive(Clone, Debug)]
pub(super) struct ServingPlanSnapshot {
    pub intent_revision: u64,
    pub plan: Option<ServingPlan>,
}
