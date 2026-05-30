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

use super::command_interceptor::{CommandInterceptor, InterceptorOutcome};
use super::{CommandResult, ModuleRegistry};

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
}

impl CommandExecutor {
    pub fn new(registry: Arc<ModuleRegistry>) -> Self {
        Self {
            registry,
            interceptors: Vec::new(),
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

    /// Number of registered interceptors. Diagnostic; not on the hot
    /// path. Useful for asserting the wire order in tests and for the
    /// `kernel/health` command to surface the chain depth.
    pub fn interceptor_count(&self) -> usize {
        self.interceptors.len()
    }

    /// Execute ANY command — walks the dispatch chain documented on the
    /// struct: interceptors → local Rust module → TypeScript bridge.
    pub async fn execute(&self, command: &str, params: Value) -> Result<CommandResult, String> {
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

    /// Convenience: execute and extract JSON directly.
    ///
    /// Delegates to [`CommandResult::to_json_value`] which handles all
    /// cell shapes — Json/Binary return their payload, Handle serializes
    /// the HandleRef, Stream/Lambda return their not-yet-wired protocol
    /// error so the caller knows the cell shape requires direct match.
    pub async fn execute_json(&self, command: &str, params: Value) -> Result<Value, String> {
        self.execute(command, params).await?.to_json_value()
    }

    /// Execute a command ONLY via TypeScript (bypasses Rust registry).
    /// Use this when a Rust module needs to forward to a TypeScript-implemented
    /// command that shares the same prefix (avoids infinite recursion).
    pub async fn execute_ts(&self, command: &str, params: Value) -> Result<CommandResult, String> {
        let json = self.execute_ts_command(command, params).await?;
        Ok(CommandResult::Json(json))
    }

    /// Convenience: execute via TypeScript only and extract JSON directly
    pub async fn execute_ts_json(&self, command: &str, params: Value) -> Result<Value, String> {
        self.execute_ts_command(command, params).await
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
    let log = super::logger("command-executor");
    let interceptor_count = interceptors.len();
    let mut executor = CommandExecutor::new(registry);
    for interceptor in interceptors {
        executor = executor.with_interceptor(interceptor);
    }
    let _ = GLOBAL_EXECUTOR.set(Arc::new(executor));
    log.info(&format!(
        "Initialized with {} interceptor(s) (TS bridge: {})",
        interceptor_count, TS_COMMAND_SOCKET
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
/// ```rust
/// use crate::runtime::command_executor;
///
/// let result = command_executor::execute("code/edit", params).await?;
/// ```
pub async fn execute(command: &str, params: Value) -> Result<CommandResult, String> {
    executor().execute(command, params).await
}

/// Execute a command and extract JSON result (convenience for most use cases)
pub async fn execute_json(command: &str, params: Value) -> Result<Value, String> {
    executor().execute_json(command, params).await
}

/// Execute a command ONLY via TypeScript, bypassing Rust registry.
/// Use when a Rust module needs to forward to a TypeScript command
/// that shares the same prefix (e.g., ai_provider forwarding ai/agent).
pub async fn execute_ts(command: &str, params: Value) -> Result<CommandResult, String> {
    executor().execute_ts(command, params).await
}

/// Execute via TypeScript only and extract JSON (convenience)
pub async fn execute_ts_json(command: &str, params: Value) -> Result<Value, String> {
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
}
