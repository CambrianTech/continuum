//! Concrete `DiskReporter`s for the substrate's known disk-cache classes.
//!
//! The 2026-07-13 incident: `DiskPressureMonitor` ran at `level=high` for
//! days logging `[no reporters]` while the shared cargo-target cache grew
//! to 363 GB and three persona workspaces copied 23 GB each — 460 GB of
//! creep the monitor could see (root-fs pressure) but could not NAME
//! (empty per-path breakdown), so nothing and no one knew where to sweep.
//! `main.rs` started the monitor with `Vec::new()` reporters from day one.
//!
//! This module closes wire (1) of task #155: one reporter per cache class,
//! all fed by ONE background scanner. The `DiskReporter::report` contract
//! is a 100 ms budget on the blocking pool — far too tight to walk a
//! 300 GB tree — so reporters read a cached `AtomicU64` and the walking
//! happens on [`DiskUsageScanner`], a canonical [`Daemon`] ticking every
//! 5 minutes (disk grows slowly; the cadence ladder says lean slower) — and
//! each tree is re-walked no sooner than 20× what its last walk cost
//! ([`next_scan_delay`]), so a million-entry tree cannot own a core.
//! One scanner for all paths, not a task per reporter — one concurrent
//! concern, one task (CONCURRENCY-STYLE-GUIDE: no parallel managers).

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;

use crate::runtime::{spawn_daemon, Daemon, DaemonChannel};

use super::disk_pressure::{DiskPathReport, DiskReporter};

/// One tracked cache-class directory: identity + cached recursive size.
/// The reporter half reads `bytes` lock-free; the scanner half refreshes it.
pub struct TrackedDir {
    name: &'static str,
    path: PathBuf,
    bytes: AtomicU64,
    /// Historical targets discovered beneath a citizen retain their hands owner.
    citizen: parking_lot::RwLock<Option<uuid::Uuid>>,
    /// Advances on a fresh measurement, not eviction write-through. Expensive
    /// negative custody checks may wait for this existing scanner cadence.
    observation: AtomicU64,
    /// Whether the scanner has completed at least one walk — before that,
    /// the reporter labels the value as pending instead of claiming "0 B"
    /// (an honest void, never a false zero).
    scanned: std::sync::atomic::AtomicBool,
    /// What the last walk COST — entries visited and wall time — so the number
    /// carries its price and the scanner can pace itself by it (see
    /// [`next_scan_delay`]). 0 entries / 0 ms = set directly, never walked.
    entries: AtomicU64,
    walk_ms: AtomicU64,
    /// Scanner-clock ms (monotonic, from the scanner's own start) before which
    /// this dir is NOT walked again. 0 = due now.
    next_due_ms: AtomicU64,
    /// Suffixes naming files this class owns beside a FILE root — a SQLite store's
    /// `-wal`/`-shm`, a log's rotated `.1`. Declared by the class, never inferred
    /// from a name prefix (that admitted `events.sqlite.backup`), and weighed
    /// whether or not the root file itself exists (a log between rotation and
    /// reopen is all predecessor). Empty for a directory class.
    companions: &'static [&'static str],
}

impl TrackedDir {
    pub fn new(name: &'static str, path: PathBuf) -> Arc<Self> {
        Self::with_companions(name, path, &[])
    }

    /// A class rooted at one FILE plus the companion files it owns (see `companions`).
    pub fn with_companions(name: &'static str, path: PathBuf, companions: &'static [&'static str]) -> Arc<Self> {
        Arc::new(Self {
            name,
            path,
            bytes: AtomicU64::new(0),
            citizen: parking_lot::RwLock::new(None),
            observation: AtomicU64::new(0),
            scanned: std::sync::atomic::AtomicBool::new(false),
            entries: AtomicU64::new(0),
            walk_ms: AtomicU64::new(0),
            next_due_ms: AtomicU64::new(0),
            companions,
        })
    }

    /// Everything this class holds: its root's walk plus each declared companion's.
    pub(crate) fn walk(&self) -> TreeWalk {
        let mut total = walk_tree(&self.path);
        for suffix in self.companions {
            let mut companion = self.path.as_os_str().to_owned();
            companion.push(suffix);
            let part = walk_tree(std::path::Path::new(&companion));
            total.bytes = total.bytes.saturating_add(part.bytes);
            total.entries = total.entries.saturating_add(part.entries);
        }
        total
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    pub(crate) fn citizen(&self) -> Option<uuid::Uuid> {
        *self.citizen.read()
    }

    pub(super) fn bind_citizen(&self, peer: uuid::Uuid) -> Result<(), String> {
        // An eviction holds the read guard through its last filesystem action.
        // Discovery retries on the existing scan tick instead of blocking an
        // async worker or changing custody beneath an in-flight generic eviction.
        let mut owner = self.citizen.try_write().ok_or("Cargo eviction is in flight; resident binding deferred")?;
        if owner.is_some_and(|prior| prior != peer) {
            return Err("Cargo root already belongs to another citizen".into());
        }
        *owner = Some(peer);
        Ok(())
    }

    pub(crate) fn eviction_custody(&self) -> parking_lot::RwLockReadGuard<'_, Option<uuid::Uuid>> {
        self.citizen.read()
    }

    /// The cache CLASS this directory is. Public because a pool's identity must be
    /// derived from it rather than hardcoded: two `CargoTargetPool`s both answering
    /// `"disk-cargo-target"` meant `PressureBroker::register` (dedup-by-name)
    /// silently REPLACED the first with the second, leaving the shared cache
    /// ungoverned while two boot lines claimed otherwise.
    pub fn class(&self) -> &'static str {
        self.name
    }

