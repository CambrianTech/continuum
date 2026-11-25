//! One parent for every decision (THE-LANE-ON-SAGE.md, "When to switch" and "The types").
//!
//! A decision is a use case: a typed situation, the actions available in it, and the measured outcome
//! of the one taken. The use case owns its envelope (what no policy may override, deferring included)
//! and its constraints (outcomes no policy may trade). A policy chooses inside the envelope by index;
//! the resource's owner executes the choice and reports what happened, which closes the record and is
//! what the policy learns from at that moment. Records name the revisions a decision read (SAGE's
//! [`crate::dataflow::InputRevision`]), never copies of owner payloads, so another policy can be
//! replayed on exactly what was known, in the order it became known.
//!
//! It decides at every lane step, so a decision allocates nothing once its buffers reach their high-water mark: actions are `Copy`
//! indices and masks, the envelope fills a reused buffer, records live in a preallocated ring.
//!
//! The design test: a new use case is added without editing any existing policy, and a new policy is
//! tried against recorded history without changing the producer or the executor (`tests.rs`).
//!
//! The lane (`cases::lane`) is the first real use case; residency, placement and volume are scaffolds
//! whose types exist from the start, so their owners add policies, not shapes.

mod algorithms;
mod capacity;
pub mod cases;
mod measured;
mod policy;
mod record;
mod replay;
mod use_case;

pub use capacity::{Capacity, Lease, Price};
pub use measured::{Ewma, Fraction, Measured, Window};
pub use algorithms::{Algorithms, Factory, Unknown};
pub use policy::{Baseline, Choice, Param, Policy, PolicyId, Refused};
pub use record::{DecisionId, Decided, Decider, Record, Verdict};
pub use replay::{replay, Replayed};
pub use use_case::{Deferred, Gate, UseCase, Violation};

#[cfg(test)]
mod tests;
