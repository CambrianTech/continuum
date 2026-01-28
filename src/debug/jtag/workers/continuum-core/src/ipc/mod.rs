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
use crate::persona::{PersonaInbox, PersonaCognitionEngine, InboxMessage, SenderType, Modality};
use crate::rag::RagEngine;
use crate::logging::TimingGuard;
use ts_rs::TS;
use crate::{log_debug, log_info, log_error};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

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
    inboxes: Arc<Mutex<HashMap<Uuid, PersonaInbox>>>,
    cognition_engines: Arc<Mutex<HashMap<Uuid, PersonaCognitionEngine>>>,
    rag_engine: Arc<RagEngine>,
}

impl ServerState {
    fn new() -> Self {
        Self {
            voice_service: Arc::new(crate::voice::voice_service::VoiceService::new()),
            inboxes: Arc::new(Mutex::new(HashMap::new())),
            cognition_engines: Arc::new(Mutex::new(HashMap::new())),
            rag_engine: Arc::new(RagEngine::new()),
        }
    }

    fn handle_request(&self, request: Request) -> Response {
        match request {
            Request::VoiceRegisterSession {
                session_id,
                room_id,
                participants,
            } => {
                let _timer = TimingGuard::new("ipc", "voice_register_session");

                match self.voice_service.register_session(&session_id, &room_id, participants) {
                    Ok(_) => Response::success(serde_json::json!({ "registered": true })),
                    Err(e) => Response::error(e),
                }
            }

            Request::VoiceOnUtterance { event } => {
                let _timer = TimingGuard::new("ipc", "voice_on_utterance").with_threshold(10);

                match self.voice_service.on_utterance(event) {
                    Ok(responder_ids) => Response::success(serde_json::json!({
                        VOICE_RESPONSE_FIELD_RESPONDER_IDS: responder_ids.into_iter().map(|id| id.to_string()).collect::<Vec<String>>()
                    })),
                    Err(e) => Response::error(e),
                }
            }

            Request::VoiceShouldRouteTts {
                session_id,
                persona_id,
            } => {
                let _timer = TimingGuard::new("ipc", "voice_should_route_tts");

                match self.voice_service.should_route_tts(&session_id, &persona_id) {
                    Ok(should_route) => Response::success(serde_json::json!({ "should_route": should_route })),
                    Err(e) => Response::error(e),
                }
            }

            Request::VoiceSynthesize { text, voice, adapter } => {
                let _timer = TimingGuard::new("ipc", "voice_synthesize");

                // Delegate to TTS service (synchronous wrapper handles runtime)
                use crate::voice::tts_service;
                use base64::Engine;

                let result = tts_service::synthesize_speech_sync(
                    &text,
                    voice.as_deref(),
                    adapter.as_deref()
                );

                match result {
                    Ok(synthesis) => {
                        // Convert to base64 for transport
                        let bytes: Vec<u8> = synthesis.samples.iter()
                            .flat_map(|s| s.to_le_bytes())
                            .collect();
                        let audio_base64 = base64::engine::general_purpose::STANDARD.encode(&bytes);

                        log_info!(
                            "ipc", "voice_synthesize",
                            "Synthesized {} samples at {}Hz ({:.1}s)",
                            synthesis.samples.len(),
                            synthesis.sample_rate,
                            synthesis.duration_ms as f64 / 1000.0
                        );

                        // CRITICAL: Return ACTUAL sample rate from TTS, not a default
                        // TTS adapters resample to 16kHz, so this should always be 16000
                        Response::success(serde_json::json!({
                            "audio": audio_base64,
                            "sample_rate": synthesis.sample_rate,  // Actual rate from TTS (16000)
                            "duration_ms": synthesis.duration_ms,
                            "adapter": adapter.unwrap_or_else(|| "default".to_string())
                        }))
                    },
                    Err(e) => {
                        log_error!("ipc", "voice_synthesize", "TTS failed: {}", e);
                        Response::error(format!("TTS failed: {}", e))
                    }
                }
            }

            Request::VoiceTranscribe { audio, language } => {
                let _timer = TimingGuard::new("ipc", "voice_transcribe");

                use crate::voice::stt_service;
                use base64::Engine;

                // Decode base64 audio
                let bytes = match base64::engine::general_purpose::STANDARD.decode(&audio) {
                    Ok(b) => b,
                    Err(e) => {
                        log_error!("ipc", "voice_transcribe", "Base64 decode failed: {}", e);
                        return Response::error(format!("Base64 decode failed: {}", e));
                    }
                };

                // Convert bytes to i16 samples
                if bytes.len() % 2 != 0 {
                    return Response::error("Audio data must have even length (i16 samples)".into());
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

                // Transcribe
                let result = stt_service::transcribe_speech_sync(
                    &samples,
                    language.as_deref()
                );

                match result {
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
                }
            }

            Request::InboxCreate { persona_id } => {
                let _timer = TimingGuard::new("ipc", "inbox_create");

                let persona_uuid = match Uuid::parse_str(&persona_id) {
                    Ok(u) => u,
                    Err(e) => return Response::error(format!("Invalid persona_id: {e}")),
                };

                let inbox = PersonaInbox::new(persona_uuid);
                let mut inboxes = match self.inboxes.lock() {
                    Ok(i) => i,
                    Err(e) => return Response::error(format!("Lock poisoned: {e}")),
                };
                inboxes.insert(persona_uuid, inbox);

                Response::success(serde_json::json!({ "created": true }))
            }

            // ================================================================
            // Cognition Handlers
            // ================================================================

            Request::CognitionCreateEngine { persona_id, persona_name } => {
                let _timer = TimingGuard::new("ipc", "cognition_create_engine");

                let persona_uuid = match Uuid::parse_str(&persona_id) {
                    Ok(u) => u,
                    Err(e) => return Response::error(format!("Invalid persona_id: {e}")),
                };

                let (_, shutdown_rx) = tokio::sync::watch::channel(false);
                let engine = PersonaCognitionEngine::new(
                    persona_uuid,
                    persona_name,
                    self.rag_engine.clone(),
                    shutdown_rx,
                );

                let mut engines = match self.cognition_engines.lock() {
                    Ok(e) => e,
                    Err(e) => return Response::error(format!("Lock poisoned: {e}")),
                };
                engines.insert(persona_uuid, engine);

                log_info!("ipc", "cognition", "Created cognition engine for {}", persona_id);
                Response::success(serde_json::json!({ "created": true }))
            }

            Request::CognitionCalculatePriority {
                persona_id, content, sender_type, is_voice, room_id, timestamp
            } => {
                let _timer = TimingGuard::new("ipc", "cognition_calculate_priority");

                let persona_uuid = match Uuid::parse_str(&persona_id) {
                    Ok(u) => u,
                    Err(e) => return Response::error(format!("Invalid persona_id: {e}")),
                };

                let room_uuid = match Uuid::parse_str(&room_id) {
                    Ok(u) => u,
                    Err(e) => return Response::error(format!("Invalid room_id: {e}")),
                };

                let sender = match sender_type.as_str() {
                    "human" => SenderType::Human,
                    "persona" => SenderType::Persona,
                    "agent" => SenderType::Agent,
                    "system" => SenderType::System,
                    _ => return Response::error(format!("Invalid sender_type: {}", sender_type)),
                };

                let engines = match self.cognition_engines.lock() {
                    Ok(e) => e,
                    Err(e) => return Response::error(format!("Lock poisoned: {e}")),
                };

                let engine = match engines.get(&persona_uuid) {
                    Some(e) => e,
                    None => return Response::error(format!("No cognition engine for {}", persona_id)),
                };

                let score = engine.calculate_priority(&content, sender, is_voice, room_uuid, timestamp);

                Response::success(serde_json::json!({
                    "score": score.score,
                    "factors": {
                        "recency_score": score.factors.recency_score,
                        "mention_score": score.factors.mention_score,
                        "room_score": score.factors.room_score,
                        "sender_score": score.factors.sender_score,
                        "voice_boost": score.factors.voice_boost,
                    }
                }))
            }

            Request::CognitionFastPathDecision { persona_id, message } => {
                let _timer = TimingGuard::new("ipc", "cognition_fast_path_decision");

                let persona_uuid = match Uuid::parse_str(&persona_id) {
                    Ok(u) => u,
                    Err(e) => return Response::error(format!("Invalid persona_id: {e}")),
                };

                let inbox_msg = match message.to_inbox_message() {
                    Ok(m) => m,
                    Err(e) => return Response::error(e),
                };

                let engines = match self.cognition_engines.lock() {
                    Ok(e) => e,
                    Err(e) => return Response::error(format!("Lock poisoned: {e}")),
                };

                let engine = match engines.get(&persona_uuid) {
                    Some(e) => e,
                    None => return Response::error(format!("No cognition engine for {}", persona_id)),
                };

                let decision = engine.fast_path_decision(&inbox_msg);

                Response::success(serde_json::json!({
                    "should_respond": decision.should_respond,
                    "confidence": decision.confidence,
                    "reason": decision.reason,
                    "decision_time_ms": decision.decision_time_ms,
                    "fast_path_used": decision.fast_path_used,
                }))
            }

            Request::CognitionEnqueueMessage { persona_id, message } => {
                let _timer = TimingGuard::new("ipc", "cognition_enqueue_message");

                let persona_uuid = match Uuid::parse_str(&persona_id) {
                    Ok(u) => u,
                    Err(e) => return Response::error(format!("Invalid persona_id: {e}")),
                };

                let inbox_msg = match message.to_inbox_message() {
                    Ok(m) => m,
                    Err(e) => return Response::error(e),
                };

                let engines = match self.cognition_engines.lock() {
                    Ok(e) => e,
                    Err(e) => return Response::error(format!("Lock poisoned: {e}")),
                };

                let engine = match engines.get(&persona_uuid) {
                    Some(e) => e,
                    None => return Response::error(format!("No cognition engine for {}", persona_id)),
                };

                engine.enqueue_message(inbox_msg);

                Response::success(serde_json::json!({ "enqueued": true }))
            }

            Request::CognitionGetState { persona_id } => {
                let _timer = TimingGuard::new("ipc", "cognition_get_state");

                let persona_uuid = match Uuid::parse_str(&persona_id) {
                    Ok(u) => u,
                    Err(e) => return Response::error(format!("Invalid persona_id: {e}")),
                };

                let engines = match self.cognition_engines.lock() {
                    Ok(e) => e,
                    Err(e) => return Response::error(format!("Lock poisoned: {e}")),
                };

                let engine = match engines.get(&persona_uuid) {
                    Some(e) => e,
                    None => return Response::error(format!("No cognition engine for {}", persona_id)),
                };

                let state = engine.state();

                Response::success(serde_json::json!({
                    "energy": state.energy,
                    "attention": state.attention,
                    "mood": format!("{:?}", state.mood).to_lowercase(),
                    "inbox_load": state.inbox_load,
                    "last_activity_time": state.last_activity_time,
                    "response_count": state.response_count,
                    "compute_budget": state.compute_budget,
                    "service_cadence_ms": state.service_cadence_ms(),
                }))
            }

            Request::HealthCheck => {
                Response::success(serde_json::json!({ "healthy": true }))
            }

            Request::GetStats { category: _ } => {
                Response::success(serde_json::json!({
                    "note": "Performance stats tracking not yet implemented"
                }))
            }
        }
    }
}

// ============================================================================
// Connection Handler
// ============================================================================

/// Helper to send JSON response, handling serialization errors gracefully
fn send_response(stream: &mut UnixStream, response: Response) -> std::io::Result<()> {
    let json = match serde_json::to_string(&response) {
        Ok(j) => j,
        Err(e) => {
            log_error!("ipc", "server", "Failed to serialize response: {}", e);
            // Fallback: send simple error JSON
            r#"{"success":false,"error":"Internal serialization error"}"#.to_string()
        }
    };
    writeln!(stream, "{json}")
}

fn handle_client(mut stream: UnixStream, state: Arc<ServerState>) -> std::io::Result<()> {
    let peer_addr = stream.peer_addr()?;
    log_debug!("ipc", "server", "Client connected: {:?}", peer_addr);

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
                send_response(&mut stream, response)?;
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
                send_response(&mut stream, response)?;
                continue;
            }
        };

        // Handle request and attach requestId to response
        let response = state.handle_request(request).with_request_id(request_id);

        // Send response
        send_response(&mut stream, response)?;
    }

    log_debug!("ipc", "server", "Client disconnected: {:?}", peer_addr);
    Ok(())
}

// ============================================================================
// Server Main Loop
// ============================================================================

pub fn start_server(socket_path: &str) -> std::io::Result<()> {
    // Remove socket file if it exists
    if Path::new(socket_path).exists() {
        std::fs::remove_file(socket_path)?;
    }

    log_info!("ipc", "server", "Starting IPC server on {}", socket_path);

    let listener = UnixListener::bind(socket_path)?;
    let state = Arc::new(ServerState::new());

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
