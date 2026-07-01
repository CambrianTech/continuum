//! Per-persona service loop — the wire driver between airc and the
//! brain.
//!
//! Takes a [`HostedPersona`] (`PersonaContext`) and a "talk to the
//! grid" abstraction ([`PersonaConversation`]) and drives one
//! cognition turn per incoming message through THE BRAIN — the
//! canonical agent pipeline:
//!
//!   subscribe → for each event:
//!     • skip pre-watermark / self / non-text
//!     • lease the brain (`ctx.cognition.lock().await`)
//!     • `compose_for_turn` — engram + airc through the
//!       FlexboxRagBudgetAdapter
//!     • project into `RespondInput` (canonical agent contract: media,
//!       capabilities, tools, recalled engrams, room context)
//!     • `persona::response::respond(input)` — the substrate's per-
//!       persona cognition cycle (shared analyze + score + genome
//!       activate + evaluate_response + clean_and_validate +
//!       tool_executor + audit + record_turn)
//!     • post the resulting text via `conversation.say` (or honor
//!       `PersonaResponse::Silent { reason, .. }`)
//!
//! **See `docs/architecture/PERSONA-COGNITION-PIPELINE.md` for the
//! full per-persona cycle.** The bypass that previously called
//! `inspect_persona_rag_with_inference` (a `will_respond + response`
//! chatbot wrapper around the inspection function) was removed in
//! slice 1C of task #160. `rag_inspect.rs::inspect_persona_rag` stays
//! as the mechanic's-view introspection function it was named for.
//!
//! ## Doctrine
//!
//! - [[no-if-statements-use-llms-for-cognition]]: the loop does the
//!   minimum substrate filtering — pre-watermark / self / non-text —
//!   and hands the rest to `respond()`. No heuristic "should I respond"
//!   gate; the brain's cycle (with `full_evaluate` + `score_persona`
//!   + the LLM's own decision via `evaluate_response`) owns that.
//! - [[no-fallbacks-ever]]: per-message errors are logged + counted on
//!   the outcome, not swallowed; the loop continues with the next
//!   message rather than substituting a default response.
//! - [[no-stdio-piping-for-process-ipc]]: the loop talks to airc only
//!   through the [`PersonaConversation`] trait. The trait is the
//!   substrate's IPC boundary; tests stub it without any daemon.

use crate::ai::adapter::AIProviderAdapter;
use crate::persona::airc_source::AircTranscriptReader;
use crate::persona::supervisor::HostedPersona;
use async_trait::async_trait;
use std::sync::Arc;
use uuid::Uuid;

/// A substrate-friendly slice of one airc event: just what the
/// service loop needs to decide whether to respond. Strips away the
/// full `TranscriptEvent` surface so the conversation abstraction
/// stays compact and the trait remains stubbable without dragging
/// every airc type into the test.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncomingMessage {
    /// Monotonic lamport clock — used for pre-attach high-water-mark
    /// filtering.
    pub lamport: u64,
    /// Cryptographic source identity per
    /// [[persona-identity-derives-from-source-id]]. The loop compares
    /// against the hosting persona's own peer_id to skip self-loop
    /// echoes.
    pub peer_id: Uuid,
    /// The message text. The loop only forwards textual messages;
    /// non-text events (binary attachments, control envelopes) are
    /// filtered upstream of this projection — they should arrive as
    /// `None` from the conversation's stream.
    pub text: String,
}

/// Polymorphism rail for "talk to the grid as this persona". The
/// substrate's loop never touches `airc_lib::Airc` directly; the
/// real surface is behind this trait. Slice 11 ships the
/// `AircPersonaConversation` impl.
///
/// All methods are async because the production impl chains over
/// airc's IPC socket. Tests use a stub that's instant.
#[async_trait]
pub trait PersonaConversation: Send + Sync {
    /// Open whatever connection / stream / state the conversation
    /// needs to start yielding messages. Called once at boot, BEFORE
    /// the first `high_water_mark` or `next_message`.
    ///
    /// Production (`AircPersonaConversation`): opens the airc
    /// subscribe stream. Without `prime`, the stream was opened
    /// lazily on first `next_message`, which paid the daemon
    /// round-trip on the cognition hot path. With `prime`, that
    /// round-trip lands at the supervisor's boot phase — the
    /// persona is "ready to converse" the moment her service loop
    /// starts iterating.
    ///
    /// Tests (`StubConversation`): no-op. Idempotent — calling
    /// `prime` twice is safe but the second call is a no-op.
    ///
    /// Returns `Err` if priming fails (daemon unreachable, room
    /// gone). Per [[no-fallbacks-ever]] the loop refuses to start
    /// rather than entering a degraded path.
    async fn prime(&mut self) -> Result<(), String>;

    /// Highest lamport observed in transcript history before live
    /// subscription. Used to ignore messages that arrived BEFORE the
    /// persona attached — avoids replying to ancient chat just
    /// because a restart loaded them through `page_recent`.
    async fn high_water_mark(&self, limit: usize) -> Result<u64, String>;

    /// Yield the next inbound message, or `Ok(None)` when the
    /// stream is exhausted (daemon disconnected, peer gone). On
    /// transient errors (stream lag, transport hiccup) the impl
    /// should yield `Err` so the loop can record + continue.
    ///
    /// Assumes `prime` was called once at boot. If it wasn't, the
    /// impl MAY lazy-prime (production currently does, for
    /// backward compat) but the substrate's preferred path is
    /// eager priming so the round-trip lands off the hot path.
    async fn next_message(&mut self) -> Result<Option<IncomingMessage>, String>;

    /// Reply with text to the persona's default room.
    async fn say(&self, text: &str) -> Result<(), String>;
}

/// Behavioral knobs for the service loop. Keep small — substrate-
/// resolved defaults handle the common case so callers don't need to
/// thread state through.
#[derive(Debug, Clone)]
pub struct ServeOptions {
    /// How many transcript events to consult when computing the
    /// pre-attach high-water mark. Matches the demo binary's
    /// `PAGE_RECENT_LIMIT` (currently 50).
    pub page_recent_limit: usize,
    /// RAG fetch limit threaded into the inspection request. Today
    /// matches `page_recent_limit`; future slices may tune
    /// independently as the RAG layer grows multiple sources.
    pub rag_fetch_limit: usize,
    /// "Now" supplied as a function so the loop stays pure-of-clock
    /// for testability — same as `inspect_persona_rag` already does.
    pub now_ms: fn() -> u64,
}

impl Default for ServeOptions {
    fn default() -> Self {
        Self {
            page_recent_limit: 50,
            rag_fetch_limit: 50,
            now_ms: || {
                use std::time::{SystemTime, UNIX_EPOCH};
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0)
            },
        }
    }
}

/// Cheap online latency aggregator. Counts samples, tracks total
/// (for mean), min, and max — all in milliseconds. Bounded memory:
/// four fields, no Vec growth even over hour-long persona sessions.
///
/// Per Joel's "computer engineer" mental model from
/// [[init-once-handle-then-lease-zero-copy-refs]]: hot-path metric
/// recording should be branch-predictable, cache-friendly, and
/// allocation-free. `record` is a few atomic-style updates against
/// stack-local fields; no heap, no pointer chasing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LatencyAggregate {
    /// Number of samples recorded.
    pub count: usize,
    /// Sum of all sample durations in milliseconds. `mean_ms()`
    /// divides by `count`.
    pub total_ms: u64,
    /// Fastest sample observed. `None` until first `record`.
    pub min_ms: Option<u64>,
    /// Slowest sample observed. `None` until first `record`.
    pub max_ms: Option<u64>,
}

impl LatencyAggregate {
    /// Record one sample. O(1), allocation-free.
    pub fn record(&mut self, ms: u64) {
        self.count += 1;
        self.total_ms = self.total_ms.saturating_add(ms);
        self.min_ms = Some(self.min_ms.map_or(ms, |m| m.min(ms)));
        self.max_ms = Some(self.max_ms.map_or(ms, |m| m.max(ms)));
    }

