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

/// Her staged rows on `card_id`, read through the substrate; `None` when they cannot be read
/// (named by the probe; the caller then settles as before).
async fn card_rows(persona_name: &str, card_id: Uuid) -> Option<(Connection<InProcessTransport>, Vec<StagedCredit>)> {
    let executor = EXECUTOR.cloned()?;
    let substrate = substrate_connection(&executor);
    let listed = substrate
        .commands()
        .execute_value(
            "data/list",
            json!({
                "collection": StagedCredit::COLLECTION,
                "dbPath": format!("@persona:{persona_name}"),
                "filter": { "cardId": card_id.to_string() },
            }),
        )
        .await
        .and_then(staged_credit_from_list);
    match listed {
        Ok(rows) => Some((substrate, rows)),
        Err(error) => {
            continuum_base::probe!(
                class = "training.credit.correction_rows_unreadable",
                persona = %persona_name,
                card = %card_id,
                error = %error,
                "her staged rows on a reviewed card could not be read: a failure marks nothing, a pass settles as a plain pass"
            );
            None
        }
    }
}

/// A failed review of her submission rejects every row staged on its card so far, so a later
/// passing review settles as a correction (and the rejected rows never train as passing).
pub(super) async fn reject_on_failed_review(persona_name: &str, card_id: Uuid) {
    if let Some((substrate, rows)) = card_rows(persona_name, card_id).await {
        mark_rejected(&substrate, persona_name, card_id, &rows).await;
    }
}

/// The verdict a passing review of the submission bound to `revision` settles with:
/// `Corrected` when an earlier failure rejected a row on this card and `revision` is not one
/// of them (her revised work after the failure), else `Passed`. A pass of the very revision a
/// failure rejected is a changed verdict, not a correction.
pub(super) async fn passing_verdict(persona_name: &str, card_id: Uuid, revision: Uuid) -> CardVerdict {
    let Some((_, rows)) = card_rows(persona_name, card_id).await else {
        return CardVerdict::Passed;
    };
    corrected_or_passed(&rows, revision)
}

/// PURE: the rule [`passing_verdict`] applies to her rows.
fn corrected_or_passed(rows: &[StagedCredit], revision: Uuid) -> CardVerdict {
    let rejected = |row: &StagedCredit| row.rejected_at_ms.is_some();
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

    fn row(rejected: Option<u64>) -> StagedCredit {
        let mut row: StagedCredit = serde_json::from_value(json!({
            "id": Uuid::new_v4(), "cardId": Uuid::from_u128(0x5c0), "claimId": null, "owner": null,
            "role": null, "receipts": [], "served": null, "prompt": "p", "completion": "c", "stagedAtMs": 1,
        }))
        .expect("a minimal staged row"); // expect: the literal above is a complete StagedCredit
        row.rejected_at_ms = rejected;
        row
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
