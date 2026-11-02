//! lane_registry.rs — accounting for EVERY `llama-server` lane this core spawns,
//! so a crashed core never leaves an orphan holding VRAM.
//!
//! ## The gap this closes (why [`crate::inference::lane_pidfile`] isn't enough)
//!
//! `lane_pidfile` tracks exactly ONE pid — the live persona lane on the canonical
//! port — because its job is canonical-PORT contention: reap or adopt whoever
//! holds *that* port. It deliberately excludes the ephemeral eval/serving-lease
//! lanes ([`crate::inference::llama_server::EphemeralServingLane`]), which run on
//! their own scanned ports and must never touch the canonical pidfile.
//!
//! That leaves a hole: an ephemeral lane is killed on graceful `Drop`, but a
//! SIGKILLed / panicked / power-cut core skips `Drop`, and NOTHING recorded the
//! ephemeral pid — so its `llama-server` survives as an orphan (~6 GB resident)
//! that no successor knows to reap. Observed live: three `llama-server` processes
//! where two prior eval lanes had leaked across crashes.
//!
//! The registry fixes it by recording a per-pid file for EVERY lane the core
//! spawns (live and ephemeral), keyed by pid, removed on graceful teardown. On the
//! next boot [`sweep_orphans`] reaps any recorded EPHEMERAL lane still alive (a
//! fresh core has no in-flight eval, so every recorded ephemeral lane is
//! definitionally an orphan), and garbage-collects dead records of any role. The
//! LIVE record is left to `lane_pidfile`'s canonical-port reclaim, which
//! already adopts-or-reaps it — the two mechanisms compose, they don't fight.
//!
//! ## Safety: never blind-kill (shared with [`crate::inference::lane_pidfile`])
//!
//! A recorded pid can be STALE — the process died and the OS reused its number.
//! So a reap fires ONLY after [`crate::inference::lane_process::is_llama_server`]
//! positively identifies the pid as one of our `llama-server` children. A dead or
//! reused-pid record is removed, never signalled ([[fallbacks-are-illegal-fail-loud]]).
//!
//! ## Test isolation
//!
//! Core ops are PURE and directory-taking (`*_in`), so tests drive them against a
//! unique temp dir and never touch the real `~/.continuum/run/lanes/` of a live
//! core (the #7 `$HOME`-pollution class). Public wrappers resolve the one
//! canonical directory and delegate.

use super::lane_pidfile;
use super::lane_process;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// What a lane is FOR — the axis `sweep_orphans` reaps on. A fresh boot never has
/// an in-flight eval, so every recorded [`LaneRole::Ephemeral`] is an orphan to
/// reap; the [`LaneRole::Live`] record is deferred to `lane_pidfile`'s
/// canonical-port reclaim (adopt-if-healthy, reap-if-not).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LaneRole {
    /// THE host's persona lane on the canonical port (also tracked by
    /// `lane_pidfile` for port adoption). Left alone by [`sweep_orphans`].
    Live,
    /// An eval / serving-lease lane on its own scanned port. Always an orphan
    /// across a core restart → reaped by [`sweep_orphans`].
    Ephemeral,
}

