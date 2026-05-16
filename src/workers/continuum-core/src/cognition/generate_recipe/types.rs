//! Wire types for `cognition/generate-recipe`. ts-rs exports keep TS in sync.
//!
//! Mirror of the TS types in `commands/recipe/generate/shared/RecipeGenerateTypes.ts`
//! (`RecipeGenerateParams`/`Result`) and the dynamic-context types this oxidization
//! introduces (`RecipeTemplateInfo` from `system/sentinel/pipelines/TemplateRegistry.ts`,
//! existing-recipe-IDs from `RecipeLoader.getInstance().getAllRecipes()`).
//!
//! Carrier-types choice (per the #1295 design comment): the runtime registry state
//! that the TS prompt depends on (TemplateRegistry.list() + existing recipe IDs)
//! crosses the IPC boundary as explicit request fields rather than as Rust-side
//! global state. Keeps the prompt builder pure + testable + parity-checkable.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// One sentinel template the host knows about. Carrier shape — mirrors the
/// fields TS `TemplateRegistry.list()` emits per entry that the prompt needs
/// (name + description + required fields). Not the full internal template
/// struct — only what the prompt renders.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../shared/generated/cognition/RecipeTemplateInfo.ts"
)]
pub struct RecipeTemplateInfo {
    pub name: String,
    pub description: String,
    pub required_fields: Vec<String>,
}

/// Optional generation hints — mirrors TS `RecipeGenerateParams.hints` exactly.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../shared/generated/cognition/RecipeGenerateHints.ts"
)]
pub struct RecipeGenerateHints {
    #[ts(optional)]
    pub category: Option<String>,
    #[ts(optional)]
    pub templates: Option<Vec<String>>,
    #[ts(optional)]
    pub tags: Option<Vec<String>>,
    #[ts(optional)]
    pub pattern: Option<String>,
}

/// PR-1 input: pure data, no IPC, no global state.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../shared/generated/cognition/RecipeGenerationRequest.ts"
)]
pub struct RecipeGenerationRequest {
    /// Natural language description of the recipe to generate.
    pub description: String,
    /// Sentinel templates available at generation time. Carried because
    /// `buildSystemPrompt()` depends on this list — without it, the prompt
    /// silently drifts between TS and Rust.
    pub available_templates: Vec<RecipeTemplateInfo>,
    /// Existing recipe uniqueIds (for in-prompt collision-avoidance hint AND
    /// for a structural duplicate check the Rust validator runs). The TS
    /// shim gathers this from `RecipeLoader.getInstance().getAllRecipes()`.
    /// Filesystem collision check stays TS-side because it's pure FS state.
    pub existing_recipe_ids: Vec<String>,
    #[ts(optional)]
    pub hints: Option<RecipeGenerateHints>,
    /// If set, overrides the LLM-emitted uniqueId on the parsed recipe.
    /// Mirrors `genParams.uniqueId` in the TS path.
    #[ts(optional)]
    pub unique_id_override: Option<String>,
}

/// Lightweight Rust shape mirroring the TS `RecipeDefinition` envelope.
///
/// The TS `RecipeDefinition` interface (system/recipes/shared/RecipeTypes.ts)
/// has many optional/nested fields; this struct carries the FIELDS THE VALIDATOR
/// READS so PR-1 can run structural validation without depending on the full
/// type definition. Kept minimal on purpose — extending it later for richer
/// validation is additive (add a field, mark `#[serde(default)]` or `Option`).
///
/// Why the "shape" suffix: this is NOT the canonical RecipeDefinition (that
/// stays TS-side, owned by the recipes module). This is the slice the
/// generator pipeline produces + the validator inspects.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../shared/generated/cognition/RecipeDefinitionShape.ts"
)]
pub struct RecipeDefinitionShape {
    #[serde(default)]
    pub unique_id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub version: Option<u32>,
    /// Pipeline steps. Carried as raw `serde_json::Value` because PR-1's
    /// validator only checks shape (array, each item has `command` +
    /// `params`), not semantic correctness of arbitrary command params.
    #[serde(default)]
    #[ts(type = "Array<unknown>")]
    pub pipeline: Vec<serde_json::Value>,
    /// RAG template — carried as opaque value; validator checks `.messageHistory` exists.
    #[serde(default)]
    #[ts(type = "unknown")]
    pub rag_template: serde_json::Value,
    /// Strategy — carried as opaque value; validator checks `.conversationPattern`
    /// is a known enum + `.responseRules` + `.decisionCriteria` are arrays.
    #[serde(default)]
    #[ts(type = "unknown")]
    pub strategy: serde_json::Value,
    #[serde(default)]
    #[ts(type = "Array<unknown>")]
    pub roles: Vec<serde_json::Value>,
    #[serde(default)]
    pub sentinel_templates: Vec<String>,
    #[serde(default)]
    pub is_public: Option<bool>,
    #[serde(default)]
    pub tags: Vec<String>,
}

