/// IPC server for continuum-core
///
/// Unix socket server that accepts JSON requests and returns JSON responses.
/// Follows the same pattern as logger worker - event-driven, no polling.
///
/// Architecture:
/// - One thread per connection (spawn on accept)
/// - Tokio async for concurrent request handling
/// - JSON protocol (JTAGRequest/JTAGResponse)
/// - Performance timing on every request
use crate::voice::{UtteranceEvent, VoiceParticipant};
use crate::persona::{PersonaInbox, PersonaCognitionEngine, InboxMessage, SenderType, Modality, ChannelRegistry, ChannelEnqueueRequest, ActivityDomain, PersonaState};
use crate::rag::RagEngine;
use crate::logging::TimingGuard;
use ts_rs::TS;
use crate::{log_debug, log_info, log_error};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::Arc;
use uuid::Uuid;
use dashmap::DashMap;

// ============================================================================
// Response Field Names - Single Source of Truth
// ============================================================================

/// Voice response field: Array of AI participant UUIDs
const VOICE_RESPONSE_FIELD_RESPONDER_IDS: &str = "responder_ids";

// ============================================================================
// Request/Response Protocol
// ============================================================================

/// Inbox message for IPC (mirrors InboxMessage but with string UUIDs for JSON transport)
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/ipc/InboxMessageRequest.ts")]
pub struct InboxMessageRequest {
    pub id: String,
    pub room_id: String,
    pub sender_id: String,
    pub sender_name: String,
    pub sender_type: String,  // "human", "persona", "agent", "system"
    pub content: String,
    /// Timestamp in milliseconds (fits in JS number, max safe ~9 quadrillion)
    #[ts(type = "number")]
    pub timestamp: u64,
    pub priority: f32,
    #[ts(optional)]
    pub source_modality: Option<String>,  // "chat", "voice"
    #[ts(optional)]
    pub voice_session_id: Option<String>,
}

impl InboxMessageRequest {
    fn to_inbox_message(&self) -> Result<InboxMessage, String> {
        Ok(InboxMessage {
            id: Uuid::parse_str(&self.id).map_err(|e| format!("Invalid id: {e}"))?,
            room_id: Uuid::parse_str(&self.room_id).map_err(|e| format!("Invalid room_id: {e}"))?,
            sender_id: Uuid::parse_str(&self.sender_id).map_err(|e| format!("Invalid sender_id: {e}"))?,
            sender_name: self.sender_name.clone(),
            sender_type: match self.sender_type.as_str() {
                "human" => SenderType::Human,
                "persona" => SenderType::Persona,
                "agent" => SenderType::Agent,
                "system" => SenderType::System,
                _ => return Err(format!("Invalid sender_type: {}", self.sender_type)),
            },
            content: self.content.clone(),
            timestamp: self.timestamp,
            priority: self.priority,
            source_modality: self.source_modality.as_ref().map(|m| match m.as_str() {
                "voice" => Modality::Voice,
                _ => Modality::Chat,
            }),
            voice_session_id: self.voice_session_id.as_ref()
                .map(|s| Uuid::parse_str(s).ok())
                .flatten(),
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "command")]
enum Request {
    #[serde(rename = "voice/register-session")]
    VoiceRegisterSession {
        session_id: String,
        room_id: String,
        participants: Vec<VoiceParticipant>,
    },

    #[serde(rename = "voice/on-utterance")]
    VoiceOnUtterance { event: UtteranceEvent },

    #[serde(rename = "voice/should-route-tts")]
    VoiceShouldRouteTts {
        session_id: String,
        persona_id: String,
    },

    #[serde(rename = "voice/synthesize")]
    VoiceSynthesize {
        text: String,
        voice: Option<String>,
        adapter: Option<String>,
    },

    /// Synthesize and inject audio directly into a call's mixer.
    /// Audio never leaves the Rust process — TypeScript gets back metadata only.
    #[serde(rename = "voice/speak-in-call")]
    VoiceSpeakInCall {
        call_id: String,
        user_id: String,
        text: String,
        voice: Option<String>,
        adapter: Option<String>,
    },

    /// Synthesize audio and store in server-side buffer pool.
    /// Returns a Handle (UUID) + metadata. Audio stays in Rust memory.
    /// Use voice/play-handle to inject into a call, or voice/discard-handle to free.
    #[serde(rename = "voice/synthesize-handle")]
    VoiceSynthesizeHandle {
        text: String,
        voice: Option<String>,
        adapter: Option<String>,
    },

    /// Inject previously synthesized audio (by handle) into a call's mixer.
    /// Audio never crosses IPC — Rust reads from buffer pool and injects directly.
    #[serde(rename = "voice/play-handle")]
    VoicePlayHandle {
        handle: String,
        call_id: String,
        user_id: String,
    },

    /// Explicitly free a synthesized audio buffer.
    /// Buffers also auto-expire after 5 minutes.
    #[serde(rename = "voice/discard-handle")]
    VoiceDiscardHandle {
        handle: String,
    },

    #[serde(rename = "voice/transcribe")]
    VoiceTranscribe {
        /// Base64-encoded i16 PCM samples, 16kHz mono
        audio: String,
        /// Language code (e.g., "en") or None for auto-detection
        language: Option<String>,
    },

    #[serde(rename = "inbox/create")]
    InboxCreate { persona_id: String },

    // ========================================================================
    // Cognition Commands
    // ========================================================================

    #[serde(rename = "cognition/create-engine")]
    CognitionCreateEngine {
        persona_id: String,
        persona_name: String,
    },

    #[serde(rename = "cognition/calculate-priority")]
    CognitionCalculatePriority {
        persona_id: String,
        content: String,
        sender_type: String,  // "human", "persona", "agent", "system"
        is_voice: bool,
        room_id: String,
        timestamp: u64,
    },

    #[serde(rename = "cognition/fast-path-decision")]
    CognitionFastPathDecision {
        persona_id: String,
        message: InboxMessageRequest,
    },

    #[serde(rename = "cognition/enqueue-message")]
    CognitionEnqueueMessage {
        persona_id: String,
        message: InboxMessageRequest,
    },

    #[serde(rename = "cognition/get-state")]
    CognitionGetState { persona_id: String },

    // ========================================================================
    // Channel Commands
    // ========================================================================

    /// Route an item to its domain channel queue
    #[serde(rename = "channel/enqueue")]
    ChannelEnqueue {
        persona_id: String,
        item: ChannelEnqueueRequest,
    },

    /// Pop the highest-priority item from a specific domain channel
    #[serde(rename = "channel/dequeue")]
    ChannelDequeue {
        persona_id: String,
        domain: Option<String>,  // "AUDIO", "CHAT", "BACKGROUND" or null for any
    },

    /// Get per-channel status snapshot
    #[serde(rename = "channel/status")]
    ChannelStatus {
        persona_id: String,
    },

    /// Run one service cycle: consolidate + return next item to process
    #[serde(rename = "channel/service-cycle")]
    ChannelServiceCycle {
        persona_id: String,
    },

    /// Service cycle + fast-path decision in ONE call.
    /// Eliminates a separate IPC round-trip for fastPathDecision.
    /// Returns: service_cycle result + optional cognition decision.
    #[serde(rename = "channel/service-cycle-full")]
    ChannelServiceCycleFull {
        persona_id: String,
    },

    /// Clear all channel queues
    #[serde(rename = "channel/clear")]
    ChannelClear {
        persona_id: String,
    },

    // ========================================================================
    // Memory / Hippocampus Commands
    // ========================================================================

    /// Load a persona's memory corpus from the TS ORM.
    /// Rust is a pure compute engine — data comes from the ORM via IPC.
    #[serde(rename = "memory/load-corpus")]
    MemoryLoadCorpus {
        persona_id: String,
        memories: Vec<crate::memory::CorpusMemory>,
        events: Vec<crate::memory::CorpusTimelineEvent>,
    },

    /// 6-layer parallel multi-recall — the improved recall algorithm.
    /// Operates on in-memory MemoryCorpus data. Zero SQL.
    #[serde(rename = "memory/multi-layer-recall")]
    MemoryMultiLayerRecall {
        persona_id: String,
        query_text: Option<String>,
        room_id: String,
        max_results: usize,
        layers: Option<Vec<String>>,
    },

    /// Build consciousness context (temporal + cross-context + intentions).
    /// Operates on in-memory MemoryCorpus data. Zero SQL.
    #[serde(rename = "memory/consciousness-context")]
    MemoryConsciousnessContext {
        persona_id: String,
        room_id: String,
        current_message: Option<String>,
        skip_semantic_search: bool,
    },

    /// Append a single memory to a persona's cached corpus.
    /// Copy-on-write: O(n) clone, but appends are rare (~1/min/persona).
    /// Keeps Rust cache coherent with the TS ORM without full reload.
    #[serde(rename = "memory/append-memory")]
    MemoryAppendMemory {
        persona_id: String,
        memory: crate::memory::CorpusMemory,
    },

    /// Append a single timeline event to a persona's cached corpus.
    #[serde(rename = "memory/append-event")]
    MemoryAppendEvent {
        persona_id: String,
        event: crate::memory::CorpusTimelineEvent,
    },

    #[serde(rename = "health-check")]
    HealthCheck,

    #[serde(rename = "get-stats")]
    GetStats { category: Option<String> },
}

#[derive(Debug, Serialize, Deserialize)]
struct Response {
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "requestId")]
    request_id: Option<u64>,
}

