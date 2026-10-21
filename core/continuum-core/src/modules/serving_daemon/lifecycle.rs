//! Lifecycle ownership for the serving daemon.

use super::*;

/// The existing reconcile gate, armed before spawning so abort-before-first-poll
/// cannot strand it. Every lifecycle operation, including empty-plan idle, owns it.
/// Where an in-flight reconcile operation is waiting — named, so a tick that skips past
/// it can say WHICH await holds the daemon (card c3c50e0d: the M5 went dark for six
/// minutes at boot beside a surviving engine; every tick and the planner skipped on the
/// gate in silence, and the evidence died with the kill that restored it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(super) enum ReconcileStep {
    None = 0,
    /// Waiting on the external endpoint's reachability probe.
    External = 1,
    /// Retiring the owned engine for an empty plan (`idle_if_current`).
    Retire = 2,
    /// `ensure_model_serving_if_current`: adopt the running engine or spawn and wait ready.
    Ensure = 3,
    /// Reading the ready engine's own per-slot window (`/props`).
    VerifyWindow = 4,
    /// Reading the ready engine's own slot count (`/props`).
    VerifyLanes = 5,
    /// Holding the gate for an academy teacher batch on the lane.
    AcademyBatch = 6,
    /// Training admission reading governed memory under the gate (aae8af55).
    TrainingAdmission = 7,
}
impl ReconcileStep {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::External => "external_probe",
            Self::Retire => "retire_owned_engine",
            Self::Ensure => "ensure_serving",
            Self::VerifyWindow => "verify_window",
            Self::VerifyLanes => "verify_lanes",
            Self::AcademyBatch => "academy_batch",
            Self::TrainingAdmission => "training_admission",
        }
    }
    pub(super) fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::External,
            2 => Self::Retire,
            3 => Self::Ensure,
            4 => Self::VerifyWindow,
            5 => Self::VerifyLanes,
            6 => Self::AcademyBatch,
            7 => Self::TrainingAdmission,
            _ => Self::None,
        }
    }
}

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

pub(super) struct ServingOperation {
    pub(super) gate: Arc<AtomicBool>,
    pub(super) started_ms: Arc<AtomicU64>,
    pub(super) step: Arc<AtomicU8>,
}
impl ServingOperation {
    pub(super) fn acquire(
        gate: Arc<AtomicBool>,
        started_ms: Arc<AtomicU64>,
        step: Arc<AtomicU8>,
    ) -> Option<Self> {
        gate.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()?;
        started_ms.store(EpochMs::now().millis(), Ordering::Release);
        step.store(ReconcileStep::None as u8, Ordering::Release);
        Some(Self {
            gate,
            started_ms,
            step,
        })
    }
    /// Name the await this operation is entering.
    pub(super) fn step(&self, step: ReconcileStep) {
        self.step.store(step as u8, Ordering::Release);
    }
}
impl Drop for ServingOperation {
    fn drop(&mut self) {
        self.step
            .store(ReconcileStep::None as u8, Ordering::Release);
        self.started_ms.store(0, Ordering::Release);
        self.gate.store(false, Ordering::Release);
    }
}

/// The serving lifecycle gate as a handle another owner can take (card aae8af55).
/// Training admission reads governed memory UNDER this gate: a relaunch or a placement
/// move holds it from the moment it frees the engine's memory until the new engine is
/// resident, so a free-memory read inside that window is refused rather than admitting
/// a trainer at a time nobody called (the 9/16 class: a 22 GB sidecar admitted inside a
/// relaunch). The gate is held only across the admission, not the run: once granted,
/// the training lease is what keeps serving out of that memory, and holding the gate
/// for hours would stop every relaunch on a node where training and serving co-reside.
#[derive(Clone)]
pub(crate) struct LifecycleGate {
    pub(super) gate: Arc<AtomicBool>,
    pub(super) started_ms: Arc<AtomicU64>,
    pub(super) step: Arc<AtomicU8>,
    pub(super) settled: Arc<AtomicBool>,
}

/// Why an admission may not decide now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GateRefusal {
    /// Serving has not published its first plan on this core: the authority may not
    /// hold a physical reading yet, so free memory is a boot artefact, not capacity.
    Unsettled,
    /// A serving operation (relaunch, placement move, teacher batch) holds the gate:
    /// free memory now is its window, not capacity.
    Operation,
}
impl GateRefusal {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Unsettled => "serving_unsettled",
            Self::Operation => "serving_operation",
        }
    }
}
pub(super) static LIFECYCLE_GATE: OnceLock<LifecycleGate> = OnceLock::new();
impl LifecycleGate {
    /// Publish THE serving daemon's gate (first writer wins — the boot path). One serving
    /// lifecycle per core, same precedent as `ResourceDaemon::set_global`.
    pub(crate) fn set_global(gate: LifecycleGate) {
        let _ = LIFECYCLE_GATE.set(gate);
    }
    /// The published gate, if boot installed a serving daemon (None in bare unit tests).
    pub(crate) fn global() -> Option<LifecycleGate> {
        LIFECYCLE_GATE.get().cloned()
    }
    /// A gate no serving daemon owns, for admission tests.
    /// A gate no serving daemon owns, for admission tests; `settled` as its planner would be.
    #[cfg(test)]
    pub(crate) fn unowned(settled: bool) -> Self {
        Self {
            gate: Arc::new(AtomicBool::new(false)),
            started_ms: Arc::new(AtomicU64::new(0)),
            step: Arc::new(AtomicU8::new(0)),
            settled: Arc::new(AtomicBool::new(settled)),
        }
    }
    /// What a test's planner does on its first publish.
    #[cfg(test)]
    pub(crate) fn settle(&self) {
        self.settled.store(true, Ordering::Release);
    }
    /// Take the gate for one admission decision, or say why it cannot decide now.
    pub(crate) fn hold_for_admission(&self) -> Result<super::AdmissionHold, super::GateRefusal> {
        if !self.settled.load(Ordering::Acquire) {
            return Err(GateRefusal::Unsettled);
        }
        let operation = ServingOperation::acquire(
            self.gate.clone(),
            self.started_ms.clone(),
            self.step.clone(),
        )
        .ok_or(GateRefusal::Operation)?;
        operation.step(ReconcileStep::TrainingAdmission);
        Ok(AdmissionHold {
            _operation: operation,
        })
    }
}

/// The gate, held while one admission reads and leases; dropping it releases the gate.
pub(crate) struct AdmissionHold {
    pub(super) _operation: ServingOperation,
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
    pub(crate) fn lifecycle_gate(&self) -> LifecycleGate {
        LifecycleGate {
            gate: self.reconciling.clone(),
            started_ms: self.reconcile_started_ms.clone(),
            step: self.reconcile_step.clone(),
            settled: self.plan_published.clone(),
        }
    }
}
