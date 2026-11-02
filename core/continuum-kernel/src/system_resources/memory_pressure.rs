//! MemoryPressureMonitor — Independent, non-blocking memory surveillance system.
//!
//! Runs on its own tokio task with its own interval. Cannot block or be blocked by
//! any other system (IPC, Bevy, audio, inference). Crash-proof: panics in any
//! reporter are caught and logged, never propagated.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────┐
//! │          MemoryPressureMonitor (own task)        │
//! │                                                 │
//! │  poll_interval: 2s                              │
//! │                                                 │
//! │  ┌─────────────┐  ┌──────────────┐              │
//! │  │ sysinfo RSS  │  │ Per-module   │              │
//! │  │ swap, avail  │  │ reporters    │              │
//! │  └─────────────┘  └──────────────┘              │
//! │         │                 │                      │
//! │         ▼                 ▼                      │
//! │  ┌──────────────────────────────┐               │
//! │  │   PressureSnapshot (atomic)  │               │
//! │  │   - level: Normal/Warning/   │               │
//! │  │     High/Critical            │               │
//! │  │   - rss_bytes                │               │
//! │  │   - available_bytes          │               │
//! │  │   - pressure: 0.0-1.0       │               │
//! │  │   - per_module breakdown    │               │
//! │  └──────────────────────────────┘               │
//! │         │                                       │
//! │         ▼                                       │
//! │  tokio::sync::watch → subscribers read freely   │
//! └─────────────────────────────────────────────────┘
//! ```
//!
//! ## Isolation Guarantees
//!
//! - Own tokio task: `tokio::spawn` with `catch_unwind` wrapping the entire future
//! - No shared locks with Bevy, IPC, or audio systems
//! - Each reporter called via `spawn_blocking` + 100ms timeout (catches hangs and panics)
//! - Reporter panics/timeouts quarantine the reporter after 3 consecutive failures
//! - Watch channel for consumers: readers never block the monitor
//!
//! ## Usage
//!
//! ```rust,ignore
//! // Start the monitor (once, at server boot)
//! let monitor = MemoryPressureMonitor::start();
//!
//! // Any system can subscribe to pressure changes
//! let mut rx = monitor.subscribe();
//! tokio::spawn(async move {
//!     while rx.changed().await.is_ok() {
//!         let snapshot = rx.borrow();
//!         if snapshot.level >= PressureLevel::High {
//!             // shed load
//!         }
//!     }
//! });
//!
//! // Or poll current state (lock-free)
//! let snapshot = monitor.current();
//! ```

use async_trait::async_trait;
use serde::Serialize;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;
use ts_rs::TS;

use crate::runtime::{spawn_daemon, Daemon, DaemonChannel};
use crate::{clog_info, clog_warn};

mod poll_interval;
pub use poll_interval::*;
mod shed_lru;
pub use shed_lru::{register_holder, ByteSized, Holder, ShedLru};
mod memory_pressure_monitor;

// =============================================================================
// Tests
// =============================================================================
#[cfg(test)]
mod tests;