    /// Arithmetic mean in milliseconds. `None` when `count == 0`.
    pub fn mean_ms(&self) -> Option<f64> {
        if self.count == 0 {
            None
        } else {
            Some(self.total_ms as f64 / self.count as f64)
        }
    }
}

/// Per-turn scratchpad for the phase decomposition (task #195
/// slice 1). Each successful reply path fills in all five fields
/// and the loop's tail records them into the matching
/// ServeOutcome aggregates. Continue-on-error arms abandon the
/// scratchpad implicitly — only fully-successful replies sample
/// the phase aggregates, matching `turn_latency`'s sample set.
///
/// `Default` zeros every field — the loop tail can record
/// unconditionally without checking which phases ran (only
/// successful replies reach the tail at all).
#[derive(Debug, Default, Clone, Copy)]
struct PhaseTimings {
    recall_ms: u64,
    admit_ms: u64,
    compose_ms: u64,
    respond_ms: u64,
    say_ms: u64,
}

/// Aggregate stats from one `serve_persona_loop` run. Returned when
/// the conversation stream ends; useful for operators + tests
/// asserting on what happened without scraping log lines.
///
/// Per Joel 2026-06-02 ("make sure timing and other metrics are in
/// place"): the substrate doesn't claim "fast airc-bound persona"
/// without measuring it. `turn_latency` is the structural record of
/// what the cognition hot path actually costs end-to-end.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ServeOutcome {
    /// Messages where the persona produced + posted a reply.
    pub turns_replied: usize,
    /// Pre-watermark / self / non-text / RAG-only messages where the
    /// loop intentionally produced no reply.
    pub turns_skipped: usize,
    /// Messages where the loop ran but RAG / inference / say failed.
    /// Per [[no-fallbacks-ever]] the loop continues; the count is the
    /// substrate's honest record of what didn't work.
    pub turns_errored: usize,
    /// Turns where the persona reached for its hands: the mind emitted a
    /// `Decision::Act`, the act→observe driver ran it ONCE, and the result was
    /// admitted as an Episodic engram (ACTING-ORGANISM.md §3.3). The turn produced
    /// no room utterance — the metronome re-perceives with the result in memory and
    /// she settles into a Speak on a later tick. Distinct from skipped (chose
    /// silence) and replied (spoke): this is the organism using its hands.
    pub turns_acted: usize,
    /// Per-replied-turn end-to-end latency: from the moment
    /// `next_message` yielded `Ok(Some(msg))` to the moment
    /// `conversation.say` returned `Ok(())`. Excludes wait time for
    /// the next event (which depends on user-typing speed). Captures
    /// the substrate's per-turn cost — RAG inspect + LLM generate +
    /// airc publish — which is what the [[init-once-handle-then-lease-zero-copy-refs]]
    /// pattern is trying to minimize.
    ///
    /// Only successful replies are recorded. Errored and skipped turns
    /// have their own counters; conflating their wall-clock with the
    /// reply path's wall-clock would muddy the metric.
    pub turn_latency: LatencyAggregate,

    // ─── Per-phase decomposition (task #195 slice 1) ──────────────
    //
    // `turn_latency` above is end-to-end; these break it down so
    // optimization is data-driven instead of guess-driven. Each
    // aggregate samples ONLY successful replies — same gate as
    // `turn_latency` — so the decomposition stays internally
    // consistent.
    //
    // Sum-of-phases ≈ turn_latency. Any residual is `RespondInput`
    // projection + bookkeeping overhead; if that grows material,
    // it gets its own aggregate in a follow-up slice. Per
    // `[[observability-is-half-the-architecture]]` + Joel's
    // 2026-06-05 "perfecting real inference latency" directive:
    // we measure before we optimize.
    /// Time spent in `cognition.admission.recall_scored` (the L2
    /// retrieval pass). Holds the cognition mutex.
    pub recall_latency: LatencyAggregate,
    /// Time spent in `cognition.admission.admit` (engram formation
    /// for the inbox message). Holds the cognition mutex.
    pub admit_latency: LatencyAggregate,
    /// Time spent in `cognition.compose_for_turn` (the RAG flexbox
    /// composer + multi-source delivery). Holds the cognition mutex.
    pub compose_latency: LatencyAggregate,
    /// Time spent in `persona::response::respond` (the cognition
    /// cycle including the LLM call). Does NOT hold the cognition
    /// mutex — the substrate stays responsive while the model
    /// decodes. This is typically the dominant cost and the
    /// primary target of subsequent optimization slices.
    pub respond_latency: LatencyAggregate,
    /// Time spent in `conversation.say` (airc publish + downstream
    /// ack). Usually small unless the airc transport is degraded.
    pub say_latency: LatencyAggregate,
}

/// Run the per-persona service loop until the conversation stream
/// ends.
///
/// **PRECONDITION**: `conversation.prime()` MUST be called by the
/// caller before invoking this function. The supervisor's
/// `spawn_persona_service` enforces this. Direct callers (e.g.
/// integration tests) must call `prime` explicitly. Per
/// [[no-fallbacks-ever]] this loop does NOT prime as a safety net —
/// one place primes, callers honor the contract.
/// If you forget to prime, the first `next_message` returns a typed
/// `Err("called before prime()")` so the failure is loud.
///
/// Returns the aggregate `ServeOutcome` summarizing what the loop
/// did. Stream-level transient errors (yielded as `Err` from
/// `next_message`) increment `turns_errored` and the loop continues;
/// `Ok(None)` from `next_message` ends the loop cleanly. Pre-attach
/// transcript is consulted once for the high-water mark and is NOT
/// replayed through RAG — that would echo every pre-restart message.
pub async fn serve_persona_loop(
    ctx: &HostedPersona,
    conversation: &mut dyn PersonaConversation,
    reader: Arc<dyn AircTranscriptReader>,
    opts: ServeOptions,
) -> Result<ServeOutcome, String> {
    use tracing::Instrument;
    serve_persona_loop_inner(ctx, conversation, reader, opts)
        .instrument(ctx.span())
        .await
}

