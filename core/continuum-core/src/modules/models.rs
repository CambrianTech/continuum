//! ModelsModule — owner of the live model universe.
//!
//! Holds the single `Arc<ModelCatalog>` (the runtime-mutable watch-snapshot layer
//! seeded from the immutable registry) and contributes the `models/*` command
//! family to the kernel's typed object map via [`commands()`](ModelsModule::commands).
//! The commands themselves live under [`crate::commands::models`].
//!
//! This module no longer routes through the stringly `handle_command` `match` —
//! every `models/*` verb is a typed [`ActionCommand`](crate::sdk_codegen::ActionCommand)
//! on the ONE registry, so each appears in the persona tool surface, the grid
//! ACL, codegen, and `cu`. `command_prefixes` is empty: there is nothing left to
//! prefix-route, and an unregistered `models/*` name fails loud at the executor
//! rather than reaching a legacy arm.

use crate::model_registry::live::ModelCatalog;
use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use async_trait::async_trait;
use serde_json::Value;
use std::any::Any;
use std::sync::Arc;

pub struct ModelsModule {
    catalog: Arc<ModelCatalog>,
}

impl ModelsModule {
    pub fn new(catalog: Arc<ModelCatalog>) -> Self {
        Self { catalog }
    }
}

#[async_trait]
impl ServiceModule for ModelsModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "models",
            priority: ModulePriority::Background,
            // Empty: every `models/*` verb is a typed command on the ONE registry,
            // routed by the executor's object map. Nothing prefix-routes here.
            command_prefixes: &[],
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
        // No legacy arms remain — every `models/*` verb is a typed ActionCommand.
        // Reaching here means the executor routed an unregistered name to this
        // module; fail loud naming it rather than silently swallowing.
        Err(format!(
            "'{command}' is not a registered models command — the models surface is the typed `models/*` ActionCommands, not a legacy handler"
        ))
    }

    fn commands(&self) -> Vec<Arc<dyn crate::sdk_codegen::DynCommand>> {
        crate::commands::models::command_objects(self.catalog.clone())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
