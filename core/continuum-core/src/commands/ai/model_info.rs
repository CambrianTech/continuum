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

/// Params for `ai/model-info`: an optional provider/model hint. Omitting both
/// resolves the registry's default provider + that provider's default model.
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
    /// modalities, pricing, slow-local flag, ...). Fuzzy-matches the model id
    /// against the provider's catalog, falling back to the provider's first model.
    /// Fails loud if no provider/model can be resolved. Gated `Privileged`.
    pub struct AiModelInfo { registry: Arc<RwLock<AdapterRegistry>> }
    name: "ai/model-info",
    access: Privileged,
    params: AiModelInfoParams,
    output: AiModelInfoResult,
    run(this, _ctx, p) => {
        let registry = this.registry.read().await;
        let (provider_id, adapter) = registry
            .select(p.provider.as_deref(), p.model.as_deref(), InferenceDevice::default())
            .ok_or_else(|| "No adapter available for requested provider/model".to_string())?;

        let models = adapter.get_available_models().await;
        let model_name = p.model.as_deref().unwrap_or_else(|| adapter.default_model());

        // Fuzzy-match the requested name against the catalog (either-direction
        // substring), else fall back to the provider's first model.
        let info = models
            .iter()
            .find(|m| {
                m.id.to_lowercase().contains(&model_name.to_lowercase())
                    || model_name.to_lowercase().contains(&m.id.to_lowercase())
            })
            .or_else(|| models.first())
            .cloned()
            .ok_or_else(|| {
                format!("No model info available for {}/{}", provider_id, model_name)
            })?;

        Ok(AiModelInfoResult { provider: provider_id.to_string(), model_info: info })
    }
}
