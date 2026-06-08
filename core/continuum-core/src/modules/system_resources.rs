//! SystemResourceModule — IPC commands for CPU, memory, and system resource monitoring.
//!
//! Commands:
//! - `system/cpu`: CPU stats (cores, usage, brand)
//! - `system/memory`: Memory stats (total, used, pressure, swap)
//! - `system/resources`: Full system resource snapshot (CPU + memory + optional processes)
//!
//! Follows the GpuModule pattern: stateless handler wrapping shared state.

use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use crate::system_resources::{MemoryPressureMonitor, SystemResourceMonitor};
use async_trait::async_trait;
use serde_json::Value;
use std::any::Any;
use std::sync::{Arc, OnceLock};

pub struct SystemResourceModule {
    monitor: Arc<SystemResourceMonitor>,
    pressure_monitor: OnceLock<Arc<MemoryPressureMonitor>>,
}

impl SystemResourceModule {
    pub fn new(monitor: Arc<SystemResourceMonitor>) -> Self {
        Self {
            monitor,
            pressure_monitor: OnceLock::new(),
        }
    }

    /// Set the memory pressure monitor reference.
    /// Uses OnceLock so this can be called on &self (through Arc) after registration.
    pub fn set_pressure_monitor(&self, pm: Arc<MemoryPressureMonitor>) {
        let _ = self.pressure_monitor.set(pm);
    }
}

