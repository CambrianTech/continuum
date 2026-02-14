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
/// - Modular runtime routes commands through ServiceModule trait (Phase 1+)
use crate::persona::{PersonaInbox, PersonaCognitionEngine, ChannelRegistry, PersonaState};
use crate::rag::RagEngine;
use crate::code::{FileEngine, ShellSession};
use crate::runtime::{Runtime, CommandResult};
use crate::modules::health::HealthModule;
use crate::modules::cognition::{CognitionModule, CognitionState};
use crate::modules::channel::{ChannelModule, ChannelState};
use crate::modules::models::ModelsModule;
use crate::modules::memory::{MemoryModule, MemoryState};
use crate::modules::voice::{VoiceModule, VoiceState};
use crate::modules::code::{CodeModule, CodeState};
use crate::modules::rag::{RagModule, RagState};
use crate::modules::data::DataModule;
use crate::modules::logger::LoggerModule;
use crate::modules::search::SearchModule;
use crate::modules::embedding::EmbeddingModule;
use crate::modules::agent::AgentModule;
use crate::modules::ai_provider::AIProviderModule;
use crate::modules::sentinel::SentinelModule;
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

// NOTE: InboxMessageRequest is used for ts-rs TypeScript generation.
// The to_inbox_message() method was removed when migrating to CognitionModule.
// See modules/cognition.rs for the parsing logic.

// All commands route through ServiceModule implementations in src/modules/.
// See modules/health.rs, cognition.rs, channel.rs, voice.rs, code.rs, memory.rs,
// models.rs, data.rs, logger.rs, search.rs, embedding.rs, rag.rs for command handlers.

#[derive(Debug, Serialize, Deserialize)]
struct Response {
    success: bool,
    result: Option<serde_json::Value>,
    error: Option<String>,
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

/// ServerState holds Arc references that are passed to ServiceModules during initialization.
/// After modules are registered with the runtime, these fields are not accessed directly
/// by ServerState methods — all command handling goes through runtime.dispatch().
/// The fields are kept here to ensure the Arc lifetimes outlive the modules.
#[allow(dead_code)]
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
    /// Per-persona file engines — workspace-scoped file operations with change tracking.
    file_engines: Arc<DashMap<String, FileEngine>>,
    /// Per-persona shell sessions — persistent bash per workspace with handle+poll.
    shell_sessions: Arc<DashMap<String, ShellSession>>,
    /// Modular runtime — ServiceModule-based command routing.
    /// Phase 1+: routes health-check, get-stats through HealthModule.
    /// Phase 2+: routes cognition/, channel/ through CognitionModule, ChannelModule.
    /// Eventually replaces the entire match statement below.
    runtime: Arc<Runtime>,
}

