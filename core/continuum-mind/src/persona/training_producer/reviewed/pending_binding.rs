//! A resident's submission names the generation that issued it; its credit binds when a review
//! arrives (card fe8554d2).
//!
//! A binding is what lets a review reach her credit. It used to exist only when `work/submit`
//! carried `staged_revision_id` or `generation_request_id`, which a resident has no reason to
//! know: Kimi's card 13d7e47a took three Failed reviews with no binding (5090, 2026-10-24: 4
//! staged rows, every `rejected_at_ms` empty, 0 bindings). The issuing generation comes from the
//! executor's own context, never from her arguments. It is usually not staged yet when she
//! submits (staging runs at act boundaries), so publish records it and the review resolves it
//! through the same exact join an explicit `generation_request_id` uses.

use super::*;

/// One publication: the fields a board submission and its retries must all match. A type, so
/// sameness is `==` and a field added later cannot be left out of it (Fable on #5386).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Publication {
    pub submission_id: Uuid,
    pub room_id: Uuid,
    pub card_id: Uuid,
    pub claim_id: Uuid,
    pub instance: String,
    pub base_sha: airc_work::GitObjectId,
    pub artifact: airc_work::SubmissionArtifact,
}

impl Publication {
    /// The publication a submission in `room_id` is.
    pub fn of(room_id: Uuid, submitted: &airc_work::WorkSubmission) -> Self {
        Self {
            submission_id: submitted.submission_id.as_uuid(),
            room_id,
            card_id: submitted.card_id.as_uuid(),
            claim_id: submitted.claim_id.as_uuid(),
            instance: submitted.instance.clone(),
            base_sha: submitted.base_sha.clone(),
            artifact: submitted.artifact.clone(),
        }
    }
}

/// What publication recorded: the publication plus the generation that issued it. Immutable
/// once written; a retry is the same publication, and keeps the first generation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingSelection {
    #[serde(flatten)]
    pub publication: Publication,
    pub issuing_generation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, crate::orm::Entity)]
#[serde(rename_all = "camelCase")]
#[entity(collection = "pending_credit_binding")]
pub struct PendingCreditBinding {
    /// The submission id: one pending row per submission, so a retry reads the same row.
    #[entity(primary_key)]
    pub id: Uuid,
    pub persona_id: Uuid,
    #[entity(indexed)]
    pub card_id: Uuid,
    #[entity(json)]
    pub selection: PendingSelection,
}

/// Why a pending binding did not bind. Each holds her credit; none of them passes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Held {
    /// The submission on the board is not the one publication recorded (a crash between
    /// recording and publishing, then a different retry): no review of it reaches this row.
    PublicationMismatch,
    /// The issuing generation's staged row also carries another submission's issuing
    /// generation, or a generation another binding already reserved (named by its submission,
    /// or its revision for a legacy transfer). Staging is periodic, so a failed submission and
    /// its correction can share one row; crediting it would train the failed generations as the
    /// correction.
    SharedRevision(Uuid),
    /// The generation is not staged yet, or names no single row: retried at the next review
    /// event and at registration replay.
    Unresolved(String),
}

/// Record the issuing generation before publication. A retry with the same selection is a
/// no-op; a different selection for the same submission id is refused.
pub async fn record_pending<T: Transport>(
    conn: &Connection<T>,
    persona_name: &str,
    persona_id: Uuid,
    selection: PendingSelection,
) -> Result<(), CreditBindingError> {
    ensure_storage(conn, persona_name).await?;
    let id = selection.publication.submission_id;
    // A retry comes from a later act, so it carries a later generation: the publication fields
    // decide sameness, and the FIRST recorded generation is kept, because that act made it.
    let same = |prior: &PendingCreditBinding| {
        prior.persona_id == persona_id && prior.selection.publication == selection.publication
    };
    if let Some(prior) = read_one::<_, PendingCreditBinding>(conn, persona_name, &id.to_string()).await? {
        return if same(&prior) { Ok(()) } else { Err(CreditBindingError::ConflictingSubmission(id)) };
    }
    let row = PendingCreditBinding { id, persona_id, card_id: selection.publication.card_id, selection: selection.clone() };
    if let Err(error) = write_batch(conn, persona_name, vec![create(id.to_string(), &row)?]).await {
        // A lost acknowledgement or a racing retry: the row that exists decides.
        match read_one::<_, PendingCreditBinding>(conn, persona_name, &id.to_string()).await? {
            Some(prior) if same(&prior) => return Ok(()),
            Some(_) => return Err(CreditBindingError::ConflictingSubmission(id)),
            None => return Err(error.into()),
        }
    }
    Ok(())
}

