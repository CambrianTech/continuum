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
pub use consume_review_credit::*;
mod pending_binding;
pub use pending_binding::*;
mod submit_refusal;
pub use submit_refusal::*;
mod retry_decided;
pub use retry_decided::*;
