//! DiskPressureMonitor — Independent, non-blocking disk-pressure surveillance.
//!
//! Mirror of [`MemoryPressureMonitor`](super::memory_pressure::MemoryPressureMonitor)
//! for disk. Own tokio task, own interval, watch channel for snapshot
//! publication, atomic gate for lock-free hot-path checks, per-source
//! reporters via spawn_blocking + 100 ms timeout, quarantine after 3
//! consecutive failures. Crash-isolated: panics anywhere inside the
//! loop or in a reporter are caught and counted, never propagated.
//!
//! Same shape because the substrate's RTOS doctrine is one shape per
//! concurrent concern — see
//! [`docs/architecture/CONCURRENCY-STYLE-GUIDE.md`](../../../../../docs/architecture/CONCURRENCY-STYLE-GUIDE.md).
//!
//! ## Why disk pressure as a substrate concern
//!
//! Joel 2026-06-08: the substrate eats disk. Cargo target dirs, model
//! caches, fixture archives, probe JSONL spool, persona home stores,
//! airc worktrees. None of these are individually unbounded; together,
//! on an Intel MacBook Pro with no swap room, they crash the machine.
//! The disk-guard slop that triggered the CONCURRENCY-STYLE-GUIDE was
//! a synchronous, env-tuned, main-thread `runtime/disk_guard.rs`. This
//! is the right shape: own task, watch channel, broker-arbitrated.
//!
//! ## Architecture
//!
//! ```text
//! ┌──────────────────────────────────────────────────┐
//! │          DiskPressureMonitor (own task)          │
//! │                                                  │
//! │   poll_interval: 30 s                            │
//! │                                                  │
//! │   ┌────────────────┐  ┌─────────────────────┐    │
//! │   │ sysinfo Disks  │  │ Per-path reporters  │    │
//! │   │  (root mount)  │  │  (cargo-target,     │    │
//! │   │ total / avail  │  │   continuum-cache,  │    │
//! │   └────────────────┘  │   model registry…)  │    │
//! │           │           └─────────────────────┘    │
//! │           ▼                     │                │
//! │   ┌─────────────────────────────┴───────────┐    │
//! │   │    DiskPressureSnapshot (atomic)        │    │
//! │   │    - level: Normal/Warning/High/        │    │
//! │   │      Critical                           │    │
//! │   │    - total_bytes / available_bytes      │    │
//! │   │    - pressure: 0.0 - 1.0                │    │
//! │   │    - per_path breakdown                 │    │
//! │   └─────────────────────────────────────────┘    │
//! │                    │                             │
//! │                    ▼                             │
//! │   tokio::sync::watch → subscribers read freely   │
//! └──────────────────────────────────────────────────┘
//! ```
//!
//! ## Cadence
//!
//! 30 s — disk fills slowly. The CONCURRENCY-STYLE-GUIDE's cadence
//! ladder says "lean slower when in doubt; the cost of a missed-by-a-
//! second spike is one eviction cycle." Memory polls every 2 s
//! because RSS can spike in milliseconds; disk doesn't.
//!
//! ## Future
//!
//! - Register as a `ResourcePool` with the `PressureBroker` so disk
//!   pressure participates in cross-resource tier-relief (task #88).
//! - Implement `ServiceModule` so the Runtime owns the lifecycle and
//!   `ready_edge()` works through the canonical seam (slice A.5).
//! - Sweep policy under pressure (Critical → delete oldest cargo
//!   incremental, oldest probe JSONL files, etc.).

use async_trait::async_trait;
use serde::Serialize;
use std::panic::AssertUnwindSafe;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;
use ts_rs::TS;

use crate::runtime::{spawn_daemon, Daemon, DaemonChannel};
use crate::{clog_info, clog_warn};

// =============================================================================
// Global Disk Gate — subsystems check before bulk writes
// =============================================================================

/// Global atomic gate: true when disk pressure is Critical and sustained.
/// Bulk-write subsystems (model download, fixture archive, probe JSONL
/// spool) should check this before allocating large chunks. Same shape
/// as `is_memory_gate_closed`.
static DISK_GATE_CLOSED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Global atomic level — updated every poll. Lock-free reads anywhere.
static CURRENT_DISK_LEVEL: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// The last published snapshot, for the command surface (`system/resources`). A
/// brief `RwLock` written once per 30 s poll and read on demand — never across an
/// await.
static CURRENT_DISK_SNAPSHOT: parking_lot::RwLock<Option<DiskPressureSnapshot>> =
    parking_lot::RwLock::new(None);

/// Check if the disk gate is closed (critical pressure sustained).
/// Subsystems should refuse new bulk writes when this returns true.
pub fn is_disk_gate_closed() -> bool {
    DISK_GATE_CLOSED.load(Ordering::Relaxed)
}

/// Force-close the disk gate (emergency use).
pub fn close_disk_gate() {
    DISK_GATE_CLOSED.store(true, Ordering::Relaxed);
}

