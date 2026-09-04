//! ServiceModule — the ONE trait every module implements.
//!
//! Inspired by CBAR's QueueThread<T>: implement handleItem(), register, done.
//! Each module declares what commands it handles and what events it subscribes to.
//! The runtime auto-wires routing from these declarations.
//!
//! Adding a new module to the system:
//! 1. Implement ServiceModule
//! 2. runtime.register(Arc::new(MyModule::new()))
//! 3. Done. Commands route automatically.

use super::artifact_handle::{ArtifactKey, ArtifactSelector, Cadence};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::any::Any;
use std::time::Duration;
use ts_rs::TS;

// ============================================================================
// Command Schema Types (for MCP tool discovery)
// ============================================================================

/// Schema for a command parameter (for MCP tool discovery).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamSchema {
    /// Parameter name
    pub name: &'static str,
    /// JSON Schema type: "string", "number", "boolean", "object", "array"
    pub param_type: &'static str,
    /// Whether this parameter is required
    pub required: bool,
    /// Description for documentation
    pub description: &'static str,
}

/// Schema for a command (for MCP tool discovery).
/// Used to dynamically generate MCP tool definitions at runtime.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandSchema {
    /// Full command name: "data/list", "voice/synthesize"
    pub name: &'static str,
    /// Human-readable description
    pub description: &'static str,
    /// Parameter definitions
    pub params: Vec<ParamSchema>,
}

/// Priority class for module scheduling.
/// Determines thread pool affinity and tick cadence.
/// Like CBAR's adaptive timeout: 10 + 100 * priority milliseconds.
///
/// Exposed to TypeScript via ts-rs for Ares (RTOS controller persona) to adjust priorities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/runtime/ModulePriority.ts"
)]
#[serde(rename_all = "lowercase")]
pub enum ModulePriority {
    /// Voice, audio — must complete within frame budget (~10ms)
    Realtime = 0,
    /// Cognition, channel scheduling — sub-10ms target
    High = 1,
    /// Code, file ops, data, search — 10-100ms acceptable
    Normal = 2,
    /// Training, archive, logging — seconds acceptable
    Background = 3,
}

/// Module configuration — declares capabilities and requirements.
/// Called ONCE at registration. Like CBP_AnalyzerThread's config hooks
/// (needsRealTime(), needsColorFrames(), etc.).
#[derive(Clone)]
pub struct ModuleConfig {
    /// Unique module name: "voice", "cognition", "code", "data", etc.
    pub name: &'static str,

    /// Priority class (determines tick cadence and thread affinity).
    pub priority: ModulePriority,

    /// Command prefixes this module handles.
    /// The registry routes commands matching these prefixes to this module.
    /// e.g., ["voice/"] routes "voice/synthesize" → VoiceModule
    pub command_prefixes: &'static [&'static str],

    /// Event glob patterns this module subscribes to.
    /// e.g., ["persona:state:*", "data:users:created"]
    pub event_subscriptions: &'static [&'static str],

    /// Whether this module needs a dedicated OS thread (like QueueThread).
    /// true = dedicated thread (voice, real-time audio)
    /// false = shares tokio work-stealing pool (most modules)
    pub needs_dedicated_thread: bool,

    /// Maximum concurrent requests. 0 = unlimited (module manages own concurrency).
    pub max_concurrency: usize,

    /// Optional periodic tick interval. When set, the runtime spawns a tokio task
    /// that calls `tick()` at this cadence. Overrides the default priority-based cadence.
    /// None = no periodic tick (module is purely reactive to commands).
    pub tick_interval: Option<Duration>,
}

