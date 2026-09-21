//! Exact work-submission ownership inside the existing per-persona credit store.
//!
//! A binding copies ONE selected revision before publishing its artifact. Periodic
//! staging may replace its working copy, but cannot rewrite this evidence. The
//! same atomic batch reserves every request id against another cumulative revision.
//! Reservations survive refusal and transfer: deleting them would permit training
//! the same generations again through a later artifact or the legacy grader.
//! Accepted exact-review replay also notifies the existing dream owner. Its bounded
//! scheduling cache is not durable completion: boot-time acceptance rescan remains
//! the retained-credit retry seam (a346); acceptance is never a training/adoption claim.

use super::*;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

/// Reuse the resident work bridge for remote and local review events. Header
/// filtering keeps ordinary chat and other work events off the storage path.
pub(crate) async fn bridge_review(event: &airc_core::TranscriptEvent, observer: Uuid) {
    if event
        .headers
        .get(airc_protocol::HEADER_FORGE_BODY_HINT)
        .is_none_or(|hint| hint != airc_work::BODY_HINT_FORGE_WORK_REVIEW)
    {
        return;
    }
    let result = async {
        let registry =
            crate::persona::PersonaAircRuntimeRegistry::try_global().ok_or_else(|| {
                crate::sdk_codegen::CommandError::Internal("persona registry is not ready".into())
            })?;
        let runtime = registry.get(observer).ok_or_else(|| {
            crate::sdk_codegen::CommandError::Internal("review observer is not registered".into())
        })?;
        let item = airc_work::decode_transcript_work_event(event)
            .map_err(|e| crate::sdk_codegen::CommandError::Invalid(e.to_string()))?;
        let airc_work::WorkEvent::WorkSubmissionReviewed(review) = item.event else {
            return Ok(None);
        };
        if review.reviewer != event.peer_id {
            return Err(crate::sdk_codegen::CommandError::Invalid(
                "review author differs from signed sender".into(),
            ));
        }
        let room = crate::modules::room_resolve::resolve_room(
            runtime.airc(),
            Some(&event.room_id.as_uuid().to_string()),
        )
        .await?;
        let board = runtime
            .airc()
            .work_board_in(&room)
            .await
            .map_err(|e| crate::sdk_codegen::CommandError::Internal(e.to_string()))?;
        if board.submission_review(review.review_id) != Some(&review) {
            return Err(crate::sdk_codegen::CommandError::Invalid(
                "review is not accepted in the room projection".into(),
            ));
        }
        let executor = EXECUTOR.cloned().ok_or_else(|| {
            crate::sdk_codegen::CommandError::Internal("credit executor is not ready".into())
        })?;
        let conn = Connection::new(InProcessTransport::new(
            executor,
            Some(CallerIdentity::local_persona(
                crate::identity::PeerId::from_uuid(observer),
            )),
        ));
        consume_observed_review(
            &conn,
            runtime.agent_name(),
            observer,
            room.channel.as_uuid(),
            &board,
            review.review_id,
        )
        .await
        .map_err(|e| crate::sdk_codegen::CommandError::Internal(e.to_string()))
    }
    .await;
    match result {
        Ok(Some(credit)) => {
            crate::probe!(class = "training.credit.reviewed", event = %event.event_id,
            state = ?credit.state, revision = ?credit.staged_revision_id, review = ?credit.decision_review_id,
            "signed review consumed; acceptance and training remain separate outcomes")
        }
        Ok(None) => {}
        Err(error) => {
            crate::probe!(class = "training.credit.review_deferred", event = %event.event_id,
            error = %error, "review evidence retained; an exact replay can retry the transfer")
        }
    }
}

/// Remote fan-out has ONE transfer owner: the submitted artifact's publisher.
/// Every observer may fold its accepted room projection, but unrelated residents
/// never open another persona's SQLite store or dispatch its training transfer.
pub(crate) async fn consume_observed_review<T: Transport>(
    conn: &Connection<T>,
    persona_name: &str,
    observer: Uuid,
    room_id: Uuid,
    board: &airc_work::WorkBoardProjection,
    review_id: airc_work::WorkReviewId,
) -> Result<Option<ReviewedCredit>, CreditBindingError> {
    let dream = crate::cognition::dream_consolidation::global();
    consume_observed_review_with_boundary(
        conn,
        persona_name,
        observer,
        room_id,
        board,
        review_id,
        dream.as_deref(),
    )
    .await
}

