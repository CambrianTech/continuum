//! CommandExecutor — Universal command execution for ALL continuum-core processes
//!
//! This is the foundational primitive that allows ANY spawned task (sentinels,
//! background jobs, etc.) to execute ANY command in the system, regardless of
//! whether it's implemented in Rust or TypeScript.
//!
//! Usage:
//! ```rust
//! // Works for Rust modules
//! runtime::execute_command_json("health-check", json!({})).await?;
//!
//! // Works for TypeScript commands (via CommandRouterServer)
//! runtime::execute_command_json("screenshot", json!({"querySelector": "body"})).await?;
//!
//! // Sentinel doesn't know or care where command is implemented
//! ```
//!
//! Architecture:
//! - Rust modules: Routed directly through ModuleRegistry
//! - TypeScript commands: Routed via Unix socket to CommandRouterServer
//!   (socket: /tmp/jtag-command-router.sock)

use serde_json::Value;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tracing::Instrument;

use super::command_events::{CommandCompletedEvent, COMMAND_COMPLETED_TOPIC};
use super::command_interceptor::{CommandInterceptor, InterceptorOutcome};
use super::message_bus::MessageBus;
use super::{CommandResult, ModuleRegistry};
use crate::routing::{route, CommandUri, RouteDecision};

/// Socket path for TypeScript command routing
const TS_COMMAND_SOCKET: &str = "/tmp/jtag-command-router.sock";

/// Universal command executor that routes to interceptors, then Rust
/// modules, then TypeScript.
///
/// # Dispatch order (the chain)
///
/// Per [docs/architecture/MODULE-ARCHITECTURE.md](../../../../../docs/architecture/MODULE-ARCHITECTURE.md)
/// §5 ("Composition: Commands Call Commands"): every command walks the
/// same dispatch chain regardless of which language or machine
/// implements it. The chain is:
///
/// 1. **Interceptors** (in insertion order). Each one gets first look at
///    `(command, params)`. An interceptor can take the command (and
///    short-circuit the chain), pass (`Decline` — try the next), or
///    fail (`Err` — propagate immediately, no silent fallthrough).
///    Today's intended order is `[airc, grid]`: explicit airc-routed
///    commands beat grid's capability-based remote routing.
///
/// 2. **Local Rust module registry**. If no interceptor took the
///    command, the registry tries to find a Rust `ServiceModule` whose
///    `command_prefixes` include this command. If found, the module's
///    `handle_command` runs locally.
///
/// 3. **TypeScript via Unix socket**. If no Rust module owns the
///    command, fall through to the existing `CommandRouterServer` IPC
///    bridge. This preserves backwards compatibility with every
///    TS-implemented command in `src/commands/`.
///
/// The chain is the same primitive for every transport: local Rust,
/// remote Rust over grid, remote Rust over airc, TS over IPC. Adding a
/// transport is adding an interceptor; no kernel changes needed.
pub struct CommandExecutor {
    /// Rust module registry (for Rust-implemented commands).
    registry: Arc<ModuleRegistry>,
    /// Interceptor chain. Tried in insertion order BEFORE local
    /// dispatch. First interceptor to return Handled wins.
    interceptors: Vec<Arc<dyn CommandInterceptor>>,
    /// Optional message bus. When wired, every `execute()` emits a
    /// `command:completed` event after the dispatch settles
    /// (success or error). `None` in test fixtures + back-compat
    /// init paths — no events fire then.
    ///
    /// Per [docs/planning/PERSONA-AS-DEVELOPER-GAP.md](../../../../../docs/planning/PERSONA-AS-DEVELOPER-GAP.md)
    /// Priority 3: the bus emission is what lets the persona's
    /// autonomous loop stay reactive instead of poll-blocking.
    bus: Option<Arc<MessageBus>>,
}

impl CommandExecutor {
    pub fn new(registry: Arc<ModuleRegistry>) -> Self {
        Self {
            registry,
            interceptors: Vec::new(),
            bus: None,
        }
    }

    /// Add an interceptor to the chain (builder-style). Interceptors are
    /// tried in insertion order, so wire higher-priority transports
    /// FIRST.
    ///
    /// Default global wire order (in `init_executor`): `[airc, grid]`.
    /// Tests and one-off bin tools can build their own chain.
    pub fn with_interceptor(mut self, interceptor: Arc<dyn CommandInterceptor>) -> Self {
        self.interceptors.push(interceptor);
        self
    }

