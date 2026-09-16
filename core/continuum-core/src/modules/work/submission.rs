//! Public submission/review commands on the existing work owner. Artifact
//! references carry content identity; publication does not imply blob verification.

use super::*;
use crate::persona::training_producer::reviewed::{self, SubmissionSelection};
use crate::runtime::{CommandExecutor, InProcessTransport, LateBound};
use continuum_client::Connection;

/// The same content-addressed reference the AIRC submission protocol carries.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/work/WorkArtifactReference.ts"
)]
pub struct WorkArtifactReference {
    /// SHA-256 of the artifact bytes, as 64 hexadecimal characters.
    pub hash: String,
    #[ts(type = "number")]
    pub size_bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub mime: Option<String>,
}

impl WorkArtifactReference {
    fn into_artifact(self) -> Result<airc_work::SubmissionArtifact, CommandError> {
        // Feed the owned string directly into the protocol's existing hash
        // decoder; no second hash parser or JSON body round-trip.
        let hash = Deserialize::deserialize(serde::de::value::StringDeserializer::<
            serde::de::value::Error,
        >::new(self.hash))
        .map_err(|e| CommandError::Invalid(format!("artifact hash: {e}")))?;
        Ok(airc_work::SubmissionArtifact {
            hash,
            size_bytes: self.size_bytes,
            mime: self.mime,
        })
    }
}

impl From<&airc_work::SubmissionArtifact> for WorkArtifactReference {
    fn from(value: &airc_work::SubmissionArtifact) -> Self {
        Self {
            hash: value.hash.to_string(),
            size_bytes: value.size_bytes,
            mime: value.mime.clone(),
        }
    }
}

pub struct WorkSubmit {
    pub registry: PersonaAircRuntimeRegistry,
    pub executor_slot: Arc<LateBound<CommandExecutor>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/work/WorkSubmitParams.ts"
)]
pub struct WorkSubmitParams {
    /// Explicit activity room; publication does not change current focus.
    pub room: String,
    /// Stable caller-chosen id. Reuse on retry with identical content.
    #[ts(type = "string")]
    pub submission_id: Uuid,
    #[ts(type = "string")]
    pub card_id: Uuid,
    #[ts(type = "string")]
    pub claim_id: Uuid,
    /// Task/instance identity carried by the existing submission protocol.
    pub instance: String,
    /// Full Git base object id of the submitted candidate.
    pub base_sha: String,
    pub artifact: WorkArtifactReference,
    /// Optional exact own staged-credit revision to preserve for independent
    /// review. Omitted means ordinary artifact publication without learning credit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string")]
    pub staged_revision_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/work/WorkSubmitResult.ts"
)]
pub struct WorkSubmitResult {
    #[ts(type = "string")]
    pub submission_id: Uuid,
    #[ts(type = "string")]
    pub card_id: Uuid,
    #[ts(type = "string")]
    pub room_id: Uuid,
    #[ts(type = "string")]
    pub publisher: Uuid,
    pub artifact: WorkArtifactReference,
    #[ts(type = "number")]
    pub submitted_at_ms: u64,
    /// Present only after the selected copy and reservations were persisted.
    #[ts(type = "string | null")]
    pub bound_staged_revision_id: Option<Uuid>,
}

#[async_trait]
impl ActionCommand for WorkSubmit {
    const NAME: &'static str = "work/submit";
    const NATIVE: bool = true;
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Submit an exact artifact reference under your own card claim. Reuse submission_id on retry. \
         Optionally bind one own staged_revision_id before publication for independent reviewed credit. \
         This records a candidate; it does not establish success or train it.";
    type Params = WorkSubmitParams;
    type Output = WorkSubmitResult;

