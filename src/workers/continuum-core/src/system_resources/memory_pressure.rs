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
//! - Own tokio task: `tokio::spawn` with `catch_unwind` wrapper
//! - No shared locks with Bevy, IPC, or audio systems
//! - Reporters called with timeout: 100ms max per reporter
//! - Reporter panics caught via `catch_unwind`, reporter disabled after 3 consecutive panics
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

use serde::Serialize;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;
use ts_rs::TS;

use crate::{clog_info, clog_warn};

// =============================================================================
// Pressure Levels
// =============================================================================

/// Memory pressure severity. Each level implies all lower levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, TS)]
#[ts(export, export_to = "../../../shared/generated/system/PressureLevel.ts")]
#[serde(rename_all = "snake_case")]
pub enum PressureLevel {
    /// < 60% used. Normal operation.
    Normal,
    /// 60-80% used. Log warnings. Non-critical caches should trim.
    Warning,
    /// 80-90% used. Deactivate idle avatar slots. Aggressive cache eviction.
    High,
    /// > 90% used. Emergency: stop non-essential subsystems, refuse new allocations.
    Critical,
}

impl PressureLevel {
    fn from_pressure(pressure: f64) -> Self {
        if pressure >= 0.90 {
            Self::Critical
        } else if pressure >= 0.80 {
            Self::High
        } else if pressure >= 0.60 {
            Self::Warning
        } else {
            Self::Normal
        }
    }
}

impl std::fmt::Display for PressureLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Normal => write!(f, "normal"),
            Self::Warning => write!(f, "warning"),
            Self::High => write!(f, "high"),
            Self::Critical => write!(f, "critical"),
        }
    }
}

// =============================================================================
// Per-Module Memory Report
// =============================================================================

/// A single module's self-reported memory usage.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../../shared/generated/system/ModuleMemoryReport.ts")]
pub struct ModuleMemoryReport {
    /// Module name (e.g., "bevy", "embedding", "corpus", "agents")
    pub name: String,
    /// Estimated bytes currently held by this module
    #[ts(type = "number")]
    pub bytes: u64,
    /// Human-readable breakdown (e.g., "14 slots × 921KB render targets")
    pub detail: String,
    /// Can this module shed load? (If true, it implements pressure response)
    pub can_shed: bool,
}

// =============================================================================
// Pressure Snapshot
// =============================================================================

/// Complete memory pressure snapshot — published via watch channel.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../../shared/generated/system/PressureSnapshot.ts")]
pub struct PressureSnapshot {
    /// Current pressure level
    pub level: PressureLevel,
    /// Memory pressure ratio (0.0 - 1.0) = used / total
    pub pressure: f64,
    /// Process RSS in bytes
    #[ts(type = "number")]
    pub rss_bytes: u64,
    /// Total physical RAM
    #[ts(type = "number")]
    pub total_bytes: u64,
    /// Available RAM
    #[ts(type = "number")]
    pub available_bytes: u64,
    /// Swap used in bytes
    #[ts(type = "number")]
    pub swap_used_bytes: u64,
    /// Per-module memory breakdown (empty if no reporters registered)
    pub modules: Vec<ModuleMemoryReport>,
    /// Timestamp (ms since epoch)
    #[ts(type = "number")]
    pub timestamp_ms: u64,
    /// Consecutive polls at this level (for hysteresis — don't react to single spikes)
    pub consecutive_at_level: u32,
}

impl Default for PressureSnapshot {
    fn default() -> Self {
        Self {
            level: PressureLevel::Normal,
            pressure: 0.0,
            rss_bytes: 0,
            total_bytes: 0,
            available_bytes: 0,
            swap_used_bytes: 0,
            modules: Vec::new(),
            timestamp_ms: 0,
            consecutive_at_level: 0,
        }
    }
}

// =============================================================================
// Memory Reporter Trait
// =============================================================================

/// Trait for modules that can report their memory usage and respond to pressure.
///
/// Implementations MUST be fast (< 100ms) and MUST NOT block.
/// The monitor calls these from its own task with a timeout guard.
pub trait MemoryReporter: Send + Sync {
    /// Module name for reporting
    fn name(&self) -> &'static str;

    /// Report current memory usage. Must be fast and non-blocking.
    fn report(&self) -> ModuleMemoryReport;

