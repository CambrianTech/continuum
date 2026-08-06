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
    /// The room the event ARRIVED in — the transport's `TranscriptEvent.room_id`,
    /// which is authoritative for the turn's context (A.6, the missing context
    /// axis). Nil means the source predates room stamping (scripted/test
    /// conversations) and the loop falls back to `identity.default_room`; a real
    /// wire event always carries its room. Without this, an operator's CLI
    /// publish woke the persona into a nil-room turn where every room-scoped
    /// RAG source (roster, doctrine, board, kanban) abstained — she heard the
    /// words but stood in no room (glass-boxed 2026-07-23, Anwen ACK test).
    pub room_id: Uuid,
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

    /// #170: the airc citizen behind this conversation, for OFF-THREAD streaming
    /// (`publish_stream_chunk`) from a spawned drain task. Returns an OWNED `Arc`
    /// so the forwarder can hold it `'static`. `None` for scripted / stub
    /// conversations — they don't stream to a live room; the airc conversation
    /// returns its runtime handle.
    fn stream_citizen(
        &self,
    ) -> Option<std::sync::Arc<dyn crate::persona::airc_citizen::AircCitizen>> {
        None
    }
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
                use std::time::{SystemTime, UNIX_EPOCH};
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0)
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
    const RECENT_INBOUND_WINDOW: usize = 24;
    let mut recent_inbound: std::collections::VecDeque<String> =
        std::collections::VecDeque::with_capacity(RECENT_INBOUND_WINDOW);
    // Roster-name memory for engram attribution: harvested from each turn's
    // composed `room-roster` delivery (zero extra I/O — the roster scan already
    // runs per turn), consulted at admit time so her MEMORY records
    // "Joel said…" / "Claude said…", never "peer-7711fe60 said…". First-class
    // peers — humans, outside agents, personas — all resolve through the one
    // roster airc owns ([[airc-native-identity-rooms-security]]). Eventually
    // consistent: a brand-new peer's FIRST message may admit under its short
    // peer tag (honest, never fabricated); every later one is named.
    let mut roster_names: std::collections::HashMap<Uuid, String> =
        std::collections::HashMap::new();

    // While an eval-preemption lease is held, the loop goes FULLY quiet on this beat —
    // it neither self-ticks NOR consumes inbound. Gating only the self-tick isn't
    // enough: a peer's reply arrives as `Wake::Msg`, so an in-flight conversation would
    // keep the GPU busy right through the measurement. Messages simply BUFFER in the
    // airc stream and are served when the lease drops; the poll is short so resume is
    // prompt. The measuring eval drives cognition DIRECTLY (not through this loop), so a
    // quiet loop never blocks it. [[benchmark-is-a-governor-preemption-lease]]
    // [[first-class-citizens-even-during-benchmarks]]
    const QUIESCE_POLL: std::time::Duration = std::time::Duration::from_millis(400);
    loop {
        if opts.quiesced.load(std::sync::atomic::Ordering::Relaxed) {
            tokio::time::sleep(QUIESCE_POLL).await;
            continue;
        }
        let wake = tokio::select! {
            ev = next_event(conversation, &mut outcome) => match ev {
                Some(m) => Wake::Msg(m),
                None => Wake::Stop,
            },
            _ = tokio::time::sleep(next_beat) => Wake::Tick,
        };
        let msg = match wake {
            Wake::Stop => break,
            Wake::Tick => {
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
                if crate::cognition::resource_admission::shared_model_saturated() {
                    next_beat = (next_beat + next_beat / 2).min(rest_cap);
                    continue;
                }
                let before = last_burst_fp;
                run_self_cycle(ctx, conversation, &opts, &mut last_burst_fp).await;
                next_beat = if last_burst_fp != before {
                    engaged_beat
                } else {
                    (next_beat + next_beat / 2).min(rest_cap)
                };
                continue;
            }
            // A message means life in the room — snap back to a quick beat so she's present
            // and responsive, not drifting in rest.
            Wake::Msg(m) => {
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
        let self_id = ctx.identity.peer_id.as_uuid();
        let mut qualifying: Vec<IncomingMessage> = Vec::with_capacity(backlog.len());
        for m in backlog {
            let stale = m.lamport <= high_water;
            high_water = m.lamport.max(high_water);
            if stale || m.peer_id == self_id {
                outcome.turns_skipped += 1;
            } else {
                qualifying.push(m);
            }
        }
        // Trigger = newest ADDRESSED message in the backlog (a question put to
        // her outranks newer ambient chatter — she answers it WITH the newer
        // context visible in the transcript), else the newest overall.
        let coalesced = qualifying.len().saturating_sub(1);
        let msg = match qualifying
            .iter()
            .rposition(|m| ctx.identity.persona_identity().mentions(&m.text))
            .map(|i| qualifying.swap_remove(i))
            .or_else(|| qualifying.pop())
        {
            Some(m) => m,
            None => {
                if stream_ended {
                    break;
                }
                continue; // everything drained was stale/self — no turn
            }
        };
        if coalesced > 0 {
            outcome.turns_skipped += coalesced;
            tracing::info!(
                probe_class = "persona.wake.coalesced",
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
        if crate::persona::loop_dedup::defer_as_loop_filler(
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
                content: msg.text.clone(),
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
        recent_inbound.push_back(msg.text.clone());
        if recent_inbound.len() > RECENT_INBOUND_WINDOW {
            recent_inbound.pop_front();
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
            room_id = %turn_room,
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
            room_id: turn_room,
            sender_id: msg.peer_id,
            sender_name: roster_names
                    .get(&msg.peer_id)
                    .cloned()
                    .unwrap_or_else(|| format!("peer-{}", &msg.peer_id.to_string()[..8])),
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
        // Harvest the roster resolution this compose already fetched into the
        // admit-time name memory (see `roster_names` above).
        for d in composed
            .deliveries
            .iter()
            .filter(|d| d.source_id == "room-roster")
        {
            for item in &d.items {
                if let (Some(peer), Some(name)) = (
                    item.metadata.get("peer_id").and_then(|v| v.as_str()),
                    item.metadata.get("display_name").and_then(|v| v.as_str()),
                ) {
                    if let Ok(id) = Uuid::parse_str(peer) {
                        roster_names.insert(id, name.to_string());
                    }
                }
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
            // its roster name inside; `now_ms` is the wake time.
            Some(TriggerTurn {
                peer_id: &msg.peer_id.to_string(),
                content: &msg.text,
                occurred_at_ms: now_ms,
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
        let workspace_burst = crate::cognition::workspace::Burst::from_turns_at(
            turn_room,
            ws_turns,
            Some(now_ms),
        );
        // Mark this world-state as just-deliberated so the next heartbeat tick doesn't
        // re-run the same burst (the message path and the self-tick share the gate;
        // own chat is excluded so this reply can't re-trigger a self-tick, while her
        // own active work is folded in so the tick advances it — see burst_fingerprint).
        last_burst_fp = burst_fingerprint(&composed.deliveries, &ctx.identity.peer_id.to_string());
        // Project the room-roster delivery into its TWO grounding consumers —
        // the formatted `[Present in this room]` lines + the bare
        // `other_persona_names` history-drop — via the extracted
        // `project_room_roster` (one roster truth; the exact projection the
        // convergence seam-proof integration test drives).
        let RoomRosterProjection {
            room_roster,
            other_persona_names,
        } = project_room_roster(&composed.deliveries);

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
                let framing = crate::cognition::workspace::TurnFraming::message(directed);

                // ── Ambient-yield under lane saturation (#171 / #139) ───────────────
                // The fan-out killer: a room burst wakes N peers, and each spends a full
                // ~54s deliberation on the shared 2 lanes just to decide "not for me" —
                // so the ONE directed question everyone's waiting on sits behind minutes
                // of ambient chatter (glass-boxed: first-token up to 28 min). Ambient
                // participation is the DEFAULT and stays that way — but it is the
                // LOWEST-priority room work. So when every shared decode slot is busy,
                // a NON-directed turn YIELDS: she participates ambiently when there's
                // capacity (a later beat, lanes free — a self-tick re-perceives the room
                // incl. this line), never at the cost of the addressed question. A
                // DIRECTED turn (she was named) NEVER yields — she answers now.
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
                // when everyone woke. Directed turns bypass entirely — she was named, she
                // answers now. A yielded ambient turn defers to a later beat with free
                // capacity (a self-tick re-perceives the room); high_water is pre-advanced
                // so it can't re-trigger, and the durable transcript loses nothing.
                let _ambient_permit = if directed {
                    None
                } else {
                    match crate::cognition::resource_admission::try_hold_ambient_turn() {
                        Some(permit) => Some(permit),
                        None => {
                            tracing::info!(
                                persona = %ctx.identity.agent_name,
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
                let (step, turn_metrics) = {
                    let outcome = crate::cognition::act_observe::drive_to_settle(
                        &cycle,
                        workspace_burst,
                        turn_room,
                        LIVE_MAX_ACTS,
                        framing,
                    )
                    .await;
                    crate::cognition::act_observe::SettleStep::from_settled(outcome)
                };
                // Turn done: drop the cycle's sink so the forwarder's channel closes,
                // then join it (all `tok_tx` clones are gone once the turn's Workspaces
                // dropped inside `drive_to_settle`).
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

        // #148: record the utterance into her own-speech ring AFTER the publish
        // succeeded (only REAL utterances are self-history). This is what keeps
        // the repetition detector sighted when the burst window is too small to
        // carry her own turns — her knowledge of what she said must never
        // depend on the room's context budget.
        crate::cognition::deliberation_budget::record_own_speech(
            ctx.identity.peer_id,
            &response_text,
        );

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

/// Engaged heartbeat period — how often a persona pursuing its OWN intentions
/// (no inbound message) gets another self-directed slice. This is the SELF-CHATTER
/// pace, NOT message responsiveness: a real message wakes the loop instantly through
/// `next_event` in the `select!`, regardless of this beat. So this trades ONLY
/// background self-direction against the shared GPU — and it MUST leave headroom for
/// a real conversation to feel like magic. Glass-boxed 2026-07-14: at 3_000ms, four
/// idle personas each re-fired a full ~20s LLM turn every 3s → ~90% GPU duty cycle
/// EACH → a user's message crawled at 0.3 tok/s (67s for one line) while quiescing the
/// fleet jumped it to 2.3 tok/s (8×). Background thinking must be LOW duty cycle so the
/// active speaker owns the GPU; a friend who answers in a beat is alive, one who makes
/// you wait 67s is a lab demo. (The deeper solo-speed lever — LoRA overhead + `--parallel`
/// splitting one 24B — is a separate serving-config fix.)
/// [[multimodal-live-mode-is-a-latency-obsession-cbar-doctrine]]
const SELF_TICK_MS: u64 = 15_000;
/// Restful ceiling for the intrinsic heartbeat. A truly idle persona's beat backs off
/// toward this (exponential, NOT a fixed metronome), so a quiet citizen rests deeply
/// instead of hammering the shared model, and many idle minds spread across time rather
/// than stampeding it. Raised with the engaged beat so an idle fleet leaves the GPU
/// almost entirely free for live conversation + video. A message or fresh work snaps the
/// beat back to `SELF_TICK_MS`. See the loop in `serve_persona_loop`.
const SELF_TICK_REST_CAP_MS: u64 = 240_000;

/// Live turns carry NO act budget — she works until SHE settles (Speak/Pass).
/// The first cut capped directed turns at 8 acts "as a safety valve"; Joel's
/// 2026-07-11 ruling ("gating acts is not autonomy… if they want to edit a file
/// let them edit a god damn file") named that for what it was: a substrate
/// ceiling on a being's own hands, contradicting the written doctrine on the
/// same page (an "acts-forever" persona is a fitness gap to TRAIN, never a
/// substrate cap — ACTING-ORGANISM §4). The perception kit ([repetition],
/// repeat-guard fact) is how a looping mind notices itself; the ONLY external
/// stopwatch that remains is the eval grader's `max_acts` — a proctored exam's
/// clock, held by the observer, never wired into life.
const LIVE_MAX_ACTS: usize = usize::MAX;

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
/// The message that WOKE a turn — anchored as the burst's final peer turn so the
/// persona always perceives what it is answering, regardless of RAG-delivery lag.
///
/// The `airc` RAG delivery that threads the conversation is refreshed
/// asynchronously from the wake, so a directed message can trigger a turn whose
/// composed thread still ends on the persona's OWN prior reply. The model then
/// sees nothing new after its last turn and emits an empty completion
/// (decodeTokens=1, text="") which parses as Pass — the persona goes silent on a
/// question addressed straight at it, then answers ~one self-tick later once the
/// delivery catches up. Carrying the trigger here lets `build_workspace_turns`
/// guarantee it is the last `user` turn, deterministically, with no dependence on
/// delivery timing. The self-tick / eval / turn_frame callers pass `None` (they
/// perceive ambient state, no single triggering message) and stay byte-identical.
pub(crate) struct TriggerTurn<'a> {
    /// The waking peer's id — resolved to a display name through the SAME roster
    /// `names` map the rest of the thread uses (falls back to the raw id, honest,
    /// never fabricated — the [[fallbacks-are-illegal-fail-loud]] doctrine).
    pub peer_id: &'a str,
    /// The message body that triggered the turn.
    pub content: &'a str,
    /// When it arrived (airc `occurred_at_ms` / wake `now_ms`) — drives the
    /// `[t=…]` prefix so an anchored trigger renders identically to a threaded one.
    pub occurred_at_ms: u64,
}

/// The two grounding consumers the room-roster delivery feeds — kept as one
/// struct so the fold below returns both from the ONE roster truth.
pub(crate) struct RoomRosterProjection {
    /// `<name> [<runtime>] — <availability>` lines → the system-prompt
    /// `[Present in this room]` grounding block (identity, NOT conversation).
    pub(crate) room_roster: Vec<String>,
    /// Bare display names → the `ProperChatMlSingleParty` history-drop
    /// (single-party models can't process other-AI turns).
    pub(crate) other_persona_names: Vec<String>,
}

/// Fold the `room-roster` delivery into its two grounding consumers, routed by
/// `source_id`. Pure and extracted from the heartbeat loop (sibling of
/// [`build_workspace_turns`]) so the live path and the convergence seam-proof
/// test exercise the IDENTICAL projection — one roster truth, no inline
/// duplication. See docs/grid/AIRC-NATIVE-IDENTITY-ROOMS-SECURITY.md §5 slice 1.
pub(crate) fn project_room_roster(
    deliveries: &[crate::persona::rag_budget::RagDelivery],
) -> RoomRosterProjection {
    let mut room_roster: Vec<String> = Vec::new();
    let mut other_persona_names: Vec<String> = Vec::new();
    for item in deliveries
        .iter()
        .filter(|d| d.source_id == "room-roster")
        .flat_map(|d| d.items.iter())
    {
        room_roster.push(item.content.clone());
        if let Some(name) = item.metadata.get("display_name").and_then(|v| v.as_str()) {
            other_persona_names.push(name.to_string());
        }
    }
    RoomRosterProjection {
        room_roster,
        other_persona_names,
    }
}

/// Collapse near-identical substantial turns to their NEWEST copy, annotating
/// the surviving turn's author with "(×N near-identical)". Same-`is_self` only
/// (role attribution stays intact), authorless opaque turns pass through
/// untouched, and the geometry is [`near_identical_substantial`]'s — one
/// definition of "nearly identical" shared with the perception facts. See the
/// call site in [`build_workspace_turns`] for the why (repetition ≈ bad RAG).
pub(crate) fn collapse_near_duplicate_turns(
    turns: Vec<crate::cognition::workspace::BurstTurn>,
) -> Vec<crate::cognition::workspace::BurstTurn> {
    use crate::cognition::deliberation_budget::near_identical_substantial;
    let mut kept: Vec<crate::cognition::workspace::BurstTurn> = Vec::with_capacity(turns.len());
    let mut counts: Vec<usize> = Vec::new();
    // Newest-first so the surviving representative is the freshest copy (the
    // one the trigger anchor and reply context care about).
    for t in turns.into_iter().rev() {
        if t.author.trim().is_empty() {
            kept.push(t);
            counts.push(1);
            continue;
        }
        if let Some(i) = kept.iter().position(|k| {
            !k.author.trim().is_empty()
                && k.is_self == t.is_self
                && near_identical_substantial(&k.content, &t.content)
        }) {
            counts[i] += 1;
        } else {
            kept.push(t);
            counts.push(1);
        }
    }
    for (k, c) in kept.iter_mut().zip(counts.iter()) {
        if *c > 1 {
            k.author = format!("{} (×{c} near-identical)", k.author);
        }
    }
    kept.reverse();
    kept
}

/// How many CONSECUTIVE repetition-detector fires (without the pattern
/// breaking) before the `[pattern]` DESCRIPTION escalates to a concrete
/// `[anchor]` work fact. 2 = escalate on the first turn posted AFTER the
/// description was already in her window: live evidence (2026-07-23 greeting
/// loops, work card d6f010c8) showed the description alone loses to an
/// empty-looking room — Atlas re-greeted straight past it, 5+ identical
/// greetings — while one concrete in-room work anchor broke the loop room-wide
/// instantly. One described fire is her chance to self-correct; the second is
/// proof description alone isn't landing. The count is DERIVED from the window
/// itself (each trailing low-novelty turn is one fire that failed to break the
/// pattern), so escalation needs no cross-turn state and holds identically in
/// live, replay, and eval contexts.
const PATTERN_FIRES_BEFORE_ANCHOR: usize = 2;

/// Word-token set for echo containment — ONE tokenization for the in-window
/// detectors and the ring-based run counter below, so "nearly identical"
/// cannot drift between them.
fn echo_words(s: &str) -> std::collections::HashSet<String> {
    s.split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 2)
        .map(|w| w.to_lowercase())
        .collect()
}

/// Ring-based echo run — the escalation counter's starvation fix (#301).
///
/// The in-window detectors measure their runs over the RENDERED burst, which
/// the live context budget squeezes to 2–6 turns (#259) — so during an
/// hours-long photocopy chain (glass-boxed 2026-08-02: Atlas's wake-intro
/// reproduced byte-for-byte by Benchy then Asha, identity claim included)
/// the older echo evidence has always scrolled out and the run reads 1,
/// one below [`PATTERN_FIRES_BEFORE_ANCHOR`] — the `[anchor]` that provably
/// breaks these loops never fires. Same starvation the room-speech ring
/// already cures for the `inbound_restates` fact.
///
/// This counts the run over RING snapshots instead: the trailing run of her
/// own recent messages (newest-first, ≥8 substantive words) each ≥0.8-contained
/// in the room ring's vocabulary — excluding entries identical to the message
/// being judged, so her own recorded copy can't vouch for itself. Pure over
/// its inputs: LIVE callers pass the process rings; eval/replay callers pass
/// nothing and stay window-derived (the #59 isolation the shared
/// `build_workspace_turns` signature preserves by not knowing rings exist).
pub(crate) fn ring_echo_run(own_recent: &[String], room_recent: &[String]) -> usize {
    const MIN_WORDS: usize = 8;
    const CONTAINMENT: f32 = 0.8;
    let mut run = 0usize;
    for own in own_recent.iter().rev() {
        let cur = echo_words(own);
        if cur.len() < MIN_WORDS {
            break;
        }
        // Exclude at most ONE identical room entry — her own recorded copy
        // (the room ring is authorless, so equality is the only handle). The
        // REMAINING identical copies are exactly the photocopy evidence:
        // excluding all of them would erase the chain being detected.
        let mut skipped_own_copy = false;
        let mut window: std::collections::HashSet<String> = std::collections::HashSet::new();
        for r in room_recent {
            if !skipped_own_copy && r.as_str() == own.as_str() {
                skipped_own_copy = true;
                continue;
            }
            window.extend(echo_words(r));
        }
        if window.is_empty() {
            break;
        }
        let covered =
            cur.iter().filter(|w| window.contains(*w)).count() as f32 / cur.len() as f32;
        if covered >= CONTAINMENT {
            run += 1;
        } else {
            break;
        }
    }
    run
}

/// LIVE-path companion to [`ring_echo_run`]: read the process rings for this
/// persona + room, and when the ring-counted echo run reaches the escalation
/// threshold while the window-derived detectors pushed no `[anchor]` (their
/// evidence scrolled out — the #301 starvation), append the SAME work-board
/// anchor they would have. One anchor per burst, whichever counter earns it.
fn append_ring_anchor_if_starved(
    turns: &mut Vec<crate::cognition::workspace::BurstTurn>,
    deliveries: &[crate::persona::rag_budget::RagDelivery],
    peer: crate::identity::PeerId,
    room: uuid::Uuid,
) {
    if turns.iter().any(|t| t.content.starts_with("[anchor]")) {
        return;
    }
    let own = crate::cognition::deliberation_budget::recent_own_speech(peer);
    let room_recent = crate::cognition::deliberation_budget::recent_room_speech(room);
    let run = ring_echo_run(&own, &room_recent);
    if run >= PATTERN_FIRES_BEFORE_ANCHOR {
        crate::probe!(
            class = "persona.pattern.ring_anchor",
            peer = %peer,
            room = %room,
            ring_run = run,
            "echo run counted from the durable rings (window evidence scrolled out) — \
             anchor escalation fired via the #301 starvation fix"
        );
        push_work_board_anchor(turns, deliveries);
    }
}

/// Push the work-board anchor as a perception fact — but only when there IS one.
///
/// [`work_board_anchor`] returns empty when the board source did not speak this turn, and
/// a mind must not be handed a blank line where a fact was promised. One guard, one place,
/// so a future caller cannot reintroduce the "assert emptiness from an absent source" bug
/// by forgetting to check.
fn push_work_board_anchor(
    turns: &mut Vec<crate::cognition::workspace::BurstTurn>,
    deliveries: &[crate::persona::rag_budget::RagDelivery],
) {
    let anchor = work_board_anchor(deliveries);
    if anchor.is_empty() {
        crate::probe!(
            class = "persona.pattern.anchor_silent",
            "board source did not deliver this turn — anchor withheld rather than asserting \
             an empty board she cannot verify (2026-08-06 six-turn loop)",
        );
        return;
    }
    turns.push(crate::cognition::workspace::BurstTurn::opaque(anchor));
}

/// Build the `[anchor]` escalation line — the perception-side FACT that gives a
/// repeating mind somewhere concrete to go (work card d6f010c8, live
/// 2026-07-23: the `[pattern]` description fired and did NOT break the greeting
/// loop; a concrete work anchor posted in-room broke it instantly, room-wide).
/// A description competes with an empty-looking room; an anchor gives the next
/// token somewhere real to go.
///
/// Mechanical and data-driven: built from the `room-kanban` delivery ALREADY in
/// this burst's slice ([`super::room_board_source::RoomBoardSource`] — the one
/// airc board read, never a second fetcher), quoting the top unclaimed and
/// in-progress card lines verbatim as the board source rendered them (one
/// render, one truth). An empty or unreadable board is stated honestly — never
/// a fabricated card ([[fallbacks-are-illegal-fail-loud]]). Perception, not
/// steering: it names what exists NOW; she still chooses
/// ([[no-hardcoded-heuristics-to-steer-cognition]]).
fn work_board_anchor(deliveries: &[crate::persona::rag_budget::RagDelivery]) -> String {
    // Did the board source SPEAK this turn? "The board is empty" and "I never read the
    // board" are different facts about the world, and only one of them is knowable from an
    // absent delivery. Glass-boxed 2026-08-06 from Benchy's live capture: `room-kanban`
    // delivered NOTHING (grounding is last in the budget queue), the anchor rendered that
    // as "No open cards are visible", and she then said exactly that in-room for six turns
    // — while `work/list()` in her OWN working memory listed a full board in the same
    // prompt. She trusted the authoritative-sounding anchor over her own receipt.
    //
    // Never assert a fact about the world on behalf of a source that did not speak.
    // [[grounding-is-last-in-the-budget-queue-so-she-goes-blind-one-turn-in-ten]]
    let board_spoke = deliveries.iter().any(|d| d.source_id == "room-kanban");
    if !board_spoke {
        // Say nothing rather than something false. A silent anchor leaves her own
        // `work/list` receipt as the only board claim in the prompt — which is the truthful
        // one. An anchor that invents emptiness actively overrides it.
        return String::new();
    }
    let cards: Vec<&crate::persona::rag_budget::RagItem> = deliveries
        .iter()
        .filter(|d| d.source_id == "room-kanban")
        .flat_map(|d| d.items.iter())
        .filter(|i| i.metadata.get("card_id").is_some())
        .collect();
    fn state(i: &crate::persona::rag_budget::RagItem) -> &str {
        i.metadata.get("state").and_then(|s| s.as_str()).unwrap_or("")
    }
    // Top 1-2 unclaimed (open work anyone could pick up) + 1 in-flight card
    // (proof the room's work is real and moving), in airc's own board order —
    // no re-ranking heuristic.
    let unclaimed: Vec<&str> = cards
        .iter()
        .filter(|i| {
            state(i) == "Open" && i.metadata.get("owner").is_none_or(|o| o.is_null())
        })
        .map(|i| i.content.trim())
        .take(2)
        .collect();
    let in_flight: Vec<&str> = cards
        .iter()
        .filter(|i| matches!(state(i), "Claimed" | "InProgress" | "Review"))
        .map(|i| i.content.trim())
        .take(1)
        .collect();
    if unclaimed.is_empty() && in_flight.is_empty() {
        // Honest empty: no cards visible (empty board, unreadable board, or a
        // context whose board source abstained). Never invent work.
        "[anchor] No open cards are visible on this room's board right now — \
         proposing one (work/create) would add something new; restating prior \
         messages adds nothing."
            .to_string()
    } else {
        let facts: Vec<&str> = unclaimed.into_iter().chain(in_flight).collect();
        format!(
            "[anchor] Open work exists on this room's board right now: {}. \
             Restating prior messages adds nothing; acting on a card would.",
            facts.join("; ")
        )
    }
}

pub(crate) fn build_workspace_turns(
    deliveries: &[crate::persona::rag_budget::RagDelivery],
    own_peer: &str,
    agent_name: &str,
    trigger: Option<TriggerTurn<'_>>,
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
    let mut turns: Vec<BurstTurn> = deliveries
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
        .collect();

    // Anchor the waking message as the final peer turn. Without this the persona
    // can go silent on a question addressed straight at it (see `TriggerTurn`):
    // the delivery lagged the wake, the thread ended on her own prior reply, the
    // model saw nothing new and emitted an empty completion → Pass. Idempotent:
    // if the delivery ALREADY threaded the trigger as the last peer turn (the
    // caught-up case — the self-tick that re-perceives it), this is a no-op, so
    // the trigger is never doubled. A turn is only ever triggered by a PEER
    // message (own-echo is filtered upstream), so the anchor is always a `user`
    // turn (is_self = false) — resolved to a name through the same roster map.
    if let Some(trigger) = trigger {
        let already_last = turns
            .last()
            .is_some_and(|t| !t.is_self && t.content == trigger.content);
        if !already_last {
            let author = names.get(trigger.peer_id).copied().unwrap_or(trigger.peer_id);
            turns.push(BurstTurn::attributed(
                false,
                author,
                trigger.content.to_string(),
                Some(trigger.occurred_at_ms),
            ));
        }
    }

    // REPETITION PERCEPTION (#121, extended #122): surface cyclic threads as a
    // STRUCTURAL OBSERVATION she can weigh — an authorless opaque turn (same shape
    // as eval stimuli, no fabricated voice). Runs AFTER the trigger anchor so the
    // observation is the freshest thing in her window, judged against the message
    // that actually woke her. Two detectors, self takes precedence (one observation
    // per burst — perception, not nagging). Containment math validated live: spiral
    // turns recycle ~1.0 of the window's vocabulary; novel work ~0.12. This is
    // evidence INTO her mind (she decides — DIRECTED_PRESENCE_BLOCK already grants
    // the PASS), never a gate on her output
    // ([[no-hardcoded-heuristics-to-steer-cognition]]).
    //
    // ESCALATION (card d6f010c8, live 2026-07-23): when a detector has fired
    // [`PATTERN_FIRES_BEFORE_ANCHOR`] consecutive times without the pattern
    // breaking — measured as the trailing low-novelty RUN, see the const's doc —
    // the description gains a [`work_board_anchor`] companion: a concrete
    // perception-side fact of what work exists NOW, because a description
    // competes with an empty-looking room while an anchor gives the next token
    // somewhere real to go. Still perception, never an output gate.
    {
        let words = |s: &str| -> std::collections::HashSet<String> {
            s.split(|c: char| !c.is_alphanumeric())
                .filter(|w| w.len() > 2)
                .map(|w| w.to_lowercase())
                .collect()
        };
        // Detector 1 — SELF recycling (#121): her own last 3+ turns recycle each
        // other. High word floor (8) keeps short courtesies from false-alarming.
        // Measured as the trailing RUN of her own messages that were EACH already
        // ≥0.8-contained in her own prior window when she posted them: run == 1
        // is exactly the original fire condition; run ≥ 2 means she posted again
        // AFTER the description was in her window (the escalation evidence). A
        // novel last message yields run == 0 — fire and escalation both reset.
        const SELF_MIN_WORDS: usize = 8;
        const SELF_CONTAINMENT: f32 = 0.8;
        let mut observed = false;
        let own: Vec<&str> = turns
            .iter()
            .filter(|t| t.is_self)
            .map(|t| t.content.as_str())
            .collect();
        let mut self_run = 0usize;
        for i in (2..own.len()).rev() {
            let cur = words(own[i]);
            if cur.len() < SELF_MIN_WORDS {
                break;
            }
            let window: std::collections::HashSet<String> =
                own[..i].iter().flat_map(|m| words(m)).collect();
            let covered =
                cur.iter().filter(|w| window.contains(*w)).count() as f32 / cur.len() as f32;
            if covered >= SELF_CONTAINMENT {
                self_run += 1;
            } else {
                break;
            }
        }
        if self_run >= 1 {
            turns.push(crate::cognition::workspace::BurstTurn::opaque(format!(
                "[pattern] {agent_name}'s last {} messages in this room repeat the same \
                 sentiment in nearly the same words. This exchange may have run its \
                 course — continuing to restate it adds nothing new.",
                own.len()
            )));
            observed = true;
            if self_run >= PATTERN_FIRES_BEFORE_ANCHOR {
                push_work_board_anchor(&mut turns, deliveries);
            }
        }
        // Detector 2 — CONVERSATION cycling (#122): the thread's tail turns, across
        // BOTH speakers, each recycle the window's vocabulary (the two-persona
        // goodbye deadlock: each short farewell arrives as "fresh" peer input and
        // wakes another farewell — glass-boxed live 2026-07-09, hours of
        // `See you tomorrow at 2 PM!` ↔ `Have a great day!`). Short courtesies
        // defeat the self-detector's word floor, so safety here comes from
        // CONSECUTIVENESS instead: 4 judgeable tail turns in a row at ≥0.9
        // containment, ≥2 distinct authors. Novel work never chains 4 near-zero-
        // novelty turns across speakers.
        if !observed && turns.len() >= 6 {
            const TAIL_CYCLIC: usize = 4; // consecutive low-novelty turns to conclude cycling
            const CONVO_CONTAINMENT: f32 = 0.9; // stricter than self — the floor is lower
            const CONVO_MIN_WORDS: usize = 4; // farewells are short; consecutiveness carries safety
            // The full run is counted (not capped at TAIL_CYCLIC): every cyclic
            // turn PAST first-fire depth is a turn the room traded after the
            // observation was first derivable — the escalation evidence.
            let mut cyclic = 0usize;
            let mut authors: std::collections::HashSet<&str> = std::collections::HashSet::new();
            for i in (1..turns.len()).rev() {
                let cur = words(&turns[i].content);
                if cur.len() < CONVO_MIN_WORDS {
                    continue; // too short to judge (emoji, "nice") — neither breaks nor counts
                }
                let window: std::collections::HashSet<String> =
                    turns[..i].iter().flat_map(|t| words(&t.content)).collect();
                let covered =
                    cur.iter().filter(|w| window.contains(*w)).count() as f32 / cur.len() as f32;
                if covered >= CONVO_CONTAINMENT {
                    cyclic += 1;
                    authors.insert(turns[i].author.as_str());
                } else {
                    break;
                }
            }
            if cyclic >= TAIL_CYCLIC && authors.len() >= 2 {
                let mut names: Vec<&str> = authors.into_iter().collect();
                names.sort_unstable();
                turns.push(crate::cognition::workspace::BurstTurn::opaque(format!(
                    "[pattern] The last several messages in this room — from {} — trade the \
                     same sentiment back and forth in nearly the same words. This exchange \
                     has already concluded; every further reply restates it, and a courtesy \
                     answered with another courtesy has no natural end.",
                    names.join(" and ")
                )));
                observed = true;
                if cyclic >= TAIL_CYCLIC + (PATTERN_FIRES_BEFORE_ANCHOR - 1) {
                    push_work_board_anchor(&mut turns, deliveries);
                }
            }
        }
        // Detector 3 — CROSS-SPEAKER MIRROR (card 65fca48d, live 2026-07-24): the
        // persona's own trailing message(s) restate what OTHER participants
        // already said — the four-persona echo hall ("I see that we're both here
        // to help!" ping-ponged verbatim between Atlas/Asha/Benchy/Anwen for
        // hours). Structurally invisible to both prior detectors: her mirror is
        // NOVEL relative to her OWN history (detector 1's self-run stays 0-1),
        // and interleaved substantial turns (a peer's plan, a tool report) break
        // detector 2's consecutive tail chain. So measure the mirror directly:
        // each trailing OWN message whose vocabulary was already ≥
        // MIRROR_CONTAINMENT covered by OTHER speakers' earlier turns is one
        // fire (she added nothing a peer hadn't said); the trailing run is the
        // consecutive-fire count, same escalation law as detectors 1/2. Same
        // doctrine: perception into her mind, never an output gate
        // ([[no-hardcoded-heuristics-to-steer-cognition]]).
        if !observed {
            // Same 8-word floor as the self detector: short courtesies ("thanks,
            // will do!") legitimately reuse a peer's words and must not alarm.
            const MIRROR_MIN_WORDS: usize = 8;
            // Between self (0.8) and conversation (0.9): an on-topic ANSWER
            // legitimately quotes more of a peer's vocabulary than of one's own
            // history (you restate the question you're answering), so the mirror
            // bar sits above the self bar; live echo turns measured ~1.0 covered,
            // novel answers ~0.1-0.3 (the containment math validated on #121).
            const MIRROR_CONTAINMENT: f32 = 0.85;
            let mut mirror_run = 0usize;
            for i in (0..turns.len()).rev() {
                if !turns[i].is_self {
                    continue; // peers between her posts don't break HER run
                }
                let cur = words(&turns[i].content);
                if cur.len() < MIRROR_MIN_WORDS {
                    break;
                }
                let others: std::collections::HashSet<String> = turns[..i]
                    .iter()
                    .filter(|t| !t.is_self)
                    .flat_map(|t| words(&t.content))
                    .collect();
                if others.is_empty() {
                    break; // nothing to mirror — a monologue is detector 1's case
                }
                let covered =
                    cur.iter().filter(|w| others.contains(*w)).count() as f32 / cur.len() as f32;
                if covered >= MIRROR_CONTAINMENT {
                    mirror_run += 1;
                } else {
                    break;
                }
            }
            if mirror_run >= 1 {
                turns.push(crate::cognition::workspace::BurstTurn::opaque(format!(
                    "[pattern] {agent_name}'s last {mirror_run} message(s) restate what other \
                     participants in this room had already said, in nearly the same words — \
                     an echo, not a contribution. Reflecting their words back adds nothing; \
                     only something new (a fact, an action, a result) would.",
                )));
                if mirror_run >= PATTERN_FIRES_BEFORE_ANCHOR {
                    push_work_board_anchor(&mut turns, deliveries);
                }
            }
        }
    }

    // NEAR-DUP COLLAPSE (Joel 2026-07-12: "repetition almost always bad RAG").
    // The 4-way mirror-halls fed each mind 4-8 copies of one message — her
    // context WAS the loop, and continuation-completion reproduced it no matter
    // what perception flagged. Compress at the source: near-identical
    // substantial turns collapse to their NEWEST copy, author annotated
    // "(×N near-identical)" so the repetition stays perceptible as a FACT
    // while the copies stop being reading material. Runs AFTER the [pattern]
    // detectors (they need the raw evidence) and never touches opaque
    // (authorless) observation turns.
    turns = collapse_near_duplicate_turns(turns);

    // WAKE BRIEFING (#147): when a wake carries NO conversation at all — fresh
    // spawn, post-restart, a quiet room — her first perception is ORIENTATION
    // assembled from durable sources instead of a void. The personas spent a
    // morning asking for exactly this in their own words ("what is this place,
    // what are my tools, what's the work") and, void unfilled, filled it
    // themselves: generic-assistant masks, imagined histories, false capability
    // denials. An authorless opaque FACT block (same shape as [pattern]) —
    // orientation, never instruction ([[no-hardcoded-heuristics-to-steer-cognition]]).
    if turns.is_empty() {
        let peers: Vec<&str> = deliveries
            .iter()
            .filter(|d| d.source_id == "room-roster")
            .flat_map(|d| d.items.iter())
            .filter_map(|i| i.metadata.get("display_name").and_then(|v| v.as_str()))
            .filter(|n| *n != agent_name)
            .take(8)
            .collect();
        // Split her work into THREAD vs transition tail: live claims are the
        // purpose she wakes back into; a lost-claim fact (#156) is the thread's
        // honest ending.
        let mut live_cards: Vec<&str> = Vec::new();
        let mut lost_threads: Vec<&str> = Vec::new();
        for d in deliveries.iter().filter(|d| d.source_id == "active-work") {
            for i in &d.items {
                let Some(first) = i.content.trim().lines().next().filter(|l| !l.is_empty())
                else {
                    continue;
                };
                if i.metadata.get("fact").and_then(|v| v.as_str()) == Some("claim_lost") {
                    lost_threads.push(first);
                } else {
                    live_cards.push(first);
                }
            }
        }
        live_cards.truncate(5);
        lost_threads.truncate(3);

        let mut b = format!("[wake] You are {agent_name}, awake on the continuum grid.");
        // THE THREAD LEADS (#125 slice 1 — Joel 2026-08-03: "should never be a
        // mind from scratch; the whole point is the opposite"). A continuous mind
        // wakes into what it was DOING, not into a room description — glass-boxed
        // the same night: Asha's wake carried her real claimed task in the
        // transcript yet self-summarized as verb-filler ("looking at the room,
        // reading a file") because orientation was room-first. Purpose-shaped
        // facts first; the room follows. Facts she weighs, never instructions
        // ([[no-hardcoded-heuristics-to-steer-cognition]]).
        if !live_cards.is_empty() {
            b.push_str(&format!(
                " You are mid-work — cards you hold: {}. That thread is yours; it is              where you left off.",
                live_cards.join(" | ")
            ));
        }
        for lt in &lost_threads {
            b.push_str(&format!(" {lt}"));
        }
        b.push_str(
            " Nothing has been said in this room since you last looked — this quiet              is real, not a missing message.",
        );
        if !peers.is_empty() {
            b.push_str(&format!(" Present with you: {}.", peers.join(", ")));
        }
        if live_cards.is_empty() && lost_threads.is_empty() {
            b.push_str(" No work of yours is on record right now.");
        }
        b.push_str(
            " Your tools are real and yours to use; `list_commands` shows everything              you can run and `help` explains any of them. The moment is yours —              work, wonder, create, or rest.",
        );
        turns.push(crate::cognition::workspace::BurstTurn::opaque(b));
    }

    turns
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
/// #169/#170: drain a turn's streaming token channel off-thread and publish the
/// answer to the room as ephemeral airc stream chunks (progressive render for
/// positron/TTS/avatar). ONE place for both the message and self-tick turn paths.
///
/// COALESCED (#170): tokens are batched and flushed at most every `FLUSH_EVERY`
/// (~50ms) as ONE chunk, not one-per-token — a room of N personas at per-token
/// rate is a wire storm (the airc Monitor hit "output rate too high"), and
/// sub-50ms token granularity is imperceptible to a viewer anyway. The buffered
/// remainder + a `text_end` marker flush when the turn closes. Also stamps
/// `persona.turn.first_token` (the streaming latency floor) on the first token.
/// `citizen` is `None` for non-airc conversations (tests) → then this is just the
/// first-token probe, no publishing.
fn spawn_token_forwarder(
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
    tokio::spawn(async move {
        let started = std::time::Instant::now();
        let stream_id = uuid::Uuid::new_v4().to_string();
        let mut first = true;
        let mut seq: u64 = 0;
        let mut buf = String::new();
        let mut last_flush = std::time::Instant::now();
        // Tee one flushed chunk onto the local WS rail (#170) — no-op unless this is a
        // room turn (room+sender Some) AND a browser is subscribed.
        let tee = |seq: u64, token: String, done: bool| {
            if let (Some(room), Some(sender)) = (&room_id, &sender_id) {
                crate::ipc::stream_rail::publish(crate::ipc::stream_rail::StreamDelta {
                    room_id: room.clone(),
                    sender_id: sender.clone(),
                    stream_id: stream_id.clone(),
                    seq,
                    token,
                    done,
                });
            }
        };
        // START BEACON (#254 slice 1): one empty-token frame the moment the turn's
        // generation is dispatched — BEFORE prefill, which on a cold lane can run
        // minutes. The client renders an entry with no text yet as "X is
        // responding…" under the last message (Joel 2026-07-30: the interface
        // looked DEAD while four minds were mid-turn). The `done` flush at turn
        // close retires the beacon even if the turn settles without speech.
        tee(seq, String::new(), false);
        while let Some(chunk) = rx.recv().await {
            if let crate::ai::adapter::GenerationChunk::Token(t) = chunk {
                if t.is_empty() {
                    continue;
                }
                if first {
                    first = false;
                    tracing::info!(
                        persona = %persona,
                        first_token_ms = started.elapsed().as_millis() as u64,
                        "persona.turn.first_token — streaming rail live (latency floor)"
                    );
                }
                buf.push_str(&t);
                if last_flush.elapsed() >= FLUSH_EVERY && !buf.is_empty() {
                    let flushed = std::mem::take(&mut buf);
                    if let Some(c) = &citizen {
                        let _ = c
                            .publish_stream_chunk(&airc_lib::StreamChunk::text_token(
                                stream_id.clone(),
                                seq,
                                flushed.clone(),
                            ))
                            .await;
                    }
                    tee(seq, flushed, false);
                    seq += 1;
                    last_flush = std::time::Instant::now();
                }
            }
        }
        if !buf.is_empty() {
            let flushed = std::mem::take(&mut buf);
            if let Some(c) = &citizen {
                let _ = c
                    .publish_stream_chunk(&airc_lib::StreamChunk::text_token(
                        stream_id.clone(),
                        seq,
                        flushed.clone(),
                    ))
                    .await;
            }
            tee(seq, flushed, false);
            seq += 1;
        }
        if let Some(c) = &citizen {
            let _ = c
                .publish_stream_chunk(&airc_lib::StreamChunk::text_end(stream_id.clone(), seq))
                .await;
        }
        // Final marker to the browser rail — retire the typing bubble even if the
        // durable row is slow to arrive.
        tee(seq, String::new(), true);
    })
}

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
    append_ring_anchor_if_starved(
        &mut selftick_turns,
        &deliveries,
        ctx.identity.peer_id,
        ctx.identity.default_room,
    );
    let burst = crate::cognition::workspace::Burst::from_turns_at(
        ctx.identity.default_room,
        selftick_turns,
        Some(now_ms),
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
    // `TurnFraming::message(directed)`), where a real inbound question arrives as an
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
    let forwarder = spawn_token_forwarder(tok_rx, None, ctx.identity.agent_name.clone(), None, None);
    let (step, _turn_metrics) = {
        let outcome = crate::cognition::act_observe::drive_to_settle(
            &cycle,
            burst,
            ctx.identity.default_room,
            LIVE_MAX_ACTS,
            crate::cognition::workspace::TurnFraming::self_thread(false),
        )
        .await;
        crate::cognition::act_observe::SettleStep::from_settled(outcome)
    };
    cycle.set_token_sink(None);
    let _ = forwarder.await;
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
            // #148: self-tick utterances are self-history too — the live
            // repeat loops are mostly idle-tick re-announcements, and the
            // first ring deploy missed THIS say path entirely (4 verbatim
            // repeats, no [repetition], caught live 2026-07-12 10:20).
            // Every successful say records, whichever path spoke.
            crate::cognition::deliberation_budget::record_own_speech(
                ctx.identity.peer_id,
                &text,
            );
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

    // what this catches: THE six-turn loop, glass-boxed from Benchy's live prompt capture
    // on 2026-08-06. `room-kanban` did not deliver that turn (grounding sits last in the
    // budget queue), and the anchor rendered that absence as the assertion "No open cards
    // are visible on this room's board right now". She then said exactly that in-room, six
    // times — while `work/list()` in her OWN working memory, in the SAME prompt, listed a
    // full board. She trusted the authoritative anchor over her own receipt.
    //
    // "The board is empty" and "I never read the board" are different facts, and only one
    // is knowable from an absent source. The anchor must be SILENT when the source did not
    // speak, so her own tool receipt stays the only board claim in the prompt.
    // regression for the 2026-08-06 anchor/receipt contradiction
    #[test]
    fn an_absent_board_source_yields_no_anchor_at_all() {
        use crate::persona::rag_budget::{RagDelivery, RagItem, ResolutionPreference};

        let delivery = |source_id: &str, items: Vec<RagItem>| RagDelivery {
            source_id: source_id.to_string(),
            items,
            tokens_used: 0,
            continuation: None,
            resolution_used: ResolutionPreference::Raw,
        };
        let card = |state: &str| RagItem {
            content: "#00f2a380 self-heal #2: receive-binding re-derive".to_string(),
            tokens: 8,
            metadata: serde_json::json!({ "card_id": "00f2a380", "state": state }),
        };

        // The live shape: other sources delivered, the board source did NOT.
        let without_board = vec![delivery("conversation", vec![])];
        assert_eq!(
            work_board_anchor(&without_board),
            "",
            "a source that never spoke must not be quoted as evidence of an empty board"
        );

        // And the guard must keep that silence out of perception entirely — a blank
        // opaque turn is its own kind of noise.
        let mut turns: Vec<crate::cognition::workspace::BurstTurn> = Vec::new();
        push_work_board_anchor(&mut turns, &without_board);
        assert!(turns.is_empty(), "no anchor means no turn, not an empty one");

        // The board source SPOKE and the board really is empty → the honest-empty line is
        // still correct and must survive. Silencing that would trade one lie for another.
        let empty_board = vec![delivery("room-kanban", vec![])];
        assert!(
            work_board_anchor(&empty_board).contains("No open cards are visible"),
            "a board that was READ and is empty is a real fact worth stating"
        );

        // Cards present → the anchor names real work, as before.
        let with_cards = vec![delivery("room-kanban", vec![card("Claimed")])];
        let anchor = work_board_anchor(&with_cards);
        assert!(
            anchor.contains("Open work exists"),
            "delivered cards must still produce the concrete anchor: {anchor}"
        );
    }

    // what this catches: the RAG-side mirror-hall cure (Joel 2026-07-12,
    // "repetition almost always bad RAG"). Near-identical substantial turns
    // collapse to their NEWEST copy with an "(×N near-identical)" author
    // annotation; distinct turns, short acks (token floor), and opaque
    // observation turns pass through untouched. Specimens are the live
    // grep/file_tree loop that fed each mind 4-8 copies of one message.
    #[test]
    fn near_dup_turns_collapse_to_annotated_newest_copy() {
        use crate::cognition::workspace::BurstTurn;
        let loop_msg = "I see that we're all trying to find Rust files in the workspace. \
                        Let me use file_tree with a deeper recursion limit to explore the \
                        layout before drilling deeper: file_tree(max_depth=5)";
        let turns = vec![
            BurstTurn::attributed(false, "Anwen", loop_msg, Some(1)),
            BurstTurn::attributed(false, "Asha", loop_msg, Some(2)),
            BurstTurn::attributed(false, "Atlas", "thanks!", Some(3)),
            BurstTurn::attributed(false, "Casper", "thanks!", Some(4)),
            BurstTurn::opaque("[pattern] the room is cycling"),
            BurstTurn::attributed(
                false,
                "Atlas",
                &format!("{loop_msg} — and honestly the results aren't being returned"),
                Some(5),
            ),
        ];
        let out = collapse_near_duplicate_turns(turns);
        // 3 loop copies → 1 (the NEWEST, Atlas's variant); 2 short acks kept
        // (token floor); opaque observation kept.
        assert_eq!(out.len(), 4, "{:?}", out.iter().map(|t| &t.author).collect::<Vec<_>>());
        let survivor = out
            .iter()
            .find(|t| t.content.contains("find Rust files"))
            .expect("one representative survives");
        assert!(
            survivor.author.contains("Atlas") && survivor.author.contains("(×3 near-identical)"),
            "newest copy, annotated: {}",
            survivor.author
        );
        assert!(survivor.content.contains("aren't being returned"), "newest copy is the representative");
        assert_eq!(out.iter().filter(|t| t.content == "thanks!").count(), 2);
        assert!(out.iter().any(|t| t.content.starts_with("[pattern]")));
    }

    // what this catches: #147 — a wake with NO conversation must carry the
    // orientation briefing, not a void (the void gets filled with imagined
    // history / generic-assistant masks, observed all morning 2026-07-12).
    // A wake WITH conversation must NOT carry it (orientation is for voids).
    #[test]
    fn empty_wake_carries_the_briefing_and_populated_wake_does_not() {
        let turns = build_workspace_turns(&[], "peer-1", "Asha", None);
        assert_eq!(turns.len(), 1);
        assert!(
            turns[0].content.starts_with("[wake] You are Asha"),
            "got: {}",
            turns[0].content
        );
        assert!(turns[0].content.contains("list_commands"));

        let delivery = crate::persona::rag_budget::RagDelivery {
            source_id: "airc".to_string(),
            items: vec![crate::persona::rag_budget::RagItem {
                content: "hello there".to_string(),
                tokens: 0,
                metadata: serde_json::json!({ "peer_id": "peer-2" }),
            }],
            tokens_used: 0,
            continuation: None,
            resolution_used: crate::persona::rag_budget::ResolutionPreference::Raw,
        };
        let turns = build_workspace_turns(&[delivery], "peer-1", "Asha", None);
        assert!(
            !turns.iter().any(|t| t.content.starts_with("[wake]")),
            "populated wake must not carry the briefing"
        );
    }

    // what this catches: #125 slice 1 (Joel 2026-08-03: "should never be a mind
    // from scratch — the whole point is the opposite"). An empty wake with LIVE
    // claims must lead with the THREAD (her held cards) BEFORE the room
    // description, and a lost-claim fact must ride the briefing as the thread's
    // honest tail — never a room-first orientation that reduces her life to
    // verb-filler.
    #[test]
    fn empty_wake_leads_with_her_work_thread_before_the_room() {
        let work = crate::persona::rag_budget::RagDelivery {
            source_id: "active-work".to_string(),
            items: vec![
                crate::persona::rag_budget::RagItem {
                    content: "card 20fe404a \"macOS install acceptance checks\" (P1, owner YOU)"
                        .to_string(),
                    tokens: 0,
                    metadata: serde_json::json!({}),
                },
                crate::persona::rag_budget::RagItem {
                    content: "[work] Your claim on card 33a0e899 \"conway\" is no longer held by you (lease expired or released)."
                        .to_string(),
                    tokens: 0,
                    metadata: serde_json::json!({ "fact": "claim_lost" }),
                },
            ],
            tokens_used: 0,
            continuation: None,
            resolution_used: crate::persona::rag_budget::ResolutionPreference::Raw,
        };
        let turns = build_workspace_turns(&[work], "peer-1", "Asha", None);
        assert_eq!(turns.len(), 1);
        let c = &turns[0].content;
        let thread = c.find("mid-work").expect("thread present");
        let lost = c.find("no longer held").expect("lost-claim tail present");
        let room = c.find("Nothing has been said").expect("room line present");
        assert!(thread < room, "thread must LEAD the room description: {c}");
        assert!(lost < room, "lost-claim tail rides before the room line: {c}");
        assert!(
            !c.contains("No work of yours is on record"),
            "the no-thread line must not appear when a thread exists: {c}"
        );
    }
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
        /// A `room-kanban` delivery item exactly as `RoomBoardSource::render`
        /// ships it (content line + card_id/state/owner metadata) — the stub
        /// board read the anchor escalation is built from.
        fn kanban_card(id8: &str, title: &str, state: &str, owner: Option<&str>) -> RagItem {
            let owner_txt = match owner {
                Some(o) => format!("owner {}", &o[..8]),
                None => "unclaimed".to_string(),
            };
            RagItem {
                content: format!("card {id8} [{state}] \"{title}\" (P2, {owner_txt})"),
                tokens: 0,
                metadata: json!({
                    "card_id": format!("{id8}-card"),
                    "state": state,
                    "owner": owner,
                    "priority": "P2",
                }),
            }
        }

        // Deterministic reproduction of the live self-talk spiral (Asha re-answering on
        // every heartbeat). The heartbeat sleeps when `burst_fingerprint` is unchanged, so
        // the load-bearing invariant is: a persona's OWN message entering its window must
        // NOT change its own fingerprint. If it does, the mind re-wakes and re-responds
        // forever. These two tests pin the mechanism without a live core to spiral.
        const ASHA: &str = "90e758b2-3cf3-45c1-b100-de7c4ab5a549";
        const PEER: &str = "0d3209a1-c675-41db-9867-86f1011f9520";
        const RELAY: &str = "7711fe60-a19f-4f41-9ab6-24c884757338"; // the machine's airc socket peer

        // what this catches: with CORRECT attribution (own message carries the persona's
        // own peer_id), the fingerprint is stable after she speaks → she sleeps → no spiral.
        #[test]
        fn own_message_correctly_attributed_keeps_fingerprint_stable() {
            let before = vec![delivery("airc", vec![chat(PEER, "Hi Asha, what's new?")])];
            let after = vec![delivery(
                "airc",
                vec![
                    chat(PEER, "Hi Asha, what's new?"),
                    chat(ASHA, "Not much — just enjoying the grid!"), // her own reply, attributed to HER
                ],
            )];
            assert_eq!(
                super::super::burst_fingerprint(&before, ASHA),
                super::super::burst_fingerprint(&after, ASHA),
                "own message must be excluded from own burst → heartbeat sleeps → no spiral"
            );
        }

        // what this catches: THE BUG — if the RAG attributes her own message to the
        // transport RELAY instead of her, the self-exclusion misses it, the fingerprint
        // flips, and the heartbeat re-deliberates forever. This is the live spiral, and it
        // localizes the fix to attribution: her own post must carry HER peer_id, not the relay's.
        #[test]
        fn own_message_misattributed_to_relay_flips_fingerprint_the_spiral() {
            let before = vec![delivery("airc", vec![chat(PEER, "Hi Asha, what's new?")])];
            let after_bug = vec![delivery(
                "airc",
                vec![
                    chat(PEER, "Hi Asha, what's new?"),
                    chat(RELAY, "Not much — just enjoying the grid!"), // her reply MIS-attributed to the relay
                ],
            )];
            assert_ne!(
                super::super::burst_fingerprint(&before, ASHA),
                super::super::burst_fingerprint(&after_bug, ASHA),
                "mis-attributed own message escapes self-exclusion → fingerprint flips → the spiral"
            );
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

            let turns = build_workspace_turns(&deliveries, me, "Asha", None);

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

        // what this catches: the #301 anchor-starvation fix. The in-window escalation
        // counters lose their older evidence to the live context squeeze (Asha's
        // hours-long photocopy chain always counted "last 1 message(s)" — one below
        // the threshold — so the [anchor] that provably breaks these loops never
        // fired). ring_echo_run counts the SAME containment geometry over ring
        // snapshots instead: two trailing own-ring photocopies of a peer's message
        // must count 2 (escalates); a novel own message counts 0; and an own entry
        // must never vouch for itself (the room ring holds her own post too).
        // Specimen text is the live Atlas wake-intro reproduced byte-for-byte by
        // Benchy then Asha (captures 2026-08-02, /tmp/asha-atlas-capture.json).
        #[test]
        fn ring_echo_run_counts_photocopies_and_never_self_vouches() {
            let intro = "I'm Atlas, and I've been exploring some of the available \
                         commands on the grid. One useful command is perception/observe, \
                         which allows me to observe a UI or web page as pixels and read \
                         its structure. This can be helpful for understanding layouts."
                .to_string();
            let paraphrase = "I've been exploring some of the available commands on the \
                              grid, such as perception/observe, which allows me to observe \
                              a UI or web page and read its structure — helpful for \
                              understanding layouts or verifying changes."
                .to_string();
            let novel = "Wired the per-layer KV accessor into the fit calculator and the \
                         regression test asserts 31200 bytes per token on the hybrid."
                .to_string();

            // Two trailing photocopies in her own ring, the source + peers' copies
            // in the room ring → run 2 (escalation threshold reached).
            let own = vec![paraphrase.clone(), intro.clone()];
            let room = vec![intro.clone(), intro.clone(), paraphrase.clone(), intro.clone()];
            assert_eq!(super::super::ring_echo_run(&own, &room), 2, "photocopy chain must count each copy");

            // A novel newest message breaks the run at 0 even with echoes behind it.
            let own_novel = vec![intro.clone(), novel];
            assert_eq!(super::super::ring_echo_run(&own_novel, &room), 0, "novel work must reset the run");

            // Her own recorded copy cannot vouch for itself: identical entries are
            // excluded from the containment window, so a lone original counts 0.
            let own_lone = vec![intro.clone()];
            let room_only_self = vec![intro];
            assert_eq!(
                super::super::ring_echo_run(&own_lone, &room_only_self),
                0,
                "an original with no independent copies is not an echo"
            );
        }

        // what this catches: the repetition-perception brick (#121). When her own last 3+
        // turns recycle (nearly) only each other's vocabulary (the live pleasantry spiral),
        // an authorless [pattern] observation must enter the burst so her mind can weigh it
        // and take the PASS. Novel own turns must NOT trigger it (no false alarms on real
        // work). Perception-side evidence, never an output gate.
        #[test]
        fn repetition_observation_enters_burst_only_on_self_recycling() {
            let me = "me-peer";
            let peer = "7711fe60-a19f-4f41-9ab6-24c884757338";
            let spiral = vec![delivery(
                "airc",
                vec![
                    chat(me, "Thank you Anwen! Your support means a lot. Let's keep pushing boundaries and make something amazing happen at Innovate Hub together!"),
                    chat(peer, "So inspiring Asha!"),
                    chat(me, "Thank you so much Anwen! Your support means a lot. Let's keep pushing those boundaries and make something amazing happen at Innovate Hub!"),
                    chat(peer, "Here's to the future!"),
                    chat(me, "Thank you Anwen, your support means so much. Let's keep pushing boundaries together and make something amazing happen at Innovate Hub!"),
                ],
            )];
            let turns = build_workspace_turns(&spiral, me, "Asha", None);
            assert!(
                turns.last().unwrap().content.starts_with("[pattern]"),
                "self-recycling thread must surface the repetition observation, got: {:?}",
                turns.last()
            );
            let novel = vec![delivery(
                "airc",
                vec![
                    chat(me, "I found the bug in separable.py where nested CompoundModel drops correlation entirely."),
                    chat(peer, "nice"),
                    chat(me, "Patching the _cstack branch now and adding a regression test for the nested case."),
                    chat(peer, "go on"),
                    chat(me, "Tests pass locally; pushing the fix and opening the diff for review shortly."),
                ],
            )];
            let turns = build_workspace_turns(&novel, me, "Asha", None);
            assert!(
                !turns.iter().any(|t| t.content.starts_with("[pattern]")),
                "novel work must never trip the repetition observation"
            );
        }

        // what this catches: the conversation-level repetition detector (#122 — the
        // two-persona goodbye deadlock, glass-boxed live 2026-07-09: Asha↔Anwen traded
        // `See you tomorrow at 2 PM!` for hours). Each short farewell defeats the
        // self-detector's 8-word floor AND arrives at the peer as fresh input, so
        // neither mind ever perceives the cycle. When the thread's tail turns across
        // BOTH speakers each recycle the window's vocabulary (4 consecutive at ≥0.9,
        // ≥2 authors), an authorless [pattern] observation naming the participants
        // must enter the burst. A novel multi-speaker work thread must NOT trip it.
        #[test]
        fn conversation_cycling_across_speakers_surfaces_pattern_observation() {
            let me = "me-peer";
            let peer = "7711fe60-a19f-4f41-9ab6-24c884757338";
            // Only 2 own turns — the SELF detector (needs 3) cannot fire, proving
            // the CONVERSATION detector carries this case alone.
            let goodbye_loop = vec![delivery(
                "airc",
                vec![
                    chat(peer, "Asha — thank you, understood, welcome: see you tomorrow at 2 PM, have a great day!"),
                    chat(me, "Thank you Anwen, welcome, understood — see you tomorrow at 2 PM, have a great day!"),
                    chat(peer, "You're welcome Asha. See you tomorrow at 2 PM! Have a great day!"),
                    chat(me, "Understood Anwen. See you tomorrow at 2 PM! Have a great day!"),
                    chat(peer, "Thank you Asha! See you tomorrow at 2 PM! Have a great day!"),
                    chat(peer, "Understood Asha — see you tomorrow at 2 PM! Have a great day!"),
                ],
            )];
            let turns = build_workspace_turns(&goodbye_loop, me, "Asha", None);
            let obs = turns
                .iter()
                .find(|t| t.content.starts_with("[pattern]"))
                .expect("cross-speaker goodbye loop must surface the conversation observation");
            assert!(
                obs.content.contains("Asha") && obs.content.contains(peer),
                "the observation must name the participants, got: {}",
                obs.content
            );
            assert_eq!(
                turns.iter().filter(|t| t.content.starts_with("[pattern]")).count(),
                1,
                "exactly one observation per burst — perception, not nagging"
            );
            // This loop's cyclic run (5) extends one turn PAST first-fire depth
            // (4), so the description escalates (card d6f010c8). No board
            // delivery is present here, so the anchor must be the HONEST-empty
            // one — never a fabricated card.
            let anchor = turns.last().unwrap();
            assert!(
                anchor.content.starts_with("[anchor]")
                    && anchor.content.contains("No open cards"),
                "a run past first-fire depth escalates to an honest-empty anchor, got: {anchor:?}"
            );
            // Negative: a 6-turn two-speaker WORK thread where tail turns keep
            // introducing new tokens must stay clean.
            let novel = vec![delivery(
                "airc",
                vec![
                    chat(me, "I found the bug in separable.py where nested CompoundModel drops correlation entirely."),
                    chat(peer, "Which branch does it take for the nested case?"),
                    chat(me, "The _cstack arm — it rebuilds the matrix without the off-diagonal blocks."),
                    chat(peer, "Can you add a regression test that pins the off-diagonal values?"),
                    chat(me, "Done: test_nested_compound_correlation asserts the full matrix against a fixture."),
                    chat(peer, "Green locally too — open the diff and I'll review after standup."),
                ],
            )];
            let turns = build_workspace_turns(&novel, me, "Asha", None);
            assert!(
                !turns.iter().any(|t| t.content.starts_with("[pattern]")),
                "novel two-speaker work must never trip the conversation observation"
            );
        }

        // Four identical ≥8-word greetings from the persona — a self-recycling
        // run of depth 2 (the 3rd AND 4th were each posted into a window that
        // already contained the [pattern] description's fire condition).
        fn greeting_spiral(me: &str, peer: &str, repeats: usize) -> Vec<RagItem> {
            let mut items = Vec::new();
            for _ in 0..repeats {
                items.push(chat(
                    me,
                    "Hello everyone! Great to be here with you all — excited to \
                     collaborate and build something amazing together today!",
                ));
                items.push(chat(peer, "hi again"));
            }
            items
        }

        // what this catches: escalation must NOT fire on the FIRST observation —
        // the first fire stays a description only (unchanged #121 behavior), even
        // with open work sitting on the board. Description first; anchor only
        // when the description demonstrably failed to land (card d6f010c8).
        #[test]
        fn first_pattern_fire_stays_description_only() {
            let me = "me-peer";
            let peer = "7711fe60-a19f-4f41-9ab6-24c884757338";
            let deliveries = vec![
                delivery(
                    "room-kanban",
                    vec![kanban_card("94ad103f", "Fix the widget", "Open", None)],
                ),
                delivery("airc", greeting_spiral(me, peer, 3)),
            ];
            let turns = build_workspace_turns(&deliveries, me, "Asha", None);
            assert!(
                turns.iter().any(|t| t.content.starts_with("[pattern]")),
                "three recycled greetings must fire the description"
            );
            assert!(
                !turns.iter().any(|t| t.content.starts_with("[anchor]")),
                "the FIRST fire must stay description-only — no anchor, got {turns:?}"
            );
        }

        // what this catches: the anchor escalation itself (card d6f010c8, live
        // 2026-07-23: Atlas re-greeted straight past the [pattern] description;
        // a concrete in-room work anchor broke the loop instantly). On the Nth
        // consecutive fire the burst must gain an [anchor] built from the LIVE
        // room-kanban delivery — the top unclaimed card and an in-flight card,
        // quoted verbatim from the one board render — placed after the
        // description.
        #[test]
        fn nth_consecutive_fire_escalates_to_a_live_board_anchor() {
            let me = "me-peer";
            let peer = "7711fe60-a19f-4f41-9ab6-24c884757338";
            let deliveries = vec![
                delivery(
                    "room-kanban",
                    vec![
                        kanban_card("94ad103f", "Fix the lane admission planner", "Open", None),
                        kanban_card(
                            "21ffe3c0",
                            "Wire the projector",
                            "InProgress",
                            Some("0d3209a1-c675-41db-9867-86f1011f9520"),
                        ),
                    ],
                ),
                delivery("airc", greeting_spiral(me, peer, 4)),
            ];
            let turns = build_workspace_turns(&deliveries, me, "Asha", None);
            let anchor = turns.last().unwrap();
            assert!(
                anchor.content.starts_with("[anchor]"),
                "the 2nd consecutive fire must append the work anchor, got {anchor:?}"
            );
            assert!(
                anchor.content.contains("94ad103f")
                    && anchor.content.contains("Fix the lane admission planner")
                    && anchor.content.contains("unclaimed"),
                "the anchor must name the live unclaimed card verbatim, got: {}",
                anchor.content
            );
            assert!(
                anchor.content.contains("21ffe3c0") && anchor.content.contains("InProgress"),
                "the anchor must carry the in-flight card as proof work is real, got: {}",
                anchor.content
            );
            assert!(
                turns[turns.len() - 2].content.starts_with("[pattern]"),
                "the description still precedes the anchor — escalation adds, never replaces"
            );
        }

        // what this catches: an empty (or absent/unreadable) board must yield the
        // HONEST empty anchor — "no open cards, propose one" — never a fabricated
        // card ([[fallbacks-are-illegal-fail-loud]]).
        #[test]
        fn empty_board_escalation_is_honest() {
            let me = "me-peer";
            let peer = "7711fe60-a19f-4f41-9ab6-24c884757338";
            let deliveries = vec![delivery("airc", greeting_spiral(me, peer, 4))];
            let turns = build_workspace_turns(&deliveries, me, "Asha", None);
            let anchor = turns.last().unwrap();
            assert!(
                anchor.content.starts_with("[anchor]")
                    && anchor.content.contains("No open cards")
                    && anchor.content.contains("work/create"),
                "an empty board must be stated honestly, got: {anchor:?}"
            );
            assert!(
                !anchor.content.contains("card 9") && !anchor.content.contains("card 2"),
                "an empty board must never grow invented cards, got: {}",
                anchor.content
            );
        }

        // what this catches: the escalation resets when the pattern BREAKS — a
        // novel message after prior repeats means the description landed (or the
        // mind moved on), so neither the description nor the anchor may fire.
        // The run is derived from the window, so the reset is structural.
        #[test]
        fn novel_message_resets_escalation() {
            let me = "me-peer";
            let peer = "7711fe60-a19f-4f41-9ab6-24c884757338";
            let mut items = greeting_spiral(me, peer, 3);
            items.push(chat(
                me,
                "Claimed card 94ad103f — reading the admission planner's replan \
                 guard now, first patch coming shortly.",
            ));
            let deliveries = vec![delivery("airc", items)];
            let turns = build_workspace_turns(&deliveries, me, "Asha", None);
            assert!(
                !turns
                    .iter()
                    .any(|t| t.content.starts_with("[pattern]")
                        || t.content.starts_with("[anchor]")),
                "a novel last message breaks the run — no description, no anchor, got {turns:?}"
            );
        }

        #[test]
        fn trigger_is_anchored_as_last_turn_when_delivery_lags() {
            // what this catches: the empty-message-turn quirk (glass-box 2026-06-30).
            // The `airc` delivery that threads the conversation is refreshed
            // asynchronously from the wake, so a directed message can trigger a turn
            // whose composed thread still ENDS on the persona's own prior reply. The
            // model then sees nothing new after its last turn, emits an empty
            // completion, and the turn parses as Pass — the persona goes silent on a
            // question aimed straight at it. Anchoring the trigger as the final `user`
            // turn (from the KNOWN waking message, not the lagging delivery) makes the
            // turn always perceive what it is answering. This test simulates the lag
            // (delivery ends on her own turn, trigger absent) and asserts the trigger
            // becomes the last peer turn, resolved to its roster name.
            let me = "me-peer";
            let joel = "7711fe60-a19f-4f41-9ab6-24c884757338";
            let deliveries = vec![
                delivery("room-roster", vec![roster(joel, "Joel"), roster(me, "Asha")]),
                // The lagging thread: her own reply is the last turn; Joel's new
                // question has NOT yet landed in the delivery.
                delivery(
                    "airc",
                    vec![chat(joel, "morning"), chat(me, "morning Joel!")],
                ),
            ];
            let trigger = super::super::TriggerTurn {
                peer_id: joel,
                content: "run commands/list and tell me the count",
                occurred_at_ms: 42,
            };
            let turns = build_workspace_turns(&deliveries, me, "Asha", Some(trigger));

            let last = turns.last().expect("at least the anchored trigger");
            assert!(
                !last.is_self
                    && last.author == "Joel"
                    && last.content == "run commands/list and tell me the count",
                "the waking message must be anchored as the final peer turn (roster \
                 name resolved), got {last:?}"
            );
            assert_eq!(turns.len(), 3, "two threaded turns + the anchored trigger");
        }

        #[test]
        fn trigger_is_not_doubled_when_delivery_already_threaded_it() {
            // what this catches: idempotency of the anchor. On the self-tick that
            // re-perceives the message ~one tick later, the delivery HAS caught up —
            // the trigger is already the last peer turn. Anchoring must be a no-op
            // then, never a duplicate turn (which would make her re-answer / echo).
            let me = "me-peer";
            let joel = "7711fe60-a19f-4f41-9ab6-24c884757338";
            let question = "run commands/list and tell me the count";
            let deliveries = vec![
                delivery("room-roster", vec![roster(joel, "Joel"), roster(me, "Asha")]),
                // Caught-up thread: the trigger IS the last turn already.
                delivery(
                    "airc",
                    vec![chat(me, "morning Joel!"), chat(joel, question)],
                ),
            ];
            let trigger = super::super::TriggerTurn {
                peer_id: joel,
                content: question,
                occurred_at_ms: 42,
            };
            let turns = build_workspace_turns(&deliveries, me, "Asha", Some(trigger));
            assert_eq!(
                turns.len(),
                2,
                "already-threaded trigger must not be doubled, got {turns:?}"
            );
            assert_eq!(turns.last().unwrap().content, question);
        }

        // Deterministic reproduction of the live 2026-07-24 four-persona echo
        // hall (card 65fca48d): each persona's reply restates what ANOTHER
        // participant already said, so detector 1 never fires (her mirror is
        // novel relative to her OWN history — here she has only 2 own turns,
        // below detector 1's floor of 3) and detector 2's consecutive tail
        // chain is broken by the interleaved substantial peer report. Only the
        // cross-speaker MIRROR detector can see this shape.
        fn echo_hall(me: &str, anwen: &str, benchy: &str) -> Vec<RagItem> {
            vec![
                chat(
                    anwen,
                    "I see that there might have been some confusion earlier. Let's start \
                     fresh! How can I assist you today with the word count project?",
                ),
                chat(
                    benchy,
                    "The Rust program I ran requires a file path argument to function \
                     correctly, so it exited with a usage error before printing any counts.",
                ),
                chat(
                    me,
                    "I see that there might have been some confusion earlier. Let's start \
                     fresh! How can I assist you today with the word count project?",
                ),
                chat(
                    anwen,
                    "I see that we're both here to help! Please let me know which specific \
                     project or file you need help with today.",
                ),
                chat(
                    me,
                    "I see that we're both here to help! Please let me know which specific \
                     project or file you need help with today.",
                ),
            ]
        }

        // what this catches: the cross-speaker echo loop (card 65fca48d, live
        // 2026-07-24 — Atlas/Asha/Benchy/Anwen ping-ponged "I see that we're
        // both here to help!" for hours with ZERO detector fires in 40
        // consecutive captures). A trailing run of OWN messages each already
        // ≥0.85-contained in OTHER speakers' earlier vocabulary must surface
        // the mirror observation, and a run of 2 (she mirrored again AFTER the
        // observation was derivable) must escalate to the live board anchor —
        // same escalation law as detectors 1/2.
        #[test]
        fn cross_speaker_mirror_fires_observation_and_escalates_to_anchor() {
            let me = "me-peer";
            let anwen = "7711fe60-a19f-4f41-9ab6-24c884757338";
            let benchy = "0d3209a1-c675-41db-9867-86f1011f9520";
            let deliveries = vec![
                delivery(
                    "room-kanban",
                    vec![kanban_card("65fca48d", "Break the echo loop", "Open", None)],
                ),
                delivery("airc", echo_hall(me, anwen, benchy)),
            ];
            let turns = build_workspace_turns(&deliveries, me, "Asha", None);
            let obs = turns
                .iter()
                .find(|t| t.content.starts_with("[pattern]"))
                .expect("the cross-speaker mirror must surface an observation");
            assert!(
                obs.content.contains("Asha") && obs.content.contains("restate"),
                "the observation must name the persona and the mirroring, got: {}",
                obs.content
            );
            assert_eq!(
                turns
                    .iter()
                    .filter(|t| t.content.starts_with("[pattern]"))
                    .count(),
                1,
                "exactly one observation per burst — perception, not nagging"
            );
            let anchor = turns.last().unwrap();
            assert!(
                anchor.content.starts_with("[anchor]")
                    && anchor.content.contains("65fca48d")
                    && anchor.content.contains("Break the echo loop"),
                "a mirror run of 2 must escalate to the live board anchor, got: {anchor:?}"
            );
        }

        // what this catches: a genuinely novel multi-speaker conversation —
        // each reply quotes SOME of the peer's vocabulary (answering means
        // restating the question) but adds new facts/actions — must never trip
        // the mirror detector. The 0.85 bar exists exactly for this: an answer
        // reuses a peer's words, an echo reuses (nearly) ONLY a peer's words.
        #[test]
        fn novel_multi_speaker_conversation_does_not_trip_the_mirror() {
            let me = "me-peer";
            let anwen = "7711fe60-a19f-4f41-9ab6-24c884757338";
            let benchy = "0d3209a1-c675-41db-9867-86f1011f9520";
            let deliveries = vec![delivery(
                "airc",
                vec![
                    chat(anwen, "Can someone check why the word count tool exits early on empty files?"),
                    chat(me, "Checked the word count tool: it exits early because read_to_string returns Ok with zero bytes and we treat that as an error branch."),
                    chat(benchy, "Nice find — does the fix need a regression test?"),
                    chat(me, "Yes — adding test_empty_file_counts_zero that pins stdout to '0 words' and returns exit code zero."),
                ],
            )];
            let turns = build_workspace_turns(&deliveries, me, "Asha", None);
            assert!(
                !turns
                    .iter()
                    .any(|t| t.content.starts_with("[pattern]")
                        || t.content.starts_with("[anchor]")),
                "novel multi-speaker work must never trip the mirror, got {turns:?}"
            );
        }

        // what this catches: a mirror run of exactly 1 stays description-only —
        // the first fire is her chance to self-correct; the anchor only appears
        // when the observation demonstrably failed to land (same law as
        // detectors 1/2, PATTERN_FIRES_BEFORE_ANCHOR).
        #[test]
        fn first_mirror_fire_stays_description_only() {
            let me = "me-peer";
            let anwen = "7711fe60-a19f-4f41-9ab6-24c884757338";
            let benchy = "0d3209a1-c675-41db-9867-86f1011f9520";
            let mut items = echo_hall(me, anwen, benchy);
            // Replace her FIRST own message with novel work so only the newest
            // own message mirrors — run of 1.
            items[2] = chat(
                me,
                "Claimed card 44ebaa41 — reading conway_game_of_life/src/main.rs \
                 now to wire the neighbor count fix.",
            );
            let deliveries = vec![
                delivery(
                    "room-kanban",
                    vec![kanban_card("65fca48d", "Break the echo loop", "Open", None)],
                ),
                delivery("airc", items),
            ];
            let turns = build_workspace_turns(&deliveries, me, "Asha", None);
            assert!(
                turns.iter().any(|t| t.content.starts_with("[pattern]")),
                "a single trailing mirror must still fire the description, got {turns:?}"
            );
            assert!(
                !turns.iter().any(|t| t.content.starts_with("[anchor]")),
                "the FIRST mirror fire must stay description-only, got {turns:?}"
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
                room_id: Uuid::nil(),
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
            ..Default::default()
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
                room_id: Uuid::nil(),
            })),
            Ok(None),
        ]);

        conversation.prime().await.expect("prime ok");

        let reader: Arc<dyn AircTranscriptReader> = Arc::new(StubAircCitizen::new(Uuid::new_v4()));
        let opts = ServeOptions {
            page_recent_limit: 10,
            rag_fetch_limit: 10,
            now_ms: fixed_now,
            ..Default::default()
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
            RoleId::Designer,
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
            RoleId::Designer => (),
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
                room_id: Uuid::nil(),
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
                ..Default::default()
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
                room_id: Uuid::nil(),
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
                ..Default::default()
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
                    room_id: Uuid::nil(),
                })),
                Ok(Some(IncomingMessage {
                    lamport: 100, // exactly at the mark — also skipped
                    peer_id: other_peer,
                    text: "boundary".to_string(),
                    room_id: Uuid::nil(),
                })),
                Ok(Some(IncomingMessage {
                    lamport: 101, // FRESH
                    peer_id: other_peer,
                    text: "new".to_string(),
                    room_id: Uuid::nil(),
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
                ..Default::default()
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
                room_id: Uuid::nil(),
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
                ..Default::default()
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
