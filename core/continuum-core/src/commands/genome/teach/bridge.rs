//! Explicit candidate preparation/replay; dataset generation remains the default.
use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;

use continuum_client::{Connection, Transport};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use ts_rs::TS;
use uuid::Uuid;

use super::{EvalTask, GenomeTeach, GenomeTeachParams, GenomeTeachResult};
use crate::commands::training_trigger::submit::{SubmitOutcome, SubmitParams};
use crate::genome::fine_tuning::{TrainingDataset, TrainingSource};
use crate::identity::PersonaRef;
use crate::modules::dataset::{CandidateReservation, DatasetCandidate, DatasetService};
use crate::persona::inbox_admission::content_hash_sha256;
use crate::runtime::InProcessTransport;
use crate::sdk_codegen::{CommandError, Ctx};

const EVIDENCE_KEY: &str = "teachCandidate";
const PRODUCER: &str = "genome/teach:v1";

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[serde(
    tag = "action",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/TeachTrainingAction.ts"
)]
pub enum TeachTrainingAction {
    /// Persist a candidate before submission. Repeating the exact request reuses
    /// it; changed input under the same ID is refused before teacher acquisition.
    PrepareAndSubmit {
        #[ts(type = "string")]
        candidate_id: Uuid,
        recipient: PersonaRef,
        base_model_id: String,
        trait_kind: String,
        heldout_gym: String,
    },
    /// Submit the exact stored request. Generation options cannot be supplied.
    SubmitCandidate {
        #[ts(type = "string")]
        candidate_id: Uuid,
    },
}

impl TeachTrainingAction {
    fn candidate_id(&self) -> Uuid {
        match self {
            Self::PrepareAndSubmit { candidate_id, .. }
            | Self::SubmitCandidate { candidate_id } => *candidate_id,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/TeachTrainingResult.ts"
)]
pub struct TeachTrainingResult {
    #[ts(type = "string")]
    pub candidate_id: Uuid,
    pub content_hash: String,
    /// True means no teacher generation occurred in this invocation.
    pub reused: bool,
    /// Durable acceptance is distinct from dispatch success and from learning.
    /// A transport failure leaves this absent; retry this candidate on the same node.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub submission: Option<SubmitOutcome>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub submission_error: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct Evidence {
    producer: String,
    /// Diagnostic build identity, not a claim of clean source or pinned weights.
    producer_build: String,
    request_hash: String,
    source_hash: String,
    source_tasks: Vec<EvalTask>,
    recipient_peer: Uuid,
    trainable_base: String,
    session_id: Option<Uuid>,
    context_id: Option<Uuid>,
    result: GenomeTeachResult,
}

pub(super) struct Preparation {
    service: Arc<DatasetService>,
    reservation: CandidateReservation,
    dataset_dir: PathBuf,
    submission: SubmitParams,
    request_hash: String,
    recipient_peer: Uuid,
    trainable_base: String,
    heldout_gym: String,
    source_tasks: Vec<EvalTask>,
    ctx: Ctx,
}

impl Preparation {
    fn resolve(
        service: Arc<DatasetService>,
        reservation: CandidateReservation,
        p: &GenomeTeachParams,
        ctx: &Ctx,
    ) -> Result<Self, CommandError> {
        let Some(TeachTrainingAction::PrepareAndSubmit {
            candidate_id,
            recipient,
            base_model_id,
            trait_kind,
            heldout_gym,
        }) = p.training.as_ref()
        else {
            return Err(CommandError::Invalid(
                "candidate preparation requires prepareAndSubmit".into(),
            ));
        };
        if trait_kind.trim().is_empty() {
            return Err(CommandError::Invalid("traitKind is required".into()));
        }
        let registry = crate::persona::PersonaAircRuntimeRegistry::try_global()
            .ok_or_else(|| CommandError::Invalid("recipient roster is unavailable".into()))?;
        let runtime = match registry.get_by_agent_name(recipient.as_str()) {
            Some(runtime) => runtime,
            None => {
                let id = crate::id_resolve::resolve(
                    recipient.as_str(),
                    &registry.live_personas(),
                    "recipient",
                )
                .map_err(CommandError::Invalid)?;
                registry.get(id).ok_or_else(|| {
                    CommandError::Invalid("recipient must have a canonical live runtime".into())
                })?
            }
        };
        let trainable_base =
            crate::model_registry::artifacts::resolve_hf_source_for_model_id(base_model_id)
                .map_err(CommandError::Invalid)?;
        let dataset_dir = service
            .candidate_dataset_directory(&reservation)
            .map_err(CommandError::Internal)?;
        Ok(Self {
            dataset_dir,
            service,
            reservation,
            submission: SubmitParams {
                submission_id: Some(*candidate_id),
                persona_id: runtime.persona_id(),
                persona_name: runtime.agent_name().into(),
                base_model: base_model_id.clone(),
                trait_kind: trait_kind.clone(),
                examples: vec![],
                source: TrainingSource::TeacherSynthesized,
                eval_set: None,
                lora: None,
                schedule: None,
                local_artifact_dir: None,
                preferred_provider: None,
                min_examples: Some(crate::modules::training_trigger::DEFAULT_MIN_EXAMPLES),
                validation_split: Some(crate::modules::training_trigger::DEFAULT_VALIDATION_SPLIT),
            },
            request_hash: request_hash(p)?,
            recipient_peer: runtime.airc().peer_id().as_uuid(),
            trainable_base,
            heldout_gym: heldout_gym.clone(),
            source_tasks: vec![],
            ctx: ctx.clone(),
        })
    }

