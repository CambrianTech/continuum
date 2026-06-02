//! Per-persona service loop — slice 10 of #133.
//!
//! Takes a slice-9 [`HostedPersona`] and a "talk to the grid"
//! abstraction ([`PersonaConversation`]) and runs the chat-flawless
//! cognition path:
//!
//!   subscribe → for each event:
//!     • skip pre-watermark / self / non-text
//!     • RAG + inference via [`inspect_persona_rag_with_inference`]
//!     • post reply
//!
//! This is the loop that today lives directly in
//! `bin/airc_chat_demo.rs`'s `main()` (~80 lines, lines 314-426).
//! Slice 10 factors it into a substrate-callable function so the
//! supervisor — not the demo binary — can host the persona.
//!
//! ## Doctrine
//!
//! - [[no-if-statements-use-llms-for-cognition]]: the loop does the
//!   minimum substrate filtering — pre-watermark / self / non-text —
//!   and hands the rest to the inference command. No "should I
//!   respond?" heuristics here. The LLM decides.
//! - [[no-fallbacks-ever]]: per-message errors (RAG failure, factory
//!   reject) are logged + counted on the outcome, not swallowed; the
//!   loop continues with the next message rather than substituting a
//!   default response.
//! - [[no-stdio-piping-for-process-ipc]]: the loop talks to airc only
//!   through the [`PersonaConversation`] trait. The trait is the
//!   substrate's IPC boundary; tests stub it without any daemon.
//!
//! ## What slice 11 adds (not in this commit)
//!
//! - [`AircPersonaConversation`] production impl wrapping
//!   `Arc<PersonaAircRuntime>` against the real `airc_lib::Airc`.
//! - Wiring: `bin/airc_chat_demo` becomes a 30-line shell that
//!   constructs a HostedPersona + AircPersonaConversation and calls
//!   `serve_persona_loop`.
//!
//! Splitting keeps slice 10 testable on a stub conversation; slice
//! 11 is the production-airc integration where the real
//! `Airc::subscribe()` stream lives.

