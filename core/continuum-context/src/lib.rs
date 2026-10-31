//! A mind's context as an epoch materialization (docs/architecture/MIND-CONTEXT-AND-ENGINE-CONFIG.md).
//! Its own crate so the monolith never grows by it and its tests run in seconds.
//!
//! - [`cause`]: what each element follows from: its occurrence id, its cause, its content.
//! - [`epoch`]: a stable head, then keyed blocks that only append.
//! - [`source`]: what a context source offers: its current items, each under a stable key.
//! - [`level`]: how much of an item the view rendered (whole, summary, pointer, folded).
//! - [`levels`]: each item's summary level, generated from its shape (mipmaps for context).
//! - [`motion`]: how each source moves between epoch starts, which orders the next start.
//! - [`view`]: one mind's epoch fed by its sources; turnover is the only rewrite.

pub mod cause;
pub mod epoch;
pub mod level;
pub mod levels;
pub mod motion;
pub mod source;
pub mod view;