    async fn run(&self, ctx: &Ctx, p: WorkSubmitParams) -> Result<WorkSubmitResult, CommandError> {
        if p.room.trim().is_empty() {
            return Err(CommandError::Invalid(
                "an explicit activity room is required".into(),
            ));
        }
        let runtime = persona_runtime(&self.registry, ctx, "work/submit")?;
        let airc = runtime.airc();
        let room = crate::modules::room_resolve::resolve_room(airc, Some(&p.room)).await?;
        let card_id = WorkCardId::from_uuid(p.card_id);
        let artifact = p.artifact.into_artifact()?;
        let base_sha = airc_work::GitObjectId::new(p.base_sha)
            .map_err(|e| CommandError::Invalid(format!("base_sha: {e}")))?;
        // Validate before allocating a durable binding. The SDK validates again
        // against its own latest projection immediately before publication.
        let board = airc
            .work_board_in(&room)
            .await
            .map_err(|e| CommandError::Internal(e.to_string()))?;
        let card = board.card(card_id).ok_or_else(|| {
            CommandError::NotFound(format!("card {} is absent from this room", p.card_id))
        })?;
        let candidate = airc_work::WorkSubmission {
            submission_id: airc_work::SubmissionId::from_uuid(p.submission_id),
            card_id,
            claim_id: ClaimId::from_uuid(p.claim_id),
            instance: p.instance,
            base_sha,
            artifact,
            publisher: airc.peer_id(),
            submitted_at_ms: chrono::Utc::now().timestamp_millis().max(0) as u64,
        };
        candidate
            .validate_for_card(card)
            .map_err(|e| CommandError::Invalid(format!("submission refused: {e}")))?;
        if let Some(revision) = p.staged_revision_id {
            if self.registry.get(runtime.persona_id()).is_none()
                || runtime.persona_id() != airc.peer_id().as_uuid()
            {
                return Err(CommandError::Invalid(
                    "selected credit must belong to the calling resident persona".into(),
                ));
            }
            let executor = self
                .executor_slot
                .cloned()
                .ok_or_else(|| CommandError::Internal("work executor is not ready".into()))?;
            let conn = Connection::new(InProcessTransport::new(
                executor,
                Some(crate::routing::CallerIdentity::local_persona(
                    crate::identity::PeerId::from_uuid(runtime.persona_id()),
                )),
            ));
            reviewed::bind_submission(
                &conn,
                runtime.agent_name(),
                runtime.persona_id(),
                SubmissionSelection {
                    submission_id: p.submission_id,
                    room_id: room.channel.as_uuid(),
                    card_id: p.card_id,
                    claim_id: p.claim_id,
                    staged_revision_id: revision,
                    instance: candidate.instance.clone(),
                    base_sha: candidate.base_sha.clone(),
                    artifact: candidate.artifact.clone(),
                },
            )
            .await
            .map_err(|e| match e {
                reviewed::CreditBindingError::Storage(source) => {
                    CommandError::Internal(source.to_string())
                }
                other => CommandError::Invalid(other.to_string()),
            })?;
        }
        let published = airc.submit_work_in(&room, airc_lib::SubmitWork {
            submission_id: candidate.submission_id, card_id, claim_id: candidate.claim_id,
            instance: candidate.instance, base_sha: candidate.base_sha, artifact: candidate.artifact,
        }).await.map_err(|e| CommandError::Internal(format!(
            "submission publication was not acknowledged; retry the same submission_id and selection: {e}")))?;
        Ok(WorkSubmitResult {
            submission_id: published.submission_id.as_uuid(),
            card_id: published.card_id.as_uuid(),
            room_id: room.channel.as_uuid(),
            publisher: published.publisher.as_uuid(),
            artifact: (&published.artifact).into(),
            submitted_at_ms: published.submitted_at_ms,
            bound_staged_revision_id: p.staged_revision_id,
        })
    }
}

crate::register_command!(WorkSubmit);

pub struct WorkReview {
    pub registry: PersonaAircRuntimeRegistry,
    pub executor_slot: Arc<LateBound<CommandExecutor>>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/work/ReviewOutcome.ts"
)]
pub enum ReviewOutcome {
    Passed,
    Failed,
    Unknown,
}

impl From<airc_work::WorkReviewOutcome> for ReviewOutcome {
    fn from(value: airc_work::WorkReviewOutcome) -> Self {
        match value {
            airc_work::WorkReviewOutcome::Passed => Self::Passed,
            airc_work::WorkReviewOutcome::Failed => Self::Failed,
            airc_work::WorkReviewOutcome::Unknown => Self::Unknown,
        }
    }
}