fn open_disk_gate() {
    DISK_GATE_CLOSED.store(false, Ordering::Relaxed);
}

// =============================================================================
// Pressure levels — mirror memory exactly so operators don't learn two ladders
// =============================================================================

/// Disk pressure severity. Same tier boundaries as memory pressure
/// (`PressureLevel` in `memory_pressure.rs`) — substrate-wide one ladder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/system/DiskPressureLevel.ts"
)]
#[serde(rename_all = "snake_case")]
pub enum DiskPressureLevel {
    /// < 80 % used. Normal operation.
    Normal,
    /// 80 – 90 %. Log warnings; consider sweeping stale fixtures.
    Warning,
    /// 90 – 95 %. Refuse non-essential downloads; broker may evict cold tiers.
    High,
    /// > 95 %. Emergency: refuse new bulk writes; broker evicts aggressively.
    Critical,
    /// The monitor CANNOT SEE: no mounted volume was found at all. A typed absence
    /// — never derived from a zero-sized reading. Until 2026-09-19 the no-volume
    /// branch announced "UNKNOWN" in a probe and then handed on `(0, 0)`, so the
    /// number said Normal while the words said unknown (Cormac + a027688a's review
    /// of #4185). Ordered last so `level >= High` treats it as hot: an unseen disk
    /// is not a disk with room.
    Unknown,
}

impl DiskPressureLevel {
    fn from_pressure(p: f64) -> Self {
        if p >= 0.95 {
            Self::Critical
        } else if p >= 0.90 {
            Self::High
        } else if p >= 0.80 {
            Self::Warning
        } else {
            Self::Normal
        }
    }

    fn to_u8(self) -> u8 {
        match self {
            Self::Normal => 0,
            Self::Warning => 1,
            Self::High => 2,
            Self::Critical => 3,
            Self::Unknown => 4,
        }
    }
}

impl std::fmt::Display for DiskPressureLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Normal => write!(f, "normal"),
            Self::Warning => write!(f, "warning"),
            Self::High => write!(f, "high"),
            Self::Critical => write!(f, "critical"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

// =============================================================================
// Per-path report
// =============================================================================

/// One path's self-reported disk usage.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/system/DiskPathReport.ts"
)]
pub struct DiskPathReport {
    /// Identifier (e.g., "cargo-target", "continuum-cache", "model-registry").
    pub name: String,
    /// Filesystem path being reported on.
    pub path: PathBuf,
    /// Bytes currently consumed at this path (recursive).
    #[ts(type = "number")]
    pub bytes: u64,
    /// Human-readable detail (e.g., "rustc incrementals: 4.2 GB; deps: 6.1 GB").
    pub detail: String,
}

/// Trait for subsystems that can report what they're consuming on disk.
/// Implementations MUST be fast (< 100 ms) and MUST NOT block on shared
/// state held by the rest of the system. Same contract as
/// [`MemoryReporter`](super::memory_pressure::MemoryReporter) — one
/// shape, two domains.
pub trait DiskReporter: Send + Sync {
    /// Stable identifier — appears in probes and dashboards.
    fn name(&self) -> &'static str;

    /// Report current usage. Called from `spawn_blocking` with a 100 ms
    /// timeout. Panics are caught; three consecutive panics quarantine
    /// the reporter.
    fn report(&self) -> DiskPathReport;
}

// =============================================================================
// Snapshot — published via watch channel
// =============================================================================

#[derive(Debug, Clone, Serialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/system/DiskPressureSnapshot.ts"
)]
pub struct DiskPressureSnapshot {
    /// Tier — derived from `pressure`.
    pub level: DiskPressureLevel,
    /// Pressure ratio (0.0 – 1.0) = used / total on the root filesystem.
    pub pressure: f64,
    /// Total bytes on the root filesystem.
    #[ts(type = "number")]
    pub total_bytes: u64,
    /// Available bytes on the root filesystem.
    #[ts(type = "number")]
    pub available_bytes: u64,
    /// Used bytes on the root filesystem.
    #[ts(type = "number")]
    pub used_bytes: u64,
    /// Per-path breakdown from registered reporters.
    pub paths: Vec<DiskPathReport>,
    /// Unix milliseconds when this snapshot was published.
    #[ts(type = "number")]
    pub timestamp_ms: u64,
    /// Consecutive polls at this level (hysteresis input).
    pub consecutive_at_level: u32,
    /// WHICH volume the numbers describe: `home` (the volume holding the substrate's
    /// home — the normal case), `hottest_volume` (no volume holds the home; the numbers
    /// are the most-pressured mounted volume's — a real reading, conservatively
    /// chosen, named as a fallback), or `none` (no mounted volume at all; level is
    /// `Unknown` and the byte fields are meaningless).
    pub volume_source: String,
}