    /// The cache-class name. `path` and `bytes` already had accessors;
    /// this one was missing, so a pool built over the class had to
    /// re-declare its own name and could drift from the reporter's.
    /// One measurement, two consumers — one name too.
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// Cached recursive size — lock-free. Shared view for the reporter
    /// AND any eviction pool built over the same class (one measurement,
    /// two consumers — never two walkers disagreeing about one dir).
    pub fn bytes(&self) -> u64 {
        self.bytes.load(Ordering::Relaxed)
    }

    pub(crate) fn observation(&self) -> u64 {
        self.observation.load(Ordering::Relaxed)
    }

    /// The scanner's write seam (also the test seam for pools built on
    /// this measurement). Marks the dir scanned — a set value is a real
    /// value, never "pending".
    pub(crate) fn set_bytes(&self, bytes: u64) {
        self.bytes.store(bytes, Ordering::Relaxed);
        self.scanned.store(true, Ordering::Relaxed);
        self.observation.fetch_add(1, Ordering::Relaxed);
    }

    /// The scanner's write seam proper: the walk's yield AND its cost. The cost
    /// sets when this dir is walked again — [`next_scan_delay`] — so a tree
    /// that takes 90 s to measure is measured every 30 min, not every 5.
    pub(crate) fn record_walk(&self, walk: TreeWalk, took: Duration, now_ms: u64) {
        self.set_bytes(walk.bytes);
        self.entries.store(walk.entries, Ordering::Relaxed);
        self.walk_ms.store(took.as_millis() as u64, Ordering::Relaxed);
        self.next_due_ms
            .store(now_ms.saturating_add(next_scan_delay(took).as_millis() as u64), Ordering::Relaxed);
    }

    /// Is this dir due for a walk on the scanner's clock? Always true before the
    /// first walk.
    pub(crate) fn due(&self, now_ms: u64) -> bool {
        now_ms >= self.next_due_ms.load(Ordering::Relaxed)
    }

    /// The last walk's cost as (entries visited, wall time); `None` when the
    /// value was set directly (tests, eviction write-through) and never walked.
    pub fn walk_cost(&self) -> Option<(u64, Duration)> {
        let ms = self.walk_ms.load(Ordering::Relaxed);
        (ms > 0).then(|| (self.entries.load(Ordering::Relaxed), Duration::from_millis(ms)))
    }

    /// Subtract freed bytes immediately after an eviction so pressure
    /// reflects the delete now, not at the scanner's next 5-min walk
    /// (the broker ticks every 5 s — a stale size would re-fire eviction
    /// against space already freed).
    pub fn record_freed(&self, freed: u64) {
        let _ = self
            .bytes
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |cur| {
                Some(cur.saturating_sub(freed))
            });
    }
}

/// One process-wide measurement registry, installed once and extended when a
/// writer selects another Cargo root. Boot consumers, durable replay and later
/// writers all use the same TrackedDir instances and scanner.
mod registry;
mod class_writer;
mod standard;
pub use standard::standard_tracked_dirs;
pub use class_writer::ClassWriter;
/// A value an owned child process keeps until its tree has ended (see
/// `inference::continuum_process::holding`): the cargo-target lease is one. Defined here because
/// `CargoAdmission::kept` hands it out and this kernel module cannot name the engine.
pub type Kept = std::sync::Arc<dyn std::any::Any + Send + Sync>;

pub use registry::{admit_cargo_target, bind_cargo_owners, register_cargo_target, restore_cargo_roots};
pub use registry::resolve_existing_storage_path;
pub(crate) use registry::citizen_has_cargo_owner;
#[cfg(any(test, feature = "test-fixtures"))]
pub use registry::prepare_writer_fixture;
use registry::TRACKED_DIRS;

pub fn registry_installed() -> bool { TRACKED_DIRS.get().is_some() }

