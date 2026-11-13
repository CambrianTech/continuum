//! continuum-memory: a mind's memory (the hippocampus: per-persona corpus, multi-layer
//! recall, consolidation, embeddings), cut out of continuum-mind (card 03e2a1c8). Depends
//! only on the engine and below; continuum-mind re-exports it under its old path. `commands` are the typed
//! `memory/<verb>` commands (all but `consolidate`, which submits training and stays in
//! core) and `state` is the `MemoryState` they share.
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
#![deny(clippy::await_holding_lock, clippy::significant_drop_in_scrutinee, clippy::let_underscore_must_use)] // clean on 2026-11-08 (#5858 CI); CI's clippy step enforces it

pub use continuum_engine::*;

pub mod commands;
pub mod memory;
pub mod state;
