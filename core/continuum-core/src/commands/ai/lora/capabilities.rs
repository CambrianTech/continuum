//! `ai/lora/capabilities` — each provider's LoRA serving capability.

use std::sync::Arc;

use tokio::sync::RwLock;

use crate::ai::AdapterRegistry;
use crate::commands::ai::AiRegistryQueryParams;

/// One provider's LoRA capability descriptor.
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/ai/LoraProviderCapabilities.ts"
)]
pub struct LoraProviderCapabilities {
    pub provider: String,
    /// Debug-rendered [`LoRACapabilities`](crate::ai::adapter::LoRACapabilities)
    /// (`None` | `SingleAdapter` | `MultiLayerPaging { .. }`).
    pub capabilities: String,
}

/// Result of `ai/lora/capabilities`.
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/ai/AiLoraCapabilitiesResult.ts"
)]
pub struct AiLoraCapabilitiesResult {
    pub providers: Vec<LoraProviderCapabilities>,
}

crate::action_command! {
    /// Report each available provider's LoRA serving capability — none,
    /// single-adapter, or multi-layer paging with hot-swap. Read-only substrate
    /// introspection, gated `Privileged`.
    pub struct AiLoraCapabilities { registry: Arc<RwLock<AdapterRegistry>> }
    name: "ai/lora/capabilities",
    access: Privileged,
    params: AiRegistryQueryParams,
    output: AiLoraCapabilitiesResult,
    run(this, _ctx, _p) => {
        let registry = this.registry.read().await;
        let available = registry.available();

        let mut providers = Vec::new();
        for id in &available {
            if let Some(adapter) = registry.get(id) {
                providers.push(LoraProviderCapabilities {
                    provider: id.to_string(),
                    capabilities: format!("{:?}", adapter.lora_capabilities()),
                });
            }
        }

        Ok(AiLoraCapabilitiesResult { providers })
    }
}