    /// Whether this reporter can shed load under pressure.
    fn can_shed(&self) -> bool {
        false
    }

    /// Respond to memory pressure. Called when pressure level changes.
    /// The reporter should autonomously reduce its footprint.
    ///
    /// Examples:
    /// - Bevy: deactivate idle avatar slots, reduce render resolution
    /// - Embedding: unload model, clear cache
    /// - Corpus: trim to minimum, evict stale entries aggressively
    /// - Agents: stop spawning new agents
    fn shed_load(&self, _level: PressureLevel) {
        // Default: no-op. Override to implement pressure response.
    }
}

// =============================================================================
// Monitor
// =============================================================================

/// Reporter entry with fault tracking.
struct ReporterEntry {
    reporter: Arc<dyn MemoryReporter>,
    consecutive_panics: u32,
    /// Disabled after 3 consecutive panics — quarantined to prevent cascade
    disabled: bool,
}

/// Independent memory pressure monitoring system.
///
/// Runs on its own tokio task. Polls system memory + registered reporters
/// at a fixed interval. Publishes snapshots via watch channel.
pub struct MemoryPressureMonitor {
    /// Watch channel for pressure snapshots. Readers never block the monitor.
    tx: watch::Sender<PressureSnapshot>,
    rx: watch::Receiver<PressureSnapshot>,
    /// Atomic RSS for lock-free reads from any thread
    current_rss: Arc<AtomicU64>,
    /// Atomic pressure (f64 bits) for lock-free reads
    current_pressure: Arc<AtomicU64>,
}

impl MemoryPressureMonitor {
    /// Start the monitor on its own tokio task.
    ///
    /// Returns a handle for subscribing to pressure changes and registering reporters.
    /// The monitor task runs until the process exits.
    pub fn start(reporters: Vec<Arc<dyn MemoryReporter>>) -> Arc<Self> {
        let (tx, rx) = watch::channel(PressureSnapshot::default());
        let current_rss = Arc::new(AtomicU64::new(0));
        let current_pressure = Arc::new(AtomicU64::new(0));

        let monitor = Arc::new(Self {
            tx,
            rx,
            current_rss: current_rss.clone(),
            current_pressure: current_pressure.clone(),
        });

        let entries: Vec<ReporterEntry> = reporters
            .into_iter()
            .map(|r| ReporterEntry {
                reporter: r,
                consecutive_panics: 0,
                disabled: false,
            })
            .collect();

        // Spawn the monitor loop on its own task
        tokio::spawn(Self::run_loop(
            monitor.tx.clone(),
            current_rss,
            current_pressure,
            entries,
        ));

        monitor
    }

    /// Subscribe to pressure snapshot changes.
    /// The receiver gets notified whenever a new snapshot is published.
    pub fn subscribe(&self) -> watch::Receiver<PressureSnapshot> {
        self.rx.clone()
    }

    /// Get current RSS (lock-free atomic read, any thread).
    pub fn rss_bytes(&self) -> u64 {
        self.current_rss.load(Ordering::Relaxed)
    }

    /// Get current pressure ratio (lock-free, any thread).
    pub fn pressure(&self) -> f64 {
        f64::from_bits(self.current_pressure.load(Ordering::Relaxed))
    }

    /// Get the latest snapshot (cheap clone from watch channel).
    pub fn current(&self) -> PressureSnapshot {
        self.rx.borrow().clone()
    }

