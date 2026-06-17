//! Recipe Pipeline Walker — Rust-native executor for recipe-data pipelines.
//!
//! Implements the executor designed in `docs/architecture/RECIPE-EXECUTION-RUNTIME.md`.
//! Recipes are data: a `pipeline[]` array of typed step descriptors. The walker
//! iterates the pipeline in order, dispatches each step's command through the
//! shared [`CommandExecutor`], threads results between steps via a variables
//! map, and emits a full execution trace.
//!
//! ## Slice 1 scope (what this lands)
//!
//! - Step iteration in declared order
//! - Param interpolation for top-level string values: `{var.path}` references
//!   in step `params` are substituted from prior steps' outputs (the variables
//!   map keyed by each step's `outputTo`)
//! - Command dispatch via `Arc<CommandExecutor>` — same Rust kernel commands
//!   live code uses
//! - Full execution trace per [`RecipeExecutionStep`] — input + output + duration —
//!   captured into a pluggable [`RecipeTraceSink`] (Noop default, zero hot-path
//!   cost, recording sink for curriculum harvest per RECIPE-EMBEDDED-LEARNING.md)
//! - `onError` semantics: `Fail` (default) aborts; `Skip` records the error and
//!   continues; `Retry` is reserved (currently behaves as `Fail` with a TODO)
//!
//! ## Out-of-scope follow-ups (intentionally — slice scope discipline)
//!
//! - `condition` expression evaluation — first cut just dispatches every step;
//!   condition gating is a follow-up slice with a small typed expression parser
//!   (NOT a Rust hand-rolled JS engine; aim for a tiny safe subset that fits
//!   the existing recipe corpus)
//! - Param interpolation inside nested objects/arrays — first cut walks the
//!   top-level params only; nested walk is a mechanical follow-up
//! - PRG.ts cutover — this is the executor; the chat surface routing through it
//!   is a separate slice (the doctrine-heavy one Joel wanted to design-review)
//!
//! ## Design discipline
//!
//! Per `RECIPE-EXECUTION-RUNTIME.md` §3 the executor is kernel-level. Commands
//! stay dumb; the executor is a small loop with no clever behaviour. M5's
//! `ai/should-respond` ServiceModule (registered against the same
//! [`CommandExecutor`]) is ONE step the walker dispatches — the walker has no
//! knowledge of which steps are AI vs IO vs cognition. The dispatch table is
//! open by design (per `[[commands-are-kernel-level-and-compose]]`).

use crate::runtime::CommandExecutor;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::sync::Arc;
use std::time::Instant;
use ts_rs::TS;
use uuid::Uuid;

// ─── Step + Pipeline types ───────────────────────────────────────────────

/// What to do when a step errors. Per RECIPE-EXECUTION-RUNTIME.md
/// §4 every step's outcome is captured regardless of policy — onError
/// only controls control flow, never observability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/recipe/RecipeOnError.ts"
)]
#[serde(rename_all = "kebab-case")]
pub enum RecipeOnError {
    /// Abort the pipeline (default).
    Fail,
    /// Record the failure in the trace, leave its output binding unset,
    /// continue to the next step.
    Skip,
    /// Reserved. Currently behaves as `Fail` with a TODO.
    Retry,
}

impl Default for RecipeOnError {
    fn default() -> Self {
        RecipeOnError::Fail
    }
}

/// One step in a recipe pipeline. Mirrors the TS `RecipeStep` interface in
/// `src/system/recipes/shared/RecipeTypes.ts` — same JSON shape, so the same
/// recipe definitions round-trip through both Rust and TS executors.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/recipe/RecipeStep.ts")]
#[serde(rename_all = "camelCase")]
pub struct RecipeStep {
    /// Command URI to dispatch (e.g., `ai/should-respond`, `ai/generate`).
    pub command: String,
    /// Step parameters. String values matching the `{var.path}` pattern
    /// are interpolated from the variables map at dispatch time.
    #[ts(type = "Record<string, unknown>")]
    pub params: Value,
    /// Variable name to bind the step result to in the execution context.
    /// Subsequent steps reference it as `{outputTo.field}` in their params.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_to: Option<String>,
    /// Reserved condition expression. Currently parsed-but-ignored — the
    /// walker dispatches every step regardless. A follow-up slice adds a
    /// small typed expression evaluator over the variables map.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    /// What to do when this step errors. Defaults to `Fail`.
    #[serde(default)]
    pub on_error: RecipeOnError,
}