    /// Wire a message bus so every dispatch emits a
    /// `command:completed` event after settling. Production
    /// startup (`ipc::start_server`) sets this; test fixtures that
    /// don't need bus events omit it.
    ///
    /// Per [docs/architecture/COMMAND-INFRASTRUCTURE-FIELD-MANUAL.md](../../../../../docs/architecture/COMMAND-INFRASTRUCTURE-FIELD-MANUAL.md):
    /// the bus is the Events primitive; this method composes it with
    /// the Commands primitive at the kernel's dispatch boundary.
    pub fn with_message_bus(mut self, bus: Arc<MessageBus>) -> Self {
        self.bus = Some(bus);
        self
    }

    /// Number of registered interceptors. Diagnostic; not on the hot
    /// path. Useful for asserting the wire order in tests and for the
    /// `kernel/health` command to surface the chain depth.
    pub fn interceptor_count(&self) -> usize {
        self.interceptors.len()
    }

    /// Whether the executor has a message bus wired (and will emit
    /// `command:completed` events on dispatch). Diagnostic; tests
    /// use it to verify wiring.
    pub fn has_message_bus(&self) -> bool {
        self.bus.is_some()
    }

    /// Execute ANY command — walks the dispatch chain documented on the
    /// struct: interceptors → local Rust module → TypeScript bridge.
    ///
    /// After the dispatch settles (success OR error), emits a
    /// `command:completed` event on the message bus when one is
    /// wired. Subscribers consume those events to implement
    /// reactive control flow per the RTOS-brain doctrine
    /// (handlers never block on result polls).
    ///
    /// Slice P note: the API takes a typed [`CommandUri`]; remote
    /// variants (Peer/Room/Broadcast) return a typed not-yet-implemented
    /// error until the transport selector lands in a subsequent commit
    /// on the Slice P branch. The interceptor chain still operates on
    /// `&str` paths internally for this commit — the interceptor trait
    /// migration is a follow-up to minimize blast radius.
    pub async fn execute(
        &self,
        command: impl Into<CommandUri>,
        params: Value,
    ) -> Result<CommandResult, String> {
        let command: CommandUri = command.into();
        let start = std::time::Instant::now();
        let outcome = self.dispatch(&command, params).await;
        self.emit_command_completed(
            command.path(),
            &outcome,
            start.elapsed().as_millis() as u64,
        );
        outcome
    }

    /// Routing decision on a [`CommandUri`]. Local URIs go through the
    /// existing chain; non-Local URIs return a typed
    /// not-yet-implemented error pending the transport selector.
    ///
    /// Slice P note: this is where the per-dispatch [`tracing::Span`] is
    /// established. The URI lives in the span as a structured field;
    /// every `debug!` / `info!` / `probe!` event inside the dispatched
    /// command inherits the tag automatically. Per-persona log
    /// segregation, cross-grid trace correlation, and URI-routed
    /// observability all fall out of this one seam — no
    /// per-call-site instrumentation needed.
    ///
    /// ## Why `.instrument(span).await` and not `let _enter = span.enter()`
    ///
    /// `tracing`'s docs explicitly forbid holding a `_enter` guard
    /// across `.await` in async code: tokio moves the task between
    /// threads at suspension points, and the thread-local
    /// `on_enter`/`on_exit` cadence breaks. The
    /// [`UriCaptureLayer`](crate::routing::UriCaptureLayer) thread-local
    /// stack goes stale on the post-await thread, and `stack!()` then
    /// returns either a frame for a span that's already exited
    /// somewhere else, or the wrong chain entirely.
    ///
    /// `.instrument(span)` wraps the future so the span enters and
    /// exits at suspension boundaries automatically. Slice P's URI
    /// ancestry guarantee — `stack!()` always returns the correct
    /// chain inside a dispatched command — depends on this shape.
    async fn dispatch(
        &self,
        command: &CommandUri,
        params: Value,
    ) -> Result<CommandResult, String> {
        let decision = route(command);
        let span = tracing::info_span!(
            "cmd",
            uri = %command,
            path = %command.path(),
            route_kind = decision.kind().as_str(),
        );
        async move {
            // Slice P note: this match is the substrate's transport
            // seam. Each non-Local variant returns a typed error
            // naming the missing transport — when the AircTransport
            // commit lands, the Peer/Room/Broadcast arms become real
            // calls and the dispatcher itself doesn't change. That's
            // the typed-primitive payoff.
            match decision {
                RouteDecision::Local { path, .. } => self.execute_inner(&path, params).await,
                RouteDecision::Peer { peer, node, env, path, .. } => Err(format!(
                    "Peer dispatch not yet implemented — \
                     AircTransport lands in a subsequent Slice P commit. \
                     Routing was: peer={peer:?}, node={node:?}, env={env:?}, path={path}"
                )),
                RouteDecision::Room { room_id, env, path, .. } => Err(format!(
                    "Room broadcast not yet implemented — \
                     AircTransport lands in a subsequent Slice P commit. \
                     Routing was: room={room_id}, env={env:?}, path={path}"
                )),
                RouteDecision::Broadcast { peer, node, path, .. } => Err(format!(
                    "Env-wildcard broadcast not yet implemented — \
                     AircTransport lands in a subsequent Slice P commit. \
                     Routing was: peer={peer:?}, node={node:?}, path={path}"
                )),
            }
        }
        .instrument(span)
        .await
    }

