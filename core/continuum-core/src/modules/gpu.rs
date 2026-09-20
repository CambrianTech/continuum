//! GpuModule — the GPU memory authority's host module.
//!
//! The `gpu/*` command surface is MIGRATED to the typed self-routing registry —
//! each verb is now an [`ActionCommand`](crate::sdk_codegen::ActionCommand) under
//! [`crate::commands::gpu`], contributed through [`commands()`](GpuModule::commands).
//! They win at `route_object` (the O(1) typed object map) before any legacy prefix
//! path, and their descriptors flow into `command_registry()` → the persona tool
//! surface, the grid ACL, and codegen. This module now only owns the shared
//! [`GpuMemoryManager`] and hands it to those commands.
//!
//! Access levels reflect the resource-authority boundary (task #56): reads are
//! `AiSafe`, `gpu/set-budget` is `Privileged`, consumer (un)register is `Internal`.

use crate::gpu::GpuMemoryManager;
use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use async_trait::async_trait;
use serde_json::Value;
use std::any::Any;
use std::sync::Arc;

pub struct GpuModule {
    manager: Arc<GpuMemoryManager>,
}

impl GpuModule {
    pub fn new(manager: Arc<GpuMemoryManager>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl ServiceModule for GpuModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "gpu",
            priority: ModulePriority::Normal,
            command_prefixes: &["gpu/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
        Ok(())
    }

    fn commands(&self) -> Vec<Arc<dyn crate::sdk_codegen::DynCommand>> {
        // The seven `gpu/*` verbs as typed self-routing objects sharing the one
        // `Arc<GpuMemoryManager>`. See [`crate::commands::gpu`].
        crate::commands::gpu::command_objects(self.manager.clone())
    }

    async fn handle_command(&self, command: &str, _params: Value) -> Result<CommandResult, String> {
        // MIGRATED: every `gpu/*` verb is now a typed self-routing command object
        // (see `commands()` above). They win at `route_object`, so nothing should
        // reach here. Fail loud on any stray name — this legacy `handle_command`
        // retires entirely in Wave Z.
        Err(format!(
            "gpu command surface is migrated to the typed registry; '{command}' has no legacy handler"
        ))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_gpu_module() -> GpuModule {
        GpuModule::new(Arc::new(GpuMemoryManager::detect()))
    }

    // what this catches: the module still owns the `gpu/` prefix and contributes all
    // seven migrated verbs as typed command objects — if `commands()` regressed to
    // empty, the persona tool surface would silently lose the GPU verbs.
    #[test]
    fn module_config_and_commands_wired() {
        let module = test_gpu_module();
        let config = module.config();
        assert_eq!(config.name, "gpu");
        assert!(config.command_prefixes.contains(&"gpu/"));
        assert_eq!(module.commands().len(), 7);
    }

    // what this catches: the legacy `handle_command` is now a dead fail-loud stub —
    // it must NEVER silently succeed (that would mask a routing regression where a
    // verb failed to reach its typed object). It names the offending command.
    #[tokio::test]
    async fn legacy_handle_command_fails_loud() {
        let module = test_gpu_module();
        let err = module
            .handle_command("gpu/stats", Value::Null)
            .await
            .unwrap_err();
        assert!(err.contains("migrated to the typed registry"));
        assert!(err.contains("gpu/stats"));
    }
}
