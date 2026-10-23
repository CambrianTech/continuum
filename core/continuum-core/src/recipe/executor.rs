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
use super::run_record::{
    CallerRecord, Invocation, InvocationState, RecipeRunRecord, RunCursor, RunLedger, RunOrigin, RunPhase,
};
use super::types::{OnError, RecipeStep};
use crate::runtime::command_executor::{CommandExecutor, DispatchFailure, DispatchOutcome};
use serde_json::Value;
use std::sync::Arc;
use uuid::Uuid;

/// Where a run's record goes. A run with no journal never starts: dispatching
/// commands no record can account for is exactly what a durable run forbids
/// (Astra review of #5008, gap 4).
enum Journal {
    Store(Arc<dyn RunLedger>),
    /// The registry holds no recipe module, or its store has not opened.
    Missing,
    /// A test that wants the walk without a store says so by name.
    #[cfg(any(test, feature = "test-fixtures"))]
    Unrecorded,
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
    /// Where the run's record is written: the node's `recipe_runs`, found through
    /// the executor's registry.
    journal: Journal,
    /// The recipe and room a run belongs to, for its record.
    origin: RunOrigin,
}

impl PipelineExecutor {
    pub fn new(executor: Arc<CommandExecutor>) -> Self {
        let journal = match crate::recipe::runs_owner::RecipeModule::runs_in(executor.registry()) {
            Some(runs) => Journal::Store(runs),
            None => Journal::Missing,
        };
        Self { executor, caller: None, state_sink: None, journal, origin: RunOrigin::default() }
    }

    /// A walk with no record at all — a test of the walk itself, never a node's run.
    #[cfg(any(test, feature = "test-fixtures"))]
    pub fn unrecorded(executor: Arc<CommandExecutor>) -> Self {
        Self { journal: Journal::Unrecorded, ..Self::new(executor) }
    }

    /// Save each step's `saves` bindings through `sink` (the activity's bundle).
    pub fn with_state_sink(mut self, sink: Arc<dyn crate::experience::activity_state::StepStateSink>) -> Self {
        self.state_sink = Some(sink);
        self
    }

    /// Act as `caller` in every step.
    pub fn with_caller(mut self, caller: Option<crate::routing::CallerIdentity>) -> Self {
        self.caller = caller;
        self
    }

    /// Name the recipe and room the run belongs to, for its record.
    pub fn with_origin(mut self, origin: RunOrigin) -> Self {
        self.origin = origin;
        self
    }

    /// Write runs to `ledger` instead of the registry's store.
    pub fn with_ledger(mut self, ledger: Arc<dyn RunLedger>) -> Self {
        self.journal = Journal::Store(ledger);
        self
    }
}

/// One run's receipt — what happened, per step, plus the final state bindings the
/// caller asked steps to produce. Derived from the run's record, never kept apart.
#[derive(Debug, serde::Serialize)]
pub struct RecipeRunReceipt {
    pub run_id: Uuid,
    pub recipe: String,
    pub phase: RunPhase,
    pub steps_run: u32,
    pub steps_skipped: u32,
    /// Step-indexed one-line outcomes, in order — the run's readable trace.
    pub trace: Vec<String>,
    /// Every `outputTo` binding's final value (args excluded) — the run's
    /// RESULT, shaped by the recipe itself.
    pub bindings: serde_json::Map<String, Value>,
    /// The step the run HELD at for approval, if it did. That step and everything
    /// after it did not run.
    pub held_at: Option<u32>,
}

impl RecipeRunReceipt {
    pub fn of(run: &RecipeRunRecord) -> Result<Self, String> {
        Ok(Self {
            run_id: run.run_id()?,
            recipe: run.purpose.clone(),
            phase: run.phase.clone(),
            steps_run: run.steps_run,
            steps_skipped: run.steps_skipped,
            trace: run.trace.clone(),
            bindings: run.bindings.clone(),
            held_at: match run.phase {
                RunPhase::Held { step } => Some(step),
                _ => None,
            },
        })
    }
}

/// How one invocation settled, as the walk needs it.
enum Settled {
    Committed(Value),
    /// Refused before any handler ran (every attempt): the step's `on_error` decides.
    Refused(String),
    /// Outcome unknown: the run is now `RecoveryRequired` and must stop.
    Uncertain,
}

impl PipelineExecutor {
    /// Walk `pipeline` under `name` with no room seed. See [`Self::run_with`].
    pub async fn run(&self, name: &str, pipeline: &[RecipeStep], args: Value) -> Result<RecipeRunReceipt, String> {
        self.run_with(name, pipeline, args, Vec::new()).await
    }

    /// Begin and drive one run of `pipeline`. `Err` = the run FAILED (a step with
    /// `on_error: fail` was refused, a reference did not resolve, or the ledger
    /// refused a write). A run that holds for approval or stops on an uncertain
    /// invocation returns `Ok` with that phase: it is not over, and nothing about
    /// it may be thrown away.
    pub async fn run_with(
        &self,
        name: &str,
        pipeline: &[RecipeStep],
        args: Value,
        seed: Vec<(String, Value)>,
    ) -> Result<RecipeRunReceipt, String> {
        let mut run = self.begin(name, pipeline, args, seed);
        self.drive(&mut run).await?;
        let receipt = RecipeRunReceipt::of(&run)?;
        match &run.phase {
            RunPhase::Failed { error } => Err(error.clone()),
            _ => Ok(receipt),
        }
    }

