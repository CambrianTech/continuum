//! Move a caller's granted lane into the physical admission owner.
//! The scope retains unconsumed custody on every error/cancellation path.
use super::{LaneAdmission, LanePriority, ServingLanePermit};
use std::{cell::RefCell, future::Future, time::Duration};

tokio::task_local! {
    static CALL_LANE: RefCell<Option<ServingLanePermit>>;
}

pub async fn with_serving_lane<F: Future>(lane: ServingLanePermit, call: F) -> F::Output {
    CALL_LANE.scope(RefCell::new(Some(lane)), call).await
}

pub(crate) fn take_serving_lane() -> Result<Option<ServingLanePermit>, String> {
    match CALL_LANE.try_with(|lane| lane.borrow_mut().take()) {
        Ok(Some(lane)) => Ok(Some(lane)),
        Ok(None) => Err("serving lane custody was already transferred for this call".into()),
        Err(_) => Ok(None), // Adapter-owned calls have no caller lane to transfer.
    }
}

pub(crate) struct LaneReacquisition {
    pub(super) gate: &'static LaneAdmission,
    pub(super) persona: Option<uuid::Uuid>,
    pub(super) priority: LanePriority,
    pub(super) expected_occupancy: Option<Duration>,
}

#[cfg(test)]
#[path = "lane_handoff_tests.rs"]
mod tests;

impl LaneReacquisition {
    pub(crate) async fn acquire(&self) -> ServingLanePermit {
        self.gate
            .acquire_serving_lane(self.persona, self.priority, self.expected_occupancy)
            .await
    }
}
