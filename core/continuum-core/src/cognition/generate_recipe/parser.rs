//! Pure parser for the recipe-generator AI's response.
//!
//! Mirrors the TS parsing in `RecipeGenerateServerCommand.execute` (the
//! `jsonMatch = response.text.match(/\{[\s\S]*\}/)` + `JSON.parse(jsonMatch[0])`
//! sequence at lines 56–77). Same regex anchor, same JSON.parse semantics via
//! `serde_json::from_str`.
//!
//! Why a separate parser module: keeping it pure + testable means PR-2's IPC
//! handler can call `parse_recipe_from_ai_response(&response.text, ...)` without
//! itself depending on the LLM. Edge cases (no JSON, malformed JSON, JSON not
//! matching the shape) become unit tests instead of live-fixture-only tests.

use crate::cognition::generate_recipe::types::RecipeDefinitionShape;
use once_cell::sync::Lazy;
use regex::Regex;

/// Why this catches non-empty output: matches the first `{ ... }` envelope in
/// the response, including newlines. Mirrors TS `/\{[\s\S]*\}/` exactly. NOT
/// anchored — the AI may emit prose before/after the JSON despite the prompt
/// rule "Output ONLY the JSON object", so the matcher tolerates it.
static JSON_ENVELOPE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?s)\{.*\}").expect("static regex compiles"));

/// Typed parse failure. Carrier for the TS shim's `validationErrors` array
/// when surfaced through PR-2's IPC handler. Avoids the silent
/// `success: false, error: '...'` flat-string anti-pattern called out by #1262.
#[derive(Debug, Clone, PartialEq)]
pub enum ParseError {
    /// AI emitted no JSON envelope — the regex `\{ ... \}` matched nothing.
    /// Usually means the AI returned prose, refused, or emitted markdown
    /// fences without JSON inside.
    NoJsonEnvelope { raw_preview: String },
    /// AI emitted a JSON envelope but it didn't deserialize into the
    /// `RecipeDefinitionShape` even with serde defaults. Usually means the
    /// JSON was malformed (trailing commas, unterminated strings) or had
    /// type mismatches (string where array expected).
    MalformedJson {
        raw_preview: String,
        serde_error: String,
    },
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::NoJsonEnvelope { raw_preview } => write!(
                f,
                "LLM did not return valid JSON. Raw output: {raw_preview}"
            ),
            ParseError::MalformedJson {
                raw_preview,
                serde_error,
            } => write!(
                f,
                "LLM returned malformed JSON: {serde_error}. Raw JSON: {raw_preview}"
            ),
        }
    }
}

impl std::error::Error for ParseError {}

/// Cap on raw-output preview length stored in `ParseError` for diagnostics.
/// Mirrors TS `slice(0, 500)` on validationErrors.
const RAW_PREVIEW_MAX: usize = 500;

/// Parse the AI's freeform response into a `RecipeDefinitionShape`. Returns
/// the shape on success, typed `ParseError` on failure. Caller (PR-2's IPC
/// handler) decides whether to surface as JTAG validationErrors or as Err.
pub fn parse_recipe_from_ai_response(
    response_text: &str,
) -> Result<RecipeDefinitionShape, ParseError> {
    let preview = preview(response_text);

    let envelope = JSON_ENVELOPE_RE
        .find(response_text)
        .ok_or(ParseError::NoJsonEnvelope {
            raw_preview: preview.clone(),
        })?;

    serde_json::from_str::<RecipeDefinitionShape>(envelope.as_str()).map_err(|err| {
        ParseError::MalformedJson {
            raw_preview: preview_str(envelope.as_str()),
            serde_error: err.to_string(),
        }
    })
}

fn preview(s: &str) -> String {
    preview_str(s)
}

