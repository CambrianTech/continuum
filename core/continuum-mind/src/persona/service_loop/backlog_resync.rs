use super::*;

/// What the resync-to-now drain decided for this wake.
pub(super) enum Resync {
    /// Turn once on this message (the newest addressed, else the newest overall).
    Trigger(IncomingMessage),
    Continue,
    Break,
}

/// The one turn trigger shared by the intake steps: the message, the room it arrived in,
/// and whether it is an already-admitted retry.
#[derive(Clone, Copy)]
pub(super) struct TurnTrigger<'a> {
    pub(super) msg: &'a IncomingMessage,
    pub(super) turn_room: Uuid,
    pub(super) admitted_retry: bool,
}

/// The inbox item an inbound message becomes when admitted to memory: attributed to the
/// roster name when known, else the short peer tag.
pub(super) fn trigger_inbox_message(
    msg: &IncomingMessage,
    turn_room: Uuid,
    roster_names: &std::collections::HashMap<Uuid, String>,
    now_ms: u64,
) -> crate::cognition::conversation::types::InboxMessage {
    crate::cognition::conversation::types::InboxMessage {
        id: Uuid::new_v4(),
        room_id: turn_room,
        sender_id: msg.peer_id,
        sender_name: roster_names
            .get(&msg.peer_id)
            .cloned()
            .unwrap_or_else(|| crate::persona::roster_slot::provisional_sender_name(msg.peer_id)),
        sender_type: crate::cognition::conversation::types::SenderType::Persona,
        content: msg.render_content(),
        timestamp: now_ms,
        priority: 0.5,
        source_modality: None,
        voice_session_id: None,
    }
}

/// Resync-to-now: drain everything already queued behind the wake message and turn ONCE
/// on the newest addressed one. An already-admitted retry passes straight through.
pub(super) async fn resync_to_now(
    ctx: &HostedPersona,
    conversation: &mut dyn CitizenConversation,
    outcome: &mut ServeOutcome,
    seen_ids: &mut crate::cognition::conversation::seen_ids::SeenIds,
    high_water: &mut u64,
    pending: &mut crate::persona::wake_backlog::PendingInput,
    admitted_retry: bool,
    msg: IncomingMessage,
) -> Resync {
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
                next_event(&mut *conversation, &mut *outcome),
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
            let stale = crate::cognition::conversation::seen_ids::is_stale(&m, &mut *seen_ids, *high_water);
            *high_water = m.lamport.max(*high_water);
            if stale || m.peer_id == self_id {
                outcome.turns_skipped += 1;
            } else {
                pending.observe_work(&m);
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
                crate::cognition::room_directory::is_human_peer(m.peer_id),
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
            continuum_base::probe!(
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
                        return Resync::Break;
                    }
                    return Resync::Continue; // everything drained was stale/self — no turn
                }
            };
        (msg, coalesced)
    };
    if coalesced > 0 {
        outcome.turns_skipped += coalesced;
        continuum_base::probe!(
            class = "persona.wake.coalesced",
            persona_id = %self_id,
            coalesced = coalesced,
            trigger_lamport = msg.lamport,
            "resync-to-now: coalesced the queued backlog into ONE turn on the newest message"
        );
    }
    Resync::Trigger(msg)
}