impl From<ReviewOutcome> for airc_work::WorkReviewOutcome {
    fn from(value: ReviewOutcome) -> Self {
        match value {
            ReviewOutcome::Passed => Self::Passed,
            ReviewOutcome::Failed => Self::Failed,
            ReviewOutcome::Unknown => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/work/WorkReviewParams.ts"
)]
pub struct WorkReviewParams {
    pub room: String,
    #[ts(type = "string")]
    pub review_id: Uuid,
    #[ts(type = "string")]
    pub card_id: Uuid,
    #[ts(type = "string")]
    pub submission_id: Uuid,
    pub artifact: WorkArtifactReference,
    #[ts(type = "string")]
    pub review_card_id: Uuid,
    #[ts(type = "string")]
    pub review_claim_id: Uuid,
    pub outcome: ReviewOutcome,
    pub evidence: WorkArtifactReference,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/work/WorkReviewResult.ts"
)]
pub struct WorkReviewResult {
    #[ts(type = "string")]
    pub review_id: Uuid,
    #[ts(type = "string")]
    pub submission_id: Uuid,
    #[ts(type = "string")]
    pub reviewer: Uuid,
    pub outcome: ReviewOutcome,
    #[ts(type = "number")]
    pub reviewed_at_ms: u64,
    #[ts(type = "string")]
    pub review_card_id: Uuid,
    #[ts(type = "string")]
    pub review_claim_id: Uuid,
    pub artifact: WorkArtifactReference,
    pub evidence: WorkArtifactReference,
    /// A review can publish even when its local learning transfer is deferred.
    pub credit: Option<reviewed::ReviewedCredit>,
    pub credit_error: Option<String>,
}

#[async_trait]
impl ActionCommand for WorkReview {
    const NAME: &'static str = "work/review";
    const NATIVE: bool = true;
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str = "Publish passed, failed or unknown judgement of an exact submission, under your linked review-card claim, with an evidence reference. Reuse review_id on retry. This attributes judgement; it does not claim the artifact was objectively graded or training completed.";
    type Params = WorkReviewParams;
    type Output = WorkReviewResult;

    async fn run(&self, ctx: &Ctx, p: WorkReviewParams) -> Result<Self::Output, CommandError> {
        if p.room.trim().is_empty() {
            return Err(CommandError::Invalid(
                "an explicit activity room is required".into(),
            ));
        }
        let runtime = persona_runtime(&self.registry, ctx, "work/review")?;
        let airc = runtime.airc();
        let room = crate::modules::room_resolve::resolve_room(airc, Some(&p.room)).await?;
        let review = airc
            .review_work_submission_in(
                &room,
                airc_lib::ReviewWorkSubmission {
                    review_id: airc_work::WorkReviewId::from_uuid(p.review_id),
                    card_id: WorkCardId::from_uuid(p.card_id),
                    submission_id: airc_work::SubmissionId::from_uuid(p.submission_id),
                    artifact: p.artifact.into_artifact()?,
                    review_card_id: WorkCardId::from_uuid(p.review_card_id),
                    review_claim_id: ClaimId::from_uuid(p.review_claim_id),
                    outcome: p.outcome.into(),
                    evidence: p.evidence.into_artifact()?,
                },
            )
            .await
            .map_err(|e| {
                CommandError::Invalid(format!(
                    "review not acknowledged; retry the same review_id and judgement: {e}"
                ))
            })?;
        // Re-read accepted signed projection even on the local command path.
        // Publication alone is not permission to bypass replay validation.
        let credit = async {
            let board = airc
                .work_board_in(&room)
                .await
                .map_err(|e| CommandError::Internal(e.to_string()))?;
            let executor = self
                .executor_slot
                .cloned()
                .ok_or_else(|| CommandError::Internal("work executor is not ready".into()))?;
            reviewed::consume_as_publisher(
                &self.registry,
                executor,
                room.channel.as_uuid(),
                &board,
                review.review_id,
            )
            .await
        }
        .await;
        let (credit, credit_error) = match credit {
            Ok(value) => (Some(value), None),
            Err(error) => (None, Some(error.to_string())),
        };
        Ok(WorkReviewResult {
            review_id: review.review_id.as_uuid(),
            submission_id: review.submission_id.as_uuid(),
            reviewer: review.reviewer.as_uuid(),
            outcome: p.outcome,
            reviewed_at_ms: review.reviewed_at_ms,
            review_card_id: review.review_card_id.as_uuid(),
            review_claim_id: review.review_claim_id.as_uuid(),
            artifact: (&review.artifact).into(),
            evidence: (&review.evidence).into(),
            credit,
            credit_error,
        })
    }
}

crate::register_command!(WorkReview);

