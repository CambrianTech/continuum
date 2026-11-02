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

/// Whether a failed dispatch can have had an effect. A caller that retries,
/// skips, or reports "nothing happened" must know which; the message alone
/// cannot say.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DispatchOutcome {
    /// Refused before any handler ran: the AuthPolicy gate (Forbidden or
    /// Deferred) or no route for the command. Nothing was attempted.
    NotCommitted,
    /// A handler, interceptor or transport was reached and failed. It may have
    /// acted before failing. Every handler error lands here, whatever its
    /// `CommandError` category: no handler contract yet promises "refused
    /// before effect" (Codex root review, 2026-10-13).
    Uncertain,
}

/// A failed dispatch: the message every caller has always read, plus the
/// [`DispatchOutcome`] decided where the failure happened, never by parsing
/// the message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DispatchFailure {
    pub outcome: DispatchOutcome,
    pub message: String,
}

impl DispatchFailure {
    pub(crate) fn not_committed(message: String) -> Self {
        Self { outcome: DispatchOutcome::NotCommitted, message }
    }

    fn uncertain(message: String) -> Self {
        Self { outcome: DispatchOutcome::Uncertain, message }
    }
}

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
    ///
    /// SHARED with the runtime's socket route (`Runtime::interceptor_chain`) so a
    /// registered interceptor applies on every route into the node, not only
    /// in-process calls (card 506a388c).
    interceptors: Arc<std::sync::RwLock<Vec<Arc<dyn CommandInterceptor>>>>,
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
            interceptors: Arc::new(std::sync::RwLock::new(Vec::new())),
            bus: None,
            policy: Arc::new(crate::routing::AllowAllPolicy),
            remote_transport: Arc::new(crate::routing::NotImplementedRemoteTransport),
        }
    }

    /// The trust this executor's policy resolves for `caller`: the same policy its gate uses
    /// ([`AuthPolicy::caller_trust`](crate::routing::AuthPolicy::caller_trust)). `None` when the
    /// policy cannot resolve it, which an owner-only handler treats as a refusal.
    pub fn caller_trust(&self, caller: Option<&crate::routing::CallerIdentity>) -> Option<crate::routing::trust_level::TrustLevel> {
        self.policy.caller_trust(caller)
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
    pub fn with_interceptor(self, interceptor: Arc<dyn CommandInterceptor>) -> Self {
        self.interceptors
            .write()
            .unwrap_or_else(|e| e.into_inner()) // unwrap_or_else: a poisoned lock still holds the list; keep appending
            .push(interceptor);
        self
    }

    /// Share ONE interceptor chain with the runtime's socket route
    /// ([`super::Runtime::interceptor_chain`]) — the compression that ends the
    /// "half the contract on each dispatcher" split (card 506a388c).
    pub fn with_interceptor_chain(
        mut self,
        chain: Arc<std::sync::RwLock<Vec<Arc<dyn CommandInterceptor>>>>,
    ) -> Self {
        self.interceptors = chain;
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
        self.interceptors
            .read()
            .unwrap_or_else(|e| e.into_inner()) // unwrap_or_else: a poisoned lock still holds the list; count it
            .len()
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

    /// The module registry this executor dispatches into — how a composing
    /// service finds the module that owns its state (the recipe executor finds
    /// the node's run store this way).
    pub fn registry(&self) -> &Arc<ModuleRegistry> {
        &self.registry
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
        self.execute_classified(command, params, caller)
            .await
            .map_err(|failure| failure.message)
    }

    /// As [`Self::execute_with_caller`], and a failure says whether the command
    /// can have had an effect ([`DispatchFailure::outcome`]). A caller that may
    /// retry or skip (the recipe executor) needs that answer; every other caller
    /// reads only the message.
    pub async fn execute_classified(
        &self,
        command: impl Into<CommandUri>,
        params: Value,
        caller: Option<crate::routing::CallerIdentity>,
    ) -> Result<CommandResult, DispatchFailure> {
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
            // The caller's hands are bound and admitted inside dispatch, for this task's whole
            // life: a release that began before its first poll refuses it by name.
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
    ) -> Result<CommandResult, DispatchFailure> {
        // A caller bound to a context (a card claim's turn, an eval, a fork) is ADMITTED to its
        // hands for this command's whole life, foreground or background: a release refuses
        // what starts after it and waits out what is admitted (card fc9e9fd7; Astra on #5026:
        // admission at the operation's lifetime boundary, never in helpers). The release itself
        // is the one operation not admitted: it closes the hands to every other.
        // Around-dispatch concerns (a caller's hands admission, the cognition pulse's work
        // accounting) are installed by their owners at boot (`dispatch_hook`); one refusal
        // ends the dispatch with nothing committed.
        let scopes = super::dispatch_hook::enter_all(caller).map_err(DispatchFailure::not_committed)?;
        let in_command = scopes.clone();
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
        let command = async move {
            // Slice P "every URI has a gate" — auth runs BEFORE the
            // transport match. Forbidden / Deferred short-circuit
            // with typed errors carrying the reason; only Allowed
            // proceeds to transport selection.
            match verdict {
                Verdict::Forbidden { reason } => {
                    return Err(DispatchFailure::not_committed(format!("forbidden: {reason}")));
                }
                Verdict::Deferred {
                    reason,
                    prompt_target_env,
                } => {
                    return Err(DispatchFailure::not_committed(format!(
                        "deferred: {reason:?} — consent prompt routed to env={prompt_target_env}"
                    )));
                }
                Verdict::Allowed => {}
            }

            // Slice P transport seam: Local handled inline against
            // this substrate's owned modules; every other variant
            // routes through the remote Transport trait. When the
            // AircTransport commit lands, swapping `remote_transport`
            // is the only change needed — this match shape doesn't
            // move.
            for scope in &in_command {
                scope.allowed();
            }
            let outcome = match decision {
                RouteDecision::Local { path, .. } => {
                    self.execute_inner(&path, params, caller).await
                }
                non_local => self
                    .remote_transport
                    .dispatch(non_local, params)
                    .await
                    .map_err(DispatchFailure::uncertain),
            };
            if outcome.is_ok() {
                for scope in &in_command {
                    scope.succeeded();
                }
            }
            outcome
        }
        .instrument(span);
        // The command runs inside every scope (the hands' admission is a task-local its
        // children inherit until their tree has ended: `continuum_process::run_owned`).
        super::dispatch_hook::wrap_all(&scopes, Box::pin(command)).await
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
    ) -> Result<CommandResult, DispatchFailure> {
        let log = super::logger("command-executor");

        // 1. Walk the interceptor chain. First Handle wins. Decline
        //    moves on. Err propagates immediately — no silent
        //    fallthrough, per the trait contract.
        let chain: Vec<Arc<dyn CommandInterceptor>> = self
            .interceptors
            .read()
            .unwrap_or_else(|e| e.into_inner()) // unwrap_or_else: a poisoned lock still holds the list; read it
            .clone();
        for interceptor in &chain {
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
                    return Err(DispatchFailure::uncertain(e));
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
                .await
                .map_err(DispatchFailure::uncertain);
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
                .await
                .map_err(DispatchFailure::uncertain);
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
        Err(DispatchFailure::not_committed(format!(
            "no Rust module handles command: '{command}'. \
             The implicit TS-bridge fallthrough is disabled per \
             [[no-fallbacks-ever]]. If this command is intentionally \
             implemented in TypeScript, the caller must invoke \
             `CommandExecutor::execute_ts_json` (or `execute_ts`) \
             explicitly. If this command should be in Rust, register a \
             `ServiceModule` whose `command_prefixes` covers it."
        )))
    }

    /// Publish a `command:completed` event on the bus (when wired).
    /// Fire-and-forget — never blocks the caller, never panics if
    /// the bus has no subscribers. Telemetry path, not contract.
    fn emit_command_completed(
        &self,
        command: &str,
        outcome: &Result<CommandResult, DispatchFailure>,
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
            error: outcome.as_ref().err().map(|failure| failure.message.clone()),
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
mod tests;
