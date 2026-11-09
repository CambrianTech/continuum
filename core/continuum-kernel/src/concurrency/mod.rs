//! Concurrency primitives: the one home for hot-path coordination policy.
//!
//! `policy`: the ConcurrencyPolicy trait and its TokioConcurrencyPolicy impl, single-flight per-key
//! coordination and refcount guards (#1235). Re-exported flat at `crate::concurrency::X`.
//!
//! The generic `MessageProcessor` and `ConcurrentPriorityQueue` that also lived here had no caller
//! outside their own files and the re-export lists (#5395), and went with their only, ignored, test.

pub mod policy;

pub use policy::*;
