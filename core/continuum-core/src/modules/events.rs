//! EventsModule — host shell for the event-class registry surface.
//!
//! Roadmap item L1-1 (see docs/grid/GRID-MIGRATION-ROADMAP.md).
//! Spec: GRID-BUS-ARCHITECTURE §2.2 (continuum#1439).
//!
//! The four `events/*` verbs are MIGRATED to the typed self-routing registry —
//! each is now a stateless [`ActionCommand`](crate::sdk_codegen::ActionCommand)
//! under [`crate::commands::events`] (the registry is a process singleton reached
//! through free functions, so the commands hold no state and self-register via
//! `register_stateless_command!`). They win at `route_object` before any legacy
//! prefix path, and their descriptors flow into `command_registry()` → the persona
//! tool surface, the grid ACL, and codegen. This module now only holds the
//! `events/` prefix declaration; it retires entirely in Wave Z.

use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use async_trait::async_trait;
use serde_json::Value;
use std::any::Any;

pub struct EventsModule;

impl EventsModule {
    pub fn new() -> Self {
        Self
    }
}

impl Default for EventsModule {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ServiceModule for EventsModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "events",
            priority: ModulePriority::Normal,
            command_prefixes: &["events/"],
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
        // MIGRATED: every `events/*` verb is a typed stateless command object (see
        // `crate::commands::events`). They win at `route_object`, so nothing should
        // reach here. Fail loud on any stray name — this legacy `handle_command`
        // retires entirely in Wave Z.
        Err(format!(
            "events command surface is migrated to the typed registry; '{command}' has no legacy handler"
        ))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the module still owns the `events/` prefix (the kernel
    // registers it) — if the prefix dropped, legacy prefix-routing would stop
    // recognizing the namespace before Wave Z deletes prefix routing entirely.
    #[test]
    fn module_owns_events_prefix() {
        let config = EventsModule::new().config();
        assert_eq!(config.name, "events");
        assert!(config.command_prefixes.contains(&"events/"));
    }

    // what this catches: the legacy `handle_command` is now a dead fail-loud stub —
    // it must NEVER silently succeed (that would mask a routing regression where a
    // verb failed to reach its typed object). It names the offending command.
    #[tokio::test]
    async fn legacy_handle_command_fails_loud() {
        let err = EventsModule::new()
            .handle_command("events/declare-class", Value::Null)
            .await
            .unwrap_err();
        assert!(err.contains("migrated to the typed registry"));
        assert!(err.contains("events/declare-class"));
    }
}
