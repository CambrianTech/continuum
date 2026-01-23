//! Continuum Core - Rust-first architecture for concurrent AI persona system
//!
//! Design principles:
//! - Message passing via Tokio channels (no locks)
//! - Trait-based abstractions (OOP interfaces)
//! - Work-stealing concurrency (Tokio runtime)
//! - Zero-copy where possible
//! - Performance timing from the ground up

pub mod concurrent;
pub mod voice;
pub mod persona;
pub mod logging;
pub mod ipc;
pub mod ffi;

pub use voice::VoiceOrchestrator;
pub use persona::PersonaInbox;
pub use concurrent::*;
pub use logging::{init_logger, logger, LogLevel};
pub use ipc::start_server;
