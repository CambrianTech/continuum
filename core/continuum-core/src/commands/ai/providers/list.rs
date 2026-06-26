//! `ai/providers/list` — enumerate registered AI providers and their capabilities.

use std::sync::Arc;

use tokio::sync::RwLock;

use crate::ai::AdapterRegistry;
use crate::commands::ai::AiRegistryQueryParams;

/// Capability summary for one provider — the subset cognition + clients read
/// when choosing a provider/model.
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/ai/ProviderCapabilitiesView.ts")]
pub struct ProviderCapabilitiesView {
    pub text_generation: bool,
    pub chat: bool,
    pub tool_use: bool,
    pub vision: bool,
    pub streaming: bool,
    pub embeddings: bool,
    pub is_local: bool,
    #[ts(type = "number")]
    pub max_context_window: u32,
}

/// One registered provider with its identity + capabilities.
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/ai/ProviderInfo.ts")]
pub struct ProviderInfo {
    pub id: String,
    pub name: String,
    pub default_model: String,
    pub capabilities: ProviderCapabilitiesView,
}

/// Result of `ai/providers/list`.
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/ai/AiProvidersListResult.ts")]
pub struct AiProvidersListResult {
    /// Provider ids currently available (registered + reachable).
    pub available: Vec<String>,
    /// Full per-provider info.
    pub providers: Vec<ProviderInfo>,
    /// Convenience count of `available`.
    pub count: usize,
}

crate::action_command! {
    /// List the AI providers currently registered and available, with each
    /// provider's id, name, default model, and capability flags (text/chat/tool/
    /// vision/streaming/embeddings, local-ness, context window). Read-only
    /// substrate introspection, gated `Privileged`.
    pub struct AiProvidersList { registry: Arc<RwLock<AdapterRegistry>> }
    name: "ai/providers/list",
    access: Privileged,
    params: AiRegistryQueryParams,
    output: AiProvidersListResult,
    run(this, _ctx, _p) => {
        let registry = this.registry.read().await;
        let available: Vec<String> = registry.available().iter().map(|s| s.to_string()).collect();

        let mut providers = Vec::new();
        for id in &available {
            if let Some(adapter) = registry.get(id) {
                let caps = adapter.capabilities();
                providers.push(ProviderInfo {
                    id: id.clone(),
                    name: adapter.name().to_string(),
                    default_model: adapter.default_model().to_string(),
                    capabilities: ProviderCapabilitiesView {
                        text_generation: caps.supports_text_generation,
                        chat: caps.supports_chat,
                        tool_use: caps.supports_tool_use,
                        vision: caps.supports_vision,
                        streaming: caps.supports_streaming,
                        embeddings: caps.supports_embeddings,
                        is_local: caps.is_local,
                        max_context_window: caps.max_context_window,
                    },
                });
            }
        }

        let count = available.len();
        Ok(AiProvidersListResult { available, providers, count })
    }
}
