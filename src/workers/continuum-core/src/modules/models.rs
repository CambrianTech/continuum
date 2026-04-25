//! ModelsModule — wraps model discovery functionality.
//!
//! Handles: models/discover
//!
//! Stateless module (like HealthModule) that performs async HTTP requests
//! to provider APIs to discover available models.

use crate::log_info;
use crate::logging::TimingGuard;
use crate::models::{discover_all, ProviderConfig};
use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use crate::utils::params::Params;
use async_trait::async_trait;
use serde_json::Value;
use std::any::Any;

pub struct ModelsModule;

impl Default for ModelsModule {
    fn default() -> Self {
        Self
    }
}

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
            tick_interval: None,
        }
    }

    async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
        Ok(())
    }

    async fn handle_command(&self, command: &str, params: Value) -> Result<CommandResult, String> {
        match command {
            "models/discover" => {
                let _timer = TimingGuard::new("module", "models_discover");
                let p = Params::new(&params);
                let providers: Vec<ProviderConfig> = p.json_or("providers");

                let provider_count = providers.len();

                // Run async discovery (all HTTP I/O off main thread)
                let models = discover_all(providers).await;

                let model_count = models.len();
                log_info!(
                    "module",
                    "models",
                    "Discovered {} models from {} providers",
                    model_count,
                    provider_count
                );

                Ok(CommandResult::Json(serde_json::json!({
                    "models": models,
                    "count": model_count,
                    "providers": provider_count
                })))
            }

            // Lookup the canonical capability vocabulary for a model from
            // models.toml. Returns kebab-case strings matching the serde
            // rename on `model_registry::types::Capability` ("vision",
            // "audio-input", "tool-use", "streaming", etc.).
            //
            // Why this exists: callers (TS PRG) need to declare a model's
            // capabilities WITH the request when invoking
            // `cognition/respond`, so Rust never has to do a global
            // registry lookup mid-inference (which silently returned
            // empty caps when keys drifted, demoting image bytes to
            // text markers — vision encoder never fired). PRG calls
            // this once per persona at construction and caches.
            //
            // Hard error when the model id isn't in the registry — that
            // means models.toml doesn't know about it and the persona's
            // configuration is broken. No silent empty-list fallback;
            // the contract is "if you ask, you get answers or you get
            // an error you can debug."
            "models/capabilities" => {
                let _timer = TimingGuard::new("module", "models_capabilities");
                let p = Params::new(&params);
                let model_id = p.str("model_id")?;

                let registry = crate::model_registry::try_global().ok_or(
                    "model_registry not initialized — models.toml never loaded".to_string(),
                )?;
                let model = registry.model(model_id).ok_or_else(|| {
                    format!(
                        "model id '{}' not in registry — add it to models.toml",
                        model_id
                    )
                })?;

                // Serialize each Capability via its serde rename so the
                // wire string matches what the cognition/respond IPC
                // handler later parses back via from_value.
                let caps: Vec<String> = model
                    .capabilities
                    .iter()
                    .filter_map(|c| serde_json::to_value(c).ok())
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect();

                Ok(CommandResult::Json(serde_json::json!({
                    "modelId": model_id,
                    "capabilities": caps,
                })))
            }

            _ => Err(format!("Unknown models command: {command}")),
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