    /// The dispatch chain itself. Extracted so `execute` can wrap it
    /// with timing + event emission without burying the routing
    /// logic in instrumentation.
    async fn execute_inner(
        &self,
        command: &str,
        params: Value,
    ) -> Result<CommandResult, String> {
        let log = super::logger("command-executor");

        // 1. Walk the interceptor chain. First Handle wins. Decline
        //    moves on. Err propagates immediately — no silent
        //    fallthrough, per the trait contract.
        for interceptor in &self.interceptors {
            match interceptor.try_route(command, &params).await {
                Ok(InterceptorOutcome::Handled(result)) => {
                    log.debug(&format!(
                        "Routing '{}' via interceptor '{}'",
                        command,
                        interceptor.name()
                    ));
                    return Ok(result);
                }
                Ok(InterceptorOutcome::Decline) => continue,
                Err(e) => {
                    log.error(&format!(
                        "Interceptor '{}' failed on '{}': {}",
                        interceptor.name(),
                        command,
                        e
                    ));
                    return Err(e);
                }
            }
        }

        // 2. Try the local Rust module registry.
        if let Some((module, cmd)) = self.registry.route_command(command) {
            log.debug(&format!("Routing '{}' to local Rust module", command));
            return module.handle_command(&cmd, params).await;
        }

        // 3. Fall through to TypeScript via Unix socket.
        log.debug(&format!(
            "Routing '{}' to TypeScript via CommandRouterServer",
            command
        ));
        let json = self.execute_ts_command(command, params).await?;
        Ok(CommandResult::Json(json))
    }

    /// Publish a `command:completed` event on the bus (when wired).
    /// Fire-and-forget — never blocks the caller, never panics if
    /// the bus has no subscribers. Telemetry path, not contract.
    fn emit_command_completed(
        &self,
        command: &str,
        outcome: &Result<CommandResult, String>,
        duration_ms: u64,
    ) {
        let Some(bus) = self.bus.as_ref() else {
            return;
        };
        let event = CommandCompletedEvent {
            command_name: command.to_string(),
            duration_ms,
            success: outcome.is_ok(),
            error: outcome.as_ref().err().cloned(),
        };
        match serde_json::to_value(&event) {
            Ok(payload) => bus.publish_async_only(COMMAND_COMPLETED_TOPIC, payload),
            Err(e) => {
                // Should be impossible (the struct is plain fields
                // with no exotic types) but tolerate to keep the
                // dispatch path infallible at the telemetry layer.
                super::logger("command-executor").warn(&format!(
                    "command-completed event serialize failed for '{command}': {e}"
                ));
            }
        }
    }

    /// Convenience: execute and extract JSON directly.
    ///
    /// Delegates to [`CommandResult::to_json_value`] which handles all
    /// cell shapes — Json/Binary return their payload, Handle serializes
    /// the HandleRef, Stream/Lambda return their not-yet-wired protocol
    /// error so the caller knows the cell shape requires direct match.
    pub async fn execute_json(
        &self,
        command: impl Into<CommandUri>,
        params: Value,
    ) -> Result<Value, String> {
        self.execute(command, params).await?.to_json_value()
    }

    /// Execute a command ONLY via TypeScript (bypasses Rust registry).
    /// Use this when a Rust module needs to forward to a TypeScript-implemented
    /// command that shares the same prefix (avoids infinite recursion).
    ///
    /// Slice P note: the URI's `path()` is forwarded over the TS bridge
    /// as the conventional command name. Remote URIs are rejected.
    pub async fn execute_ts(
        &self,
        command: impl Into<CommandUri>,
        params: Value,
    ) -> Result<CommandResult, String> {
        let command: CommandUri = command.into();
        if !command.is_local() {
            return Err(format!(
                "Remote dispatch for {command} not supported via execute_ts \
                 — TS bridge only handles local URIs."
            ));
        }
        let json = self.execute_ts_command(command.path(), params).await?;
        Ok(CommandResult::Json(json))
    }