impl Default for DiskPressureSnapshot {
    fn default() -> Self {
        Self {
            level: DiskPressureLevel::Normal,
            pressure: 0.0,
            total_bytes: 0,
            available_bytes: 0,
            used_bytes: 0,
            paths: Vec::new(),
            timestamp_ms: 0,
            consecutive_at_level: 0,
            volume_source: "none".to_string(),
        }
    }
}

// =============================================================================
// Monitor
// =============================================================================

/// Internal entry tracking a reporter's fault count.
struct ReporterEntry {
    reporter: Arc<dyn DiskReporter>,
    consecutive_panics: u32,
    disabled: bool,
}

/// One reporter call's classified outcome, carried from the off-lock fan-out
/// phase back to the brief fold-lock phase where the fault counters live. The
/// reporter's name + count are resolved during the fold (so the warning text
/// always reflects the post-increment count), keeping this enum data-only.
enum ReporterCallOutcome {
    Report(DiskPathReport),
    /// Reporter body panicked (caught inside `spawn_blocking`).
    Panicked,
    /// `spawn_blocking` itself failed to join (runtime-level, not a body panic).
    JoinError(String),
    /// Reporter exceeded the 100 ms call budget.
    TimedOut,
}

/// Loop-private mutable state, owned by the daemon and mutated only on its own
/// tick. Held behind a brief `parking_lot::Mutex` (never across an await) so the
/// `Daemon::tick(&self)` contract can reach it — the same lock→compute→drop→
/// async→fold shape every daemon on the base shares. Nothing else contends this
/// lock: subscribers read the published snapshot through the channel, and
/// `add_reporter` hands new reporters in via the mpsc, not by locking here.
struct DiskTickState {
    disks: sysinfo::Disks,
    reporters: Vec<ReporterEntry>,
    reporter_rx: tokio::sync::mpsc::UnboundedReceiver<Arc<dyn DiskReporter>>,
    prev_level: DiskPressureLevel,
    consecutive_at_level: u32,
    log_counter: u64,
    first_snapshot_published: bool,
}

/// Independent disk-pressure monitoring system.
///
/// Construct via [`DiskPressureMonitor::start`]; the constructor spawns the
/// monitor on the shared [`Daemon`] runner ([`spawn_daemon`]) and returns an
/// `Arc<Self>` holding the public API. The task runs until the process exits,
/// with each tick isolated by the runner's per-tick `catch_unwind` — a stray
/// panic in one poll loses that poll, never the whole monitor.
pub struct DiskPressureMonitor {
    /// The embedded publish channel — the base's watch + derived gate in one.
    /// Gating lives in the global `DISK_GATE_CLOSED` static (sustained-Critical,
    /// read cross-module by bulk-write subsystems), so this channel is
    /// [`ungated`](DaemonChannel::ungated) — the base does not force its gate on
    /// a daemon whose gate semantics live elsewhere.
    channel: DaemonChannel<DiskPressureSnapshot>,
    /// Ready edge — flips `false → true` exactly once when the first
    /// snapshot is published, same shape as
    /// [`ServiceModule::ready_edge`](crate::runtime::ServiceModule::ready_edge).
    /// Callers (main.rs, future ServiceModule consumers) await the
    /// transition instead of polling.
    ready_tx: watch::Sender<bool>,
    /// Atomic pressure (f64 bits) — lock-free reads from any thread.
    current_pressure: Arc<AtomicU64>,
    /// Dynamically growable reporter list. New reporters get picked up
    /// by the monitor loop on its next poll.
    reporters: parking_lot::RwLock<Vec<Arc<dyn DiskReporter>>>,
    /// Channel for sending new reporters to the loop task.
    reporter_tx: tokio::sync::mpsc::UnboundedSender<Arc<dyn DiskReporter>>,
    /// Loop-private tick state — see [`DiskTickState`].
    state: parking_lot::Mutex<DiskTickState>,
}

/// Poll interval — every 30 s. Disk pressure changes slowly; faster
/// polls burn CPU on a value that rarely changes. Per the
/// CONCURRENCY-STYLE-GUIDE cadence ladder: lean slower in doubt.
const POLL_INTERVAL: Duration = Duration::from_secs(30);

/// Quarantine threshold — matches the broker / memory monitor.
const QUARANTINE_AFTER: u32 = 3;

/// Reporter call budget — 100 ms hard ceiling per call.
const REPORTER_TIMEOUT: Duration = Duration::from_millis(100);

/// THE VOLUME WE MEASURE IS THE ONE THAT HOLDS THE SUBSTRATE'S HOME — resolved
/// through the drive adapter ([`crate::capacity::system_profile::drive_holding`]),
/// never named. Until 2026-09-18 this file held `const ROOT_MOUNT = "/"` and
/// matched every mount point against it by string equality. On Windows no mount is
/// `/` (`C:\`), so the lookup found nothing, read `(0, 0)`, computed pressure 0.0,
/// and the level stayed `Normal` on every Windows host for as long as the monitor
/// existed: the 5090 filled its 1.9 TB volume to 0 bytes free with the broker never
/// asked to relieve a byte, the cargo-target pool sitting "within budget", and the
/// core's own checkpoints dying with os error 112. Two of this codebase's own laws in
/// one constant: a platform that chooses differently, silently, and an absence
/// rendered as a positive fact — `(0, 0)` reads as "fine". Now: no drive holds the
/// home = `disk.pressure.unmeasured`, said on every blind poll, never a zero.

