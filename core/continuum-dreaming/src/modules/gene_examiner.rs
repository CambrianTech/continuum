//! Installs the gene EXAMINER (`genome::examiner`) at boot: its receipts in the main data
//! store, its sitting on her own cognition (`cognition::eval::MindExamHost`), and this core
//! process as its boot. The examiner owns the triggers and the verdict; this module only
//! wires what it needs and starts it once.

use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;

use crate::genome::examiner::{Examiner, GeneExamReceipt};
use crate::genome::gene_trial::GeneTrials;
use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};

#[derive(Default)]
pub struct GeneExaminerModule;

#[async_trait]
impl ServiceModule for GeneExaminerModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "gene-examiner",
            priority: ModulePriority::Background,
            command_prefixes: &[],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    async fn initialize(&self, ctx: &ModuleContext) -> Result<(), String> {
        let adapter = ctx.storage("the gene examiner")?.adapter("main").await?;
        let receipts = crate::orm::OrmStore::<GeneExamReceipt>::new(adapter).await.map_err(|e| e.to_string())?;
        let trials = GeneTrials::default_store().ok_or_else(|| "gene examiner: no home for the trial file".to_string())?;
        let examiner = Examiner::new(
            trials,
            receipts,
            Arc::new(crate::cognition::eval::MindExamHost),
            crate::capacity::gossip::this_process_origin(),
        );
        Arc::new(examiner).start();
        continuum_base::probe!(
            class = "genome.examiner.started",
            "the gene examiner is serving: retention re-run now, then a pass per trial opened"
        );
        Ok(())
    }

    async fn handle_command(&self, command: &str, _params: Value) -> Result<CommandResult, String> {
        Err(format!("gene-examiner handles no commands: {command}"))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