    /// Convenience: execute via TypeScript only and extract JSON directly
    pub async fn execute_ts_json(
        &self,
        command: impl Into<CommandUri>,
        params: Value,
    ) -> Result<Value, String> {
        let command: CommandUri = command.into();
        if !command.is_local() {
            return Err(format!(
                "Remote dispatch for {command} not supported via execute_ts_json \
                 — TS bridge only handles local URIs."
            ));
        }
        self.execute_ts_command(command.path(), params).await
    }

    /// Execute command via TypeScript CommandRouterServer (Unix socket)
    ///
    /// Protocol:
    /// - Request: `{"command": "...", "params": {...}}\n`
    /// - Response: `{"success": true, "result": ...}\n` or `{"success": false, "error": "..."}\n`
    async fn execute_ts_command(&self, command: &str, params: Value) -> Result<Value, String> {
        let log = super::logger("command-executor");

        // Connect to CommandRouterServer
        log.debug(&format!(
            "Connecting to TypeScript socket: {}",
            TS_COMMAND_SOCKET
        ));
        let stream = UnixStream::connect(TS_COMMAND_SOCKET).await.map_err(|e| {
            format!(
                "Failed to connect to CommandRouterServer at {}: {}",
                TS_COMMAND_SOCKET, e
            )
        })?;

        let (reader, mut writer) = stream.into_split();
        let mut buf_reader = BufReader::new(reader);

        // Build and send request
        let request = serde_json::json!({
            "command": command,
            "params": params,
        });
        let request_line = format!("{}\n", request);

        log.debug(&format!("Sending: {}", command));
        writer
            .write_all(request_line.as_bytes())
            .await
            .map_err(|e| format!("Failed to send command: {}", e))?;
        writer
            .flush()
            .await
            .map_err(|e| format!("Failed to flush: {}", e))?;

        // Read response
        let mut response_line = String::new();
        buf_reader
            .read_line(&mut response_line)
            .await
            .map_err(|e| format!("Failed to read response: {}", e))?;

        log.debug(&format!("Received response: {} bytes", response_line.len()));

        // Parse response
        let response: Value = serde_json::from_str(&response_line).map_err(|e| {
            format!(
                "Invalid response JSON: {} (raw: {})",
                e,
                response_line.trim()
            )
        })?;

        // Check success
        if response.get("success").and_then(|v| v.as_bool()) == Some(true) {
            let result = response.get("result").cloned().unwrap_or(Value::Null);
            log.info(&format!("Command '{}' succeeded", command));
            Ok(result)
        } else {
            let error = response
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown error from TypeScript");
            log.error(&format!("Command '{}' failed: {}", command, error));
            Err(error.to_string())
        }
    }
}

// Global executor instance - initialized once at startup
static GLOBAL_EXECUTOR: std::sync::OnceLock<Arc<CommandExecutor>> = std::sync::OnceLock::new();

/// Initialize the global command executor with no interceptors.
///
/// Back-compat shim around [`init_executor_with_interceptors`] for
/// callers that don't have transports to wire. Prefer the
/// `_with_interceptors` form in production startup so commands can
/// transparently route to remote peers via grid / airc / future
/// transports.
pub fn init_executor(registry: Arc<ModuleRegistry>) {
    init_executor_with_interceptors(registry, Vec::new());
}

/// Initialize the global command executor with a wired interceptor
/// chain.
///
/// Production startup (`ipc::start_server`) calls this with
/// `[AircInterceptor, GridInterceptor]` so capability-based routing
/// and explicit airc-targeted commands work transparently from any
/// caller. The chain order is policy: the earlier an interceptor
/// sits, the higher its priority (airc beats grid because explicit
/// peer targets shouldn't be overridden by grid's capability heuristic).
///
/// Idempotent: only the first call wins (per the underlying
/// `OnceLock`). A subsequent call is silently a no-op — useful for
/// test fixtures that may try to init multiple times but should
/// preserve the production wiring.
pub fn init_executor_with_interceptors(
    registry: Arc<ModuleRegistry>,
    interceptors: Vec<Arc<dyn CommandInterceptor>>,
) {
    init_executor_full(registry, interceptors, None);
}

/// Initialize the global executor with interceptors AND a wired
/// message bus, so every dispatch emits a `command:completed` event.
///
/// Production startup should prefer this form — the event stream is
/// what lets the persona autonomous loop stay reactive (per RTOS
/// doctrine) instead of poll-blocking on `code/shell/watch` style
/// surfaces. See
/// [docs/planning/PERSONA-AS-DEVELOPER-GAP.md](../../../../../docs/planning/PERSONA-AS-DEVELOPER-GAP.md)
/// Priority 3.
pub fn init_executor_with_bus_and_interceptors(
    registry: Arc<ModuleRegistry>,
    bus: Arc<MessageBus>,
    interceptors: Vec<Arc<dyn CommandInterceptor>>,
) {
    init_executor_full(registry, interceptors, Some(bus));
}