/// Which volume a reading describes — see [`DiskPressureSnapshot::volume_source`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VolumeSource {
    Home,
    HottestVolume,
    None,
}

impl VolumeSource {
    fn label(self) -> &'static str {
        match self {
            Self::Home => "home",
            Self::HottestVolume => "hottest_volume",
            Self::None => "none",
        }
    }
}

/// One poll's volume reading, PURE over the mount list so the absence cases are
/// pinned by a test without sysinfo.
#[derive(Debug, Clone, PartialEq, Eq)]
struct VolumeReading {
    total: u64,
    available: u64,
    source: VolumeSource,
    /// `Some(Unknown)` only when there is no volume to measure at all.
    level_override: Option<DiskPressureLevel>,
}

/// THE reading for this poll: the volume holding the home; else the hottest mounted
/// volume (a real number, conservatively chosen, named as a fallback); else a typed
/// Unknown. Never `(0, 0)` presented as a measurement — that read as pressure 0.0 =
/// Normal, which is the lie #4185 was written to end and then re-committed in its
/// own no-match arm (2026-09-19).
fn volume_reading(
    drives: &[crate::capacity::system_profile::DriveInfo],
    home: &std::path::Path,
) -> VolumeReading {
    if let Some(d) = crate::capacity::system_profile::drive_holding(drives, home)
        .filter(|d| d.total_bytes > 0)
    {
        return VolumeReading {
            total: d.total_bytes,
            available: d.available_bytes,
            source: VolumeSource::Home,
            level_override: None,
        };
    }
    let hottest = drives
        .iter()
        .filter(|d| d.total_bytes > 0)
        .max_by(|a, b| {
            let pa = 1.0 - a.available_bytes as f64 / a.total_bytes as f64;
            let pb = 1.0 - b.available_bytes as f64 / b.total_bytes as f64;
            pa.partial_cmp(&pb).unwrap_or(std::cmp::Ordering::Equal) // unwrap_or: NaN is impossible (total_bytes > 0 filtered above); Equal keeps the order
        });
    match hottest {
        Some(d) => VolumeReading {
            total: d.total_bytes,
            available: d.available_bytes,
            source: VolumeSource::HottestVolume,
            level_override: None,
        },
        None => VolumeReading {
            total: 0,
            available: 0,
            source: VolumeSource::None,
            level_override: Some(DiskPressureLevel::Unknown),
        },
    }
}