pub(super) async fn consume_observed_review_with_boundary<T: Transport>(
    conn: &Connection<T>,
    persona_name: &str,
    observer: Uuid,
    room_id: Uuid,
    board: &airc_work::WorkBoardProjection,
    review_id: airc_work::WorkReviewId,
    dream: Option<&crate::cognition::dream_consolidation::DreamConsolidationRegion>,
) -> Result<Option<ReviewedCredit>, CreditBindingError> {
    let review = board
        .submission_review(review_id)
        .ok_or(CreditBindingError::WrongSelection)?;
    let submitted = board
        .card(review.card_id)
        .and_then(|card| {
            card.submissions
                .iter()
                .find(|s| s.submission_id == review.submission_id)
        })
        .ok_or(CreditBindingError::WrongSelection)?;
    if submitted.publisher.as_uuid() != observer {
        return Ok(None);
    }
    consume_review_with_boundary(
        conn,
        persona_name,
        observer,
        room_id,
        board,
        review_id,
        dream,
    )
    .await
    .map(Some)
}

pub(crate) async fn consume_as_publisher(
    registry: &crate::persona::PersonaAircRuntimeRegistry,
    executor: Arc<CommandExecutor>,
    room_id: Uuid,
    board: &airc_work::WorkBoardProjection,
    review_id: airc_work::WorkReviewId,
) -> Result<ReviewedCredit, crate::sdk_codegen::CommandError> {
    let review = board.submission_review(review_id).ok_or_else(|| {
        crate::sdk_codegen::CommandError::Invalid("review is not accepted in this room".into())
    })?;
    let submitted = board
        .card(review.card_id)
        .and_then(|card| {
            card.submissions
                .iter()
                .find(|s| s.submission_id == review.submission_id)
        })
        .ok_or_else(|| {
            crate::sdk_codegen::CommandError::Invalid("review has no accepted submission".into())
        })?;
    let Some(runtime) = registry.get(submitted.publisher.as_uuid()) else {
        return Ok(ReviewedCredit::pending(
            ReviewedCreditState::PublisherNotResident,
        ));
    };
    if runtime.airc().peer_id() != submitted.publisher {
        return Err(crate::sdk_codegen::CommandError::Invalid(
            "registered publisher identity differs".into(),
        ));
    }
    // The publisher must actually inhabit this activity; never substitute focus.
    crate::modules::room_resolve::resolve_room(runtime.airc(), Some(&room_id.to_string())).await?;
    let conn = Connection::new(InProcessTransport::new(
        executor,
        Some(CallerIdentity::local_persona(
            crate::identity::PeerId::from_uuid(runtime.persona_id()),
        )),
    ));
    consume_review(
        &conn,
        runtime.agent_name(),
        runtime.persona_id(),
        room_id,
        board,
        review_id,
    )
    .await
    .map_err(|e| crate::sdk_codegen::CommandError::Internal(e.to_string()))
}

/// Immutable selection supplied to work/submit. It is a publication request,
/// not an AIRC publication receipt or a statement that the artifact was verified.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmissionSelection {
    pub submission_id: Uuid,
    pub room_id: Uuid,
    pub card_id: Uuid,
    pub claim_id: Uuid,
    pub staged_revision_id: Uuid,
    pub instance: String,
    pub base_sha: airc_work::GitObjectId,
    pub artifact: airc_work::SubmissionArtifact,
}

/// Durable evidence retained independently of the mutable staging working set.
#[derive(Debug, Clone, Serialize, Deserialize, crate::orm::Entity)]
#[serde(rename_all = "camelCase")]
#[entity(collection = "work_credit_binding")]
pub struct WorkCreditBinding {
    #[entity(primary_key)]
    pub id: Uuid,
    pub persona_id: Uuid,
    #[entity(indexed)]
    pub card_id: Uuid,
    #[entity(json)]
    pub selection: SubmissionSelection,
    #[entity(foreign_key("credit_transfer_intent.id", on_delete = "restrict"))]
    pub transfer_intent_id: Uuid,
}

