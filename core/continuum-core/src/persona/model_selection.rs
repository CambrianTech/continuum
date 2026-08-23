//! Model Selection Engine
//!
//! Selects the concrete adapter-backed model for a persona turn. This module is
//! intentionally fail-hard: if no trained adapter is available for the persona,
//! the caller receives a typed error instead of silently using a base model.
//!
//! Priority chain:
//! 0. Signature distance (the need's embedding vs each gene's MINTED signature —
//!    [[gene-routing-is-distance-not-keywords]]; gated by a significance floor
//!    because the recall space is anisotropic)
//! 1. Trait-specific adapter (domain -> trait mapping, e.g. "code" -> reasoning_style)
//! 2. Current active adapter (most recently used)
//! 3. Any available trained adapter
//!
//! Rung 0 exists because `domain_to_trait` is a hardcoded keyword match — exactly
//! the routing the genome doctrine replaces with proximity: an FP gene should lift
//! a Scheme task no keyword table anticipated. The rung fires only when the caller
//! pre-computed the need's embedding (this fn stays PURE + sync — the async embed
//! happens upstream, mirroring `memory/recall`'s `query_embedding` pattern) AND a
//! signed adapter clears the floor; otherwise every existing tier behaves exactly
//! as before (pinned by the pre-rung tests, which pass unchanged).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Instant;
use ts_rs::TS;

// =============================================================================
// TYPES (ts-rs generated)
// =============================================================================

/// Request to select the best model for a persona given optional task context.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/ModelSelectionRequest.ts"
)]
pub struct ModelSelectionRequest {
    #[ts(type = "string")]
    pub persona_id: uuid::Uuid,
    /// Optional task domain for trait-specific adapter lookup.
    /// Values: "code", "debug", "analysis", "creative", "art", "writing",
    ///         "support", "help", "social", "facts", "knowledge", "expertise"
    #[ts(optional)]
    pub task_domain: Option<String>,
    /// Optional free-text NEED ("refactor rust async code") — embedded by the
    /// command layer and matched by DISTANCE against each gene's minted
    /// signature (rung 0). Falls back to `task_domain`'s text when absent; when
    /// neither is present rung 0 is skipped and the keyword tiers answer.
    #[serde(default)]
    #[ts(optional)]
    pub need: Option<String>,
}

/// Result of model selection — which model to use and why.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/ModelSelectionResult.ts"
)]
pub struct ModelSelectionResult {
    /// The selected trained adapter model.
    pub model: String,
    /// Which tier selected it: "trait_adapter", "current_adapter", "any_adapter"
    pub source: String,
    /// Name of the adapter used (if any).
    #[ts(optional)]
    pub adapter_name: Option<String>,
    /// Trait that matched (if tier 1).
    #[ts(optional)]
    pub trait_used: Option<String>,
    /// Signature similarity that carried the pick (rung 0 only) — observability
    /// for the distance rung, absent on every other tier.
    #[ts(optional)]
    pub similarity: Option<f32>,
    /// How long the selection took (microseconds).
    pub decision_time_us: f64,
}

/// The need's embedding, pre-computed by the (async) caller so selection stays
/// pure + sync — the same split `memory/recall` uses (`query_embedding` on the
/// query, layers never embed inline).
#[derive(Debug, Clone)]
pub struct NeedEmbedding {
    /// Embedding-space identity (`EmbeddingProvider::id()`); signatures from a
    /// different space answer `None` and simply don't compete.
    pub embedder_id: String,
    pub vector: Vec<f32>,
    /// The space's MEASURED unrelated-cosine null (mean, std) when the provider
    /// knows it — the significance floor derives from it (never an absolute
    /// cosine threshold in an anisotropic space).
    pub unrelated_null: Option<(f32, f32)>,
}

/// Significance floor when the space's null is unmeasured: the recall space's
/// unrelated band sits ≈0.25–0.30 (recall_faculty's war story), so clearing it
/// by a wide margin is required before distance may outrank a declared trait.
const SIGNATURE_FLOOR_WITHOUT_NULL: f32 = 0.45;

/// How many stds above the unrelated null a signature match must clear when the
/// null IS measured — same 2σ discipline recall uses.
const SIGNATURE_NULL_SIGMAS: f32 = 2.0;

