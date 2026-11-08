//! continuum-embedding: the `embedding/*` domain, cut out of continuum-core (commands by
//! module). `module` is the `EmbeddingModule` service module; `commands` are its typed
//! `embedding/<verb>` commands. Depends on the engine (`ai::embedding`, serving) and the
//! kernel only; core re-exports both under their old `modules::embedding` /
//! `commands::embedding` paths.
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub use continuum_engine::*;

pub mod commands;
pub mod module;
