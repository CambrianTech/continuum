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

pub mod absence_watch;
pub mod citizen_workspace_pool;
pub mod eye_reaper;
pub mod harness_archive;
pub mod process_anomaly;
pub mod bounded_command;
pub mod cargo_discovery;
pub mod concurrency;
pub mod disk_eviction;
pub mod disk_pressure;
pub mod disk_relief;
pub mod airc_worktree_pool;
pub mod dev_worktree_pool;
pub mod fd_gauge;
pub mod power_assertion;
pub mod fd_pressure;
pub mod hardware_profile;
pub mod disk_reporters;
pub mod drives;
pub mod hands_lease;
pub mod confinement;
pub mod memory_pressure;
pub mod monitor;
pub mod rotation_log_pool;
pub mod swap_activity;
pub mod sustained;
pub mod ports;
pub mod process_tree;

pub use concurrency::local_inference_capacity;

pub use disk_eviction::{
    serving_active_artifacts, serving_tier_capacity_bytes, ActiveArtifactSet, CargoTargetPool,
    cargo_target_budget_bytes, NvmeServingTierPool, DEFAULT_CARGO_TARGET_BUDGET_BYTES,
};
pub use disk_pressure::{
    is_disk_gate_closed, DiskPathReport, DiskPressureLevel, DiskPressureMonitor,
    DiskPressureSnapshot, DiskReporter,
};
pub use disk_reporters::{
    install_tracked_dirs, standard_tracked_dirs, tracked_dir, DiskUsageScanner, TrackedDir,
    CARGO_TARGET_CLASSES,
};
pub use memory_pressure::{
    current_available_bytes, is_memory_gate_closed, MemoryBudgetAllocation, MemoryBudgetSnapshot,
    MemoryBudgetSpec, MemoryPressureMonitor, MemoryPriority, MemoryReporter, ModuleMemoryReport,
    PressureLevel, PressureSnapshot,
};
pub use monitor::{
    CpuStats, MemoryStats, ProcessStats, SystemResourceMonitor, SystemResourceSnapshot, TopProcess,
};
pub use rotation_log_pool::RotationLogPool;