impl Response {
    fn success(result: serde_json::Value) -> Self {
        Self {
            success: true,
            result: Some(result),
            error: None,
            request_id: None,
        }
    }

    fn error(msg: String) -> Self {
        Self {
            success: false,
            result: None,
            error: Some(msg),
            request_id: None,
        }
    }

    fn with_request_id(mut self, request_id: Option<u64>) -> Self {
        self.request_id = request_id;
        self
    }
}

// ============================================================================
// IPC Server State
// ============================================================================

struct ServerState {
    voice_service: Arc<crate::voice::voice_service::VoiceService>,
    /// Per-persona inboxes — DashMap for per-key locking (no cross-persona contention).
    inboxes: Arc<DashMap<Uuid, PersonaInbox>>,
    /// Per-persona cognition engines — DashMap: all hot-path ops are &self (read-only).
    cognition_engines: Arc<DashMap<Uuid, PersonaCognitionEngine>>,
    /// Per-persona channel registries + state — DashMap: hot-path ops are &mut self.
    /// 14 personas across DashMap's shards = near-zero contention.
    channel_registries: Arc<DashMap<Uuid, (ChannelRegistry, PersonaState)>>,
    rag_engine: Arc<RagEngine>,
    /// Shared CallManager for direct audio injection (speak-in-call).
    /// Audio never leaves Rust — IPC only returns metadata.
    call_manager: Arc<crate::voice::call_server::CallManager>,
    /// Server-side audio buffer pool for handle-based synthesis.
    /// Audio stays in Rust — TypeScript gets Handle + metadata only.
    audio_pool: Arc<crate::voice::audio_buffer::AudioBufferPool>,
    /// Tokio runtime handle for calling async CallManager methods from IPC threads.
    rt_handle: tokio::runtime::Handle,
    /// Per-persona memory manager — pure compute on in-memory MemoryCorpus.
    /// Data comes from the TS ORM via IPC. Zero SQL access.
    memory_manager: Arc<crate::memory::PersonaMemoryManager>,
}

impl ServerState {
    fn new(
        call_manager: Arc<crate::voice::call_server::CallManager>,
        rt_handle: tokio::runtime::Handle,
        memory_manager: Arc<crate::memory::PersonaMemoryManager>,
    ) -> Self {
        Self {
            voice_service: Arc::new(crate::voice::voice_service::VoiceService::new()),
            inboxes: Arc::new(DashMap::new()),
            cognition_engines: Arc::new(DashMap::new()),
            channel_registries: Arc::new(DashMap::new()),
            rag_engine: Arc::new(RagEngine::new()),
            call_manager,
            audio_pool: Arc::new(crate::voice::audio_buffer::AudioBufferPool::new()),
            rt_handle,
            memory_manager,
        }
    }

