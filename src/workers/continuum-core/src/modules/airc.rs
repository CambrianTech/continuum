//! ServiceModule adapter for Rust-native AIRC commands.

use crate::airc::{
    AircQueueClient, AircQueueListRequest, AircQueueScanParams, CliAircQueueClient,
    TokioAircCommandRunner,
};
use crate::runtime::{
    CommandResult, CommandSchema, ModuleConfig, ModuleContext, ModulePriority, ParamSchema,
    ServiceModule,
};
use async_trait::async_trait;
use serde_json::Value;
use std::any::Any;
use std::sync::Arc;

pub struct AircModule {
    queue_client: Arc<dyn AircQueueClient>,
}

impl AircModule {
    pub fn new() -> Self {
        Self {
            queue_client: Arc::new(CliAircQueueClient::new(TokioAircCommandRunner)),
        }
    }

    pub fn with_queue_client(queue_client: Arc<dyn AircQueueClient>) -> Self {
        Self { queue_client }
    }
}

impl Default for AircModule {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ServiceModule for AircModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "airc",
            priority: ModulePriority::Normal,
            command_prefixes: &["airc/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 4,
            tick_interval: None,
        }
    }

    async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
        Ok(())
    }

    async fn handle_command(&self, command: &str, params: Value) -> Result<CommandResult, String> {
        match command {
            "airc/queue-scan" => {
                let params: AircQueueScanParams = serde_json::from_value(params)
                    .map_err(|e| format!("invalid airc/queue-scan params: {e}"))?;
                let request = AircQueueListRequest::try_from(params)?;
                let result = self.queue_client.list_queue(request).await;
                CommandResult::json(&result)
            }
            _ => Err(format!("Unknown airc command: {command}")),
        }
    }

    fn command_schemas(&self) -> Vec<CommandSchema> {
        vec![CommandSchema {
            name: "airc/queue-scan",
            description: "Rust-native AIRC queue scan for no-Node agent flywheel polling.",
            params: vec![
                ParamSchema {
                    name: "repo",
                    param_type: "string",
                    required: true,
                    description: "GitHub repo in owner/name form, e.g. CambrianTech/continuum.",
                },
                ParamSchema {
                    name: "limit",
                    param_type: "number",
                    required: false,
                    description: "Maximum cards to return, 1..100.",
                },
                ParamSchema {
                    name: "owner",
                    param_type: "string",
                    required: false,
                    description: "Optional queue owner filter.",
                },
                ParamSchema {
                    name: "status",
                    param_type: "string",
                    required: false,
                    description: "Optional queue status filter.",
                },
                ParamSchema {
                    name: "airc_bin",
                    param_type: "string",
                    required: false,
                    description: "Optional AIRC binary path; defaults to PATH lookup.",
                },
                ParamSchema {
                    name: "timeout_ms",
                    param_type: "number",
                    required: false,
                    description: "Command timeout in milliseconds, 100..60000.",
                },
            ],
        }]
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::airc::AircQueueScanResult;
    use serde_json::json;

    struct FakeQueueClient;

    #[async_trait]
    impl AircQueueClient for FakeQueueClient {
        async fn list_queue(&self, request: AircQueueListRequest) -> AircQueueScanResult {
            let command = request.args();
            AircQueueScanResult {
                ok: true,
                repo: request.repo,
                card_count: 0,
                statuses: Vec::new(),
                owners: Vec::new(),
                command,
                stdout_bytes: 0,
                stderr: String::new(),
                queue: None,
                error: None,
            }
        }
    }

    #[tokio::test]
    async fn queue_scan_command_uses_queue_client() {
        let module = AircModule::with_queue_client(Arc::new(FakeQueueClient));
        let result = module
            .handle_command(
                "airc/queue-scan",
                json!({
                    "repo": "CambrianTech/continuum",
                    "limit": 2
                }),
            )
            .await
            .unwrap();

        let CommandResult::Json(value) = result else {
            panic!("expected JSON result");
        };
        assert_eq!(value["ok"], true);
        assert_eq!(value["repo"], "CambrianTech/continuum");
        assert_eq!(value["command"][0], "queue");
        assert_eq!(value["command"][1], "list");
    }
}
