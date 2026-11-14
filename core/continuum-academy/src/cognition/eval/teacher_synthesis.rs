//! The teacher's lane, owned here: the genome's teacher (`genome::teach`) runs on whatever
//! adapter it is handed, and acquiring that adapter is the eval lane owner's job. These
//! wrappers keep the serving-readiness prerequisite and the eval lane policy on the one path
//! every self-driven synthesis takes, and project an eval task into the teacher's lesson.

use super::*;
use crate::genome::teach::{
    synthesize_lived_expansion_with_adapter, synthesize_remediation_with_adapter,
    teachable_stimuli, RemediationCorpus, TeachTask,
};

impl From<&EvalTask> for TeachTask {
    fn from(task: &EvalTask) -> Self {
        Self {
            id: task.id.clone(),
            prompt: task.prompt.clone(),
            test: task.test.clone(),
            lang: task.lang.clone(),
        }
    }
}

/// The teacher's lessons for a set of eval tasks, in order.
pub fn teach_tasks(tasks: &[EvalTask]) -> Vec<TeachTask> {
    tasks.iter().map(TeachTask::from).collect()
}

/// Preserve the serving-readiness prerequisite and the eval owner's lane policy.
/// Successful acquisition may share resident weights; refusal ends synthesis.
async fn acquire_teacher_lane(model: &str) -> Result<EvalLane, CommandError> {
    use crate::inference::llama_server::{await_ready_serving, DEFAULT_SERVING_WAIT};
    if await_ready_serving(DEFAULT_SERVING_WAIT).await.is_none() {
        return Err(CommandError::Internal(
            "no served model became ready within the serving-wait budget -- cannot run the teacher. \
             Bring up serving (ai/inference/serve) before genome/teach."
                .to_string(),
        ));
    }
    spawn_base_eval_lane(model).await
}

/// Remediation over eval tasks on a dedicated teacher lane: the lane is acquired here and
/// held for the whole batch, then the genome's synthesizer runs on its adapter.
pub async fn synthesize_remediation(
    tasks: &[EvalTask],
    teacher_model: &str,
    temperature: f32,
    max_fix_iters: Option<u32>,
) -> Result<RemediationCorpus, CommandError> {
    synthesize_remediation_with_lane(
        tasks,
        teacher_model,
        temperature,
        max_fix_iters,
        acquire_teacher_lane(teacher_model),
    )
    .await
}

// The lane future keeps acquisition and the generation loop on the same path in tests.
pub(super) async fn synthesize_remediation_with_lane(
    tasks: &[EvalTask],
    teacher_model: &str,
    temperature: f32,
    max_fix_iters: Option<u32>,
    lane: impl std::future::Future<Output = Result<EvalLane, CommandError>>,
) -> Result<RemediationCorpus, CommandError> {
    // Retain the owner for the entire batch, including grading between generations.
    let teacher_lane = lane.await?;
    synthesize_remediation_with_adapter(
        &teach_tasks(tasks),
        teacher_model,
        temperature,
        max_fix_iters,
        &teacher_lane.adapter,
        None,
    )
    .await
}

/// Lived expansion on a dedicated teacher lane. Empty stimuli in, empty out, and no lane
/// is acquired for nothing to answer.
pub async fn synthesize_lived_expansion(
    stimuli: &[String],
    teacher_model: &str,
    temperature: f32,
) -> Result<Vec<serde_json::Value>, CommandError> {
    synthesize_lived_expansion_with_lane(
        stimuli,
        teacher_model,
        temperature,
        acquire_teacher_lane(teacher_model),
    )
    .await
}

pub(super) async fn synthesize_lived_expansion_with_lane(
    stimuli: &[String],
    teacher_model: &str,
    temperature: f32,
    lane: impl std::future::Future<Output = Result<EvalLane, CommandError>>,
) -> Result<Vec<serde_json::Value>, CommandError> {
    let stimuli = teachable_stimuli(stimuli);
    if stimuli.is_empty() {
        return Ok(Vec::new());
    }
    let teacher_lane = lane.await?;
    synthesize_lived_expansion_with_adapter(
        &stimuli,
        teacher_model,
        temperature,
        &teacher_lane.adapter,
    )
    .await
}
