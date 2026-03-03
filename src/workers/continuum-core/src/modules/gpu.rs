//! GpuModule — IPC commands for GPU memory management.
//!
//! Commands:
//! - `gpu/stats`: Full GPU stats snapshot (total VRAM, per-subsystem budgets/usage, pressure)
//! - `gpu/pressure`: Quick pressure query (0.0-1.0)
//! - `gpu/set-budget`: Set subsystem budget (params: subsystem, budgetMb). Returns stats snapshot.
//! - `gpu/eviction-registry`: Full eviction registry snapshot (all tracked consumers)
//! - `gpu/eviction-candidates`: Sorted eviction candidates (highest score first)
//!
//! Follows the HealthModule pattern: stateless handler wrapping shared state.

use crate::gpu::{GpuMemoryManager, GpuSubsystem};
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
        params: Value,
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

            "gpu/set-budget" => {
                let subsystem_name = params.get("subsystem")
                    .and_then(|v| v.as_str())
                    .ok_or("gpu/set-budget requires 'subsystem' string param")?;

                let budget_mb = params.get("budgetMb")
                    .and_then(|v| v.as_f64())
                    .ok_or("gpu/set-budget requires 'budgetMb' number param")?;

                if budget_mb <= 0.0 {
                    return Err("budgetMb must be > 0".to_string());
                }

                let subsystem = GpuSubsystem::from_name(subsystem_name)
                    .ok_or_else(|| format!(
                        "Unknown subsystem '{}'. Valid: rendering, inference, tts",
                        subsystem_name
                    ))?;

                let budget_bytes = (budget_mb * 1024.0 * 1024.0) as u64;
                self.manager.set_budget(subsystem, budget_bytes);

                // Return fresh stats snapshot so caller sees the result
                let stats = self.manager.stats();
                let json = serde_json::to_value(stats)
                    .map_err(|e| format!("Failed to serialize GPU stats: {e}"))?;
                Ok(CommandResult::Json(json))
            }

            "gpu/eviction-registry" => {
                let snapshot = self.manager.eviction_registry.snapshot();
                let json = serde_json::to_value(snapshot)
                    .map_err(|e| format!("Failed to serialize eviction registry: {e}"))?;
                Ok(CommandResult::Json(json))
            }

            "gpu/eviction-candidates" => {
                let candidates = self.manager.eviction_registry.candidates();
                let json = serde_json::to_value(candidates)
                    .map_err(|e| format!("Failed to serialize eviction candidates: {e}"))?;
                Ok(CommandResult::Json(json))
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
    async fn test_eviction_registry_empty() {
        let module = test_gpu_module();
        let result = module.handle_command("gpu/eviction-registry", Value::Null).await;
        assert!(result.is_ok());
        if let Ok(CommandResult::Json(json)) = result {
            assert_eq!(json["entries"].as_array().unwrap().len(), 0);
            assert_eq!(json["total_tracked_bytes"].as_u64().unwrap(), 0);
            assert_eq!(json["evictable_count"].as_u64().unwrap(), 0);
        }
    }

    #[tokio::test]
    async fn test_eviction_registry_with_entries() {
        let module = test_gpu_module();
        use crate::gpu::{make_entry, GpuPriority};

        module.manager.eviction_registry.register(
            make_entry("candle:llama", "Llama 3.2", GpuPriority::Interactive, 3_000_000_000)
        );

        let result = module.handle_command("gpu/eviction-registry", Value::Null).await;
        assert!(result.is_ok());
        if let Ok(CommandResult::Json(json)) = result {
            assert_eq!(json["entries"].as_array().unwrap().len(), 1);
            assert_eq!(json["total_tracked_bytes"].as_u64().unwrap(), 3_000_000_000);
            assert_eq!(json["evictable_count"].as_u64().unwrap(), 1);
        }
    }

    #[tokio::test]
    async fn test_eviction_candidates_empty() {
        let module = test_gpu_module();
        let result = module.handle_command("gpu/eviction-candidates", Value::Null).await;
        assert!(result.is_ok());
        if let Ok(CommandResult::Json(json)) = result {
            assert!(json.as_array().unwrap().is_empty());
        }
    }

    #[tokio::test]
    async fn test_eviction_candidates_excludes_realtime() {
        let module = test_gpu_module();
        use crate::gpu::{make_entry, GpuPriority};

        module.manager.eviction_registry.register(
            make_entry("render:targets", "Render Targets", GpuPriority::Realtime, 100_000_000)
        );
        module.manager.eviction_registry.register(
            make_entry("candle:llama", "Llama 3.2", GpuPriority::Interactive, 3_000_000_000)
        );

        let result = module.handle_command("gpu/eviction-candidates", Value::Null).await;
        assert!(result.is_ok());
        if let Ok(CommandResult::Json(json)) = result {
            let candidates = json.as_array().unwrap();
            assert_eq!(candidates.len(), 1, "Realtime should be excluded from candidates");
            assert_eq!(candidates[0]["id"].as_str().unwrap(), "candle:llama");
        }
    }

    #[tokio::test]
    async fn test_set_budget() {
        let module = test_gpu_module();
        let params = serde_json::json!({
            "subsystem": "inference",
            "budgetMb": 2048.0
        });
        let result = module.handle_command("gpu/set-budget", params).await;
        assert!(result.is_ok());
        if let Ok(CommandResult::Json(json)) = result {
            // Returns full stats snapshot with updated budget
            assert_eq!(json["inference"]["budget_mb"].as_f64().unwrap(), 2048.0);
        }
    }

    #[tokio::test]
    async fn test_set_budget_invalid_subsystem() {
        let module = test_gpu_module();
        let params = serde_json::json!({
            "subsystem": "nonexistent",
            "budgetMb": 100.0
        });
        let result = module.handle_command("gpu/set-budget", params).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown subsystem"));
    }

    #[tokio::test]
    async fn test_set_budget_invalid_amount() {
        let module = test_gpu_module();
        let params = serde_json::json!({
            "subsystem": "tts",
            "budgetMb": -50.0
        });
        let result = module.handle_command("gpu/set-budget", params).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("must be > 0"));
    }

    #[tokio::test]
    async fn test_set_budget_missing_params() {
        let module = test_gpu_module();
        let result = module.handle_command("gpu/set-budget", Value::Null).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_unknown_command() {
        let module = test_gpu_module();
        let result = module.handle_command("gpu/unknown", Value::Null).await;
        assert!(result.is_err());
    }
}