/// Result of handling a command — one of the four cell return shapes
/// per [MODULE-ARCHITECTURE.md §5.1](../../../../../docs/architecture/MODULE-ARCHITECTURE.md).
///
/// See [`super::cell_shapes`] for the cell taxonomy + the rationale
/// for each variant. Short version:
///
/// - `Json` / `Binary` — the **Value** cell shape (immediate typed
///   result). Kept under their original names for back-compat with
///   the 300+ existing handlers; new code that produces a typed
///   result still uses `Json` (or `CommandResult::json(&value)?`).
/// - `Handle` — the **Handle** cell shape, NEW in this PR. Typed
///   reference to state owned by the producing module. See
///   [`super::cell_shapes::HandleRef`] for the round-trip protocol.
///   Answers MODULE-ARCHITECTURE.md §13.1 (hot-path cross-module
///   state via reference, not copy).
/// - `Stream` / `Lambda` — reserved cell shapes. Returning these
///   today is a runtime error per the contract — the variant exists
///   so the enum shape is fixed before the wire protocols land. See
///   [`super::cell_shapes::StreamPlaceholder`] and
///   [`super::cell_shapes::LambdaPlaceholder`].
///
/// # Adding to this enum
///
/// `#[non_exhaustive]` lets downstream crates match without breaking
/// when new variants land. Within continuum-core, exhaustive matches
/// MUST cover the new variants — the compiler enforces this. Use
/// [`CommandResult::to_json_value`] when the call site just needs the
/// payload as JSON regardless of which cell shape arrived.
#[derive(Debug)]
#[non_exhaustive]
pub enum CommandResult {
    /// Standard JSON response. The Value cell shape under the legacy
    /// name; preferred for new code that produces a typed result.
    Json(Value),

    /// Binary response: JSON metadata + raw bytes.
    /// Wire format: `[JSON header bytes][\0][raw binary bytes]`.
    /// Used for audio synthesis, embedding vectors, etc.
    Binary { metadata: Value, data: Vec<u8> },

    /// Typed reference to state owned by the producing module. See
    /// [`super::cell_shapes::HandleRef`] for the round-trip protocol.
    Handle(super::cell_shapes::HandleRef),

    /// Reserved: streaming result. Returning this today is a runtime
    /// error — see [`super::cell_shapes::StreamPlaceholder`] for the
    /// open protocol design.
    Stream(super::cell_shapes::StreamPlaceholder),

    /// Reserved: lambda (callable returned by a command). Returning
    /// this today is a runtime error — see
    /// [`super::cell_shapes::LambdaPlaceholder`] for the open protocol
    /// design.
    Lambda(super::cell_shapes::LambdaPlaceholder),
}

impl CommandResult {
    /// Create a Json result from any Serialize type.
    /// Eliminates the `serde_json::to_value(x).unwrap()` anti-pattern.
    pub fn json(value: &impl serde::Serialize) -> Result<Self, String> {
        serde_json::to_value(value)
            .map(CommandResult::Json)
            .map_err(|e| format!("Serialization error: {e}"))
    }

    /// Create a Handle result from a producer-allocated UUID.
    ///
    /// Use this when the producer minted a UUID up front to insert
    /// state into its own map under a specific key:
    ///
    /// ```ignore
    /// let id = uuid::Uuid::new_v4();
    /// self.sessions.insert(id, session_state);
    /// Ok(CommandResult::handle("ai/inference", id, "ai::InferenceSession"))
    /// ```
    ///
    /// For the simpler case where the producer doesn't need to know
    /// the UUID before constructing the handle, use
    /// [`super::cell_shapes::HandleRef::mint`] directly and wrap with
    /// `CommandResult::Handle(...)`.
    pub fn handle(owner: impl Into<String>, id: uuid::Uuid, type_tag: impl Into<String>) -> Self {
        CommandResult::Handle(super::cell_shapes::HandleRef::with_id(owner, id, type_tag))
    }

    /// Project the result into a JSON `Value` for callers that don't
    /// care about the cell shape — e.g., the TS bridge that wants to
    /// serialize the result over a Unix socket regardless of which
    /// cell shape the producer chose.
    ///
    /// `Json` returns itself. `Binary` returns its metadata (the
    /// bytes are dropped — callers needing the raw data must match
    /// on the variant directly). `Handle` serializes the HandleRef
    /// as JSON so a TS caller can hold it and pass it back. `Stream`
    /// and `Lambda` return errors per the not-yet-wired contract:
    /// projecting them as plain JSON would lose the protocol shape
    /// the caller needs to consume them, so we fail loud rather than
    /// silently degrade.
    pub fn to_json_value(&self) -> Result<Value, String> {
        match self {
            CommandResult::Json(v) => Ok(v.clone()),
            CommandResult::Binary { metadata, .. } => Ok(metadata.clone()),
            CommandResult::Handle(h) => {
                serde_json::to_value(h).map_err(|e| format!("HandleRef serialization failed: {e}"))
            }
            CommandResult::Stream(_) => Err(Self::stream_protocol_error()),
            CommandResult::Lambda(_) => Err(Self::lambda_protocol_error()),
        }
    }

