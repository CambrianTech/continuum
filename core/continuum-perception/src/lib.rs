//! continuum-perception: a mind's senses (`perception/*` commands: observe, look, interact,
//! hot-edit), cut out of continuum-mind (MODULE-ARCHITECTURE.md section 9.2, card 2f5b275b).
//! Depends only on the live layer and below; continuum-mind re-exports it under its old path.
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub use continuum_live::*;

pub mod perception;
pub mod search;
