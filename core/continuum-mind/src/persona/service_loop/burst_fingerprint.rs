use super::*;

/// The "is there anything for me to attend to?" MECHANISM for the heartbeat: a
/// fingerprint of the burst the deliberation will reason over, combining the TWO
/// concerns the never-stop loop serves —
///
/// 1. **EXTERNAL airc chat** — items NOT authored by this persona. A concern must
///    not subscribe to its OWN chat output (the cbar rule): if own posts counted,
///    the act of speaking would change the world, re-trigger the next slice, and
///    the persona would talk to itself forever (observed live, 2026-06-22 — 19
///    self-talk cycles flooding the room). So own chat posts are excluded: wake on
///    others' activity, act, and once spoken go back to sleep because nothing
///    EXTERNAL changed.
///
/// 2. **The persona's OWN active work** — its claimed `WorkCard`s (the `active-work`
///    source). This is the interior DRIVE: a persona is not a request→response
///    handler that only wakes when others poke it — it carries its own work and the
///    heartbeat advances it ([[alignment-through-mutual-self-interest]]: provide
///    compute, let her pursue her own thread; AUTONOMOUS-PROJECT-LOOP). Folding
///    work-card state in is FLOOD-SAFE precisely because it is cards, not chat:
///    speaking does not change her cards, so speech still cannot re-trigger the
///    loop; only real progress (a card's state changing as she works it) re-fires
///    the next slice — so the loop continues exactly as long as she is making
///    progress on her own work, and goes quiet when she is not. (Grinding an
///    unchanged card every tick — deliberating across ticks before any state change
///    — is a later slice with its own cadence; this slice gives her work the power
///    to WAKE her, symmetric to external content.)
///
/// NOT a judgment — the `WorkspaceCycle` LLM still decides what to do; this is pure
/// change-detection over what she should be attending to.
/// [[organic-substrate-continuous-concern-scheduler]].
pub(super) fn burst_fingerprint(
    deliveries: &[crate::cognition::rag_budget::RagDelivery],
    own_peer: &str,
) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for item in deliveries
        .iter()
        .filter(|d| d.source_id == "airc")
        .flat_map(|d| d.items.iter())
    {
        let who = item
            .metadata
            .get("peer_id")
            .and_then(|v| v.as_str())
            .unwrap_or("peer");
        if who == own_peer {
            continue; // don't subscribe to my own chat output
        }
        item.content.hash(&mut h);
    }
    // The interior drive: her own claimed work. Cards, not chat — so this adds
    // self-origination without reopening the self-talk flood (see doc above). The
    // source_id literal mirrors the "airc" literal above; `ActiveWorkSource` owns it.
    for item in deliveries
        .iter()
        .filter(|d| d.source_id == "active-work")
        .flat_map(|d| d.items.iter())
    {
        item.content.hash(&mut h);
    }
    h.finish()
}

