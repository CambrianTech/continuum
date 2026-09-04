//! `recipe/run` — execute a stored recipe pipeline by name.
//!
//! The recipe is a ROW in the `recipes` collection (authored via
//! `data/create`, listed via `data/list`); its `pipeline[]` steps are command
//! invocations walked by [`crate::recipe::PipelineExecutor`]. Loading goes
//! through `data/list` AS A COMMAND (the universal primitive, same as the
//! benchmark recipe path) — never a private store read.
//!
//! Access is Privileged: a pipeline dispatches arbitrary commands with the
//! substrate's own trust. Per-caller identity threading (so a persona-invoked
//! recipe runs AS the persona against AuthPolicy) is the designed follow-up in
//! RECIPE-EXECUTION-RUNTIME.md — until it lands, only the operator and
//! substrate code may run pipelines.

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
    /// The recipe row's `name` in the `recipes` collection.
    pub name: String,
    /// Invocation arguments, readable by steps as `$args.*`. The recipe's own
    /// pipeline decides which are required — an unresolved reference fails
    /// loudly, naming itself.
    #[serde(default)]
    #[ts(optional, type = "unknown")]
    #[schemars(skip)]
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
}

pub struct RecipeRun {
    pub executor_slot: Arc<LateBound<CommandExecutor>>,
}

#[async_trait]
impl ActionCommand for RecipeRun {


    const NAME: &'static str = "recipe/run";
    const ACCESS: AccessLevel = AccessLevel::Privileged;
    const DESCRIPTION: &'static str =
        "Run a stored recipe: a pipeline of command invocations authored as data \
         (rows in the `recipes` collection). Pass `args` for the recipe's `$args.*` \
         references.";
    type Params = RecipeRunParams;
    type Output = RecipeRunResult;

    async fn run(&self, _ctx: &Ctx, p: RecipeRunParams) -> Result<RecipeRunResult, CommandError> {
        let executor = self
            .executor_slot
            .get()
            .ok_or_else(|| CommandError::Internal("recipe/run: substrate executor not yet bound".into()))?;

        // Load the row through the data layer — the same envelope discipline
        // the benchmark recipe loader established.
        let listed = executor
            .execute(
                "data/list",
                serde_json::json!({
                    "collection": "recipes",
                    "filter": {"name": p.name},
                    "limit": 1
                }),
            )
            .await
            .map_err(CommandError::Internal)?
            .to_json_value()
            .map_err(CommandError::Internal)?;
        let row = listed
            .get("items")
            .and_then(|v| v.as_array())
            .and_then(|items| items.first())
            .and_then(|item| item.get("data"))
            .cloned()
            .ok_or_else(|| {
                CommandError::Invalid(format!(
                    "recipe `{}` not found — author it with data/create --collection=recipes \
                     (see docs/architecture/RECIPE-EXECUTION-RUNTIME.md for the pipeline shape)",
                    p.name
                ))
            })?;
        let recipe: crate::recipe::Recipe = serde_json::from_value(row) // boundary: a data-layer row crossing into a typed Recipe
            .map_err(|e| CommandError::Invalid(format!("recipe `{}` row malformed: {e}", p.name)))?;

        let receipt = crate::recipe::PipelineExecutor::new(executor.clone())
            .run(&recipe, p.args.unwrap_or(serde_json::Value::Null)) // unwrap_or: no args = null, steps referencing $args.* then fail loud by name
            .await
            .map_err(CommandError::Internal)?;

        Ok(RecipeRunResult {
            recipe: receipt.recipe,
            steps_run: receipt.steps_run,
            steps_skipped: receipt.steps_skipped,
            trace: receipt.trace,
            bindings: serde_json::Value::Object(receipt.bindings),
        })
    }
}

crate::register_command!(RecipeRun);
