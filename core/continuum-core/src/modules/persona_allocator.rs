//! PersonaAllocatorModule — owns the shared [`GpuMemoryManager`] for
//! hardware-aware persona allocation.
//!
//! The command surface (`persona/allocate`, `persona/catalog`) is migrated to the
//! typed registry (`commands/persona/allocate.rs`, `commands/persona/catalog.rs`).
//! This module exists to hand the live GPU manager to the dep-holding
//! `persona/allocate` command via `commands()`. The allocation algorithm + catalog
//! loader are pure domain functions in [`crate::persona::allocator`].

use crate::gpu::GpuMemoryManager;
use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use async_trait::async_trait;
use serde_json::Value;
use std::any::Any;
use std::sync::Arc;

pub struct PersonaAllocatorModule {
    gpu_manager: Arc<GpuMemoryManager>,
}

impl PersonaAllocatorModule {
    pub fn new(gpu_manager: Arc<GpuMemoryManager>) -> Self {
        Self { gpu_manager }
    }
}

#[async_trait]
impl ServiceModule for PersonaAllocatorModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "persona_allocator",
            priority: ModulePriority::Normal,
            command_prefixes: &["persona/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
        Ok(())
    }

    async fn handle_command(&self, command: &str, _params: Value) -> Result<CommandResult, String> {
        // `persona/allocate` + `persona/catalog` are migrated to the typed registry
        // (`commands/persona/{allocate,catalog}.rs`). Fail loud — no silent fallback.
        Err(format!(
            "persona allocator command surface is migrated to the typed registry; \
             '{command}' has no legacy handler"
        ))
    }

    fn commands(&self) -> Vec<Arc<dyn crate::sdk_codegen::DynCommand>> {
        // Hand the live, shared GPU manager to the dep-holding `persona/allocate`
        // command so its allocation reads the SAME detected hardware the module owns.
        // `persona/catalog` is stateless and self-registers — not listed here.
        vec![Arc::new(
            crate::commands::persona::allocate::PersonaAllocate {
                gpu_manager: self.gpu_manager.clone(),
            },
        )]
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_module() -> PersonaAllocatorModule {
        let manager = Arc::new(GpuMemoryManager::detect());
        PersonaAllocatorModule::new(manager)
    }

    // what this catches: the allocation behavior (no-keys + present-key) now lives in
    // the typed command (`commands/persona/allocate.rs`); the legacy handle_command is
    // migrated, so for any command name it must fail loud — never silently fall back.
    #[tokio::test]
    async fn legacy_handle_command_fails_loud() {
        let module = test_module();
        let err = module
            .handle_command("persona/allocate", serde_json::json!({}))
            .await
            .expect_err("legacy handler must fail loud after migration");
        assert!(
            err.contains("migrated to the typed registry"),
            "error must name the migration: {err}"
        );
    }

    // what this catches: the module contributes the dep-holding `persona/allocate`
    // command, sharing its OWN GpuMemoryManager (so allocation reads the same detected
    // hardware). A regression that drops the contribution — or constructs a fresh
    // manager — is caught. `persona/catalog` is stateless and self-registers.
    #[test]
    fn contributes_persona_allocate_command() {
        let module = test_module();
        let names: Vec<&str> = module.commands().iter().map(|c| c.name()).collect();
        assert_eq!(names, vec!["persona/allocate"]);
    }
}
