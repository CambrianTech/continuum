//! `ai/lora/list` — every LoRA adapter known to every available provider.

use std::sync::Arc;

use tokio::sync::RwLock;

use crate::ai::AdapterRegistry;
use crate::commands::ai::AiRegistryQueryParams;

/// One LoRA adapter as reported by its host provider.
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/ai/LoraAdapterView.ts"
)]
pub struct LoraAdapterView {
    pub provider: String,
    pub adapter_id: String,
    pub path: String,
    #[ts(type = "number")]
    pub scale: f64,
    pub loaded: bool,
    pub active: bool,
}

/// Result of `ai/lora/list`.
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/ai/AiLoraListResult.ts"
)]
pub struct AiLoraListResult {
    pub adapters: Vec<LoraAdapterView>,
    pub count: usize,
}

crate::action_command! {
    /// List every LoRA adapter known to every available provider, with its
    /// id, path, scale, and loaded/active state — the genome page-table view.
    /// Read-only substrate introspection, gated `Privileged`.
    pub struct AiLoraList { registry: Arc<RwLock<AdapterRegistry>> }
    name: "ai/lora/list",
    access: Privileged,
    params: AiRegistryQueryParams,
    output: AiLoraListResult,
    run(this, _ctx, _p) => {
        let registry = this.registry.read().await;
        let available = registry.available();

        let mut adapters = Vec::new();
        for id in &available {
            if let Some(adapter) = registry.get(id) {
                for lora in adapter.list_lora_adapters() {
                    adapters.push(LoraAdapterView {
                        provider: id.to_string(),
                        adapter_id: lora.adapter_id,
                        path: lora.path,
                        scale: lora.scale,
                        loaded: lora.loaded,
                        active: lora.active,
                    });
                }
            }
        }

        let count = adapters.len();
        Ok(AiLoraListResult { adapters, count })
    }
}