/// Install the boot-time tracked-dir set. Second call is a boot-sequence
/// bug — fail loud, never silently keep two measurement sets.
pub async fn install_tracked_dirs(dirs: Vec<Arc<TrackedDir>>) -> Vec<Arc<TrackedDir>> {
    let registry = registry::Registry::new(dirs).await;
    let dirs = registry.snapshot();
    if TRACKED_DIRS.set(registry).is_err() {
        panic!("install_tracked_dirs called twice — one measurement registry per process");
    }
    dirs
}

/// Fetch a tracked cache class by name, e.g. `"cargo-target"`. `None`
/// before boot installs the registry (tests, tools) — callers treat that
/// as "class not under management," never a default path guess.
pub fn tracked_dir(name: &str) -> Option<Arc<TrackedDir>> {
    TRACKED_DIRS.get()?.snapshot().into_iter().find(|d| d.name == name)
}

impl DiskReporter for TrackedDir {
    fn name(&self) -> &'static str {
        self.name
    }

    fn report(&self) -> DiskPathReport {
        // Also suppress a reporter already queued in an off-lock monitor poll.
        // Its bytes remain available to the scanner/broker through bytes().
        if self.citizen().is_some() {
            return DiskPathReport {
                name: self.name.to_string(), path: self.path.clone(), bytes: 0,
                detail: "included in citizens parent; child measured by Cargo pool".into(),
            };
        }
        let scanned = self.scanned.load(Ordering::Relaxed);
        DiskPathReport {
            name: self.name.to_string(),
            path: self.path.clone(),
            bytes: self.bytes.load(Ordering::Relaxed),
            detail: match (scanned, self.walk_cost()) {
                (false, _) => "first scan pending".to_string(),
                (true, None) => "cached recursive size (set directly, not walked)".to_string(),
                (true, Some((entries, took))) => format!(
                    "cached recursive size (walk {:.1} s over {} entries; next walk no sooner than {} s after it)",
                    took.as_secs_f64(),
                    entries,
                    next_scan_delay(took).as_secs()
                ),
            },
        }
    }
}

/// Every cache class that is a cargo target tree, and therefore takes the shared
/// [`super::CargoTargetPool`] owner.
///
/// ONE list, TWO consumers: the boot registration loop (`ipc/mod.rs`) walks it to give each
/// class a pool, and `every_cargo_target_class_has_an_owner` asserts no tracked class
/// matching this shape is missing from it. Before this existed the loop held its own
/// literal array, so a new cargo tree could be given a row and still reach production with
/// no pool — which is how `cargo-target-airc` came to hold 9.8 GB that was tracked by
/// nobody and evicted by nobody (2026-09-21, the M5 at 97%).
///
/// Adding a fourth tree means adding it here; both consumers then follow, and the test
/// fails until it does.
pub const CARGO_TARGET_CLASSES: [&str; 3] = ["cargo-target", "cargo-target-wt", "cargo-target-airc"];

/// Recursive size of `path` in bytes. Symlinks are NOT followed (a
/// persona workspace symlinking shared models must not double-count
/// them into its class). Unreadable entries are skipped, not fatal —
/// a half-measured tree is still a truthful lower bound.
pub(crate) fn dir_size_bytes(path: &std::path::Path) -> u64 {
    walk_tree(path).bytes
}

/// A walk's yield: the bytes under the root and every directory entry it read to
/// learn them (files, dirs, symlinks). The entries are the walk's PRICE — one
/// `lstat` each — and the price is what paces the next walk.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TreeWalk {
    pub bytes: u64,
    pub entries: u64,
}

/// Recursive size of `path` with the count of entries visited. Same contract as
/// [`dir_size_bytes`]: symlinks are counted as entries but never followed, an
/// unreadable entry is skipped, a missing root is an empty walk. A FILE root is its
/// own length: a class can be one file another program keeps beside tracked
/// directories (airc's event store in `~/.airc`); the files it owns beside it are
/// the class's declared companions ([`TrackedDir::walk`]), never a name prefix.
pub(crate) fn walk_tree(path: &std::path::Path) -> TreeWalk {
    if let Some(file) = std::fs::symlink_metadata(path).ok().filter(std::fs::Metadata::is_file) {
        return TreeWalk { bytes: file.len(), entries: 1 };
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return TreeWalk::default();
    };
    let mut walk = TreeWalk::default();
    for entry in entries.flatten() {
        walk.entries = walk.entries.saturating_add(1);
        let Ok(meta) = entry.metadata() else { continue };
        if meta.is_symlink() {
            continue;
        }
        if meta.is_dir() {
            let sub = walk_tree(&entry.path());
            walk.bytes = walk.bytes.saturating_add(sub.bytes);
            walk.entries = walk.entries.saturating_add(sub.entries);
        } else {
            walk.bytes = walk.bytes.saturating_add(meta.len());
        }
    }
    walk
}