// ─── Execution trace ─────────────────────────────────────────────────────

/// One step's execution record — input, outcome, duration.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/recipe/RecipeExecutionStep.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct RecipeExecutionStep {
    pub step_index: usize,
    pub command: String,
    /// The params after `{var.path}` interpolation — i.e., what the
    /// command actually received. Captured for curriculum / replay.
    #[ts(type = "Record<string, unknown>")]
    pub resolved_params: Value,
    /// Result JSON (if success), error message (if failure).
    #[ts(type = "unknown")]
    pub result: Option<Value>,
    pub error: Option<String>,
    pub duration_ms: u64,
}

/// Full execution record — the pipeline's input, every step's outcome, and
/// the final variables map. Curriculum-harvestable per the
/// "every recipe execution generates LoRA training data" pattern in
/// RECIPE-EMBEDDED-LEARNING.md.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/recipe/RecipeExecutionTrace.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct RecipeExecutionTrace {
    /// Unique id for this execution (correlation in logs + persistence).
    #[ts(type = "string")]
    pub execution_id: Uuid,
    /// Initial variables provided by the caller.
    #[ts(type = "Record<string, unknown>")]
    pub initial_variables: Value,
    /// Final variables map after the last step.
    #[ts(type = "Record<string, unknown>")]
    pub final_variables: Value,
    /// Every step's trace, in declaration order.
    pub steps: Vec<RecipeExecutionStep>,
    /// True if every step succeeded (or skipped per onError=Skip without
    /// the pipeline aborting). False if any Fail step errored.
    pub success: bool,
    /// Total wall time across all steps.
    pub total_duration_ms: u64,
}

/// Trace sink — pluggable per the OBSERVABILITY-AS-SUBSTRATE pattern.
/// Default [`NoopRecipeTraceSink`] drops; recording sinks persist for
/// curriculum harvest or replay.
pub trait RecipeTraceSink: Send + Sync {
    fn record(&self, trace: &RecipeExecutionTrace);
}

/// Zero-cost default — drops traces on the floor. Same shape as
/// `NoopWorkspaceCaptureSink` in `cognition::workspace`.
pub struct NoopRecipeTraceSink;
impl RecipeTraceSink for NoopRecipeTraceSink {
    fn record(&self, _trace: &RecipeExecutionTrace) {}
}

// ─── Walker errors ───────────────────────────────────────────────────────

/// Errors that abort the walker (as distinct from per-step errors which
/// are captured in the trace).
#[derive(Debug, Clone, thiserror::Error)]
pub enum RecipeWalkerError {
    /// A step's command failed and its `onError` policy is `Fail`.
    #[error("step {step_index} ({command}) failed: {message}")]
    StepFailed {
        step_index: usize,
        command: String,
        message: String,
    },
    /// A `{var.path}` reference couldn't be resolved against the
    /// variables map.
    #[error("step {step_index} ({command}) param interpolation failed: {message}")]
    InterpolationFailed {
        step_index: usize,
        command: String,
        message: String,
    },
}

// ─── The walker ──────────────────────────────────────────────────────────

/// Recipe pipeline executor. Holds an [`Arc<CommandExecutor>`] to dispatch
/// step commands and a pluggable [`RecipeTraceSink`] for capture.
///
/// One walker can be shared across many concurrent walks (it owns no
/// per-execution state — every walk allocates a fresh execution context).
pub struct RecipeWalker {
    executor: Arc<CommandExecutor>,
    trace_sink: Arc<dyn RecipeTraceSink>,
}

impl RecipeWalker {
    /// New walker with the Noop trace sink. Swap in a recording sink with
    /// [`Self::with_trace_sink`] for curriculum capture.
    pub fn new(executor: Arc<CommandExecutor>) -> Self {
        Self {
            executor,
            trace_sink: Arc::new(NoopRecipeTraceSink),
        }
    }

    /// Install a trace sink. Returns `self` for builder-style chaining.
    pub fn with_trace_sink(mut self, sink: Arc<dyn RecipeTraceSink>) -> Self {
        self.trace_sink = sink;
        self
    }