impl DiskPressureMonitor {
    /// Spawn the monitor on the shared [`Daemon`] runner. Returns the handle.
    pub fn start(reporters: Vec<Arc<dyn DiskReporter>>) -> Arc<Self> {
        let (ready_tx, _ready_rx) = watch::channel(false);
        let current_pressure = Arc::new(AtomicU64::new(0));
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
            channel: DaemonChannel::ungated(DiskPressureSnapshot::default()),
            ready_tx,
            current_pressure,
            reporters: parking_lot::RwLock::new(reporters),
            reporter_tx,
            state: parking_lot::Mutex::new(DiskTickState {
                disks: sysinfo::Disks::new_with_refreshed_list(),
                reporters: entries,
                reporter_rx,
                prev_level: DiskPressureLevel::Normal,
                consecutive_at_level: 0,
                log_counter: 0,
                first_snapshot_published: false,
            }),
        });

        clog_info!(
            "💾 DiskPressureMonitor started (interval={:?}, reporters={})",
            POLL_INTERVAL,
            monitor.reporters.read().len()
        );

        // The shared runner owns the interval + per-tick catch_unwind. We don't
        // hold the returned handle — subscribers reach the channel through us.
        let _ = spawn_daemon(monitor.clone());
        monitor
    }

    /// The last published snapshot, process-wide — what `system/resources` shows
    /// beside cpu/memory/gpu. `None` before the first poll: "not measured yet", never
    /// a zero-sized volume.
    pub fn current_snapshot() -> Option<DiskPressureSnapshot> {
        CURRENT_DISK_SNAPSHOT.read().clone()
    }

    /// Lock-free current level. Updated every poll.
    pub fn current_level() -> DiskPressureLevel {
        match CURRENT_DISK_LEVEL.load(Ordering::Relaxed) {
            1 => DiskPressureLevel::Warning,
            2 => DiskPressureLevel::High,
            3 => DiskPressureLevel::Critical,
            4 => DiskPressureLevel::Unknown,
            _ => DiskPressureLevel::Normal,
        }
    }

    /// Add a reporter after startup. The monitor loop picks it up on the
    /// next poll. Same shape as `MemoryPressureMonitor::add_reporter`.
    pub fn add_reporter(&self, reporter: Arc<dyn DiskReporter>) {
        self.reporters.write().push(reporter.clone());
        let _ = self.reporter_tx.send(reporter);
    }

    /// Subscribe to snapshot changes. The receiver gets notified
    /// whenever a new snapshot is published.
    pub fn subscribe(&self) -> watch::Receiver<DiskPressureSnapshot> {
        self.channel.handle().subscribe()
    }

    /// Subscribe to the ready edge — flips `false → true` exactly once
    /// when the first snapshot is published. Same shape as
    /// [`ServiceModule::ready_edge`](crate::runtime::ServiceModule::ready_edge).
    pub fn ready_edge(&self) -> watch::Receiver<bool> {
        self.ready_tx.subscribe()
    }

    /// Latest pressure ratio (0.0 – 1.0) — lock-free.
    pub fn pressure(&self) -> f64 {
        f64::from_bits(self.current_pressure.load(Ordering::Relaxed))
    }

    /// Latest snapshot (cheap clone from the watch channel).
    pub fn current(&self) -> DiskPressureSnapshot {
        self.channel.snapshot()
    }

    /// One poll cycle — the [`Daemon::tick`] body. Structured in the canonical
    /// daemon shape: a brief lock to ingest + plan (drain new reporters, refresh
    /// disks, compute pressure/level/hysteresis, snapshot the live reporters),
    /// the async reporter fan-out OFF the lock, then a brief lock to fold the
    /// fault counters + build the snapshot. The state lock is never held across
    /// an await; `self.channel.publish` happens last, lock-free.
    async fn poll(&self) {
        // --- Phase 1: brief lock — ingest + plan. ---
        let (total, available, used, pressure, level, consecutive_at_level, live, volume_source) = {
            let mut st = self.state.lock();

            // Drain dynamically-added reporters.
            while let Ok(new_reporter) = st.reporter_rx.try_recv() {
                let name = new_reporter.name();
                st.reporters.push(ReporterEntry {
                    reporter: new_reporter,
                    consecutive_panics: 0,
                    disabled: false,
                });
                clog_info!(
                    "💾 Disk reporter '{}' registered dynamically (total: {})",
                    name,
                    st.reporters.len()
                );
            }

            // Refresh disk stats. sysinfo caches mount-point state, so this is
            // cheap once warmed. The lock is uncontended (only this task locks
            // it), so even a momentarily slow stat can't stall a reader.
            st.disks.refresh(true);
            let home = crate::modules::persona_instance_manager::resolve_continuum_root();
            let drives: Vec<crate::capacity::system_profile::DriveInfo> = st
                .disks
                .iter()
                .map(|d| crate::capacity::system_profile::DriveInfo {
                    mount: d.mount_point().to_path_buf(),
                    total_bytes: d.total_space(),
                    available_bytes: d.available_space(),
                    role: crate::capacity::system_profile::DriveRole::System,
                })
                .collect();
            let reading = volume_reading(&drives, &home);
            match reading.source {
                VolumeSource::Home => {}
                VolumeSource::HottestVolume => crate::probe!(
                    class = "disk.pressure.unmeasured_home",
                    home = %home.display(),
                    mounts = drives.len() as u64,
                    fallback_total = reading.total,
                    fallback_available = reading.available,
                    "no mounted volume holds the substrate's home — reporting the HOTTEST mounted volume, named as a fallback, never a zero"
                ),
                VolumeSource::None => crate::probe!(
                    class = "disk.pressure.unmeasured",
                    home = %home.display(),
                    "no mounted volume at all — disk pressure is UNKNOWN (typed), not Normal"
                ),
            }
            let volume_source = reading.source.label();
            let (total, available) = (reading.total, reading.available);
            let used = total.saturating_sub(available);
            let pressure = if total > 0 {
                used as f64 / total as f64
            } else {
                0.0
            };
            let level = reading.level_override.unwrap_or_else(|| DiskPressureLevel::from_pressure(pressure)); // unwrap_or_else: a measured volume derives its level from pressure; only "no volume at all" overrides to Unknown

            // Atomic publish — lock-free reads from anywhere.
            self.current_pressure
                .store(pressure.to_bits(), Ordering::Relaxed);
            CURRENT_DISK_LEVEL.store(level.to_u8(), Ordering::Relaxed);

            // Hysteresis.
            if level == st.prev_level {
                st.consecutive_at_level = st.consecutive_at_level.saturating_add(1);
            } else {
                st.consecutive_at_level = 1;
                st.prev_level = level;
            }
            let consecutive_at_level = st.consecutive_at_level;

            // Snapshot the live (non-quarantined) reporters by index so the fold
            // phase can update their fault counters back in place. Indices are
            // stable within a tick — only this single task mutates the vec, and
            // ticks never overlap.
            let live: Vec<(usize, Arc<dyn DiskReporter>)> = st
                .reporters
                .iter()
                .enumerate()
                .filter(|(_, e)| !e.disabled)
                .map(|(i, e)| (i, e.reporter.clone()))
                .collect();

            (
                total,
                available,
                used,
                pressure,
                level,
                consecutive_at_level,
                live,
                volume_source,
            )
        };

        // --- Phase 2: off-lock fan-out — each reporter on the blocking pool
        // with a 100 ms budget + panic isolation. `report()` is a sync stat that
        // may block, so it runs on `spawn_blocking` (not an inline future), and
        // its panic is caught inside the blocking closure — the right isolation
        // for a sync reporter, distinct from the inline `guarded()` path. ---
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

        // --- Phase 3: brief lock — fold outcomes into counters, build snapshot,
        // log, decide the ready edge. ---
        let (snapshot, fire_ready) = {
            let mut st = self.state.lock();

            let mut path_reports = Vec::with_capacity(results.len());
            for (idx, outcome) in results {
                let entry = &mut st.reporters[idx];
                let name = entry.reporter.name();
                match outcome {
                    ReporterCallOutcome::Report(report) => {
                        entry.consecutive_panics = 0;
                        path_reports.push(report);
                    }
                    ReporterCallOutcome::Panicked => {
                        entry.consecutive_panics += 1;
                        clog_warn!(
                            "💾 DiskReporter '{}' panicked ({}/{})",
                            name,
                            entry.consecutive_panics,
                            QUARANTINE_AFTER
                        );
                        if entry.consecutive_panics >= QUARANTINE_AFTER {
                            clog_warn!(
                                "💾 DiskReporter '{}' quarantined after {} panics",
                                name,
                                QUARANTINE_AFTER
                            );
                            entry.disabled = true;
                        }
                    }
                    ReporterCallOutcome::JoinError(e) => {
                        clog_warn!("💾 DiskReporter '{}' spawn_blocking failed: {}", name, e);
                    }
                    ReporterCallOutcome::TimedOut => {
                        entry.consecutive_panics += 1;
                        clog_warn!(
                            "💾 DiskReporter '{}' timed out (>100ms) ({}/{})",
                            name,
                            entry.consecutive_panics,
                            QUARANTINE_AFTER
                        );
                        if entry.consecutive_panics >= QUARANTINE_AFTER {
                            clog_warn!(
                                "💾 DiskReporter '{}' quarantined after {} failures",
                                name,
                                QUARANTINE_AFTER
                            );
                            entry.disabled = true;
                        }
                    }
                }
            }

            // Disk gate — close on sustained Critical (3+ polls = 90 s). Lives in
            // the cross-module `DISK_GATE_CLOSED` static (not the channel gate),
            // because bulk-write subsystems read it through `is_disk_gate_closed`.
            if level == DiskPressureLevel::Critical && consecutive_at_level >= 3 {
                if !is_disk_gate_closed() {
                    close_disk_gate();
                    clog_warn!(
                        "🚨 DISK GATE CLOSED — disk critical for {}+ seconds. \
                         Bulk-write subsystems should refuse new allocations.",
                        consecutive_at_level as u64 * POLL_INTERVAL.as_secs()
                    );
                }
            } else if level < DiskPressureLevel::Critical && is_disk_gate_closed() {
                open_disk_gate();
                clog_info!("💾 Disk gate re-opened — pressure eased to {}", level);
            }

            // Build the snapshot. Timestamp via SystemTime (not a Date.now
            // equivalent) so the value is real even in resume / replay contexts.
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            let snapshot = DiskPressureSnapshot {
                level,
                pressure,
                total_bytes: total,
                available_bytes: available,
                used_bytes: used,
                paths: path_reports,
                timestamp_ms: now_ms,
                consecutive_at_level,
                volume_source: volume_source.to_string(),
            };

            // Periodic logging — quiet under Normal, louder under stress.
            st.log_counter += 1;
            let should_log = match level {
                DiskPressureLevel::Normal => st.log_counter.is_multiple_of(10),
                DiskPressureLevel::Warning => st.log_counter.is_multiple_of(3),
                DiskPressureLevel::High | DiskPressureLevel::Critical | DiskPressureLevel::Unknown => true,
            };
            if should_log {
                let total_gb = total / (1024 * 1024 * 1024);
                let avail_gb = available / (1024 * 1024 * 1024);
                let paths_summary: String = snapshot
                    .paths
                    .iter()
                    .map(|p| format!("{}={}GB", p.name, p.bytes / (1024 * 1024 * 1024)))
                    .collect::<Vec<_>>()
                    .join(", ");
                clog_info!(
                    "💾 Disk: total={}GB avail={}GB pressure={:.1}% level={} [{}]",
                    total_gb,
                    avail_gb,
                    pressure * 100.0,
                    level,
                    if paths_summary.is_empty() {
                        "no reporters".to_string()
                    } else {
                        paths_summary
                    }
                );
            }

            // Fire the ready edge exactly once — AFTER the snapshot publishes
            // (below), so a subscriber woken by the edge reads real data, never
            // the default. We only decide it here while holding the state lock.
            let fire_ready = !st.first_snapshot_published;
            if fire_ready {
                st.first_snapshot_published = true;
            }

            (snapshot, fire_ready)
        };

        // Publish lock-free. The channel overwrites; readers never block us. The
        // process-wide copy is what `system/resources` reads: until 2026-09-18 the
        // disk had NO field on any command surface, so a monitor reading (0, 0) for
        // months was invisible from every seat — the absence had nowhere to show.
        *CURRENT_DISK_SNAPSHOT.write() = Some(snapshot.clone());
        self.channel.publish(snapshot);

        if fire_ready {
            let _ = self.ready_tx.send(true);
            crate::probe!(class = "ready.observed", module = "disk-pressure-monitor");
        }
    }
}

