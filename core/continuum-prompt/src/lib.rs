//! Prompt assembly and its budgets: the assembled prompt and its multi-party framing, the context and
//! token budgets, the prefix-reuse comparison, the audience, and the checks on what comes back (framing
//! echo, not-speech, response validation). Cut out of continuum-cognition as a leaf of its crate tree
//! (docs/architecture/COGNITION-CRATE-TREE.md, step 4); cognition re-exports each module under its old path.
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
#![deny(clippy::await_holding_lock, clippy::significant_drop_in_scrutinee)] // clean on 2026-11-08 (#5858 CI); CI's clippy step enforces it

pub mod audience;
pub mod context_budget;
pub mod framing_echo;
pub mod not_speech;
pub mod prompt_assembly;
pub mod prompt_prefix;
pub mod response_validator;
pub mod token_budget;
