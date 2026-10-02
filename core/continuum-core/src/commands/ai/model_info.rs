//! `ai/model-info` — resolve the [`ModelInfo`] for a specific provider+model.
//!
//! Called once at persona boot — the PRG caches the returned struct and passes it
//! through the turn, eliminating every ad-hoc lookup (context window, slow-local
//! detection, etc.). One struct, one source of truth.

use std::sync::Arc;

use tokio::sync::RwLock;

use crate::ai::adapter::InferenceDevice;
use crate::ai::types::ModelInfo;
use crate::ai::AdapterRegistry;

/// Params for `ai/model-info`: specify a provider or exact model ID. A provider
/// without a model uses that adapter's default; omitting both is rejected.
#[derive(
    Debug, Clone, Default, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema,
)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/ai/AiModelInfoParams.ts"
)]
pub struct AiModelInfoParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub model: Option<String>,
}

/// Result of `ai/model-info`.
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/ai/AiModelInfoResult.ts"
)]
pub struct AiModelInfoResult {
    /// Provider id that resolved the model.
    pub provider: String,
    /// The resolved model descriptor.
    pub model_info: ModelInfo,
}

crate::action_command! {
    /// Resolve the canonical [`ModelInfo`] for a provider+model (context window,
    /// modalities, pricing, slow-local flag, ...). Requires the exact model id
    /// from the provider's catalog; unrelated metadata is never a substitute.
    /// Fails loud if no provider/model can be resolved. Gated `Privileged`.
    pub struct AiModelInfo { registry: Arc<RwLock<AdapterRegistry>> }
    name: "ai/model-info",
    access: Privileged,
    params: AiModelInfoParams,
    output: AiModelInfoResult,
    run(this, _ctx, p) => {
        // Lease the selected adapter before catalog I/O so a slow provider cannot
        // retain the registry read lock and block binding updates for other users.
        let (provider_id, adapter) = {
            let registry = this.registry.read().await;
            registry
                .select_arc(p.provider.as_deref(), p.model.as_deref(), InferenceDevice::default())
                .ok_or_else(|| "No adapter available for requested provider/model".to_string())?
        };

        let models = adapter.get_available_models().await;
        let model_name = p.model.as_deref().unwrap_or_else(|| adapter.default_model());

        let info = exact_model_info(&models, model_name)
            .cloned()
            .ok_or_else(|| {
                format!("No model info available for {}/{}", provider_id, model_name)
            })?;

        Ok(AiModelInfoResult { provider: provider_id, model_info: info })
    }
}

/// Metadata can grant native media capabilities, so aliases require an explicit
/// provider resolution before this boundary, never substring/catalog-order choice.
pub(super) fn exact_model_info<'a>(models: &'a [ModelInfo], id: &str) -> Option<&'a ModelInfo> {
    models.iter().find(|model| model.id == id)
}
