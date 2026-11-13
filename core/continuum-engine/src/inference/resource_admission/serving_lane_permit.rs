//! Serving-lane lifetime and release accounting, owned by its RAII permit.

use super::{note_lane_held_ms, InflightModelCall, LanePriority};

/// RAII permit for one serving-lane model call. Holds a physical-lane permit, the
/// (optional) non-directed sub-cap permit, and the in-flight gauge marker — ALL released
/// on drop. Acquire it around the single generate call so the reservation window matches
/// exactly the lane-consuming window (queue + prefill + decode) and nothing downstream.
pub struct ServingLanePermit {
    pub(super) gate: &'static super::LaneAdmission,
    pub(super) expected_occupancy: Option<std::time::Duration>,
    pub(super) _progress: super::progress::ProgressGuard,
    pub(super) _lane: tokio::sync::OwnedSemaphorePermit,
    pub(super) _nondirected: Option<tokio::sync::OwnedSemaphorePermit>,
    pub(super) _inflight: InflightModelCall,
    /// When it was granted: its held time on release (`lane_hold_p50_ms`, `admission.lane.tally`).
    pub(super) granted_at: std::time::Instant,
    pub(super) priority: LanePriority,
    /// Attribution survives until release; None is genuinely unowned background work.
    pub(super) persona: Option<uuid::Uuid>,
    /// The gate this permit came from — its release wakes parked ambient callers.
    pub(super) released: &'static tokio::sync::Notify,
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
        // Permits drop AFTER this body (field order), so wake on the next poll: the
        // parked caller's try_acquire runs after the semaphore permit has returned.
        self.released.notify_waiters();
    }
}
