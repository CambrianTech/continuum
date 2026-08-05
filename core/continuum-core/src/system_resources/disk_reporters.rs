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
//! 5 minutes (disk grows slowly; the cadence ladder says lean slower).
//! One scanner for all paths, not a task per reporter — one concurrent
//! concern, one task (CONCURRENCY-STYLE-GUIDE: no parallel managers).

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

use crate::runtime::{spawn_daemon, Daemon, DaemonChannel};

use super::disk_pressure::{DiskPathReport, DiskReporter};

/// One tracked cache-class directory: identity + cached recursive size.
/// The reporter half reads `bytes` lock-free; the scanner half refreshes it.
pub struct TrackedDir {
    name: &'static str,
    path: PathBuf,
    bytes: AtomicU64,
    /// Whether the scanner has completed at least one walk — before that,
    /// the reporter labels the value as pending instead of claiming "0 B"
    /// (an honest void, never a false zero).
    scanned: std::sync::atomic::AtomicBool,
}

impl TrackedDir {
    pub fn new(name: &'static str, path: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            name,
            path,
            bytes: AtomicU64::new(0),
            scanned: std::sync::atomic::AtomicBool::new(false),
        })
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    /// Cached recursive size — lock-free. Shared view for the reporter
    /// AND any eviction pool built over the same class (one measurement,
    /// two consumers — never two walkers disagreeing about one dir).
    pub fn bytes(&self) -> u64 {
        self.bytes.load(Ordering::Relaxed)
    }

    /// The scanner's write seam (also the test seam for pools built on
    /// this measurement). Marks the dir scanned — a set value is a real
    /// value, never "pending".
    pub(crate) fn set_bytes(&self, bytes: u64) {
        self.bytes.store(bytes, Ordering::Relaxed);
        self.scanned.store(true, Ordering::Relaxed);
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

/// Process-wide registry of the tracked cache classes, set ONCE at boot by
/// [`install_tracked_dirs`]. Exists so later boot phases (the IPC thread's
/// broker block registering eviction pools) can reach the SAME TrackedDir
/// instances the reporters use without threading a param through every
/// server-start signature. Same singleton pattern as the rest of the boot
/// sequence's shared services.
static TRACKED_DIRS: std::sync::OnceLock<Vec<Arc<TrackedDir>>> = std::sync::OnceLock::new();

/// Install the boot-time tracked-dir set. Second call is a boot-sequence
/// bug — fail loud, never silently keep two measurement sets.
pub fn install_tracked_dirs(dirs: Vec<Arc<TrackedDir>>) {
    if TRACKED_DIRS.set(dirs).is_err() {
        panic!("install_tracked_dirs called twice — one measurement registry per process");
    }
}

/// Fetch a tracked cache class by name, e.g. `"cargo-target"`. `None`
/// before boot installs the registry (tests, tools) — callers treat that
/// as "class not under management," never a default path guess.
pub fn tracked_dir(name: &str) -> Option<Arc<TrackedDir>> {
    TRACKED_DIRS
        .get()?
        .iter()
        .find(|d| d.name == name)
        .cloned()
}

impl DiskReporter for TrackedDir {
    fn name(&self) -> &'static str {
        self.name
    }

    fn report(&self) -> DiskPathReport {
        let scanned = self.scanned.load(Ordering::Relaxed);
        DiskPathReport {
            name: self.name.to_string(),
            path: self.path.clone(),
            bytes: self.bytes.load(Ordering::Relaxed),
            detail: if scanned {
                "cached recursive size (5 min scanner)".to_string()
            } else {
                "first scan pending".to_string()
            },
        }
    }
}