/// The never-stop heartbeat slice for the deliberation concern — a cognition
/// cycle with NO inbound message. Compose the current world-state; if it CHANGED
/// since the last cycle (cheap mechanism) run the persona's `WorkspaceCycle` (the
/// LLM is the judge) and speak if it decides to, else sleep. This is how a persona
/// follows through on its OWN open intention (it said "I'll search" → next tick its
/// own post is in the burst → it acts) WITHOUT anyone parsing its words: the mind,
/// always getting time, judges its own state. `Pass` = woke, nothing to do, sleep.
/// #169/#170: drain a turn's streaming token channel off-thread and publish the
/// answer to the room as ephemeral airc stream chunks (progressive render for
/// positron/TTS/avatar). ONE place for both the message and self-tick turn paths.
///
/// COALESCED (#170): tokens are batched and flushed at most every `FLUSH_EVERY`
/// (250ms) as ONE chunk, not one-per-token — a room of N personas at per-token
/// rate is a wire storm (the airc Monitor hit "output rate too high"). A timer
/// flushes buffered tokens even when generation pauses. The buffered
/// remainder + a `text_end` marker flush when the turn closes. Also stamps
/// `persona.turn.first_token` (the streaming latency floor) on the first token.
/// `citizen` is `None` for non-airc conversations (tests) → then this is just the
/// first-token probe, no publishing.
pub(super) fn spawn_token_forwarder(
    mut rx: tokio::sync::mpsc::UnboundedReceiver<crate::ai::adapter::GenerationChunk>,
    citizen: Option<std::sync::Arc<dyn crate::persona::airc_citizen::AircCitizen>>,
    persona: String,
    // #170: room + sender for the local WS token rail. Some on a room turn (tee to the
    // browser so a persona visibly types); None on an idle self-tick (nothing to show
    // — an idle mind isn't addressing anyone). Correlate the client typing bubble by
    // (room_id, sender_id) — the per-turn `stream_id` is NOT the final message id.
    room_id: Option<String>,
    sender_id: Option<String>,
) -> tokio::task::JoinHandle<()> {
    // 250ms: a typing indicator does NOT need per-token frames (the durable `say`
    // is the authoritative text) — batching to ~4 frames/sec keeps the render smooth
    // to a human eye while cutting wire traffic ~6x. 50ms flooded the bus (a room of
    // personas × per-token frames killed subscribers).
    const FLUSH_EVERY: std::time::Duration = std::time::Duration::from_millis(250);
    // A turn in her mind room types to no one (PRIVACY-OF-THOUGHT.md §4, sink 7): the
    // forwarder still drains her tokens, but publishes nothing to the room and tees
    // nothing to the browser rail.
    let private = match (
        sender_id.as_deref().and_then(|s| Uuid::parse_str(s).ok()),
        room_id.as_deref().and_then(|r| Uuid::parse_str(r).ok()),
    ) {
        (Some(sender), Some(room)) => crate::cognition::mind_room::is_private_room(sender, room),
        _ => false,
    };
    let (citizen, room_id, sender_id) = if private {
        if let Some(sender) = sender_id.as_deref().and_then(|s| Uuid::parse_str(s).ok()) {
            crate::cognition::mind_room::note_withheld(sender, "token_stream");
        }
        (None, None, None)
    } else {
        (citizen, room_id, sender_id)
    };
    tokio::spawn(async move {
        let started = std::time::Instant::now();
        let stream_id = uuid::Uuid::new_v4().to_string();
        let mut first = true;
        let mut seq: u64 = 0;
        let mut buf = String::new();
        let mut flush =
            tokio::time::interval_at(tokio::time::Instant::now() + FLUSH_EVERY, FLUSH_EVERY);
        flush.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        // Tee one flushed chunk onto the local WS rail (#170) — no-op unless this is a
        // room turn (room+sender Some) AND a browser is subscribed.
        let tee = |seq: u64, token: String, done: bool| {
            if let (Some(room), Some(sender)) = (&room_id, &sender_id) {
                crate::persona::stream_rail::publish(crate::persona::stream_rail::StreamDelta {
                    room_id: room.clone(),
                    sender_id: sender.clone(),
                    stream_id: stream_id.clone(),
                    seq,
                    token,
                    done,
                });
            }
        };
        // START BEACON (#254 slice 1): one empty-token frame when the turn's
        // forwarder starts, before admission or prefill, which on a cold lane can run
        // minutes. The client renders an entry with no text yet as "X is
        // responding…" under the last message (Joel 2026-07-30: the interface
        // looked DEAD while four minds were mid-turn). The `done` flush at turn
        // close retires the beacon even if the turn settles without speech.
        tee(seq, String::new(), false);
        loop {
            let closed = tokio::select! {
                // A continuously ready producer cannot starve the flush deadline.
                biased;
                _ = flush.tick(), if !buf.is_empty() => false,
                chunk = rx.recv() => match chunk {
                    Some(crate::ai::adapter::GenerationChunk::Token(t)) if !t.is_empty() => {
                        if first {
                            first = false;
                            tracing::info!(
                                persona = %persona,
                                first_token_ms = started.elapsed().as_millis() as u64,
                                "persona.turn.first_token — streaming rail live (latency floor)"
                            );
                        }
                        buf.push_str(&t);
                        continue;
                    }
                    Some(_) => continue,
                    None => true,
                },
            };
            if !buf.is_empty() {
                let flushed = std::mem::take(&mut buf);
                // Local subscribers must see this batch before remote publication waits.
                tee(seq, flushed.clone(), false);
                if let Some(c) = &citizen {
                    let _ = c
                        .publish_stream_chunk(&airc_lib::StreamChunk::text_token(
                            stream_id.clone(),
                            seq,
                            flushed,
                        ))
                        .await;
                }
                seq += 1;
            }
            if closed {
                break;
            }
        }
        // Retire the local bubble independently of remote end-marker delivery.
        tee(seq, String::new(), true);
        if let Some(c) = &citizen {
            let _ = c
                .publish_stream_chunk(&airc_lib::StreamChunk::text_end(stream_id.clone(), seq))
                .await;
        }
    })
}