impl ServerState {
    /// Create with shared state (for module state sharing).
    /// Phase 3+: Modules and ServerState share all per-persona and service state.
    #[allow(clippy::too_many_arguments)]
    fn new_with_shared_state(
        call_manager: Arc<crate::voice::call_server::CallManager>,
        rt_handle: tokio::runtime::Handle,
        memory_manager: Arc<crate::memory::PersonaMemoryManager>,
        runtime: Arc<Runtime>,
        cognition_engines: Arc<DashMap<Uuid, PersonaCognitionEngine>>,
        inboxes: Arc<DashMap<Uuid, PersonaInbox>>,
        channel_registries: Arc<DashMap<Uuid, (ChannelRegistry, PersonaState)>>,
        rag_engine: Arc<RagEngine>,
        voice_service: Arc<crate::voice::voice_service::VoiceService>,
        audio_pool: Arc<crate::voice::audio_buffer::AudioBufferPool>,
        file_engines: Arc<DashMap<String, FileEngine>>,
        shell_sessions: Arc<DashMap<String, ShellSession>>,
    ) -> Self {
        Self {
            voice_service,
            inboxes,
            cognition_engines,
            channel_registries,
            rag_engine,
            call_manager,
            audio_pool,
            rt_handle,
            memory_manager,
            file_engines,
            shell_sessions,
            runtime,
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

/// Handle a single IPC client connection with concurrent request processing.
///
/// Architecture:
/// - Reader thread (this function): reads newline-delimited JSON requests from the socket
/// - Writer thread: serializes responses back to the socket in arrival order
/// - Rayon pool: processes each request concurrently on worker threads
///
/// The TS client multiplexes via requestId — responses can arrive in any order.
/// This eliminates the sequential bottleneck where 6 concurrent requests from
/// RAGComposer (global-awareness, semantic-memory, etc.) were serialized per-connection.
fn handle_client(stream: UnixStream, state: Arc<ServerState>) -> std::io::Result<()> {
    let peer_addr = stream.peer_addr()?;
    log_debug!("ipc", "server", "Client connected: {:?}", peer_addr);

    let reader = BufReader::new(stream.try_clone()?);

    // Response channel — rayon tasks send completed results, writer thread serializes to socket.
    // Unbounded: request rate is limited by socket read speed, not processing speed.
    let (tx, rx) = std::sync::mpsc::channel::<(Option<u64>, HandleResult)>();

    // Writer thread — owns the write half of the socket, serializes response frames.
    // Multiple rayon tasks complete concurrently; this thread ensures atomic frame writes.
    let mut writer_stream = stream.try_clone()?;
    let writer_handle = std::thread::spawn(move || {
        for (request_id, result) in rx {
            let write_result = match result {
                HandleResult::Json(response) => {
                    let response = response.with_request_id(request_id);
                    send_json_frame(&mut writer_stream, &response)
                }
                HandleResult::Binary { json_header, binary_data } => {
                    let json_header = json_header.with_request_id(request_id);
                    send_binary_frame(&mut writer_stream, &json_header, &binary_data)
                }
            };
            if let Err(e) = write_result {
                log_error!("ipc", "server", "Write error: {}", e);
                break;
            }
        }
    });

    // Reader loop — parse requests and dispatch to rayon for concurrent processing.
    // No longer blocks waiting for handle_request() to complete before reading next request.
    for line in reader.lines() {
        let line = line?;
        if line.is_empty() {
            continue;
        }

        // Parse JSON to extract requestId and command
        let json_value: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                let _ = tx.send((None, HandleResult::Json(Response::error(format!("Invalid JSON: {e}")))));
                continue;
            }
        };

        let request_id = json_value.get("requestId").and_then(|v| v.as_u64());
        let command = json_value.get("command").and_then(|v| v.as_str()).map(|s| s.to_string());

        // Dispatch to rayon thread pool — each request runs concurrently.
        // handle_request(&self) is safe for concurrent calls (DashMap per-key locking).
        let state = state.clone();
        let tx = tx.clone();
        rayon::spawn(move || {
            // Route through modular runtime (all commands handled by ServiceModules)
            let handle_result = if let Some(ref cmd) = command {
                match state.runtime.route_command_sync(cmd, json_value.clone(), &state.rt_handle) {
                    Some(Ok(CommandResult::Json(value))) => HandleResult::Json(Response::success(value)),
                    Some(Ok(CommandResult::Binary { metadata, data })) => HandleResult::Binary {
                        json_header: Response::success(metadata),
                        binary_data: data,
                    },
                    Some(Err(e)) => HandleResult::Json(Response::error(e)),
                    None => HandleResult::Json(Response::error(format!(
                        "Unknown command: '{}'. No module registered for this command prefix.",
                        cmd
                    ))),
                }
            } else {
                HandleResult::Json(Response::error("Missing 'command' field in request".to_string()))
            };
            let _ = tx.send((request_id, handle_result));
        });
    }

    // Drop sender to signal writer thread to exit, then wait for it
    drop(tx);
    let _ = writer_handle.join();

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
    // Response Serialization Tests
    // ========================================================================
    // NOTE: Request deserialization tests removed - legacy Request enum deleted.
    // Commands now route through ServiceModule implementations (modules/*.rs).
    // Each module has its own tests for command handling.

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

    // Create modular runtime
    log_info!("ipc", "server", "Initializing modular runtime...");
    let runtime = Arc::new(Runtime::new());

    // Phase 1: HealthModule (stateless)
    runtime.register(Arc::new(HealthModule::new()));

    // Create shared DashMaps for per-persona state (shared between ServerState and modules)
    let cognition_engines: Arc<DashMap<Uuid, PersonaCognitionEngine>> = Arc::new(DashMap::new());
    let inboxes: Arc<DashMap<Uuid, PersonaInbox>> = Arc::new(DashMap::new());
    let channel_registries: Arc<DashMap<Uuid, (ChannelRegistry, PersonaState)>> = Arc::new(DashMap::new());
    let rag_engine = Arc::new(RagEngine::new());

    // Phase 2: CognitionModule + ChannelModule (per-persona DashMap state)
    let cognition_state = Arc::new(CognitionState::from_existing(
        cognition_engines.clone(),
        inboxes.clone(),
        rag_engine.clone(),
    ));
    runtime.register(Arc::new(CognitionModule::new(cognition_state)));

    let channel_state = Arc::new(ChannelState::from_existing(
        channel_registries.clone(),
        cognition_engines.clone(),
    ));
    runtime.register(Arc::new(ChannelModule::new(channel_state)));

    // Phase 3: ModelsModule (stateless, async HTTP discovery)
    runtime.register(Arc::new(ModelsModule::new()));

    // Phase 3: MemoryModule (wraps PersonaMemoryManager)
    let memory_state = Arc::new(MemoryState::new(memory_manager.clone()));
    runtime.register(Arc::new(MemoryModule::new(memory_state)));

    // Phase 3: RagModule (batched RAG composition with parallel Rayon loading)
    let rag_state = Arc::new(RagState::new(memory_manager.clone()));
    runtime.register(Arc::new(RagModule::new(rag_state)));

    // Phase 3: VoiceModule (wraps VoiceService, CallManager, AudioBufferPool)
    let voice_service = Arc::new(crate::voice::voice_service::VoiceService::new());
    let audio_pool = Arc::new(crate::voice::audio_buffer::AudioBufferPool::new());
    let voice_state = Arc::new(VoiceState::new(
        voice_service.clone(),
        call_manager.clone(),
        audio_pool.clone(),
    ));
    runtime.register(Arc::new(VoiceModule::new(voice_state)));

    // Phase 3: CodeModule (wraps file engines and shell sessions per-persona)
    let file_engines: Arc<DashMap<String, FileEngine>> = Arc::new(DashMap::new());
    let shell_sessions: Arc<DashMap<String, ShellSession>> = Arc::new(DashMap::new());
    let code_state = Arc::new(CodeState::new(
        file_engines.clone(),
        shell_sessions.clone(),
        rt_handle.clone(),
    ));
    runtime.register(Arc::new(CodeModule::new(code_state)));

    // Phase 4: DataModule (database-agnostic storage via ORM adapters)
    // DB path is passed per-request from TypeScript - NO defaults
    runtime.register(Arc::new(DataModule::new()));

    // Phase 4a: LoggerModule (absorbs standalone logger worker)
    // Provides log/write, log/ping via main socket
    runtime.register(Arc::new(LoggerModule::new()));

    // Phase 4b: SearchModule (absorbs standalone search worker)
    // Provides search/execute, search/vector, search/list, search/params
    runtime.register(Arc::new(SearchModule::new()));

    // Phase 4c: EmbeddingModule (absorbs standalone embedding worker)
    // Provides embedding/generate, embedding/model/{load,list,info,unload}
    runtime.register(Arc::new(EmbeddingModule::new()));

    // RuntimeModule: Exposes metrics and control for AI-driven system management (Ares)
    // Provides runtime/metrics/{all,module,slow}, runtime/list
    runtime.register(Arc::new(crate::modules::runtime_control::RuntimeModule::new()));

    // MCPModule: Dynamic tool discovery for MCP servers
    // Provides mcp/list-tools, mcp/search-tools, mcp/tool-help
    runtime.register(Arc::new(crate::modules::mcp::MCPModule::new()));

    // AgentModule: Autonomous AI coding agents with structured tool calling
    // Provides agent/start, agent/status, agent/stop, agent/list, agent/wait
    runtime.register(Arc::new(AgentModule::new(rt_handle.clone())));

    // AIProviderModule: Unified AI provider for cloud and local inference
    // Provides ai/generate, ai/providers/list, ai/providers/health
    // Routes to DeepSeek, Anthropic, OpenAI, Together, Groq, Fireworks, XAI, Google
    runtime.register(Arc::new(AIProviderModule::new()));

    // SentinelModule: Concurrent, fault-tolerant build/task execution
    // Provides sentinel/execute, sentinel/status, sentinel/cancel, sentinel/list
    // And sentinel/logs/list, sentinel/logs/read, sentinel/logs/tail
    // Process isolation via child processes - safe for Xcode, cargo, etc.
    runtime.register(Arc::new(SentinelModule::new()));

    // Initialize modules (runs async init in sync context)
    rt_handle.block_on(async {
        if let Err(e) = runtime.initialize().await {
            log_error!("ipc", "server", "Runtime initialization failed: {}", e);
        }
    });

    // Verify all expected modules are registered (fails server if any missing)
    if let Err(e) = runtime.verify_registration() {
        log_error!("ipc", "server", "{}", e);
        return Err(std::io::Error::new(std::io::ErrorKind::Other, e));
    }

    log_info!("ipc", "server", "Modular runtime ready with {} modules: {:?}",
        runtime.registry().list_modules().len(),
        runtime.registry().list_modules());

    // Initialize global CommandExecutor for all spawned processes (sentinels, agents, etc.)
    // This allows ANY async task to execute ANY command (Rust or TypeScript)
    // TypeScript commands route via Unix socket to /tmp/jtag-command-router.sock
    crate::runtime::init_executor(runtime.registry_arc());

    let listener = UnixListener::bind(socket_path)?;
    let state = Arc::new(ServerState::new_with_shared_state(
        call_manager,
        rt_handle,
        memory_manager,
        runtime,
        cognition_engines,
        inboxes,
        channel_registries,
        rag_engine,
        voice_service,
        audio_pool,
        file_engines,
        shell_sessions,
    ));

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