struct CitizenTargetObservation {
    path: PathBuf,
    peer: uuid::Uuid,
    walk: TreeWalk,
}

/// One inclusive citizen scan, also observing Cargo-marked derived subtrees.
/// Names shortlist candidates; Cargo's markers establish class. Children belong
/// to the scanner/broker only: the parent remains the single disk reporter even
/// when publication fails, or a durable child is restored before this scan.
fn walk_citizen_tree(root: &std::path::Path) -> (TreeWalk, Vec<CitizenTargetObservation>) {
    fn walk(path: &std::path::Path, root: &std::path::Path, found: &mut Vec<CitizenTargetObservation>) -> TreeWalk {
        let mut measured = TreeWalk::default();
        let Ok(entries) = std::fs::read_dir(path) else { return measured };
        for entry in entries.flatten() {
            measured.entries = measured.entries.saturating_add(1);
            let Ok(metadata) = std::fs::symlink_metadata(entry.path()) else { continue };
            #[cfg(windows)]
            { use std::os::windows::fs::MetadataExt;
              if metadata.file_attributes() & 0x400 != 0 { continue; } }
            if metadata.file_type().is_symlink() { continue; }
            if metadata.is_dir() {
                let child = entry.path();
                let name = entry.file_name();
                let candidate = name.to_str().is_some_and(|name| name == "target" || name.starts_with("target-"));
                let peer = child.strip_prefix(root.join("peers")).ok()
                    .and_then(|relative| relative.components().next())
                    .and_then(|component| component.as_os_str().to_str())
                    .and_then(|id| uuid::Uuid::parse_str(id).ok());
                if let Some(peer) = peer.filter(|_| candidate && super::citizen_workspace_pool::is_cargo_target(&child)) {
                    let child_walk = walk_tree(&child);
                    measured.entries = measured.entries.saturating_add(child_walk.entries);
                    measured.bytes = measured.bytes.saturating_add(child_walk.bytes);
                    found.push(CitizenTargetObservation { path: child, peer, walk: child_walk });
                } else {
                    let child_walk = walk(&child, root, found);
                    measured.entries = measured.entries.saturating_add(child_walk.entries);
                    measured.bytes = measured.bytes.saturating_add(child_walk.bytes);
                }
            } else { measured.bytes = measured.bytes.saturating_add(metadata.len()); }
        }
        measured
    }
    let mut found = Vec::new();
    let measured = walk(root, root, &mut found);
    (measured, found)
}

/// Scanner tick — 5 minutes, and the FLOOR between two walks of one tree.
/// Disk-class sizes move on build/download timescales, not milliseconds.
const SCAN_INTERVAL: Duration = Duration::from_secs(300);

/// A WALK'S COST PACES THE WALK (2026-09-19, the M5): `citizens` had grown to
/// 2.44 M entries — 755 peer workspaces, 335 staged SWE checkouts, 95 GB — and
/// the recursive `lstat` of it took 90.7 s wall / 49 s sys, every 300 s, on a
/// node serving two 27B lanes: a 30% duty cycle of one core in the kernel,
/// forever, growing with every checkout (Joel: "never look up again … eats
/// CPU like mad"). The 9/4 fix (card 7c956bb4) only moved that walk off the
/// boot path; nothing bounded it. The bound: a tree is re-walked no sooner
/// than [`crate::runtime::daemon::DUTY_DIVISOR`] × the time its last walk took —
/// a walk may cost at most 1/20 of one core on average — and never sooner than
/// [`SCAN_INTERVAL`]. The measurement stays exact (no sampling, no partial trees);
/// only its cadence follows its price. A 2 s cargo-target walk keeps the 5 min; the
/// 90 s citizens walk moves to 30 min, and the report says so.
///
/// PURE: how long after a walk that took `last_walk` the same tree is walked
/// again — [`crate::runtime::daemon::paced_delay`] floored at [`SCAN_INTERVAL`].
pub fn next_scan_delay(last_walk: Duration) -> Duration {
    crate::runtime::daemon::paced_delay(SCAN_INTERVAL, last_walk)
}

/// Refreshes every [`TrackedDir`]'s cached size on its own tick, off the
/// hot path via `spawn_blocking`. Publishes a unit snapshot (the real
/// output is the atomics the reporters read); channel exists to satisfy
/// the canonical daemon seam and give tests a tick-completed edge.
pub struct DiskUsageScanner {
    dirs: Vec<Arc<TrackedDir>>,
    channel: DaemonChannel<u64>,
    ticks: AtomicU64,
    /// The scanner's own clock: `TrackedDir::next_due_ms` is ms on it. Monotonic,
    /// so a wall-clock jump can neither skip a walk nor storm them.
    started: Instant,
}

