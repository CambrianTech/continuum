//! The use cases. The lane is the first real one; residency, placement and volume are scaffolds whose
//! situations, actions, outcomes and envelopes exist now, with `Baseline` as their policy until their
//! owners build theirs. A new use case is a new file here; nothing else changes.

pub mod lane;
pub mod lane_cost;
pub mod lane_history;
pub mod placement;
pub mod residency;
pub mod volume;
