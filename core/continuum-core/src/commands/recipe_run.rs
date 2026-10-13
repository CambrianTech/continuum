//! `recipe/run` — execute an authored recipe's pipeline by purpose.
//!
//! The recipe is the SAME activity recipe a room spawns from (embedded floor +
//! `<continuum_root>/recipes` overlay), resolved through the node's one
//! [`ExperienceSource`](crate::experience::source::ExperienceSource) by purpose;
//! its `pipeline[]` steps are command invocations walked by
//! [`crate::recipe::PipelineExecutor`]. Until S2 this read a second store — rows in
//! a `recipes` data collection — of which zero ever existed on any node.
//!
//! Every step acts as THE CALLER of `recipe/run` (`Ctx::caller`), the same way an
//! activity birth runs its pipeline as the spawner: each step is gated against
//! AuthPolicy as that identity, never with the substrate's own trust. A caller of
//! `None` is the substrate's own local code. Access stays Privileged; widening it to
//! personas is a separate decision.

use crate::runtime::command_executor::CommandExecutor;
use crate::runtime::LateBound;
use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use async_trait::async_trait;
use std::sync::Arc;
use ts_rs::TS;

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

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/recipe/RecipeRunResult.ts")]
#[serde(rename_all = "camelCase")]
pub struct RecipeRunResult {
    pub recipe: String,
    pub steps_run: u32,
    pub steps_skipped: u32,
    /// Step-indexed one-line outcomes — the run's readable trace.
    pub trace: Vec<String>,
    /// Every `outputTo` binding's final value — the run's result, shaped by
    /// the recipe itself.
    #[ts(type = "unknown")]
    pub bindings: serde_json::Value,
    /// The step the run HELD at for a human's approval, if any (S3).
    #[ts(optional)]
    pub held_at: Option<u32>,
}

pub struct RecipeRun {
    pub executor_slot: Arc<LateBound<CommandExecutor>>,
}

#[async_trait]
impl ActionCommand for RecipeRun {


    const NAME: &'static str = "recipe/run";
    const ACCESS: AccessLevel = AccessLevel::Privileged;
    const DESCRIPTION: &'static str =
        "Run an authored recipe's pipeline by purpose (the same catalogue \
         `activity/recipes` lists and `activity/spawn` accepts). Pass `args` for the \
         recipe's `$args.*` references.";
    type Params = RecipeRunParams;
    type Output = RecipeRunResult;

    async fn run(&self, ctx: &Ctx, p: RecipeRunParams) -> Result<RecipeRunResult, CommandError> {
        let executor = self
            .executor_slot
            .get()
            .ok_or_else(|| CommandError::Internal("recipe/run: substrate executor not yet bound".into()))?;

        // One registry for what a room IS and what an activity DOES (S2). Absence
        // of the source is a boot-order fact, not a recipe fact — say which.
        let source = crate::experience::source::node_experience_source().ok_or_else(|| {
            CommandError::Internal(
                "recipe/run: the node's experience source is not installed — the boot path \
                 that builds the room-purpose index has not run"
                    .into(),
            )
        })?;
        let recipe = source.recipe_for_purpose(&p.name).ok_or_else(|| {
            CommandError::Invalid(format!(
                "no recipe with purpose `{}` — `activity/recipes` lists the catalogue; author \
                 one as a JSON file in the recipes overlay directory (no code, no deploy)",
                p.name
            ))
        })?;

        let receipt = crate::recipe::PipelineExecutor::new(executor.clone())
            .with_caller(ctx.caller.clone())
            .run(&recipe.purpose, &recipe.pipeline, p.args.unwrap_or(serde_json::Value::Null)) // unwrap_or: no args = null, steps referencing $args.* then fail loud by name
            .await
            .map_err(CommandError::Internal)?;

        Ok(RecipeRunResult {
            recipe: receipt.recipe,
            steps_run: receipt.steps_run,
            steps_skipped: receipt.steps_skipped,
            trace: receipt.trace,
            bindings: serde_json::Value::Object(receipt.bindings),
            held_at: receipt.held_at,
        })
    }
}

crate::register_command!(RecipeRun);
