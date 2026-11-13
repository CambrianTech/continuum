//! continuum-mind: cut out of continuum-core (MICROKERNEL-DECOMPOSITION.md): persona,
//! context and vdd, over cognition (its own crate, re-exported here under its old path). Depends on the live crate (which carries the engine and the
//! kernel), the hands, and the crates cut out of the mind: perception (perception and rag),
//! genome, memory and experience (experience and recipe), re-exported here under their old
//! paths; core re-exports these modules under their old names.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
#![deny(clippy::await_holding_lock, clippy::significant_drop_in_scrutinee)] // clean on 2026-11-08 (#5858 CI); CI's clippy step enforces it

pub use continuum_hands::code;
pub use continuum_live::*;
pub use continuum_genome::genome;
pub use continuum_memory::memory;
pub use continuum_experience::{experience, recipe};
pub use continuum_perception::perception;

pub use continuum_academy::cognition;
pub mod activity;
pub mod chat;
pub mod commands;
pub mod context;
pub mod modules;
mod native_outcomes;
pub mod persona;
pub mod room;
pub mod vdd;
pub mod work;
