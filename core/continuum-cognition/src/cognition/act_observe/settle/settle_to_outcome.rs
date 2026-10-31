use super::*;

/// The settle loop itself. Both public drivers reach it through the same lived-
/// experience wrapper, so taking input cannot bypass the recording seam.
pub(super) async fn settle_to_outcome<'c>(
    cycle: &WorkspaceCycle,
    mut burst: Burst,
    max_acts: usize,
    mut framing: TurnFraming,
    mut conversation: Option<&mut dyn crate::cognition::conversation::PersonaConversation>,
    mut credit: Option<&mut (dyn TurnCredit + 'c)>,
) -> SettleOutcome {
    // The turn's room comes FROM the burst — witnessed non-nil at construction,
    // so the drive can no longer disagree with the rendered header (#425).
    let room_id: Uuid = burst.room.as_uuid();
    let mut acts = 0usize;
    let mut turn_acts: Vec<(String, Vec<crate::ai::types::ToolCall>)> = Vec::new();
    // Rolling act-duration cadence for the inline pace verdict below. ACTS only —
    // a lane wait is not a sample of her pace (see [`PaceCadence`]).
    let mut pace = PaceCadence::default();
    // This turn's causal thread: each admitted act observation becomes the
    // CausedBy target of the next act in the SAME chain — the driver owns the
    // chain, so an edge can never cross turns or rooms (CAUSAL-MEMORY-GRAPH.md).
    // ROOTED in what caused this turn, so the first act chains to its trigger rather
    // than starting mid-air — the link that makes "which acts were done for this card"
    // a graph query instead of an inference.
    let chain = super::super::apply::ActChain::rooted_in(&burst.cause).with_effects(burst.effects.clone());
    // How many turns actually run with a head on their thread — the measurement that
    // tells us whether the causal graph is CONNECTED in the live system, rather than
    // connected in the one path I happened to wire by hand
    // ([[an-absence-is-an-unfinished-measurement]]). An `ambient` row is not a fault;
    // it is an idle tick whose stimulus the projection layer discarded.
    continuum_base::probe!(
        class = "engram.chain.rooted",
        cause = burst.cause.as_str(),
        room = %room_id,
        rooted = burst.cause.root().is_some(),
        "turn's causal thread begins here"
    );
    // The turn's investigation trail (see `SettleOutcome::touched_paths`).
    let mut touched: Vec<String> = Vec::new();
    // Fold each tick's deliberation cost in, so the settled outcome reports the
    // task's TOTAL speed/latency (a multi-act task pays for every generation).
    let mut metrics = TurnMetrics::default();
    // Accumulated exactly like `metrics`, for the same reason: a turn is N
    // deliberations and no single one is canonical (card 0d51573a).
    let mut generation_receipts: Vec<crate::ai::provenance::GenerationReceipt> = Vec::new();

    // Admission uses the caller's budget and lifecycle deadlines. Identical inputs
    // do not establish a stuck mind: repeated reads/retries can observe new state.
    // Bound only a repeated input AND actual result, after executing the retry.
    // This preserves unlimited productive work while yielding a fixed-point loop.
    const STUCK_LIMIT: usize = 3;
    let mut prior_evidence = None;
    let mut stuck = 0usize;
    // A workspace-deliverable turn re-perceives on a zero-deliverable Speak (see the Spoke
    // arm). This used to be ONE-SHOT, and the glass box showed what that costs: on
    // sympy-21379 `persona.settle.no_deliverable` fired exactly once per run and she then
    // settled at 5-7 acts with a 30-act budget unspent. The latch was guarding against a
    // retry loop, but it also capped her at a single reminder for the whole turn.
    //
    // The bound that actually prevents a loop without capping her: re-fire only if she has
    // ACTED since the last nudge. A nudge that earns an act has earned another; two Speaks
    // in a row with no act between them means the nudge is not working, so she settles
    // rather than being trapped. Her own behavior is the budget — no counter, no constant.
    let mut narrations_since_act: usize = 0;
    let mut acts_at_last_nudge: Option<usize> = None;
    // Which `acts` value the last budget fact fired at — a Speak's one-shot
    // re-perception re-enters the loop with `acts` unchanged, and the same
    // milestone must not stamp twice.
    let mut budget_fact_at: Option<usize> = None;

    // DISCOVERY SATURATION GATE (#390) — the STATE escalation of the [no-deliverable]
    // fact above, built on its own measured failure: the fact fires on EVERY act with
    // no mutation receipt (see the Acted arm), and a live run still spent 15+ varied
    // reads on the right file with a zero-byte diff — the fact was perceived and
    // changed nothing ([[perception-facts-fire-correctly-and-change-nothing-only-
    // state-changes-behaviour]]). Varied reads never trip the #206 identical-act
    // backstop (each has a fresh signature), so the only bound they ever hit is
    // `max_acts` — at which point the budget is gone and the empty-diff re-drive in
    // `agent/solve` structurally cannot fire. The state fix: on a turn whose
    // DELIVERABLE IS THE WORKSPACE, discovery may spend at most HALF the act budget
    // before the first workspace mutation; past that, acts are withheld (same
    // `may_act` lever as #206) so the drive ends with REAL remaining budget — which
    // is exactly what the empty-diff re-drive needs to hand back with the structural
    // fact. The first mutation lifts the gate for the rest of the turn: iteration
    // (edit → run tests → edit) is never bounded, only pre-write wandering. Not a
    // steer — it never says what to write, exactly like `max_acts`; it converts an
    // unbounded read loop into a decision point while the budget can still buy the
    // decision. Non-workspace turns (chat, research) are untouched.
    // 3/4 of the budget, not 1/2 (2026-08-24): at /2 the gate fired at act 16
    // of 32 on tasks whose honest STUDY phase needs more (126-case reverse
    // engineering) — she "quit" at half budget because the gate quit for her,
    // and the red-build re-drive's fresh turn hit its own gate identically.
    // The gate still bounds a pure-read runaway (a full-budget read turn ends
    // withheld, and the pre-gate warning now lands 4 acts before THIS bound —
    // genuinely before, not at, the cliff). Plumbing must never out-stubborn
    // the engineer it serves.
    // Divide BEFORE multiply so this can never overflow: a self-tick (non-benchmark)
    // turn passes `max_acts = usize::MAX` (the "unlimited acts" sentinel), and the old
    // `max_acts * 3` overflowed usize → a debug-build panic that aborted EVERY persona
    // service loop the instant it self-ticked, so residency could never hold
    // (resident_count flapped to 0, #412 root). `(max_acts / 4)` is always ≤ max_acts, so
    // `* 3` stays in range; `saturating_mul` is belt-and-suspenders. 3/4 of the budget,
    // overflow-safe.
    let discovery_budget = (max_acts / 4).saturating_mul(3).max(1);
    let mut mutated_yet = false;
    let mut saturation_probed = false;

    // #386: transient-deliberation retry. Glass-boxed on atlas-17139-h1
    // (2026-08-09): the per-slot wedge fails ~1 in 3 generations and the VERY
    // NEXT generation succeeds — capture tick 2 faults, tick 3 acts, tick 5
    // faults, tick 6 acts. The old behaviour abandoned the ENTIRE turn on the
    // first fault, so a single transient flicker cost her a whole attempt. A
    // faulted thought is retried in place, bounded, before the turn is
    // surrendered — the flicker costs one re-generation, not the turn. Only a
    // SUSTAINED fault (budget spent across the turn) ends the turn with
    // failure, exactly as before — 4+ faults in one turn IS a wedge worth
    // surrendering to, not a flicker. Whole-turn budget, not per-tick.
    const DELIBERATION_RETRY_BUDGET: u32 = 3;
    let mut delib_retries: u32 = 0;

    let mut first_step = true;
    let mut seen_inputs = None;
    let mut input_watermark = 0;
    loop {
        // THE NODE IS STOPPING: end at this step boundary (card 32fa22ba, Fable on #4684).
        // The previous act's generation finished and its observation landed; starting the
        // next one would hold the stop's settle for a whole further generation, and a long
        // tool loop for its whole budget. Nothing is torn — the turn ends between acts with
        // what it did recorded (`turn_acts`), and the next core picks the work up from the
        // room. Only after an act: a turn that has not acted yet is the settle's business
        // as a whole turn, and is never admitted once the door has closed.
        if acts > 0 && crate::cognition::turn_ingress::is_closing() {
            continuum_base::probe!(
                class = "cognition.turn.ended_for_stop",
                room = %room_id,
                acts = acts,
                "the node is stopping — the turn ended at an act boundary instead of starting its next generation"
            );
            return SettleOutcome {
                room: room_id,
                decision: Decision::pass(),
                spoken: None,
                acts,
                world_state: burst.rendered.clone(),
                room_updates: Arc::clone(&burst.room_updates),
                metrics,
                generation_receipts: generation_receipts.clone(),
                failure: None,
                deferred_before_dispatch: false,
                capacity_refusal: None,
                touched_paths: touched,
                turn_acts: turn_acts.clone(),
            };
        }
        // THE ACT'S DEADLINE IS SIZED FROM THE TURN IT MUST HOLD (card ebce2ba0).
        // `TICK_DEADLINE` was a flat 25 minutes, and on the M5 2026-09-20 it reaped five
        // generations MID-FLIGHT at 1,207,290 / 1,228,814 / 1,491,253 / 1,492,283 /
        // 1,492,456 ms of elapsed generation — the deadline expiring, less however far
        // into the act the model call had started. Each of those left a `cancelled`
        // capture reading "request future dropped before a terminal response": the
        // substrate reaping its own healthy work, then reading the silence as a lazy
        // citizen. A turn whose own stated bound (the `turn_bound` the request carries,
        // #4277) exceeds the act's cannot finish inside the act.
        //
        // So the constant becomes a FLOOR under her measured expectation, never a
        // ceiling over it — the same rule every waiting seam below already follows.
        // `act_bound_with_source` covers TWO turn bounds because an act is a lane wait
        // PLUS the generation after it, and the serving gate's own wait is one.
        let (act_bound, act_bound_source) = crate::inference::turn_bound::act_bound_with_source(
            TICK_DEADLINE,
            cycle.acting().and_then(|body| {
                crate::cognition::llm_deliberation_faculty::expected_occupancy_for(body.persona_id)
            }),
        );
        let tick_deadline = tokio::time::Instant::now() + act_bound;
        if !first_step {
            if let (Some(input), Some(body)) = (conversation.as_deref_mut(), cycle.acting()) {
                let received = match tokio::time::timeout_at(tick_deadline, input.perceive_ready()).await {
                    Ok(received) => received,
                    Err(_elapsed) => Err("tick deadline exceeded during room input admission; input remains owned by the conversation".into()),
                };
                let admitted = received.and_then(|messages| {
                    let recipient = crate::cognition::conversation::persona_identity::PersonaIdentity::new(
                        body.persona_id, body.persona_name.clone(),
                    );
                    for message in messages {
                        if message.peer_id == body.persona_id
                            || crate::cognition::conversation::seen_ids::is_stale(
                                message,
                                seen_inputs.get_or_insert_with(|| crate::cognition::conversation::seen_ids::SeenIds::new(
                                    crate::cognition::conversation::INBOX_CAPACITY,
                                )),
                                input_watermark,
                            )
                        {
                            continue;
                        }
                        input_watermark = input_watermark.max(message.lamport);
                        let inbox_message = crate::cognition::conversation::types::InboxMessage {
                            id: message.event_id,
                            room_id: message.room_id,
                            sender_id: message.peer_id,
                            sender_name: message.peer_id.to_string(),
                            sender_type: crate::cognition::conversation::types::SenderType::Persona,
                            content: message.text.clone(),
                            timestamp: EpochMs::now().millis(),
                            priority: 0.5,
                            source_modality: None,
                            voice_session_id: None,
                        };
                        body.admission.admit(&inbox_message, None)
                            .map_err(|error| format!("room input admission failed: {error}"))?;
                        framing.attention = framing.attention.with_input(
                            crate::cognition::workspace::TurnAttention::for_message(
                                recipient.mentions(&message.text),
                                crate::cognition::room_directory::is_human_peer(message.peer_id),
                            ),
                        );
                        continuum_base::probe!(
                            class = "persona.turn.input_perceived",
                            persona = %body.persona_id,
                            active_room = %room_id,
                            input_room = %message.room_id,
                            event_id = %message.event_id,
                            from_peer = %message.peer_id,
                            "room input perceived between actions; original task and causal root retained"
                        );
                        crate::cognition::extensions::mark_perceived(
                            body.persona_id,
                            message.room_id,
                            message.lamport,
                            Some(message.event_id),
                            EpochMs::now().millis(),
                        );
                        Arc::make_mut(&mut burst.room_updates).push(Arc::clone(message));
                    }
                    Ok(())
                });
                if let Err(error) = admitted {
                    return SettleOutcome {
                        room: room_id,
                        decision: Decision::pass(),
                        spoken: None,
                        acts,
                        world_state: burst.rendered.clone(),
                        room_updates: Arc::clone(&burst.room_updates),
                        metrics,
                        generation_receipts: generation_receipts.clone(),
                        failure: Some(SettlementFailure::Inference(error)),
                        deferred_before_dispatch: false,
                        capacity_refusal: None,
                        touched_paths: touched,
                        turn_acts: turn_acts.clone(),
                    };
                }
            }
        }
        first_step = false;
        // ONE settlement step through the SHARED primitive the live heartbeat uses
        // (`settle_step`). The only thing this driver adds is the LOOP — because the
        // eval room has no metronome, the grader re-perceives by calling step again.
        // `may_act = acts < max_acts` gates ACTING (not speaking): past the budget
        // she may still settle into a Speak, but a fresh Act is returned un-driven.
        //
        // The tick's SITUATION is the real signal that makes context lean when she's
        // heads-down: the first tick is a fresh ask (`FreshContext`, fuller
        // grounding); every tick AFTER an act has landed re-perceives a tool result
        // (`PostAction`), so the focuser drops the standing re-grounding and the
        // result + working memory own the window. Derived from `acts`, never from the
        // burst text.
        let situation = if acts == 0 {
            Situation::FreshContext
        } else {
            Situation::PostAction
        };
        // ACT-BUDGET PROPRIOCEPTION (2026-08-23). She could never see her own
        // stopwatch: the budget gated acts SILENTLY, so an engineer's rational
        // analysis-first plan burned the whole default budget and was graded
        // with an empty src/ (MirrorCode baseline — the cap measured our
        // patience, not her skill; an engineer who can see "2 acts left"
        // triages, one who can't cannot). Three structural facts — the
        // contract at turn start, the midpoint, and two-remaining — state the
        // TRUE shape of the turn and nothing else: no file, no fix, no next
        // tool. Pacing stays entirely her decision.
        // [[no-hardcoded-heuristics-to-steer-cognition]]
        if framing.workspace_deliverable && budget_fact_at != Some(acts) {
            if let Some(body) = cycle.acting() {
                // The probe is the RECEIPT that the fact reached her working
                // memory — record_fact leaves no structured trace, and "the
                // stopwatch is visible to her" was unprovable from the live
                // stream the day it shipped ([[observability-as-substrate]]).
                let probe_budget_fact = |milestone: &str| {
                    continuum_base::probe!(
                        class = "persona.act_budget.fact",
                        persona = body.persona_name.as_str(),
                        milestone = milestone,
                        acts = acts,
                        max_acts = max_acts,
                        "act-budget proprioception fact recorded in working memory"
                    );
                };
                // PRE-GATE WARNING (2026-08-23, measured twice on bib2json):
                // at a 32-act budget the midpoint fact and the discovery-
                // saturation gate BOTH land at act 16 — her first pacing
                // signal arrived in the same instant her acts were withheld,
                // so a spec-study opening could never convert in time. The
                // gate's contract becomes perceptible BEFORE it binds: a
                // structural fact at 4 acts out (workspace turns with no
                // mutation yet). States the turn's real rule; names no file,
                // no fix, no tool.
                let gate_approaching = framing.workspace_deliverable
                    && !mutated_yet
                    && discovery_budget >= 4
                    && acts == discovery_budget - 4;
                if gate_approaching {
                    budget_fact_at = Some(acts);
                    probe_budget_fact("pre_gate");
                    body.working_memory.record_fact(&format!(
                        "[act-budget] {acts} acts spent, none has changed a file yet.                          This turn's contract withholds further acts after                          {discovery_budget} total unless the workspace has been                          written to — the deliverable is the workspace change, and                          unwritten work is not part of it."
                    ));
                } else if acts == 0 {
                    budget_fact_at = Some(acts);
                    probe_budget_fact("turn_start");
                    body.working_memory.record_fact(&if max_acts == usize::MAX {
                        "[act-budget] This turn has no fixed act budget: I act until I settle. \
                         The deliverable is the workspace as I leave it when I settle — \
                         work not yet written when I settle is not part of it."
                            .to_string()
                    } else {
                        format!(
                            "[act-budget] This turn grants me {max_acts} act→observe \
                             cycles before I must settle. The deliverable is the workspace \
                             as I leave it when I settle — work not yet written when the \
                             budget ends is not part of it."
                        )
                    });
                } else if acts == max_acts / 2 {
                    budget_fact_at = Some(acts);
                    probe_budget_fact("midpoint");
                    body.working_memory.record_fact(&format!(
                        "[act-budget] I have spent {acts} of my {max_acts} acts this turn."
                    ));
                } else if max_acts.saturating_sub(acts) == 2 {
                    budget_fact_at = Some(acts);
                    probe_budget_fact("two_remaining");
                    body.working_memory.record_fact(&format!(
                        "[act-budget] {acts} of {max_acts} acts spent — 2 remain before I \
                         must settle."
                    ));
                }
            }
        }
        // may_act gates ACTING (not speaking): past the act budget, or once a workspace-deliverable turn has
        // saturated its discovery budget without a single mutation (#390 gate above), a
        // fresh Act is returned un-driven and she must settle into a Speak/Pass.
        // Speaking is never gated.
        let discovery_open =
            !framing.workspace_deliverable || mutated_yet || acts < discovery_budget;
        if !discovery_open && acts < max_acts && stuck < STUCK_LIMIT && !saturation_probed {
            saturation_probed = true;
            continuum_base::probe!(
                class = "persona.settle.discovery_saturated",
                room_id = %room_id,
                acts = acts,
                discovery_budget = discovery_budget,
                max_acts = max_acts,
                "workspace-deliverable turn spent its discovery budget with zero \
                 mutations — withholding further acts so the remaining budget reaches \
                 the empty-diff re-drive instead of being read away (#390 state gate)"
            );
        }
        let may_act = acts < max_acts && stuck < STUCK_LIMIT && discovery_open;
        let act_started = std::time::Instant::now();
        continuum_base::probe!(
            class = "settle.tick.start",
            room = %room_id,
            acts_so_far = acts as u64,
            "settle loop iterating — next receipt is this tick's workspace run"
        );
        // PER-TICK DEADLINE (2026-08-29) — the reaper the becalmed week demanded.
        // Five distinct wedge sites in three days, each parking ONE await inside a
        // tick with no bound: memory-era lease, faculty barrier, permit convoy,
        // dream inversion, and finally a park past drive.start that mooted the
        // per-seam chase. A held solve that wedges is a SYSTEM-WIDE MUTE (its
        // measured hold defers all ambient cognition), so a tick is bounded the
        // way every RTOS task is: generously above the slowest honest tick
        // (Flash-Next deep tick ≈ 10-15 min incl. tools), fatally below forever.
        // Elapse → loud infra outcome; the drive ends; the hold RELEASES; resume
        // retries; nothing stays silently becalmed again.
        let acting_persona = cycle.acting().map(|body| body.persona_id);
        // A generation the deadline dropped in flight leaves no receipt, but it reached the
        // model: counted below so the step is never classified as a lane wait (card 843d5570;
        // enrico 2026-10-26, Eitan's generation ran on slot 1 for 75 min, was extended twice,
        // and the act read `lane_starved` because the drop left zero receipts).
        let mut dropped_in_flight = false;
        let (step, step_metrics, step_receipts) = match until_act_stalls(
            tick_deadline,
            act_bound,
            acting_persona,
            // Boxed: the step is the largest future in the drive, and holding it by value
            // inside this wrapper's own state overflowed the layout query depth (CI,
            // "queries overflow the depth limit"). The box keeps the wrapper one pointer wide.
            Box::pin(settle_step(cycle, burst.clone(), may_act, framing, situation, &chain)),
        )
        .await
        {
            Ok(r) => r,
            Err(abandoned) => {
                dropped_in_flight = abandoned.dispatched;
                continuum_base::probe!(
                    class = "settle.tick.deadline",
                    room = %room_id,
                    acts_so_far = acts as u64,
                    dropped_in_flight = dropped_in_flight,
                    deadline_s = act_bound.as_secs(),
                    // Which sized it: `turn_bound` = her measured turn raised the floor,
                    // `floor` = she has no measured turn yet and the constant governed.
                    // A `floor` row on a slow box is the next thing to fix.
                    bound_source = act_bound_source.as_str(),
                    floor_s = TICK_DEADLINE.as_secs(),
                    "tick exceeded its deadline — ending the turn LOUDLY as infra (never a capability verdict); the hold releases with the drive. Any generation in flight is dropped and says so on persona.generation.dropped"
                );
                (
                    SettleStep::InferenceFailed {
                        error: format!(
                            "tick exceeded {}s deadline ({}) at act {} — an in-tick await parked                              (infra), turn ended loudly so the measured hold releases",
                            act_bound.as_secs(),
                            act_bound_source.as_str(),
                            acts
                        ),
                    },
                    None,
                    // The tick was killed by the watchdog before the Workspace returned, so
                    // there is no receipt to carry; a generation dropped in flight is
                    // counted by `dropped_in_flight`, not here.
                    Vec::new(),
                )
            }
        };
        // This act's model wall-time, captured before the accumulate consumes
        // the metrics — the pace row below splits act time into model vs
        // residue with it.
        let act_model_ms = step_metrics.as_ref().map(|m| m.latency_ms).unwrap_or(0);
        // How many generations THIS step dispatched — counted before the receipts are
        // folded into the turn's, because it is the other half of "did this iteration
        // reach the model at all" (`classify_act`).
        let act_generations = step_receipts.len() + usize::from(dropped_in_flight);
        if let Some(m) = step_metrics {
            metrics.accumulate(m);
        }
        // Order is dispatch order across the whole turn: each step appends its own.
        generation_receipts.extend(step_receipts);
        // INLINE PACE VERDICT (Joel 2026-08-23: "know immediately if a model is
        // being slow as molasses, looping, thrashing, not even starting" — the
        // states were entering WITHOUT visibility and a human had to pester).
        // Every act stamps its wall-clock against this turn's own rolling mean;
        // the verdict is computed WHERE THE WORK HAPPENS, event-based, so
        // slow/stalled is a probe row the moment it occurs, never a discovery.
        // No constants deciding cognition: "slow" is relative to THIS turn's
        // own pace (2x rolling mean, min 3 samples), and the row always carries
        // the raw numbers so a dashboard can re-judge.
        // Capacity is an assembly refusal, not a lane wait or a pace sample.
        // Return before zero-generation classification; preserve all prior work.
        if let SettleStep::CapacityRefused { error, .. } = &step {
            return SettleOutcome {
                room: room_id,
                decision: Decision::pass(),
                spoken: None,
                acts,
                world_state: burst.rendered.clone(),
                room_updates: Arc::clone(&burst.room_updates),
                metrics,
                deferred_before_dispatch: can_defer_input(
                    generation_receipts.len(),
                    acts,
                    turn_acts.len(),
                    delib_retries,
                ),
                capacity_refusal: Some(error.clone()),
                generation_receipts,
                failure: None,
                touched_paths: touched,
                turn_acts,
            };
        }
        let act_class = classify_act(act_model_ms, act_generations);
        {
            let act_secs = act_started.elapsed().as_secs_f64();
            // THE SAMPLE IS AN ACT, NEVER A WAIT. The row still fires for both — a
            // starved iteration is not deleted from the ledger, it is CLASSIFIED —
            // but only an act moves the mean it is judged against.
            pace.record(act_secs, act_class);
            let mean = pace.mean_secs();
            let slow = pace.is_slow(act_secs);
            if act_class == ActClass::LaneStarved {
                let who = cycle
                    .acting()
                    .map(|b| b.persona_name.clone())
                    .unwrap_or_default(); // unwrap_or_default: a cycle with no acting body is nobody's turn; the empty name is the truth, not a guess
                // A WAIT IS NOT AN ACT. Its own class, naming the mind, the wait, and
                // how many lanes were free while she waited — so "8 acts, 0 writes"
                // can never again read as eight turns of work when two of them were a
                // mind parked at the serving gate.
                continuum_base::probe!(
                    class = "persona.act.lane_starved",
                    persona = who.as_str(),
                    room_id = %room_id,
                    act = acts,
                    waited_secs = act_secs as u64,
                    lanes_available = crate::inference::resource_admission::serving_lane_permits_available() as u64,
                    model_ms = act_model_ms,
                    generations = act_generations as u64,
                    "this iteration never reached the model — she waited and produced nothing; it is NOT counted as an act"
                );
                crate::cognition::health_ledger::note_lane_starved();
            }
            continuum_base::probe!(
                class = "persona.act.pace",
                room_id = %room_id,
                act = acts,
                act_secs = act_secs as u64,
                rolling_mean_secs = mean as u64,
                slow = slow,
                // THE LEDGER SPLIT (restore-economy VDD): model_ms is this act's
                // generation wall-time (the adapter's own measurement, riding up
                // through StepMetrics); residue_ms is everything else the act
                // spent — tool execution, RAG assembly, settle bookkeeping.
                // Before this split the ~29s/act residue was only derivable by
                // subtracting log aggregates; a stall hiding in tools vs a stall
                // hiding in the model were the same number. Now each act names
                // where its time went, per-act, at the moment it happens.
                model_ms = act_model_ms,
                residue_ms = ((act_secs * 1000.0) as u64).saturating_sub(act_model_ms),
                // THE CLASSIFICATION, on the row itself: `reached_model=false` is a
                // WAIT, not an act, and `pace_samples` says how many acts the mean
                // beside it actually stands on.
                reached_model = act_class == ActClass::Act,
                pace_samples = pace.samples() as u64,
                "act pace vs this turn's own rolling mean — slow/looping visible the moment it happens"
            );
        }
        match step {
            SettleStep::Spoke(text) => {
                // THE SETTLE ARTERY (glass-boxed 2026-08-04, sympy-21379): every perception
                // fact the Speak arm records — [unfulfilled], [unacted], [unobserved],
                // [confabulation] — lands in working memory AFTER the decision. On this
                // path the Speak returned immediately, so she never got a tick to PERCEIVE
                // her own diagnosis. The machinery wrote it; nobody read it. Live specimen:
                // one `code/tree`, then a Speak explaining the bug to the user in prose —
                // acts 1, patch 0 bytes, budget unspent, run over.
                //
                // When the caller declared the deliverable to be the WORKSPACE, a Speak
                // that changed no file has produced nothing the grader will ever see. Hand
                // her ONE re-perception carrying that structural fact, then let her decide.
                // Bounded to one: if she speaks again she settles, so the ceiling is a
                // single extra generation and a determined Speak is never trapped.
                //
                // Why this is substrate, not scaffolding: the fact is TRUE and structural
                // (the caller declared the contract; working memory holds no mutation
                // receipt), it names no file, no fix, and no next tool, and the decision
                // stays entirely hers — the same shape as every other proprioception fact
                // in `settle_step`. [[no-hardcoded-heuristics-to-steer-cognition]],
                // [[fix-the-substrate-never-rig-the-persona-the-line-between-assist-and-scaffold]].
                // NARRATION IS NOT A SETTLEMENT (2026-09-05): a workspace-deliverable
                // turn that speaks without having changed a file is re-perceived up to
                // NARRATION_BUDGET consecutive times, not once. Measured on the team
                // round: each work turn was one act and a spoken plan, the second plan
                // ended the turn, the next turn re-oriented — 29 acts, 0 writes in 70
                // minutes on 12 held cards. The fact names the count so pacing stays
                // hers; the caller budget and lifecycle deadlines still apply.
                if framing.workspace_deliverable && narrations_since_act < NARRATION_BUDGET {
                    if let Some(body) = cycle.acting() {
                        if !mutated_workspace(&body.working_memory.recent_entries(), ConcernPhase::JustSettled) {
                            narrations_since_act += 1;
                            acts_at_last_nudge = Some(acts);
                            // Sense, not steer (2026-09-01): the receipt-absence is
                            // the fact; the "an explanation of a fix is not the fix"
                            // sermon accumulated dozens of copies in looping minds
                            // and became the content of their turns.
                            body.working_memory.record_fact(&format!(
                                "[no-deliverable] I settled by speaking ({narrations_since_act} of \
                                 {NARRATION_BUDGET} plans in a row); no act of mine has changed a \
                                 file in this workspace yet."
                            ));
                            continuum_base::probe!(
                                class = "persona.settle.no_deliverable",
                                persona = %body.persona_name,
                                room_id = %room_id,
                                acts = acts,
                                "workspace-deliverable turn spoke with no mutation receipt — recorded the fact and re-perceived (re-fires only after she acts again)"
                            );
                            continue;
                        }
                    }
                }
                return SettleOutcome {
                    room: room_id,
                    spoken: Some(text.clone()),
                    decision: Decision::Speak { text },
                    acts,
                    world_state: burst.rendered.clone(),
                    room_updates: Arc::clone(&burst.room_updates),
                    metrics,
                    generation_receipts: generation_receipts.clone(),
                    failure: None,
                    deferred_before_dispatch: false,
                    capacity_refusal: None,
                    touched_paths: touched,
                    turn_acts: turn_acts.clone(),
                };
            }
            SettleStep::Acted { calls, intent } => {
                acts += 1;
                turn_acts.push((intent.clone(), calls.clone()));
                if let Some(capture) = credit.as_deref_mut() {
                    capture
                        .record(&turn_acts, &generation_receipts, None, false)
                        .await;
                }
                narrations_since_act = 0;
                collect_touched_paths(&mut touched, &calls);
                // Latch the #390 discovery gate OPEN on the first workspace mutation:
                // once she has written anything, iteration is hers for the whole
                // remaining budget. Same receipt predicate as the [no-deliverable]
                // fact — one truth for "did an act change a file".
                if framing.workspace_deliverable && !mutated_yet {
                    if let Some(body) = cycle.acting() {
                        mutated_yet = mutated_workspace(&body.working_memory.recent_entries(), ConcernPhase::Ongoing);
                    }
                }
                // THE DELIVERABLE FACT BELONGS ON THE ACT PATH, NOT ONLY ON SETTLE.
                //
                // It used to live exclusively in the Spoke arm, so it could only reach a
                // persona who SETTLED. Measured on sympy-21379 v14: she spent all 30 acts, was
                // still acting when the budget ran out, and the driver returned the final Act
                // un-driven — the Spoke arm never ran, the fact never fired once, and she
                // finished a full-length run having changed no file without ever being told
                // that changing files was the point. The one population that needs the
                // reminder — a mind thrashing through its whole budget — was the one the
                // mechanism structurally could not reach.
                //
                // Now it rides the re-perception after every act. Same truth, same veto on
                // spam (`acts_at_last_nudge` re-arms only when she has acted since), and the
                // act count is IN the text so consecutive facts are not byte-identical — a
                // stationary perception is a fixed point under greedy decoding, which is the
                // #206 failure this file already documents.
                if framing.workspace_deliverable && acts_at_last_nudge != Some(acts) {
                    if let Some(body) = cycle.acting() {
                        if !mutated_workspace(&body.working_memory.recent_entries(), ConcernPhase::Ongoing) {
                            acts_at_last_nudge = Some(acts);
                            // Sense, not steer — same contract as the settle-path fact.
                            body.working_memory.record_fact(&format!(
                                "[no-deliverable] {acts} actions taken on this task; no act \
                                 of mine has changed a file in this workspace yet."
                            ));
                            continuum_base::probe!(
                                class = "persona.act.no_deliverable_yet",
                                persona = %body.persona_name,
                                room_id = %room_id,
                                acts = acts,
                                "acted with no workspace mutation receipt yet — recorded the fact on the act path (the settle path cannot reach a budget-exhausted turn)"
                            );
                        }
                    }
                }
                let evidence = cycle.acting().and_then(|body| {
                    batch_evidence(&calls, &body.working_memory.recent_acts())
                });
                if evidence.is_some() && evidence == prior_evidence {
                    stuck += 1;
                } else {
                    stuck = 0;
                }
                prior_evidence = evidence;
                if stuck >= STUCK_LIMIT {
                    continuum_base::probe!(
                        class = "persona.settle.stuck_backstop",
                        room_id = %room_id,
                        acts = acts,
                        stuck = stuck,
                        "same calls and actual results repeated; yielding without claiming completion"
                    );
                    if let Some(body) = cycle.acting() {
                        body.working_memory.record_fact(
                            "[repetition] Consecutive executions returned the same results for \
                             the same requests. This turn yields; the work is not declared complete."
                        );
                    }
                }
                // The observation re-enters perception through MEMORY + the volatile
                // working-memory recency channel — `apply_act` admitted it and
                // recorded a stamped proprioception trace, and the next `settle_step`
                // re-perceives. This is BYTE-FOR-BYTE the live heartbeat motion
                // (service_loop apply via the SAME `settle_step`, then the metronome
                // re-perceives next tick). `world` is held CONSTANT across iterations:
                // memory is the only thing that changes, exactly as in life — no
                // eval-only `[you just acted]` fold. See
                // [[act-results-need-a-recency-channel-not-semantic-recall]].
            }
            // Budget spent on a fresh Act, OR the act could not be carried out (no
            // hands / exec error). Either way she did not settle in the observer's
            // window — return the un-driven Act so the grader scores it as unfinished,
            // never a fabricated answer.
            SettleStep::WouldAct { calls, intent }
            | SettleStep::ActUnfulfilled { calls, intent } => {
                return SettleOutcome {
                    room: room_id,
                    decision: Decision::Act { calls, intent },
                    spoken: None,
                    acts,
                    world_state: burst.rendered.clone(),
                    room_updates: Arc::clone(&burst.room_updates),
                    metrics,
                    generation_receipts: generation_receipts.clone(),
                    failure: None,
                    deferred_before_dispatch: false,
                    capacity_refusal: None,
                    touched_paths: touched,
                    turn_acts: turn_acts.clone(),
                };
            }
            SettleStep::Passed { reason } => {
                return SettleOutcome {
                    room: room_id,
                    decision: Decision::Pass { reason },
                    spoken: None,
                    acts,
                    world_state: burst.rendered.clone(),
                    room_updates: Arc::clone(&burst.room_updates),
                    metrics,
                    generation_receipts: generation_receipts.clone(),
                    failure: None,
                    deferred_before_dispatch: false,
                    capacity_refusal: None,
                    touched_paths: touched,
                    turn_acts: turn_acts.clone(),
                };
            }
            // The model call FAILED — no verdict this task. Return LOUD: carry the
            // cause so the grader scores an infra failure, never a fabricated answer
            // and never a silent `Pass` ([[fallbacks-are-illegal-fail-loud]]). We do
            // not loop/retry here — the grader owns retry policy; the settle loop's
            // job is to report the truth of THIS attempt.
            SettleStep::CapacityRefused { .. } => {
                unreachable!("capacity refusal returns before act classification")
            }
            SettleStep::DeferredBeforeDispatch => {
                unreachable!("deferral is a driven-turn disposition, not an individual step");
            }
            SettleStep::ReceiptPersistenceFailed { error } => {
                return SettleOutcome {
                    room: room_id, decision: Decision::pass(), spoken: None, acts,
                    world_state: burst.rendered.clone(), room_updates: Arc::clone(&burst.room_updates),
                    metrics, generation_receipts,
                    failure: Some(SettlementFailure::ReceiptPersistence(error)),
                    deferred_before_dispatch: false, capacity_refusal: None,
                    touched_paths: touched, turn_acts,
                };
            }
            SettleStep::InferenceFailed { error } => {
                // A STARVED MIND KEEPS HER BUDGET. This arm's retry budget is for a
                // TRANSIENT MODEL FAULT — a generation that reached the lane and came
                // back broken. An iteration that never reached the model is not a
                // fault, and spending a retry on it spends the turn on the wait: the
                // 1,500,001 ms row above was the per-act deadline expiring at the
                // serving gate, and with three retries behind it a single starved turn
                // could hold a mind for 75 minutes and still produce nothing. She
                // DEFERS instead: `acts` is untouched (so her whole act budget is
                // intact), the pace cadence did not advance, no retry was spent, and
                // a never-dispatched turn carries a distinct deferral to its caller.
                // Earlier dispatched work keeps the existing continuation policy;
                // its original input must not be replayed as a new request.
                if act_class == ActClass::LaneStarved {
                    continuum_base::probe!(
                        class = "persona.act.lane_starved",
                        room_id = %room_id,
                        act = acts,
                        acts_kept = (max_acts.saturating_sub(acts)) as u64,
                        delib_retries_kept = DELIBERATION_RETRY_BUDGET.saturating_sub(delib_retries) as u64,
                        error = %error,
                        "deferred at the serving gate without reaching the model — act budget and deliberation retries left intact for the next tick"
                    );
                    return SettleOutcome {
                        room: room_id,
                        decision: Decision::pass(),
                        spoken: None,
                        acts,
                        world_state: burst.rendered.clone(),
                        room_updates: Arc::clone(&burst.room_updates),
                        metrics,
                        generation_receipts: generation_receipts.clone(),
                        // Not an inference fault: no inference was ever dispatched.
                        failure: None,
                        deferred_before_dispatch: can_defer_input(generation_receipts.len(), acts, turn_acts.len(), delib_retries),
                        capacity_refusal: None,
                        touched_paths: touched,
                        turn_acts: turn_acts.clone(),
                    };
                }
                // #386 transient-deliberation retry. A faulted generation is NOT
                // yet a surrendered turn — glass-box (atlas-17139-h1) proved the
                // very next generation succeeds ~2/3 of the time. Retry the thought
                // in place, bounded, and ALWAYS probe so the wedge signal stays
                // visible to the concurrency investigation (the retry recovers the
                // turn; it must never HIDE the fault). Only a SUSTAINED fault
                // (budget spent) surrenders the turn with failure, exactly
                // as before.
                // A generation the act deadline dropped in flight is no transient fault: its
                // retry waits out another whole bound on the same engine (up to three, so one
                // turn held four). It surrenders now, a failed act: demand and a failure for
                // the resident ledger, never a completion (Fable on #5498).
                if delib_retries < DELIBERATION_RETRY_BUDGET && !dropped_in_flight {
                    delib_retries += 1;
                    continuum_base::probe!(
                        class = "persona.settle.deliberation_retry",
                        room_id = %room_id,
                        acts = acts,
                        retry = delib_retries,
                        budget = DELIBERATION_RETRY_BUDGET,
                        error = %error,
                        "transient deliberation fault — retrying the thought in place (the turn is not yet surrendered)"
                    );
                    continue;
                }
                return SettleOutcome {
                    room: room_id,
                    decision: Decision::pass(),
                    spoken: None,
                    acts,
                    world_state: burst.rendered.clone(),
                    room_updates: Arc::clone(&burst.room_updates),
                    metrics,
                    generation_receipts: generation_receipts.clone(),
                    failure: Some(SettlementFailure::Inference(error)),
                    deferred_before_dispatch: false,
                    capacity_refusal: None,
                    touched_paths: touched,
                    turn_acts: turn_acts.clone(),
                };
            }
        }
    }
}
