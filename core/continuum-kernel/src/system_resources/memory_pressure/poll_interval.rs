use super::*;

/// Poll cadence — every 2 s. The shared [`Daemon`] runner drives this via
/// `tokio::time::interval` (not a sleep loop), so cadence cannot drift under
/// load and a slow tick collapses rather than stacks.
pub(super) const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// How often the anomaly scan (a full process-table refresh, and on macOS a `ps` child
/// on its own thread) may run while pressure stays at High or above. Card 948c30c2
/// (Joel: "rederiving = outrageously expensive CPU everywhere"): under sustained
/// pressure this ran on EVERY 2 s poll — the whole table refreshed and a process spawned
/// thirty times a minute to re-name a fault that had not changed. The first tick of an
/// episode names it at once; after that the fault is re-read at this cadence, and a
/// drop below High re-arms the immediate read for the next episode.
pub(super) const ANOMALY_SCAN_EVERY: Duration = Duration::from_secs(30);

/// How often a PERSISTING anomaly is re-said in the org room. The scan re-reads every
/// [`ANOMALY_SCAN_EVERY`] (30 s) — right for the probe, far too often for a room. The
/// room hears the first crossing at once, then a reminder at this cadence while the
/// same process stays over the floor.
pub(super) const ANOMALY_SAY_EVERY: Duration = Duration::from_secs(30 * 60);

/// PURE: should this anomaly be said in the org room now? Yes the first time a process
/// is named (`last_said` none), then once per [`ANOMALY_SAY_EVERY`].
pub(super) fn anomaly_line_due(
    last_said: Option<std::time::Instant>,
    now: std::time::Instant,
) -> bool {
    last_said.is_none_or(|at| now.saturating_duration_since(at) >= ANOMALY_SAY_EVERY)
}

/// Last time each named process was said, so a long episode is one line plus reminders.
pub(super) static ANOMALY_SAID: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<String, std::time::Instant>>,
> = std::sync::LazyLock::new(Default::default);

/// How the monitor reaches the org room. The room belongs to the grid module, which is above
/// this kernel, so the grid registers its voice at boot (the way owners register what they
/// hold with the disk reporters) instead of the monitor naming the grid.
pub type OrgRoomVoice = fn(String) -> futures::future::BoxFuture<'static, ()>;

static ORG_ROOM_VOICE: std::sync::OnceLock<OrgRoomVoice> = std::sync::OnceLock::new();

/// Register the org-room voice. First registration wins: one voice per process.
pub fn register_org_room_voice(voice: OrgRoomVoice) {
    let _ = ORG_ROOM_VOICE.set(voice); // set: a second registration is the same voice, ignored
}

/// SAY THE ANOMALY WHERE A PERSON OR A PEER WILL SEE IT. `memory.pressure.anomaly` was
/// written after fseventsd held 26.6 GB (2026-09-08) and NOTHING read it: the probe file
/// rotates within the hour, so fseventsd grew to 14 GB again on 2026-09-24 and the only
/// record was a line no one saw. A root-owned process is not ours to kill — the remedy
/// needs a person or a peer — so the substrate's job is to TELL someone, once, promptly.
pub(super) fn say_anomaly(name: &str, pid: u32, rss_bytes: u64, total_bytes: u64) {
    let now = std::time::Instant::now();
    {
        let mut said = ANOMALY_SAID.lock().unwrap_or_else(|e| e.into_inner()); // unwrap_or_else: a poisoned map is still the last-said times; recover it
        if !anomaly_line_due(said.get(name).copied(), now) {
            return;
        }
        said.insert(name.to_string(), now);
    }
    let gb = |b: u64| b / (1024 * 1024 * 1024);
    let remedy = if name.contains("fseventsd") {
        " fseventsd is root-owned: `sudo killall fseventsd` restarts it (it respawns empty)."
    } else {
        ""
    };
    let line = format!(
        "[memory] {name} (pid {pid}) holds {} GB of this node's {} GB — over a quarter of physical memory, and not the model server. Left alone it degrades every continuum function and can take the machine down.{remedy}",
        gb(rss_bytes),
        gb(total_bytes),
    );
    let Some(voice) = ORG_ROOM_VOICE.get() else {
        continuum_base::probe!(
            class = "memory.pressure.anomaly_not_said",
            process = %name,
            "no org-room voice registered — the anomaly stays in the probe only"
        );
        return;
    };
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => {
            handle.spawn(voice(line));
        }
        Err(_) => continuum_base::probe!(
            class = "memory.pressure.anomaly_not_said",
            process = %name,
            "no async runtime on the monitor thread — the anomaly stays in the probe only"
        ),
    }
}

