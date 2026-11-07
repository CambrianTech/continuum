//! Each review's credit outcome reaches the health line once (Cormac on #5810).
//!
//! A review is consumed more than once: its live delivery, a redrive, and every registration's
//! replay (each core start) decide it again. Counting each consumption made a replayed failed
//! review read as a new one every restart. Here the count is keyed by (review, outcome) in her
//! own credit store, so it is the same once-only guarantee the acceptance row gives: the first
//! decision writes the key and counts; any later decision finds it and counts nothing. An
//! acceptance whose write committed but whose acknowledgement was lost counts here too, once.

use super::*;
use crate::cognition::health_ledger::{note_credit_outcome_of, CreditOutcome};

/// The receipt that one review's outcome was counted. Its key is derived from the review and the
/// outcome, so a second decision of the same outcome cannot write another.
#[derive(Debug, Clone, Serialize, Deserialize, crate::orm::Entity)]
#[serde(rename_all = "camelCase")]
#[entity(collection = "counted_credit_outcome")]
pub struct CountedCreditOutcome {
    /// `Uuid::new_v5(review_id, outcome name)`: one row per (review, outcome).
    #[entity(primary_key)]
    pub id: Uuid,
    pub review_id: Uuid,
    /// The credit state the outcome was decided at.
    #[entity(json)]
    pub state: ReviewedCreditState,
}

/// The key one (review, outcome) is counted under.
fn key(review_id: Uuid, outcome: CreditOutcome) -> Uuid {
    Uuid::new_v5(&review_id, outcome.as_str().as_bytes())
}

/// Count `state`'s outcome for `review_id` on the health line, once ever per (review, outcome).
/// A failure to record the key is said and not counted: a missed count, never a double one.
pub(super) async fn count_outcome_once<T: Transport>(
    conn: &Connection<T>,
    persona_name: &str,
    persona_id: Uuid,
    review_id: Uuid,
    state: ReviewedCreditState,
) {
    let outcome = state.outcome();
    let id = key(review_id, outcome);
    let row = CountedCreditOutcome { id, review_id, state };
    let written = match create(id.to_string(), &row) {
        Ok(op) => write_batch(conn, persona_name, vec![op]).await,
        Err(error) => Err(error),
    };
    let error = match written {
        Ok(()) => return note_credit_outcome_of(persona_id, outcome),
        Err(error) => error,
    };
    // Already counted (a replay, or a racing decision) reads back; anything else is a missed count.
    match read_one::<_, CountedCreditOutcome>(conn, persona_name, &id.to_string()).await {
        Ok(Some(_)) => {}
        Ok(None) | Err(_) => continuum_base::probe!(
            class = "training.credit.outcome_uncounted",
            persona = %persona_name,
            review = %review_id,
            outcome = outcome.as_str(),
            error = %error,
            "a review's credit outcome could not be recorded as counted: the health line misses it once"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: two outcomes of one review sharing a key (one would never count), or one
    // outcome keyed differently on a replay (it would count again).
    #[test]
    fn one_key_per_review_and_outcome_stable_across_replays() {
        let review = Uuid::from_u128(7);
        assert_eq!(key(review, CreditOutcome::FailedReview), key(review, CreditOutcome::FailedReview));
        assert_ne!(key(review, CreditOutcome::FailedReview), key(review, CreditOutcome::Accepted));
        assert_ne!(key(review, CreditOutcome::Accepted), key(Uuid::from_u128(8), CreditOutcome::Accepted));
    }
}
