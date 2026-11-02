use super::*;

/// Mark the turn as started on the probe stream and take in the room up to this line.
pub(super) fn announce_turn_start(ctx: &HostedPersona, trigger: TurnTrigger<'_>, opts: &ServeOptions) {
    let TurnTrigger { msg, turn_room, admitted_retry } = trigger;
    continuum_base::probe!(
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
}

/// Admit the trigger into the persona's hippocampus (the L2 store growing), timing the
/// recall and admission passes. Returns the wake engram: the admitted engram of this
/// message, else the one the retained input already carries.
pub(super) async fn admit_trigger(
    ctx: &HostedPersona,
    trigger: TurnTrigger<'_>,
    roster_names: &std::collections::HashMap<Uuid, String>,
    now_ms: u64,
    phase_timings: &mut PhaseTimings,
    mut wake_engram: Option<Uuid>,
) -> Option<Uuid> {
    let TurnTrigger { msg, turn_room, admitted_retry } = trigger;
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
    let inbox_msg = trigger_inbox_message(msg, turn_room, roster_names, now_ms);
    // Rank engrams for the L2→prompt introspection seam, then admit this turn's
    // incoming message as an engram. The recall that actually REACHES the prompt
    // is done inside the WorkspaceCycle's RecallFaculty — this read stays for the
    // Hebbian salience side-effect (Algorithm 4 #165) and the per-engram
    // introspection log ([[observability-is-half-the-architecture]] + Joel's
    // 2026-06-03 "introspect all rag" directive). The ranked Vec is no longer
    // threaded into a per-turn RespondInput (that path is gone); only the side-
    // effects and the admit remain.
    if !admitted_retry {
        let cognition = &ctx.cognition;
        // recall BEFORE admit so the ranking is "what I knew going in" — the
        // current message is the trigger, not recall.
        // Per #195 slice 1: time the L2 retrieval pass.
        let recall_started = std::time::Instant::now();
        let scored = cognition.admission.recall_scored(now_ms, 8);
        phase_timings.recall_ms = recall_started.elapsed().as_millis() as u64;
        for (rank, (engram, salience)) in scored.iter().enumerate() {
            tracing::info!(
                lamport = msg.lamport,
                rank,
                engram_id = %engram.id,
                salience = format!("{:.3}", salience),
                content_chars = engram.content.chars().count(),
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
    wake_engram
}

/// Compose the turn (budget + multi-source RAG composition), then harvest the roster resolution
/// it fetched into the admit-time name memory.
pub(super) async fn compose_turn(
    ctx: &HostedPersona,
    turn_room: Uuid,
    now_ms: u64,
    phase_timings: &mut PhaseTimings,
    roster_names: &mut std::collections::HashMap<Uuid, String>,
) -> crate::persona::unified::ComposedTurn {
    // 2. compose_for_turn: the budget + multi-source RAG composition via the
    //    FlexboxRagBudgetAdapter. The brain is shared, never locked: it is wired once at
    //    materialize, so composition, recall and admission run concurrently.
    // Per #195 slice 1: time the RAG flexbox composer.
    let compose_started = std::time::Instant::now();
    let composed = {
        let cognition = &ctx.cognition;
        // Stamp the WHERE axis: this turn is happening INSIDE `turn_room`.
        // Without it every room-scoped source abstains and she perceives no
        // board, no roster, no doctrine, no wall (#331 / #127).
        cognition
            .compose_for_turn(now_ms, Some(turn_room))
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
    composed
}

/// The consolidated burst the WorkspaceCycle reasons over, built where the item metadata
/// is still intact and carrying the wake message's engram as its cause.
pub(super) fn build_turn_burst(
    ctx: &HostedPersona,
    composed: &crate::persona::unified::ComposedTurn,
    trigger: TurnTrigger<'_>,
    now_ms: u64,
    wake_engram: Option<Uuid>,
) -> crate::cognition::workspace::Burst {
    let TurnTrigger { msg, turn_room, .. } = trigger;
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
            source: Some(crate::cognition::workspace::TurnSource { room: msg.room_id, event: msg.event_id, lamport: msg.lamport }),
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
    crate::cognition::workspace::Burst::from_turns_at(
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
    )
}
