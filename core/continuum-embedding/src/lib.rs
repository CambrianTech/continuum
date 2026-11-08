//! continuum-embedding: the `embedding/*` domain, cut out of continuum-core (commands by
//! module). `module` is the `EmbeddingModule` service module; `commands` are its typed
//! `embedding/<verb>` commands. Depends on the engine (`ai::embedding`, serving) and the
//! kernel only; core re-exports both under their old `modules::embedding` /
//! `commands::embedding` paths.
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
#![deny(clippy::await_holding_lock, clippy::significant_drop_in_scrutinee, clippy::let_underscore_must_use)] // clean on 2026-11-08 (#5858 CI); CI's clippy step enforces it

pub use continuum_engine::*;

pub mod commands;
pub mod module;
