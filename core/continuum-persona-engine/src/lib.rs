//! The persona engine (CONTINUUM-ENGINE 4.14): our loop over libllama, which replaced the
//! continuous-batching scheduler (continuum-engine `persona_scheduler` drives it). ONE mind decodes at a time. The next is staged as data while she
//! decodes, the previous retires to the RAM tier after the handoff fence, and a switch binds
//! her blocks in milliseconds. Nothing here is a launch flag or a slot count: one-at-a-time is
//! the shape of [`pipeline::Roles`], which cannot hold two active minds.
//!
//! The engine is libllama in-process, called through its API (the fork's `llama_persona_*` and
//! `llama_state_seq_*` over `core/llama`): never a llama-server process configured by launch
//! flags. Each [`residency::Tier`] adapter wraps those calls; the pipeline decides when they run.

pub mod engine;
pub mod persona;
pub mod pipeline;
pub mod residency;
pub mod schedule;
pub mod turn;