    fn handle_request(&self, request: Request) -> HandleResult {
        match request {
            Request::VoiceRegisterSession {
                session_id,
                room_id,
                participants,
            } => {
                let _timer = TimingGuard::new("ipc", "voice_register_session");

                HandleResult::Json(match self.voice_service.register_session(&session_id, &room_id, participants) {
                    Ok(_) => Response::success(serde_json::json!({ "registered": true })),
                    Err(e) => Response::error(e),
                })
            }

            Request::VoiceOnUtterance { event } => {
                let _timer = TimingGuard::new("ipc", "voice_on_utterance").with_threshold(10);

                HandleResult::Json(match self.voice_service.on_utterance(event) {
                    Ok(responder_ids) => Response::success(serde_json::json!({
                        VOICE_RESPONSE_FIELD_RESPONDER_IDS: responder_ids.into_iter().map(|id| id.to_string()).collect::<Vec<String>>()
                    })),
                    Err(e) => Response::error(e),
                })
            }

            Request::VoiceShouldRouteTts {
                session_id,
                persona_id,
            } => {
                let _timer = TimingGuard::new("ipc", "voice_should_route_tts");

                HandleResult::Json(match self.voice_service.should_route_tts(&session_id, &persona_id) {
                    Ok(should_route) => Response::success(serde_json::json!({ "should_route": should_route })),
                    Err(e) => Response::error(e),
                })
            }

            Request::VoiceSynthesize { text, voice, adapter } => {
                let _timer = TimingGuard::new("ipc", "voice_synthesize");

                // Delegate to TTS service (synchronous wrapper handles runtime)
                use crate::voice::tts_service;

                let result = tts_service::synthesize_speech_sync(
                    &text,
                    voice.as_deref(),
                    adapter.as_deref()
                );

                match result {
                    Ok(synthesis) => {
                        // Raw PCM bytes — NO base64, NO JSON encoding of audio.
                        // Binary framing sends these bytes directly over the socket.
                        let pcm_bytes: Vec<u8> = synthesis.samples.iter()
                            .flat_map(|s| s.to_le_bytes())
                            .collect();

                        log_info!(
                            "ipc", "voice_synthesize",
                            "Synthesized {} samples at {}Hz ({:.1}s) → {} bytes raw PCM",
                            synthesis.samples.len(),
                            synthesis.sample_rate,
                            synthesis.duration_ms as f64 / 1000.0,
                            pcm_bytes.len()
                        );

                        // JSON header carries metadata only — audio travels as raw binary
                        HandleResult::Binary {
                            json_header: Response::success(serde_json::json!({
                                "sample_rate": synthesis.sample_rate,
                                "duration_ms": synthesis.duration_ms,
                                "adapter": adapter.unwrap_or_else(|| "default".to_string()),
                                "num_samples": synthesis.samples.len(),
                                "binary_pcm": true
                            })),
                            binary_data: pcm_bytes,
                        }
                    },
                    Err(e) => {
                        log_error!("ipc", "voice_synthesize", "TTS failed: {}", e);
                        HandleResult::Json(Response::error(format!("TTS failed: {}", e)))
                    }
                }
            }

            Request::VoiceSpeakInCall { call_id, user_id, text, voice, adapter } => {
                let _timer = TimingGuard::new("ipc", "voice_speak_in_call");

                // Direct injection: synthesize + inject into call mixer.
                // Audio NEVER leaves the Rust process. TypeScript gets metadata only.
                let call_manager = self.call_manager.clone();
                let result = self.rt_handle.block_on(async {
                    call_manager.speak_in_call(
                        &call_id,
                        &user_id,
                        &text,
                        voice.as_deref(),
                        adapter.as_deref(),
                    ).await
                });

                HandleResult::Json(match result {
                    Ok((num_samples, duration_ms, sample_rate)) => {
                        log_info!(
                            "ipc", "voice_speak_in_call",
                            "Injected {} samples ({:.1}s) into call {} for user {}",
                            num_samples, duration_ms as f64 / 1000.0, call_id, user_id
                        );
                        Response::success(serde_json::json!({
                            "num_samples": num_samples,
                            "duration_ms": duration_ms,
                            "sample_rate": sample_rate,
                            "injected": true
                        }))
                    },
                    Err(e) => {
                        log_error!("ipc", "voice_speak_in_call", "Failed: {}", e);
                        Response::error(e)
                    }
                })
            }

            Request::VoiceSynthesizeHandle { text, voice, adapter } => {
                let _timer = TimingGuard::new("ipc", "voice_synthesize_handle");

                use crate::voice::tts_service;

                let result = tts_service::synthesize_speech_sync(
                    &text,
                    voice.as_deref(),
                    adapter.as_deref()
                );

                HandleResult::Json(match result {
                    Ok(synthesis) => {
                        let adapter_name = adapter.unwrap_or_else(|| "default".to_string());
                        let info = self.audio_pool.store(
                            synthesis.samples,
                            synthesis.sample_rate,
                            synthesis.duration_ms,
                            &adapter_name,
                        );

                        log_info!(
                            "ipc", "voice_synthesize_handle",
                            "Stored handle {} ({} samples, {}ms, {})",
                            &info.handle[..8], info.sample_count, info.duration_ms, info.adapter
                        );

                        Response::success(serde_json::json!({
                            "handle": info.handle,
                            "sample_count": info.sample_count,
                            "sample_rate": info.sample_rate,
                            "duration_ms": info.duration_ms,
                            "adapter": info.adapter,
                        }))
                    },
                    Err(e) => {
                        log_error!("ipc", "voice_synthesize_handle", "TTS failed: {}", e);
                        Response::error(format!("TTS failed: {}", e))
                    }
                })
            }

            Request::VoicePlayHandle { handle, call_id, user_id } => {
                let _timer = TimingGuard::new("ipc", "voice_play_handle");

                use crate::voice::handle::Handle as VoiceHandle;

                let voice_handle: VoiceHandle = match handle.parse() {
                    Ok(h) => h,
                    Err(e) => {
                        return HandleResult::Json(Response::error(
                            format!("Invalid handle UUID: {}", e)
                        ));
                    }
                };

                // Retrieve audio from buffer pool
                let samples = match self.audio_pool.get(&voice_handle) {
                    Some(s) => s,
                    None => {
                        return HandleResult::Json(Response::error(
                            format!("Audio handle not found or expired: {}", &handle[..8.min(handle.len())])
                        ));
                    }
                };

                let sample_count = samples.len();
                let duration_ms = (sample_count as u64 * 1000) / crate::audio_constants::AUDIO_SAMPLE_RATE as u64;

                // Inject into call mixer
                let call_manager = self.call_manager.clone();
                let result = self.rt_handle.block_on(async {
                    call_manager.inject_audio(&call_id, &user_id, samples).await
                });

                HandleResult::Json(match result {
                    Ok(()) => {
                        log_info!(
                            "ipc", "voice_play_handle",
                            "Injected handle {} into call {} ({} samples, {}ms)",
                            &handle[..8.min(handle.len())], call_id, sample_count, duration_ms
                        );
                        Response::success(serde_json::json!({
                            "injected": true,
                            "sample_count": sample_count,
                            "duration_ms": duration_ms,
                        }))
                    },
                    Err(e) => {
                        log_error!("ipc", "voice_play_handle", "Injection failed: {}", e);
                        Response::error(format!("Audio injection failed: {}", e))
                    }
                })
            }

            Request::VoiceDiscardHandle { handle } => {
                use crate::voice::handle::Handle as VoiceHandle;

                let voice_handle: VoiceHandle = match handle.parse() {
                    Ok(h) => h,
                    Err(e) => {
                        return HandleResult::Json(Response::error(
                            format!("Invalid handle UUID: {}", e)
                        ));
                    }
                };

                let discarded = self.audio_pool.discard(&voice_handle);

                HandleResult::Json(Response::success(serde_json::json!({
                    "discarded": discarded,
                })))
            }

            Request::VoiceTranscribe { audio, language } => {
                let _timer = TimingGuard::new("ipc", "voice_transcribe");

                use crate::voice::stt_service;
                use base64::Engine;

                // Decode base64 audio (STT input is small — base64 is acceptable here)
                let bytes = match base64::engine::general_purpose::STANDARD.decode(&audio) {
                    Ok(b) => b,
                    Err(e) => {
                        log_error!("ipc", "voice_transcribe", "Base64 decode failed: {}", e);
                        return HandleResult::Json(Response::error(format!("Base64 decode failed: {}", e)));
                    }
                };

                // Convert bytes to i16 samples
                if bytes.len() % 2 != 0 {
                    return HandleResult::Json(Response::error("Audio data must have even length (i16 samples)".into()));
                }
                let samples: Vec<i16> = bytes
                    .chunks_exact(2)
                    .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
                    .collect();

                log_info!(
                    "ipc", "voice_transcribe",
                    "Transcribing {} samples ({:.1}s)",
                    samples.len(),
                    samples.len() as f64 / crate::audio_constants::AUDIO_SAMPLE_RATE as f64
                );

                let result = stt_service::transcribe_speech_sync(
                    &samples,
                    language.as_deref()
                );

                HandleResult::Json(match result {
                    Ok(transcript) => {
                        log_info!(
                            "ipc", "voice_transcribe",
                            "Transcribed: \"{}\" (confidence: {:.2})",
                            transcript.text,
                            transcript.confidence
                        );

                        Response::success(serde_json::json!({
                            "text": transcript.text,
                            "language": transcript.language,
                            "confidence": transcript.confidence,
                            "segments": transcript.segments.iter().map(|s| {
                                serde_json::json!({
                                    "text": s.text,
                                    "start_ms": s.start_ms,
                                    "end_ms": s.end_ms
                                })
                            }).collect::<Vec<_>>()
                        }))
                    },
                    Err(e) => {
                        log_error!("ipc", "voice_transcribe", "STT failed: {}", e);
                        Response::error(format!("STT failed: {}", e))
                    }
                })
            }

            Request::InboxCreate { persona_id } => {
                let _timer = TimingGuard::new("ipc", "inbox_create");

                let persona_uuid = match Uuid::parse_str(&persona_id) {
                    Ok(u) => u,
                    Err(e) => return HandleResult::Json(Response::error(format!("Invalid persona_id: {e}"))),
                };

                let inbox = PersonaInbox::new(persona_uuid);
                self.inboxes.insert(persona_uuid, inbox);

                HandleResult::Json(Response::success(serde_json::json!({ "created": true })))
            }

            // ================================================================
            // Cognition Handlers
            // ================================================================

            Request::CognitionCreateEngine { persona_id, persona_name } => {
                let _timer = TimingGuard::new("ipc", "cognition_create_engine");

                let persona_uuid = match Uuid::parse_str(&persona_id) {
                    Ok(u) => u,
                    Err(e) => return HandleResult::Json(Response::error(format!("Invalid persona_id: {e}"))),
                };

                let (_, shutdown_rx) = tokio::sync::watch::channel(false);
                let engine = PersonaCognitionEngine::new(
                    persona_uuid,
                    persona_name,
                    self.rag_engine.clone(),
                    shutdown_rx,
                );

                self.cognition_engines.insert(persona_uuid, engine);

                log_info!("ipc", "cognition", "Created cognition engine for {}", persona_id);
                HandleResult::Json(Response::success(serde_json::json!({ "created": true })))
            }

            Request::CognitionCalculatePriority {
                persona_id, content, sender_type, is_voice, room_id, timestamp
            } => {
                let _timer = TimingGuard::new("ipc", "cognition_calculate_priority");

                let persona_uuid = match Uuid::parse_str(&persona_id) {
                    Ok(u) => u,
                    Err(e) => return HandleResult::Json(Response::error(format!("Invalid persona_id: {e}"))),
                };

                let room_uuid = match Uuid::parse_str(&room_id) {
                    Ok(u) => u,
                    Err(e) => return HandleResult::Json(Response::error(format!("Invalid room_id: {e}"))),
                };

                let sender = match sender_type.as_str() {
                    "human" => SenderType::Human,
                    "persona" => SenderType::Persona,
                    "agent" => SenderType::Agent,
                    "system" => SenderType::System,
                    _ => return HandleResult::Json(Response::error(format!("Invalid sender_type: {}", sender_type))),
                };

                let engine = match self.cognition_engines.get(&persona_uuid) {
                    Some(e) => e,
                    None => return HandleResult::Json(Response::error(format!("No cognition engine for {}", persona_id))),
                };

                let score = engine.calculate_priority(&content, sender, is_voice, room_uuid, timestamp);

                HandleResult::Json(Response::success(serde_json::json!({
                    "score": score.score,
                    "factors": {
                        "recency_score": score.factors.recency_score,
                        "mention_score": score.factors.mention_score,
                        "room_score": score.factors.room_score,
                        "sender_score": score.factors.sender_score,
                        "voice_boost": score.factors.voice_boost,
                    }
                })))
            }

            Request::CognitionFastPathDecision { persona_id, message } => {
                let _timer = TimingGuard::new("ipc", "cognition_fast_path_decision");

                let persona_uuid = match Uuid::parse_str(&persona_id) {
                    Ok(u) => u,
                    Err(e) => return HandleResult::Json(Response::error(format!("Invalid persona_id: {e}"))),
                };

                let inbox_msg = match message.to_inbox_message() {
                    Ok(m) => m,
                    Err(e) => return HandleResult::Json(Response::error(e)),
                };

                let engine = match self.cognition_engines.get(&persona_uuid) {
                    Some(e) => e,
                    None => return HandleResult::Json(Response::error(format!("No cognition engine for {}", persona_id))),
                };

                let decision = engine.fast_path_decision(&inbox_msg);

                HandleResult::Json(Response::success(serde_json::json!({
                    "should_respond": decision.should_respond,
                    "confidence": decision.confidence,
                    "reason": decision.reason,
                    "decision_time_ms": decision.decision_time_ms,
                    "fast_path_used": decision.fast_path_used,
                })))
            }

            Request::CognitionEnqueueMessage { persona_id, message } => {
                let _timer = TimingGuard::new("ipc", "cognition_enqueue_message");

                let persona_uuid = match Uuid::parse_str(&persona_id) {
                    Ok(u) => u,
                    Err(e) => return HandleResult::Json(Response::error(format!("Invalid persona_id: {e}"))),
                };

                let inbox_msg = match message.to_inbox_message() {
                    Ok(m) => m,
                    Err(e) => return HandleResult::Json(Response::error(e)),
                };

                let engine = match self.cognition_engines.get(&persona_uuid) {
                    Some(e) => e,
                    None => return HandleResult::Json(Response::error(format!("No cognition engine for {}", persona_id))),
                };

                engine.enqueue_message(inbox_msg);

                HandleResult::Json(Response::success(serde_json::json!({ "enqueued": true })))
            }

            Request::CognitionGetState { persona_id } => {
                let _timer = TimingGuard::new("ipc", "cognition_get_state");

                let persona_uuid = match Uuid::parse_str(&persona_id) {
                    Ok(u) => u,
                    Err(e) => return HandleResult::Json(Response::error(format!("Invalid persona_id: {e}"))),
                };

                let engine = match self.cognition_engines.get(&persona_uuid) {
                    Some(e) => e,
                    None => return HandleResult::Json(Response::error(format!("No cognition engine for {}", persona_id))),
                };

                let state = engine.state();

                HandleResult::Json(Response::success(serde_json::json!({
                    "energy": state.energy,
                    "attention": state.attention,
                    "mood": format!("{:?}", state.mood).to_lowercase(),
                    "inbox_load": state.inbox_load,
                    "last_activity_time": state.last_activity_time,
                    "response_count": state.response_count,
                    "compute_budget": state.compute_budget,
                    "service_cadence_ms": state.service_cadence_ms(),
                })))
            }

            // ================================================================
            // Channel Handlers
            // ================================================================

            Request::ChannelEnqueue { persona_id, item } => {
                let _timer = TimingGuard::new("ipc", "channel_enqueue");

                let persona_uuid = match Uuid::parse_str(&persona_id) {
                    Ok(u) => u,
                    Err(e) => return HandleResult::Json(Response::error(format!("Invalid persona_id: {e}"))),
                };

                let queue_item = match item.to_queue_item() {
                    Ok(qi) => qi,
                    Err(e) => return HandleResult::Json(Response::error(e)),
                };

                let mut entry = self.channel_registries
                    .entry(persona_uuid)
                    .or_insert_with(|| (ChannelRegistry::new(), PersonaState::new()));
                let (registry, _state) = entry.value_mut();

                match registry.route(queue_item) {
                    Ok(domain) => {
                        let status = registry.status();
                        HandleResult::Json(Response::success(serde_json::json!({
                            "routed_to": domain,
                            "status": status,
                        })))
                    }
                    Err(e) => HandleResult::Json(Response::error(e)),
                }
            }

            Request::ChannelDequeue { persona_id, domain } => {
                let _timer = TimingGuard::new("ipc", "channel_dequeue");

                let persona_uuid = match Uuid::parse_str(&persona_id) {
                    Ok(u) => u,
                    Err(e) => return HandleResult::Json(Response::error(format!("Invalid persona_id: {e}"))),
                };

                let mut entry = match self.channel_registries.get_mut(&persona_uuid) {
                    Some(r) => r,
                    None => return HandleResult::Json(Response::error(format!("No channel registry for {persona_id}"))),
                };
                let (registry, _state) = entry.value_mut();

                // Parse optional domain filter
                let target_domain: Option<ActivityDomain> = match &domain {
                    Some(d) => match serde_json::from_value::<ActivityDomain>(serde_json::json!(d)) {
                        Ok(ad) => Some(ad),
                        Err(e) => return HandleResult::Json(Response::error(format!("Invalid domain '{d}': {e}"))),
                    },
                    None => None,
                };

                let item = match target_domain {
                    Some(d) => registry.get_mut(d).and_then(|ch| ch.pop()),
                    None => {
                        // Pop from highest-priority channel that has work
                        use crate::persona::channel_types::DOMAIN_PRIORITY_ORDER;
                        let mut popped = None;
                        for &d in DOMAIN_PRIORITY_ORDER {
                            if let Some(ch) = registry.get_mut(d) {
                                if ch.has_work() {
                                    popped = ch.pop();
                                    break;
                                }
                            }
                        }
                        popped
                    }
                };

                match item {
                    Some(qi) => HandleResult::Json(Response::success(serde_json::json!({
                        "item": qi.to_json(),
                        "has_more": registry.has_work(),
                    }))),
                    None => HandleResult::Json(Response::success(serde_json::json!({
                        "item": null,
                        "has_more": false,
                    }))),
                }
            }

            Request::ChannelStatus { persona_id } => {
                let _timer = TimingGuard::new("ipc", "channel_status");

                let persona_uuid = match Uuid::parse_str(&persona_id) {
                    Ok(u) => u,
                    Err(e) => return HandleResult::Json(Response::error(format!("Invalid persona_id: {e}"))),
                };

                let entry = match self.channel_registries.get(&persona_uuid) {
                    Some(r) => r,
                    None => {
                        // Return empty status if no registry exists yet
                        return HandleResult::Json(Response::success(serde_json::json!({
                            "channels": [],
                            "total_size": 0,
                            "has_urgent_work": false,
                            "has_work": false,
                        })));
                    }
                };
                let (registry, _state) = entry.value();

                let status = registry.status();
                HandleResult::Json(Response::success(serde_json::to_value(&status).unwrap_or_default()))
            }

            Request::ChannelServiceCycle { persona_id } => {
                let _timer = TimingGuard::new("ipc", "channel_service_cycle");

                let persona_uuid = match Uuid::parse_str(&persona_id) {
                    Ok(u) => u,
                    Err(e) => return HandleResult::Json(Response::error(format!("Invalid persona_id: {e}"))),
                };

                let mut entry = self.channel_registries
                    .entry(persona_uuid)
                    .or_insert_with(|| (ChannelRegistry::new(), PersonaState::new()));
                let (registry, state) = entry.value_mut();

                let result = registry.service_cycle(state);
                HandleResult::Json(Response::success(serde_json::to_value(&result).unwrap_or_default()))
            }

            Request::ChannelServiceCycleFull { persona_id } => {
                let _timer = TimingGuard::new("ipc", "channel_service_cycle_full");

                let persona_uuid = match Uuid::parse_str(&persona_id) {
                    Ok(u) => u,
                    Err(e) => return HandleResult::Json(Response::error(format!("Invalid persona_id: {e}"))),
                };

                // Step 1: Service cycle — consolidate, schedule, return next item
                let mut entry = self.channel_registries
                    .entry(persona_uuid)
                    .or_insert_with(|| (ChannelRegistry::new(), PersonaState::new()));
                let (registry, state) = entry.value_mut();
                let service_result = registry.service_cycle(state);
                drop(entry); // Release channel_registries lock before acquiring cognition_engines

                // Step 2: If item returned, run fast_path_decision in the SAME IPC call
                let decision = if service_result.should_process {
                    if let Some(ref item_json) = service_result.item {
                        // Reconstruct InboxMessage from queue item JSON
                        let id = item_json.get("id")
                            .and_then(|v| v.as_str())
                            .and_then(|s| Uuid::parse_str(s).ok())
                            .unwrap_or_default();
                        let sender_id = item_json.get("senderId")
                            .and_then(|v| v.as_str())
                            .and_then(|s| Uuid::parse_str(s).ok())
                            .unwrap_or_default();
                        let room_id = item_json.get("roomId")
                            .and_then(|v| v.as_str())
                            .and_then(|s| Uuid::parse_str(s).ok())
                            .unwrap_or_default();
                        let sender_name = item_json.get("senderName")
                            .and_then(|v| v.as_str())
                            .unwrap_or("Unknown")
                            .to_string();
                        let sender_type_str = item_json.get("senderType")
                            .and_then(|v| v.as_str())
                            .unwrap_or("system");
                        let content = item_json.get("content")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let priority = item_json.get("priority")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.5) as f32;
                        let timestamp = item_json.get("timestamp")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0);

                        let sender_type = match sender_type_str {
                            "human" => SenderType::Human,
                            "persona" => SenderType::Persona,
                            "agent" => SenderType::Agent,
                            _ => SenderType::System,
                        };

                        let inbox_msg = InboxMessage {
                            id,
                            room_id,
                            sender_id,
                            sender_name,
                            sender_type,
                            content,
                            timestamp,
                            priority,
                            source_modality: None,
                            voice_session_id: None,
                        };

                        // Run fast_path_decision on cognition engine
                        if let Some(engine) = self.cognition_engines.get(&persona_uuid) {
                            let d = engine.fast_path_decision(&inbox_msg);
                            Some(serde_json::json!({
                                "should_respond": d.should_respond,
                                "confidence": d.confidence,
                                "reason": d.reason,
                                "decision_time_ms": d.decision_time_ms,
                                "fast_path_used": d.fast_path_used,
                            }))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };

                // Return combined result: service cycle + optional decision
                let mut result_json = serde_json::to_value(&service_result).unwrap_or_default();
                if let Some(decision_val) = decision {
                    result_json["decision"] = decision_val;
                }
                HandleResult::Json(Response::success(result_json))
            }

            Request::ChannelClear { persona_id } => {
                let _timer = TimingGuard::new("ipc", "channel_clear");

                let persona_uuid = match Uuid::parse_str(&persona_id) {
                    Ok(u) => u,
                    Err(e) => return HandleResult::Json(Response::error(format!("Invalid persona_id: {e}"))),
                };

                if let Some(mut entry) = self.channel_registries.get_mut(&persona_uuid) {
                    let (registry, _state) = entry.value_mut();
                    registry.clear_all();
                }

                HandleResult::Json(Response::success(serde_json::json!({ "cleared": true })))
            }

            // ================================================================
            // Memory / Hippocampus Handlers
            // ================================================================

            Request::MemoryLoadCorpus { persona_id, memories, events } => {
                let _timer = TimingGuard::new("ipc", "memory_load_corpus");

                let resp = self.memory_manager.load_corpus(&persona_id, memories, events);
                log_info!(
                    "ipc", "memory_load_corpus",
                    "Loaded corpus for {}: {} memories ({} embedded), {} events ({} embedded), {:.1}ms",
                    persona_id, resp.memory_count, resp.embedded_memory_count,
                    resp.timeline_event_count, resp.embedded_event_count, resp.load_time_ms
                );
                HandleResult::Json(Response::success(serde_json::to_value(&resp).unwrap_or_default()))
            }

            Request::MemoryMultiLayerRecall { persona_id, query_text, room_id, max_results, layers } => {
                let _timer = TimingGuard::new("ipc", "memory_multi_layer_recall");

                let req = crate::memory::MultiLayerRecallRequest {
                    query_text,
                    room_id,
                    max_results,
                    layers,
                };

                HandleResult::Json(match self.memory_manager.multi_layer_recall(&persona_id, &req) {
                    Ok(resp) => {
                        log_info!(
                            "ipc", "memory_multi_layer_recall",
                            "Multi-layer recall for {}: {} memories in {:.1}ms ({} candidates from {} layers)",
                            persona_id, resp.memories.len(), resp.recall_time_ms,
                            resp.total_candidates, resp.layer_timings.len()
                        );
                        Response::success(serde_json::to_value(&resp).unwrap_or_default())
                    }
                    Err(e) => Response::error(format!("memory/multi-layer-recall failed: {e}")),
                })
            }

            Request::MemoryConsciousnessContext { persona_id, room_id, current_message, skip_semantic_search } => {
                let _timer = TimingGuard::new("ipc", "memory_consciousness_context");

                let req = crate::memory::ConsciousnessContextRequest {
                    room_id,
                    current_message,
                    skip_semantic_search,
                };

                HandleResult::Json(match self.memory_manager.consciousness_context(&persona_id, &req) {
                    Ok(resp) => {
                        log_info!(
                            "ipc", "memory_consciousness_context",
                            "Consciousness context for {}: {:.1}ms, {} cross-context events, {} intentions",
                            persona_id, resp.build_time_ms, resp.cross_context_event_count, resp.active_intention_count
                        );
                        Response::success(serde_json::to_value(&resp).unwrap_or_default())
                    }
                    Err(e) => Response::error(format!("memory/consciousness-context failed: {e}")),
                })
            }

            Request::MemoryAppendMemory { persona_id, memory } => {
                let _timer = TimingGuard::new("ipc", "memory_append_memory");

                HandleResult::Json(match self.memory_manager.append_memory(&persona_id, memory) {
                    Ok(()) => {
                        log_debug!("ipc", "memory_append_memory", "Appended memory to corpus for {}", persona_id);
                        Response::success(serde_json::json!({ "appended": true }))
                    }
                    Err(e) => Response::error(format!("memory/append-memory failed: {e}")),
                })
            }

            Request::MemoryAppendEvent { persona_id, event } => {
                let _timer = TimingGuard::new("ipc", "memory_append_event");

                HandleResult::Json(match self.memory_manager.append_event(&persona_id, event) {
                    Ok(()) => {
                        log_debug!("ipc", "memory_append_event", "Appended event to corpus for {}", persona_id);
                        Response::success(serde_json::json!({ "appended": true }))
                    }
                    Err(e) => Response::error(format!("memory/append-event failed: {e}")),
                })
            }

            Request::HealthCheck => {
                HandleResult::Json(Response::success(serde_json::json!({ "healthy": true })))
            }

            Request::GetStats { category: _ } => {
                HandleResult::Json(Response::success(serde_json::json!({
                    "note": "Performance stats tracking not yet implemented"
                })))
            }
        }
    }
}

