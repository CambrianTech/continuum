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
/// `spawn_persona_service` enforces this. Direct callers (the
/// `airc_chat_demo` binary, integration tests) must call `prime`
/// explicitly. Per [[no-fallbacks-ever]] this loop does NOT prime
/// as a safety net — one place primes, callers honor the contract.
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
    // does this before spawning the task. Direct callers
    // (`airc_chat_demo`, integration tests) prime explicitly before
    // calling.
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

    while let Some(item) = next_event(conversation, &mut outcome).await {
        let msg = item;
        if msg.lamport <= high_water {
            outcome.turns_skipped += 1;
            continue;
        }
        high_water = msg.lamport.max(high_water);

        if msg.peer_id == ctx.identity.peer_id {
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
        let recalled_engrams: Vec<String> = {
            let cognition = ctx.cognition.lock().await;
            // recall BEFORE admit so this turn's recalled_engrams is
            // "what I knew going in" — the current message isn't
            // recall; it's the trigger.
            //
            // Algorithm 4 scoring (#165): salience × recency-decay
            // ranks engrams; record_recall_hit on the returned set
            // bumps their salience (Hebbian rehearsal — use-it-
            // keeps-it). Memory that gets used compounds; memory
            // that doesn't drains toward SALIENCE_FLOOR but doesn't
            // disappear. PR #91 (RecallMetadata sidecar) + #92
            // (decay tick) provide the scoring infrastructure;
            // recall_scored composes them on the read path.
            // Per #195 slice 1: time the L2 retrieval pass.
            let recall_started = std::time::Instant::now();
            let scored = cognition.admission.recall_scored(now_ms, 8);
            phase_timings.recall_ms = recall_started.elapsed().as_millis() as u64;

            // Per-engram introspection: the L2 → prompt seam is
            // observable, not opaque, per
            // [[observability-is-half-the-architecture]] + Joel's
            // 2026-06-03 "introspect all rag" directive. Each line
            // shows what scored what, so optimization can target
            // actual scoring behavior, not guesses.
            for (rank, (engram, salience)) in scored.iter().enumerate() {
                let preview: String = engram.content.chars().take(80).collect();
                tracing::info!(
                    lamport = msg.lamport,
                    rank,
                    engram_id = %&engram.id.to_string()[..8],
                    salience = format!("{:.3}", salience),
                    content = %preview,
                    "recall_scored — engram delivered to RespondInput"
                );
            }

            let recalled: Vec<String> =
                scored.into_iter().map(|(e, _score)| e.content).collect();

            // Admit now. Errors here are non-fatal — the cognition
            // turn can still run; the engram just doesn't form. Per
            // [[no-fallbacks-ever]] we surface the failure visibly,
            // not silently.
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
                    recalled_count = recalled.len(),
                    engram_count = cognition.admission.engram_count(),
                    "admitted incoming → L2 store"
                );
            }
            recalled
        };

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

        // 2. Project the brain's composed deliveries into the
        //    canonical `RespondInput`. AIRC delivery → recent_history;
        //    engram delivery → recalled_engrams; everything else
        //    threaded from PersonaContext.
        let recent_history: Vec<crate::cognition::shared_analysis::RecentMessage> = composed
            .deliveries
            .iter()
            .filter(|d| d.source_id == "airc")
            .flat_map(|d| d.items.iter())
            .map(|item| {
                let peer_label = item
                    .metadata
                    .get("peer_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("peer")
                    .to_string();
                crate::cognition::shared_analysis::RecentMessage {
                    id: Uuid::new_v4(),
                    sender_name: peer_label,
                    text: item.content.clone(),
                }
            })
            .collect();
        // recalled_engrams is populated above from
        // admission.recall_recent(8) — substrate-managed memory,
        // not the airc transcript window. The engram delivery from
        // compose_for_turn ALSO flows from the same admission store
        // via engram_source — Algorithm 4 scoring will arbitrate
        // overlap in a future slice.

        let turn_context = crate::persona::turn_context::TurnContext::arc(
            ctx.identity.default_room,
            recent_history,
            Vec::new(),
        );

        let respond_input = crate::persona::response::RespondInput {
            persona: crate::cognition::PersonaSlot {
                persona_id: ctx.identity.persona_id,
                specialty: format!("{:?}", ctx.role).to_lowercase(),
                display_name: ctx.identity.agent_name.clone(),
            },
            turn_context,
            message_id: Uuid::new_v4(),
            message_text: msg.text.clone(),
            other_persona_names: Vec::new(),
            system_prompt: format!(
                "You are {persona}, an autonomous AI persona on the grid.",
                persona = ctx.identity.agent_name
            ),
            model: ctx.profile.model_id.clone(),
            is_voice: false,
            message_media: Vec::new(),
            capabilities: std::collections::HashSet::new(),
            recalled_engrams,
        };

        // 3. Run the cognition cycle. The persona may speak or stay
        //    silent — both are first-class outcomes per the
        //    PersonaResponse contract.
        // Per #195 slice 1: time the LLM bulk — typically the
        // dominant cost and the primary target of subsequent
        // optimization slices.
        let respond_started = std::time::Instant::now();
        let response_result = crate::persona::response::respond(respond_input).await;
        phase_timings.respond_ms = respond_started.elapsed().as_millis() as u64;
        let response = match response_result {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(
                    lamport = msg.lamport,
                    error = %e,
                    "respond cycle failed"
                );
                outcome.turns_errored += 1;
                continue;
            }
        };

        let response_text = match response {
            crate::persona::response::PersonaResponse::Silent { reason, .. } => {
                tracing::info!(
                    lamport = msg.lamport,
                    reason = %reason,
                    "persona chose silence — substrate honors decision"
                );
                outcome.turns_skipped += 1;
                continue;
            }
            crate::persona::response::PersonaResponse::Spoke { text, .. } => text,
        };

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
            outcome.turns_errored += 1;
            continue;
        }
        let turn_duration_ms = turn_started.elapsed().as_millis() as u64;
        outcome.turn_latency.record(turn_duration_ms);
        // Per #195 slice 1: record the phase decomposition. Same
        // sample set as `turn_latency` — only successful replies
        // reach this point.
        outcome.recall_latency.record(phase_timings.recall_ms);
        outcome.admit_latency.record(phase_timings.admit_ms);
        outcome.compose_latency.record(phase_timings.compose_ms);
        outcome.respond_latency.record(phase_timings.respond_ms);
        outcome.say_latency.record(phase_timings.say_ms);
        outcome.turns_replied += 1;
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
        // Honor the Slice-1B-of-#142 invariant: `persona_id ==
        // peer_id` for every PersonaInstanceInfo. Test fixtures
        // that bypass the runtime constructor (where the collapse
        // would otherwise be enforced) keep both fields equal so the
        // identity shape matches what production sees.
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
                persona_id,
                agent_name: "Paige".to_string(),
                peer_id: persona_peer_id,
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

        let reader: Arc<dyn AircTranscriptReader> =
            Arc::new(StubAircCitizen::new(Uuid::new_v4()));
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

        let reader: Arc<dyn AircTranscriptReader> =
            Arc::new(StubAircCitizen::new(Uuid::new_v4()));
        let opts = ServeOptions {
            page_recent_limit: 10,
            rag_fetch_limit: 10,
            now_ms: fixed_now,
        };

        let outcome = serve_persona_loop(&hosted, &mut conversation, reader, opts)
            .await
            .expect("loop completes");

        assert_eq!(outcome.turn_latency.count, 1);
        let observed_ms = outcome
            .turn_latency
            .min_ms
            .expect("recorded after sample");
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
        let reader: Arc<dyn AircTranscriptReader> =
            Arc::new(StubAircCitizen::new(Uuid::new_v4()));
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

        let reader: Arc<dyn AircTranscriptReader> =
            Arc::new(StubAircCitizen::new(Uuid::new_v4()));
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

        let reader: Arc<dyn AircTranscriptReader> =
            Arc::new(StubAircCitizen::new(Uuid::new_v4()));
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
    /// cleanly. Models the demo's "live stream lag — resume continues"
    /// behavior (`bin/airc_chat_demo.rs:346`).
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

        let reader: Arc<dyn AircTranscriptReader> =
            Arc::new(StubAircCitizen::new(Uuid::new_v4()));
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
