//! `ai/models/list` — every model exposed by every available provider.

use std::sync::Arc;

use tokio::sync::RwLock;

use crate::ai::types::ModelInfo;
use crate::ai::AdapterRegistry;
use crate::commands::ai::AiRegistryQueryParams;

/// Result of `ai/models/list`.
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/ai/AiModelsListResult.ts"
)]
pub struct AiModelsListResult {
    /// Flattened catalog across all available providers.
    pub models: Vec<ModelInfo>,
    /// Convenience count of `models`.
    pub count: usize,
}

crate::action_command! {
    /// List every model offered by every currently-available provider, flattened
    /// into one catalog. Read-only substrate introspection, gated `Privileged`.
    pub struct AiModelsList { registry: Arc<RwLock<AdapterRegistry>> }
    name: "ai/models/list",
    access: Privileged,
    params: AiRegistryQueryParams,
    output: AiModelsListResult,
    run(this, _ctx, _p) => {
        let catalogs = AdapterRegistry::query_available(&this.registry, |_, adapter| async move {
            adapter.get_available_models().await
        }).await;
        let models: Vec<_> = catalogs.into_iter().flatten().collect();
        let count = models.len();
        Ok(AiModelsListResult { models, count })
    }
}
