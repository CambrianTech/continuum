//! A mind's engrams and what steers them: the engram entity and its graph, recall metadata and
//! ranking, focus and its policy, the turn trace, redaction and the citizen home layout.
//! Cut out of continuum-cognition as a leaf of its crate tree (docs/architecture/
//! COGNITION-CRATE-TREE.md, step 7); cognition re-exports each module under its old path.
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
#![deny(clippy::await_holding_lock, clippy::significant_drop_in_scrutinee, clippy::let_underscore_must_use)] // clean on 2026-11-08 (#5858 CI); CI's clippy step enforces it

pub mod engram;
pub mod engram_graph;
pub mod recall_metadata;
pub mod recall_ranker;
pub mod focus;
pub mod focus_policy;
pub mod trace;
pub mod redaction;
pub mod home;
