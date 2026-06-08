//! EventsModule — IPC commands for the event-class registry.
//!
//! Roadmap item L1-1 (see docs/grid/GRID-MIGRATION-ROADMAP.md).
//! Spec: GRID-BUS-ARCHITECTURE §2.2 (continuum#1439).
//!
//! Commands:
//! - `events/declare-class`: Register a new event class with transport-routing
//!   metadata. Idempotent for identical re-declarations; errors on conflicting
//!   re-declarations (wire-contract integrity).
//! - `events/get-class`: Look up a single class's resolved config. Returns
//!   null when undeclared (caller falls back to default backward-compat
//!   behavior).
//! - `events/list-classes`: Snapshot of all declared classes. Used by the
//!   TS-side cache on startup + by `grid/show-event-classes` introspection.
//! - `events/resolve-channel`: Resolve the airc channel for an emit. Used
//!   by the L1-2 AircEventTransport when it lands.

use crate::events::{
    declare_event_class, list_event_classes, lookup_event_class, resolve_event_class_channel,
    EventClassChannelResolveError, EventClassConfig, EventClassRegistryError,
};
use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use async_trait::async_trait;
use serde::Deserialize;
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

#[derive(Debug, Deserialize)]
struct DeclareClassParams {
    name: String,
    #[serde(flatten)]
    config: EventClassConfig,
}

#[derive(Debug, Deserialize)]
struct GetClassParams {
    name: String,
}

#[derive(Debug, Deserialize)]
struct ResolveChannelParams {
    name: String,
    /// Event payload. Channel strategies that depend on payload fields
    /// (ByRoomId, ByPeerId) extract from this.
    #[serde(default)]
    payload: Value,
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

