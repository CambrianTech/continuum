//! The exclusive teacher window, reached without naming its owner (card dc3e9f6f).
//!
//! `genome/teach` with `exclusive_teacher` asks the node's serving owner to checkpoint its lanes,
//! run a teacher batch on a private lane, and restore them. The teach command and the genome
//! module held the core's serving daemon itself just to call that one operation, so neither could
//! leave the core. The serving daemon implements this trait instead; they hold the trait.

use super::*;
use crate::genome::teach::RemediationCorpus;

/// What a teacher batch runs: the tasks, the teacher model, and its sampling and fix budget.
pub struct TeacherBatchRequest {
    pub tasks: Vec<EvalTask>,
    pub teacher_model: String,
    pub temperature: f32,
    pub max_fix_iters: Option<u32>,
}

/// The serving owner's exclusive teacher window: only a corpus comes back.
#[async_trait::async_trait]
pub trait TeacherServing: Send + Sync {
    async fn run_teacher_batch(&self, request: TeacherBatchRequest) -> Result<RemediationCorpus, CommandError>;
}
