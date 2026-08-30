//! The pipeline executor — the kernel loop that walks a recipe's steps and
//! dispatches each through the command system.
//!
//! Everything a step can DO comes from the command registry; everything a
//! step can DECIDE comes from the three pure modules beside this one
//! (interpolate/condition/state). The executor itself is deliberately dumb:
//! resolve, dispatch, bind, probe. Sub-millisecond per step outside the
//! command's own work.

use super::condition;
use super::interpolate::interpolate;
use super::state::ExecutionState;
use super::types::Recipe;
use crate::runtime::command_executor::CommandExecutor;
use serde_json::Value;
use std::sync::Arc;

/// One executed run's receipt — what happened, per step, plus the final state
/// bindings the caller asked steps to produce.
#[derive(Debug, serde::Serialize)]
pub struct RecipeRunReceipt {
    pub recipe: String,
    pub steps_run: u32,
    pub steps_skipped: u32,
    /// Step-indexed one-line outcomes, in order — the run's readable trace.
    pub trace: Vec<String>,
    /// Every `outputTo` binding's final value (args excluded) — the run's
    /// RESULT, shaped by the recipe itself.
    pub bindings: serde_json::Map<String, Value>,
}

pub struct PipelineExecutor {
    executor: Arc<CommandExecutor>,
}

impl PipelineExecutor {
    pub fn new(executor: Arc<CommandExecutor>) -> Self {
        Self { executor }
    }

    pub async fn run(&self, recipe: &Recipe, args: Value) -> Result<RecipeRunReceipt, String> {
        let mut state = ExecutionState::with_args(args);
        let mut trace = Vec::new();
        let mut steps_run = 0u32;
        let mut steps_skipped = 0u32;
        let mut bound: Vec<String> = Vec::new();

        for (idx, step) in recipe.pipeline.iter().enumerate() {
            if let Some(cond) = &step.condition {
                if !condition::evaluate(cond, &state)? {
                    crate::probe!(
                        class = "recipe.step.skipped",
                        recipe = %recipe.name,
                        step = idx as u64,
                        command = %step.command,
                        condition = %cond,
                        "condition falsy — step skipped"
                    );
                    trace.push(format!("[{idx}] {} SKIP ({cond})", step.command));
                    steps_skipped += 1;
                    continue;
                }
            }
            let params = interpolate(&step.params, &state)
                .map_err(|e| format!("step {idx} ({}): {e}", step.command))?;

            let mut outcome: Result<Value, String> = Err("unattempted".into());
            for attempt in 0..=step.retry_count {
                let dispatch = self.executor.execute(step.command.as_str(), params.clone());
                let result = match step.timeout_ms {
                    Some(ms) => {
                        match tokio::time::timeout(std::time::Duration::from_millis(ms), dispatch)
                            .await
                        {
                            Ok(r) => r,
                            Err(_) => Err(format!("timed out after {ms}ms")),
                        }
                    }
                    None => dispatch.await,
                };
                match result {
                    Ok(r) => match r.to_json_value() {
                        Ok(v) => {
                            outcome = Ok(v);
                            break;
                        }
                        Err(e) => outcome = Err(e),
                    },
                    Err(e) => {
                        if attempt < step.retry_count {
                            crate::probe!(
                                class = "recipe.step.retry",
                                recipe = %recipe.name,
                                step = idx as u64,
                                command = %step.command,
                                attempt = (attempt + 1) as u64,
                                error = %e,
                                "step failed — retrying per its declared retry_count"
                            );
                        }
                        outcome = Err(e);
                    }
                }
            }

            match outcome {
                Ok(value) => {
                    crate::probe!(
                        class = "recipe.step.ok",
                        recipe = %recipe.name,
                        step = idx as u64,
                        command = %step.command,
                        "step completed"
                    );
                    trace.push(format!("[{idx}] {} OK", step.command));
                    steps_run += 1;
                    if let Some(name) = &step.output_to {
                        state.bind(name.clone(), value);
                        if !bound.contains(name) {
                            bound.push(name.clone());
                        }
                    }
                }
                Err(e) => {
                    crate::probe!(
                        class = "recipe.step.failed",
                        recipe = %recipe.name,
                        step = idx as u64,
                        command = %step.command,
                        on_error = %step.on_error.as_deref().unwrap_or("fail"),
                        error = %e,
                        "step failed"
                    );
                    trace.push(format!("[{idx}] {} ERR {e}", step.command));
                    match step.on_error.as_deref() {
                        Some("skip") => steps_skipped += 1,
                        // Default and "fail": the run stops, loudly, at the step.
                        _ => {
                            return Err(format!(
                                "recipe `{}` failed at step {idx} ({}): {e}",
                                recipe.name, step.command
                            ))
                        }
                    }
                }
            }
        }

        let mut bindings = serde_json::Map::new();
        for name in bound {
            if let Some(v) = state.lookup(&name) {
                bindings.insert(name, v.clone());
            }
        }
        crate::probe!(
            class = "recipe.run.done",
            recipe = %recipe.name,
            steps_run = steps_run as u64,
            steps_skipped = steps_skipped as u64,
            "pipeline complete"
        );
        Ok(RecipeRunReceipt {
            recipe: recipe.name.clone(),
            steps_run,
            steps_skipped,
            trace,
            bindings,
        })
    }
}
