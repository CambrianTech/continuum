use super::*;

/// The conversation as persona drives it: cognition's [`PersonaConversation`] plus the airc
/// citizen behind it. Cognition never needs the citizen (its hands and claims are bound up
/// here, in `persona::card_hands`), so the accessor lives on persona's side of the seam and
/// a persona call into cognition upcasts to the narrower contract.
pub trait CitizenConversation: PersonaConversation {
    /// #170: the airc citizen behind this conversation, for OFF-THREAD streaming
    /// (`publish_stream_chunk`) from a spawned drain task. Returns an OWNED `Arc`
    /// so the forwarder can hold it `'static`. `None` for scripted / stub
    /// conversations — they don't stream to a live room; the airc conversation
    /// returns its runtime handle.
    fn stream_citizen(&self) -> Option<std::sync::Arc<dyn crate::persona::airc_citizen::AircCitizen>> {
        None
    }
}

/// Publish an utterance and record it before subsequent work can begin.
/// All resident speech paths share this boundary; refused publications never
/// enter the own-speech repetition-observation ring. Assistant history still
/// comes from the attributed room timeline. The conversation owns transport.
pub(crate) async fn publish_own_speech(
    conversation: &dyn PersonaConversation,
    peer: crate::identity::PeerId,
    room: Uuid,
    text: &str,
) -> Result<(), String> {
    conversation.say_in(room, text).await?;
    crate::cognition::deliberation_budget::record_own_speech(peer, room, text);
    Ok(())
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
    /// Eval-preemption gate — ALWAYS present (the loop never asks "is there a gate?",
    /// only reads its value). While `true` the service loop goes FULLY quiet: it neither
    /// self-ticks nor consumes inbound (peer replies are `Wake::Msg` — gating only the
    /// self-tick would let a conversation cascade keep the GPU busy). Messages buffer in
    /// the stream and resume when the lease drops. The measuring eval drives cognition
    /// directly (not through this loop), so a quiet loop never blocks it — a benchmark
    /// gets a clean GPU without despawning her.
    /// The caller supplies the SHARED atomic the registry owns (`quiesced_flag`) so a
    /// `QuiesceLease` can flip it; `Default` is a private, never-set flag (this persona
    /// is simply never quiesced). Registry owns write, loop owns read.
    /// [[benchmark-is-a-governor-preemption-lease]]
    pub quiesced: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl Default for ServeOptions {
    fn default() -> Self {
        Self {
            page_recent_limit: 50,
            rag_fetch_limit: 50,
            now_ms: || {
                EpochMs::now().millis()
            },
            // A private, never-set flag: a persona built with default options is
            // simply never quiesced (no lease can reach this atomic). Production
            // overrides it with the registry's shared flag.
            quiesced: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
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
pub(super) struct PhaseTimings {
    pub(super) recall_ms: u64,
    pub(super) admit_ms: u64,
    pub(super) compose_ms: u64,
    pub(super) respond_ms: u64,
    pub(super) say_ms: u64,
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
    /// Time spent in the turn's cognition cycle (including the LLM
    /// call). Does NOT hold the cognition
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
    conversation: &mut dyn CitizenConversation,
    opts: ServeOptions,
    obligations: crate::persona::obligation::Obligations,
) -> Result<ServeOutcome, String> {
    use tracing::Instrument;
    // Her presence reads THIS: "ready" means a loop is running, not merely that her airc
    // runtime is up (card 7524aa5b). Held for the loop's whole life, released on any exit.
    let _live = crate::persona::cognition_pulse::enter_loop(ctx.identity.peer_id.as_uuid());
    serve_persona_loop_inner(ctx, conversation, opts, obligations)
        .instrument(ctx.span())
        .await
}
