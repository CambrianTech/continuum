//! Model Selection Engine
//!
//! Moves the 4-tier model priority chain from TypeScript to Rust.
//! Decisions in Rust, execution in TypeScript.
//!
//! Priority chain:
//! 1. Trait-specific adapter (domain → trait mapping, e.g. "code" → reasoning_style)
//! 2. Current active adapter (most recently used)
//! 3. Any available trained adapter
//! 4. Configured base model fallback

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Instant;
use ts_rs::TS;

// =============================================================================
// TYPES (ts-rs generated)
// =============================================================================

/// Request to select the best model for a persona given optional task context.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/persona/ModelSelectionRequest.ts")]
pub struct ModelSelectionRequest {
    #[ts(type = "string")]
    pub persona_id: uuid::Uuid,
    /// Optional task domain for trait-specific adapter lookup.
    /// Values: "code", "debug", "analysis", "creative", "art", "writing",
    ///         "support", "help", "social", "facts", "knowledge", "expertise"
    #[ts(optional)]
    pub task_domain: Option<String>,
    /// Configured base model (fallback tier 4).
    pub base_model: String,
}

/// Result of model selection — which model to use and why.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/persona/ModelSelectionResult.ts")]
pub struct ModelSelectionResult {
    /// The selected model name (Ollama model ID or base model).
    pub model: String,
    /// Which tier selected it: "trait_adapter", "current_adapter", "any_adapter", "base_model"
    pub source: String,
    /// Name of the adapter used (if any).
    #[ts(optional)]
    pub adapter_name: Option<String>,
    /// Trait that matched (if tier 1).
    #[ts(optional)]
    pub trait_used: Option<String>,
    /// How long the selection took (microseconds).
    pub decision_time_us: f64,
}

