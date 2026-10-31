use super::*;

/// Publish a spoken verdict into the room it was reasoned in and record the turn: the
/// raw-envelope invariant, the airc publish, the act-question, latency/phase aggregates,
/// and the L2 training producer.
pub(super) async fn finish_spoken_turn(
    ctx: &HostedPersona,
    conversation: &mut dyn CitizenConversation,
    outcome: &mut ServeOutcome,
    trigger: TurnTrigger<'_>,
    spoken: SpokenTurn,
    mut phase_timings: PhaseTimings,
    turn_started: std::time::Instant,
    pending: &mut crate::persona::wake_backlog::PendingInput,
    effects: &crate::cognition::act_observe::EffectGate,
) -> SpeechMark {
    let TurnTrigger { msg, turn_room, .. } = trigger;
    let SpokenTurn {
        response_text,
        turn_credit,
        turn_generation_receipts,
    } = spoken;
    // Invariant: a raw tool-call envelope must NEVER reach the room — from ANY
    // path. The cycle's verdict text is sometimes just `{"tool_call":…}` (an
    // un-acted call the model emitted as its "answer"); broadcasting it spams
    // peers with JSON (observed live). Treat it as silence — the deliberation
    // already executes real calls internally; only prose is a contribution.
    if crate::ai::json_in_prompt_tools::parse_tool_call(&response_text).is_some() {
        continuum_base::probe!(
            class = "persona.turn.raw_envelope_withheld",
            persona = %ctx.identity.agent_name,
            lamport = msg.lamport,
            "verdict was a raw tool-call envelope — not broadcasting"
        );
        outcome.turns_skipped += 1;
        return SpeechMark::Done;
    }

    // Stop-drain S2: her reply is recorded on the turn's obligation BEFORE it is posted, so a
    // stop after this point is known to have spoken and the turn is never re-run blindly. An
    // unconfirmed mark withholds the post (fail closed): nothing reaches the room unrecorded.
    if let Err(refusal) = crate::cognition::act_observe::EffectGate::before(
        Some(effects),
        &crate::cognition::act_observe::Effect::Speech,
    )
    .await
    {
        continuum_base::probe!(
            class = "persona.turn.speech_unmarked",
            persona = %ctx.identity.agent_name,
            room_id = %turn_room,
            error = %refusal,
            "her reply could not be recorded before posting, so it was not posted (fail closed)"
        );
        outcome.turns_errored += 1;
        return SpeechMark::Refused;
    }
    // Per #195 slice 1: time the airc publish + downstream ack.
    let say_started = std::time::Instant::now();
    // Into the room the trigger arrived in (A.6 `turn_room`), which is the
    // same room whose context this turn reasoned over — never her ambient
    // default, or she answers one room's question to a different audience.
    let say_result = publish_own_speech(
        conversation,
        ctx.identity.peer_id,
        turn_room,
        &response_text,
    )
    .await;
    phase_timings.say_ms = say_started.elapsed().as_millis() as u64;
    if let Err(e) = say_result {
        tracing::warn!(
            lamport = msg.lamport,
            error = %e,
            "say failed"
        );
        // RTOS-debugger breakpoint: cognition succeeded, airc
        // publish failed. Distinct error stage from
        // `stage="respond"`.
        continuum_base::probe!(
            class = "persona.turn.error",
            persona = %ctx.identity.agent_name,
            lamport = msg.lamport,
            stage = "say",
            error = %e,
            "airc publish failed"
        );
        outcome.turns_errored += 1;
        return SpeechMark::Done;
    }
    // SHE SPOKE — and she may ALSO hold work. Ask the act-question here too.
    //
    // The question used to live only on the PASS arm, which made answering someone
    // mutually exclusive with working your own card: a citizen who replied in the room
    // was never asked whether to act on the claim she was holding. That is backwards
    // for a colleague — talking about the work and doing the work are not alternatives.
    // Asked AFTER the reply is sent, so the room hears her answer at the same latency
    // as before and the act-question can never delay a conversation.
    // `directed` is recomputed here rather than threaded: it is a pure function of
    // (identity, msg.text) and the binding above lives inside the cycle branch.
    let spoke_directed = ctx.identity.persona_identity().mentions(&msg.text);
    let work = crate::persona::act_question::ask_the_act_question(
        ctx,
        conversation,
        msg.lamport,
        turn_room,
        spoke_directed,
        // A message turn never offers her initiative: the message snapped her beat to
        // engaged, and that beat (the self-tick) offers it.
        crate::persona::initiative_turn::Initiative::NotOffered,
    )
    .await;
    pending.settle_work(&work);

    let turn_duration_ms = turn_started.elapsed().as_millis() as u64;
    outcome.turn_latency.record(turn_duration_ms);

    // RTOS-debugger breakpoint: turn completed successfully.
    // The phase fields below are the per-phase decomposition
    // from #195 slice 1 — together they let an operator find
    // bottlenecks (recall is slow? compose is slow?) without
    // a separate timing probe per stage. Per Joel
    // [[jtag-probes-are-rtos-debugger]]: "timing of anything,
    // so we can hunt down bottlenecks."
    continuum_base::probe!(
        class = "persona.turn.spoke",
        persona = %ctx.identity.agent_name,
        lamport = msg.lamport,
        response_len = response_text.len(),
        turn_duration_ms = turn_duration_ms,
        recall_ms = phase_timings.recall_ms,
        admit_ms = phase_timings.admit_ms,
        compose_ms = phase_timings.compose_ms,
        respond_ms = phase_timings.respond_ms,
        say_ms = phase_timings.say_ms,
        "turn complete"
    );
    // Per #195 slice 1: record the phase decomposition. Same
    // sample set as `turn_latency` — only successful replies
    // reach this point.
    outcome.recall_latency.record(phase_timings.recall_ms);
    outcome.admit_latency.record(phase_timings.admit_ms);
    outcome.compose_latency.record(phase_timings.compose_ms);
    outcome.respond_latency.record(phase_timings.respond_ms);
    outcome.say_latency.record(phase_timings.say_ms);
    outcome.turns_replied += 1;

    // L2 continuous-learning producer: this completed live turn (the
    // triggering message → the reply just published) is a `(context,
    // completion)` training example [[capability-is-driver-plus-genome]].
    // Hand it to the producer, which scores + classifies + submits it on a
    // spawned task — best-effort, never touching this turn's latency or
    // correctness. It lives ONLY on this live `Spoke` path, which eval forks
    // (`drive_to_settle`) never run, so the training set can never be
    // contaminated by a measurement simulation.
    // Card-linked work was captured once by the shared driver, including its
    // report and final generation. Only genuinely unlinked speech submits here.
    if !turn_credit
        .as_ref()
        .is_some_and(|credit| credit.is_card_linked())
    {
        crate::persona::training_producer::produce(
            ctx.identity.peer_id.as_uuid(),
            ctx.identity.agent_name.clone(),
            msg.render_content(),
            response_text.clone(),
            // Captured in the cycle arm above, at selection. `None` is an ordinary
            // conversation: it leaves a `training.example.unverified` probe and is NOT
            // a training example (card 8e3dd206) — only a graded turn is.
            turn_credit,
            turn_generation_receipts,
        );
    }
    tracing::info!(
        lamport = msg.lamport,
        turn_duration_ms = turn_duration_ms,
        turns_replied = outcome.turns_replied,
        mean_ms = outcome.turn_latency.mean_ms().unwrap_or(0.0),
        min_ms = outcome.turn_latency.min_ms.unwrap_or(0), // unwrap_or: clock before the epoch = 0
        max_ms = outcome.turn_latency.max_ms.unwrap_or(0), // unwrap_or: clock before the epoch = 0
        recall_ms = phase_timings.recall_ms,
        admit_ms = phase_timings.admit_ms,
        compose_ms = phase_timings.compose_ms,
        respond_ms = phase_timings.respond_ms,
        say_ms = phase_timings.say_ms,
        "turn complete — substrate's per-reply cost recorded with phase decomposition"
    );
    SpeechMark::Done
}

/// Whether her reply's pre-effect mark was refused (stop-drain S2). `Refused` keeps the turn's
/// obligation open: nothing was posted and no effect was recorded for it, so a later start may
/// re-admit it. `Done` covers every other end of the turn, posted or not.
pub(super) enum SpeechMark {
    Done,
    Refused,
}
