//! System Resources — CPU, memory, and system-wide resource monitoring.
//!
//! Follows the same architecture as gpu/:
//!   - Core types with ts-rs for TypeScript generation
//!   - Singleton monitor with cached readings
//!   - IPC module in modules/system_resources.rs
//!
//! Two monitoring systems:
//!   - `SystemResourceMonitor` — on-demand snapshots (request/response via IPC)
//!   - `MemoryPressureMonitor` — autonomous loop, watch channel, pressure-driven
//!
//! Uses the `sysinfo` crate for cross-platform (macOS/Linux/Windows) monitoring.

pub mod memory_pressure;
pub mod monitor;

pub use memory_pressure::{
    MemoryPressureMonitor, MemoryReporter, ModuleMemoryReport, PressureLevel, PressureSnapshot,
};
pub use monitor::{
    CpuStats, MemoryStats, ProcessStats, SystemResourceMonitor, SystemResourceSnapshot, TopProcess,
};
