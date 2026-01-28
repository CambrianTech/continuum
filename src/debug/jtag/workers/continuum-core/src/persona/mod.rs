//! Persona Cognition Module
//!
//! Core persona intelligence in Rust:
//! - PersonaInbox: Priority queue for messages/tasks
//! - PersonaCognitionEngine: Fast decision making
//! - PersonaState: Energy, mood, attention tracking

pub mod cognition;
pub mod inbox;
pub mod types;

pub use cognition::{CognitionDecision, PersonaCognitionEngine, PriorityFactors, PriorityScore};
pub use inbox::PersonaInbox;
pub use types::*;