pub struct WorkSubmission {
    pub registry: PersonaAircRuntimeRegistry,
    pub executor_slot: Arc<LateBound<CommandExecutor>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/work/WorkSubmissionParams.ts"
)]
pub struct WorkSubmissionParams {
    pub room: String,
    #[ts(type = "string")]
    pub card_id: Uuid,
    #[ts(type = "string")]
    pub submission_id: Uuid,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/work/WorkSubmissionResult.ts"
)]
pub struct WorkSubmissionResult {
    pub submission: WorkSubmitResult,
    #[ts(type = "string")]
    pub claim_id: Uuid,
    pub base_sha: String,
    pub instance: String,
    pub reviews: Vec<WorkReviewResult>,
    pub credit: Option<reviewed::ReviewedCredit>,
    pub credit_error: Option<String>,
}

#[async_trait]
impl ActionCommand for WorkSubmission {
    const NAME: &'static str = "work/submission";
    const NATIVE: bool = true;
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str = "Inspect one accepted submission, signed review evidence and local learning acceptance metadata in an explicit room. Does not return private training prompts or initiate training.";
    type Params = WorkSubmissionParams;
    type Output = WorkSubmissionResult;

    async fn run(&self, ctx: &Ctx, p: WorkSubmissionParams) -> Result<Self::Output, CommandError> {
        if p.room.trim().is_empty() {
            return Err(CommandError::Invalid(
                "an explicit activity room is required".into(),
            ));
        }
        let runtime = persona_runtime(&self.registry, ctx, "work/submission")?;
        let room =
            crate::modules::room_resolve::resolve_room(runtime.airc(), Some(&p.room)).await?;
        let board = runtime
            .airc()
            .work_board_in(&room)
            .await
            .map_err(|e| CommandError::Internal(e.to_string()))?;
        let submitted = board
            .card(WorkCardId::from_uuid(p.card_id))
            .and_then(|card| {
                card.submissions
                    .iter()
                    .find(|s| s.submission_id.as_uuid() == p.submission_id)
            })
            .ok_or_else(|| {
                CommandError::NotFound("submission is absent from this card and room".into())
            })?;
        let credit = async {
            let Some(owner) = self.registry.get(submitted.publisher.as_uuid()) else {
                return Ok(reviewed::ReviewedCredit::pending(
                    reviewed::ReviewedCreditState::PublisherNotResident,
                ));
            };
            if owner.airc().peer_id() != submitted.publisher {
                return Err(CommandError::Invalid(
                    "registered publisher identity differs".into(),
                ));
            }
            let executor = self
                .executor_slot
                .cloned()
                .ok_or_else(|| CommandError::Internal("work executor is not ready".into()))?;
            let conn = Connection::new(InProcessTransport::new(
                executor,
                Some(crate::routing::CallerIdentity::local_persona(
                    crate::identity::PeerId::from_uuid(owner.persona_id()),
                )),
            ));
            reviewed::credit_status(&conn, owner.agent_name(), p.submission_id)
                .await
                .map_err(|e| CommandError::Internal(e.to_string()))
        }
        .await;
        let (credit, credit_error) = match credit {
            Ok(value) => (Some(value), None),
            Err(e) => (None, Some(e.to_string())),
        };
        let reviews = board
            .submission_reviews_for(submitted.submission_id)
            .filter(|r| r.card_id == submitted.card_id)
            .map(|r| WorkReviewResult {
                review_id: r.review_id.as_uuid(),
                submission_id: r.submission_id.as_uuid(),
                reviewer: r.reviewer.as_uuid(),
                outcome: r.outcome.into(),
                reviewed_at_ms: r.reviewed_at_ms,
                review_card_id: r.review_card_id.as_uuid(),
                review_claim_id: r.review_claim_id.as_uuid(),
                artifact: (&r.artifact).into(),
                evidence: (&r.evidence).into(),
                credit: None,
                credit_error: None,
            })
            .collect();
        Ok(WorkSubmissionResult {
            submission: WorkSubmitResult {
                submission_id: p.submission_id,
                card_id: p.card_id,
                room_id: room.channel.as_uuid(),
                publisher: submitted.publisher.as_uuid(),
                artifact: (&submitted.artifact).into(),
                submitted_at_ms: submitted.submitted_at_ms,
                bound_staged_revision_id: credit.as_ref().and_then(|c| c.staged_revision_id),
            },
            claim_id: submitted.claim_id.as_uuid(),
            base_sha: submitted.base_sha.to_string(),
            instance: submitted.instance.clone(),
            reviews,
            credit,
            credit_error,
        })
    }
}

crate::register_command!(WorkSubmission);
