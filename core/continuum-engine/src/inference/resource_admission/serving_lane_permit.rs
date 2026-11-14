//! Serving-lane lifetime and release accounting, owned by its RAII permit.

use super::{note_lane_held_ms, InflightModelCall, LanePriority};

/// RAII permit for one serving-lane model call. Holds a physical-lane permit, the
/// (optional) non-directed sub-cap permit, and the in-flight gauge marker — ALL released
/// on drop. Acquire it around the single generate call so the reservation window matches
/// exactly the lane-consuming window (queue + prefill + decode) and nothing downstream.
pub struct ServingLanePermit {
    pub(super) gate: &'static super::LaneAdmission,
    pub(super) expected_occupancy: Option<std::time::Duration>,
    // Fields drop in declaration order: the permits return FIRST, and only then do the wakes
    // fire (the budget permit's own, then the progress guard's), so a caller woken on another
    // worker always finds what this release freed.
    pub(super) _lane: tokio::sync::OwnedSemaphorePermit,
    pub(super) _nondirected: Option<BudgetPermit>,
    pub(super) _inflight: InflightModelCall,
    pub(super) _progress: super::progress::ProgressGuard,
    /// When it was granted: its held time on release (`lane_hold_p50_ms`, `admission.lane.tally`).
    pub(super) granted_at: std::time::Instant,
    pub(super) priority: LanePriority,
    /// Attribution survives until release; None is genuinely unowned background work.
    pub(super) persona: Option<uuid::Uuid>,
}

/// A non-directed budget permit that wakes the gate's parked callers once it is back in its
/// semaphore. A wake sent while the permit is still held is a missed wake on a multi-threaded
/// runtime: the woken caller tries, finds nothing free, and parks with no further wake coming
/// (a fixed 1 s re-check hid exactly that; timer audit row H3, 2026-11-14).
pub(super) struct BudgetPermit {
    _permit: tokio::sync::OwnedSemaphorePermit,
    _wake: WakeOnDrop,
}

impl BudgetPermit {
    pub(super) fn new(permit: tokio::sync::OwnedSemaphorePermit, released: &'static tokio::sync::Notify) -> Self {
        Self { _permit: permit, _wake: WakeOnDrop(released) }
    }
}

/// Wakes every caller parked on the gate when dropped; the LAST field of its holder.
struct WakeOnDrop(&'static tokio::sync::Notify);

impl Drop for WakeOnDrop {
    fn drop(&mut self) {
        self.0.notify_waiters();
    }
}

impl std::fmt::Debug for ServingLanePermit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServingLanePermit")
            .field("persona", &self.persona)
            .field("priority", &self.priority)
            .finish_non_exhaustive()
    }
}

impl ServingLanePermit {
    /// The same admission authority and priority used for this granted call.
    /// A parked physical-slot wait may relinquish capacity, then join this
    /// authority's queue again; it must not invent a different lane budget.
    pub(crate) fn reacquisition(&self) -> super::lane_handoff::LaneReacquisition {
        super::lane_handoff::LaneReacquisition {
            gate: self.gate,
            persona: self.persona,
            priority: self.priority,
            expected_occupancy: self.expected_occupancy,
        }
    }
}

impl Drop for ServingLanePermit {
    fn drop(&mut self) {
        let held_ms = self.granted_at.elapsed().as_millis() as u64;
        // The measured turn time the NEXT mind's wait is bounded by (`lane_wait_bound`).
        // One ring push per release, on a path that already probes.
        note_lane_held_ms(held_ms);
        // Counted, never a row per release: a hot path flooded the ledger with them (card 6742aad1).
        self.gate.tally.released(held_ms);
        // No wake here: this body runs BEFORE the fields drop, so the permits are still held.
        // The wakes ride the fields themselves (`BudgetPermit`, `ProgressGuard`), after them.
    }
}
