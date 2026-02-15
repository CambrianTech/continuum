//! RuntimeModule — Exposes runtime metrics and control via IPC.
//!
//! Enables AI-driven system management (Ares pattern):
//! - runtime/metrics/all: Get stats for all modules
//! - runtime/metrics/module: Get stats for specific module
//! - runtime/metrics/slow: List recent slow commands
//! - runtime/list: List all modules with their configs
//!
//! The runtime automatically tracks timing for ALL commands.
//! This module just exposes that data via queryable commands.

use crate::runtime::{
    CommandResult, ModuleConfig, ModuleContext, ModulePriority, ModuleRegistry, ServiceModule,
};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::any::Any;
use std::sync::Arc;
use tokio::sync::OnceCell;

pub struct RuntimeModule {
    /// Reference to registry for querying metrics (set during initialize)
    registry: OnceCell<Arc<ModuleRegistry>>,
}

impl RuntimeModule {
    pub fn new() -> Self {
        Self {
            registry: OnceCell::new(),
        }
    }
}

#[async_trait]
impl ServiceModule for RuntimeModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "runtime",
            priority: ModulePriority::Normal,
            command_prefixes: &["runtime/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
        }
    }

    async fn initialize(&self, ctx: &ModuleContext) -> Result<(), String> {
        // Store registry reference for metric queries
        self.registry
            .set(ctx.registry.clone())
            .map_err(|_| "RuntimeModule already initialized")?;
        Ok(())
    }

    async fn handle_command(&self, command: &str, params: Value) -> Result<CommandResult, String> {
        let registry = self
            .registry
            .get()
            .ok_or("RuntimeModule not initialized")?;

        match command {
            // Get stats for ALL modules
            "runtime/metrics/all" => {
                let module_names = registry.module_names();
                let mut stats = Vec::new();

                for name in module_names {
                    if let Some(metrics) = registry.get_metrics(&name) {
                        stats.push(metrics.stats());
                    }
                }

                Ok(CommandResult::Json(json!({
                    "modules": stats,
                    "count": stats.len(),
                })))
            }

            // Get stats for specific module
            "runtime/metrics/module" => {
                let module_name = params
                    .get("module")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing 'module' parameter")?;

                let metrics = registry
                    .get_metrics(module_name)
                    .ok_or_else(|| format!("Module '{}' not found", module_name))?;

                CommandResult::json(&metrics.stats())
            }

            // Get recent slow commands
            "runtime/metrics/slow" => {
                let module_names = registry.module_names();
                let mut all_slow = Vec::new();

                for name in module_names {
                    if let Some(metrics) = registry.get_metrics(&name) {
                        let slow = metrics.slow_commands();
                        for timing in slow {
                            all_slow.push(json!({
                                "module": name,
                                "command": timing.command,
                                "total_ms": timing.total_time_ms,
                                "execute_ms": timing.execute_time_ms,
                                "queue_ms": timing.queue_time_ms,
                            }));
                        }
                    }
                }

                // Sort by total_ms descending
                all_slow.sort_by(|a, b| {
                    let a_ms = a["total_ms"].as_u64().unwrap_or(0);
                    let b_ms = b["total_ms"].as_u64().unwrap_or(0);
                    b_ms.cmp(&a_ms)
                });

                Ok(CommandResult::Json(json!({
                    "slow_commands": all_slow,
                    "count": all_slow.len(),
                    "threshold_ms": 50,
                })))
            }

            // List all modules with configs
            "runtime/list" => {
                let module_names = registry.module_names();
                let mut modules = Vec::new();

                for name in module_names {
                    if let Some(config) = registry.get_config(&name) {
                        modules.push(json!({
                            "name": config.name,
                            "priority": format!("{:?}", config.priority),
                            "command_prefixes": config.command_prefixes,
                            "needs_dedicated_thread": config.needs_dedicated_thread,
                            "max_concurrency": config.max_concurrency,
                        }));
                    }
                }

                Ok(CommandResult::Json(json!({
                    "modules": modules,
                    "count": modules.len(),
                })))
            }

            _ => Err(format!("Unknown runtime command: {command}")),
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_module_config() {
        let module = RuntimeModule::new();
        let config = module.config();
        assert_eq!(config.name, "runtime");
        assert!(config.command_prefixes.contains(&"runtime/"));
    }
}
