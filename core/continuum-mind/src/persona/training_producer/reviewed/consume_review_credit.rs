use super::*;

pub(super) async fn consume_review_credit<T: Transport>(
    conn: &Connection<T>,
    persona_name: &str,
    persona_id: Uuid,
    room_id: Uuid,
    board: &airc_work::WorkBoardProjection,
    review_id: airc_work::WorkReviewId,
    self_review: &SelfReview,
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
    let binding = match read_one::<_, WorkCreditBinding>(conn, persona_name, &id.to_string()).await? {
        Some(binding) => binding,
        // No binding yet: the generation publication recorded binds now, against the
        // submission as the board holds it (card fe8554d2).
        None => match resolve_pending(conn, persona_name, persona_id, room_id, submitted).await? {
            Some(Ok(binding)) => binding,
            Some(Err(held)) => {
                continuum_base::probe!(
                    class = "training.credit.provenance_held",
                    persona = %persona_name,
                    submission = %id,
                    held = ?held,
                    "a review arrived for a submission whose issuing generation does not bind yet: credit held, not passed"
                );
                return Ok(ReviewedCredit::pending(ReviewedCreditState::ProvenanceHeld));
            }
            None => {
                // Her own submission with no binding and no record of what issued it: a review
                // that cannot reach her credit, said once per consumption (Kimi's 13d7e47a shape).
                continuum_base::probe!(
                    class = "training.credit.unbound",
                    persona = %persona_name,
                    submission = %id,
                    review = %review_id.as_uuid(),
                    "a review of her submission arrived with no credit binding: it cannot reach her credit"
                );
                return Ok(ReviewedCredit::pending(ReviewedCreditState::Unbound));
            }
        },
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
        // Her own review earns credit only where the room admits it (`SelfReview`); an
        // unreadable policy is `PolicyUnavailable`, nothing persisted, replayed at her next
        // registration.
        airc_work::WorkReviewOutcome::Passed if review.reviewer == submitted.publisher => match self_review {
            SelfReview::Admitted => ReviewedCreditState::AwaitingAcceptance,
            SelfReview::NotAdmitted => ReviewedCreditState::IndependentReviewRequired,
            SelfReview::Unknown(_) => ReviewedCreditState::PolicyUnavailable,
        },
        airc_work::WorkReviewOutcome::Passed => ReviewedCreditState::AwaitingAcceptance,
    };
    if result.state == ReviewedCreditState::FailedReview {
        correction::reject_on_failed_review(persona_name, submitted.card_id.as_uuid()).await;
    }
    if result.state != ReviewedCreditState::AwaitingAcceptance {
        return Ok(result);
    }
    settle_passed(conn, persona_name, persona_id, id, &binding, review, review_id, result).await
}

