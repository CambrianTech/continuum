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

/// Poll cadence — every 2 s. The shared [`Daemon`] runner drives this via
/// `tokio::time::interval` (not a sleep loop), so cadence cannot drift under
/// load and a slow tick collapses rather than stacks.
const POLL_INTERVAL: Duration = Duration::from_secs(2);
/// Per-reporter call budget. A reporter exceeding this is treated as a fault.
const REPORTER_TIMEOUT: Duration = Duration::from_millis(100);
/// Consecutive faults before a reporter is quarantined (skipped) to stop a
/// misbehaving reporter from cascading into the monitor.
const QUARANTINE_AFTER: u32 = 3;

// =============================================================================
// Global Memory Gate — subsystems check before expensive allocations
// =============================================================================

/// Global atomic gate: true when pressure >= Critical for 3+ consecutive polls.
/// Any subsystem can check this before allocating large buffers.
static MEMORY_GATE_CLOSED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Global atomic pressure level — updated every 2s by the monitor loop.
/// Any subsystem can read this lock-free to make graduated decisions.
static CURRENT_PRESSURE_LEVEL: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0); // 0=Normal

/// Global atomic free-physical-memory reading (bytes), updated every 2s by the
/// monitor loop from `sysinfo::available_memory()` — the honest "how much can I
/// allocate before the OS OOM-kills me" number. The pressure LEVEL is a ratio and
/// macOS reports it Normal while counting compressible/cached pages as available;
/// a subsystem about to allocate a KNOWN large footprint (e.g. standing up a second
/// llama-server) must size against these real free bytes, not the level, or it gets
/// jetsam-SIGKILLed on a memory-tight machine. 0 until the first monitor poll (then
/// treat as "unknown — don't veto on it"). Lock-free read from anywhere.
static CURRENT_AVAILABLE_BYTES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Check if the memory gate is closed (critical pressure sustained).
/// Subsystems should refuse new allocations when this returns true.
pub fn is_memory_gate_closed() -> bool {
    MEMORY_GATE_CLOSED.load(std::sync::atomic::Ordering::Relaxed)
}

/// The latest free physical memory (bytes) the monitor observed, or `None` before
/// the first poll. This is the number to size a large elective allocation against —
/// unlike [`MemoryPressureMonitor::current_level`], it does not lie when the OS
/// counts compressible/cached pages as available.
/// Free physical memory, derived as `total − used` rather than read from
/// `sysinfo::available_memory()`.
///
/// # Why this exists — one call returns 0 and the other does not (2026-08-19)
///
/// Measured on this M5, same process, same `System`:
///
/// ```text
/// System::new() + refresh_memory()  → total=68719476736  available_memory()=0
/// System::new_all()                 → total=68719476736  available_memory()=0
/// ```
///
/// `available_memory()` returns **zero** here while `total_memory()` and
/// `used_memory()` both return real values. `SystemMonitor::read_memory` has always
/// derived `total − used` and consequently reports correctly (10.5 GB available,
/// 58.2 GB used, live); this monitor called `available_memory()` and therefore
/// published a permanent 0 into `CURRENT_AVAILABLE_BYTES`.
///
/// The consequence was not local. That atomic is the "how much can I allocate before
/// the OS kills me" number the doc above promises, and it read as "nothing is free"
/// (or, once inverted by a consumer, "everything is free") for anyone who trusted it.
/// A bogus `usable_gb=0` is exactly the shape of the #438 downshift incident.
///
/// ONE derivation, used by every reader, so the two can never disagree again.
pub fn available_from(sys: &sysinfo::System) -> u64 {
    sys.total_memory().saturating_sub(sys.used_memory())
}

pub fn current_available_bytes() -> Option<u64> {
    match CURRENT_AVAILABLE_BYTES.load(std::sync::atomic::Ordering::Relaxed) {
        0 => None,
        n => Some(n),
    }
}

