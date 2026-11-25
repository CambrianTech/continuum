//! One parent for every decision (THE-LANE-ON-SAGE.md, "When to switch" and "The types").
//!
//! A decision is a use case: a typed situation, the actions available in it, and the measured outcome
//! of the one taken. The use case owns its envelope (what no policy may override) and its constraints
//! (outcomes no policy may trade). A policy chooses inside the envelope, from views of history; the
//! resource's owner executes the choice and reports what happened, which closes the record. Records
//! name the revisions a decision read (SAGE's [`crate::dataflow::InputRevision`]), never copies of the
//! values, so another policy can be replayed on exactly what was known at each choice.
//!
//! The design test: a new use case is added without editing any existing policy, and a new policy is
//! tried against recorded history without changing the producer or the executor (`tests.rs`).
//!
//! The lane (`cases::lane`) is the first real use case; residency, placement and volume are scaffolds
//! whose types exist from the start, so their owners add policies, not shapes.

mod capacity;
pub mod cases;
mod measured;
mod policy;
mod record;
mod use_case;

pub use capacity::{Capacity, Lease, Price};
pub use measured::{Ewma, Fraction, Measured, Window};
pub use policy::{Baseline, Choice, Policy, PolicyId};
pub use record::{replay, DecisionId, Decider, Record, Replayed};
pub use use_case::{Envelope, NonEmpty, UseCase, Violation};

#[cfg(test)]
mod tests;