    /// Canonical error message for handlers that try to return a Stream
    /// today. Surfaced from any callsite that needs to reject the
    /// not-yet-wired streaming variant — same wording everywhere so
    /// the failure mode is easy to grep.
    pub fn stream_protocol_error() -> String {
        "Stream cell shape is reserved but not yet wired — the streaming \
         wire protocol (frame format, correlation IDs, backpressure, \
         cancellation) hasn't been designed yet. Handlers MUST return \
         Json/Binary/Handle until the protocol lands. See \
         MODULE-ARCHITECTURE.md §5.1 + runtime::cell_shapes::StreamPlaceholder."
            .to_string()
    }

    /// Canonical error message for handlers that try to return a Lambda
    /// today. Same shape as [`Self::stream_protocol_error`].
    pub fn lambda_protocol_error() -> String {
        "Lambda cell shape is reserved but not yet wired — the lambda \
         invocation protocol (curried-command dispatch, bound-params \
         merge, return-shape propagation) hasn't been designed yet. \
         Handlers MUST return Json/Binary/Handle until the protocol \
         lands. See MODULE-ARCHITECTURE.md §5.1 + \
         runtime::cell_shapes::LambdaPlaceholder."
            .to_string()
    }
}

/// The ONE trait. Implement this and register — done.
///
/// Every module in the system implements ServiceModule. The runtime:
/// - Routes commands to the correct module based on command_prefixes
/// - Delivers events matching event_subscriptions
/// - Calls tick() at the module's priority-determined cadence
/// - Provides ModuleContext for inter-module communication
#[async_trait]
pub trait ServiceModule: Send + Sync + Any {
    /// Module configuration — declares what this module handles.
    /// Called ONCE at registration time.
    fn config(&self) -> ModuleConfig;

    /// Initialize the module. Called after registration, before any commands.
    /// The ModuleContext provides access to the registry (query other modules),
    /// the message bus (pub/sub), and the shared compute cache.
    async fn initialize(&self, ctx: &super::ModuleContext) -> Result<(), String>;

    /// Handle a command routed to this module.
    ///
    /// The full command name is passed (e.g., "voice/synthesize").
    /// Params is the full JSON request object.
    ///
    /// This is QueueThread<T>::handleItem() generalized to async request/response.
    async fn handle_command(&self, command: &str, params: Value) -> Result<CommandResult, String>;

    /// Handle an event published on the message bus.
    /// Only called for events matching event_subscriptions globs.
    ///
    /// Default behavior (PIECE-2 PR-3): auto-route to
    /// `on_artifact_available` when `event_name` matches one of this
    /// module's `artifact_subscriptions`. This is what makes the
    /// artifact dispatch path work without every module overriding
    /// `handle_event` manually — the runtime subscribes the module's
    /// artifact keys to the bus, the bus delivers via `handle_event`,
    /// and the default impl forwards to `on_artifact_available`.
    ///
    /// Modules with `event_subscriptions` (glob patterns on the bus
    /// that are NOT artifact keys) MUST override `handle_event` —
    /// otherwise a bus event matching their glob will be silently
    /// checked against `artifact_subscriptions` and dropped if it
    /// doesn't match. Overriding restores explicit control; from an
    /// override the module can still call
    /// `self.on_artifact_available(key, payload).await` to opt into
    /// the same auto-route behavior.
    async fn handle_event(&self, event_name: &str, payload: Value) -> Result<(), String> {
        let subs = self.artifact_subscriptions();
        if subs.is_empty() {
            return Ok(());
        }
        let key = ArtifactKey::from(event_name);
        if subs.iter().any(|sel| sel.matches(&key)) {
            return self.on_artifact_available(&key, payload).await;
        }
        Ok(())
    }