/// One recorded lane. Small, human-readable JSON so a `cat ~/.continuum/run/lanes/*`
/// tells an operator exactly what the core believes it spawned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaneRecord {
    pub pid: u32,
    pub port: u16,
    pub role: LaneRole,
    /// The base model id the lane serves — for operator legibility in the sweep
    /// log, AND (since 2026-08-19) the key a successor core sizes it by.
    #[serde(default)]
    pub model: String,
    /// The per-slot context window this lane was launched with.
    ///
    /// # Why the record carries the SHAPE, not just the identity
    ///
    /// A successor core must be able to answer "how many bytes do my past forms
    /// hold?" — Joel's third quantity — *before* it plans anything. Residency is
    /// `weights + KV(window, lanes)`, so a record naming only the model cannot
    /// answer it, and a core that cannot answer it counts its own predecessor's
    /// 50 GB as FOREIGN. Measured 2026-08-19: that misfiling read as
    /// `usable_gb = 2` on a 64 GB machine, so the planner chose a 3B and reaped a
    /// healthy 27B to make room for it — twice in one boot.
    ///
    /// `0` means an older record that predates these fields. That is UNKNOWN, and
    /// callers must refuse to size it rather than treat it as a zero-byte lane
    /// ([[an-absence-is-an-unfinished-measurement]]).
    #[serde(default)]
    pub context_window: u32,
    /// The continuous-batching slot count (`--parallel` / `n_seq_max`) this lane
    /// was launched with. `0` is UNKNOWN — see [`Self::context_window`].
    #[serde(default)]
    pub lanes: u32,
    /// The KV page directory this lane was launched with (`--slot-save-path`) — the
    /// live-lane inventory the legacy page-dir migration protects. `None` for lanes
    /// recorded before this field existed (their dir is unknown → the migration stays
    /// its hand, see `migrate_legacy_page_dir`).
    #[serde(default)]
    pub page_dir: Option<PathBuf>,
    /// The engine binary this lane was launched from: how a deploy knows which engine slot a
    /// live lane still runs, so it never builds into it (`engine_slots::idle_slot`, card
    /// 2c5d0ec0), without guessing from the process table (unreadable across Windows service
    /// sessions). `None` for lanes recorded before this field existed.
    #[serde(default)]
    pub engine_bin: Option<PathBuf>,
    /// The process's OS start time (seconds since the epoch), stamped at spawn. With `pid` it
    /// names ONE engine incarnation: a successor core compares both, so a recycled pid is
    /// never mistaken for this engine (SHARED-RESIDENT-LIFECYCLE.md step 1). `0` for lanes
    /// recorded before this field existed, which never match a live process.
    #[serde(default)]
    pub started_s: u64,
    /// Where this lane was launched to keep its bytes (`--n-gpu-layers 0` or the GPU), so
    /// the pool its work leases is the one it actually uses, never the next launch's
    /// configuration (Codex on #5264). `None` for lanes recorded before this field existed.
    #[serde(default)]
    pub placement: Option<crate::inference::llama_server::LanePlacement>,
    /// The weights file the engine was launched on, stamped before spawn: what an adapter
    /// trained in this lane names as its base (card 3fd98325). `None` for lanes recorded before
    /// this field existed, or whose file could not be stamped; their adapters stay where they
    /// were trained.
    #[serde(default)]
    pub weights: Option<crate::inference::weights_stamp::WeightsStamp>,
}

impl LaneRecord {
    /// The governed pool this lane's bytes live in, when its placement was recorded.
    pub fn memory(&self) -> Option<crate::resources::ResourceKind> {
        self.placement.map(crate::inference::llama_server::LanePlacement::pool_kind)
    }

    /// The lane's root url, the one spelling every in-process client addresses it by (the
    /// training run posts `/train` under it).
    pub fn root_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    /// This lane's engine incarnation.
    pub fn incarnation(&self) -> super::engine_residency::EngineIncarnation {
        super::engine_residency::EngineIncarnation { pid: self.pid, started_s: self.started_s, port: self.port }
    }
}

