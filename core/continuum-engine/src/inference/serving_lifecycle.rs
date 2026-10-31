//! The serving LIFECYCLE GATE: one operation at a time on the engine (a relaunch, a
//! placement move, a teacher batch, a training admission), named by step so a skip can say
//! which await holds it. An engine fact: the serving daemon (`modules::serving_daemon`)
//! drives it, and training admission (`inference::training_admission`) reads memory under
//! it, so neither the genome nor the forge reaches into the module layer for it.

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, OnceLock};

mod handoff;
pub use handoff::{HandoffClose, HandoffOwner};

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
/// What [`LifecycleGate::close_for_handoff`] came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandoffCloseOutcome {
    /// Closed until reopened with `token` or the owner's death.
    Closed { token: uuid::Uuid },
    /// A serving operation holds the gate now: nothing changed; ask again.
    Busy,
    /// Another handoff, whose owner runs (or cannot be verified gone), holds it closed: nothing
    /// changed; it is that handoff's to reopen.
    Taken { owner: HandoffOwner },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateRefusal {
    /// Serving has not published its first plan on this core: the authority may not
    /// hold a physical reading yet, so free memory is a boot artefact, not capacity.
    Unsettled,
    /// A serving operation (relaunch, placement move, teacher batch) holds the gate:
    /// free memory now is its window, not capacity.
    Operation,
    /// A core handoff closed training admission (card 91921d0f): a run admitted now would be
    /// ended by the stop that follows. Waits for the reopen, or for the handoff's owner to die.
    Handoff,
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
    /// A core handoff's close of training admission, if one stands.
    handoff: Arc<parking_lot::Mutex<Option<HandoffClose>>>,
}

