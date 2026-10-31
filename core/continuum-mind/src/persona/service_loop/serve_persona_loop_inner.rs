use super::*;

pub(super) async fn serve_persona_loop_inner(
    ctx: &HostedPersona,
    conversation: &mut dyn CitizenConversation,
    opts: ServeOptions,
    obligations: crate::persona::obligation::Obligations,
) -> Result<ServeOutcome, String> {
    // PRECONDITION: caller MUST have called `conversation.prime()`
    // before entering this loop. The supervisor's `spawn_persona_service`
    // does this before spawning the task. Direct callers (e.g.
    // integration tests) prime explicitly before calling.
    //
    // Per [[no-fallbacks-ever]]: this loop does NOT prime as a safety
    // net. Calling prime here AND in `spawn_persona_service` would be
    // belt-and-suspenders fallback shape — one place primes, the
    // other relies on the contract. If a caller forgot to prime, the
    // first `next_message` returns a typed `Err("called before prime()")`
    // — fail-loud, not silently-warm.
    let mut high_water = conversation
        .initial_water_mark(opts.page_recent_limit)
        .await
        .map_err(|e| format!("high_water_mark failed: {e}"))?;
    // Event-id ring for staleness at the loop head (see `wake_backlog::is_stale`).
    let mut seen_ids = crate::cognition::conversation::seen_ids::SeenIds::new(256);
    // Admitted triggers remain owned while capacity is unavailable. New inbox
    // input and Stop stay reachable without replacing an older trigger's custody.
    let mut deferred_input = crate::persona::wake_backlog::PendingInput::for_persona(ctx.identity.peer_id.as_uuid());
    // Its durable half (card 920619c5): each admitted turn is a row until it ends, so the
    // turns the last stop cut are named here, at the start, instead of vanishing. Stop-drain
    // S3: each gets a notice; an untouched one is handed back, one that began an effect never is.
    obligations
        .recover((opts.now_ms)(), &mut deferred_input, |trigger| new_message_attention(ctx, trigger))
        .await;

    // The persona's adapter (`ctx.adapter`) is reached by the
    // cognition layer through the global provider registry — slice
    // 1D / task #161 registers it at supervisor boot so
    // `evaluate_response` finds it by model_id. The loop itself does
    // not invoke inference directly; the workspace cycle owns the
    // agent contract per the cognition pipeline doc.
    let mut outcome = ServeOutcome::default();

    // Never-stop heartbeat: a persona is not a request→response handler that goes
    // idle between messages — it is a mind that always gets time. The loop now
    // selects between the airc wire and a periodic tick; on a tick the deliberation
    // concern runs over the CURRENT world-state (no inbound message), so the persona
    // pursues its own open intentions and goes idle only when its own judgment says
    // there is nothing to do. [[organic-substrate-continuous-concern-scheduler]].
    // Each persona beats at ITS OWN rhythm — there is no shared metronome. The tempo comes
    // from the being's own life: quick while there's something to engage, drifting toward
    // restful when it's alone (exponential backoff to a cap). Different beings living
    // different moments beat at different rates, so they never tick in lockstep — the
    // thundering herd that made six minds stampede one model isn't scheduled-around, it
    // simply never forms. A stable identity-derived phase desyncs even the first beat.
    // [[design-the-persona-as-a-being]] [[idle-is-self-directed-free-time]].
    let engaged_beat = std::time::Duration::from_millis(SELF_TICK_MS);
    let rest_cap = std::time::Duration::from_millis(SELF_TICK_REST_CAP_MS);
    let phase = std::time::Duration::from_millis(
        (ctx.identity.peer_id.as_uuid().as_u128() as u64) % SELF_TICK_MS,
    );
    let mut beat = BeatClock {
        engaged: engaged_beat,
        rest_cap,
        next: engaged_beat + phase,
    };
    // Burst fingerprint of the last cycle (message OR self-tick), so a heartbeat
    // over an unchanged world is free. Shared by both paths: a message turn updates
    // it, so the very next tick doesn't re-deliberate the same burst.
    let mut last_burst_fp: u64 = 0;
    // Recent inbound texts (bounded ring) — the exchange record the message-path
    // loop-filler gate below judges against. Inbound only: own posts are filtered
    // above the gate, and the peer's repeats are what re-arm the resonance.
    let mut recent_inbound: std::collections::VecDeque<String> =
        std::collections::VecDeque::with_capacity(RECENT_INBOUND_WINDOW);
    // Roster-name memory for engram attribution: harvested from each turn's
    // composed roster delivery (zero extra I/O — the shared projection already
    // runs per turn), consulted at admit time so her MEMORY records
    // "Joel said…" / "Claude said…", never "peer-7711fe60 said…". First-class
    // peers — humans, outside agents, personas — all resolve through the one
    // roster airc owns ([[airc-native-identity-rooms-security]]). Eventually
    // consistent: a brand-new peer's FIRST message may admit under its short
    // peer tag (honest, never fabricated); every later one is named.
    let mut roster_names: std::collections::HashMap<Uuid, String> =
        std::collections::HashMap::new();

    // While an eval-preemption lease is held, the loop suspends WANDERING, not
    // reachability: self-ticks are skipped (the measured lane keeps its quiet)
    // and inbound directed turns still serve — the guard inside the loop below
    // carries the full rationale. The measuring eval drives cognition DIRECTLY
    // (not through this loop), so a quiet loop never blocks it.
    // [[benchmark-is-a-governor-preemption-lease]]
    // [[first-class-citizens-even-during-benchmarks]]
    // Her mind's wakes (EVENT-MIND §1b, phase 2): taken once her region boots; a timer
    // at her continuation's deadline or the floor of her dial; Perceive wakes wait for
    // their recheck so a line her inbox takes never also starts a self-turn.
    let mut mind = MindWakeState::default();
    loop {
        // An interrupted turn that began an effect closes after a completed turn read its notice.
        for answered in deferred_input.take_answered() {
            obligations.close(Some(answered)).await;
        }
        let (wake, pending_before_wake) = await_wake(
            ctx,
            conversation,
            &mut outcome,
            &opts,
            &deferred_input,
            &mut mind,
            beat.next,
        )
        .await;
        if matches!(wake, Wake::Idle) {
            continue;
        }
        if let Wake::Mind { why, room } = &wake {
            continuum_base::probe!(
                class = "persona.turn.mind_wake",
                persona = %ctx.identity.agent_name,
                why = *why,
                room = ?room,
                "her own mind woke her: the turn starts from her perception, not the tick"
            );
        }
        if let LoopStep::Continue = quiesce_gate(&opts, &wake) {
            continue;
        }
        // ADMIT THE TURN, or leave because the node is stopping.
        //
        // Taken the moment a wake is produced, BEFORE the match, because the
        // `Wake::Tick` arm does its self-tick deliberation INLINE and then `continue`s —
        // it never reaches the code below it. A permit taken after the match would have
        // counted inbound turns and silently missed every self-directed one: turns that have
        // consumed input and written nothing.
        //
        // Held to the end of the loop body, so it is released on EVERY exit path — the
        // `continue` at the end of the tick arm included. A manual decrement would have to
        // be repeated at each of them, and would be missed at the next one added.
        let Some(_turn) = crate::cognition::turn_ingress::admit() else {
            continuum_base::probe!(
                class = "persona.ingress.closed",
                persona = ctx.identity.peer_id.as_uuid().to_string(),
                "turn ingress closed for shutdown — this citizen takes no further turns and leaves her loop"
            );
            break;
        };
        let wake_room = match &wake {
            Wake::Mind { room, .. } => *room,
            _ => None,
        };
        let admitted_retry = matches!(&wake, Wake::Retry(_));
        let msg = match wake {
            Wake::Stop => break,
            Wake::Idle => continue,
            Wake::Tick | Wake::Mind { .. } => {
                if deferred_input.take_ready_self(wake_room) {
                    last_burst_fp = 0;
                }
                serve_self_tick(
                    ctx,
                    conversation,
                    &opts,
                    &mut last_burst_fp,
                    wake_room,
                    pending_before_wake,
                    &mut beat,
                    &mut deferred_input,
                )
                .await;
                continue;
            }
            // A message means life in the room — snap back to a quick beat so she's present
            // and responsive, not drifting in rest.
            Wake::Msg(m) | Wake::Retry(m) => {
                if admitted_retry { deferred_input.resume_input(&m); }
                beat.next = beat.engaged;
                m
            }
        };
        let msg = match resync_to_now(
            ctx,
            conversation,
            &mut outcome,
            &mut seen_ids,
            &mut high_water,
            &mut deferred_input,
            admitted_retry,
            msg,
        )
        .await
        {
            Resync::Trigger(msg) => msg,
            Resync::Continue => continue,
            Resync::Break => break,
        };

        // A.6 — the turn's room is the room the trigger message ARRIVED in
        // (transport-stamped, authoritative), not the identity's ambient
        // default. Nil only from sources that predate room stamping
        // (scripted/test conversations) → ambient default. This is what lets
        // an operator's CLI publish wake a turn that can actually SEE the
        // room's board/kanban/roster instead of a nil-room ghost context.
        let turn_room = if msg.room_id.is_nil() {
            ctx.identity.default_room
        } else {
            msg.room_id
        };
        let trigger = TurnTrigger {
            msg: &msg,
            turn_room,
            admitted_retry,
        };

        // Message-path loop-filler gate (#16): the heartbeat dedups its burst, but
        // this path ran a full ~55s decode for EVERY inbound message — so two
        // personas trading one goodbye template cycled forever on a metronome equal
        // to decode time (glass-boxed 2026-07-09; the [pattern] observation, the
        // presence clause, and honored PASSes all reached her and the loop survived
        // on scheduling alone). A near-duplicate of an already-seen contribution
        // arriving into an ALREADY-CYCLING exchange is not news: ADMIT it to memory
        // (she remembers hearing it) but defer the dedicated turn to the heartbeat,
        // whose deduped fingerprint stays stable → the resonance starves. A repeated
        // sincere question in a non-cycling exchange never defers (the never-ghost
        // floor) — see `loop_dedup::defer_as_loop_filler` for the two-condition
        // trigger. Scheduling hygiene, not an output gate
        // ([[no-hardcoded-heuristics-to-steer-cognition]]).
        // This heuristic compares text only. Equal captions do not establish
        // that two media-bearing messages are repeated input.
        if let LoopStep::Continue = defer_loop_filler(
            ctx,
            trigger,
            &roster_names,
            &mut recent_inbound,
            &opts,
            &mut outcome,
        )
        .await
        {
            continue;
        }

        // Per-turn latency clock starts AFTER the filters above —
        // we measure the substrate's per-reply cost (RAG + inference
        // + say), not the wall-clock that includes pre-watermark or
        // self-loop filtering. Monotonic `Instant` so the metric is
        // immune to wall-clock skew. Per [[init-once-handle-then-lease-zero-copy-refs]]
        // the metric is what verifies the doctrine actually shaved
        // the round-trip the doctrine claims to shave.
        if !admitted_retry && msg.work.is_none() {
            deferred_input.payload_changed(&msg);
        }
        let turn_started = std::time::Instant::now();
        // Per #195 slice 1: per-phase decomposition. Each successful
        // reply records all five phases into the matching
        // ServeOutcome aggregates after `say` succeeds. The
        // continue-on-error arms above this line don't touch
        // phase_timings, so phase aggregates stay consistent with
        // `turn_latency` (sample-set identical).
        let mut phase_timings = PhaseTimings::default();

        // RTOS-debugger breakpoint: turn entry at the service-loop
        // level. Pair with `persona.turn.spoke` /
        // `persona.turn.silent` / `persona.turn.error` (same lamport)
        // for the complete per-turn record. This one names the
        // airc-boundary turn.
        // Cognition pulse: this stamp is what EARNS her claim renewals
        // (cognition_pulse.rs) — a turn starting, even one that later defers
        // on serving pressure, is proof she is working her holds.
        crate::persona::cognition_pulse::touch(
            ctx.identity.peer_id.as_uuid(),
            EpochMs::now().millis(), // pre-1970 reads 0: a pre-epoch clock reads 0
        );
        // Held for the turn's lifetime: every exit below (`continue` on silence or
        // error, the spoke tail) drops it, which ends the turn on the admission ledger
        // — the in-flight mark the placement switch honours (a move lands between
        // turns only) and the measured turn shape it derives her cooldown from.
        let _turn_in_flight = crate::cognition::turn_ledger::begin_turn(
            ctx.identity.peer_id.as_uuid(),
            EpochMs::now().millis(),
        );
        announce_turn_start(ctx, trigger, &opts);

        // ===========================================================
        // The brain services the turn through the WorkspaceCycle:
        // admit → build_workspace_turns → Burst → workspace cycle
        // (RecallFaculty feeds the prompt, deliberation decides,
        // act→observe drives to settle). That is the live path below.
        //
        // See docs/architecture/PERSONA-COGNITION-PIPELINE.md for the
        // full pipeline.
        //
        // NOT a `will_respond + response_text` chatbot contract. NOT
        // a parallel rag_inspect bypass. The verbs in `cognition/`
        // do the work; this loop is the wire driver per
        // [[no-if-statements-use-llms-for-cognition]] +
        // [[no-fallbacks-ever]].
        let now_ms = (opts.now_ms)();

        // The engram of the message that woke this turn — the root its acts chain back
        // to. Written at admission and read at burst assembly, two blocks apart.
        let wake_engram = admit_trigger(
            ctx,
            trigger,
            &roster_names,
            now_ms,
            &mut phase_timings,
            admitted_retry
                .then(|| deferred_input.wake_engram_for(&msg))
                .flatten(),
        )
        .await;
        // Recorded BEFORE the turn can mark its trigger read (the read receipt of its sent
        // prompt): from here until the turn ends, a stop that cuts it leaves this row behind.
        // A retry reconciles the id its first attempt minted (confirmed or not); only a new input
        // mints one. Only a CONFIRMED row lets the turn run (stop-drain S2): an unknown row could be
        // re-run by a later start after this turn acted. Unconfirmed, the input is retained WITH its
        // candidate id (Codex on #5492: a create may have committed unseen), and the turn stops
        // here, before any prompt could mark the trigger read.
        let candidate = admitted_retry.then(|| deferred_input.obligation_for(&msg)).flatten();
        let obligation = match obligations.admit(turn_room, &msg, wake_engram, now_ms, candidate).await {
            crate::persona::obligation::Admission::Confirmed(turn) => turn,
            crate::persona::obligation::Admission::Unconfirmed(candidate) => {
                deferred_input.settle(
                    &msg,
                    wake_engram,
                    new_message_attention(ctx, &msg),
                    &crate::cognition::act_observe::SettleStep::DeferredBeforeDispatch,
                );
                if !deferred_input.keep_obligation(&msg, Some(candidate)) {
                    // Not retained (ambient input keeps its re-perception policy): a row that
                    // committed unseen must not outlive it, so close it best-effort.
                    obligations.close(Some(candidate)).await;
                }
                continue;
            }
        };
        // This turn's pre-effect mark, carried in its burst and to its reply: built from its
        // confirmed obligation, dropped with the turn.
        let effects = obligations.gate_for(obligation);
        // 2. Compose the turn's RAG context (budget + multi-source composition); inference
        //    runs WITHOUT holding the cognition mutex.
        let composed = compose_turn(
            ctx,
            turn_room,
            now_ms,
            &mut phase_timings,
            &mut roster_names,
        )
        .await;
        let workspace_burst = build_turn_burst(ctx, &composed, trigger, now_ms, wake_engram).with_effects(Some(effects.clone()));
        // Mark this world-state as just-deliberated so the next heartbeat tick doesn't
        // re-run the same burst (the message path and the self-tick share the gate;
        // own chat is excluded so this reply can't re-trigger a self-tick, while her
        // own active work is folded in so the tick advances it — see burst_fingerprint).
        last_burst_fp = burst_fingerprint(&composed.deliveries, &ctx.identity.peer_id.to_string());
        // Room-doctrine grounding (the [Room operating doctrine] block) now reaches
        // the prompt via the WorkspaceCycle's RagSourceFaculty bridge (task #12), as
        // does the room roster — not threaded through a per-turn RespondInput. The
        // brain owns its own grounding. ([[rag-source-faculty-convergence]])

        // 3. Run the cognition cycle. The persona may speak or stay
        //    silent — both are first-class outcomes per the
        //    PersonaResponse contract.
        // Per #195 slice 1: time the LLM bulk — typically the
        // dominant cost and the primary target of subsequent
        // optimization slices.
        //
        // Wrapped in `time_probe!` so every persona turn emits a
        // dedicated `class = "timing", seam = "persona.respond"` event
        // to the JsonlProbeFileSink. Operators tailing the JSONL get
        // per-turn latency on the canonical timing channel — pair with
        // the per-stage `persona.response.*` probes inside the cycle
        // for a full breakdown by seam. Doctrine
        // `[[jtag-probes-are-rtos-debugger]]`: every meaningful seam
        // should emit timing on the same wire so multi-persona
        // optimization campaigns work from one probe stream.
        // === GATING CUTOVER (task #9): the decision AND the response now come
        // from the persona's WorkspaceCycle (the brain) — NOT the heuristic
        // calculate_priority / fast_path_decision_core path. We resolve the
        // per-persona cycle (registered at spawn), run it over the consolidated
        // burst, and take its `Decision` as the turn. The model judges whether to
        // speak; Rust no longer gates cognition ([[no-rust-gates-around-cognition]]).
        // No cycle registered ⇒ a spawn wiring bug ⇒ fail loud, no legacy respond()
        // fallback ([[fallbacks-are-illegal-fail-loud]]).
        //
        // The burst INCLUDES the persona's OWN prior posts, ATTRIBUTED by name — a
        // mind must see what IT said to follow context and avoid repeating itself.
        // The self-talk loop is prevented upstream, not by hiding own posts: a
        // persona's own message never TRIGGERS a turn (the `msg.peer_id == own`
        // filter above), so including own posts purely as read-context is safe.
        let spoken = match run_turn_cycle(
            ctx,
            conversation,
            &mut outcome,
            &mut deferred_input,
            &mut phase_timings,
            trigger,
            CycleInput {
                workspace_burst,
                wake_engram,
            },
        )
        .await
        {
            CycleVerdict::Spoke(spoken) => spoken,
            CycleVerdict::Skip => {
                // A refused or deferred input is still owed: PendingInput retains it for a
                // retry, so its row stays open with it (Codex on #5480). Only a finished
                // skip closes.
                if !deferred_input.keep_obligation(&msg, Some(obligation)) {
                    obligations.close(Some(obligation)).await;
                }
                continue;
            }
        };
        let speech = finish_spoken_turn(
            ctx,
            conversation,
            &mut outcome,
            trigger,
            spoken,
            phase_timings,
            turn_started,
            &mut deferred_input,
            &effects,
        )
        .await;
        // A refused reply mark recorded nothing and posted nothing: the row stays open.
        if matches!(speech, SpeechMark::Done) {
            obligations.close(Some(obligation)).await;
        }
    }

    deferred_input.stop();
    Ok(outcome)
}