    async fn handle_command(&self, command: &str, params: Value) -> Result<CommandResult, String> {
        match command {
            "events/declare-class" => {
                let parsed: DeclareClassParams = serde_json::from_value(params)
                    .map_err(|e| format!("events/declare-class: invalid params: {e}"))?;
                let resolved = declare_event_class(&parsed.name, &parsed.config)
                    .map_err(declare_error_to_string)?;
                let json = serde_json::to_value(&resolved)
                    .map_err(|e| format!("events/declare-class: serialize result: {e}"))?;
                Ok(CommandResult::Json(json))
            }

            "events/get-class" => {
                let parsed: GetClassParams = serde_json::from_value(params)
                    .map_err(|e| format!("events/get-class: invalid params: {e}"))?;
                match lookup_event_class(&parsed.name) {
                    Some(cfg) => {
                        let json = serde_json::to_value(&cfg)
                            .map_err(|e| format!("events/get-class: serialize result: {e}"))?;
                        Ok(CommandResult::Json(json))
                    }
                    // Return JSON null — caller treats as "no class declared,
                    // use default backward-compat behavior."
                    None => Ok(CommandResult::Json(Value::Null)),
                }
            }

            "events/list-classes" => {
                let classes = list_event_classes();
                let json = serde_json::to_value(&classes)
                    .map_err(|e| format!("events/list-classes: serialize result: {e}"))?;
                Ok(CommandResult::Json(json))
            }

            "events/resolve-channel" => {
                let parsed: ResolveChannelParams = serde_json::from_value(params)
                    .map_err(|e| format!("events/resolve-channel: invalid params: {e}"))?;
                match resolve_event_class_channel(&parsed.name, &parsed.payload) {
                    Ok(channel) => Ok(CommandResult::Json(serde_json::json!({
                        "channel": channel,
                    }))),
                    Err(e) => Err(resolve_error_to_string(e)),
                }
            }

            other => Err(format!("Unknown events command: {other}")),
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

fn declare_error_to_string(e: EventClassRegistryError) -> String {
    match e {
        EventClassRegistryError::Declare(inner) => format!("events/declare-class: {inner}"),
    }
}

fn resolve_error_to_string(e: EventClassChannelResolveError) -> String {
    format!("events/resolve-channel: {e}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::EventClassChannelStrategy;

    fn declare_params_local(name: &str) -> Value {
        serde_json::json!({
            "name": name,
            "broadcast": false,
            "schemaVersion": "v1",
        })
    }

    fn declare_params_broadcast_global(name: &str) -> Value {
        serde_json::json!({
            "name": name,
            "broadcast": true,
            "channel": "global",
            "schemaVersion": "v1",
        })
    }

    #[tokio::test]
    async fn declare_then_get_via_ipc() {
        let module = EventsModule::new();
        // Use unique-per-test names to avoid cross-test contamination of
        // the singleton.
        let name = "ipc-test:declare-then-get";

        let result = module
            .handle_command(
                "events/declare-class",
                declare_params_broadcast_global(name),
            )
            .await
            .unwrap();
        match result {
            CommandResult::Json(v) => {
                assert_eq!(v.get("name").and_then(|x| x.as_str()), Some(name));
                assert_eq!(v.get("broadcast").and_then(|x| x.as_bool()), Some(true));
                assert_eq!(v.get("channel").and_then(|x| x.as_str()), Some("global"));
            }
            _ => panic!("expected json result"),
        }

        let result = module
            .handle_command("events/get-class", serde_json::json!({ "name": name }))
            .await
            .unwrap();
        match result {
            CommandResult::Json(v) => {
                assert_eq!(v.get("name").and_then(|x| x.as_str()), Some(name));
            }
            _ => panic!("expected json result"),
        }
    }

    #[tokio::test]
    async fn get_undeclared_returns_null() {
        let module = EventsModule::new();
        let result = module
            .handle_command(
                "events/get-class",
                serde_json::json!({ "name": "never:declared-by-ipc-test" }),
            )
            .await
            .unwrap();
        match result {
            CommandResult::Json(Value::Null) => {}
            other => panic!("expected null, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn declare_idempotent() {
        let module = EventsModule::new();
        let name = "ipc-test:idempotent";

        let first = module
            .handle_command("events/declare-class", declare_params_local(name))
            .await
            .unwrap();
        let second = module
            .handle_command("events/declare-class", declare_params_local(name))
            .await
            .unwrap();
        match (first, second) {
            (CommandResult::Json(a), CommandResult::Json(b)) => assert_eq!(a, b),
            _ => panic!("expected json results"),
        }
    }

    #[tokio::test]
    async fn resolve_channel_global_via_ipc() {
        let module = EventsModule::new();
        let name = "ipc-test:resolve-global";
        module
            .handle_command(
                "events/declare-class",
                declare_params_broadcast_global(name),
            )
            .await
            .unwrap();

        let result = module
            .handle_command(
                "events/resolve-channel",
                serde_json::json!({ "name": name, "payload": {} }),
            )
            .await
            .unwrap();
        match result {
            CommandResult::Json(v) => {
                assert_eq!(v.get("channel").and_then(|x| x.as_str()), Some("global"));
            }
            _ => panic!("expected json result"),
        }
    }

    #[tokio::test]
    async fn list_classes_includes_declared() {
        let module = EventsModule::new();
        // Use a uniquely-prefixed name so we can find it in the global
        // list even if other tests declared others.
        let name = "ipc-test:list-check-unique-name-xyz";
        module
            .handle_command("events/declare-class", declare_params_local(name))
            .await
            .unwrap();

        let result = module
            .handle_command("events/list-classes", serde_json::json!({}))
            .await
            .unwrap();
        match result {
            CommandResult::Json(v) => {
                let arr = v.as_array().expect("list returns array");
                let found = arr
                    .iter()
                    .any(|c| c.get("name").and_then(|n| n.as_str()) == Some(name));
                assert!(found, "declared class should appear in list");
            }
            _ => panic!("expected json array"),
        }
    }

    // Smoke that the channel-strategy enum serializes the way the TS side expects.
    #[test]
    fn channel_strategy_serializes_camel_case() {
        let global = EventClassChannelStrategy::Global;
        let by_room = EventClassChannelStrategy::ByRoomId;
        let by_peer = EventClassChannelStrategy::ByPeerId;
        assert_eq!(serde_json::to_string(&global).unwrap(), "\"global\"");
        assert_eq!(serde_json::to_string(&by_room).unwrap(), "\"byRoomId\"");
        assert_eq!(serde_json::to_string(&by_peer).unwrap(), "\"byPeerId\"");
    }
}
