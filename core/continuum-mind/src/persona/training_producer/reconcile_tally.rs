//! What a registration's review replay decided, one count per typed outcome. Before 2026-11-07
//! `reconcile_reviews` read every `Ok(_)` as success, so its probe's `deferred 0` hid a failed
//! review, an unbound credit and a self-review that needs another reviewer alike.

use super::reviewed::{CreditBindingError, ReviewedCredit, ReviewedCreditState};
use crate::cognition::health_ledger::{CreditOutcome, CreditOutcomeCounts};

/// The replay's outcomes: each review's decision by type, the reviews not hers to settle, and the
/// ones that errored.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReconcileTally {
    pub outcomes: CreditOutcomeCounts,
    /// The review's submission is not hers (`consume_observed_review` answered `None`).
    pub not_hers: u64,
    pub errors: u64,
}

/// Why a replayed review stays owed: the next registration replays it.
#[derive(Debug, PartialEq, Eq)]
pub enum Deferred {
    PolicyUnavailable,
    ProvenanceHeld,
    Error(String),
}

impl std::fmt::Display for Deferred {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Deferred::PolicyUnavailable => f.write_str("the room's review policy could not be read"),
            Deferred::ProvenanceHeld => f.write_str("its submission's provenance is held (not staged yet, or shared)"),
            Deferred::Error(error) => f.write_str(error),
        }
    }
}

impl ReconcileTally {
    /// Count one replayed review's result; `Some` when it stays owed to the next registration.
    pub fn note(&mut self, result: &Result<Option<ReviewedCredit>, CreditBindingError>) -> Option<Deferred> {
        match result {
            Ok(Some(credit)) => {
                self.outcomes.note(credit.state.outcome());
                match credit.state {
                    ReviewedCreditState::PolicyUnavailable => Some(Deferred::PolicyUnavailable),
                    ReviewedCreditState::ProvenanceHeld => Some(Deferred::ProvenanceHeld),
                    _ => None,
                }
            }
            Ok(None) => {
                self.not_hers += 1;
                None
            }
            Err(error) => {
                self.errors += 1;
                Some(Deferred::Error(error.to_string()))
            }
        }
    }

    pub fn of(&self, outcome: CreditOutcome) -> u64 {
        self.outcomes.of(outcome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the registration replay reading every `Ok(_)` as success (before
    // 2026-11-07), so a failed review, an unbound credit and a self-review that needs another
    // reviewer all read as settled. Each is counted as what it is; only the replayable ones defer.
    #[test]
    fn a_failed_review_is_counted_failed_never_success() {
        let mut tally = ReconcileTally::default();
        let failed = Ok(Some(ReviewedCredit::pending(ReviewedCreditState::FailedReview)));
        assert_eq!(tally.note(&failed), None, "a failed review is decided, not deferred");
        assert_eq!(tally.of(CreditOutcome::FailedReview), 1);
        assert_eq!(tally.of(CreditOutcome::Accepted), 0, "a failed review is not success");
        tally.note(&Ok(Some(ReviewedCredit::pending(ReviewedCreditState::Unbound))));
        tally.note(&Ok(Some(ReviewedCredit::pending(ReviewedCreditState::IndependentReviewRequired))));
        assert_eq!((tally.of(CreditOutcome::Held), tally.of(CreditOutcome::IndependentReviewRequired)), (1, 1));
        assert_eq!(
            tally.note(&Ok(Some(ReviewedCredit::pending(ReviewedCreditState::ProvenanceHeld)))),
            Some(Deferred::ProvenanceHeld)
        );
        assert!(matches!(tally.note(&Err(CreditBindingError::WrongSelection)), Some(Deferred::Error(_))));
        tally.note(&Ok(None));
        assert_eq!((tally.errors, tally.not_hers), (1, 1));
    }
}