/// The LIVE-role lane left behind by a previous generation of this core, if one
/// is still running — a **past form of ourself**.
///
/// # Two authorities, one question each (corrected 2026-08-19, measured)
///
/// The first cut of this function scanned `read_dir` for any `Live` record whose pid
/// was `is_alive`. Both halves were wrong, and the live board caught it:
/// `serving = 0.81 GB — "qwen2.5-0.5b … inherited from a previous generation"` while
/// the actual live lane was a 27B holding ~50 GB. Two defects compounding:
///
/// 1. **`read_dir` order is arbitrary.** A crashed generation can leave several `Live`
///    records; the loop returned whichever the filesystem listed first, which was a
///    stale 0.5B.
/// 2. **`is_alive` is not identity.** A dead lane's pid gets REUSED by an unrelated
///    process, and then a stale record reads as live forever. `lane_pidfile::reclaim`
///    has always been "identity-verified (never a reused pid)" and
///    [`lane_process::is_llama_server`] exists precisely for this; the first cut simply
///    did not call it.
///
/// So: the **pidfile** answers *which* lane is live (it holds exactly one canonical
/// pid), the **registry record** supplies that lane's *shape*, and `is_llama_server`
/// proves the pid still belongs to a lane rather than to whoever inherited its number.
/// One authority per question, which is why neither can drift from the other.
///
/// The shape fields may still be `0` (a record predating them); sizing remains the
/// caller's judgement and it must refuse rather than guess.
pub fn live_lane() -> Option<LaneRecord> {
    let pid = lane_pidfile::read()?;
    if !lane_process::is_llama_server(pid) {
        // Dead, or the number now belongs to something else. Either way this machine
        // has no past form of itself to count — an absence, not a zero-byte lane.
        return None;
    }
    let body = std::fs::read_to_string(lanes_dir()?.join(format!("{pid}.lane"))).ok()?;
    let rec = serde_json::from_str::<LaneRecord>(&body).ok()?;
    (rec.role == LaneRole::Live && rec.pid == pid).then_some(rec)
}

/// The incarnation (pid + OS start time + port) of the LIVE lane the canonical pidfile names,
/// from its registry record alone: two small file reads, no process-table probe, so the gateway
/// reconcile can ask it every tick. Every live launch rewrites both files, so a replacement
/// engine always reads as a different incarnation. `None` without a pidfile or a matching live
/// record, or for a record without a start time, which names no one process.
pub fn live_incarnation() -> Option<super::engine_residency::EngineIncarnation> {
    let pid = lane_pidfile::read()?;
    let body = std::fs::read_to_string(lanes_dir()?.join(format!("{pid}.lane"))).ok()?;
    let rec = serde_json::from_str::<LaneRecord>(&body).ok()?;
    (rec.role == LaneRole::Live && rec.pid == pid && rec.started_s != 0).then(|| rec.incarnation())
}

/// WHY a sweep is running — the one axis on which boot and shutdown differ.
///
/// They differ in exactly one judgement: what a still-alive LIVE-role record
/// means. At boot it may be a perfectly good server this core can adopt, so the
/// decision belongs to `lane_pidfile`'s canonical-port reclaim. At shutdown
/// nothing is adoptable by definition — the core is going away — so a survivor is
/// a leak. Encoding that as a MODE on one sweep (rather than a second sweep
/// function) is what stops the two paths from drifting: every other rule —
/// never-blind-kill, GC dead records, drop unparseable garbage — is shared by
/// construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SweepMode {
    /// A fresh core starting up. Ephemeral records are definitionally orphans;
    /// the LIVE record is left for `lane_pidfile` to adopt-or-reap.
    Boot,
    /// This core is shutting down. EVERY lane it owns must die with it, live
    /// included — `stop` that leaves a server holding VRAM has not stopped.
    Shutdown,
}

/// What a sweep did to one record — exhaustive so every branch is
/// loggable and a new state can't be silently dropped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SweepOutcome {
    /// Termination was requested, but exit is not observed; retain the record for retry.
    ExitUnconfirmed { pid: u32, port: u16 },
    TerminationRefused { pid: u32, started_s: u64, reason: String },
    /// Killed a live ephemeral orphan and removed its record.
    ReapedEphemeral { pid: u32, port: u16 },
    /// Killed the LIVE lane and removed its record. [`SweepMode::Shutdown`] only —
    /// at boot a live survivor is adoptable, at shutdown it is a leak.
    ReapedLive { pid: u32, port: u16 },
    /// Record named a pid that is no longer alive — stale file removed, no kill.
    RemovedDead { pid: u32 },
    /// Pid is alive but NOT a `llama-server` (reused number) — record removed, the
    /// unrelated process left untouched.
    RemovedReused { pid: u32 },
    /// A live-lane record — left for `lane_pidfile` to adopt-or-reap.
    LeftLive { pid: u32 },
    /// A `.lane` file that didn't parse — removed as garbage.
    RemovedUnparseable { path: PathBuf },
}

