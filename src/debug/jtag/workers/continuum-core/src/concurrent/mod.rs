//! Reusable concurrent patterns for message processing
//!
//! OOP-style traits for common operations:
//! - PriorityQueue<T>: Generic priority-based message queue
//! - MessageProcessor<T>: Process messages concurrently
//! - EventBus<T>: Publish-subscribe pattern
pub mod priority_queue;
pub mod message_processor;

pub use priority_queue::*;
pub use message_processor::*;
