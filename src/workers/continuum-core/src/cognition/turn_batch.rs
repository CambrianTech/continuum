//! Rust-owned turn batching contract for recipe/RAG orchestration.
//!
//! This module is intentionally pure: no ORM, no inference, no IPC, no
//! filesystem. The host passes the room trigger, persona candidates, and
//! active RAG source names; Rust returns a deterministic turn plan that
//! defines what is shared once per turn and what remains per-persona.
//!
//! Node may still load entities and render UI, but it should not invent
//! batching keys, duplicate persona admission rules, or source fan-out
//! policy. Those belong here so every host (desktop, Docker, game engine,
//! airc bridge) sees the same control-plane shape.

use crate::model_registry::Capability;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, HashSet};
use ts_rs::TS;
use uuid::Uuid;

/// Message/event that starts one cognition turn.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../shared/generated/cognition/RecipeTurnTrigger.ts"
)]
pub struct RecipeTurnTrigger {
    #[ts(type = "string")]
    pub room_id: Uuid,
    #[ts(optional, type = "string")]
    pub message_id: Option<Uuid>,
    pub text: String,
    #[ts(type = "number")]
    pub timestamp_ms: u64,
}

/// Lightweight persona candidate used for admission + RAG planning.
///
/// Deliberately smaller than `PersonaContext`: no full system prompt, no
/// recent history, no media blobs. The batch planner should be cheap enough
/// to run before any heavyweight context build.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../shared/generated/cognition/RecipePersonaCandidate.ts"
)]
pub struct RecipePersonaCandidate {
    #[ts(type = "string")]
    pub persona_id: Uuid,
    pub display_name: String,
    pub specialty: String,
    pub model: String,
    pub provider: String,
    pub capabilities: Vec<Capability>,
    pub context_window: usize,
    pub max_output_tokens: usize,
    #[ts(optional)]
    pub tokens_per_second: Option<f32>,
}

/// Caller-supplied policy for one RAG source.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../shared/generated/cognition/RecipeRagSourcePolicy.ts"
)]
pub struct RecipeRagSourcePolicy {
    /// Stable source identifier, e.g. `conversation-history`.
    pub source_name: String,
    /// True when the source should be loaded once for the whole turn and
    /// reused by persona-specific prompt assembly.
    #[serde(default = "default_true")]
    pub shared_across_personas: bool,
    /// Relative budget. Zero or absent means neutral weight.
    #[serde(default)]
    pub weight: f32,
}

fn default_true() -> bool {
    true
}

/// IPC request for `cognition/plan-turn-batch`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../shared/generated/cognition/RecipeTurnBatchRequest.ts"
)]
pub struct RecipeTurnBatchRequest {
    pub trigger: RecipeTurnTrigger,
    pub personas: Vec<RecipePersonaCandidate>,
    #[serde(default)]
    pub rag_sources: Vec<RecipeRagSourcePolicy>,
    /// Total input-token budget for shared RAG planning. Per-persona
    /// generation still uses each candidate's model limits.
    #[serde(default)]
    pub total_input_budget_tokens: usize,
}

/// One shared RAG source load in the plan.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../shared/generated/cognition/SharedRagSourcePlan.ts"
)]
pub struct SharedRagSourcePlan {
    pub source_name: String,
    pub cache_key: String,
    pub budget_tokens: usize,
}

/// Persona-specific work item for the turn.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../shared/generated/cognition/PersonaTurnPlan.ts"
)]
pub struct PersonaTurnPlan {
    #[ts(type = "string")]
    pub persona_id: Uuid,
    pub display_name: String,
    pub specialty: String,
    pub model: String,
    pub provider: String,
    pub local_model: bool,
    pub generation_order: usize,
    pub persona_context_key: String,
    pub rag_cache_key: String,
    pub input_budget_tokens: usize,
    pub max_output_tokens: usize,
    pub source_names: Vec<String>,
}

/// Result of `cognition/plan-turn-batch`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../shared/generated/cognition/RecipeTurnBatchPlan.ts"
)]
pub struct RecipeTurnBatchPlan {
    pub turn_key: String,
    #[ts(type = "string")]
    pub room_id: Uuid,
    #[ts(optional, type = "string")]
    pub message_id: Option<Uuid>,
    pub query_text: String,
    pub shared_sources: Vec<SharedRagSourcePlan>,
    pub persona_plans: Vec<PersonaTurnPlan>,
    pub skipped_duplicate_persona_ids: Vec<String>,
    pub max_concurrent_local_generations: usize,
}