    /// A new run's record: the steps snapshotted as they are now, `Running`, cursor
    /// at the first step. Nothing is written until [`Self::drive`].
    pub fn begin(&self, name: &str, pipeline: &[RecipeStep], args: Value, seed: Vec<(String, Value)>) -> RecipeRunRecord {
        RecipeRunRecord::begin(self.origin, name, pipeline.to_vec(), args, seed.into_iter().collect(), self.caller.as_ref().map(CallerRecord::of))
    }

    /// Walk the run from its cursor to an end, a hold, or an uncertain
    /// invocation, writing the record at every boundary. The run's outcomes are its
    /// phase. `Err` only when NOTHING was dispatched and the run could not be
    /// recorded (no store, or its first write refused): then it never started.
    ///
    /// A write refused AFTER an invocation was sent is not a failure the run can
    /// report as one: a handler may have committed while its result was never
    /// recorded (Astra review of #5008, gap 3). The run is then `RecoveryRequired`
    /// naming the last invocation, returned `Ok`, so the room and the run id
    /// survive; the row's last durable state says the same at the next boot.
    pub async fn drive(&self, run: &mut RecipeRunRecord) -> Result<(), String> {
        if matches!(self.journal, Journal::Missing) {
            let error = "no recipe run store is open on this node (the recipe module is absent or \
                         failed to initialize), so the run was refused before any dispatch"
                .to_string();
            run.phase = RunPhase::Failed { error: error.clone() };
            run.is_active = false;
            return Err(error);
        }
        match self.walk(run).await {
            Ok(()) => Ok(()),
            Err(error) => match run.invocations.last() {
                None => {
                    run.phase = RunPhase::Failed { error: format!("the run could not be recorded: {error}") };
                    run.is_active = false;
                    Err(error)
                }
                Some(last) => {
                    let invocation = last.id;
                    continuum_base::probe!(class = "recipe.run.unrecorded_after_dispatch", recipe = %run.purpose, invocation = %invocation, error = %error, "a record write failed after a dispatch — the run waits for recovery");
                    run.phase = RunPhase::RecoveryRequired {
                        invocation,
                        reason: format!("the run's record could not be written after this invocation was sent ({error}); its durable state is unknown"),
                    };
                    if let Err(again) = self.persist(run).await {
                        continuum_base::probe!(class = "recipe.run.recovery_unrecorded", recipe = %run.purpose, error = %again, "the RecoveryRequired phase could not be written either; the next boot finds the row Running and exposes it");
                    }
                    Ok(())
                }
            },
        }
    }

