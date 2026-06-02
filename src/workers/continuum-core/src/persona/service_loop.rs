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
/// All four methods are async because the production impl chains
/// over airc's IPC socket. Tests use a stub that's instant.
#[async_trait]
pub trait PersonaConversation: Send + Sync {
    /// Highest lamport observed in transcript history before live
    /// subscription. Used to ignore messages that arrived BEFORE the
    /// persona attached — avoids replying to ancient chat just
    /// because a restart loaded them through `page_recent`.
    async fn high_water_mark(&self, limit: usize) -> Result<u64, String>;

    /// Yield the next inbound message, or `Ok(None)` when the
    /// stream is exhausted (daemon disconnected, peer gone). On
    /// transient errors (stream lag, transport hiccup) the impl
    /// should yield `Err` so the loop can record + continue.
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

/// Aggregate stats from one `serve_persona_loop` run. Returned when
/// the conversation stream ends; useful for operators + tests
/// asserting on what happened without scraping log lines.
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
}

/// Run the per-persona service loop until the conversation stream
/// ends.
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
        outcome.turns_replied += 1;
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
    /// every `say` call for assertions.
    struct StubConversation {
        high_water: u64,
        events: Mutex<VecDeque<Result<Option<IncomingMessage>, String>>>,
        said: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl PersonaConversation for StubConversation {
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
    struct CannedAdapter {
        reply: String,
        calls: AtomicUsize,
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
        use crate::persona::hw_tier_descriptor::HwTierCategory;
        use crate::persona::inference_profile::{PersonaInferenceProfile, SamplingProfile};
        let adapter = CannedAdapter {
            reply: reply.to_string(),
            calls: AtomicUsize::new(0),
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
            // Test fixtures don't run through `spawn_persona_service`,
            // so the runtime stub is None. Production paths always
            // populate this from the registry post-bootstrap.
            runtime: None,
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
        };

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
        };

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
        };

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
        };

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
