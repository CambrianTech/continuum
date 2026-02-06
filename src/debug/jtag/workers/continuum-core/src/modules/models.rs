/// ModelsModule — wraps model discovery functionality.
///
/// Handles: models/discover
///
/// Stateless module (like HealthModule) that performs async HTTP requests
/// to provider APIs to discover available models.

use crate::runtime::{ServiceModule, ModuleConfig, ModulePriority, CommandResult, ModuleContext};
use crate::models::{ProviderConfig, discover_all};
use crate::logging::TimingGuard;
use crate::log_info;
use async_trait::async_trait;
use serde_json::Value;
use std::any::Any;

pub struct ModelsModule;

impl ModelsModule {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ServiceModule for ModelsModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "models",
            priority: ModulePriority::Background,
            command_prefixes: &["models/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
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
            "models/discover" => {
                let _timer = TimingGuard::new("module", "models_discover");

                // Parse providers from params
                let providers: Vec<ProviderConfig> = params.get("providers")
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_default();

                let provider_count = providers.len();

                // Run async discovery (all HTTP I/O off main thread)
                let models = discover_all(providers).await;

                let model_count = models.len();
                log_info!("module", "models",
                    "Discovered {} models from {} providers", model_count, provider_count);

                Ok(CommandResult::Json(serde_json::json!({
                    "models": models,
                    "count": model_count,
                    "providers": provider_count
                })))
            }

            _ => Err(format!("Unknown models command: {command}")),
        }
    }

    fn as_any(&self) -> &dyn Any { self }
}
