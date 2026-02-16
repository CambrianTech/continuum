//! HealthModule — the trivial outlier that validates the ServiceModule interface.
//!
//! Handles: health-check, get-stats
//! This is Phase 1: if this module routes correctly through the registry,
//! the ServiceModule trait design is proven for the simplest case.

use crate::runtime::{ServiceModule, ModuleConfig, ModulePriority, CommandResult, ModuleContext};
use async_trait::async_trait;
use serde_json::Value;
use std::any::Any;
use std::time::Instant;

pub struct HealthModule {
    started_at: Instant,
}

impl HealthModule {
    pub fn new() -> Self {
        Self {
            started_at: Instant::now(),
        }
    }
}

#[async_trait]
impl ServiceModule for HealthModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "health",
            priority: ModulePriority::Normal,
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

    async fn handle_command(
        &self,
        command: &str,
        _params: Value,
    ) -> Result<CommandResult, String> {
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

    fn as_any(&self) -> &dyn Any { self }
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
