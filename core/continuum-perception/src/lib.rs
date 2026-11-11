//! continuum-perception: a mind's senses (`perception/*` commands: observe, look, interact,
//! hot-edit), cut out of continuum-mind (MODULE-ARCHITECTURE.md section 9.2, card 2f5b275b).
//! Depends only on the live layer and below; continuum-mind re-exports it under its old path.
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
#![deny(clippy::await_holding_lock, clippy::significant_drop_in_scrutinee, clippy::let_underscore_must_use)] // clean on 2026-11-08 (#5858 CI); CI's clippy step enforces it

pub use continuum_live::*;

pub mod perception;
pub mod search;
