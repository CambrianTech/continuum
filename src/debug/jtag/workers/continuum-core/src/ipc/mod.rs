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
use crate::persona::PersonaInbox;
use crate::logging::TimingGuard;
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
}

impl ServerState {
    fn new() -> Self {
        Self {
            voice_service: Arc::new(crate::voice::voice_service::VoiceService::new()),
            inboxes: Arc::new(Mutex::new(HashMap::new())),
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
