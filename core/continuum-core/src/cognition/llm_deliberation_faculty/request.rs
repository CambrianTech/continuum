use super::*;

/// The kind of turn a completion is for — its latency budget and its runaway bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TurnKind {
    /// A work turn: deliverable + hands — a tool call plus a short plan.
    Act,
    /// Every other turn — a reply in a room, or a workspace turn with nothing
    /// deliverable: a sentence and a reason. A short turn, on a thinking model, is
    /// still a THINK and then a sentence.
    Pass,
}

impl TurnKind {
    /// The seconds the TIME term of the allowance is derived from — a floor on a fast
    /// lane, never the bound (see [`output_allowance`]).
    fn latency_budget_secs(self) -> f64 {
        match self {
            Self::Act => LlmDeliberationFaculty::ACT_LATENCY_BUDGET_SECS,
            Self::Pass => LlmDeliberationFaculty::PASS_LATENCY_BUDGET_SECS,
        }
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Act => "act",
            Self::Pass => "pass",
        }
    }
}

/// The reported terms beside a turn's allowance — every field rides the
/// `delib.turn.output_allowance` receipt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct OutputAllowance {
    /// `latency budget × her decode rate`; `None` when no rate is known on this box.
    pub(super) time_term: Option<u32>,
    /// Her measured reasoning + answer need; `None` until measured.
    pub(super) need_term: Option<u32>,
    /// What the request carries as `max_tokens` — the reply's flex grant.
    pub(super) allowance: u32,
    /// The measured need did not fit the grant the window left — said, not hidden.
    pub(super) need_clipped: bool,
}

/// What the turn is EXPECTED to use beside the allowance it was granted: `time budget ×
/// her decode rate` and her measured reasoning + answer need. Reported, never the
/// bound: the bound is the reply's flex grant ([`reply_claim`]).
///
/// Why both terms are still read (the M5, 2026-09-20 20:58–21:14Z): #4194 sized the
/// allowance as TIME alone. After a restart her rate read 0.0 and the allowance fell to
/// a floor — `kind=pass measured_tps=0.0 allowance=768`, `kind=act allowance=1500` —
/// and two turns ended inside the reasoning channel (`persona.act.think_only
/// reasoning_len=2730`, `2112`): no answer, no act. So the NEED term sizes the reply's
/// claim ([`reply_claim`]) and the time term is reported beside it. A 0.0 or NaN rate
/// is no rate.
///
/// THE MEASUREMENTS ARE EXPECTATIONS, NEVER THE CEILING (the 5090, 2026-10-06): a p90
/// need with headroom is exceeded by about one turn in ten, and a cut tool call commits
/// nothing (Kimi's `tools/<cut at the output limit> ✗`, allowance 10,177 = need). So the
/// allowance is the grant plus every token no other claim took, under the runaway bound.
pub(super) fn output_allowance(
    kind: TurnKind,
    tps: Option<f64>,
    need: Option<super::super::working_set::OutputNeed>,
    allowance: u32,
) -> OutputAllowance {
    let time_term = tps
        .filter(|t| t.is_finite() && *t > 0.0)
        .map(|t| (kind.latency_budget_secs() * t).round() as u32);
    let need_term = need.map(|n| n.total());
    let allowance = allowance.max(1);
    OutputAllowance {
        time_term,
        need_term,
        allowance,
        need_clipped: need_term.is_some_and(|n| n > allowance),
    }
}

