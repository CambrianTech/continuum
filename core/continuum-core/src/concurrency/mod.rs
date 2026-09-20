//! Concurrency primitives — single source of truth for hot-path coordination.
//!
//! Consolidates the previously-parallel `concurrent/` and `concurrency/`
//! top-level dirs into one module. Prior to this refactor:
//!   - `concurrent/`: data structures (MessageProcessor, PriorityQueue)
//!   - `concurrency/`: policies (ConcurrencyPolicy, TokioConcurrencyPolicy,
//!     single-flight maps, semaphores)
//!
//! Two dirs with overlapping names was an architecture smell — neither
//! was the canonical "where do concurrency mechanics live" answer. This
//! module now is. Domain modules import from `crate::concurrency::*`.
//!
//! ## Module layout
//!
//! - `policy` — ConcurrencyPolicy trait + TokioConcurrencyPolicy impl,
//!   single-flight per-key coordination, refcount guards (#1235).
//!   Used by `cognition::shared_analysis`.
//! - `message_processor` — Reusable `MessageProcessor` trait for
//!   processing messages concurrently. Generic over message type.
//! - `priority_queue` — Generic priority-based message queue.
//!
//! ## Submodules vs flat
//!
//! Files stay separate so callers reading a 200-LOC priority_queue
//! impl don't also have to scroll past 600+ LOC of policy machinery.
//! Re-exports here keep the public API flat at `crate::concurrency::X`.

pub mod message_processor;
pub mod policy;
pub mod priority_queue;

pub use message_processor::*;
pub use policy::*;
pub use priority_queue::*;
