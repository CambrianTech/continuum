//! Sentinel Module — Concurrent, fault-tolerant build/task execution with pipeline support
//!
//! Sentinels are autonomous agents that can run builds, tests, and other
//! long-running processes with proper isolation and logging.
//!
//! Key Design Principles:
//! - **Process Isolation**: Each sentinel runs in a child process (crash isolation)
//! - **Non-blocking**: Heavy processes (Xcode, cargo) don't block the runtime
//! - **Fault Tolerant**: One sentinel failure doesn't cascade to others
//! - **Concurrent**: Multiple sentinels can run in parallel
//! - **Observable**: All output streamed to logs in real-time
//! - **Event-driven**: Emits sentinel:{handle}:log events for real-time streaming
//! - **Pipeline Support**: Multi-step pipelines with LLM, conditions, loops

pub mod checkpoint;
pub mod escalation;
pub mod executor;
pub mod interpolation;
pub mod logs;
pub mod steps;
pub mod types;

pub use types::*;

use async_trait::async_trait;
use dashmap::DashMap;
use parking_lot::RwLock;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

use continuum_kernel::runtime::{
    message_bus::MessageBus, CommandResult, LateBound, ModuleConfig, ModuleContext, ModulePriority,
    ModuleRegistry, ServiceModule,
};
use continuum_kernel::runtime::params::Params;

mod global_sentinel;
pub use global_sentinel::*;
mod sentinel_module;

#[cfg(test)]
mod tests;