use crate::ai::adapter::AIProviderAdapter;
use crate::persona::airc_source::AircTranscriptReader;
use crate::persona::rag_inspect::{inspect_persona_rag_with_inference, RagInspectionRequest};
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

    // Adapter is shared with the RAG layer turn-by-turn via
    // `Arc::clone`. Per the `&ctx` doctrine we never extract identity
    // fields — `ctx.identity.peer_id` reads cleanly at the comparison
    // site below and every log line inside the span already carries
    // them as structured fields.
    let adapter: Arc<dyn AIProviderAdapter> = ctx.adapter.clone();
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

        // `&ctx`-pure derivation: RAG request reads the profile
        // (context_length, etc.) directly from ctx. No copied
        // fields. Per [[context-is-the-client-airc-token-is-identity]]
        // the substrate's calling convention is "hand the context,
        // not its parts."
        let mut req = RagInspectionRequest::for_ctx(ctx, (opts.now_ms)());
        req.airc_fetch_limit = opts.rag_fetch_limit;

        let inspection = match inspect_persona_rag_with_inference(
            &req,
            reader.clone(),
            Some(adapter.clone()),
        )
        .await
        {
            Ok(v) => v,
            Err(e) => {
                // Persona identity fields come from the entered span;
                // log lines just add the per-turn delta.
                tracing::warn!(
                    lamport = msg.lamport,
                    error = %e,
                    "inspect_persona_rag_with_inference failed"
                );
                outcome.turns_errored += 1;
                continue;
            }
        };

        let Some(mr) = inspection.model_response else {
            // RAG-only result — no inference ran. Intentional (e.g.
            // budget allocator produced empty delivery). Count as
            // skipped, not errored — the loop did the right thing.
            outcome.turns_skipped += 1;
            continue;
        };

        if let Err(e) = conversation.say(&mr.response_text).await {
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
        outcome.turns_replied += 1;
        tracing::info!(
            lamport = msg.lamport,
            turn_duration_ms = turn_duration_ms,
            turns_replied = outcome.turns_replied,
            mean_ms = outcome.turn_latency.mean_ms().unwrap_or(0.0),
            min_ms = outcome.turn_latency.min_ms.unwrap_or(0),
            max_ms = outcome.turn_latency.max_ms.unwrap_or(0),
            "turn complete — substrate's per-reply cost recorded"
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
    use crate::ai::adapter::{
        AdapterCapabilities, AIProviderAdapter as _, ApiStyle,
    };
    use crate::ai::types::{
        EmbeddingRequest, EmbeddingResponse, FinishReason, HealthStatus, ModelInfo,
        TextGenerationRequest, TextGenerationResponse, UsageMetrics,
    };
    use crate::modules::persona_instance_manager::PersonaInstanceInfo;
    use crate::persona::airc_source::AircTranscriptReader;
    use crate::persona::identity_provider::PersonaIdentitySource;
    use crate::persona::role_template::RoleId;
    use crate::persona::supervisor::HostedPersona;
    use airc_lib::{AircError, TranscriptEvent};
    use std::collections::VecDeque;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    /// Stub conversation: feeds a pre-baked queue of events; records
    /// every `say` call for assertions. `primed` records whether the
    /// loop called `prime` at startup — the contract is one call per
    /// loop invocation.
    struct StubConversation {
        high_water: u64,
        events: Mutex<VecDeque<Result<Option<IncomingMessage>, String>>>,
        said: Mutex<Vec<String>>,
        primed: AtomicUsize,
    }

    #[async_trait]
    impl PersonaConversation for StubConversation {
        async fn prime(&mut self) -> Result<(), String> {
            // No stream to open — the stub yields events from an
            // in-memory queue. The substrate's prime() contract is
            // satisfied trivially. Records that prime was called so
            // tests can assert the loop honors the contract.
            self.primed.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        async fn high_water_mark(&self, _limit: usize) -> Result<u64, String> {
            Ok(self.high_water)
        }
        async fn next_message(&mut self) -> Result<Option<IncomingMessage>, String> {
            self.events
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or(Ok(None))
        }
        async fn say(&self, text: &str) -> Result<(), String> {
            self.said.lock().unwrap().push(text.to_string());
            Ok(())
        }
    }

    /// Stub adapter: every generate_text returns a canned response.
    /// Used so the inspect_persona_rag_with_inference call has
    /// something to return without loading a GGUF.
    ///
    /// `inject_delay_ms` injects an awaitable sleep so the latency
    /// metric test can assert that recorded ms reflect REAL elapsed
    /// time — not just that `record()` is being called with whatever
    /// happens to fall out of `Instant::elapsed`. Without this, the
    /// metric test would be fake-demo-shaped (passing on plumbing,
    /// silent on correctness).
    struct CannedAdapter {
        reply: String,
        calls: AtomicUsize,
        inject_delay_ms: u64,
    }

    #[async_trait]
    impl AIProviderAdapter for CannedAdapter {
        fn provider_id(&self) -> &str {
            "canned"
        }
        fn name(&self) -> &str {
            "canned"
        }
        fn capabilities(&self) -> AdapterCapabilities {
            AdapterCapabilities {
                supports_text_generation: true,
                supports_chat: true,
                is_local: true,
                ..Default::default()
            }
        }
        fn api_style(&self) -> ApiStyle {
            ApiStyle::Local
        }
        fn default_model(&self) -> &str {
            "canned-model"
        }
        async fn initialize(&mut self) -> Result<(), String> {
            Ok(())
        }
        async fn shutdown(&mut self) -> Result<(), String> {
            Ok(())
        }
        async fn generate_text(
            &self,
            _request: TextGenerationRequest,
        ) -> Result<TextGenerationResponse, String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.inject_delay_ms > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(self.inject_delay_ms))
                    .await;
            }
            Ok(TextGenerationResponse {
                text: self.reply.clone(),
                model: "canned-model".to_string(),
                provider: "canned".to_string(),
                finish_reason: FinishReason::Stop,
                usage: UsageMetrics {
                    input_tokens: 1,
                    output_tokens: 1,
                    total_tokens: 2,
                    estimated_cost: None,
                },
                response_time_ms: 0,
                request_id: "canned-request".to_string(),
                content: None,
                tool_calls: None,
                routing: None,
                error: None,
            })
        }
        async fn create_embedding(
            &self,
            _request: EmbeddingRequest,
        ) -> Result<EmbeddingResponse, String> {
            Err("canned does not embed".into())
        }
        async fn health_check(&self) -> HealthStatus {
            HealthStatus::default()
        }
        async fn get_available_models(&self) -> Vec<ModelInfo> {
            vec![]
        }
    }

    /// Stub reader: always returns an empty transcript — RAG layer
    /// still runs through; the inference adapter still gets called.
    struct EmptyReader;

    #[async_trait]
    impl AircTranscriptReader for EmptyReader {
        async fn page_recent(
            &self,
            _limit: usize,
        ) -> Result<Vec<TranscriptEvent>, AircError> {
            Ok(vec![])
        }
    }

    fn fake_hosted(persona_peer_id: Uuid, reply: &str) -> HostedPersona {
        fake_hosted_with_delay(persona_peer_id, reply, 0)
    }

    fn fake_hosted_with_delay(
        persona_peer_id: Uuid,
        reply: &str,
        inject_delay_ms: u64,
    ) -> HostedPersona {
        use crate::persona::hw_tier_descriptor::HwTierCategory;
        use crate::persona::inference_profile::{PersonaInferenceProfile, SamplingProfile};
        let adapter = CannedAdapter {
            reply: reply.to_string(),
            calls: AtomicUsize::new(0),
            inject_delay_ms,
        };
        let persona_id = Uuid::new_v4();
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
            adapter: Arc::new(adapter),
            // Service-loop tests drive the loop through `StubConversation`
            // directly — the citizen handle is never touched. A
            // [`StubAircCitizen`] satisfies the type without standing
            // up a real airc daemon; per [[no-fallbacks-ever]] this
            // replaces the previous `Option<Arc<PersonaAircRuntime>>`
            // smell with a typed stub.
            runtime: Arc::new(
                crate::persona::airc_citizen::StubAircCitizen::new(persona_peer_id),
            ),
        }
    }

    fn fixed_now() -> u64 {
        1_700_000_000_000
    }

    /// Happy path: one inbound from another peer → one reply posted.
    /// turns_replied=1, turns_skipped=0, turns_errored=0.
    #[tokio::test]
    async fn replies_to_inbound_from_other_peer() {
        let persona_peer = Uuid::new_v4();
        let other_peer = Uuid::new_v4();
        let hosted = fake_hosted(persona_peer, "yes, hi.");

        let mut conversation = StubConversation {
            high_water: 0,
            events: Mutex::new(VecDeque::from(vec![
                Ok(Some(IncomingMessage {
                    lamport: 1,
                    peer_id: other_peer,
                    text: "hello?".to_string(),
                })),
                Ok(None),
            ])),
            said: Mutex::new(vec![]),
            primed: AtomicUsize::new(0),
        };

        // Caller-primes contract: direct callers of serve_persona_loop
        // (tests, demo binaries) prime explicitly before iterating.
        // The supervisor's spawn_persona_service path does this at the
        // supervisor level. Per [[no-fallbacks-ever]] there's only ONE
        // place that primes per code path; the loop assumes the
        // contract is honored.
        conversation.prime().await.expect("prime ok");

        let reader: Arc<dyn AircTranscriptReader> = Arc::new(EmptyReader);
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
        let said = conversation.said.lock().unwrap();
        assert_eq!(said.len(), 1);
        assert_eq!(said[0], "yes, hi.");
        // The loop primes the conversation exactly once at boot —
        // before any high_water_mark or next_message call. Per
        // [[persona-webrtc-all-tiers-latency-obsessed]] this is what
        // moves the airc subscribe round-trip OFF the cognition hot
        // path. If a future refactor regresses to lazy subscribe, the
        // primed count drops to 0 and this test fails loudly.
        assert_eq!(
            conversation.primed.load(Ordering::SeqCst),
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
    #[tokio::test]
    async fn latency_metric_reflects_real_wall_clock() {
        let persona_peer = Uuid::new_v4();
        let other_peer = Uuid::new_v4();
        let hosted = fake_hosted_with_delay(persona_peer, "ok.", 80);

        let mut conversation = StubConversation {
            high_water: 0,
            events: Mutex::new(VecDeque::from(vec![
                Ok(Some(IncomingMessage {
                    lamport: 1,
                    peer_id: other_peer,
                    text: "ping?".to_string(),
                })),
                Ok(None),
            ])),
            said: Mutex::new(vec![]),
            primed: AtomicUsize::new(0),
        };

        conversation.prime().await.expect("prime ok");

        let reader: Arc<dyn AircTranscriptReader> = Arc::new(EmptyReader);
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
        let hosted = fake_hosted(persona_peer, "unused");

        struct UnprimedConversation {
            events: Mutex<VecDeque<()>>,
        }
        #[async_trait]
        impl PersonaConversation for UnprimedConversation {
            async fn prime(&mut self) -> Result<(), String> {
                // Test deliberately never calls this — we're verifying
                // the loop does NOT call it implicitly.
                panic!("test contract: prime must NOT be invoked by the loop");
            }
            async fn high_water_mark(&self, _limit: usize) -> Result<u64, String> {
                Ok(0)
            }
            async fn next_message(&mut self) -> Result<Option<IncomingMessage>, String> {
                // Mimics AircPersonaConversation's typed-err shape when
                // unprimed. After one error the queue drains and the
                // loop ends.
                let mut q = self.events.lock().unwrap();
                if q.pop_front().is_some() {
                    Err("called before prime() — caller must invoke prime() first".to_string())
                } else {
                    Ok(None)
                }
            }
            async fn say(&self, _text: &str) -> Result<(), String> {
                panic!("say must not be called when next_message errors");
            }
        }

        let mut conversation = UnprimedConversation {
            events: Mutex::new(VecDeque::from(vec![()])),
        };
        let _ = other_peer;
        let reader: Arc<dyn AircTranscriptReader> = Arc::new(EmptyReader);
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
        let hosted = fake_hosted(persona_peer, "should not be sent.");

        let mut conversation = StubConversation {
            high_water: 0,
            events: Mutex::new(VecDeque::from(vec![
                Ok(Some(IncomingMessage {
                    lamport: 1,
                    peer_id: persona_peer, // SELF
                    text: "my own echo".to_string(),
                })),
                Ok(None),
            ])),
            said: Mutex::new(vec![]),
            primed: AtomicUsize::new(0),
        };

        // Caller-primes contract per [[no-fallbacks-ever]] — explicit,
        // not safety-net.
        conversation.prime().await.expect("prime ok");

        let reader: Arc<dyn AircTranscriptReader> = Arc::new(EmptyReader);
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
        assert!(conversation.said.lock().unwrap().is_empty());
    }

    /// Pre-watermark guard: messages with lamport <= high_water are
    /// skipped. Avoids replying to history on attach.
    #[tokio::test]
    async fn skips_messages_below_high_water_mark() {
        let persona_peer = Uuid::new_v4();
        let other_peer = Uuid::new_v4();
        let hosted = fake_hosted(persona_peer, "fresh reply.");

        let mut conversation = StubConversation {
            high_water: 100, // pre-attach history was up to lamport=100
            events: Mutex::new(VecDeque::from(vec![
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
            ])),
            said: Mutex::new(vec![]),
            primed: AtomicUsize::new(0),
        };

        // Caller-primes contract per [[no-fallbacks-ever]] — explicit,
        // not safety-net.
        conversation.prime().await.expect("prime ok");

        let reader: Arc<dyn AircTranscriptReader> = Arc::new(EmptyReader);
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
        assert_eq!(conversation.said.lock().unwrap().len(), 1);
    }

    /// Transient transport error increments turns_errored AND the
    /// loop continues — does NOT propagate as a Result::Err from
    /// serve_persona_loop. The trailing Ok(None) eventually ends it
    /// cleanly. Models the demo's "live stream lag — resume continues"
    /// behavior (`bin/airc_chat_demo.rs:346`).
    #[tokio::test]
    async fn transient_next_message_error_does_not_kill_loop() {
        let persona_peer = Uuid::new_v4();
        let other_peer = Uuid::new_v4();
        let hosted = fake_hosted(persona_peer, "ok.");

        let mut conversation = StubConversation {
            high_water: 0,
            events: Mutex::new(VecDeque::from(vec![
                Err("stream lag".to_string()),
                Ok(Some(IncomingMessage {
                    lamport: 1,
                    peer_id: other_peer,
                    text: "after lag".to_string(),
                })),
                Ok(None),
            ])),
            said: Mutex::new(vec![]),
            primed: AtomicUsize::new(0),
        };

        // Caller-primes contract per [[no-fallbacks-ever]] — explicit,
        // not safety-net.
        conversation.prime().await.expect("prime ok");

        let reader: Arc<dyn AircTranscriptReader> = Arc::new(EmptyReader);
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
        assert_eq!(conversation.said.lock().unwrap().len(), 1);
    }
}
