//! Spawner planning — derive the full set of [`PersonaInferenceProfile`]s
//! the substrate intends to spawn for a given hardware tier.
//!
//! ## Doctrine
//!
//! Per [[intent-driven-api-not-hot-patches]] + #121 PersonaSpawnerModule:
//! the substrate's "what personas should be alive on this machine?"
//! decision is a function of (hardware tier × declared role roster ×
//! model registry). This module owns that derivation; the
//! ServiceModule wrapper that turns the plan into running peers on
//! airc lands in slice 7.
//!
//! ## Sequencing within #133
//!
//! - Slice 5 (`profile_builder.rs`): `build_profile` — ONE persona
//!   from (persona_id, persona_name, role_id, tier_id, tier_category,
//!   model_id, registry).
//! - Slice 6 (this file): `derive_spawn_plan` — MANY personas from a
//!   roster declaration. Each entry composes via `build_profile`.
//! - Slice 7 (planned): `PersonaSpawnerModule` — wraps the plan,
//!   handles airc attach + room join + persona instance lifecycle.
//!
//! ## Why a roster declaration (not auto-derivation)
//!
//! The slice 6 API takes an explicit roster instead of calling
//! `role_template::defaults_for_tier`. Two reasons:
//!
//! 1. **Identity in-substrate**: each persona needs a peer_id +
//!    persona_name. Per [[persona-identity-derives-from-source-id]]
//!    those come from the airc identity layer, not from role
//!    templates. The slice 7 ServiceModule allocates each persona's
//!    airc identity FIRST, then hands the (peer_id, name) pair into
//!    the planner.
//! 2. **Model selection**: today's `defaults_for_tier` returns the
//!    same fixed [Helper, Coder] vec for every tier. Future slices
//!    refine this via #123 ORM-stored role_templates. The planner
//!    stays clean by consuming a resolved roster instead of doing the
//!    selection itself.
//!
//! This keeps slice 6 testable without an airc fixture and without
//! touching the role_template hardcoded-Rust path.

use crate::persona::hw_tier_descriptor::HwTierCategory;
use crate::persona::inference_profile::{InferenceProfileError, PersonaInferenceProfile};
use crate::persona::profile_builder::{build_profile, ServingParams};
use crate::persona::role_template::RoleId;
use std::sync::Arc;
use uuid::Uuid;

/// One row of the roster: a substrate-resolved persona slot ready for
/// profile materialization. The slice 7 ServiceModule allocates each
/// slot's airc identity then hands these in.
#[derive(Debug, Clone)]
pub struct RosterEntry {
    /// Role identifier (Helper / Coder / Sentinel / Custom).
    pub role: RoleId,
    /// Persona's UUID — derived from the persona's airc peer_id per
    /// [[persona-identity-derives-from-source-id]]. Substrate gets one
    /// peer per persona at airc-attach time; this is the result of
    /// `peer_id.as_uuid()`.
    pub persona_id: Uuid,
    /// Display name — typically derived deterministically from the
    /// peer_id via `name_generator::agent_name_from_identity`. Used in
    /// chat surface labels and inference traces.
    pub persona_name: String,
    /// Model registry id the substrate picked for this role at this
    /// tier. Today's roster builders read `role_template`'s
    /// `model_per_tier` table; future refinements via #123 ORM data
    /// substitute this without changing the planner contract.
    pub model_id: String,
    /// Serving-plan-derived backend knobs (lanes + host-fit served context
    /// window) for this persona, from the serving daemon's ServingPlan (honest
    /// host budget + footprint). Grouped per [[pass-the-model-struct-no-param-hell]]
    /// and threaded as one unit into `build_profile`.
    pub serving: ServingParams,
}