/// One intrinsic heartbeat: the deterministic head (held-work act question, kanban pull),
/// then the musing tail, whose generation asks the lane at ambient priority.
pub(super) async fn run_self_cycle(
    ctx: &HostedPersona,
    conversation: &mut dyn CitizenConversation,
    opts: &ServeOptions,
    last_burst_fp: &mut u64,
    // The room her own mind woke her in (a Perceive wake): she works there this cycle,
    // and that room counts as perceived. `None` = her held claim's room, else home.
    wake_room: Option<uuid::Uuid>,
    pending: &mut crate::persona::wake_backlog::PendingInput,
) {
    let now_ms = (opts.now_ms)();
    // A self-cycle IS cognition: the claim-renewal pump reads this pulse, and it
    // was stamped only on message turns — a holder working her card for thirty
    // minutes without a room message read as idle, her renewals were denied, her
    // hold lapsed, she looked free and pulled a second card (2026-09-05: 32 pulls
    // and 32 re-stagings for 12 cards in 15 minutes).
    crate::persona::cognition_pulse::touch(ctx.identity.peer_id.as_uuid(), now_ms);
    // One explicit work choice drives both the room and the working checkout.
    // Ordinary project cards are on subscribed boards, not in the benchmark
    // registry. Lease renewal is liveness, not a new focus selection.
    if let Some(room) = wake_room {
        crate::persona::perception_feed::mark_perceived(
            ctx.identity.peer_id.as_uuid(),
            room,
            0,
            None,
            now_ms,
        );
    }
    let focus_room = if wake_room.is_some() { wake_room } else { match conversation.stream_citizen() {
        Some(citizen) => match super::super::work_focus::focus_room(citizen.as_ref()).await {
            Ok(room) => room,
            Err(error) => {
                continuum_base::probe!(
                    class = "persona.selftick.focus_unavailable",
                    persona = %ctx.identity.agent_name,
                    error = %error,
                    "cannot resolve held work room; preserve work and retry next self-cycle",
                );
                return;
            }
        },
        None => None,
    } };
    if let Some(room) = focus_room {
        if room != ctx.identity.default_room {
            continuum_base::probe!(
                class = "persona.selftick.focused",
                persona = %ctx.identity.agent_name,
                room = %room,
                "self-tick bound to the held claim's room instead of home — \
                 focus holds until the card settles",
            );
        }
    }
    // THE HELD-WORK HEARTBEAT. If she holds a claimed card, the self-tick ADVANCES
    // it — a real held-work turn (hands rooted at the staged checkout, the
    // pass-reason completion edge), not a musing turn. This is what finishes a
    // claimed card in a QUIET room: the held-work gate used to live ONLY on the
    // message path (`ask_the_act_question` after a speak/pass), so an idle citizen
    // holding a claim just mused and never worked it — measured 2026-09-02, 22 cards
    // Claimed with zero `persona.work.gate` and zero progress. A card is
    // progress-driven, not change-driven, so this runs every tick until she
    // concludes it (`PASS: done`) — the autonomous loop the architecture always
    // promised ("the heartbeat advances my thread, not just reacts to pokes").
    // Returns early so she never ALSO spends a musing turn the same tick.
    // NO AUTOMATIC PULL, EVER (HER-LOOP row A; Joel 2026-10-04: "Kimi is not supposed
    // to be doing benchmarks"). The self-cycle used to take a card off a working round's
    // deck for an idle citizen; that is how ac49a7d5 landed in her hands without her
    // choosing it. Boards are in her perception; claiming is her act, with her hands.
    let work_room = focus_room.unwrap_or(ctx.identity.default_room); // unwrap_or: no held claim = home room
    // HER INITIATIVE rides this beat (`persona::initiative_turn`): with nothing held, the
    // gate offers her a turn with her hands, paced by THIS heartbeat and no timer of its
    // own. Not on a wake her own mind raised (a continuation, a perceive): that wake is
    // already her choice of what to attend to, and the turn below serves it.
    let initiative = if wake_room.is_none() {
        crate::persona::initiative_turn::Initiative::on_beat(
            pending.initiative_passed_at_ms(),
            crate::persona::perception_feed::moved_at_ms(ctx.identity.peer_id.as_uuid()),
        )
    } else {
        crate::persona::initiative_turn::Initiative::NotOffered
    };
    let work = crate::persona::act_question::ask_the_act_question(
        ctx,
        conversation,
        now_ms, // no triggering message — the tick clock stands in for lamport
        work_room,
        false, // a self-tick is never a directed address
        initiative,
    )
    .await;
    pending.settle_work(&work);
    pending.note_initiative(&work, now_ms);
    if work.drove {
        // She WORKED this tick (her claim, or her own initiative) — that is engagement, so
        // the caller must keep her at the fast beat, not rest her. The caller reads
        // BeatOutcome from whether `last_burst_fp` changed; bump it so a work turn always
        // counts as Engaged. Without this she works one tick, is judged "nothing new", and
        // backs off toward the rest cap — the opposite of "virtually no time between
        // contexts" while a card is in her hands. (LATENCY LAW / #the-build-order.)
        // An initiative turn she PASSED is not engagement: the beat rests, as a fruitless
        // musing did, so an idle mind is not offered a turn every engaged beat.
        if work.engaged {
            *last_burst_fp = last_burst_fp.wrapping_add(1);
        }
        return;
    }
    let composed = {
        let cognition = &ctx.cognition;
        // Passing None here is what made idle ticks blind to the board (#331 / #127).
        cognition
            .compose_for_turn(
                now_ms,
                Some(focus_room.unwrap_or(ctx.identity.default_room)),
            )
            .await
    };
    // Collapse loop-filler BEFORE anything reasons over the burst (task #16). Two idle
    // personas cycling stock courtesy templates each append another COPY per tick — the
    // item list grows so the fingerprint changes, even though no DISTINCT turn is new,
    // and the wake fires into a 40s decode → resonance. Deduping to first-occurrence
    // makes a repeated template a no-op for the fingerprint (stable → sleep), while a
    // genuinely new turn still changes it (→ wake). Symmetric to burst_fingerprint's
    // own-post exclusion — scheduling hygiene, not cognition-steering. See
    // `persona::loop_dedup`. [[false-refusal-anchor-present-but-positionally-defeated]].
    let deliveries = crate::persona::loop_dedup::dedup_loop_filler(&composed.deliveries);
    // Wake on a CHANGE to what I should attend to — others' chat (own chat excluded
    // so my speech can't spiral into self-talk) OR my own active work (so the
    // heartbeat advances my thread, not just reacts to pokes). See burst_fingerprint.
    let fp = burst_fingerprint(&deliveries, &ctx.identity.peer_id.to_string());
    if fp == *last_burst_fp {
        return; // nothing NEW to attend to (no external change, no work progress) → sleep
    }
    *last_burst_fp = fp;
    // Structured turns (own posts attributed as self → assistant, peers → user),
    // wrapped into a Burst carrying both the turns and their text projection — the
    // SAME shape the message path builds.
    let mut selftick_turns = build_workspace_turns(
        &deliveries,
        &ctx.identity.peer_id.to_string(),
        &ctx.identity.agent_name,
        // The self-tick perceives AMBIENT state — no single triggering
        // message to anchor. It re-derives the thread from the (now caught-up)
        // delivery, which is exactly why it recovers the message-path's missed
        // turn ~one tick later; anchoring is the message path's job.
        None,
    );
    // #301: the self-tick is where the live photocopy chains actually run
    // (idle re-announcements), so it gets the same ring-counted anchor
    // escalation as the message path — window-derived counters starve here too.
    // A6 CONSISTENCY: the tick's room is her FOCUS room when she holds live work
    // (the room of her freshest claim — the same room the composition above framed
    // grounding against), else her default. Before this, the composition used the
    // focus room while the anchor/burst/drive/say all used default — she reasoned
    // about her claim's room and then acted in a different one.
    let tick_room = focus_room.unwrap_or(ctx.identity.default_room); // no focus claim → the tick drives in her default room by design
    append_ring_anchor_if_starved(
        &mut selftick_turns,
        &deliveries,
        ctx.identity.peer_id,
        tick_room,
    );
    let room_invariant =
        "focus room comes from a live claim's real room; default_room is minted v4 — never nil";
    let activity_room = crate::identity::ActivityRoom::from_uuid(tick_room).expect(room_invariant); // claim rooms are spawned real; default_room minted v4 — nil unreachable
    let burst = crate::cognition::workspace::Burst::from_turns_at(
        activity_room,
        selftick_turns,
        Some(now_ms),
        // Ambient, and honestly so. The self-tick wakes on a CHANGE to a re-read
        // projection (`burst_fingerprint` over composed deliveries), not on an
        // arrival — the RAG sources hand back rendered text with the identity of the
        // items that produced it already discarded, so there is no engram to point at.
        // Something DID cause this turn; the projection layer is where its name was
        // lost. Handles are what would make it nameable
        // (docs/architecture/CONTENT-TRAVELS-BY-HANDLE.md).
        crate::cognition::workspace::Cause::Ambient,
    );
    // A continuation naming her mind room lands here as `tick_room`: it runs on her MIND
    // cycle, with its own working memory (PRIVACY-OF-THOUGHT.md §4).
    let Some(cycle) = crate::cognition::persona_workspace::global()
        .cycle_for_room(&ctx.identity.peer_id.as_uuid(), tick_room)
    else {
        return; // no cycle registered (shouldn't happen) — nothing to run
    };
    // Addressing PERCEPTION, not a silence directive. The old framing asserted "no one
    // addressed you just now" unconditionally and told her to "stay silent (PASS)" —
    // but a self-tick fires on ANY external change, which may BE a message that named
    // her, so the blanket claim was sometimes a lie, and the "stay silent" clause
    // puppeted the outcome ([[no-hardcoded-heuristics-to-steer-cognition]],
    // [[design-the-persona-as-a-being]]). Instead we DERIVE whether she was addressed
    // from the actual external content — `PersonaIdentity::mentions` (word-boundary,
    // identity-aware, so short names like "ai"/"bo" don't false-match) over the airc
    // items authored by OTHERS — and feed it as a fact. The mind decides what to do
    // with the fact; the substrate only perceives.
    let identity = ctx.identity.persona_identity();
    let own_peer = ctx.identity.peer_id.to_string();
    let addressed = deliveries
        .iter()
        .filter(|d| d.source_id == "airc")
        .flat_map(|d| d.items.iter())
        .filter(|item| {
            // External authors only — her own posts naming herself aren't an address.
            item.metadata
                .get("peer_id")
                .and_then(|v| v.as_str())
                .map(|p| p != own_peer)
                .unwrap_or(true) // unwrap_or: an unreadable board counts as claimable so the pull tries, never silently skips
        })
        .any(|item| identity.mentions(&item.content));
    continuum_base::probe!(
        class = "persona.selftick.perceive",
        persona = %ctx.identity.agent_name,
        addressed,
        "self-tick addressing perception — input-derived fact, not a force-respond gate"
    );
    // The wake FLOOR (#91): honor a mute SHE set on this lane. A hard mute = she
    // chose deliberate, duration-bounded blindness (even a direct address is held);
    // a soft mute drops ambient chatter but a direct address still cuts through
    // (the interrupt floor — "mute the chatter, not the alarm"). With no mute set
    // (the default) this is always true — identical to prior behavior, inert until
    // she steers. The fingerprint was already advanced above, so a muted change is
    // SEEN-and-dismissed (it won't re-fire); a snooze auto-expiring restores
    // awareness to the next change. This gates SCHEDULING on her own choice + a
    // structural fact, never her decision ([[no-hardcoded-heuristics-to-steer-cognition]],
    // [[focus-is-self-allocation-not-siloing]]).
    // Focus is resolved from the by-persona registry — the single home reachable by
    // BOTH this loop and the self-set tool she invokes (the brain holds no global
    // handle), so her steering and this floor read one state. Locked only across the
    // synchronous read (no await held), per the concurrency style guide.
    let focus_handle = crate::cognition::focus::registry().handle(ctx.identity.peer_id.as_uuid());
    let wakes = {
        let mut focus = focus_handle
            .lock()
            .expect("focus mutex poisoned by a prior panic");
        focus.prune_expired(now_ms); // drop lapsed snoozes while we hold it (bounded list)
        focus.wakes_on(ctx.identity.default_room, addressed, now_ms)
    };
    if !wakes {
        continuum_base::probe!(
            class = "persona.selftick.muted",
            persona = %ctx.identity.agent_name,
            room = %ctx.identity.default_room,
            addressed,
            "self-tick wake held by a self-set mute — back to sleep (interrupt floor honored)"
        );
        return;
    }
    // The self-initiated framing (formerly an `[Self-initiated moment…]` text
    // preamble concatenated onto the burst) now rides `TurnFraming::self_thread`
    // into the system prompt, so the conversation turns stay clean role-attributed
    // messages and the "this is your own time" framing is standing instruction, not
    // a fake conversation line. The addressing fact rides the SAME framing's
    // `directed` bit (withholds the silent-PASS hatch when she was named); no
    // hand-written "someone addressed you" sentence is needed.
    // ONE settlement step through the SAME shared primitive as the message path and
    // the eval driver (`act_observe::settle_step`, `may_act = true`): run ONCE, and
    // on `Act` admit the result as memory + let the next heartbeat re-perceive — no
    // synchronous loop, no narration of the intermediate step into the room. She
    // speaks only when she has something worth the others' attention
    // (ACTING-ORGANISM.md §4).
    // Self-initiated free time NEVER force-withholds the silence hatch. A self-tick
    // fires on ANY external change — INCLUDING another party's content-free courtesy
    // that happens to name her by id — and forcing a turn on that is EXACTLY what
    // generated the two-persona courtesy spiral: two AIs mutually naming each other →
    // `addressed` true every tick → hatch withheld → a forced content-free turn → the
    // other's self-tick perceives it → resonance, burning GPU (~40s/decode each) and
    // flooding the room with poisoned `assistant` precedent (proven live 2026-07-02,
    // room cb2e21a1, personas 90e758b2 + 0d3209a1). On her OWN time the hatch is
    // inviolable: she may always yield when nothing is worth the others' attention
    // ([[idle-is-self-directed-free-time]], [[organic-substrate-continuous-concern-scheduler]]).
    //
    // `addressed` stays a PERCEIVED fact — the `persona.selftick.perceive` probe above
    // surfaces it for observability, and it still floors the wake decision (`wakes_on`,
    // so a hard mute can't swallow a real alarm). The message content itself is in the
    // burst turns, so the mind still SEES that it was named and can choose to answer; it
    // is simply never COMPELLED to on the ambient tick. Anti-ghosting of a genuine direct
    // question is the REACTIVE message path's job (service_loop ~675,
    // `TurnFraming::message(attention)`), where a real inbound question arrives as an
    // addressed event — not this ambient digest tick. Framing over a structural fact,
    // never a gate on cognition ([[no-hardcoded-heuristics-to-steer-cognition]]): the
    // previous `self_thread(addressed)` was a dumb function steering the mind away from
    // silence; removing the force lets per-slice judgment decide, which is the whole
    // organic-substrate thesis.
    // Self-ticks drive to settlement like every other live turn (2026-07-11,
    // Joel: "gating acts is not autonomy… if they want to edit a file let them
    // edit a god damn file"). The old one-step motion executed her first act and
    // then yanked the turn — glass-boxed the same day: Casper's read_file on his
    // own claimed card executed cleanly and the chain died next tick under chat
    // pressure. She keeps her hands until SHE settles (Speak/Pass); no act cap.
    // #169 STREAMING on the SELF-TICK path too (mirrors the message path ~882):
    // hand this turn a token sink so the deliberation faculty forwards each decoded
    // chunk as it generates. Slice 1 stamps time-to-first-token; slice 2 forwards
    // Token chunks to the room/TTS/avatar. Cleared after the turn (byte-identical
    // when unused).
    let (tok_tx, tok_rx) =
        tokio::sync::mpsc::unbounded_channel::<crate::ai::adapter::GenerationChunk>();
    cycle.set_token_sink(Some(tok_tx));
    // #169/#170: self-tick (autonomic) turns do NOT broadcast a live typing stream —
    // a room doesn't need every persona's idle musing streamed token-by-token (that
    // was most of the flood that killed subscribers). `None` citizen → first_token
    // probe still fires (observability), but no chunks publish. The durable utterance
    // still `say`s once. Only message-driven turns (real conversation) stream live.
    // Idle self-tick: no citizen AND no rail tee (room/sender None) — an idle mind
    // musing isn't addressing anyone, so nothing streams to the browser (#170).
    // #350: don't spend a whole deliberation on a turn the serving guard is CERTAIN to
    // refuse. When no lane is live, the single-resident guard rejects every generation —
    // so the tick can only end in a loud `inference_failed` naming a serving fault that
    // the persona did not cause and cannot fix.
    //
    // Gate on the LIVE snapshot, not on "has the daemon ever reconciled". The first cut of
    // this gate used a lifetime latch and caught only the first-ever warmup: measured
    // 2026-08-07, three citizens still failed together at +432s of a boot whose latch had
    // been set at +76s. The daemon deliberately republishes `ServingSnapshot::empty()`
    // whenever it tears a lane down — no plan, a re-home, or a #175 wedge self-heal
    // (`declare_lane_wedged`) — so `serving: <none>` is a RECURRING transition, not a
    // boot-only state. That burst sat inside a 59s window opened by a decode-heartbeat
    // wedge at +382s and closed by the reconcile at +441s. Boot is merely the longest
    // such window, which is what made a boot-shaped fix look sufficient.
    //
    // This is the same live signal the presence pump already derives per tick
    // (`persona/airc_runtime.rs` → `Ready`/`Away`); a mind that presence has just marked
    // Away must not simultaneously be spending a deliberation into a guaranteed refusal.
    //
    // Skipping is honest rather than a swallowed error: nothing has failed yet, the mind
    // simply has no brain attached for a few more seconds, and it could not have thought
    // either way. Deliberately NOT re-derived here: how long the lane has been down and
    // whether that is now an outage. The serving daemon already owns that judgement and
    // emits it loudly (`serving.health` action=relaunch); a second severity clock in the
    // persona hot path would be a parallel monitor of a fact its owner already publishes
    // (CONCURRENCY-STYLE-GUIDE). `has_reconciled()` still earns its keep in the probe: it
    // separates a cold boot from a lane that HAS served and dropped, which the empty
    // snapshot itself cannot say (`ready_verified_at_ms` is erased by `empty()`).
    if !crate::inference::llama_server::current_serving().is_live() {
        continuum_base::probe!(
            class = "persona.selftick.awaiting_serving",
            persona = %ctx.identity.agent_name,
            served_before = crate::inference::llama_server::has_reconciled(),
            "no lane is live — skipping this self-tick rather than deliberating into a \
             guaranteed refusal (serving transition, not a persona fault)"
        );
        return;
    }
    let forwarder =
        spawn_token_forwarder(tok_rx, None, ctx.identity.agent_name.clone(), None, None);
    let (step, _turn_metrics, generation_receipts) = {
        let outcome = crate::cognition::act_observe::drive_to_settle_with_input(
            &cycle,
            burst,
            LIVE_MAX_ACTS,
            crate::cognition::workspace::TurnFraming::self_thread(false),
            conversation,
        )
        .await;
        crate::cognition::act_observe::SettleStep::from_settled(outcome)
    };
    cycle.set_token_sink(None);
    let _ = forwarder.await;
    pending.settle_self(tick_room, &step);
    pending.notices_read(tick_room, &generation_receipts);
    match step {
        crate::cognition::act_observe::SettleStep::Spoke(text) => {
            // Never broadcast a raw tool-call envelope to the room (same guard the
            // message path has). A decision whose text is just {"tool_call":…} is an
            // un-acted call, not a contribution — stay silent.
            if crate::ai::json_in_prompt_tools::parse_tool_call(&text).is_some() {
                return;
            }
            // A self-cycle answers no one — the audience is the room the tick
            // ran IN (her claim's focus room when working, else her default) —
            // the same room the cycle framed its context against.
            if let Err(e) =
                publish_own_speech(conversation, ctx.identity.peer_id, tick_room, &text).await
            {
                tracing::warn!(persona = %ctx.identity.agent_name, error = %e, "self-cycle say failed");
                return;
            }
            continuum_base::probe!(
                class = "persona.selftick.spoke",
                persona = %ctx.identity.agent_name,
                response_len = text.len(),
                "never-stop heartbeat — persona pursued its own thread (no inbound message)"
            );
        }
        crate::cognition::act_observe::SettleStep::Acted { calls, intent } => {
            continuum_base::probe!(
                class = "persona.selftick.acted",
                persona = %ctx.identity.agent_name,
                tools = calls.len(),
                intent = %intent,
                "self-thread act; result admitted as memory, re-perceives next tick"
            );
        }
        crate::cognition::act_observe::SettleStep::InferenceFailed { error } => {
            // Even on the self-thread a failed model call is surfaced, never swallowed
            // by the `_ => {}` sleep ([[fallbacks-are-illegal-fail-loud]]): a broken
            // serving lane is a real fault the operator must see, not idle quiet.
            tracing::warn!(
                persona = %ctx.identity.agent_name,
                error = %error,
                "self-cycle deliberation inference FAILED — sleeping this tick (not a chosen silence)"
            );
            continuum_base::probe!(
                class = "persona.selftick.inference_failed",
                persona = %ctx.identity.agent_name,
                error = %error,
                "self-thread model call failed; sleeping, retries next tick"
            );
        }
        // ActUnfulfilled (no hands / exec error) → nothing to say, sleep. WouldAct is
        // unreachable (live always permits its one act). Passed → sleep.
        _ => {}
    }
}

/// Helper: pull the next event from the conversation, handling the
/// transient-error case (lag, transport hiccup) by logging + counting
/// + continuing. Returns `None` only when the stream is genuinely
/// over.
pub(super) async fn next_event(
    conversation: &mut dyn CitizenConversation,
    outcome: &mut ServeOutcome,
) -> Option<IncomingMessage> {
    loop {
        match conversation.next_message().await {
            Ok(Some(msg)) => return Some(msg),
            Ok(None) => return None,
            Err(e) => {
                tracing::warn!(error = %e, "serve_persona_loop: next_message transient error");
                outcome.turns_errored += 1;
                continue;
            }
        }
    }
}