/// Internal: full init taking optional bus. Single OnceLock-set call
/// path so production + back-compat paths share one source of truth.
fn init_executor_full(
    registry: Arc<ModuleRegistry>,
    interceptors: Vec<Arc<dyn CommandInterceptor>>,
    bus: Option<Arc<MessageBus>>,
) {
    let log = super::logger("command-executor");
    let interceptor_count = interceptors.len();
    let has_bus = bus.is_some();
    let mut executor = CommandExecutor::new(registry);
    for interceptor in interceptors {
        executor = executor.with_interceptor(interceptor);
    }
    if let Some(b) = bus {
        executor = executor.with_message_bus(b);
    }
    let _ = GLOBAL_EXECUTOR.set(Arc::new(executor));
    log.info(&format!(
        "Initialized with {} interceptor(s), bus={} (TS bridge: {})",
        interceptor_count, has_bus, TS_COMMAND_SOCKET
    ));
}

/// Get the global command executor
/// Panics if not initialized - this is intentional, executor MUST be initialized at startup
pub fn executor() -> Arc<CommandExecutor> {
    GLOBAL_EXECUTOR
        .get()
        .expect("CommandExecutor not initialized - call init_executor() at startup")
        .clone()
}

/// Execute a command from anywhere, returning CommandResult
///
/// Usage:
/// ```ignore
/// use crate::runtime::command_executor;
/// use crate::routing::CommandUri;
///
/// let result = command_executor::execute(
///     CommandUri::local("code/edit"),
///     params,
/// ).await?;
/// ```
pub async fn execute(
    command: impl Into<CommandUri>,
    params: Value,
) -> Result<CommandResult, String> {
    executor().execute(command, params).await
}

/// Execute a command and extract JSON result (convenience for most use cases)
pub async fn execute_json(
    command: impl Into<CommandUri>,
    params: Value,
) -> Result<Value, String> {
    executor().execute_json(command, params).await
}

/// Execute a command ONLY via TypeScript, bypassing Rust registry.
/// Use when a Rust module needs to forward to a TypeScript command
/// that shares the same prefix (e.g., ai_provider forwarding ai/agent).
pub async fn execute_ts(
    command: impl Into<CommandUri>,
    params: Value,
) -> Result<CommandResult, String> {
    executor().execute_ts(command, params).await
}

/// Execute via TypeScript only and extract JSON (convenience)
pub async fn execute_ts_json(
    command: impl Into<CommandUri>,
    params: Value,
) -> Result<Value, String> {
    executor().execute_ts_json(command, params).await
}

#[cfg(test)]
mod tests {
    use super::super::airc_interceptor::AircInterceptor;
    use super::*;
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn test_executor_creation() {
        let registry = Arc::new(ModuleRegistry::new());
        let _executor = CommandExecutor::new(registry);
        // Just verify it compiles and can be created
    }

    #[test]
    fn empty_chain_by_default() {
        let registry = Arc::new(ModuleRegistry::new());
        let executor = CommandExecutor::new(registry);
        assert_eq!(
            executor.interceptor_count(),
            0,
            "fresh executor must have NO interceptors; \
             interceptors are opt-in via with_interceptor or init_executor wiring"
        );
    }

    #[test]
    fn with_interceptor_grows_chain_in_insertion_order() {
        let registry = Arc::new(ModuleRegistry::new());
        let executor = CommandExecutor::new(registry)
            .with_interceptor(Arc::new(AircInterceptor::new()));
        assert_eq!(
            executor.interceptor_count(),
            1,
            "with_interceptor must append, not replace"
        );
    }

    /// Test interceptor that records the call order so we can prove the
    /// chain walks in insertion order.
    struct RecordingDecliner {
        name: &'static str,
        seen: Arc<AtomicUsize>,
        mark: usize,
    }

    #[async_trait]
    impl CommandInterceptor for RecordingDecliner {
        async fn try_route(
            &self,
            _command: &str,
            _params: &Value,
        ) -> Result<InterceptorOutcome, String> {
            // Record which slot was consulted. The test asserts the
            // observed counter equals the expected slot, proving order.
            self.seen.store(self.mark, Ordering::SeqCst);
            Ok(InterceptorOutcome::Decline)
        }

