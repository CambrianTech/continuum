//! The stage contract (M0): every brain function is a stage on one substrate.
//!
//! A stage declares its inputs (typed topics), its triggers, its cost (execution and residency,
//! claimed from the one resource authority) and its yield points; the substrate gives it
//! delivery, scheduling, fusion, interruption and capture once, for all stages
//! (docs/architecture/STAGE-CONTRACT.md, THE-WHOLE-MIND §1b).
//!
//! This module owns the contract's types and the pure rules every stage shares (identity,
//! provenance, the publication compare-and-set). The task lifetime and delivery owner and the
//! resource authority implement their halves behind these types.

mod control;
mod cost;
mod identity;
mod publication;
mod runner;
mod stage;
mod topic;
mod triggers;

pub use control::{ControlRequest, StopReceipt, StopRefusal};
pub use cost::{Cost, ExecutionCost, ModelTier, ResidencyCost};
pub use identity::{MindId, RunId, StageId, StageIncarnation};
pub use publication::{Publication, Published};
pub use runner::{PublishedOutput, StageRunner};
pub use stage::{RunOutcome, Stage, StageContext};
pub use topic::{InputRevision, Provenance, TopicKind, TopicName};
pub use triggers::Triggers;

#[cfg(test)]
mod tests;