/// Bind a published submission's credit from its pending row. `submitted` is the submission
/// as the board holds it, so only a publication that happened can resolve. `Ok(None)` when no
/// pending row exists.
pub async fn resolve_pending<T: Transport>(
    conn: &Connection<T>,
    persona_name: &str,
    persona_id: Uuid,
    room_id: Uuid,
    submitted: &airc_work::WorkSubmission,
) -> Result<Option<Result<WorkCreditBinding, Held>>, CreditBindingError> {
    ensure_storage(conn, persona_name).await?;
    let id = submitted.submission_id.as_uuid();
    let Some(pending) = read_one::<_, PendingCreditBinding>(conn, persona_name, &id.to_string()).await? else {
        return Ok(None);
    };
    if !matches_publication(&pending, persona_id, room_id, submitted) {
        return Ok(Some(Err(Held::PublicationMismatch)));
    }
    let generation = pending.selection.issuing_generation.as_str();
    let row = match generation_row(conn, persona_name, persona_id, submitted, generation).await {
        Ok(row) => row,
        Err(CreditBindingError::Storage(e)) => return Err(e.into()),
        Err(other) => return Ok(Some(Err(Held::Unresolved(other.to_string())))),
    };
    if let Some(other) = issued_elsewhere(conn, persona_name, &row, id).await? {
        return Ok(Some(Err(Held::SharedRevision(other))));
    }
    let selection = SubmissionSelection {
        submission_id: id,
        room_id,
        card_id: pending.selection.publication.card_id,
        claim_id: pending.selection.publication.claim_id,
        staged_revision_id: row.id,
        instance: pending.selection.publication.instance.clone(),
        base_sha: pending.selection.publication.base_sha.clone(),
        artifact: pending.selection.publication.artifact.clone(),
    };
    match bind_snapshot(conn, persona_name, persona_id, selection, row).await {
        Ok(binding) => Ok(Some(Ok(binding))),
        Err(CreditBindingError::Storage(e)) => Err(e.into()),
        Err(other) => Ok(Some(Err(Held::Unresolved(other.to_string())))),
    }
}

/// Another submission sharing this staged row's provenance: one whose issuing generation the
/// row carries (pending rows), or one already holding a reservation on any of its generations
/// (explicit and legacy bindings, which have no pending row). Checked when a pending row
/// resolves AND each time a binding is consumed for positive credit, against the binding's
/// immutable snapshot, so a submission recorded after the binding was made is still seen.
pub async fn issued_elsewhere<T: Transport>(
    conn: &Connection<T>,
    persona_name: &str,
    row: &StagedCredit,
    submission: Uuid,
) -> Result<Option<Uuid>, CreditBindingError> {
    // Every pending row counts, published or not: an airc publish whose answer was lost or
    // cancelled may still have landed (its error is not definitive), so a row cannot be ruled
    // out by its absence from a board. An unpublished record resolves by its own same-id
    // retry, which work/submit's error names and record_pending accepts, keeping its first
    // generation; it is never superseded by a new id.
    let others = pending_on_card(conn, persona_name, row.card_id).await?;
    let row_generations: Vec<&str> = row.receipts.iter().map(|r| r.submitted_request_id.as_str()).collect();
    let issued: Vec<(Uuid, &str)> =
        others.iter().map(|o| (o.id, o.selection.issuing_generation.as_str())).collect();
    if let Some(other) = shared_with(&row_generations, submission, &issued) {
        return Ok(Some(other));
    }
    let held = read_reservations(conn, persona_name, &reservations(row, Some(submission))?).await?;
    Ok(held
        .into_values()
        .find(|r| r.work_submission_id != Some(submission))
        // A legacy transfer reserves with no submission: name its revision instead.
        .map(|r| match r.work_submission_id {
            Some(other) => other,
            None => r.revision_id,
        }))
}