#[async_trait]
impl ServiceModule for SystemResourceModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "system",
            priority: ModulePriority::Normal,
            command_prefixes: &["system/"],
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
            "system/cpu" => {
                // Refresh CPU readings and return
                let snapshot = self.monitor.refresh();
                let json = serde_json::to_value(snapshot.cpu)
                    .map_err(|e| format!("Failed to serialize CPU stats: {e}"))?;
                Ok(CommandResult::Json(json))
            }

            "system/memory" => {
                // Refresh memory readings and return
                let snapshot = self.monitor.refresh();
                let json = serde_json::to_value(snapshot.memory)
                    .map_err(|e| format!("Failed to serialize memory stats: {e}"))?;
                Ok(CommandResult::Json(json))
            }

            "system/resources" => {
                // Full snapshot with optional process listing
                let include_processes = params
                    .get("includeProcesses")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let top_n = params.get("topN").and_then(|v| v.as_u64()).unwrap_or(10) as usize;

                let snapshot = if include_processes {
                    self.monitor.refresh_with_processes(top_n)
                } else {
                    self.monitor.refresh()
                };

                let json = serde_json::to_value(snapshot)
                    .map_err(|e| format!("Failed to serialize system resources: {e}"))?;
                Ok(CommandResult::Json(json))
            }

            "system/pressure" => {
                // Memory pressure snapshot from the autonomous monitor
                if let Some(pm) = self.pressure_monitor.get() {
                    let snapshot = pm.current();
                    let json = serde_json::to_value(snapshot)
                        .map_err(|e| format!("Failed to serialize pressure: {e}"))?;
                    Ok(CommandResult::Json(json))
                } else {
                    Err("Memory pressure monitor not initialized".to_string())
                }
            }

            "system/memory-gate" => {
                // Check if the memory gate is closed (critical pressure sustained).
                // TypeScript side should check this before expensive operations.
                let closed = crate::system_resources::is_memory_gate_closed();
                let json = serde_json::json!({
                    "closed": closed,
                    "pressure": self.pressure_monitor.get().map(|pm| pm.pressure()).unwrap_or(0.0),
                    "rss_bytes": self.pressure_monitor.get().map(|pm| pm.rss_bytes()).unwrap_or(0),
                });
                Ok(CommandResult::Json(json))
            }

            "system/memory-budget" => {
                // Budget snapshot — per-consumer allocation vs actual usage.
                // Human-visible dashboard: priority, budget, usage, headroom, warnings.
                if let Some(pm) = self.pressure_monitor.get() {
                    let snapshot = pm.budget_snapshot();
                    let json = serde_json::to_value(snapshot)
                        .map_err(|e| format!("Failed to serialize budget: {e}"))?;
                    Ok(CommandResult::Json(json))
                } else {
                    Err("Memory pressure monitor not initialized".to_string())
                }
            }

            "system/docker-tier-stats" => {
                // Phase 1 of #1239 — surface Docker storage tier directly,
                // bypassing the (not-yet-instantiated) PressureBroker
                // singleton. `DockerTierPool::snapshot_stats()` does one
                // probe and returns capacity_bytes / used_bytes / pressure
                // / detected. Phase 2 will add the broker singleton + tick
                // loop + alert sinks; Phase 3 will add typed
                // `ResourceError::DiskCapacity` refusal at production hot
                // paths (model pull, container start, image build).
                let stats = crate::modules::docker_tier_pool::DockerTierPool::snapshot_stats();
                let json = serde_json::to_value(&stats)
                    .map_err(|e| format!("Failed to serialize docker-tier-stats: {e}"))?;
                Ok(CommandResult::Json(json))
            }

            _ => Err(format!("Unknown system command: {command}")),
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_module() -> SystemResourceModule {
        let monitor = Arc::new(SystemResourceMonitor::new());
        SystemResourceModule::new(monitor)
    }

    #[tokio::test]
    async fn test_system_cpu() {
        let module = test_module();
        // sysinfo needs a baseline then delta — sleep briefly
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let result = module.handle_command("system/cpu", Value::Null).await;
        assert!(result.is_ok());
        if let Ok(CommandResult::Json(json)) = result {
            assert!(json["physical_cores"].as_u64().unwrap() >= 1);
            assert!(json["logical_cores"].as_u64().unwrap() >= 1);
            assert!(json["brand"].is_string());
            let usage = json["global_usage"].as_f64().unwrap();
            assert!(
                usage >= 0.0 && usage <= 1.0,
                "CPU usage should be 0.0-1.0, got {usage}"
            );
        }
    }

    #[tokio::test]
    async fn test_system_memory() {
        let module = test_module();
        let result = module.handle_command("system/memory", Value::Null).await;
        assert!(result.is_ok());
        if let Ok(CommandResult::Json(json)) = result {
            assert!(json["total_bytes"].as_u64().unwrap() > 0);
            let pressure = json["pressure"].as_f64().unwrap();
            assert!(
                pressure >= 0.0 && pressure <= 1.0,
                "Memory pressure should be 0.0-1.0, got {pressure}"
            );
        }
    }

    #[tokio::test]
    async fn test_system_resources() {
        let module = test_module();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let result = module.handle_command("system/resources", Value::Null).await;
        assert!(result.is_ok());
        if let Ok(CommandResult::Json(json)) = result {
            assert!(json["cpu"]["physical_cores"].as_u64().unwrap() >= 1);
            assert!(json["memory"]["total_bytes"].as_u64().unwrap() > 0);
            assert!(json["timestamp_ms"].as_u64().unwrap() > 0);
            assert!(json["uptime_seconds"].as_u64().unwrap() > 0);
            assert!(
                json["processes"].is_null(),
                "Processes should be null by default"
            );
        }
    }

    #[tokio::test]
    async fn test_system_resources_with_processes() {
        let module = test_module();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let params = serde_json::json!({
            "includeProcesses": true,
            "topN": 5
        });
        let result = module.handle_command("system/resources", params).await;
        assert!(result.is_ok());
        if let Ok(CommandResult::Json(json)) = result {
            assert!(
                json["processes"].is_object(),
                "Processes should be present when requested"
            );
            assert!(json["processes"]["top_by_cpu"].is_array());
            assert!(json["processes"]["top_by_memory"].is_array());
        }
    }

    #[tokio::test]
    async fn test_unknown_command() {
        let module = test_module();
        let result = module.handle_command("system/unknown", Value::Null).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_docker_tier_stats_shape() {
        // Phase 1 of #1239 — verify the IPC always returns the expected
        // shape (capacityBytes, usedBytes, pressure, detected) regardless
        // of whether Docker is installed on the test host. CI runs without
        // Docker, so `detected: false` + zeros is the expected shape.
        let module = test_module();
        let result = module
            .handle_command("system/docker-tier-stats", Value::Null)
            .await;
        assert!(result.is_ok(), "docker-tier-stats should always Ok");
        if let Ok(CommandResult::Json(json)) = result {
            // All four fields must be present so callers can structurally
            // pattern-match on the shape — even when Docker isn't here.
            assert!(json["capacityBytes"].is_number(), "capacityBytes missing");
            assert!(json["usedBytes"].is_number(), "usedBytes missing");
            assert!(json["pressure"].is_number(), "pressure missing");
            assert!(json["detected"].is_boolean(), "detected missing");
            // Pressure must be in [0.0, ∞) — never NaN even when capacity
            // is 0 (the `if cap == 0` guard handles it).
            let pressure = json["pressure"].as_f64().unwrap();
            assert!(pressure.is_finite(), "pressure must not be NaN/Inf");
            assert!(pressure >= 0.0, "pressure must be ≥ 0.0");
        }
    }
}
