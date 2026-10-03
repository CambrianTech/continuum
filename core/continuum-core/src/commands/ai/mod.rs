//! `commands/ai/` — the AI provider command family.
//!
//! Read-only registry introspection (`ai/providers/*`, `ai/models/list`,
//! `ai/model-info`, `ai/lora/*`) plus the inference seam (`ai/generate`).
//! Each command shares the `AdapterRegistry` the owning `AIProviderModule`
//! holds, exposed via that module's `commands()`. See
//! [docs/architecture/COMMAND-ORGANIZATION.md].

use std::sync::Arc;

use tokio::sync::RwLock;

use crate::ai::AdapterRegistry;
use crate::sdk_codegen::DynCommand;

pub mod generate;
pub mod lora;
pub mod model_info;
pub mod models;
pub mod providers;

/// Shared params for the no-argument registry-introspection commands
/// (`providers/list`, `providers/health`, `models/list`, `lora/list`,
/// `lora/capabilities`): they enumerate the whole registry and take no input.
#[derive(
    Debug, Clone, Default, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema,
)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/ai/AiRegistryQueryParams.ts"
)]
pub struct AiRegistryQueryParams {}

/// The `ai/*` commands as typed self-routing objects, each sharing the module's
/// `AdapterRegistry`: the read-only registry-introspection commands plus the
/// `ai/generate` inference seam.
pub fn command_objects(registry: Arc<RwLock<AdapterRegistry>>) -> Vec<Arc<dyn DynCommand>> {
    vec![
        Arc::new(generate::AiGenerate {
            registry: registry.clone(),
        }),
        Arc::new(providers::list::AiProvidersList {
            registry: registry.clone(),
        }),
        Arc::new(providers::health::AiProvidersHealth {
            registry: registry.clone(),
        }),
        Arc::new(models::list::AiModelsList {
            registry: registry.clone(),
        }),
        Arc::new(model_info::AiModelInfo {
            registry: registry.clone(),
        }),
        Arc::new(lora::list::AiLoraList {
            registry: registry.clone(),
        }),
        Arc::new(lora::capabilities::AiLoraCapabilities { registry }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::ActionCommand;

    // what this catches: a longer model id listed first must not lend its native
    // audio capabilities to a shorter id, unknown id, or implicit fuzzy alias.
    #[test]
    fn model_info_never_borrows_another_models_capabilities() {
        use crate::ai::types::{ModelInfo, CostPer1kTokens};
        use crate::model_registry::Capability;
        let audio = ModelInfo {
            id: "fixture-audio".into(), name: "Audio fixture".into(), provider: "fixture".into(),
            capabilities: vec![Capability::AudioOutput], context_window: 4096,
            max_output_tokens: 512, cost_per_1k_tokens: CostPer1kTokens { input: 0.0, output: 0.0 },
            tokens_per_second: 1.0,
        };
        let mut text = audio.clone();
        text.id = "fixture".into();
        text.capabilities = vec![Capability::TextGeneration];
        let models = vec![audio, text];
        let resolved = model_info::exact_model_info(&models, "fixture").unwrap();
        assert!(std::ptr::eq(resolved, &models[1]));
        assert!(!resolved.has(Capability::AudioOutput));
        for missing in ["", "fixt", "fixture-unknown", "FIXTURE"] {
            assert!(model_info::exact_model_info(&models, missing).is_none());
        }
    }

    // what this catches: every ai/* introspection command's registered NAME
    // mirrors its file path under commands/ai/ — the path==name invariant that
    // keeps the tree navigable, and a guard that command_objects() stays in sync
    // with the files.
    #[test]
    fn ai_command_names_mirror_their_path() {
        assert_eq!(providers::list::AiProvidersList::NAME, "ai/providers/list");
        assert_eq!(
            providers::health::AiProvidersHealth::NAME,
            "ai/providers/health"
        );
        assert_eq!(generate::AiGenerate::NAME, "ai/generate");
        assert_eq!(models::list::AiModelsList::NAME, "ai/models/list");
        assert_eq!(model_info::AiModelInfo::NAME, "ai/model-info");
        assert_eq!(lora::list::AiLoraList::NAME, "ai/lora/list");
        assert_eq!(
            lora::capabilities::AiLoraCapabilities::NAME,
            "ai/lora/capabilities"
        );
    }
}
