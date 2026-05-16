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
    export_to = "../../../shared/generated/runtime/ModulePriority.ts"
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

/// Result of handling a command.
/// Supports both JSON-only and binary responses (audio, embeddings).
#[derive(Debug)]
pub enum CommandResult {
    /// Standard JSON response
    Json(Value),

    /// Binary response: JSON metadata + raw bytes.
    /// Wire format: [JSON header bytes][\0][raw binary bytes]
    /// Used for audio synthesis, embedding vectors, etc.
    Binary { metadata: Value, data: Vec<u8> },
}

impl CommandResult {
    /// Create a Json result from any Serialize type.
    /// Eliminates the `serde_json::to_value(x).unwrap()` anti-pattern.
    pub fn json(value: &impl serde::Serialize) -> Result<Self, String> {
        serde_json::to_value(value)
            .map(CommandResult::Json)
            .map_err(|e| format!("Serialization error: {e}"))
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
    /// Default: no-op (most modules only handle commands).
    async fn handle_event(&self, _event_name: &str, _payload: Value) -> Result<(), String> {
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

    /// Graceful shutdown. Release resources, flush buffers.
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
    async fn on_artifact_available(
        &self,
        _key: &ArtifactKey,
        _value: Value,
    ) -> Result<(), String> {
        Ok(())
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
        async fn initialize(&self, _ctx: &super::super::ModuleContext) -> Result<(), String> { Ok(()) }
        async fn handle_command(&self, _: &str, _: Value) -> Result<CommandResult, String> {
            Err("not handled".to_string())
        }
        fn as_any(&self) -> &dyn Any { self }
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
        async fn initialize(&self, _ctx: &super::super::ModuleContext) -> Result<(), String> { Ok(()) }
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

        fn as_any(&self) -> &dyn Any { self }
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
            .on_artifact_available(
                &ArtifactKey::from("anything/at/all"),
                Value::Null,
            )
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
            !subs.iter()
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
}
