use crate::code::{FileEngine, ShellSession};
use crate::gpu::GpuMemoryManager;
use crate::modules::agent::AgentModule;
use crate::modules::ai_provider::AIProviderModule;
use crate::modules::airc::AircModule;
use crate::modules::airc_bridge_directive::AircBridgeDirectiveModule;
use crate::modules::airc_bridge_dispatch::AircBridgeDispatchModule;
use crate::modules::auth::ExternalWebviewAuthModule;
use crate::modules::avatar::AvatarModule;
use crate::modules::channel::{ChannelModule, ChannelState};
use crate::modules::code::{CodeModule, CodeState};
use crate::modules::cognition::{CognitionModule, CognitionState};
use crate::modules::data::DataModule;
use crate::modules::dataset::DatasetModule;
use crate::modules::embedding::EmbeddingModule;
use crate::modules::events::EventsModule;
use crate::modules::forge::ForgeModule;
use crate::modules::gpu::GpuModule;
use crate::modules::grid::GridModule;
use crate::modules::health::HealthModule;
use crate::modules::launch_mode::LaunchModeModule;
use crate::modules::live::{VoiceModule, VoiceState};
use crate::modules::logger::LoggerModule;
use crate::modules::memory::{MemoryModule, MemoryState};
use crate::modules::models::ModelsModule;
use crate::modules::persona_allocator::PersonaAllocatorModule;
use crate::modules::rag::{RagModule, RagState};
use crate::modules::sentinel::SentinelModule;
use crate::modules::system_resources::SystemResourceModule;
use crate::modules::tool_parsing::ToolParsingModule;
use crate::modules::vision::VisionModule;
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
use crate::persona::{ChannelRegistry, PersonaState};
use crate::rag::RagEngine;
use crate::runtime::{CommandResult, Runtime};
use crate::system_resources::SystemResourceMonitor;
use crate::{log_debug, log_error, log_info};
use dashmap::DashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::Arc;
use uuid::Uuid;

fn prepare_unix_socket_path(socket_path: &str) -> std::io::Result<()> {
    let path = Path::new(socket_path);

    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }

    if path.exists() {
        std::fs::remove_file(path)?;
    }

    Ok(())
}

/// Stream abstraction that lets handle_client serve both Unix socket clients
/// (native callers — continuum-core-server's primary IPC path) and TCP clients
/// (container callers — node-server running inside Docker on Mac, where Unix
/// sockets don't traverse the Docker VM boundary). Same request/response
/// protocol over both transports.
trait IpcStream: Read + Write + Send + Sized + 'static {
    fn try_clone_stream(&self) -> std::io::Result<Self>;
    fn peer_addr_str(&self) -> String;
}

impl IpcStream for UnixStream {
    fn try_clone_stream(&self) -> std::io::Result<Self> {
        self.try_clone()
    }
    fn peer_addr_str(&self) -> String {
        format!("{:?}", self.peer_addr().ok())
    }
}

impl IpcStream for TcpStream {
    fn try_clone_stream(&self) -> std::io::Result<Self> {
        self.try_clone()
    }
    fn peer_addr_str(&self) -> String {
        self.peer_addr()
            .map(|a| a.to_string())
            .unwrap_or_else(|_| "unknown".to_string())
    }
}

// ============================================================================
// Request/Response Protocol + Memory Diagnostics
// ============================================================================
// Split out of this file 2026-05-18 — see ipc/protocol.rs (InboxMessageRequest,
// Response) and ipc/diagnostics.rs (per-command RSS tracking). Re-exported
// here so existing call sites resolve unchanged.

pub mod diagnostics;
pub mod experience_resolver;
pub mod positron_dispatch;
pub mod positron_foundry_source;
pub mod positron_kanban_source;
pub mod positron_nav_source;
pub mod positron_presence;
pub mod positron_source;
pub mod positron_wall_source;
pub mod protocol;
pub mod provider_bridge;
pub mod room_purpose;
pub mod stream_rail;
pub mod vitals_emitter;
pub mod ws;

use diagnostics::{current_rss_mb, dump_memory_report, log_command_rss_delta};
pub use protocol::InboxMessageRequest;
use protocol::Response;

// See modules/health.rs, cognition.rs, channel.rs, voice.rs, code.rs, memory.rs,
// models.rs, data.rs, logger.rs, search.rs, embedding.rs, rag.rs for command handlers.

// ============================================================================
// IPC Server State
// ============================================================================

/// ServerState holds Arc references that are passed to ServiceModules during initialization.
/// After modules are registered with the runtime, these fields are not accessed directly
/// by ServerState methods — all command handling goes through runtime.dispatch().
/// The fields are kept here to ensure the Arc lifetimes outlive the modules.
#[allow(dead_code)]
struct ServerState {
    voice_service: Arc<crate::live::session::voice_service::VoiceService>,
    /// Per-persona channel registries + state — DashMap: hot-path ops are &mut self.
    channel_registries: Arc<DashMap<Uuid, (ChannelRegistry, PersonaState)>>,
    rag_engine: Arc<RagEngine>,
    /// Server-side audio buffer pool for handle-based synthesis.
    audio_pool: Arc<crate::live::audio::buffer::AudioBufferPool>,
    /// Tokio runtime handle for async operations from IPC threads.
    rt_handle: tokio::runtime::Handle,
    /// Per-persona memory manager — pure compute on in-memory MemoryCorpus.
    memory_manager: Arc<crate::memory::PersonaMemoryManager>,
    /// Per-persona file engines — workspace-scoped file operations with change tracking.
    file_engines: Arc<DashMap<String, FileEngine>>,
    /// Per-persona shell sessions — persistent bash per workspace with handle+poll.
    shell_sessions: Arc<DashMap<String, ShellSession>>,
    /// Modular runtime — ServiceModule-based command routing.
    runtime: Arc<Runtime>,
    /// GPU memory manager — unified VRAM coordination.
    gpu_manager: Arc<GpuMemoryManager>,
    /// Connected `Provided`-command providers (eye-nodes), keyed by command name.
    /// The SAME `Arc` the `ProvidedCommandInterceptor` reads: a `provider/register`
    /// on any connection binds here, and the interceptor routes perception/observe
    /// + interface/screenshot to whoever is bound. Empty ⇒ those commands fail loud.
    provider_registry: Arc<crate::runtime::ProviderRegistry>,
}