/// The canonical lanes directory: `~/.continuum/run/lanes/`. `None` only if there
/// is no home directory — a degenerate environment where accounting is disarmed.
pub fn lanes_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".continuum").join("run").join("lanes"))
}

/// Record a spawned lane at the canonical directory. A write failure DISARMS
/// future orphan-reclaim for this lane but must NEVER fail the serve (the server
/// is up; the record is a recovery aid). Returns the error so the caller can probe
/// it loud without aborting.
pub fn record(rec: &LaneRecord) -> std::io::Result<()> {
    let dir = lanes_dir().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "no home dir for lane registry",
        )
    })?;
    record_in(&dir, rec)
}

/// Remove a lane's record on graceful teardown. Idempotent — a missing file is
/// success.
pub fn remove(pid: u32) {
    if let Some(dir) = lanes_dir() {
        remove_in(&dir, pid);
    }
}

/// Reap every orphaned ephemeral lane recorded by a crashed predecessor and
/// garbage-collect dead records. Resolves the canonical directory then delegates
/// to the pure [`sweep_in`]. Returns what it did, for the caller to log.
pub fn sweep_orphans() -> Vec<SweepOutcome> {
    match lanes_dir() {
        Some(dir) => sweep_in(&dir, SweepMode::Boot),
        None => Vec::new(),
    }
}

/// Reap EVERY lane this install owns — live and ephemeral — for shutdown.
///
/// `stop` reaps cores ([`continuum_cli_lifecycle::core_bind_guard`]) and owned engine
/// orphans, but until this existed it never touched a `llama-server`: the whole
/// registry was swept only on the NEXT boot, so shutting Continuum down left
/// every lane resident. Measured 2026-08-17 on the M5: an ephemeral 27B lane
/// (19 GB) and the live 14B lane were up simultaneously; the planner sized its
/// window against what was left and served citizens a 2,816-token context, which
/// cannot even hold the tool surface. `reboot` could not clear it — reboot is
/// stop + start, and neither half owned lanes.
///
/// Role-blind on purpose. The live/ephemeral split exists to decide ADOPTION, and
/// nothing is adoptable by a core that is exiting.
pub async fn sweep_all(deadline: std::time::Instant) -> Result<Vec<SweepOutcome>, String> {
    let Some(dir) = lanes_dir() else {
        return Ok(Vec::new());
    };
    let (outcomes, pending) = tokio::task::spawn_blocking(move || {
        let mut pending = Vec::new();
        let outcomes = sweep_collect(&dir, SweepMode::Shutdown, &mut pending);
        (outcomes, pending)
    })
    .await
    .map_err(|e| format!("lane shutdown sweep failed: {e}"))?;
    confirm_pending(outcomes, pending, deadline.saturating_duration_since(std::time::Instant::now())).await
}

async fn confirm_pending(
    mut outcomes: Vec<SweepOutcome>,
    pending: Vec<(PathBuf, LaneRecord)>,
    within: std::time::Duration,
) -> Result<Vec<SweepOutcome>, String> {
    let identities = pending.iter().map(|(_, rec)| lane_identity(rec)).collect();
    let survivors = continuum_process::confirm_gone(identities, within).await;
    tokio::task::spawn_blocking(move || {
        for (path, rec) in pending {
            if survivors.contains(&lane_identity(&rec)) {
                continue;
            }
            if let Some(outcome) = outcomes.iter_mut().find(|o| {
                matches!(o,
                SweepOutcome::ExitUnconfirmed { pid, .. } if *pid == rec.pid)
            }) {
                *outcome = finish_confirmed_reap(&path, &rec);
            }
        }
        outcomes
    })
    .await
    .map_err(|e| format!("lane exit confirmation failed: {e}"))
}

