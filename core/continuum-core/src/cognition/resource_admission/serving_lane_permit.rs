//! Serving-lane lifetime and release accounting, owned by its RAII permit.

use super::{note_lane_held_ms, InflightModelCall, LanePriority};

/// RAII permit for one serving-lane model call. Holds a physical-lane permit, the
/// (optional) non-directed sub-cap permit, and the in-flight gauge marker — ALL released
/// on drop. Acquire it around the single generate call so the reservation window matches
/// exactly the lane-consuming window (queue + prefill + decode) and nothing downstream.
#[derive(Debug)]
pub struct ServingLanePermit {
    /// `Option` only so `drop` can return it BEFORE waking parked callers.
    pub(super) _lane: Option<tokio::sync::OwnedSemaphorePermit>,
    pub(super) _nondirected: Option<tokio::sync::OwnedSemaphorePermit>,
    pub(super) _inflight: InflightModelCall,
    /// Ledger tag for grant/release probes (2026-08-29 leak hunt) — permits
    /// vanished with no probe naming the taker, so the permit itself speaks.
    pub(super) granted_at: std::time::Instant,
    pub(super) directed: bool,
    pub(super) priority: LanePriority,
    /// Attribution survives until release; None is genuinely unowned background work.
    pub(super) persona: Option<uuid::Uuid>,
    /// The gate this permit came from — its release wakes parked ambient callers.
    pub(super) released: &'static tokio::sync::Notify,
}

impl Drop for ServingLanePermit {
    fn drop(&mut self) {
        let held_ms = self.granted_at.elapsed().as_millis() as u64;
        // The measured turn time the NEXT mind's wait is bounded by (`lane_wait_bound`).
        // One ring push per release, on a path that already probes.
        note_lane_held_ms(held_ms);
        crate::probe!(
            class = "admission.lane.released",
            persona = ?self.persona,
            directed = self.directed,
            priority = ?self.priority,
            held_ms = held_ms,
            "serving-lane permit released"
        );
        // Return the permits BEFORE waking parked callers. Fields would otherwise drop
        // after this body, and on a multi-threaded runtime a woken caller can poll in
        // that gap, find no lane, re-park and miss this wake (a 1 s stall).
        drop(self._lane.take());
        drop(self._nondirected.take());
        self.released.notify_waiters();
    }
}
