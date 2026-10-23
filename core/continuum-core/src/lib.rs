//! Continuum Core - Rust-first architecture for concurrent AI persona system
//!
//! Design principles:
//! - Message passing via Tokio channels (no locks)
//! - Trait-based abstractions (OOP interfaces)
//! - Work-stealing concurrency (Tokio runtime)
//! - Zero-copy where possible
//! - Performance timing from the ground up
//!
//! Architecture: Rust is the brain, TypeScript is the face.
//! Target: 60-70% Rust (cognition, compute, real-time), 30-40% TypeScript (UI only)

// Self-alias so the `#[derive(Entity)]` macro's emitted paths
// (`::continuum_core::orm::*`) resolve when used from within this
// crate. External consumers reach the same paths through their
// regular `continuum_core` dependency; this `extern crate self`
// closes the home-crate loop. Standard pattern for proc-macros that
// emit absolute paths back into their home crate.
extern crate self as continuum_core;

// The microkernel (core/continuum-kernel): re-exported module by module so `crate::runtime::…`
// paths, and `continuum_core::runtime::…` for every other crate, keep resolving to the one
// owner. New code may name `continuum_kernel` directly.
pub use continuum_engine::{ai, capacity, inference, inference_capability, model_registry, sensory};
pub use continuum_hands::code;
pub use continuum_live::{live, media};
pub use continuum_kernel::{
    concurrency, governor, gpu, identity, logging, orm, paging, paths, resources, routing, runtime,
    sdk_codegen, system_resources,
};
pub use continuum_kernel::{
    action_command, clog_debug, clog_error, clog_info, clog_warn, log_debug, log_error, log_info,
    log_warn, register_command, register_event, register_outcome, register_stateless_command, stack,
    time_async, time_section,
};
pub mod airc;
pub mod cognition;
pub mod commands;
pub mod comms;
pub mod context;
pub mod contracts;
pub mod experience;
pub mod forge;
pub mod genome;
pub mod http;
pub mod boot_plan;
pub mod interface;
pub mod ipc;
pub mod memory;
pub mod modules;
pub mod perception;
pub mod persona;
pub mod provisioning;
pub mod rag;
pub mod recipe;
#[cfg(test)]
pub(crate) mod test_env;
pub mod vdd;


pub use continuum_kernel::concurrency::*;
pub use live::VoiceOrchestrator;
pub use persona::{
    CognitionDecision, InboxMessage, InboxTask, Modality, Mood, PersonaCognitionEngine,
    PersonaInbox, PersonaState, PriorityScore, QueueItem, SenderType,
};
// Easy logging macros - auto-route to proper log files based on module_path!()
// Usage: clog_info!("Session started"); clog_warn!("Warning"); etc.
pub use ipc::start_server;
pub use logging::{extract_component, module_path_to_category};
pub use rag::{LlmMessage, MessageRole, RagContext, RagEngine, RagOptions};
