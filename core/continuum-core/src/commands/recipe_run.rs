//! `recipe/*` — start, read and cancel durable recipe runs.
//!
//! A run is a [`RecipeRunRecord`] in the node's `recipe_runs` collection (see
//! [`crate::recipe::run_record`]). `recipe/run` resolves the recipe by PURPOSE
//! through the node's one [`ExperienceSource`](crate::experience::source::ExperienceSource)
//! (the same catalogue `activity/spawn` reads), snapshots its steps into a new
//! record, and drives it. Every answer is the same envelope — the run's id, its
//! phase, and its receipt — whether the run completed, held for a human, or stopped
//! on an invocation whose outcome is unknown.
//!
//! Every step acts as THE CALLER of `recipe/run` (`Ctx::caller`), gated against
//! AuthPolicy as that identity, never with the substrate's own trust. A caller of
//! `None` is the substrate's own local code.
//!
//! WHO MAY SEE A RUN. Each verb declares `AccessLevel::Privileged`, but that level
//! is not enforced by the gate today (see `sdk_codegen::AccessLevel`); only
//! AuthPolicy restricts who calls these verbs. So `recipe/status`, `recipe/list` and
//! `recipe/cancel` check each run themselves ([`may_access`]): the caller that
//! started it, the substrate's own code, and the node's operator and agent
//! identities. Anyone else is told the run does not exist, so one persona can
//! neither read another's bindings nor cancel her run (Astra review of #5008, gap 1).

use crate::recipe::run_record::{RecipeRunRecord, RecipeRuns, RunOrigin, RunPhase};
use crate::recipe::{PipelineExecutor, RecipeRunReceipt};
use crate::runtime::command_executor::CommandExecutor;
use crate::runtime::LateBound;
use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx, DynCommand};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use ts_rs::TS;
use uuid::Uuid;

/// The verbs over one node's run store.
pub fn command_objects(
    executor_slot: Arc<LateBound<CommandExecutor>>,
    runs: Arc<LateBound<RecipeRuns>>,
) -> Vec<Arc<dyn DynCommand>> {
    vec![
        Arc::new(RecipeRun { executor_slot, runs: runs.clone() }),
        Arc::new(RecipeStatus { runs: runs.clone() }),
        Arc::new(RecipeList { runs: runs.clone() }),
        Arc::new(RecipeCancel { runs }),
    ]
}

/// May this caller read or cancel `run`? The caller that started it, the
/// substrate's own code (`None`), and the node's local operator and agent
/// identities. Nobody else.
fn may_access(ctx: &Ctx, run: &RecipeRunRecord) -> bool {
    let Some(caller) = &ctx.caller else { return true };
    let peer = caller.peer_id.as_uuid();
    crate::persona::operator_peer::is_local_identity(peer) || run.caller.as_ref().is_some_and(|c| c.peer_id == peer)
}

/// The run `run_id` names, if this caller may see it. A run she may not see is
/// "not found", exactly like one that never existed.
async fn visible_run(runs: &RecipeRuns, ctx: &Ctx, run_id: Uuid) -> Result<RecipeRunRecord, CommandError> {
    runs.get(run_id)
        .await
        .map_err(CommandError::Internal)?
        .filter(|run| may_access(ctx, run))
        .ok_or_else(|| CommandError::NotFound(format!("no recipe run {run_id}")))
}

