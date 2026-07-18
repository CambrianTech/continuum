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

pub mod concurrency;
pub mod disk_eviction;
pub mod disk_pressure;
pub mod disk_reporters;
pub mod memory_pressure;
pub mod monitor;

pub use concurrency::local_inference_capacity;

pub use disk_pressure::{
    is_disk_gate_closed, DiskPathReport, DiskPressureLevel, DiskPressureMonitor,
    DiskPressureSnapshot, DiskReporter,
};
pub use disk_eviction::{CargoTargetPool, DEFAULT_CARGO_TARGET_BUDGET_BYTES};
pub use disk_reporters::{
    install_tracked_dirs, standard_tracked_dirs, tracked_dir, DiskUsageScanner, TrackedDir,
};
pub use memory_pressure::{
    is_memory_gate_closed, MemoryBudgetAllocation, MemoryBudgetSnapshot, MemoryBudgetSpec,
    MemoryPressureMonitor, MemoryPriority, MemoryReporter, ModuleMemoryReport, PressureLevel,
    PressureSnapshot,
};
pub use monitor::{
    CpuStats, MemoryStats, ProcessStats, SystemResourceMonitor, SystemResourceSnapshot, TopProcess,
};

/// Get current process RSS in MB. Reads directly from OS (no caching).
pub fn process_rss_mb() -> u64 {
    #[cfg(target_os = "macos")]
    {
        use std::mem::MaybeUninit;
        let mut info = MaybeUninit::<libc::mach_task_basic_info_data_t>::uninit();
        let mut count = (std::mem::size_of::<libc::mach_task_basic_info_data_t>()
            / std::mem::size_of::<libc::natural_t>())
            as libc::mach_msg_type_number_t;
        #[allow(deprecated)]
        let task = unsafe { libc::mach_task_self() };
        let ret = unsafe {
            libc::task_info(
                task,
                libc::MACH_TASK_BASIC_INFO,
                info.as_mut_ptr() as *mut _,
                &mut count,
            )
        };
        if ret == libc::KERN_SUCCESS {
            let info = unsafe { info.assume_init() };
            return info.resident_size / (1024 * 1024);
        }
        0
    }
    #[cfg(not(target_os = "macos"))]
    {
        if let Ok(statm) = std::fs::read_to_string("/proc/self/statm") {
            if let Some(rss_pages) = statm.split_whitespace().nth(1) {
                if let Ok(pages) = rss_pages.parse::<u64>() {
                    return pages * 4096 / (1024 * 1024);
                }
            }
        }
        0
    }
}