    pub(super) async fn bind_source(mut self, tasks: Vec<EvalTask>) -> Result<Self, CommandError> {
        tokio::task::spawn_blocking(move || {
            let (origin, text) = crate::cognition::gym::resolve_gym(&self.heldout_gym)
                .map_err(CommandError::Invalid)?;
            let heldout = crate::cognition::gym::parse_tasks(&text, &origin)
                .map_err(CommandError::Invalid)?;
            ensure_disjoint(&tasks, &heldout)?;
            self.submission.eval_set =
                Some(crate::cognition::gym::publish_gym(&text).map_err(CommandError::Internal)?);
            self.source_tasks = tasks;
            Ok(self)
        })
        .await
        .map_err(|e| CommandError::Internal(e.to_string()))?
    }

    pub(super) fn dataset_dir(&self) -> Result<PathBuf, CommandError> {
        Ok(self.dataset_dir.clone())
    }

    pub(super) async fn publish(
        mut self,
        result: GenomeTeachResult,
    ) -> Result<GenomeTeachResult, CommandError> {
        tokio::task::spawn_blocking(move || {
            let dataset = TrainingDataset::from_chat_jsonl(
                &self.dataset_dir()?.join("train.jsonl"),
                TrainingSource::TeacherSynthesized,
            )
            .map_err(CommandError::Invalid)?;
            self.submission.examples = dataset.examples;
            let source = serde_json::to_string(&self.source_tasks)
                .map_err(|e| CommandError::Internal(e.to_string()))?;
            let evidence = Evidence {
                producer: PRODUCER.into(),
                producer_build: env!("CONTINUUM_BUILD_GIT_SHA").into(),
                request_hash: self.request_hash,
                source_hash: content_hash_sha256(&source),
                source_tasks: self.source_tasks,
                recipient_peer: self.recipient_peer,
                trainable_base: self.trainable_base,
                session_id: self.ctx.session_id,
                context_id: self.ctx.context_id,
                result: result.clone(),
            };
            let first =
                self.submission.examples.first_mut().ok_or_else(|| {
                    CommandError::Invalid("candidate train split is empty".into())
                })?;
            let metadata = first
                .metadata
                .get_or_insert_with(|| json!({}))
                .as_object_mut()
                .ok_or_else(|| {
                    CommandError::Invalid("candidate metadata is not an object".into())
                })?;
            metadata.insert(
                EVIDENCE_KEY.into(),
                serde_json::to_value(evidence)
                    .map_err(|e| CommandError::Internal(e.to_string()))?,
            );
            self.service
                .publish_candidate(self.reservation, self.submission)
                .map_err(CommandError::Internal)?;
            Ok(result)
        })
        .await
        .map_err(|e| CommandError::Internal(e.to_string()))?
    }
}

fn request_hash(p: &GenomeTeachParams) -> Result<String, CommandError> {
    let mut normalized = p.clone();
    normalized.detach = None;
    normalized.run_id = None;
    serde_json::to_string(&normalized)
        .map(|s| content_hash_sha256(&s))
        .map_err(|e| CommandError::Invalid(e.to_string()))
}

fn ensure_disjoint(source: &[EvalTask], heldout: &[EvalTask]) -> Result<(), CommandError> {
    if heldout.is_empty()
        || heldout.iter().any(|task| {
            task.test.as_deref().is_none_or(|s| s.trim().is_empty())
                && task
                    .dod_shell
                    .as_deref()
                    .is_none_or(|s| s.trim().is_empty())
        })
    {
        return Err(CommandError::Invalid(
            "heldoutGym must contain independent executable tasks".into(),
        ));
    }
    for task in heldout {
        if source.iter().any(|other| {
            (!task.id.is_empty() && task.id == other.id)
                || task.prompt.trim() == other.prompt.trim()
                || task.test.as_deref().is_some_and(|test| {
                    !test.trim().is_empty()
                        && other
                            .test
                            .as_deref()
                            .is_some_and(|t| t.trim() == test.trim())
                })
                || task.dod_shell.as_deref().is_some_and(|test| {
                    !test.trim().is_empty()
                        && other
                            .dod_shell
                            .as_deref()
                            .is_some_and(|t| t.trim() == test.trim())
                })
        }) {
            return Err(CommandError::Invalid(format!(
                "heldoutGym task '{}' overlaps the selected teach source",
                task.id
            )));
        }
    }
    Ok(())
}

fn candidate_evidence(candidate: &DatasetCandidate) -> Result<Evidence, CommandError> {
    let value = candidate
        .submission
        .examples
        .first()
        .and_then(|e| e.metadata.as_ref())
        .and_then(|m| m.get(EVIDENCE_KEY))
        .ok_or_else(|| {
            CommandError::Invalid("candidate is not a published GenomeTeach candidate".into())
        })?;
    let evidence: Evidence = serde_json::from_value(value.clone())
        .map_err(|e| CommandError::Invalid(format!("invalid teach candidate provenance: {e}")))?;
    if evidence.producer != PRODUCER {
        return Err(CommandError::Invalid(
            "unsupported teach candidate producer".into(),
        ));
    }
    Ok(evidence)
}

async fn candidate_flow<T, F, Fut>(
    service: Arc<DatasetService>,
    p: GenomeTeachParams,
    conn: &Connection<T>,
    gym_cache: PathBuf,
    generate: F,
) -> Result<GenomeTeachResult, CommandError>
where
    T: Transport,
    F: FnOnce(CandidateReservation) -> Fut,
    Fut: Future<Output = Result<GenomeTeachResult, CommandError>>,
{
    let action = p
        .training
        .as_ref()
        .ok_or_else(|| CommandError::Invalid("training action required".into()))?;
    let id = action.candidate_id();
    let resume = matches!(action, TeachTrainingAction::SubmitCandidate { .. });
    if resume
        && (p.tasks.is_some()
            || p.teach_set.is_some()
            || p.from_experience.is_some()
            || p.teacher_model.is_some()
            || p.max_fix_iters.is_some()
            || p.temperature.is_some()
            || p.name.is_some()
            || p.output_dir.is_some()
            || p.split_ratio.is_some())
    {
        return Err(CommandError::Invalid(
            "submitCandidate accepts no generation or dataset overrides".into(),
        ));
    }
    let storage = service.clone();
    let loaded = tokio::task::spawn_blocking(move || storage.load_candidate(id))
        .await
        .map_err(|e| CommandError::Internal(e.to_string()))?;
    let (candidate, reused) = match loaded {
        Ok(candidate) => (candidate, true),
        Err(error) if resume => return Err(CommandError::Invalid(error)),
        Err(_) => {
            let storage = service.clone();
            let reservation = tokio::task::spawn_blocking(move || storage.reserve_candidate(id))
                .await
                .map_err(|e| CommandError::Internal(e.to_string()))?
                .map_err(CommandError::Invalid)?;
            generate(reservation).await?;
            let candidate = tokio::task::spawn_blocking(move || service.load_candidate(id))
                .await
                .map_err(|e| CommandError::Internal(e.to_string()))?
                .map_err(CommandError::Internal)?;
            (candidate, false)
        }
    };
    let evidence = candidate_evidence(&candidate)?;
    if !resume && evidence.request_hash != request_hash(&p)? {
        return Err(CommandError::Invalid(
            "candidate identity already binds different teach inputs".into(),
        ));
    }
    let gym =
        candidate.submission.eval_set.clone().ok_or_else(|| {
            CommandError::Invalid("teach candidate has no pinned heldout gym".into())
        })?;
    let selected = evidence.source_tasks.clone();
    tokio::task::spawn_blocking(move || {
        let (origin, text) = crate::cognition::gym::resolve_published_gym_in(&gym_cache, &gym)
            .map_err(CommandError::Invalid)?;
        let tasks =
            crate::cognition::gym::parse_tasks(&text, &origin).map_err(CommandError::Invalid)?;
        ensure_disjoint(&selected, &tasks)
    })
    .await
    .map_err(|e| CommandError::Internal(e.to_string()))??;
    let mut result = evidence.result;
    result.run_id = p.run_id;
    let request = serde_json::to_value(&candidate.submission)
        .map_err(|e| CommandError::Internal(e.to_string()))?;
    let (submission, submission_error) =
        match crate::persona::training_producer::submit_training(conn, request).await {
            Ok(outcome) => (Some(outcome), None),
            Err(error) => (None, Some(error.to_string())),
        };
    result.training = Some(TeachTrainingResult {
        candidate_id: id,
        content_hash: candidate.content.content_hash,
        reused,
        submission,
        submission_error,
    });
    Ok(result)
}

impl GenomeTeach {
    pub(super) async fn run_owned(
        &self,
        ctx: &Ctx,
        p: GenomeTeachParams,
    ) -> Result<GenomeTeachResult, CommandError> {
        if p.training.is_none() {
            return Self::run_teach(p, None).await;
        }
        if p.output_dir.is_some() {
            return Err(CommandError::Invalid(
                "training candidates use the canonical datasets root; outputDir is dataset-only"
                    .into(),
            ));
        }
        if p.temperature.is_some_and(|value| !value.is_finite())
            || p.split_ratio
                .is_some_and(|value| !value.is_finite() || value <= 0.0 || value > 1.0)
        {
            return Err(CommandError::Invalid(
                "candidate temperature must be finite and splitRatio must be in (0, 1]".into(),
            ));
        }
        let executor = self.executor.cloned().ok_or_else(|| {
            CommandError::Internal("genome command executor is not installed".into())
        })?;
        let conn = Connection::new(InProcessTransport::new(executor, ctx.caller.clone()));
        let service = Arc::new(DatasetService::new(
            crate::modules::dataset::default_datasets_root(),
        ));
        let generate_params = p.clone();
        let generation_service = service.clone();
        candidate_flow(
            service,
            p,
            &conn,
            crate::cognition::gym::gym_cache_dir(),
            |reservation| async move {
                let preparation_params = generate_params.clone();
                let preparation_ctx = ctx.clone();
                let preparation = tokio::task::spawn_blocking(move || {
                    Preparation::resolve(
                        generation_service,
                        reservation,
                        &preparation_params,
                        &preparation_ctx,
                    )
                })
                .await
                .map_err(|e| CommandError::Internal(e.to_string()))??;
                Self::run_teach(generate_params, Some(preparation)).await
            },
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::adapter::AIProviderAdapter;
    use crate::ai::heuristic_adapter::HeuristicInferenceAdapter;
    use crate::ai::types::{FinishReason, TextGenerationResponse, UsageMetrics};
    use crate::ai::ChatMessage;
    use crate::commands::training_trigger::test_support::build_runtime;
    use std::sync::Mutex;

    // what this catches: the actual candidate→producer→durable trigger route must
    // retry without teacher calls, preserve acceptance after owner reconstruction,
    // and refuse changed input under the same identity before spending inference.
    #[tokio::test]
    async fn candidate_replay_preserves_acceptance_without_teacher_regeneration() {
        let temp = tempfile::tempdir().unwrap();
        let service = Arc::new(DatasetService::new(temp.path().join("datasets")));
        let gym_cache = temp.path().join("gyms");
        let gym_reference = crate::cognition::gym::publish_gym_in(&gym_cache,
            r#"{"id":"heldout-subtract","prompt":"Implement subtract(a,b)","test":"assert_eq!(subtract(4,1),3);"}"#).unwrap();
        let id = Uuid::new_v4();
        let recipient = Uuid::new_v4();
        let params: GenomeTeachParams = serde_json::from_value(json!({
            "training": {"action": "prepareAndSubmit", "candidateId": id,
                "recipient": recipient.to_string(), "baseModelId": "fixture-base",
                "traitKind": "code", "heldoutGym": gym_reference},
            "tasks": [{"id":"teach-add", "prompt":"Implement add(a,b)", "test":"assert_eq!(add(1,2),3);"}],
            "teacherModel":"requested-teacher", "splitRatio":1.0
        })).unwrap();
        let task = params.tasks.as_ref().unwrap()[0].clone();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let adapter: Arc<dyn AIProviderAdapter> = Arc::new(
            HeuristicInferenceAdapter::new()
                .with_responses(vec![TextGenerationResponse {
                    text: "fn add(a:i32,b:i32)->i32 {a+b}".into(),
                    finish_reason: FinishReason::Stop,
                    model: "actual-teacher".into(),
                    provider: "fixture-provider".into(),
                    usage: UsageMetrics::default(),
                    response_time_ms: 1,
                    request_id: "served-request".into(),
                    content: None,
                    tool_calls: None,
                    reasoning: Some("private-sentinel".into()),
                    routing: None,
                    error: None,
                    timing: None,
                }])
                .with_request_recorder(requests.clone()),
        );
        let (storage, _database_dir) = crate::orm::store::fresh_adapter().await;
        let (first_owner, executor) = build_runtime(storage.clone(), false).await;
        let conn = Connection::new(InProcessTransport::new(executor.clone(), None));
        let generation_service = service.clone();
        let generation_params = params.clone();
        let first = candidate_flow(
            service.clone(),
            params.clone(),
            &conn,
            gym_cache.clone(),
            |reservation| async move {
                let generated = super::super::teacher_generate(
                    &adapter,
                    "requested-teacher",
                    vec![ChatMessage::text("user", &task.prompt)],
                    0.0,
                )
                .await?;
                let (passed, grade) = crate::cognition::gym_grader::test_grade(
                    &generated.text,
                    "rust",
                    task.test.as_deref().unwrap(),
                )
                .await;
                assert!(passed, "{grade}");
                let row = super::super::build_sharegpt(
                    &[
                        ChatMessage::text("user", &task.prompt),
                        ChatMessage::text("assistant", generated.text),
                    ],
                    &[generated.receipt],
                );
                let directory = generation_service
                    .candidate_dataset_directory(&reservation)
                    .unwrap();
                DatasetService::split_and_write("fixture", &directory, &[row], 1.0, None).unwrap();
                let preparation = Preparation {
                    service: generation_service,
                    reservation,
                    dataset_dir: directory.clone(),
                    submission: SubmitParams {
                        submission_id: Some(id),
                        persona_id: recipient,
                        persona_name: "fixture-recipient".into(),
                        base_model: "fixture-base".into(),
                        trait_kind: "code".into(),
                        examples: vec![],
                        source: TrainingSource::TeacherSynthesized,
                        eval_set: Some(gym_reference),
                        lora: None,
                        schedule: None,
                        local_artifact_dir: None,
                        preferred_provider: None,
                        min_examples: Some(crate::modules::training_trigger::DEFAULT_MIN_EXAMPLES),
                        validation_split: Some(
                            crate::modules::training_trigger::DEFAULT_VALIDATION_SPLIT,
                        ),
                    },
                    request_hash: request_hash(&generation_params)?,
                    recipient_peer: Uuid::new_v4(),
                    trainable_base: "fixture/trainable".into(),
                    heldout_gym: "independent-gym".into(),
                    source_tasks: vec![task],
                    ctx: Ctx {
                        session_id: Some(Uuid::new_v4()),
                        ..Ctx::default()
                    },
                };
                preparation
                    .publish(GenomeTeachResult {
                        dataset: "fixture".into(),
                        teacher_model: "requested-teacher".into(),
                        dataset_dir: directory.display().to_string(),
                        tasks_total: 1,
                        tasks_solved: 1,
                        examples: 1,
                        train_examples: 1,
                        ..Default::default()
                    })
                    .await
            },
        )
        .await
        .unwrap();
        let first_submission = first
            .training
            .as_ref()
            .unwrap()
            .submission
            .as_ref()
            .unwrap();
        assert!(first_submission.success);
        assert!(!first_submission.acceptance.as_ref().unwrap().replayed);
        assert_eq!(requests.lock().unwrap().len(), 1);
        let persisted = service.load_candidate(id).unwrap();
        let exact_request = serde_json::to_value(&persisted.submission).unwrap();
        let text = exact_request.to_string();
        assert!(text.contains("actual-teacher"));
        assert!(!text.contains("private-sentinel"));
        assert!(!text.contains("cardId"));
        drop(conn);
        drop(executor);
        drop(first_owner);

        let (next_owner, executor) = build_runtime(storage, false).await;
        let conn = Connection::new(InProcessTransport::new(executor, None));
        let replay = candidate_flow(
            service.clone(),
            params.clone(),
            &conn,
            gym_cache.clone(),
            |_| async { panic!("retry attempted teacher resolution/generation") },
        )
        .await
        .unwrap();
        let receipt = replay.training.unwrap();
        assert!(receipt.reused);
        assert!(receipt.submission.unwrap().acceptance.unwrap().replayed);
        assert_eq!(
            next_owner
                .state
                .bucket_example_count(recipient, "code", "fixture-base"),
            Some(1)
        );
        let mut changed = params.clone();
        if let Some(TeachTrainingAction::PrepareAndSubmit { base_model_id, .. }) =
            &mut changed.training
        {
            *base_model_id = "different-base".into();
        }
        let refusal = candidate_flow(
            service.clone(),
            changed,
            &conn,
            gym_cache.clone(),
            |_| async { panic!("changed identity attempted teacher resolution/generation") },
        )
        .await
        .unwrap_err();
        assert!(refusal.to_string().contains("different teach inputs"));
        let resume: GenomeTeachParams = serde_json::from_value(
            json!({"training":{"action":"submitCandidate", "candidateId":id}}),
        )
        .unwrap();
        let resumed = candidate_flow(
            service.clone(),
            resume.clone(),
            &conn,
            gym_cache.clone(),
            |_| async { panic!("explicit resume attempted teacher generation") },
        )
        .await
        .unwrap();
        assert!(
            resumed
                .training
                .unwrap()
                .submission
                .unwrap()
                .acceptance
                .unwrap()
                .replayed
        );
        assert_eq!(requests.lock().unwrap().len(), 1);
        assert_eq!(
            serde_json::to_value(service.load_candidate(id).unwrap().submission).unwrap(),
            exact_request
        );
        let blob = std::fs::read_dir(gym_cache.join("content").join("sha256"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        std::fs::write(blob, "changed heldout bytes").unwrap();
        let refusal = candidate_flow(service, resume, &conn, gym_cache, |_| async {
            panic!("damaged gym attempted teacher generation")
        })
        .await
        .unwrap_err();
        assert!(refusal.to_string().contains("integrity mismatch"));
        assert_eq!(requests.lock().unwrap().len(), 1);
        assert_eq!(
            next_owner
                .state
                .bucket_example_count(recipient, "code", "fixture-base"),
            Some(1)
        );
    }

    // what this catches: a renamed copy of the teach task or a chat-only split is
    // not independent executable heldout evidence; a distinct task remains valid.
    #[test]
    fn heldout_requires_executable_disjoint_tasks() {
        let source: EvalTask = serde_json::from_value(
            json!({"id":"source", "prompt":"Implement add", "test":"assert_eq!(add(1,2),3);"}),
        )
        .unwrap();
        let mut renamed = source.clone();
        renamed.id = "different-id".into();
        assert!(ensure_disjoint(&[source.clone()], &[renamed]).is_err());
        let mut heldout = source.clone();
        heldout.id = "heldout".into();
        heldout.prompt = "Implement subtract".into();
        assert!(ensure_disjoint(&[source.clone()], &[heldout.clone()]).is_err());
        heldout.test = Some("assert_eq!(subtract(3,1),2);".into());
        assert!(ensure_disjoint(&[source.clone()], &[heldout.clone()]).is_ok());
        heldout.test = None;
        assert!(ensure_disjoint(&[source], &[heldout]).is_err());
        assert!(ensure_disjoint(&[], &[]).is_err());
    }
}