/// The substrate's known cache classes, rooted under `home` (normally the
/// user's home directory). These are exactly the directories that produced
/// the 2026-07-13 creep, each of which needs an eviction owner (wire 2):
///
/// | class | grows by | owner (planned) |
/// |---|---|---|
/// | cargo-target | every cargo build/test | age-based sweeper |
/// | genome-models | model downloads/forges | model-store LRU |
/// | hf-hub | HF downloads | hub LRU |
/// | citizens | persona workspaces/stores | WorkspaceResolver CoW fix |
/// | forge | export intermediates | export trimmer |
///
/// Paths that the operator can RELOCATE are resolved through the same authority the rest of the
/// substrate uses, never re-derived from `home`. A tracked dir that points somewhere the artifacts
/// are not is worse than no tracking at all: the scanner reports a reassuring 0 bytes, the class
/// looks governed, and the real bytes accumulate unwatched and unowned by any eviction pool —
/// which is precisely the 2026-07-13 shape this registry exists to prevent, one level up.
///
/// MEASURED on BigMama 2026-08-05: `hf-hub` tracked `~/.cache/huggingface` (0 bytes) while the
/// cold-storage installer had pointed `HF_HOME` at `D:\continuum-cold\huggingface` holding
/// **2.48 TB** — Kimi-K3 at 1.45 TB, Kimi-K2.7 at 430 GB, GLM-5.2 at 375 GB. Entirely invisible to
/// the disk governor.
pub fn standard_tracked_dirs(home: &std::path::Path) -> Vec<Arc<TrackedDir>> {
    // The HF cache is relocatable (HF_HOME / config.env, set by the cold-storage installer), so ask
    // the resolver instead of assuming the default. `huggingface_cache_root()` returns `<root>/hub`;
    // track the PARENT so the class covers the whole cache (blobs, snapshots, locks, refs), which is
    // what actually consumes the volume.
    // NOT `.unwrap_or_else(|| home.join(".cache/huggingface"))`. That fallback is a FABRICATION in
    // the sense swallow-audit.py means: it substitutes a plausible path for an unknown one, and the
    // scanner then reports a confident 0 bytes for a class whose real bytes are elsewhere. That is
    // the exact defect this function was changed to fix — 2.48 TB invisible because the tracked path
    // was not the real one — so silently re-introducing it as the failure branch would be the same
    // bug wearing a fallback's clothes.
    //
    // If the resolver cannot answer, the honest outcome is to say so and NOT claim the class is
    // tracked. A missing hf-hub row is visibly missing; a wrong one looks governed.
    let hf_root = match crate::model_registry::artifacts::huggingface_cache_root()
        .and_then(|hub| hub.parent().map(|p| p.to_path_buf()))
    {
        Some(root) => Some(root),
        None => {
            tracing::warn!(
                probe_class = "disk.tracked_dirs",
                "hf-hub cache root could not be resolved (no HF_HOME, no config.env entry, no home \
                 dir) — the hf-hub class is NOT being tracked this boot. Not defaulting to \
                 ~/.cache/huggingface: a tracked dir pointing at the wrong place reports a \
                 reassuring zero and is worse than an absent one."
            );
            None
        }
    };

    let mut dirs = vec![
        TrackedDir::new("cargo-target", home.join(".continuum/cache/cargo-target")),
        TrackedDir::new("genome-models", home.join(".continuum/genome/models")),
        TrackedDir::new("citizens", home.join(".continuum/citizens")),
        TrackedDir::new("forge", home.join(".continuum/forge")),
    ];
    // Present only when its real location is KNOWN (see the warn above).
    if let Some(root) = hf_root {
        dirs.push(TrackedDir::new("hf-hub", root));
    }
    dirs
}

/// Recursive size of `path` in bytes. Symlinks are NOT followed (a
/// persona workspace symlinking shared models must not double-count
/// them into its class). Unreadable entries are skipped, not fatal —
/// a half-measured tree is still a truthful lower bound.
pub(crate) fn dir_size_bytes(path: &std::path::Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    let mut total = 0u64;
    for entry in entries.flatten() {
        let Ok(meta) = entry.metadata() else { continue };
        if meta.is_symlink() {
            continue;
        }
        if meta.is_dir() {
            total = total.saturating_add(dir_size_bytes(&entry.path()));
        } else {
            total = total.saturating_add(meta.len());
        }
    }
    total
}

/// Scanner cadence — 5 minutes. A full walk of a 300 GB cargo-target is
/// seconds of blocking-pool work; every 5 min it is noise, and disk-class
/// sizes move on build/download timescales, not milliseconds.
const SCAN_INTERVAL: Duration = Duration::from_secs(300);

/// Refreshes every [`TrackedDir`]'s cached size on its own tick, off the
/// hot path via `spawn_blocking`. Publishes a unit snapshot (the real
/// output is the atomics the reporters read); channel exists to satisfy
/// the canonical daemon seam and give tests a tick-completed edge.
pub struct DiskUsageScanner {
    dirs: Vec<Arc<TrackedDir>>,
    channel: DaemonChannel<u64>,
    ticks: AtomicU64,
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
        for dir in &self.dirs {
            let path = dir.path.clone();
            // Walk on the blocking pool — a deep tree must never stall
            // the async runtime the substrate's minds run on.
            let bytes = tokio::task::spawn_blocking(move || dir_size_bytes(&path))
                .await
                .unwrap_or(0);
            dir.set_bytes(bytes);
        }
        let n = self.ticks.fetch_add(1, Ordering::Relaxed) + 1;
        self.channel.publish(n);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        // SAFETY: single-threaded test; the var is restored below.
        let prev = std::env::var("HF_HOME").ok();
        unsafe { std::env::set_var("HF_HOME", relocated.path()) };

        let dirs = standard_tracked_dirs(home.path());
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

        match prev {
            Some(v) => unsafe { std::env::set_var("HF_HOME", v) },
            None => unsafe { std::env::remove_var("HF_HOME") },
        }
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
    }

    // what this catches: the standard cache-class registry names exactly
    // the directories from the 2026-07-13 incident — losing one from this
    // list silently un-names a known creep source.
    #[test]
    fn standard_dirs_cover_the_incident_cache_classes() {
        let dirs = standard_tracked_dirs(std::path::Path::new("/home/u"));
        let names: Vec<&str> = dirs.iter().map(|d| d.name).collect();
        for must in ["cargo-target", "genome-models", "hf-hub", "citizens", "forge"] {
            assert!(names.contains(&must), "missing cache class: {must}");
        }
    }
}