    /// Walk a pipeline. `initial_variables` seeds the variables map (the
    /// caller's inputs — Signal, PersonaContext, room metadata, whatever
    /// the recipe references). Returns the execution trace on success
    /// (which may include skipped/failed steps if their policy permitted
    /// continuation) or a [`RecipeWalkerError`] on hard abort.
    pub async fn walk(
        &self,
        steps: &[RecipeStep],
        initial_variables: Value,
    ) -> Result<RecipeExecutionTrace, RecipeWalkerError> {
        let pipeline_start = Instant::now();
        let execution_id = Uuid::new_v4();
        let mut variables = match initial_variables.clone() {
            Value::Object(m) => m,
            // Caller passed a non-object seed; wrap so future bindings still
            // work — record the original under "input".
            other => {
                let mut m = Map::new();
                m.insert("input".to_string(), other);
                m
            }
        };

        let mut step_traces: Vec<RecipeExecutionStep> = Vec::with_capacity(steps.len());
        let mut all_success = true;

        for (step_index, step) in steps.iter().enumerate() {
            let resolved_params =
                interpolate_params(&step.params, &variables).map_err(|message| {
                    RecipeWalkerError::InterpolationFailed {
                        step_index,
                        command: step.command.clone(),
                        message,
                    }
                })?;

            let step_start = Instant::now();
            let outcome = self
                .executor
                .execute(step.command.as_str(), resolved_params.clone())
                .await;
            let duration_ms = step_start.elapsed().as_millis() as u64;

            match outcome {
                Ok(result) => match command_result_to_json(result) {
                    Ok(result_json) => {
                        if let Some(name) = step.output_to.as_deref() {
                            variables.insert(name.to_string(), result_json.clone());
                        }
                        step_traces.push(RecipeExecutionStep {
                            step_index,
                            command: step.command.clone(),
                            resolved_params,
                            result: Some(result_json),
                            error: None,
                            duration_ms,
                        });
                    }
                    Err(projection_err) => {
                        // Stream/Lambda not yet wired — treat as step failure
                        // so the recipe author sees the contract bug.
                        all_success = false;
                        step_traces.push(RecipeExecutionStep {
                            step_index,
                            command: step.command.clone(),
                            resolved_params,
                            result: None,
                            error: Some(projection_err.clone()),
                            duration_ms,
                        });
                        match step.on_error {
                            RecipeOnError::Fail | RecipeOnError::Retry => {
                                let trace = RecipeExecutionTrace {
                                    execution_id,
                                    initial_variables,
                                    final_variables: Value::Object(variables),
                                    steps: step_traces,
                                    success: false,
                                    total_duration_ms: pipeline_start.elapsed().as_millis()
                                        as u64,
                                };
                                self.trace_sink.record(&trace);
                                return Err(RecipeWalkerError::StepFailed {
                                    step_index,
                                    command: step.command.clone(),
                                    message: projection_err,
                                });
                            }
                            RecipeOnError::Skip => {}
                        }
                    }
                },
                Err(message) => {
                    all_success = false;
                    step_traces.push(RecipeExecutionStep {
                        step_index,
                        command: step.command.clone(),
                        resolved_params,
                        result: None,
                        error: Some(message.clone()),
                        duration_ms,
                    });
                    match step.on_error {
                        RecipeOnError::Fail | RecipeOnError::Retry => {
                            // Retry is reserved; for slice 1 it falls through
                            // to Fail (the executor never silently re-runs).
                            let trace = RecipeExecutionTrace {
                                execution_id,
                                initial_variables,
                                final_variables: Value::Object(variables),
                                steps: step_traces,
                                success: false,
                                total_duration_ms: pipeline_start.elapsed().as_millis() as u64,
                            };
                            self.trace_sink.record(&trace);
                            return Err(RecipeWalkerError::StepFailed {
                                step_index,
                                command: step.command.clone(),
                                message,
                            });
                        }
                        RecipeOnError::Skip => {
                            // Continue without binding outputTo — subsequent
                            // steps that reference {this_step.field} will
                            // get an interpolation error if they try.
                        }
                    }
                }
            }
        }

        let trace = RecipeExecutionTrace {
            execution_id,
            initial_variables,
            final_variables: Value::Object(variables),
            steps: step_traces,
            success: all_success,
            total_duration_ms: pipeline_start.elapsed().as_millis() as u64,
        };
        self.trace_sink.record(&trace);
        Ok(trace)
    }
}

