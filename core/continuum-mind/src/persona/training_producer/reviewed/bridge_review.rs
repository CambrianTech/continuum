use super::*;

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
        let Some(verified) = verified_review(event, runtime.airc()).await? else {
            return Ok(None);
        };
        // An ordinary submission review settles ONLY through the exact binding (Codex
        // root, #5017): a card-wide settle credited every resident's staged rows before
        // exact validation. Card-wide settlement stays for bounded benchmark evidence.
        let VerifiedReview { review, room, board } = verified;
        let admission = self_review(runtime.airc(), &room).await;
        let conn = credit_connection(observer)?;
        consume_observed_review(
            &conn,
            runtime.agent_name(),
            observer,
            room.channel.as_uuid(),
            &board,
            review.review_id,
            &admission,
        )
        .await
        .map_err(|e| crate::sdk_codegen::CommandError::Internal(e.to_string()))
    }
    .await;
    match result {
        Ok(Some(credit)) => {
            continuum_base::probe!(class = "training.credit.reviewed", event = %event.event_id,
            state = ?credit.state, revision = ?credit.staged_revision_id, review = ?credit.decision_review_id,
            "signed review consumed; acceptance and training remain separate outcomes")
        }
        Ok(None) => {}
        Err(error) => {
            continuum_base::probe!(class = "training.credit.review_deferred", event = %event.event_id,
            error = %error, "review evidence retained; an exact replay can retry the transfer")
        }
    }
}

/// A review event that passed every check a settlement may rely on.
pub(crate) struct VerifiedReview {
    pub review: airc_work::WorkSubmissionReview,
    pub room: airc_lib::Room,
    pub board: airc_work::WorkBoardProjection,
}

/// Why a review event may not settle anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReviewRefusal {
    /// The review names an author other than the event's signed sender.
    NotSignedByReviewer,
    /// The room's projection does not hold this exact review.
    NotAccepted,
}

impl std::fmt::Display for ReviewRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            ReviewRefusal::NotSignedByReviewer => "review author differs from signed sender",
            ReviewRefusal::NotAccepted => "review is not accepted in the room projection",
        })
    }
}

/// THE event-level check a review passes before it may reach the exact path: it is signed
/// by its author and the room's projection holds it. Pure. Whether a self-review earns
/// credit is not decided here: [`SelfReview`] decides it once, in the exact path every
/// entry shares.
pub(crate) fn check_review(
    signer: crate::identity::PeerId,
    review: &airc_work::WorkSubmissionReview,
    accepted: Option<&airc_work::WorkSubmissionReview>,
) -> Result<(), ReviewRefusal> {
    if review.reviewer != signer {
        return Err(ReviewRefusal::NotSignedByReviewer);
    }
    if accepted != Some(review) {
        return Err(ReviewRefusal::NotAccepted);
    }
    Ok(())
}

/// Whether an author's review of her OWN submission may earn credit in a room. Only where
/// the room declares `self: true`. A room that declares no policy keeps the exact path's
/// rule (an independent review is required); a policy that cannot be read is `Unknown`,
/// which settles nothing now and is never admitted. Its retry owner is her registration
/// replay (`reconcile_reviews`, each core start), not an assumed next delivery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelfReview {
    Admitted,
    NotAdmitted,
    Unknown(String),
}

/// THE self-review rule, read from the room's policy. Every entry (the live bridge,
/// `work/review`, registration replay) passes its answer to the exact path.
pub(crate) async fn self_review(airc: &airc_lib::Airc, room: &airc_lib::Room) -> SelfReview {
    use crate::persona::work_board::review::PolicyRead;
    match crate::persona::work_board::review::room_policy(airc, room).await {
        PolicyRead::Declared(policy) if policy.self_review => SelfReview::Admitted,
        PolicyRead::Declared(_) | PolicyRead::Absent => SelfReview::NotAdmitted,
        PolicyRead::Unavailable(why) => SelfReview::Unknown(why),
    }
}

