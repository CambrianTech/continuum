//! System Resources — CPU, memory, and system-wide resource monitoring.
//!
//! Follows the same architecture as gpu/:
//!   - Core types with ts-rs for TypeScript generation
//!   - Singleton monitor with cached readings
//!   - IPC module in modules/system_resources.rs
//!
//! Uses the `sysinfo` crate for cross-platform (macOS/Linux/Windows) monitoring.

pub mod monitor;

pub use monitor::{
    SystemResourceMonitor, CpuStats, MemoryStats, SystemResourceSnapshot,
    ProcessStats, TopProcess,
};