/// Force-close the memory gate (for emergency use).
pub fn close_memory_gate() {
    MEMORY_GATE_CLOSED.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// Re-open the memory gate (pressure has eased).
fn open_memory_gate() {
    MEMORY_GATE_CLOSED.store(false, std::sync::atomic::Ordering::Relaxed);
}

// =============================================================================
// Pressure Levels
// =============================================================================

/// Memory pressure severity. Each level implies all lower levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/system/PressureLevel.ts"
)]
#[serde(rename_all = "snake_case")]
pub enum PressureLevel {
    /// < 80% system memory. Normal operation.
    Normal,
    /// 80-90% system memory. Log warnings. Non-critical caches should trim.
    Warning,
    /// 90-95% system memory. Deactivate idle avatar slots. Aggressive cache eviction.
    High,
    /// > 95% system memory. Emergency: stop non-essential subsystems, refuse new allocations.
    Critical,
}

/// Threshold constants for pressure level boundaries.
/// Below FLOOR = Normal (0.0 normalized), above CEILING = Critical (1.0 normalized).
const PRESSURE_FLOOR: f64 = 0.80;
const PRESSURE_CEILING: f64 = 0.95;

impl PressureLevel {
    fn from_pressure(pressure: f64) -> Self {
        // Thresholds are SYSTEM-WIDE memory, not process-only.
        // A 32GB machine with browser + Claude Code + Node typically sits at 70-80%.
        // Old thresholds (60/80/90) kept the system permanently at warning/high,
        // throttling all AI personas even when there was no actual danger.
        if pressure >= 0.95 {
            Self::Critical
        } else if pressure >= 0.90 {
            Self::High
        } else if pressure >= 0.80 {
            Self::Warning
        } else {
            Self::Normal
        }
    }

    /// Normalize raw pressure (used/total) to 0.0-1.0 action range.
    /// 0.0 = at or below floor (no concern), 1.0 = at or above ceiling (emergency).
    /// Linear interpolation between PRESSURE_FLOOR and PRESSURE_CEILING.
    fn normalize(pressure: f64) -> f64 {
        ((pressure - PRESSURE_FLOOR) / (PRESSURE_CEILING - PRESSURE_FLOOR)).clamp(0.0, 1.0)
    }

    fn to_u8(self) -> u8 {
        match self {
            Self::Normal => 0,
            Self::Warning => 1,
            Self::High => 2,
            Self::Critical => 3,
        }
    }

    fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Warning,
            2 => Self::High,
            3 => Self::Critical,
            _ => Self::Normal,
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
#[ts(
    export,
    export_to = "../../../protocol/typescript/system/ModuleMemoryReport.ts"
)]
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
#[ts(
    export,
    export_to = "../../../protocol/typescript/system/MemoryPriority.ts"
)]
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
#[ts(
    export,
    export_to = "../../../protocol/typescript/system/MemoryBudgetSpec.ts"
)]
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
#[ts(
    export,
    export_to = "../../../protocol/typescript/system/MemoryBudgetAllocation.ts"
)]
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
#[ts(
    export,
    export_to = "../../../protocol/typescript/system/MemoryBudgetSnapshot.ts"
)]
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
#[ts(
    export,
    export_to = "../../../protocol/typescript/system/PressureSnapshot.ts"
)]
pub struct PressureSnapshot {
    /// Current pressure level
    pub level: PressureLevel,
    /// Memory pressure ratio (0.0 - 1.0) = used / total
    pub pressure: f64,
    /// Normalized pressure (0.0 - 1.0) mapped to action zone.
    /// 0.0 = at/below 80% (no concern), 1.0 = at/above 95% (emergency).
    pub normalized_pressure: f64,
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
            normalized_pressure: 0.0,
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
/// Each reporter call runs on a blocking thread pool with a 100ms timeout.
/// Panics are caught; reporters are quarantined after 3 consecutive failures.
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

/// One reporter call's classified outcome, carried from the off-lock fan-out
/// phase back to the brief fold-lock phase where the fault counters live. The
/// reporter's name + count are resolved during the fold (so the warning text
/// reflects the post-increment count), keeping this enum data-only.
enum ReporterCallOutcome {
    Report(ModuleMemoryReport),
    /// Reporter body panicked (caught inside `spawn_blocking`).
    Panicked,
    /// `spawn_blocking` itself failed to join (runtime-level, not a body panic).
    JoinError(String),
    /// Reporter exceeded the 100 ms call budget.
    TimedOut,
}

/// Loop-private mutable state, owned by the daemon and mutated only on its own
/// tick. Held behind a brief `parking_lot::Mutex` (never across an await) so the
/// [`Daemon::tick`]`(&self)` contract can reach it — the same lock→compute→drop→
/// async→fold shape every daemon on the base shares. Nothing else contends this
/// lock: subscribers read the published snapshot through the channel, and
/// `add_reporter` hands new reporters in via the mpsc + the separate RwLock view.
struct MemoryTickState {
    sys: sysinfo::System,
    pid: Option<sysinfo::Pid>,
    reporters: Vec<ReporterEntry>,
    reporter_rx: tokio::sync::mpsc::UnboundedReceiver<Arc<dyn MemoryReporter>>,
    prev_level: PressureLevel,
    consecutive_at_level: u32,
    log_counter: u64,
}

/// Independent memory pressure monitoring system.
///
/// Runs on the shared [`Daemon`] runner ([`spawn_daemon`]), polling system
/// memory + registered reporters every [`POLL_INTERVAL`]. Publishes snapshots
/// via the embedded [`DaemonChannel`]. Each tick is isolated by the runner's
/// per-tick `catch_unwind` — a stray panic in one poll loses that poll, never
/// the whole monitor.
///
/// Budget model (RAG-budgeter-inspired):
/// - Each reporter declares priority + min/preferred/max bounds
/// - `budget_snapshot()` computes allocation vs actual for human visibility
/// - Future: automatic flexbox-style allocation algorithm
pub struct MemoryPressureMonitor {
    /// The embedded publish channel — the base's watch + derived gate in one.
    /// Gating lives in the cross-module `MEMORY_GATE_CLOSED` static (sustained-
    /// Critical, read by allocation sites via `is_memory_gate_closed`), so this
    /// channel is [`ungated`](DaemonChannel::ungated): the base does not force
    /// its gate on a daemon whose gate semantics live elsewhere.
    channel: DaemonChannel<PressureSnapshot>,
    /// Atomic RSS for lock-free reads from any thread
    current_rss: AtomicU64,
    /// Atomic pressure (f64 bits) for lock-free reads
    current_pressure: AtomicU64,
    /// Dynamically growable reporter view, read by `budget_snapshot()`.
    /// `add_reporter` pushes here AND sends through the mpsc to the tick state.
    reporters: parking_lot::RwLock<Vec<Arc<dyn MemoryReporter>>>,
    /// Channel to hand new reporters to the tick.
    reporter_tx: tokio::sync::mpsc::UnboundedSender<Arc<dyn MemoryReporter>>,
    /// Loop-private tick state — see [`MemoryTickState`].
    state: parking_lot::Mutex<MemoryTickState>,
}

impl MemoryPressureMonitor {
    /// Start the monitor on its own tokio task.
    ///
    /// Returns a handle for subscribing to pressure changes and registering reporters.
    /// The monitor task runs until the process exits.
    /// Reporters can be added later via `add_reporter()`.
    pub fn start(reporters: Vec<Arc<dyn MemoryReporter>>) -> Arc<Self> {
        let (reporter_tx, reporter_rx) = tokio::sync::mpsc::unbounded_channel();

        let entries: Vec<ReporterEntry> = reporters
            .iter()
            .cloned()
            .map(|r| ReporterEntry {
                reporter: r,
                consecutive_panics: 0,
                disabled: false,
            })
            .collect();

        let monitor = Arc::new(Self {
            channel: DaemonChannel::ungated(PressureSnapshot::default()),
            current_rss: AtomicU64::new(0),
            current_pressure: AtomicU64::new(0),
            reporters: parking_lot::RwLock::new(reporters),
            reporter_tx,
            state: parking_lot::Mutex::new(MemoryTickState {
                sys: sysinfo::System::new(),
                pid: sysinfo::get_current_pid().ok(),
                reporters: entries,
                reporter_rx,
                prev_level: PressureLevel::Normal,
                consecutive_at_level: 0,
                log_counter: 0,
            }),
        });

        clog_info!(
            "🧠 MemoryPressureMonitor started (interval={:?}, reporters={})",
            POLL_INTERVAL,
            monitor.reporters.read().len()
        );

        // The shared runner owns the interval + per-tick catch_unwind. We don't
        // hold the returned handle — subscribers reach the channel through us.
        let _ = spawn_daemon(monitor.clone());
        monitor
    }

