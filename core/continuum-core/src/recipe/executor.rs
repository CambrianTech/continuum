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
use super::types::{OnError, RecipeStep};
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
    /// The identity every step acts as (`operator_peer::acting_caller`). `None` =
    /// the substrate's own code — right for a bare `recipe/run`, wrong for a birth,
    /// whose steps must be the spawner (2026-09-13: cards posted as the operator
    /// into a room only the agent peer had joined).
    caller: Option<crate::routing::CallerIdentity>,
    /// Where a step's `saves` go: the activity's bundle, bound to its room by the
    /// birth path. `None` for a bare `recipe/run`.
    state_sink: Option<Arc<dyn crate::experience::activity_state::StepStateSink>>,
}

impl PipelineExecutor {
    pub fn new(executor: Arc<CommandExecutor>) -> Self {
        Self { executor, caller: None, state_sink: None }
    }

    /// Save each step's `saves` bindings through `sink` (the activity's bundle).
    pub fn with_state_sink(mut self, sink: Arc<dyn crate::experience::activity_state::StepStateSink>) -> Self {
        self.state_sink = Some(sink);
        self
    }

    /// THE SAVE BOUNDARY. After a step completes, the bindings it marks with `saves`
    /// go into the activity's bundle — the framework saves, the recipe declares what.
    /// A name that is bound to nothing is skipped by name; no sink at all is probed
    /// once per step and never fails the run (state is a durability concern, not a
    /// correctness gate for the step that just ran).
    async fn save_step_state(&self, name: &str, idx: usize, step: &RecipeStep, state: &ExecutionState) {
        if step.saves.is_empty() {
            return;
        }
        let Some(sink) = &self.state_sink else {
            crate::probe!(
                class = "recipe.step.save_unsunk",
                recipe = %name,
                step = idx as u64,
                command = %step.command,
                saves = %step.saves.join(","),
                "step asks to save bindings but this run has no activity bundle (bare recipe/run?)"
            );
            return;
        };
        let mut patch = serde_json::Map::new();
        for key in &step.saves {
            match state.lookup(key) {
                Some(v) => {
                    patch.insert(key.clone(), v.clone());
                }
                None => crate::probe!(
                    class = "recipe.step.save_unbound",
                    recipe = %name,
                    step = idx as u64,
                    command = %step.command,
                    binding = %key,
                    "saves names a binding nothing bound — skipped"
                ),
            }
        }
        if patch.is_empty() {
            return;
        }
        if let Err(e) = sink.save(patch).await {
            crate::probe!(
                class = "recipe.step.save_failed",
                recipe = %name,
                step = idx as u64,
                command = %step.command,
                error = %e,
                "activity bundle save failed — the step's result stands, the bundle is stale"
            );
        }
    }

