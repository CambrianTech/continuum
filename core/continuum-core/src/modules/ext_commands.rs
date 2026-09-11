//! The loader module for authored commands (Phase C, S7): hands every manifest-backed
//! [`ProcessCommand`] to the kernel through the ONE seam every dep-holding command
//! already uses — [`ServiceModule::commands`] — so routing, catalogue, ACL, tool surface
//! and the recipe gate all see an authored verb exactly as they see a shipped one.
//!
//! Loading happens at construction (boot), once; the descriptors are installed into
//! [`crate::sdk_codegen::ext::command_registry_live`] BEFORE any reader memoizes. A
//! refused manifest is probed by file and reason and never refuses the boot.

use crate::runtime::{ModuleConfig, ModulePriority, ServiceModule};
use crate::sdk_codegen::ext::{install_ext_descriptors, load_manifests, overlay_dir, LoadedCommand};
use crate::sdk_codegen::DynCommand;
use async_trait::async_trait;
use std::sync::Arc;

pub struct ExtCommandsModule {
    commands: Vec<Arc<dyn DynCommand>>,
}

impl ExtCommandsModule {
    /// Load `<continuum_root>/commands/*.json`, install the descriptors, keep the objects.
    pub fn new(continuum_root: &std::path::Path) -> Self {
        let shipped: Vec<String> = crate::sdk_codegen::command_registry().into_iter().map(|d| d.name.to_string()).collect();
        let shipped_refs: Vec<&str> = shipped.iter().map(String::as_str).collect();
        let dir = overlay_dir(continuum_root);
        let (loaded, refused) = load_manifests(&dir, &shipped_refs);
        for e in &refused {
            crate::probe!(
                class = "ext.command.refused",
                reason = %e,
                "an authored command manifest was refused — fix or remove the named file"
            );
        }
        crate::probe!(
            class = "ext.command.loaded",
            dir = %dir.display(),
            loaded = loaded.len() as u64,
            refused = refused.len() as u64,
            "authored commands loaded from the overlay directory"
        );
        install_ext_descriptors(loaded.iter().map(|l| l.descriptor.clone()).collect());
        let commands = loaded
            .into_iter()
            .map(|LoadedCommand { command, .. }| command as Arc<dyn DynCommand>)
            .collect();
        Self { commands }
    }
}

#[async_trait]
impl ServiceModule for ExtCommandsModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "ext_commands",
            priority: ModulePriority::Normal,
            command_prefixes: &[],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    async fn initialize(&self, _ctx: &crate::runtime::ModuleContext) -> Result<(), String> {
        Ok(())
    }

    /// Nothing routes here by prefix: every authored verb is a typed object in the
    /// kernel's command map (via `commands()` below), so a call reaching this arm
    /// names a verb no manifest declares.
    async fn handle_command(&self, command: &str, _params: serde_json::Value) -> Result<crate::runtime::CommandResult, String> {
        Err(format!("`{command}` is not an authored command on this node (see commands/list)"))
    }

    fn commands(&self) -> Vec<Arc<dyn DynCommand>> {
        self.commands.clone()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