/// Materialize a spawn plan from a roster + tier descriptor.
///
/// Returns one `Result<PersonaInferenceProfile>` per roster entry.
/// Per-row failures are kept separate so that a single bad row (e.g.,
/// a model row not yet in the registry) doesn't block the others —
/// the slice 7 ServiceModule decides whether to refuse boot or skip
/// the bad personas with a diagnostic. Per [[no-fallbacks-ever]] the
/// errors are structured and named; the substrate never substitutes a
/// "default" persona for a failed derivation.
pub fn derive_spawn_plan(
    roster: &[RosterEntry],
    tier_id: &str,
    tier_category: HwTierCategory,
    registry: &crate::model_registry::Registry,
) -> Vec<Result<PersonaInferenceProfile, InferenceProfileError>> {
    roster
        .iter()
        .map(|entry| {
            build_profile(
                entry.persona_id,
                entry.persona_name.clone(),
                entry.role.as_str(),
                tier_id,
                tier_category,
                &entry.model_id,
                entry.serving,
                registry,
            )
        })
        .collect()
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

    fn make_fake_gguf_tempfile(slug: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "spawner_test_{}-{}.gguf",
            slug,
            uuid::Uuid::new_v4()
        ));
        // Must be a STRUCTURALLY VALID empty GGUF, not arbitrary bytes:
        // `Registry::from_catalog` hydrates every resolved GGUF's header at
        // load (#74), and `b"fake gguf"` fails loud with `unknown magic` the
        // moment hydration reads it. `write_empty_gguf` is the one canonical
        // "a model is present here" stand-in — parseable, zero metadata, so
        // the row's hand-authored fields stand unchanged.
        crate::model_registry::artifacts::write_empty_gguf(&path);
        path
    }

    fn registry_with_lcd() -> Arc<Registry> {
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
        let qwen25_05b = Model {
            weights_bytes: None,
            mmproj_bytes: None,
            id: "continuum-ai/qwen2.5-0.5b-instruct-GGUF".to_string(),
            name: Some("Qwen2.5 0.5B Instruct".to_string()),
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
            gguf_hint: None,
            hf_source: None,
            gguf_local_path: Some(make_fake_gguf_tempfile("lcd")),
            chat_template: Some("{% for m in messages %}".to_string()),
            stop_sequences: vec!["<|im_end|>".to_string()],
            multi_party_strategy: MultiPartyChatStrategy::ProperChatMlSingleParty,
            mmproj_local_path: None,
            parameter_count: 0,
            sampling: crate::model_registry::types::ModelSampling::default(),
            persona_serving_eligible: true,
            serving: Default::default(), // test/fixture literal: substrate defaults (text-only main lane, unverified kv-shift)
        };
        Arc::new(
            Registry::from_catalog(vec![qwen25_05b], vec![llamacpp_provider])
                .expect("build registry"),
        )
    }

    /// Stand-in served window the ServingPlan would compute for the host.
    /// The planner is unit-tested in `serving_plan.rs`; here we only assert
    /// the value threads through unchanged for local models.
    // context-budget-exempt: a TEST fixture stating the window it measures against — the pattern this guard asks for
    const TEST_SERVE_WINDOW: u32 = 8192;

    fn helper_paige() -> RosterEntry {
        RosterEntry {
            role: RoleId::Helper,
            persona_id: Uuid::nil(),
            persona_name: "Paige".to_string(),
            model_id: "continuum-ai/qwen2.5-0.5b-instruct-GGUF".to_string(),
            serving: ServingParams {
                lanes: 1,
                served_context_window: TEST_SERVE_WINDOW,
            },
        }
    }

    fn coder_pax() -> RosterEntry {
        RosterEntry {
            role: RoleId::Coder,
            persona_id: Uuid::nil(),
            persona_name: "Pax".to_string(),
            model_id: "continuum-ai/qwen2.5-0.5b-instruct-GGUF".to_string(),
            serving: ServingParams {
                lanes: 1,
                served_context_window: TEST_SERVE_WINDOW,
            },
        }
    }

    /// Compat tier with Helper + Coder roster: substrate plans both
    /// personas with the LCD model, Compat-shaped knobs. This is the
    /// canonical Intel-Mac multi-persona startup state #133 targets.
    #[test]
    fn plans_helper_and_coder_for_compat_tier() {
        let registry = registry_with_lcd();
        let roster = vec![helper_paige(), coder_pax()];
        let plan = derive_spawn_plan(
            &roster,
            "mac_intel_metal_discrete",
            HwTierCategory::Compat,
            &registry,
        );
        assert_eq!(plan.len(), 2);
        let helper = plan[0].as_ref().expect("Helper plan").clone();
        assert_eq!(helper.persona_name, "Paige");
        assert_eq!(helper.tier_category, HwTierCategory::Compat);
        // Local-served → exactly the planner's served window (task #50),
        // threaded through unchanged (not a per-tier clamp, task #46).
        assert_eq!(helper.context_length, TEST_SERVE_WINDOW);
        assert_eq!(helper.n_gpu_layers, 0);
        let coder = plan[1].as_ref().expect("Coder plan").clone();
        assert_eq!(coder.persona_name, "Pax");
        assert_eq!(coder.tier_category, HwTierCategory::Compat);
        // Both share the LCD model on Compat — shared base for #122
        // LoRA paging when that ships.
        assert_eq!(helper.model_id, coder.model_id);
    }

    /// A bad model_id in the roster is reported per-row, not as a
    /// catastrophic failure. Other personas still plan cleanly. This
    /// is what lets the substrate boot multi-persona even when ONE
    /// role's model isn't yet registered.
    #[test]
    fn per_row_errors_dont_block_other_personas() {
        let registry = registry_with_lcd();
        let mut bad_coder = coder_pax();
        bad_coder.model_id = "nonexistent/sentinel-model".to_string();
        let roster = vec![helper_paige(), bad_coder];
        let plan = derive_spawn_plan(
            &roster,
            "mac_intel_metal_discrete",
            HwTierCategory::Compat,
            &registry,
        );
        assert_eq!(plan.len(), 2);
        assert!(plan[0].is_ok(), "Helper still resolves cleanly");
        match plan[1] {
            Err(InferenceProfileError::UnknownModel {
                ref model_id,
                ref role_id,
            }) => {
                assert_eq!(model_id, "nonexistent/sentinel-model");
                assert_eq!(role_id, "coder");
            }
            ref other => panic!("expected UnknownModel, got {other:?}"),
        }
    }

    /// Empty roster → empty plan. Slice 7's ServiceModule treats this
    /// as "no personas to spawn"; whether that's a substrate boot
    /// error or a no-op is a ServiceModule-level policy decision.
    #[test]
    fn empty_roster_yields_empty_plan() {
        let registry = registry_with_lcd();
        let plan = derive_spawn_plan(
            &[],
            "mac_intel_metal_discrete",
            HwTierCategory::Compat,
            &registry,
        );
        assert!(plan.is_empty());
    }

    /// Same roster, different tier → different tier-shaped knobs in
    /// the resulting profiles. Validates that the planner threads
    /// `tier_category` through to every persona without leaking
    /// state across rows.
    #[test]
    fn tier_category_threads_into_every_profile() {
        let registry = registry_with_lcd();
        let roster = vec![helper_paige(), coder_pax()];

        let compat_plan = derive_spawn_plan(
            &roster,
            "mac_intel_metal_discrete",
            HwTierCategory::Compat,
            &registry,
        );
        for p in &compat_plan {
            let prof = p.as_ref().unwrap();
            assert_eq!(prof.tier_category, HwTierCategory::Compat);
            assert_eq!(prof.n_gpu_layers, 0);
            // Local-served → the planner's served window, threaded through (task #50).
            assert_eq!(prof.context_length, TEST_SERVE_WINDOW);
        }

        let mseries_plan =
            derive_spawn_plan(&roster, "m1_uma_8gb", HwTierCategory::MSeries, &registry);
        for p in &mseries_plan {
            let prof = p.as_ref().unwrap();
            assert_eq!(prof.tier_category, HwTierCategory::MSeries);
            assert_eq!(prof.n_gpu_layers, -1);
            // Local-served → the served window the roster carried (task #50).
            assert_eq!(prof.context_length, TEST_SERVE_WINDOW);
        }
    }
}
