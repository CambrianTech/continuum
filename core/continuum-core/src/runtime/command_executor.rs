//! CommandExecutor — universal command execution for substrate-internal callers
//!
//! Foundational primitive that lets any spawned task (sentinels, persona
//! loops, background jobs) dispatch any command in the substrate. The
//! implicit dispatch chain is **Rust-only** per `[[no-fallbacks-ever]]`
//! (task #219). Commands that have no Rust handler produce a typed
//! `CommandNotFound` error — there is no silent fallthrough to a TS
//! host.
//!
//! Usage:
//! ```rust
//! // Implicit dispatch — Rust modules + interceptors only.
//! // Unknown commands return Err("no Rust module handles command: ...").
//! runtime::execute_command_json("health-check", json!({})).await?;
//!
//! // Explicit TS-bridge dispatch — for the ~6 documented TS-only call
//! // sites that knowingly target a TypeScript handler.
//! // executor.execute_ts_json("ai/agent", params).await?;
//! ```
//!
//! Architecture:
//! - Implicit chain: interceptors (airc, grid, ...) → Rust module
//!   registry → typed `CommandNotFound` error. Substrate-internal.
//! - Explicit TS bridge: `execute_ts` / `execute_ts_json` public
//!   methods over Unix socket `/tmp/jtag-command-router.sock`. Used
//!   only by the documented TS-only call sites (sentinel steps, grid
//!   connection retry, ai_provider cloud-adapter fallthrough).
//!
//! Per `[[rust-is-the-core-node-is-the-shell]]`: substrate dispatch
//! ends at the Rust registry. TS is an explicit-API destination, not
//! a silent dependency.

use serde_json::Value;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
// The TS CommandRouterServer is reached over a Unix-domain socket. On Windows
// there is no Unix socket; alias to TcpStream so the TS-bridge path compiles
// unchanged. `connect()` to the filesystem-path socket then fails gracefully at
// runtime. BEHAVIORAL GAP: the explicit TS-bridge (`execute_ts*`) is
// unavailable on Windows until a TCP endpoint is wired; the Rust dispatch chain
// (the primary path) is unaffected.
#[cfg(windows)]
use tokio::net::TcpStream as UnixStream;
#[cfg(unix)]
use tokio::net::UnixStream;
use tracing::Instrument;

use super::command_events::{CommandCompletedEvent, COMMAND_COMPLETED_TOPIC};
use super::command_interceptor::{CommandInterceptor, InterceptorOutcome};
use super::message_bus::MessageBus;
use super::{CommandResult, ModuleRegistry};
use crate::routing::{route, CommandUri, RouteDecision, Transport, Verdict};

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
/// 3. **No silent TS fallthrough.** If no Rust module owns the command,
///    `execute_inner` returns a typed `Err` naming the missing command.
///    Per `[[no-fallbacks-ever]]` + `[[rust-is-the-core-node-is-the-shell]]`:
///    a substrate that silently routes unmigrated commands to a TS
///    bridge appears "broken in headless mode" the day someone forgets
///    to bring up `CommandRouterServer`, when the real bug was the
///    silent dependency. Callers that EXPLICITLY want the TS bridge
///    use [`Self::execute_ts`] / [`Self::execute_ts_json`] — those
///    public methods stay live for the handful of remaining TS-only
///    call sites (sentinel steps, grid retry, ai_provider TS
///    fallthrough for unmigrated cloud adapters). The implicit chain
///    is Rust-only.
///
/// The chain is the same primitive for every transport: local Rust,
/// remote Rust over grid, remote Rust over airc. Adding a transport is
/// adding an interceptor; no kernel changes needed. Adding a "fallback
/// to other transports" is forbidden.
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
    /// Auth policy consulted between `route()` and the dispatcher's
    /// variant match. Defaults to [`AllowAllPolicy`] so existing
    /// callers and tests don't break; operators install an ORM-backed
    /// or capability-backed impl at boot via
    /// [`Self::with_policy`].
    ///
    /// Per Slice P "every URI has a gate" — the policy is a single
    /// substrate-wide chokepoint, not a per-module concern.
    policy: Arc<dyn crate::routing::AuthPolicy>,
    /// Transport for non-Local routing decisions (Peer / Room /
    /// Broadcast). Defaults to
    /// [`NotImplementedRemoteTransport`](crate::routing::NotImplementedRemoteTransport)
    /// which produces typed errors per variant. The
    /// [`AircTransport`] commit lands the real cross-grid impl and
    /// swaps in via [`Self::with_remote_transport`].
    ///
    /// Local decisions never reach this — the dispatcher handles
    /// them inline against the owned `registry` + `interceptors` +
    /// TS bridge.
    remote_transport: Arc<dyn Transport>,
}

