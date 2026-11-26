//! Owned process execution and generation-safe termination, below the engine.
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
#![deny(clippy::await_holding_lock, clippy::significant_drop_in_scrutinee)] // clean on 2026-11-08 (#5858 CI); CI's clippy step enforces it
mod owned;
#[cfg(test)]
use owned::read_stream;
pub use owned::{
    command_name, confirm_gone, descendants, end_tree, holding, kill9, kill_if_same, recheck,
    run_owned, run_owned_with_input, signal_if_same, still_present, Answer, CaptureEnd, Captured,
    Ended, OwnedOutput, RunCapture, StreamCapture, Unanswered, Why, KILL_POLL,
};
#[cfg(test)]
mod tests;
