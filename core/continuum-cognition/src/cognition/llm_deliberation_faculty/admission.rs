//! Per-call admission for the concrete adapter target; all local permits stay together.
use super::*;
use crate::ai::adapter::GenerationAdmissionOwner;

pub(super) enum GenerationAdmission {
    AdapterOwned,
    SharedLocal {
        _lane: crate::inference::resource_admission::ServingLanePermit,
    },
}

impl LlmDeliberationFaculty {
    pub(super) async fn admit_generation(
        &self,
        ws: &Workspace,
        request: &TextGenerationRequest,
        owner: GenerationAdmissionOwner,
        capture: &mut Option<super::super::prompt_capture::CaptureLease>,
    ) -> Option<GenerationAdmission> {
        admit_target(owner, self.admit_local_generation(ws, request, capture)).await
    }

    async fn admit_local_generation(
        &self,
        ws: &Workspace,
        request: &TextGenerationRequest,
        capture: &mut Option<super::super::prompt_capture::CaptureLease>,
    ) -> Option<GenerationAdmission> {
        // MEASURED-HOLD DEFER, ABOVE the admission gates (2026-08-29). The adapter's
        // own defer sits BELOW acquire_serving_lane + the prefill slot, so a
        // Background generation (dream-belief-review, glass-boxed live) would take
        // the lane permits, THEN park on the hold — starving the measured work's
        // own next generation of the very permits it needs to finish and release
        // the hold. Priority inversion; the solve's tick 2 waited forever on a
        // permit a deferred dream held. Defer FIRST, holding nothing; the adapter
        // seam stays as the backstop for entry paths that skip these gates.
        continuum_base::probe!(
            class = "delib.defer.entered",
            persona = %self.persona_name,
            directed = ws.directed_at_self(),
            attention = ?ws.attention,
            "at the pre-gate hold defer"
        );
        {
            let class = deliberation_slot_class(request.purpose.as_deref(), ws.attention);
            crate::inference::measured_hold::defer_while_held(
                class,
                Some(self.persona_id),
                request.purpose.as_deref(),
            )
            .await;
        }
        continuum_base::probe!(
            class = "delib.defer.passed",
            persona = %self.persona_name,
            "past the pre-gate defer — admission gates next"
        );
        // Measured occupancy and queue depth estimate when to report a slow wait.
        // They do not cancel admission: a median is not a deadline for a queue.
        // Dropping acquire at this estimate repeatedly sent cold residents to the
        // back of FIFO (M5 2026-10-22: Aiko cancelled every 60 seconds).
        let lane_queue_ahead = crate::inference::resource_admission::work_waiting_now();
        let lanes_serving = crate::inference::resource_admission::served_lane_count();
        let (lane_hold_p50_ms, lane_hold_samples) =
            crate::inference::resource_admission::lane_hold_p50_ms();
        let lane_bound = crate::inference::resource_admission::lane_wait_bound(
            lane_queue_ahead,
            lanes_serving,
            lane_hold_p50_ms,
            lane_hold_samples,
        );
        let lane_bound_derived = lane_bound.is_some();
        let lane_bound =
            lane_bound.unwrap_or(crate::inference::resource_admission::LANE_WAIT_CEILING); // unwrap_or: unmeasured lanes use the existing diagnostic ceiling
        let expected_turn = self.expected_occupancy();
        let turn_bound = crate::inference::turn_bound::from_expectation(expected_turn);
        let (lane_bound, lane_bound_source) =
            crate::inference::turn_bound::effective_bound_with_source(lane_bound, turn_bound);
        continuum_base::probe!(
            class = "delib.gate.lane_wait",
            persona = %self.persona_name,
            directed = ws.directed_at_self(),
            attention = ?ws.attention,
            lanes_available = crate::inference::resource_admission::serving_lane_permits_available() as u64,
            // The bound she is about to wait under, and what it was derived FROM.
            bound_secs = lane_bound.as_secs(),
            bound_derived = lane_bound_derived,
            // Which of the two sized the wait: `turn_bound` = her own measured turn,
            // `floor` = the queue's estimate (she has no measured turn yet).
            bound_source = lane_bound_source.as_str(),
            turn_bound_secs = turn_bound.map(|b| b.as_secs()).unwrap_or(0), // unwrap_or: 0 = no measured expectation; the queue's estimate governs alone
            queue_ahead = lane_queue_ahead as u64,
            lanes_serving = lanes_serving as u64,
            lane_hold_p50_ms = lane_hold_p50_ms,
            lane_hold_samples = lane_hold_samples as u64,
            "at the serving-lane admission gate"
        );
        // A NON-directed wait yields to a directed line pending in her inbox:
        // the park held no lane, so abandoning it costs nothing, and the loop
        // head drains the line next (`cognition::directed_pending`). A `None`
        // here is her CHOICE to answer first, named by the probe — not a fault.
        // A held card, or her initiative turn with her hands, outranks a musing turn for
        // the non-directed budget (`LanePriority::Work`): the lane goes to the mind that
        // may write, whether or not a card named the work.
        let lane_wait_started = std::time::Instant::now();
        let priority = if ws.attention.requires_priority() {
            crate::inference::resource_admission::LanePriority::Directed
        } else if self.is_work_turn(ws) || Self::holds_work_card(ws) {
            // The turn shape and held-card window share the existing Work priority.
            crate::inference::resource_admission::LanePriority::Work
        } else {
            crate::inference::resource_admission::LanePriority::Ambient
        };
        let expected = match priority {
            crate::inference::resource_admission::LanePriority::Work => expected_turn,
            _ => None,
        };
        // One acquisition future owns the queue entry for this turn. An estimate
        // expiring reports contention but never drops or recreates that future.
        let (wait, acquire) = crate::inference::resource_admission::serving_lane_request(
                Some(self.persona_id),
                priority,
                expected,
            );
        let admission = super::admission_clock::waiting(wait, observe_wait(
            acquire,
            tokio::time::sleep(lane_bound),
            || {
                continuum_base::probe!(
                    class = "delib.gate.wait_estimate_exceeded",
                    persona = %self.persona_name,
                    waited_secs = lane_wait_started.elapsed().as_secs(),
                    bound_secs = lane_bound.as_secs(),
                    bound_source = lane_bound_source.as_str(),
                    lanes_available = crate::inference::resource_admission::serving_lane_permits_available() as u64,
                    "lane wait exceeded its estimate; retaining the same admission request and queue position"
                );
            },
        ));
        let _lane = if ws.attention.requires_priority() {
            admission.await
        } else {
            match crate::cognition::directed_pending::unless_directed(self.persona_id, admission).await {
                Some(lane) => lane,
                None => {
                    continuum_base::probe!(
                        class = "delib.gate.yielded_to_directed",
                        persona = %self.persona_name,
                        "parked self-work lane wait yielded: a directed line is pending"
                    );
                    if let Some(lease) = capture.as_mut() {
                        lease.abandon("yielded before dispatch: a directed line was pending and she answers first");
                    }
                    return None;
                }
            }
        };
        // The node's own queue, measured where SHE waits for it (card c84d885a, S1b):
        // the comparator every spill is judged against.
        crate::inference::resource_admission::note_local_lane_wait_ms(
            lane_wait_started.elapsed().as_millis() as u64,
        );
        // The reserve's guarantee, measured in its own unit: what a DIRECTED call waited.
        if ws.attention.requires_priority() {
            crate::inference::resource_admission::note_directed_lane_wait_ms(
                lane_wait_started.elapsed().as_millis() as u64,
            );
        }
        continuum_base::probe!(
            class = "delib.gate.lane_acquired",
            persona = %self.persona_name,
            "lane admission granted — issuing the model call"
        );
        crate::cognition::health_ledger::note_lane_granted_to(self.persona_id);
        Some(GenerationAdmission::SharedLocal { _lane })
    }
}

/// The local future is lazy: independent targets never poll or enqueue on it.
/// Keeping selection outside that future also releases partial local admission
/// on cancellation through the existing permit destructors.
pub(super) async fn admit_target(
    owner: GenerationAdmissionOwner,
    local: impl std::future::Future<Output = Option<GenerationAdmission>>,
) -> Option<GenerationAdmission> {
    match owner {
        GenerationAdmissionOwner::Adapter => Some(GenerationAdmission::AdapterOwned),
        GenerationAdmissionOwner::SharedLocal => local.await,
    }
}

/// Observe an estimated wait once without replacing the acquisition future.
/// Dropping this outer future still drops acquisition immediately, so cancellation,
/// despawn and shutdown retain the scheduler's existing RAII cleanup.
async fn observe_wait<T>(
    acquisition: impl std::future::Future<Output = T>,
    estimate: impl std::future::Future<Output = ()>,
    observe: impl FnOnce(),
) -> T {
    tokio::pin!(acquisition);
    tokio::select! {
        biased;
        permit = &mut acquisition => return permit,
        _ = estimate => observe(),
    }
    acquisition.await
}

#[cfg(test)]
#[path = "admission_tests.rs"]
mod tests;