    /// Periodic tick — called at the module's priority-determined cadence.
    /// Like CBThread::tick() or RTOS periodic servicing.
    /// Default: no-op (most modules are purely reactive).
    async fn tick(&self) -> Result<(), String> {
        Ok(())
    }

    /// Self-adjusting priority (like CBAR's context-aware priority).
    /// Called periodically by the runtime. Return None to keep current priority.
    /// A module can detect context changes and adjust its own scheduling.
    fn adjusted_priority(&self) -> Option<ModulePriority> {
        None
    }

    /// SAVE this node's volatile state to its durable home — the explicit half
    /// of the CBAR contract (Joel 2026-09-02: "I can call all nodes and tell
    /// them to save or load state"). Broadcast by [`Runtime::shutdown`] before
    /// `shutdown()`, in parallel, under the same 2s bound. Default no-op is
    /// honest for modules whose discipline is save-on-write; a module holding
    /// anything volatile at stop time implements this or loses it BY CONTRACT
    /// (never by surprise).
    async fn save_state(&self) -> Result<(), String> {
        Ok(())
    }

    /// LOAD this node's state — the symmetric half, broadcast by the runtime
    /// after `initialize` succeeds. Default no-op; a module that saves must
    /// load, and the boot receipt shows which nodes did.
    async fn load_state(&self) -> Result<(), String> {
        Ok(())
    }

    /// Graceful shutdown. Release resources, flush buffers. Runs AFTER
    /// `save_state` in the runtime's broadcast — save first, then join.
    async fn shutdown(&self) -> Result<(), String> {
        Ok(())
    }

    /// Return command schemas for all commands this module handles.
    /// Used by MCPModule to dynamically generate MCP tool definitions.
    /// Default: empty (module doesn't expose structured schemas).
    fn command_schemas(&self) -> Vec<CommandSchema> {
        vec![]
    }

    // ─── PIECE-2 PR-2: artifact subscription / cadence / dispatch ─────
    //
    // Three default-impl methods so existing modules don't change.
    // Module authors opt in by overriding `artifact_subscriptions` to
    // name what they want, `cadence` to declare their wake policy, and
    // `on_artifact_available` to react. PR-3 of CBAR-PIECE-2 wires the
    // runtime dispatch path that calls `on_artifact_available` when a
    // producer publishes a matching key.
    //
    // Pattern matches the existing `handle_event` / `tick` defaults —
    // no-op default keeps every existing implementor (HealthModule,
    // PressureBrokerModule, CognitionModule, …) compiling without
    // edits. Opt-in only.

    /// Artifact subscriptions this module wants delivery for. Each
    /// returned `ArtifactSelector` matches a stream of artifacts the
    /// runtime will dispatch to `on_artifact_available`. Default: no
    /// subscriptions (module is not artifact-driven).
    ///
    /// Same shape Lane D's `PersonaTurnFrame` will eventually subscribe
    /// to its inbox-frame-ready artifact through; PR-3 wires the
    /// dispatcher. For now this is the data layer + the seam.
    fn artifact_subscriptions(&self) -> Vec<ArtifactSelector> {
        Vec::new()
    }

    /// Wake policy override. Returning `None` means "use the cadence
    /// implied by `ModuleConfig.tick_interval`" — `Some(Periodic)` if
    /// `tick_interval` is set, `Some(EventDriven)` if not. Returning
    /// `Some(...)` overrides, letting a module declare e.g.
    /// `Cadence::OnArtifact` without needing a tick_interval.
    ///
    /// Default: `None` (preserve existing tick_interval semantics).
    /// PR-3's `start_tick_loops` consults this when deciding whether
    /// to spawn a periodic task vs. wire the module to artifact wakes.
    fn cadence(&self) -> Option<Cadence> {
        None
    }

