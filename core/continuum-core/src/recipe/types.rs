//! Recipe wire types — the data a pipeline row deserializes into.
//!
//! Serde-TOLERANT by policy: unknown fields are ignored, every field beyond
//! `name`/`pipeline` (and `command` per step) defaults. Rows authored for a
//! future executor still load on an old one; capability grows in DATA first.

use serde::{Deserialize, Serialize};

/// One stored recipe — a named pipeline of command invocations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Recipe {
    /// The name `recipe/run --name` selects.
    pub name: String,
    /// One line of intent, shown when listing/erroring.
    #[serde(default)]
    pub description: String,
    /// The steps, walked in order. Empty = a legal no-op recipe.
    #[serde(default)]
    pub pipeline: Vec<RecipeStep>,
    /// Author-managed row version (data-layer convention).
    #[serde(default)]
    pub version: u32,
}

/// One pipeline step: a command invocation with interpolated params.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecipeStep {
    /// The command to dispatch — any discoverable command is legal
    /// (`commands/list`). THIS is the extension surface: new behavior is a
    /// new command or a new composition, never a new field here.
    pub command: String,
    /// Params for the command. String values may reference execution state:
    /// `"$name"` (whole bound value) or embedded `"${name.path}"` (rendered
    /// into the string). `$args.*` reads the caller's invocation params.
    #[serde(default)]
    pub params: serde_json::Value,
    /// Bind this step's JSON result into state under this name, readable by
    /// later steps' params/conditions.
    #[serde(default)]
    pub output_to: Option<String>,
    /// Skip-condition, evaluated against state BEFORE the step runs. Absent =
    /// always run. See [`crate::recipe::condition`] for the tiny grammar.
    #[serde(default)]
    pub condition: Option<String>,
    /// What a step error does to the run: `"fail"` (default — the run stops,
    /// loudly) or `"skip"` (the error is probed, the step binds nothing, the
    /// run continues). No silent third option, per
    /// [[fallbacks-are-illegal-fail-loud]].
    #[serde(default)]
    pub on_error: Option<String>,
    /// Retries before `on_error` applies (default 0 — a benchmarked command
    /// owns its own retry policy; this is for known-flaky externals).
    #[serde(default)]
    pub retry_count: u32,
    /// Per-attempt wall-clock bound. Absent = the command's own timeout
    /// discipline governs.
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recipe_rows_parse_tolerantly_with_unknown_fields() {
        // what this catches: a row authored for a NEWER executor (extra
        // fields, absent optionals) must load on this one — data-first growth.
        let row: Recipe = serde_json::from_value(serde_json::json!({
            "name": "x",
            "futureConcept": {"nested": true},
            "pipeline": [
                {"command": "data/list", "params": {"collection": "users"}, "outputTo": "rows"},
                {"command": "chat/send", "condition": "$rows.total != 0", "someFutureKnob": 3}
            ]
        }))
        .expect("tolerant parse");
        assert_eq!(row.pipeline.len(), 2);
        assert_eq!(row.pipeline[0].output_to.as_deref(), Some("rows"));
        assert_eq!(row.pipeline[1].condition.as_deref(), Some("$rows.total != 0"));
        assert_eq!(row.pipeline[1].retry_count, 0);
    }
}