pub fn plan_turn_batch(req: RecipeTurnBatchRequest) -> RecipeTurnBatchPlan {
    let turn_key = stable_key(&[
        "turn",
        &req.trigger.room_id.to_string(),
        &req.trigger
            .message_id
            .map(|id| id.to_string())
            .unwrap_or_else(|| "no-message-id".to_string()),
        &req.trigger.timestamp_ms.to_string(),
        req.trigger.text.trim(),
    ]);

    let source_policies = normalize_sources(req.rag_sources);
    let shared_source_names: Vec<String> = source_policies
        .iter()
        .filter(|source| source.shared_across_personas)
        .map(|source| source.source_name.clone())
        .collect();
    let shared_sources =
        build_shared_sources(&turn_key, &source_policies, req.total_input_budget_tokens);

    let mut seen_personas = HashSet::new();
    let mut skipped_duplicate_persona_ids = Vec::new();
    let mut persona_plans = Vec::new();

    for candidate in req.personas {
        if !seen_personas.insert(candidate.persona_id) {
            skipped_duplicate_persona_ids.push(candidate.persona_id.to_string());
            continue;
        }

        let generation_order = persona_plans.len();
        let input_budget_tokens = candidate
            .context_window
            .saturating_sub(candidate.max_output_tokens)
            .saturating_sub(1024);
        let persona_context_key = stable_key(&[
            "persona-context",
            &turn_key,
            &candidate.persona_id.to_string(),
            &candidate.model,
            &candidate.specialty,
        ]);
        let rag_cache_key = stable_key(&[
            "persona-rag",
            &turn_key,
            &candidate.persona_id.to_string(),
            &shared_source_names.join("|"),
        ]);

        persona_plans.push(PersonaTurnPlan {
            persona_id: candidate.persona_id,
            display_name: candidate.display_name,
            specialty: candidate.specialty,
            model: candidate.model.clone(),
            provider: candidate.provider.clone(),
            local_model: is_local_provider(&candidate.provider, &candidate.model),
            generation_order,
            persona_context_key,
            rag_cache_key,
            input_budget_tokens,
            max_output_tokens: candidate.max_output_tokens,
            source_names: shared_source_names.clone(),
        });
    }

    RecipeTurnBatchPlan {
        turn_key,
        room_id: req.trigger.room_id,
        message_id: req.trigger.message_id,
        query_text: req.trigger.text,
        shared_sources,
        persona_plans,
        skipped_duplicate_persona_ids,
        max_concurrent_local_generations: 1,
    }
}

fn normalize_sources(sources: Vec<RecipeRagSourcePolicy>) -> Vec<RecipeRagSourcePolicy> {
    let mut seen = BTreeSet::new();
    let mut normalized = Vec::new();

    for mut source in sources {
        let name = source.source_name.trim().to_string();
        if name.is_empty() || !seen.insert(name.clone()) {
            continue;
        }
        source.source_name = name;
        normalized.push(source);
    }

    normalized.sort_by(|a, b| a.source_name.cmp(&b.source_name));
    normalized
}

fn build_shared_sources(
    turn_key: &str,
    sources: &[RecipeRagSourcePolicy],
    total_budget: usize,
) -> Vec<SharedRagSourcePlan> {
    let shared: Vec<&RecipeRagSourcePolicy> = sources
        .iter()
        .filter(|source| source.shared_across_personas)
        .collect();
    if shared.is_empty() {
        return Vec::new();
    }

    let positive_weight_sum: f32 = shared.iter().map(|source| source.weight.max(0.0)).sum();
    let equal_budget = if total_budget == 0 {
        0
    } else {
        total_budget / shared.len()
    };

    shared
        .into_iter()
        .map(|source| {
            let budget_tokens = if total_budget == 0 {
                0
            } else if positive_weight_sum > 0.0 && source.weight > 0.0 {
                ((total_budget as f32) * (source.weight / positive_weight_sum)).round() as usize
            } else {
                equal_budget
            };

            SharedRagSourcePlan {
                source_name: source.source_name.clone(),
                cache_key: stable_key(&["shared-rag", turn_key, &source.source_name]),
                budget_tokens,
            }
        })
        .collect()
}