    /// Get current pressure level — lock-free, callable from any thread.
    /// Updated every 2s by the monitor loop. Subsystems use this to make
    /// graduated decisions (cache sizes, inference concurrency, render quality).
    pub fn current_level() -> PressureLevel {
        PressureLevel::from_u8(CURRENT_PRESSURE_LEVEL.load(std::sync::atomic::Ordering::Relaxed))
    }

    /// Dynamically add a reporter after startup.
    /// The reporter will be picked up by the monitor loop on its next poll.
    pub fn add_reporter(&self, reporter: Arc<dyn MemoryReporter>) {
        self.reporters.write().push(reporter.clone());
        let _ = self.reporter_tx.send(reporter);
    }

    /// Subscribe to pressure snapshot changes.
    /// The receiver gets notified whenever a new snapshot is published.
    pub fn subscribe(&self) -> watch::Receiver<PressureSnapshot> {
        self.channel.handle().subscribe()
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
        self.channel.snapshot()
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
        let reporters = self.reporters.read();
        let mut consumers = Vec::with_capacity(reporters.len());
        let mut warnings = Vec::new();

        for reporter in reporters.iter() {
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

    /// One poll cycle — the [`Daemon::tick`] body. Structured in the canonical
    /// daemon shape: a brief lock to ingest + plan (drain new reporters, refresh
    /// system memory + process RSS, compute pressure/level/hysteresis, snapshot
    /// the live reporters), the async reporter fan-out OFF the lock, then a brief
    /// lock to fold the fault counters + build the snapshot. The state lock is
    /// never held across an await; `self.channel.publish` and the shed fan-out
    /// happen last, lock-free.
    async fn poll(&self) {
        use sysinfo::ProcessesToUpdate;

        // --- Phase 1: brief lock — ingest + plan. ---
        let (total, available, swap_used, rss, pressure, level, consecutive_at_level, live) = {
            let mut st = self.state.lock();

            // Drain new reporters (added via add_reporter()).
            while let Ok(new_reporter) = st.reporter_rx.try_recv() {
                let name = new_reporter.name();
                st.reporters.push(ReporterEntry {
                    reporter: new_reporter,
                    consecutive_panics: 0,
                    disabled: false,
                });
                clog_info!(
                    "🧠 Memory reporter '{}' registered dynamically (total: {})",
                    name,
                    st.reporters.len()
                );
            }

            // System memory.
            st.sys.refresh_memory();
            let total = st.sys.total_memory();
            // NOT `available_memory()` — it returns 0 on this platform while
            // total/used are correct, which published a permanent zero into the
            // global atomic every consumer budgets against. See `available_from`.
            let available = available_from(&st.sys);
            let used = total.saturating_sub(available);
            let swap_used = st.sys.used_swap();

            // Process RSS.
            let rss = if let Some(p) = st.pid {
                st.sys
                    .refresh_processes(ProcessesToUpdate::Some(&[p]), true);
                st.sys.process(p).map(|proc| proc.memory()).unwrap_or(0)
            } else {
                0
            };

            // Pressure — SYSTEM-WIDE (not just our RSS) because other processes
            // matter to the machine's headroom.
            let pressure = if total > 0 {
                used as f64 / total as f64
            } else {
                0.0
            };
            let level = PressureLevel::from_pressure(pressure);

            // Atomics + cross-module level — lock-free reads from anywhere.
            self.current_rss.store(rss, Ordering::Relaxed);
            self.current_pressure
                .store(pressure.to_bits(), Ordering::Relaxed);
            CURRENT_PRESSURE_LEVEL.store(level.to_u8(), Ordering::Relaxed);
            // Publish the honest free-physical-bytes number too, so a subsystem sizing
            // a large elective allocation can veto against real headroom, not the ratio.
            CURRENT_AVAILABLE_BYTES.store(available, Ordering::Relaxed);

            // Hysteresis.
            if level == st.prev_level {
                st.consecutive_at_level = st.consecutive_at_level.saturating_add(1);
            } else {
                st.consecutive_at_level = 1;
                st.prev_level = level;
            }
            let consecutive_at_level = st.consecutive_at_level;

            // Snapshot the live (non-quarantined) reporters by index so the fold
            // phase can update their fault counters in place. Indices are stable
            // within a tick — only this single task mutates the vec, and ticks
            // never overlap.
            let live: Vec<(usize, Arc<dyn MemoryReporter>)> = st
                .reporters
                .iter()
                .enumerate()
                .filter(|(_, e)| !e.disabled)
                .map(|(i, e)| (i, e.reporter.clone()))
                .collect();

            (
                total,
                available,
                swap_used,
                rss,
                pressure,
                level,
                consecutive_at_level,
                live,
            )
        };

        // --- Phase 2: off-lock report fan-out — each reporter on the blocking
        // pool with a 100 ms budget + panic isolation. `report()` is a sync call
        // that may block, so it runs on `spawn_blocking` (not an inline future),
        // and its panic is caught inside the blocking closure — the right
        // isolation for a sync reporter, distinct from the inline `guarded()`
        // path the hybrid daemon uses. ---
        let mut results: Vec<(usize, ReporterCallOutcome)> = Vec::with_capacity(live.len());
        for (idx, reporter) in live {
            let handle = tokio::task::spawn_blocking(move || {
                std::panic::catch_unwind(AssertUnwindSafe(|| reporter.report()))
            });
            let outcome = match tokio::time::timeout(REPORTER_TIMEOUT, handle).await {
                Ok(Ok(Ok(report))) => ReporterCallOutcome::Report(report),
                Ok(Ok(Err(_panic))) => ReporterCallOutcome::Panicked,
                Ok(Err(join_err)) => ReporterCallOutcome::JoinError(format!("{join_err:?}")),
                Err(_elapsed) => ReporterCallOutcome::TimedOut,
            };
            results.push((idx, outcome));
        }

        // --- Phase 3: brief lock — fold outcomes into counters, decide the gate,
        // build the snapshot, log, and collect shed targets. ---
        let (snapshot, shed_targets) = {
            let mut st = self.state.lock();

            let mut module_reports = Vec::with_capacity(results.len());
            for (idx, outcome) in results {
                let entry = &mut st.reporters[idx];
                let name = entry.reporter.name();
                match outcome {
                    ReporterCallOutcome::Report(report) => {
                        entry.consecutive_panics = 0;
                        module_reports.push(report);
                    }
                    ReporterCallOutcome::Panicked => {
                        entry.consecutive_panics += 1;
                        clog_warn!(
                            "🧠 MemoryReporter '{}' panicked ({}/{})",
                            name,
                            entry.consecutive_panics,
                            QUARANTINE_AFTER
                        );
                        if entry.consecutive_panics >= QUARANTINE_AFTER {
                            clog_warn!(
                                "🧠 MemoryReporter '{}' quarantined after {} panics",
                                name,
                                QUARANTINE_AFTER
                            );
                            entry.disabled = true;
                        }
                    }
                    ReporterCallOutcome::JoinError(e) => {
                        clog_warn!("🧠 MemoryReporter '{}' spawn_blocking failed: {}", name, e);
                    }
                    ReporterCallOutcome::TimedOut => {
                        entry.consecutive_panics += 1;
                        clog_warn!(
                            "🧠 MemoryReporter '{}' timed out (>100ms) ({}/{})",
                            name,
                            entry.consecutive_panics,
                            QUARANTINE_AFTER
                        );
                        if entry.consecutive_panics >= QUARANTINE_AFTER {
                            clog_warn!(
                                "🧠 MemoryReporter '{}' quarantined after {} failures",
                                name,
                                QUARANTINE_AFTER
                            );
                            entry.disabled = true;
                        }
                    }
                }
            }

            // Emergency brake: memory gate. Close on sustained Critical (3+ polls
            // = 6 s). Lives in the cross-module `MEMORY_GATE_CLOSED` static (not
            // the channel gate), because allocation sites read it through
            // `is_memory_gate_closed`.
            if level == PressureLevel::Critical && consecutive_at_level >= 3 {
                if !is_memory_gate_closed() {
                    close_memory_gate();
                    clog_warn!(
                        "🚨 MEMORY GATE CLOSED — pressure critical for {}+ seconds. \
                         Blocking new allocations.",
                        consecutive_at_level as u64 * POLL_INTERVAL.as_secs()
                    );
                }
            } else if level < PressureLevel::Critical && is_memory_gate_closed() {
                open_memory_gate();
                clog_info!("🧠 Memory gate re-opened — pressure eased to {}", level);
            }

            // Decide whether to notify shedders this tick, and collect the live
            // can-shed reporters (post-quarantine) to fire OFF the lock. First
            // notification after 2 polls at a level (hysteresis); at Critical,
            // shed on every poll once sustained.
            let should_shed = match level {
                PressureLevel::Critical => consecutive_at_level >= 2,
                PressureLevel::High => consecutive_at_level == 2,
                PressureLevel::Warning => consecutive_at_level == 2,
                PressureLevel::Normal => false,
            };
            let shed_targets: Vec<Arc<dyn MemoryReporter>> = if should_shed {
                st.reporters
                    .iter()
                    .filter(|e| !e.disabled && e.reporter.can_shed())
                    .map(|e| e.reporter.clone())
                    .collect()
            } else {
                Vec::new()
            };

            // Build the snapshot.
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            let snapshot = PressureSnapshot {
                level,
                pressure,
                normalized_pressure: PressureLevel::normalize(pressure),
                rss_bytes: rss,
                total_bytes: total,
                available_bytes: available,
                swap_used_bytes: swap_used,
                modules: module_reports,
                timestamp_ms: now_ms,
                consecutive_at_level,
            };

            // Periodic logging — every 15 polls (30 s) at Normal, louder higher.
            st.log_counter += 1;
            let should_log = match level {
                PressureLevel::Normal => st.log_counter.is_multiple_of(15),
                PressureLevel::Warning => st.log_counter.is_multiple_of(5),
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

            (snapshot, shed_targets)
        };

        // Publish lock-free. The channel overwrites; readers never block us.
        self.channel.publish(snapshot);

        // Shed OFF the lock — a slow shedder must never stall the state lock (and
        // thus the next tick's ingest). Synchronous + panic-isolated, fire-and-
        // forget, exactly as before — just hoisted out from under the lock.
        for reporter in shed_targets {
            let _ = std::panic::catch_unwind(AssertUnwindSafe(|| {
                reporter.shed_load(level);
            }));
        }
    }
}

#[async_trait]
impl Daemon for MemoryPressureMonitor {
    type Snapshot = PressureSnapshot;

    fn name(&self) -> &'static str {
        "memory-pressure"
    }

    fn cadence(&self) -> Duration {
        POLL_INTERVAL
    }

    fn channel(&self) -> &DaemonChannel<PressureSnapshot> {
        &self.channel
    }

    async fn tick(&self) {
        self.poll().await;
    }
}

// =============================================================================
// PressureBroker integration — MemoryPressureMonitor as a signal-only pool
// =============================================================================

/// Plug `MemoryPressureMonitor` into `PressureBroker` as a `ResourcePool`.
/// **Signal source**, not a holder of evictable bytes —
/// `evict_at_least` returns 0 because the monitor doesn't own any RAM
/// it could free. Concrete RAM consumers (genome cache, recall cache,
/// fixture replay buffers, Bevy GPU residency) register their own
/// `ResourcePool` impls and the broker drives eviction against THOSE.
///
/// Symmetric with [`DiskPressureMonitor`](super::disk_pressure::DiskPressureMonitor):
/// both monitors appear on the broker's surface as `"sys-memory"` and
/// `"disk-root"` tiers. The zero-byte alert is the desired signal:
/// "memory hot AND nobody owns the eviction on this tier."
impl crate::paging::pool::ResourcePool for MemoryPressureMonitor {
    fn tier_name(&self) -> &str {
        "sys-memory"
    }

    fn capacity_bytes(&self) -> u64 {
        self.current().total_bytes
    }

    fn usage_bytes(&self) -> u64 {
        let snap = self.current();
        snap.total_bytes.saturating_sub(snap.available_bytes)
    }

    fn evict_at_least(&self, _want_bytes: u64) -> u64 {
        // Signal-only — RAM eviction happens at concrete consumer
        // pools (genome cache, recall cache, etc.); they register
        // their own `ResourcePool` impls. The broker still emits a
        // `PressureAlert` for this tier when pressure crosses the
        // threshold, so operators see "memory hot AND stuck on the
        // global tier" exactly because the broker found nothing to
        // act on at the system level.
        0
    }

    fn snapshot(&self) -> Vec<crate::paging::pool::ResourcePoolEntry> {
        // Per-consumer detail lives on `PressureSnapshot.modules`
        // (populated by registered `MemoryReporter`s); the broker
        // surface stays a tier-level summary.
        Vec::new()
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
        // Thresholds: Normal < 0.80, Warning 0.80-0.90, High 0.90-0.95, Critical >= 0.95
        assert_eq!(PressureLevel::from_pressure(0.3), PressureLevel::Normal);
        assert_eq!(PressureLevel::from_pressure(0.79), PressureLevel::Normal);
        assert_eq!(PressureLevel::from_pressure(0.80), PressureLevel::Warning);
        assert_eq!(PressureLevel::from_pressure(0.85), PressureLevel::Warning);
        assert_eq!(PressureLevel::from_pressure(0.90), PressureLevel::High);
        assert_eq!(PressureLevel::from_pressure(0.93), PressureLevel::High);
        assert_eq!(PressureLevel::from_pressure(0.95), PressureLevel::Critical);
        assert_eq!(PressureLevel::from_pressure(0.99), PressureLevel::Critical);

        // Normalized pressure: 0.80 → 0.0, 0.95 → 1.0, linear between
        let eps = 1e-9;
        assert!((PressureLevel::normalize(0.50) - 0.0).abs() < eps); // below floor → clamped 0
        assert!((PressureLevel::normalize(0.80) - 0.0).abs() < eps); // at floor → 0.0
        assert!((PressureLevel::normalize(0.875) - 0.5).abs() < eps); // midpoint → 0.5
        assert!((PressureLevel::normalize(0.95) - 1.0).abs() < eps); // at ceiling → 1.0
        assert!((PressureLevel::normalize(0.99) - 1.0).abs() < eps); // above ceiling → clamped 1
    }

    #[test]
    fn test_memory_priority_ordering() {
        // Higher priority = harder to evict = higher allocation weight
        assert!(
            MemoryPriority::Realtime.allocation_weight()
                > MemoryPriority::Interactive.allocation_weight()
        );
        assert!(
            MemoryPriority::Interactive.allocation_weight()
                > MemoryPriority::Background.allocation_weight()
        );
        assert!(
            MemoryPriority::Background.allocation_weight()
                > MemoryPriority::Batch.allocation_weight()
        );
    }

    #[test]
    fn test_memory_priority_pressure_gates() {
        // Lower priority sheds load at lower pressure
        assert!(MemoryPriority::Batch.pressure_gate() < MemoryPriority::Background.pressure_gate());
        assert!(
            MemoryPriority::Background.pressure_gate()
                < MemoryPriority::Interactive.pressure_gate()
        );
        assert!(
            MemoryPriority::Interactive.pressure_gate() < MemoryPriority::Realtime.pressure_gate()
        );
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
