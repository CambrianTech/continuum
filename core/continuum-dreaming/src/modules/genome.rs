//! GenomeModule — the `genome/teach` and `genome/recall` verbs.
//!
//! A persona learns in place inside the serving engine (the training trigger's
//! `learn_in_place`); this module owns only what a caller asks of her genome by name:
//! teaching a lesson and recalling which gene a turn should carry.

use std::any::Any;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use crate::sdk_codegen::DynCommand;

pub struct GenomeModule {
    serving: Arc<crate::runtime::LateBound<dyn crate::cognition::eval::TeacherServing>>,
    executor: Arc<crate::runtime::LateBound<crate::runtime::CommandExecutor>>,
    decisions: Arc<
        crate::runtime::LateBound<crate::orm::OrmStore<crate::genome::recall_impl::RecallDecision>>,
    >,
}

impl GenomeModule {
    pub fn new() -> Self {
        Self {
            serving: Arc::new(crate::runtime::LateBound::new("teacher serving owner")),
            executor: Arc::new(crate::runtime::LateBound::new("genome command executor")),
            decisions: Arc::new(crate::runtime::LateBound::new("genome recall decisions")),
        }
    }

    /// Inject the existing lifecycle owner for explicit teacher windows.
    pub fn with_teacher_serving(
        self,
        serving: Arc<dyn crate::cognition::eval::TeacherServing>,
    ) -> Self {
        self.serving.install(serving);
        self
    }
}

impl Default for GenomeModule {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ServiceModule for GenomeModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "genome",
            priority: ModulePriority::Normal,
            command_prefixes: &["genome/recall", "genome/teach"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    async fn initialize(&self, ctx: &ModuleContext) -> Result<(), String> {
        let adapter = ctx.storage("genome")?.adapter("main").await?;
        let store = crate::orm::OrmStore::new(adapter)
            .await
            .map_err(|e| e.to_string())?;
        self.decisions.install(Arc::new(store));
        Ok(())
    }

    fn commands(&self) -> Vec<Arc<dyn DynCommand>> {
        vec![
            Arc::new(crate::commands::genome_recall::GenomeRecall { decisions: Arc::clone(&self.decisions) }),
            Arc::new(crate::commands::genome_recall::GenomeRecallReplay {
                decisions: Arc::clone(&self.decisions),
                executor: Arc::clone(&self.executor),
            }),
            Arc::new(crate::commands::genome::teach::GenomeTeach {
                serving: Arc::clone(&self.serving),
                executor: Arc::clone(&self.executor),
            }),
        ]
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
        GenomeModule::new()
    }

    // what this catches: the module exposes exactly the teach and recall verbs; a dropped one
    // silently leaves the persona tool surface, and a returned batch-training verb would
    // resurrect a second learning path beside learning in place.
    #[test]
    fn exposes_teach_and_recall_only() {
        let mut names: Vec<&str> = module().commands().iter().map(|c| c.name()).collect();
        names.sort();
        assert_eq!(names, ["genome/recall", "genome/recall/replay", "genome/teach"]);
    }

    // what this catches: the legacy string-dispatch path is dead — any call into it
    // fails loud naming the command, never silently no-ops. The genome verbs route
    // through the typed registry, not handle_command.
    #[tokio::test]
    async fn legacy_handle_command_fails_loud() {
        let err = module()
            .handle_command("genome/teach", Value::Null)
            .await
            .unwrap_err();
        assert!(err.contains("genome/teach"));
        assert!(err.contains("migrated to the typed registry"));
    }
}
