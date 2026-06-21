//! HealthModule — the trivial outlier that validates the ServiceModule interface.
//!
//! Handles: health-check, get-stats
//! This is Phase 1: if this module routes correctly through the registry,
//! the ServiceModule trait design is proven for the simplest case.

use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use crate::sdk_codegen::{ActionCommand, CommandError, Ctx};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::any::Any;
use std::time::Instant;
use ts_rs::TS;

/// Params for `ping` — the canonical health/liveness command every SDK exposes.
/// An optional echo message round-trips so a caller can correlate.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/health/PingParams.ts")]
pub struct PingParams {
    /// Optional message echoed back (for correlation / a hello).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub message: Option<String>,
}

/// Result of `ping` — the substrate is alive.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/health/PingResult.ts")]
pub struct PingResult {
    /// Always true on a successful round-trip.
    pub ok: bool,
    /// Substrate-measured handling time in milliseconds.
    pub round_trip_ms: u32,
}

/// `ping` — the canonical self-routing command. As an [`ActionCommand`] it gets
/// `CommandSpec` (Bare wire), `CommandHandler`, AND `DynCommand` from the blanket
/// impls — so this ONE type + a `run` body is the whole command. It is STATELESS,
/// so `register_stateless_command!` puts it on the kernel's typed object map with
/// ZERO host-module ceremony (no `handle_command` arm, no `commands()` override).
#[derive(Default)]
pub struct PingCommand;

#[async_trait]
impl ActionCommand for PingCommand {
    const NAME: &'static str = "ping";
    const DESCRIPTION: &'static str =
        "Health check: confirm the substrate is alive and responding. Returns a pong.";
    type Params = PingParams;
    type Output = PingResult;

    async fn run(&self, _ctx: &Ctx, _p: PingParams) -> Result<PingResult, CommandError> {
        Ok(PingResult {
            ok: true,
            round_trip_ms: 0,
        })
    }
}
crate::register_stateless_command!(PingCommand);

pub struct HealthModule {
    started_at: Instant,
}

impl Default for HealthModule {
    fn default() -> Self {
        Self {
            started_at: Instant::now(),
        }
    }
}

impl HealthModule {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl ServiceModule for HealthModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "health",
            priority: ModulePriority::Normal,
            // `ping` is NOT here — it's a STATELESS self-routing command
            // (`register_stateless_command!`), live on the typed object map with no
            // host-module ceremony. Only the un-migrated `health-`/`get-` verbs
            // still prefix-route to `handle_command`.
            command_prefixes: &["health-", "get-"],
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
        match command {
            "health-check" => {
                let uptime_secs = self.started_at.elapsed().as_secs();
                Ok(CommandResult::Json(serde_json::json!({
                    "healthy": true,
                    "uptime_seconds": uptime_secs,
                    "version": env!("CARGO_PKG_VERSION"),
                })))
            }

            "get-stats" => {
                // Stats tracking not yet implemented — stub matches legacy behavior
                Ok(CommandResult::Json(serde_json::json!({
                    "note": "Performance stats tracking not yet implemented"
                })))
            }

            _ => Err(format!("Unknown health command: {command}")),
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_health_check() {
        let module = HealthModule::new();
        let result = module.handle_command("health-check", Value::Null).await;
        assert!(result.is_ok());
        if let Ok(CommandResult::Json(json)) = result {
            assert_eq!(json["healthy"], true);
            assert!(json["uptime_seconds"].is_number());
        }
    }

    // what this catches: `ping` migrated to the TYPED PATH end-to-end — it is NOT
    // in the prefix table (so the legacy handle_command match can't serve it), and
    // it IS in the registry's DynCommand object map, where it invokes to the bare
    // PingResult (no envelope). This is the proof that a base-trait command
    // (ActionCommand ⟹ DynCommand) self-routes through the kernel registry with no
    // per-module match arm. Regression here = the typed path silently falling back
    // to prefix routing (or ping vanishing entirely).
    #[tokio::test]
    async fn ping_routes_through_the_typed_object_map() {
        use crate::runtime::ModuleRegistry;
        use std::sync::Arc;
        let registry = ModuleRegistry::new();
        registry.register(Arc::new(HealthModule::new()));

        // Not prefix-routed any more — only the typed object map serves it.
        assert!(
            registry.route_command("ping").is_none(),
            "ping must NOT be on the prefix table (it's a DynCommand object)"
        );
        assert!(
            registry.list_command_objects().contains(&"ping"),
            "ping is registered as a self-routing command object"
        );

        let cmd = registry
            .route_object("ping")
            .expect("ping resolves through the typed object map");
        let cr = cmd
            .invoke(serde_json::json!({}))
            .await
            .expect("ping invoke ok");
        match cr {
            CommandResult::Json(v) => {
                assert_eq!(v["ok"], true);
                assert!(v.get("success").is_none(), "Bare wire — no envelope");
            }
            other => panic!("expected Json, got {other:?}"),
        }
    }
}