async fn serve_persona_loop_inner(
    ctx: &HostedPersona,
    conversation: &mut dyn PersonaConversation,
    reader: Arc<dyn AircTranscriptReader>,
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
        .high_water_mark(opts.page_recent_limit)
        .await
        .map_err(|e| format!("high_water_mark failed: {e}"))?;

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
    let mut heartbeat = tokio::time::interval(std::time::Duration::from_millis(SELF_TICK_MS));
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    // Burst fingerprint of the last cycle (message OR self-tick), so a heartbeat
    // over an unchanged world is free. Shared by both paths: a message turn updates
    // it, so the very next tick doesn't re-deliberate the same burst.
    let mut last_burst_fp: u64 = 0;

    loop {
        let wake = tokio::select! {
            ev = next_event(conversation, &mut outcome) => match ev {
                Some(m) => Wake::Msg(m),
                None => Wake::Stop,
            },
            _ = heartbeat.tick() => Wake::Tick,
        };
        let msg = match wake {
            Wake::Stop => break,
            Wake::Tick => {
                // Heartbeat slice — the mind gets time with no inbound message.
                run_self_cycle(ctx, conversation, &opts, &mut last_burst_fp).await;
                continue;
            }
            Wake::Msg(m) => m,
        };
        if msg.lamport <= high_water {
            outcome.turns_skipped += 1;
            continue;
        }
        high_water = msg.lamport.max(high_water);

        if msg.peer_id == ctx.identity.peer_id.as_uuid() {
            outcome.turns_skipped += 1;
            continue;
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
        crate::probe!(
            class = "persona.turn.start",
            persona = %ctx.identity.agent_name,
            persona_id = %ctx.identity.peer_id.as_uuid(),
            room_id = %ctx.identity.default_room,
            lamport = msg.lamport,
            peer_id = %msg.peer_id,
            text_len = msg.text.len(),
            "turn started"
        );

        // ===========================================================
        // The brain services the turn through the canonical cognition
        // pipeline — `persona::response::respond(RespondInput)`. This
        // is the agent contract Joel and I have been building for a
        // year: shared analysis (single-flight cache) → specialty
        // scoring → genome activate → evaluate_response (adapter-
        // translated, model-canonical tool calls + multi-modal) →
        // clean_and_validate → tool_executor → audit → record_turn.
        //
        // See docs/architecture/PERSONA-COGNITION-PIPELINE.md for the
        // full pipeline and the bypass this commit replaces.
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
            room_id: ctx.identity.default_room,
            sender_id: msg.peer_id,
            sender_name: format!("peer-{}", &msg.peer_id.to_string()[..8]),
            sender_type: crate::persona::types::SenderType::Persona,
            content: msg.text.clone(),
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
        {
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
            if let Err(e) = admit_result {
                tracing::warn!(
                    lamport = msg.lamport,
                    error = %e,
                    "admission.admit failed — engram not formed this turn"
                );
            } else {
                tracing::info!(
                    lamport = msg.lamport,
                    engram_count = cognition.admission.engram_count(),
                    "admitted incoming → L2 store"
                );
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
            cognition.compose_for_turn(&ctx.profile, now_ms).await
        };
        phase_timings.compose_ms = compose_started.elapsed().as_millis() as u64;

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
        let workspace_burst = crate::cognition::workspace::Burst::from_turns(
            ctx.identity.default_room,
            build_workspace_turns(
                &composed.deliveries,
                &ctx.identity.peer_id.to_string(),
                &ctx.identity.agent_name,
            ),
        );
        // Mark this world-state as just-deliberated so the next heartbeat tick doesn't
        // re-run the same burst (the message path and the self-tick share the gate;
        // own chat is excluded so this reply can't re-trigger a self-tick, while her
        // own active work is folded in so the tick advances it — see burst_fingerprint).
        last_burst_fp = burst_fingerprint(&composed.deliveries, &ctx.identity.peer_id.to_string());
        // Project the room-roster delivery into TWO consumers from the
        // ONE source of truth (the roster delivery), routed by source_id:
        //   • `room_roster` — the formatted `name [runtime] — avail`
        //     lines → system-prompt GROUNDING ([Present in this room]).
        //     Deliberately NOT recent_history: the roster names who is
        //     present (identity grounding), it is not conversation —
        //     injecting it as history is the confusion the source removes.
        //   • `other_persona_names` — bare display names → the
        //     `ProperChatMlSingleParty` history-drop (single-party models
        //     can't process other-AI turns). This field was reserved for
        //     exactly this roster data and previously sat empty; the same
        //     delivery now feeds it, so there is one roster truth, not two.
        // See docs/grid/AIRC-NATIVE-IDENTITY-ROOMS-SECURITY.md §5 slice 1.
        let mut room_roster: Vec<String> = Vec::new();
        let mut other_persona_names: Vec<String> = Vec::new();
        for item in composed
            .deliveries
            .iter()
            .filter(|d| d.source_id == "room-roster")
            .flat_map(|d| d.items.iter())
        {
            room_roster.push(item.content.clone());
            if let Some(name) = item.metadata.get("display_name").and_then(|v| v.as_str()) {
                other_persona_names.push(name.to_string());
            }
        }

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
        let response_text = match crate::cognition::persona_workspace::global()
            .get(&ctx.identity.peer_id.as_uuid())
        {
            Some(cycle) => {
                // Run the mind over the metadata-rich burst built above
                // (WHO/WHEN/WHAT per inbox item, own posts attributed, room as the
                // WHERE). Recall (the persona's own memories) is injected by the
                // RecallFaculty inside the cycle — so "given this RAG, do I have
                // what I need?" is: this burst + recalled memory.
                // Scope the cognition tick to the room this turn is FOR (the
                // contextId), so the deliberation faculty stamps tool calls with
                // it and the persona's hands act in the real room, not a phantom
                // nil one. The serviced room is `ctx.identity.default_room` — the
                // same room the burst header above declares. (IncomingMessage
                // carries no per-message room yet; a per-message contextId on the
                // cognition input is the deeper fix — A.6 / the missing context
                // axis. Today default_room is the correct available context.)
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
                // `directed`. So `directed` is reserved for its one job — withholding
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
                let directed = ctx.identity.persona_identity().mentions(&msg.text);
                let (step, turn_metrics) = crate::cognition::act_observe::settle_step(
                    &cycle,
                    workspace_burst,
                    ctx.identity.default_room,
                    true,
                    crate::cognition::workspace::TurnFraming::message(directed),
                )
                .await;
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
                match step {
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
                    crate::cognition::act_observe::SettleStep::ActUnfulfilled {
                        calls,
                        intent,
                    } => {
                        // No hands or the executor errored. Abstain — never a
                        // fabricated result, never a raw call envelope to the room.
                        tracing::warn!(
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
                    crate::cognition::act_observe::SettleStep::Passed => {
                        tracing::info!(
                            lamport = msg.lamport,
                            "persona chose silence (workspace) — substrate honors decision"
                        );
                        crate::probe!(
                            class = "persona.turn.silent",
                            persona = %ctx.identity.agent_name,
                            lamport = msg.lamport,
                            reason = "workspace-pass",
                            "persona chose silence"
                        );
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
            tracing::info!(
                lamport = msg.lamport,
                "verdict was a raw tool-call envelope — not broadcasting"
            );
            outcome.turns_skipped += 1;
            continue;
        }

        // Per #195 slice 1: time the airc publish + downstream ack.
        let say_started = std::time::Instant::now();
        let say_result = conversation.say(&response_text).await;
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
        crate::persona::training_producer::produce(
            ctx.identity.peer_id.as_uuid(),
            ctx.identity.agent_name.clone(),
            ctx.profile.model_id.clone(),
            msg.text.clone(),
            response_text.clone(),
        );
        tracing::info!(
            lamport = msg.lamport,
            turn_duration_ms = turn_duration_ms,
            turns_replied = outcome.turns_replied,
            mean_ms = outcome.turn_latency.mean_ms().unwrap_or(0.0),
            min_ms = outcome.turn_latency.min_ms.unwrap_or(0),
            max_ms = outcome.turn_latency.max_ms.unwrap_or(0),
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

/// Never-stop heartbeat period. The deliberation concern gets a slice this often
/// even with NO inbound message, so a persona pursues its OWN open intentions
/// instead of going idle the instant it speaks. This is MECHANISM (the scheduler
/// handing out time), not a judgment — a learned, per-state cadence is a later
/// slice ([[organic-substrate-continuous-concern-scheduler]] kill-list). Kept
/// conservative so a full LLM deliberation can't fire faster than the world
/// meaningfully changes; the burst-fingerprint gate keeps idle ticks free.
const SELF_TICK_MS: u64 = 3_000;

/// What woke the service loop this cycle. A message from the wire, the never-stop
/// heartbeat, or the end of the stream. Returned by the `select!` so the borrow of
/// `conversation` taken inside the select ends before the handler reuses it.
enum Wake {
    Msg(IncomingMessage),
    Tick,
    Stop,
}

/// Build the consolidated workspace burst (WHO/WHEN/WHAT per inbox item, own posts
/// attributed, room as the WHERE) the `WorkspaceCycle` reasons over. A pure
/// projection of the airc deliveries — extracted so the message turn and the
/// never-stop self-tick share ONE burst truth ([[compression-principle]]).
/// Assemble the perception envelope a persona's deliberation sees: the room
/// header + each airc delivery item rendered as `[t=ms] who: content`, with the
/// persona's OWN past posts attributed to `agent_name` (so she recognizes her
/// own voice) and everyone else by peer_id. This is the ONE place the burst
/// string format lives — the live heartbeat (both the message path and the
/// self-thread) AND the eval fork call it, so a measured turn perceives its
/// world byte-identically to a lived one. Takes `&[RagDelivery]` (not the whole
/// `ComposedTurn`) because the format only depends on the deliveries; eval can
/// hand-build a single synthetic airc delivery for a task without composing a
/// full turn.
pub(crate) fn build_workspace_turns(
    deliveries: &[crate::persona::rag_budget::RagDelivery],
    own_peer: &str,
    agent_name: &str,
) -> Vec<crate::cognition::workspace::BurstTurn> {
    use crate::cognition::workspace::BurstTurn;
    use std::collections::HashMap;
    // airc owns identity: the `room-roster` source already joined peer_id → display
    // name in one batched scan ([[airc-native-identity-rooms-security]]). Reuse THAT
    // resolution — same deliveries slice — so the history reads `Joel:`, `BigMama:`
    // instead of raw UUIDs. A peer not in the roster (or an eval-synthesized delivery
    // with no roster) falls back to its own peer_id: honest, never invisible, never a
    // fabricated name ([[fallbacks-are-illegal-fail-loud]]).
    let names: HashMap<&str, &str> = deliveries
        .iter()
        .filter(|d| d.source_id == "room-roster")
        .flat_map(|d| d.items.iter())
        .filter_map(|item| {
            let peer = item.metadata.get("peer_id").and_then(|v| v.as_str())?;
            let name = item.metadata.get("display_name").and_then(|v| v.as_str())?;
            Some((peer, name))
        })
        .collect();
    deliveries
        .iter()
        .filter(|d| d.source_id == "airc")
        .flat_map(|d| d.items.iter())
        .map(|item| {
            let who_raw = item
                .metadata
                .get("peer_id")
                .and_then(|v| v.as_str())
                .unwrap_or("peer");
            // is_self — the ONE structural fact role attribution turns on (own posts
            // → assistant, peers → user). Carried here where the peer_id is intact,
            // never re-derived from a `Name:` prefix downstream.
            let is_self = who_raw == own_peer;
            let author = if is_self {
                agent_name
            } else {
                names.get(who_raw).copied().unwrap_or(who_raw)
            };
            let occurred_at_ms = item.metadata.get("occurred_at_ms").and_then(|v| v.as_u64());
            BurstTurn::attributed(is_self, author, item.content.clone(), occurred_at_ms)
        })
        .collect()
}

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
fn burst_fingerprint(
    deliveries: &[crate::persona::rag_budget::RagDelivery],
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
async fn run_self_cycle(
    ctx: &HostedPersona,
    conversation: &dyn PersonaConversation,
    opts: &ServeOptions,
    last_burst_fp: &mut u64,
) {
    let now_ms = (opts.now_ms)();
    let composed = {
        let cognition = ctx.cognition.lock().await;
        cognition.compose_for_turn(&ctx.profile, now_ms).await
    };
    // Wake on a CHANGE to what I should attend to — others' chat (own chat excluded
    // so my speech can't spiral into self-talk) OR my own active work (so the
    // heartbeat advances my thread, not just reacts to pokes). See burst_fingerprint.
    let fp = burst_fingerprint(&composed.deliveries, &ctx.identity.peer_id.to_string());
    if fp == *last_burst_fp {
        return; // nothing NEW to attend to (no external change, no work progress) → sleep
    }
    *last_burst_fp = fp;
    // Structured turns (own posts attributed as self → assistant, peers → user),
    // wrapped into a Burst carrying both the turns and their text projection — the
    // SAME shape the message path builds.
    let burst = crate::cognition::workspace::Burst::from_turns(
        ctx.identity.default_room,
        build_workspace_turns(
            &composed.deliveries,
            &ctx.identity.peer_id.to_string(),
            &ctx.identity.agent_name,
        ),
    );
    let Some(cycle) = crate::cognition::persona_workspace::global().get(&ctx.identity.peer_id.as_uuid())
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
    let addressed = composed
        .deliveries
        .iter()
        .filter(|d| d.source_id == "airc")
        .flat_map(|d| d.items.iter())
        .filter(|item| {
            // External authors only — her own posts naming herself aren't an address.
            item.metadata
                .get("peer_id")
                .and_then(|v| v.as_str())
                .map(|p| p != own_peer)
                .unwrap_or(true)
        })
        .any(|item| identity.mentions(&item.content));
    crate::probe!(
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
    let focus_handle = crate::persona::focus::registry().handle(ctx.identity.peer_id.as_uuid());
    let wakes = {
        let mut focus = focus_handle
            .lock()
            .expect("focus mutex poisoned by a prior panic");
        focus.prune_expired(now_ms); // drop lapsed snoozes while we hold it (bounded list)
        focus.wakes_on(ctx.identity.default_room, addressed, now_ms)
    };
    if !wakes {
        crate::probe!(
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
    // `directed = addressed`: a self-tick fires on ANY external change, INCLUDING a
    // message that named her — the `addressed` perception above captures exactly that.
    // When she was addressed, the turn IS directed: the bare-PASS escape is withheld so
    // a question put to her by name isn't ghosted. This is the SAME structural-addressing
    // fact the message path feeds to `directed` (service_loop ~658), now also honored on
    // the digest-perceived path (ordinary room chat reaches her here, not via the inbound
    // settle_step). When `addressed` is false the turn is genuinely ambient and silence
    // stays first-class. Framing over a structural fact, never an output filter
    // ([[no-hardcoded-heuristics-to-steer-cognition]]): she can still decline in her own
    // words, she just isn't handed the silent hatch when named.
    let (step, _turn_metrics) = crate::cognition::act_observe::settle_step(
        &cycle,
        burst,
        ctx.identity.default_room,
        true,
        crate::cognition::workspace::TurnFraming::self_thread(addressed),
    )
    .await;
    match step {
        crate::cognition::act_observe::SettleStep::Spoke(text) => {
            // Never broadcast a raw tool-call envelope to the room (same guard the
            // message path has). A decision whose text is just {"tool_call":…} is an
            // un-acted call, not a contribution — stay silent.
            if crate::ai::json_in_prompt_tools::parse_tool_call(&text).is_some() {
                return;
            }
            if let Err(e) = conversation.say(&text).await {
                tracing::warn!(persona = %ctx.identity.agent_name, error = %e, "self-cycle say failed");
                return;
            }
            crate::probe!(
                class = "persona.selftick.spoke",
                persona = %ctx.identity.agent_name,
                response_len = text.len(),
                "never-stop heartbeat — persona pursued its own thread (no inbound message)"
            );
        }
        crate::cognition::act_observe::SettleStep::Acted { calls, intent } => {
            crate::probe!(
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
            crate::probe!(
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
async fn next_event(
    conversation: &mut dyn PersonaConversation,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::HeuristicInferenceAdapter;
    use crate::modules::persona_instance_manager::PersonaInstanceInfo;
    use crate::persona::airc_citizen::StubAircCitizen;
    use crate::persona::airc_source::AircTranscriptReader;
    use crate::persona::identity_provider::PersonaIdentitySource;
    use crate::persona::role_template::RoleId;
    use crate::persona::scripted_conversation::ScriptedConversation;
    use crate::persona::supervisor::HostedPersona;
    use std::path::PathBuf;

    /// Drive-coupling guard for the never-stop heartbeat's wake gate.
    /// what this catches: `burst_fingerprint` must treat the persona's OWN active
    /// work as interior DRIVE — a new/changed work card moves the fingerprint so the
    /// tick fires to advance it — WITHOUT reopening the self-talk flood (own CHAT
    /// posts stay excluded; observed live 2026-06-22 as 19 self-talk cycles). This
    /// is the AUTONOMOUS-PROJECT-LOOP fix: the external-only gate was backwards for
    /// thought. [[alignment-through-mutual-self-interest]].
    mod burst_fingerprint_drive {
        use super::super::burst_fingerprint;
        use crate::persona::rag_budget::{RagDelivery, RagItem, ResolutionPreference};
        use serde_json::json;

        const ME: &str = "me-peer";

        fn delivery(source_id: &str, items: Vec<RagItem>) -> RagDelivery {
            RagDelivery {
                source_id: source_id.to_string(),
                items,
                tokens_used: 0,
                continuation: None,
                resolution_used: ResolutionPreference::Raw,
            }
        }
        fn chat(peer_id: &str, content: &str) -> RagItem {
            RagItem {
                content: content.to_string(),
                tokens: 0,
                metadata: json!({ "peer_id": peer_id }),
            }
        }
        fn card(content: &str) -> RagItem {
            RagItem {
                content: content.to_string(),
                tokens: 0,
                metadata: json!({}),
            }
        }

        #[test]
        fn own_chat_inert_but_own_work_wakes_the_heartbeat() {
            let base = vec![delivery("airc", vec![chat("other", "hi")])];
            let fp0 = burst_fingerprint(&base, ME);

            // My own chat post → excluded → fingerprint unchanged (no self-talk spiral).
            let with_my_chat = vec![delivery(
                "airc",
                vec![chat("other", "hi"), chat(ME, "I'll look into it")],
            )];
            assert_eq!(
                fp0,
                burst_fingerprint(&with_my_chat, ME),
                "own chat must not move the fingerprint (else speech re-triggers self-talk)"
            );

            // My active work card appears → fingerprint MUST change (interior drive).
            let with_my_work = vec![
                delivery("airc", vec![chat("other", "hi")]),
                delivery("active-work", vec![card("card abc [InProgress] \"impl X\"")]),
            ];
            assert_ne!(
                fp0,
                burst_fingerprint(&with_my_work, ME),
                "own active work must wake the heartbeat to advance it"
            );
        }

        #[test]
        fn work_progress_refires_the_next_slice() {
            // A card's state change is real progress → must re-fire so she continues.
            let in_progress = vec![delivery(
                "active-work",
                vec![card("card abc [InProgress] \"impl X\"")],
            )];
            let done = vec![delivery(
                "active-work",
                vec![card("card abc [Done] \"impl X\"")],
            )];
            assert_ne!(
                burst_fingerprint(&in_progress, ME),
                burst_fingerprint(&done, ME),
                "card state change (progress) must move the fingerprint to continue the loop"
            );
        }
    }

    /// what this catches: a remote peer's message must render with their roster
    /// display name (`Joel: ...`), not the raw peer UUID. Regression for the live
    /// glass-box finding 2026-06-29 — Asha saw `7711fe60-...: <text>` and could only
    /// recover the human's name because it was signed in the body; an unsigned peer
    /// would have been an indistinguishable UUID (the confabulation root cause). The
    /// roster delivery already in the same slice carries peer_id→display_name; the
    /// burst must consume it. Own posts stay attributed to `agent_name`; a peer with
    /// no roster entry falls back to its id (honest, never a fabricated name).
    mod workspace_burst_names {
        use super::super::build_workspace_turns;
        use crate::cognition::workspace::Burst;
        use crate::persona::rag_budget::{RagDelivery, RagItem, ResolutionPreference};
        use serde_json::json;
        use uuid::Uuid;

        fn delivery(source_id: &str, items: Vec<RagItem>) -> RagDelivery {
            RagDelivery {
                source_id: source_id.to_string(),
                items,
                tokens_used: 0,
                continuation: None,
                resolution_used: ResolutionPreference::Raw,
            }
        }
        fn chat(peer_id: &str, content: &str) -> RagItem {
            RagItem {
                content: content.to_string(),
                tokens: 0,
                metadata: json!({ "peer_id": peer_id, "occurred_at_ms": 1u64 }),
            }
        }
        fn roster(peer_id: &str, name: &str) -> RagItem {
            RagItem {
                content: format!("{name} [claude]"),
                tokens: 0,
                metadata: json!({ "peer_id": peer_id, "display_name": name }),
            }
        }

        #[test]
        fn remote_peer_renders_with_roster_name_self_with_agent_name() {
            let room = Uuid::nil();
            let me = "me-peer";
            let joel = "7711fe60-a19f-4f41-9ab6-24c884757338";
            let stranger = "deadbeef-0000-0000-0000-000000000000";

            let deliveries = vec![
                delivery(
                    "room-roster",
                    vec![roster(joel, "Joel"), roster(me, "Asha")],
                ),
                delivery(
                    "airc",
                    vec![
                        chat(joel, "Asha — are you there?"),
                        chat(me, "I'm here, Joel!"),
                        chat(stranger, "lurking"),
                    ],
                ),
            ];

            let turns = build_workspace_turns(&deliveries, me, "Asha");

            // STRUCTURAL attribution (the whole point of the turns refactor): the
            // `is_self` bit role attribution turns on must be set from peer_id ==
            // own_peer, NOT re-derived from a `Name:` prefix downstream. Own post →
            // self; peers (rostered or not) → not-self.
            assert_eq!(turns.len(), 3, "one turn per airc item");
            let joel_turn = &turns[0];
            let own_turn = &turns[1];
            let stranger_turn = &turns[2];
            assert!(!joel_turn.is_self && joel_turn.author == "Joel");
            assert!(
                own_turn.is_self && own_turn.author == "Asha",
                "own post must be attributed to self/agent_name, got {own_turn:?}"
            );
            assert!(
                !stranger_turn.is_self && stranger_turn.author == stranger,
                "unrostered peer is not-self and falls back to its id, got {stranger_turn:?}"
            );

            // The rendered text projection (what `world_state` IS) must still read
            // byte-identically — roster names resolved, no raw UUID leak for rostered
            // peers, own post attributed, unrostered peer honest-by-id.
            let burst = Burst::from_turns(room, turns).rendered;
            assert!(
                burst.contains("Joel: Asha — are you there?"),
                "remote peer must render with roster name, got:\n{burst}"
            );
            assert!(
                !burst.contains(joel),
                "the raw peer UUID must NOT leak into the burst, got:\n{burst}"
            );
            assert!(
                burst.contains("Asha: I'm here, Joel!"),
                "own post must attribute to agent_name, got:\n{burst}"
            );
            assert!(
                burst.contains(&format!("{stranger}: lurking")),
                "unrostered peer falls back to its id, got:\n{burst}"
            );
        }
    }

    // Bespoke `StubConversation` / `CannedAdapter` / `EmptyReader` /
    // `fake_hosted` / `fake_hosted_with_delay` deleted per
    // [[test-fixtures-are-system-primitives]]. The substrate's
    // ubiquitous primitives now power every test below:
    //   * `ScriptedConversation` for the `PersonaConversation` impl
    //     (with `.with_events`, `.with_high_water`,
    //     `.with_prime_failure`, `.require_prime_before_next_message`)
    //   * `HeuristicInferenceAdapter` for the adapter (with
    //     `.with_delay_ms` for the honest-latency test)
    //   * `StubAircCitizen` for the AircTranscriptReader — citizens
    //     are also transcript readers via the trait's supertrait
    //     bound, so it doubles as the empty-transcript reader
    //   * `hosted_with_adapter` is the local-helper-of-helpers
    //     wrapping HostedPersona — small, no impls

    fn hosted_with_adapter(
        persona_peer_id: Uuid,
        adapter: Arc<dyn AIProviderAdapter>,
    ) -> HostedPersona {
        use crate::persona::hw_tier_descriptor::HwTierCategory;
        use crate::persona::inference_profile::{PersonaInferenceProfile, SamplingProfile};
        // The PersonaInferenceProfile still keys on a bare Uuid; the
        // PersonaInstanceInfo now carries the single canonical
        // `peer_id: PeerId` (Step 4b collapsed the persona_id twin).
        let persona_id = persona_peer_id;
        // Build a profile shaped like the LCD Compat tier — the
        // substrate's lowest common denominator. Test exercises the
        // same `&ctx` derivation path production uses; no hardcoded
        // budget constants outside `profile_builder`.
        let profile = PersonaInferenceProfile {
            persona_id,
            persona_name: "Paige".to_string(),
            model_id: "fake-model".to_string(),
            gguf_local_path: None,
            tier_category: HwTierCategory::Compat,
            tier_id: "test_fixture".to_string(),
            context_length: 2048,
            n_ubatch: 512,
            n_batch: 2048,
            n_seq_max: 1,
            n_gpu_layers: 0,
            sampling: SamplingProfile::chat_defaults(),
            chat_template: None,
            stop_sequences: vec![],
        };
        HostedPersona {
            role: RoleId::Helper,
            identity: PersonaInstanceInfo {
                agent_name: "Paige".to_string(),
                peer_id: crate::identity::PeerId::from_uuid(persona_peer_id),
                home: PathBuf::from("/tmp/fake-service-loop"),
                default_room: Uuid::nil(),
                source: PersonaIdentitySource::FreshlyMinted,
            },
            profile,
            adapter,
            // System-level `StubAircCitizen` satisfies the trait
            // without standing up a real airc daemon per
            // [[test-fixtures-are-system-primitives]].
            runtime: Arc::new(StubAircCitizen::new(persona_peer_id)),
            // Brain. Tests construct a default PersonaCognition
            // and DO NOT bind airc_source — the stub citizen's
            // page_recent returns empty per the no-fallback doctrine,
            // so unit tests exercising the loop don't need the
            // airc-side composition to land items.
            cognition: Arc::new(tokio::sync::Mutex::new(
                crate::persona::unified::PersonaCognition::new(
                    persona_id,
                    "Paige".to_string(),
                    Arc::new(crate::rag::RagEngine::new()),
                ),
            )),
            // #195 slice 2: build via the same helper production
            // uses, NOT a copy-pasted template. If a future PR
            // adjusts the prompt wording, this fixture stays
            // honest about what the test exercises.
            system_prompt: super::super::supervisor::build_persona_system_prompt("Paige"),
        }
    }

    fn hosted_with_heuristic(persona_peer_id: Uuid) -> HostedPersona {
        hosted_with_adapter(persona_peer_id, Arc::new(HeuristicInferenceAdapter::new()))
    }

    fn hosted_with_delay_ms(persona_peer_id: Uuid, delay_ms: u64) -> HostedPersona {
        hosted_with_adapter(
            persona_peer_id,
            Arc::new(HeuristicInferenceAdapter::new().with_delay_ms(delay_ms)),
        )
    }

    fn fixed_now() -> u64 {
        1_700_000_000_000
    }

    /// Happy path: one inbound from another peer → one reply posted.
    /// turns_replied=1, turns_skipped=0, turns_errored=0.
    #[ignore = "slice 1D — global adapter registration (#161). respond() needs adapter in GLOBAL_REGISTRY; fixture not yet wired."]
    #[tokio::test]
    async fn replies_to_inbound_from_other_peer() {
        let persona_peer = Uuid::new_v4();
        let other_peer = Uuid::new_v4();
        let hosted = hosted_with_heuristic(persona_peer);

        let mut conversation = ScriptedConversation::new().with_events(vec![
            Ok(Some(IncomingMessage {
                lamport: 1,
                peer_id: other_peer,
                text: "hello?".to_string(),
            })),
            Ok(None),
        ]);

        // Caller-primes contract per [[no-fallbacks-ever]] — explicit.
        conversation.prime().await.expect("prime ok");

        let reader: Arc<dyn AircTranscriptReader> = Arc::new(StubAircCitizen::new(Uuid::new_v4()));
        let opts = ServeOptions {
            page_recent_limit: 10,
            rag_fetch_limit: 10,
            now_ms: fixed_now,
        };

        let outcome = serve_persona_loop(&hosted, &mut conversation, reader, opts)
            .await
            .expect("loop completes");

        assert_eq!(outcome.turns_replied, 1);
        assert_eq!(outcome.turns_skipped, 0);
        assert_eq!(outcome.turns_errored, 0);
        let said = conversation.said();
        assert_eq!(said.len(), 1);
        // HeuristicInferenceAdapter responses are shaped
        // `[heuristic:<hash>] ack: "<echo>"`. Per
        // [[test-fixtures-are-system-primitives]] we verify the
        // shape, not the literal text — the substrate's deterministic
        // adapter wired through cleanly.
        assert!(
            said[0].contains("[heuristic:") && said[0].contains("ack:"),
            "reply must come from HeuristicInferenceAdapter: {}",
            said[0]
        );
        // The loop primes the conversation exactly once at boot —
        // before any high_water_mark or next_message call. Per
        // [[persona-webrtc-all-tiers-latency-obsessed]] this is what
        // moves the airc subscribe round-trip OFF the cognition hot
        // path. If a future refactor regresses to lazy subscribe, the
        // primed count drops to 0 and this test fails loudly.
        assert_eq!(
            conversation.primed_count(),
            1,
            "serve_persona_loop must call prime() exactly once at boot"
        );
        // Latency metric recorded for the one successful reply.
        // count == 1 — exact assertion; the *value* is wall-clock-
        // dependent so we just check it's been captured. Per
        // [[init-once-handle-then-lease-zero-copy-refs]]: the metric
        // is what verifies the prime/warmup/etc. doctrines actually
        // moved cold-start off the hot path; if a future refactor
        // forgets to record, this drops to 0 and fails loudly.
        assert_eq!(
            outcome.turn_latency.count, 1,
            "successful reply must record exactly one latency sample"
        );
        assert!(
            outcome.turn_latency.min_ms.is_some(),
            "min_ms set after one sample"
        );
        assert!(
            outcome.turn_latency.max_ms.is_some(),
            "max_ms set after one sample"
        );
        assert!(
            outcome.turn_latency.mean_ms().is_some(),
            "mean computable after one sample"
        );
    }

    /// Honest latency test: injects a real ~80ms sleep into the
    /// adapter's generate_text, asserts the recorded turn_latency
    /// reflects that delay. Without this, the count-only test below
    /// would be fake-demo-shaped — passing on plumbing, silent on
    /// whether the metric tracks ACTUAL elapsed wall-clock.
    ///
    /// Bounds are generous (lower 50ms, upper 5s) so the test is
    /// jitter-tolerant on noisy CI hosts. A regression that records
    /// the wrong duration (e.g., measuring something other than the
    /// reply path) would land outside this range and fail.
    #[ignore = "slice 1D — global adapter registration (#161). respond() needs adapter in GLOBAL_REGISTRY; fixture not yet wired."]
    #[tokio::test]
    async fn latency_metric_reflects_real_wall_clock() {
        let persona_peer = Uuid::new_v4();
        let other_peer = Uuid::new_v4();
        let hosted = hosted_with_delay_ms(persona_peer, 80);

        let mut conversation = ScriptedConversation::new().with_events(vec![
            Ok(Some(IncomingMessage {
                lamport: 1,
                peer_id: other_peer,
                text: "ping?".to_string(),
            })),
            Ok(None),
        ]);

        conversation.prime().await.expect("prime ok");

        let reader: Arc<dyn AircTranscriptReader> = Arc::new(StubAircCitizen::new(Uuid::new_v4()));
        let opts = ServeOptions {
            page_recent_limit: 10,
            rag_fetch_limit: 10,
            now_ms: fixed_now,
        };

        let outcome = serve_persona_loop(&hosted, &mut conversation, reader, opts)
            .await
            .expect("loop completes");

        assert_eq!(outcome.turn_latency.count, 1);
        let observed_ms = outcome.turn_latency.min_ms.expect("recorded after sample");
        assert!(
            observed_ms >= 50,
            "recorded latency ({observed_ms}ms) must reflect the injected 80ms \
             sleep (allowing CI jitter floor of 50ms)"
        );
        assert!(
            observed_ms < 5000,
            "recorded latency ({observed_ms}ms) must not balloon — \
             upper bound 5s for sanity"
        );
    }

    /// `LatencyAggregate` math: cheap online min/max/sum/count over
    /// arbitrary inputs. Empty aggregate returns None for everything;
    /// after samples, mean = total / count and min/max track extremes.
    #[test]
    fn latency_aggregate_records_min_max_sum_count() {
        let mut agg = LatencyAggregate::default();
        assert_eq!(agg.count, 0);
        assert_eq!(agg.total_ms, 0);
        assert!(agg.min_ms.is_none());
        assert!(agg.max_ms.is_none());
        assert!(agg.mean_ms().is_none());

        agg.record(10);
        assert_eq!(agg.count, 1);
        assert_eq!(agg.total_ms, 10);
        assert_eq!(agg.min_ms, Some(10));
        assert_eq!(agg.max_ms, Some(10));
        assert_eq!(agg.mean_ms(), Some(10.0));

        agg.record(50);
        agg.record(30);
        assert_eq!(agg.count, 3);
        assert_eq!(agg.total_ms, 90);
        assert_eq!(agg.min_ms, Some(10));
        assert_eq!(agg.max_ms, Some(50));
        assert_eq!(agg.mean_ms(), Some(30.0));
    }

    /// Saturating add: if the substrate ever runs a session long
    /// enough for total_ms to approach u64::MAX (~580 million years
    /// at 1ms/turn), `record` saturates rather than wraps. Locks the
    /// invariant; the substrate would never hit this in practice but
    /// the safety property matters per [[every-error-is-an-opportunity-to-battle-harden]].
    #[test]
    fn latency_aggregate_saturates_on_overflow() {
        let mut agg = LatencyAggregate {
            count: 1,
            total_ms: u64::MAX - 5,
            min_ms: Some(0),
            max_ms: Some(u64::MAX - 5),
        };
        agg.record(100);
        assert_eq!(agg.total_ms, u64::MAX, "saturated, not wrapped");
        assert_eq!(agg.count, 2);
    }

    /// `ServeOutcome::default()` initializes all five phase
    /// aggregates to empty (count=0). Pin this so future fields
    /// added to the struct don't accidentally regress the existing
    /// fields' Default-impl behavior — every aggregate stays
    /// independently zeroed and the decomposition starts clean.
    #[test]
    fn serve_outcome_phase_aggregates_default_to_empty() {
        let outcome = ServeOutcome::default();
        assert_eq!(outcome.recall_latency.count, 0);
        assert_eq!(outcome.admit_latency.count, 0);
        assert_eq!(outcome.compose_latency.count, 0);
        assert_eq!(outcome.respond_latency.count, 0);
        assert_eq!(outcome.say_latency.count, 0);
        assert!(outcome.recall_latency.mean_ms().is_none());
        assert!(outcome.respond_latency.mean_ms().is_none());
    }

    /// #152 LCD identity grounding. Pre-fix prompt was a single
    /// line ("You are {name}, an autonomous AI persona on the
    /// grid.") that LCD-tier models (Qwen2.5-0.5B etc.) couldn't
    /// hold under any pressure — they drifted to training
    /// defaults (Claude / GPT / Siemens PLC etc).
    ///
    /// This test pins the structural contract of the post-fix
    /// prompt — the SPECIFIC clauses that address known drift
    /// modes — without pinning prose verbatim (so a future PR
    /// can tighten wording without churning the test). The pin
    /// covers:
    ///
    /// 1. Name + role anchoring in opening line.
    /// 2. Explicit "you are NOT" list of common drift targets.
    /// 3. First-person stability instruction.
    /// 4. Grid / room context vocabulary.
    /// 5. Silence-token reference (couples to the silence
    ///    affordance shipped in the same arc — both prompt
    ///    blocks are load-bearing for #151 + #152).
    ///
    /// A future PR that materially changes any clause must update
    /// the matching assertion; silent drift would re-introduce the
    /// identity-drift bug.
    #[test]
    fn system_prompt_carries_lcd_identity_grounding() {
        let prompt = super::super::supervisor::build_persona_system_prompt("Paige");
        let s = prompt.as_ref();

        assert!(
            s.contains("Paige"),
            "persona name must appear at least once. Got: {s}"
        );
        assert!(
            s.contains("autonomous AI persona"),
            "role line missing. Got: {s}"
        );
        assert!(
            s.contains("Identity"),
            "identity block header missing. Got: {s}"
        );
        // Drift-target enumeration. Concrete name list is the
        // operationally effective form on LCD models — abstract
        // "don't drift" instructions don't stick.
        for target in &["Claude", "GPT", "Gemini", "Llama", "Qwen", "Siemens PLC"] {
            assert!(
                s.contains(target),
                "drift target {target:?} missing from 'you are NOT' enumeration. Got: {s}"
            );
        }
        // First-person stability. Single most effective LCD anti-
        // drift instruction per Joel 2026-06-03 testing.
        assert!(
            s.contains("first person") || s.contains("first-person"),
            "first-person stability clause missing. Got: {s}"
        );
        // Grid / room context vocabulary.
        assert!(s.contains("grid"), "'grid' vocabulary missing. Got: {s}");
        assert!(
            s.contains("Room") || s.contains("room"),
            "'room' vocabulary missing. Got: {s}"
        );
        // Couples to the silence affordance.
        assert!(
            s.contains("[Conversational Presence]") || s.contains("silence"),
            "silence-affordance reference missing — identity prompt should hint at the affordance assembled downstream. Got: {s}"
        );
    }

    /// #195 slice 2: pin that cloning the cached prompt is a
    /// cheap Arc refcount bump, not a deep copy. Without this,
    /// a future refactor swapping `Arc<str>` for `String` would
    /// silently restore the per-turn-allocation cost the slice
    /// shipped to eliminate.
    #[test]
    fn cached_system_prompt_clones_via_arc_refcount() {
        let original = super::super::supervisor::build_persona_system_prompt("Paige");
        let cloned = std::sync::Arc::clone(&original);
        assert_eq!(
            std::sync::Arc::strong_count(&original),
            2,
            "Arc::clone must bump the refcount (cheap pointer copy) — \
             not deep-copy the underlying str"
        );
        // Pointer-equality on the underlying str confirms zero-copy.
        assert!(
            std::ptr::eq(
                original.as_ref() as *const str,
                cloned.as_ref() as *const str
            ),
            "cloned Arc must point at the SAME str storage as the original"
        );
    }

    /// #195 slice 3: pin that the migration from
    /// `format!("{:?}", role).to_lowercase()` to `role.as_str()`
    /// in `serve_persona_loop_inner` is byte-for-byte
    /// behavior-preserving for EVERY current `RoleId` variant.
    ///
    /// Non-circular: the two sides are independently-derived —
    /// the left is the new direct production path (a hand-
    /// curated `match role { ... }` in `role_template.rs::as_str`),
    /// the right is the pre-slice-3 Debug-format + Unicode
    /// lowercase chain. They are equal today by careful design
    /// of `as_str()`; if a future PR adds a `RoleId` variant
    /// whose `as_str()` choice DOESN'T match the legacy
    /// Debug+lowercase, this test breaks loudly so the author
    /// makes an explicit decision (update the test to record
    /// the intentional divergence, or align `as_str()` with
    /// the legacy form).
    ///
    /// The exhaustive `match` matcher (no wildcard) makes the
    /// compiler force a new variant to appear here at the same
    /// time it appears in `RoleId`, so the `variants` array
    /// stays complete by construction.
    #[test]
    fn role_as_str_preserves_pre_slice3_specialty_format_for_each_role() {
        // RoleId is already in scope via `use crate::persona::role_template::RoleId`
        // at the top of the test module — no per-test import needed.
        let variants: &[RoleId] = &[
            RoleId::Helper,
            RoleId::Coder,
            RoleId::Sentinel,
            RoleId::Custom,
        ];
        for role in variants {
            let direct = role.as_str();
            let legacy = format!("{:?}", role).to_lowercase();
            assert_eq!(
                direct, legacy,
                "RoleId::{:?}.as_str() must produce the SAME string \
                 the pre-#195-slice-3 per-turn \
                 format!(\"{{:?}}\", role).to_lowercase() did — \
                 otherwise switching the service loop's specialty \
                 source from Debug+lowercase to as_str() silently \
                 changes what downstream prompt assembly reads",
                role
            );
        }
        // Compile-time exhaustiveness: a new RoleId variant
        // forces a new arm here, which forces the author to add
        // it to the `variants` array above. The arms each touch
        // the variant explicitly (no `_` wildcard) so the
        // compiler rejects silent drift.
        let _ = |role: RoleId| match role {
            RoleId::Helper => (),
            RoleId::Coder => (),
            RoleId::Sentinel => (),
            RoleId::Custom => (),
        };
    }

    /// Aggregate-math property: each phase aggregate is structurally
    /// identical to `turn_latency` (same `LatencyAggregate` shape).
    /// A future refactor that swaps one to a different type would
    /// fail this property test. Belt-and-braces for the
    /// "decomposition components compose the same way" contract.
    #[test]
    fn phase_aggregates_use_same_type_as_turn_latency() {
        let mut outcome = ServeOutcome::default();
        // Record one sample into each — same call shape as
        // `turn_latency.record`. If any phase aggregate had drifted
        // to a different type/signature, this fails to compile.
        outcome.turn_latency.record(100);
        outcome.recall_latency.record(5);
        outcome.admit_latency.record(2);
        outcome.compose_latency.record(8);
        outcome.respond_latency.record(80);
        outcome.say_latency.record(3);

        // Sum-of-phases should be ≤ turn_latency. Sample test
        // (100 vs 5+2+8+80+3 = 98) demonstrates the residual is
        // the projection + bookkeeping overhead the decomposition
        // doesn't account for. The substrate's `turn_latency`
        // remains the source of truth for end-to-end cost.
        let phase_sum = outcome.recall_latency.total_ms
            + outcome.admit_latency.total_ms
            + outcome.compose_latency.total_ms
            + outcome.respond_latency.total_ms
            + outcome.say_latency.total_ms;
        assert_eq!(phase_sum, 98, "phases sum correctly");
        assert!(
            phase_sum <= outcome.turn_latency.total_ms,
            "phase-sum ({phase_sum}ms) must not exceed turn_latency ({}ms) — \
             the decomposition is internal-to-turn",
            outcome.turn_latency.total_ms
        );
    }

    /// Caller-primes contract: per [[no-fallbacks-ever]] the loop does
    /// NOT prime as a safety net — callers honor the contract. If a
    /// test forgets to call `conversation.prime()` before
    /// `serve_persona_loop`, the conversation's `next_message`
    /// returns a typed `Err("called before prime()")`. The loop
    /// surfaces this as `turns_errored` per the substrate's
    /// honest-error doctrine, then ends when the stub yields `None`.
    ///
    /// Locks the absence of the belt-and-suspenders prime() call in
    /// serve_persona_loop_inner. If a future refactor adds a
    /// safety-net prime back into the loop, this test starts
    /// reporting `turns_errored == 0`, exposing the regression.
    #[tokio::test]
    async fn loop_without_caller_prime_surfaces_typed_error_per_turn() {
        let persona_peer = Uuid::new_v4();
        let other_peer = Uuid::new_v4();
        let hosted = hosted_with_heuristic(persona_peer);

        // System primitive: `require_prime_before_next_message` makes
        // `ScriptedConversation` mirror `AircPersonaConversation`'s
        // caller-primes contract — next_message returns Err if prime
        // wasn't called first. Substitutes the bespoke
        // UnprimedConversation per [[test-fixtures-are-system-primitives]].
        let mut conversation = ScriptedConversation::new()
            .with_events(vec![Ok(Some(IncomingMessage {
                lamport: 1,
                peer_id: other_peer,
                text: "would-be-message".to_string(),
            }))])
            .require_prime_before_next_message();

        // INTENTIONALLY do NOT prime — verify the loop doesn't auto-prime.
        let reader: Arc<dyn AircTranscriptReader> = Arc::new(StubAircCitizen::new(Uuid::new_v4()));
        let outcome = serve_persona_loop(
            &hosted,
            &mut conversation,
            reader,
            ServeOptions {
                page_recent_limit: 10,
                rag_fetch_limit: 10,
                now_ms: fixed_now,
            },
        )
        .await
        .expect("loop completes (each next_message err counts as turn_errored)");

        assert_eq!(
            outcome.turns_errored, 1,
            "unprimed conversation's typed next_message err counts as errored turn"
        );
        assert_eq!(outcome.turns_replied, 0);
    }

    /// Self-loop guard: when the inbound peer_id matches the
    /// persona's own peer_id, the loop skips it (no inference call,
    /// no say). turns_skipped=1.
    #[tokio::test]
    async fn skips_self_loop_messages() {
        let persona_peer = Uuid::new_v4();
        let hosted = hosted_with_heuristic(persona_peer);

        let mut conversation = ScriptedConversation::new().with_events(vec![
            Ok(Some(IncomingMessage {
                lamport: 1,
                peer_id: persona_peer, // SELF
                text: "my own echo".to_string(),
            })),
            Ok(None),
        ]);
        conversation.prime().await.expect("prime ok");

        let reader: Arc<dyn AircTranscriptReader> = Arc::new(StubAircCitizen::new(Uuid::new_v4()));
        let outcome = serve_persona_loop(
            &hosted,
            &mut conversation,
            reader,
            ServeOptions {
                page_recent_limit: 10,
                rag_fetch_limit: 10,
                now_ms: fixed_now,
            },
        )
        .await
        .expect("loop completes");

        assert_eq!(outcome.turns_replied, 0);
        assert_eq!(outcome.turns_skipped, 1);
        assert_eq!(outcome.turns_errored, 0);
        assert!(conversation.said().is_empty());
    }

    /// Pre-watermark guard: messages with lamport <= high_water are
    #[ignore = "slice 1D — global adapter registration (#161). respond() needs adapter in GLOBAL_REGISTRY; fixture not yet wired."]
    /// skipped. Avoids replying to history on attach.
    #[tokio::test]
    async fn skips_messages_below_high_water_mark() {
        let persona_peer = Uuid::new_v4();
        let other_peer = Uuid::new_v4();
        let hosted = hosted_with_heuristic(persona_peer);

        let mut conversation = ScriptedConversation::new()
            .with_high_water(100) // pre-attach history was up to lamport=100
            .with_events(vec![
                Ok(Some(IncomingMessage {
                    lamport: 50, // BEFORE attach
                    peer_id: other_peer,
                    text: "ancient".to_string(),
                })),
                Ok(Some(IncomingMessage {
                    lamport: 100, // exactly at the mark — also skipped
                    peer_id: other_peer,
                    text: "boundary".to_string(),
                })),
                Ok(Some(IncomingMessage {
                    lamport: 101, // FRESH
                    peer_id: other_peer,
                    text: "new".to_string(),
                })),
                Ok(None),
            ]);
        conversation.prime().await.expect("prime ok");

        let reader: Arc<dyn AircTranscriptReader> = Arc::new(StubAircCitizen::new(Uuid::new_v4()));
        let outcome = serve_persona_loop(
            &hosted,
            &mut conversation,
            reader,
            ServeOptions {
                page_recent_limit: 10,
                rag_fetch_limit: 10,
                now_ms: fixed_now,
            },
        )
        .await
        .expect("loop completes");

        assert_eq!(outcome.turns_replied, 1, "only lamport=101 should reply");
        assert_eq!(
            outcome.turns_skipped, 2,
            "lamport=50 and lamport=100 both pre-mark"
        );
        assert_eq!(outcome.turns_errored, 0);
        assert_eq!(conversation.said().len(), 1);
    }

    /// Transient transport error increments turns_errored AND the
    /// loop continues — does NOT propagate as a Result::Err from
    #[ignore = "slice 1D — global adapter registration (#161). respond() needs adapter in GLOBAL_REGISTRY; fixture not yet wired."]
    /// serve_persona_loop. The trailing Ok(None) eventually ends it
    /// cleanly. Pins the "live stream lag — resume continues" shape
    /// per `[[no-fallbacks-ever]]`: typed Err on the boundary, loop
    /// handles, no silent drop.
    #[tokio::test]
    async fn transient_next_message_error_does_not_kill_loop() {
        let persona_peer = Uuid::new_v4();
        let other_peer = Uuid::new_v4();
        let hosted = hosted_with_heuristic(persona_peer);

        let mut conversation = ScriptedConversation::new().with_events(vec![
            Err("stream lag".to_string()),
            Ok(Some(IncomingMessage {
                lamport: 1,
                peer_id: other_peer,
                text: "after lag".to_string(),
            })),
            Ok(None),
        ]);
        conversation.prime().await.expect("prime ok");

        let reader: Arc<dyn AircTranscriptReader> = Arc::new(StubAircCitizen::new(Uuid::new_v4()));
        let outcome = serve_persona_loop(
            &hosted,
            &mut conversation,
            reader,
            ServeOptions {
                page_recent_limit: 10,
                rag_fetch_limit: 10,
                now_ms: fixed_now,
            },
        )
        .await
        .expect("loop completes despite transient error");

        assert_eq!(outcome.turns_replied, 1);
        assert_eq!(outcome.turns_errored, 1);
        assert_eq!(outcome.turns_skipped, 0);
        assert_eq!(conversation.said().len(), 1);
    }
}