    /// Called when an artifact this module subscribes to is published.
    /// Default: no-op (matches the empty-subscriptions default).
    ///
    /// Implementations should be cheap-and-return — the runtime calls
    /// this from the publisher's task; long work belongs in `tick` or
    /// in a spawned task. Errors are logged by the dispatcher; the
    /// publisher is not blocked by a slow subscriber.
    async fn on_artifact_available(&self, _key: &ArtifactKey, _value: Value) -> Result<(), String> {
        Ok(())
    }

    /// Ready edge — the canonical "this module has finished initializing
    /// and is ready to do real work" signal. Modules with internal
    /// initialization that races with `initialize()` (Bevy renderer
    /// warming up its render thread; IPC server binding its socket;
    /// MemoryPressureMonitor publishing its first snapshot) override
    /// this to expose a `watch::Receiver<bool>` whose value flips from
    /// `false` to `true` exactly once, when the module is ready.
    ///
    /// Default impl returns `None`, which the runtime treats as "ready
    /// immediately after `initialize()` resolves" — the right semantics
    /// for the ~30 existing modules whose `initialize` IS the ready edge.
    ///
    /// Per [[docs/architecture/CONCURRENCY-STYLE-GUIDE.md]]: signals
    /// replace races. Callers wait on `Runtime::wait_for_ready(name)`
    /// (forwarded to this method) instead of polling, sleeping, or
    /// threading bespoke `oneshot::Sender`s through start_server-style
    /// bootstrap APIs.
    ///
    /// Implementations should publish through a `watch::Sender<bool>`
    /// they own, returning fresh `Receiver` clones on each call — cheap
    /// (the sender is an Arc internally) and lets multiple consumers
    /// subscribe independently.
    fn ready_edge(&self) -> Option<tokio::sync::watch::Receiver<bool>> {
        None
    }

    /// Install the substrate-wide `CommandExecutor` into this module.
    ///
    /// Modules that need to dispatch commands (channel sends a chat message;
    /// PIM bootstraps a persona; sentinel runs nested steps) store a
    /// `runtime::LateBound<CommandExecutor>` and override this method to
    /// populate it via `self.executor.install(executor)`. Default: no-op
    /// (most modules don't dispatch commands). See `runtime::late_bound`
    /// for the canonical injection slot.
    ///
    /// Called by `start_server` AFTER the executor is built and BEFORE any
    /// dispatch can reach this module — `Runtime::install_executor_on_all`
    /// walks every registered module exactly once. Per [[no-fallbacks-ever]]:
    /// when a module's command handler needs the executor and it isn't there,
    /// the right answer is a typed error at that call site, NOT a global
    /// panicking accessor. See task #224 for the GLOBAL_EXECUTOR removal
    /// rationale.
    fn install_executor(
        &self,
        _executor: std::sync::Arc<super::command_executor::CommandExecutor>,
    ) {
        // Default: module doesn't dispatch commands.
    }

    /// The self-routing command objects this module contributes to the kernel's
    /// `name -> Arc<dyn DynCommand>` map (see
    /// [docs/architecture/COMMAND-ORGANIZATION.md]). Each
    /// [`DynCommand`](crate::sdk_codegen::DynCommand) captures the module's deps
    /// at construction (an `Arc<Shared>`), so the kernel can route a command name
    /// DIRECTLY to it — no prefix scan, no per-module `match` arm. Default: none,
    /// so a module that hasn't migrated keeps routing through the legacy
    /// prefix → [`handle_command`](ServiceModule::handle_command) path. A module
    /// migrates a command by returning its object here and dropping its match arm;
    /// the typed object wins over the prefix fallback in the executor.
    fn commands(&self) -> Vec<std::sync::Arc<dyn crate::sdk_codegen::DynCommand>> {
        Vec::new()
    }

    /// Downcast support for typed discovery.
    /// Enables registry.module_as::<VoiceModule>() — like CBAR's getAnalyzerOfType<T>().
    fn as_any(&self) -> &dyn Any;
}