#[async_trait]
impl Daemon for DiskPressureMonitor {
    type Snapshot = DiskPressureSnapshot;

    fn name(&self) -> &'static str {
        "disk-pressure"
    }

    fn cadence(&self) -> Duration {
        POLL_INTERVAL
    }

    fn channel(&self) -> &DaemonChannel<DiskPressureSnapshot> {
        &self.channel
    }

    async fn tick(&self) {
        self.poll().await;
    }
}

// =============================================================================
// PressureBroker integration — DiskPressureMonitor as a signal-only pool
// =============================================================================

/// Plug `DiskPressureMonitor` into `PressureBroker` as a `ResourcePool`.
/// This is a **signal source**, not a holder of evictable files —
/// `evict_at_least` returns 0 because the monitor doesn't own any
/// disk content. Concrete disk-paged resources (genome cache, probe
/// JSONL spool, model registry, fixture archive) register their own
/// `ResourcePool` impls and the broker drives eviction against THOSE.
///
/// The broker still emits a typed `PressureAlert` for this pool when
/// disk pressure crosses tier thresholds — operators see "disk-root
/// at 92 % — freed 0 bytes (stuck)" exactly because the monitor is
/// signal-only. The zero-byte alert is the desired signal: "disk is
/// hot AND nobody owns the eviction."
impl crate::paging::pool::ResourcePool for DiskPressureMonitor {
    fn tier_name(&self) -> &str {
        "disk-root"
    }