impl CommandExecutor {
    pub fn new(registry: Arc<ModuleRegistry>) -> Self {
        Self {
            registry,
            interceptors: Vec::new(),
            bus: None,
            policy: Arc::new(crate::routing::AllowAllPolicy),
            remote_transport: Arc::new(crate::routing::NotImplementedRemoteTransport),
        }
    }

    /// Replace the auth policy. Defaults to [`AllowAllPolicy`];
    /// operators install an ORM-backed or capability-backed impl
    /// here at boot. Builder-style so it chains with `new()`,
    /// `with_interceptor()`, `with_message_bus()`.
    pub fn with_policy(mut self, policy: Arc<dyn crate::routing::AuthPolicy>) -> Self {
        self.policy = policy;
        self
    }

    /// Replace the cross-grid transport. Defaults to
    /// [`NotImplementedRemoteTransport`](crate::routing::NotImplementedRemoteTransport).
    /// Operators / boot wire `AircTransport` (or a test
    /// [`ClosureTransport`](crate::routing::ClosureTransport)) here.
    ///
    /// Builder-style for chaining with the rest of the
    /// `CommandExecutor::new(...)...` setup.
    pub fn with_remote_transport(mut self, transport: Arc<dyn Transport>) -> Self {
        self.remote_transport = transport;
        self
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

    /// The wired message bus, if any. The Events primitive the executor
    /// composes with Commands at dispatch. Ingress-adjacent subscribers
    /// that need the live event stream (e.g. the positron chat
    /// projection in `ipc::positron_source`, which subscribes to
    /// `chat:*`/`presence:*` and stores the projected view into the
    /// thin-client `Substrate`) take a clone through this accessor
    /// rather than reaching into the kernel's internals. `None` when no
    /// bus is wired (headless test executors) — callers fail loud on
    /// their own precondition rather than this defaulting a bus.
    pub fn message_bus(&self) -> Option<Arc<MessageBus>> {
        self.bus.clone()
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
        self.execute_with_caller(command, params, None).await
    }

    /// Execute a command on behalf of a specific caller identity.
    ///
    /// Local code invoking `execute()` passes `None` — substrate's
    /// own code is implicitly trusted by the default AuthPolicy.
    /// Cross-grid handlers (the peer-side `CommandRequestHandler`)
    /// pass `Some(CallerIdentity::airc(verified_sender))` so the
    /// local AuthPolicy gate sees who the remote caller actually
    /// is and applies the right policy.
    ///
    /// Same dispatch chain, same observability, same routing
    /// decision — only the caller identity threaded into the gate
    /// changes.
    pub async fn execute_with_caller(
        &self,
        command: impl Into<CommandUri>,
        params: Value,
        caller: Option<crate::routing::CallerIdentity>,
    ) -> Result<CommandResult, String> {
        let command: CommandUri = command.into();
        let start = std::time::Instant::now();
        let outcome = self.dispatch(&command, params, caller.as_ref()).await;
        self.emit_command_completed(
            command.path(),
            &outcome,
            start.elapsed().as_millis() as u64,
            None, // synchronous: the caller holds the return value
        );
        outcome
    }

    /// Fire a command in the BACKGROUND: return a handle (UUID) immediately, run the
    /// dispatch on a spawned task, and on completion emit `command:completed` carrying the
    /// handle AND the result. A subscriber — e.g. a persona that sent a sentinel, a
    /// compile, or a debugger away — matches the completion by handle and folds the outcome
    /// in when it lands, never blocking the turn. This is the fire-and-poll shape (#86): the
    /// caller does not await the work. The handle is reusable in a follow-up command
    /// (cancel/query/attach), which is why it's a plain UUID ([[commands-are-agency-algs-are-pathways]]).
    pub fn dispatch_background(
        self: &std::sync::Arc<Self>,
        command: impl Into<CommandUri>,
        params: Value,
        caller: Option<crate::routing::CallerIdentity>,
    ) -> uuid::Uuid {
        let handle = uuid::Uuid::new_v4();
        let command: CommandUri = command.into();
        let this = std::sync::Arc::clone(self);
        tokio::spawn(async move {
            let start = std::time::Instant::now();
            let outcome = this.dispatch(&command, params, caller.as_ref()).await;
            this.emit_command_completed(
                command.path(),
                &outcome,
                start.elapsed().as_millis() as u64,
                Some(handle),
            );
        });
        handle
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
        caller: Option<&crate::routing::CallerIdentity>,
    ) -> Result<CommandResult, String> {
        let decision = route(command);
        // Local in-process dispatches pass `None` — substrate's own
        // code calling itself, implicitly trusted by the default
        // policy. Cross-grid handlers thread
        // `Some(CallerIdentity::airc(verified_sender))` through
        // `execute_with_caller` so the gate sees the real remote
        // caller.
        let verdict = self.policy.gate(&decision, caller);
        let span = tracing::info_span!(
            "cmd",
            uri = %command,
            path = %command.path(),
            route_kind = decision.kind().as_str(),
            verdict = verdict.kind(),
        );
        async move {
            // Slice P "every URI has a gate" — auth runs BEFORE the
            // transport match. Forbidden / Deferred short-circuit
            // with typed errors carrying the reason; only Allowed
            // proceeds to transport selection.
            match verdict {
                Verdict::Forbidden { reason } => {
                    return Err(format!("forbidden: {reason}"));
                }
                Verdict::Deferred {
                    reason,
                    prompt_target_env,
                } => {
                    return Err(format!(
                        "deferred: {reason:?} — consent prompt routed to env={prompt_target_env}"
                    ));
                }
                Verdict::Allowed => {}
            }

            // Slice P transport seam: Local handled inline against
            // this substrate's owned modules; every other variant
            // routes through the remote Transport trait. When the
            // AircTransport commit lands, swapping `remote_transport`
            // is the only change needed — this match shape doesn't
            // move.
            match decision {
                RouteDecision::Local { path, .. } => {
                    self.execute_inner(&path, params, caller).await
                }
                non_local => self.remote_transport.dispatch(non_local, params).await,
            }
        }
        .instrument(span)
        .await
    }

    /// The dispatch chain itself. Extracted so `execute` can wrap it
    /// with timing + event emission without burying the routing
    /// logic in instrumentation.
    ///
    /// PR #1529 reviewer 2: `caller` threaded through so interceptors
    /// see the same identity the gate already saw. Closes the silent-
    /// privilege-escalation seam where a remote `airc://this-peer/...`
    /// dispatch would reach AircInterceptor/GridInterceptor as if it
    /// were a local in-process invocation.
    async fn execute_inner(
        &self,
        command: &str,
        params: Value,
        caller: Option<&crate::routing::CallerIdentity>,
    ) -> Result<CommandResult, String> {
        let log = super::logger("command-executor");

        // 1. Walk the interceptor chain. First Handle wins. Decline
        //    moves on. Err propagates immediately — no silent
        //    fallthrough, per the trait contract.
        for interceptor in &self.interceptors {
            match interceptor.try_route(command, &params, caller).await {
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

        // 2. Typed path wins: a registered DynCommand object routes DIRECTLY —
        //    O(1) lock-free map lookup, no prefix scan, no per-module match arm.
        //    A migrated command lives here and beats its module's legacy
        //    handle_command arm; see docs/architecture/COMMAND-ORGANIZATION.md.
        if let Some(cmd) = self.registry.route_object(command) {
            log.debug(&format!(
                "Routing '{}' to DynCommand object (typed path)",
                command
            ));
            // Thread the gated caller into the command's Ctx — the SAME identity
            // the policy gate just saw (persona / cross-grid airc sender), so the
            // handler can gate/scope/compose by identity.
            return super::runtime::dispatch_object_with_panic_guard(cmd, params, caller.cloned())
                .await;
        }

        // 3. Fallback: prefix-routed local Rust module registry (un-migrated
        //    commands still flow through the module's handle_command match).
        if let Some((module, cmd)) = self.registry.route_command(command) {
            log.debug(&format!("Routing '{}' to local Rust module", command));
            let module_name = module.config().name;
            // catch_unwind guard — same shape `Runtime::route_command`
            // uses. Persona tool execution flows through this path; a
            // panicking handler converts to typed Err instead of
            // poisoning the caller's task.
            return super::runtime::dispatch_with_panic_guard(&module, &cmd, params, module_name)
                .await;
        }

        // 4. No DynCommand object and no Rust module owns this command.
        //    Refuse to silently route to the TS bridge — that path was the
        //    [[no-fallbacks-ever]] violation flagged as task #219. A
        //    substrate that silently routes unmigrated commands to
        //    `CommandRouterServer` appears "broken in headless mode"
        //    the day the operator forgets to bring up the TS host,
        //    when the real bug was the silent dependency. Surface a
        //    typed `CommandNotFound` error naming the command + the
        //    explicit escape hatch.
        //
        //    Callers that KNOW their command is TS-only use
        //    `execute_ts_json` (or `execute_ts`) directly — those
        //    public methods stay live for the documented TS-only
        //    call sites.
        log.warn(&format!(
            "no Rust module handles command '{command}' — refusing silent TS fallthrough"
        ));
        Err(format!(
            "no Rust module handles command: '{command}'. \
             The implicit TS-bridge fallthrough is disabled per \
             [[no-fallbacks-ever]]. If this command is intentionally \
             implemented in TypeScript, the caller must invoke \
             `CommandExecutor::execute_ts_json` (or `execute_ts`) \
             explicitly. If this command should be in Rust, register a \
             `ServiceModule` whose `command_prefixes` covers it."
        ))
    }

    /// Publish a `command:completed` event on the bus (when wired).
    /// Fire-and-forget — never blocks the caller, never panics if
    /// the bus has no subscribers. Telemetry path, not contract.
    fn emit_command_completed(
        &self,
        command: &str,
        outcome: &Result<CommandResult, String>,
        duration_ms: u64,
        handle: Option<uuid::Uuid>,
    ) {
        let Some(bus) = self.bus.as_ref() else {
            return;
        };
        // The result rides the event ONLY for a tracked background dispatch (handle set),
        // so the dispatcher learns the outcome from the event itself — no second call. Sync
        // commands stay thin: the caller already holds the return value.
        let result = handle
            .and(outcome.as_ref().ok())
            .and_then(|r| r.to_json_value().ok());
        let event = CommandCompletedEvent {
            command_name: command.to_string(),
            duration_ms,
            success: outcome.is_ok(),
            error: outcome.as_ref().err().cloned(),
            handle,
            result,
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

// GLOBAL_EXECUTOR + init_executor* + executor() + execute_command* +
// execute_ts* free functions were deleted in task #224 — the
// substrate refactor that eliminated the OnceLock + panicking
// accessor pattern. Modules that need to dispatch commands now hold
// an `Arc<CommandExecutor>` explicitly, installed at construction or
// via `install_executor()` after the executor is built.
//
// The dispatch entry points still exist as METHODS on `CommandExecutor`
// itself (`execute`, `execute_json`, `execute_ts`, `execute_ts_json`)
// — see the impl block above. Production code calls
// `self.executor.execute(...)` directly on its stored Arc instead of
// the deleted free helpers.
//
// Why removed:
//   - The global `OnceLock<Arc<CommandExecutor>>` made "is X
//     initialized before Y" an unsolvable-by-types property. Today's
//     PR #1568 round-1 BLOCK was caused by exactly this: eager
//     `executor()` lookup at PIM construction panicked because
//     `init_executor` ran 265 lines later in start_server.
//   - `[[no-fallbacks-ever]]`: the panic accessor swapped for the
//     correct shape, where the type system enforces "if you have
//     `Arc<CommandExecutor>`, the executor exists."
//   - `[[headless-success-is-personas-talking-over-airc]]`: the
//     deleted `execute_ts*` free helpers were the silent-fallback
//     into the legacy TS bridge for unmigrated commands. The
//     `CommandExecutor::execute_ts*` methods remain (the bridge
//     itself isn't dead yet) but every call site is now an explicit
//     `executor.execute_ts(...)` — a legible smell that task #219's
//     follow-up slices can pick off one command at a time.
//
// Construction pattern (see ipc/mod.rs::start_server):
//   1. Build `Arc<ModuleRegistry>`
//   2. Register every ServiceModule that DOESN'T need the executor
//   3. Build `Arc<CommandExecutor>` with the registry + interceptors
//   4. Register modules that need the executor, threading the Arc
//      into their constructor (or call `install_executor()` on
//      already-registered modules)
//
// Pure dependency injection. No global. No panic-if-uninitialized.

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
        let executor =
            CommandExecutor::new(registry).with_interceptor(Arc::new(AircInterceptor::new()));
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
            _caller: Option<&crate::routing::CallerIdentity>,
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
            _caller: Option<&crate::routing::CallerIdentity>,
        ) -> Result<InterceptorOutcome, String> {
            Ok(InterceptorOutcome::Handled(CommandResult::Json(
                serde_json::json!({ "handled": true }),
            )))
        }

        fn name(&self) -> &'static str {
            "always-handle"
        }
    }

    // what this catches: the TYPED PATH end-to-end through the REAL executor —
    // `ping` (migrated to a DynCommand object via ActionCommand) dispatches all the
    // way through CommandExecutor::execute, which consults the registry's object map
    // (step 2) BEFORE any prefix routing, and returns the bare PingResult. This is
    // the integration proof that the per-module match arm is gone for ping and the
    // self-routing object map serves it. Pure headless Rust — no Node, no socket,
    // no npm start.
    #[tokio::test]
    async fn ping_dispatches_through_executor_via_typed_object_path() {
        let registry = Arc::new(ModuleRegistry::new());
        registry.register(Arc::new(crate::modules::health::HealthModule::new()));
        let executor = CommandExecutor::new(registry);

        let result = executor
            .execute("ping", serde_json::json!({}))
            .await
            .expect("ping dispatches through the executor");
        match result {
            CommandResult::Json(v) => {
                assert_eq!(v["ok"], true, "ping returned the bare PingResult");
                assert!(
                    v.get("success").is_none(),
                    "Bare wire — no envelope wrapping on the typed path"
                );
            }
            other => panic!("expected Json, got {other:?}"),
        }
    }

    // what this catches: NO ESCALATION THROUGH COMPOSITION, via a REAL composing
    // handler (not just the gate). `Composer` is an ActionCommand whose `run`
    // composes a sub-command with `ctx.caller.clone()` — exactly the pattern a
    // dep-holding command uses. Invoked as the local owner the sub-call passes the
    // gate; invoked as an airc/Provisional caller the propagated identity is gated,
    // so it CANNOT reach the Owner-only `data/delete`. This pins that identity flows
    // through composition (and, by the same mechanism, across the grid), never
    // escalating — the guarantee the COMMAND-ORGANIZATION doc claims.
    #[tokio::test]
    async fn composing_handler_propagates_ctx_caller_no_escalation() {
        use crate::routing::{CallerIdentity, GridTrustAuthPolicy};
        use crate::sdk_codegen::{ActionCommand, CommandError, Ctx};

        #[derive(
            Default, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema,
        )]
        struct NoParams {}
        #[derive(serde::Serialize, serde::Deserialize, ts_rs::TS)]
        struct Out {
            forbidden: bool,
        }

        // A command that COMPOSES another, propagating its own caller (ctx.caller).
        struct Composer {
            exec: Arc<CommandExecutor>,
        }
        #[async_trait]
        impl ActionCommand for Composer {
            const NAME: &'static str = "test/composer";
            type Params = NoParams;
            type Output = Out;
            async fn run(&self, ctx: &Ctx, _p: NoParams) -> Result<Out, CommandError> {
                let r = self
                    .exec
                    .execute_with_caller(
                        "data/delete",
                        Value::Object(Default::default()),
                        ctx.caller.clone(),
                    )
                    .await;
                Ok(Out {
                    forbidden: r
                        .as_ref()
                        .err()
                        .map(|e| e.contains("forbidden"))
                        .unwrap_or(false),
                })
            }
        }

        let registry = Arc::new(ModuleRegistry::new());
        let exec = Arc::new(
            CommandExecutor::new(registry).with_policy(Arc::new(GridTrustAuthPolicy::new())),
        );
        let composer = Composer { exec: exec.clone() };

        // Composed as the local owner (ctx.caller None) → sub-call NOT gate-forbidden.
        let owner = composer.run(&Ctx::default(), NoParams {}).await.unwrap();
        assert!(
            !owner.forbidden,
            "owner composing data/delete is not forbidden"
        );

        // Composed as an airc/Provisional caller → identity propagated → FORBIDDEN.
        let airc_ctx = Ctx {
            caller: Some(CallerIdentity::airc(crate::identity::PeerId::new())),
            ..Default::default()
        };
        let escalated = composer.run(&airc_ctx, NoParams {}).await.unwrap();
        assert!(
            escalated.forbidden,
            "airc composing data/delete must be forbidden — handler propagated ctx.caller, no escalation"
        );
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
        // existing local-Rust commands that don't carry airc routing
        // params. This is the back-compat guarantee that lets the airc
        // interceptor be safely installed at init_executor.
        //
        // Without a registered Rust module for "test/cmd", the executor
        // walks the interceptor chain (airc Declines because no
        // aircPeer in params), then the registry (no match), then
        // returns CommandNotFound per [[no-fallbacks-ever]] (task #219).
        // The test is asserting the airc interceptor did NOT
        // short-circuit — the failure source MUST be the registry miss,
        // NOT the airc interceptor.
        let registry = Arc::new(ModuleRegistry::new());
        let executor =
            CommandExecutor::new(registry).with_interceptor(Arc::new(AircInterceptor::new()));

        let result = executor
            .execute("test/cmd", serde_json::json!({ "ordinaryParam": "value" }))
            .await;

        // Failure must be the CommandNotFound shape (no Rust module),
        // not the airc interceptor short-circuiting. The substring
        // "no Rust module handles command" is the contract we depend on
        // — if the airc interceptor had wrongly intercepted, the error
        // would mention "airc" instead.
        let err = result.expect_err("command has no Rust handler; expect typed error");
        assert!(
            err.contains("no Rust module handles command"),
            "error must come from registry miss (the no-fallbacks surface), \
             not from airc interceptor. err: {err}"
        );
        assert!(
            !err.contains("aircPeer") && !err.contains("airc interceptor"),
            "error must NOT mention airc routing — proves the airc \
             interceptor declined cleanly. err: {err}"
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
    // No-fallbacks contract (task #219)
    // ════════════════════════════════════════════════════════════════

    // what this catches: a command not handled by any interceptor and
    // not registered in the Rust module registry returns a typed
    // `CommandNotFound`-shaped error per [[no-fallbacks-ever]]. Pre-
    // PR #1585 the executor silently routed to the TS bridge on
    // `/tmp/jtag-command-router.sock`, which in headless mode
    // surfaced as a cryptic "Failed to connect" error and in
    // hybrid-host mode silently delegated to TS — both breaking the
    // mental model. The fix: refuse the implicit fallthrough,
    // surface a typed error that names the missing command and the
    // explicit escape hatch (`execute_ts_json`).
    #[tokio::test]
    async fn unknown_command_returns_typed_no_fallback_error_not_ts_attempt() {
        let registry = Arc::new(ModuleRegistry::new());
        let executor = CommandExecutor::new(registry);

        let err = executor
            .execute("totally/made-up/command", Value::Null)
            .await
            .expect_err("unknown command must produce a typed error");

        // Must NOT mention the TS socket or "CommandRouterServer" —
        // those strings would prove the implicit fallthrough was
        // still alive. Substring assertions over exact-string
        // matches so we don't pin too tightly on phrasing.
        assert!(
            !err.contains("CommandRouterServer"),
            "error must NOT attempt the TS fallthrough: {err}"
        );
        assert!(
            !err.contains("/tmp/jtag-command-router.sock"),
            "error must NOT mention the TS socket path: {err}"
        );

        // MUST name the missing command so operators know what to
        // migrate or remove.
        assert!(
            err.contains("totally/made-up/command"),
            "error must name the missing command: {err}"
        );
        // MUST point at the explicit escape hatch so a caller that
        // genuinely meant to hit TS knows what method to call.
        assert!(
            err.contains("execute_ts_json") || err.contains("execute_ts"),
            "error must point at the explicit TS-bridge API: {err}"
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
        async fn initialize(&self, _ctx: &crate::runtime::ModuleContext) -> Result<(), String> {
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
        serde_json::from_value((*recv.payload).clone()).expect("event payload must parse")
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
        // A synchronous dispatch stays thin: no handle, no result on the event (the caller
        // already holds the return value).
        assert_eq!(
            event.handle, None,
            "sync command carries no dispatch handle"
        );
        assert_eq!(
            event.result, None,
            "sync command's result is not duplicated onto the event"
        );
    }

    // what this catches: dispatch_background returns a handle IMMEDIATELY (fire-and-poll)
    // and its completion event carries BOTH the handle and the result — so a subscriber
    // (a persona that sent a sentinel away) matches the completion to its dispatch and
    // folds the outcome in without a second call. This is the producer for the WM async
    // recency channel.
    #[tokio::test]
    async fn dispatch_background_completion_carries_handle_and_result() {
        let registry = Arc::new(ModuleRegistry::new());
        registry.register(Arc::new(CannedModule {
            canned: Ok(serde_json::json!({ "built": true, "warnings": 0 })),
        }));
        let bus = Arc::new(MessageBus::new());
        let mut rx = bus.receiver();
        let executor = Arc::new(CommandExecutor::new(registry).with_message_bus(bus));

        // Fire in the background — returns a handle immediately, does not await the work.
        let handle = executor.dispatch_background("canned/ping", serde_json::json!({}), None);

        let event = next_command_completed(&mut rx).await;
        assert_eq!(event.command_name, "canned/ping");
        assert!(event.success);
        assert_eq!(
            event.handle,
            Some(handle),
            "completion carries the dispatch handle"
        );
        assert_eq!(
            event.result,
            Some(serde_json::json!({ "built": true, "warnings": 0 })),
            "completion carries the result — no second call needed"
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
        let r = executor.execute("canned/ping", serde_json::json!({})).await;
        assert!(r.is_ok());
    }

    #[tokio::test]
    async fn no_rust_module_failure_still_emits_completed_event() {
        // When all dispatch tiers fail (no interceptor handled, no
        // Rust module registered, no implicit fallthrough per task
        // #219) — the event should still emit with success=false +
        // the typed CommandNotFound error. Telemetry must cover
        // every dispatch path's terminal state, including the new
        // no-fallback surface.
        let registry = Arc::new(ModuleRegistry::new());
        let bus = Arc::new(MessageBus::new());
        let mut rx = bus.receiver();
        let executor = CommandExecutor::new(registry).with_message_bus(bus);

        let err = executor
            .execute("nonexistent/command", serde_json::json!({}))
            .await
            .expect_err("unknown command produces typed error");
        // Don't pin specific error text here; just confirm it's an
        // Err. The dedicated `unknown_command_returns_typed_no_fallback_error_not_ts_attempt`
        // test pins the exact contract.
        let _ = err;

        let event = next_command_completed(&mut rx).await;
        assert_eq!(event.command_name, "nonexistent/command");
        assert!(!event.success);
        assert!(
            event.error.is_some(),
            "no-fallback failure path must populate error: {event:?}"
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
                        serde_json::from_value((*event.payload).clone()).expect("payload parses");
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

    // ─── Slice P: auth policy gate ──────────────────────────────────

    /// The policy gate runs BEFORE the dispatcher's transport match —
    /// a Forbidden verdict short-circuits to a typed error without
    /// hitting any interceptor or module. Proves the chokepoint
    /// behavior the design doc names: "every URI has a gate."
    #[tokio::test]
    async fn forbidden_policy_short_circuits_before_local_dispatch() {
        let later_called = Arc::new(AtomicUsize::new(0));
        let registry = Arc::new(ModuleRegistry::new());
        let executor = CommandExecutor::new(registry)
            .with_policy(Arc::new(crate::routing::deny_path_prefix("forbidden/")))
            // An interceptor that would normally short-circuit — the
            // policy should reject the request BEFORE it gets here.
            .with_interceptor(Arc::new(RecordingDecliner {
                name: "should-never-run",
                seen: later_called.clone(),
                mark: 42,
            }));

        let err = executor
            .execute("forbidden/some-op", Value::Null)
            .await
            .expect_err("forbidden policy must reject the dispatch");
        assert!(
            err.contains("forbidden"),
            "error must name the forbidden verdict, got: {err}"
        );
        assert!(
            err.contains("forbidden/some-op"),
            "error must name the URI path that was denied, got: {err}"
        );
        assert_eq!(
            later_called.load(Ordering::SeqCst),
            0,
            "interceptors must NOT be consulted after a Forbidden verdict"
        );
    }

    /// A Deferred verdict also short-circuits — the dispatcher's
    /// error names the prompt target env so the operator knows where
    /// consent will be routed (once the consent transport lands).
    #[tokio::test]
    async fn deferred_policy_short_circuits_with_target_env_in_error() {
        let registry = Arc::new(ModuleRegistry::new());
        let executor = CommandExecutor::new(registry).with_policy(Arc::new(
            crate::routing::defer_path_prefix(
                "persona/state/",
                crate::routing::EnvironmentId::Named("web".into()),
            ),
        ));

        let err = executor
            .execute("persona/state/mutate", Value::Null)
            .await
            .expect_err("deferred policy returns a typed error");
        assert!(
            err.contains("deferred"),
            "error must name the deferred verdict, got: {err}"
        );
        assert!(
            err.contains("web"),
            "error must name the consent target env, got: {err}"
        );
    }

    /// Proves the Transport trait extraction is wired correctly: a
    /// Peer URI flows through `route()` → policy gate → the
    /// installed remote Transport. When AircTransport lands, it
    /// slots into the same call site and personas talk
    /// cross-machine.
    #[tokio::test]
    async fn peer_uri_routes_through_installed_remote_transport() {
        use crate::routing::{ClosureTransport, RouteDecision};
        use std::sync::Mutex;

        let captured_path: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let captured_path_clone = captured_path.clone();

        let transport =
            ClosureTransport::new(
                "test-peer-transport",
                move |decision, _params| match &decision {
                    RouteDecision::Peer { path, .. } => {
                        *captured_path_clone.lock().unwrap() = Some(path.clone());
                        Ok(CommandResult::Json(serde_json::json!({
                            "routed-through": "test-peer-transport",
                        })))
                    }
                    other => panic!("expected Peer, got {other:?}"),
                },
            );

        let registry = Arc::new(ModuleRegistry::new());
        let executor = CommandExecutor::new(registry).with_remote_transport(Arc::new(transport));

        let result = executor
            .execute("airc://maya/inference/llm/generate", Value::Null)
            .await
            .expect("transport routes the peer URI");

        match result {
            CommandResult::Json(v) => {
                assert_eq!(v["routed-through"], "test-peer-transport");
            }
            other => panic!("expected Json, got {other:?}"),
        }
        assert_eq!(
            captured_path.lock().unwrap().as_deref(),
            Some("inference/llm/generate"),
            "transport must receive the parsed Peer path"
        );
    }

    /// AllowAllPolicy (the default) is transparent — no behavior
    /// difference from a substrate without a gate. Proves the
    /// retrofit doesn't break existing call sites.
    #[tokio::test]
    async fn default_policy_lets_dispatch_through() {
        let later_called = Arc::new(AtomicUsize::new(0));
        let registry = Arc::new(ModuleRegistry::new());
        let executor = CommandExecutor::new(registry)
            // No with_policy call — AllowAllPolicy is the default
            .with_interceptor(Arc::new(RecordingDecliner {
                name: "must-run",
                seen: later_called.clone(),
                mark: 99,
            }))
            .with_interceptor(Arc::new(AlwaysHandle));

        let _ = executor
            .execute("anything", Value::Null)
            .await
            .expect("default policy allows dispatch");
        assert_eq!(
            later_called.load(Ordering::SeqCst),
            99,
            "decliner must have been consulted (proves policy was Allow)"
        );
    }
}