#[cfg(test)]
mod tests {
    //! Tests for the PIECE-2 PR-2 default-impl methods added to
    //! ServiceModule (artifact_subscriptions / cadence /
    //! on_artifact_available). Two test modules — one that takes the
    //! defaults, one that overrides — prove the opt-in pattern works
    //! through trait-object dispatch (the dispatch shape PR-3 will use).
    use super::*;
    use crate::runtime::artifact_handle::{ArtifactKey, ArtifactSelector, Cadence};
    use std::sync::Arc;

    /// Module that takes ALL defaults — represents every existing
    /// implementor (HealthModule, PressureBrokerModule, etc.) that
    /// hasn't opted in to artifact dispatch.
    struct DefaultsModule;

    #[async_trait]
    impl ServiceModule for DefaultsModule {
        fn config(&self) -> ModuleConfig {
            ModuleConfig {
                name: "defaults-test",
                priority: ModulePriority::Normal,
                command_prefixes: &[],
                event_subscriptions: &[],
                needs_dedicated_thread: false,
                max_concurrency: 0,
                tick_interval: None,
            }
        }
        async fn initialize(&self, _ctx: &super::super::ModuleContext) -> Result<(), String> {
            Ok(())
        }
        async fn handle_command(&self, _: &str, _: Value) -> Result<CommandResult, String> {
            Err("not handled".to_string())
        }
        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    /// Module that opts in — represents what Lane D's persona modules
    /// or any new artifact-driven module will look like.
    struct OptedInModule;

    #[async_trait]
    impl ServiceModule for OptedInModule {
        fn config(&self) -> ModuleConfig {
            ModuleConfig {
                name: "opted-in-test",
                priority: ModulePriority::Normal,
                command_prefixes: &[],
                event_subscriptions: &[],
                needs_dedicated_thread: false,
                max_concurrency: 0,
                tick_interval: None,
            }
        }
        async fn initialize(&self, _ctx: &super::super::ModuleContext) -> Result<(), String> {
            Ok(())
        }
        async fn handle_command(&self, _: &str, _: Value) -> Result<CommandResult, String> {
            Err("not handled".to_string())
        }

        fn artifact_subscriptions(&self) -> Vec<ArtifactSelector> {
            vec![
                ArtifactSelector::Prefix("persona/".to_string()),
                ArtifactSelector::Exact(ArtifactKey::from("paging/broker.snapshot")),
            ]
        }

        fn cadence(&self) -> Option<Cadence> {
            Some(Cadence::OnArtifact)
        }

        async fn on_artifact_available(
            &self,
            key: &ArtifactKey,
            value: Value,
        ) -> Result<(), String> {
            if key.as_str() == "trigger/fail" {
                return Err("intentional test failure".to_string());
            }
            // Echo to prove the dispatcher passed the right payload.
            // PR-3's runtime will record this kind of call for telemetry.
            let _ = value;
            Ok(())
        }

        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    /// What this catches: default-impl methods return the "no
    /// subscriptions / no cadence override / no-op handler" baseline,
    /// so existing modules that haven't been touched compile + behave
    /// as before. Guards against accidentally making the new methods
    /// required.
    #[tokio::test]
    async fn defaults_module_uses_no_op_implementations() {
        let m: Arc<dyn ServiceModule> = Arc::new(DefaultsModule);
        assert!(m.artifact_subscriptions().is_empty());
        assert_eq!(m.cadence(), None);
        let result = m
            .on_artifact_available(&ArtifactKey::from("anything/at/all"), Value::Null)
            .await;
        assert!(
            result.is_ok(),
            "default on_artifact_available must be Ok for every key"
        );
    }