/// The settlement of a credit whose review lets it pass, from its immutable binding: the
/// bound snapshot's provenance, its lived calls, the reservation, the one decision, then the
/// training submit and its acceptance (or the refusal kept, card 99b2773a). Shared by a
/// review's consumption and the retry of a decided credit ([`retry_decided`]), so the two
/// cannot drift.
#[allow(clippy::too_many_arguments)] // each is one fact the settlement checks; a struct would only rename them
pub(super) async fn settle_passed<T: Transport>(
    conn: &Connection<T>,
    persona_name: &str,
    persona_id: Uuid,
    id: Uuid,
    binding: &WorkCreditBinding,
    review: &airc_work::WorkSubmissionReview,
    review_id: airc_work::WorkReviewId,
    mut result: ReviewedCredit,
) -> Result<ReviewedCredit, CreditBindingError> {
    let selection = &binding.selection;
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
    // Positive credit re-checks provenance against the immutable snapshot every time, so a
    // submission recorded after this binding was made still holds a shared row (card fe8554d2).
    if let Some(other) = issued_elsewhere(conn, persona_name, row, id).await? {
        continuum_base::probe!(
            class = "training.credit.provenance_held",
            persona = %persona_name,
            submission = %id,
            held = ?Held::SharedRevision(other),
            "a bound snapshot carries another submission's issuing generation: credit held, not passed"
        );
        result.state = ReviewedCreditState::ProvenanceHeld;
        return Ok(result);
    }
    // The snapshot stays the bound evidence; only the params read back calls a capture
    // completed after staging.
    let resolved = with_lived(persona_id, persona_name, row).await;
    // A pass after a reviewed failure on this card is a correction: its own bucket, one trains.
    let verdict = correction::passing_verdict(persona_name, row.card_id, row.id).await;
    let mut params = match staged_submission_params(persona_id, persona_name, &*resolved, verdict) {
        Ok(params) => params,
        Err(reason) => {
            // This path ALREADY named its state (`IneligibleEvidence`), so it was
            // never silent the way the other caller was — but it could not say WHY.
            // Now it can, from the same named reason.
            continuum_base::probe!(
                class = "training.credit.reviewed_ineligible",
                persona = %persona_name,
                card = %row.card_id,
                revision = %row.id,
                reason,
                "a reviewed credit's staged revision cannot be submitted — the state was already named, the reason now is too"
            );
            result.state = ReviewedCreditState::IneligibleEvidence;
            return Ok(result);
        }
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
        // Kept, so her status names why the credit waits; the credit-acceptance region settles it again
        // (card 99b2773a); a refusal that cannot be kept is said, and the decision stands.
        continuum_base::probe!(
            class = "training.credit.submit_refused",
            persona = %persona_name,
            submission = %id,
            revision = %row.id,
            error_kind = ?receipt.error_kind,
            error = ?receipt.error,
            "the training trigger answered a reviewed credit without accepting it: kept, and settled again by the credit-acceptance region"
        );
        if let Err(error) = record_refusal(conn, persona_name, id, &receipt).await {
            continuum_base::probe!(
                class = "training.credit.submit_refusal_unkept",
                persona = %persona_name,
                submission = %id,
                error = %error,
                "a refused reviewed credit could not be kept: the credit-acceptance region still finds it by its decision"
            );
        }
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
        write_batch(conn, persona_name, vec![create(id.to_string(), &accepted)?, clear_refusal(id)]).await
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
    // A decided credit the trigger refused names the refusal as its destination (card 99b2773a).
    let refused = match (&accepted, &decision) {
        (None, Some(_)) => read_one::<_, CreditSubmitRefusal>(conn, persona_name, &id).await?.map(|r| r.receipt),
        _ => None,
    };
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
        destination: accepted.map(|a| a.receipt).or(refused),
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
    let mut result: crate::orm::list_result::DataListResult = serde_json::from_value(value)?; // Decode the data/list command envelope once at the storage boundary.
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

pub(in crate::persona::training_producer) fn create<E: OrmEntity + Serialize>(id: String, entity: &E) -> Result<BatchOperation, ClientError> {
    Ok(BatchOperation {
        operation_type: BatchOperationType::Create,
        collection: E::COLLECTION.into(),
        id: Some(id),
        data: Some(serde_json::to_value(entity)?), // Encode one entity into the ORM batch's required Value payload.
    })
}

pub(super) fn create_intent(row: &StagedCredit, work_submission_id: Option<Uuid>) -> BatchOperation {
    BatchOperation {
        operation_type: BatchOperationType::Create,
        collection: CreditTransferIntent::COLLECTION.into(),
        id: Some(row.id.to_string()),
        // Encode the borrowed snapshot once at the storage boundary; retaining
        // a second Rust clone of its complete prompt/act chain is unnecessary.
        data: Some(json!({"id": row.id, "workSubmissionId": work_submission_id, "snapshot": row})),
    }
}

pub(super) fn same_intent(
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

pub(in crate::persona::training_producer) async fn write_batch<T: Transport>(
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

pub async fn transfer_accepted<T: Transport>(
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
pub(in crate::persona::training_producer) async fn accept_transfer<T: Transport>(
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

pub(super) fn reservations(
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

pub(super) fn same_selection(
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
    bind_snapshot(conn, persona_name, persona_id, selection, snapshot).await
}

/// Resolve stable causal evidence at publication, not a staging UUID that can
/// disappear during the resident's intervening acts. Retries use retained intent.
pub async fn bind_submission_generation<T: Transport>(
    conn: &Connection<T>,
    persona_name: &str,
    persona_id: Uuid,
    room_id: Uuid,
    submitted: &airc_work::WorkSubmission,
    generation: &str,
) -> Result<WorkCreditBinding, CreditBindingError> {
    ensure_storage(conn, persona_name).await?;
    let selection = |revision| SubmissionSelection {
        submission_id: submitted.submission_id.as_uuid(),
        room_id,
        card_id: submitted.card_id.as_uuid(),
        claim_id: submitted.claim_id.as_uuid(),
        staged_revision_id: revision,
        instance: submitted.instance.clone(),
        base_sha: submitted.base_sha.clone(),
        artifact: submitted.artifact.clone(),
    };
    if submitted.publisher.as_uuid() != persona_id || generation.is_empty() {
        return Err(CreditBindingError::WrongSelection);
    }
    if let Some(prior) =
        read_one::<_, WorkCreditBinding>(conn, persona_name, &submitted.submission_id.to_string())
            .await?
    {
        let expected = selection(prior.selection.staged_revision_id);
        let prior = same_selection(prior, persona_id, &expected)?;
        let intent = read_one::<_, CreditTransferIntent>(
            conn,
            persona_name,
            &prior.transfer_intent_id.to_string(),
        )
        .await?
        .ok_or(CreditBindingError::MissingRevision(
            prior.transfer_intent_id,
        ))?;
        if !intent
            .snapshot
            .receipts
            .iter()
            .any(|r| r.submitted_request_id == generation)
        {
            return Err(CreditBindingError::WrongSelection);
        }
        return Ok(prior);
    }
    let snapshot = generation_row(conn, persona_name, persona_id, submitted, generation).await?;
    // Carry the captured snapshot through the transaction; rereading its mutable
    // staging key here would reintroduce the inspection/publication race.
    bind_snapshot(
        conn,
        persona_name,
        persona_id,
        selection(snapshot.id),
        snapshot,
    )
    .await
}

/// The ONE staged row on this card, claim and owner whose receipts carry `generation`, read
/// through the generation index. Absent, repeated or replaced mid-read is ambiguous, never
/// evidence for another row.
pub(super) async fn generation_row<T: Transport>(
    conn: &Connection<T>,
    persona_name: &str,
    persona_id: Uuid,
    submitted: &airc_work::WorkSubmission,
    generation: &str,
) -> Result<StagedCredit, CreditBindingError> {
    let value = conn
        .commands()
        .execute_value(
            "data/list",
            json!({
                "collection": StagedCreditGeneration::COLLECTION,
                "dbPath": format!("@persona:{persona_name}"),
                "filter": {"submittedRequestId": generation},
            }),
        )
        .await?;
    // The child index exists precisely for this join. Loading every lived turn
    // on the card can materialize tens of MiB before finding a single request.
    // Never bind from a partial result, or trust the index instead of the parent.
    let result: crate::orm::list_result::DataListResult =
        serde_json::from_value(value).map_err(ClientError::from)?; // Decode the data/list storage command response envelope.
    if result.items.len() != result.total as usize {
        return Err(ClientError::Transport("incomplete generation credit lookup".into()).into());
    }
    let mut selected = None;
    for value in result.items {
        let record: crate::orm::types::DataRecord =
            serde_json::from_value(value).map_err(ClientError::from)?; // Decode the storage protocol's DataRecord envelope.
        let link: StagedCreditGeneration =
            serde_json::from_value(record.data).map_err(ClientError::from)?; // Decode the persisted generation-index entity from its storage record.
        if link.submitted_request_id != generation {
            return Err(CreditBindingError::WrongSelection);
        }
        let Some(row) =
            read_one::<_, StagedCredit>(conn, persona_name, &link.staged_credit_id.to_string())
                .await?
        else {
            // Staging can replace the parent between these reads. Retry against
            // its new index; an absent parent is not evidence for another row.
            return Err(CreditBindingError::AmbiguousGeneration(generation.into()));
        };
        if row.card_id == submitted.card_id.as_uuid()
            && row.claim_id == Some(submitted.claim_id.as_uuid())
            && row.owner == Some(persona_id)
            && row.role.is_some()
            && row
                .receipts
                .iter()
                .any(|r| r.submitted_request_id == generation)
        {
            if selected.is_some() {
                return Err(CreditBindingError::AmbiguousGeneration(generation.into()));
            }
            selected = Some(row);
        }
    }
    selected.ok_or_else(|| CreditBindingError::AmbiguousGeneration(generation.into()))
}

pub(super) async fn bind_snapshot<T: Transport>(
    conn: &Connection<T>,
    persona_name: &str,
    persona_id: Uuid,
    selection: SubmissionSelection,
    snapshot: StagedCredit,
) -> Result<WorkCreditBinding, CreditBindingError> {
    let id = selection.submission_id.to_string();
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

pub(super) async fn read_reservations<T: Transport>(
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
    let result: crate::orm::list_result::DataListResult = serde_json::from_value(value)?; // Decode the indexed data/list response at the storage boundary.
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
