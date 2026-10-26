//! continuum-memory: a mind's memory (the hippocampus: per-persona corpus, multi-layer
//! recall, consolidation, embeddings), cut out of continuum-mind (card 03e2a1c8). Depends
//! only on the engine and below; continuum-mind re-exports it under its old path. `commands` are the typed
//! `memory/<verb>` commands (all but `consolidate`, which submits training and stays in
//! core) and `state` is the `MemoryState` they share.

pub use continuum_engine::*;

pub mod commands;
pub mod memory;
pub mod state;
