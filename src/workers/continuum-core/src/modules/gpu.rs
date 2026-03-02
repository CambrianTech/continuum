//! GpuModule — IPC commands for GPU memory management.
//!
//! Commands:
//! - `gpu/stats`: Full GPU stats snapshot (total VRAM, per-subsystem budgets/usage, pressure)
//! - `gpu/pressure`: Quick pressure query (0.0-1.0)
//!
//! Follows the HealthModule pattern: stateless handler wrapping shared state.

use crate::gpu::GpuMemoryManager;
use crate::runtime::{ServiceModule, ModuleConfig, ModulePriority, CommandResult, ModuleContext};
use async_trait::async_trait;
use serde_json::Value;
use std::any::Any;
use std::sync::Arc;

pub struct GpuModule {
    manager: Arc<GpuMemoryManager>,
}

impl GpuModule {
    pub fn new(manager: Arc<GpuMemoryManager>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl ServiceModule for GpuModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "gpu",
            priority: ModulePriority::Normal,
            command_prefixes: &["gpu/"],
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
            "gpu/stats" => {
                let stats = self.manager.stats();
                let json = serde_json::to_value(stats)
                    .map_err(|e| format!("Failed to serialize GPU stats: {e}"))?;
                Ok(CommandResult::Json(json))
            }

            "gpu/pressure" => {
                let pressure = self.manager.pressure();
                Ok(CommandResult::Json(serde_json::json!({
                    "pressure": pressure,
                })))
            }

            _ => Err(format!("Unknown GPU command: {command}")),
        }
    }

    fn as_any(&self) -> &dyn Any { self }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_gpu_module() -> GpuModule {
        let manager = Arc::new(GpuMemoryManager::detect());
        GpuModule::new(manager)
    }

    #[tokio::test]
    async fn test_gpu_stats() {
        let module = test_gpu_module();
        let result = module.handle_command("gpu/stats", Value::Null).await;
        assert!(result.is_ok());
        if let Ok(CommandResult::Json(json)) = result {
            assert!(json["gpu_name"].is_string());
            assert!(json["total_vram_mb"].is_number());
            assert!(json["pressure"].is_number());
            assert!(json["inference"]["budget_mb"].is_number());
            assert!(json["tts"]["budget_mb"].is_number());
            assert!(json["rendering"]["budget_mb"].is_number());
        }
    }

    #[tokio::test]
    async fn test_gpu_pressure() {
        let module = test_gpu_module();
        let result = module.handle_command("gpu/pressure", Value::Null).await;
        assert!(result.is_ok());
        if let Ok(CommandResult::Json(json)) = result {
            let pressure = json["pressure"].as_f64().unwrap();
            assert!(pressure >= 0.0 && pressure <= 1.0);
        }
    }

    #[tokio::test]
    async fn test_unknown_command() {
        let module = test_gpu_module();
        let result = module.handle_command("gpu/unknown", Value::Null).await;
        assert!(result.is_err());
    }
}
