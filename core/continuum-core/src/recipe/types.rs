//! The pipeline STEP — the one unit of authored behaviour.
//!
//! There is no `Recipe` here any more (S2): the activity recipe
//! (`experience::ExperienceRecipe`) carries `pipeline: Vec<RecipeStep>`, and
//! `PipelineExecutor::run` takes the steps. One schema, one store, one executor.
//!
//! Serde-TOLERANT by policy: unknown fields are ignored, every field beyond
//! `command` defaults. A step authored for a future executor still loads on an
//! old one; capability grows in DATA first.

use serde::{Deserialize, Serialize};

/// One pipeline step: a command invocation with interpolated params.
///
/// Exported to TypeScript because an ACTIVITY recipe carries these too — the
/// pipeline is how an authored activity expresses behaviour, and the same step
/// shape serves both entry points (`recipe/run` and `activity/spawn`). One
/// schema, one executor; see docs/planning/RECIPE-CONVERGENCE-PLAN.md S0.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/experience/RecipeStep.ts"
)]
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
    #[ts(type = "unknown")]
    pub params: serde_json::Value,
    /// Bind this step's JSON result into state under this name, readable by
    /// later steps' params/conditions.
    #[serde(default)]
    #[ts(optional)]
    pub output_to: Option<String>,
    /// Skip-condition, evaluated against state BEFORE the step runs. Absent =
    /// always run. See [`crate::recipe::condition`] for the tiny grammar.
    #[serde(default)]
    #[ts(optional)]
    pub condition: Option<String>,
    /// What a step error does to the run. Typed: a misspelled policy is refused at
    /// load, never silently treated as the default. No third option, per
    /// [[fallbacks-are-illegal-fail-loud]].
    #[serde(default)]
    pub on_error: OnError,
    /// Retries before `on_error` applies (default 0 — a benchmarked command
    /// owns its own retry policy; this is for known-flaky externals).
    #[serde(default)]
    pub retry_count: u32,
    /// Per-attempt wall-clock bound. Absent = the command's own timeout
    /// discipline governs.
    #[serde(default)]
    #[ts(optional, type = "number")]
    pub timeout_ms: Option<u64>,
    /// Who must say yes before this step runs. `"human"` = the run HOLDS here: the
    /// step is not dispatched, the receipt names it (`held_at`), and nothing after
    /// it runs. Absent = the step runs unattended. This is the approval boundary
    /// for irreversible outward actions (submitting an application, sending mail)
    /// — a property of the STEP, authored in data, never a policy hidden elsewhere.
    #[serde(default)]
    #[ts(optional)]
    pub approval: Option<Approval>,
    /// Fan out: a `$binding` (or `${path}`) that resolves to an ARRAY; the step runs
    /// once per element with `$item` (and `$index`) bound, and `outputTo` binds the
    /// array of per-element results in order. A step with no `each` runs once.
    /// This is how one `work/create` step posts a whole imported suite.
    #[serde(default)]
    #[ts(optional)]
    pub each: Option<String>,
}

/// What a failed step does to the run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/experience/OnError.ts")]
#[serde(rename_all = "snake_case")]
pub enum OnError {
    /// The run stops at the step, loudly, naming it. The default.
    #[default]
    Fail,
    /// The error is probed, the step binds nothing, the run continues.
    Skip,
}

impl std::fmt::Display for OnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            OnError::Fail => "fail",
            OnError::Skip => "skip",
        })
    }
}

/// Who must say yes before a step runs. Only humans today; the enum is the
/// extension point (a named role, a quorum) so a future value is a variant with
/// meaning, never a string the executor has to guess at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/experience/Approval.ts")]
#[serde(rename_all = "snake_case")]
pub enum Approval {
    /// A human presses the button. The run HOLDS here.
    Human,
}

impl std::fmt::Display for Approval {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Approval::Human => "human",
        })
    }
}
