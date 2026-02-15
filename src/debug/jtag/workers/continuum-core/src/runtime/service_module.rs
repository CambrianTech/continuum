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

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::any::Any;
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
#[ts(export, export_to = "../../../shared/generated/runtime/ModulePriority.ts")]
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
    Binary {
        metadata: Value,
        data: Vec<u8>,
    },
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
    async fn handle_command(
        &self,
        command: &str,
        params: Value,
    ) -> Result<CommandResult, String>;

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

    /// Downcast support for typed discovery.
    /// Enables registry.module_as::<VoiceModule>() — like CBAR's getAnalyzerOfType<T>().
    fn as_any(&self) -> &dyn Any;
}