// ─── Param interpolation ────────────────────────────────────────────────

/// Substitute `{var.path}` references in string param values with values
/// from the variables map. Returns the resolved params or an error message
/// describing the failed reference.
///
/// Slice 1 walks only top-level string values. Nested objects/arrays are
/// passed through verbatim — a follow-up slice walks them recursively
/// (mechanical; staged separately to keep this slice small).
fn interpolate_params(params: &Value, variables: &Map<String, Value>) -> Result<Value, String> {
    match params {
        Value::Object(map) => {
            let mut resolved = Map::with_capacity(map.len());
            for (k, v) in map {
                let new_v = match v {
                    Value::String(s) => resolve_string_ref(s, variables)?,
                    other => other.clone(),
                };
                resolved.insert(k.clone(), new_v);
            }
            Ok(Value::Object(resolved))
        }
        // Walker only dispatches commands with object params today; other
        // top-level shapes pass through.
        other => Ok(other.clone()),
    }
}

/// If `s` is exactly `{var.path}`, look up the path in `variables` and
/// return the resolved value. Otherwise return `s` as a JSON string
/// unchanged.
fn resolve_string_ref(s: &str, variables: &Map<String, Value>) -> Result<Value, String> {
    if !s.starts_with('{') || !s.ends_with('}') {
        return Ok(Value::String(s.to_string()));
    }
    let path = &s[1..s.len() - 1];
    let segments: Vec<&str> = path.split('.').collect();
    let mut current: &Value = match variables.get(segments[0]) {
        Some(v) => v,
        None => {
            return Err(format!(
                "unknown variable `{}` in reference `{}`",
                segments[0], s
            ));
        }
    };
    for seg in &segments[1..] {
        current = match current {
            Value::Object(m) => m
                .get(*seg)
                .ok_or_else(|| format!("path `{}` not found in reference `{}`", seg, s))?,
            _ => {
                return Err(format!(
                    "cannot descend into non-object at `{}` in reference `{}`",
                    seg, s
                ));
            }
        };
    }
    Ok(current.clone())
}

/// Convert a [`crate::runtime::CommandResult`] to a JSON value for trace
/// capture + downstream-step param interpolation.
///
/// Returns `Ok(value)` for Json/Binary/Handle, `Err(msg)` for the not-yet-
/// wired Stream/Lambda cell shapes. The walker treats the err case as a
/// step failure (the step "succeeded" producing an un-projectable cell,
/// which is a contract bug the recipe author needs to see).
fn command_result_to_json(result: crate::runtime::CommandResult) -> Result<Value, String> {
    result.to_json_value()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interpolate_passes_through_literal_strings() {
        let mut vars = Map::new();
        vars.insert("foo".into(), Value::String("bar".into()));
        let params = serde_json::json!({"text": "hello world", "n": 5});
        let resolved = interpolate_params(&params, &vars).expect("resolve");
        assert_eq!(resolved["text"], "hello world");
        assert_eq!(resolved["n"], 5);
    }

    #[test]
    fn interpolate_resolves_top_level_ref() {
        let mut vars = Map::new();
        vars.insert(
            "decision".into(),
            serde_json::json!({"shouldRespond": true, "reason": "user asked"}),
        );
        let params = serde_json::json!({"flag": "{decision.shouldRespond}"});
        let resolved = interpolate_params(&params, &vars).expect("resolve");
        assert_eq!(resolved["flag"], Value::Bool(true));
    }

    #[test]
    fn interpolate_errors_on_unknown_variable() {
        let vars = Map::new();
        let params = serde_json::json!({"x": "{missing.path}"});
        let err = interpolate_params(&params, &vars).unwrap_err();
        assert!(err.contains("missing"), "got: {}", err);
    }

    #[test]
    fn interpolate_errors_on_missing_path() {
        let mut vars = Map::new();
        vars.insert("decision".into(), serde_json::json!({"a": 1}));
        let params = serde_json::json!({"x": "{decision.nonexistent}"});
        let err = interpolate_params(&params, &vars).unwrap_err();
        assert!(err.contains("nonexistent"), "got: {}", err);
    }
}