    /// What this catches: an opted-in module's overrides are visible
    /// through the trait-object dispatch path PR-3 will use. If the
    /// runtime gets a `&dyn ServiceModule` and calls the new methods,
    /// it sees the override, not the default.
    #[tokio::test]
    async fn opted_in_module_returns_overrides_via_dyn_dispatch() {
        let m: Arc<dyn ServiceModule> = Arc::new(OptedInModule);
        let subs = m.artifact_subscriptions();
        assert_eq!(subs.len(), 2);
        // Verify the subscription set covers the cases PR-3 will dispatch
        // against — Prefix matches persona/* and Exact matches the broker.
        assert!(
            subs.iter()
                .any(|s| s.matches(&ArtifactKey::from("persona/inbox.frame_ready"))),
            "opted-in module should subscribe to persona/*"
        );
        assert!(
            subs.iter()
                .any(|s| s.matches(&ArtifactKey::from("paging/broker.snapshot"))),
            "opted-in module should subscribe to broker snapshot"
        );
        assert!(
            !subs
                .iter()
                .any(|s| s.matches(&ArtifactKey::from("cognition/rate_proposals.result"))),
            "subscription set is bounded — random unrelated keys don't match"
        );
        assert_eq!(m.cadence(), Some(Cadence::OnArtifact));
    }

    /// What this catches: error propagation through
    /// on_artifact_available. PR-3's dispatcher will log + continue;
    /// the subscriber error must NOT bubble up to the publisher (per
    /// the docstring: "publisher is not blocked by a slow subscriber").
    /// This test pins that the trait-method return shape is what the
    /// dispatcher can handle.
    #[tokio::test]
    async fn on_artifact_available_error_path_returns_err_not_panic() {
        let m: Arc<dyn ServiceModule> = Arc::new(OptedInModule);
        let result = m
            .on_artifact_available(&ArtifactKey::from("trigger/fail"), Value::Null)
            .await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "intentional test failure");
    }

    /// What this catches: a heterogeneous Vec of trait objects — the
    /// shape PR-3's dispatcher walks — handles modules with mixed
    /// opt-in status without special-casing.
    #[tokio::test]
    async fn dispatcher_can_walk_heterogeneous_subscriber_list() {
        let modules: Vec<Arc<dyn ServiceModule>> = vec![
            Arc::new(DefaultsModule),
            Arc::new(OptedInModule),
            Arc::new(DefaultsModule),
        ];

        // Compute: who would receive an artifact published under this key?
        // This is the exact filter PR-3's dispatcher applies.
        let key = ArtifactKey::from("persona/inbox.frame_ready");
        let interested: Vec<&Arc<dyn ServiceModule>> = modules
            .iter()
            .filter(|m| {
                m.artifact_subscriptions()
                    .iter()
                    .any(|sel| sel.matches(&key))
            })
            .collect();
        assert_eq!(
            interested.len(),
            1,
            "only the OptedInModule subscribes to persona/*; the two DefaultsModules ignore"
        );

        // And the inverse: a key nobody subscribed to wakes nobody.
        let unrelated = ArtifactKey::from("nothing/here");
        let interested_unrelated: Vec<&Arc<dyn ServiceModule>> = modules
            .iter()
            .filter(|m| {
                m.artifact_subscriptions()
                    .iter()
                    .any(|sel| sel.matches(&unrelated))
            })
            .collect();
        assert_eq!(
            interested_unrelated.len(),
            0,
            "no module subscribes to nothing/here — dispatcher walks zero"
        );
    }

    // ── CommandResult cell shape integration tests ─────────────────
    //
    // The cell shape unit tests live in
    // `runtime::cell_shapes::tests` (HandleRef construction,
    // serialization, distinct UUIDs, etc.). The tests below assert
    // the integration between the cell shapes and `CommandResult` —
    // the constructors + `to_json_value` projection that every
    // wire-crossing site uses.

    use crate::runtime::cell_shapes::{HandleRef, LambdaPlaceholder, StreamPlaceholder};
    use serde_json::json;
    use uuid::Uuid;

    #[test]
    fn json_to_json_value_returns_original() {
        let v = json!({ "x": 1 });
        let r = CommandResult::Json(v.clone());
        assert_eq!(r.to_json_value().unwrap(), v);
    }

    #[test]
    fn binary_to_json_value_returns_metadata_drops_bytes() {
        // The Binary variant carries metadata + raw bytes; projecting
        // to plain JSON drops the bytes and returns metadata. Callers
        // who need the raw bytes match on the variant directly (e.g.,
        // the IPC layer encodes them in the binary frame).
        let metadata = json!({ "format": "pcm-16le", "sample_rate": 48_000 });
        let r = CommandResult::Binary {
            metadata: metadata.clone(),
            data: vec![0u8, 1, 2, 3],
        };
        assert_eq!(r.to_json_value().unwrap(), metadata);
    }

