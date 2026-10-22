//! Per-call admission for the concrete adapter target; all local permits stay together.
use super::*;
use crate::ai::adapter::GenerationAdmissionOwner;

pub(super) enum GenerationAdmission {
    AdapterOwned,
    SharedLocal {
        _lane: crate::inference::resource_admission::ServingLanePermit,
        _prefill: tokio::sync::OwnedSemaphorePermit,
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
        // THE WAIT'S OWN BOUND, NAMED AND SIZED BY THE QUEUE (card cff534ba, S3).
        // Until now this park was UNBOUNDED, so the thing that ended it was the
        // per-act `TICK_DEADLINE` — which is why the M5 carried a pace row reading
        // `residue_ms = 1,500,001`: 1500 s to the millisecond is a 25-minute act
        // deadline expiring, not work. A bound sized for an ACT is not a bound for
        // a QUEUE. Hers is derived from what the serving daemon already measures:
        // the depth of held-work callers ahead of her, the lanes serving, and the
        // lanes' own measured turn time — `lane_wait_bound`. Unmeasured (a fresh
        // boot) falls back to the named `LANE_WAIT_CEILING`, never to a short
        // guess: busy is not dead.
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
            lane_bound.unwrap_or(crate::inference::resource_admission::LANE_WAIT_CEILING); // unwrap_or: UNMEASURED lanes get the named ceiling, never a short guess
                                                                                           // THE QUEUE'S ESTIMATE IS A FLOOR, NEVER A CEILING (card ebce2ba0). The
                                                                                           // derivation above measures the LANES — `rounds x lane_hold_p50 x SLACK`,
                                                                                           // clamped — and on the M5 2026-09-20 it was computed over TWO recorded holds
                                                                                           // (`lane_hold_p50_ms=82918`, `queue_ahead=0`, `lanes_serving=2`) while the
                                                                                           // lanes were actually holding four to seven minutes. It told six minds to
                                                                                           // wait 165 s each and then give up first in line, having generated nothing:
                                                                                           // six cancelled captures at 165,843 / 165,844 / 165,845 / 165,856 / 165,917 /
                                                                                           // 165,982 ms, each within milliseconds of the bound this probe named. Four
                                                                                           // more at ~412 s and two at ~485 s are the same derivation at a later p50,
                                                                                           // and 60,009 ms is `LANE_WAIT_FLOOR` itself.
                                                                                           //
                                                                                           // A median of the lanes is not a bound for HER turn. Hers is the same
                                                                                           // measured expectation the request already carries on the wire
                                                                                           // (`expected_occupancy` x headroom, #4277's `turn_bound`): a mind whose own
                                                                                           // turns take five minutes may not be told a lane is hopeless in under two of
                                                                                           // them. The queue's number stays the floor — it is the only thing that speaks
                                                                                           // when she has no measured turn yet.
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
        // A held card outranks a musing turn for the non-directed budget
        // (`LanePriority::Work`): the lane goes to the mind that will write.
        let lane_wait_started = std::time::Instant::now();
        // `None` from either arm below = the bound tripped, reported once at the
        // single seam after it.
        let lane_or_starved = if ws.attention.requires_priority() {
            tokio::select! {
                lane = crate::inference::resource_admission::acquire_serving_lane(
                    Some(self.persona_id),
                    crate::inference::resource_admission::LanePriority::Directed,
                    None,
                ) => Some(lane),
                _ = tokio::time::sleep(lane_bound) => None,
            }
        } else {
            // The faculty's own work-turn key (`is_work_turn`: the workspace is
            // deliverable and her hands are offered — the same key that caps act
            // output) OR a held card in the window. #4060's receipt on 5333: 0 Work
            // grants in 20 min with 14 held cards — the rendered block alone is not
            // a reliable key at the gate; the turn shape is.
            let priority = if self.is_work_turn(ws) || Self::holds_work_card(ws) {
                crate::inference::resource_admission::LanePriority::Work
            } else {
                crate::inference::resource_admission::LanePriority::Ambient
            };
            // A work call states its own bound on the lane (`expected_occupancy`), so the
            // reserved lane may be lent to it inside the directed wait budget.
            let expected = match priority {
                crate::inference::resource_admission::LanePriority::Work => expected_turn,
                _ => None,
            };
            tokio::select! {
                lane = crate::inference::resource_admission::acquire_serving_lane(Some(self.persona_id), priority, expected) => Some(lane),
                _ = tokio::time::sleep(lane_bound) => None,
                _ = crate::cognition::directed_pending::wait(self.persona_id) => {
                    continuum_base::probe!(
                        class = "delib.gate.yielded_to_directed",
                        persona = %self.persona_name,
                        "parked self-work lane wait yielded: a directed line is pending"
                    );
                    // Her CHOICE, not a drop — but the capture lease dies with this
                    // return, so say which (card ebce2ba0).
                    if let Some(lease) = capture.as_mut() {
                        lease.abandon(
                            "yielded before dispatch: a directed line was pending and she answers first",
                        );
                    }
                    return None;
                }
            }
        };
        // THE BOUND TRIPPED, SAID BY NAME — the mind, what she waited, what was
        // free while she waited, and the queue the bound was derived from. The
        // settle loop classifies the iteration that follows (`classify_act`): no
        // generation was dispatched, so it is a WAIT, not an act, and her act
        // budget survives it intact.
        let Some(_lane) = lane_or_starved else {
            continuum_base::probe!(
                class = "persona.act.lane_starved",
                persona = %self.persona_name,
                directed = ws.directed_at_self(),
                waited_secs = lane_wait_started.elapsed().as_secs(),
                bound_secs = lane_bound.as_secs(),
                bound_derived = lane_bound_derived,
                queue_ahead = lane_queue_ahead as u64,
                lanes_serving = lanes_serving as u64,
                lane_hold_p50_ms = lane_hold_p50_ms,
                lanes_available = crate::inference::resource_admission::serving_lane_permits_available() as u64,
                bound_source = lane_bound_source.as_str(),
                turn_bound_secs = turn_bound.map(|b| b.as_secs()).unwrap_or(0), // unwrap_or: 0 = no measured expectation; the queue's estimate governed alone
                "the lane wait hit its bound (her own measured turn when she has one, else the queue's estimate, floored at LANE_WAIT_FLOOR) — deferring without reaching the model; she keeps her act budget and tries again next tick"
            );
            // NEVER DISPATCHED, AND THE CAPTURE MUST SAY SO (card ebce2ba0). Without
            // this the lease's `Drop` wrote "request future dropped before a terminal
            // response" — the SAME row a generation the model was still producing
            // writes — and eighteen of fifty captures on the M5 read as dropped
            // generations when most of them never reached the model at all.
            if let Some(lease) = capture.as_mut() {
                lease.abandon(&format!(
                        "lane starved before dispatch: waited {}s of a {}s bound ({}) with {} lanes free",
                        lane_wait_started.elapsed().as_secs(),
                        lane_bound.as_secs(),
                        lane_bound_source.as_str(),
                        crate::inference::resource_admission::serving_lane_permits_available(),
                    ));
            }
            return None;
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
            "lane admission granted — prefill slot next"
        );
        crate::persona::health_ledger::note_lane_granted_to(self.persona_id);
        // #56 prefill throttle: under live external GPU pressure (a game, the browser)
        // fewer than the served lane count may PREFILL concurrently — the instant valve
        // for the 2026-07-16 compute-buffer OOM. Same fit rule the capacity sim proves;
        // no pressure → target == lanes → this never waits. Released with the block.
        let _prefill = crate::inference::prefill_throttle::acquire_prefill_slot().await;
        continuum_base::probe!(
            class = "delib.gate.prefill_acquired",
            persona = %self.persona_name,
            "prefill slot granted — issuing the model call"
        );
        Some(GenerationAdmission::SharedLocal { _lane, _prefill })
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
