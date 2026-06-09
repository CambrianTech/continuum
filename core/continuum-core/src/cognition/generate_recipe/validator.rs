//! Pure structural validator for parsed `RecipeDefinitionShape`.
//!
//! Mirrors the TS `validateRecipe()` checks in `RecipeGenerateServerCommand.ts`
//! lines 253–349, with one deliberate split:
//!
//! - **Structural validation lives here** — uniqueId format, required fields,
//!   pipeline shape, RAG template shape, strategy enum + arrays, role schema,
//!   in-request duplicate check via the `existing_recipe_ids` carrier.
//! - **Filesystem collision check stays TS-side** — `RecipeLoader.getInstance()
//!   .getAllRecipes().some(r => r.uniqueId === recipe.uniqueId)` is pure FS
//!   state. The TS shim (PR-3) does that check after Rust returns.
//! - **Sentinel-template existence check stays TS-side** — `TemplateRegistry.has(tmpl)`
//!   reads runtime registry state. PR-1's validator can't see the registry; the
//!   carrier just lists what the AI emitted as `sentinelTemplates`. PR-3 shim
//!   verifies each name is registered.
//!
//! Why split this way: keeps the validator a pure function (input shape +
//! existing IDs → list of errors) so it's trivially testable and identical
//! across runs. The bits that depend on filesystem/registry state are clearly
//! marked as TS-shim concerns.

use crate::cognition::generate_recipe::types::RecipeDefinitionShape;
use once_cell::sync::Lazy;
use regex::Regex;

/// Mirror of the TS regex `/^[a-z0-9-]+$/` for uniqueId format.
static KEBAB_CASE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[a-z0-9-]+$").expect("static regex compiles"));

/// Valid `conversationPattern` values from `RecipeStrategy`. Mirrors TS array
/// at line 297 exactly. Drift here = false-positive validation rejections of
/// recipes the TS path would accept.
const VALID_CONVERSATION_PATTERNS: &[&str] = &[
    "human-focused",
    "collaborative",
    "competitive",
    "teaching",
    "exploring",
    "cooperative",
];

/// Valid `RecipeRoleType` values. Mirrors TS array at line 320.
const VALID_ROLE_TYPES: &[&str] = &["organizational", "perceptual", "creative"];

/// One structural validation error, attached to a field path. The TS path
/// returns these as plain `string[]`; this Rust enum keeps the variants
/// typed so PR-3 shim can decide rendering (could surface as JTAG strings
/// for backwards-compat or as structured for richer UIs).
#[derive(Debug, Clone, PartialEq)]
pub enum ValidationError {
    Missing(&'static str),
    InvalidFormat {
        field: &'static str,
        value: String,
        expected: &'static str,
    },
    InvalidEnumValue {
        field: &'static str,
        value: String,
        allowed: &'static [&'static str],
    },
    PipelineEmpty,
    PipelineStepMissingField {
        index: usize,
        field: &'static str,
    },
    DuplicateUniqueId(String),
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationError::Missing(field) => write!(f, "Missing {field}"),
            ValidationError::InvalidFormat { field, value, expected } => {
                write!(f, "{field} must be {expected}: \"{value}\"")
            }
            ValidationError::InvalidEnumValue { field, value, allowed } => write!(
                f,
                "Invalid {field}: \"{value}\". Must be one of: {}",
                allowed.join(", ")
            ),
            ValidationError::PipelineEmpty => write!(f, "Pipeline must have at least one step"),
            ValidationError::PipelineStepMissingField { index, field } => {
                write!(f, "Pipeline step {index}: missing {field}")
            }
            ValidationError::DuplicateUniqueId(id) => write!(
                f,
                "Recipe with uniqueId \"{id}\" already exists. Use a different uniqueId or specify --uniqueId."
            ),
        }
    }
}