fn is_local_provider(provider: &str, model: &str) -> bool {
    let provider = provider.to_ascii_lowercase();
    provider == "local"
        || provider == "dmr"
        || model.starts_with("continuum-ai/")
        || model.starts_with("qwen")
}

fn stable_key(parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part.as_bytes());
    }
    let digest = hasher.finalize();
    let mut out = String::with_capacity(24);
    for byte in digest.iter().take(12) {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trigger() -> RecipeTurnTrigger {
        RecipeTurnTrigger {
            room_id: Uuid::parse_str("aaaaaaaa-aaaa-4aaa-aaaa-aaaaaaaaaaaa").unwrap(),
            message_id: Some(Uuid::parse_str("bbbbbbbb-bbbb-4bbb-bbbb-bbbbbbbbbbbb").unwrap()),
            text: "explain the smoke failure".to_string(),
            timestamp_ms: 1_778_200_000,
        }
    }

    fn candidate(id: &str, name: &str, provider: &str) -> RecipePersonaCandidate {
        RecipePersonaCandidate {
            persona_id: Uuid::parse_str(id).unwrap(),
            display_name: name.to_string(),
            specialty: "code".to_string(),
            model: "continuum-ai/qwen3.5-4b-code-forged".to_string(),
            provider: provider.to_string(),
            capabilities: vec![Capability::TextGeneration, Capability::Chat],
            context_window: 262_144,
            max_output_tokens: 32_768,
            tokens_per_second: Some(12.0),
        }
    }

    fn request() -> RecipeTurnBatchRequest {
        RecipeTurnBatchRequest {
            trigger: trigger(),
            personas: vec![
                candidate(
                    "11111111-1111-4111-8111-111111111111",
                    "CodeReview AI",
                    "local",
                ),
                candidate("22222222-2222-4222-8222-222222222222", "Helper AI", "local"),
            ],
            rag_sources: vec![
                RecipeRagSourcePolicy {
                    source_name: "semantic-memory".to_string(),
                    shared_across_personas: true,
                    weight: 2.0,
                },
                RecipeRagSourcePolicy {
                    source_name: "conversation-history".to_string(),
                    shared_across_personas: true,
                    weight: 1.0,
                },
            ],
            total_input_budget_tokens: 12_000,
        }
    }

    #[test]
    fn turn_plan_is_deterministic() {
        let first = plan_turn_batch(request());
        let second = plan_turn_batch(request());

        assert_eq!(first.turn_key, second.turn_key);
        assert_eq!(
            first.shared_sources[0].cache_key,
            second.shared_sources[0].cache_key
        );
        assert_eq!(
            first.persona_plans[0].persona_context_key,
            second.persona_plans[0].persona_context_key
        );
    }

    #[test]
    fn deduplicates_persona_candidates() {
        let mut req = request();
        req.personas.push(candidate(
            "11111111-1111-4111-8111-111111111111",
            "Duplicate",
            "local",
        ));

        let plan = plan_turn_batch(req);

        assert_eq!(plan.persona_plans.len(), 2);
        assert_eq!(plan.skipped_duplicate_persona_ids.len(), 1);
        assert_eq!(
            plan.skipped_duplicate_persona_ids[0],
            "11111111-1111-4111-8111-111111111111"
        );
    }

    #[test]
    fn shared_sources_are_sorted_and_weighted_once() {
        let plan = plan_turn_batch(request());
        let names: Vec<&str> = plan
            .shared_sources
            .iter()
            .map(|source| source.source_name.as_str())
            .collect();

        assert_eq!(names, vec!["conversation-history", "semantic-memory"]);
        assert_eq!(plan.shared_sources[0].budget_tokens, 4_000);
        assert_eq!(plan.shared_sources[1].budget_tokens, 8_000);
        assert_eq!(
            plan.persona_plans[0].source_names,
            vec![
                "conversation-history".to_string(),
                "semantic-memory".to_string()
            ]
        );
    }

    #[test]
    fn local_generation_is_single_lane_until_pressure_broker_expands_it() {
        let plan = plan_turn_batch(request());

        assert_eq!(plan.max_concurrent_local_generations, 1);
        assert!(plan.persona_plans.iter().all(|p| p.local_model));
        assert_eq!(plan.persona_plans[0].generation_order, 0);
        assert_eq!(plan.persona_plans[1].generation_order, 1);
    }
}
