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

/// What publication recorded: everything the published submission must match, plus the
/// generation that issued it. Immutable once written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingSelection {
    pub submission_id: Uuid,
    pub room_id: Uuid,
    pub card_id: Uuid,
    pub claim_id: Uuid,
    pub instance: String,
    pub base_sha: airc_work::GitObjectId,
    pub artifact: airc_work::SubmissionArtifact,
    pub issuing_generation: String,
}

impl PendingSelection {
    /// Everything but the issuing generation: the publication a retry repeats.
    pub fn same_publication(&self, other: &Self) -> bool {
        self.submission_id == other.submission_id
            && self.room_id == other.room_id
            && self.card_id == other.card_id
            && self.claim_id == other.claim_id
            && self.instance == other.instance
            && self.base_sha == other.base_sha
            && self.artifact == other.artifact
    }
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
    /// The submission that superseded this one before it was ever published: her next record
    /// on the same claim while this one was still off the board. Written in that record's own
    /// batch, before its publication, so ordering is her store's, not a board snapshot's. An
    /// abandoned id is never published later (its retry is refused) and never holds another.
    #[serde(default)]
    pub abandoned_by: Option<Uuid>,
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
    /// This submission was superseded before it was published (`abandoned_by`), yet a review
    /// of it reached her: the board and her record disagree, so nothing is credited.
    Superseded(Uuid),
}

/// Record the issuing generation before publication. The same id again (a retry, from a later
/// act) is the same publication when its fields match: a no-op that keeps the first generation.
/// A different publication under that id is refused, and so is retrying an id that was
/// superseded. `published` is what the board holds on this card: any earlier pending row of hers
/// on this claim that is not there was never published, and this record supersedes it.
pub async fn record_pending<T: Transport>(
    conn: &Connection<T>,
    persona_name: &str,
    persona_id: Uuid,
    selection: PendingSelection,
    published: &std::collections::HashSet<Uuid>,
) -> Result<(), CreditBindingError> {
    ensure_storage(conn, persona_name).await?;
    let id = selection.submission_id;
    let same = |prior: &PendingCreditBinding| {
        prior.persona_id == persona_id && prior.selection.same_publication(&selection)
    };
    let decide = |prior: PendingCreditBinding| match prior.abandoned_by {
        Some(by) => Err(CreditBindingError::Superseded(id, by)),
        None if same(&prior) => Ok(()),
        None => Err(CreditBindingError::ConflictingSubmission(id)),
    };
    if let Some(prior) = read_one::<_, PendingCreditBinding>(conn, persona_name, &id.to_string()).await? {
        return decide(prior);
    }
    let superseded: Vec<Uuid> = pending_on_card(conn, persona_name, selection.card_id)
        .await?
        .into_iter()
        .filter(|p| {
            p.id != id
                && p.persona_id == persona_id
                && p.selection.claim_id == selection.claim_id
                && p.abandoned_by.is_none()
                && !published.contains(&p.id)
        })
        .map(|p| p.id)
        .collect();
    let row = PendingCreditBinding {
        id,
        persona_id,
        card_id: selection.card_id,
        selection: selection.clone(),
        abandoned_by: None,
    };
    let mut operations = vec![create(id.to_string(), &row)?];
    operations.extend(superseded.iter().map(|old| BatchOperation {
        operation_type: BatchOperationType::Update,
        collection: PendingCreditBinding::COLLECTION.into(),
        id: Some(old.to_string()),
        data: Some(json!({ "abandonedBy": id })),
    }));
    if let Err(error) = write_batch(conn, persona_name, operations).await {
        // A lost acknowledgement or a racing retry: the row that exists decides.
        return match read_one::<_, PendingCreditBinding>(conn, persona_name, &id.to_string()).await? {
            Some(prior) => decide(prior),
            None => Err(error.into()),
        };
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
    if let Some(by) = pending.abandoned_by {
        return Ok(Some(Err(Held::Superseded(by))));
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
        card_id: pending.selection.card_id,
        claim_id: pending.selection.claim_id,
        staged_revision_id: row.id,
        instance: pending.selection.instance.clone(),
        base_sha: pending.selection.base_sha.clone(),
        artifact: pending.selection.artifact.clone(),
    };
    match bind_snapshot(conn, persona_name, persona_id, selection, row).await {
        Ok(binding) => Ok(Some(Ok(binding))),
        Err(CreditBindingError::Storage(e)) => Err(e.into()),
        Err(other) => Ok(Some(Err(Held::Unresolved(other.to_string())))),
    }
}

/// Another submission sharing this staged row's provenance: one whose issuing generation the
/// row carries (pending rows not superseded), or one already holding a reservation on any of its generations
/// (explicit and legacy bindings, which have no pending row). Checked when a pending row
/// resolves AND each time a binding is consumed for positive credit, against the binding's
/// immutable snapshot, so a submission recorded after the binding was made is still seen.
pub async fn issued_elsewhere<T: Transport>(
    conn: &Connection<T>,
    persona_name: &str,
    row: &StagedCredit,
    submission: Uuid,
) -> Result<Option<Uuid>, CreditBindingError> {
    // Every pending row counts, published or still in flight, except one durably superseded
    // before it was published. No board snapshot decides: a lagging view cannot hide one.
    let others: Vec<PendingCreditBinding> = pending_on_card(conn, persona_name, row.card_id)
        .await?
        .into_iter()
        .filter(|o| o.abandoned_by.is_none())
        .collect();
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
    let s = &pending.selection;
    pending.persona_id == persona_id
        && submitted.publisher.as_uuid() == persona_id
        && pending.id == submitted.submission_id.as_uuid()
        && s.submission_id == pending.id
        && s.room_id == room_id
        && s.card_id == submitted.card_id.as_uuid()
        && s.claim_id == submitted.claim_id.as_uuid()
        && s.instance == submitted.instance
        && s.base_sha == submitted.base_sha
        && s.artifact == submitted.artifact
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
