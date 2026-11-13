//! The recipe module: owner of the node's recipe runs.
//!
//! It holds the one [`RecipeRuns`] store (the `recipe_runs` ORM collection) and
//! registers the verbs that start, read and cancel runs. At boot it exposes every
//! run the last process left `Running` (see [`RecipeRuns::expose_interrupted`]):
//! nothing is re-dispatched, and nothing is left looking as if a task still owned it.
//!
//! The pipeline executor finds this store through the module registry
//! ([`RecipeModule::runs_in`]), so every caller that already hands it the substrate
//! executor — activity birth, benchmark dispatch, `recipe/run` — gets a durable run
//! without carrying a second handle.

use crate::recipe::run_record::RecipeRuns;
use crate::runtime::command_executor::CommandExecutor;
use crate::runtime::{ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use crate::runtime::CommandResult;
use crate::sdk_codegen::DynCommand;
use async_trait::async_trait;
use std::any::Any;
use std::sync::Arc;

use crate::recipe::runs_owner::{RecipeModule, MODULE_NAME};

/// The `ServiceModule` face of the mind's [`RecipeModule`]: its verbs and its boot are the
/// surface's (they need the data module and the recipe commands), the store is the mind's.
/// `as_any` is the inner module, so [`RecipeModule::runs_in`] finds it through the registry.
pub struct RecipeService(pub RecipeModule);

#[async_trait]
impl ServiceModule for RecipeService {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: MODULE_NAME,
            priority: ModulePriority::Normal,
            command_prefixes: &[],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    async fn initialize(&self, ctx: &ModuleContext) -> Result<(), String> {
        let runs = RecipeRuns::open(ctx.storage("recipe")?.adapter("main").await?).await?;
        let (recovery, interrupted) = runs.expose_interrupted().await?;
        continuum_base::probe!(
            class = "recipe.boot.exposed",
            recovery_required = recovery as u64,
            interrupted = interrupted as u64,
            "recipe runs the last process left Running"
        );
        self.0.runs.install(Arc::new(runs));
        Ok(())
    }

    async fn handle_command(&self, command: &str, _params: serde_json::Value) -> Result<CommandResult, String> {
        Err(format!("recipe: `{command}` is not a recipe verb (they register as typed commands)"))
    }

    fn commands(&self) -> Vec<Arc<dyn DynCommand>> {
        crate::commands::recipe_run::command_objects(self.0.executor_slot.clone(), self.0.runs.clone())
    }

    fn install_executor(&self, executor: Arc<CommandExecutor>) {
        self.0.executor_slot.install(executor);
    }

    fn as_any(&self) -> &dyn Any {
        &self.0
    }
}