/// PR-1 output envelope — the parsed recipe + structural validation errors.
/// Empty `validation_errors` means the recipe passed structural validation;
/// the TS shim still has to do the filesystem collision check and the actual
/// save before declaring `success: true` on the JTAG envelope.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../shared/generated/cognition/RecipeGenerationResponse.ts"
)]
pub struct RecipeGenerationResponse {
    pub recipe: RecipeDefinitionShape,
    pub validation_errors: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What this catches: serde camelCase round-trip preserves field
    /// names. The TS shim that calls `Commands.execute` with these
    /// shapes reads `availableTemplates` not `available_templates`;
    /// drift here would silently break the IPC contract.
    #[test]
    fn recipe_template_info_serde_camelcase() {
        let t = RecipeTemplateInfo {
            name: "research-loop".into(),
            description: "Iterative research with verification".into(),
            required_fields: vec!["topic".into(), "depth".into()],
        };
        let j = serde_json::to_string(&t).unwrap();
        assert!(j.contains("\"name\":\"research-loop\""));
        assert!(j.contains("\"requiredFields\":[\"topic\",\"depth\"]"));
        let back: RecipeTemplateInfo = serde_json::from_str(&j).unwrap();
        assert_eq!(back, t);
    }

    /// What this catches: hints are fully optional and serde accepts a
    /// JSON object missing every field. The TS shim sends `hints` only
    /// when the user passed `--category` or similar; the Rust side has
    /// to accept a missing `hints` field cleanly.
    #[test]
    fn recipe_generate_hints_all_optional() {
        let json = r#"{}"#;
        let h: RecipeGenerateHints = serde_json::from_str(json).unwrap();
        assert!(h.category.is_none());
        assert!(h.templates.is_none());
        assert!(h.tags.is_none());
        assert!(h.pattern.is_none());
    }

    /// What this catches: full RecipeGenerationRequest round-trips with
    /// hints + uniqueId override. Verifies the camelCase contract on
    /// every field the TS shim populates.
    #[test]
    fn recipe_generation_request_full_serde() {
        let req = RecipeGenerationRequest {
            description: "code review with tests".into(),
            available_templates: vec![RecipeTemplateInfo {
                name: "test-driven".into(),
                description: "TDD loop".into(),
                required_fields: vec!["target".into()],
            }],
            existing_recipe_ids: vec!["general-chat".into(), "academy-lesson".into()],
            hints: Some(RecipeGenerateHints {
                category: Some("dev".into()),
                templates: None,
                tags: Some(vec!["code".into(), "review".into()]),
                pattern: Some("collaborative".into()),
            }),
            unique_id_override: Some("code-review-tdd".into()),
        };
        let j = serde_json::to_string(&req).unwrap();
        assert!(j.contains("\"availableTemplates\":[{"));
        assert!(j.contains("\"existingRecipeIds\":[\"general-chat\""));
        assert!(j.contains("\"uniqueIdOverride\":\"code-review-tdd\""));
        let back: RecipeGenerationRequest = serde_json::from_str(&j).unwrap();
        assert_eq!(back, req);
    }

    /// What this catches: response shape ts-rs export. PR-3 shim awaits
    /// `Commands.execute<RecipeGenerationResponse>(...)` — the wire
    /// fields must stay `recipe` + `validationErrors` (camelCase).
    #[test]
    fn recipe_generation_response_serde_shape() {
        let resp = RecipeGenerationResponse {
            recipe: RecipeDefinitionShape::default(),
            validation_errors: vec![],
        };
        let j = serde_json::to_string(&resp).unwrap();
        assert!(j.contains("\"recipe\":{"));
        assert!(j.contains("\"validationErrors\":[]"));
        let back: RecipeGenerationResponse = serde_json::from_str(&j).unwrap();
        assert_eq!(back, resp);
    }

    /// What this catches: the lightweight RecipeDefinitionShape accepts
    /// the JSON the LLM is expected to emit. Defaults let unknown/missing
    /// fields parse without failing — the validator surfaces the gaps,
    /// not the deserializer.
    #[test]
    fn recipe_definition_shape_accepts_minimal_llm_output() {
        let json = r#"{
            "uniqueId": "code-review",
            "name": "Code Review",
            "displayName": "Review",
            "description": "Review code with TDD",
            "version": 1,
            "pipeline": [
                {"command": "rag/build", "params": {}},
                {"command": "ai/should-respond", "params": {}},
                {"command": "ai/generate", "params": {}}
            ],
            "ragTemplate": {"messageHistory": {"maxMessages": 30, "orderBy": "chronological", "includeTimestamps": true}},
            "strategy": {
                "conversationPattern": "collaborative",
                "responseRules": ["always cite the file:line"],
                "decisionCriteria": ["is the change tested?"]
            },
            "isPublic": true,
            "tags": ["code", "review"]
        }"#;
        let shape: RecipeDefinitionShape = serde_json::from_str(json).unwrap();
        assert_eq!(shape.unique_id, "code-review");
        assert_eq!(shape.version, Some(1));
        assert_eq!(shape.pipeline.len(), 3);
        assert_eq!(shape.is_public, Some(true));
    }
}