impl ServerState {
    #[allow(clippy::too_many_arguments)]
    fn new_with_shared_state(
        rt_handle: tokio::runtime::Handle,
        memory_manager: Arc<crate::memory::PersonaMemoryManager>,
        runtime: Arc<Runtime>,
        channel_registries: Arc<DashMap<Uuid, (ChannelRegistry, PersonaState)>>,
        rag_engine: Arc<RagEngine>,
        voice_service: Arc<crate::live::session::voice_service::VoiceService>,
        audio_pool: Arc<crate::live::audio::buffer::AudioBufferPool>,
        file_engines: Arc<DashMap<String, FileEngine>>,
        shell_sessions: Arc<DashMap<String, ShellSession>>,
        gpu_manager: Arc<GpuMemoryManager>,
        provider_registry: Arc<crate::runtime::ProviderRegistry>,
    ) -> Self {
        Self {
            voice_service,
            channel_registries,
            rag_engine,
            audio_pool,
            rt_handle,
            memory_manager,
            file_engines,
            shell_sessions,
            runtime,
            gpu_manager,
            provider_registry,
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

/// One item the per-connection writer thread serializes to the socket. The
/// writer is the SOLE owner of the write half, so every outbound frame — a
/// response to a client request AND a core-initiated call to the client — flows
/// through this one channel, keeping frames atomic.
///
/// `ProvideCall` is the back-channel [`provider_bridge`] rides: an eye-node
/// fulfilling `perception/observe`/`interface/screenshot`. It carries a
/// core-allocated `call_id` the client echoes back in its `provideResult`.
enum Outbound {
    /// A response to a client request (the entire pre-existing path).
    Response {
        request_id: Option<u64>,
        result: HandleResult,
    },
    /// A core→client request: forward a `Provided` command to the connected
    /// client that registered as its provider. Framed distinctly (`type:
    /// "provideCall"`) so the client dispatches it against its
    /// `Commands.provide` registrations rather than treating it as a response.
    ProvideCall {
        call_id: u64,
        command: String,
        params: serde_json::Value,
    },
}

// ============================================================================
// Connection Handler - Length-Prefixed Binary Framing
// ============================================================================

/// Send a length-prefixed JSON response frame.
/// Frame format: [4 bytes u32 BE length][JSON payload bytes]
fn send_json_frame<S: Write>(stream: &mut S, response: &Response) -> std::io::Result<()> {
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
fn send_binary_frame<S: Write>(
    stream: &mut S,
    response: &Response,
    binary_data: &[u8],
) -> std::io::Result<()> {
    let json = match serde_json::to_string(response) {
        Ok(j) => j,
        Err(e) => {
            log_error!(
                "ipc",
                "server",
                "Failed to serialize binary response header: {}",
                e
            );
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

/// Send a length-prefixed core→client `provideCall` frame — the substrate asking
/// a connected eye-node to fulfil a `Provided` command. Same `[u32 BE len][json]`
/// framing as a response so the client's existing frame reader delivers it; the
/// client discriminates on `type: "provideCall"` and replies with a
/// newline-delimited `{type:"provideResult", callId, ...}`.
fn send_provide_call_frame<S: Write>(
    stream: &mut S,
    call_id: u64,
    command: &str,
    params: &serde_json::Value,
) -> std::io::Result<()> {
    let frame = serde_json::json!({
        "type": "provideCall",
        "callId": call_id,
        "command": command,
        "params": params,
    });
    let json = serde_json::to_string(&frame).unwrap_or_else(|e| {
        log_error!("ipc", "server", "Failed to serialize provideCall: {}", e);
        format!(r#"{{"type":"provideCall","callId":{call_id},"error":"serialize failed"}}"#)
    });
    let payload = json.as_bytes();
    stream.write_all(&(payload.len() as u32).to_be_bytes())?;
    stream.write_all(payload)?;
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
/// `caller` is the connection's identity: `None` for the LOCAL Unix socket
/// (owner-by-locality — the operator on the box), `Some(CallerIdentity::tcp(..))`
/// for the TCP listener (an unauthenticated remote socket). TCP-sourced commands
/// are ACL-gated at the remote (non-owner) ceiling at the dispatch boundary, so a
/// TCP peer can never run Owner-gated commands (`data/delete`, `grid/trust`, …) —
/// closing the "TCP == local owner" hole (security review 2026-06-21).
fn handle_client<S: IpcStream>(
    stream: S,
    state: Arc<ServerState>,
    caller: Option<crate::routing::CallerIdentity>,
) -> std::io::Result<()> {
    let peer_addr = stream.peer_addr_str();
    log_debug!("ipc", "server", "Client connected: {}", peer_addr);

    let reader = BufReader::new(stream.try_clone_stream()?);

    // Outbound channel — tokio tasks send completed results, the reader thread
    // sends core→client provideCall frames; the writer thread serializes both to
    // the socket. Unbounded: request rate is limited by socket read speed, not
    // processing speed.
    let (tx, rx) = std::sync::mpsc::channel::<Outbound>();

    // Writer thread — owns the write half of the socket, serializes every frame.
    // Multiple tokio tasks complete concurrently; this thread ensures atomic frame
    // writes AND is the sole path a core-initiated provideCall reaches the client,
    // so a response and a provideCall never interleave mid-frame.
    let mut writer_stream = stream.try_clone_stream()?;
    let writer_handle = std::thread::spawn(move || {
        for outbound in rx {
            let write_result = match outbound {
                Outbound::Response { request_id, result } => match result {
                    HandleResult::Json(response) => {
                        let response = response.with_request_id(request_id);
                        send_json_frame(&mut writer_stream, &response)
                    }
                    HandleResult::Binary {
                        json_header,
                        binary_data,
                    } => {
                        let json_header = json_header.with_request_id(request_id);
                        send_binary_frame(&mut writer_stream, &json_header, &binary_data)
                    }
                },
                Outbound::ProvideCall {
                    call_id,
                    command,
                    params,
                } => send_provide_call_frame(&mut writer_stream, call_id, &command, &params),
            };
            if let Err(e) = write_result {
                log_error!("ipc", "server", "Write error: {}", e);
                break;
            }
        }
    });

    // Per-connection back-channel state for `Provided` commands (provider_bridge):
    // `pending` correlates a core→client provideCall to the client's provideResult
    // by core-allocated `call_id`; `next_call_id` allocates those ids;
    // `my_registrations` tracks the providers this connection registered so they
    // can be unregistered (pointer-matched) at disconnect.
    let pending: provider_bridge::PendingCalls = Arc::new(DashMap::new());
    let next_call_id = Arc::new(std::sync::atomic::AtomicU64::new(1));
    let mut my_registrations: Vec<provider_bridge::ConnRegistration> = Vec::new();

    // #85: track in-flight command tasks for THIS connection so we can abort them if the
    // client disconnects mid-flight — a blocking handler with no one left to answer would
    // otherwise run to completion writing into a dead socket (a zombie). Detached #86 jobs
    // spawn on their OWN task inside the handler, so the command task tracked here has already
    // returned for them — fire-and-poll is never aborted, only abandoned blocking requests are.
    let mut inflight: Vec<tokio::task::JoinHandle<()>> = Vec::new();

    // Reader loop — parse requests and dispatch to tokio for concurrent processing.
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
                let _ = tx.send(Outbound::Response {
                    request_id: None,
                    result: HandleResult::Json(Response::error(format!("Invalid JSON: {e}"))),
                });
                continue;
            }
        };

        // Back-channel reply: a connected eye-node answering a core-initiated
        // provideCall. It is NOT a command — complete the pending correlation and
        // move on. Discriminated by `type: "provideResult"` (only the provider
        // back-channel uses `type`; a normal request has `command`).
        if json_value.get("type").and_then(|v| v.as_str()) == Some("provideResult") {
            provider_bridge::complete_provide_result(&pending, &json_value);
            continue;
        }

        let request_id = json_value.get("requestId").and_then(|v| v.as_u64());
        let command = json_value
            .get("command")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // Provider-registration handshake: a client declaring which `Provided`
        // commands it fulfils (an eye-node offering perception/observe +
        // interface/screenshot). Handled inline — NOT routed through a
        // ServiceModule — because building the back-channel provider needs this
        // connection's writer `tx` + correlation state, which no module can see.
        // (This is transport infrastructure, like requestId framing, not an
        // application command; hence the deliberate name-check here.)
        if command.as_deref() == Some("provider/register") {
            let reply = provider_bridge::register_provider(
                &state.provider_registry,
                &tx,
                &pending,
                &next_call_id,
                &json_value,
                &mut my_registrations,
            );
            let _ = tx.send(Outbound::Response {
                request_id,
                result: HandleResult::Json(reply),
            });
            continue;
        }

        // Dispatch to tokio directly — NO RAYON THREAD BLOCKED.
        //
        // Previous: rayon::spawn → route_command_sync (blocks rayon thread for up to 60s)
        // Now: tokio::spawn → route_command (async, zero thread blocking)
        //
        // rayon::spawn was the root cause of system-wide starvation:
        // - Every IPC request occupied a rayon thread for its entire duration (up to 60s)
        // - With 14 agents sending concurrent ai/generate + data/count + voice/speak-in-call,
        //   all rayon threads were blocked waiting, and new commands couldn't start
        // - This caused voice/speak-in-call timeouts → intermittent mouth animation
        // - Also caused ai/generate and data/count timeouts → general system degradation
        //
        // tokio handles thousands of concurrent tasks without blocking any OS threads.
        let state = state.clone();
        let tx = tx.clone();
        let caller = caller.clone();
        let rt_handle = state.rt_handle.clone();
        // Reap already-finished handles so this Vec stays bounded to what's actually in flight.
        inflight.retain(|h| !h.is_finished());
        let inflight_handle = rt_handle.spawn(async move {
            let handle_result = if let Some(ref cmd) = command {
                // Boundary gate for a REMOTE (TCP) caller: the route_command path is
                // owner-by-locality (ungated) for the local Unix socket, so a remote
                // caller MUST be ACL-gated here at its trust ceiling. Owner-gated
                // commands are refused; no unauthenticated Owner execution over TCP.
                // Local (caller == None) skips this — the operator on the box is owner.
                if let Some(ref c) = caller {
                    let trust = crate::routing::caller_trust(Some(c));
                    if !crate::modules::grid::acl::is_command_authorized(cmd, trust) {
                        let _ = tx.send(Outbound::Response {
                            request_id,
                            result: HandleResult::Json(Response::error(format!(
                                "forbidden: command '{cmd}' is not permitted for a remote \
                                 (TCP) caller — Owner-gated commands are local-only"
                            ))),
                        });
                        return;
                    }
                }
                let rss_before = current_rss_mb();
                // Thread the caller so the typed object path sees the REMOTE identity
                // (composition then propagates remote-not-owner — no escalation).
                let result = state
                    .runtime
                    .route_command(cmd, json_value.clone(), caller.clone())
                    .await;
                let rss_after = current_rss_mb();
                log_command_rss_delta(cmd, rss_before, rss_after);

                match result {
                    Some(Ok(CommandResult::Json(value))) => {
                        // Propagate operation-level failure: if the inner value
                        // has success:false, the IPC response must reflect that.
                        // Otherwise callers only see the transport-level success.
                        let is_inner_failure = value
                            .get("success")
                            .and_then(|v| v.as_bool())
                            .map(|s| !s)
                            .unwrap_or(false);
                        if is_inner_failure {
                            let error = value
                                .get("error")
                                .and_then(|v| v.as_str())
                                .unwrap_or("Operation failed")
                                .to_string();
                            HandleResult::Json(Response {
                                success: false,
                                result: Some(value),
                                error: Some(error),
                                request_id: None,
                            })
                        } else {
                            HandleResult::Json(Response::success(value))
                        }
                    }
                    Some(Ok(CommandResult::Binary { metadata, data })) => HandleResult::Binary {
                        json_header: Response::success(metadata),
                        binary_data: data,
                    },
                    // Cell shapes from MODULE-ARCHITECTURE.md §5.1.
                    // Handle: serialize the HandleRef as JSON over the
                    // wire; the TS-side caller holds it and passes back
                    // on subsequent calls (long-running session pattern
                    // — inference, training, hosting, ORM).
                    Some(Ok(other)) => match other.to_json_value() {
                        Ok(value) => HandleResult::Json(Response::success(value)),
                        Err(e) => HandleResult::Json(Response::error(e)),
                    },
                    Some(Err(e)) => HandleResult::Json(Response::error(e)),
                    None => {
                        // Don't dead-end with a developer-internal "no module registered"
                        // (the discoverability lie): suggest the nearest commands the
                        // caller can actually run, via the shared matcher — same PX every
                        // caller gets, persona or CLI or MCP.
                        let caller_trust = crate::routing::caller_trust(caller.as_ref());
                        let names: Vec<&str> = crate::sdk_codegen::command_registry()
                            .into_iter()
                            .filter(|d| {
                                crate::modules::grid::acl::is_command_authorized(d.name, caller_trust)
                            })
                            .map(|d| d.name)
                            .collect();
                        let suggestions = crate::commands::help::did_you_mean(cmd, &names);
                        let hint = if suggestions.is_empty() {
                            "Call `commands/help` with no arguments to list every command you can run."
                                .to_string()
                        } else {
                            format!("Did you mean: {}?", suggestions.join(", "))
                        };
                        HandleResult::Json(Response::error(format!(
                            "Unknown command: '{cmd}'. {hint}"
                        )))
                    }
                }
            } else {
                HandleResult::Json(Response::error(
                    "Missing 'command' field in request".to_string(),
                ))
            };
            let _ = tx.send(Outbound::Response {
                request_id,
                result: handle_result,
            });
        });
        inflight.push(inflight_handle);
    }

    // #85: the reader hit EOF — the client is gone. Abort any handler still running for this
    // connection (aborting an already-finished task is a harmless no-op). Fire-and-poll (#86)
    // jobs are untouched: their command task already returned the handle, so nothing here holds
    // them — only an abandoned BLOCKING request is cancelled, freeing its lane and CPU.
    let aborted = inflight.iter().filter(|h| !h.is_finished()).count();
    for h in &inflight {
        h.abort();
    }
    if aborted > 0 {
        log_debug!(
            "ipc",
            "server",
            "aborted {aborted} in-flight handler(s) for disconnected client: {peer_addr}"
        );
    }

    // Unregister any `Provided` capabilities this connection offered (an eye-node
    // going away). Pointer-matched so we never evict a NEWER eye-node that
    // re-registered the same command — after this, a persona's perception/observe
    // fails loud "no eye-node connected" rather than routing into a dead socket.
    if !my_registrations.is_empty() {
        for (provider, commands) in &my_registrations {
            state
                .provider_registry
                .unregister_matching(commands, provider);
        }
        log_debug!(
            "ipc",
            "server",
            "unregistered {} provider binding(s) for disconnected eye-node: {peer_addr}",
            my_registrations.len()
        );
    }
    // Drop this connection's provider Arcs BEFORE joining the writer: each held a
    // clone of `tx`, so the writer's channel would never close (deadlock) while
    // they live. (A mid-flight fulfill still holds a clone; it drops on timeout,
    // which is why the join can lag by at most the provideCall timeout in that
    // rare race — bounded, never forever.)
    drop(my_registrations);

    // Drop sender to signal writer thread to exit, then wait for it
    drop(tx);
    let _ = writer_handle.join();

    log_debug!("ipc", "server", "Client disconnected: {}", peer_addr);
    Ok(())
}

// ============================================================================
// Tests - Binary Framing & Protocol
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepare_unix_socket_path_creates_parent_dir() {
        let temp_dir = tempfile::tempdir().unwrap();
        let socket_path = temp_dir
            .path()
            .join("missing")
            .join("sockets")
            .join("continuum-core.sock");

        prepare_unix_socket_path(socket_path.to_str().unwrap()).unwrap();

        assert!(socket_path.parent().unwrap().is_dir());
    }

    #[test]
    fn prepare_unix_socket_path_removes_stale_socket_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let socket_path = temp_dir.path().join("continuum-core.sock");
        std::fs::write(&socket_path, b"stale").unwrap();

        prepare_unix_socket_path(socket_path.to_str().unwrap()).unwrap();

        assert!(!socket_path.exists());
    }

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

        let parsed_json: serde_json::Value =
            serde_json::from_slice(&frame[4..4 + parsed_length]).unwrap();
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
        let pcm_bytes: Vec<u8> = audio_samples.iter().flat_map(|s| s.to_le_bytes()).collect();

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
        let sep_idx = payload
            .iter()
            .position(|&b| b == 0)
            .expect("Should have separator");
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
        assert!(
            !bytes.contains(&0u8),
            "serde_json should never emit raw 0x00 byte, got: {:?}",
            serialized
        );
        // Should contain the escaped form
        assert!(
            serialized.contains("\\u0000"),
            "Null should be escaped as \\u0000"
        );
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
        let mut stream =
            UnixStream::connect(socket_path).expect("Failed to connect to continuum-core socket");

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
        let request =
            r#"{"command":"voice/synthesize","text":"Hello world","voice":"af","requestId":2}"#;
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
            assert_eq!(
                pcm_bytes.len(),
                num_samples as usize * 2,
                "PCM bytes should be 2 * num_samples"
            );

            // Verify PCM data is valid i16 audio (not all zeros)
            let samples: Vec<i16> = pcm_bytes
                .chunks_exact(2)
                .map(|c| i16::from_le_bytes([c[0], c[1]]))
                .collect();
            let max_amp = samples.iter().map(|s| s.abs()).max().unwrap_or(0);
            assert!(
                max_amp > 100,
                "Audio should not be silence, max amplitude: {}",
                max_amp
            );

            println!(
                "IPC voice/synthesize: {} samples, {}Hz, {}ms, {} bytes PCM, max amp: {}",
                num_samples,
                sample_rate,
                duration_ms,
                pcm_bytes.len(),
                max_amp
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

/// Ready-edge watch for the IPC server — flips `false → true` exactly
/// once when the Unix socket is bound + chmod'd. Any consumer can
/// `subscribe_ready()` to get a `watch::Receiver<bool>` and await the
/// transition; subscribers added after the flip see the current `true`
/// value on first `borrow_and_update`. Per the substrate concurrency
/// doctrine: signals replace races. Same shape as
/// `bevy_renderer::subscribe_ready` and `ServiceModule::ready_edge`.
static IPC_READY: std::sync::OnceLock<tokio::sync::watch::Sender<bool>> =
    std::sync::OnceLock::new();

fn ipc_ready_tx() -> &'static tokio::sync::watch::Sender<bool> {
    IPC_READY.get_or_init(|| tokio::sync::watch::channel(false).0)
}

/// Subscribe to the IPC server's ready edge. Returns a receiver that
/// starts at `false` and flips to `true` exactly once when the Unix
/// socket is bound + world-rw chmod'd. Replaces the prior oneshot
/// parameter that had to be threaded through `start_server` — now any
/// number of consumers can await the same edge without coordination
/// at the call site.
pub fn subscribe_ready() -> tokio::sync::watch::Receiver<bool> {
    ipc_ready_tx().subscribe()
}

pub fn start_server(
    socket_path: &str,
    livekit_manager: Arc<crate::live::transport::bridge_client::LiveKitAgentManager>,
    rt_handle: tokio::runtime::Handle,
    memory_manager: Arc<crate::memory::PersonaMemoryManager>,
    pressure_monitor: Arc<crate::system_resources::MemoryPressureMonitor>,
    disk_pressure_monitor: Arc<crate::system_resources::DiskPressureMonitor>,
    boot_mode: crate::runtime::BootMode,
) -> std::io::Result<()> {
    prepare_unix_socket_path(socket_path)?;

    log_info!(
        "ipc",
        "server",
        "Starting IPC server on {} (mode={})",
        socket_path,
        boot_mode.label()
    );

    // (Removed the startup UNSLOTH_API_KEY boot check. The local llama-server
    // gateway has no credential — its catalog entry is `auth: None` — so there is
    // no key to resolve or self-heal. Gateway readiness is now expressed by the
    // serving daemon's snapshot, checked at adapter registration, not a key probe.)

    // Load the model_registry BEFORE any ServiceModule is constructed.
    // Several adapters (AnthropicAdapter, LlamaCppAdapter, …) read from
    // `model_registry::global()` in their constructors — if init hasn't
    // happened yet those panic at module registration time. Failure here
    // is fatal: the registry is the single source of truth for model ids
    // and a missing config is a boot-order / packaging bug, not a runtime
    // condition we can recover from.
    match crate::model_registry::init_global() {
        Ok(reg) => log_info!(
            "ipc",
            "server",
            "model_registry loaded: {} models across {} providers",
            reg.models().count(),
            reg.providers().count()
        ),
        Err(e) => panic!("failed to load model_registry: {e}"),
    }

    // Create modular runtime
    log_info!("ipc", "server", "Initializing modular runtime...");
    let runtime = Arc::new(Runtime::new());

    // Phase 0: GPU Memory Manager (detect VRAM, create budgets)
    let gpu_manager = Arc::new(GpuMemoryManager::detect());

    // Provide GPU manager to TTS and renderer subsystems for VRAM tracking.
    // (Embedding no longer loads an in-process ONNX model — it is adapter-routed,
    // task #40 — so it has no GPU allocation to track here.)
    crate::live::audio::tts::set_gpu_manager(gpu_manager.clone());
    crate::live::video::bevy_renderer::set_gpu_manager(gpu_manager.clone());

    // Phase 1: HealthModule (stateless)
    runtime.register(Arc::new(HealthModule::new()));

    // NavModule — nav/mark-read: advance the shared read cursor + publish NAV_CHANGED
    // on the airc bus (nav slice 3). Command in, Event out; the write half of the
    // dual-consumer atom — one (user, room) cursor, read by the human unread badge AND
    // the persona's RAG grounding. Captures the bus in initialize (like vision).
    runtime.register(Arc::new(crate::modules::nav::NavModule::new()));

    // ai/should-respond — the kernel command that runs a persona's WorkspaceCycle
    // (the brain) and returns a Decision. Resolves the per-persona cycle from the
    // process-global PersonaWorkspaceRegistry, which is populated at persona spawn
    // (supervisor::materialize_adapters). One command, N lane-routed handlers;
    // this is the continuum-native handler. Additive — the recipe walker + the
    // service-loop cutover consume it; existing heuristics stay live until then.
    runtime.register(Arc::new(
        crate::cognition::should_respond_module::ShouldRespondModule::new(
            crate::cognition::persona_workspace::global(),
        ),
    ));

    // LaunchModeModule — system/launch-mode/{get,set} as typed self-routing commands
    // (`get` stateless, `set` dep-holding over the module bus). Headless-native
    // runtime lever for the headless-vs-UI launch preference; persists
    // CONTINUUM_LAUNCH_MODE to config.env (same key bin/continuum reads) and emits
    // system:launch-mode:changed so a running UI can attach/tear down its overlay.
    runtime.register(Arc::new(LaunchModeModule::new()));

    // AircBridgeDirectiveModule — recognizes inbound `!continuum` directives on
    // the airc bus (chat:posted) OFF the transport hot path, emitting an
    // observable airc:bridge:directive event. Passive subscriber, NO execution
    // (slice 3a). Header+kind gated so media/WebRTC never reach it.
    runtime.register(Arc::new(AircBridgeDirectiveModule::new()));

    // AircBridgeDispatchModule — consumes airc:bridge:directive (from 3a) and
    // emits airc:bridge:reply. ping/status reply locally; command-executing
    // directives are recognized but NOT executed (peer-command authorization is
    // slice 3b-2). Off-loop consumer, no kernel execution from peer content.
    runtime.register(Arc::new(AircBridgeDispatchModule::new()));

    // ExternalWebviewAuthModule — OAuth 2.0 + PKCE via system browser.
    // Landed in 26ab8c0ad; re-enabling after merge from feat/mac-docker-model-runner
    // briefly restored m5's stub from before 26ab8c0ad landed.
    runtime.register(Arc::new(ExternalWebviewAuthModule::new()));

    // Phase 1: GpuModule (GPU stats + pressure IPC)
    runtime.register(Arc::new(GpuModule::new(gpu_manager.clone())));

    // ForgeModule (continuum#1164 Phase 4 stub — forge/run IPC).
    // v1 returns a stub ForgeArtifact from a recipe; Phase 5+ wires the
    // real foundry executor.
    runtime.register(Arc::new(ForgeModule::new()));

    // EventsModule (L1-1 — event-class declaration registry).
    // Spec: GRID-BUS-ARCHITECTURE §2.2 (continuum#1439).
    // Exposes events/declare-class, events/get-class, events/list-classes,
    // events/resolve-channel. The TS thin shim at src/system/events/shared/
    // EventClass.ts reads through this; the L1-2 AircEventTransport will
    // consult resolve-channel at emit time.
    runtime.register(Arc::new(EventsModule::new()));

    // Phase 1: PersonaAllocatorModule (hardware-aware persona allocation)
    runtime.register(Arc::new(PersonaAllocatorModule::new(gpu_manager.clone())));

    // Phase 1: SystemResourceModule (CPU + memory + process monitoring IPC)
    let system_monitor = Arc::new(SystemResourceMonitor::new());
    let system_resource_module = Arc::new(SystemResourceModule::new(system_monitor.clone()));
    system_resource_module.set_pressure_monitor(pressure_monitor.clone());
    runtime.register(system_resource_module);

    // ServingDaemonModule — the ever-present control loop that decides (and
    // re-decides every tick) how this host serves persona inference: which base
    // model, how many continuous-batching lanes, how many models warm. Budget
    // comes from the LIVE free-memory monitor (organic ebb/flow — drops when a
    // game/build/renderer grabs memory) capped at physical VRAM. It is ONE
    // consumer of the holistic resource budget the PressureBroker arbitrates
    // across inference + TTS/STT + classifier CNNs + renderers; next refinement
    // is negotiating an arbitrated share from the broker rather than reading
    // raw free memory. Registered after the monitor so it can read it.
    // The live model universe — the runtime-mutable watch-snapshot layer SEEDED
    // from the immutable registry global (initialized above at `init_global`).
    // Constructed HERE so the SAME `Arc<ModelCatalog>` is shared by the serving
    // daemon (which plans off its snapshot) and the `models/*` command surface
    // (which mutates it). One owner, one live universe: a `models/pull` that
    // flips a model Ready is seen by the very next serving tick — no reboot.
    let model_catalog = Arc::new(crate::model_registry::live::ModelCatalog::from_registry(
        crate::model_registry::global(),
    ));

    // The ONE per-machine resource authority (#56). Its VRAM ceiling comes from
    // the LIVE GpuMonitor (`gpu::monitor::detect()` — Metal + every NVIDIA host),
    // so serving leases against capacity net of Bevy/LiveKit + outstanding leases
    // rather than a fraction of *total* VRAM (the host_budget() OOM bug). Bytes
    // held off the top so we never lease the last sliver the driver/compositor
    // needs; the ceiling already nets out external *resident* usage, so this is a
    // small safety reserve, not a budget for the compositor.
    const GPU_SAFETY_RESERVE_BYTES: u64 = 512 * 1024 * 1024;
    // `gpu::monitor::detect()` (→ `MetalMonitor::new`/`NvidiaMonitor::new`) and
    // `ResourceDaemon::start()` each adopt the canonical Daemon base, which calls
    // `tokio::spawn` from inside the constructor to own its interval task. But
    // `start_server` runs on a plain `std::thread` (main spawns it OFF the runtime
    // so its blocking accept-loop never steals a tokio worker — see main.rs), so
    // there is no ambient reactor and those `tokio::spawn`s panic "no reactor
    // running". Enter the runtime context for exactly this daemon-construction
    // region. The guard is scoped so it drops here, well before the `block_on`
    // regions further down — we never hold a runtime-context guard across a
    // `block_on`.
    let resource_daemon = {
        let _rt_guard = rt_handle.enter();
        let mut capacity_sources: Vec<Arc<dyn crate::resources::CapacitySource>> = Vec::new();
        match crate::gpu::monitor::detect() {
            Some(monitor) => {
                log_info!(
                    "ipc",
                    "server",
                    "ResourceGovernor: live VRAM scan via {} ({}) — VRAM is governed",
                    monitor.platform(),
                    monitor.device_name()
                );
                capacity_sources.push(Arc::new(crate::resources::GpuCapacitySource::new(
                    monitor,
                    GPU_SAFETY_RESERVE_BYTES,
                )));
            }
            None => {
                // GpuMemoryManager::detect() already panics on a truly GPU-less host,
                // so reaching here means a GPU exists but has no live GpuMonitor
                // adapter yet — the non-NVIDIA Vulkan (VK_EXT_memory_budget/ash) gap.
                // Name it loudly: VRAM stays UNGOVERNED and serving fails closed
                // (host_budget caps at 0) rather than over-committing against a
                // fabricated number. Build the Vulkan adapter or run on Metal/NVIDIA.
                log_error!(
                    "ipc",
                    "server",
                    "ResourceGovernor: NO live GpuMonitor (nvidia-smi absent and the \
                     non-NVIDIA Vulkan VK_EXT_memory_budget adapter is not built) — \
                     VRAM is UNGOVERNED; serving will refuse until a live monitor exists"
                );
            }
        }
        crate::resources::ResourceDaemon::start(
            capacity_sources,
            Vec::new(),
            crate::resources::DaemonConfig::default(),
        )
    };

    let serving_daemon = Arc::new(crate::modules::serving_daemon::ServingDaemonModule::new(
        gpu_manager.clone(),
        system_monitor.clone(),
        resource_daemon.clone(),
        model_catalog.clone(),
    ));
    runtime.register(serving_daemon.clone());

    // #79: expose the one per-machine resource authority's accounting board as a typed
    // read command (`resources/board`). The daemon owns its background poll + watch
    // snapshot; this thin module wraps the same `Arc<ResourceDaemon>` so the measured
    // per-consumer attributions + drift are readable by an operator, a persona, or a
    // grid peer — the reporting half of "accurate footprint() drift-reporting."
    runtime.register(Arc::new(
        crate::modules::resources_module::ResourcesModule::new(resource_daemon.clone()),
    ));

    // Phase 2 of #1239 (continuum#1299 PR-1): PressureBrokerModule.
    // Brings the cross-pool PressureBroker online — instantiates the
    // singleton, pre-registers DockerTierPool as a ResourcePool, and
    // hands the broker's `relieve()` tick to the runtime's standard
    // start_tick_loops() machinery (cadence = BrokerConfig.tick_interval,
    // default 5s, matching DMR_TICK_INTERVAL). Other pools (VRAM, KV
    // cache) attach via `module.broker().register(...)` from their own
    // construction sites. Observer-only in PR-1: no commands routed
    // here yet. PR-2 of #1299 adds `system/pressure-broker-state` IPC;
    // PR-3 wires the chat-substrate alert sink.
    runtime.register(Arc::new(
        crate::modules::pressure_broker_module::PressureBrokerModule::new(),
    ));
    // InferenceCoordinatorModule — stands up the multi-persona-one-model
    // lane coordinator. Registered before the broker block below so its
    // CoordinatorResourcePool can be attached to the broker in the same
    // pass, closing the realistic-lane pressure→eviction loop in
    // production. HARDWARE-DETECTED silicon (probes the machine: Gpu on a
    // discrete GPU, UnifiedMemory on Apple Silicon, Cpu on a GPU-less host)
    // — no hardcoded CPU/UMA floor. Per-tier budgets governor-informed in a
    // later slice. Opens route through the same coordinator Arc via the
    // handle module.
    runtime.register(Arc::new(
        crate::modules::inference_coordinator_module::InferenceCoordinatorModule::with_detected_hardware(),
    ));
    // Register DiskPressureMonitor with the broker as a signal-only
    // ResourcePool — `evict_at_least` returns 0 (the monitor doesn't
    // own files), but the broker emits typed PressureAlert events on
    // tier transitions, surfacing disk pressure on the same wire +
    // dashboard as memory + Docker + LoRA + KV pools. Concrete disk
    // pools (genome cache, probe JSONL rotation, model registry) will
    // register their own ResourcePool impls in follow-up slices and
    // the broker will drive eviction against THOSE. Per task #88
    // "broker pool" half.
    let broker_arc = runtime
        .registry()
        .module_of_type::<crate::modules::pressure_broker_module::PressureBrokerModule>()
        .and_then(|m| {
            m.as_any()
                .downcast_ref::<crate::modules::pressure_broker_module::PressureBrokerModule>()
                .map(|bm| bm.broker())
        });
    if let Some(broker) = broker_arc {
        broker
            .register(disk_pressure_monitor.clone() as Arc<dyn crate::paging::pool::ResourcePool>);
        broker.register(pressure_monitor.clone() as Arc<dyn crate::paging::pool::ResourcePool>);

        // Cargo-target eviction owner (task #155 wire 2). The 2026-07-13
        // incident: the broker spent days emitting the designed zero-byte
        // "disk hot AND nobody owns the eviction" alerts while the unswept
        // cargo-target cache reached 363 GB. This pool owns that class:
        // budget-capped, flock-guarded against live builds, derived
        // artifacts only (safe on any user's machine — the next build
        // recreates everything it deletes). Shares its TrackedDir with the
        // disk reporter — one measurement per class.
        if let Some(cargo_dir) = crate::system_resources::tracked_dir("cargo-target") {
            broker.register(Arc::new(crate::system_resources::CargoTargetPool::new(
                cargo_dir,
                crate::system_resources::DEFAULT_CARGO_TARGET_BUDGET_BYTES,
            )) as Arc<dyn crate::paging::pool::ResourcePool>);
            log_info!(
                "ipc",
                "server",
                "CargoTargetPool registered with PressureBroker (budget-capped, flock-guarded)"
            );
        }

        // Wire the resource authority's per-kind lease pools onto the broker so
        // cross-resource pressure relief reaches VRAM/RAM/disk leases: when a
        // kind goes over its scanned ceiling (a game grabs VRAM), the broker's
        // tick asks the lease pool to free bytes and the daemon's reconcile
        // claws them back from the lowest-priority leaseholder. One orchestrator
        // (the broker), not a parallel one. (#56)
        resource_daemon.register_with_broker(&broker);
        log_info!(
            "ipc",
            "server",
            "ResourceDaemon lease pools registered with PressureBroker (VRAM/RAM/disk leases)"
        );

        // Register a `FilesystemTierPool` for the probe JSONL
        // rotation dir — closes the broker → pool → real eviction
        // loop end-to-end. When disk pressure crosses the broker's
        // act_above threshold, the broker walks the registered pools
        // and asks each to free bytes; this pool actually does it
        // (deletes oldest probe files until want_bytes are freed).
        //
        // The probe sink's own `tracing_appender::rolling` handles
        // the time axis (daily rotation, 7-day retention); this pool
        // handles the space-pressure axis (broker-driven eviction
        // when disk is hot). Both bounds active simultaneously.
        //
        // Soft cap = 500 MB. Picked to be larger than a typical
        // 7-day probe window (probably < 100 MB even under heavy
        // capture) but bounded so a runaway sprinkle / leak gets
        // visible to the broker before disk fills.
        const PROBE_POOL_SOFT_CAP_BYTES: u64 = 500 * 1024 * 1024;
        if let Some(home) = dirs::home_dir() {
            let probe_dir = home.join(".continuum/jtag/logs/probes");
            broker.register(Arc::new(crate::paging::FilesystemTierPool::new(
                "probe-jsonl",
                probe_dir,
                PROBE_POOL_SOFT_CAP_BYTES,
            )) as Arc<dyn crate::paging::pool::ResourcePool>);
            log_info!(
                "ipc",
                "server",
                "FilesystemTierPool 'probe-jsonl' registered with PressureBroker (soft cap 500 MB)"
            );
        } else {
            log_error!(
                "ipc",
                "server",
                "no HOME — probe-jsonl FilesystemTierPool not registered"
            );
        }

        // Register a `FilesystemTierPool` for the tool-output spill dir — the
        // space-pressure axis of tier-2 flood protection. When a build/test/read
        // tool overflows the context budget, the executor spills the WHOLE result
        // to `~/.continuum/tool-output/<persona_id>/<handle>.log` so the persona
        // can grep it back with `tool/output`. Those artifacts accrete with no
        // intrinsic bound; this pool lets the broker delete the oldest spills
        // (recursively across persona dirs) when disk goes hot — exactly the
        // probe-jsonl pattern above, against the spill root.
        //
        // Soft cap = 1 GB. Larger than the probe pool because a single Xcode /
        // cargo build log can be tens of MB and many personas spill in parallel,
        // but bounded so a runaway tool can't fill the disk before the broker acts.
        // Path comes from `spill::spill_root()` — single source, no re-typed string.
        const SPILL_POOL_SOFT_CAP_BYTES: u64 = 1024 * 1024 * 1024;
        match crate::cognition::tool_executor::spill::spill_root() {
            Ok(spill_dir) => {
                broker.register(Arc::new(crate::paging::FilesystemTierPool::new(
                    "tool-output-spill",
                    spill_dir,
                    SPILL_POOL_SOFT_CAP_BYTES,
                ))
                    as Arc<dyn crate::paging::pool::ResourcePool>);
                log_info!(
                    "ipc",
                    "server",
                    "FilesystemTierPool 'tool-output-spill' registered with PressureBroker (soft cap 1 GB)"
                );
            }
            Err(e) => {
                log_error!(
                    "ipc",
                    "server",
                    "no HOME — tool-output-spill FilesystemTierPool not registered: {e}"
                );
            }
        }

        log_info!(
            "ipc",
            "server",
            "Disk + Memory pressure monitors registered as ResourcePools ('disk-root', 'sys-memory') with PressureBroker"
        );

        // Register the inference lane coordinator's pool so the broker's
        // tick drives lane eviction (expired → Hard → Graceful, never an
        // active Pinned lane) the same way it drives disk/Docker eviction.
        // Until opens route through the coordinator (follow-up slice) the
        // pool is armed but idle (usage 0 → Normal tier → broker no-ops),
        // which is the correct inert state for the not-yet-trafficked path.
        let coordinator_pool_registered = runtime
            .registry()
            .module_of_type::<crate::modules::inference_coordinator_module::InferenceCoordinatorModule>()
            .and_then(|m| {
                m.as_any()
                    .downcast_ref::<crate::modules::inference_coordinator_module::InferenceCoordinatorModule>()
                    .map(|cm| cm.register_with_broker(&broker))
            })
            .is_some();
        if coordinator_pool_registered {
            log_info!(
                "ipc",
                "server",
                "InferenceCoordinator lanes registered as ResourcePool ('inference-lanes') with PressureBroker"
            );
        } else {
            // Mirrors the parent broker-fetch failure log: a load-bearing
            // registration that silently no-ops would hide the lane pool
            // from pressure relief. Unreachable today (the module is
            // registered unconditionally above) but logged for symmetry.
            log_error!(
                "ipc",
                "server",
                "InferenceCoordinatorModule not retrievable after registration — lane pool won't appear on the broker"
            );
        }
    } else {
        log_error!(
            "ipc",
            "server",
            "PressureBrokerModule not retrievable after registration — pressure monitors won't appear on the broker"
        );
    }

    // Runtime-owned lease ledger for CPU/GPU/memory/disk/network admission.
    // Subsystems ask this broker for capacity instead of keeping private caps.
    runtime.register(Arc::new(
        crate::modules::resource_broker::ResourceBrokerModule::new(),
    ));

    // `inference/capacity` is now a stateless self-routing command
    // (`commands/inference/capacity.rs`) — same single-source-of-truth RAM
    // formula (issue #887), no module shell needed. See the typed registry.

    // InferenceHandleModule — the `ai/inference/{open,generate,close,inspect}`
    // lane command surface, routed through the SAME `InferenceCoordinator`
    // the InferenceCoordinatorModule stood up + registered with the broker.
    // This makes the realistic-lane path LIVE: opens create lanes (consuming
    // the coordinator's admission budget + reporting footprint), and the
    // broker's tick evicts them under pressure. Disjoint from InferenceModule
    // above — `ai/inference/` vs `inference/`, "ai-inference-handle" vs
    // "inference" — so this is purely additive, not a replacement.
    let handle_coordinator = runtime
        .registry()
        .module_of_type::<crate::modules::inference_coordinator_module::InferenceCoordinatorModule>()
        .and_then(|m| {
            m.as_any()
                .downcast_ref::<crate::modules::inference_coordinator_module::InferenceCoordinatorModule>()
                .map(|cm| cm.coordinator())
        });
    if let Some(coordinator) = handle_coordinator {
        runtime.register(Arc::new(
            crate::inference::handle_module::InferenceHandleModule::with_coordinator(coordinator),
        ));
        log_info!(
            "ipc",
            "server",
            "InferenceHandleModule registered (ai/inference/* routed through the broker-managed coordinator)"
        );
    } else {
        log_error!(
            "ipc",
            "server",
            "InferenceCoordinatorModule missing — ai/inference/* lane surface not registered"
        );
    }

    // Phase 5: InferenceLlmModule (MODULE-CATALOG §II `inference-llm`)
    // — the substrate's local-LLM generation surface. Subscribes to
    // inference/llm/request commands, returns InferenceComplete +
    // FirstTokenEmitted bundles. Stub-backed in PR-2; adapter-routed
    // in PR-4 (#1395) when constructed via with_adapter. PR-5 (this
    // registration) wires the module into the runtime so it's
    // callable from the cognition path — no Runtime adapter wiring
    // yet (caller construction option lands when persona-cognition
    // composes via with_bus_and_adapter).
    //
    // Shipped via the .new() constructor (bus-less, stub-backed)
    // so this PR doesn't bind us to a specific LlamaCppAdapter
    // initialization story; downstream PRs swap construction when
    // the LlamaCppAdapter init lifecycle is integrated with the
    // Runtime startup phase.
    runtime.register(Arc::new(
        crate::inference::llm_module_service::InferenceLlmModule::new(),
    ));

    // Lane C PR-3: VddModule — `vdd/report` reads structured
    // VDD records from `~/.continuum/vdd/<sha>/<scenario>/record.jsonl`
    // (written by the harness via `ArtifactWriter`) and emits a
    // machine-readable report. Replaces "tail the log and grep
    // for first-token-ms" with a single command return. PR-body
    // VDD claims become `./jtag vdd/report --git_sha=<sha>`,
    // not pasted terminal text.
    runtime.register(Arc::new(crate::modules::vdd::VddModule::new()));

    // Shared state for per-persona cognition (unified: engine + inbox + rate limiter + sleep + adapters + genome)
    let rag_engine = Arc::new(RagEngine::new());
    let cognition_state =
        Arc::new(CognitionState::new(rag_engine.clone()).with_gpu_manager(gpu_manager.clone()));
    let personas = cognition_state.personas.clone();
    runtime.register(Arc::new(CognitionModule::new(cognition_state)));

    // Channel module shares the unified personas map for fast-path decisions
    let channel_registries: Arc<DashMap<Uuid, (ChannelRegistry, PersonaState)>> =
        Arc::new(DashMap::new());
    let channel_state = Arc::new(ChannelState::from_existing(
        channel_registries.clone(),
        personas,
    ));
    runtime.register(Arc::new(ChannelModule::new(channel_state)));

    // Phase 3: ModelsModule holds the SAME live model universe constructed above
    // (shared with the serving daemon). The rich `models/*` commands capture this
    // one `Arc<ModelCatalog>`, so every caller reads/mutates — and the serving
    // daemon plans off — the SAME live universe.
    runtime.register(Arc::new(ModelsModule::new(
        model_catalog,
        crate::modules::ai_provider::global_registry(),
        // The serving daemon's live snapshot — so `models/remove` refuses to
        // delete weights out from under the currently-served lane.
        serving_daemon.subscribe_serving(),
    )));

    // Phase 3: MemoryModule (wraps PersonaMemoryManager)
    let memory_state = Arc::new(MemoryState::new(memory_manager.clone()));
    runtime.register(Arc::new(MemoryModule::new(memory_state)));

    // Phase 3: RagModule (batched RAG composition with parallel Rayon loading)
    let rag_state = Arc::new(RagState::new(memory_manager.clone()));
    runtime.register(Arc::new(RagModule::new(rag_state)));

    // Phase 3: VoiceModule (wraps VoiceService, CallManager, AudioBufferPool)
    let voice_service = Arc::new(crate::live::session::voice_service::VoiceService::new());
    let audio_pool = Arc::new(crate::live::audio::buffer::AudioBufferPool::new());
    let voice_state = Arc::new(VoiceState::new(
        voice_service.clone(),
        livekit_manager.clone(),
        audio_pool.clone(),
    ));
    // Voice joins the resource authority as a peer consumer (#56): under VRAM
    // pressure it REFUSES while a call is live (never kicks a human mid-call) and
    // sheds its idle STT/TTS models otherwise — serving tiers down first.
    resource_daemon.add_consumer(Arc::new(
        crate::modules::live_session_consumer::VoiceConsumer::new(
            voice_state.resource_lifecycle.clone(),
            gpu_manager.clone(),
        ),
    ));
    // The Bevy avatar renderer joins as the third peer consumer (#56): fat and
    // reclaimable (~3GB) when idle, but its output texture IS the LiveKit video
    // feed during a call — so under pressure while a call is live it REFUSES
    // (tearing it down would freeze the avatar mid-call) and sheds the renderer
    // only when nothing is rendering. Shares the same live-session lifecycle as
    // voice, so both sides of a call are protected together.
    resource_daemon.add_consumer(Arc::new(crate::modules::bevy_consumer::BevyConsumer::new(
        voice_state.resource_lifecycle.clone(),
        gpu_manager.clone(),
    )));
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

    // ChatModule: the kernel `chat/send` + `chat/poll` surface (aliases of
    // `collaboration/chat/*`). Unlike `search/*` these DynCommands are NOT
    // inventory-self-registering — they carry a late-bound `CommandExecutor`
    // (via `command_objects(executor_slot)`) so `chat/send` can dual-write to
    // `data/*` + airc. That injected state is why the module is the required
    // carrier. It was defined and schema-registered but never wired into boot,
    // so `chat/send`/`chat/poll` LISTED via `commands/list` yet failed to route
    // ("No module registered for this command prefix") — a discoverability lie.
    // The slot is filled by `install_executor_on_all` after all registration,
    // so ordering here is irrelevant.
    // (#140: the durable-transcript writer is spawned by ChatModule::initialize
    // on the runtime handle — registration here runs on a non-tokio thread.)
    runtime.register(Arc::new(crate::modules::chat::ChatModule::new()));

    // Phase 4a: LoggerModule (absorbs standalone logger worker)
    // Provides log/write, log/ping via main socket
    runtime.register(Arc::new(LoggerModule::new()));

    // search/* migrated to the DynCommand registry (commands/search/*) — the four
    // verbs self-register via inventory, no module registration needed here.

    // Phase 4c: EmbeddingModule (absorbs standalone embedding worker)
    // Provides embedding/generate, embedding/model/{load,list,info,unload}
    runtime.register(Arc::new(EmbeddingModule::new()));

    // RuntimeModule: Exposes metrics and control for AI-driven system management (Ares)
    // Provides runtime/metrics/{all,module,slow}, runtime/list
    runtime.register(Arc::new(
        crate::modules::runtime_control::RuntimeModule::new(),
    ));

    // MCPModule: Dynamic tool discovery for MCP servers
    // Provides mcp/list-tools, mcp/search-tools, mcp/tool-help
    runtime.register(Arc::new(crate::modules::mcp::MCPModule::new()));

    // AgentModule: Autonomous AI coding agents with structured tool calling
    // Provides agent/start, agent/status, agent/stop, agent/list, agent/wait
    runtime.register(Arc::new(AgentModule::new(rt_handle.clone())));

    // AircModule: Rust-native AIRC queue/flywheel primitives.
    // Provides airc/queue-scan without routing through Node/TypeScript.
    // Discovery: `AircModule::discover_and_construct` asks `airc ipc-
    // endpoint` (airc#1095) for the canonical daemon socket and auto-
    // installs airc if missing — the previous derive-from-home scheme
    // drifted and broke headless boot. Uses rt_handle.block_on because
    // start_server is sync but discovery is async; we're on the main
    // bootstrap thread, not inside a tokio task, so blocking here is
    // safe and gates module registration on the discovery result.
    //
    // Outer 180s timeout caps total boot stall. Inner subprocess
    // waits have their own per-call deadlines (5s socket discovery,
    // 5s peer_id status, 120s auto-install) but the OUTER call has
    // no overall budget without this wrapper — a wedged daemon
    // could theoretically chain stalls beyond what individual
    // deadlines catch. 180s covers worst-case auto-install + a few
    // discovery rounds. Reviewer-defect-driven (continuum #1507
    // finding 6); substrate-is-a-good-citizen "predictable startup"
    // non-negotiable.
    const AIRC_DISCOVERY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(180);
    let discovery = rt_handle.block_on(async {
        match tokio::time::timeout(AIRC_DISCOVERY_TIMEOUT, crate::airc::discover()).await {
            Ok(d) => d,
            Err(_) => {
                tracing::error!(
                    timeout_secs = AIRC_DISCOVERY_TIMEOUT.as_secs(),
                    "AIRC discovery exceeded outer timeout — promoting to Unreachable."
                );
                crate::airc::AircDiscovery::Unreachable {
                    reason: crate::airc::DiscoveryFailure::EndpointCommandFailed(format!(
                        "discovery did not return within {}s — substrate is unresponsive",
                        AIRC_DISCOVERY_TIMEOUT.as_secs()
                    )),
                }
            }
        }
    });
    tracing::info!(
        kind = discovery.kind(),
        reason = ?discovery.reason(),
        "AIRC discovery complete"
    );
    // Honest boot-health line for the persona-comms-over-airc wire (card
    // e9f50a36 slice A3, reworked onto core/continuum-core). Operators +
    // sentinels see at boot whether airc is live / degraded / unreachable —
    // emitted via boot_status -> probe! (class "boot.status"), so it's both a
    // human stderr line AND subscribable by debug/probes/*. This is the
    // comms→airc lane making "is the persona comms wire actually up?" honest.
    {
        use crate::runtime::boot_status::{boot_status, BootStatusKind};
        let (kind, detail) = match &discovery {
            crate::airc::AircDiscovery::Healthy {
                socket,
                peer_id,
                room_name,
                ..
            } => {
                let peer_short: String = peer_id.to_string().chars().take(8).collect();
                (
                    BootStatusKind::Ok,
                    format!(
                        "socket={} peer={} room={}",
                        socket.display(),
                        peer_short,
                        room_name
                    ),
                )
            }
            crate::airc::AircDiscovery::Degraded { reason, .. } => {
                (BootStatusKind::Degraded, format!("degraded: {reason}"))
            }
            crate::airc::AircDiscovery::Unreachable { reason } => {
                (BootStatusKind::Failed, format!("unreachable: {reason}"))
            }
        };
        boot_status("airc", kind, &detail);
    }
    let airc_module = Arc::new(AircModule::from_discovery(&discovery));
    let persona_bootstrap_deps = airc_module
        .daemon_socket()
        .map(|p| p.to_path_buf())
        .zip(airc_module.default_room());
    let persona_bootstrap_room_name = airc_module.default_room_name().map(|s| s.to_string());
    // The node-level presence emitter (WS seam below) needs the same
    // (daemon_socket, default_room) the persona block consumes. Clone the
    // deps here BEFORE that `if let Some(...)` moves them, so the emitter
    // can attach a heartbeat-less node reader against the same daemon +
    // room the citizens attach to.
    let node_presence_deps = persona_bootstrap_deps.clone();
    runtime.register(airc_module);

    // A.2 [[no-fallbacks-ever]]: in `FullCitizen` or `FailFast` mode,
    // the substrate REFUSES to boot if AIRC isn't `Healthy` — there's
    // no path where degraded discovery silently substitutes
    // inference-only mode (Slice A's R2#2 violation). Operator must
    // explicitly opt into `--mode=inference-only` to allow degraded
    // boot. The seed-presence heuristic Slice A used is gone.
    if boot_mode.requires_persona_hosting() && !discovery.can_host_personas() {
        let reason_msg = discovery
            .reason()
            .map(|r| format!("{}", r))
            .unwrap_or_else(|| "AIRC discovery did not produce Healthy state".to_string());
        tracing::error!(
            mode = boot_mode.label(),
            discovery_kind = discovery.kind(),
            reason = %reason_msg,
            "Refusing to boot: --mode={} requires AIRC Healthy, but discovery is {}. \
             Resolve the AIRC issue (see error above) OR re-launch with \
             --mode=inference-only to opt into degraded operation.",
            boot_mode.label(),
            discovery.kind()
        );
        return Err(std::io::Error::other(format!(
            "AIRC discovery {} but --mode={} requires Healthy. \
             Reason: {}. Resolve AIRC or use --mode=inference-only ([[no-fallbacks-ever]])",
            discovery.kind(),
            boot_mode.label(),
            reason_msg
        )));
    }

    // PersonaInstanceManagerModule: owns the live PersonaAircRuntime
    // registry — the kernel's roster of citizens in The Grid. Exposes
    // `persona/instances/bootstrap`, `persona/instances/list`,
    // `persona/instances/get`. Only registered when AIRC discovery
    // produced both a daemon socket AND a default room — without
    // either, citizens have nowhere to attach. The degraded path
    // logs and skips registration so the rest of the server boots;
    // the operator's remedy is the same as for AIRC discovery
    // failures (install airc / run `airc room <name>`).
    // Set by the persona-supervisor block below when persona hosting
    // is wired. Fired AFTER `init_executor_with_interceptors` so the
    // spawned supervisor task can safely dereference `executor()` in
    // its eventual `bootstrap_one` calls. None in `--mode=inference-only`
    // where personas are not hosted at all.
    // Carries the WIRED executor (GridTrustAuthPolicy + interceptors) to the
    // persona supervisor task once it's built — both the readiness signal AND the
    // executor the personas' hands ride. One channel, two jobs (order + payload).
    let mut persona_supervisor_executor_ready_tx: Option<
        tokio::sync::oneshot::Sender<Arc<crate::runtime::CommandExecutor>>,
    > = None;

    if let Some((daemon_socket, default_room)) = persona_bootstrap_deps {
        // Grid capacity gossip (#56 step 4): this node offers its live capacity to
        // the grid on the module tick and hears every peer's offers (its own echo
        // included — the loopback proof) via inbound_attach → gossip::global_ledger.
        // Rides the DISCOVERED default room, same dep the citizens attach to.
        runtime.register(Arc::new(crate::modules::grid_capacity::GridCapacityModule::new(
            resource_daemon.clone(),
            default_room,
        )));
        let continuum_root = crate::modules::persona_instance_manager::resolve_continuum_root();
        let daemon_socket_for_rag_inspect = daemon_socket.clone();
        let registry = crate::persona::PersonaAircRuntimeRegistry::new();
        // Publish the live roster process-globally so host-independent callers — the
        // detached cognition/eval body — can acquire an eval-preemption lease over
        // the fleet without a threaded handle. First writer wins (this boot path).
        crate::persona::PersonaAircRuntimeRegistry::set_global(registry.clone());
        // Publish THE bus process-globally too (same first-writer-wins shape) so
        // host-independent bodies — the detached eval's `eval:progress` scoreboard —
        // can emit events widgets/observers subscribe to (#123/#141).
        crate::runtime::MessageBus::set_global(runtime.bus_arc());
        crate::resources::ResourceDaemon::set_global(resource_daemon.clone());
        // Native airc kanban tools — personas claim/create/release cards on the
        // shared board as THEIR OWN airc key, delegating to airc's work API. Shares
        // the SAME registry (cheap Clone over an inner Arc) so a work command can
        // resolve the calling persona's live airc runtime. Registered before the
        // executor is built so its typed commands land on the one registry.
        runtime.register(Arc::new(crate::modules::work::WorkModule::new(
            registry.clone(),
        )));
        // SubstrateGovernor — the deterministic cognitive-region scheduler daemon.
        // Schedules the ChannelDigestRegion: per live persona it pre-stages the
        // persona's current-channel digest into the SHARED digest buffer
        // (channel_substrate globals) that AircRagSource peeks. Flood-safe — building
        // a digest runs NO inference (element embeddings are lazy). Slice 2C live.
        let digest_region: Arc<dyn crate::runtime::BrainRegion> = Arc::new(
            crate::cognition::channel_digest_region::ChannelDigestRegion::with_buffer(
                crate::cognition::channel_substrate::global_channel_digest_builder(),
                Arc::new(registry.clone())
                    as Arc<dyn crate::cognition::channel_digest_region::PersonaChannelReader>,
                crate::cognition::channel_substrate::global_channel_digest_buffer(),
            ),
        );
        // The dream/consolidation region goes LIVE (#145 slice B): per live persona,
        // on a material-driven cadence (dreams only when undigested episodic
        // experience accrues, `CadenceHint::Sleep` otherwise), it distills episodic
        // clusters into durable Semantic facts and leaves ONE `[thought:historian]`
        // SelfReflection per dreaming tick — the first mind-wanderer. Hippocampus +
        // adapter resolve per tick from the live workspace registry (re-home-safe),
        // never a parallel persona→adapter map.
        let dream_region: Arc<dyn crate::runtime::BrainRegion> = Arc::new(
            crate::cognition::dream_consolidation::DreamConsolidationRegion::new(
                crate::cognition::persona_workspace::global()
                    as Arc<dyn crate::cognition::dream_consolidation::PersonaReflectionSource>,
            ),
        );
        // Wire the live memory-pressure feed (R4 slice 3): each pass sizes its slice
        // budget to the host's current memory band so a society of inference-bearing
        // background regions can't stampede the model backend under load. A homeostatic
        // protection, not cognition steering — and on a healthy host the band is Normal
        // → budget None → behavior is identical to the uncapped default the digest
        // region runs under today (the digest runs no inference; the consolidation
        // region is the first InferenceHeavy tenant under this floor).
        runtime.register(Arc::new(
            crate::runtime::SubstrateGovernor::new(
                vec![digest_region, dream_region],
                registry.clone(),
            )
            .with_pressure_gate(pressure_monitor.subscribe()),
        ));
        let instance_manager = Arc::new(
            crate::modules::persona_instance_manager::PersonaInstanceManagerModule::new(
                registry,
                daemon_socket,
                default_room,
                persona_bootstrap_room_name.clone(),
                continuum_root,
            ),
        );
        // Task #222 + R1/R2 BLOCK on PR #1568: the executor lookup
        // happens LAZILY inside `bootstrap_one` (in PIM), so no
        // ordering coupling with `init_executor_with_interceptors`
        // further down in `start_server`. The pre-fix shape
        // eagerly fetched the global here and panicked because
        // `init_executor` hadn't run yet.
        runtime.register(instance_manager.clone());
        log_info!(
            "ipc",
            "server",
            "PersonaInstanceManagerModule registered — citizens can be bootstrapped via \
             `persona/instances/bootstrap`"
        );

        // `grid/grant/issue`: the owner mints capability grants signed by a running
        // persona's airc identity (sharing the same runtime registry). Owner-gated
        // by ACL (not in the cross-grid allow-list), so only the local operator can
        // sell its personas' compute. Closes the contracted-grid loop with an
        // operator surface over `routing::grant_issuance::issue_grant`.
        runtime.register(Arc::new(
            crate::modules::grant_issuance::GrantIssuanceModule::new(
                instance_manager.registry().clone(),
            ),
        ));
        log_info!(
            "ipc",
            "server",
            "GrantIssuanceModule registered — owner can mint grants via `grid/grant/issue`"
        );

        // ── persona/rag-inspect — RAG introspection callable from any AI ──
        //
        // FilesystemPersonaResolver reads the persona's seed.json + attaches
        // via airc_lib::Airc::attach_as using the same continuum_root +
        // daemon_socket the instance manager just used. The module exposes
        // the `persona/rag-inspect` command so sentinel personas, Claude,
        // and any other AI can `Commands.execute('persona/rag-inspect', {
        // persona: 'Paige' })` to honestly see what Paige's RAG layer would
        // surface right now. Per [[observability-is-half-the-architecture]].
        //
        // chain_inference path stays RAG-only here (default_adapter=None).
        //
        // The old reason cited on this line — "AdapterRegistry is Box-based
        // + can't hand out Arcs" — is a STALE LIE (same class as #77): #162
        // already moved the registry to `Arc<dyn AIProviderAdapter>` and
        // added `get_arc`, and `global_registry()` hands out Arcs today.
        //
        // The REAL blocker is per-persona model resolution. The canonical
        // live generation path (`cognition/generate_response.rs`) selects
        // via `registry.select(DEFAULT_GENERATE_PROVIDER, Some(&session.model),
        // Auto)` — it needs the persona's SESSION MODEL to honor the
        // no-fallbacks `select()` guard (which refuses no-specifier
        // auto-discovery). The FilesystemPersonaResolver only reads
        // seed.json (persona_id + name); it has no session/model, and the
        // substrate "doesn't yet model per-persona adapter preferences"
        // (see FilesystemPersonaResolver::with_default_adapter). Threading
        // the persona's live model in is the real follow-up — NOT an Arc
        // refactor. Until then the chained variant is exercised by unit
        // tests with an explicit adapter; RAG-only is the honest default
        // for production callers (the recent deliveries already carry the
        // persona's own last generations, which is what an inspector reads).
        let rag_inspect_resolver = std::sync::Arc::new(
            crate::modules::persona_rag_inspect_filesystem::FilesystemPersonaResolver::new(
                crate::modules::persona_instance_manager::resolve_continuum_root(),
                daemon_socket_for_rag_inspect,
            ),
        );
        let rag_inspect_module = std::sync::Arc::new(
            crate::modules::persona_rag_inspect::PersonaRagInspectModule::new(rag_inspect_resolver),
        );
        runtime.register(rag_inspect_module);
        log_info!(
            "ipc",
            "server",
            "PersonaRagInspectModule registered — `persona/rag-inspect` available"
        );

        // The Grid's first heartbeat at server boot: resume any
        // existing citizens from disk + ensure at least one is
        // present. ResumeOrMintProvider scans
        // `<continuum_root>/personas/*/seed.json`; for each parsed
        // seed it yields a ResumedFromDisk intent (airc-lib will load
        // the existing keypair from identity.key when bootstrap runs
        // — same persona, same peer_id, across restarts). If no
        // citizens are on disk, it floor-mints one fresh per the
        // `min_personas = 1` policy below.
        //
        // Fired as an async task off the IPC bootstrap thread so the
        // server-ready signal isn't blocked on daemon round-trips.
        // Failure of any single bootstrap is non-fatal — log + move
        // on; the operator can re-fire via the
        // `persona/instances/bootstrap` command once the underlying
        // issue (disk full, daemon down, corrupted seed) is resolved.
        // ── Slice 13: substrate-managed persona hosting ──────────────
        //
        // Composes slices 7-12 into the production boot path:
        //
        //   PersonaSpawnerModule::plan_for_tier (slice 7)
        //     → bootstrap_planned (slice 8): mints/resumes airc identities
        //     → materialize_adapters (slice 9): builds inference adapters
        //     → spawn_persona_service (slice 12): runs the serve_persona_loop
        //     → PersonaAircRuntimeRegistry::attach_service_loop (slice 13 Q3):
        //       parks the JoinHandle in the slot alongside the runtime
        //
        // BEFORE slice 13: this boot loop called `bootstrap_one` per
        // persona and logged a welcome — the persona was reachable via
        // `airc peers` but never responded. Mute citizens.
        //
        // AFTER slice 13 (this code): each planned persona gets her
        // serve_persona_loop running on rt_handle, with the JoinHandle
        // owned by the registry slot for orderly shutdown.
        //
        // Per the design doc HEADLESS-PERSONA-HOST-LOOP.md (PR #1510):
        //   - P2 (in effect): plan_for_tier returns single Helper until
        //     slice 14 lands role-in-seed.json.
        //   - Q1 (applied): bootstrap_planned takes &Registry.
        //   - Q3 (applied): single registry keyspace owns runtime +
        //     service loop.
        //   - P1 (deferred): tokio::signal::ctrl_c wiring lands in a
        //     follow-up alongside the Runtime::shutdown caller.
        //     Slot-level shutdown_slot is available via the registry
        //     and exercised by the persona/instances/* IPC commands.
        //   - Q2 (deferred): detect_host_capability needs a production
        //     GpuMonitor constructor that doesn't exist yet (see TODO
        //     below). For now slice 13 uses CpuOnly + Compat, which
        //     produces the LCD Qwen2.5-0.5B Helper for all tiers. When
        //     the GpuMonitor construction lands, swap the hardcoded
        //     tier for `detect_host_capability(&gpu_monitor, &system_info)`.
        //   - P3 (deferred): ResourceBroker.acquire admission for each
        //     adapter spawn is its own slice. Current LCD case (1
        //     persona × ~500 MiB GGUF) is well within all supported
        //     tiers' headroom; broker admission becomes load-bearing
        //     when multi-persona returns in slice 14.
        // Slice 13.5: the persona spawn pipeline lives in
        // `PersonaSpawnSupervisor` now. IPC boot just constructs
        // it and calls `spawn_all` — every previously-inline
        // composition step (bootstrap_planned, materialize_adapters,
        // spawn_persona_service, attach_service_loop, orderly
        // drain, BootSummary) is encapsulated in the supervisor and
        // unit-testable in isolation.
        //
        // TODO #52: replace `CpuOnly + Compat` with
        // `detect_host_capability(&gpu_monitor, &system_info)` once
        // a production GpuMonitor constructor exists.
        // The serving daemon is the single hardware authority: it detects the
        // tier (drives n_gpu_layers — retires the hardcoded CpuOnly+Compat of
        // TODO #52) AND decides the model + lanes from an honest budget +
        // on-disk footprints with GPU-residency. The spawner obeys the plan —
        // no hardcoded tier/model/lanes. (Supersedes the #1645 tier-clamp fix.)
        // Glass-box: surface the artifact cache state at boot so the real provisioning
        // picture (what weights/avatars are present, how much disk) is visible without
        // a debugger ([[never-blind-feedback-driven-iteration]]). First live use of the
        // provisioning system on the running core. `eprintln!` (not the category logger)
        // so a boot summary is UNCONDITIONALLY visible in the server log, regardless of
        // which log categories the operator has enabled.
        eprintln!(
            "📦 artifact cache at boot: {}",
            crate::provisioning::Provisioner::default().cache_report()
        );
        let (hw_cap, tier_cat, tier_id) = serving_daemon.detected_tier();
        // Persona floor — how many citizens to host (read early: it is ALSO the
        // serving plan's lane DEMAND, so it must be set before the first
        // compute_plan; see below for the full doc).
        let persona_floor = crate::config_env::read("CONTINUUM_PERSONA_FLOOR")
            .and_then(|v| v.trim().parse::<usize>().ok())
            .filter(|&n| n >= 1)
            .unwrap_or(1);
        // Lanes serve DEMAND: the floor is how many minds need a concurrent
        // lane. Without this, the planner maximized lane count and split the KV
        // budget across slots nobody asked for — 2 personas served through 4
        // slots at a quarter-window each (the 2026-07-10 starvation).
        serving_daemon.set_lane_demand(persona_floor as u32);
        // The plan is the single grouped source of truth (model + lanes +
        // host-fit served window). Pass it by reference to the spawner per
        // [[pass-the-model-struct-no-param-hell]] — no destructured loose
        // params, no constant clamps re-derived downstream.
        let serving_plan = serving_daemon.compute_plan();
        match &serving_plan {
            Some(p) if p.fits_on_gpu => {
                tracing::info!(
                    base_model = %p.base_model_id,
                    lanes = p.lanes,
                    served_context_window = p.served_context_window,
                    resident = p.resident_models,
                    tier_id,
                    "serving daemon drives persona spawn (model + lanes + served window from ServingPlan)"
                );
            }
            other => {
                tracing::warn!(
                    plan = ?other,
                    "no GPU-fitting serving plan — spawner falls back to the tier default model, 1 lane, floor window"
                );
            }
        }
        // Persona floor — how many citizens to host. Single source: this one
        // config value drives BOTH the spawner's plan-slot count (via
        // with_population) AND the identity provider's mint floor below, so
        // plan-slots and minted identities stay 1:1. Default 1. Raise it to host
        // a collaborating POPULATION — the two-solver cooperation loop needs ≥2
        // distinct citizens in a room so their coordinated turns become training
        // signal (rooms → recorder → dataset → genome). Config-owned
        // ([[config-env-single-owner]]); once minted, citizens persist + resume
        // even if the floor is later lowered.
        let supervisor = crate::persona::host::PersonaSpawnSupervisor::new(
            crate::persona::spawner_module::PersonaSpawnerModule::new(hw_cap, tier_cat)
                .with_serving(serving_plan.as_ref())
                .with_population(persona_floor),
            instance_manager.clone(),
            // Persona reasoning binds to whatever the serving daemon has live,
            // read off its published ServingSnapshot (not a probe of our own).
            // The OpenAI-compatible `/v1` transport gives native function-calling
            // (the persona's HANDS actually fire) for free, vs the in-process
            // llama.cpp adapter which silently dropped tools. Joel 2026-06-21.
            std::sync::Arc::new(crate::persona::supervisor::ServedModelPersonaAdapterFactory),
            tier_id,
            crate::model_registry::global(),
            rt_handle.clone(),
        );
        let continuum_root_for_boot =
            crate::modules::persona_instance_manager::resolve_continuum_root();
        // Round-2 verifier finding on PR #1568: `spawn_all` calls
        // `bootstrap_one` which (since task #222) lazily fetches the
        // process-global CommandExecutor via `executor()`. If the
        // spawned task reached that call BEFORE the IPC thread runs
        // `init_executor_with_interceptors` below, production would
        // panic. The earlier shape only "worked" because
        // `ResumeOrMintProvider::new` does disk I/O that's slower
        // than the IPC thread reaching init_executor — a race, not
        // an ordering guarantee.
        //
        // Structural fix: gate the spawned task on a oneshot that
        // the IPC thread sends AFTER `init_executor_with_interceptors`.
        // The spawn task literally cannot reach `executor()` before
        // the global is initialized — the ordering is enforced by
        // the channel, not by relative I/O latency.
        let (executor_ready_tx, executor_ready_rx) =
            tokio::sync::oneshot::channel::<Arc<crate::runtime::CommandExecutor>>();
        persona_supervisor_executor_ready_tx = Some(executor_ready_tx);
        // The serving daemon's published plan is the readiness signal the
        // persona host reacts to (event-driven spawn — see the reconcile loop
        // below). Subscribe BEFORE the spawn so no plan edge is missed while
        // the task waits on the executor-ready oneshot.
        let mut serving_plan_rx = serving_daemon.subscribe();
        rt_handle.spawn(async move {
            // Wait for the IPC thread to deliver the WIRED executor (this both
            // gates ordering AND hands us the executor the personas' hands ride).
            // If the sender is dropped without sending (substrate shutdown
            // mid-boot), exit cleanly without bootstrapping personas against an
            // uninitialized executor.
            let tool_executor = match executor_ready_rx.await {
                Ok(ex) => ex,
                Err(_) => {
                    tracing::warn!(
                        "persona supervisor task aborting: executor-ready signal dropped \
                         before fire (substrate shutdown mid-boot? init_executor never \
                         reached?). No personas bootstrapped this run."
                    );
                    return;
                }
            };
            use crate::persona::resume_or_mint_provider::ResumeOrMintProvider;
            // `persona_floor` (read once above) floors the provider's mint count
            // to the SAME value that sized the spawner's plan via with_population
            // — so plan-slots and minted identities stay 1:1.
            let mut provider =
                match ResumeOrMintProvider::new(&continuum_root_for_boot, persona_floor).await {
                    Ok(p) => p,
                    Err(e) => {
                        tracing::warn!(
                            error = %e,
                            "ResumeOrMintProvider construction failed — server up, no \
                             citizens online. Resolve continuum_root permissions + \
                             restart, or fire `persona/instances/bootstrap` manually."
                        );
                        return;
                    }
                };
            // Event-driven spawn: the persona host reacts to the serving
            // daemon's published plan (its watch channel) instead of probing
            // the gateway once at boot. The serving daemon ticks every 5s; the
            // unsloth gateway may still be loading the model when boot first
            // reaches here, so the adapter factory's live `/v1/models` probe
            // can fail on the first attempt — the "0 citizens hosted" race.
            //
            // Rather than leave the citizen permanently mute (the bug: a
            // one-shot probe that lost a race), reconcile on each plan edge:
            // retry `spawn_all` until the persona materializes. When the
            // gateway finishes warming, the next tick's plan edge drives a
            // successful spawn — no restart, self-healing like the rest of the
            // substrate. (Single-persona reality today — P2. Idempotent
            // multi-slot reconcile + respawn-on-death is the slice-14
            // follow-on; we break once a citizen is hosted to avoid
            // re-bootstrapping a live persona's airc identity.)
            let mut attempt = 0u32;
            loop {
                let plan_ready = serving_plan_rx
                    .borrow()
                    .as_ref()
                    .map(|p| p.fits_on_gpu)
                    .unwrap_or(false);
                if plan_ready {
                    // `fits_on_gpu` is a RESOURCE decision (the model fits VRAM) — it does
                    // NOT prove the lane can DECODE. A lane can fit yet fail EVERY
                    // generation with `500 "Compute error."` while `/health` still answers
                    // 200 — e.g. spawned with a flag that forces embedding/non-causal mode
                    // (the live-2026-07-03 `--embeddings` outage), a bad LoRA, or a wedged
                    // Metal context. Hosting personas onto such a lane makes every turn
                    // 500 SILENTLY. So gate the spawn on the DECODE-VERIFIED serving
                    // snapshot: `ServingSnapshot.ready` is published only on a serve/adopt
                    // outcome that passed the 1-token decode smoke-probe. `await_ready_serving`
                    // parks on the serving snapshot until the lane proves it can think, or
                    // the deadline lapses. On timeout we do NOT spawn (a can't-decode lane
                    // is a loud, novel failure — never a silent per-turn 500); we park and
                    // retry on the next serving edge, self-healing once the lane recovers.
                    if crate::inference::llama_server::await_ready_serving(
                        std::time::Duration::from_secs(120),
                    )
                    .await
                    .is_none()
                    {
                        tracing::warn!(
                            "serving plan fits on GPU but the lane is NOT decode-ready \
                             within 120s — /health may be 200 while every generation 500s \
                             (a broken spawn flag / wedged compute context). NOT hosting \
                             personas onto a lane that cannot generate; retrying on the \
                             next serving edge."
                        );
                    } else {
                        attempt += 1;
                        let summary = supervisor
                            .spawn_all(&mut provider, Some(tool_executor.clone()))
                            .await;
                        if summary.hosted > 0 {
                            tracing::info!(
                                hosted = summary.hosted,
                                failed = summary.failed(),
                                attempts = attempt,
                                "🌐 Substrate boot composition complete (slice 13.5) — \
                                 citizen(s) hosted, event-driven on serving-plan readiness"
                            );
                            break;
                        }
                        tracing::warn!(
                            failed = summary.failed(),
                            attempt,
                            "persona spawn found a decode-ready serving lane but no citizen \
                             materialized — will retry on the next serving-plan edge"
                        );
                    }
                }
                // Park until the serving daemon republishes (every tick, or
                // sooner on a pressure edge). `changed()` errs only if the
                // daemon is gone — then there is nothing left to react to.
                if serving_plan_rx.changed().await.is_err() {
                    tracing::warn!(
                        "serving-daemon watch closed before any persona materialized \
                         — no citizens online this run"
                    );
                    break;
                }
            }
        });
    } else {
        // A.2: by this point the mode-driven gate above has already
        // returned `Err` if persona hosting was required and discovery
        // failed. Reaching here means `--mode=inference-only` — the
        // operator explicitly opted into degraded operation. Single
        // info-level line tells them what's missing without scaring.
        tracing::info!(
            mode = boot_mode.label(),
            discovery_kind = discovery.kind(),
            "PersonaInstanceManagerModule not registered (--mode=inference-only, AIRC \
             discovery {}). Substrate continues for inference / embedding / forge / \
             cargo / code. Resolve AIRC and re-launch in --mode=full-citizen to host \
             personas.",
            discovery.kind()
        );
    }

    // ── Served-model re-home reconciler ─────────────────────────────────────
    // The base-model sibling of LoRA/genome paging: when the serving daemon
    // publishes a NEW active model — a `serving/pin`, a re-plan under memory
    // pressure, or a grid failover — every ALREADY-hosted persona's deliberation
    // must rebind to it (new shared adapter + new served context window) WITHOUT
    // losing the genome, working memory, or admission it accumulated. This task
    // owns the serving-SNAPSHOT watch and drives `re_home_all` on an ACTUAL model
    // change — the seam that turns `serving/pin <model>` into a no-reboot,
    // coherent model-sweep lever AND delivers portable-self live re-home.
    //
    // Distinct from the boot spawn loop above (which reacts to the serving PLAN to
    // FIRST-host personas): this reacts to the serving SNAPSHOT to re-home
    // ALREADY-hosted personas. Both are the canonical concurrent-concern shape —
    // own task, `watch` receiver, no lock across await, log-and-continue, exit on
    // watch close. The swap itself is wait-free (`ArcSwap` store under the cycles
    // lock); the only cost per model edge is ONE shared adapter HTTP-init, reused
    // by every persona lane. See [[seamless-persona-failover-model-and-genome]].
    {
        let mut serving_rx = serving_daemon.subscribe_serving();
        rt_handle.spawn(async move {
            // The model live personas are currently bound to. `None` until the
            // first ready snapshot: boot binds personas via the upstart factory,
            // so the FIRST ready model is NOT a re-home (they spawn already bound
            // to it). We adopt it as the baseline without re-homing, and re-home
            // only on a SUBSEQUENT change — avoiding a wasteful boot-time adapter
            // rebuild + redundant swap.
            let mut bound: Option<String> = None;
            loop {
                // Park until the daemon republishes its serving snapshot.
                if serving_rx.changed().await.is_err() {
                    tracing::info!(
                        "serving-snapshot watch closed — served-model re-home reconciler \
                         exiting (substrate shutdown)"
                    );
                    break;
                }
                let snap = serving_rx.borrow_and_update().clone();
                if !snap.ready {
                    continue;
                }
                let Some(active) = snap.active_model.clone() else {
                    continue;
                };
                if bound.as_deref() == Some(active.as_str()) {
                    continue; // same model already bound — nothing to re-home.
                }
                if bound.is_none() {
                    // The boot upstart already bound (or will bind) personas to
                    // this first served model; adopt it as the baseline.
                    bound = Some(active);
                    continue;
                }
                // Build the ONE shared served-model adapter for this edge (HTTP
                // init once, shared by every persona lane). Fail LOUD into a log —
                // a failed re-home leaves personas on their prior (still-live)
                // binding rather than a silent wrong-brain, and retries on the
                // next snapshot edge ([[fallbacks-are-illegal-fail-loud]]).
                let adapter = match crate::persona::supervisor::build_served_adapter(&snap).await {
                    Ok(a) => a,
                    Err(e) => {
                        tracing::warn!(
                            model = %active,
                            error = %e,
                            "served-model re-home: adapter build failed — personas stay \
                             on their prior binding; will retry on the next snapshot edge"
                        );
                        continue;
                    }
                };
                let n = crate::cognition::persona_workspace::global().re_home_all(
                    adapter,
                    Some(active.clone()),
                    snap.served_context_window,
                );
                crate::probe!(
                    class = "persona.rehome",
                    model = %active,
                    context_window = snap.served_context_window,
                    personas = n,
                    "served model changed — re-homed live personas onto the new binding"
                );
                bound = Some(active);
            }
        });
    }

    // AIProviderModule: Unified AI provider for cloud and local inference
    // Provides ai/generate, ai/providers/list, ai/providers/health
    // Routes to DeepSeek, Anthropic, OpenAI, Together, Groq, Fireworks, XAI, Google, Mistral
    runtime.register(Arc::new(AIProviderModule::with_gpu_manager(
        gpu_manager.clone(),
    )));

    // GenomeModule: substrate-side dispatch for `genome/*` commands.
    // genome/job-create, genome/job-status, genome/job-cancel route
    // through the FineTuningCoordinator + a registry seeded with
    // whichever cloud LoRA-trainer adapters have credentials, plus
    // the LocalCandleFineTuner skeleton (always registered — slot
    // is visible to the coordinator even before tasks #231-#233
    // implement the optimizer loop). Per
    // [[commands-are-dumb-daemons-are-smart]] the module is narrow;
    // selection logic lives in the coordinator.
    {
        use crate::genome::fine_tuning::{
            FineTuningRegistry, LocalCandleFineTuner, MlxLoraFineTuner, OpenAIFineTuningAdapter,
        };
        let ft_registry = std::sync::Arc::new(FineTuningRegistry::new());

        // OpenAI when credentials present. Other cloud LoRA-trainer
        // adapters (Mistral, Anthropic, Fireworks, DeepSeek,
        // Together) plug in here as their impls land — same pattern,
        // gate on the matching `*_API_KEY` secret.
        if crate::secrets::get_secret("OPENAI_API_KEY").is_some() {
            ft_registry.register(std::sync::Arc::new(OpenAIFineTuningAdapter::new()));
            log_info!(
                "ipc",
                "server",
                "GenomeModule: registered OpenAIFineTuningAdapter"
            );
        }

        // LocalCandleFineTuner always registered. The skeleton
        // returns LocalTrainerFailed with a pointer to follow-up
        // tasks until the math lands; making the slot present means
        // the coordinator + operator + telemetry can see the
        // architectural seam.
        ft_registry.register(std::sync::Arc::new(LocalCandleFineTuner::new()));
        log_info!(
            "ipc",
            "server",
            "GenomeModule: registered LocalCandleFineTuner (skeleton — tasks #231-#233 \
             track the optimizer-loop landing)"
        );

        // MlxLoraFineTuner — the REAL owned trainer (#32): Apple's
        // mlx_lm.lora on the Metal GPU → forge-custodian converts to a
        // GGUF-lora gene → llama-server serves it. Always registered; its
        // capability declares `requires: TrainerHardware::Metal`, so the
        // coordinator routes here only on a Metal host, and create_job
        // fails loud (never silently) when Metal / the mlx python env /
        // a base model / examples are missing. Without this slot the live
        // registry held only the synthetic Candle skeleton + conditional
        // cloud OpenAI, so genome/job-create could never produce a
        // real, loadable LoRA — the closed L1→L3 loop was broken here.
        ft_registry.register(std::sync::Arc::new(MlxLoraFineTuner::new()));
        log_info!(
            "ipc",
            "server",
            "GenomeModule: registered MlxLoraFineTuner (mlx-local — real Apple-Silicon \
             LoRA trainer; coordinator gates on a probed Metal device)"
        );

        // L3 completion sentinel shares the SAME registry — it polls the handles the
        // trigger registers and looks the owning adapter back up here. Clone the Arc
        // before the registry is moved into GenomeModule.
        let completion_sentinel = Arc::new(
            crate::modules::training_completion_sentinel::TrainingCompletionSentinel::new(
                ft_registry.clone(),
            ),
        );

        runtime.register(Arc::new(crate::modules::genome::GenomeModule::new(
            ft_registry,
        )));

        // TrainingCompletionSentinel: L3 of the dev-task continuous-learning loop.
        // Polls in-flight training jobs (the TrainingJobBoard the trigger writes to);
        // on completion runs `cognition/eval` and pages the gene into the live
        // persona ONLY on lift>0 — the keystone that makes the single-machine loop
        // automatic (`docs/genome/DEV-TASK-LOOP-CLOSURE-PLAN.md` L3). Its executor is
        // installed below by `install_executor_on_all`.
        runtime.register(completion_sentinel);
        log_info!(
            "ipc",
            "server",
            "TrainingCompletionSentinel: registered (L3 train-done → eval → lift>0 → page-in)"
        );

        // TrainingTriggerModule: substrate-native batching coordinator
        // sitting between curriculum producers (teacher persona's
        // synthesis, hippocampus's noteworthy drain, operator submits)
        // and `genome/job-create`. Accumulates per-(persona, trait)
        // and auto-dispatches when the threshold is reached. Per
        // `[[no-fallbacks-ever]]`, dispatch failure preserves bucket
        // contents — curated examples never silently disappear.
        runtime.register(Arc::new(
            crate::modules::training_trigger::TrainingTriggerModule::new(),
        ));
        log_info!(
            "ipc",
            "server",
            "TrainingTriggerModule: registered (genome/training-trigger/*)"
        );
    }

    // SentinelModule: Concurrent, fault-tolerant build/task execution
    // Provides sentinel/execute, sentinel/status, sentinel/cancel, sentinel/list
    // And sentinel/logs/list, sentinel/logs/read, sentinel/logs/tail
    // Process isolation via child processes - safe for Xcode, cargo, etc.
    let sentinel_module = Arc::new(SentinelModule::new());
    runtime.register(sentinel_module.clone());
    crate::modules::sentinel::register_for_shutdown(sentinel_module);

    // ToolParsingModule: Stateless tool call parsing + correction
    // Provides tool-parsing/parse, tool-parsing/correct, tool-parsing/register-tools,
    // tool-parsing/decode-name, tool-parsing/encode-name
    // Replaces 784 lines of TypeScript ToolFormatAdapter hierarchy
    runtime.register(Arc::new(ToolParsingModule::new()));

    // PlasticityModule: Adaptive neural plasticity optimization engine
    // Provides plasticity/analyze, plasticity/compact, plasticity/topology
    // Per-head utilization-aware pruning, mixed-precision quantization, GQA-aware
    runtime.register(Arc::new(crate::modules::plasticity::PlasticityModule::new()));

    // AvatarModule: Bevy 3D avatar snapshots for profile pictures
    // Provides avatar/snapshot — allocates render slot, captures frame, saves PNG
    runtime.register(Arc::new(AvatarModule::new()));

    // DatasetModule: Training dataset import and management
    // Provides dataset/import-csv, dataset/import-realclasseval, dataset/list, dataset/info
    runtime.register(Arc::new(DatasetModule::new()));

    // VisionModule: Content-addressed cache + event notification for vision descriptions
    // Provides vision/description-get, vision/description-put, vision/description-status,
    // vision/cache-stats, vision/cache-warm, vision/cache-evict
    runtime.register(Arc::new(VisionModule::new()));

    // GridModule: inter-node transport + routing (Tailscale, Reticulum)
    let grid_dir = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".continuum")
        .join("grid");
    let local_has_gpu = gpu_manager.total_vram_bytes() > 0;
    let local_vram_mb = gpu_manager.total_vram_bytes() / (1024 * 1024);
    // Keep a handle on the GridModule's state so we can build the
    // GridInterceptor below. The interceptor needs the same router +
    // node registry + transports the GridModule itself runs on; using
    // the public `state()` getter avoids duplicating any of that.
    let grid_module = Arc::new(GridModule::new(grid_dir, local_has_gpu, local_vram_mb));
    let grid_state = grid_module.state();
    runtime.register(grid_module);

    // Initialize modules (runs async init in sync context)
    rt_handle.block_on(async {
        if let Err(e) = runtime.initialize().await {
            log_error!("ipc", "server", "Runtime initialization failed: {}", e);
        }
    });

    // Start periodic tick loops for modules that declare a tick_interval.
    // Replaces TypeScript's per-persona setIntervals (task polling, self-task gen, training checks).
    // Tick loops run as tokio tasks — they're lightweight and don't block the IPC thread.
    let _tick_handles = rt_handle.block_on(async { runtime.start_tick_loops() });

    // Verify the required-module set for THIS (discovery, mode) pair.
    // A.2 makes the set conditional — `persona_instance_manager` and
    // `persona-rag-inspect` are only required when AIRC is `Healthy`
    // AND mode requires persona hosting. Slice A's flat list put
    // those in the unconditional set, which broke `--mode=inference-only`
    // (R1#1 BLOCK). Conditional dispatch eliminates the contradiction
    // structurally.
    if let Err(e) = runtime.verify_registration(&discovery, boot_mode) {
        log_error!("ipc", "server", "{}", e);
        return Err(std::io::Error::other(e));
    }

    log_info!(
        "ipc",
        "server",
        "Modular runtime ready with {} modules: {:?}",
        runtime.registry().list_modules().len(),
        runtime.registry().list_modules()
    );

    // Build the substrate-wide `CommandExecutor` and install it on every
    // registered module via the typed `ServiceModule::install_executor`
    // path (task #224 — replaces the deleted `GLOBAL_EXECUTOR` +
    // `executor()` panic accessor).
    //
    // Interceptor chain order (per MODULE-ARCHITECTURE.md §5): airc
    // sits at the head so explicit aircPeer/aircRoom targeting beats
    // grid's capability-based remote routing. grid sits next so
    // routingHint / nodeId / capability-based commands hop to a peer
    // before the kernel tries local Rust dispatch. Both interceptors
    // decline cleanly when their routing decision is "local," so
    // existing commands see zero behavior change.
    //
    // Hoisted so BOTH the ProvidedCommandInterceptor (reader) and the connection
    // layer (writer, via ServerState) share the ONE registry: an eye-node's
    // `provider/register` on any connection binds here, and this interceptor
    // routes perception/observe + interface/screenshot to it. Empty until an
    // eye-node connects ⇒ those commands fail loud, honestly.
    let provider_registry = Arc::new(crate::runtime::ProviderRegistry::new());
    let executor = Arc::new(
        crate::runtime::CommandExecutor::new(runtime.registry_arc())
            // Share the ONE runtime bus (the same Arc every ModuleContext
            // gets — `chat:posted` from the airc daemon-attach projector
            // and `presence:updated` from the node presence emitter both
            // land here). Without this the WS positron projection has no
            // event source and `message_bus()` is None — the boot-loud
            // panic the `CONTINUUM_CORE_WS` block asserts against.
            .with_message_bus(runtime.bus_arc())
            .with_interceptor(Arc::new(crate::runtime::AircInterceptor::new()))
            .with_interceptor(Arc::new(crate::runtime::GridInterceptor::new(grid_state)))
            // `provided` sits at the TAIL of the chain: airc/grid get first look
            // so an explicitly remote-targeted perception/observe still hops to a
            // peer's eye, but an ordinary (untargeted) Provided call — a persona
            // asking to SEE — routes here to a connected eye-node adapter, or
            // fails loud naming the missing eye-node. Empty registry today (the
            // eye-node client rides task #29): perception/observe + interface/
            // screenshot fail loud honestly instead of "no Rust module handles".
            // The connection layer will register providers via the interceptor's
            // shared `ProviderRegistry` when an eye-node connects.
            .with_interceptor(Arc::new(crate::runtime::ProvidedCommandInterceptor::new(
                Arc::clone(&provider_registry),
            )))
            // Hard ACL gate: cross-grid (airc) + TCP callers — incl. a persona's
            // command inbound pump — are gated by the grid ACL, capped at
            // Provisional. A remote room peer may request ai/generate and nothing
            // privileged; local/substrate callers are unaffected.
            //
            // NOTE: the per-peer trust BRIDGE (GridTrustAuthPolicy::with_trust_source)
            // is NOT wired here yet — the grid NodeRegistry is keyed by transport
            // ADDRESS (Tailscale IP / Reticulum hash), not by the airc peer_id the
            // CallerIdentity carries, so it can't resolve an airc caller's trust
            // (it would silently no-op). Wiring awaits a real airc-peer → trust
            // source (the airc↔grid identity unification, task #38). The flat
            // ceiling (`new()`) is correct and honest until then.
            .with_policy(Arc::new(crate::routing::GridTrustAuthPolicy::new())),
    );
    runtime
        .registry()
        .install_executor_on_all(Arc::clone(&executor));

    // L2 continuous-learning producer: hand the same wired executor to the
    // turn-completion training producer so a live persona reply can be scored,
    // classified, and submitted to `genome/training-trigger` AS the persona
    // (LocalPersona → Trusted). Late-bound because the service loop has no
    // executor in scope; this is the one install site.
    crate::persona::training_producer::install_executor(Arc::clone(&executor));

    // Round-2 verifier fix on PR #1568: now that the executor is
    // installed on every module, release the persona-supervisor task
    // (if wired). The spawned task at line ~1239 has been awaiting
    // this signal; it can now safely dispatch through `bootstrap_one`,
    // whose PIM has just been populated with the executor. Send-failure
    // means the receiver was dropped — substrate shutdown mid-boot —
    // and the supervisor already exited; ignoring is correct.
    if let Some(tx) = persona_supervisor_executor_ready_tx.take() {
        // Deliver the WIRED executor (GridTrustAuthPolicy + interceptors) — the
        // personas' hands ride exactly this, so the ACL gates what they touch.
        let _ = tx.send(Arc::clone(&executor));
    }

    let listener = UnixListener::bind(socket_path)?;
    // Make the socket world-rw so callers running under a different UID
    // than the server can connect. Concrete failure (#1008): on Windows
    // WSL2 + Docker Desktop, continuum-core runs as root inside the
    // container and binds the socket; the host-side jtag (running as
    // the WSL user, uid 1000) gets EACCES connecting to the root-owned
    // socket. Mac/Linux dev mode (server + caller both run as the same
    // user) is unaffected. 0o666 is appropriate for an IPC substrate
    // socket that lives in a path the caller can already see — same
    // blast radius as anything reading /tmp. Failing-loud (no `?` here
    // would suppress the error; let it propagate) is intentional per
    // the global "evidence is for the debugger" rule. Caught live by
    // continuum-b69f 2026-05-02 during Carl-OOTB Windows Phase 4.
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(socket_path, std::fs::Permissions::from_mode(0o666))?;
    }

    // Socket is bound + world-rw chmod'd. Fire the ready watch so any
    // consumer (main.rs awaiting boot completion, future subsystems
    // wanting to know when IPC is up) observes the false→true edge.
    // The watch supersedes the previous oneshot parameter — any number
    // of late or eager subscribers can attach via `subscribe_ready()`.
    let _ = ipc_ready_tx().send(true);
    crate::probe!(
        class = "ready.observed",
        module = "ipc-server",
        socket = socket_path
    );

    let state = Arc::new(ServerState::new_with_shared_state(
        rt_handle,
        memory_manager,
        runtime,
        channel_registries,
        rag_engine,
        voice_service,
        audio_pool,
        file_engines,
        shell_sessions,
        gpu_manager,
        provider_registry,
    ));

    log_info!("ipc", "server", "IPC server ready");

    // Optional TCP listener — exposes the same IPC protocol over TCP for
    // callers that can't reach a Unix socket (containerized node-server on
    // Mac, where the Unix socket lives on the host outside the Docker VM
    // boundary). Set CONTINUUM_CORE_TCP=<port> (typically 9100) to enable.
    //
    // Bind address: CONTINUUM_CORE_BIND env, default 127.0.0.1 (safe —
    // loopback only). Mac Option B install.sh sets 0.0.0.0 explicitly
    // because Docker Desktop's `host.docker.internal` resolves to the
    // host's docker-bridge IP (~192.168.65.254), NOT to 127.0.0.1 — a
    // loopback-bound listener is unreachable from containers. Binding
    // 0.0.0.0 accepts on the docker bridge; Mac's application firewall
    // blocks LAN inbound for unsigned dev binaries by default, so the
    // exposure stays local in practice. Explicit env-driven choice beats
    // hidden platform detection.
    //
    // Unix socket remains the primary path — same binary, same server state,
    // same handle_client code via the IpcStream trait. TCP is additive.
    //
    // SECURITY (adversarial review 2026-06-21): TCP callers are stamped
    // CallerSource::Tcp → Provisional ceiling, so Owner-gated commands
    // (data/delete, grid/trust, …) are REFUSED — no unauthenticated Owner
    // execution. BUT the Provisional AiSafe surface IS reachable over this socket
    // UNauthenticated: arbitrary `data/list`/`data/query` reads, `chat/send`
    // writes, and `ai/generate` (the intended container use). That is safe on the
    // default loopback bind; binding 0.0.0.0 exposes that surface to anyone on the
    // bridge/LAN. TODO(authenticated-tcp): require a shared secret / signed
    // handshake for the TCP listener (and/or a sub-Provisional read-only ceiling)
    // before relying on a non-loopback bind — pairs with the airc↔grid per-peer
    // trust bridge. Until then: do NOT bind 0.0.0.0 on an untrusted network.
    if let Ok(tcp_port_str) = std::env::var("CONTINUUM_CORE_TCP") {
        if let Ok(port) = tcp_port_str.parse::<u16>() {
            if port > 0 {
                let bind_host = std::env::var("CONTINUUM_CORE_BIND")
                    .unwrap_or_else(|_| "127.0.0.1".to_string());
                let bind_addr = format!("{}:{}", bind_host, port);
                match TcpListener::bind(&bind_addr) {
                    Ok(tcp_listener) => {
                        log_info!(
                            "ipc",
                            "server",
                            "TCP listener ready on {} (for container callers via host.docker.internal)",
                            bind_addr
                        );
                        let tcp_state = state.clone();
                        std::thread::spawn(move || {
                            for stream in tcp_listener.incoming() {
                                match stream {
                                    Ok(stream) => {
                                        let state = tcp_state.clone();
                                        std::thread::spawn(move || {
                                            // TCP = unauthenticated remote socket →
                                            // stamp a non-owner identity so the
                                            // dispatch boundary ACL-gates it (no Owner
                                            // commands over TCP). peer_id is nil (no
                                            // verified peer); trust comes from the Tcp
                                            // source, not the id.
                                            let caller = Some(crate::routing::CallerIdentity::tcp(
                                                crate::identity::PeerId::from_uuid(
                                                    uuid::Uuid::nil(),
                                                ),
                                            ));
                                            if let Err(e) = handle_client(stream, state, caller) {
                                                log_error!(
                                                    "ipc",
                                                    "server",
                                                    "TCP client error: {}",
                                                    e
                                                );
                                            }
                                        });
                                    }
                                    Err(e) => {
                                        log_error!("ipc", "server", "TCP accept error: {}", e);
                                    }
                                }
                            }
                        });
                    }
                    Err(e) => {
                        log_error!(
                            "ipc",
                            "server",
                            "TCP listener failed to bind {}: {}",
                            bind_addr,
                            e
                        );
                    }
                }
            }
        }
    }

    // WebSocket listener for the thin-client fleet (task #29). ON BY DEFAULT on
    // localhost:`DEFAULT_WS_PORT` so web + terminal clients connect out-of-the-box
    // with zero setup — override the port with `CONTINUUM_CORE_WS`, change the
    // bind host with `CONTINUUM_CORE_BIND` (default 127.0.0.1), or set
    // `CONTINUUM_CORE_WS=0` to disable the ingress entirely. Browsers can't speak
    // the length-prefixed IPC frame format, so thin clients (`sdk/typescript`
    // WebSocketTransport) speak WebSocket + the multiplexed
    // WsClientMessage/WsServerMessage envelope. Every frame dispatches through
    // the SAME `CommandRequestHandler::execute_command_request` owner the airc
    // peer path uses, stamped `CallerSource::Ws` → Provisional ceiling (see
    // ipc::ws module docs + the TCP SECURITY note above; same unauthenticated
    // AiSafe surface, 127.0.0.1 by default — do NOT bind 0.0.0.0 on an untrusted
    // network).
    const DEFAULT_WS_PORT: u16 = 8974;
    let ws_port_str =
        std::env::var("CONTINUUM_CORE_WS").unwrap_or_else(|_| DEFAULT_WS_PORT.to_string());
    {
        if let Ok(port) = ws_port_str.parse::<u16>() {
            if port > 0 {
                let bind_host = std::env::var("CONTINUUM_CORE_BIND")
                    .unwrap_or_else(|_| "127.0.0.1".to_string());
                let bind_addr = format!("{bind_host}:{port}");
                let ws_executor = Arc::clone(&executor);
                // The positron state substrate for the thin-client fleet:
                // one shared snapshot+broadcast cell that WS sessions
                // subscribe against. The airc source wiring (task #29)
                // holds this same handle and calls `Substrate::store` on
                // each airc chat/roster change, so the projection tracks
                // the airc-owned truth (see `ipc::positron_source`).
                // Constructed here (not in `serve`) so the projection
                // subscriber shares the instance the server serves.
                let ws_substrate = continuum_positron::Substrate::new();

                // airc source wiring: subscribe the chat projection to
                // the live event bus with a clone of the served
                // substrate. It folds `chat:posted` + `presence:updated`
                // into the `chat` `ChatViewState` and stores each
                // transition, which streams down to subscribed WS
                // sessions as a `State` frame. Requires a wired bus —
                // fail loud (not a silent skip) if the executor has none,
                // because a WS server with no state source is a boot bug,
                // not a runtime condition ([[fallbacks-are-illegal-fail-loud]]).
                let projection_bus = ws_executor.message_bus().expect(
                    "CONTINUUM_CORE_WS is set but the command executor has no message bus — \
                     the positron chat projection has no airc source to subscribe to",
                );
                positron_source::spawn(
                    &state.rt_handle,
                    projection_bus.clone(),
                    ws_substrate.clone(),
                );

                // Producer half of the same stream: attach a node-level
                // roster reader and emit `presence:updated` so the consumer
                // above has an identity source to fold in — otherwise every
                // rendered message keeps its provisional peer-id label
                // forever. Gated on the SAME (socket, room) precondition as
                // persona hosting; if airc discovery gave us neither, the
                // roster has no source and we log the skip (not a silent
                // fallback — an honestly-disabled projection).
                match node_presence_deps
                    .clone()
                    .zip(persona_bootstrap_room_name.clone())
                {
                    Some(((daemon_socket, room_id), room_name)) => {
                        let continuum_root =
                            crate::modules::persona_instance_manager::resolve_continuum_root();
                        let node_home = continuum_root
                            .join("citizens")
                            .join("node")
                            .join("presence")
                            .join("airc");
                        positron_presence::spawn_node_presence_emitter(
                            &state.rt_handle,
                            daemon_socket.clone(),
                            node_home,
                            room_id.as_uuid(),
                            room_name.clone(),
                            projection_bus.clone(),
                        );

                        // Roster-vitals radiator (design B emit half): sample each
                        // resident persona's live WorkspaceCycle (service-tick tempo
                        // + paged-in genome) and publish `persona:vitals` on the SAME
                        // bus the chat projection reads, so those readouts breathe as
                        // meters on the who-panel card. Reads the workspace registry
                        // itself (where residents run); the projection folds by id.
                        vitals_emitter::spawn_vitals_emitter(
                            &state.rt_handle,
                            projection_bus.clone(),
                        );

                        // Wall projector: the consuming half of `wall:changed`.
                        // A dedicated node reader (own home + identity, distinct
                        // from the presence lurker) re-reads the airc-owned
                        // supersede-projected board on each change and stores it
                        // as `kind="wall"`, so a chat window shows its pinned
                        // board with zero resident personas. Same (socket, room)
                        // precondition as presence — an empty roster would leave
                        // wall authors provisionally labelled, but the board
                        // itself still renders.
                        let wall_home = continuum_root
                            .join("citizens")
                            .join("node")
                            .join("wall")
                            .join("airc");
                        positron_wall_source::spawn_node_wall_projector(
                            &state.rt_handle,
                            daemon_socket.clone(),
                            wall_home,
                            room_id.as_uuid(),
                            room_name.clone(),
                            ws_substrate.clone(),
                            projection_bus.clone(),
                        );

                        // Kanban projector: the consuming half of
                        // `kanban:changed`. Its own node reader (distinct home
                        // + identity from presence and wall) re-reads the
                        // airc-work-owned board fold (`work_board_complete`) on
                        // each change and stores it as `kind="kanban"`, so a
                        // chat window shows the room's work board with zero
                        // resident personas. Same (socket, room) precondition
                        // as presence/wall — an empty roster leaves card
                        // authors provisionally labelled, but the board still
                        // renders. Wired here TOGETHER with the inbound
                        // classification so the projector goes live fed.
                        let kanban_home = continuum_root
                            .join("citizens")
                            .join("node")
                            .join("kanban")
                            .join("airc");
                        positron_kanban_source::spawn_node_kanban_projector(
                            &state.rt_handle,
                            daemon_socket,
                            kanban_home,
                            room_id.as_uuid(),
                            room_name,
                            ws_substrate.clone(),
                            projection_bus,
                        );
                    }
                    None => {
                        tracing::warn!(
                            "positron presence emitter not started — airc discovery produced no \
                             (daemon socket, default room, room name); the WS roster will stay \
                             empty until airc is Healthy"
                        );
                    }
                }

                // Per-citizen substrates for per-user views (nav): each connecting
                // citizen (?me) reads its own nav from here, unioned with the node
                // substrate for per-room views. Shared instance so the nav projector
                // (write) and the session (read) agree on for_citizen(me).
                let per_user =
                    std::sync::Arc::new(continuum_positron::scoping::PerUserSubstrates::new());
                state.rt_handle.spawn(async move {
                    ws::serve(bind_addr, ws_executor, ws_substrate, per_user).await;
                });
            }
        }
    }

    // Periodic memory leak reporter — logs RSS + top leakers every 10s
    // Also acts as OOM guard: exits gracefully before OOM kills us ungracefully.
    // Limit is 80% of system RAM (not a fixed 4GB) — scales from an 8GB MacBook
    // Air to a 192GB workstation without false kills. The old 4GB limit was killing
    // the process on 48GB machines where 5.6GB RSS is perfectly normal (whisper
    // 1.6GB + ORT embedding runtime 1.8GB one-time alloc + working set).
    let mem_rt = state.rt_handle.clone();
    mem_rt.spawn(async {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
        let system_ram_mb = {
            #[cfg(target_os = "macos")]
            {
                use std::process::Command;
                Command::new("sysctl")
                    .args(["-n", "hw.memsize"])
                    .output()
                    .ok()
                    .and_then(|o| String::from_utf8(o.stdout).ok())
                    .and_then(|s| s.trim().parse::<u64>().ok())
                    .map(|bytes| bytes / (1024 * 1024))
                    .unwrap_or(8192) // fallback 8GB
            }
            #[cfg(target_os = "linux")]
            {
                std::fs::read_to_string("/proc/meminfo")
                    .ok()
                    .and_then(|s| {
                        s.lines()
                            .find(|l| l.starts_with("MemTotal:"))
                            .and_then(|l| l.split_whitespace().nth(1))
                            .and_then(|kb| kb.parse::<u64>().ok())
                            .map(|kb| kb / 1024)
                    })
                    .unwrap_or(8192)
            }
            #[cfg(not(any(target_os = "macos", target_os = "linux")))]
            { 8192u64 }
        };
        // 80% of system RAM — aggressive enough to catch real leaks,
        // generous enough not to false-kill on big machines.
        let max_rss_mb: u64 = (system_ram_mb * 80) / 100;
        eprintln!(
            "[MEMGUARD] Memory guard: system={}MB, limit={}MB (80%)", system_ram_mb, max_rss_mb
        );
        loop {
            interval.tick().await;
            dump_memory_report();
            let rss = current_rss_mb();
            if rss > max_rss_mb {
                eprintln!(
                    "[MEMLEAK] FATAL: RSS {}MB exceeds {}MB limit (80% of {}MB system RAM) — \
                     exiting gracefully to avoid OOM. Restart with: npm start. Fix tracked in #603.",
                    rss, max_rss_mb, system_ram_mb
                );
                // Give time for the message to flush
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                std::process::exit(1);
            }
        }
    });

    // Accept connections (event-driven - sleeps until connection)
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let state = state.clone();

                // Spawn thread for concurrent handling. Unix socket = LOCAL caller
                // (the operator on the box) → None = owner-by-locality.
                std::thread::spawn(move || {
                    if let Err(e) = handle_client(stream, state, None) {
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
