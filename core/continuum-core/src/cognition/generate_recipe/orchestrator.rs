//! AI-driven recipe generator. Wires the prompt+parser+validator shipped in
//! PR-1 to `AIProviderRegistry::generate_text` so the chat substrate's
//! recipe-generation flow can call into Rust instead of the TS path.
//!
//! Mirror of TS `RecipeGenerateServerCommand.execute` lines 27–117 — the
//! buildSystemPrompt + buildUserPrompt + AIProviderDaemon.generateText +
//! JSON.parse + validateRecipe sequence.
//!
//! ## Why no fallback
//!
//! Per #1262, the TS path returned `{ success: false, error: '...' }` on AI
//! failure, masking provider outages as parser errors. This Rust path returns
//! typed `Err(String)` on inference failure — PR-3 TS shim maps it to a
//! validationErrors[] entry that preserves the failure mode.

use crate::ai::{ChatMessage, MessageContent, TextGenerationRequest};
use crate::cognition::generate_recipe::parser::{parse_recipe_from_ai_response, ParseError};
use crate::cognition::generate_recipe::prompt::build_prompts;
use crate::cognition::generate_recipe::types::{
    RecipeDefinitionShape, RecipeGenerationRequest, RecipeGenerationResponse,
};
use crate::cognition::generate_recipe::validator::validate_recipe_structure;
use crate::modules::ai_provider::{generate_text, global_registry};

/// Default temperature for recipe generation. Mirrors TS `temperature: 0.4`
/// at line 51 — low enough to keep the JSON well-formed, high enough to
/// allow creative pipeline choices.
const DEFAULT_TEMPERATURE: f32 = 0.4;

/// Default provider when caller doesn't specify. Mirrors TS
/// `provider = 'anthropic'` default at line 29.
const DEFAULT_PROVIDER: &str = "anthropic";

/// Default model per provider. Mirrors TS `defaultModelForProvider()`
/// switch statement at lines 360–369. Pulled into a const-fn so PR-2's
/// orchestrator picks the same default the TS path picked.
fn default_model_for_provider(provider: &str) -> &'static str {
    match provider {
        "anthropic" => "claude-sonnet-4-5-20250929",
        "openai" => "gpt-4o",
        "groq" => "llama-3.3-70b-versatile",
        "deepseek" => "deepseek-chat",
        "google" => "gemini-2.5-flash",
        "xai" => "grok-3",
        _ => "claude-sonnet-4-5-20250929",
    }
}

/// Orchestrator request — extends `RecipeGenerationRequest` with optional
/// per-call provider/model/temperature overrides. Carrier for what the
/// TS path passes via `genParams`. This IS the typed params of
/// `cognition/generate-recipe` (the whole `{ request, provider?, model?,
/// temperature? }` payload deserializes into it), so it carries the full wire
/// derive set + camelCase serde.
#[derive(
    Debug,
    Clone,
    serde::Serialize,
    serde::Deserialize,
    ts_rs::TS,
    schemars::JsonSchema,
)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/GenerateRecipeOrchestratorParams.ts"
)]
pub struct GenerateRecipeOrchestratorParams {
    pub request: RecipeGenerationRequest,
    #[ts(optional)]
    pub provider: Option<String>,
    #[ts(optional)]
    pub model: Option<String>,
    #[ts(optional)]
    pub temperature: Option<f32>,
}

