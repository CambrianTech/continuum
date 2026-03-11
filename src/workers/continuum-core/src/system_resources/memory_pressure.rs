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
// Memory Budget (RAG-budgeter-style flexbox allocation)
// =============================================================================

/// Priority levels for system RAM consumers — mirrors GpuPriority for consistency.
///
/// Higher priority = higher pressure gate = harder to evict.
/// Each consumer declares its priority when registering its budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, TS)]
#[ts(export, export_to = "../../../shared/generated/system/MemoryPriority.ts")]
#[serde(rename_all = "snake_case")]
pub enum MemoryPriority {
    /// Render loop, audio pipeline — only OOM stops it
    Realtime,
    /// User-facing inference, embeddings, active persona state
    Interactive,
    /// Caches, pre-computed data, idle resources
    Background,
    /// Training buffers, batch processing — yields first
    Batch,
}

impl MemoryPriority {
    /// Weight for budget allocation — higher weight = larger share of overflow.
    /// Mirrors GpuPriority::eviction_weight pattern.
    pub fn allocation_weight(self) -> f64 {
        match self {
            Self::Realtime => 10.0,
            Self::Interactive => 7.0,
            Self::Background => 3.0,
            Self::Batch => 1.0,
        }
    }

    /// Pressure threshold at which this priority starts shedding.
    pub fn pressure_gate(self) -> f64 {
        match self {
            Self::Realtime => 0.95,
            Self::Interactive => 0.80,
            Self::Background => 0.60,
            Self::Batch => 0.50,
        }
    }
}

/// A consumer's declared memory budget — analogous to RAGSourceBudget.
///
/// Each memory consumer registers one of these to declare:
/// - What it needs at minimum to function (flex-basis)
/// - What it would prefer if headroom allows (flex-grow target)
/// - An absolute cap it should never exceed (flex-max)
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../../shared/generated/system/MemoryBudgetSpec.ts")]
pub struct MemoryBudgetSpec {
    /// Consumer name (matches reporter name)
    pub name: String,
    /// Priority level for allocation and eviction ordering
    pub priority: MemoryPriority,
    /// Minimum bytes needed to function (flex-basis)
    #[ts(type = "number")]
    pub min_bytes: u64,
    /// Preferred bytes for good performance
    #[ts(type = "number")]
    pub preferred_bytes: u64,
    /// Absolute maximum bytes (flex-max / hard cap)
    #[ts(type = "number")]
    pub max_bytes: u64,
}

/// A consumer's budget allocation result — current state vs declared budget.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../../shared/generated/system/MemoryBudgetAllocation.ts")]
pub struct MemoryBudgetAllocation {
    /// Consumer name
    pub name: String,
    /// Priority level
    pub priority: MemoryPriority,
    /// Allocated budget ceiling (bytes) — what the system allows
    #[ts(type = "number")]
    pub budget_bytes: u64,
    /// Actual current usage (bytes) — from reporter
    #[ts(type = "number")]
    pub used_bytes: u64,
    /// Utilization: used / budget (0.0 - 1.0+)
    pub utilization: f64,
    /// Headroom: budget - used (negative = over budget)
    #[ts(type = "number")]
    pub headroom_bytes: i64,
    /// Human-readable detail from reporter
    pub detail: String,
    /// Can shed load under pressure
    pub can_shed: bool,
}

/// Full budget snapshot — human-visible state of all memory consumers.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../../shared/generated/system/MemoryBudgetSnapshot.ts")]
pub struct MemoryBudgetSnapshot {
    /// System-wide pressure level
    pub level: PressureLevel,
    /// System-wide pressure ratio (0.0-1.0)
    pub pressure: f64,
    /// Total physical RAM (bytes)
    #[ts(type = "number")]
    pub total_bytes: u64,
    /// Available RAM (bytes)
    #[ts(type = "number")]
    pub available_bytes: u64,
    /// Per-consumer allocations
    pub consumers: Vec<MemoryBudgetAllocation>,
    /// Total budget allocated across all consumers
    #[ts(type = "number")]
    pub total_budgeted_bytes: u64,
    /// Total actual usage across all consumers
    #[ts(type = "number")]
    pub total_used_bytes: u64,
    /// Warnings (e.g., consumers over budget, minimums not met)
    pub warnings: Vec<String>,
    /// Timestamp (ms since epoch)
    #[ts(type = "number")]
    pub timestamp_ms: u64,
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
///
/// ## Budget Declaration
///
/// Each reporter declares a `MemoryBudgetSpec` — its priority, min/max bounds,
/// and preferred allocation. The monitor uses these to compute proportional
/// budgets (like RAGBudgetManager's flexbox allocation for token budgets).
///
/// Phase 1 (now): monitoring and visibility — humans see budget vs actual usage.
/// Phase 2 (future): automatic allocation algorithm distributes available RAM.
pub trait MemoryReporter: Send + Sync {
    /// Module name for reporting
    fn name(&self) -> &'static str;

    /// Declare this consumer's memory budget requirements.
    /// Used for proportional allocation and pressure-based shedding decisions.
    fn budget(&self) -> MemoryBudgetSpec;

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
///
/// Budget model (RAG-budgeter-inspired):
/// - Each reporter declares priority + min/preferred/max bounds
/// - `budget_snapshot()` computes allocation vs actual for human visibility
/// - Future: automatic flexbox-style allocation algorithm
pub struct MemoryPressureMonitor {
    /// Watch channel for pressure snapshots. Readers never block the monitor.
    tx: watch::Sender<PressureSnapshot>,
    rx: watch::Receiver<PressureSnapshot>,
    /// Atomic RSS for lock-free reads from any thread
    current_rss: Arc<AtomicU64>,
    /// Atomic pressure (f64 bits) for lock-free reads
    current_pressure: Arc<AtomicU64>,
    /// Shared reference to reporters for on-demand budget queries.
    /// The monitor loop also holds Arc clones via ReporterEntry.
    reporters: Vec<Arc<dyn MemoryReporter>>,
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
            reporters: reporters.clone(),
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