    async fn walk(&self, run: &mut RecipeRunRecord) -> Result<(), String> {
        let name = run.purpose.clone();
        let pipeline = run.pipeline.clone();
        let mut state = ExecutionState::seeded(run.args.clone().unwrap_or(Value::Null), run.seed.clone()); // unwrap_or: a run given no args reads them as null, and a `$args.*` reference then fails loud by name
        for (k, v) in run.bindings.clone() {
            state.bind(k, v);
        }
        run.phase = RunPhase::Running;
        self.persist(run).await?;

        while (run.cursor.step as usize) < pipeline.len() {
            if self.cancel_requested(run).await? {
                return self.finish(run, RunPhase::Cancelled).await;
            }
            let idx = run.cursor.step as usize;
            let step = &pipeline[idx];
            let fresh = run.cursor.item.is_none();
            if fresh {
                if let Some(cond) = &step.condition {
                    match condition::evaluate(cond, &state) {
                        Ok(true) => {}
                        Ok(false) => {
                            continuum_base::probe!(class = "recipe.step.skipped", recipe = %name, step = idx as u64, command = %step.command, condition = %cond, "condition falsy — step skipped");
                            run.trace.push(format!("[{idx}] {} SKIP ({cond})", step.command));
                            run.steps_skipped += 1;
                            run.cursor = RunCursor { step: run.cursor.step + 1, item: None };
                            self.persist(run).await?;
                            continue;
                        }
                        Err(e) => return self.fail(run, format!("step {idx} ({}): {e}", step.command)).await,
                    }
                }
                // THE APPROVAL BOUNDARY (S3): a step that needs a human holds the run
                // before it resolves its arguments; nothing at or after it runs.
                if let Some(who) = step.approval {
                    continuum_base::probe!(class = "recipe.step.held", recipe = %name, step = idx as u64, command = %step.command, approval = %who, "step needs approval — the run holds here; nothing after it runs");
                    run.trace.push(format!("[{idx}] {} HELD (approval: {who})", step.command));
                    run.phase = RunPhase::Held { step: idx as u32 };
                    return self.persist(run).await;
                }
            }
            if let Some(each) = &step.each {
                // FAN-OUT (S4), frozen: the array is resolved ONCE and kept on the
                // record, so a resumed run walks the same items it began with.
                let key = idx.to_string();
                let items = match run.frozen_each.get(&key) {
                    Some(Value::Array(items)) => items.clone(),
                    _ => match interpolate(&Value::String(each.clone()), &state) {
                        Ok(Value::Array(items)) => {
                            run.frozen_each.insert(key, Value::Array(items.clone()));
                            run.cursor.item = Some(0);
                            self.persist(run).await?;
                            items
                        }
                        Ok(other) => {
                            let kind = match other { Value::Null => "null", Value::Object(_) => "an object", _ => "a scalar" };
                            return self.fail(run, format!("step {idx} ({}): `each` must name an array, got {kind}", step.command)).await;
                        }
                        Err(e) => return self.fail(run, format!("step {idx} ({}): {e}", step.command)).await,
                    },
                };
                let start = run.cursor.item.unwrap_or(0) as usize;
                for (i, item) in items.into_iter().enumerate().skip(start) {
                    if self.cancel_requested(run).await? {
                        return self.finish(run, RunPhase::Cancelled).await;
                    }
                    state.bind("item", item);
                    state.bind("index", Value::from(i as u64));
                    let next = RunCursor { step: idx as u32, item: Some(i as u32 + 1) };
                    if let Some(cond) = &step.condition {
                        match condition::evaluate(cond, &state) {
                            Ok(true) => {}
                            Ok(false) => {
                                run.steps_skipped += 1;
                                run.cursor = next;
                                self.persist(run).await?;
                                continue;
                            }
                            Err(e) => return self.fail(run, format!("step {idx} ({}) item {i}: {e}", step.command)).await,
                        }
                    }
                    let params = match interpolate(&step.params, &state) {
                        Ok(p) => p,
                        Err(e) => return self.fail(run, format!("step {idx} ({}) item {i}: {e}", step.command)).await,
                    };
                    match self.invoke(run, idx, Some(i as u32), step, params).await? {
                        Settled::Committed(_) => run.steps_run += 1,
                        Settled::Refused(e) => match step.on_error {
                            OnError::Skip => {
                                continuum_base::probe!(class = "recipe.step.item_skipped", recipe = %name, step = idx as u64, item = i as u64, command = %step.command, error = %e, "item refused before dispatch — skipped per on_error");
                                run.steps_skipped += 1;
                            }
                            OnError::Fail => {
                                return self.fail(run, format!("recipe `{name}` failed at step {idx} ({}) item {i}: {e}", step.command)).await
                            }
                        },
                        Settled::Uncertain => return Ok(()),
                    }
                    // THE COMMIT: the invocation's result and the cursor past it, one write.
                    run.cursor = next;
                    self.persist(run).await?;
                }
                let results: Vec<Value> = run
                    .invocations
                    .iter()
                    .filter(|inv| inv.step == idx as u32)
                    .filter_map(|inv| match &inv.state {
                        InvocationState::Committed { result } => Some(result.clone()),
                        _ => None,
                    })
                    .collect();
                run.trace.push(format!("[{idx}] {} OK×{} (each)", step.command, results.len()));
                if let Some(out) = &step.output_to {
                    state.bind(out.clone(), Value::Array(results.clone()));
                    run.bindings.insert(out.clone(), Value::Array(results));
                }
                self.complete_step(run, idx, step, &state).await?;
                continue;
            }

            let params = match interpolate(&step.params, &state) {
                Ok(p) => p,
                Err(e) => return self.fail(run, format!("step {idx} ({}): {e}", step.command)).await,
            };
            match self.invoke(run, idx, None, step, params).await? {
                Settled::Committed(value) => {
                    continuum_base::probe!(class = "recipe.step.ok", recipe = %name, step = idx as u64, command = %step.command, "step completed");
                    run.trace.push(format!("[{idx}] {} OK", step.command));
                    run.steps_run += 1;
                    if let Some(out) = &step.output_to {
                        state.bind(out.clone(), value.clone());
                        run.bindings.insert(out.clone(), value);
                    }
                }
                Settled::Refused(e) => {
                    run.trace.push(format!("[{idx}] {} ERR {e}", step.command));
                    match step.on_error {
                        OnError::Skip => run.steps_skipped += 1,
                        OnError::Fail => {
                            return self.fail(run, format!("recipe `{name}` failed at step {idx} ({}): {e}", step.command)).await
                        }
                    }
                }
                Settled::Uncertain => return Ok(()),
            }
            self.complete_step(run, idx, step, &state).await?;
        }
        continuum_base::probe!(class = "recipe.run.done", recipe = %name, steps_run = run.steps_run as u64, steps_skipped = run.steps_skipped as u64, "pipeline complete");
        self.finish(run, RunPhase::Completed).await
    }