/// One durable payload owner for ordinary review and legacy grader transfers.
/// A working-set replacement cannot strand reservations without retryable data.
#[derive(Debug, Clone, Serialize, Deserialize, crate::orm::Entity)]
#[serde(rename_all = "camelCase")]
#[entity(collection = "credit_transfer_intent")]
pub struct CreditTransferIntent {
    #[entity(primary_key)]
    pub id: Uuid,
    pub work_submission_id: Option<Uuid>,
    #[entity(json)]
    pub snapshot: StagedCredit,
}

/// Destination ownership for a transfer, independent of a review or training.
/// Create-only acknowledgement retains the source evidence and reservations.
#[derive(Debug, Clone, Serialize, Deserialize, crate::orm::Entity)]
#[serde(rename_all = "camelCase")]
#[entity(collection = "credit_transfer_acceptance")]
pub struct CreditTransferAcceptance {
    #[entity(primary_key)]
    pub id: Uuid,
    #[entity(foreign_key("credit_transfer_intent.id", on_delete = "restrict"))]
    pub transfer_intent_id: Uuid,
    #[entity(json)]
    pub receipt: SubmitOutcome,
}

/// A request id is reserved within its persona's store, not globally across
/// peers. Reused/ambiguous request ids conservatively refuse overlapping credit.
/// None denotes the existing benchmark settlement path; Some binds the exact
/// ordinary submission. Neither path may borrow the other's reservation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, crate::orm::Entity)]
#[serde(rename_all = "camelCase")]
#[entity(collection = "credit_generation_reservation")]
pub struct CreditGenerationReservation {
    #[entity(primary_key)]
    pub id: String,
    #[entity(foreign_key("credit_transfer_intent.id", on_delete = "restrict"))]
    pub revision_id: Uuid,
    pub work_submission_id: Option<Uuid>,
}

/// First eligible independent judgement selected for this immutable binding.
/// Persisted before dispatch: concurrent reviews cannot change retry metadata.
#[derive(Debug, Clone, Serialize, Deserialize, crate::orm::Entity)]
#[serde(rename_all = "camelCase")]
#[entity(collection = "credit_review_decision")]
pub struct CreditReviewDecision {
    #[entity(primary_key)]
    pub id: Uuid,
    #[entity(foreign_key("work_credit_binding.id", on_delete = "restrict"))]
    pub binding_id: Uuid,
    #[entity(json)]
    pub review: airc_work::WorkSubmissionReview,
}

/// Destination ownership, separate from the judgement and from training itself.
/// Immutable publication prevents a late refused retry overwriting acceptance.
#[derive(Debug, Clone, Serialize, Deserialize, crate::orm::Entity)]
#[serde(rename_all = "camelCase")]
#[entity(collection = "credit_review_acceptance")]
pub struct CreditReviewAcceptance {
    #[entity(primary_key)]
    pub id: Uuid,
    #[entity(foreign_key("credit_review_decision.id", on_delete = "restrict"))]
    pub decision_id: Uuid,
    pub review_id: Uuid,
    #[entity(json)]
    pub receipt: SubmitOutcome,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/work/ReviewedCreditState.ts"
)]
pub enum ReviewedCreditState {
    Unbound,
    AwaitingReview,
    FailedReview,
    UnknownReview,
    IndependentReviewRequired,
    IneligibleEvidence,
    AnotherReviewSelected,
    AwaitingAcceptance,
    Accepted,
    PublisherNotResident,
}

/// Safe public projection: identities and receipts, never the training payload.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/work/ReviewedCredit.ts"
)]
pub struct ReviewedCredit {
    pub state: ReviewedCreditState,
    #[ts(type = "string | null")]
    pub staged_revision_id: Option<Uuid>,
    #[ts(type = "string | null")]
    pub decision_review_id: Option<Uuid>,
    pub destination: Option<SubmitOutcome>,
}