/// Hard failure when no adapter-backed model satisfies a persona turn.
#[derive(Debug, Clone, Serialize, Deserialize, TS, thiserror::Error)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/ModelSelectionError.ts"
)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum ModelSelectionError {
    #[error(
        "no trained model candidate for persona {persona_id}; task_domain={task_domain:?}; adapters={adapter_count}"
    )]
    NoCandidate {
        #[ts(type = "string")]
        persona_id: uuid::Uuid,
        #[ts(optional)]
        task_domain: Option<String>,
        adapter_count: usize,
        adapters_with_trained_model: usize,
    },
}

/// Adapter info synced from TypeScript to Rust.
/// Lightweight: only what's needed for model selection decisions.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/AdapterInfo.ts"
)]
pub struct AdapterInfo {
    /// Adapter name (e.g. "typescript-expertise", "conversational")
    pub name: String,
    /// Trait/domain this adapter specializes in (e.g. "reasoning_style", "tone_and_voice")
    pub domain: String,
    /// Trained model name for inference (if available after LoRA fine-tuning)
    #[ts(optional)]
    pub trained_model_name: Option<String>,
    /// Is this adapter currently loaded in memory?
    pub is_loaded: bool,
    /// Is this the current active adapter?
    pub is_current: bool,
    /// LRU priority (0.0-1.0)
    pub priority: f32,
}

/// Per-persona adapter registry state.
/// Synced from TypeScript genome state.
#[derive(Debug, Clone, Default)]
pub struct AdapterRegistry {
    /// All known adapters keyed by name.
    pub adapters: HashMap<String, AdapterInfo>,
}

// =============================================================================
// DOMAIN → TRAIT MAPPING
// =============================================================================

/// Maps a task domain string to the relevant personality trait.
/// This is the canonical mapping — TypeScript no longer has its own copy.
pub fn domain_to_trait(domain: &str) -> &'static str {
    match domain.to_lowercase().as_str() {
        "code" | "debug" | "analysis" => "reasoning_style",
        "creative" | "art" | "writing" => "creative_expression",
        "support" | "help" | "social" => "social_dynamics",
        "facts" | "knowledge" | "expertise" => "domain_expertise",
        _ => "tone_and_voice",
    }
}

// =============================================================================
// MODEL SELECTION
// =============================================================================

/// Select the best model using the adapter priority chain.
///
/// Tier 1: Trait-specific adapter (domain → trait → adapter with trained_model_name)
/// Tier 2: Current active adapter (is_current=true with trained_model_name)
/// Tier 3: Any adapter with an trained_model_name
pub fn select_model(
    request: &ModelSelectionRequest,
    registry: &AdapterRegistry,
) -> Result<ModelSelectionResult, ModelSelectionError> {
    select_model_with_signatures(request, registry, None, &std::collections::HashMap::new())
}