fn lane_identity(rec: &LaneRecord) -> crate::system_resources::process_tree::ProcessIdentity {
    crate::system_resources::process_tree::ProcessIdentity {
        pid: rec.pid,
        start_time: rec.started_s,
    }
}

// The wait releases the registry lock. A replacement must never be removed or
// reported stopped by the predecessor's exit proof.
fn finish_confirmed_reap(path: &Path, rec: &LaneRecord) -> SweepOutcome {
    let _writes = writes();
    let unchanged = std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str::<LaneRecord>(&raw).ok())
        .as_ref()
        == Some(rec);
    if !unchanged || std::fs::remove_file(path).is_err() {
        return SweepOutcome::ExitUnconfirmed {
            pid: rec.pid,
            port: rec.port,
        };
    }
    match rec.role {
        LaneRole::Live => SweepOutcome::ReapedLive {
            pid: rec.pid,
            port: rec.port,
        },
        LaneRole::Ephemeral => SweepOutcome::ReapedEphemeral {
            pid: rec.pid,
            port: rec.port,
        },
    }
}

/// `<pid>.lane` under `dir`.
/// Every write to the registry (record, remove, sweep, restamp) holds this, so a
/// compare-then-write ([`restamp_placement_in`]) never interleaves with a replacement or a
/// removal by another task of this core. A poisoned lock still serializes: the data it
/// guards is on disk, not behind it.
static WRITES: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn writes() -> std::sync::MutexGuard<'static, ()> {
    WRITES.lock().unwrap_or_else(std::sync::PoisonError::into_inner) // poisoned still serializes: the guarded data is on disk, a panicked writer left no torn state here
}

/// Record `placement` on the lane `expected` names, only if its record on disk is still
/// exactly `expected` (card 22aa3379). `Ok(false)` = the record was replaced, changed or
/// removed since it was read, and nothing was written: a removed record is never recreated,
/// and no other lane's record is touched (unlike [`record_in`], which clears prior live
/// records). The write is a temp file renamed over the record.
pub fn restamp_placement(
    expected: &LaneRecord,
    placement: crate::inference::llama_server::LanePlacement,
) -> std::io::Result<bool> {
    let dir = lanes_dir().ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no home dir for lane registry"))?;
    restamp_placement_in(&dir, expected, placement)
}

pub(crate) fn restamp_placement_in(
    dir: &Path,
    expected: &LaneRecord,
    placement: crate::inference::llama_server::LanePlacement,
) -> std::io::Result<bool> {
    let _writes = writes();
    let path = record_path(dir, expected.pid);
    let current = match std::fs::read_to_string(&path) {
        Ok(raw) => serde_json::from_str::<LaneRecord>(&raw).ok(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e),
    };
    if current.as_ref() != Some(expected) {
        return Ok(false);
    }
    let stamped = LaneRecord { placement: Some(placement), ..expected.clone() };
    let tmp = path.with_extension("lane.tmp");
    std::fs::write(&tmp, encode(&stamped)?)?;
    std::fs::rename(&tmp, &path)?;
    Ok(true)
}

fn record_path(dir: &Path, pid: u32) -> PathBuf {
    dir.join(format!("{pid}.lane"))
}

/// Write `rec` as JSON to `<pid>.lane`, creating the directory if needed. Pure:
/// the caller owns the dir so tests never touch the real registry.
pub(crate) fn record_in(dir: &Path, rec: &LaneRecord) -> std::io::Result<()> {
    let _writes = writes();
    std::fs::create_dir_all(dir)?;
    // A LIVE lane is SINGULAR by nature — one persona lane on the canonical port.
    // Recording a new live lane therefore SUPERSEDES any prior live record, which
    // must name a now-dead predecessor (its process was reclaimed on this boot).
    // Clearing it at write-time is what stops a stale live record from lingering:
    // `sweep_orphans` deliberately never reaps a live record (it defers to
    // `lane_pidfile`), so if the OS later recycled that dead pid the stale file
    // would otherwise never be cleaned. Ephemeral records are many-at-once and
    // clear nothing.
    if rec.role == LaneRole::Live {
        clear_live_records_in(dir);
    }
    std::fs::write(record_path(dir, rec.pid), encode(rec)?)
}

