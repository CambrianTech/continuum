//! A mind's vitals: the health and turn ledgers, turn ingress, grid allocation, the vitals pulse,
//! the activity gate, directed pending and mind events. Cut out of continuum-cognition as a leaf of
//! its crate tree (docs/architecture/COGNITION-CRATE-TREE.md, step 3); cognition re-exports each
//! module under its old path.

pub mod health_ledger;
pub mod turn_ledger;
pub mod turn_ingress;
pub mod grid_allocation;
pub mod vitals_pulse;
pub mod activity_gate;
pub mod directed_pending;
pub mod mind_events;
pub mod pass_intent;