    /// The step boundary: its `saves` patch and the cursor past it commit together
    /// with whatever the step bound; then the patch goes to the room bundle. A
    /// bundle write that fails leaves `pending_saves` for a later retry and never
    /// re-dispatches the step.
    async fn complete_step(&self, run: &mut RecipeRunRecord, idx: usize, step: &RecipeStep, state: &ExecutionState) -> Result<(), String> {
        // A patch MERGES into whatever earlier step's patch is still unsaved: a failed
        // projection is retried with its input, never replaced by the next step's
        // (Astra review of #5008, gap 2).
        if let Some(patch) = self.saves_patch(&run.purpose, idx, step, state) {
            let pending = run.pending_saves.get_or_insert_with(serde_json::Map::new);
            for (key, value) in patch {
                pending.insert(key, value);
            }
        }
        run.cursor = RunCursor { step: idx as u32 + 1, item: None };
        self.persist(run).await?;
        let Some(patch) = run.pending_saves.clone() else { return Ok(()) };
        let Some(sink) = &self.state_sink else {
            continuum_base::probe!(class = "recipe.step.save_unsunk", recipe = %run.purpose, step = idx as u64, command = %step.command, saves = %step.saves.join(","), "step asks to save bindings but this run has no activity bundle (bare recipe/run?)");
            run.pending_saves = None;
            return self.persist(run).await;
        };
        match sink.save(patch).await {
            Ok(()) => {
                run.pending_saves = None;
                self.persist(run).await
            }
            Err(e) => {
                continuum_base::probe!(class = "recipe.step.save_failed", recipe = %run.purpose, step = idx as u64, command = %step.command, error = %e, "activity bundle save failed — the step's result stands; the patch stays pending on the run");
                Ok(())
            }
        }
    }

    /// The bindings a step marks with `saves`, by name. A name bound to nothing
    /// is probed and skipped. `None` = nothing to save.
    fn saves_patch(&self, name: &str, idx: usize, step: &RecipeStep, state: &ExecutionState) -> Option<serde_json::Map<String, Value>> {
        let mut patch = serde_json::Map::new();
        for key in &step.saves {
            match state.lookup(key) {
                Some(v) => {
                    patch.insert(key.clone(), v.clone());
                }
                None => continuum_base::probe!(class = "recipe.step.save_unbound", recipe = %name, step = idx as u64, command = %step.command, binding = %key, "saves names a binding nothing bound — skipped"),
            }
        }
        (!patch.is_empty()).then_some(patch)
    }

    /// One invocation: written as `Dispatching` BEFORE it is sent, then settled.
    /// Retries (`retry_count`) apply only to a refusal that reached no handler; an
    /// uncertain outcome, including a timeout, is never retried — the run goes to
    /// `RecoveryRequired` holding this invocation.
    async fn invoke(&self, run: &mut RecipeRunRecord, idx: usize, item: Option<u32>, step: &RecipeStep, params: Value) -> Result<Settled, String> {
        let id = Invocation::id_for(run.run_id()?, idx as u32, item);
        let slot = match run.invocations.iter().position(|inv| inv.id == id) {
            Some(pos) => pos,
            None => {
                run.invocations.push(Invocation {
                    id,
                    step: idx as u32,
                    item,
                    command: step.command.clone(),
                    params: params.clone(),
                    state: InvocationState::Dispatching { attempt: 0 },
                });
                run.invocations.len() - 1
            }
        };
        let mut attempt = 0u32;
        loop {
            run.invocations[slot].state = InvocationState::Dispatching { attempt };
            // WRITE-AHEAD: after a crash, this row is the proof a dispatch may have happened.
            self.persist(run).await?;
            let dispatch = self.executor.execute_classified(step.command.as_str(), params.clone(), self.caller.clone());
            let outcome = match step.timeout_ms {
                Some(ms) => match tokio::time::timeout(std::time::Duration::from_millis(ms), dispatch).await {
                    Ok(r) => r,
                    // The handler may still have acted: a timeout is never a refusal.
                    Err(_) => Err(DispatchFailure { outcome: DispatchOutcome::Uncertain, message: format!("timed out after {ms}ms") }),
                },
                None => dispatch.await,
            };
            let failure = match outcome {
                Ok(result) => match result.to_json_value() {
                    Ok(value) => {
                        run.invocations[slot].state = InvocationState::Committed { result: value.clone() };
                        return Ok(Settled::Committed(value));
                    }
                    // It ran; only its result is unreadable. Still not a refusal.
                    Err(e) => DispatchFailure { outcome: DispatchOutcome::Uncertain, message: format!("result unreadable: {e}") },
                },
                Err(f) => f,
            };
            if failure.outcome == DispatchOutcome::NotCommitted {
                run.invocations[slot].state = InvocationState::NotCommitted { error: failure.message.clone() };
                if attempt < step.retry_count {
                    attempt += 1;
                    continuum_base::probe!(class = "recipe.step.retry", recipe = %run.purpose, step = idx as u64, command = %step.command, attempt = attempt as u64, error = %failure.message, "refused before dispatch — retrying per its declared retry_count");
                    continue;
                }
                return Ok(Settled::Refused(failure.message));
            }
            continuum_base::probe!(class = "recipe.step.uncertain", recipe = %run.purpose, step = idx as u64, command = %step.command, invocation = %id, error = %failure.message, "the outcome is unknown — the run stops for recovery rather than repeat it");
            run.trace.push(format!("[{idx}] {} UNCERTAIN {}", step.command, failure.message));
            run.invocations[slot].state = InvocationState::Uncertain { error: failure.message.clone() };
            run.phase = RunPhase::RecoveryRequired { invocation: id, reason: failure.message };
            self.persist(run).await?;
            return Ok(Settled::Uncertain);
        }
    }

