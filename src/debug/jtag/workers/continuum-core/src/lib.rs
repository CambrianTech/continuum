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

pub mod audio_constants;
pub mod concurrent;
pub mod voice;
pub mod persona;
pub mod logging;
pub mod ipc;
pub mod ffi;
pub mod utils;
pub mod rag;
pub mod memory;

pub use audio_constants::*;

pub use voice::VoiceOrchestrator;
pub use persona::{
    PersonaInbox, PersonaCognitionEngine, PersonaState,
    CognitionDecision, PriorityScore, InboxMessage, InboxTask,
    QueueItem, SenderType, Modality, Mood,
};
pub use concurrent::*;
pub use logging::{init_logger, logger, LogLevel};
pub use ipc::start_server;
pub use voice::call_server::CallManager;
pub use rag::{RagEngine, RagContext, RagOptions, LlmMessage, MessageRole};