/// Adapter info synced from TypeScript to Rust.
/// Lightweight: only what's needed for model selection decisions.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/persona/AdapterInfo.ts")]
pub struct AdapterInfo {
    /// Adapter name (e.g. "typescript-expertise", "conversational")
    pub name: String,
    /// Trait/domain this adapter specializes in (e.g. "reasoning_style", "tone_and_voice")
    pub domain: String,
    /// Ollama model name for inference (if available)
    #[ts(optional)]
    pub ollama_model_name: Option<String>,
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

/// Select the best model using the 4-tier priority chain.
///
/// Tier 1: Trait-specific adapter (domain → trait → adapter with ollama_model_name)
/// Tier 2: Current active adapter (is_current=true with ollama_model_name)
/// Tier 3: Any adapter with an ollama_model_name
/// Tier 4: base_model fallback
pub fn select_model(
    request: &ModelSelectionRequest,
    registry: &AdapterRegistry,
) -> ModelSelectionResult {
    let start = Instant::now();

    // TIER 1: Trait-specific adapter
    if let Some(ref domain) = request.task_domain {
        let target_trait = domain_to_trait(domain);
        // Prefer loaded adapters, then any matching
        let trait_match = registry
            .adapters
            .values()
            .filter(|a| a.domain == target_trait && a.ollama_model_name.is_some())
            .max_by(|a, b| {
                // Prefer loaded > unloaded, then higher priority
                (a.is_loaded as u8, (a.priority * 1000.0) as u32)
                    .cmp(&(b.is_loaded as u8, (b.priority * 1000.0) as u32))
            });

        if let Some(adapter) = trait_match {
            return ModelSelectionResult {
                model: adapter.ollama_model_name.clone().unwrap(),
                source: "trait_adapter".into(),
                adapter_name: Some(adapter.name.clone()),
                trait_used: Some(target_trait.to_string()),
                decision_time_us: start.elapsed().as_secs_f64() * 1_000_000.0,
            };
        }
    }

    // TIER 2: Current active adapter
    let current = registry
        .adapters
        .values()
        .find(|a| a.is_current && a.ollama_model_name.is_some());

    if let Some(adapter) = current {
        return ModelSelectionResult {
            model: adapter.ollama_model_name.clone().unwrap(),
            source: "current_adapter".into(),
            adapter_name: Some(adapter.name.clone()),
            trait_used: None,
            decision_time_us: start.elapsed().as_secs_f64() * 1_000_000.0,
        };
    }

    // TIER 3: Any available adapter with an ollama model name
    let any_adapter = registry
        .adapters
        .values()
        .filter(|a| a.ollama_model_name.is_some())
        .max_by(|a, b| {
            (a.is_loaded as u8, (a.priority * 1000.0) as u32)
                .cmp(&(b.is_loaded as u8, (b.priority * 1000.0) as u32))
        });

    if let Some(adapter) = any_adapter {
        return ModelSelectionResult {
            model: adapter.ollama_model_name.clone().unwrap(),
            source: "any_adapter".into(),
            adapter_name: Some(adapter.name.clone()),
            trait_used: None,
            decision_time_us: start.elapsed().as_secs_f64() * 1_000_000.0,
        };
    }

    // TIER 4: Base model fallback
    ModelSelectionResult {
        model: request.base_model.clone(),
        source: "base_model".into(),
        adapter_name: None,
        trait_used: None,
        decision_time_us: start.elapsed().as_secs_f64() * 1_000_000.0,
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn make_request(domain: Option<&str>, base: &str) -> ModelSelectionRequest {
        ModelSelectionRequest {
            persona_id: Uuid::new_v4(),
            task_domain: domain.map(String::from),
            base_model: base.to_string(),
        }
    }

    fn make_adapter(
        name: &str,
        domain: &str,
        ollama: Option<&str>,
        loaded: bool,
        current: bool,
    ) -> AdapterInfo {
        AdapterInfo {
            name: name.to_string(),
            domain: domain.to_string(),
            ollama_model_name: ollama.map(String::from),
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
            make_adapter("code-expert", "reasoning_style", Some("codellama:7b"), true, false),
        );

        let req = make_request(Some("code"), "llama3:8b");
        let result = select_model(&req, &registry);

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
            make_adapter("code-unloaded", "reasoning_style", Some("codellama:7b-unloaded"), false, false),
        );
        registry.adapters.insert(
            "code-loaded".into(),
            make_adapter("code-loaded", "reasoning_style", Some("codellama:7b-loaded"), true, false),
        );

        let req = make_request(Some("code"), "llama3:8b");
        let result = select_model(&req, &registry);

        assert_eq!(result.model, "codellama:7b-loaded");
        assert_eq!(result.source, "trait_adapter");
    }

    #[test]
    fn test_tier2_current_adapter() {
        let mut registry = AdapterRegistry::default();
        // No matching trait adapter, but has current adapter
        registry.adapters.insert(
            "conversational".into(),
            make_adapter("conversational", "tone_and_voice", Some("llama3:8b-tuned"), true, true),
        );

        let req = make_request(Some("code"), "llama3:8b");
        let result = select_model(&req, &registry);

        // code → reasoning_style, no match → falls to tier 2
        assert_eq!(result.model, "llama3:8b-tuned");
        assert_eq!(result.source, "current_adapter");
    }

    #[test]
    fn test_tier3_any_adapter() {
        let mut registry = AdapterRegistry::default();
        // Not current, but has ollama model
        registry.adapters.insert(
            "creative-writer".into(),
            make_adapter("creative-writer", "creative_expression", Some("mistral:7b-creative"), false, false),
        );

        let req = make_request(Some("code"), "llama3:8b");
        let result = select_model(&req, &registry);

        // No trait match, no current → tier 3
        assert_eq!(result.model, "mistral:7b-creative");
        assert_eq!(result.source, "any_adapter");
    }

    #[test]
    fn test_tier4_base_model_fallback() {
        let registry = AdapterRegistry::default(); // empty

        let req = make_request(Some("code"), "llama3:8b");
        let result = select_model(&req, &registry);

        assert_eq!(result.model, "llama3:8b");
        assert_eq!(result.source, "base_model");
        assert!(result.adapter_name.is_none());
    }

    #[test]
    fn test_no_domain_skips_tier1() {
        let mut registry = AdapterRegistry::default();
        registry.adapters.insert(
            "code-expert".into(),
            make_adapter("code-expert", "reasoning_style", Some("codellama:7b"), true, false),
        );

        // No task_domain → skip tier 1, no current → tier 3
        let req = make_request(None, "llama3:8b");
        let result = select_model(&req, &registry);

        assert_eq!(result.model, "codellama:7b");
        assert_eq!(result.source, "any_adapter");
    }

    #[test]
    fn test_adapter_without_ollama_name_skipped() {
        let mut registry = AdapterRegistry::default();
        // Adapter exists but no ollama_model_name
        registry.adapters.insert(
            "training-only".into(),
            make_adapter("training-only", "reasoning_style", None, true, true),
        );

        let req = make_request(Some("code"), "llama3:8b");
        let result = select_model(&req, &registry);

        // All tiers skip because no ollama_model_name → fallback
        assert_eq!(result.model, "llama3:8b");
        assert_eq!(result.source, "base_model");
    }

    #[test]
    fn test_decision_time_is_fast() {
        let registry = AdapterRegistry::default();
        let req = make_request(Some("code"), "llama3:8b");
        let result = select_model(&req, &registry);

        // Should be sub-microsecond for empty registry
        assert!(
            result.decision_time_us < 100.0,
            "Decision should be <100μs, was {}μs",
            result.decision_time_us
        );
    }
}
