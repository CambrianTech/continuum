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
use uuid::Uuid;

use crate::genome::training_outcome::{AcceptanceReceipt, Took};
use crate::orm::{
    adapter::StorageAdapter,
    entity::{BaseEntity, OrmEntity},
    query::{QueryBuilder, QueryOperator},
    Entity, OrmStore,
};

mod acceptance_receipt;
pub(in crate::modules::training_trigger) use acceptance_receipt::*;
mod arrivals;
pub(crate) use arrivals::Arrival;
mod learn_checkpoint;
mod reclaim;
pub(crate) use learn_checkpoint::{Generation, LearnCheckpoint, Position};
mod training_trigger_state;
mod training_trigger_state_2;
mod training_trigger_state_3;
pub(crate) use training_trigger_state_3::*;

#[cfg(test)]
mod tests;