/// PURE: is the anomaly scan due on this tick? Due on the first tick at High or above
/// (`last_scan` none), and thereafter once per [`ANOMALY_SCAN_EVERY`]; never below High.
pub(super) fn anomaly_scan_due(
    level: PressureLevel,
    last_scan: Option<Duration>,
    since_start: Duration,
) -> bool {
    if level.to_u8() < PressureLevel::High.to_u8() {
        return false;
    }
    match last_scan {
        None => true,
        Some(at) => since_start.saturating_sub(at) >= ANOMALY_SCAN_EVERY,
    }
}

/// Per-reporter call budget. A reporter exceeding this is treated as a fault.
pub const REPORTER_TIMEOUT: Duration = Duration::from_millis(100);

/// Consecutive faults before a reporter is quarantined (skipped) to stop a
/// misbehaving reporter from cascading into the monitor.
pub(super) const QUARANTINE_AFTER: u32 = 3;

// =============================================================================
// Global Memory Gate — subsystems check before expensive allocations
// =============================================================================
/// Global atomic gate: true when pressure >= Critical for 3+ consecutive polls.
/// Any subsystem can check this before allocating large buffers.
pub(super) static MEMORY_GATE_CLOSED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Global atomic pressure level — updated every 2s by the monitor loop.
/// Any subsystem can read this lock-free to make graduated decisions.
pub(super) static CURRENT_PRESSURE_LEVEL: std::sync::atomic::AtomicU8 =
    std::sync::atomic::AtomicU8::new(0); // 0=Normal

/// Latest complete memory sampling attempt, published by the monitor only.
static CURRENT_MEMORY: std::sync::LazyLock<
    arc_swap::ArcSwap<continuum_resource_observation::Measurement>,
> = std::sync::LazyLock::new(|| {
    arc_swap::ArcSwap::from_pointee(continuum_resource_observation::Measurement::Unavailable(
        continuum_resource_observation::UnavailableReason::NotSampled,
    ))
});

/// One coherent attempt from the shared monitor. Measured exhaustion is a valid
/// zero; it must not be confused with a monitor that has never published.
pub fn current_memory_measurement() -> continuum_resource_observation::Measurement {
    **CURRENT_MEMORY.load()
}

pub(super) fn publish_memory_measurement(total: u64, used: u64) {
    use continuum_resource_observation::{ByteMeasurement, Measurement, UnavailableReason};
    let measurement = match total.checked_sub(used) {
        Some(available) if total > 0 => match ByteMeasurement::new(total, available) {
            Ok(bytes) => Measurement::Measured(bytes),
            Err(_) => Measurement::Unavailable(UnavailableReason::InvalidMeasurement),
        },
        _ => Measurement::Unavailable(UnavailableReason::InvalidMeasurement),
    };
    CURRENT_MEMORY.store(std::sync::Arc::new(measurement));
}

/// Check if the memory gate is closed (critical pressure sustained).
/// Subsystems should refuse new allocations when this returns true.
pub fn is_memory_gate_closed() -> bool {
    MEMORY_GATE_CLOSED.load(std::sync::atomic::Ordering::Relaxed)
}

/// Pressure telemetry derives availability from the same OS total/used pair.
/// Allocation readers consume current_memory_measurement instead of this scalar.
pub fn available_from(sys: &sysinfo::System) -> u64 {
    sys.total_memory().saturating_sub(sys.used_memory())
}

pub fn current_available_bytes() -> Option<u64> {
    match current_memory_measurement() {
        continuum_resource_observation::Measurement::Measured(bytes) => Some(bytes.available()),
        continuum_resource_observation::Measurement::Unavailable(_) => None,
    }
}

