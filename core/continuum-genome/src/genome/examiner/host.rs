//! The examiner's one seam into a mind: sit ONE gene's acceptance test on her own cognition.
//! Cognition implements it (`cognition::eval::gene_exam`) and core installs it at boot; the
//! genome never names the turn machinery. A composer that runs many genes at once meets the
//! same seam: `with` is her genome, `without` is her genome minus this one gene.

use super::verdict::ExamSitting;
use crate::ai::types::ActiveAdapterRequest;
use async_trait::async_trait;
use std::path::PathBuf;
use uuid::Uuid;

/// One gene to examine.
#[derive(Debug, Clone)]
pub struct GeneExam {
    pub persona: Uuid,
    /// The gene, at the scale she serves it (`a`); the without arm runs it at 0.
    pub gene: ActiveAdapterRequest,
    /// The base it was trained on: the sitting runs only on that base, live.
    pub base_model_id: String,
    /// The training job's directory, whose `request.json` names what it trained on.
    pub trained_from: Option<PathBuf>,
}

/// Why no sitting happened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExamRefusal {
    /// Not now, maybe later: she is not online, her base is not the one serving, the engine
    /// is cold or faulted mid-sitting. No receipt; the next wake tries again.
    NotNow(String),
    /// Never: no task can be proven unseen, or none is left. Receipted as unexaminable.
    Unexaminable(String),
}

#[async_trait]
pub trait ExamHost: Send + Sync + 'static {
    /// Her own turns on related tasks the gene never trained on and on unrelated tasks, each
    /// sat with the gene and without it, on the live engine, with the same base, tools,
    /// workspace and the rest of her genome.
    async fn sit(&self, exam: &GeneExam) -> Result<ExamSitting, ExamRefusal>;
}
