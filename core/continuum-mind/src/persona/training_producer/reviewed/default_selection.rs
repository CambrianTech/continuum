//! Which staged revision a resident's submission binds when she names none (card fe8554d2).
//!
//! A binding is what lets a review reach her credit: a Failed review marks the bound
//! revision rejected, so a later pass trains only the correction. Binding used to happen
//! only when `work/submit` carried `staged_revision_id` or `generation_request_id`, which a
//! resident has no reason to know. Kimi's card 13d7e47a took three Failed reviews with no
//! binding, so none reached her credit (5090, 2026-10-24: 4 rows, every `rejected_at_ms`
//! empty). Her core knows what she staged on the card, so it selects for her.

use uuid::Uuid;

use super::StagedCreditEvidence;

/// Why a submission with no explicit selection publishes unbound. Probed by the caller, never
/// a refusal: the submission still publishes, and its reviews stay unresolved, not passed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefaultUnbound {
    /// No revision on this card belongs to the submission's claim and owner, carries a
    /// generation, and was staged before the submission.
    NoEligibleRevision,
    /// Two eligible revisions share the newest stage time; picking one would be a guess.
    AmbiguousNewest,
}

impl DefaultUnbound {
    pub fn as_str(self) -> &'static str {
        match self {
            DefaultUnbound::NoEligibleRevision => "no_eligible_revision",
            DefaultUnbound::AmbiguousNewest => "ambiguous_newest",
        }
    }
}

/// The revision a submission with no explicit selection binds: the unique newest revision on
/// this card of the submission's own claim and owner, with at least one generation, staged
/// before it. `staged_evidence` already scopes the rows to the card; `bind_submission` then
/// validates the pick exactly as it validates an explicit selection.
pub fn default_selection(evidence: &[StagedCreditEvidence]) -> Result<Uuid, DefaultUnbound> {
    let eligible: Vec<&StagedCreditEvidence> = evidence
        .iter()
        .filter(|row| row.matches_submission_claim && row.predates_submission && row.generation_count > 0)
        .collect();
    let newest_at = eligible
        .iter()
        .map(|row| row.staged_at_ms)
        .max()
        .ok_or(DefaultUnbound::NoEligibleRevision)?;
    match eligible.iter().filter(|row| row.staged_at_ms == newest_at).collect::<Vec<_>>()[..] {
        [only] => Ok(only.revision_id),
        _ => Err(DefaultUnbound::AmbiguousNewest),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(revision: u128, staged_at_ms: u64, matches_claim: bool, predates: bool) -> StagedCreditEvidence {
        StagedCreditEvidence {
            revision_id: Uuid::from_u128(revision),
            claim_id: None,
            owner: None,
            staged_at_ms,
            generation_count: 1,
            generation_request_ids: vec![format!("request-{revision}")],
            matches_submission_claim: matches_claim,
            predates_submission: predates,
        }
    }

    // what this catches (card fe8554d2): a submission with no explicit selection bound nothing,
    // so every review of it missed her credit. The default is her unique newest revision of THIS
    // claim with a generation, staged before the submission. Another claim's work, work staged
    // after it, an empty turn, or a tie for newest is never bound: the submission goes unbound
    // with the reason, rather than binding a guess.
    #[test]
    fn the_default_binds_only_her_unique_newest_revision_of_this_claim_staged_before_the_submission() {
        let mut empty_turn = row(5, 350, true, true);
        empty_turn.generation_count = 0;
        let evidence = [
            row(1, 100, true, true),
            row(2, 300, true, true),
            empty_turn,               // no generation: nothing to credit
            row(3, 400, false, true), // another claim's work
            row(4, 500, true, false), // staged after the submission
        ];
        assert_eq!(default_selection(&evidence), Ok(Uuid::from_u128(2)));
        assert_eq!(
            default_selection(&[row(3, 400, false, true), row(4, 500, true, false)]),
            Err(DefaultUnbound::NoEligibleRevision)
        );
        assert_eq!(default_selection(&[]), Err(DefaultUnbound::NoEligibleRevision));
        assert_eq!(
            default_selection(&[row(1, 100, true, true), row(6, 300, true, true), row(7, 300, true, true)]),
            Err(DefaultUnbound::AmbiguousNewest)
        );
    }
}