    /// Act as `caller` in every step.
    pub fn with_caller(mut self, caller: Option<crate::routing::CallerIdentity>) -> Self {
        self.caller = caller;
        self
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
            if let Some(who) = step.approval {
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
            // FAN-OUT (S4): once per element of the named array, `$item` bound.
            if let Some(each) = &step.each {
                let items = match interpolate(&Value::String(each.clone()), &state)? {
                    Value::Array(items) => items,
                    other => {
                        return Err(format!(
                            "step {idx} ({}): `each` must name an array, got {}",
                            step.command,
                            match other { Value::Null => "null", Value::Object(_) => "an object", _ => "a scalar" }
                        ))
                    }
                };
                let mut results = Vec::with_capacity(items.len());
                let mut ran = 0u32;
                let mut skipped = 0u32;
                for (i, item) in items.into_iter().enumerate() {
                    state.bind("item", item);
                    state.bind("index", Value::from(i as u64));
                    if let Some(cond) = &step.condition {
                        if !condition::evaluate(cond, &state)? {
                            skipped += 1;
                            continue;
                        }
                    }
                    let params = interpolate(&step.params, &state)
                        .map_err(|e| format!("step {idx} ({}) item {i}: {e}", step.command))?;
                    match self.dispatch_step(name, idx, step, params).await {
                        Ok(v) => {
                            results.push(v);
                            ran += 1;
                        }
                        Err(e) => match step.on_error {
                            OnError::Skip => {
                                crate::probe!(
                                    class = "recipe.step.item_skipped",
                                    recipe = %name,
                                    step = idx as u64,
                                    item = i as u64,
                                    command = %step.command,
                                    error = %e,
                                    "item failed — skipped per on_error"
                                );
                                skipped += 1;
                            }
                            OnError::Fail => {
                                return Err(format!(
                                    "recipe `{}` failed at step {idx} ({}) item {i}: {e}",
                                    name, step.command
                                ))
                            }
                        },
                    }
                }
                trace.push(format!("[{idx}] {} OK×{ran} SKIP×{skipped} (each)", step.command));
                steps_run += ran;
                steps_skipped += skipped;
                if let Some(out) = &step.output_to {
                    state.bind(out.clone(), Value::Array(results));
                    if !bound.contains(out) {
                        bound.push(out.clone());
                    }
                }
                self.save_step_state(name, idx, step, &state).await;
                continue;
            }
            let params = interpolate(&step.params, &state)
                .map_err(|e| format!("step {idx} ({}): {e}", step.command))?;

            let outcome = self.dispatch_step(name, idx, step, params).await;
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
                    self.save_step_state(name, idx, step, &state).await;
                }
                Err(e) => {
                    crate::probe!(
                        class = "recipe.step.failed",
                        recipe = %name,
                        step = idx as u64,
                        command = %step.command,
                        on_error = %step.on_error,
                        error = %e,
                        "step failed"
                    );
                    trace.push(format!("[{idx}] {} ERR {e}", step.command));
                    match step.on_error {
                        OnError::Skip => steps_skipped += 1,
                        // The run stops, loudly, at the step.
                        OnError::Fail => {
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

    /// One dispatch of `step` with its declared retries and per-attempt bound —
    /// shared by the single-run and `each` paths so the retry policy has one home.
    async fn dispatch_step(&self, name: &str, idx: usize, step: &RecipeStep, params: Value) -> Result<Value, String> {
        let mut outcome: Result<Value, String> = Err("unattempted".into());
        for attempt in 0..=step.retry_count {
            let dispatch =
                self.executor.execute_with_caller(step.command.as_str(), params.clone(), self.caller.clone());
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
        outcome
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::registry::ModuleRegistry;

    fn step(command: &str, approval: Option<super::super::types::Approval>) -> RecipeStep {
        RecipeStep {
            command: command.to_string(),
            params: Value::Null,
            output_to: None,
            saves: Vec::new(),
            condition: None,
            on_error: OnError::Fail,
            retry_count: 0,
            timeout_ms: None,
            approval,
            each: None,
        }
    }

    struct MemorySink(std::sync::Mutex<Vec<serde_json::Map<String, Value>>>);
    #[async_trait::async_trait]
    impl crate::experience::activity_state::StepStateSink for MemorySink {
        async fn save(&self, patch: serde_json::Map<String, Value>) -> Result<(), String> {
            self.0.lock().unwrap().push(patch); // JUSTIFIED: a test's own mutex
            Ok(())
        }
    }

    /// what this catches: a step's `saves` never reaching the bundle — the round
    /// that "resumed" from an empty state (card c9ddb911) — and the mirror: a bare
    /// run with no bundle must not fail the step that just ran.
    #[tokio::test]
    async fn a_steps_saves_land_in_the_bundle_and_a_run_without_a_bundle_still_completes() {
        let exec = Arc::new(crate::runtime::command_executor::CommandExecutor::new(Arc::new(
            ModuleRegistry::new(),
        )));
        let mut post = step("work/create", None);
        post.each = Some("$args.rows".to_string());
        post.params = serde_json::json!({ "title": "$item.title" });
        post.on_error = OnError::Skip;
        post.output_to = Some("cards".to_string());
        post.saves = vec!["cards".to_string(), "never_bound".to_string()];
        let args = serde_json::json!({ "rows": [ {"title": "a"} ] });
        let sink = Arc::new(MemorySink(std::sync::Mutex::new(Vec::new())));
        let sink_dyn: Arc<dyn crate::experience::activity_state::StepStateSink> = sink.clone();
        PipelineExecutor::new(exec.clone())
            .with_state_sink(sink_dyn)
            .run("bench/saves", std::slice::from_ref(&post), args.clone())
            .await
            .expect("skips are not failures"); // JUSTIFIED: on_error skip by construction
        let saved = sink.0.lock().unwrap(); // JUSTIFIED: a test's own mutex
        assert_eq!(saved.len(), 1, "one save at the step boundary");
        assert_eq!(saved[0].get("cards"), Some(&Value::Array(vec![])), "the bound name is saved");
        assert!(saved[0].get("never_bound").is_none(), "an unbound name is skipped, not saved as null");
        drop(saved);
        let receipt = PipelineExecutor::new(exec)
            .run("bench/saves-unsunk", &[post], args)
            .await
            .expect("no sink is a probe, never a failure"); // JUSTIFIED: the invariant under test
        assert_eq!(receipt.steps_skipped, 1);
    }

    #[tokio::test]
    async fn each_fans_out_one_dispatch_per_item_never_once_with_the_whole_array() {
        // what this catches: fan-out is how one `work/create` step posts a whole
        // imported suite. Over an EMPTY registry every dispatch fails, so with
        // on_error: skip the skip count IS the dispatch count: three items, three
        // skips. A step that ran once with the array would report one.
        let exec = Arc::new(crate::runtime::command_executor::CommandExecutor::new(Arc::new(
            ModuleRegistry::new(),
        )));
        let mut post = step("work/create", None);
        post.each = Some("$args.rows".to_string());
        post.params = serde_json::json!({ "title": "$item.title" });
        post.on_error = OnError::Skip;
        post.output_to = Some("cards".to_string());
        let args = serde_json::json!({ "rows": [ {"title": "a"}, {"title": "b"}, {"title": "c"} ] });
        let receipt = PipelineExecutor::new(exec)
            .run("bench/fan-out", &[post], args)
            .await
            .expect("skips are not failures");
        assert_eq!(receipt.steps_skipped, 3, "one dispatch per item: {:?}", receipt.trace);
        assert_eq!(receipt.steps_run, 0);
        assert_eq!(receipt.bindings.get("cards"), Some(&Value::Array(vec![])));
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
        let pipeline = vec![step("browser/act", Some(super::super::types::Approval::Human)), step("mail/send", None)];
        let receipt = PipelineExecutor::new(exec)
            .run("campaign/applications", &pipeline, Value::Null)
            .await
            .expect("a held run is a clean receipt, not an error");
        assert_eq!(receipt.held_at, Some(0));
        assert_eq!(receipt.steps_run, 0);
        assert_eq!(receipt.trace, vec!["[0] browser/act HELD (approval: human)"]);
    }
}
