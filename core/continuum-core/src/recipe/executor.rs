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
use super::types::RecipeStep;
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
    /// The step index the run HELD at for approval (`approval: "human"`), if it
    /// did. That step and everything after it did not run; the receipt is how a
    /// human learns what is waiting on them. `None` = the run reached its end.
    pub held_at: Option<u32>,
}

pub struct PipelineExecutor {
    executor: Arc<CommandExecutor>,
}

impl PipelineExecutor {
    pub fn new(executor: Arc<CommandExecutor>) -> Self {
        Self { executor }
    }

    /// Walk `pipeline` under `name` (the recipe's purpose, for receipts and probes).
    /// Takes the steps rather than a recipe struct so the ONE activity recipe
    /// (`ExperienceRecipe.pipeline`, S0) drives it directly — there is no second
    /// "pipeline recipe" type any more (S2).
    pub async fn run(
        &self,
        name: &str,
        pipeline: &[RecipeStep],
        args: Value,
    ) -> Result<RecipeRunReceipt, String> {
        self.run_with(name, pipeline, args, Vec::new()).await
    }

    /// As [`Self::run`], with bindings readable before the first step — the ROOM an
    /// activity's pipeline runs in (`$room`, S3). `activity/spawn` seeds it; a bare
    /// `recipe/run` seeds nothing.
    pub async fn run_with(
        &self,
        name: &str,
        pipeline: &[RecipeStep],
        args: Value,
        seed: Vec<(String, Value)>,
    ) -> Result<RecipeRunReceipt, String> {
        let mut state = ExecutionState::seeded(args, seed);
        let mut held_at: Option<u32> = None;
        let mut trace = Vec::new();
        let mut steps_run = 0u32;
        let mut steps_skipped = 0u32;
        let mut bound: Vec<String> = Vec::new();

        for (idx, step) in pipeline.iter().enumerate() {
            if let Some(cond) = &step.condition {
                if !condition::evaluate(cond, &state)? {
                    crate::probe!(
                        class = "recipe.step.skipped",
                        recipe = %name,
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
            // THE APPROVAL BOUNDARY (S3). A step that needs a human does not run
            // unattended — not "runs and asks", not "runs if confident": it HOLDS,
            // the receipt names it, and the run ends here. Checked before
            // interpolation so a held step never even resolves its arguments.
            if let Some(who) = step.approval.as_deref() {
                crate::probe!(
                    class = "recipe.step.held",
                    recipe = %name,
                    step = idx as u64,
                    command = %step.command,
                    approval = %who,
                    "step needs approval — the run holds here; nothing after it runs"
                );
                trace.push(format!("[{idx}] {} HELD (approval: {who})", step.command));
                held_at = Some(idx as u32);
                break;
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
                                recipe = %name,
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
                        recipe = %name,
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
                        recipe = %name,
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
                                name, step.command
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
            recipe = %name,
            steps_run = steps_run as u64,
            steps_skipped = steps_skipped as u64,
            held_at = held_at.map(|i| i as i64).unwrap_or(-1),
            "pipeline complete"
        );
        Ok(RecipeRunReceipt {
            recipe: name.to_string(),
            steps_run,
            steps_skipped,
            trace,
            bindings,
            held_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::registry::ModuleRegistry;

    fn step(command: &str, approval: Option<&str>) -> RecipeStep {
        RecipeStep {
            command: command.to_string(),
            params: Value::Null,
            output_to: None,
            condition: None,
            on_error: None,
            retry_count: 0,
            timeout_ms: None,
            approval: approval.map(str::to_string),
        }
    }

    #[tokio::test]
    async fn a_step_that_needs_a_human_holds_the_run_before_it_dispatches_anything() {
        // what this catches: the approval boundary. A held step must never reach the
        // executor — over an EMPTY registry any dispatch would fail loudly, so a
        // clean Ok receipt with held_at = 0 proves nothing was attempted — and the
        // step after it must not run either. If this regresses, an authored
        // campaign submits applications in the human's name unattended.
        let exec = Arc::new(crate::runtime::command_executor::CommandExecutor::new(Arc::new(
            ModuleRegistry::new(),
        )));
        let pipeline = vec![step("browser/act", Some("human")), step("mail/send", None)];
        let receipt = PipelineExecutor::new(exec)
            .run("campaign/applications", &pipeline, Value::Null)
            .await
            .expect("a held run is a clean receipt, not an error");
        assert_eq!(receipt.held_at, Some(0));
        assert_eq!(receipt.steps_run, 0);
        assert_eq!(receipt.trace, vec!["[0] browser/act HELD (approval: human)"]);
    }
}