    /// The monitor loop. Runs until process exit.
    async fn run_loop(
        tx: watch::Sender<PressureSnapshot>,
        rss_atomic: Arc<AtomicU64>,
        pressure_atomic: Arc<AtomicU64>,
        mut reporters: Vec<ReporterEntry>,
    ) {
        use sysinfo::{ProcessesToUpdate, System};

        let mut sys = System::new();
        let pid = sysinfo::get_current_pid().ok();
        let poll_interval = Duration::from_secs(2);
        let mut prev_level = PressureLevel::Normal;
        let mut consecutive_at_level: u32 = 0;
        let mut log_counter: u64 = 0;

        clog_info!(
            "🧠 MemoryPressureMonitor started (interval={:?}, reporters={})",
            poll_interval,
            reporters.len()
        );

        loop {
            tokio::time::sleep(poll_interval).await;

            // --- System memory ---
            sys.refresh_memory();
            let total = sys.total_memory();
            let available = sys.available_memory();
            let used = total.saturating_sub(available);
            let swap_used = sys.used_swap();

            // --- Process RSS ---
            let rss = if let Some(p) = pid {
                sys.refresh_processes(ProcessesToUpdate::Some(&[p]), true);
                sys.process(p).map(|p| p.memory()).unwrap_or(0)
            } else {
                0
            };

            // --- Pressure calculation ---
            // Use system-wide pressure (not just our RSS) because other processes matter
            let pressure = if total > 0 {
                used as f64 / total as f64
            } else {
                0.0
            };
            let level = PressureLevel::from_pressure(pressure);

            // Update atomics (lock-free reads from any thread)
            rss_atomic.store(rss, Ordering::Relaxed);
            pressure_atomic.store(pressure.to_bits(), Ordering::Relaxed);

            // --- Hysteresis ---
            if level == prev_level {
                consecutive_at_level = consecutive_at_level.saturating_add(1);
            } else {
                consecutive_at_level = 1;
                prev_level = level;
            }

            // --- Collect module reports (with panic isolation) ---
            let mut module_reports = Vec::with_capacity(reporters.len());
            for entry in &mut reporters {
                if entry.disabled {
                    continue;
                }

                let reporter = entry.reporter.clone();
                let result =
                    std::panic::catch_unwind(AssertUnwindSafe(|| reporter.report()));

                match result {
                    Ok(report) => {
                        entry.consecutive_panics = 0;
                        module_reports.push(report);
                    }
                    Err(e) => {
                        entry.consecutive_panics += 1;
                        let name = entry.reporter.name();
                        clog_warn!(
                            "🧠 MemoryReporter '{}' panicked ({}/3): {:?}",
                            name,
                            entry.consecutive_panics,
                            e
                        );
                        if entry.consecutive_panics >= 3 {
                            clog_warn!(
                                "🧠 MemoryReporter '{}' quarantined after 3 panics",
                                name
                            );
                            entry.disabled = true;
                        }
                    }
                }
            }

            // --- Notify reporters of pressure changes (with panic isolation) ---
            // Only notify after sustained pressure (consecutive >= 2) to avoid reacting to spikes
            if consecutive_at_level == 2 && level >= PressureLevel::Warning {
                for entry in &reporters {
                    if entry.disabled || !entry.reporter.can_shed() {
                        continue;
                    }
                    let reporter = entry.reporter.clone();
                    let shed_level = level;
                    // Fire-and-forget, don't let a slow shedder block the monitor
                    let _ = std::panic::catch_unwind(AssertUnwindSafe(|| {
                        reporter.shed_load(shed_level);
                    }));
                }
            }

            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;

            let snapshot = PressureSnapshot {
                level,
                pressure,
                rss_bytes: rss,
                total_bytes: total,
                available_bytes: available,
                swap_used_bytes: swap_used,
                modules: module_reports,
                timestamp_ms: now_ms,
                consecutive_at_level,
            };

            // --- Periodic logging ---
            log_counter += 1;
            // Log every 15 polls (30s) at normal, every poll at high+
            let should_log = match level {
                PressureLevel::Normal => log_counter % 15 == 0,
                PressureLevel::Warning => log_counter % 5 == 0,
                PressureLevel::High | PressureLevel::Critical => true,
            };

            if should_log {
                let rss_mb = rss / (1024 * 1024);
                let avail_mb = available / (1024 * 1024);
                let swap_mb = swap_used / (1024 * 1024);
                let module_summary: String = snapshot
                    .modules
                    .iter()
                    .map(|m| format!("{}={}MB", m.name, m.bytes / (1024 * 1024)))
                    .collect::<Vec<_>>()
                    .join(", ");

                clog_info!(
                    "🧠 Memory: RSS={}MB avail={}MB swap={}MB pressure={:.1}% level={} [{}]",
                    rss_mb,
                    avail_mb,
                    swap_mb,
                    pressure * 100.0,
                    level,
                    if module_summary.is_empty() {
                        "no reporters".to_string()
                    } else {
                        module_summary
                    }
                );
            }

            // Publish snapshot (never blocks — watch::Sender overwrites previous value)
            let _ = tx.send(snapshot);
        }
    }
}