    async fn fail(&self, run: &mut RecipeRunRecord, error: String) -> Result<(), String> {
        continuum_base::probe!(class = "recipe.step.failed", recipe = %run.purpose, error = %error, "run failed");
        run.trace.push(format!("FAILED {error}"));
        self.finish(run, RunPhase::Failed { error }).await
    }

    async fn finish(&self, run: &mut RecipeRunRecord, phase: RunPhase) -> Result<(), String> {
        run.phase = phase;
        self.persist(run).await
    }

    async fn persist(&self, run: &mut RecipeRunRecord) -> Result<(), String> {
        run.revision += 1;
        run.is_active = !run.phase.is_terminal();
        match &self.journal {
            Journal::Store(ledger) => ledger.write(run).await,
            Journal::Missing => Err("no recipe run store is open on this node".into()),
            #[cfg(any(test, feature = "test-fixtures"))]
            Journal::Unrecorded => Ok(()),
        }
    }

    async fn cancel_requested(&self, run: &RecipeRunRecord) -> Result<bool, String> {
        match &self.journal {
            Journal::Store(ledger) => ledger.cancel_requested(run.run_id()?).await,
            Journal::Missing => Err("no recipe run store is open on this node".into()),
            #[cfg(any(test, feature = "test-fixtures"))]
            Journal::Unrecorded => Ok(false),
        }
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
        PipelineExecutor::unrecorded(exec.clone())
            .with_state_sink(sink_dyn)
            .run("bench/saves", std::slice::from_ref(&post), args.clone())
            .await
            .expect("skips are not failures"); // JUSTIFIED: on_error skip by construction
        let saved = sink.0.lock().unwrap(); // JUSTIFIED: a test's own mutex
        assert_eq!(saved.len(), 1, "one save at the step boundary");
        assert_eq!(saved[0].get("cards"), Some(&Value::Array(vec![])), "the bound name is saved");
        assert!(saved[0].get("never_bound").is_none(), "an unbound name is skipped, not saved as null");
        drop(saved);
        let receipt = PipelineExecutor::unrecorded(exec)
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
        let receipt = PipelineExecutor::unrecorded(exec)
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
        let receipt = PipelineExecutor::unrecorded(exec)
            .run("campaign/applications", &pipeline, Value::Null)
            .await
            .expect("a held run is a clean receipt, not an error");
        assert_eq!(receipt.held_at, Some(0));
        assert_eq!(receipt.steps_run, 0);
        assert_eq!(receipt.trace, vec!["[0] browser/act HELD (approval: human)"]);
    }

    /// The durable run (PR-A): every test runs a real `recipe_runs` store in SQLite and
    /// a real dispatch chain into the one canned module.
    mod durable {
        use super::*;
        use crate::recipe::run_record::{InvocationState, RecipeRunRecord, RecipeRuns, RunLedger, RunPhase};
        use crate::runtime::command_executor::CommandExecutor;
        use crate::runtime::module_harness::CannedModule;
        use crate::runtime::CommandResult;
        use std::sync::{Mutex, OnceLock};

        async fn store() -> (Arc<RecipeRuns>, tempfile::TempDir) {
            let (adapter, dir) = crate::orm::store::fresh_adapter().await;
            (Arc::new(RecipeRuns::open(adapter).await.expect("open recipe_runs")), dir) // JUSTIFIED: a fresh temp db
        }

        fn hosting(canned: &Arc<CannedModule>) -> CommandExecutor {
            let registry = Arc::new(ModuleRegistry::new());
            registry.register(canned.clone());
            CommandExecutor::new(registry)
        }

        async fn run_row(runs: &RecipeRuns, run: &RecipeRunRecord) -> RecipeRunRecord {
            runs.get(run.run_id().unwrap()).await.unwrap().expect("the run's row exists") // JUSTIFIED: written by drive
        }

