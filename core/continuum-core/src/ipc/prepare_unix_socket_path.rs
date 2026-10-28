use super::*;

pub(super) fn prepare_unix_socket_path(socket_path: &str) -> std::io::Result<()> {
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
pub(super) trait IpcStream: Read + Write + Send + Sized + 'static {
    fn try_clone_stream(&self) -> std::io::Result<Self>;
    fn peer_addr_str(&self) -> String;
}

#[cfg(unix)]
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
// IPC Server State
// ============================================================================
/// The socket path's ONE routing decision: a command string that names a PEER or a
/// ROOM (`airc://<peer>[@node]/<path>`, `airc://#room/<path>`) is dispatched by the
/// CommandExecutor, which owns the URI grammar and the remote transport; anything
/// else — a bare name, or the explicit-local `airc:///<path>` — stays on the
/// name-based `Runtime::route_command` path, byte-for-byte as before. PURE, so the
/// decision is pinned by a test rather than discovered at the CLI ("Unknown command:
/// 'airc://…/ping'", card 4fb5895c — #3690's transport was installed and unreachable
/// from every socket client).
pub(super) fn addressed_elsewhere(command: &str) -> Option<crate::routing::CommandUri> {
    match crate::routing::CommandUri::parse(command) {
        Ok(crate::routing::CommandUri::Local { .. }) | Err(_) => None,
        Ok(uri) => Some(uri),
    }
}

/// ServerState holds Arc references that are passed to ServiceModules during initialization.
/// After modules are registered with the runtime, these fields are not accessed directly
/// by ServerState methods — all command handling goes through runtime.dispatch().
/// The fields are kept here to ensure the Arc lifetimes outlive the modules.
#[allow(dead_code)]
pub(super) struct ServerState {
    pub(super) voice_service: Arc<crate::live::session::voice_service::VoiceService>,
    /// Per-persona channel registries + state — DashMap: hot-path ops are &mut self.
    pub(super) channel_registries: Arc<DashMap<Uuid, (ChannelRegistry, PersonaState)>>,
    pub(super) rag_engine: Arc<RagEngine>,
    /// Server-side audio buffer pool for handle-based synthesis.
    pub(super) audio_pool: Arc<crate::live::audio::buffer::AudioBufferPool>,
    /// Tokio runtime handle for async operations from IPC threads.
    pub(super) rt_handle: tokio::runtime::Handle,
    /// Per-persona memory manager — pure compute on in-memory MemoryCorpus.
    pub(super) memory_manager: Arc<crate::memory::PersonaMemoryManager>,
    /// Per-persona file engines — workspace-scoped file operations with change tracking.
    pub(super) file_engines: Arc<DashMap<String, FileEngine>>,
    /// Per-persona shell sessions — persistent bash per workspace with handle+poll.
    pub(super) shell_sessions: Arc<DashMap<String, ShellSession>>,
    /// Modular runtime — ServiceModule-based command routing.
    pub(super) runtime: Arc<Runtime>,
    /// GPU memory manager — unified VRAM coordination.
    pub(super) gpu_manager: Arc<GpuMemoryManager>,
    /// Connected `Provided`-command providers (eye-nodes), keyed by command name.
    /// The SAME `Arc` the `ProvidedCommandInterceptor` reads: a `provider/register`
    /// on any connection binds here, and the interceptor routes perception/observe
    /// + interface/screenshot to whoever is bound. Empty ⇒ those commands fail loud.
    pub(super) provider_registry: Arc<crate::runtime::ProviderRegistry>,
    /// The ONE command executor (the same Arc every module and the training
    /// producer hold) — the owner of URI routing and the remote transport. The
    /// socket path dispatches a PEER- or ROOM-addressed command through it; a bare
    /// name keeps the name-based `Runtime::route_command` path (card 4fb5895c).
    pub(super) executor: Arc<crate::runtime::CommandExecutor>,
}

impl ServerState {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new_with_shared_state(
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
        executor: Arc<crate::runtime::CommandExecutor>,
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
            executor,
        }
    }
}

// ============================================================================
// Handle Result - supports JSON and binary responses
// ============================================================================
/// Result from handling an IPC request.
/// Binary variant allows raw PCM audio to bypass base64 encoding entirely.
pub(super) enum HandleResult {
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
pub(super) enum Outbound {
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
pub(super) fn send_json_frame<S: Write>(stream: &mut S, response: &Response) -> std::io::Result<()> {
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
pub(super) fn send_binary_frame<S: Write>(
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
pub(super) fn send_provide_call_frame<S: Write>(
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
pub(super) fn handle_client<S: IpcStream>(
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
                    let trust = crate::modules::grid::caller_trust(Some(c));
                    if !crate::routing::command_acl::is_command_authorized(cmd, trust) {
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
                // A PEER- OR ROOM-ADDRESSED COMMAND GOES TO THE EXECUTOR, which owns the
                // URI grammar and the remote transport (#3690 installed it; nothing on
                // the socket path ever reached it — card 4fb5895c: `airc://<peer>/ping`
                // was refused as "Unknown command" because this path only ever knew
                // names). A bare name keeps the name-based route exactly as before.
                let result = match addressed_elsewhere(cmd) {
                    Some(uri) => Some(
                        state
                            .executor
                            .execute_with_caller(uri, json_value.clone(), caller.clone())
                            .await,
                    ),
                    None => {
                        state
                            .runtime
                            .route_command(cmd, json_value.clone(), caller.clone())
                            .await
                    }
                };
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
                        let caller_trust = crate::modules::grid::caller_trust(caller.as_ref());
                        let names: Vec<&str> = crate::sdk_codegen::command_registry()
                            .into_iter()
                            .filter(|d| {
                                crate::routing::command_acl::is_command_authorized(d.name, caller_trust)
                            })
                            .map(|d| d.name)
                            .collect();
                        let suggestions = crate::sdk_codegen::ai_help::did_you_mean(cmd, &names);
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

