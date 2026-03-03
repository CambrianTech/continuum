//! SystemResourceMonitor — Cached system resource readings via `sysinfo`.
//!
//! ONE place samples CPU/memory, ALL consumers read cached values.
//! Same compression principle as GpuMemoryManager.
//!
//! ## Refresh Strategy
//!
//! sysinfo's `System::refresh_*()` methods are relatively expensive (~2-5ms).
//! The monitor caches readings and refreshes on demand via `refresh()`.
//! The TypeScript ResourcePressureWatcher calls refresh() on its adaptive
//! polling interval — same pattern as GpuPressureWatcher polling gpuPressure().

use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use sysinfo::{System, ProcessesToUpdate};
use ts_rs::TS;

// =============================================================================
// Types (ts-rs exported)
// =============================================================================

/// CPU statistics snapshot.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../../shared/generated/system/CpuStats.ts")]
pub struct CpuStats {
    /// Number of physical CPU cores
    pub physical_cores: u32,
    /// Number of logical CPU cores (includes hyperthreading)
    pub logical_cores: u32,
    /// Global CPU usage (0.0 - 1.0)
    pub global_usage: f32,
    /// Per-core CPU usage (0.0 - 1.0 each)
    pub per_core_usage: Vec<f32>,
    /// CPU brand/model string (e.g., "Apple M4 Max")
    pub brand: String,
}

/// Memory statistics snapshot.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../../shared/generated/system/MemoryStats.ts")]
pub struct MemoryStats {
    /// Total physical RAM in bytes
    #[ts(type = "number")]
    pub total_bytes: u64,
    /// Used physical RAM in bytes
    #[ts(type = "number")]
    pub used_bytes: u64,
    /// Available physical RAM in bytes (total - used)
    #[ts(type = "number")]
    pub available_bytes: u64,
    /// Memory pressure (0.0 - 1.0) = used / total
    pub pressure: f32,
    /// Total swap in bytes
    #[ts(type = "number")]
    pub swap_total_bytes: u64,
    /// Used swap in bytes
    #[ts(type = "number")]
    pub swap_used_bytes: u64,
}

/// A single process's resource usage.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../../shared/generated/system/TopProcess.ts")]
pub struct TopProcess {
    /// Process ID
    pub pid: u32,
    /// Process name
    pub name: String,
    /// CPU usage (0.0 - 100.0+ for multi-core)
    pub cpu_percent: f32,
    /// Memory usage in bytes
    #[ts(type = "number")]
    pub memory_bytes: u64,
}

/// Top processes by resource usage.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../../shared/generated/system/ProcessStats.ts")]
pub struct ProcessStats {
    /// Top N processes by CPU usage
    pub top_by_cpu: Vec<TopProcess>,
    /// Top N processes by memory usage
    pub top_by_memory: Vec<TopProcess>,
}

/// Full system resource snapshot — returned by `system/resources` IPC command.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../../shared/generated/system/SystemResourceSnapshot.ts")]
pub struct SystemResourceSnapshot {
    /// CPU statistics
    pub cpu: CpuStats,
    /// Memory statistics
    pub memory: MemoryStats,
    /// Top processes (optional, only when requested)
    #[ts(optional)]
    pub processes: Option<ProcessStats>,
    /// Timestamp of this snapshot (ms since epoch)
    #[ts(type = "number")]
    pub timestamp_ms: u64,
    /// System uptime in seconds
    #[ts(type = "number")]
    pub uptime_seconds: u64,
}

// =============================================================================
// Monitor
// =============================================================================

/// Cached system resource monitor.
///
/// Thread-safe via Mutex. Refresh is explicit — callers control when to sample.
/// The TypeScript ResourcePressureWatcher drives the refresh cadence.
pub struct SystemResourceMonitor {
    inner: Mutex<MonitorInner>,
}

