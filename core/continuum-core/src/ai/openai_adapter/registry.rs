//! Registry-driven construction and finish-reason mapping.

use super::*;

impl OpenAICompatibleAdapter {
    /// Build an adapter for `provider_id` by reading everything from the
    /// model_registry. Replaces eight hand-rolled factories whose combined
    /// bulk was ~280 LOC of `ModelInfo { ... }` literals that drifted
    /// whenever a new model shipped. Now the catalog is the only place a
    /// new model's context_window / capabilities / pricing lives.
    ///
    /// Panics if the provider isn't in the registry — that's a boot-time
    /// config bug, not a runtime condition (per the no-fallback rule).
    ///
    /// Capability flags (`supports_tools`, `supports_vision`) are derived
    /// from whether ANY model under this provider advertises the relevant
    /// Capability. A new Vision-capable model showing up in the catalog flips
    /// the adapter's vision flag automatically on next boot — no code
    /// change.
    pub fn from_registry(provider_id: &str) -> Self {
        let reg = crate::model_registry::global();
        let provider = reg.provider(provider_id).unwrap_or_else(|| {
            panic!(
                "provider `{}` not in the Rust catalog (catalog.rs) — can't build \
                 OpenAICompatibleAdapter",
                provider_id
            )
        });

        let models = models_for_provider_via_registry(provider_id);

        // Project this provider's real capabilities into the ONE vocabulary
        // (#65). Every OpenAI-compatible adapter does text + chat + streaming;
        // tool-use + vision come from scanning the provider's models; embeddings
        // + image-gen come from the provider's declared `ProviderCapabilities`.
        // A new vision-capable model in the catalog flips the Vision flag
        // automatically on next boot — no code change, no `id == "..."` branch.
        let mut capabilities = BTreeSet::from([
            Capability::TextGeneration,
            Capability::Chat,
            Capability::Streaming,
        ]);
        if reg
            .models_for_provider(provider_id)
            .any(|m| m.has(Capability::ToolUse))
        {
            capabilities.insert(Capability::ToolUse);
        }
        if reg
            .models_for_provider(provider_id)
            .any(|m| m.has(Capability::Vision))
        {
            capabilities.insert(Capability::Vision);
        }
        // Embedding + ImageGeneration are derived the SAME way as ToolUse and
        // Vision above (#68): scan the provider's model rows. A provider
        // "supports embeddings" iff it serves a model that declares it — the
        // fact lives on the model, never on a provider-level bool.
        if reg
            .models_for_provider(provider_id)
            .any(|m| m.has(Capability::Embedding))
        {
            capabilities.insert(Capability::Embedding);
        }
        if reg
            .models_for_provider(provider_id)
            .any(|m| m.has(Capability::ImageGeneration))
        {
            capabilities.insert(Capability::ImageGeneration);
        }
        let requires_auth = !matches!(provider.auth, AuthKind::None);

        // `default_model` is non-optional in the adapter trait
        // (`fn default_model(&self) -> &str`) — callers always get a
        // concrete id back. Providers with genuinely dynamic catalogs
        // (DMR) still declare a default id the user is most likely to
        // want; operator overrides flow through explicit request.model.
        // Panic if missing: the registry row is incomplete, not a runtime
        // condition.
        let default_model = provider.default_model.clone().unwrap_or_else(|| {
            panic!(
                "provider `{}` has no `default_model` in the Rust catalog (catalog.rs) — \
                 every OpenAI-compatible adapter needs one because the trait \
                 returns &str, not Option<&str>",
                provider_id
            )
        });

        Self::new(OpenAICompatibleConfig {
            provider_id: provider.id.clone(),
            name: provider.display_name().to_string(),
            base_url: provider.base_url.clone(),
            api_key_env: provider.api_key_env.clone(),
            default_model,
            capabilities,
            models,
            model_prefixes: provider.model_prefixes.clone(),
            requires_auth,
            // Tool-call shape + thinking + embeddings + single-slot residency +
            // dynamic-catalog + llama.cpp sampling extensions ALL come from the
            // registry's declared `Provider.capabilities` (#55) — the adapter
            // CONSUMES them, it does not branch on `provider.id`. A local GGUF
            // gateway declares its real flags; cloud providers inherit the
            // NativeFunctionCalling/keep-thinking defaults. One source of truth
            // (model_registry/catalog.rs), no id stand-ins here, and the ONE
            // `ToolProtocol` (#69) — stored verbatim, no per-adapter translation.
            tool_protocol: provider.capabilities.tool_protocol,
            // A gateway that declares `suppress_thinking` (its forged reasoner
            // rambles/loops yet answers correctly without CoT) defaults to
            // SUPPRESS. Operator override: `UNSLOTH_THINKING=on` forces thinking
            // back on per-run (the reasoning-strip still protects the room).
            thinking: {
                let keep_thinking = std::env::var("UNSLOTH_THINKING")
                    .map(|v| v.trim().eq_ignore_ascii_case("on"))
                    .unwrap_or(false);
                if provider.capabilities.suppress_thinking && !keep_thinking {
                    ThinkingMode::Suppress
                } else {
                    ThinkingMode::Default
                }
            },
            single_resident_model: provider.capabilities.single_resident_model,
            dynamic_model_catalog: provider.capabilities.dynamic_model_catalog,
            llamacpp_sampling_extensions: provider.capabilities.llamacpp_sampling_extensions,
        })
    }

    // `format_messages` lives in `crate::inference::request_body` (S3b decompose).

    /// Map OpenAI finish reason to our enum
    pub(super) fn map_finish_reason(&self, reason: &str) -> FinishReason {
        match reason {
            "stop" => FinishReason::Stop,
            "length" => FinishReason::Length,
            "tool_calls" => FinishReason::ToolUse,
            _ => FinishReason::Error,
        }
    }
}