impl ReviewedCredit {
    pub fn pending(state: ReviewedCreditState) -> Self {
        Self {
            state,
            staged_revision_id: None,
            decision_review_id: None,
            destination: None,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CreditBindingError {
    #[error("credit storage: {0}")]
    Storage(#[from] ClientError),
    #[error("staged revision {0} is absent; no publication binding was created")]
    MissingRevision(Uuid),
    #[error("selected credit does not belong to this persona, card and accepted claim")]
    WrongSelection,
    #[error("submission {0} is already bound to another immutable selection")]
    ConflictingSubmission(Uuid),
    #[error("generation evidence is empty, repeated or already reserved by another revision")]
    Overlap,
}

pub async fn ensure_storage<T: Transport>(
    conn: &Connection<T>,
    persona_name: &str,
) -> Result<(), ClientError> {
    for collection in [
        StagedCredit::COLLECTION,
        CreditTransferIntent::COLLECTION,
        CreditTransferAcceptance::COLLECTION,
        WorkCreditBinding::COLLECTION,
        CreditGenerationReservation::COLLECTION,
        CreditReviewDecision::COLLECTION,
        CreditReviewAcceptance::COLLECTION,
    ] {
        let result = conn
            .commands()
            .execute_value(
                "data/ensure-schema",
                json!({
                    "collection": collection, "dbPath": format!("@persona:{persona_name}")
                }),
            )
            .await?;
        storage_ok(&result, "data/ensure-schema", collection)?;
    }
    Ok(())
}

/// Consume only an accepted AIRC projection, never an arbitrary decoded review.
/// The projection verified signed author and the historical review-card claim.
/// A pass remains reviewer judgement; this does not fetch or grade the artifact.
pub async fn consume_review<T: Transport>(
    conn: &Connection<T>,
    persona_name: &str,
    persona_id: Uuid,
    room_id: Uuid,
    board: &airc_work::WorkBoardProjection,
    review_id: airc_work::WorkReviewId,
) -> Result<ReviewedCredit, CreditBindingError> {
    let dream = crate::cognition::dream_consolidation::global();
    consume_review_with_boundary(
        conn,
        persona_name,
        persona_id,
        room_id,
        board,
        review_id,
        dream.as_deref(),
    )
    .await
}

pub(super) async fn consume_review_with_boundary<T: Transport>(
    conn: &Connection<T>,
    persona_name: &str,
    persona_id: Uuid,
    room_id: Uuid,
    board: &airc_work::WorkBoardProjection,
    review_id: airc_work::WorkReviewId,
    dream: Option<&crate::cognition::dream_consolidation::DreamConsolidationRegion>,
) -> Result<ReviewedCredit, CreditBindingError> {
    let credit =
        consume_review_credit(conn, persona_name, persona_id, room_id, board, review_id).await?;
    if credit.state == ReviewedCreditState::Accepted
        && credit.decision_review_id == Some(review_id.as_uuid())
    {
        if let Some(dream) = dream {
            // First acceptance AND exact replay use the same scheduling owner. A
            // cancellation after durable acceptance can retry; another review's
            // status, a refusal, and unrelated observers cannot request a pass.
            let submission_id = board
                .submission_review(review_id)
                .ok_or(CreditBindingError::WrongSelection)?
                .submission_id
                .as_uuid();
            dream.request_reviewed_boundary(persona_id, submission_id);
        }
    }
    Ok(credit)
}

async fn consume_review_credit<T: Transport>(
    conn: &Connection<T>,
    persona_name: &str,
    persona_id: Uuid,
    room_id: Uuid,
    board: &airc_work::WorkBoardProjection,
    review_id: airc_work::WorkReviewId,
) -> Result<ReviewedCredit, CreditBindingError> {
    let review = board
        .submission_review(review_id)
        .ok_or(CreditBindingError::WrongSelection)?;
    let submitted = board
        .card(review.card_id)
        .and_then(|card| {
            card.submissions
                .iter()
                .find(|s| s.submission_id == review.submission_id)
        })
        .ok_or(CreditBindingError::WrongSelection)?;
    if submitted.publisher.as_uuid() != persona_id || submitted.artifact != review.artifact {
        return Err(CreditBindingError::WrongSelection);
    }
    ensure_storage(conn, persona_name).await?;
    let id = submitted.submission_id.as_uuid();
    let Some(binding) =
        read_one::<_, WorkCreditBinding>(conn, persona_name, &id.to_string()).await?
    else {
        return Ok(ReviewedCredit::pending(ReviewedCreditState::Unbound));
    };
    let selection = &binding.selection;
    if binding.persona_id != persona_id
        || selection.room_id != room_id
        || selection.card_id != submitted.card_id.as_uuid()
        || selection.claim_id != submitted.claim_id.as_uuid()
        || selection.instance != submitted.instance
        || selection.base_sha != submitted.base_sha
        || selection.artifact != submitted.artifact
        || selection.submission_id != id
    {
        return Err(CreditBindingError::WrongSelection);
    }
    let mut result = credit_status(conn, persona_name, id).await?;
    if result.state == ReviewedCreditState::Accepted {
        return Ok(result);
    }
    result.state = match review.outcome {
        airc_work::WorkReviewOutcome::Failed => ReviewedCreditState::FailedReview,
        airc_work::WorkReviewOutcome::Unknown => ReviewedCreditState::UnknownReview,
        airc_work::WorkReviewOutcome::Passed if review.reviewer == submitted.publisher => {
            ReviewedCreditState::IndependentReviewRequired
        }
        airc_work::WorkReviewOutcome::Passed => ReviewedCreditState::AwaitingAcceptance,
    };
    if result.state != ReviewedCreditState::AwaitingAcceptance {
        return Ok(result);
    }
    let intent = read_one::<_, CreditTransferIntent>(
        conn,
        persona_name,
        &binding.transfer_intent_id.to_string(),
    )
    .await?
    .ok_or(CreditBindingError::MissingRevision(
        binding.transfer_intent_id,
    ))?;
    let row = &intent.snapshot;
    if intent.work_submission_id != Some(id)
        || row.id != selection.staged_revision_id
        || row.card_id != selection.card_id
        || row.claim_id != Some(selection.claim_id)
        || row.owner != Some(persona_id)
    {
        return Err(CreditBindingError::WrongSelection);
    }
    let Some(mut params) = staged_submission_params(persona_id, persona_name, row, true) else {
        result.state = ReviewedCreditState::IneligibleEvidence;
        return Ok(result);
    };
    reserve_transfer(conn, persona_name, row, Some(id)).await?;
    let decision = CreditReviewDecision {
        id,
        binding_id: id,
        review: review.clone(),
    };
    let prior = read_one::<_, CreditReviewDecision>(conn, persona_name, &id.to_string()).await?;
    let decision = if let Some(prior) = prior {
        prior
    } else {
        let write = write_batch(conn, persona_name, vec![create(id.to_string(), &decision)?]).await;
        match write {
            Ok(()) => decision,
            Err(error) => read_one::<_, CreditReviewDecision>(conn, persona_name, &id.to_string())
                .await?
                .ok_or(error)?,
        }
    };
    result.decision_review_id = Some(decision.review.review_id.as_uuid());
    if decision.binding_id != id || decision.review != *review {
        result.state = ReviewedCreditState::AnotherReviewSelected;
        return Ok(result);
    }
    // Encode provenance once into the actual destination example. The payload is
    // immutable across a lost acknowledgement and across periodic replacement.
    params["examples"][0]["metadata"]["reviewedSubmission"] = json!({
        "selection": selection, "review": &decision.review, "generationReceipts": &row.receipts
    });
    let receipt = submit_training(conn, params).await?;
    if receipt
        .acceptance
        .as_ref()
        .is_none_or(|accepted| accepted.submission_id != row.id)
    {
        result.destination = Some(receipt);
        return Ok(result);
    }
    let accepted = CreditReviewAcceptance {
        id,
        decision_id: id,
        review_id: review_id.as_uuid(),
        receipt,
    };
    if let Err(error) =
        write_batch(conn, persona_name, vec![create(id.to_string(), &accepted)?]).await
    {
        let prior =
            read_one::<_, CreditReviewAcceptance>(conn, persona_name, &id.to_string()).await?;
        if prior.as_ref().is_none_or(|prior| {
            prior.decision_id != id
                || prior.review_id != accepted.review_id
                || prior
                    .receipt
                    .acceptance
                    .as_ref()
                    .is_none_or(|a| a.submission_id != row.id)
        }) {
            return Err(error.into());
        }
    }
    // No evidence deletion: binding and immutable intent remain inspectable. The
    // mutable working copy may already be absent or belong to a newer revision.
    credit_status(conn, persona_name, id)
        .await
        .map_err(Into::into)
}

pub async fn credit_status<T: Transport>(
    conn: &Connection<T>,
    persona_name: &str,
    submission_id: Uuid,
) -> Result<ReviewedCredit, ClientError> {
    let id = submission_id.to_string();
    let Some(binding) = read_one::<_, WorkCreditBinding>(conn, persona_name, &id).await? else {
        return Ok(ReviewedCredit::pending(ReviewedCreditState::Unbound));
    };
    let decision = read_one::<_, CreditReviewDecision>(conn, persona_name, &id).await?;
    let accepted = read_one::<_, CreditReviewAcceptance>(conn, persona_name, &id).await?;
    if accepted.as_ref().is_some_and(|accepted| {
        accepted.decision_id != submission_id
            || decision.as_ref().is_none_or(|decision| {
                decision.binding_id != submission_id
                    || decision.review.review_id.as_uuid() != accepted.review_id
            })
            || accepted
                .receipt
                .acceptance
                .as_ref()
                .is_none_or(|receipt| receipt.submission_id != binding.selection.staged_revision_id)
    }) {
        return Err(ClientError::Transport(
            "review acceptance does not match its decision and selected revision".into(),
        ));
    }
    Ok(ReviewedCredit {
        state: if accepted.is_some() {
            ReviewedCreditState::Accepted
        } else if decision.is_some() {
            ReviewedCreditState::AwaitingAcceptance
        } else {
            ReviewedCreditState::AwaitingReview
        },
        staged_revision_id: Some(binding.selection.staged_revision_id),
        decision_review_id: decision.map(|d| d.review.review_id.as_uuid()),
        destination: accepted.map(|a| a.receipt),
    })
}

/// An indexed one-row list distinguishes absence from storage/decoding failure;
/// the older data/read result represents both as an error string.
pub async fn read_one<T: Transport, E: OrmEntity + DeserializeOwned>(
    conn: &Connection<T>,
    persona_name: &str,
    id: &str,
) -> Result<Option<E>, ClientError> {
    let value = conn
        .commands()
        .execute_value(
            "data/list",
            json!({
                "collection": E::COLLECTION, "dbPath": format!("@persona:{persona_name}"),
                "filter": {"id": id}, "limit": 1
            }),
        )
        .await?;
    let mut result: crate::modules::data::DataListResult = serde_json::from_value(value)?; // Decode the data/list command envelope once at the storage boundary.
    if result.total > 1 || result.items.len() != result.total as usize {
        return Err(ClientError::Transport(
            "incomplete or nonunique credit lookup".into(),
        ));
    }
    result
        .items
        .pop()
        .map(|value| {
            let record: crate::orm::types::DataRecord = serde_json::from_value(value)?; // Decode the ORM envelope before its typed entity payload.
            Ok(serde_json::from_value(record.data)?) // The entity payload crosses the Value-native storage boundary here.
        })
        .transpose()
}

fn create<E: OrmEntity + Serialize>(id: String, entity: &E) -> Result<BatchOperation, ClientError> {
    Ok(BatchOperation {
        operation_type: BatchOperationType::Create,
        collection: E::COLLECTION.into(),
        id: Some(id),
        data: Some(serde_json::to_value(entity)?), // Encode one entity into the ORM batch's required Value payload.
    })
}

fn create_intent(row: &StagedCredit, work_submission_id: Option<Uuid>) -> BatchOperation {
    BatchOperation {
        operation_type: BatchOperationType::Create,
        collection: CreditTransferIntent::COLLECTION.into(),
        id: Some(row.id.to_string()),
        // Encode the borrowed snapshot once at the storage boundary; retaining
        // a second Rust clone of its complete prompt/act chain is unnecessary.
        data: Some(json!({"id": row.id, "workSubmissionId": work_submission_id, "snapshot": row})),
    }
}

fn same_intent(
    intent: &CreditTransferIntent,
    row: &StagedCredit,
    work_submission_id: Option<Uuid>,
) -> bool {
    let prior = &intent.snapshot;
    // A retry may have a later staging clock. The actual immutable credit
    // payload and all selected/provenance identities must remain identical.
    intent.id == row.id
        && intent.work_submission_id == work_submission_id
        && prior.id == row.id
        && prior.card_id == row.card_id
        && prior.claim_id == row.claim_id
        && prior.owner == row.owner
        && prior.role == row.role
        && prior.receipts == row.receipts
        && prior.served == row.served
        && prior.prompt == row.prompt
        && prior.completion == row.completion
}

async fn write_batch<T: Transport>(
    conn: &Connection<T>,
    persona_name: &str,
    operations: Vec<BatchOperation>,
) -> Result<(), ClientError> {
    let value = conn
        .commands()
        .execute_value(
            "data/batch",
            json!({
                "operations": operations, "dbPath": format!("@persona:{persona_name}")
            }),
        )
        .await?;
    storage_ok(&value, "data/batch", WorkCreditBinding::COLLECTION)
}

pub(super) async fn transfer_accepted<T: Transport>(
    conn: &Connection<T>,
    persona_name: &str,
    revision: Uuid,
) -> Result<bool, ClientError> {
    let prior =
        read_one::<_, CreditTransferAcceptance>(conn, persona_name, &revision.to_string()).await?;
    Ok(prior.is_some_and(|prior| {
        prior.id == revision
            && prior.transfer_intent_id == revision
            && prior
                .receipt
                .acceptance
                .as_ref()
                .is_some_and(|a| a.submission_id == revision)
    }))
}

/// Called only after exact-intent reservation and destination identity validation.
/// A competing successful retry may win this create; a refusal never overwrites it.
pub(super) async fn accept_transfer<T: Transport>(
    conn: &Connection<T>,
    persona_name: &str,
    revision: Uuid,
    receipt: SubmitOutcome,
) -> Result<(), ClientError> {
    if receipt
        .acceptance
        .as_ref()
        .is_none_or(|a| a.submission_id != revision)
    {
        return Err(ClientError::Transport(
            "destination did not accept this revision".into(),
        ));
    }
    let accepted = CreditTransferAcceptance {
        id: revision,
        transfer_intent_id: revision,
        receipt,
    };
    if let Err(error) = write_batch(
        conn,
        persona_name,
        vec![create(revision.to_string(), &accepted)?],
    )
    .await
    {
        if !transfer_accepted(conn, persona_name, revision).await? {
            return Err(error);
        }
    }
    Ok(())
}

fn reservations(
    row: &StagedCredit,
    work_submission_id: Option<Uuid>,
) -> Result<Vec<CreditGenerationReservation>, CreditBindingError> {
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::with_capacity(row.receipts.len());
    for receipt in &row.receipts {
        let id = &receipt.submitted_request_id;
        if id.is_empty() || !seen.insert(id.as_str()) {
            return Err(CreditBindingError::Overlap);
        }
        result.push(CreditGenerationReservation {
            id: id.clone(),
            revision_id: row.id,
            work_submission_id,
        });
    }
    if result.is_empty() {
        return Err(CreditBindingError::Overlap);
    }
    Ok(result)
}

fn same_selection(
    binding: WorkCreditBinding,
    persona_id: Uuid,
    selection: &SubmissionSelection,
) -> Result<WorkCreditBinding, CreditBindingError> {
    if binding.persona_id != persona_id || &binding.selection != selection {
        return Err(CreditBindingError::ConflictingSubmission(
            selection.submission_id,
        ));
    }
    Ok(binding)
}

/// Publish only after this returns. A cancelled/uncertain write is resolved by
/// rereading the immutable binding on retry, never by repeating a new selection.
pub async fn bind_submission<T: Transport>(
    conn: &Connection<T>,
    persona_name: &str,
    persona_id: Uuid,
    selection: SubmissionSelection,
) -> Result<WorkCreditBinding, CreditBindingError> {
    ensure_storage(conn, persona_name).await?;
    let id = selection.submission_id.to_string();
    if let Some(prior) = read_one::<_, WorkCreditBinding>(conn, persona_name, &id).await? {
        return same_selection(prior, persona_id, &selection);
    }
    let snapshot = read_one::<_, StagedCredit>(
        conn,
        persona_name,
        &selection.staged_revision_id.to_string(),
    )
    .await?
    .ok_or(CreditBindingError::MissingRevision(
        selection.staged_revision_id,
    ))?;
    if snapshot.card_id != selection.card_id
        || snapshot.claim_id != Some(selection.claim_id)
        || snapshot.owner != Some(persona_id)
        || snapshot.role.is_none()
        || snapshot.id != selection.staged_revision_id
    {
        return Err(CreditBindingError::WrongSelection);
    }
    let reserved = reservations(&snapshot, Some(selection.submission_id))?;
    let binding = WorkCreditBinding {
        id: selection.submission_id,
        persona_id,
        card_id: selection.card_id,
        transfer_intent_id: snapshot.id,
        selection,
    };
    let mut operations = vec![
        create_intent(&snapshot, Some(binding.id)),
        create(id.clone(), &binding)?,
    ];
    for reservation in reserved {
        operations.push(create(reservation.id.clone(), &reservation)?);
    }
    if let Err(error) = write_batch(conn, persona_name, operations).await {
        // Another identical caller, or a committed write with a lost receipt.
        // The transaction cannot leave a binding with only some reservations.
        if let Some(prior) = read_one::<_, WorkCreditBinding>(conn, persona_name, &id).await? {
            return same_selection(prior, persona_id, &binding.selection);
        }
        return Err(error.into());
    }
    Ok(binding)
}

/// Shared by the existing grader and the reviewed-submission consumer. An
/// idempotent retry must retain EVERY exact reservation. A different revision,
/// including a cumulative successor, cannot receive the previous verdict.
pub async fn reserve_transfer<T: Transport>(
    conn: &Connection<T>,
    persona_name: &str,
    row: &StagedCredit,
    work_submission_id: Option<Uuid>,
) -> Result<(), CreditBindingError> {
    ensure_storage(conn, persona_name).await?;
    let expected = reservations(row, work_submission_id)?;
    let existing = read_reservations(conn, persona_name, &expected).await?;
    let mut operations = Vec::new();
    match read_one::<_, CreditTransferIntent>(conn, persona_name, &row.id.to_string()).await? {
        Some(prior) if same_intent(&prior, row, work_submission_id) => {}
        Some(_) => return Err(CreditBindingError::Overlap),
        None if work_submission_id.is_some() => return Err(CreditBindingError::Overlap),
        None => operations.push(create_intent(row, None)),
    }
    for reservation in &expected {
        match existing.get(&reservation.id) {
            Some(prior) if prior == reservation => {}
            Some(_) => return Err(CreditBindingError::Overlap),
            None if work_submission_id.is_some() => return Err(CreditBindingError::Overlap),
            None => operations.push(create(reservation.id.clone(), reservation)?),
        }
    }
    if operations.is_empty() {
        return Ok(());
    }
    if let Err(error) = write_batch(conn, persona_name, operations).await {
        // The unique keys decide races, including a concurrent binding. A
        // cancelled successful transaction is reusable only by this exact row.
        let existing = read_reservations(conn, persona_name, &expected).await?;
        for reservation in &expected {
            match existing.get(&reservation.id) {
                Some(prior) if prior == reservation => {}
                Some(_) => return Err(CreditBindingError::Overlap),
                None => return Err(error.into()),
            }
        }
        let intent =
            read_one::<_, CreditTransferIntent>(conn, persona_name, &row.id.to_string()).await?;
        if intent
            .as_ref()
            .is_none_or(|intent| !same_intent(intent, row, work_submission_id))
        {
            return Err(error.into());
        }
    }
    Ok(())
}

async fn read_reservations<T: Transport>(
    conn: &Connection<T>,
    persona_name: &str,
    expected: &[CreditGenerationReservation],
) -> Result<std::collections::HashMap<String, CreditGenerationReservation>, ClientError> {
    let ids: Vec<&str> = expected.iter().map(|r| r.id.as_str()).collect();
    let limit = u32::try_from(ids.len())
        .map_err(|_| ClientError::Transport("too many generation reservations".into()))?;
    let value = conn
        .commands()
        .execute_value(
            "data/list",
            json!({
                "collection": CreditGenerationReservation::COLLECTION,
                "dbPath": format!("@persona:{persona_name}"),
                "filter": {"id": {"$in": ids}}, "limit": limit
            }),
        )
        .await?;
    let result: crate::modules::data::DataListResult = serde_json::from_value(value)?; // Decode the indexed data/list response at the storage boundary.
    if result.items.len() != result.total as usize {
        return Err(ClientError::Transport(
            "incomplete generation reservation lookup".into(),
        ));
    }
    let mut rows = std::collections::HashMap::with_capacity(result.items.len());
    for value in result.items {
        let record: crate::orm::types::DataRecord = serde_json::from_value(value)?; // Decode the data/list ORM envelope before moving its payload.
        let row: CreditGenerationReservation = serde_json::from_value(record.data)?; // Decode the entity once at its storage boundary.
        if rows.insert(row.id.clone(), row).is_some() {
            return Err(ClientError::Transport(
                "duplicate generation reservation".into(),
            ));
        }
    }
    Ok(rows)
}