    fn capacity_bytes(&self) -> u64 {
        self.current().total_bytes
    }

    fn usage_bytes(&self) -> u64 {
        self.current().used_bytes
    }

    fn evict_at_least(&self, _want_bytes: u64) -> u64 {
        // Signal-only pool — the monitor observes disk; it does not
        // own any path to delete from. Concrete disk pools (genome
        // cache etc.) register their own ResourcePool impls; the
        // broker drives eviction against those. Returning 0 surfaces
        // as a `PressureAlert { bytes_freed: 0, action_taken: true }`
        // — operator sees "disk hot AND stuck on this tier" exactly
        // because nothing owns the eviction here.
        0
    }

    fn snapshot(&self) -> Vec<crate::paging::pool::ResourcePoolEntry> {
        // No entries — the broker's snapshot view shows
        // capacity/usage/pressure for this tier without any per-entry
        // detail. Per-path detail lives on `DiskPressureSnapshot.paths`
        // (populated by registered `DiskReporter`s), not on the broker
        // surface.
        Vec::new()
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    //! What this catches:
    //!   1. Pressure tier thresholds match the doctrine (80/90/95)
    //!   2. Snapshot Default is Normal at 0.0 pressure
    //!   3. Reporter trait is object-safe (boxes into Arc<dyn>)
    //!
    //! Deliberately NOT testing: the loop body itself, because that
    //! requires sysinfo + a real tokio runtime and the unit-test value
    //! per Joel's doctrine ("less tests with more coverage") is in one
    //! integration test that actually starts the monitor and observes
    //! the watch — a follow-up PR adds that under the test-fixtures
    //! gate once a synthetic mount-point fake exists.
    use super::*;

    // what this catches (2026-09-19, Cormac + a027688a on #4185): the no-match arm
    // said UNKNOWN and handed on (0, 0) → pressure 0.0 → Normal. Now: the home's
    // volume when one holds it; else the HOTTEST mounted volume, named as a fallback;
    // else a typed Unknown — never a zero that reads as room.
    #[test]
    fn a_monitor_that_cannot_see_says_unknown_never_normal() {
        use crate::capacity::system_profile::{DriveInfo, DriveRole};
        let drive = |mount: &str, total: u64, avail: u64| DriveInfo {
            mount: std::path::PathBuf::from(mount),
            total_bytes: total,
            available_bytes: avail,
            role: DriveRole::System,
        };
        let drives = [drive("/", 1_000, 900), drive("/Volumes/hot", 1_000, 20)];
        let home_on_root = volume_reading(&drives, std::path::Path::new("/home/x/.continuum"));
        assert_eq!(home_on_root.source, VolumeSource::Home);
        assert_eq!((home_on_root.total, home_on_root.available), (1_000, 900));
        // No volume holds the home (a path no mount prefixes): the hottest volume, named.
        let nowhere = [drive("/Volumes/a", 1_000, 900), drive("/Volumes/hot", 1_000, 20)];
        let fallback = volume_reading(&nowhere, std::path::Path::new("/home/x/.continuum"));
        assert_eq!(fallback.source, VolumeSource::HottestVolume);
        assert_eq!(fallback.available, 20, "the most-pressured volume, not the first");
        assert_eq!(fallback.level_override, None);
        // No volumes at all: typed Unknown, and Unknown is ordered as HOT.
        let none = volume_reading(&[], std::path::Path::new("/home/x/.continuum"));
        assert_eq!(none.source, VolumeSource::None);
        assert_eq!(none.level_override, Some(DiskPressureLevel::Unknown));
        assert!(DiskPressureLevel::Unknown > DiskPressureLevel::Critical);
        assert_eq!(DiskPressureLevel::Unknown.to_string(), "unknown");
    }

    #[test]
    fn pressure_tiers_match_substrate_doctrine() {
        assert_eq!(
            DiskPressureLevel::from_pressure(0.0),
            DiskPressureLevel::Normal
        );
        assert_eq!(
            DiskPressureLevel::from_pressure(0.79),
            DiskPressureLevel::Normal
        );
        assert_eq!(
            DiskPressureLevel::from_pressure(0.80),
            DiskPressureLevel::Warning
        );
        assert_eq!(
            DiskPressureLevel::from_pressure(0.89),
            DiskPressureLevel::Warning
        );
        assert_eq!(
            DiskPressureLevel::from_pressure(0.90),
            DiskPressureLevel::High
        );
        assert_eq!(
            DiskPressureLevel::from_pressure(0.94),
            DiskPressureLevel::High
        );
        assert_eq!(
            DiskPressureLevel::from_pressure(0.95),
            DiskPressureLevel::Critical
        );
        assert_eq!(
            DiskPressureLevel::from_pressure(0.99),
            DiskPressureLevel::Critical
        );
    }

    #[test]
    fn level_to_u8_round_trips_via_current_level() {
        // Walking the atomic publication path: each variant's u8 maps
        // back to itself via `current_level`. This pins the atomic
        // contract — if someone reorders the variants and breaks the
        // u8 assignment, this test catches it.
        CURRENT_DISK_LEVEL.store(0, Ordering::Relaxed);
        assert_eq!(
            DiskPressureMonitor::current_level(),
            DiskPressureLevel::Normal
        );
        CURRENT_DISK_LEVEL.store(1, Ordering::Relaxed);
        assert_eq!(
            DiskPressureMonitor::current_level(),
            DiskPressureLevel::Warning
        );
        CURRENT_DISK_LEVEL.store(2, Ordering::Relaxed);
        assert_eq!(
            DiskPressureMonitor::current_level(),
            DiskPressureLevel::High
        );
        CURRENT_DISK_LEVEL.store(3, Ordering::Relaxed);
        assert_eq!(
            DiskPressureMonitor::current_level(),
            DiskPressureLevel::Critical
        );
        // Reset for any other test.
        CURRENT_DISK_LEVEL.store(0, Ordering::Relaxed);
    }

    /// What this catches: the gate is forward-flippable AND
    /// idempotent. Idempotency matters because the loop calls
    /// `close_disk_gate` every Critical poll once latched; opening
    /// after pressure eases must not reset other state.
    #[test]
    fn disk_gate_flips_and_resets() {
        // Cleanup pre-state from any prior test.
        open_disk_gate();
        assert!(!is_disk_gate_closed());
        close_disk_gate();
        assert!(is_disk_gate_closed());
        // Idempotent close — same state.
        close_disk_gate();
        assert!(is_disk_gate_closed());
        open_disk_gate();
        assert!(!is_disk_gate_closed());
    }

    /// What this catches: `DiskReporter` is object-safe. Without this,
    /// `Arc<dyn DiskReporter>` doesn't compile and the whole monitor
    /// shape (which depends on the trait object) collapses.
    #[test]
    fn reporter_trait_is_object_safe() {
        struct Stub;
        impl DiskReporter for Stub {
            fn name(&self) -> &'static str {
                "stub"
            }
            fn report(&self) -> DiskPathReport {
                DiskPathReport {
                    name: "stub".to_string(),
                    path: PathBuf::from("/tmp"),
                    bytes: 0,
                    detail: "".to_string(),
                }
            }
        }
        let _: Arc<dyn DiskReporter> = Arc::new(Stub);
    }
}
