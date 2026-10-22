//! A mind's context as an epoch materialization (docs/architecture/MIND-CONTEXT-AND-ENGINE-CONFIG.md).
//! Its own crate so the monolith never grows by it and its tests run in seconds.
//!
//! - [`epoch`]: a stable head, then keyed blocks that only append.
//! - [`source`]: what a context source offers: its current items, each under a stable key.
//! - [`view`]: one mind's epoch fed by its sources; turnover is the only rewrite.

pub mod epoch;
pub mod source;
pub mod view;
