//! continuum-models: the `models/*` domain, cut out of continuum-core (commands by module).
//! `module` is the `ModelsModule` service module; `commands` are its typed `models/<verb>`
//! commands. Depends on the engine (model registry, adapters, serving) and the kernel only;
//! core re-exports both under their old `modules::models` / `commands::models` paths.
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
#![deny(clippy::await_holding_lock, clippy::significant_drop_in_scrutinee)] // clean on 2026-11-08 (#5858 CI); CI's clippy step enforces it

pub use continuum_engine::*;

pub mod commands;
pub mod module;