impl GateRefusal {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Unsettled => "serving_unsettled",
            Self::Operation => "serving_operation",
            Self::Handoff => "core_handoff",
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
        Self { gate, started_ms, step, settled, handoff: Arc::new(parking_lot::Mutex::new(None)) }
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
            handoff: Arc::new(parking_lot::Mutex::new(None)),
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
        // read UNDER the gate: a close is set only while holding it, so an admission either
        // decided before the close (and its hold is published) or sees it here
        let mut handoff = self.handoff.lock();
        match handoff::verdict(handoff.as_ref(), crate::inference::engine_residency::probe_process) {
            handoff::CloseVerdict::Open => {}
            handoff::CloseVerdict::Closed => return Err(GateRefusal::Handoff),
            handoff::CloseVerdict::OwnerGone => {
                continuum_base::probe!(class = "serving.handoff.owner_gone", owner_pid = handoff.as_ref().map_or(0, |c| c.owner.pid),
                    "the process that closed training admission for a core handoff is gone without reopening it: admission reopens");
                *handoff = None;
            }
        }
        drop(handoff);
        operation.step(ReconcileStep::TrainingAdmission);
        Ok(AdmissionHold {
            _operation: operation,
        })
    }

    /// Close training admission for a core handoff owned by `owner` (card 91921d0f). Taken under
    /// the gate: an admission deciding now finishes first, so its hold is published before this
    /// returns, and every later one is refused (`GateRefusal::Handoff`) until
    /// [`Self::reopen_after_handoff`] or the owner's death. ONE handoff at a time: a close that
    /// another live (or unverifiable) owner holds is never replaced, or its handoff would proceed
    /// to the stop with admission reopened by the other (Codex on #5553); only a verified-dead
    /// owner's close is replaced.
    pub fn close_for_handoff(&self, owner: HandoffOwner) -> HandoffCloseOutcome {
        let Some(operation) = ServingOperation::acquire(self.gate.clone(), self.started_ms.clone(), self.step.clone()) else {
            return HandoffCloseOutcome::Busy;
        };
        let mut handoff = self.handoff.lock();
        if let Some(standing) = handoff.as_ref() {
            if handoff::verdict(Some(standing), crate::inference::engine_residency::probe_process) == handoff::CloseVerdict::Closed {
                return HandoffCloseOutcome::Taken { owner: standing.owner };
            }
        }
        let token = uuid::Uuid::new_v4();
        *handoff = Some(HandoffClose { token, owner });
        drop(handoff);
        drop(operation);
        continuum_base::probe!(class = "serving.handoff.closed", owner_pid = owner.pid, token = %token,
            "a core handoff closed training admission; admissions wait until it reopens or its owner dies");
        HandoffCloseOutcome::Closed { token }
    }

    /// Reopen what close `token` closed. `false` when no close stands or another does: a stale
    /// token never reopens a newer handoff's close.
    pub fn reopen_after_handoff(&self, token: uuid::Uuid) -> bool {
        let mut handoff = self.handoff.lock();
        if handoff.as_ref().is_some_and(|close| close.token == token) {
            *handoff = None;
            continuum_base::probe!(class = "serving.handoff.reopened", token = %token, "a core handoff reopened training admission");
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (card 91921d0f, Codex on #5551): a training run admitted between a
    // handoff's hold read and its stop. Once closed, the gate refuses admission as Handoff (a
    // wait, not a failure); a stale token never reopens a newer close; the right token reopens it;
    // and a close is not taken while a serving operation holds the gate.
    #[test]
    fn a_handoff_close_refuses_admission_until_its_own_token_reopens_it() {
        let gate = LifecycleGate::unowned(true);
        let me = HandoffOwner::of(std::process::id()).expect("test: this process's start time is readable");
        assert!(gate.hold_for_admission().is_ok(), "open: admits");
        let HandoffCloseOutcome::Closed { token } = gate.close_for_handoff(me) else { panic!("test: a free gate closes") };
        assert_eq!(gate.hold_for_admission().err(), Some(GateRefusal::Handoff), "closed by a live owner: waits");
        assert!(!gate.reopen_after_handoff(uuid::Uuid::from_u128(7)), "a stale token reopens nothing");
        assert_eq!(gate.hold_for_admission().err(), Some(GateRefusal::Handoff));
        assert!(gate.reopen_after_handoff(token));
        assert!(gate.hold_for_admission().is_ok(), "reopened: admits");
        let deciding = gate.hold_for_admission().expect("an admission holds the gate");
        assert_eq!(gate.close_for_handoff(me), HandoffCloseOutcome::Busy, "a close waits for the admission deciding now");
        drop(deciding);
        assert!(matches!(gate.close_for_handoff(me), HandoffCloseOutcome::Closed { .. }));
    }

    // what this catches (Codex on #5553 at 2e3092026): a second handoff replacing a live owner's
    // close, then reopening (or dying), so the first proceeded to its stop with admission open. A
    // live owner's close is never replaced or released by another's token; a verified-dead owner's is.
    #[test]
    fn a_second_handoff_never_releases_a_live_owners_close() {
        let gate = LifecycleGate::unowned(true);
        let a = HandoffOwner::of(std::process::id()).expect("test: this process's start time is readable");
        let HandoffCloseOutcome::Closed { token: a_token } = gate.close_for_handoff(a) else { panic!("test: A closes") };
        let b = HandoffOwner { pid: a.pid, started_s: a.started_s }; // another handoff from a live process
        assert_eq!(gate.close_for_handoff(b), HandoffCloseOutcome::Taken { owner: a }, "B cannot take A's close");
        assert!(!gate.reopen_after_handoff(uuid::Uuid::from_u128(0xb)), "nor reopen it");
        assert_eq!(gate.hold_for_admission().err(), Some(GateRefusal::Handoff), "A's exclusion stands");
        assert!(gate.reopen_after_handoff(a_token));
        // a close whose owner is verified gone (pid 0 is never a process) is replaced
        *gate.handoff.lock() = Some(HandoffClose { token: uuid::Uuid::from_u128(1), owner: HandoffOwner { pid: 0, started_s: 1 } });
        assert!(matches!(gate.close_for_handoff(b), HandoffCloseOutcome::Closed { .. }), "a dead owner's close is replaced");
    }
}

