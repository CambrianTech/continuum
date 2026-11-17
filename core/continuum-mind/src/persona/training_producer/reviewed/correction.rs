//! A reviewed correction trains as a correction, on the live review path too.
//!
//! The learning acceptance test is a reviewed correction, retained across restart, on unseen
//! tasks. Only the graded card path (`settle_card_credit_bound`) knew a correction: a failure
//! marked the card's staged rows rejected, and a later pass settled as `CardVerdict::Corrected`,
//! its own bucket where one example trains. A resident's live review did neither: a failed
//! review stopped at `FailedReview` and marked nothing, and the later pass appended one example
//! to a bucket that waits for sixteen. On the M5 (2026-10-31, 12 h) nothing ever trained.
//!
//! Now a failed review marks the card's rows rejected the same way, through the substrate (her
//! own identity resolves to Trusted, where data/update is refused: cards fd47a804, 44aee459),
//! and a passing review settles as a correction when the card holds a rejected row.

use super::*;

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RejectionRow {
    id: Uuid,
    rejected_at_ms: Option<u64>,
}

/// Read rejection state through the caller's storage authority. An unavailable read is
/// an unsettled review, never evidence of a plain pass.
async fn card_rows<T: Transport>(conn: &Connection<T>, persona_name: &str, card_id: Uuid) -> Result<Vec<RejectionRow>, ClientError> {
    crate::orm::client_read::matching(
        conn, &format!("@persona:{persona_name}"), StagedCredit::COLLECTION,
        serde_json::Map::from_iter([("cardId".into(), json!(card_id.to_string()))]),
        Some(vec!["rejectedAtMs".into()]),
    ).await
}

/// A failed review is settled only once its rejection marks are durably written.
/// Bookkeeping retains substrate authority; persona identities cannot update these rows.
pub(super) async fn reject_on_failed_review<T: Transport>(substrate: &Connection<T>, persona_name: &str, card_id: Uuid) -> Result<(), ClientError> {
    let rows = card_rows(substrate, persona_name, card_id).await?;
    mark_rejected(substrate, persona_name, card_id, &rows.iter().map(|r| (r.id, r.rejected_at_ms)).collect::<Vec<_>>()).await
}

/// A revised pass is a correction only when the durable rejection state proves it.
pub(super) async fn passing_verdict<T: Transport>(conn: &Connection<T>, persona_name: &str, card_id: Uuid, revision: Uuid) -> Result<CardVerdict, ClientError> {
    let rows = card_rows(conn, persona_name, card_id).await?;
    Ok(corrected_or_passed(&rows, revision))
}

/// PURE: the rule [`passing_verdict`] applies to her rows.
fn corrected_or_passed(rows: &[RejectionRow], revision: Uuid) -> CardVerdict {
    let rejected = |row: &RejectionRow| row.rejected_at_ms.is_some();
    let revision_rejected = rows.iter().any(|row| row.id == revision && rejected(row));
    if !revision_rejected && rows.iter().any(rejected) {
        CardVerdict::Corrected
    } else {
        CardVerdict::Passed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn unreadable_rejections_cannot_become_a_passing_verdict() {
        let mock = continuum_client::mock::MockTransport::new();
        mock.respond_to("data/list", |_| Err(ClientError::Transport("store unavailable".into())));
        let result = passing_verdict(&Connection::new(mock), "fixture", Uuid::new_v4(), Uuid::new_v4()).await;
        assert!(matches!(result, Err(ClientError::Transport(message)) if message == "store unavailable"));
    }

    #[tokio::test]
    async fn rejection_commit_is_one_transaction_and_failure_stays_unsettled() {
        let mock = continuum_client::mock::MockTransport::new();
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        mock.respond_to("data/batch", move |params| {
            let operations = params["operations"].as_array().expect("transaction operations");
            assert_eq!(operations.len(), 2);
            assert_eq!(operations[0]["id"], first.to_string());
            assert_eq!(operations[1]["id"], second.to_string());
            assert_eq!(operations[0]["data"]["rejectedAtMs"], operations[1]["data"]["rejectedAtMs"]);
            Err(ClientError::Transport("commit refused".into()))
        });
        let result = mark_rejected(&Connection::new(mock), "fixture", Uuid::new_v4(), &[
            (first, None), (Uuid::new_v4(), Some(42)), (second, None),
        ]).await;
        assert!(matches!(result, Err(ClientError::Transport(message)) if message == "commit refused"));
    }

    fn row(rejected: Option<u64>) -> RejectionRow {
        RejectionRow { id: Uuid::new_v4(), rejected_at_ms: rejected }
    }

    // what this catches: the live review path settling a reviewed correction as a plain pass
    // (one example into a bucket of sixteen, never trained), or training the very revision a
    // failure rejected as a correction when a reviewer re-passes it.
    #[test]
    fn a_pass_of_revised_work_after_a_failed_review_is_a_correction() {
        let failed = row(Some(50));
        let revised = row(None);
        let rows = [failed.clone(), revised.clone()];
        assert_eq!(corrected_or_passed(&rows, revised.id), CardVerdict::Corrected, "her revised work after the failure");
        assert_eq!(corrected_or_passed(&rows, failed.id), CardVerdict::Passed, "a re-pass of the rejected revision is a changed verdict");
        assert_eq!(corrected_or_passed(&[row(None)], revised.id), CardVerdict::Passed, "never failed: a plain pass");
    }
}