struct MonitorInner {
    system: System,
    /// Cached CPU stats
    cpu: CpuStats,
    /// Cached memory stats
    memory: MemoryStats,
    /// Last refresh timestamp
    last_refresh_ms: u64,
    /// Whether processes have been refreshed at least once (needed for CPU delta baseline)
    processes_baselined: bool,
}

impl SystemResourceMonitor {
    /// Create a new monitor. Performs initial CPU + memory refresh.
    pub fn new() -> Self {
        let mut system = System::new();
        // Initial refresh: CPU list for brand/core count + memory
        system.refresh_cpu_all();
        system.refresh_memory();
        // sysinfo requires a second CPU refresh after a delay for accurate usage.
        // First call establishes baseline, second gives delta. The monitor starts
        // with 0% usage — first real readings come from the watcher's first refresh().

        let physical_cores = system.physical_core_count().unwrap_or(1) as u32;
        let cpus = system.cpus();
        let logical_cores = cpus.len() as u32;
        let brand = cpus.first().map(|c| c.brand().to_string()).unwrap_or_default();

        let cpu = CpuStats {
            physical_cores,
            logical_cores,
            global_usage: system.global_cpu_usage() / 100.0,
            per_core_usage: cpus.iter().map(|c| c.cpu_usage() / 100.0).collect(),
            brand,
        };

        let memory = Self::read_memory(&system);

        Self {
            inner: Mutex::new(MonitorInner {
                system,
                cpu,
                memory,
                last_refresh_ms: now_ms(),
                processes_baselined: false,
            }),
        }
    }

    /// Refresh all readings. Call this from the TypeScript watcher on each poll.
    /// Returns the updated snapshot.
    pub fn refresh(&self) -> SystemResourceSnapshot {
        if let Ok(mut inner) = self.inner.lock() {
            inner.system.refresh_cpu_all();
            inner.system.refresh_memory();

            inner.cpu.global_usage = inner.system.global_cpu_usage() / 100.0;
            inner.cpu.per_core_usage = inner.system.cpus().iter().map(|c| c.cpu_usage() / 100.0).collect();
            inner.memory = Self::read_memory(&inner.system);
            inner.last_refresh_ms = now_ms();

            SystemResourceSnapshot {
                cpu: inner.cpu.clone(),
                memory: inner.memory.clone(),
                processes: None,
                timestamp_ms: inner.last_refresh_ms,
                uptime_seconds: System::uptime(),
            }
        } else {
            // Mutex poisoned — return stale data
            self.snapshot()
        }
    }

    /// Refresh with process data (more expensive — iterates all processes).
    /// Used by `system/resources?includeProcesses=true`.
    ///
    /// sysinfo computes per-process CPU as a delta between consecutive
    /// `refresh_processes()` calls. The first call establishes a baseline
    /// (all CPU values are 0%). We detect this and do a double-refresh
    /// with a brief sleep so the first query returns real data.
    pub fn refresh_with_processes(&self, top_n: usize) -> SystemResourceSnapshot {
        if let Ok(mut inner) = self.inner.lock() {
            inner.system.refresh_cpu_all();
            inner.system.refresh_memory();

            if !inner.processes_baselined {
                // First process refresh ever — establish baseline
                inner.system.refresh_processes(ProcessesToUpdate::All, true);
                // Brief delay so sysinfo can compute CPU deltas
                std::thread::sleep(std::time::Duration::from_millis(150));
                inner.processes_baselined = true;
            }
            // Real refresh with delta-accurate CPU data
            inner.system.refresh_processes(ProcessesToUpdate::All, true);

            inner.cpu.global_usage = inner.system.global_cpu_usage() / 100.0;
            inner.cpu.per_core_usage = inner.system.cpus().iter().map(|c| c.cpu_usage() / 100.0).collect();
            inner.memory = Self::read_memory(&inner.system);
            inner.last_refresh_ms = now_ms();

            let processes = Self::read_processes(&inner.system, top_n);

            SystemResourceSnapshot {
                cpu: inner.cpu.clone(),
                memory: inner.memory.clone(),
                processes: Some(processes),
                timestamp_ms: inner.last_refresh_ms,
                uptime_seconds: System::uptime(),
            }
        } else {
            self.snapshot()
        }
    }

