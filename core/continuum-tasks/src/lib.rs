//! The task contract: work a mind starts gets a handle at once, ends exactly once on every
//! path (done, failed, timed out, stopped, or lost to a panic), and its end is delivered to
//! its OWNER, recorded when it started, never to whoever remembers the handle.
//!
//! One job, below the kernel: the command executor, sentinels and minds build on it. The
//! lineage is an operating system's process table: a task is a process, its end is a signal
//! to its owner. Design: docs/architecture/HER-TASKS.md. Depends only on continuum-base.

mod registry;
mod types;

pub use registry::{TaskGuard, TaskRegistry, TaskWatch};
pub use types::{EndSink, Landing, Owner, StopRefused, TaskEnd, TaskId, TaskKind, TaskRow};

#[cfg(test)]
mod tests;
