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
use crate::voice::{VoiceOrchestrator, UtteranceEvent, VoiceParticipant};
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
    voice_orchestrator: Arc<Mutex<VoiceOrchestrator>>,
    inboxes: Arc<Mutex<HashMap<Uuid, PersonaInbox>>>,
}

impl ServerState {
    fn new() -> Self {
        Self {
            voice_orchestrator: Arc::new(Mutex::new(VoiceOrchestrator::new())),
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

                let session_uuid = match Uuid::parse_str(&session_id) {
                    Ok(u) => u,
                    Err(e) => return Response::error(format!("Invalid session_id: {e}")),
                };

                let room_uuid = match Uuid::parse_str(&room_id) {
                    Ok(u) => u,
                    Err(e) => return Response::error(format!("Invalid room_id: {e}")),
                };

                let orchestrator = match self.voice_orchestrator.lock() {
                    Ok(o) => o,
                    Err(e) => return Response::error(format!("Lock poisoned: {e}")),
                };
                orchestrator.register_session(session_uuid, room_uuid, participants);

                Response::success(serde_json::json!({ "registered": true }))
            }

            Request::VoiceOnUtterance { event } => {
                let _timer = TimingGuard::new("ipc", "voice_on_utterance").with_threshold(10);

                let orchestrator = match self.voice_orchestrator.lock() {
                    Ok(o) => o,
                    Err(e) => return Response::error(format!("Lock poisoned: {e}")),
                };
                let responder_id = orchestrator.on_utterance(event);

                Response::success(serde_json::json!({
                    "responder_id": responder_id.map(|id| id.to_string())
                }))
            }

            Request::VoiceShouldRouteTts {
                session_id,
                persona_id,
            } => {
                let _timer = TimingGuard::new("ipc", "voice_should_route_tts");

                let session_uuid = match Uuid::parse_str(&session_id) {
                    Ok(u) => u,
                    Err(e) => return Response::error(format!("Invalid session_id: {e}")),
                };

                let persona_uuid = match Uuid::parse_str(&persona_id) {
                    Ok(u) => u,
                    Err(e) => return Response::error(format!("Invalid persona_id: {e}")),
                };

                let orchestrator = match self.voice_orchestrator.lock() {
                    Ok(o) => o,
                    Err(e) => return Response::error(format!("Lock poisoned: {e}")),
                };
                let should_route = orchestrator.should_route_to_tts(session_uuid, persona_uuid);

                Response::success(serde_json::json!({ "should_route": should_route }))
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
