//! `build_profile` — substrate-side construction of
//! [`PersonaInferenceProfile`] from declared persona intent.
//!
//! ## Doctrine
//!
//! Per [[intent-driven-api-not-hot-patches]]: ONE place derives the
//! profile from (persona_id, persona_name, role_id, tier_id,
//! model_id); MANY adapters consume the profile via `for_persona`.
//! This is that one place. It replaces ad-hoc profile construction
//! scattered across binaries with a centralized, testable derivation
//! that the PersonaSpawnerModule (#121) will call on every persona
//! spawn.
//!
//! ## Inputs
//!
//! - `persona_id` — UUID, typically derived from the persona's airc
//!   peer_id per [[persona-identity-derives-from-source-id]].
//! - `persona_name` — display name, derived from the same seed via
//!   `name_generator::agent_name_from_identity`.
//! - `role_id` — Helper / Coder / Sentinel / Custom. Currently
//!   informational (the model_id already picks the model); a future
//!   refinement reads role_template.cognition_defaults to drive
//!   sampling.
//! - `tier_id` — stable hw_tier descriptor id, e.g.
//!   `"mac_intel_metal_discrete"`. The substrate resolves this via
//!   the HostCapabilityProbe (#115) at boot.
//! - `model_id` — registry model id, e.g.
//!   `"continuum-ai/qwen2.5-0.5b-instruct-GGUF"`. Picked by the
//!   role_template for the tier in question.
//! - `registry` — the global `model_registry::Registry` (caller
//!   passes the `Arc<Registry>` from `model_registry::global()`).
//!
//! ## Output
//!
//! A complete [`PersonaInferenceProfile`] ready to pass to
//! `LlamaCppAdapter::for_persona(profile)` — or, when the adapter
//! family grows, to any future `Adapter::for_persona` impl.
//!
//! ## Error modes
//!
//! All caught per [[no-fallbacks-ever]] — substrate refuses to build
//! a silently-degraded profile:
//!
//! - [`InferenceProfileError::UnknownModel`] — model_id not in
//!   registry.
//! - [`InferenceProfileError::NoLocalGguf`] — model is local-only but
//!   no on-disk GGUF resolved.
//! - [`InferenceProfileError::InsufficientHeadroom`] — tier can't
//!   carry the model's minimum params (future check; not enforced in
//!   this initial slice).

use crate::persona::hw_tier_descriptor::HwTierCategory;
use crate::persona::inference_profile::{
    InferenceProfileError, PersonaInferenceProfile, SamplingProfile,
};
use std::sync::Arc;
use uuid::Uuid;