// ============================================================================
// Handle Result - supports JSON and binary responses
// ============================================================================

/// Result from handling an IPC request.
/// Binary variant allows raw PCM audio to bypass base64 encoding entirely.
enum HandleResult {
    /// Standard JSON response (all non-audio commands)
    Json(Response),
    /// Binary response: JSON metadata + raw bytes (audio commands)
    /// Eliminates base64 encoding overhead for audio data.
    Binary {
        json_header: Response,
        binary_data: Vec<u8>,
    },
}

// ============================================================================
// Connection Handler - Length-Prefixed Binary Framing
// ============================================================================

/// Send a length-prefixed JSON response frame.
/// Frame format: [4 bytes u32 BE length][JSON payload bytes]
fn send_json_frame(stream: &mut UnixStream, response: &Response) -> std::io::Result<()> {
    let json = match serde_json::to_string(response) {
        Ok(j) => j,
        Err(e) => {
            log_error!("ipc", "server", "Failed to serialize response: {}", e);
            r#"{"success":false,"error":"Internal serialization error"}"#.to_string()
        }
    };
    let payload = json.as_bytes();
    let length = payload.len() as u32;

    stream.write_all(&length.to_be_bytes())?;
    stream.write_all(payload)?;
    stream.flush()
}

/// Send a length-prefixed binary response frame.
/// Frame format: [4 bytes u32 BE total_length][JSON header bytes][\0][raw binary bytes]
/// The \0 separator is unambiguous — serde_json encodes null chars as \u0000.
fn send_binary_frame(stream: &mut UnixStream, response: &Response, binary_data: &[u8]) -> std::io::Result<()> {
    let json = match serde_json::to_string(response) {
        Ok(j) => j,
        Err(e) => {
            log_error!("ipc", "server", "Failed to serialize binary response header: {}", e);
            r#"{"success":false,"error":"Internal serialization error"}"#.to_string()
        }
    };
    let json_bytes = json.as_bytes();
    let total_length = (json_bytes.len() + 1 + binary_data.len()) as u32; // +1 for \0 separator

    stream.write_all(&total_length.to_be_bytes())?;
    stream.write_all(json_bytes)?;
    stream.write_all(&[0u8])?; // separator
    stream.write_all(binary_data)?;
    stream.flush()
}