impl LlmDeliberationFaculty {
    /// Build a generation request for the message thread. Centralized so the
    /// first prompt and any future re-prompt share one shape. Takes the model
    /// binding as an already-loaded snapshot (`contribute` loads it ONCE per turn)
    /// so the requested `model` and the `max_tokens` reserve derived from
    /// `context_window` come from the SAME atomic {adapter, model, window} triple —
    /// a re-home landing mid-turn can never tear the model away from its window.
    /// One adapter attempt through the existing hold, lane, prefill and capture
    /// boundaries. A capacity refusal returns its typed receipt to the fitter;
    /// no model output has been accepted or tools executed by a rejected attempt.
    pub(super) async fn generate_for_workspace(
        &self,
        ws: &Workspace,
        binding: &ModelBinding,
        context_window: u32,
        mut request: TextGenerationRequest,
        receipts: &mut Vec<super::super::provenance::GenerationReceipt>,
    ) -> Option<Result<TextGenerationResponse, crate::ai::inference_error::InferenceError>> {
        // Identity belongs to the submitted adapter request. Provider response IDs
        // remain separate, and one workspace can issue several calls.
        let request_id = request
            .request_id
            .get_or_insert_with(|| Uuid::new_v4().to_string())
            .clone();
        // How much of this prompt her previous request already held (card 5b09111e).
        crate::cognition::prompt_prefix::observe(self.persona_id, &request);
        // Stage the request for the reuse split (card 9e4d61e8); settled below on its result.
        crate::cognition::prompt_prefix::stage_reuse(self.persona_id, &request_id, &request);
        // The genome this turn runs on, read before the request moves: the receipt names it,
        // so the room's outcome for the turn can be credited to the genes that produced it.
        let genes = super::super::provenance::genes_of(request.active_adapters.as_deref());
        let mut capture = self.prompt_capture.as_ref().map(|sink| {
            super::super::prompt_capture::CaptureLease::start(
                Arc::clone(sink),
                &super::super::prompt_capture::PromptCall {
                    request_id: request_id.clone(),
                    persona_id: self.persona_id,
                    room_id: ws.room_id,
                    context_window: Some(context_window),
                    cycle_id: (ws.cycle != super::super::workspace::CycleId::UNSTAMPED)
                        .then_some(ws.cycle.0),
                    cause: ws.cause.as_str(),
                    cause_root: ws.cause.root(),
                    replay_of: None,
                },
                &request,
            )
        });
        // #169 STREAMING: when THIS turn carries a token sink (a live Speak the caller
        // wants progressive), generate through `generate_stream` so each decoded chunk
        // is forwarded to the caller (→ persona.turn.delta → room/TTS/avatar). The
        // adapter returns the SAME full `TextGenerationResponse` either way
        // (`generate_text` IS `generate_stream` + accumulate), so everything below —
        // capture, working-memory, decision parse, act/speak — is byte-identical; only
        // DELIVERY timing changes. `None` (every non-streaming caller, every test) takes
        // the unchanged accumulate path. The sink carries BOTH Reasoning and Token
        // chunks; the consumer forwards only Token to output (think-deep/speak-answer).
        // #139 latency split: time the generate call (lane-queue + prefill + decode).
        // Compared against the forwarder's `first_token_ms` (spawn→first token, the
        // WHOLE turn), this localizes the minutes-to-first-token: if `gen_await_ms`
        // is large, it's the model/lane (queue+prefill); if small, the time is in
        // cognition-prep BEFORE generation (recall/embeddings/context assembly).
        // #139 serving-lane admission, priced by priority. Acquire a decode lane for the
        // model-call window ONLY (queue+prefill+decode); the RAII permit also carries the
        // in-flight gauge marker (so the self-tick saturation read stays accurate) and, for
        // NON-directed calls, a slot in the non-directed budget (every lane). A directed
        // (addressed) turn is first in line for the next lane freed, so it never queues
        // behind idle musing or a long ambient turn, and no lane is held back idle for it. Scoped to a
        // block so every lane releases the instant generation returns — downstream
        // capture/parse/act hold nothing. [[conversational-latency-is-a-misdirection-budget]]
        // WIRE CAPTURE (2026-08-24, the churn hunt's missing instrument): when
        // `SERVING_WIRE_CAPTURE_DIR` is configured (config.env — same channel as
        // SERVING_KV_CACHE_TYPE), append this request's EXACT message list to
        // `<dir>/<persona>.wire.jsonl` before it ships. Byte-diffing consecutive
        // rows names the precise prompt position where the KV prefix dies —
        // today that diagnosis ran on the BID capture, which is not the wire,
        // and misled twice. Off (unset) = zero cost, zero IO — the Noop default
        // every capture sink owes the hot path.
        if let Some(dir) = crate::config_env::read("SERVING_WIRE_CAPTURE_DIR") {
            append_wire_capture(std::path::Path::new(&dir), self.persona_id, &self.persona_name, ws.room_id, &request);
        }
        // MEASURED-HOLD DEFER, ABOVE the admission gates (2026-08-29). The adapter's
        // own defer sits BELOW acquire_serving_lane + the prefill slot, so a
        // Background generation (dream-belief-review, glass-boxed live) would take
        // the lane permits, THEN park on the hold — starving the measured work's
        // own next generation of the very permits it needs to finish and release
        // the hold. Priority inversion; the solve's tick 2 waited forever on a
        // permit a deferred dream held. Defer FIRST, holding nothing; the adapter
        // seam stays as the backstop for entry paths that skip these gates.
        crate::probe!(
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
        crate::probe!(
            class = "delib.defer.passed",
            persona = %self.persona_name,
            "past the pre-gate defer — admission gates next"
        );
        if ws.attention.requires_priority() {
            // #2561: foreground engagement marks the organism ACTIVE (with linger).
            // Human opportunity remains active without asserting a literal address.
            crate::cognition::activity_gate::note_directed();
        }
        let gen_result = {
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
            let lane_queue_ahead = crate::cognition::resource_admission::work_waiting_now();
            let lanes_serving = crate::cognition::resource_admission::served_lane_count();
            let (lane_hold_p50_ms, lane_hold_samples) =
                crate::cognition::resource_admission::lane_hold_p50_ms();
            let lane_bound = crate::cognition::resource_admission::lane_wait_bound(
                lane_queue_ahead,
                lanes_serving,
                lane_hold_p50_ms,
                lane_hold_samples,
            );
            let lane_bound_derived = lane_bound.is_some();
            let lane_bound = lane_bound
                .unwrap_or(crate::cognition::resource_admission::LANE_WAIT_CEILING); // unwrap_or: UNMEASURED lanes get the named ceiling, never a short guess
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
            crate::probe!(
                class = "delib.gate.lane_wait",
                persona = %self.persona_name,
                directed = ws.directed_at_self(),
                attention = ?ws.attention,
                lanes_available = crate::cognition::resource_admission::serving_lane_permits_available() as u64,
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
                    lane = crate::cognition::resource_admission::acquire_serving_lane(
                        Some(self.persona_id),
                        crate::cognition::resource_admission::LanePriority::Directed,
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
                    crate::cognition::resource_admission::LanePriority::Work
                } else {
                    crate::cognition::resource_admission::LanePriority::Ambient
                };
                tokio::select! {
                    lane = crate::cognition::resource_admission::acquire_serving_lane(Some(self.persona_id), priority) => Some(lane),
                    _ = tokio::time::sleep(lane_bound) => None,
                    _ = crate::cognition::directed_pending::wait(self.persona_id) => {
                        crate::probe!(
                            class = "delib.gate.yielded_to_directed",
                            persona = %self.persona_name,
                            "parked self-work lane wait yielded: a directed line is pending"
                        );
                        // Her CHOICE, not a drop — but the capture lease dies with this
                        // return, so say which (card ebce2ba0).
                        if let Some(lease) = &mut capture {
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
                crate::probe!(
                    class = "persona.act.lane_starved",
                    persona = %self.persona_name,
                    directed = ws.directed_at_self(),
                    waited_secs = lane_wait_started.elapsed().as_secs(),
                    bound_secs = lane_bound.as_secs(),
                    bound_derived = lane_bound_derived,
                    queue_ahead = lane_queue_ahead as u64,
                    lanes_serving = lanes_serving as u64,
                    lane_hold_p50_ms = lane_hold_p50_ms,
                    lanes_available = crate::cognition::resource_admission::serving_lane_permits_available() as u64,
                    bound_source = lane_bound_source.as_str(),
                    turn_bound_secs = turn_bound.map(|b| b.as_secs()).unwrap_or(0), // unwrap_or: 0 = no measured expectation; the queue's estimate governed alone
                    "the lane wait hit its bound (her own measured turn when she has one, else the queue's estimate, floored at LANE_WAIT_FLOOR) — deferring without reaching the model; she keeps her act budget and tries again next tick"
                );
                // NEVER DISPATCHED, AND THE CAPTURE MUST SAY SO (card ebce2ba0). Without
                // this the lease's `Drop` wrote "request future dropped before a terminal
                // response" — the SAME row a generation the model was still producing
                // writes — and eighteen of fifty captures on the M5 read as dropped
                // generations when most of them never reached the model at all.
                if let Some(lease) = &mut capture {
                    lease.abandon(&format!(
                        "lane starved before dispatch: waited {}s of a {}s bound ({}) with {} lanes free",
                        lane_wait_started.elapsed().as_secs(),
                        lane_bound.as_secs(),
                        lane_bound_source.as_str(),
                        crate::cognition::resource_admission::serving_lane_permits_available(),
                    ));
                }
                return None;
            };
            // The node's own queue, measured where SHE waits for it (card c84d885a, S1b):
            // the comparator every spill is judged against.
            crate::cognition::resource_admission::note_local_lane_wait_ms(
                lane_wait_started.elapsed().as_millis() as u64,
            );
            // The reserve's guarantee, measured in its own unit: what a DIRECTED call waited.
            if ws.attention.requires_priority() {
                crate::cognition::resource_admission::note_directed_lane_wait_ms(
                    lane_wait_started.elapsed().as_millis() as u64,
                );
            }
            // HER task-positive system is engaged from here: the per-citizen boredom gate
            // (dreams) reads this stamp, never a room wake.
            crate::cognition::activity_gate::persona_engaged(self.persona_id);
            // COGNITION PULSE per generation, not only per turn start. The claim-renewal
            // gate reads this stamp, and a single service-loop turn can run many acts: on
            // the M5 an act takes 7-9 minutes, so a multi-act turn outlives the 30-minute
            // lease while she is working inside it. Measured 2026-09-26: Aris's turn
            // started 01:07:52, she was acting through lane waits at 01:18 and 01:28, and
            // renewal was denied at 02:01:53 — her live claim on django-15098 read as
            // expired to the fleet (Kimi flagged it as a collision risk). Same class as
            // the detached-solve tick in `agent/solve` (#425): a lane granted for a
            // generation is her thinking this instant, the strongest witness there is.
            crate::persona::cognition_pulse::touch(
                self.persona_id,
                crate::time::EpochMs::now().millis(), // pre-1970 reads 0: a pre-1970 clock stamps 0, which DENIES renewal — the conservative direction, same as no stamp
            );
            crate::ipc::vitals_emitter::record_reasoning(self.persona_id);
            crate::probe!(
                class = "delib.gate.lane_acquired",
                persona = %self.persona_name,
                "lane admission granted — prefill slot next"
            );
            crate::modules::citizen_health::note_lane_granted_to(self.persona_id);
            // #56 prefill throttle: under live external GPU pressure (a game, the browser)
            // fewer than the served lane count may PREFILL concurrently — the instant valve
            // for the 2026-07-16 compute-buffer OOM. Same fit rule the capacity sim proves;
            // no pressure → target == lanes → this never waits. Released with the block.
            let _prefill = crate::cognition::prefill_throttle::acquire_prefill_slot().await;
            crate::probe!(
                class = "delib.gate.prefill_acquired",
                persona = %self.persona_name,
                "prefill slot granted — issuing the model call"
            );
            // THE DROP WITNESS (card ebce2ba0). From here the model IS producing, and
            // the only thing that still runs if this future is torn down is a `Drop`.
            // `InFlight` is that Drop: it counts the loss on the hour's ledger and says
            // what was in flight — the bound this turn stated for itself, how long it had
            // been running, and (on the non-observing path, which owns the stream) how
            // much answer, reasoning and how many COMPLETE tool calls were thrown away.
            // Five generations died here unwitnessed on the M5 2026-09-20 at 1,207,290 /
            // 1,228,814 / 1,491,253 / 1,492,283 / 1,492,456 ms — the act deadline, whose
            // own bound is now derived from the same expectation (`act_bound_with_source`).
            if let Some(sink) = ws.token_sink.as_ref() {
                // The CALLER owns this stream (a live Speak), so the witness cannot see
                // the chunks and reports `observed = false` rather than a false zero.
                let mut witness =
                    crate::cognition::generation_drop::InFlight::arm(&self.persona_name, turn_bound, None);
                let result = binding
                    .adapter
                    .generate_stream_checked(request, sink.clone())
                    .await;
                witness.disarm();
                result
            } else {
                let (sink, receiver) = tokio::sync::mpsc::unbounded_channel();
                // The receiver is no longer thrown away: the witness holds it, so a drop
                // can drain what the model had already produced. Nothing reads it on the
                // healthy path — the accumulate still comes from the returned response.
                let mut witness = crate::cognition::generation_drop::InFlight::arm(
                    &self.persona_name,
                    turn_bound,
                    Some(receiver),
                );
                let result = binding.adapter.generate_stream_checked(request, sink).await;
                witness.disarm();
                result
            }
        };
        // The actual submitted identity survives a provider-owned response ID.
        // Record every completed attempt, including a corrective capacity refusal,
        // independently of whether optional disk capture is installed.
        // The completed entry's cursor, not the submission's: only the completed entry
        // reads back as the request AND her response (card ad107e18).
        let completed = capture.as_mut().and_then(|lease| match &gen_result {
            Ok(response) => lease.finish(Some(response), None),
            Err(error) => lease.finish(None, Some(&error.to_string())),
        });
        crate::cognition::prompt_prefix::settle_reuse(
            self.persona_id,
            &request_id,
            gen_result
                .as_ref()
                .ok()
                .and_then(|r| r.timing.as_ref())
                .map(|t| (t.cached_tokens, t.prefill_tokens)),
        );
        receipts.push(
            match &gen_result {
                Ok(response) => {
                    super::super::provenance::GenerationReceipt::from_response(request_id, response)
                }
                Err(error) => {
                    super::super::provenance::GenerationReceipt::faulted(request_id, error.to_string())
                }
            }
            .with_capture(completed.as_deref())
            .with_genes(genes),
        );
        Some(gen_result)
    }

    pub(super) fn build_request_within(
        &self,
        binding: &ModelBinding,
        // The reply's allowance as the prompt plan granted it
        // (`DeliberationPromptView::completion_reserve`): carried as `max_tokens`
        // verbatim, never re-derived here — the prompt was fitted to leave exactly it.
        allowance: u32,
        messages: Vec<ChatMessage>,
        tools: Option<Vec<NativeToolSpec>>,
        system_prompt: String,
        stop_sequences: Option<Vec<String>>,
        // The room this turn speaks in — the ACTIVITY identity. It is authoritative
        // for the serving slot lease: warm KV is per (persona, room), so a persona
        // running N concurrent activities (N detached solves) each keeps its own warm
        // slot instead of all N collapsing onto one and thrashing (the 2026-08-26
        // KV-reuse-0% bug). None only for the roomless test rig.
        room_id: Option<uuid::Uuid>,
        // The kind of turn — its purpose on the wire and the terms its receipt reports
        // ([`output_allowance`]); its bound was already decided by [`reply_claim`].
        kind: TurnKind,
    ) -> TextGenerationRequest {
        let rate = crate::inference::decode_knee::rate_for(binding.requested_model());
        let need = self
            .working_set
            .as_ref()
            .and_then(|reg| reg.need_of(self.persona_id));
        let derived = output_allowance(kind, rate.tps, need, allowance);
        let max_tokens = derived.allowance;
        crate::probe!(
            class = "delib.turn.output_allowance",
            persona = %self.persona_name,
            kind = kind.label(),
            rate_source = rate.source.label(),
            measured_tps = rate.tps.unwrap_or(0.0), // unwrap_or: 0.0 on the receipt means NO rate — rate_source says "none"; the derivation never used it
            time_term = derived.time_term.map_or(0, u64::from),
            need_measured = need.is_some(),
            reasoning_need = need.map_or(0, |n| u64::from(n.reasoning)),
            answer_need = need.map_or(0, |n| u64::from(n.answer)),
            need_term = derived.need_term.map_or(0, u64::from),
            need_clipped = derived.need_clipped,
            allowance = max_tokens,
            "the turn's output allowance — the reply's flex grant plus what no other claim took, under its runaway bound; time × her rate and her measured think + answer are what she is expected to use"
        );
        TextGenerationRequest {
            messages,
            system_prompt: Some(system_prompt),
            model: binding.model.clone(),
            provider: None,
            // Greedy override (the eval window) wins over her lived warmth so the
            // reward metric is reproducible; `None` (live cognition) → her own
            // configured temperature. Wait-free read, like `genome` below.
            temperature: Some((**self.decoding.load()).unwrap_or(self.temperature)),
            // Generation is bounded to the room the prompt plan GRANTED it — the
            // reply's flex allocation, the same value `prompt_view` subtracts from the
            // served window. This is the ONE place the two must agree: the prompt is
            // sized to leave this many tokens, so the reply is allowed exactly this
            // many. Left unbounded (`None`), a verbose turn overruns the reserve,
            // `prompt + completion` reaches `n_ctx`, and llama-server (started with
            // `--embeddings`, so context-shift is off) returns 500 "Compute error" —
            // muting the persona for the whole tick. This is NOT the flat cap that
            // truncated qwen3.5 mid-`<think>`: the grant comes out of the real served
            // window by priority against everything else in it, so the reply gets every
            // token allocated to it — never a const we picked, never an overrun ([[fallbacks-are-illegal-fail-loud]]).
            max_tokens: Some(max_tokens),
            top_p: None,
            top_k: None,
            // `None` here does NOT mean "no repetition penalty" — it defers to the
            // adapter, which for llama.cpp-family gateways (selected by the TYPED
            // `llamacpp_sampling_extensions` capability, never a provider-name match)
            // forwards `repeat_penalty` to the server, defaulting to 1.1 when we pass
            // None. That 1.1 is load-bearing: at llama.cpp's 1.0 default (disabled) a
            // small model can collapse into degenerate repetition — reprinting one line
            // to the token budget for a multi-minute, truncated turn (seen in the glass
            // box). Cloud OpenAI-compat providers don't accept the non-standard field,
            // so they correctly leave it off. TODO(#76): lift the penalty (like
            // temperature) onto the Model row so it's a per-model default the faculty
            // passes through, not an adapter magic number.
            repeat_penalty: None,
            frequency_penalty: None,
            repeat_last_n: None,
            // #150: peer-name turn-boundary stops when the caller derived them
            // from the burst (`peer_stop_sequences`) — generation ends where
            // her turn ends, never continuing the transcript as a teammate.
            stop_sequences,
            tools,
            tool_choice: None,
            response_format: None,
            // Page in the persona's current genome (wait-free read). Empty → None,
            // so the base model runs with no LoRA; a paged-in gene rides into the
            // request as `active_adapters`, which the adapter injects as
            // `"lora":[{id,scale}]`. This is the measured page-in seam.
            active_adapters: {
                let genome = self.genome.load();
                (!genome.is_empty()).then(|| genome.as_ref().clone())
            },
            request_id: None,
            user_id: None,
            room_id: room_id.map(|r| r.to_string()),
            // An ACT turn (the hands surface is offered) announces itself: the body
            // builder bounds the model's thinking on it (card 12ef9c10), the lane
            // class stays Turn. A PASS keeps the model's own thinking policy — there
            // is no per-request thinking switch on this type; its allowance holds the
            // measured think instead ([`output_allowance`]).
            purpose: Some(
                match kind {
                    TurnKind::Act => crate::inference::request_body::ACT_PURPOSE,
                    TurnKind::Pass => crate::inference::request_body::DELIBERATION_PURPOSE,
                }
                .to_string(),
            ),
            persona_id: Some(self.persona_id.to_string()),
            // The turn's bound on the wire: her measured expectation with headroom
            // (card ba82d0a0). Every waiting seam takes max(its floor, this).
            turn_bound: self.turn_bound(),
        }
    }
}

/// Lift the speed/latency cost of one generation off the adapter response — the
/// measured wall-clock + the prompt/completion token counts. The brain stamps this
/// onto its verdict [`Contribution`] so latency and throughput leave the mind on
/// the same path as the decision, and the settle loop folds it into the per-task
/// total. Token counts are 0 when the gateway omitted `usage` (older endpoints);
/// `latency_ms` is always present (the adapter times every request).
/// A mind's last generation, as the next work call's occupancy bound reads it.
#[derive(Debug, Clone, Copy)]
pub(super) struct TurnShape {
    pub(super) input: u32,
    pub(super) cached: u32,
    pub(super) output: u32,
}
