//! Bridge between the `model_registry` crate (the new source of truth)
//! and the legacy `ai::ModelInfo` / `ai::ModelCapability` types that the
//! existing adapter trait returns.
//!
//! Both shapes coexist for this PR:
//! - `model_registry::Model` is the CONFIG-driven value, loaded from TOML.
//! - `ai::ModelInfo` is the WIRE type that adapters return (via `get_available_models()`)
//!   and that ts-rs projects to TypeScript.
//!
//! This module converts one into the other so adapters can stop hand-
//! constructing `ai::ModelInfo` literals and instead consume the registry.
//! A later PR should collapse the two — `ai::ModelInfo` effectively
//! becomes a thin TS-projection of `model_registry::Model` and the bridge
//! goes away. That collapse touches the generated TS types, so it's its
//! own sweep; for now we coexist.

use super::types::{CostPer1kTokens, ModelCapability, ModelInfo};
use crate::model_registry::{Capability, Model};

impl From<&Model> for ModelInfo {
    fn from(m: &Model) -> Self {
        // Display name — fall back to id if TOML didn't supply one.
        // The fallback is intentionally ugly (full id, often dotted
        // hf.co paths) so the empty-name case surfaces at UI time and
        // the TOML gets fixed.
        let name = m.name.clone().unwrap_or_else(|| m.id.clone());

        // Capability mapping:
        //   Registry's closed vocabulary is richer than ai::ModelCapability
        //   and uses "streaming" + "tool-use" as capability entries rather
        //   than bool fields. Here we project back to the legacy shape.
        let mut capabilities: Vec<ModelCapability> = Vec::new();
        for cap in &m.capabilities {
            match cap {
                Capability::TextGeneration => capabilities.push(ModelCapability::TextGeneration),
                Capability::Chat => capabilities.push(ModelCapability::Chat),
                Capability::ToolUse => capabilities.push(ModelCapability::ToolUse),
                Capability::Vision => capabilities.push(ModelCapability::ImageAnalysis),
                Capability::ImageGeneration => capabilities.push(ModelCapability::ImageGeneration),
                Capability::Embedding => capabilities.push(ModelCapability::Embeddings),
                // Capabilities that exist in the registry but have no legacy
                // equivalent don't project. They're still available via
                // Model::has(Capability::X) — adapters that need them
                // should read the registry directly rather than parse the
                // projected ai::ModelInfo.
                Capability::Streaming
                | Capability::FineTuning
                | Capability::LoraAdapter
                | Capability::Reranking
                | Capability::AudioInput
                | Capability::AudioOutput => {}
            }
        }

        ModelInfo {
            id: m.id.clone(),
            name,
            provider: m.provider.clone(),
            capabilities,
            context_window: m.context_window,
            max_output_tokens: m.max_output_tokens,
            cost_per_1k_tokens: CostPer1kTokens {
                input: m.cost_input_per_1k as f64,
                output: m.cost_output_per_1k as f64,
            },
            tokens_per_second: m.tokens_per_second,
            supports_streaming: m.has(Capability::Streaming),
            supports_tools: m.has(Capability::ToolUse),
        }
    }
}

/// Collect all models for a given provider from the global registry as
/// a Vec<ai::ModelInfo>. Convenience for adapters implementing
/// `get_available_models()` — typical use:
///
/// ```ignore
/// async fn get_available_models(&self) -> Vec<ModelInfo> {
///     models_for_provider_via_registry("anthropic")
/// }
/// ```
///
/// Returns an empty vec if the provider is unknown or has no models —
/// adapters that want to panic on missing-provider (wiring error, not
/// runtime) should check `Registry::provider()` explicitly.
pub fn models_for_provider_via_registry(provider_id: &str) -> Vec<ModelInfo> {
    let reg = crate::model_registry::global();
    reg.models_for_provider(provider_id)
        .map(ModelInfo::from)
        .collect()
}

/// Default model id for a provider, per the registry. `None` if the
/// provider is unknown OR hasn't declared a default (e.g. dynamic
/// catalogs like docker-model-runner). Adapters whose trait contract
/// requires a concrete default should unwrap with a meaningful panic —
/// a missing default for a provider that needs one is a TOML bug, not
/// a runtime failure mode.
pub fn default_model_for_provider(provider_id: &str) -> Option<String> {
    let reg = crate::model_registry::global();
    reg.provider(provider_id).and_then(|p| p.default_model.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projects_sonnet_with_streaming_and_tools() {
        let reg = crate::model_registry::init_global().expect("seed loads");
        let sonnet = reg
            .model("claude-sonnet-4-5-20250929")
            .expect("sonnet in registry");
        let projected: ModelInfo = sonnet.into();
        assert_eq!(projected.id, "claude-sonnet-4-5-20250929");
        assert_eq!(projected.name, "Claude Sonnet 4.5");
        assert_eq!(projected.provider, "anthropic");
        assert!(projected.supports_streaming);
        assert!(projected.supports_tools);
        assert!(projected.capabilities.contains(&ModelCapability::ImageAnalysis));
        assert!(projected.capabilities.contains(&ModelCapability::Chat));
        assert!(projected.capabilities.contains(&ModelCapability::ToolUse));
        assert_eq!(projected.context_window, 200_000);
        assert_eq!(projected.max_output_tokens, 8_192);
        assert!((projected.cost_per_1k_tokens.input - 0.003).abs() < 1e-9);
    }

    #[test]
    fn collects_three_anthropic_models() {
        let _ = crate::model_registry::init_global().expect("seed loads");
        let models = models_for_provider_via_registry("anthropic");
        assert_eq!(models.len(), 3, "anthropic has 3 models in seeded config");
        let ids: Vec<&str> = models.iter().map(|m| m.id.as_str()).collect();
        assert!(ids.contains(&"claude-sonnet-4-5-20250929"));
        assert!(ids.contains(&"claude-opus-4-20250514"));
        assert!(ids.contains(&"claude-3-5-haiku-20250107"));
    }

    #[test]
    fn default_model_for_anthropic_is_sonnet() {
        let _ = crate::model_registry::init_global().expect("seed loads");
        assert_eq!(
            default_model_for_provider("anthropic").as_deref(),
            Some("claude-sonnet-4-5-20250929"),
        );
    }

    #[test]
    fn unknown_provider_returns_empty_and_none() {
        let _ = crate::model_registry::init_global().expect("seed loads");
        assert!(models_for_provider_via_registry("no-such-provider").is_empty());
        assert!(default_model_for_provider("no-such-provider").is_none());
    }
}
