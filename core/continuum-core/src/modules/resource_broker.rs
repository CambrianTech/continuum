//! ResourceBrokerModule — runtime-owned admission and lease ledger.
//!
//! This wraps `crate::resources::ResourceBroker` as a ServiceModule so TS,
//! commands, and Rust subsystems can share one daemon-shaped resource contract.

use crate::resources::{ResourceAdmissionReport, ResourceBroker, ResourceDemand};
use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use async_trait::async_trait;
use parking_lot::Mutex;
use serde::Deserialize;
use serde_json::Value;
use std::any::Any;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

const SYSTEM_RESOURCE_BROKER_STATE: &str = "system/resource-broker-state";
const SYSTEM_RESOURCE_ADMIT: &str = "system/resource-admit";
const SYSTEM_RESOURCE_RELEASE: &str = "system/resource-release";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AdmitParams {
    demands: Vec<ResourceDemand>,
    #[serde(default)]
    ready_artifact_keys: Vec<String>,
    now_ms: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReleaseParams {
    lease_id: String,
}

pub struct ResourceBrokerModule {
    broker: Arc<Mutex<ResourceBroker>>,
}

impl ResourceBrokerModule {
    pub fn new() -> Self {
        Self {
            broker: Arc::new(Mutex::new(ResourceBroker::local_default())),
        }
    }

    pub fn broker(&self) -> Arc<Mutex<ResourceBroker>> {
        self.broker.clone()
    }
}

impl Default for ResourceBrokerModule {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ServiceModule for ResourceBrokerModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "resource-broker",
            priority: ModulePriority::High,
            command_prefixes: &[
                SYSTEM_RESOURCE_BROKER_STATE,
                SYSTEM_RESOURCE_ADMIT,
                SYSTEM_RESOURCE_RELEASE,
            ],
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
            SYSTEM_RESOURCE_BROKER_STATE => {
                let now_ms = now_ms()?;
                let broker = self.broker.lock();
                CommandResult::json(&serde_json::json!({
                    "laneBudgets": broker.lane_budgets(),
                    "leases": broker.active_leases(now_ms),
                    "reclaimable": broker.reclaimable(now_ms),
                }))
            }
            SYSTEM_RESOURCE_ADMIT => {
                let params: AdmitParams = serde_json::from_value(params)
                    .map_err(|e| format!("resource-broker admit params invalid: {e}"))?;
                let now_ms = params.now_ms.unwrap_or(now_ms()?);
                let report: ResourceAdmissionReport =
                    self.broker
                        .lock()
                        .admit(params.demands, params.ready_artifact_keys, now_ms);
                CommandResult::json(&report)
            }
            SYSTEM_RESOURCE_RELEASE => {
                let params: ReleaseParams = serde_json::from_value(params)
                    .map_err(|e| format!("resource-broker release params invalid: {e}"))?;
                let released = self
                    .broker
                    .lock()
                    .release(&params.lease_id)
                    .map_err(|e| format!("resource-broker release failed: {e:?}"))?;
                CommandResult::json(&released)
            }
            other => Err(format!(
                "resource-broker: unknown command '{other}' (handled: {SYSTEM_RESOURCE_BROKER_STATE}, {SYSTEM_RESOURCE_ADMIT}, {SYSTEM_RESOURCE_RELEASE})"
            )),
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

fn now_ms() -> Result<u64, String> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("system clock before UNIX_EPOCH: {e}"))?;
    u64::try_from(duration.as_millis()).map_err(|_| "system clock millis overflow u64".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn admit_command_uses_one_runtime_owned_lease_ledger() {
        let module = ResourceBrokerModule::new();
        let params = serde_json::json!({
            "nowMs": 100,
            "demands": [
                ResourceDemand::persona_generation("helper", "event-a", 90, 10, 1_000),
                ResourceDemand::persona_generation("planner", "event-a", 89, 10, 1_000)
            ],
            "readyArtifactKeys": []
        });

        let result = module
            .handle_command(SYSTEM_RESOURCE_ADMIT, params)
            .await
            .expect("admit command should succeed");

        let CommandResult::Json(json) = result else {
            panic!("expected JSON result");
        };
        let report: ResourceAdmissionReport =
            serde_json::from_value(json).expect("report should deserialize");
        assert_eq!(report.admitted.len(), 2);
        assert!(report.refused.is_empty());
    }

    #[tokio::test]
    async fn malformed_admit_request_fails_loudly() {
        let module = ResourceBrokerModule::new();
        let result = module
            .handle_command(SYSTEM_RESOURCE_ADMIT, serde_json::json!({}))
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("params invalid"));
    }
}
