//! The capture of a mind's turns: the one fixture-directory policy and the prompt-capture
//! recorder, its per-path owner and its playback store. Cut out of continuum-cognition as a leaf
//! of its crate tree (docs/architecture/COGNITION-CRATE-TREE.md, step 9); cognition re-exports
//! each module under its old path.
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
#![deny(clippy::await_holding_lock, clippy::significant_drop_in_scrutinee)] // clean on 2026-11-08 (#5858 CI); CI's clippy step enforces it

/// Where captures land: the one fixture-directory policy shared by every live capture
/// (persona turns, prompt and workspace captures) and their readers.
pub mod capture;
pub mod prompt_capture;
