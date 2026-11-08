//! A mind's context as an epoch materialization (docs/architecture/MIND-CONTEXT-AND-ENGINE-CONFIG.md).
//! Its own crate so the monolith never grows by it and its tests run in seconds.
//!
//! - [`cause`]: what each element follows from: its occurrence id, its cause, its content.
//! - [`cost`]: the caller's allocation estimate of a block, in the unit its grants are made in.
//! - [`epoch`]: a stable head, then keyed blocks that only append.
//! - [`source`]: what a context source offers: its current items, each under a stable key.
//! - [`level`]: how much of an item the view rendered (whole, summary, pointer, folded).
//! - [`levels`]: each item's summary level, generated from its shape (mipmaps for context).
//! - [`nav`]: a folded group of items: its label, its count and the typed command that opens it.
//! - [`motion`]: how each source moves between epoch starts, which orders the next start.
//! - [`price`]: what holding context costs an act in seconds, measured per model on its node.
//! - [`rereads`]: how often she asks again for what she observed, by its age.
//! - [`depth`]: how deep a turnover collapses her history, from those re-reads and her growth.
//! - [`view`]: one mind's epoch fed by its sources; turnover is the only rewrite.
//! - [`flex`]: the one allocator: claims by floor, min, max and priority.
//! - [`ladder`]: the one contract every budget consumer answers: its items' levels, its hard limit.
//! - [`allocation`]: an act's ONE allocation over everything her prompt holds, run once; its
//!   [`allocation::search`] chooses every leveled consumer's levels together.
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod allocation;
pub mod cause;
pub mod cost;
pub mod depth;
pub mod epoch;
pub mod flex;
pub mod ladder;
#[cfg(test)]
mod fixtures;
pub mod level;
pub mod levels;
pub mod motion;
pub mod nav;
pub mod price;
pub mod rereads;
pub mod source;
pub mod view;