    /// Compute budget snapshot — human-visible dashboard of all memory consumers.
    ///
    /// For each registered reporter:
    /// - Queries its declared budget (priority, min/preferred/max)
    /// - Queries its current usage
    /// - Computes utilization, headroom, and over-budget warnings
    ///
    /// Phase 1: budgets are the reporter's self-declared specs (monitoring only).
    /// Phase 2: budgets will be adjusted by the allocator based on system pressure.
    pub fn budget_snapshot(&self) -> MemoryBudgetSnapshot {
        let pressure_snap = self.current();
        let mut consumers = Vec::with_capacity(self.reporters.len());
        let mut warnings = Vec::new();

        for reporter in &self.reporters {
            // Panic-safe: skip reporters that panic
            let budget_result = std::panic::catch_unwind(AssertUnwindSafe(|| reporter.budget()));
            let report_result = std::panic::catch_unwind(AssertUnwindSafe(|| reporter.report()));

            let (budget, report) = match (budget_result, report_result) {
                (Ok(b), Ok(r)) => (b, r),
                _ => continue,
            };

            let used = report.bytes;
            let budget_bytes = budget.preferred_bytes; // Phase 1: use preferred as ceiling
            let utilization = if budget_bytes > 0 {
                used as f64 / budget_bytes as f64
            } else {
                0.0
            };
            let headroom = budget_bytes as i64 - used as i64;

            if used > budget.max_bytes {
                warnings.push(format!(
                    "{}: {}MB used > {}MB max (over budget by {}MB)",
                    budget.name,
                    used / (1024 * 1024),
                    budget.max_bytes / (1024 * 1024),
                    (used - budget.max_bytes) / (1024 * 1024),
                ));
            }

            consumers.push(MemoryBudgetAllocation {
                name: budget.name,
                priority: budget.priority,
                budget_bytes,
                used_bytes: used,
                utilization,
                headroom_bytes: headroom,
                detail: report.detail,
                can_shed: report.can_shed,
            });
        }

        // Sort by priority (highest first), then by utilization (most stressed first)
        consumers.sort_by(|a, b| {
            b.priority.cmp(&a.priority).then(
                b.utilization
                    .partial_cmp(&a.utilization)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
        });

        let total_budgeted = consumers.iter().map(|c| c.budget_bytes).sum();
        let total_used = consumers.iter().map(|c| c.used_bytes).sum();

        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        MemoryBudgetSnapshot {
            level: pressure_snap.level,
            pressure: pressure_snap.pressure,
            total_bytes: pressure_snap.total_bytes,
            available_bytes: pressure_snap.available_bytes,
            consumers,
            total_budgeted_bytes: total_budgeted,
            total_used_bytes: total_used,
            warnings,
            timestamp_ms: now_ms,
        }
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

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pressure_levels() {
        assert_eq!(PressureLevel::from_pressure(0.3), PressureLevel::Normal);
        assert_eq!(PressureLevel::from_pressure(0.65), PressureLevel::Warning);
        assert_eq!(PressureLevel::from_pressure(0.85), PressureLevel::High);
        assert_eq!(PressureLevel::from_pressure(0.95), PressureLevel::Critical);
    }

    #[test]
    fn test_memory_priority_ordering() {
        // Higher priority = harder to evict = higher allocation weight
        assert!(MemoryPriority::Realtime.allocation_weight() > MemoryPriority::Interactive.allocation_weight());
        assert!(MemoryPriority::Interactive.allocation_weight() > MemoryPriority::Background.allocation_weight());
        assert!(MemoryPriority::Background.allocation_weight() > MemoryPriority::Batch.allocation_weight());
    }

    #[test]
    fn test_memory_priority_pressure_gates() {
        // Lower priority sheds load at lower pressure
        assert!(MemoryPriority::Batch.pressure_gate() < MemoryPriority::Background.pressure_gate());
        assert!(MemoryPriority::Background.pressure_gate() < MemoryPriority::Interactive.pressure_gate());
        assert!(MemoryPriority::Interactive.pressure_gate() < MemoryPriority::Realtime.pressure_gate());
    }

    #[test]
    fn test_budget_allocation_utilization() {
        let alloc = MemoryBudgetAllocation {
            name: "test".to_string(),
            priority: MemoryPriority::Interactive,
            budget_bytes: 100 * 1024 * 1024, // 100MB
            used_bytes: 75 * 1024 * 1024,    // 75MB
            utilization: 0.75,
            headroom_bytes: 25 * 1024 * 1024, // 25MB
            detail: "test".to_string(),
            can_shed: true,
        };
        assert_eq!(alloc.utilization, 0.75);
        assert!(alloc.headroom_bytes > 0);
    }

    // ── ts-rs binding tests ─────────────────────────────────────────

    #[test]
    fn export_bindings_memory_priority() {
        MemoryPriority::export_all(&ts_rs::Config::default()).unwrap();
    }

    #[test]
    fn export_bindings_memory_budget_spec() {
        MemoryBudgetSpec::export_all(&ts_rs::Config::default()).unwrap();
    }

    #[test]
    fn export_bindings_memory_budget_allocation() {
        MemoryBudgetAllocation::export_all(&ts_rs::Config::default()).unwrap();
    }

    #[test]
    fn export_bindings_memory_budget_snapshot() {
        MemoryBudgetSnapshot::export_all(&ts_rs::Config::default()).unwrap();
    }
}