/// A record's on-disk form, the one encoder both writers use.
fn encode(rec: &LaneRecord) -> std::io::Result<String> {
    serde_json::to_string(rec).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e)) // file format on disk: the `.lane` JSON an operator reads and a successor core adopts by
}

/// Remove every existing LIVE-role record under `dir`. Called only when a fresh
/// live lane records itself, enforcing the one-live-lane invariant. A dir read
/// failure means "nothing to clear" — the create_dir_all in `record_in` runs
/// first, so the directory exists.
fn clear_live_records_in(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("lane") {
            continue;
        }
        let is_live = std::fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str::<LaneRecord>(&raw).ok())
            .is_some_and(|r| r.role == LaneRole::Live);
        if is_live {
            let _ = std::fs::remove_file(&path);
        }
    }
}

/// Remove `<pid>.lane` under `dir` if present. Idempotent.
pub(crate) fn remove_in(dir: &Path, pid: u32) {
    let _writes = writes();
    let _ = std::fs::remove_file(record_path(dir, pid));
}

/// Every lane record on disk, in no particular order.
///
/// The census an admission decision reads: `llama_server`'s ephemeral spawn gate needs
/// to know what is ALREADY running before it stands another engine up, and it needs
/// that mid-life — not at the boot/stop edges the sweeps fire on. Read-only and
/// non-destructive on purpose: reconciling records against liveness is the CALLER's
/// job (it owns the never-blind-kill identity check), so this stays a plain read.
pub fn records() -> Vec<LaneRecord> {
    lanes_dir().map(|d| records_in(&d)).unwrap_or_default() // unwrap_or_default: no home dir ⇒ no registry ⇒ an empty census is the honest answer
}

/// The census a DESTRUCTIVE decision reads — fail-closed. [`records`] is the right
/// read for an admission count (an unreadable record is one fewer lane to worry
/// about); it is the wrong read for a sweep that deletes what it does not find:
/// a `.lane` file that fails to read or parse is a lane whose page dir the sweep
/// would then not protect (Astra's review of #4069). Missing directory = `Ok(empty)`
/// (the normal first-run state); any entry that cannot be read or parsed = `Err`,
/// and the caller must not delete on it.
pub fn records_checked() -> std::io::Result<Vec<LaneRecord>> {
    match lanes_dir() {
        Some(d) => records_checked_in(&d),
        None => Ok(Vec::new()),
    }
}

/// The fail-closed enumeration against an explicit `dir` (see [`records_checked`]).
pub(crate) fn records_checked_in(dir: &Path) -> std::io::Result<Vec<LaneRecord>> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let mut out = Vec::new();
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|x| x.to_str()) != Some("lane") {
            continue;
        }
        let raw = std::fs::read_to_string(&path)?;
        let rec: LaneRecord = serde_json::from_str(&raw).map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("{}: {e}", path.display()),
            )
        })?;
        out.push(rec);
    }
    Ok(out)
}

/// The pure enumeration against an explicit `dir`, so tests read a temp registry and
/// never the live core's `~/.continuum/run/lanes/`.
fn records_in(dir: &Path) -> Vec<LaneRecord> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        // Missing directory is the normal first-run state — nothing recorded, not a
        // failure to look.
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("lane"))
        .filter_map(|e| {
            std::fs::read_to_string(e.path())
                .ok()
                .and_then(|raw| serde_json::from_str::<LaneRecord>(&raw).ok())
        })
        .collect()
}

fn finish_signal(path: &Path, rec: &LaneRecord, signal: Result<bool, String>) -> SweepOutcome {
    match signal {
        Ok(_) => finish_reap(path, rec),
        Err(reason) => SweepOutcome::TerminationRefused {
            pid: rec.pid,
            started_s: rec.started_s,
            reason,
        },
    }
}

