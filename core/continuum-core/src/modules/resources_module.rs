//! ResourcesModule — the command surface over the one per-machine resource authority.
//!
//! The [`ResourceDaemon`](crate::resources::ResourceDaemon) is the single authority over
//! VRAM/RAM/disk/ports (#56). It runs its own background task (interval + watch snapshot)
//! and is NOT itself a `ServiceModule` — it predates the command surface and owns its
//! accounting loop directly. This thin module wraps the shared `Arc<ResourceDaemon>` so
//! the authority gets a home in the typed command registry, the same way every other
//! singleton in the server (gpu_manager, system_monitor, the PressureBroker) lives behind
//! or is owned by a `ServiceModule`.
//!
//! Its whole job is `commands()` → contribute the dep-holding `resources/*` read verbs
//! (currently just `resources/board`, the #79 drift-*reporting* read surface). It owns no
//! tick (the daemon owns its own), routes no legacy `handle_command` arm, and holds no
//! state beyond the daemon handle. Future `resources/*` verbs (capacity, reservations,
//! reclaim status) land here without touching boot wiring.

use std::any::Any;
use std::sync::Arc;

use async_trait::async_trait;

use crate::resources::ResourceDaemon;
use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};

pub struct ResourcesModule {
    daemon: Arc<ResourceDaemon>,
}

impl ResourcesModule {
    /// Wrap the boot-time `Arc<ResourceDaemon>` so its board is queryable. The daemon
    /// is already started (owns its interval task) before this module is constructed;
    /// we only borrow the handle to serve reads.
    pub fn new(daemon: Arc<ResourceDaemon>) -> Self {
        Self { daemon }
    }
}

#[async_trait]
impl ServiceModule for ResourcesModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "resources",
            priority: ModulePriority::Normal,
            // Typed path only — the `resources/*` verbs route via `route_object`
            // against the objects `commands()` contributes. No prefix arm, no tick
            // (the daemon owns its own accounting cadence).
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

    /// Contribute the dep-holding `resources/*` read verbs over the live
    /// `Arc<ResourceDaemon>`. See `crate::commands::resources::command_objects`.
    fn commands(&self) -> Vec<Arc<dyn crate::sdk_codegen::DynCommand>> {
        crate::commands::resources::command_objects(self.daemon.clone())
    }

    /// The `resources/*` verbs are typed-registry commands; they route via
    /// `route_object` against the objects `commands()` contributes. Reaching this arm
    /// means the typed path failed to register — fail loud naming the cause rather than
    /// silently re-handling (there is no legacy `resources/*` handler to fall back to).
    async fn handle_command(
        &self,
        command: &str,
        _params: serde_json::Value,
    ) -> Result<CommandResult, String> {
        Err(format!(
            "resources: '{command}' is a typed-registry command — it must route via \
             route_object (commands/resources/), not the legacy handle_command path"
        ))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resources::DaemonConfig;
    use crate::sdk_codegen::ActionCommand;

    fn module() -> ResourcesModule {
        ResourcesModule::new(ResourceDaemon::start(
            Vec::new(),
            Vec::new(),
            DaemonConfig::default(),
        ))
    }

    // what this catches: the module contributes exactly the `resources/board` object to
    // the typed registry — the wiring that makes the #79 board queryable. If the family
    // stops being exposed (or the name drifts), the drift-reporting read surface silently
    // disappears from the persona/operator/grid tool surface.
    // tokio runtime required: `ResourceDaemon::start` spawns its own interval task from
    // the constructor (the canonical Daemon base), so it must be built under a reactor.
    #[tokio::test]
    async fn contributes_the_board_command() {
        let objs = module().commands();
        let names: Vec<&str> = objs.iter().map(|c| c.name()).collect();
        assert!(
            names.contains(&crate::commands::resources::board::ResourcesBoard::NAME),
            "resources module must contribute resources/board; got {names:?}"
        );
    }
}