        /// what this catches: the write-ahead. An invocation must be on the run's row as
        /// `Dispatching` BEFORE its handler runs — after a crash that row is the only
        /// proof a dispatch may have happened — and its result, its binding and the cursor
        /// past it must land together once it returns.
        #[tokio::test]
        async fn an_invocation_is_written_before_its_dispatch_and_committed_with_its_binding() {
            let (runs, _dir) = store().await;
            let run_id: Arc<OnceLock<Uuid>> = Arc::default();
            let seen: Arc<Mutex<Option<InvocationState>>> = Arc::default();
            let (runs_in, id_in, seen_in) = (runs.clone(), run_id.clone(), seen.clone());
            let canned = Arc::new(CannedModule::scripted(move |_, _| {
                let (runs, id, seen) = (runs_in.clone(), id_in.clone(), seen_in.clone());
                Box::pin(async move {
                    let row = runs.get(*id.get().expect("set before drive")).await?.ok_or("no row before dispatch")?; // JUSTIFIED: set below
                    *seen.lock().unwrap() = Some(row.invocations[0].state.clone()); // JUSTIFIED: a test's own mutex
                    Ok(CommandResult::Json(serde_json::json!({ "n": 1 })))
                })
            }));
            let mut one = step("canned/do", None);
            one.output_to = Some("out".into());
            let pipeline = PipelineExecutor::new(Arc::new(hosting(&canned))).with_ledger(runs.clone());
            let mut run = pipeline.begin("t/write-ahead", std::slice::from_ref(&one), Value::Null, Vec::new());
            run_id.set(run.run_id().unwrap()).unwrap();
            pipeline.drive(&mut run).await.expect("the ledger accepts every write"); // JUSTIFIED: a fresh temp db

            assert_eq!(*seen.lock().unwrap(), Some(InvocationState::Dispatching { attempt: 0 }), "written before the handler ran"); // JUSTIFIED: a test's own mutex
            let row = run_row(&runs, &run).await;
            assert_eq!(row.phase, RunPhase::Completed);
            assert!(!row.is_active);
            assert_eq!(row.bindings.get("out"), Some(&serde_json::json!({ "n": 1 })));
            assert!(matches!(row.invocations[0].state, InvocationState::Committed { .. }));
            assert_eq!(row.cursor.step, 1, "the cursor moved past the step with its result");
        }

        /// what this catches: a handler failure retried or skipped as if nothing had
        /// happened. It reached a handler, so it may have acted: ONE dispatch despite
        /// `retry_count` and `on_error: skip`, the run in RecoveryRequired naming that
        /// invocation, nothing after it run, and the run still `Ok` (it is not over).
        #[tokio::test]
        async fn a_handler_error_is_never_retried_or_skipped_and_the_run_waits_for_recovery() {
            let (runs, _dir) = store().await;
            let canned = Arc::new(CannedModule::answering(Err("wrote half the cards, then failed".into())));
            let mut first = step("canned/do", None);
            first.retry_count = 2;
            first.on_error = OnError::Skip;
            let pipeline = PipelineExecutor::new(Arc::new(hosting(&canned))).with_ledger(runs.clone());
            let mut run = pipeline.begin("t/uncertain", &[first, step("canned/next", None)], Value::Null, Vec::new());
            pipeline.drive(&mut run).await.unwrap();

            assert_eq!(canned.calls(), 1, "never re-dispatched");
            let row = run_row(&runs, &run).await;
            let open = Invocation::id_for(run.run_id().unwrap(), 0, None);
            assert!(matches!(&row.phase, RunPhase::RecoveryRequired { invocation, .. } if *invocation == open), "{:?}", row.phase);
            assert!(row.is_active, "a run waiting on recovery is not over");
            assert_eq!(row.invocations.len(), 1, "the step after it never ran");
            assert!(matches!(row.invocations[0].state, InvocationState::Uncertain { .. }));
        }

        /// what this catches: a timeout treated as a refusal. The handler may still act
        /// after the wait gives up, so a timed-out invocation is Uncertain: one dispatch,
        /// not `1 + retry_count`.
        #[tokio::test]
        async fn a_timed_out_invocation_is_uncertain_and_never_retried() {
            let (runs, _dir) = store().await;
            let canned = Arc::new(CannedModule::scripted(|_, _| {
                Box::pin(async {
                    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                    Ok(CommandResult::Json(Value::Null))
                })
            }));
            let mut slow = step("canned/slow", None);
            slow.timeout_ms = Some(20);
            slow.retry_count = 1;
            let pipeline = PipelineExecutor::new(Arc::new(hosting(&canned))).with_ledger(runs.clone());
            let mut run = pipeline.begin("t/timeout", &[slow], Value::Null, Vec::new());
            pipeline.drive(&mut run).await.unwrap();

            assert_eq!(canned.calls(), 1);
            let row = run_row(&runs, &run).await;
            assert!(matches!(&row.phase, RunPhase::RecoveryRequired { reason, .. } if reason.contains("timed out")), "{:?}", row.phase);
        }

        /// what this catches: the other half of the rule — a dispatch the gate refused
        /// never reached a handler, so `retry_count` applies to it: three gate calls for
        /// `retry_count: 2`, zero handler calls, and `on_error: fail` ends the run Failed.
        #[tokio::test]
        async fn a_gate_refusal_is_retried_as_declared_and_reaches_no_handler() {
            let (runs, _dir) = store().await;
            let gate_calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let counted = gate_calls.clone();
            let refuse = crate::routing::ClosurePolicy::new("refuse-canned", move |_decision, _caller| {
                counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                crate::routing::Verdict::Forbidden { reason: crate::routing::ForbiddenReason::UnknownPeer }
            });
            let canned = Arc::new(CannedModule::answering(Ok(Value::Null)));
            let exec = Arc::new(hosting(&canned).with_policy(Arc::new(refuse)));
            let mut gated = step("canned/do", None);
            gated.retry_count = 2;
            let failed = PipelineExecutor::new(exec)
                .with_ledger(runs.clone())
                .run("t/refused", &[gated], Value::Null)
                .await
                .expect_err("a refusal with on_error: fail fails the run"); // JUSTIFIED: the invariant under test

            assert!(failed.contains("forbidden"), "{failed}");
            assert_eq!(gate_calls.load(std::sync::atomic::Ordering::SeqCst), 3);
            assert_eq!(canned.calls(), 0);
            let active = runs.active().await.unwrap();
            assert!(active.is_empty(), "a failed run is over: {active:?}");
        }