impl DiskUsageScanner {
    /// Spawn on the shared daemon runner and return the handle. The
    /// first walk happens on the first tick (immediately — the runner
    /// ticks once at spawn), so reporters carry real numbers within
    /// seconds of boot.
    pub fn start(dirs: Vec<Arc<TrackedDir>>) -> Arc<Self> {
        let scanner = Arc::new(Self {
            dirs,
            channel: DaemonChannel::ungated(0),
            ticks: AtomicU64::new(0),
            started: Instant::now(),
        });
        let _ = spawn_daemon(scanner.clone());
        scanner
    }
}

#[async_trait]
impl Daemon for DiskUsageScanner {
    type Snapshot = u64;

    fn name(&self) -> &'static str {
        "disk-usage-scanner"
    }

    fn cadence(&self) -> Duration {
        SCAN_INTERVAL
    }

    fn channel(&self) -> &DaemonChannel<u64> {
        &self.channel
    }

    async fn tick(&self) {
        // NEVER ON THE BOOT PATH: the first walk waits for the IPC bind. On
        // BigMama's GPU host (2026-09-04, card 7c956bb4) the recursive stat of
        // `citizens` (97k files on a cold HDD) ran during module init, saturated
        // the disk the registry and every module were reading, and the 300 s
        // boot watchdog killed a working core. "first scan pending" is the
        // honest report until then; the walk is worth nothing before the
        // socket answers.
        let mut ready = crate::runtime::ipc_ready::subscribe_ready();
        while !*ready.borrow_and_update() {
            if ready.changed().await.is_err() {
                return; // IPC thread gone — the process is exiting
            }
        }
        let dirs = TRACKED_DIRS.get().map(|registry| registry.snapshot())
            .unwrap_or_else(|| self.dirs.clone());
        for dir in &dirs {
            // A tree whose last walk was expensive is not due yet: its own cost set
            // the wait ([`next_scan_delay`]). The cached size stands meanwhile.
            if !dir.due(self.started.elapsed().as_millis() as u64) {
                continue;
            }
            let class = dir.clone();
            let citizens = dir.class() == "citizens";
            // Walk on the blocking pool — a deep tree must never stall
            // the async runtime the substrate's minds run on. The citizens tree also
            // observes Cargo targets; every other class weighs itself and the
            // companions it declares (`TrackedDir::walk`, card 0af5d5ec).
            let began = Instant::now();
            let walked = tokio::task::spawn_blocking(move || {
                if citizens { walk_citizen_tree(&class.path) } else { (class.walk(), Vec::new()) }
            }).await;
            let took = began.elapsed();
            match walked {
                Ok((walk, targets)) => {
                    for target in targets {
                        match registry::observe_citizen_target(&target.path, target.peer).await {
                            Ok(child) => child.record_walk(target.walk, took, self.started.elapsed().as_millis() as u64),
                            Err(error) => {
                                tracing::warn!(%error, "citizen Cargo observation remains charged to its parent");
                            }
                        }
                    }
                    dir.record_walk(walk, took, self.started.elapsed().as_millis() as u64);
                    continuum_base::probe!(
                        class = "disk.scan.walk",
                        class_name = dir.name,
                        bytes = walk.bytes,
                        entries = walk.entries,
                        walk_ms = took.as_millis() as u64,
                        next_walk_in_s = next_scan_delay(took).as_secs(),
                        "one cache class measured — its walk's cost sets when it is measured again"
                    );
                }
                // A walk that panicked measured nothing: the last value stands, never
                // a false zero, and it is retried on the next tick.
                Err(e) => {
                    continuum_base::probe!(
                        class = "disk.scan.walk_failed",
                        class_name = dir.name,
                        walk_ms = took.as_millis() as u64,
                        error = %e,
                        "the walk did not complete — cached size kept, retried next tick"
                    );
                }
            }
            // A breath between directories so one deep tree does not own the disk.
            tokio::task::yield_now().await;
        }
        let n = self.ticks.fetch_add(1, Ordering::Relaxed) + 1;
        self.channel.publish(n);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (Fable on the partition, 2026-10-11): a job directory now holds PARKED
    // examples (partition.json beside request.json) that nobody has trained. No tracked class may
    // contain a persona's job directories, so no pool's eviction can delete them; one that is
    // added must first decide what happens to parked examples (move them, or skip such dirs).
    #[test]
    fn no_tracked_class_contains_a_persona_job_directory() {
        let home = std::path::Path::new("/h");
        let job = home.join(".continuum/genome/Kimi/code_owner/00000000-0000-0000-0000-000000000001");
        for dir in standard_tracked_dirs(home, &home.join(".continuum")) {
            assert!(!job.starts_with(dir.path()), "{} ({}) contains persona job directories", dir.class(), dir.path().display());
        }
    }

    // what this catches (2026-09-21, the M5 at 97% full): a cargo target tree that has a
    // TrackedDir row but no eviction owner, because the boot registration loop held its
    // OWN literal list of classes and nobody updated both places. `cargo-target-airc`
    // held 9.8 GB that no reporter could see and no pool could evict — the same failure
    // `cargo-target-wt` had at 87 GB, repeating after the law against it was written,
    // because a law lives in a doc and this lives in a test.
    //
    // The invariant: every tracked class that IS a cargo target tree appears in
    // `CARGO_TARGET_CLASSES`, which `ipc/mod.rs` walks to hand each one a
    // `CargoTargetPool`. Add a fourth tree's row without adding it here and this fails.
    //
    // Mutation check: dropping "cargo-target-airc" from the const fails the first assert.
    #[test]
    fn every_cargo_target_class_has_an_owner() {
        let dirs = standard_tracked_dirs(std::path::Path::new("/h"), std::path::Path::new("/h/.continuum"));
        let tracked_cargo: Vec<&str> = dirs
            .iter()
            .map(|d| d.class())
            .filter(|c| c.starts_with("cargo-target"))
            .collect();

        for class in tracked_cargo {
            assert!(
                CARGO_TARGET_CLASSES.contains(&class),
                "tracked class {class:?} is a cargo target tree with no entry in \
                 CARGO_TARGET_CLASSES — it has a row (so the scanner sees it) but the boot \
                 loop in ipc/mod.rs never hands it a CargoTargetPool, so nothing evicts it. \
                 Add it to the const; both consumers then follow."
            );
        }

        // ...and the const names nothing that is not actually tracked, so a stale entry
        // cannot sit there implying an owner for a class that no longer exists.
        let tracked: Vec<&str> = dirs.iter().map(|d| d.class()).collect();
        for class in CARGO_TARGET_CLASSES {
            assert!(
                tracked.contains(&class),
                "CARGO_TARGET_CLASSES names {class:?}, which standard_tracked_dirs does not \
                 track — the boot loop will look it up, find nothing, and silently register \
                 no pool at all."
            );
        }
    }

    // what this catches: the hf-hub class tracking a path the artifacts are NOT at. It hardcoded
    // `home/.cache/huggingface` while the cold-storage installer relocates HF_HOME — measured on
    // BigMama, the class reported 0 bytes at the default while 2.48 TB sat on the cold drive,
    // invisible to the disk governor and owned by no eviction pool. A tracked dir pointing at the
    // wrong place is worse than an untracked one: it reports a reassuring zero.
    //
    // Mutation check: reverting to `home.join(".cache/huggingface")` fails the first assert.
    #[test]
    fn hf_hub_class_follows_a_relocated_cache_root() {
        let relocated = tempfile::tempdir().expect("tempdir");
        let home = tempfile::tempdir().expect("home");
        // Under the one crate-wide home lock (#4082): an ordinary #[test] is concurrent
        // with the rest of libtest, and every HomeGuard writes HF_HOME. The guard pins
        // the four home variables to `home`; the relocated HF_HOME override sits on top
        // and the guard's drop restores everything.
        let _home = crate::paths::test_home::HomeGuard::set_blocking(home.path());
        std::env::set_var("HF_HOME", relocated.path());

        let dirs = standard_tracked_dirs(home.path(), &home.path().join(".continuum"));
        let hf = dirs
            .iter()
            .find(|d| d.name == "hf-hub")
            .expect("hf-hub class must exist");
        assert_eq!(
            hf.path(),
            relocated.path(),
            "hf-hub must track the RELOCATED cache root, not the default under home"
        );
        assert!(
            !hf.path().starts_with(home.path()),
            "tracking a path under home while HF_HOME points elsewhere is the false-zero bug"
        );

    }

    // what this catches: the reporter half of the seam — a TrackedDir
    // reports its cached value inside the DiskReporter contract, labels
    // an unscanned value as pending (never a false "0 B measured"), and
    // flips to the measured detail once the scanner writes through.
    #[test]
    fn tracked_dir_reports_cached_value_and_honest_pending_state() {
        let dir = TrackedDir::new("cargo-target", PathBuf::from("/nonexistent"));
        let before = dir.report();
        assert_eq!(before.bytes, 0);
        assert!(before.detail.contains("pending"));

        dir.bytes.store(42_000_000_000, Ordering::Relaxed);
        dir.scanned.store(true, Ordering::Relaxed);
        let after = dir.report();
        assert_eq!(after.bytes, 42_000_000_000);
        assert!(!after.detail.contains("pending"));
    }

    // what this catches: dir_size_bytes measures real recursive content,
    // skips symlinks (no double-counting shared models into a workspace's
    // class), and returns 0 — not an error — on an unreadable/missing
    // root (a truthful lower bound, never a crash in the scanner tick).
    #[test]
    fn dir_size_walks_recursively_and_skips_symlinks() {
        let tmp = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir(tmp.path().join("sub")).expect("mkdir");
        std::fs::write(tmp.path().join("a.bin"), vec![0u8; 1000]).expect("write");
        std::fs::write(tmp.path().join("sub/b.bin"), vec![0u8; 500]).expect("write");
        #[cfg(unix)]
        std::os::unix::fs::symlink(tmp.path().join("a.bin"), tmp.path().join("link.bin"))
            .expect("symlink");

        assert_eq!(dir_size_bytes(tmp.path()), 1500);
        assert_eq!(dir_size_bytes(std::path::Path::new("/nonexistent-xyz")), 0);
        // The walk names its price: every entry read, the symlink included, the
        // missing root none.
        let walk = walk_tree(tmp.path());
        assert_eq!(walk.bytes, 1500);
        assert_eq!(walk.entries, 3 + u64::from(cfg!(unix)));
        assert_eq!(walk_tree(std::path::Path::new("/nonexistent-xyz")), TreeWalk::default());
        let citizen = tempfile::tempdir().unwrap();
        let peer = uuid::Uuid::new_v4();
        let workspace = citizen.path().join("peers").join(peer.to_string()).join("workspace");
        let target = workspace.join("checkout/target-historical");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("CACHEDIR.TAG"), b"Signature: 8a477f597d28d172789f06886806bc55\n").unwrap();
        std::fs::write(target.join(".rustc_info.json"), b"{}").unwrap();
        std::fs::write(target.join("derived"), vec![0u8; 4096]).unwrap();
        std::fs::create_dir_all(workspace.join("target-not-cargo")).unwrap();
        std::fs::write(workspace.join("target-not-cargo/work"), b"authored").unwrap();
        let (parent, children) = walk_citizen_tree(citizen.path());
        assert_eq!(children.len(), 1, "a target name without both Cargo markers stays work");
        assert_eq!(children[0].peer, peer);
        assert_eq!(children[0].path, target);
        assert_eq!(parent.bytes, walk_tree(citizen.path()).bytes,
            "parent always retains the complete charge, including failed child publication");
        assert_eq!(parent.bytes - children[0].walk.bytes, 8);
        // A file root is its own length; a class weighs exactly the companions it
        // declares (card 0af5d5ec, Codex review of #5080): a rotated predecessor
        // counts while its live file is absent, and a same-prefix file the class
        // does not own (a backup, an unrelated `-x` file) never does. Before, read_dir
        // failed on a file and the class reported a false 0 B.
        let family = tempfile::tempdir().expect("tempdir");
        std::fs::write(family.path().join("events.sqlite"), vec![0u8; 1000]).expect("write");
        std::fs::write(family.path().join("events.sqlite-wal"), vec![0u8; 300]).expect("write");
        std::fs::write(family.path().join("events.sqlite.backup"), vec![0u8; 70]).expect("write");
        std::fs::write(family.path().join("airc-daemon.log.1"), vec![0u8; 400]).expect("write");
        std::fs::write(family.path().join("airc-daemon.log-unrelated"), vec![0u8; 90]).expect("write");
        assert_eq!(walk_tree(&family.path().join("events.sqlite")), TreeWalk { bytes: 1000, entries: 1 });
        let store = TrackedDir::with_companions("airc-store", family.path().join("events.sqlite"), &["-wal", "-shm"]);
        assert_eq!(store.walk(), TreeWalk { bytes: 1300, entries: 2 });
        let log = TrackedDir::with_companions("airc-daemon-log", family.path().join("airc-daemon.log"), &[".1"]);
        assert_eq!(log.walk(), TreeWalk { bytes: 400, entries: 1 }, "the predecessor counts with no live log");
        #[cfg(unix)]
        assert_eq!(walk_tree(&tmp.path().join("link.bin")), TreeWalk::default());
    }

    // what this catches (the M5, 2026-09-19): a 2.44 M-entry `citizens` tree walked
    // in 90.7 s every 300 s — a 30% duty cycle of one core, forever. The bound is a
    // duty cycle: a walk is repeated no sooner than 20× its own cost, and a cheap
    // walk keeps the 5-minute floor. Mutation check: dropping the `max` fails the
    // first assert; dropping the multiply fails the second.
    #[test]
    fn a_walks_cost_paces_the_next_walk_to_a_five_percent_duty_cycle() {
        assert_eq!(next_scan_delay(Duration::from_secs(2)), SCAN_INTERVAL);
        assert_eq!(next_scan_delay(Duration::from_millis(90_700)), Duration::from_millis(1_814_000));
        assert_eq!(next_scan_delay(Duration::ZERO), SCAN_INTERVAL);
    }

    // what this catches: the scanner's tick re-walking an expensive tree on its
    // next 5-minute tick anyway — the pacing must live on the TrackedDir the tick
    // consults, not only in a constant. A dir is due before any walk, not due
    // 300 s after a 90 s walk, due again once its 30 min have passed; a cheap
    // walk is due again at the floor. The report carries the cost.
    #[test]
    fn a_tracked_dir_whose_walk_was_expensive_is_not_walked_on_the_next_tick() {
        let dir = TrackedDir::new("citizens", PathBuf::from("/nonexistent"));
        assert!(dir.due(0), "never walked = due now");
        let now = 1_000_000;
        dir.record_walk(TreeWalk { bytes: 95_000_000_000, entries: 2_440_220 }, Duration::from_secs(90), now);
        assert!(!dir.due(now + 300_000), "one tick later: the 90 s walk bought 30 min");
        assert!(!dir.due(now + 1_799_999));
        assert!(dir.due(now + 1_800_000));
        assert_eq!(dir.walk_cost(), Some((2_440_220, Duration::from_secs(90))));
        let detail = dir.report().detail;
        assert!(detail.contains("90.0 s") && detail.contains("2440220 entries") && detail.contains("1800 s"), "{detail}");

        let cheap = TrackedDir::new("cargo-target", PathBuf::from("/nonexistent"));
        cheap.record_walk(TreeWalk { bytes: 1, entries: 1 }, Duration::from_secs(2), now);
        assert!(!cheap.due(now + 299_999));
        assert!(cheap.due(now + 300_000), "a cheap walk keeps the 5-minute floor");
    }

    // what this catches: the standard cache-class registry names exactly
    // the directories from the 2026-07-13 incident — losing one from this
    // list silently un-names a known creep source.
    #[test]
    fn standard_dirs_cover_the_incident_cache_classes() {
        let dirs = standard_tracked_dirs(std::path::Path::new("/home/u"), std::path::Path::new("/home/u/.continuum"));
        let names: Vec<&str> = dirs.iter().map(|d| d.name).collect();
        for must in [
            "cargo-target",
            "genome-models",
            "hf-hub",
            "citizens",
            "forge",
        ] {
            assert!(names.contains(&must), "missing cache class: {must}");
        }
    }

    // what this catches (card 4dedb177, plan line 7, Codex's review of #5368): every continuum
    // cache class was a hand-built `~/.continuum/...`, so a relocated `$CONTINUUM_HOME` left the
    // tracker measuring a home nothing uses; and a tracker moved without its writer would hide
    // real downloads. The artifact writer's root IS a tracked root; the other continuum classes
    // follow the continuum home; airc stays under the user home; bytes already in the historical
    // artifact root stay tracked (once) while they exist, and only that class remains there.
    #[test]
    fn a_relocated_continuum_home_tracks_the_writers_roots_and_the_legacy_bytes() {
        let home = tempfile::tempdir().expect("home");
        let continuum = tempfile::tempdir().expect("continuum home");
        let _home = crate::paths::test_home::HomeGuard::set_blocking(home.path());
        let at = |dirs: &[Arc<TrackedDir>], class: &str| dirs.iter().find(|d| d.class() == class).map(|d| d.path().to_path_buf());

        let dirs = standard_tracked_dirs(home.path(), continuum.path());
        let writer = crate::paths::artifacts::root_under(continuum.path()).expect("writer root");
        assert!(dirs.iter().any(|d| d.path() == writer.as_path()), "the writer's root is tracked");
        assert_eq!(at(&dirs, "kv-pages"), Some(continuum.path().join("cache/kv-pages")));
        assert_eq!(at(&dirs, "genome-models"), Some(continuum.path().join("genome/models")));
        assert_eq!(at(&dirs, "airc-worktrees"), Some(home.path().join(".airc/worktrees")), "airc keeps its own home");
        assert_eq!(at(&dirs, "core-artifacts-legacy"), None, "no historical bytes, no row");

        let legacy = crate::paths::artifacts::legacy_root(home.path());
        std::fs::create_dir_all(&legacy).expect("historical artifacts");
        let dirs = standard_tracked_dirs(home.path(), continuum.path());
        assert_eq!(at(&dirs, "core-artifacts-legacy"), Some(legacy.clone()), "bytes already written stay visible");
        let old: Vec<&str> = dirs.iter().filter(|d| d.path().starts_with(home.path().join(".continuum"))).map(|d| d.class()).collect();
        assert_eq!(old, ["core-artifacts-legacy"], "only the historical artifacts remain there");
        let default = standard_tracked_dirs(home.path(), &home.path().join(".continuum"));
        assert_eq!(default.iter().filter(|d| d.path() == legacy.as_path()).count(), 1, "default homes: counted once");
    }
}
