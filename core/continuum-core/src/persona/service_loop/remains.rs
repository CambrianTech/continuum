use super::*;

/// A substrate-friendly slice of one airc event: just what the
/// service loop needs to decide whether to respond. Strips away the
/// full `TranscriptEvent` surface so the conversation abstraction
/// stays compact and the trait remains stubbable without dragging
/// every airc type into the test.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct IncomingMessage {
    /// The transcript event this turn came from — the row a `chat:heard`
    /// receipt points back at. Nil for scripted/synthetic turns.
    pub event_id: Uuid,
    /// Monotonic lamport clock — used for pre-attach high-water-mark
    /// filtering.
    pub lamport: u64,
    /// Cryptographic source identity per
    /// [[persona-identity-derives-from-source-id]]. The loop compares
    /// against the hosting persona's own peer_id to skip self-loop
    /// echoes.
    pub peer_id: Uuid,
    /// Caption/text of the room message.
    pub text: String,
    /// Durable references only; decoding a message never fetches attachment bytes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub media: Vec<super::super::channel_items::MediaItemRequest>,
    /// The room the event ARRIVED in — the transport's `TranscriptEvent.room_id`,
    /// which is authoritative for the turn's context (A.6, the missing context
    /// axis). Nil means the source predates room stamping (scripted/test
    /// conversations) and the loop falls back to `identity.default_room`; a real
    /// wire event always carries its room. Without this, an operator's CLI
    /// publish woke the persona into a nil-room turn where every room-scoped
    /// RAG source (roster, doctrine, board, kanban) abstained — she heard the
    /// words but stood in no room (glass-boxed 2026-07-23, Anwen ACK test).
    pub room_id: Uuid,
    /// A typed work fact (a verdict, a card move, a submission, a claim) decoded
    /// from the room by `realtime_wire::room_work_from_event`. `Some` means this
    /// turn is a BOARD FACT she perceives, rendered as such, never as the actor's
    /// speech; `peer_id` is the actor (the reviewer, the mover). Phase 1 wiring of
    /// EVENT-MIND.md: measured 2026-10-04, a PASS on her own card reached her node
    /// and was dropped as `non_chat_schema`; nothing on her board could wake her.
    pub work: Option<crate::airc::realtime_wire::RoomWork>,
}

impl IncomingMessage {
    pub(crate) fn render_content(&self) -> String {
        if let Some(work) = &self.work {
            return crate::airc::realtime_wire::render_room_work(work, self.peer_id);
        }
        crate::airc::realtime_wire::render_room_content(&self.text, &self.media)
    }

    /// The same source-labelled input at inference, replay and teaching boundaries.
    /// This is rendering, not a replacement for the typed event/room identity.
    pub(crate) fn render_room_update(&self) -> String {
        format!(
            "[Room message received during this turn; room {}; peer {}; event {}]\n{}",
            self.room_id, self.peer_id, self.event_id, self.render_content()
        )
    }
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

    /// The cutoff established by successful priming. Live conversations freeze
    /// this at attach, so starting the loop does not issue another fallible RPC.
    async fn initial_water_mark(&self, limit: usize) -> Result<u64, String> {
        self.high_water_mark(limit).await
    }


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

    /// Perceive currently ready room input without waiting for a new arrival.
    /// The conversation RETAINS ownership for subsequent room attention, including
    /// if the active drive is cancelled or cannot fit these inputs. Repeated
    /// snapshots may overlap: the drive uses the ordinary event-id deduper.
    async fn perceive_ready(
        &mut self,
    ) -> Result<&std::collections::VecDeque<Arc<IncomingMessage>>, String>;

    /// Reply with text INTO A NAMED ROOM — normally the room the turn
    /// being answered arrived in ([`IncomingMessage::room_id`]).
    ///
    /// Not "the persona's default room", which is what this used to
    /// be. A persona is in more than one room, so a reply that always
    /// went to her default answered the wrong audience the moment she
    /// was addressed anywhere else — visibly worse than silence,
    /// because it reads as a non-sequitur rather than a missing wire.
    /// `Uuid::nil()` keeps the documented pre-room-stamping contract
    /// (scripted / test sources) and routes to her default.
    async fn say_in(&self, room_id: Uuid, text: &str) -> Result<(), String>;

    /// #170: the airc citizen behind this conversation, for OFF-THREAD streaming
    /// (`publish_stream_chunk`) from a spawned drain task. Returns an OWNED `Arc`
    /// so the forwarder can hold it `'static`. `None` for scripted / stub
    /// conversations — they don't stream to a live room; the airc conversation
    /// returns its runtime handle.
    /// Her wakes from the one feed (EVENT-MIND §1b), once her perception region has
    /// booted; `None` before then, after they were taken, or for a conversation with no
    /// region. The serve loop takes them once and consumes them beside her inbox.
    fn take_wakes(&mut self) -> Option<tokio::sync::mpsc::Receiver<crate::persona::perception_region::Wake>> {
        None
    }

    fn stream_citizen(
        &self,
    ) -> Option<std::sync::Arc<dyn crate::persona::airc_citizen::AircCitizen>> {
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
    opts: ServeOptions,
) -> Result<ServeOutcome, String> {
    use tracing::Instrument;
    // Her presence reads THIS: "ready" means a loop is running, not merely that her airc
    // runtime is up (card 7524aa5b). Held for the loop's whole life, released on any exit.
    let _live = crate::persona::cognition_pulse::enter_loop(ctx.identity.peer_id.as_uuid());
    serve_persona_loop_inner(ctx, conversation, opts)
        .instrument(ctx.span())
        .await
}