/// PURE: the pending row and the board's submission name the same publication.
fn matches_publication(
    pending: &PendingCreditBinding,
    persona_id: Uuid,
    room_id: Uuid,
    submitted: &airc_work::WorkSubmission,
) -> bool {
    pending.persona_id == persona_id
        && submitted.publisher.as_uuid() == persona_id
        && pending.id == submitted.submission_id.as_uuid()
        && pending.selection.publication == Publication::of(room_id, submitted)
}

/// PURE: another submission whose issuing generation sits in this row, if any.
fn shared_with(row_generations: &[&str], submission: Uuid, issued: &[(Uuid, &str)]) -> Option<Uuid> {
    issued
        .iter()
        .find(|(other, generation)| *other != submission && row_generations.contains(generation))
        .map(|(other, _)| *other)
}

/// Does a row staged with these generations carry a pending submission's issuing generation?
/// Then a review of that submission that waited (`Unresolved`: not staged yet) can bind now.
pub async fn issues_pending<T: Transport>(
    conn: &Connection<T>,
    persona_name: &str,
    card_id: Uuid,
    receipts: &[crate::ai::provenance::GenerationReceipt],
) -> Result<bool, ClientError> {
    let pending = pending_on_card(conn, persona_name, card_id).await?;
    Ok(pending
        .iter()
        .any(|p| receipts.iter().any(|r| r.submitted_request_id == p.selection.issuing_generation)))
}

/// Every pending row on the card. An incomplete list is an error, never "no other submission".
async fn pending_on_card<T: Transport>(
    conn: &Connection<T>,
    persona_name: &str,
    card_id: Uuid,
) -> Result<Vec<PendingCreditBinding>, ClientError> {
    let value = conn
        .commands()
        .execute_value(
            "data/list",
            json!({
                "collection": PendingCreditBinding::COLLECTION,
                "dbPath": format!("@persona:{persona_name}"),
                "filter": {"cardId": card_id.to_string()},
            }),
        )
        .await?;
    let result: crate::orm::list_result::DataListResult = serde_json::from_value(value)?; // Decode the data/list command envelope once at the storage boundary.
    if result.items.len() != result.total as usize {
        return Err(ClientError::Transport("incomplete pending credit lookup".into()));
    }
    result
        .items
        .into_iter()
        .map(|value| {
            let record: crate::orm::types::DataRecord = serde_json::from_value(value)?; // Decode the ORM envelope before its typed entity payload.
            Ok(serde_json::from_value(record.data)?) // The entity payload crosses the Value-native storage boundary here.
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (card fe8554d2, Codex on #5386): staging is periodic, so failed
    // submission S and its correction M can share one staged row. M's generation resolving to
    // that row must hold, whichever review arrives first, or a pass trains S's failed
    // generations as the correction. A row carrying only M's own generations binds.
    #[test]
    fn a_row_carrying_another_submissions_issuing_generation_is_shared() {
        let (s, m) = (Uuid::from_u128(1), Uuid::from_u128(2));
        let issued = [(s, "gen-s"), (m, "gen-m")];
        let shared = ["gen-s", "gen-x", "gen-m"];
        assert_eq!(shared_with(&shared, m, &issued), Some(s));
        assert_eq!(shared_with(&shared, s, &issued), Some(m));
        assert_eq!(shared_with(&["gen-x", "gen-m"], m, &issued), None);
        assert_eq!(shared_with(&["gen-m"], m, &[(m, "gen-m")]), None, "her own generation is not a share");
    }
}
