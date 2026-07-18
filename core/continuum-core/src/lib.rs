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

// objc macros (sel!, msg_send!, class!) must be imported at crate root.
// Used by live::video::metal_gpu_convert for Metal compute shader dispatch.
#[cfg(target_os = "macos")]
#[macro_use]
extern crate objc;

// Self-alias so the `#[derive(Entity)]` macro's emitted paths
// (`::continuum_core::orm::*`) resolve when used from within this
// crate. External consumers reach the same paths through their
// regular `continuum_core` dependency; this `extern crate self`
// closes the home-crate loop. Standard pattern for proc-macros that
// emit absolute paths back into their home crate.
extern crate self as continuum_core;

pub mod ai;
pub mod airc;
pub mod audio_constants;
pub mod code;
pub mod cognition;
pub mod commands;
pub mod comms;
pub mod concurrency;
pub mod config_env;
pub mod context;
pub mod contracts;
pub mod events;
pub mod experience;
pub mod ffi;
pub mod forge;
pub mod genome;
pub mod capacity;
pub mod governor;
pub mod gpu;
pub mod http;
pub mod id_resolve;
pub mod identity;
pub mod inference;
pub mod inference_capability;
pub mod interface;
pub mod ipc;
pub mod live;
pub mod logging;
pub mod memory;
pub mod model_registry;
pub mod modules;
pub mod orm;
pub mod paging;
pub mod paths;
pub mod perception;
pub mod persona;
pub mod provisioning;
pub mod rag;
pub mod resources;
pub mod routing;
pub mod runtime;
pub mod sdk_codegen;
pub mod sensory;
pub mod secrets;
pub mod system_resources;
pub mod tool_parsing;
pub mod utils;
pub mod vdd;

pub use audio_constants::*;

pub use concurrency::*;
pub use live::VoiceOrchestrator;
pub use persona::{
    CognitionDecision, InboxMessage, InboxTask, Modality, Mood, PersonaCognitionEngine,
    PersonaInbox, PersonaState, PriorityScore, QueueItem, SenderType,
};
// Easy logging macros - auto-route to proper log files based on module_path!()
// Usage: clog_info!("Session started"); clog_warn!("Warning"); etc.
pub use ipc::start_server;
pub use logging::{extract_component, init_logger, logger, module_path_to_category, LogLevel};
pub use rag::{LlmMessage, MessageRole, RagContext, RagEngine, RagOptions};
