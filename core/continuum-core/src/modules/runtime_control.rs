//! RuntimeModule — Exposes runtime metrics and control via IPC.
//!
//! Enables AI-driven system management (Ares pattern):
//! - runtime/metrics/all: Get stats for all modules
//! - runtime/metrics/module: Get stats for specific module
//! - runtime/metrics/slow: List recent slow commands
//! - runtime/list: List all modules with their configs
//!
//! The runtime automatically tracks timing for ALL commands.
//! This module just exposes that data via queryable commands.

use crate::runtime::{
    CommandResult, ModuleConfig, ModuleContext, ModulePriority, ModuleRegistry, ServiceModule,
};
use async_trait::async_trait;
use serde_json::Value;
use std::any::Any;
use std::sync::Arc;
use tokio::sync::OnceCell;

/// The runtime-introspection registry handle, shared between the module (which
/// populates it at `initialize`) and the typed `runtime/*` command objects (which
/// read it at dispatch time). `commands()` is collected during `register()` —
/// before `initialize()` — so the commands capture this cell empty and resolve it
/// lazily; a dispatch before init fails loud ("RuntimeModule not initialized").
pub type RuntimeRegistryCell = Arc<OnceCell<Arc<ModuleRegistry>>>;

pub struct RuntimeModule {
    /// Reference to registry for querying metrics (set during initialize)
    registry: RuntimeRegistryCell,
}

impl Default for RuntimeModule {
    fn default() -> Self {
        Self {
            registry: Arc::new(OnceCell::new()),
        }
    }
}

impl RuntimeModule {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl ServiceModule for RuntimeModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "runtime",
            priority: ModulePriority::Normal,
            command_prefixes: &["runtime/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    async fn initialize(&self, ctx: &ModuleContext) -> Result<(), String> {
        // Store registry reference for metric queries
        self.registry
            .set(ctx.registry.clone())
            .map_err(|_| "RuntimeModule already initialized")?;
        Ok(())
    }

    fn commands(&self) -> Vec<Arc<dyn crate::sdk_codegen::DynCommand>> {
        // The four `runtime/*` introspection commands, each sharing the cell this
        // module fills at `initialize`. See [`crate::commands::runtime`].
        crate::commands::runtime::command_objects(self.registry.clone())
    }

    async fn handle_command(&self, command: &str, _params: Value) -> Result<CommandResult, String> {
        // MIGRATED: `runtime/metrics/{all,module,slow}` + `runtime/list` are now typed
        // self-routing command objects (see `commands()` above). They win at
        // `route_object`, so nothing should reach here. Fail loud on any stray name —
        // this legacy `handle_command` retires entirely in Wave Z.
        Err(format!(
            "runtime command surface is migrated to the typed registry; '{command}' has no legacy handler"
        ))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_module_config() {
        let module = RuntimeModule::new();
        let config = module.config();
        assert_eq!(config.name, "runtime");
        assert!(config.command_prefixes.contains(&"runtime/"));
    }
}
