//! Exact work-submission ownership inside the existing per-persona credit store.
//!
//! A binding copies ONE selected revision before publishing its artifact. Periodic
//! staging may replace its working copy, but cannot rewrite this evidence. The
//! same atomic batch reserves every request id against another cumulative revision.
//! Reservations survive refusal and transfer: deleting them would permit training
//! the same generations again through a later artifact or the legacy grader.
//! Accepted exact-review replay also notifies the existing dream owner. Its bounded
//! scheduling cache is backed by ORM acceptance recovery and processed-boundary
//! receipts; neither acceptance nor consolidation is a training/adoption claim.

use super::*;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

pub mod boundaries;

mod bridge_review;
pub use bridge_review::*;
mod consume_review_credit;
mod correction;
mod credit_outcome;
mod counted_outcome;
pub use counted_outcome::CountedCreditOutcome;
pub use consume_review_credit::*;
mod pending_binding;
pub use pending_binding::*;
mod submit_refusal;
pub use submit_refusal::*;
mod retry_decided;
pub use retry_decided::*;

/// Explicit authorities for review settlement. Citizen calls retain their identity;
/// only rejection bookkeeping uses the separately supplied substrate connection.
pub struct ReviewConnections<'a, T: Transport, S: Transport> {
    pub citizen: &'a Connection<T>,
    pub substrate: &'a Connection<S>,
}

/// The accepted review projection and the resident whose exact binding it may settle.
pub struct ReviewSelection<'a> {
    pub persona_name: &'a str,
    pub persona_id: Uuid,
    pub room_id: Uuid,
    pub board: &'a airc_work::WorkBoardProjection,
    pub review_id: airc_work::WorkReviewId,
    pub self_review: &'a SelfReview,
}
