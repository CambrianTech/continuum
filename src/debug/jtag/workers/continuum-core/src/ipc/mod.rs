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
    /// Shared CallManager for direct audio injection (speak-in-call).
    /// Audio never leaves Rust — IPC only returns metadata.
    call_manager: Arc<crate::voice::call_server::CallManager>,
    /// Tokio runtime handle for calling async CallManager methods from IPC threads.
    rt_handle: tokio::runtime::Handle,
}

impl ServerState {
    fn new(call_manager: Arc<crate::voice::call_server::CallManager>, rt_handle: tokio::runtime::Handle) -> Self {
        Self {
            voice_service: Arc::new(crate::voice::voice_service::VoiceService::new()),
            inboxes: Arc::new(Mutex::new(HashMap::new())),
            cognition_engines: Arc::new(Mutex::new(HashMap::new())),
            rag_engine: Arc::new(RagEngine::new()),
            call_manager,
            rt_handle,
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
                let mut inboxes = match self.inboxes.lock() {
                    Ok(i) => i,
                    Err(e) => return HandleResult::Json(Response::error(format!("Lock poisoned: {e}"))),
                };
                inboxes.insert(persona_uuid, inbox);

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

                let mut engines = match self.cognition_engines.lock() {
                    Ok(e) => e,
                    Err(e) => return HandleResult::Json(Response::error(format!("Lock poisoned: {e}"))),
                };
                engines.insert(persona_uuid, engine);

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

                let engines = match self.cognition_engines.lock() {
                    Ok(e) => e,
                    Err(e) => return HandleResult::Json(Response::error(format!("Lock poisoned: {e}"))),
                };

                let engine = match engines.get(&persona_uuid) {
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

                let engines = match self.cognition_engines.lock() {
                    Ok(e) => e,
                    Err(e) => return HandleResult::Json(Response::error(format!("Lock poisoned: {e}"))),
                };

                let engine = match engines.get(&persona_uuid) {
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

                let engines = match self.cognition_engines.lock() {
                    Ok(e) => e,
                    Err(e) => return HandleResult::Json(Response::error(format!("Lock poisoned: {e}"))),
                };

                let engine = match engines.get(&persona_uuid) {
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

                let engines = match self.cognition_engines.lock() {
                    Ok(e) => e,
                    Err(e) => return HandleResult::Json(Response::error(format!("Lock poisoned: {e}"))),
                };

                let engine = match engines.get(&persona_uuid) {
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
) -> std::io::Result<()> {
    // Remove socket file if it exists
    if Path::new(socket_path).exists() {
        std::fs::remove_file(socket_path)?;
    }

    log_info!("ipc", "server", "Starting IPC server on {}", socket_path);

    let listener = UnixListener::bind(socket_path)?;
    let state = Arc::new(ServerState::new(call_manager, rt_handle));

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
