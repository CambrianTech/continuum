//! Lifecycle ownership for the serving daemon.

use super::*;

/// An in-flight reconcile past this age is WEDGED: named on the gate-skip probe with the
/// step it is in, so the next sample of the core knows which await to read. Twice the
/// engine's ready timeout: the longest single wait a healthy reconcile can hold.
// derived-or-floor: derived — 2 × READY_TIMEOUT (a launch may wait ready once, then verify).
pub(super) const RECONCILE_WEDGE_BOUND: Duration = Duration::from_secs(2 * READY_TIMEOUT.as_secs());

/// Ticks a LIVE lane may sit with no published plan before it is said: the boot plan
/// lands within the first tick or two; three is past any honest transient.
// derived-or-floor: a floor — three ticks at TICK is well past the boot's own first plan.
pub(super) const PLAN_NONE_LIVE_LANE_TICKS: u64 = 3;

/// A gate skip says something once per this interval, never per tick.
// derived-or-floor: a floor — one line per half minute is readable in a log and loud enough.
pub(super) const RECONCILE_BUSY_SAY_EVERY: Duration = Duration::from_secs(30);

/// True when an operation of this age has outlived the bound a healthy reconcile can hold.
pub(super) fn reconcile_wedged(age: Duration) -> bool {
    age > RECONCILE_WEDGE_BOUND
}

/// The bound is PER STEP (Cormac on #4421): an academy teacher batch holds the gate for
/// as long as its corpus takes — minutes, legitimately — so the single serving bound
/// would read every long batch as a wedge. A batch is `busy`, never `wedged`; the batch
/// has its own deadline and receipt. Every other step is bounded as a reconcile.
pub(super) fn reconcile_wedged_at(step: ReconcileStep, age: Duration) -> bool {
    match step {
        ReconcileStep::AcademyBatch => false,
        _ => reconcile_wedged(age),
    }
}

/// A forced health check stays owed if its task is cancelled or superseded.
/// Completion never clears a newer report raised while this claim was in flight.
pub(super) struct ForcedProbe {
    pub(super) flag: Arc<AtomicBool>,
    pub(super) pending: bool,
}
impl ForcedProbe {
    pub(super) fn claim(flag: Arc<AtomicBool>) -> Self {
        let pending = flag.swap(false, Ordering::AcqRel);
        Self { flag, pending }
    }
}
impl Drop for ForcedProbe {
    fn drop(&mut self) {
        if self.pending {
            self.flag.store(true, Ordering::Release);
        }
    }
}

impl ServingDaemonModule {
    /// This daemon's lifecycle gate, for the boot path to publish (`LifecycleGate::set_global`).
    pub fn lifecycle_gate(&self) -> LifecycleGate {
        LifecycleGate::new(
            self.reconciling.clone(),
            self.reconcile_started_ms.clone(),
            self.reconcile_step.clone(),
            self.plan_published.clone(),
        )
    }
}