/// Decode `event` as a review and run [`check_review`] against the room it was
/// signed in. `Ok(None)` = not a review event; `Err` = a review that may not settle.
pub(crate) async fn verified_review(
    event: &airc_core::TranscriptEvent,
    airc: &airc_lib::Airc,
) -> Result<Option<VerifiedReview>, crate::sdk_codegen::CommandError> {
    let item = airc_work::decode_transcript_work_event(event)
        .map_err(|e| crate::sdk_codegen::CommandError::Invalid(e.to_string()))?;
    let airc_work::WorkEvent::WorkSubmissionReviewed(review) = item.event else {
        return Ok(None);
    };
    let room = crate::persona::room_resolve::resolve_room(airc, Some(&event.room_id.as_uuid().to_string())).await?;
    let board = airc
        .work_board_in(&room)
        .await
        .map_err(|e| crate::sdk_codegen::CommandError::Internal(e.to_string()))?;
    check_review(event.peer_id, &review, board.submission_review(review.review_id)).map_err(|refusal| {
        // The projection keeps WHY it refused a review (lease expired, not the review card's
        // owner, a settled card...): name it, or say it has not folded the review yet, so a
        // deferred credit is diagnosable from the probe alone.
        let why = match board.last_review_rejection(review.card_id).filter(|rejected| rejected.review_id == review.review_id) {
            Some(rejected) => format!("{refusal}: {}", rejected.reason),
            None if refusal == ReviewRefusal::NotAccepted => format!("{refusal}: not folded into this projection yet"),
            None => refusal.to_string(),
        };
        crate::sdk_codegen::CommandError::Invalid(why)
    })?;
    Ok(Some(VerifiedReview { review, room, board }))
}

/// The in-process connection a persona's credit store is reached through.
pub fn credit_connection(
    persona: Uuid,
) -> Result<Connection<InProcessTransport>, crate::sdk_codegen::CommandError> {
    let executor = EXECUTOR.cloned().ok_or_else(|| {
        crate::sdk_codegen::CommandError::Internal("credit executor is not ready".into())
    })?;
    Ok(Connection::new(InProcessTransport::new(
        executor,
        Some(CallerIdentity::local_persona(crate::identity::PeerId::from_uuid(persona))),
    )))
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
    self_review: &SelfReview,
) -> Result<Option<ReviewedCredit>, CreditBindingError> {
    let dream = crate::cognition::dream_consolidation::global();
    consume_observed_review_with_boundary(
        conn,
        persona_name,
        observer,
        room_id,
        board,
        review_id,
        self_review,
        dream.as_deref(),
    )
    .await
}

pub async fn consume_observed_review_with_boundary<T: Transport>(
    conn: &Connection<T>,
    persona_name: &str,
    observer: Uuid,
    room_id: Uuid,
    board: &airc_work::WorkBoardProjection,
    review_id: airc_work::WorkReviewId,
    self_review: &SelfReview,
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
        self_review,
        dream,
    )
    .await
    .map(Some)
}

pub async fn consume_as_publisher(
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
    let room = crate::persona::room_resolve::resolve_room(runtime.airc(), Some(&room_id.to_string())).await?;
    let admission = self_review(runtime.airc(), &room).await;
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
        &admission,
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
    /// Publication recorded the issuing generation, but it does not yet bind: not staged,
    /// shared with another submission's row, or not the publication on the board. Held, never
    /// passed; retried at the next review event and at registration replay.
    ProvenanceHeld,
    AwaitingReview,
    FailedReview,
    UnknownReview,
    IndependentReviewRequired,
    /// Her own review, in a room whose review policy could not be read: nothing settles
    /// now and nothing is recorded; her next registration replay decides it.
    PolicyUnavailable,
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

/// Inspectable staging metadata, not authorization to train or an inferred link
/// to an artifact. In particular, the newest turn may only discuss the review.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/work/StagedCreditEvidence.ts"
)]
pub struct StagedCreditEvidence {
    #[ts(type = "string")]
    pub revision_id: Uuid,
    #[ts(type = "string | null")]
    pub claim_id: Option<Uuid>,
    #[ts(type = "string | null")]
    pub owner: Option<Uuid>,
    // u64 -> number, the house convention two fields away in submission.rs
    // (`submitted_at_ms`, `reviewed_at_ms`). serde sends a JSON NUMBER; without this
    // ts-rs declares `bigint`, and a client honouring that type throws on
    // JSON.stringify. Caught as a BLOCKER in review and carried across a rebase.
    #[ts(type = "number")]
    pub staged_at_ms: u64,
    pub generation_count: usize,
    /// Join keys into the existing capture owner, in dispatch order. Snapshot
    /// write time is not their dispatch time; an absent capture stays unknown.
    pub generation_request_ids: Vec<String>,
    pub matches_submission_claim: bool,
    pub predates_submission: bool,
}

