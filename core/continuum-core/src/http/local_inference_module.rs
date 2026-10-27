//! The `sentinel/local-inference-*` commands: they report and start THIS crate's local
//! inference HTTP server (`super::port` / `super::start_if_needed`), so they live beside it.
//! The command names are unchanged. The longer prefix wins over the sentinel module's
//! `sentinel/` (the registry routes longest-match-first).

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};

pub struct LocalInferenceModule;

fn endpoint(port: u16) -> Value {
    json!({ "success": true, "port": port, "url": format!("http://127.0.0.1:{}", port) })
}

#[async_trait]
impl ServiceModule for LocalInferenceModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "local-inference-http",
            priority: ModulePriority::Normal,
            command_prefixes: &["sentinel/local-inference-"],
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
            "sentinel/local-inference-port" => Ok(CommandResult::Json(match super::port().await {
                Some(port) => endpoint(port),
                None => json!({ "success": false, "error": "HTTP inference server not started" }),
            })),
            "sentinel/local-inference-start" => {
                Ok(CommandResult::Json(match super::start_if_needed().await {
                    Ok(port) => endpoint(port),
                    Err(e) => json!({ "success": false, "error": e }),
                }))
            }
            _ => Err(format!("Unknown local-inference command: {command}")),
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