        /// what this catches: a restart leaving runs that look owned. The last process's
        /// `Running` rows are exposed at boot: one with an invocation still `Dispatching`
        /// becomes RecoveryRequired (its outcome is unknown), one stopped between
        /// invocations becomes Interrupted, and a held run is left exactly as it was.
        #[tokio::test]
        async fn boot_exposes_every_run_the_last_process_left_running() {
            let (runs, _dir) = store().await;
            let begin = |purpose: &str| {
                RecipeRunRecord::begin(Default::default(), purpose, vec![step("canned/do", None)], Value::Null, Default::default(), None)
            };
            let mut mid_dispatch = begin("t/mid-dispatch");
            let open = Invocation::id_for(mid_dispatch.run_id().unwrap(), 0, None);
            mid_dispatch.invocations.push(Invocation {
                id: open,
                step: 0,
                item: None,
                command: "canned/do".into(),
                params: Value::Null,
                state: InvocationState::Dispatching { attempt: 0 },
            });
            let between = begin("t/between");
            let mut held = begin("t/held");
            held.phase = RunPhase::Held { step: 0 };
            for run in [&mut mid_dispatch, &mut held] {
                run.revision = 1;
            }
            for run in [&mid_dispatch, &between, &held] {
                runs.write(run).await.unwrap();
            }

            assert_eq!(runs.expose_interrupted().await.unwrap(), (1, 1));
            assert!(matches!(run_row(&runs, &mid_dispatch).await.phase, RunPhase::RecoveryRequired { invocation, .. } if invocation == open));
            assert_eq!(run_row(&runs, &between).await.phase, RunPhase::Interrupted);
            assert_eq!(run_row(&runs, &held).await.phase, RunPhase::Held { step: 0 });
        }

        /// what this catches: cancel as a process-local flag, or as an interrupt. A cancel
        /// recorded while an `each` step runs stops the run at the NEXT item boundary
        /// (the item in flight finishes; no later item is dispatched), and it survives on
        /// the row; a run waiting on recovery stays there with the cancel recorded.
        #[tokio::test]
        async fn a_cancel_stops_the_run_at_its_next_boundary_and_never_settles_an_unknown() {
            let (runs, _dir) = store().await;
            let run_id: Arc<OnceLock<Uuid>> = Arc::default();
            let (runs_in, id_in) = (runs.clone(), run_id.clone());
            let canned = Arc::new(CannedModule::scripted(move |_, _| {
                let (runs, id) = (runs_in.clone(), id_in.clone());
                Box::pin(async move {
                    runs.request_cancel(*id.get().expect("set before drive")).await?; // JUSTIFIED: set below
                    Ok(CommandResult::Json(Value::Null))
                })
            }));
            let mut fan = step("canned/do", None);
            fan.each = Some("$args.rows".into());
            let pipeline = PipelineExecutor::new(Arc::new(hosting(&canned))).with_ledger(runs.clone());
            let mut run = pipeline.begin("t/cancel", &[fan], serde_json::json!({ "rows": [1, 2, 3] }), Vec::new());
            run_id.set(run.run_id().unwrap()).unwrap();
            pipeline.drive(&mut run).await.unwrap();

            assert_eq!(canned.calls(), 1, "the item in flight finished; no later item was dispatched");
            let row = run_row(&runs, &run).await;
            assert_eq!(row.phase, RunPhase::Cancelled);
            assert!(row.cancel_requested, "the owner's writes never clear a recorded cancel");

            let failing = Arc::new(CannedModule::answering(Err("unknown".into())));
            let pipeline = PipelineExecutor::new(Arc::new(hosting(&failing))).with_ledger(runs.clone());
            let mut stuck = pipeline.begin("t/stuck", &[step("canned/do", None)], Value::Null, Vec::new());
            pipeline.drive(&mut stuck).await.unwrap();
            let stuck_id = stuck.run_id().unwrap();
            runs.request_cancel(stuck_id).await.unwrap();
            let after = runs.settle_cancel_if_idle(stuck_id).await.unwrap();
            assert!(matches!(after.phase, RunPhase::RecoveryRequired { .. }), "{:?}", after.phase);
            assert!(after.cancel_requested);
        }