/// The existing credit owner projects its own rows. Consumers never receive the
/// prompt/completion, and an incomplete store response is an error, not "no work".
pub async fn staged_evidence<T: Transport>(
    conn: &Connection<T>,
    persona_name: &str,
    submitted: &airc_work::WorkSubmission,
) -> Result<Vec<StagedCreditEvidence>, ClientError> {
    ensure_storage(conn, persona_name).await?;
    let value = conn
        .commands()
        .execute_value(
            "data/list",
            json!({
                "collection": StagedCredit::COLLECTION,
                "dbPath": format!("@persona:{persona_name}"),
                "filter": {"cardId": submitted.card_id.as_uuid().to_string()},
            }),
        )
        .await?;
    let rows = staged_credit_from_list(value)?;
    let mut evidence = Vec::with_capacity(rows.len());
    for row in rows {
        if row.card_id != submitted.card_id.as_uuid() {
            return Err(ClientError::Transport(
                "staged credit query returned another card".into(),
            ));
        }
        evidence.push(StagedCreditEvidence {
            revision_id: row.id,
            claim_id: row.claim_id,
            owner: row.owner,
            staged_at_ms: row.staged_at_ms,
            generation_count: row.receipts.len(),
            generation_request_ids: row
                .receipts
                .into_iter()
                .map(|receipt| receipt.submitted_request_id)
                .collect(),
            matches_submission_claim: row.claim_id == Some(submitted.claim_id.as_uuid())
                && row.owner == Some(submitted.publisher.as_uuid()),
            predates_submission: row.staged_at_ms <= submitted.submitted_at_ms,
        });
    }
    evidence.sort_by_key(|row| (row.staged_at_ms, row.revision_id));
    Ok(evidence)
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
    #[error(
        "generation {0} must identify exactly one staged revision for this persona, card and claim"
    )]
    AmbiguousGeneration(String),
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
        StagedCreditGeneration::COLLECTION,
        CreditTransferIntent::COLLECTION,
        CreditTransferAcceptance::COLLECTION,
        graded::BenchmarkCreditAttribution::COLLECTION,
        WorkCreditBinding::COLLECTION,
        PendingCreditBinding::COLLECTION,
        CreditGenerationReservation::COLLECTION,
        CreditReviewDecision::COLLECTION,
        CreditReviewAcceptance::COLLECTION,
        CreditSubmitRefusal::COLLECTION,
        boundaries::ProcessedReviewBoundary::COLLECTION,
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
    self_review: &SelfReview,
) -> Result<ReviewedCredit, CreditBindingError> {
    let dream = crate::cognition::dream_consolidation::global();
    consume_review_with_boundary(
        conn,
        persona_name,
        persona_id,
        room_id,
        board,
        review_id,
        self_review,
        dream.as_deref(),
    )
    .await
}

pub async fn consume_review_with_boundary<T: Transport>(
    conn: &Connection<T>,
    persona_name: &str,
    persona_id: Uuid,
    room_id: Uuid,
    board: &airc_work::WorkBoardProjection,
    review_id: airc_work::WorkReviewId,
    self_review: &SelfReview,
    dream: Option<&crate::cognition::dream_consolidation::DreamConsolidationRegion>,
) -> Result<ReviewedCredit, CreditBindingError> {
    let credit =
        consume_review_credit(conn, persona_name, persona_id, room_id, board, review_id, self_review).await?;
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
            if !boundaries::is_processed(conn, persona_name, persona_id, submission_id).await? {
                dream.request_reviewed_boundary(persona_id, submission_id);
            }
        }
    }
    Ok(credit)
}
