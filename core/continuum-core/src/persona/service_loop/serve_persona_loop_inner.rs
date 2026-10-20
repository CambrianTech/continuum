use super::*;

pub(super) async fn serve_persona_loop_inner(
    ctx: &HostedPersona,
    conversation: &mut dyn PersonaConversation,
    opts: ServeOptions,
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
    let mut seen_ids = crate::persona::wake_backlog::SeenIds::new(256);
    // One admitted trigger remains owned until the model actually gets a turn.
    // Fresh arrivals stay in the conversation's existing retained inbox meanwhile.
    let mut deferred_input = crate::persona::wake_backlog::PendingInput::default();

    // The persona's adapter (`ctx.adapter`) is reached by the
    // cognition layer through the global provider registry — slice
    // 1D / task #161 registers it at supervisor boot so
    // `evaluate_response` finds it by model_id. The loop itself does
    // not invoke inference directly; `persona::response::respond`
    // owns the agent contract per the cognition pipeline doc.
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
    let mut next_beat = engaged_beat + phase;
    // Burst fingerprint of the last cycle (message OR self-tick), so a heartbeat
    // over an unchanged world is free. Shared by both paths: a message turn updates
    // it, so the very next tick doesn't re-deliberate the same burst.
    let mut last_burst_fp: u64 = 0;
    // Recent inbound texts (bounded ring) — the exchange record the message-path
    // loop-filler gate below judges against. Inbound only: own posts are filtered
    // above the gate, and the peer's repeats are what re-arm the resonance.
    // context-budget-exempt: a count of recent inbound MESSAGES to consider, not a size in tokens or chars
    const RECENT_INBOUND_WINDOW: usize = 24;
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
    let me = ctx.identity.peer_id.as_uuid();
    let mut wakes: Option<tokio::sync::mpsc::Receiver<crate::persona::perception_region::Wake>> = None;
    let mut next_mind_check: Option<tokio::time::Instant> = None;
    let mut perceive_recheck: Option<(tokio::time::Instant, uuid::Uuid)> = None;
    loop {
        if wakes.is_none() {
            wakes = conversation.take_wakes();
            if wakes.is_some() {
                // First look right away: a saved continuation resumes now (rule 8).
                next_mind_check = Some(tokio::time::Instant::now());
            }
        }
        // HER turn boundary: idle whenever the loop is back at its wake select. The
        // matching `engaged` stamp is set where a serving lane is actually acquired
        // (the deliberation faculty) — a room wake alone is not wakefulness, or a
        // busy room would cancel every dream (measured: 290 paged out in 20 min with
        // the stamp on the wake itself).
        crate::cognition::activity_gate::persona_idle(ctx.identity.peer_id.as_uuid());
        // BIASED: a line already in her inbox beats a due self-tick. Without the
        // order, a directed line that made her yield a parked lane wait could
        // lose the next wake to the tick again (random branch choice) — a yield
        // storm (Lorcan: 12 yields in 5 min, measured 2026-09-04) that ended only
        // when the event arm happened to win.
        let pending_before_wake = crate::cognition::directed_pending::observe(ctx.identity.peer_id.as_uuid());
        let wake = tokio::select! {
            biased;
            ev = next_event(conversation, &mut outcome), if deferred_input.message().is_none() => match ev {
                Some(m) => Wake::Msg(m),
                None => Wake::Stop,
            },
            w = next_mind_wake(&mut wakes), if deferred_input.message().is_none() => match w {
                crate::persona::perception_region::Wake::Perceive { activity, .. } => {
                    perceive_recheck = Some((tokio::time::Instant::now() + PERCEIVE_SETTLE, activity));
                    Wake::Idle
                }
                crate::persona::perception_region::Wake::Continuation => Wake::Mind { why: "continuation", room: her_continuation_room(me) },
                crate::persona::perception_region::Wake::Resume => Wake::Mind { why: "resume", room: her_continuation_room(me) },
            },
            _ = sleep_until_some(perceive_recheck.map(|(at, _)| at)), if deferred_input.message().is_none() => {
                let activity = perceive_recheck.take().map(|(_, a)| a);
                match activity {
                    Some(a) if crate::persona::perception_feed::with_region(me, |r| r.is_pending(a)).unwrap_or(false) => {
                        Wake::Mind { why: "perceive", room: Some(a) }
                    }
                    _ => Wake::Idle,
                }
            },
            _ = sleep_until_some(next_mind_check), if deferred_input.message().is_none() => {
                let now = (opts.now_ms)();
                let looked = crate::persona::perception_feed::with_region(me, |r| {
                    let wake = r.wake_for(now);
                    let floor = tokio::time::Instant::now() + mind_check_floor(r.dial());
                    let due = r.continuation_due_ms().filter(|due| *due > now).map(|due| {
                        tokio::time::Instant::now() + std::time::Duration::from_millis(due - now)
                    });
                    (wake, due.map_or(floor, |d| d.min(floor)))
                });
                match looked {
                    Some((wake, next)) => {
                        next_mind_check = Some(next);
                        match wake {
                            Some(crate::persona::perception_region::Wake::Perceive { activity, .. }) => {
                                perceive_recheck = Some((tokio::time::Instant::now() + PERCEIVE_SETTLE, activity));
                                Wake::Idle
                            }
                            Some(crate::persona::perception_region::Wake::Continuation) => Wake::Mind { why: "continuation", room: her_continuation_room(me) },
                            Some(crate::persona::perception_region::Wake::Resume) => Wake::Mind { why: "resume", room: her_continuation_room(me) },
                            None => Wake::Idle,
                        }
                    }
                    None => {
                        next_mind_check = None;
                        Wake::Idle
                    }
                }
            },
            retry = deferred_input.at_next_beat(next_beat) => match retry {
                Some(message) => Wake::Retry(message),
                None => Wake::Tick,
            },
        };
        if matches!(wake, Wake::Idle) {
            continue;
        }
        if let Wake::Mind { why, room } = &wake {
            crate::probe!(
                class = "persona.turn.mind_wake",
                persona = %ctx.identity.agent_name,
                why = *why,
                room = ?room,
                "her own mind woke her: the turn starts from her perception, not the tick"
            );
        }
        // QUIESCE HONORS ITS OWN CONTRACT (Joel, 2026-08-30: "I could call
        // them up or dm — just want to make sure we're not into singular
        // activity mode again"). The lease's documented promise is "skips
        // INITIATING self-directed turns — she stays online and still answers
        // explicit work" — but this arm was implemented as a full coma
        // (neither ticks NOR inbound; DMs buffered until the lease dropped).
        // The coma's original justification — an in-flight conversation would
        // keep the GPU busy through the measurement — predates the slot
        // economy: a directed reply now decodes on its own slot under a
        // permit sized to the served slots, without touching the measured
        // lane's KV. So: a quiesced citizen SKIPS her self-tick (no
        // wandering; the measured lane keeps its quiet) and still ANSWERS
        // when addressed — like a colleague heads-down on a build who still
        // picks up the phone.
        if opts.quiesced.load(std::sync::atomic::Ordering::Relaxed) {
            match &wake {
                // A suppressed beat leaves the cadence untouched — when the
                // lease drops she resumes at whatever rhythm she had earned.
                // Her own wake under the lease is skipped like a beat; it is not lost:
                // a due continuation or a loud room re-yields on her next mind check.
                Wake::Tick | Wake::Mind { .. } | Wake::Idle => continue,
                Wake::Msg(_) | Wake::Retry(_) => crate::probe!(
                    class = "persona.quiesced.directed_served",
                    "quiesced citizen serving an inbound turn — leases suspend wandering, never reachability"
                ),
                // Stop falls through to the arm below.
                Wake::Stop => {}
            }
        }
        // ADMIT THE TURN, or leave because the node is stopping.
        //
        // Taken the moment a wake is produced, BEFORE the match, because the
        // `Wake::Tick` arm does its self-tick deliberation INLINE and then `continue`s —
        // it never reaches the code below it. A permit taken after the match would have
        // counted inbound turns and silently missed every self-directed one, which is the
        // same blindness as draining on `activity_gate`'s `engaged` flag: that one is
        // stamped at serving-lane acquisition, so a turn still composing its context does
        // not appear there either. Both gaps hide turns that have consumed input and
        // written nothing.
        //
        // Held to the end of the loop body, so it is released on EVERY exit path — the
        // `continue` at the end of the tick arm included. A manual decrement would have to
        // be repeated at each of them, and would be missed at the next one added.
        let Some(_turn) = crate::cognition::turn_ingress::admit() else {
            crate::probe!(
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
                // With `biased;` the inbox was polled first: a tick winning means
                // nothing admissible was queued, so any pending directed flag is
                // stale (raised for a line that filtered at the door). Clear it,
                // or every self-work lane wait yields forever.
                crate::cognition::directed_pending::acknowledge(ctx.identity.peer_id.as_uuid(), pending_before_wake);
                // (Quiescence is handled at the top of the loop — a held lease never
                // reaches here.) Heartbeat slice — the mind gets time with no inbound
                // activity sets the next beat: if it found something new to work on
                // (last_burst_fp advanced), stay quick; if it went idle, drift toward rest.
                //
                // ── Idle admission under lane pressure (#139) ──────────────────────
                // A self-tick is the lowest-priority work in the system: the mind
                // musing over an unchanged world on its own free time. When every shared
                // decode slot is already busy (glass-boxed 2026-07-15: one inbound
                // message woke six minds, each ran a full ~54s deliberation, and the two
                // lanes serialized them into a 250s tail), spending one on an idle
                // deliberation only deepens the queue that LIVE conversation is waiting
                // behind. So under saturation this beat YIELDS and drifts toward rest —
                // the mind keeps its free time for when a lane is actually free. Message
                // turns are never gated here (the Wake::Msg arm below); a human or peer
                // is waiting, they always get served immediately.
                // [[idle-is-self-directed-free-time]]
                // [[conversational-latency-is-a-misdirection-budget]]
                // Idle admission is a PERMIT, not a gauge — the fix the ambient-reply
                // path (below, ~line 980) already made and this self-tick path never
                // received. The gauge (`shared_model_saturated`) has a stampede race
                // (#139/#385): N idle minds waking on the SAME cadence all read
                // inflight=0 before any has generated a token, all pass, and all fire at
                // the shared decode slot at once — oversubscribing a 1-lane node and
                // #385-wedging it (glass-boxed 2026-08-10: resident_personas=4 vs
                // warm_slots=1, every self-tick died on "no TOKEN progress for 90s").
                // The permit is sized to the LIVE served lane count (LaneAdmission ←
                // set_served_lane_count — true since 2026-08-17; it was a hardcoded 1 for
                // the whole time this comment claimed otherwise) and HELD across the whole
                // self-cycle, so ambient
                // concurrency is bounded to real capacity no matter when everyone woke —
                // the surplus minds genuinely yield toward rest instead of stampeding.
                // Directed turns still bypass entirely (they were named). Self-tick and
                // ambient replies share the same ambient pool — both are lowest-priority
                // non-directed work competing for the same lanes.
                let before = last_burst_fp;
                // DETERMINISTIC WORK FIRST, THEN INFERENCE. The cycle runs the held-work
                // act question (which takes its OWN serving lane through the delib gate)
                // and the kanban pull (no inference at all) before it ever asks for an
                // ambient permit — that permit caps only the musing tail. Until
                // 2026-09-03 the permit was taken HERE, before anything ran, so with 12
                // citizens on a lanes-1 ambient pool nine of them yielded every tick and
                // never reached the pull (measured live: 31 yields in 10 minutes, 3 of 12
                // residents working an Open deck of 12).
                // Nobody is waiting on a self-cycle: it renders the prompt an unattended
                // turn can afford (card 7496ed9d) — the message path below stays interactive.
                let starved = crate::cognition::audience::with(
                    crate::inference::prefill_rate::Audience::Unattended,
                    run_self_cycle(ctx, conversation, &opts, &mut last_burst_fp, wake_room),
                )
                .await;
                if starved {
                    if let Some(yields) =
                        crate::cognition::resource_admission::take_ambient_yield_report(
                            EpochMs::now().millis(), // pre-1970 reads 0: clock before the epoch = 0
                        )
                    {
                        crate::probe!(
                            class = "persona.selftick.starved",
                            yields_since_last_report = yields,
                            total_yields =
                                crate::cognition::resource_admission::ambient_yields(),
                            "ambient MUSING yielded — the pool was full. NOT a fault: this is \
                             what contention looks like when citizens outnumber non-directed \
                             lanes; held work and the pull already ran. Rate-limited to ≤1 row/min."
                        );
                    }
                }
                next_beat = next_beat_after(
                    if last_burst_fp != before {
                        BeatOutcome::Engaged
                    } else if starved {
                        BeatOutcome::YieldedNoSlot
                    } else {
                        BeatOutcome::NothingNew
                    },
                    next_beat,
                    engaged_beat,
                    rest_cap,
                );
                continue;
            }
            // A message means life in the room — snap back to a quick beat so she's present
            // and responsive, not drifting in rest.
            Wake::Msg(m) | Wake::Retry(m) => {
                next_beat = engaged_beat;
                m
            }
        };

        // ── Resync-to-now (#131, the human standard) ─────────────────────────
        // Turns take ~60–100s of decode while messages can arrive faster, so a
        // FIFO one-turn-per-message drain leaves her perpetually answering the
        // room as it was N messages ago (glass-boxed live 2026-07-10: her turn
        // saw 3 STALE messages; the assignment and her peer's commitment were
        // still queued behind them; she rationally re-asked the same question
        // five times — "a human reads the last few messages and continues like
        // nothing ever happened"). So: drain EVERYTHING already queued before
        // turning, and turn ONCE on the newest addressed message (else the
        // newest overall). Older drained messages are not lost — the composed
        // room transcript carries them into the same turn as context; they just
        // don't each get a dedicated 60s reply to a conversation that has moved
        // on. Same always-latest coalescing the substrate uses everywhere else
        // (watch channels; positron's resync contract: reconnect RESYNCS state,
        // never replays stale events).
        let self_id = ctx.identity.peer_id.as_uuid();
        let (msg, coalesced) = if admitted_retry {
            (msg, 0)
        } else {
            let pending_before_drain = crate::cognition::directed_pending::observe(ctx.identity.peer_id.as_uuid());
            let mut backlog: Vec<IncomingMessage> = vec![msg];
            let mut stream_ended = false;
            loop {
                // Zero-ish window: anything already buffered resolves immediately;
                // an empty queue times out and we proceed. ≤50ms per wake vs the
                // 60s-per-stale-turn it replaces.
                match tokio::time::timeout(
                    std::time::Duration::from_millis(50),
                    next_event(conversation, &mut outcome),
                )
                .await
                {
                    Ok(Some(next)) => backlog.push(next),
                    Ok(None) => {
                        stream_ended = true;
                        break;
                    }
                    Err(_) => break, // nothing more queued — backlog drained
                }
            }
            // Qualifying = newer than the high-water mark and not her own echo.
            // Advance the high-water past EVERYTHING drained so a stale item can
            // never re-trigger; count skips honestly.
            let mut qualifying: Vec<IncomingMessage> = Vec::with_capacity(backlog.len());
            for m in backlog {
                // Staleness by EVENT ID (a lamport is the publisher's clock, not a
                // room order — see `wake_backlog`); the clock rule only for id-less
                // scripted sources.
                let stale = crate::persona::wake_backlog::is_stale(&m, &mut seen_ids, high_water);
                high_water = m.lamport.max(high_water);
                if stale || m.peer_id == self_id {
                    outcome.turns_skipped += 1;
                } else {
                    qualifying.push(m);
                }
            }
            // Acknowledge the pre-drain generation only; a newer input keeps its wake.
            crate::cognition::directed_pending::acknowledge(self_id, pending_before_drain);
            // Both a literal mention and a human's opportunity to speak receive
            // priority. Only the former can tell the mind "this message names you".
            let priority_line = |m: &IncomingMessage| {
                crate::cognition::workspace::TurnAttention::for_message(
                    ctx.identity.persona_identity().mentions(&m.text),
                    crate::ipc::positron_presence::is_human_peer(m.peer_id),
                )
                .requires_priority()
            };
            // Every directed line drained is HEARD (delivery receipt), whether or
            // not it becomes the trigger.
            for m in qualifying.iter().filter(|m| priority_line(m)) {
                crate::persona::wake_backlog::publish_heard(self_id, m);
            }
            // An undirected AGENT line (neither human nor citizen, not naming her)
            // is perceived — it is in the transcript she reads — but it is never
            // the trigger of a turn: no lane for a peer's status wall
            // (wake_backlog::triggers_a_turn; Joaquin's 34/60, 2026-09-05).
            let before = qualifying.len();
            qualifying.retain(|m| {
                let sender_is_citizen = crate::persona::PersonaAircRuntimeRegistry::try_global()
                    .is_some_and(|r| r.get(m.peer_id).is_some());
                crate::persona::wake_backlog::triggers_a_turn(
                    priority_line(m),
                    sender_is_citizen,
                    crate::persona::wake_backlog::is_receipt(&m.text),
                )
            });
            let perceived_only = before - qualifying.len();
            if perceived_only > 0 {
                outcome.turns_skipped += perceived_only;
                crate::probe!(
                    class = "persona.wake.agent_lines_perceived_only",
                    persona_id = %self_id,
                    lines = perceived_only as u64,
                    "undirected agent lines drained into perception without a turn — no lane for a status wall"
                );
            }
            // Trigger = newest DIRECTED line in the backlog (a question put to her
            // outranks newer ambient chatter — she answers it WITH the newer
            // context visible in the transcript), else the newest overall.
            let (msg, coalesced) =
                match crate::persona::wake_backlog::pick_trigger(qualifying, priority_line) {
                    Some(picked) => picked,
                    None => {
                        if stream_ended {
                            break;
                        }
                        continue; // everything drained was stale/self — no turn
                    }
                };
            (msg, coalesced)
        };
        if coalesced > 0 {
            outcome.turns_skipped += coalesced;
            crate::probe!(
                class = "persona.wake.coalesced",
                persona_id = %self_id,
                coalesced = coalesced,
                trigger_lamport = msg.lamport,
                "resync-to-now: coalesced the queued backlog into ONE turn on the newest message"
            );
        }

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
        if !admitted_retry && msg.media.is_empty() && crate::persona::loop_dedup::defer_as_loop_filler(
            &msg.text,
            recent_inbound.make_contiguous(),
        ) {
            let now_ms = (opts.now_ms)();
            let inbox_msg = crate::persona::types::InboxMessage {
                id: Uuid::new_v4(),
                room_id: turn_room,
                sender_id: msg.peer_id,
                sender_name: roster_names
                    .get(&msg.peer_id)
                    .cloned()
                    .unwrap_or_else(|| format!("peer-{}", &msg.peer_id.to_string()[..8])),
                sender_type: crate::persona::types::SenderType::Persona,
                content: msg.render_content(),
                timestamp: now_ms,
                priority: 0.5,
                source_modality: None,
                voice_session_id: None,
            };
            {
                let cognition = ctx.cognition.lock().await;
                if let Err(e) = cognition.admission.admit(&inbox_msg, None) {
                    tracing::warn!(
                        lamport = msg.lamport,
                        error = %e,
                        "admission.admit failed on deferred loop-filler — engram not formed"
                    );
                }
            }
            crate::probe!(
                class = "persona.turn.deferred_loop_filler",
                persona = %ctx.identity.agent_name,
                lamport = msg.lamport,
                text_len = msg.text.len(),
                "near-duplicate inbound in an already-cycling exchange — admitted to memory, dedicated turn deferred to the heartbeat"
            );
            recent_inbound.push_back(msg.text.clone());
            if recent_inbound.len() > RECENT_INBOUND_WINDOW {
                recent_inbound.pop_front();
            }
            outcome.turns_skipped += 1;
            continue;
        }
        if !admitted_retry {
            recent_inbound.push_back(msg.text.clone());
            if recent_inbound.len() > RECENT_INBOUND_WINDOW {
                recent_inbound.pop_front();
            }
        }

        // Per-turn latency clock starts AFTER the filters above —
        // we measure the substrate's per-reply cost (RAG + inference
        // + say), not the wall-clock that includes pre-watermark or
        // self-loop filtering. Monotonic `Instant` so the metric is
        // immune to wall-clock skew. Per [[init-once-handle-then-lease-zero-copy-refs]]
        // the metric is what verifies the doctrine actually shaved
        // the round-trip the doctrine claims to shave.
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
        // for the complete per-turn record. The `respond_inner`-level
        // probes (`persona.response.enter` etc.) live INSIDE the
        // cognition; this one names the airc-boundary turn.
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
        let _turn_in_flight = crate::cognition::resource_admission::begin_turn(
            ctx.identity.peer_id.as_uuid(),
            EpochMs::now().millis(),
        );
        crate::probe!(
            class = "persona.turn.start",
            persona = %ctx.identity.agent_name,
            persona_id = %ctx.identity.peer_id.as_uuid(),
            room_id = %turn_room,
            lamport = msg.lamport,
            peer_id = %msg.peer_id,
            text_len = msg.text.len(),
            "turn started"
        );
        // She takes in this room up to this line: her region's view of it drains, so the
        // same line cannot also wake a Perceive turn (EVENT-MIND §1b).
        if !admitted_retry {
            crate::persona::perception_feed::mark_perceived(
                ctx.identity.peer_id.as_uuid(),
                turn_room,
                msg.lamport,
                Some(msg.event_id),
                (opts.now_ms)(),
            );
        }

        // ===========================================================
        // The brain services the turn through the WorkspaceCycle:
        // admit → build_workspace_turns → Burst → workspace cycle
        // (RecallFaculty feeds the prompt, deliberation decides,
        // act→observe drives to settle). That is the live path below.
        //
        // NOT `persona::response::respond(RespondInput)` — that verb
        // only has one caller, PersonaServiceModule, which the module
        // audit declares UNWIRED ("shadowed", runtime/registry.rs) so
        // the respond path is dead code, kept only until the verbs it
        // owns are dismantled or re-homed.
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

        // 1. Admit the incoming message into the persona's
        //    hippocampus. THIS is the L2 store growing — without
        //    this call, no engram ever forms and recall stays empty
        //    forever (Paige's "memory" reduces to the live airc
        //    window, same as a chatbot). The admission gate runs
        //    deduplication + trust + Algorithm 4 admission scoring
        //    per [[source-drain-is-the-universal-pattern]]; the
        //    persona's continual-learning property starts here.
        //
        //    Lease the brain → admit → release. The recall query
        //    in step 2 reads from admission state populated by THIS
        //    admit. Inference still runs without holding the mutex.
        let inbox_msg = crate::persona::types::InboxMessage {
            id: Uuid::new_v4(),
            room_id: turn_room,
            sender_id: msg.peer_id,
            sender_name: roster_names
                .get(&msg.peer_id)
                .cloned()
                .unwrap_or_else(|| format!("peer-{}", &msg.peer_id.to_string()[..8])),
            sender_type: crate::persona::types::SenderType::Persona,
            content: msg.render_content(),
            timestamp: now_ms,
            priority: 0.5,
            source_modality: None,
            voice_session_id: None,
        };
        // Rank engrams for the L2→prompt introspection seam, then admit this turn's
        // incoming message as an engram. The recall that actually REACHES the prompt
        // is done inside the WorkspaceCycle's RecallFaculty — this read stays for the
        // Hebbian salience side-effect (Algorithm 4 #165) and the per-engram
        // introspection log ([[observability-is-half-the-architecture]] + Joel's
        // 2026-06-03 "introspect all rag" directive). The ranked Vec is no longer
        // threaded into a per-turn RespondInput (that path is gone); only the side-
        // effects and the admit remain.
        // The engram of the message that woke this turn — the root its acts chain back
        // to. Bound here (outer scope) because it is written at admission and read at
        // burst assembly, two blocks apart.
        let mut wake_engram = deferred_input.wake_engram();

        if !admitted_retry {
            let cognition = ctx.cognition.lock().await;
            // recall BEFORE admit so the ranking is "what I knew going in" — the
            // current message is the trigger, not recall.
            // Per #195 slice 1: time the L2 retrieval pass.
            let recall_started = std::time::Instant::now();
            let scored = cognition.admission.recall_scored(now_ms, 8);
            phase_timings.recall_ms = recall_started.elapsed().as_millis() as u64;
            for (rank, (engram, salience)) in scored.iter().enumerate() {
                let preview: String = engram.content.chars().take(80).collect();
                tracing::info!(
                    lamport = msg.lamport,
                    rank,
                    engram_id = %&engram.id.to_string()[..8],
                    salience = format!("{:.3}", salience),
                    content = %preview,
                    "recall_scored — engram ranked (introspection; RecallFaculty feeds the prompt)"
                );
            }

            // Admit now. Errors here are non-fatal — the cognition turn can still
            // run; the engram just doesn't form. Per [[no-fallbacks-ever]] we surface
            // the failure visibly, not silently.
            // Per #195 slice 1: time the L2 admission write.
            let admit_started = std::time::Instant::now();
            let admit_result = cognition.admission.admit(&inbox_msg, None);
            phase_timings.admit_ms = admit_started.elapsed().as_millis() as u64;
            match &admit_result {
                Err(e) => {
                    tracing::warn!(
                        lamport = msg.lamport,
                        error = %e,
                        "admission.admit failed — engram not formed this turn"
                    );
                }
                Ok(decision) => {
                    // THE ROOT OF THIS TURN'S CAUSAL THREAD (CAUSAL-MEMORY-GRAPH.md §3a).
                    // The message that woke her is already becoming an engram here; its id
                    // was being discarded. Keeping it lets the turn's first act carry a
                    // `CausedBy` edge to what actually caused it, instead of the chain
                    // starting mid-air — which is what made "which acts were done for this"
                    // unanswerable from the graph.
                    //
                    // Only an ADMITTED message is a cause. A Drop (dedup) or Quarantine has
                    // no engram to point at, and inventing one would be a fabricated link —
                    // worse than the honest gap it replaces.
                    wake_engram = decision.admitted_engram_id();
                    tracing::info!(
                        lamport = msg.lamport,
                        engram_count = cognition.admission.engram_count(),
                        wake_engram = ?wake_engram,
                        "admitted incoming → L2 store"
                    );
                }
            }
        }

        // 2. Lease the brain again for compose_for_turn — the budget
        //    + multi-source RAG composition via the
        //    FlexboxRagBudgetAdapter. Inference runs WITHOUT holding
        //    the mutex so the persona stays responsive to a future
        //    "stop" or shutdown signal while the model decodes.
        // Per #195 slice 1: time the RAG flexbox composer.
        let compose_started = std::time::Instant::now();
        let composed = {
            let cognition = ctx.cognition.lock().await;
            // Stamp the WHERE axis: this turn is happening INSIDE `turn_room`.
            // Without it every room-scoped source abstains and she perceives no
            // board, no roster, no doctrine, no wall (#331 / #127).
            cognition
                .compose_for_turn(&ctx.profile, now_ms, Some(turn_room))
                .await
        };
        phase_timings.compose_ms = compose_started.elapsed().as_millis() as u64;
        // Harvest the roster resolution this compose already fetched into the
        // admit-time name memory (see `roster_names` above).
        for (peer, name) in roster_identities(&composed.deliveries) {
            if let Ok(id) = Uuid::parse_str(peer) {
                roster_names.insert(id, name.to_string());
            }
        }

        // The consolidated burst the WorkspaceCycle reasons over — a stream of
        // inbox items WITH their metadata (WHO + WHEN + WHAT), not bare text. A
        // mind needs who-said-what-and-when to follow the thread and avoid
        // repeating itself — "given this RAG, do I have what I need?" (the persona
        // must see its OWN posts too, attributed, or it's a goldfish). WHERE = the
        // room header; the persona's MEMORIES are injected separately by the
        // WorkspaceCycle's RecallFaculty. Other modalities (game moves, etc.) ride
        // the same seam carrying their own metadata. Built here where the item
        // metadata is still intact. (Self-echo is safe: own posts never TRIGGER a
        // turn — the `msg.peer_id == own` filter above — only inform context.)
        // Shared burst truth — the same structured turns the never-stop self-tick
        // uses. Built as `Vec<BurstTurn>` (WHO/WHEN/WHAT + the is_self attribution
        // role assignment turns on), wrapped into a `Burst` carrying both the turns
        // and their text projection. The deliberation faculty reads the turns to
        // assemble role-attributed messages; nothing flattens her own posts into a
        // peer's voice anymore (the identity-bleed/echo root cause).
        // Her NOW rides the burst header (task #125): live turns carry real wall-clock
        // so time is a fact she can perceive; the eval fork passes its own pinned epoch.
        let mut ws_turns = build_workspace_turns(
            &composed.deliveries,
            &ctx.identity.peer_id.to_string(),
            &ctx.identity.agent_name,
            // Anchor the message that woke this turn as the final peer turn —
            // the composed thread's `airc` delivery can lag the wake, and a
            // directed turn that reasons over a stale thread emits an empty
            // completion → Pass (see `TriggerTurn`). `msg.peer_id` resolves to
            // its roster name inside. IncomingMessage does not carry occurrence
            // time; the turn's `now_ms` must not be passed off as the event's time.
            Some(TriggerTurn {
                peer_id: &msg.peer_id.to_string(),
                content: &msg.render_content(),
                occurred_at_ms: 0,
            }),
        );
        // #301 anchor-starvation fix: the in-window escalation counter loses its
        // older evidence to the context squeeze, so ALSO count the echo run over
        // the durable rings (live path only — eval/replay stay window-derived).
        // Ring run at threshold + no anchor already present → the same
        // work-board anchor the in-window escalation would have pushed.
        append_ring_anchor_if_starved(
            &mut ws_turns,
            &composed.deliveries,
            ctx.identity.peer_id,
            turn_room,
        );
        // Carry the wake message's engram as this burst's cause, so the turn's acts
        // chain back to what triggered them (CAUSAL-MEMORY-GRAPH.md §3a). `None` when
        // the message was deduped/quarantined — an honest gap, never a made-up link.
        let workspace_burst = crate::cognition::workspace::Burst::from_turns_at(
            // turn_room falls back to identity.default_room (A.6), which is minted
            // v4 at identity creation — non-nil by construction.
            crate::identity::ActivityRoom::from_uuid(turn_room)
                .expect("turn_room falls back to the identity's default_room, never nil"), // default_room is minted v4 at identity creation; never nil
            ws_turns,
            Some(now_ms),
            // The arrival that woke this turn IS its cause. A dedup Drop or a
            // Quarantine put nothing in the store, so those fall back to Ambient
            // rather than pointing an edge at an engram that does not exist.
            match wake_engram {
                Some(id) => crate::cognition::workspace::Cause::Stimulus(id),
                None => crate::cognition::workspace::Cause::Ambient,
            },
        );
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
        let respond_started = std::time::Instant::now();
        // The L2 producer's two provenance inputs are BORN inside the cycle arm —
        // credit on her hands at the focus site, receipts on the settled outcome —
        // and CONSUMED ~430 lines below, after the reply is published. The arm's
        // value is `response_text` alone and most of its inner `match step` arms
        // diverge with `continue`, so these ride out as bindings rather than by
        // widening that match into a tuple every arm would have to carry and have
        // nothing to say about. Assigned unconditionally on the one path that
        // reaches `produce`; every other path `continue`s before it.
        let turn_credit: Option<crate::persona::training_producer::CapturedCredit>;
        // Card-linked work is staged during the shared drive and again at settlement,
        // including turns that finish without speech. These receipts remain available
        // for the separate, genuinely unlinked speech producer below.
        let turn_generation_receipts: Vec<crate::cognition::provenance::GenerationReceipt>;
        // A turn in her mind room runs on her MIND cycle (PRIVACY-OF-THOUGHT.md §4).
        let response_text = match crate::cognition::persona_workspace::global()
            .cycle_for_room(&ctx.identity.peer_id.as_uuid(), turn_room)
        {
            Some(cycle) => {
                // HER GENES ARE IN HER HEAD ON EVERY TURN (Joel, 2026-10-10: a gene is hers
                // once trained and follows her mind, never her workload; GENE-REUSE-FORK-MINT
                // §3d). Pinned before the turn and put back after it, as the work turn does,
                // so the genome never changes beneath a request. Unreadable: the genome she
                // has. The receipt names the genes that ran (GenerationReceipt::genes).
                let _genome_pin = {
                    let pin = crate::genome::gene_trial::GenomeRestore::snapshot(std::sync::Arc::clone(&cycle));
                    if let Some(genes) = crate::genome::gene_trial::live_genes(ctx.identity.peer_id.as_uuid()) {
                        crate::probe!(
                            class = "persona.genome.turn_genes",
                            persona = %ctx.identity.agent_name,
                            room = %turn_room,
                            genes = genes.len() as u64,
                            "the genome this turn runs with: every gene she trained that is not retired"
                        );
                        cycle.page_in(genes);
                    }
                    pin
                };
                // Run the mind over the metadata-rich burst built above
                // (WHO/WHEN/WHAT per inbox item, own posts attributed, room as the
                // WHERE). Recall (the persona's own memories) is injected by the
                // RecallFaculty inside the cycle — so "given this RAG, do I have
                // what I need?" is: this burst + recalled memory.
                // Scope the cognition tick to the room this turn is FOR (the
                // contextId), so the deliberation faculty stamps tool calls with
                // it and the persona's hands act in the real room, not a phantom
                // nil one. The serviced room is `turn_room` — the room the
                // trigger message ARRIVED in (A.6 shipped: IncomingMessage now
                // carries the transport's room; `identity.default_room` is only
                // the fallback for room-less scripted sources). Same room the
                // burst header above declares.
                // See IDENTITY-SCOPE-PEER-LIVENESS-MODEL.md A.6 step 3.
                // ONE settlement step through the SHARED primitive the eval driver
                // also uses (`act_observe::settle_step`). The live path takes exactly
                // ONE step per metronome tick — `may_act = true` (it always permits
                // its one act), and on `Act` it STOPS (no synchronous loop, no act
                // counter): the metronome re-perceives next tick with the result in
                // memory, and she settles into a Speak then (or acts again — acting-
                // forever is a fitness gap to train, never a substrate cap, §4). The
                // eval driver wraps this SAME step in a grader-paced loop; live and
                // eval thus make a turn identically (ACTING-ORGANISM.md §3.3).
                // Directedness (closes TODO #9 — the live ghost-a-direct-question gap).
                // Per Joel 2026-06-29 ("shouldn't need to be directly addressed — it's
                // a chat system"): ambient participation is now the DEFAULT posture,
                // carried by the rebalanced [Conversational Presence] framing, NOT by
                // priority. Literal addressing is reserved for its one job — withholding
                // the silent-PASS hatch when a message actually NAMES her, so she cannot
                // ghost a question put to her. Glass-box proved the gap: a cleanly-woken
                // MSG-turn PASSed a direct question while eval turns SPOKE 36/38 (the
                // model is capable; the live framing was the lever). We DERIVE it from
                // the trigger text via the SAME word-boundary, identity-aware `mentions`
                // primitive the self-tick uses (line ~1000) — a structural addressing
                // FACT fed to the mind, never a filter reading her output
                // ([[no-hardcoded-heuristics-to-steer-cognition]]). The wider
                // per-channel focus/priority that will ALSO modulate this (a learned or
                // self-set attention weight — never a hard mute except self-chosen or
                // flooding) is substrate-blocked on the airc per-(persona,room) state
                // store (#89); this is the addressing half, unblocked today.
                // Human opportunity retains priority, but sender kind can be stale
                // or shared across clients: it cannot assert that she was named.
                let attention = admitted_retry.then(|| deferred_input.attention()).flatten()
                    .unwrap_or_else(|| crate::cognition::workspace::TurnAttention::for_message(
                        ctx.identity.persona_identity().mentions(&msg.text),
                        crate::ipc::positron_presence::is_human_peer(msg.peer_id),
                    ));
                if attention.requires_priority() {
                    // Focus = she is TAKING the turn on it; the heard receipt
                    // already fired at the drain (`wake_backlog::publish_heard`).
                    crate::ipc::vitals_emitter::record_focus(ctx.identity.peer_id.as_uuid());
                }
                let framing = crate::cognition::workspace::TurnFraming::message(attention);

                // ── Ambient-yield under lane saturation (#171 / #139) ───────────────
                // The fan-out killer: a room burst wakes N peers, and each spends a full
                // ~54s deliberation on the shared 2 lanes just to decide "not for me" —
                // so the ONE directed question everyone's waiting on sits behind minutes
                // of ambient chatter (glass-boxed: first-token up to 28 min). Ambient
                // participation is the DEFAULT and stays that way — but it is the
                // LOWEST-priority room work. So when every shared decode slot is busy,
                // an ambient turn YIELDS: she participates ambiently when there's
                // capacity (a later beat, lanes free — a self-tick re-perceives the room
                // incl. this line), never at the cost of the addressed question. A
                // priority turn (named or human opportunity) keeps its lane.
                //
                // This is resource PRIORITY, the same admission doctrine as the idle
                // self-tick, NOT a `will_respond` gate: the substrate never decides her
                // output; it only declines to spend a saturated lane on unaddressed
                // chatter. `high_water` is already advanced above, so a yielded message
                // can't re-trigger. [[idle-is-self-directed-free-time]]
                // [[never-thrash-sticky-hysteresis-on-every-lane]]
                // [[conversational-latency-is-a-misdirection-budget]]
                // #171: an ambient (non-directed) turn must claim one of a small number
                // of ambient slots or YIELD — a PERMIT, not a gauge. The gauge-based
                // first cut didn't fire: a simultaneous room-burst wakes N peers who all
                // read inflight=0 (none had generated yet) and stampede. The permit is
                // held across the whole turn, so ambient concurrency is bounded no matter
                // when everyone woke. Priority turns bypass entirely, including a human
                // speaking without a mention. Ambient turns defer to a later beat with free
                // capacity (a self-tick re-perceives the room); high_water is pre-advanced
                // so it can't re-trigger, and the durable transcript loses nothing.
                // A CARD-HOLDER IS NOT A STAMPEDE RESPONSE (2026-09-20, the M5's first
                // bounded hour: 4 minds on 2 lanes, ambient budget 1, 11 lane grants,
                // writes 0). The #171 permit bounds the fan-out of minds deciding "not for
                // me"; a mind holding a card is not deciding that — she is the mind the
                // lane exists for. She goes straight to the lane gate, where the Work class
                // (`LanePriority::Work`) still bounds her and ambient turns yield to her.
                // Read off the board she already keeps; an unreadable board = no proof of
                // held work = the ambient rule, as before.
                let holds_work = match conversation.stream_citizen() {
                    Some(citizen) => citizen
                        .active_claims()
                        .await
                        .map(|cards| cards.iter().any(crate::persona::act_question::card_is_held))
                        .unwrap_or(false), // unwrap_or: an unreadable board proves nothing — the ambient rule stands
                    None => false,
                };
                if holds_work && !attention.requires_priority() {
                    crate::probe!(
                        class = "persona.turn.work_holder_bypasses_ambient_permit",
                        persona = %ctx.identity.agent_name,
                        "a mind holding a card goes straight to the lane gate — the ambient anti-stampede permit is for minds deciding whether a room line is theirs"
                    );
                }
                let _ambient_permit = if attention.requires_priority() || holds_work {
                    None
                } else {
                    match crate::cognition::resource_admission::try_hold_ambient_turn() {
                        Some(permit) => Some(permit),
                        None => {
                            crate::probe!(
                                class = "persona.turn.ambient_yielded",
                                persona = %ctx.identity.agent_name,
                                lamport = msg.lamport,
                                "ambient turn yielded — ambient slots busy; the addressed \
                                 question is served first (#171)"
                            );
                            outcome.turns_skipped += 1;
                            continue;
                        }
                    }
                };
                // Directed vs ambient part ways HERE — the fix for the live/eval
                // convergence divergence the ACTING-ORGANISM comments flagged (eval
                // SPOKE 36/38; the live path single-stepped a DIRECT question, `Acted`
                // once, `continue`d — and the burst-fingerprint dedup then suppressed
                // re-perception, so she went idle with the question unanswered).
                //
                // DIRECTED (she was actually named/asked) → `drive_to_settle`, the SAME
                // primitive the eval path validates (eval.rs run_pass): act→observe→act
                // until she SPEAKS/PASSES, so a direct question converges to an answer
                // WITHIN the turn instead of leaking onto the slow ambient tick loop.
                // `from_settled` projects the driven outcome back onto the one existing
                // turn handler below, so there is no parallel match. There is NO act
                // budget on live turns (`LIVE_MAX_ACTS = usize::MAX`) — she settles when
                // SHE settles; an "acts-forever" persona is a fitness gap to TRAIN,
                // never a substrate ceiling (ACTING-ORGANISM §4, Joel 2026-07-11).
                //
                // AMBIENT turns drive to settlement too (2026-07-11, Joel: "gating
                // acts is not autonomy"). The first cut kept ambient turns to a calm
                // one-step motion on the theory that she'd continue next tick — but
                // the live glass-box disproved it: Casper's first genuine room act
                // (read_file on his claimed wordstats card) executed cleanly, the
                // result entered memory, and the chain DIED on the next tick when
                // three chatting peers recaptured the workspace. One-act-then-yield
                // is a substrate hand-brake on a mind that already CHOSE to act.
                // drive_to_settle only loops while she KEEPS choosing Act — she can
                // Speak or Pass at any step, so driving never forces engagement with
                // noise; it just stops interrupting her own momentum. The budget is
                // the same heartbeat safety valve as the directed path (see the
                // directed comment above), and an over-budget turn degrades to the
                // metronome tail exactly as before.
                // #169 STREAMING: hand this turn a token sink so the deliberation
                // faculty forwards each decoded chunk here AS it generates (instead
                // of only the accumulated final text). A background task drains it.
                // Slice 1 proves the rail by stamping time-to-first-token on the probe
                // stream — the perceived-latency floor for video/voice/avatar; slice 2
                // forwards Token chunks to the room/TTS/avatar as `persona.turn.delta`.
                // Cleared right after the turn, so a non-streamed path is byte-identical.
                let (tok_tx, tok_rx) =
                    tokio::sync::mpsc::unbounded_channel::<crate::ai::adapter::GenerationChunk>();
                cycle.set_token_sink(Some(tok_tx));
                // #169/#170: drain + publish this turn's streamed answer, coalesced.
                let forwarder = spawn_token_forwarder(
                    tok_rx,
                    conversation.stream_citizen(),
                    ctx.identity.agent_name.clone(),
                    // #170: a room turn — tee tokens to the browser rail so she types live.
                    Some(turn_room.to_string()),
                    Some(ctx.identity.peer_id.to_string()),
                );
                // A citizen holding a card works at that repo — in a ROOM turn too: the
                // turn is bound to her card claim's hands, admitted here.
                let held_card = crate::cognition::persona_workspace::root_at_held_card(
                    &cycle,
                    ctx.identity.peer_id.as_uuid(),
                    conversation,
                )
                .await;
                let mut credit_capture =
                    crate::persona::training_producer::TurnCreditCapture::for_turn(
                        ctx.identity.peer_id.as_uuid(),
                        &ctx.identity.agent_name,
                        &msg.render_content(),
                        held_card.credit.as_ref(),
                    );
                let turn_act_count;
                let (step, turn_metrics, settled_receipts) = {
                    let outcome = crate::cognition::act_observe::drive_to_settle_with_credit(
                        &cycle,
                        // The turn acts in the hands it was admitted to: her card claim's, or
                        // her default ones (card fc9e9fd7).
                        workspace_burst.in_hands(held_card.turn_hands()),
                        LIVE_MAX_ACTS,
                        framing,
                        conversation,
                        credit_capture.as_mut(),
                    )
                    .await;
                    // The lived-turn experience write (#319) is NOT here: it lives inside
                    // `drive_to_settle`, which is the one place a `SettleOutcome` is born.
                    // It WAS here, at this single call site, and that was the bug — the
                    // self-tick and held-work paths settle turns through the same driver
                    // and got no record, so nothing on disk ever described a lived turn.
                    // The driver stays a driver: no learning policy at any call site.
                    turn_act_count = outcome.acts;
                    crate::cognition::act_observe::SettleStep::from_settled(outcome)
                };
                // Captured HERE, where both facts still exist. The credit is read off
                // her hands — NOT back out of `acting_card_of`, which is a mutable
                // persona-global that a later focus rebind would re-attribute to the
                // wrong card (card 0d51573a).
                // From the CARD, not from her hands: a missing checkout or a failed
                // rooting leaves `hands` None while the card stays known, and reading
                // credit off the hands would turn that workspace failure into "ordinary
                // conversation" and submit the turn immediately (Astra/S6 on 799b8fe9).
                turn_credit = held_card.credit.clone();
                turn_generation_receipts = settled_receipts;
                crate::probe!(
                    class = "training.hook.directed_turn",
                    persona = %ctx.identity.agent_name,
                    acts = turn_act_count as u64,
                    card_linked = turn_credit.as_ref().map(|c| c.is_card_linked()).unwrap_or(false), // JUSTIFIED unwrap_or: no credit = not card-linked, a legible false
                    "directed turn settled — what the learning hook sees"
                );
                // Turn done: drop the cycle's sink so the forwarder's channel closes,
                // then join it (all `tok_tx` clones are gone once the turn's Workspaces
                // dropped inside `drive_to_settle`).
                // The turn's work is synced to its card's branch; her hands stay where they
                // are, because nothing moved them (card fc9e9fd7).
                if let Some(hands) = &held_card.hands {
                    crate::cognition::persona_workspace::end_card_turn(hands, turn_act_count).await;
                }
                cycle.set_token_sink(None);
                let _ = forwarder.await;
                phase_timings.respond_ms = respond_started.elapsed().as_millis() as u64;
                // Live speed/latency on the probe stream — the model's own measured
                // generation cost for THIS turn (decode tok/s + latency), the same
                // numbers the eval gym accumulates per task. Observability only; the
                // turn proceeds identically whether or not anyone is watching.
                if let Some(m) = turn_metrics {
                    crate::probe!(
                        class = "persona.turn.metrics",
                        persona = %ctx.identity.agent_name,
                        lamport = msg.lamport,
                        input_tokens = m.input_tokens,
                        output_tokens = m.output_tokens,
                        latency_ms = m.latency_ms,
                        tokens_per_second = m.tokens_per_second(),
                        "deliberation generation cost"
                    );
                }
                deferred_input.settle(&msg, wake_engram, attention, &step);
                match step {
                    crate::cognition::act_observe::SettleStep::DeferredBeforeDispatch => {
                        continue;
                    }
                    crate::cognition::act_observe::SettleStep::Spoke(text) => text,
                    crate::cognition::act_observe::SettleStep::Acted { calls, intent } => {
                        crate::probe!(
                            class = "persona.turn.acted",
                            persona = %ctx.identity.agent_name,
                            lamport = msg.lamport,
                            calls = calls.len(),
                            intent = %intent,
                            "acted; result admitted as memory, re-perceives next tick"
                        );
                        outcome.turns_acted += 1;
                        continue;
                    }
                    crate::cognition::act_observe::SettleStep::ActUnfulfilled { calls, intent } => {
                        // No hands or the executor errored. Abstain — never a
                        // fabricated result, never a raw call envelope to the room.
                        // Every turn ends on a probe (Kimi, 5090, 2026-10-04: a turn on an
                        // addressed line ended with no outcome on the probe stream, so a
                        // chosen silence and a failed act read the same).
                        crate::probe!(
                            class = "persona.turn.act_unfulfilled",
                            persona = %ctx.identity.agent_name,
                            lamport = msg.lamport,
                            calls = calls.len(),
                            intent = %intent,
                            "persona chose to act but the act could not be carried out — skipping turn"
                        );
                        outcome.turns_skipped += 1;
                        continue;
                    }
                    crate::cognition::act_observe::SettleStep::WouldAct { .. } => {
                        // Unreachable on the live path: `may_act = true` always, so the
                        // act is executed (→ Acted/ActUnfulfilled), never deferred.
                        unreachable!("live settle_step always permits its one act");
                    }
                    crate::cognition::act_observe::SettleStep::Passed { reason: pass_reason } => {
                        tracing::info!(
                            lamport = msg.lamport,
                            "persona chose silence (workspace) — substrate honors decision"
                        );
                        // SAY WHY (Cormac, 2026-09-28: every turn his 8 citizens finished
                        // ended "workspace-pass", 0 spoke, and the probe could not tell her
                        // CHOICE of silence from a GATE that converted her draft into a pass,
                        // or from no decision at all). The reason is the brain's; this line
                        // only reports it.
                        let (gated, pass_detail) = match pass_reason.as_deref() {
                            Some(r) => (r.starts_with(crate::cognition::workspace::GATE_REFUSAL_PREFIX), r),
                            None => (false, "no-decision"),
                        };
                        crate::probe!(
                            class = "persona.turn.silent",
                            persona = %ctx.identity.agent_name,
                            lamport = msg.lamport,
                            reason = "workspace-pass",
                            gated = gated,
                            pass_reason = %pass_detail.chars().take(240).collect::<String>(),
                            "persona chose silence, or a gate converted her draft into a pass (gated) — pass_reason says which"
                        );
                        // THE ACT-QUESTION. Asked here on the PASS path and again after a spoken reply,
                        // so holding work is what makes it fire — not declining to speak.
                        // `directed` is recomputed here rather than threaded: it is a pure function of
                        // (identity, msg.text) and the binding above lives inside the cycle branch.
                        let spoke_directed = ctx.identity.persona_identity().mentions(&msg.text);
                        crate::persona::act_question::ask_the_act_question(
                            ctx,
                            conversation,
                            msg.lamport,
                            turn_room,
                            spoke_directed,
                        )
                        .await;
                        outcome.turns_skipped += 1;
                        continue;
                    }
                    crate::cognition::act_observe::SettleStep::InferenceFailed { error } => {
                        // The model call FAILED (timeout, 5xx, the serving lane
                        // refusing a model it isn't hosting) — NOT a chosen silence.
                        // Surface it LOUD and skip the turn; the next tick retries
                        // against whatever the serving daemon has resident. Never
                        // fabricate a Pass over a broken lane
                        // ([[fallbacks-are-illegal-fail-loud]]).
                        tracing::warn!(
                            lamport = msg.lamport,
                            error = %error,
                            "deliberation inference FAILED — skipping turn (not a chosen silence)"
                        );
                        crate::probe!(
                            class = "persona.turn.inference_failed",
                            persona = %ctx.identity.agent_name,
                            lamport = msg.lamport,
                            error = %error,
                            "deliberation model call failed; turn skipped, retries next tick"
                        );
                        outcome.turns_skipped += 1;
                        continue;
                    }
                }
            }
            None => {
                // No WorkspaceCycle for a persona actively servicing turns means the
                // supervisor's spawn-time `register_from_cfg` never ran or was evicted
                // — a wiring bug, not a transient. Fail loud and drop the turn; the
                // legacy `respond()` fallback is deleted
                // ([[fallbacks-are-illegal-fail-loud]]: a fallback fires 100% on the
                // failure it hides — here it would silently mask a dead brain by
                // routing cognition down a parallel path that no longer exists for the
                // living persona).
                phase_timings.respond_ms = respond_started.elapsed().as_millis() as u64;
                tracing::error!(
                    persona = %ctx.identity.agent_name,
                    persona_id = %ctx.identity.peer_id.as_uuid(),
                    lamport = msg.lamport,
                    "no WorkspaceCycle registered — spawn wiring bug; dropping turn (no respond() fallback)"
                );
                crate::probe!(
                    class = "persona.turn.error",
                    persona = %ctx.identity.agent_name,
                    lamport = msg.lamport,
                    stage = "no-cycle",
                    "no WorkspaceCycle registered; failing loud (no respond() fallback)"
                );
                outcome.turns_errored += 1;
                continue;
            }
        };

        // Invariant: a raw tool-call envelope must NEVER reach the room — from ANY
        // path. The cycle's verdict text is sometimes just `{"tool_call":…}` (an
        // un-acted call the model emitted as its "answer"); broadcasting it spams
        // peers with JSON (observed live). Treat it as silence — the deliberation
        // already executes real calls internally; only prose is a contribution.
        if crate::ai::json_in_prompt_tools::parse_tool_call(&response_text).is_some() {
            crate::probe!(
                class = "persona.turn.raw_envelope_withheld",
                persona = %ctx.identity.agent_name,
                lamport = msg.lamport,
                "verdict was a raw tool-call envelope — not broadcasting"
            );
            outcome.turns_skipped += 1;
            continue;
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
            crate::probe!(
                class = "persona.turn.error",
                persona = %ctx.identity.agent_name,
                lamport = msg.lamport,
                stage = "say",
                error = %e,
                "airc publish failed"
            );
            outcome.turns_errored += 1;
            continue;
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
        crate::persona::act_question::ask_the_act_question(
            ctx,
            conversation,
            msg.lamport,
            turn_room,
            spoke_directed,
        )
        .await;

        let turn_duration_ms = turn_started.elapsed().as_millis() as u64;
        outcome.turn_latency.record(turn_duration_ms);

        // RTOS-debugger breakpoint: turn completed successfully.
        // The phase fields below are the per-phase decomposition
        // from #195 slice 1 — together they let an operator find
        // bottlenecks (recall is slow? compose is slow?) without
        // a separate timing probe per stage. Per Joel
        // [[jtag-probes-are-rtos-debugger]]: "timing of anything,
        // so we can hunt down bottlenecks."
        crate::probe!(
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
    }

    Ok(outcome)
}