        /// A bundle sink that refuses its first save and records every save after.
        struct FlakySink(Mutex<Vec<serde_json::Map<String, Value>>>, std::sync::atomic::AtomicBool);
        #[async_trait::async_trait]
        impl crate::experience::activity_state::StepStateSink for FlakySink {
            async fn save(&self, patch: serde_json::Map<String, Value>) -> Result<(), String> {
                if !self.1.swap(true, std::sync::atomic::Ordering::SeqCst) {
                    return Err("the wall refused this write".into());
                }
                self.0.lock().unwrap().push(patch); // JUSTIFIED: a test's own mutex
                Ok(())
            }
        }

        /// what this catches (Astra, #5008 gap 2): a failed room projection lost to the
        /// next step. Step 0's `saves` write is refused; step 1's write must carry
        /// step 0's patch with its own, and nothing stays pending after it lands.
        #[tokio::test]
        async fn a_refused_projection_is_carried_into_the_next_steps_save() {
            let (runs, _dir) = store().await;
            let canned = Arc::new(CannedModule::answering(Ok(serde_json::json!(1))));
            let sink = Arc::new(FlakySink(Mutex::new(Vec::new()), std::sync::atomic::AtomicBool::new(false)));
            let sink_dyn: Arc<dyn crate::experience::activity_state::StepStateSink> = sink.clone();
            let mut first = step("canned/a", None);
            first.output_to = Some("a".into());
            first.saves = vec!["a".into()];
            let mut second = step("canned/b", None);
            second.output_to = Some("b".into());
            second.saves = vec!["b".into()];
            let pipeline = PipelineExecutor::new(Arc::new(hosting(&canned))).with_ledger(runs.clone()).with_state_sink(sink_dyn);
            let mut run = pipeline.begin("t/projection", &[first, second], Value::Null, Vec::new());
            pipeline.drive(&mut run).await.unwrap();

            let saved = sink.0.lock().unwrap().clone(); // JUSTIFIED: a test's own mutex
            assert_eq!(saved.len(), 1, "only the second write landed");
            assert_eq!(saved[0].get("a"), Some(&serde_json::json!(1)), "step 0's refused patch was carried forward");
            assert_eq!(saved[0].get("b"), Some(&serde_json::json!(1)));
            assert!(run_row(&runs, &run).await.pending_saves.is_none(), "nothing pending once it landed");
        }

        /// A ledger that refuses every write once an invocation has committed — the
        /// moment between a handler's effect and its record.
        struct LosesTheResult(Arc<RecipeRuns>);
        #[async_trait::async_trait]
        impl RunLedger for LosesTheResult {
            async fn write(&self, run: &RecipeRunRecord) -> Result<(), String> {
                if run.invocations.iter().any(|inv| matches!(inv.state, InvocationState::Committed { .. })) {
                    return Err("disk full".into());
                }
                self.0.write(run).await
            }
            async fn cancel_requested(&self, run_id: Uuid) -> Result<bool, String> {
                self.0.cancel_requested(run_id).await
            }
        }

        /// what this catches (Astra, #5008 gap 3): a handler that committed while its
        /// result could not be recorded, reported as a FAILED run (which archives an
        /// activity's room). One dispatch; the run answers `Ok` in RecoveryRequired
        /// naming that invocation; the row's last durable state is `Dispatching`, and
        /// the boot pass exposes it as RecoveryRequired too.
        #[tokio::test]
        async fn a_committed_handler_whose_result_cannot_be_recorded_waits_for_recovery() {
            let (runs, _dir) = store().await;
            let canned = Arc::new(CannedModule::answering(Ok(serde_json::json!({ "posted": 12 }))));
            let receipt = PipelineExecutor::new(Arc::new(hosting(&canned)))
                .with_ledger(Arc::new(LosesTheResult(runs.clone())))
                .run("t/lost-result", &[step("canned/post", None), step("canned/next", None)], Value::Null)
                .await
                .expect("an unknown outcome is not a failed run"); // JUSTIFIED: the invariant under test

            assert_eq!(canned.calls(), 1);
            let open = Invocation::id_for(receipt.run_id, 0, None);
            assert!(matches!(&receipt.phase, RunPhase::RecoveryRequired { invocation, .. } if *invocation == open), "{:?}", receipt.phase);
            let row = runs.get(receipt.run_id).await.unwrap().expect("the run's first writes landed"); // JUSTIFIED: writes before the commit succeed
            assert!(matches!(row.invocations[0].state, InvocationState::Dispatching { .. }), "{:?}", row.invocations[0].state);
            assert_eq!(runs.expose_interrupted().await.unwrap(), (1, 0));
        }

        /// what this catches (Astra, #5008 gap 4): a node with no run store dispatching
        /// anyway. No recipe module in the registry: the run is refused, and no command
        /// reaches a handler.
        #[tokio::test]
        async fn a_run_with_no_store_is_refused_before_any_dispatch() {
            let canned = Arc::new(CannedModule::answering(Ok(Value::Null)));
            let refused = PipelineExecutor::new(Arc::new(hosting(&canned)))
                .run("t/no-store", &[step("canned/do", None)], Value::Null)
                .await
                .expect_err("no store, no run"); // JUSTIFIED: the invariant under test
            assert!(refused.contains("no recipe run store"), "{refused}");
            assert_eq!(canned.calls(), 0);
        }
    }
}
