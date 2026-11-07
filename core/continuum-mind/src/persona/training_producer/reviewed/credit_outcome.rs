//! Her credit's state as the health line counts it: one exhaustive map, so a new state fails to
//! compile until it is counted (`health_ledger::learning`).

use super::*;
use crate::cognition::health_ledger::CreditOutcome;

impl ReviewedCreditState {
    /// The outcome this state counts as.
    pub fn outcome(self) -> CreditOutcome {
        match self {
            ReviewedCreditState::Accepted => CreditOutcome::Accepted,
            ReviewedCreditState::FailedReview => CreditOutcome::FailedReview,
            ReviewedCreditState::UnknownReview => CreditOutcome::UnknownReview,
            ReviewedCreditState::AwaitingAcceptance => CreditOutcome::AwaitingAcceptance,
            ReviewedCreditState::IndependentReviewRequired => CreditOutcome::IndependentReviewRequired,
            ReviewedCreditState::Unbound | ReviewedCreditState::ProvenanceHeld => CreditOutcome::Held,
            ReviewedCreditState::IneligibleEvidence => CreditOutcome::Ineligible,
            ReviewedCreditState::AwaitingReview
            | ReviewedCreditState::PolicyUnavailable
            | ReviewedCreditState::AnotherReviewSelected
            | ReviewedCreditState::PublisherNotResident => CreditOutcome::Undecided,
        }
    }
}
