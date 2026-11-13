//! Adapters from an attached airc handle to the persona-facing citizen traits.
//!
//! The `Context` actor-handle trait, `StubContext` and `AgentContext` that lived here had no
//! caller outside their own tests and were removed as dead; only the airc adapter is wired
//! (`ipc/positron_presence.rs`).

pub mod airc_adapter;