// A successful signal request is not exit evidence (notably taskkill access
// denial on Windows). Preserve ownership until the process is actually gone.
fn finish_reap(path: &Path, rec: &LaneRecord) -> SweepOutcome {
    if continuum_cli_lifecycle::process::is_alive(rec.pid) {
        return SweepOutcome::ExitUnconfirmed { pid: rec.pid, port: rec.port };
    }
    let _ = std::fs::remove_file(path);
    match rec.role {
        LaneRole::Live => SweepOutcome::ReapedLive { pid: rec.pid, port: rec.port },
        LaneRole::Ephemeral => SweepOutcome::ReapedEphemeral { pid: rec.pid, port: rec.port },
    }
}

/// The pure sweep against an explicit `dir`. See [`sweep_orphans`] / [`sweep_all`].
fn sweep_in(dir: &Path, mode: SweepMode) -> Vec<SweepOutcome> {
    sweep_collect(dir, mode, &mut Vec::new())
}

fn sweep_collect(dir: &Path, mode: SweepMode, pending: &mut Vec<(PathBuf, LaneRecord)>) -> Vec<SweepOutcome> {
    let _writes = writes();
    let mut outcomes = Vec::new();
    // A missing directory is the normal first-run / all-graceful-prior-shutdown
    // state — nothing to sweep, not a fallback.
    let Ok(entries) = std::fs::read_dir(dir) else {
        return outcomes;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("lane") {
            continue;
        }
        let Some(rec) = std::fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str::<LaneRecord>(&raw).ok())
        else {
            // Garbage file — remove it and move on, never act on a number we
            // can't trust.
            let _ = std::fs::remove_file(&path);
            outcomes.push(SweepOutcome::RemovedUnparseable { path });
            continue;
        };

        // Dead record of ANY role → garbage-collect, never a kill on a recycled
        // number.
        if !continuum_cli_lifecycle::process::is_alive(rec.pid) {
            let _ = std::fs::remove_file(&path);
            outcomes.push(SweepOutcome::RemovedDead { pid: rec.pid });
            continue;
        }

        // At BOOT a live survivor may be adoptable, so the decision defers to
        // `lane_pidfile`'s canonical-port reclaim. At SHUTDOWN nothing is
        // adoptable — this core is exiting — so every role is reaped.
        let reap = match (rec.role, mode) {
            (LaneRole::Ephemeral, _) => true,
            (LaneRole::Live, SweepMode::Shutdown) => true,
            // The live lane's port is `lane_pidfile`'s job (adopt-or-reap). Leave
            // both the process and its record; if `lane_pidfile` reaps it, the next
            // boot sees a dead pid here and GCs the file.
            (LaneRole::Live, SweepMode::Boot) => false,
        };
        if !reap {
            outcomes.push(SweepOutcome::LeftLive { pid: rec.pid });
            continue;
        }
        if lane_process::is_llama_server(rec.pid) {
            // Capture the stamped incarnation before signalling. Unstamped
            // legacy records cannot authorize termination of a recycled PID.
            let identified = rec.started_s != 0;
            let outcome = if identified {
                finish_signal(&path, &rec, continuum_process::signal_if_same(lane_identity(&rec)))
            } else {
                // No stamped incarnation: retain evidence instead of signalling
                // an unbound PID after the destructive stop has begun.
                SweepOutcome::TerminationRefused {
                    pid: rec.pid,
                    started_s: rec.started_s,
                    reason: "recorded process identity could not be verified".into(),
                }
            };
            if identified && matches!(outcome, SweepOutcome::ExitUnconfirmed { .. }) {
                pending.push((path, rec));
            }
            outcomes.push(outcome);
        } else {
            // Alive but not one of ours — a reused pid. Drop the stale
            // record; never signal an unrelated process.
            let _ = std::fs::remove_file(&path);
            outcomes.push(SweepOutcome::RemovedReused { pid: rec.pid });
        }
    }
    outcomes
}

#[cfg(test)]
mod tests;
