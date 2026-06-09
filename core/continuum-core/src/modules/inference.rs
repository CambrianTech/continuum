//! InferenceModule — IPC commands for inference-side system facts.
//!
//! Commands:
//! - `inference/capacity`: Local inference concurrency cap (how many parallel
//!   generate requests the hardware can handle simultaneously). Scaled by
//!   RAM. Matches `n_seq_max` used by the BatchScheduler. TS's
//!   InferenceCoordinator reads this at startup to stop duplicating the
//!   formula.
//!
//! This module exists so TypeScript-side admission control (TS
//! InferenceCoordinator) and Rust-side scheduler sizing stay on a single
//! source of truth — see issue #887. Previously the TS side had its own
//! `localInferenceCapacity()` RAM formula and the Rust side had
//! `concurrent_inference_permits()`; they happened to agree but were
//! trivially drift-prone.
//!
//! Follows the GpuModule / SystemResourceModule pattern: stateless handler
//! over a shared-state-free compute.

use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use crate::system_resources::local_inference_capacity;
use async_trait::async_trait;
use serde_json::Value;
use std::any::Any;

pub struct InferenceModule;

impl InferenceModule {
    pub fn new() -> Self {
        Self
    }
}

impl Default for InferenceModule {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ServiceModule for InferenceModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "inference",
            priority: ModulePriority::Normal,
            command_prefixes: &["inference/"],
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
            "inference/capacity" => {
                // Adapter-owned source of truth: at the boundary here we
                // call the system_resources util directly. `CandleAdapter::
                // inference_capacity()` is just the same value; wrapping
                // that would require holding an adapter reference for no
                // semantic gain today. When capacity becomes dynamic
                // (pressure-reactive), this handler is where it gets read.
                let capacity = local_inference_capacity();
                Ok(CommandResult::Json(serde_json::json!({
                    "capacity": capacity,
                })))
            }

            _ => Err(format!("Unknown inference command: {command}")),
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
    async fn test_inference_capacity_returns_positive() {
        let module = InferenceModule::new();
        let result = module
            .handle_command("inference/capacity", Value::Null)
            .await;
        assert!(result.is_ok());
        if let Ok(CommandResult::Json(json)) = result {
            let capacity = json["capacity"].as_u64().unwrap();
            assert!(capacity >= 1, "capacity must be >= 1, got {capacity}");
        }
    }

    #[tokio::test]
    async fn test_unknown_inference_command() {
        let module = InferenceModule::new();
        let result = module
            .handle_command("inference/unknown", Value::Null)
            .await;
        assert!(result.is_err());
    }
}