/// Force-close the memory gate (for emergency use).
pub fn close_memory_gate() {
    MEMORY_GATE_CLOSED.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// Re-open the memory gate (pressure has eased).
pub(super) fn open_memory_gate() {
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
pub(super) const PRESSURE_FLOOR: f64 = 0.80;

pub(super) const PRESSURE_CEILING: f64 = 0.95;

impl PressureLevel {
    pub(super) fn from_pressure(pressure: f64) -> Self {
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
    /// Swap is its own pressure axis. macOS counts swapped-out and compressible pages
    /// as "available", so `from_pressure` read NORMAL on 2026-09-08 while swap sat at
    /// 40 of 41 GB and the model server paged weights from disk at 1–9 t/s. Half the
    /// swap file in use is High whatever the page counters say; nine tenths is Critical.
    pub fn from_swap(swap_pct: f64) -> Self {
        if swap_pct >= 0.90 {
            Self::Critical
        } else if swap_pct >= 0.50 {
            Self::High
        } else if swap_pct >= 0.25 {
            Self::Warning
        } else {
            Self::Normal
        }
    }

    /// The swap axis from what the node is DOING with swap (card f26d6568). A file nine
    /// tenths full is Critical whatever the rate (nowhere left to page). Below that the
    /// smoothed swap-in + swap-out rate decides, so inert pages a build left behind an
    /// hour ago read Normal while real paging reads High. With no rate (no counters on
    /// this platform, or the first poll) occupancy decides, as [`Self::from_swap`].
    pub fn from_swap_activity(swap_pct: f64, bytes_per_sec: Option<f64>) -> Self {
        use crate::system_resources::swap_activity::{
            SWAP_FULL_FRACTION, SWAP_HIGH_BYTES_PER_SEC, SWAP_WARNING_BYTES_PER_SEC,
        };
        if swap_pct >= SWAP_FULL_FRACTION {
            return Self::Critical;
        }
        match bytes_per_sec {
            None => Self::from_swap(swap_pct),
            Some(rate) if rate >= SWAP_HIGH_BYTES_PER_SEC => Self::High,
            Some(rate) if rate >= SWAP_WARNING_BYTES_PER_SEC => Self::Warning,
            Some(_) => Self::Normal,
        }
    }

    /// The worse of two readings — the level the node acts on.
    pub fn worse(a: Self, b: Self) -> Self {
        if b.to_u8() > a.to_u8() {
            b
        } else {
            a
        }
    }

    pub(super) fn normalize(pressure: f64) -> f64 {
        ((pressure - PRESSURE_FLOOR) / (PRESSURE_CEILING - PRESSURE_FLOOR)).clamp(0.0, 1.0)
    }

    pub(super) fn to_u8(self) -> u8 {
        match self {
            Self::Normal => 0,
            Self::Warning => 1,
            Self::High => 2,
            Self::Critical => 3,
        }
    }

    pub(super) fn from_u8(v: u8) -> Self {
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
pub(super) struct ReporterEntry {
    pub(super) reporter: Arc<dyn MemoryReporter>,
    pub(super) consecutive_panics: u32,
    /// Disabled after 3 consecutive panics — quarantined to prevent cascade
    pub(super) disabled: bool,
}

/// One reporter call's classified outcome, carried from the off-lock fan-out
/// phase back to the brief fold-lock phase where the fault counters live. The
/// reporter's name + count are resolved during the fold (so the warning text
/// reflects the post-increment count), keeping this enum data-only.
pub(super) enum ReporterCallOutcome {
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
pub(super) struct MemoryTickState {
    pub(super) sys: sysinfo::System,
    pub(super) pid: Option<sysinfo::Pid>,
    pub(super) reporters: Vec<ReporterEntry>,
    pub(super) reporter_rx: tokio::sync::mpsc::UnboundedReceiver<Arc<dyn MemoryReporter>>,
    pub(super) prev_level: PressureLevel,
    pub(super) consecutive_at_level: u32,
    pub(super) log_counter: u64,
    /// When the anomaly scan last ran (monotonic, since the monitor started); `None`
    /// re-arms an immediate scan for the next High episode. See [`anomaly_scan_due`].
    pub(super) anomaly_scanned_at: Option<Duration>,
    /// The monitor's own clock origin for `anomaly_scanned_at`.
    pub(super) started: std::time::Instant,
    /// The smoothed swap traffic the swap axis reads (card f26d6568).
    pub(super) swap_activity: crate::system_resources::swap_activity::SwapActivity,
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
    pub(super) channel: DaemonChannel<PressureSnapshot>,
    /// Atomic RSS for lock-free reads from any thread
    pub(super) current_rss: AtomicU64,
    /// Atomic pressure (f64 bits) for lock-free reads
    pub(super) current_pressure: AtomicU64,
    /// Dynamically growable reporter view, read by `budget_snapshot()`.
    /// `add_reporter` pushes here AND sends through the mpsc to the tick state.
    pub(super) reporters: parking_lot::RwLock<Vec<Arc<dyn MemoryReporter>>>,
    /// Channel to hand new reporters to the tick.
    pub(super) reporter_tx: tokio::sync::mpsc::UnboundedSender<Arc<dyn MemoryReporter>>,
    /// Loop-private tick state — see [`MemoryTickState`].
    pub(super) state: parking_lot::Mutex<MemoryTickState>,
}
