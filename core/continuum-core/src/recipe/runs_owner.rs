//! The node's recipe-run store, as the module that holds it: the pipeline executor finds the
//! store through the module registry ([`RecipeModule::runs_in`]). The store and its lookup
//! are the recipe's; the `ServiceModule` wiring (open the store at boot, register the
//! run verbs) is `modules::recipe`.

use std::sync::Arc;

use crate::recipe::run_record::RecipeRuns;
use crate::runtime::command_executor::CommandExecutor;
use crate::runtime::registry::ModuleRegistry;
use crate::runtime::LateBound;

pub const MODULE_NAME: &str = "recipe";

pub struct RecipeModule {
    pub(crate) runs: Arc<LateBound<RecipeRuns>>,
    pub(crate) executor_slot: Arc<LateBound<CommandExecutor>>,
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