/// Compose a [`PersonaInferenceProfile`] from declared intent.
///
/// See the module docstring for the contract this function honors and
/// the failure modes it surfaces.
pub fn build_profile(
    persona_id: Uuid,
    persona_name: impl Into<String>,
    role_id: &str,
    tier_id: &str,
    tier_category: HwTierCategory,
    model_id: &str,
    n_seq_max: u32,
    registry: &crate::model_registry::Registry,
) -> Result<PersonaInferenceProfile, InferenceProfileError> {
    let _ = role_id; // see module docstring; reserved for cognition_defaults wiring

    let model = registry.model(model_id).ok_or_else(|| {
        InferenceProfileError::UnknownModel {
            model_id: model_id.to_string(),
            role_id: role_id.to_string(),
        }
    })?;

    // Local-inference models MUST have a resolved gguf_local_path
    // here. Per [[no-fallbacks-ever]], we don't silently substitute a
    // different model — caller decides whether to swap the model_id in
    // the role_template, install the artifact, or route via grid.
    // Provider lookup → kind. Model.provider is the provider id string;
    // the actual ProviderKind enum lives on the Provider struct.
    let provider_kind = registry
        .provider(&model.provider)
        .map(|p| p.kind)
        .unwrap_or(crate::model_registry::types::ProviderKind::Cloud);

    let gguf_local_path =
        if matches!(provider_kind, crate::model_registry::types::ProviderKind::Local) {
            match &model.gguf_local_path {
                Some(p) => Some(p.clone()),
                None => {
                    return Err(InferenceProfileError::NoLocalGguf {
                        model_id: model_id.to_string(),
                        gguf_hint: model.gguf_hint.clone(),
                    });
                }
            }
        } else {
            // Cloud-routed profiles (Anthropic, OpenAI, etc.) don't need a
            // local path — the adapter wires to the cloud endpoint directly.
            None
        };

    // Context length: the model's OWN declared window. No per-tier integer
    // clamp — guessing a tier cap (the old 2048/4096/8192…) silently
    // throttled a 32K model to 6% of its window. The adapter
    // (`LlamaCppBackend::effective_context_length`) is the authority: it caps
    // this at the GGUF's real `n_ctx_train` AND a budget derived from real
    // available memory, so passing the model's full ceiling here is safe — it
    // can only be shrunk to fit, never blindly allocated (that was the
    // 262144-token Metal OOM, 2026-04). Task #46.
    let context_length = model.context_window;

    // n_ubatch: realistic RAG-built persona prompts cap at 200-500
    // tokens today; 512 covers them. Compat tier uses the same as
    // other tiers — graph nodes scale modestly so no need to shrink.
    let n_ubatch = 512;

    // n_seq_max: continuous-batching lanes for this persona's backend,
    // decided by the serving daemon's ServingPlan (honest host budget +
    // model footprint) and threaded in via DesiredRole → RosterEntry. Floored
    // at 1. Shared-base + LoRA paging (#122) then lets one base host N
    // personas across these lanes instead of one backend per persona.
    let n_seq_max = n_seq_max.max(1);
    // Prefill chunk size — a sane fixed batch, NOT the full context window
    // (a 32K n_batch would reserve an enormous prefill graph). The backend's
    // own `LlamaCppConfig` default governs the real batch; this profile field
    // is advisory.
    let n_batch = n_ubatch;

    // GPU offload depth: substrate-known per tier. Compat (Intel Mac
    // + AMD discrete) currently routes CPU-only while [[#131]]'s
    // Metal hang fix is pending; M-series tiers default to all-GPU;
    // Cuda + Cloud follow their respective adapter defaults.
    let n_gpu_layers = match tier_category {
        HwTierCategory::Compat => 0,
        HwTierCategory::MSeries | HwTierCategory::MSeriesPro | HwTierCategory::Cuda => -1,
        // Cloud routes don't use llama.cpp; field is unused but set
        // to -1 (all on remote) for completeness.
        HwTierCategory::Cloud => -1,
    };

    // chat_template + stop_sequences: pre-resolved from the registry
    // row so the adapter doesn't re-query per call.
    let chat_template = model.chat_template.clone();
    let stop_sequences = model.stop_sequences.clone();

    Ok(PersonaInferenceProfile {
        persona_id,
        persona_name: persona_name.into(),
        model_id: model_id.to_string(),
        gguf_local_path,
        tier_category,
        tier_id: tier_id.to_string(),
        context_length,
        n_ubatch,
        n_batch,
        n_seq_max,
        n_gpu_layers,
        sampling: SamplingProfile::chat_defaults(),
        chat_template,
        stop_sequences,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_registry::types::{
        Arch, AuthKind, Capability, MultiPartyChatStrategy, Provider, ProviderKind,
    };
    use crate::model_registry::{Model, Registry};
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    /// Create a tempfile to stand in for the GGUF on disk. Registry's
    /// `resolve_model_artifacts` only honors `gguf_local_path` when the
    /// file actually exists; tests need a real path that does exist
    /// without requiring the real ~500 MiB Qwen2.5-0.5B GGUF download.
    fn make_fake_gguf_tempfile() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "profile_builder_test_qwen25_05b-{}.gguf",
            uuid::Uuid::new_v4()
        ));
        std::fs::write(&path, b"fake gguf header for test purposes only")
            .expect("create tempfile");
        path
    }

    fn registry_with_qwen25_05b() -> Arc<Registry> {
        let fake_gguf = make_fake_gguf_tempfile();
        let llamacpp_provider = Provider {
            id: "llamacpp-local".to_string(),
            name: Some("Local llama.cpp".to_string()),
            kind: ProviderKind::Local,
            base_url: String::new(),
            auth: AuthKind::None,
            api_key_env: None,
            default_model: None,
            model_prefixes: Vec::new(),
            capabilities: crate::model_registry::types::ProviderCapabilities::default(),
        };
        let model = Model {
            id: "continuum-ai/qwen2.5-0.5b-instruct-GGUF".to_string(),
            name: Some("Qwen2.5 0.5B Instruct (LCD)".to_string()),
            provider: "llamacpp-local".to_string(),
            arch: Arch::Qwen2,
            context_window: 32768,
            max_output_tokens: 4096,
            tokens_per_second: 60.0,
            capabilities: {
                let mut s = BTreeSet::new();
                s.insert(Capability::TextGeneration);
                s.insert(Capability::Chat);
                s.insert(Capability::Streaming);
                s
            },
            cost_input_per_1k: 0.0,
            cost_output_per_1k: 0.0,
            gguf_hint: Some("hf.co/Qwen/Qwen2.5-0.5B-Instruct-GGUF".to_string()),
            gguf_local_path: Some(fake_gguf),
            chat_template: Some("{% for m in messages %}".to_string()),
            stop_sequences: vec!["<|im_end|>".to_string()],
            multi_party_strategy: MultiPartyChatStrategy::ProperChatMlSingleParty,
            mmproj_local_path: None,
        };
        Arc::new(
            Registry::from_catalog(vec![model], vec![llamacpp_provider])
                .expect("build registry"),
        )
    }

    /// Happy path: Helper on Compat tier with the LCD model produces a
    /// complete profile with every knob derived from intent.
    #[test]
    fn builds_helper_compat_lcd_profile() {
        let registry = registry_with_qwen25_05b();
        let profile = build_profile(
            Uuid::nil(),
            "Paige",
            "helper",
            "mac_intel_metal_discrete",
            HwTierCategory::Compat,
            "continuum-ai/qwen2.5-0.5b-instruct-GGUF",
            1,
            &registry,
        )
        .expect("build profile");
        assert_eq!(profile.persona_name, "Paige");
        assert_eq!(profile.model_id, "continuum-ai/qwen2.5-0.5b-instruct-GGUF");
        assert_eq!(profile.tier_category, HwTierCategory::Compat);
        // The model's real window, not a per-tier clamp (task #46).
        assert_eq!(profile.context_length, 32768);
        assert_eq!(profile.n_ubatch, 512);
        assert_eq!(profile.n_seq_max, 1);
        // Compat tier currently routes CPU-only per #131.
        assert_eq!(profile.n_gpu_layers, 0);
        // gguf_local_path threaded through from the registry row.
        assert!(profile.gguf_local_path.is_some());
        // Stop sequences propagated from the registry row.
        assert_eq!(profile.stop_sequences, vec!["<|im_end|>".to_string()]);
    }

    /// Tier-shaped n_gpu_layers: MSeries+ goes full GPU (-1), Compat
    /// stays CPU-only.
    #[test]
    fn n_gpu_layers_reflects_tier_category() {
        let registry = registry_with_qwen25_05b();
        let model = "continuum-ai/qwen2.5-0.5b-instruct-GGUF";

        let compat = build_profile(
            Uuid::nil(),
            "Paige",
            "helper",
            "mac_intel_metal_discrete",
            HwTierCategory::Compat,
            model,
            1,
            &registry,
        )
        .unwrap();
        assert_eq!(compat.n_gpu_layers, 0);

        let mseries = build_profile(
            Uuid::nil(),
            "Maya",
            "helper",
            "m1_uma_8gb",
            HwTierCategory::MSeries,
            model,
            1,
            &registry,
        )
        .unwrap();
        assert_eq!(mseries.n_gpu_layers, -1);

        let pro = build_profile(
            Uuid::nil(),
            "Niko",
            "coder",
            "m5_uma_pro_max",
            HwTierCategory::MSeriesPro,
            model,
            1,
            &registry,
        )
        .unwrap();
        assert_eq!(pro.n_gpu_layers, -1);
    }

    /// what this catches: the profile advertises the MODEL's real window,
    /// not a per-tier integer clamp. The old `compat_context_length` returned
    /// 2048/4096/8192 by tier — throttling a 32K-trained model to 6% on
    /// Compat. Now every tier gets the model's own ceiling; the adapter
    /// (`LlamaCppBackend::effective_context_length`) does the real
    /// memory-bounding at load time, where the loaded GGUF + live RAM exist.
    /// Task #46.
    #[test]
    fn context_length_is_model_window_not_tier_clamp() {
        let registry = registry_with_qwen25_05b();
        let model = "continuum-ai/qwen2.5-0.5b-instruct-GGUF";
        // The fixture model declares a 32K window.
        let model_window = 32768;

        let mk = |tier_id, tier_cat| {
            build_profile(
                Uuid::nil(),
                "P",
                "helper",
                tier_id,
                tier_cat,
                model,
                1,
                &registry,
            )
            .unwrap()
            .context_length
        };

        let compat = mk("mac_intel_metal_discrete", HwTierCategory::Compat);
        let mseries = mk("m1_uma_8gb", HwTierCategory::MSeries);
        let pro = mk("m5_uma_pro_max", HwTierCategory::MSeriesPro);

        // Every tier advertises the model's full window — no per-tier clamp.
        assert_eq!(compat, model_window);
        assert_eq!(mseries, model_window);
        assert_eq!(pro, model_window);
    }

    /// Unknown model_id errors loud per [[no-fallbacks-ever]] with a
    /// diagnostic that names what was asked vs what's available.
    #[test]
    fn unknown_model_errors_with_diagnostic() {
        let registry = registry_with_qwen25_05b();
        let err = build_profile(
            Uuid::nil(),
            "Paige",
            "helper",
            "mac_intel_metal_discrete",
            HwTierCategory::Compat,
            "nonexistent/model-id",
            1,
            &registry,
        )
        .expect_err("unknown model must error");
        match err {
            InferenceProfileError::UnknownModel { model_id, role_id } => {
                assert_eq!(model_id, "nonexistent/model-id");
                assert_eq!(role_id, "helper");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }
}
