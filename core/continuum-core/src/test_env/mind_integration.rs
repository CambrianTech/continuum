#![cfg(test)]
//! Tests that exercise a mind module (cognition, persona, genome, …) TOGETHER with a core
//! surface module: the data, code, grid, room, airc, dataset, health and benchmark modules,
//! and the source walk of every crate. They sat inside the mind's own files; the mind crate
//! has nothing above it to name, so they live in the crate that has both. Each asserts
//! exactly what it asserted before the move.

mod airc_conversation;
mod bound_card_activity;
mod eval;
mod finish_awaits_merge;
mod recipe;
mod registry;
mod hands_isolation;
mod obligation;
mod remote_lane;
mod source_guards;
mod teach;
mod tool_executor;
mod training_credit;
