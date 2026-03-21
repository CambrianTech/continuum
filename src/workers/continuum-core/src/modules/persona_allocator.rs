//! PersonaAllocatorModule — IPC command for hardware-aware persona allocation.
//!
//! Commands:
//! - `persona/allocate`: Given available API keys, returns optimal persona allocations
//!   based on detected GPU hardware and VRAM budget.
//!
//! This is the single source of truth for "which personas should exist on this machine."
//! TypeScript calls this at seed time AND at runtime (when API keys are added/removed).

use crate::gpu::GpuMemoryManager;
use crate::persona::{allocate_personas, load_catalog};
use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use async_trait::async_trait;
use serde_json::Value;
use std::any::Any;
use std::sync::Arc;

pub struct PersonaAllocatorModule {
    gpu_manager: Arc<GpuMemoryManager>,
}

impl PersonaAllocatorModule {
    pub fn new(gpu_manager: Arc<GpuMemoryManager>) -> Self {
        Self { gpu_manager }
    }
}

#[async_trait]
impl ServiceModule for PersonaAllocatorModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "persona_allocator",
            priority: ModulePriority::Normal,
            command_prefixes: &["persona/"],
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
            "persona/allocate" => {
                // Extract available API key names from params
                let api_keys: Vec<String> = params
                    .get("availableApiKeys")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default();

                let catalog = load_catalog();
                let result = allocate_personas(&self.gpu_manager, &api_keys, &catalog);

                let json = serde_json::to_value(&result)
                    .map_err(|e| format!("Failed to serialize allocation result: {e}"))?;
                Ok(CommandResult::Json(json))
            }

            "persona/catalog" => {
                // Return the raw catalog (for UI display)
                let catalog = load_catalog();
                let json = serde_json::to_value(&catalog)
                    .map_err(|e| format!("Failed to serialize catalog: {e}"))?;
                Ok(CommandResult::Json(json))
            }

            _ => Err(format!("Unknown persona command: {command}")),
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_module() -> PersonaAllocatorModule {
        let manager = Arc::new(GpuMemoryManager::detect());
        PersonaAllocatorModule::new(manager)
    }

    #[tokio::test]
    async fn test_allocate_no_keys() {
        let module = test_module();
        let params = serde_json::json!({ "availableApiKeys": [] });
        let result = module.handle_command("persona/allocate", params).await;
        assert!(result.is_ok());
        if let Ok(CommandResult::Json(json)) = result {
            assert!(json["allocations"].is_array());
            assert!(json["summary"].is_array());
            assert!(json["gpuName"].is_string());
            assert!(json["localModel"].is_string());
        }
    }

    #[tokio::test]
    async fn test_allocate_with_keys() {
        let module = test_module();
        let params = serde_json::json!({
            "availableApiKeys": ["ANTHROPIC_API_KEY", "DEEPSEEK_API_KEY"]
        });
        let result = module.handle_command("persona/allocate", params).await;
        assert!(result.is_ok());
        if let Ok(CommandResult::Json(json)) = result {
            let allocations = json["allocations"].as_array().unwrap();
            // Should have Anthropic personas
            assert!(allocations.iter().any(|a| {
                a["apiKeyEnv"].as_str() == Some("ANTHROPIC_API_KEY")
            }));
        }
    }

    #[tokio::test]
    async fn test_catalog() {
        let module = test_module();
        let result = module
            .handle_command("persona/catalog", Value::Null)
            .await;
        assert!(result.is_ok());
        if let Ok(CommandResult::Json(json)) = result {
            let entries = json.as_array().unwrap();
            assert!(!entries.is_empty());
        }
    }

    #[tokio::test]
    async fn test_unknown_command() {
        let module = test_module();
        let result = module
            .handle_command("persona/unknown", Value::Null)
            .await;
        assert!(result.is_err());
    }
}
