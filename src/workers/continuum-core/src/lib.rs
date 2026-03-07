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

pub mod ai;
pub mod audio_constants;
pub mod code;
pub mod concurrent;
pub mod ffi;
pub mod gpu;
pub mod inference;
pub mod ipc;
pub mod live;
pub mod logging;
pub mod memory;
pub mod models;
pub mod modules;
pub mod orm;
pub mod persona;
pub mod rag;
pub mod runtime;
pub mod secrets;
pub mod system_resources;
pub mod tool_parsing;
pub mod utils;

pub use audio_constants::*;

pub use concurrent::*;
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
