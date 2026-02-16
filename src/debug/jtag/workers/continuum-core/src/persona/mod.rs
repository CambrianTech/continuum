//! Persona Cognition Module
//!
//! Core persona intelligence in Rust:
//! - PersonaInbox: Priority queue for messages/tasks (flat, legacy)
//! - PersonaCognitionEngine: Fast decision making
//! - PersonaState: Energy, mood, attention tracking
//! - Channel system: Multi-channel queue with item polymorphism (replaces flat inbox)
//!   - channel_types: ActivityDomain enum + QueueItemBehavior trait
//!   - channel_items: Voice, Chat, Task concrete item structs
//!   - channel_queue: Generic per-domain queue container
//!   - channel_registry: Domain-to-queue routing + service_cycle()

pub mod channel_items;
pub mod channel_queue;
pub mod channel_registry;
pub mod channel_types;
pub mod cognition;
pub mod inbox;
pub mod self_task_generator;
pub mod text_analysis;
pub mod types;

pub use channel_items::ChannelEnqueueRequest;
pub use channel_registry::ChannelRegistry;
pub use channel_types::{ActivityDomain, ChannelRegistryStatus, ChannelStatus, ServiceCycleResult};
pub use cognition::{CognitionDecision, PersonaCognitionEngine, PriorityFactors, PriorityScore};
pub use inbox::PersonaInbox;
pub use types::*;