/// Run structural validation. Returns `Vec<String>` (TS-compatible flat
/// strings) so PR-2's IPC handler can drop them straight into the
/// `validationErrors` field of the response. Future PR could surface
/// `Vec<ValidationError>` instead for typed UIs.
///
/// Caller responsibility: gather `existing_recipe_ids` from the host's
/// recipe loader and pass them in. Validator does NOT touch the
/// filesystem; caller does that.
pub fn validate_recipe_structure(
    recipe: &RecipeDefinitionShape,
    existing_recipe_ids: &[String],
) -> Vec<String> {
    let mut errors: Vec<ValidationError> = Vec::new();

    // ── Required top-level fields ──────────────────────────────────
    if recipe.unique_id.trim().is_empty() {
        errors.push(ValidationError::Missing("uniqueId"));
    }
    if recipe.name.trim().is_empty() {
        errors.push(ValidationError::Missing("name"));
    }
    if recipe.display_name.trim().is_empty() {
        errors.push(ValidationError::Missing("displayName"));
    }
    if recipe.description.trim().is_empty() {
        errors.push(ValidationError::Missing("description"));
    }
    if recipe.version.is_none() {
        errors.push(ValidationError::Missing("version"));
    }

    // ── uniqueId format ────────────────────────────────────────────
    if !recipe.unique_id.is_empty() && !KEBAB_CASE_RE.is_match(&recipe.unique_id) {
        errors.push(ValidationError::InvalidFormat {
            field: "uniqueId",
            value: recipe.unique_id.clone(),
            expected: "kebab-case",
        });
    }

    // ── Pipeline shape ─────────────────────────────────────────────
    if recipe.pipeline.is_empty() {
        errors.push(ValidationError::PipelineEmpty);
    } else {
        for (idx, step) in recipe.pipeline.iter().enumerate() {
            let has_command = step
                .get("command")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .is_some();
            if !has_command {
                errors.push(ValidationError::PipelineStepMissingField {
                    index: idx,
                    field: "command",
                });
            }
            let has_params_object = step.get("params").map(|v| v.is_object()).unwrap_or(false);
            if !has_params_object {
                errors.push(ValidationError::PipelineStepMissingField {
                    index: idx,
                    field: "params",
                });
            }
        }
    }

    // ── RAG template shape ─────────────────────────────────────────
    if recipe.rag_template.is_null() || !recipe.rag_template.is_object() {
        errors.push(ValidationError::Missing("ragTemplate"));
    } else if recipe
        .rag_template
        .get("messageHistory")
        .filter(|v| v.is_object())
        .is_none()
    {
        errors.push(ValidationError::Missing("ragTemplate.messageHistory"));
    }

    // ── Strategy shape + enum + required arrays ────────────────────
    if recipe.strategy.is_null() || !recipe.strategy.is_object() {
        errors.push(ValidationError::Missing("strategy"));
    } else {
        let pattern = recipe
            .strategy
            .get("conversationPattern")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if pattern.is_empty() {
            errors.push(ValidationError::Missing("strategy.conversationPattern"));
        } else if !VALID_CONVERSATION_PATTERNS.contains(&pattern) {
            errors.push(ValidationError::InvalidEnumValue {
                field: "conversationPattern",
                value: pattern.to_string(),
                allowed: VALID_CONVERSATION_PATTERNS,
            });
        }

        if !recipe
            .strategy
            .get("responseRules")
            .map(|v| v.is_array())
            .unwrap_or(false)
        {
            errors.push(ValidationError::Missing("strategy.responseRules array"));
        }
        if !recipe
            .strategy
            .get("decisionCriteria")
            .map(|v| v.is_array())
            .unwrap_or(false)
        {
            errors.push(ValidationError::Missing("strategy.decisionCriteria array"));
        }
    }

    // ── Roles (when present) — type + requires shape ───────────────
    for (idx, role) in recipe.roles.iter().enumerate() {
        let role_name = role
            .get("role")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty());
        if role_name.is_none() {
            errors.push(ValidationError::PipelineStepMissingField {
                index: idx,
                field: "role.role",
            });
        }

        let role_type = role.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if role_type.is_empty() {
            errors.push(ValidationError::Missing("role.type"));
        } else if !VALID_ROLE_TYPES.contains(&role_type) {
            errors.push(ValidationError::InvalidEnumValue {
                field: "role.type",
                value: role_type.to_string(),
                allowed: VALID_ROLE_TYPES,
            });
        }

        let requires_ok = role
            .get("requires")
            .and_then(|v| v.as_array())
            .map(|arr| !arr.is_empty())
            .unwrap_or(false);
        if !requires_ok {
            errors.push(ValidationError::Missing(
                "role.requires (must be non-empty array)",
            ));
        }
    }

    // ── Top-level isPublic + tags ──────────────────────────────────
    if recipe.is_public.is_none() {
        errors.push(ValidationError::Missing("isPublic (must be boolean)"));
    }
    // Recipe without tags is allowed-but-warned in the TS path; mirror by not
    // adding an error here. The `validateRecipe` TS check at line 338 is
    // `if (!recipe.tags || !Array.isArray(recipe.tags))` — it errors only when
    // MISSING, not when empty. The serde default gives us [], which is
    // "missing → empty"; we accept it. Catching tag-emptiness would be a
    // stricter policy worth a separate card.

    // ── In-request duplicate check (replaces FS collision check) ───
    // The filesystem collision check stays TS-side (RecipeLoader.getInstance().
    // getAllRecipes()), but the in-request check using the carrier list runs
    // here so the AI can be told "that ID is taken" without an extra IPC trip.
    if !recipe.unique_id.is_empty() && existing_recipe_ids.iter().any(|id| id == &recipe.unique_id)
    {
        errors.push(ValidationError::DuplicateUniqueId(recipe.unique_id.clone()));
    }

    errors.into_iter().map(|e| e.to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn valid_minimal_recipe() -> RecipeDefinitionShape {
        RecipeDefinitionShape {
            unique_id: "valid-test".into(),
            name: "Valid Test".into(),
            display_name: "Valid".into(),
            description: "A valid test recipe".into(),
            version: Some(1),
            pipeline: vec![
                json!({"command": "rag/build", "params": {}}),
                json!({"command": "ai/should-respond", "params": {}}),
                json!({"command": "ai/generate", "params": {}}),
            ],
            rag_template: json!({"messageHistory": {"maxMessages": 30, "orderBy": "chronological", "includeTimestamps": true}}),
            strategy: json!({
                "conversationPattern": "collaborative",
                "responseRules": ["be concise"],
                "decisionCriteria": ["is the question clear?"]
            }),
            roles: vec![],
            sentinel_templates: vec![],
            is_public: Some(true),
            tags: vec!["test".into()],
        }
    }

    /// What this catches: a complete, well-formed recipe passes with zero
    /// errors. Happy-path baseline — if this ever regresses, every other
    /// test is suspect.
    #[test]
    fn happy_path_well_formed_recipe_validates_clean() {
        let recipe = valid_minimal_recipe();
        let errors = validate_recipe_structure(&recipe, &[]);
        assert!(errors.is_empty(), "expected no errors, got: {errors:?}");
    }

    /// What this catches: missing top-level required fields are surfaced
    /// individually. The TS path errors on each missing field separately
    /// — so debuggers see all gaps in one report rather than one-at-a-time
    /// fix loops.
    #[test]
    fn missing_required_fields_each_reported() {
        let recipe = RecipeDefinitionShape::default();
        let errors = validate_recipe_structure(&recipe, &[]);
        assert!(errors.iter().any(|e| e.contains("Missing uniqueId")));
        assert!(errors.iter().any(|e| e.contains("Missing name")));
        assert!(errors.iter().any(|e| e.contains("Missing displayName")));
        assert!(errors.iter().any(|e| e.contains("Missing description")));
        assert!(errors.iter().any(|e| e.contains("Missing version")));
    }

    /// What this catches: uniqueId with uppercase / underscores / spaces
    /// fails the kebab-case regex. The publish-side disk path uses
    /// uniqueId as the filename; non-kebab IDs corrupt cross-platform
    /// filesystem behavior.
    #[test]
    fn unique_id_must_be_kebab_case() {
        let mut recipe = valid_minimal_recipe();
        recipe.unique_id = "Bad_Format ID".into();
        let errors = validate_recipe_structure(&recipe, &[]);
        assert!(
            errors.iter().any(|e| e.contains("kebab-case")),
            "got: {errors:?}"
        );
    }

    /// What this catches: empty pipeline gets the dedicated PipelineEmpty
    /// error (not just missing). Recipes need at least one step to do
    /// anything; emptiness is a definitional bug.
    #[test]
    fn empty_pipeline_errors() {
        let mut recipe = valid_minimal_recipe();
        recipe.pipeline = vec![];
        let errors = validate_recipe_structure(&recipe, &[]);
        assert!(
            errors
                .iter()
                .any(|e| e.contains("Pipeline must have at least one step")),
            "got: {errors:?}"
        );
    }

    /// What this catches: pipeline step missing `command` AND missing
    /// `params` both surface, with index. Catches the AI emitting
    /// half-formed steps that the runtime would silently no-op on.
    #[test]
    fn pipeline_step_missing_fields_surface_with_index() {
        let mut recipe = valid_minimal_recipe();
        recipe.pipeline = vec![
            json!({"command": "rag/build", "params": {}}),
            json!({}),                         // step 1 has neither command nor params
            json!({"command": "ai/generate"}), // step 2 has command but no params
        ];
        let errors = validate_recipe_structure(&recipe, &[]);
        assert!(errors
            .iter()
            .any(|e| e.contains("Pipeline step 1: missing command")));
        assert!(errors
            .iter()
            .any(|e| e.contains("Pipeline step 1: missing params")));
        assert!(errors
            .iter()
            .any(|e| e.contains("Pipeline step 2: missing params")));
    }

    /// What this catches: `conversationPattern` set to a value not in the
    /// 6-element enum. The error lists the valid options so the AI's
    /// next attempt has the actionable info.
    #[test]
    fn invalid_conversation_pattern_lists_allowed_values() {
        let mut recipe = valid_minimal_recipe();
        recipe.strategy = json!({
            "conversationPattern": "freestyle",
            "responseRules": [],
            "decisionCriteria": []
        });
        let errors = validate_recipe_structure(&recipe, &[]);
        let msg = errors
            .iter()
            .find(|e| e.contains("conversationPattern"))
            .unwrap_or_else(|| panic!("expected conversationPattern error, got: {errors:?}"));
        assert!(msg.contains("freestyle"));
        assert!(msg.contains("human-focused"));
        assert!(msg.contains("cooperative"));
    }

    /// What this catches: missing strategy.responseRules / decisionCriteria
    /// arrays are reported individually. The TS path checks both
    /// independently — so a recipe missing only one gets a precise gap
    /// report rather than a vague "strategy malformed".
    #[test]
    fn missing_strategy_arrays_each_reported() {
        let mut recipe = valid_minimal_recipe();
        recipe.strategy = json!({"conversationPattern": "collaborative"});
        let errors = validate_recipe_structure(&recipe, &[]);
        assert!(errors.iter().any(|e| e.contains("responseRules array")));
        assert!(errors.iter().any(|e| e.contains("decisionCriteria array")));
    }

    /// What this catches: ragTemplate present but missing messageHistory.
    /// Mirrors TS check at line 286.
    #[test]
    fn rag_template_must_have_message_history() {
        let mut recipe = valid_minimal_recipe();
        recipe.rag_template = json!({"someOtherField": "value"});
        let errors = validate_recipe_structure(&recipe, &[]);
        assert!(errors
            .iter()
            .any(|e| e.contains("ragTemplate.messageHistory")));
    }

    /// What this catches: roles array with invalid type / missing
    /// requires. Roles are how the system matches models to recipes —
    /// drift here means the role assembler can't satisfy the recipe.
    #[test]
    fn role_validation_catches_invalid_type_and_empty_requires() {
        let mut recipe = valid_minimal_recipe();
        recipe.roles = vec![
            json!({"role": "implementer", "type": "wizard", "requires": ["coding"]}),
            json!({"role": "writer", "type": "creative", "requires": []}),
        ];
        let errors = validate_recipe_structure(&recipe, &[]);
        assert!(errors
            .iter()
            .any(|e| e.contains("Invalid role.type") && e.contains("wizard")));
        assert!(errors
            .iter()
            .any(|e| e.contains("role.requires (must be non-empty array)")));
    }

    /// What this catches: in-request uniqueId collision is detected even
    /// before the FS check happens. The TS shim does the FS check after
    /// Rust returns; this catches dupes the AI proposes against the
    /// host's already-loaded recipes carried in `existing_recipe_ids`.
    #[test]
    fn in_request_duplicate_unique_id_errors() {
        let recipe = valid_minimal_recipe();
        let existing = vec!["valid-test".to_string(), "general-chat".into()];
        let errors = validate_recipe_structure(&recipe, &existing);
        let msg = errors
            .iter()
            .find(|e| e.contains("already exists"))
            .unwrap_or_else(|| panic!("expected duplicate error, got: {errors:?}"));
        assert!(msg.contains("valid-test"));
    }

    /// What this catches: empty `existing_recipe_ids` carrier doesn't
    /// false-positive on the duplicate check. Common case (fresh install,
    /// no recipes loaded yet).
    #[test]
    fn empty_existing_ids_no_duplicate_false_positive() {
        let recipe = valid_minimal_recipe();
        let errors = validate_recipe_structure(&recipe, &[]);
        assert!(
            !errors.iter().any(|e| e.contains("already exists")),
            "got: {errors:?}"
        );
    }

    /// What this catches: missing isPublic surfaces the typed gap. Future
    /// recipes that set `isPublic: false` should validate; only the
    /// undefined case errors.
    #[test]
    fn missing_is_public_errors_but_false_is_accepted() {
        let mut recipe = valid_minimal_recipe();
        recipe.is_public = None;
        let errors = validate_recipe_structure(&recipe, &[]);
        assert!(errors.iter().any(|e| e.contains("isPublic")));

        recipe.is_public = Some(false);
        let errors = validate_recipe_structure(&recipe, &[]);
        assert!(
            !errors.iter().any(|e| e.contains("isPublic")),
            "isPublic: false should be accepted, got: {errors:?}"
        );
    }
}
