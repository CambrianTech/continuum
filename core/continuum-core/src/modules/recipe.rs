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
use crate::runtime::registry::ModuleRegistry;
use crate::runtime::{LateBound, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use crate::runtime::CommandResult;
use crate::sdk_codegen::DynCommand;
use async_trait::async_trait;
use std::any::Any;
use std::sync::Arc;

pub const MODULE_NAME: &str = "recipe";

pub struct RecipeModule {
    runs: Arc<LateBound<RecipeRuns>>,
    executor_slot: Arc<LateBound<CommandExecutor>>,
}

impl RecipeModule {
    pub fn new() -> Self {
        Self {
            runs: Arc::new(LateBound::new("recipe::runs")),
            executor_slot: Arc::new(LateBound::new("recipe::executor")),
        }
    }

    /// The node's run store, if this registry holds an initialized recipe module.
    /// `None` in a bare registry (a unit test's): the executor then runs without
    /// a record and probes that it did.
    pub fn runs_in(registry: &ModuleRegistry) -> Option<Arc<RecipeRuns>> {
        let module = registry.get_by_name(MODULE_NAME)?;
        module.as_any().downcast_ref::<RecipeModule>()?.runs.cloned()
    }
}

#[async_trait]
impl ServiceModule for RecipeModule {
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
        let module = ctx
            .registry
            .get_by_name("data")
            .ok_or_else(|| "recipe requires the data module".to_string())?;
        let data = module
            .as_any()
            .downcast_ref::<crate::modules::data::DataModule>()
            .ok_or_else(|| "recipe: data module type mismatch".to_string())?;
        let runs = RecipeRuns::open(data.state.get_adapter("main").await?).await?;
        let (recovery, interrupted) = runs.expose_interrupted().await?;
        crate::probe!(
            class = "recipe.boot.exposed",
            recovery_required = recovery as u64,
            interrupted = interrupted as u64,
            "recipe runs the last process left Running"
        );
        self.runs.install(Arc::new(runs));
        Ok(())
    }

    async fn handle_command(&self, command: &str, _params: serde_json::Value) -> Result<CommandResult, String> {
        Err(format!("recipe: `{command}` is not a recipe verb (they register as typed commands)"))
    }

    fn commands(&self) -> Vec<Arc<dyn DynCommand>> {
        crate::commands::recipe_run::command_objects(self.executor_slot.clone(), self.runs.clone())
    }

    fn install_executor(&self, executor: Arc<CommandExecutor>) {
        self.executor_slot.install(executor);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
