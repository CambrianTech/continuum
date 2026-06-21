//! HealthModule — the trivial outlier that validates the ServiceModule interface.
//!
//! Handles: health-check, get-stats
//! This is Phase 1: if this module routes correctly through the registry,
//! the ServiceModule trait design is proven for the simplest case.

use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use crate::sdk_codegen::{dispatch, CommandError, CommandHandler, Ctx, Outcome};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::any::Any;
use std::time::Instant;
use ts_rs::TS;

/// Params for `ping` — the canonical health/liveness command every SDK exposes.
/// An optional echo message round-trips so a caller can correlate.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
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

/// `ping` — Bare: bare `PingParams` in, bare `PingResult` out. The simplest
/// command, authored with the typed trait (a trivial-outlier proof of the
/// authoring surface alongside the ai/inference family).
pub struct PingCommand;
impl crate::sdk_codegen::CommandSpec for PingCommand {
    const NAME: &'static str = "ping";
    const ACCESS_LEVEL: crate::sdk_codegen::AccessLevel = crate::sdk_codegen::AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Health check: confirm the substrate is alive and responding. Returns a pong.";
    const WIRE: crate::sdk_codegen::WireShape = crate::sdk_codegen::WireShape::Bare;
    type Params = PingParams;
    type Result = PingResult;
}
crate::register_command!(PingCommand);

struct PingHandler;
#[async_trait]
impl CommandHandler for PingHandler {
    type Spec = PingCommand;
    async fn execute(&self, _ctx: &Ctx, _p: PingParams) -> Result<Outcome<PingResult>, CommandError> {
        Ok(PingResult {
            ok: true,
            round_trip_ms: 0,
        }
        .into())
    }
}

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
            command_prefixes: &["health-", "get-", "ping"],
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
            "ping" => dispatch(&PingHandler, params).await,

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
}