/// [`select_model`] plus rung 0: when the caller pre-computed the need's
/// embedding AND adapters carry minted signatures (keyed by adapter NAME —
/// the alias the whole adoption chain speaks), the nearest signed adapter
/// above the significance floor wins with `source: "signature_distance"`.
/// No embedding / no signatures / nothing above the floor → identical
/// behavior to the keyword tiers.
pub fn select_model_with_signatures(
    request: &ModelSelectionRequest,
    registry: &AdapterRegistry,
    need: Option<&NeedEmbedding>,
    signatures: &std::collections::HashMap<String, crate::genome::signature::GeneSignature>,
) -> Result<ModelSelectionResult, ModelSelectionError> {
    let start = Instant::now();

    // RUNG 0: signature distance — proximity beats keyword guessing, but only
    // significantly ([[gene-routing-is-distance-not-keywords]]).
    if let Some(need) = need {
        let floor = match need.unrelated_null {
            Some((mean, std)) => mean + SIGNATURE_NULL_SIGMAS * std,
            None => SIGNATURE_FLOOR_WITHOUT_NULL,
        };
        let best = registry
            .adapters
            .values()
            .filter(|a| a.trained_model_name.is_some())
            .filter_map(|a| {
                signatures
                    .get(&a.name)
                    .and_then(|sig| sig.similarity_in(&need.embedder_id, &need.vector))
                    .map(|sim| (a, sim))
            })
            .filter(|(_, sim)| *sim > floor)
            .max_by(|(a, sa), (b, sb)| {
                sa.partial_cmp(sb)
                    .unwrap_or(std::cmp::Ordering::Equal) // NaN can't arise from cosine over finite vectors; Equal keeps the max total
                    .then((a.is_loaded as u8).cmp(&(b.is_loaded as u8)))
            });
        if let Some((adapter, sim)) = best {
            return Ok(ModelSelectionResult {
                model: adapter.trained_model_name.clone().unwrap(), // filtered on is_some() four lines up, same as every tier below
                source: "signature_distance".into(),
                adapter_name: Some(adapter.name.clone()),
                trait_used: None,
                similarity: Some(sim),
                decision_time_us: start.elapsed().as_secs_f64() * 1_000_000.0,
            });
        }
    }

    // TIER 1: Trait-specific adapter
    if let Some(ref domain) = request.task_domain {
        let target_trait = domain_to_trait(domain);
        // Prefer loaded adapters, then any matching
        let trait_match = registry
            .adapters
            .values()
            .filter(|a| a.domain == target_trait && a.trained_model_name.is_some())
            .max_by(|a, b| {
                // Prefer loaded > unloaded, then higher priority
                (a.is_loaded as u8, (a.priority * 1000.0) as u32)
                    .cmp(&(b.is_loaded as u8, (b.priority * 1000.0) as u32))
            });

        if let Some(adapter) = trait_match {
            return Ok(ModelSelectionResult {
                model: adapter.trained_model_name.clone().unwrap(),
                source: "trait_adapter".into(),
                adapter_name: Some(adapter.name.clone()),
                trait_used: Some(target_trait.to_string()),
                similarity: None,
                decision_time_us: start.elapsed().as_secs_f64() * 1_000_000.0,
            });
        }
    }

    // TIER 2: Current active adapter
    let current = registry
        .adapters
        .values()
        .find(|a| a.is_current && a.trained_model_name.is_some());

    if let Some(adapter) = current {
        return Ok(ModelSelectionResult {
            model: adapter.trained_model_name.clone().unwrap(),
            source: "current_adapter".into(),
            adapter_name: Some(adapter.name.clone()),
            trait_used: None,
            similarity: None,
            decision_time_us: start.elapsed().as_secs_f64() * 1_000_000.0,
        });
    }

    // TIER 3: Any available adapter with a trained model name
    let any_adapter = registry
        .adapters
        .values()
        .filter(|a| a.trained_model_name.is_some())
        .max_by(|a, b| {
            (a.is_loaded as u8, (a.priority * 1000.0) as u32)
                .cmp(&(b.is_loaded as u8, (b.priority * 1000.0) as u32))
        });

    if let Some(adapter) = any_adapter {
        return Ok(ModelSelectionResult {
            model: adapter.trained_model_name.clone().unwrap(),
            source: "any_adapter".into(),
            adapter_name: Some(adapter.name.clone()),
            trait_used: None,
            similarity: None,
            decision_time_us: start.elapsed().as_secs_f64() * 1_000_000.0,
        });
    }

    Err(ModelSelectionError::NoCandidate {
        persona_id: request.persona_id,
        task_domain: request.task_domain.clone(),
        adapter_count: registry.adapters.len(),
        adapters_with_trained_model: registry
            .adapters
            .values()
            .filter(|a| a.trained_model_name.is_some())
            .count(),
    })
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn make_request(domain: Option<&str>) -> ModelSelectionRequest {
        ModelSelectionRequest {
            persona_id: Uuid::new_v4(),
            task_domain: domain.map(String::from),
            need: None,
        }
    }

    fn make_adapter(
        name: &str,
        domain: &str,
        model_name: Option<&str>,
        loaded: bool,
        current: bool,
    ) -> AdapterInfo {
        AdapterInfo {
            name: name.to_string(),
            domain: domain.to_string(),
            trained_model_name: model_name.map(String::from),
            is_loaded: loaded,
            is_current: current,
            priority: 0.5,
        }
    }

    #[test]
    fn test_domain_to_trait_mapping() {
        assert_eq!(domain_to_trait("code"), "reasoning_style");
        assert_eq!(domain_to_trait("debug"), "reasoning_style");
        assert_eq!(domain_to_trait("analysis"), "reasoning_style");
        assert_eq!(domain_to_trait("creative"), "creative_expression");
        assert_eq!(domain_to_trait("art"), "creative_expression");
        assert_eq!(domain_to_trait("writing"), "creative_expression");
        assert_eq!(domain_to_trait("support"), "social_dynamics");
        assert_eq!(domain_to_trait("help"), "social_dynamics");
        assert_eq!(domain_to_trait("social"), "social_dynamics");
        assert_eq!(domain_to_trait("facts"), "domain_expertise");
        assert_eq!(domain_to_trait("knowledge"), "domain_expertise");
        assert_eq!(domain_to_trait("expertise"), "domain_expertise");
        assert_eq!(domain_to_trait("chat"), "tone_and_voice");
        assert_eq!(domain_to_trait("unknown"), "tone_and_voice");
        // Case insensitive
        assert_eq!(domain_to_trait("CODE"), "reasoning_style");
        assert_eq!(domain_to_trait("Creative"), "creative_expression");
    }

    #[test]
    fn test_tier1_trait_specific_adapter() {
        let mut registry = AdapterRegistry::default();
        registry.adapters.insert(
            "code-expert".into(),
            make_adapter(
                "code-expert",
                "reasoning_style",
                Some("codellama:7b"),
                true,
                false,
            ),
        );

        let req = make_request(Some("code"));
        let result = select_model(&req, &registry).unwrap();

        assert_eq!(result.model, "codellama:7b");
        assert_eq!(result.source, "trait_adapter");
        assert_eq!(result.adapter_name.as_deref(), Some("code-expert"));
        assert_eq!(result.trait_used.as_deref(), Some("reasoning_style"));
    }

    #[test]
    fn test_tier1_prefers_loaded_adapter() {
        let mut registry = AdapterRegistry::default();
        registry.adapters.insert(
            "code-unloaded".into(),
            make_adapter(
                "code-unloaded",
                "reasoning_style",
                Some("codellama:7b-unloaded"),
                false,
                false,
            ),
        );
        registry.adapters.insert(
            "code-loaded".into(),
            make_adapter(
                "code-loaded",
                "reasoning_style",
                Some("codellama:7b-loaded"),
                true,
                false,
            ),
        );

        let req = make_request(Some("code"));
        let result = select_model(&req, &registry).unwrap();

        assert_eq!(result.model, "codellama:7b-loaded");
        assert_eq!(result.source, "trait_adapter");
    }

    #[test]
    fn test_tier2_current_adapter() {
        let mut registry = AdapterRegistry::default();
        // No matching trait adapter, but has current adapter
        registry.adapters.insert(
            "conversational".into(),
            make_adapter(
                "conversational",
                "tone_and_voice",
                Some("llama3:8b-tuned"),
                true,
                true,
            ),
        );

        let req = make_request(Some("code"));
        let result = select_model(&req, &registry).unwrap();

        // code → reasoning_style, no match → falls to tier 2
        assert_eq!(result.model, "llama3:8b-tuned");
        assert_eq!(result.source, "current_adapter");
    }

    #[test]
    fn test_tier3_any_adapter() {
        let mut registry = AdapterRegistry::default();
        // Not current, but has trained model
        registry.adapters.insert(
            "creative-writer".into(),
            make_adapter(
                "creative-writer",
                "creative_expression",
                Some("mistral:7b-creative"),
                false,
                false,
            ),
        );

        let req = make_request(Some("code"));
        let result = select_model(&req, &registry).unwrap();

        // No trait match, no current → tier 3
        assert_eq!(result.model, "mistral:7b-creative");
        assert_eq!(result.source, "any_adapter");
    }

    #[test]
    fn test_empty_registry_fails_hard() {
        let registry = AdapterRegistry::default(); // empty

        let req = make_request(Some("code"));
        let err = select_model(&req, &registry).unwrap_err();

        match err {
            ModelSelectionError::NoCandidate {
                persona_id,
                task_domain,
                adapter_count,
                adapters_with_trained_model,
            } => {
                assert_eq!(persona_id, req.persona_id);
                assert_eq!(task_domain.as_deref(), Some("code"));
                assert_eq!(adapter_count, 0);
                assert_eq!(adapters_with_trained_model, 0);
            }
        }
    }

    #[test]
    fn test_no_domain_skips_tier1() {
        let mut registry = AdapterRegistry::default();
        registry.adapters.insert(
            "code-expert".into(),
            make_adapter(
                "code-expert",
                "reasoning_style",
                Some("codellama:7b"),
                true,
                false,
            ),
        );

        // No task_domain → skip tier 1, no current → tier 3
        let req = make_request(None);
        let result = select_model(&req, &registry).unwrap();

        assert_eq!(result.model, "codellama:7b");
        assert_eq!(result.source, "any_adapter");
    }

    #[test]
    fn test_adapter_without_trained_model_skipped() {
        let mut registry = AdapterRegistry::default();
        // Adapter exists but no trained_model_name
        registry.adapters.insert(
            "training-only".into(),
            make_adapter("training-only", "reasoning_style", None, true, true),
        );

        let req = make_request(Some("code"));
        let err = select_model(&req, &registry).unwrap_err();

        match err {
            ModelSelectionError::NoCandidate {
                adapter_count,
                adapters_with_trained_model,
                ..
            } => {
                assert_eq!(adapter_count, 1);
                assert_eq!(adapters_with_trained_model, 0);
            }
        }
    }

    // what this catches: rung 0's whole contract, the distance-routing claim made
    // mechanical ([[gene-routing-is-distance-not-keywords]]): (a) a SIGNED adapter
    // near the need outranks the keyword tier even when the keyword table would
    // pick another adapter; (b) below the significance floor the rung stays SILENT
    // and every legacy tier behaves exactly as before (no-embedding calls are
    // pinned by the untouched pre-rung tests); (c) wrong-space signatures don't
    // compete (honest absence, never a lying 0).
    #[tokio::test]
    async fn rung0_distance_beats_keywords_only_when_significant_and_same_space() {
        use crate::cognition::embedding::{EmbeddingProvider, LexicalEmbedder};
        use std::sync::Arc;
        let embedder: Arc<dyn EmbeddingProvider> = Arc::new(LexicalEmbedder::default());
        let corpus: Vec<String> =
            vec!["fold the list recursively".into(), "map over the vector".into()];
        let corpus_ref = crate::forge::recipe::CorpusRef {
            name: "fp-corpus".into(),
            content_hash: "sha256:00".into(),
            size_bytes: 1,
            source_url: None,
        };
        let sig = crate::genome::signature::GeneSignature::mint(&corpus, corpus_ref, &embedder, 0)
            .await
            .expect("mint");

        let mut registry = AdapterRegistry::default();
        // The keyword tier's darling: matches domain_to_trait("code") directly.
        registry.adapters.insert(
            "keyword-pick".into(),
            make_adapter("keyword-pick", "reasoning_style", Some("keyword-model"), true, false),
        );
        // The signed FP gene: its DOMAIN is unrelated junk (keywords would never
        // pick it) — only its minted signature can carry it.
        registry.adapters.insert(
            "fp-gene".into(),
            make_adapter("fp-gene", "cooking_recipes", Some("fp-model"), false, false),
        );
        let mut signatures = std::collections::HashMap::new();
        signatures.insert("fp-gene".to_string(), sig.clone());

        let need_text = "fold the list recursively";
        let need = NeedEmbedding {
            embedder_id: embedder.id().to_string(),
            vector: embedder.embed(need_text).await,
            unrelated_null: embedder.unrelated_null(),
        };
        let req = make_request(Some("code"));

        // (a) near + significant → the distance rung wins over the keyword tier.
        let picked = select_model_with_signatures(&req, &registry, Some(&need), &signatures)
            .expect("select");
        assert_eq!(picked.source, "signature_distance");
        assert_eq!(picked.adapter_name.as_deref(), Some("fp-gene"));
        assert!(picked.similarity.expect("carried") > 0.0);

        // (b) far below the floor → silent rung, keyword tier answers as before.
        let far = NeedEmbedding {
            embedder_id: embedder.id().to_string(),
            vector: embedder.embed("quarterly tax accounting spreadsheet").await,
            unrelated_null: embedder.unrelated_null(),
        };
        let fell = select_model_with_signatures(&req, &registry, Some(&far), &signatures)
            .expect("select");
        assert_eq!(fell.source, "trait_adapter");
        assert_eq!(fell.adapter_name.as_deref(), Some("keyword-pick"));

        // (c) wrong-space signature never competes.
        let mut alien = signatures.clone();
        alien.get_mut("fp-gene").expect("present").embedder = "some-other-space".into();
        let unswayed = select_model_with_signatures(&req, &registry, Some(&need), &alien)
            .expect("select");
        assert_eq!(unswayed.source, "trait_adapter");
    }

    #[test]
    fn test_decision_time_is_fast() {
        let registry = AdapterRegistry::default();
        let req = make_request(Some("code"));
        let start = Instant::now();
        let result = select_model(&req, &registry);
        let decision_time_us = start.elapsed().as_secs_f64() * 1_000_000.0;

        assert!(result.is_err());
        // what this catches: selection ceasing to be a pure in-memory decision
        // (blocking I/O / a lock sneaking in). Deliberately NOT a perf SLA: the
        // old <500us bound flaked at 982us on a machine running 4-way inference
        // (2026-08-23) — a scheduler stall is not a selection regression, the
        // same stopwatch-test class as tool_parsing's parse_time fix. 100ms only
        // catches an accidental syscall/await, which is the actual invariant.
        assert!(
            decision_time_us < 100_000.0,
            "selection stopped being an in-memory decision: {decision_time_us}us"
        );
    }
}