/// Run AI-driven recipe generation. Pure async, no global state mutation.
///
/// Order of operations (mirrors TS):
///   1. build system + user prompts from request + carried template list
///   2. dispatch ai/generate via AIProviderRegistry
///   3. parse response (regex envelope → RecipeDefinitionShape)
///   4. apply unique_id_override if set
///   5. run structural validator (no FS access; uses carried existing IDs)
///   6. return { recipe, validationErrors }
///
/// Errors that propagate as `Err`:
///   - inference dispatch failure (provider down, auth, rate limit)
///   - parser failure (no JSON envelope, malformed JSON)
///
/// Validation errors do NOT propagate as `Err` — they're returned in the
/// response so the caller (PR-3 TS shim) can decide how to render them.
/// Mirrors TS behavior: `validationErrors` go in the JTAG envelope alongside
/// the parsed recipe; `success: false` reflects the validation gate, not
/// a parse failure.
pub async fn generate_recipe_with_ai(
    params: GenerateRecipeOrchestratorParams,
) -> Result<RecipeGenerationResponse, String> {
    let GenerateRecipeOrchestratorParams {
        request,
        provider,
        model,
        temperature,
    } = params;

    let (system_prompt, user_prompt) = build_prompts(&request);

    let provider_id = provider.as_deref().unwrap_or(DEFAULT_PROVIDER).to_string();
    let model_id = model.unwrap_or_else(|| default_model_for_provider(&provider_id).to_string());

    let inference_request = TextGenerationRequest {
        messages: vec![
            ChatMessage {
                role: "system".to_string(),
                content: MessageContent::Text(system_prompt),
                name: None,
            },
            ChatMessage {
                role: "user".to_string(),
                content: MessageContent::Text(user_prompt),
                name: None,
            },
        ],
        system_prompt: None,
        model: Some(model_id),
        provider: Some(provider_id),
        temperature: Some(temperature.unwrap_or(DEFAULT_TEMPERATURE)),
        // Model owns its length (None → adapter forwards no ceiling). A full
        // RecipeDefinition's JSON envelope bounds the output, not a const of ours.
        max_tokens: None,
        top_p: None,
        top_k: None,
        repeat_penalty: None,
        frequency_penalty: None,
        repeat_last_n: None,
        stop_sequences: None,
        tools: None,
        tool_choice: None,
        response_format: None,
        active_adapters: None,
        request_id: None,
        user_id: None,
        room_id: None,
        purpose: Some("cognition-generate-recipe".to_string()),
        persona_id: None,
    };

    let registry = global_registry();
    let registry_guard = registry.read().await;
    let response = generate_text(&registry_guard, inference_request).await?;

    let parsed: RecipeDefinitionShape =
        parse_recipe_from_ai_response(&response.text).map_err(|e: ParseError| e.to_string())?;

    let recipe = apply_unique_id_override(parsed, request.unique_id_override.as_deref());

    let validation_errors = validate_recipe_structure(&recipe, &request.existing_recipe_ids);

    Ok(RecipeGenerationResponse {
        recipe,
        validation_errors,
    })
}

/// Apply the optional `unique_id_override` from the request, mirroring TS
/// `if (genParams.uniqueId) { recipe.uniqueId = genParams.uniqueId; }`.
/// Pure function so it's testable in isolation.
fn apply_unique_id_override(
    mut recipe: RecipeDefinitionShape,
    override_id: Option<&str>,
) -> RecipeDefinitionShape {
    if let Some(id) = override_id {
        recipe.unique_id = id.to_string();
    }
    recipe
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cognition::generate_recipe::types::RecipeDefinitionShape;

    /// What this catches: default model selection per provider matches TS.
    /// If the TS-side `defaultModelForProvider` ever changes (e.g. anthropic
    /// upgrades default to claude-opus-4-7), this test catches the drift
    /// before the migration silently picks a different model than the TS
    /// caller would have.
    #[test]
    fn default_model_per_provider_matches_ts() {
        assert_eq!(
            default_model_for_provider("anthropic"),
            "claude-sonnet-4-5-20250929"
        );
        assert_eq!(default_model_for_provider("openai"), "gpt-4o");
        assert_eq!(
            default_model_for_provider("groq"),
            "llama-3.3-70b-versatile"
        );
        assert_eq!(default_model_for_provider("deepseek"), "deepseek-chat");
        assert_eq!(default_model_for_provider("google"), "gemini-2.5-flash");
        assert_eq!(default_model_for_provider("xai"), "grok-3");
        // Unknown provider falls back to anthropic default — matches TS.
        assert_eq!(
            default_model_for_provider("unknown-provider"),
            "claude-sonnet-4-5-20250929"
        );
    }

    /// What this catches: the default temperature stays at the documented
    /// value. Drift here changes generation behavior silently (higher temp →
    /// more creative + more malformed-JSON failures). max_tokens is no longer
    /// pinned: the model owns its generation length (the adapter forwards no
    /// ceiling), so there's no const to drift.
    #[test]
    fn generation_constants_pinned_to_ts_defaults() {
        assert!((DEFAULT_TEMPERATURE - 0.4).abs() < 1e-6);
    }

    /// What this catches: unique_id_override applies cleanly. The TS path
    /// runs this AFTER parse but BEFORE validation; validator then sees
    /// the overridden ID for kebab-case + duplicate checks.
    #[test]
    fn unique_id_override_replaces_parsed_id() {
        let recipe = RecipeDefinitionShape {
            unique_id: "ai-generated-name".into(),
            ..Default::default()
        };
        let result = apply_unique_id_override(recipe, Some("user-supplied-name"));
        assert_eq!(result.unique_id, "user-supplied-name");
    }

    /// What this catches: no override → no mutation. Passing None must
    /// preserve the AI-emitted uniqueId verbatim.
    #[test]
    fn no_unique_id_override_preserves_parsed_id() {
        let recipe = RecipeDefinitionShape {
            unique_id: "ai-generated-name".into(),
            ..Default::default()
        };
        let result = apply_unique_id_override(recipe.clone(), None);
        assert_eq!(result.unique_id, "ai-generated-name");
        assert_eq!(result, recipe);
    }
}