        fn name(&self) -> &'static str {
            self.name
        }
    }

    /// Test interceptor that always handles, used to short-circuit the
    /// fall-through to local Rust + TS dispatch (which would require
    /// actual modules and a live TS bridge — out of scope for unit tests).
    struct AlwaysHandle;

    #[async_trait]
    impl CommandInterceptor for AlwaysHandle {
        async fn try_route(
            &self,
            _command: &str,
            _params: &Value,
        ) -> Result<InterceptorOutcome, String> {
            Ok(InterceptorOutcome::Handled(CommandResult::Json(
                serde_json::json!({ "handled": true }),
            )))
        }

        fn name(&self) -> &'static str {
            "always-handle"
        }
    }

    #[tokio::test]
    async fn interceptors_walked_in_insertion_order_when_all_decline() {
        let last_seen = Arc::new(AtomicUsize::new(0));
        let registry = Arc::new(ModuleRegistry::new());
        let executor = CommandExecutor::new(registry)
            .with_interceptor(Arc::new(RecordingDecliner {
                name: "first",
                seen: last_seen.clone(),
                mark: 1,
            }))
            .with_interceptor(Arc::new(RecordingDecliner {
                name: "second",
                seen: last_seen.clone(),
                mark: 2,
            }))
            .with_interceptor(Arc::new(AlwaysHandle));

        let result = executor
            .execute("anything", Value::Null)
            .await
            .expect("AlwaysHandle should resolve the dispatch");

        match result {
            CommandResult::Json(v) => assert_eq!(v["handled"], true),
            other => panic!("expected Json, got {other:?}"),
        }
        // The last decliner to run was `second` (mark 2). If the chain
        // walked out of order, this would be `1` or `0`.
        assert_eq!(
            last_seen.load(Ordering::SeqCst),
            2,
            "interceptors must be consulted in insertion order"
        );
    }

    #[tokio::test]
    async fn first_handler_short_circuits_later_interceptors() {
        let later_called = Arc::new(AtomicUsize::new(0));
        let registry = Arc::new(ModuleRegistry::new());
        let executor = CommandExecutor::new(registry)
            .with_interceptor(Arc::new(AlwaysHandle))
            .with_interceptor(Arc::new(RecordingDecliner {
                name: "should-never-run",
                seen: later_called.clone(),
                mark: 99,
            }));

        let _ = executor.execute("anything", Value::Null).await.unwrap();
        assert_eq!(
            later_called.load(Ordering::SeqCst),
            0,
            "interceptors after the first Handled must not be consulted"
        );
    }

    #[tokio::test]
    async fn airc_interceptor_declines_when_no_airc_target_params() {
        // The airc interceptor at the head of the chain must NOT block
        // existing local-Rust or TS commands that don't carry airc
        // routing params. This is the back-compat guarantee that lets
        // the airc interceptor be safely installed at init_executor.
        //
        // Without a registered Rust module for "test/cmd", the executor
        // will fall through past the airc interceptor (Decline) past the
        // registry (no match) and try to connect to the TS bridge,
        // which fails in tests because the socket doesn't exist. That
        // failure is expected: the test is asserting the airc
        // interceptor did NOT short-circuit, NOT that TS dispatch works.
        let registry = Arc::new(ModuleRegistry::new());
        let executor =
            CommandExecutor::new(registry).with_interceptor(Arc::new(AircInterceptor::new()));

        let result = executor
            .execute(
                "test/cmd",
                serde_json::json!({ "ordinaryParam": "value" }),
            )
            .await;

        // We expect the TS bridge connection to fail (no socket in tests).
        // The IMPORTANT assertion is that the failure came from the TS
        // bridge, NOT from the airc interceptor — proving the airc
        // interceptor declined cleanly and the chain fell through.
        let err = result.expect_err("TS bridge will fail in tests; that's OK");
        assert!(
            !err.contains("airc"),
            "error must come from TS bridge fallthrough, not from airc \
             interceptor — otherwise the airc interceptor incorrectly \
             intercepted a non-airc command. err: {err}"
        );
    }

    #[tokio::test]
    async fn airc_interceptor_fails_loud_when_airc_peer_targeted() {
        // The airc interceptor MUST short-circuit with a loud error when
        // a caller passes aircPeer, even before the transport is wired.
        // Silent fall-through would hide the missing transport from the
        // caller, who would then see local-dispatch results (or worse,
        // success on the wrong machine) and not know airc wasn't used.
        let registry = Arc::new(ModuleRegistry::new());
        let executor =
            CommandExecutor::new(registry).with_interceptor(Arc::new(AircInterceptor::new()));

        let err = executor
            .execute(
                "chat/send",
                serde_json::json!({ "aircPeer": "peer-id", "content": "hello" }),
            )
            .await
            .expect_err(
                "explicit aircPeer must error until transport is wired — \
                 not silently fall through to local",
            );
        assert!(
            err.contains("airc"),
            "error must identify airc as the unresolved transport: {err}"
        );
        assert!(
            err.contains("peer-id"),
            "error must echo the target so the caller can correlate logs: {err}"
        );
    }

    // ════════════════════════════════════════════════════════════════
    // command:completed event emission (PERSONA-AS-DEVELOPER-GAP §P3)
    // ════════════════════════════════════════════════════════════════
    //
    // Every dispatch through `execute()` should publish ONE
    // command:completed event on the wired bus, with the command
    // name + duration + success flag + optional error. Tests pin the
    // wire shape, the success/failure parity, the no-bus no-op
    // path, and the multi-thread emission invariants.

    use super::super::command_events::{CommandCompletedEvent, COMMAND_COMPLETED_TOPIC};
    use super::super::message_bus::MessageBus;

    /// Test-only ServiceModule that returns canned results so we can
    /// drive `execute()` through the local-Rust dispatch path
    /// without standing up a real module. Stores the canned outcome
    /// as `Result<Value, String>` (not `CommandResult`) because
    /// `CommandResult` doesn't impl Clone — we re-wrap in Json each
    /// call. Uses a fixed `canned/` prefix to keep the trait's
    /// `&'static [&'static str]` requirement satisfied without
    /// test-time string juggling.
    struct CannedModule {
        canned: Result<serde_json::Value, String>,
    }

    impl CannedModule {
        const PREFIXES: &'static [&'static str] = &["canned/"];
    }

    #[async_trait]
    impl crate::runtime::ServiceModule for CannedModule {
        fn config(&self) -> crate::runtime::ModuleConfig {
            crate::runtime::ModuleConfig {
                name: "canned",
                priority: crate::runtime::ModulePriority::Normal,
                command_prefixes: Self::PREFIXES,
                event_subscriptions: &[],
                needs_dedicated_thread: false,
                max_concurrency: 0,
                tick_interval: None,
            }
        }
        async fn initialize(
            &self,
            _ctx: &crate::runtime::ModuleContext,
        ) -> Result<(), String> {
            Ok(())
        }
        async fn handle_command(
            &self,
            _command: &str,
            _params: serde_json::Value,
        ) -> Result<CommandResult, String> {
            match &self.canned {
                Ok(v) => Ok(CommandResult::Json(v.clone())),
                Err(e) => Err(e.clone()),
            }
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    /// Drain the bus receiver until we find an event named
    /// `command:completed`. Returns the parsed payload.
    async fn next_command_completed(
        rx: &mut tokio::sync::broadcast::Receiver<crate::runtime::message_bus::BusEvent>,
    ) -> CommandCompletedEvent {
        // Bound the wait so a missing event fails the test loudly
        // instead of hanging.
        let recv = tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                let event = rx.recv().await.expect("bus channel must not close");
                if event.name == COMMAND_COMPLETED_TOPIC {
                    return event;
                }
            }
        })
        .await
        .expect("expected a command:completed event within 2s");
        serde_json::from_value(recv.payload).expect("event payload must parse")
    }

    #[tokio::test]
    async fn dispatch_emits_completed_event_on_success() {
        let registry = Arc::new(ModuleRegistry::new());
        registry.register(Arc::new(CannedModule {
            canned: Ok(serde_json::json!({ "ok": true })),
        }));
        let bus = Arc::new(MessageBus::new());
        let mut rx = bus.receiver();
        let executor = CommandExecutor::new(registry).with_message_bus(bus);

        executor
            .execute("canned/ping", serde_json::json!({}))
            .await
            .expect("dispatch succeeds");

        let event = next_command_completed(&mut rx).await;
        assert_eq!(event.command_name, "canned/ping");
        assert!(event.success);
        assert!(
            event.error.is_none(),
            "success path must not carry an error: {event:?}"
        );
        // Duration is wall-clock — should be non-pathological. The
        // canned module returns immediately; even on slow CI 500ms
        // is generous.
        assert!(
            event.duration_ms < 500,
            "trivial dispatch should be fast: {} ms",
            event.duration_ms
        );
    }

    #[tokio::test]
    async fn dispatch_emits_completed_event_on_handler_error() {
        let registry = Arc::new(ModuleRegistry::new());
        registry.register(Arc::new(CannedModule {
            canned: Err("simulated handler failure".to_string()),
        }));
        let bus = Arc::new(MessageBus::new());
        let mut rx = bus.receiver();
        let executor = CommandExecutor::new(registry).with_message_bus(bus);

        let err = executor
            .execute("canned/boom", serde_json::json!({}))
            .await
            .expect_err("handler returned Err");
        assert_eq!(err, "simulated handler failure");

        let event = next_command_completed(&mut rx).await;
        assert_eq!(event.command_name, "canned/boom");
        assert!(!event.success, "handler Err → success=false");
        assert_eq!(
            event.error.as_deref(),
            Some("simulated handler failure"),
            "error field carries the underlying message"
        );
    }

    #[tokio::test]
    async fn dispatch_without_wired_bus_is_no_op_telemetry() {
        // No bus = no event emission, but the dispatch itself must
        // still complete normally. This is the back-compat path for
        // tests + the old init_executor calls.
        let registry = Arc::new(ModuleRegistry::new());
        registry.register(Arc::new(CannedModule {
            canned: Ok(serde_json::json!({ "ok": true })),
        }));
        let executor = CommandExecutor::new(registry);
        assert!(!executor.has_message_bus(), "no bus wired");

        // Must succeed; no events emitted (nothing to subscribe to).
        let r = executor
            .execute("canned/ping", serde_json::json!({}))
            .await;
        assert!(r.is_ok());
    }

    #[tokio::test]
    async fn ts_bridge_failure_still_emits_completed_event() {
        // When all 3 dispatch tiers fail (no interceptor handled,
        // no Rust module registered, TS socket missing in tests) —
        // the event should still emit with success=false + the TS
        // connection error. Telemetry must cover every dispatch
        // path's terminal state.
        let registry = Arc::new(ModuleRegistry::new());
        let bus = Arc::new(MessageBus::new());
        let mut rx = bus.receiver();
        let executor = CommandExecutor::new(registry).with_message_bus(bus);

        let err = executor
            .execute("nonexistent/command", serde_json::json!({}))
            .await
            .expect_err("TS socket missing in tests");
        // Don't assert specific TS error text; just confirm it's an Err.
        let _ = err;

        let event = next_command_completed(&mut rx).await;
        assert_eq!(event.command_name, "nonexistent/command");
        assert!(!event.success);
        assert!(
            event.error.is_some(),
            "TS bridge failure path must populate error: {event:?}"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_dispatches_each_emit_their_own_event() {
        // N parallel dispatches must each emit ONE event with the
        // correct command_name + success flag. No event interleaving
        // corruption, no event loss, no event duplication.
        const PARALLEL: usize = 32;
        let registry = Arc::new(ModuleRegistry::new());
        registry.register(Arc::new(CannedModule {
            canned: Ok(serde_json::json!({ "ok": true })),
        }));
        let bus = Arc::new(MessageBus::new());
        let mut rx = bus.receiver();
        let executor = Arc::new(CommandExecutor::new(registry).with_message_bus(bus));

        let mut tasks = Vec::with_capacity(PARALLEL);
        for i in 0..PARALLEL {
            let exec = executor.clone();
            let cmd = format!("canned/op-{i:02}");
            tasks.push(tokio::spawn(async move {
                exec.execute(&cmd, serde_json::json!({})).await
            }));
        }
        for t in tasks {
            t.await.unwrap().expect("each dispatch succeeds");
        }

        // Drain bus; collect every command:completed event up to N
        // (with a deadline so a missing event fails loud).
        let mut events: Vec<CommandCompletedEvent> = Vec::with_capacity(PARALLEL);
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
        while events.len() < PARALLEL {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                break;
            }
            match tokio::time::timeout(remaining, rx.recv()).await {
                Ok(Ok(event)) if event.name == COMMAND_COMPLETED_TOPIC => {
                    let parsed: CommandCompletedEvent =
                        serde_json::from_value(event.payload).expect("payload parses");
                    events.push(parsed);
                }
                Ok(Ok(_)) => continue, // unrelated event topic — skip
                Ok(Err(_)) => break,
                Err(_) => break,
            }
        }

        assert_eq!(
            events.len(),
            PARALLEL,
            "each concurrent dispatch must emit exactly one event"
        );

        // Every emitted command_name must be unique and match a
        // dispatched op. No event corruption from interleaved
        // publish().
        let mut names: Vec<String> = events.iter().map(|e| e.command_name.clone()).collect();
        names.sort();
        let expected: Vec<String> = (0..PARALLEL).map(|i| format!("canned/op-{i:02}")).collect();
        let mut expected_sorted = expected.clone();
        expected_sorted.sort();
        assert_eq!(
            names, expected_sorted,
            "every dispatched command must appear exactly once in the event stream"
        );

        // Every event reports success (the canned module returns Ok).
        for e in &events {
            assert!(e.success, "all canned dispatches succeed: {e:?}");
            assert!(e.error.is_none());
        }
    }

    // Note: the URI-propagation-across-await assertion lives in
    // `crate::routing::uri_layer::tests` where it can run against the
    // Layer directly without the noise of CommandExecutor's other
    // tokio-runtime tests sharing the cargo test process.
}