    /// Get cached CPU stats (no refresh).
    pub fn cpu(&self) -> CpuStats {
        self.inner.lock()
            .map(|inner| inner.cpu.clone())
            .unwrap_or_else(|_| CpuStats {
                physical_cores: 1,
                logical_cores: 1,
                global_usage: 0.0,
                per_core_usage: vec![],
                brand: String::new(),
            })
    }

    /// Get cached memory stats (no refresh).
    pub fn memory(&self) -> MemoryStats {
        self.inner.lock()
            .map(|inner| inner.memory.clone())
            .unwrap_or_else(|_| MemoryStats {
                total_bytes: 0,
                used_bytes: 0,
                available_bytes: 0,
                pressure: 0.0,
                swap_total_bytes: 0,
                swap_used_bytes: 0,
            })
    }

    /// Get cached snapshot (no refresh).
    pub fn snapshot(&self) -> SystemResourceSnapshot {
        if let Ok(inner) = self.inner.lock() {
            SystemResourceSnapshot {
                cpu: inner.cpu.clone(),
                memory: inner.memory.clone(),
                processes: None,
                timestamp_ms: inner.last_refresh_ms,
                uptime_seconds: System::uptime(),
            }
        } else {
            SystemResourceSnapshot {
                cpu: CpuStats {
                    physical_cores: 1,
                    logical_cores: 1,
                    global_usage: 0.0,
                    per_core_usage: vec![],
                    brand: String::new(),
                },
                memory: MemoryStats {
                    total_bytes: 0,
                    used_bytes: 0,
                    available_bytes: 0,
                    pressure: 0.0,
                    swap_total_bytes: 0,
                    swap_used_bytes: 0,
                },
                processes: None,
                timestamp_ms: 0,
                uptime_seconds: 0,
            }
        }
    }

    /// CPU pressure (0.0 - 1.0). Cached value.
    pub fn cpu_pressure(&self) -> f32 {
        self.inner.lock().map(|i| i.cpu.global_usage).unwrap_or(0.0)
    }

    /// Memory pressure (0.0 - 1.0). Cached value.
    pub fn memory_pressure(&self) -> f32 {
        self.inner.lock().map(|i| i.memory.pressure).unwrap_or(0.0)
    }

    // ── Internal ────────────────────────────────────────────────────

    fn read_memory(system: &System) -> MemoryStats {
        let total = system.total_memory();
        let used = system.used_memory();
        let available = total.saturating_sub(used);
        let pressure = if total > 0 { used as f32 / total as f32 } else { 0.0 };

        MemoryStats {
            total_bytes: total,
            used_bytes: used,
            available_bytes: available,
            pressure,
            swap_total_bytes: system.total_swap(),
            swap_used_bytes: system.used_swap(),
        }
    }

    fn read_processes(system: &System, top_n: usize) -> ProcessStats {
        let mut by_cpu: Vec<TopProcess> = system.processes().values()
            .map(|p| TopProcess {
                pid: p.pid().as_u32(),
                name: p.name().to_string_lossy().to_string(),
                cpu_percent: p.cpu_usage(),
                memory_bytes: p.memory(),
            })
            .collect();

        // Sort by CPU descending
        by_cpu.sort_by(|a, b| b.cpu_percent.partial_cmp(&a.cpu_percent).unwrap_or(std::cmp::Ordering::Equal));
        let top_by_cpu: Vec<TopProcess> = by_cpu.iter().take(top_n).cloned().collect();

        // Sort by memory descending
        by_cpu.sort_by(|a, b| b.memory_bytes.cmp(&a.memory_bytes));
        let top_by_memory: Vec<TopProcess> = by_cpu.into_iter().take(top_n).collect();

        ProcessStats {
            top_by_cpu,
            top_by_memory,
        }
    }
}

