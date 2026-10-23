//! The serving LIFECYCLE GATE: one operation at a time on the engine (a relaunch, a
//! placement move, a teacher batch, a training admission), named by step so a skip can say
//! which await holds it. An engine fact: the serving daemon (`modules::serving_daemon`)
//! drives it, and training admission (`inference::training_admission`) reads memory under
//! it, so neither the genome nor the forge reaches into the module layer for it.

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, OnceLock};

use continuum_base::time::EpochMs;

/// The existing reconcile gate, armed before spawning so abort-before-first-poll
/// cannot strand it. Every lifecycle operation, including empty-plan idle, owns it.
/// Where an in-flight reconcile operation is waiting — named, so a tick that skips past
/// it can say WHICH await holds the daemon (card c3c50e0d: the M5 went dark for six
/// minutes at boot beside a surviving engine; every tick and the planner skipped on the
/// gate in silence, and the evidence died with the kill that restored it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ReconcileStep {
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
    pub fn name(self) -> &'static str {
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
    pub fn from_u8(v: u8) -> Self {
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

pub struct ServingOperation {
    pub(crate) gate: Arc<AtomicBool>,
    pub(crate) started_ms: Arc<AtomicU64>,
    pub(crate) step: Arc<AtomicU8>,
}
impl ServingOperation {
    pub fn acquire(
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
    pub fn step(&self, step: ReconcileStep) {
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

/// Why an admission may not decide now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateRefusal {
    /// Serving has not published its first plan on this core: the authority may not
    /// hold a physical reading yet, so free memory is a boot artefact, not capacity.
    Unsettled,
    /// A serving operation (relaunch, placement move, teacher batch) holds the gate:
    /// free memory now is its window, not capacity.
    Operation,
}
/// The gate, held while one admission reads and leases; dropping it releases the gate.
pub struct AdmissionHold {
    _operation: ServingOperation,
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
pub struct LifecycleGate {
    gate: Arc<AtomicBool>,
    started_ms: Arc<AtomicU64>,
    step: Arc<AtomicU8>,
    settled: Arc<AtomicBool>,
}

impl GateRefusal {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Unsettled => "serving_unsettled",
            Self::Operation => "serving_operation",
        }
    }
}
pub(crate) static LIFECYCLE_GATE: OnceLock<LifecycleGate> = OnceLock::new();
impl LifecycleGate {
    /// The gate over a serving daemon's own operation state (its reconcile flag, the
    /// operation's start and step, and whether its first plan is published).
    pub fn new(
        gate: Arc<AtomicBool>,
        started_ms: Arc<AtomicU64>,
        step: Arc<AtomicU8>,
        settled: Arc<AtomicBool>,
    ) -> Self {
        Self { gate, started_ms, step, settled }
    }
    /// Publish THE serving daemon's gate (first writer wins — the boot path). One serving
    /// lifecycle per core, same precedent as `ResourceDaemon::set_global`.
    pub fn set_global(gate: LifecycleGate) {
        let _ = LIFECYCLE_GATE.set(gate);
    }
    /// The published gate, if boot installed a serving daemon (None in bare unit tests).
    pub fn global() -> Option<LifecycleGate> {
        LIFECYCLE_GATE.get().cloned()
    }
    /// A gate no serving daemon owns, for admission tests.
    /// A gate no serving daemon owns, for admission tests; `settled` as its planner would be.
    #[cfg(any(test, feature = "test-fixtures"))]
    pub fn unowned(settled: bool) -> Self {
        Self {
            gate: Arc::new(AtomicBool::new(false)),
            started_ms: Arc::new(AtomicU64::new(0)),
            step: Arc::new(AtomicU8::new(0)),
            settled: Arc::new(AtomicBool::new(settled)),
        }
    }
    /// What a test's planner does on its first publish.
    #[cfg(any(test, feature = "test-fixtures"))]
    pub fn settle(&self) {
        self.settled.store(true, Ordering::Release);
    }
    /// Take the gate for one admission decision, or say why it cannot decide now.
    pub fn hold_for_admission(&self) -> Result<AdmissionHold, GateRefusal> {
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