fn handle_client(mut stream: UnixStream, state: Arc<ServerState>) -> std::io::Result<()> {
    let peer_addr = stream.peer_addr()?;
    log_debug!("ipc", "server", "Client connected: {:?}", peer_addr);

    // Requests still arrive as newline-delimited JSON (small control messages).
    // Responses use length-prefixed binary framing (supports large audio payloads).
    let reader = BufReader::new(stream.try_clone()?);

    for line in reader.lines() {
        let line = line?;
        if line.is_empty() {
            continue;
        }

        // Parse JSON to extract requestId first
        let json_value: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                let response = Response::error(format!("Invalid JSON: {e}"));
                send_json_frame(&mut stream, &response)?;
                continue;
            }
        };

        // Extract requestId if present
        let request_id = json_value.get("requestId").and_then(|v| v.as_u64());

        // Parse request
        let request: Request = match serde_json::from_value(json_value) {
            Ok(r) => r,
            Err(e) => {
                let response = Response::error(format!("Invalid request: {e}")).with_request_id(request_id);
                send_json_frame(&mut stream, &response)?;
                continue;
            }
        };

        // Handle request
        let result = state.handle_request(request);

        // Send response using appropriate framing
        match result {
            HandleResult::Json(response) => {
                let response = response.with_request_id(request_id);
                send_json_frame(&mut stream, &response)?;
            }
            HandleResult::Binary { json_header, binary_data } => {
                let json_header = json_header.with_request_id(request_id);
                send_binary_frame(&mut stream, &json_header, &binary_data)?;
            }
        }
    }

    log_debug!("ipc", "server", "Client disconnected: {:?}", peer_addr);
    Ok(())
}

