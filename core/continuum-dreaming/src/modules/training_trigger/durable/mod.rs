//! Durable acceptance belongs to the trigger, not to each curriculum producer.
//!
//! Each submission is inserted once. A dispatch intent names its submissions
//! BEFORE any row is assigned or any provider is called. Interrupted assignments
//! can therefore be completed from the intent; no multi-row transaction is
//! assumed. An interrupted provider call is never repeated on absence of proof.

mod policy;

use super::{BucketKey, PendingBatch, TrainingTriggerState};
use std::sync::{atomic::Ordering, Arc};

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::genome::training_outcome::{AcceptanceReceipt, Took};
#[cfg(not(test))]
use crate::genome::fine_tuning::TrainingJobBoard;
use crate::genome::fine_tuning::{job_board::DispatchLookup, JobHandle};
use crate::orm::{
    adapter::StorageAdapter,
    entity::{BaseEntity, OrmEntity},
    query::{QueryBuilder, QueryOperator},
    Entity, OrmStore,
};

mod acceptance_receipt;
pub use acceptance_receipt::*;
mod training_trigger_state;
pub(crate) use training_trigger_state::JobInputError;
mod training_trigger_state_2;
mod training_trigger_state_3;
pub(crate) use training_trigger_state_3::*;

#[cfg(test)]
mod tests;