fn runs_of(slot: &LateBound<RecipeRuns>) -> Result<&Arc<RecipeRuns>, CommandError> {
    slot.require().map_err(|e| CommandError::Internal(format!("recipe: the run store is not open yet ({e})")))
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/recipe/RecipeRunParams.ts")]
#[serde(rename_all = "camelCase")]
pub struct RecipeRunParams {
    /// The recipe's PURPOSE (`activity/recipes` lists them) — the same string
    /// `activity/spawn --recipe` accepts.
    pub name: String,
    /// Invocation arguments, readable by steps as `$args.*`. The recipe's own
    /// pipeline decides which are required — an unresolved reference fails
    /// loudly, naming itself.
    #[serde(default)]
    #[ts(optional, type = "unknown")]
    pub args: Option<serde_json::Value>,
}

/// Every `recipe/*` answer about one run: its id, where it is, and what it did.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/recipe/RecipeRunResult.ts")]
#[serde(rename_all = "camelCase")]
pub struct RecipeRunResult {
    #[ts(type = "string")]
    pub run_id: Uuid,
    pub recipe: String,
    pub phase: RunPhase,
    pub cancel_requested: bool,
    pub steps_run: u32,
    pub steps_skipped: u32,
    /// Step-indexed one-line outcomes — the run's readable trace.
    pub trace: Vec<String>,
    /// Every `outputTo` binding so far — the run's result, shaped by the recipe.
    #[ts(type = "unknown")]
    pub bindings: serde_json::Value,
}

impl RecipeRunResult {
    fn of(run: &RecipeRunRecord) -> Result<Self, CommandError> {
        Ok(Self {
            run_id: run.run_id().map_err(CommandError::Internal)?,
            recipe: run.purpose.clone(),
            phase: run.phase.clone(),
            cancel_requested: run.cancel_requested,
            steps_run: run.steps_run,
            steps_skipped: run.steps_skipped,
            trace: run.trace.clone(),
            bindings: serde_json::Value::Object(run.bindings.clone()),
        })
    }

    fn of_receipt(receipt: RecipeRunReceipt) -> Self {
        Self {
            run_id: receipt.run_id,
            recipe: receipt.recipe,
            phase: receipt.phase,
            cancel_requested: false,
            steps_run: receipt.steps_run,
            steps_skipped: receipt.steps_skipped,
            trace: receipt.trace,
            bindings: serde_json::Value::Object(receipt.bindings),
        }
    }
}

pub struct RecipeRun {
    executor_slot: Arc<LateBound<CommandExecutor>>,
    runs: Arc<LateBound<RecipeRuns>>,
}

#[async_trait]
impl ActionCommand for RecipeRun {
    const NAME: &'static str = "recipe/run";
    const ACCESS: AccessLevel = AccessLevel::Privileged;
    const DESCRIPTION: &'static str =
        "Run an authored recipe's pipeline by purpose (the same catalogue \
         `activity/recipes` lists and `activity/spawn` accepts). Pass `args` for the \
         recipe's `$args.*` references. The answer names the run (`runId`), so \
         `recipe/status` can read it again later.";
    type Params = RecipeRunParams;
    type Output = RecipeRunResult;

    async fn run(&self, ctx: &Ctx, p: RecipeRunParams) -> Result<RecipeRunResult, CommandError> {
        let executor = self
            .executor_slot
            .get()
            .ok_or_else(|| CommandError::Internal("recipe/run: substrate executor not yet bound".into()))?;
        let runs = runs_of(&self.runs)?.clone();

        // One registry for what a room IS and what an activity DOES (S2). Absence
        // of the source is a boot-order fact, not a recipe fact — say which.
        let source = crate::experience::source::node_experience_source().ok_or_else(|| {
            CommandError::Internal(
                "recipe/run: the node's experience source is not installed — the boot path \
                 that builds the room-purpose index has not run"
                    .into(),
            )
        })?;
        run_recipe(source.as_ref(), executor.clone(), runs, ctx, p).await
    }
}

/// The command's body over an explicit recipe source, so a test can drive the real
/// boundary without installing the node's process-wide one.
async fn run_recipe(
    source: &dyn crate::experience::source::ExperienceSource,
    executor: Arc<CommandExecutor>,
    runs: Arc<RecipeRuns>,
    ctx: &Ctx,
    p: RecipeRunParams,
) -> Result<RecipeRunResult, CommandError> {
    let recipe = source.recipe_for_purpose(&p.name).ok_or_else(|| {
        CommandError::Invalid(format!(
            "no recipe with purpose `{}` — `activity/recipes` lists the catalogue; author \
             one as a JSON file in the recipes overlay directory (no code, no deploy)",
            p.name
        ))
    })?;

    let receipt = PipelineExecutor::new(executor)
        .with_ledger(runs)
        .with_caller(ctx.caller.clone())
        .with_origin(RunOrigin { recipe: Some((recipe.id.0, recipe.version)), room: None })
        .run(&recipe.purpose, &recipe.pipeline, p.args.unwrap_or(serde_json::Value::Null)) // unwrap_or: no args = null, steps referencing $args.* then fail loud by name
        .await
        .map_err(CommandError::Internal)?;
    Ok(RecipeRunResult::of_receipt(receipt))
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/recipe/RecipeRunRef.ts")]
#[serde(rename_all = "camelCase")]
pub struct RecipeRunRef {
    /// The `runId` a `recipe/run` (or an activity birth) answered with.
    #[ts(type = "string")]
    pub run_id: Uuid,
}

pub struct RecipeStatus {
    runs: Arc<LateBound<RecipeRuns>>,
}

#[async_trait]
impl ActionCommand for RecipeStatus {
    const NAME: &'static str = "recipe/status";
    const ACCESS: AccessLevel = AccessLevel::Privileged;
    const DESCRIPTION: &'static str = "Read one recipe run by `runId`: its phase, trace and bindings.";
    type Params = RecipeRunRef;
    type Output = RecipeRunResult;

    async fn run(&self, ctx: &Ctx, p: RecipeRunRef) -> Result<RecipeRunResult, CommandError> {
        let run = visible_run(runs_of(&self.runs)?, ctx, p.run_id).await?;
        RecipeRunResult::of(&run)
    }
}

/// `recipe/list` takes no input.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/recipe/RecipeListParams.ts")]
#[serde(rename_all = "camelCase")]
pub struct RecipeListParams {}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/recipe/RecipeListResult.ts")]
#[serde(rename_all = "camelCase")]
pub struct RecipeListResult {
    /// Every run that is not over: running, held, interrupted, or waiting on recovery.
    pub active: Vec<RecipeRunResult>,
}

pub struct RecipeList {
    runs: Arc<LateBound<RecipeRuns>>,
}

#[async_trait]
impl ActionCommand for RecipeList {
    const NAME: &'static str = "recipe/list";
    const ACCESS: AccessLevel = AccessLevel::Privileged;
    const DESCRIPTION: &'static str =
        "List every recipe run that is not over: running, held for approval, interrupted by a restart, or waiting on recovery.";
    type Params = RecipeListParams;
    type Output = RecipeListResult;

    async fn run(&self, ctx: &Ctx, _p: RecipeListParams) -> Result<RecipeListResult, CommandError> {
        let active = runs_of(&self.runs)?.active().await.map_err(CommandError::Internal)?;
        Ok(RecipeListResult {
            active: active
                .iter()
                .filter(|run| may_access(ctx, run))
                .map(RecipeRunResult::of)
                .collect::<Result<_, _>>()?,
        })
    }
}

pub struct RecipeCancel {
    runs: Arc<LateBound<RecipeRuns>>,
}

#[async_trait]
impl ActionCommand for RecipeCancel {
    const NAME: &'static str = "recipe/cancel";
    const ACCESS: AccessLevel = AccessLevel::Privileged;
    const DESCRIPTION: &'static str =
        "Ask a recipe run to stop. A running run stops at its next step or item boundary; \
         a dispatch already sent is never undone. A run waiting on recovery keeps waiting, \
         with the cancel recorded, because its open invocation's outcome is still unknown.";
    type Params = RecipeRunRef;
    type Output = RecipeRunResult;

    async fn run(&self, ctx: &Ctx, p: RecipeRunRef) -> Result<RecipeRunResult, CommandError> {
        let runs = runs_of(&self.runs)?;
        let run = visible_run(runs, ctx, p.run_id).await?;
        if run.phase.is_terminal() {
            return Err(CommandError::Invalid(format!("recipe run {} is already over ({:?})", p.run_id, run.phase)));
        }
        runs.request_cancel(p.run_id).await.map_err(CommandError::Internal)?;
        let run = runs
            .settle_cancel_if_idle(p.run_id)
            .await
            .map_err(CommandError::Internal)?;
        RecipeRunResult::of(&run)
    }
}

crate::register_command!(RecipeRun);
crate::register_command!(RecipeStatus);
crate::register_command!(RecipeList);
crate::register_command!(RecipeCancel);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::{CallerIdentity, ClosurePolicy, ForbiddenReason, Verdict};
    use std::sync::Mutex;

    /// what this catches: `recipe/run` dispatching its steps with the substrate's own
    /// trust instead of as its caller. Until 2026-10-13 it ignored `Ctx` and every
    /// step reached the AuthPolicy gate as `None`. The policy here records the caller
    /// each dispatch presents and refuses it, so no step does real work; the shipped
    /// `benchmark/round` recipe is driven through the command's own body. Deleting
    /// `with_caller` makes the persona run record `None` and fails this.
    #[tokio::test]
    async fn recipe_run_presents_its_caller_at_every_steps_gate() {
        let seen: Arc<Mutex<Vec<Option<CallerIdentity>>>> = Arc::new(Mutex::new(Vec::new()));
        let record = seen.clone();
        let policy = ClosurePolicy::new("record-and-refuse", move |_decision, caller| {
            record.lock().unwrap().push(caller.cloned()); // JUSTIFIED: a test's own mutex
            Verdict::Forbidden { reason: ForbiddenReason::UnknownPeer }
        });
        let executor = Arc::new(
            CommandExecutor::new(Arc::new(crate::runtime::registry::ModuleRegistry::new()))
                .with_policy(Arc::new(policy)),
        );
        let source = crate::experience::source::RecipeExperienceSource::builtins(
            crate::experience::room_purpose::default_source(),
        );
        let (adapter, _db) = crate::orm::store::fresh_adapter().await;
        let runs = Arc::new(RecipeRuns::open(adapter).await.expect("open recipe_runs")); // JUSTIFIED: a fresh temp db
        let params = || RecipeRunParams {
            name: "benchmark/round".into(),
            args: Some(serde_json::json!({
                "suite": "s", "instances": [], "sample": 0, "seed": 0, "limit": 0, "language": "rust"
            })),
        };
        let persona = CallerIdentity::local_persona(crate::identity::PeerId::from_u128(7));

        for caller in [Some(persona), None] {
            seen.lock().unwrap().clear(); // JUSTIFIED: a test's own mutex
            let ctx = Ctx { caller: caller.clone(), ..Ctx::default() };
            let refused = run_recipe(&source, executor.clone(), runs.clone(), &ctx, params())
                .await
                .expect_err("the policy refuses every step"); // JUSTIFIED: the invariant under test
            assert!(matches!(&refused, CommandError::Internal(e) if e.contains("forbidden")), "{refused:?}");
            let seen = seen.lock().unwrap(); // JUSTIFIED: a test's own mutex
            assert!(!seen.is_empty(), "a step reached the gate");
            assert!(seen.iter().all(|c| c == &caller), "every gate saw {caller:?}, got {seen:?}");
        }
    }

    /// what this catches (Astra, #5008 gap 1): one persona reading another's run — its
    /// bindings — or cancelling it. Run verbs are not restricted by the gate, so each
    /// checks the run: its own caller sees and cancels it; another persona is told it
    /// does not exist, and her list omits it.
    #[tokio::test]
    async fn a_run_answers_only_to_the_caller_that_started_it() {
        use crate::recipe::run_record::{CallerRecord, RecipeRunRecord, RunLedger, RunPhase};
        let (adapter, _db) = crate::orm::store::fresh_adapter().await;
        let slot = Arc::new(LateBound::new("test::runs"));
        slot.install(Arc::new(RecipeRuns::open(adapter).await.expect("open recipe_runs"))); // JUSTIFIED: a fresh temp db
        let persona = |n: u128| CallerIdentity::local_persona(crate::identity::PeerId::from_u128(n));
        let (alice, bob) = (persona(41), persona(42));
        let mut run = RecipeRunRecord::begin(
            Default::default(),
            "t/private",
            Vec::new(),
            serde_json::Value::Null,
            Default::default(),
            Some(CallerRecord::of(&alice)),
        );
        run.phase = RunPhase::Held { step: 0 };
        run.bindings.insert("secret".into(), serde_json::json!("alice's"));
        slot.require().unwrap().write(&run).await.unwrap();
        let id = run.run_id().unwrap();
        let as_ = |who: &CallerIdentity| Ctx { caller: Some(who.clone()), ..Ctx::default() };
        let (status, list, cancel) = (
            RecipeStatus { runs: slot.clone() },
            RecipeList { runs: slot.clone() },
            RecipeCancel { runs: slot.clone() },
        );

        assert!(matches!(status.run(&as_(&bob), RecipeRunRef { run_id: id }).await, Err(CommandError::NotFound(_))));
        assert!(matches!(cancel.run(&as_(&bob), RecipeRunRef { run_id: id }).await, Err(CommandError::NotFound(_))));
        assert!(list.run(&as_(&bob), RecipeListParams {}).await.unwrap().active.is_empty());

        let seen = status.run(&as_(&alice), RecipeRunRef { run_id: id }).await.expect("her own run");
        assert_eq!(seen.bindings["secret"], "alice's");
        assert_eq!(list.run(&as_(&alice), RecipeListParams {}).await.unwrap().active.len(), 1);
        let cancelled = cancel.run(&as_(&alice), RecipeRunRef { run_id: id }).await.expect("she cancels her own held run");
        assert_eq!(cancelled.phase, RunPhase::Cancelled);
    }
}