impl std::fmt::Debug for SystemResourceMonitor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Ok(inner) = self.inner.lock() {
            f.debug_struct("SystemResourceMonitor")
                .field("cpu_usage", &inner.cpu.global_usage)
                .field("mem_pressure", &inner.memory.pressure)
                .finish()
        } else {
            f.debug_struct("SystemResourceMonitor").finish()
        }
    }
}

// =============================================================================
// Helpers
// =============================================================================

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_monitor() {
        let monitor = SystemResourceMonitor::new();
        let cpu = monitor.cpu();
        assert!(cpu.physical_cores >= 1);
        assert!(cpu.logical_cores >= 1);
        assert!(!cpu.brand.is_empty());
    }

    #[test]
    fn test_memory_stats() {
        let monitor = SystemResourceMonitor::new();
        let mem = monitor.memory();
        assert!(mem.total_bytes > 0, "Total memory should be > 0");
        assert!(mem.pressure >= 0.0 && mem.pressure <= 1.0,
            "Memory pressure should be 0.0-1.0, got {}", mem.pressure);
    }

    #[test]
    fn test_refresh() {
        let monitor = SystemResourceMonitor::new();
        // Give sysinfo baseline time
        std::thread::sleep(std::time::Duration::from_millis(200));
        let snapshot = monitor.refresh();
        assert!(snapshot.cpu.physical_cores >= 1);
        assert!(snapshot.memory.total_bytes > 0);
        assert!(snapshot.timestamp_ms > 0);
        assert!(snapshot.uptime_seconds > 0);
    }

    #[test]
    fn test_refresh_with_processes() {
        let monitor = SystemResourceMonitor::new();
        std::thread::sleep(std::time::Duration::from_millis(200));
        let snapshot = monitor.refresh_with_processes(5);
        assert!(snapshot.processes.is_some());
        let procs = snapshot.processes.unwrap();
        // Should find at least 1 process (our own test process)
        assert!(!procs.top_by_cpu.is_empty() || !procs.top_by_memory.is_empty(),
            "Should find at least one process");
    }

    #[test]
    fn test_cpu_pressure_range() {
        let monitor = SystemResourceMonitor::new();
        let pressure = monitor.cpu_pressure();
        assert!(pressure >= 0.0 && pressure <= 1.0,
            "CPU pressure should be 0.0-1.0, got {}", pressure);
    }

    #[test]
    fn test_memory_pressure_range() {
        let monitor = SystemResourceMonitor::new();
        let pressure = monitor.memory_pressure();
        assert!(pressure >= 0.0 && pressure <= 1.0,
            "Memory pressure should be 0.0-1.0, got {}", pressure);
    }

    #[test]
    fn test_snapshot_no_refresh() {
        let monitor = SystemResourceMonitor::new();
        let snap = monitor.snapshot();
        assert!(snap.cpu.physical_cores >= 1);
        assert!(snap.processes.is_none(), "snapshot() should not include processes");
    }

    // ── ts-rs binding tests ─────────────────────────────────────────

    #[test]
    fn export_bindings_cpu_stats() {
        let cfg = ts_rs::Config::default();
        CpuStats::export_all(&cfg).unwrap();
    }

    #[test]
    fn export_bindings_memory_stats() {
        let cfg = ts_rs::Config::default();
        MemoryStats::export_all(&cfg).unwrap();
    }

    #[test]
    fn export_bindings_system_resource_snapshot() {
        let cfg = ts_rs::Config::default();
        SystemResourceSnapshot::export_all(&cfg).unwrap();
    }

    #[test]
    fn export_bindings_top_process() {
        let cfg = ts_rs::Config::default();
        TopProcess::export_all(&cfg).unwrap();
    }

    #[test]
    fn export_bindings_process_stats() {
        let cfg = ts_rs::Config::default();
        ProcessStats::export_all(&cfg).unwrap();
    }
}