    #[test]
    fn handle_to_json_value_serializes_handle_ref() {
        let id = Uuid::new_v4();
        let r = CommandResult::handle("ai/inference", id, "ai::InferenceSession");
        let json = r.to_json_value().expect("Handle must project to JSON");
        assert_eq!(json["owner"], "ai/inference");
        assert_eq!(json["type_tag"], "ai::InferenceSession");
        assert!(json["id"].is_string(), "id must serialize as string");
        assert_eq!(json["id"].as_str().unwrap(), id.to_string());
        assert!(json["created_at_ms"].is_number());
    }

    #[test]
    fn stream_to_json_value_returns_protocol_error() {
        let r = CommandResult::Stream(StreamPlaceholder::new("corr-001"));
        let err = r
            .to_json_value()
            .expect_err("Stream must NOT project as JSON — protocol not wired");
        assert!(
            err.contains("Stream cell shape is reserved"),
            "error must name the cell shape so callers find the doc: {err}"
        );
        assert!(
            err.contains("MODULE-ARCHITECTURE"),
            "error must point at the canonical doc: {err}"
        );
    }

    #[test]
    fn lambda_to_json_value_returns_protocol_error() {
        let r = CommandResult::Lambda(LambdaPlaceholder::new("ai/generate", json!({})));
        let err = r
            .to_json_value()
            .expect_err("Lambda must NOT project as JSON — protocol not wired");
        assert!(
            err.contains("Lambda cell shape is reserved"),
            "error must name the cell shape so callers find the doc: {err}"
        );
    }

    #[test]
    fn command_result_handle_constructor_matches_handle_ref_with_id() {
        let id = Uuid::new_v4();
        let r = CommandResult::handle("ai/inference", id, "ai::InferenceSession");
        match r {
            CommandResult::Handle(h) => {
                assert_eq!(h.id.as_uuid(), id);
                assert_eq!(h.owner, "ai/inference");
                assert_eq!(h.type_tag, "ai::InferenceSession");
            }
            other => panic!("expected Handle variant, got {other:?}"),
        }
    }

    #[test]
    fn command_result_protocol_errors_have_stable_wording() {
        // The error wording is matched on by callers (the sentinel
        // step builds its own step_err from these). Pin the prefix
        // so future edits don't accidentally break matching code.
        let stream_err = CommandResult::stream_protocol_error();
        let lambda_err = CommandResult::lambda_protocol_error();
        assert!(stream_err.starts_with("Stream cell shape is reserved"));
        assert!(lambda_err.starts_with("Lambda cell shape is reserved"));
        // Both should point at the architecture doc for context.
        for err in [&stream_err, &lambda_err] {
            assert!(
                err.contains("MODULE-ARCHITECTURE"),
                "error must point at the canonical doc: {err}"
            );
        }
    }

    #[test]
    fn handle_ref_round_trips_through_command_result_serialization() {
        // End-to-end pinning: a Handle returned by a Rust handler can
        // be projected to JSON, sent over the wire, deserialized on the
        // TS side as { owner, id, type_tag, created_at_ms }, echoed
        // back as a param on a subsequent call, deserialized in Rust
        // as HandleRef, and resolve to the same handle.
        let id = Uuid::new_v4();
        let original = HandleRef::with_id("ai/inference", id, "ai::InferenceSession");
        // Mint a Handle result, project to JSON (wire crossing #1).
        let r = CommandResult::Handle(original.clone());
        let wire = r.to_json_value().unwrap();
        // TS-side echo: serialize the JSON to a string and parse back.
        let echoed = serde_json::to_string(&wire).unwrap();
        let from_wire: HandleRef = serde_json::from_str(&echoed).unwrap();
        assert_eq!(from_wire, original);
        assert_eq!(from_wire.id.as_uuid(), id);
    }
}
