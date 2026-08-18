//! ContentModule — host for the content-handle surface.
//!
//! Owns the one [`ContentRegistry`] and hands it to the `content/*` verbs, exactly as
//! [`GpuModule`](crate::modules::gpu::GpuModule) owns its `GpuMemoryManager`. The module
//! owning the state and contributing the commands that read it is the pattern; nothing
//! here reaches for a global.
//!
//! It is deliberately thin. All the behaviour lives in the [`ContentSource`] implementations
//! at the producers — this module exists so a citizen's `content/fetch` call has a
//! registered home and so the registry has one owner.
//!
//! [`ContentSource`]: crate::content::ContentSource
//! [`ContentRegistry`]: crate::content::ContentRegistry

use std::any::Any;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use crate::content::ContentRegistry;
use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};

pub struct ContentModule {
    registry: Arc<ContentRegistry>,
}

impl ContentModule {
    pub fn new(registry: Arc<ContentRegistry>) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl ServiceModule for ContentModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "content",
            priority: ModulePriority::Normal,
            command_prefixes: &["content/"],
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
        crate::commands::content::command_objects(self.registry.clone())
    }

    async fn handle_command(&self, command: &str, _params: Value) -> Result<CommandResult, String> {
        // Born on the typed registry — there is no legacy surface to fall back to, so a
        // name reaching here is a routing defect and says so rather than failing quietly.
        Err(format!(
            "content command surface is typed-registry only; '{command}' has no handler"
        ))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
