//! Projects `model_registry::Model` (the TOML-driven source of truth) into
//! `ai::ModelInfo` (the wire type adapters return and ts-rs exports to TS).
//!
//! After the #65 capability collapse there is no longer a vocabulary to
//! translate: both shapes carry `model_registry::Capability` directly, so
//! the projection is a field-for-field clone (plus the two derived
//! convenience bools `supports_streaming`/`supports_tools` the wire type
//! exposes for callers that don't want to scan the set). `ModelInfo` is now
//! effectively a thin TS-projection of `Model`.

use super::types::{CostPer1kTokens, ModelInfo};
use crate::model_registry::{Capability, Model};

impl From<&Model> for ModelInfo {
    fn from(m: &Model) -> Self {
        // Display name — fall back to id if TOML didn't supply one. The
        // fallback is intentionally ugly (full id, often dotted hf.co paths)
        // so the empty-name case surfaces at UI time and the TOML gets fixed.
        let name = m.name.clone().unwrap_or_else(|| m.id.clone());

        ModelInfo {
            id: m.id.clone(),
            name,
            provider: m.provider.clone(),
            // ONE vocabulary — clone the registry's capability set verbatim.
            capabilities: m.capabilities.iter().copied().collect(),
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
    reg.provider(provider_id)
        .and_then(|p| p.default_model.clone())
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
        assert!(projected.capabilities.contains(&Capability::Vision));
        assert!(projected.capabilities.contains(&Capability::Chat));
        assert!(projected.capabilities.contains(&Capability::ToolUse));
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
