//! GenomeModule — owns the [`FineTuningRegistry`] + [`FineTuningCoordinator`] and
//! exposes the `genome/job-*` lifecycle through the typed command registry.
//!
//! The three verbs (`genome/job-create`, `genome/job-status`, `genome/job-cancel`)
//! live as [`ActionCommand`](crate::sdk_codegen::ActionCommand)s under
//! `commands/genome/`; this module's only job is to construct the registry +
//! coordinator and hand the command objects out via [`ServiceModule::commands`].
//!
//! ## Boot
//!
//! `start_server` builds a `GenomeModule` AFTER reading credentials, seeds the
//! registry with whichever cloud adapters have keys (OpenAIFineTuningAdapter when
//! `OPENAI_API_KEY` is set, etc.) and always registers [`LocalCandleFineTuner`] so
//! the architectural slot is visible to the coordinator even before #231-#233 land
//! the optimizer loop. See `ipc::mod::start_server`.

use std::any::Any;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use crate::genome::fine_tuning::{coordinator::FineTuningCoordinator, FineTuningRegistry};
use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use crate::sdk_codegen::DynCommand;

pub struct GenomeModule {
    executor: Arc<crate::runtime::LateBound<crate::runtime::CommandExecutor>>,
    decisions: Arc<
        crate::runtime::LateBound<crate::orm::OrmStore<crate::genome::recall_impl::RecallDecision>>,
    >,
    registry: Arc<FineTuningRegistry>,
    coordinator: Arc<FineTuningCoordinator>,
    #[cfg(test)]
    test_job_board: Arc<crate::genome::fine_tuning::TrainingJobBoard>,
    #[cfg(test)]
    test_artifacts: Arc<tempfile::TempDir>,
}

impl GenomeModule {
    /// Construct with a pre-populated registry. The boot path
    /// (`start_server`) registers all adapters BEFORE building the
    /// module, so the coordinator's view of registered providers is
    /// stable across the module's lifetime.
    pub fn new(registry: Arc<FineTuningRegistry>) -> Self {
        let coordinator = Arc::new(FineTuningCoordinator::new(Arc::clone(&registry)));
        Self {
            executor: Arc::new(crate::runtime::LateBound::new("genome command executor")),
            decisions: Arc::new(crate::runtime::LateBound::new("genome recall decisions")),
            registry,
            coordinator,
            #[cfg(test)]
            test_job_board: Arc::new(crate::genome::fine_tuning::TrainingJobBoard::default()),
            #[cfg(test)]
            test_artifacts: Arc::new(tempfile::tempdir().expect("genome test artifact directory")),
        }
    }

    /// The ordinary command family and real adapters, with fixture-owned storage.
    #[cfg(test)]
    pub(crate) fn with_test_storage(
        registry: Arc<FineTuningRegistry>,
        test_job_board: Arc<crate::genome::fine_tuning::TrainingJobBoard>,
        test_artifacts: Arc<tempfile::TempDir>,
    ) -> Self {
        let coordinator = Arc::new(FineTuningCoordinator::new(Arc::clone(&registry)));
        Self {
            executor: Arc::new(crate::runtime::LateBound::new("genome command executor")),
            decisions: Arc::new(crate::runtime::LateBound::new("genome recall decisions")),
            registry,
            coordinator,
            test_job_board,
            test_artifacts,
        }
    }

    /// Visible to tests + boot. Returns the inner registry so a
    /// caller can introspect or register adapters after
    /// construction (substrate hot-reload future work; not used in
    /// the boot path today).
    pub fn registry(&self) -> Arc<FineTuningRegistry> {
        Arc::clone(&self.registry)
    }
}

#[async_trait]
impl ServiceModule for GenomeModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "genome",
            priority: ModulePriority::Normal,
            command_prefixes: &["genome/job-", "genome/recall", "genome/teach"],
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
            .ok_or_else(|| "genome requires the data module".to_string())?;
        let data = module
            .as_any()
            .downcast_ref::<crate::modules::data::DataModule>()
            .ok_or_else(|| "genome data module type mismatch".to_string())?;
        let adapter = data.state.get_adapter("main").await?;
        let store = crate::orm::OrmStore::new(adapter)
            .await
            .map_err(|e| e.to_string())?;
        self.decisions.install(Arc::new(store));
        Ok(())
    }

    /// The `genome/job-*` verbs over the module's registry + coordinator. The
    /// typed registry dispatches them; this module owns no legacy `match` arm.
    fn commands(&self) -> Vec<Arc<dyn DynCommand>> {
        let mut commands = crate::commands::genome::command_objects(
            Arc::clone(&self.registry),
            Arc::clone(&self.coordinator),
            #[cfg(test)]
            Arc::clone(&self.test_job_board),
            #[cfg(test)]
            Arc::clone(&self.test_artifacts),
        );
        commands.push(Arc::new(crate::commands::genome_recall::GenomeRecall {
            decisions: Arc::clone(&self.decisions),
        }));
        commands.push(Arc::new(
            crate::commands::genome_recall::GenomeRecallReplay {
                decisions: Arc::clone(&self.decisions),
            },
        ));
        commands.push(Arc::new(crate::commands::genome::teach::GenomeTeach {
            executor: Arc::clone(&self.executor),
        }));
        commands
    }

    fn install_executor(&self, executor: Arc<crate::runtime::CommandExecutor>) {
        self.executor.install(executor);
    }

    async fn handle_command(&self, command: &str, _params: Value) -> Result<CommandResult, String> {
        Err(format!(
            "genome command surface is migrated to the typed registry; \
             '{command}' has no legacy handler"
        ))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn module() -> GenomeModule {
        GenomeModule::new(Arc::new(FineTuningRegistry::new()))
    }

    // what this catches: the module exposes all three genome verbs to the typed
    // registry. A dropped family wiring would silently remove genome/job-* from the
    // persona tool surface, cu, and the grid — invisible without this assert.
    #[test]
    fn exposes_training_and_recall_commands() {
        let names: Vec<&str> = module().commands().iter().map(|c| c.name()).collect();
        assert_eq!(names.len(), 6);
        assert!(names.contains(&"genome/teach"));
        assert!(names.contains(&"genome/recall"));
        assert!(names.contains(&"genome/recall/replay"));
        assert!(names.contains(&"genome/job-create"));
        assert!(names.contains(&"genome/job-status"));
        assert!(names.contains(&"genome/job-cancel"));
    }

    // what this catches: the legacy string-dispatch path is dead — any call into it
    // fails loud naming the command, never silently no-ops. The genome verbs route
    // through the typed registry, not handle_command.
    #[tokio::test]
    async fn legacy_handle_command_fails_loud() {
        let err = module()
            .handle_command("genome/job-create", Value::Null)
            .await
            .unwrap_err();
        assert!(err.contains("genome/job-create"));
        assert!(err.contains("migrated to the typed registry"));
    }
}