fn preview_str(s: &str) -> String {
    if s.len() <= RAW_PREVIEW_MAX {
        s.to_string()
    } else {
        // Truncate at char boundary to avoid panic on multi-byte chars.
        let mut idx = RAW_PREVIEW_MAX;
        while !s.is_char_boundary(idx) && idx > 0 {
            idx -= 1;
        }
        s[..idx].to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What this catches: well-formed JSON envelope parses into the shape
    /// with all top-level fields populated. Happy-path mirror of the TS
    /// JSON.parse success branch.
    #[test]
    fn parses_well_formed_recipe_envelope() {
        let response = r#"{
            "uniqueId": "novel-writing",
            "name": "Novel Writing",
            "displayName": "Writer",
            "description": "Iterative novel writing with critique loop",
            "version": 1,
            "pipeline": [
                {"command": "rag/build", "params": {}},
                {"command": "ai/should-respond", "params": {}},
                {"command": "ai/generate", "params": {}}
            ],
            "ragTemplate": {"messageHistory": {"maxMessages": 30, "orderBy": "chronological", "includeTimestamps": true}},
            "strategy": {"conversationPattern": "creative", "responseRules": ["be vivid"], "decisionCriteria": ["does it advance plot?"]},
            "isPublic": true,
            "tags": ["writing", "creative"]
        }"#;
        let shape = parse_recipe_from_ai_response(response).expect("happy path");
        assert_eq!(shape.unique_id, "novel-writing");
        assert_eq!(shape.name, "Novel Writing");
        assert_eq!(shape.version, Some(1));
        assert_eq!(shape.pipeline.len(), 3);
        assert_eq!(shape.tags, vec!["writing".to_string(), "creative".into()]);
    }

    /// What this catches: AI prepends prose ("Sure, here's the recipe:")
    /// before the JSON. The regex `\{ ... \}` finds the JSON anyway,
    /// matching TS behavior. Common failure mode of weaker models.
    #[test]
    fn extracts_json_envelope_from_prose_preamble() {
        let response = r#"Sure, here's the recipe you asked for:

{"uniqueId": "test", "name": "Test", "displayName": "T", "description": "test", "version": 1, "pipeline": [], "ragTemplate": {}, "strategy": {}, "isPublic": true, "tags": []}

Hope that helps!"#;
        let shape = parse_recipe_from_ai_response(response).expect("envelope extracted");
        assert_eq!(shape.unique_id, "test");
    }

    /// What this catches: AI wraps in markdown fences. The regex matches
    /// the inner `{...}` because `[\s\S]*` is greedy — same as TS
    /// `JSON.parse(jsonMatch[0])` which would extract the same envelope.
    #[test]
    fn extracts_json_envelope_from_markdown_fence() {
        let response = "```json\n{\"uniqueId\": \"fenced\", \"name\": \"F\", \"displayName\": \"F\", \"description\": \"d\", \"version\": 1, \"pipeline\": [], \"ragTemplate\": {}, \"strategy\": {}, \"isPublic\": true, \"tags\": []}\n```";
        let shape = parse_recipe_from_ai_response(response).expect("fence handled");
        assert_eq!(shape.unique_id, "fenced");
    }

    /// What this catches: AI returns prose with NO JSON object at all.
    /// The regex matches nothing → `NoJsonEnvelope` typed error. Caller
    /// can surface this as `validationErrors` without losing the original
    /// AI output for debugging.
    #[test]
    fn no_json_returns_typed_no_envelope_error() {
        let response =
            "I'm sorry, I cannot generate a recipe without more information about the activity.";
        let err = parse_recipe_from_ai_response(response).expect_err("no envelope");
        match err {
            ParseError::NoJsonEnvelope { raw_preview } => {
                assert!(raw_preview.contains("I'm sorry"));
            }
            other => panic!("expected NoJsonEnvelope, got {other:?}"),
        }
    }

    /// What this catches: AI emits a JSON-shaped envelope that's actually
    /// malformed (trailing comma, missing close brace inside, etc.). The
    /// envelope regex matches but serde fails. Typed `MalformedJson`
    /// carries the serde error so debuggers can see what choked.
    #[test]
    fn malformed_json_returns_typed_malformed_error() {
        // Trailing comma after the last field — invalid JSON.
        let response = r#"{"uniqueId": "x", "name": "X",}"#;
        let err = parse_recipe_from_ai_response(response).expect_err("malformed");
        match err {
            ParseError::MalformedJson { serde_error, .. } => {
                assert!(
                    !serde_error.is_empty(),
                    "serde_error should carry the underlying parse failure"
                );
            }
            other => panic!("expected MalformedJson, got {other:?}"),
        }
    }

    /// What this catches: extra unknown fields don't reject the parse.
    /// The TS path uses `JSON.parse` then casts — extra fields are
    /// silently kept. Rust serde with default `deny_unknown_fields` off
    /// (the default) matches that behavior. Forward-compat for future
    /// recipe schema additions.
    #[test]
    fn unknown_fields_dont_fail_parse() {
        let response = r#"{
            "uniqueId": "future",
            "name": "Future",
            "displayName": "F",
            "description": "has unknown fields",
            "version": 1,
            "pipeline": [],
            "ragTemplate": {},
            "strategy": {},
            "isPublic": true,
            "tags": [],
            "experimentalFeatureWeArentReadyFor": {"foo": "bar"}
        }"#;
        let shape = parse_recipe_from_ai_response(response).expect("forward-compat");
        assert_eq!(shape.unique_id, "future");
    }

    /// What this catches: missing optional fields (no `version`, no
    /// `isPublic`) parse to None / default. The validator surfaces the
    /// gaps; the parser tolerates them. Prevents the parser from
    /// short-circuiting on issues the validator should report with
    /// human-readable messages.
    #[test]
    fn missing_optional_fields_default_to_none_or_empty() {
        let response =
            r#"{"uniqueId": "minimal", "name": "M", "displayName": "M", "description": "min"}"#;
        let shape = parse_recipe_from_ai_response(response).expect("partial parses");
        assert_eq!(shape.unique_id, "minimal");
        assert_eq!(shape.version, None);
        assert_eq!(shape.is_public, None);
        assert!(shape.pipeline.is_empty());
    }

    /// What this catches: very long raw output gets truncated at the
    /// 500-char preview boundary. Without this, error logs balloon
    /// when the AI emits a 50KB JSON blob with one syntax error.
    /// Mirrors TS `slice(0, 500)`.
    #[test]
    fn raw_preview_caps_at_500_chars() {
        let big = "x".repeat(2000);
        let response = format!("{big} no json here");
        let err = parse_recipe_from_ai_response(&response).expect_err("no envelope");
        match err {
            ParseError::NoJsonEnvelope { raw_preview } => {
                assert!(
                    raw_preview.len() <= RAW_PREVIEW_MAX,
                    "preview should cap at {RAW_PREVIEW_MAX} chars, got {}",
                    raw_preview.len(),
                );
            }
            other => panic!("expected NoJsonEnvelope, got {other:?}"),
        }
    }
}
