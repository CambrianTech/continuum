//! `ai/providers/health` — live health check across registered providers.

use std::sync::Arc;

use tokio::sync::RwLock;

use crate::ai::AdapterRegistry;
use crate::commands::ai::AiRegistryQueryParams;

/// Health snapshot for one provider.
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/ai/ProviderHealth.ts"
)]
pub struct ProviderHealth {
    pub provider: String,
    pub name: String,
    /// Lowercased health state (`healthy` | `degraded` | `unhealthy` | ...).
    pub status: String,
    pub api_available: bool,
    #[ts(type = "number")]
    pub response_time_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub message: Option<String>,
}

/// Result of `ai/providers/health`.
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/ai/AiProvidersHealthResult.ts"
)]
pub struct AiProvidersHealthResult {
    pub providers: Vec<ProviderHealth>,
}

crate::action_command! {
    /// Run a live health check against every available provider and report each
    /// one's status, API reachability, response time, and any message. Performs
    /// network probes, so gated `Privileged`.
    pub struct AiProvidersHealth { registry: Arc<RwLock<AdapterRegistry>> }
    name: "ai/providers/health",
    access: Privileged,
    params: AiRegistryQueryParams,
    output: AiProvidersHealthResult,
    run(this, _ctx, _p) => {
        let registry = this.registry.read().await;
        let available = registry.available();

        let mut providers = Vec::new();
        for id in &available {
            if let Some(adapter) = registry.get(id) {
                let health = adapter.health_check().await;
                providers.push(ProviderHealth {
                    provider: id.to_string(),
                    name: adapter.name().to_string(),
                    status: format!("{:?}", health.status).to_lowercase(),
                    api_available: health.api_available,
                    response_time_ms: health.response_time_ms,
                    message: health.message,
                });
            }
        }

        Ok(AiProvidersHealthResult { providers })
    }
}