// ============================================================================
// Tests - Binary Framing & Protocol
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Binary Framing Unit Tests
    // ========================================================================

    #[test]
    fn test_json_frame_roundtrip() {
        // Create a response, write to buffer, verify framing
        let response = Response::success(serde_json::json!({"healthy": true}));
        let json = serde_json::to_string(&response).unwrap();
        let payload = json.as_bytes();

        // Build frame: [4-byte BE length][payload]
        let length = payload.len() as u32;
        let mut frame = Vec::new();
        frame.extend_from_slice(&length.to_be_bytes());
        frame.extend_from_slice(payload);

        // Parse frame
        assert!(frame.len() >= 4);
        let parsed_length = u32::from_be_bytes([frame[0], frame[1], frame[2], frame[3]]) as usize;
        assert_eq!(parsed_length, payload.len());

        let parsed_json: serde_json::Value = serde_json::from_slice(&frame[4..4 + parsed_length]).unwrap();
        assert_eq!(parsed_json["success"], true);
        assert_eq!(parsed_json["result"]["healthy"], true);
    }

    #[test]
    fn test_binary_frame_roundtrip() {
        // Simulate binary response: JSON header + \0 + raw PCM
        let response = Response::success(serde_json::json!({
            "sample_rate": 16000,
            "duration_ms": 500,
            "binary_pcm": true
        }));
        let json = serde_json::to_string(&response).unwrap();
        let json_bytes = json.as_bytes();

        // Simulate PCM audio data (4 samples of i16)
        let audio_samples: Vec<i16> = vec![1000, -2000, 3000, -4000];
        let pcm_bytes: Vec<u8> = audio_samples.iter()
            .flat_map(|s| s.to_le_bytes())
            .collect();

        // Build binary frame: [4-byte BE total_length][JSON][\0][PCM]
        let total_length = (json_bytes.len() + 1 + pcm_bytes.len()) as u32;
        let mut frame = Vec::new();
        frame.extend_from_slice(&total_length.to_be_bytes());
        frame.extend_from_slice(json_bytes);
        frame.push(0u8); // separator
        frame.extend_from_slice(&pcm_bytes);

        // Parse frame
        let parsed_total = u32::from_be_bytes([frame[0], frame[1], frame[2], frame[3]]) as usize;
        let payload = &frame[4..4 + parsed_total];

        // Find \0 separator
        let sep_idx = payload.iter().position(|&b| b == 0).expect("Should have separator");
        let parsed_json_bytes = &payload[..sep_idx];
        let parsed_binary = &payload[sep_idx + 1..];

        // Verify JSON header
        let parsed: serde_json::Value = serde_json::from_slice(parsed_json_bytes).unwrap();
        assert_eq!(parsed["result"]["sample_rate"], 16000);
        assert_eq!(parsed["result"]["binary_pcm"], true);

        // Verify binary PCM data
        assert_eq!(parsed_binary.len(), pcm_bytes.len());
        let parsed_samples: Vec<i16> = parsed_binary
            .chunks_exact(2)
            .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
            .collect();
        assert_eq!(parsed_samples, audio_samples);
    }

    #[test]
    fn test_binary_frame_separator_unambiguous() {
        // Verify that serde_json never produces a raw 0x00 byte
        // (it encodes null chars as \u0000, which is 6 ASCII bytes)
        let json_with_null = serde_json::json!({"text": "before\0after"});
        let serialized = serde_json::to_string(&json_with_null).unwrap();
        let bytes = serialized.as_bytes();

        // Should NOT contain raw 0x00 byte
        assert!(!bytes.contains(&0u8),
            "serde_json should never emit raw 0x00 byte, got: {:?}", serialized);
        // Should contain the escaped form
        assert!(serialized.contains("\\u0000"),
            "Null should be escaped as \\u0000");
    }

    // ========================================================================
    // Request/Response Serialization Tests
    // ========================================================================

    #[test]
    fn test_request_deserialization_health_check() {
        let json = r#"{"command":"health-check"}"#;
        let request: Request = serde_json::from_str(json).expect("Should parse health-check");
        match request {
            Request::HealthCheck => {} // correct
            _ => panic!("Expected HealthCheck variant"),
        }
    }

    #[test]
    fn test_request_deserialization_voice_synthesize() {
        let json = r#"{"command":"voice/synthesize","text":"Hello","voice":"af","adapter":"kokoro"}"#;
        let request: Request = serde_json::from_str(json).expect("Should parse voice/synthesize");
        match request {
            Request::VoiceSynthesize { text, voice, adapter } => {
                assert_eq!(text, "Hello");
                assert_eq!(voice, Some("af".to_string()));
                assert_eq!(adapter, Some("kokoro".to_string()));
            }
            _ => panic!("Expected VoiceSynthesize variant"),
        }
    }

    #[test]
    fn test_request_deserialization_voice_synthesize_minimal() {
        let json = r#"{"command":"voice/synthesize","text":"Hello"}"#;
        let request: Request = serde_json::from_str(json).expect("Should parse minimal synthesize");
        match request {
            Request::VoiceSynthesize { text, voice, adapter } => {
                assert_eq!(text, "Hello");
                assert!(voice.is_none());
                assert!(adapter.is_none());
            }
            _ => panic!("Expected VoiceSynthesize variant"),
        }
    }

    #[test]
    fn test_request_deserialization_speak_in_call() {
        let json = r#"{
            "command": "voice/speak-in-call",
            "call_id": "call-123",
            "user_id": "user-456",
            "text": "Hello there"
        }"#;
        let request: Request = serde_json::from_str(json).expect("Should parse speak-in-call");
        match request {
            Request::VoiceSpeakInCall { call_id, user_id, text, voice, adapter } => {
                assert_eq!(call_id, "call-123");
                assert_eq!(user_id, "user-456");
                assert_eq!(text, "Hello there");
                assert!(voice.is_none());
                assert!(adapter.is_none());
            }
            _ => panic!("Expected VoiceSpeakInCall variant"),
        }
    }

    #[test]
    fn test_response_success_serialization() {
        let response = Response::success(serde_json::json!({"key": "value"}));
        let json = serde_json::to_string(&response).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["success"], true);
        assert_eq!(parsed["result"]["key"], "value");
        assert!(parsed.get("error").is_none() || parsed["error"].is_null());
    }

    #[test]
    fn test_response_error_serialization() {
        let response = Response::error("something broke".to_string());
        let json = serde_json::to_string(&response).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["success"], false);
        assert_eq!(parsed["error"], "something broke");
    }

    #[test]
    fn test_response_with_request_id() {
        let response = Response::success(serde_json::json!({})).with_request_id(Some(42));
        let json = serde_json::to_string(&response).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["requestId"], 42);
    }

    #[test]
    fn test_inbox_message_request_to_inbox_message() {
        let request = InboxMessageRequest {
            id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            room_id: "660e8400-e29b-41d4-a716-446655440000".to_string(),
            sender_id: "770e8400-e29b-41d4-a716-446655440000".to_string(),
            sender_name: "Test User".to_string(),
            sender_type: "human".to_string(),
            content: "Hello".to_string(),
            timestamp: 1234567890,
            priority: 0.5,
            source_modality: Some("voice".to_string()),
            voice_session_id: Some("880e8400-e29b-41d4-a716-446655440000".to_string()),
        };

        let inbox_msg = request.to_inbox_message().expect("Should convert");
        assert_eq!(inbox_msg.content, "Hello");
        assert_eq!(inbox_msg.sender_name, "Test User");
        assert!(matches!(inbox_msg.sender_type, SenderType::Human));
        assert!(matches!(inbox_msg.source_modality, Some(Modality::Voice)));
        assert!(inbox_msg.voice_session_id.is_some());
    }

    #[test]
    fn test_inbox_message_request_invalid_uuid() {
        let request = InboxMessageRequest {
            id: "not-a-uuid".to_string(),
            room_id: "also-invalid".to_string(),
            sender_id: "nope".to_string(),
            sender_name: "Test".to_string(),
            sender_type: "human".to_string(),
            content: "Hello".to_string(),
            timestamp: 0,
            priority: 0.0,
            source_modality: None,
            voice_session_id: None,
        };

        let result = request.to_inbox_message();
        assert!(result.is_err(), "Invalid UUIDs should fail");
    }

    // ========================================================================
    // Handle-Based Audio IPC Request Deserialization
    // ========================================================================

    #[test]
    fn test_request_deserialization_synthesize_handle() {
        let json = r#"{"command":"voice/synthesize-handle","text":"Hello world","voice":"af","adapter":"kokoro"}"#;
        let request: Request = serde_json::from_str(json).expect("Should parse synthesize-handle");
        match request {
            Request::VoiceSynthesizeHandle { text, voice, adapter } => {
                assert_eq!(text, "Hello world");
                assert_eq!(voice, Some("af".to_string()));
                assert_eq!(adapter, Some("kokoro".to_string()));
            }
            _ => panic!("Expected VoiceSynthesizeHandle variant"),
        }
    }

    #[test]
    fn test_request_deserialization_play_handle() {
        let json = r#"{"command":"voice/play-handle","handle":"550e8400-e29b-41d4-a716-446655440000","call_id":"call-1","user_id":"user-1"}"#;
        let request: Request = serde_json::from_str(json).expect("Should parse play-handle");
        match request {
            Request::VoicePlayHandle { handle, call_id, user_id } => {
                assert_eq!(handle, "550e8400-e29b-41d4-a716-446655440000");
                assert_eq!(call_id, "call-1");
                assert_eq!(user_id, "user-1");
            }
            _ => panic!("Expected VoicePlayHandle variant"),
        }
    }

    #[test]
    fn test_request_deserialization_discard_handle() {
        let json = r#"{"command":"voice/discard-handle","handle":"550e8400-e29b-41d4-a716-446655440000"}"#;
        let request: Request = serde_json::from_str(json).expect("Should parse discard-handle");
        match request {
            Request::VoiceDiscardHandle { handle } => {
                assert_eq!(handle, "550e8400-e29b-41d4-a716-446655440000");
            }
            _ => panic!("Expected VoiceDiscardHandle variant"),
        }
    }

    // ========================================================================
    // Channel Command Deserialization Tests
    // ========================================================================

    #[test]
    fn test_request_deserialization_channel_enqueue_chat() {
        let json = r#"{
            "command": "channel/enqueue",
            "persona_id": "550e8400-e29b-41d4-a716-446655440000",
            "item": {
                "item_type": "chat",
                "id": "660e8400-e29b-41d4-a716-446655440000",
                "room_id": "770e8400-e29b-41d4-a716-446655440000",
                "content": "Hello team",
                "sender_id": "880e8400-e29b-41d4-a716-446655440000",
                "sender_name": "Joel",
                "sender_type": "human",
                "mentions": true,
                "timestamp": 1234567890,
                "priority": 0.7
            }
        }"#;
        let request: Request = serde_json::from_str(json).expect("Should parse channel/enqueue");
        match request {
            Request::ChannelEnqueue { persona_id, item } => {
                assert_eq!(persona_id, "550e8400-e29b-41d4-a716-446655440000");
                let queue_item = item.to_queue_item().expect("Should convert to queue item");
                assert_eq!(queue_item.item_type(), "chat");
                assert!(queue_item.is_urgent()); // mentions = true
            }
            _ => panic!("Expected ChannelEnqueue variant"),
        }
    }

    #[test]
    fn test_request_deserialization_channel_service_cycle() {
        let json = r#"{"command":"channel/service-cycle","persona_id":"550e8400-e29b-41d4-a716-446655440000"}"#;
        let request: Request = serde_json::from_str(json).expect("Should parse channel/service-cycle");
        match request {
            Request::ChannelServiceCycle { persona_id } => {
                assert_eq!(persona_id, "550e8400-e29b-41d4-a716-446655440000");
            }
            _ => panic!("Expected ChannelServiceCycle variant"),
        }
    }

    #[test]
    fn test_request_deserialization_channel_status() {
        let json = r#"{"command":"channel/status","persona_id":"550e8400-e29b-41d4-a716-446655440000"}"#;
        let request: Request = serde_json::from_str(json).expect("Should parse channel/status");
        match request {
            Request::ChannelStatus { persona_id } => {
                assert_eq!(persona_id, "550e8400-e29b-41d4-a716-446655440000");
            }
            _ => panic!("Expected ChannelStatus variant"),
        }
    }

    // ========================================================================
    // Integration Test: Full IPC Round-Trip via Unix Socket
    // Requires: continuum-core-server running (cargo test --ignored)
    // ========================================================================

    #[test]
    #[ignore] // Requires running continuum-core server
    fn test_ipc_health_check_live() {
        use std::io::Write;
        use std::os::unix::net::UnixStream;

        let socket_path = "/tmp/continuum-core.sock";
        let mut stream = UnixStream::connect(socket_path)
            .expect("Failed to connect to continuum-core socket");

        // Send health-check request
        let request = r#"{"command":"health-check","requestId":1}"#;
        stream.write_all(request.as_bytes()).unwrap();
        stream.write_all(b"\n").unwrap();
        stream.flush().unwrap();

        // Read length-prefixed response
        let mut len_buf = [0u8; 4];
        std::io::Read::read_exact(&mut stream, &mut len_buf).unwrap();
        let length = u32::from_be_bytes(len_buf) as usize;

        let mut payload = vec![0u8; length];
        std::io::Read::read_exact(&mut stream, &mut payload).unwrap();

        let response: serde_json::Value = serde_json::from_slice(&payload).unwrap();
        assert_eq!(response["success"], true);
        assert_eq!(response["result"]["healthy"], true);
        assert_eq!(response["requestId"], 1);

        println!("IPC health-check response: {}", response);
    }

    #[test]
    #[ignore] // Requires running continuum-core server with Kokoro model
    fn test_ipc_voice_synthesize_binary_live() {
        use std::io::Write;

        let socket_path = "/tmp/continuum-core.sock";
        let mut stream = std::os::unix::net::UnixStream::connect(socket_path)
            .expect("Failed to connect to continuum-core socket");

        // Send voice/synthesize request
        let request = r#"{"command":"voice/synthesize","text":"Hello world","voice":"af","requestId":2}"#;
        stream.write_all(request.as_bytes()).unwrap();
        stream.write_all(b"\n").unwrap();
        stream.flush().unwrap();

        // Read length-prefixed response (may be binary)
        let mut len_buf = [0u8; 4];
        std::io::Read::read_exact(&mut stream, &mut len_buf).unwrap();
        let length = u32::from_be_bytes(len_buf) as usize;
        assert!(length > 0, "Response should not be empty");

        let mut payload = vec![0u8; length];
        std::io::Read::read_exact(&mut stream, &mut payload).unwrap();

        // Find \0 separator for binary frame
        let sep_idx = payload.iter().position(|&b| b == 0);

        if let Some(idx) = sep_idx {
            // Binary response: JSON header + \0 + raw PCM
            let json_bytes = &payload[..idx];
            let pcm_bytes = &payload[idx + 1..];

            let header: serde_json::Value = serde_json::from_slice(json_bytes).unwrap();
            assert_eq!(header["success"], true);
            assert_eq!(header["result"]["binary_pcm"], true);

            let sample_rate = header["result"]["sample_rate"].as_u64().unwrap();
            let num_samples = header["result"]["num_samples"].as_u64().unwrap();
            let duration_ms = header["result"]["duration_ms"].as_u64().unwrap();

            assert_eq!(sample_rate, 16000);
            assert!(num_samples > 100, "Should have >100 samples");
            assert!(duration_ms > 50, "Should be >50ms");
            assert_eq!(pcm_bytes.len(), num_samples as usize * 2, "PCM bytes should be 2 * num_samples");

            // Verify PCM data is valid i16 audio (not all zeros)
            let samples: Vec<i16> = pcm_bytes
                .chunks_exact(2)
                .map(|c| i16::from_le_bytes([c[0], c[1]]))
                .collect();
            let max_amp = samples.iter().map(|s| s.abs()).max().unwrap_or(0);
            assert!(max_amp > 100, "Audio should not be silence, max amplitude: {}", max_amp);

            println!(
                "IPC voice/synthesize: {} samples, {}Hz, {}ms, {} bytes PCM, max amp: {}",
                num_samples, sample_rate, duration_ms, pcm_bytes.len(), max_amp
            );
        } else {
            // JSON-only response (likely an error)
            let response: serde_json::Value = serde_json::from_slice(&payload).unwrap();
            panic!("Expected binary response, got JSON: {}", response);
        }
    }
}

// ============================================================================
// Server Main Loop
// ============================================================================

pub fn start_server(
    socket_path: &str,
    call_manager: Arc<crate::voice::call_server::CallManager>,
    rt_handle: tokio::runtime::Handle,
    memory_manager: Arc<crate::memory::PersonaMemoryManager>,
) -> std::io::Result<()> {
    // Remove socket file if it exists
    if Path::new(socket_path).exists() {
        std::fs::remove_file(socket_path)?;
    }

    log_info!("ipc", "server", "Starting IPC server on {}", socket_path);

    let listener = UnixListener::bind(socket_path)?;
    let state = Arc::new(ServerState::new(call_manager, rt_handle, memory_manager));

    log_info!("ipc", "server", "IPC server ready");

    // Accept connections (event-driven - sleeps until connection)
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let state = state.clone();

                // Spawn thread for concurrent handling
                std::thread::spawn(move || {
                    if let Err(e) = handle_client(stream, state) {
                        log_error!("ipc", "server", "Client error: {}", e);
                    }
                });
            }
            Err(e) => {
                log_error!("ipc", "server", "Connection error: {}", e);
            }
        }
    }

    Ok(())
}
