//! Concrete disk eviction pools — wire (2) of task #155.
//!
//! The 2026-07-13 incident, part two: the eviction ECONOMY was already
//! live (PressureBrokerModule ticks `relieve()` every 5 s at boot, and
//! `DiskPressureMonitor` registers as a signal-only pool), but no pool
//! that OWNS deletable disk content was ever registered — so the broker
//! spent the incident emitting the designed "disk hot AND nobody owns
//! the eviction" zero-byte alerts while cargo-target grew to 363 GB.
//! On an operator's machine that's an inconvenience; on a public user's
//! machine it trashes their system. This module is the first concrete
//! owner; each cache class from `standard_tracked_dirs` gets one over
//! time (genome-models and hf-hub need reference-aware LRU and come
//! later; cargo-target is pure derived artifact and comes first).
//!
//! ## Safety invariants (each pinned by a test)
//!
//! 1. **Never race a live build.** Eviction takes non-blocking exclusive
//!    flocks on cargo's own lock files (`<root>/.cargo-lock`,
//!    `<root>/debug/.cargo-lock`) before touching anything; any lock we
//!    can't get means a build is in flight → free 0 bytes and let the
//!    broker retry next tick.
//! 2. **Never leave the root.** The eviction ladder is a fixed list of
//!    `root.join(...)` subpaths — no pattern matching, no following
//!    symlinks out of the tree.
//! 3. **Derived artifacts only.** The ladder deletes incremental state,
//!    test binaries, then the debug tree — all reproducible by the next
//!    `cargo build`. `release/` is never touched here (it is small and
//!    holds the binaries ops copies into `~/.continuum/bin`).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use crate::paging::pool::{ResourcePool, ResourcePoolEntry};

use super::disk_reporters::{dir_size_bytes, TrackedDir};

/// Default cargo-target budget: 50 GiB. Generous for a warm dev cache,
/// an order of magnitude under the 363 GB the unswept cache reached.
/// Plumbing this through config.env's single owner is part of the
/// de-hardcode audit (task #124); the constant is the safe default a
/// public user gets with zero configuration.
pub const DEFAULT_CARGO_TARGET_BUDGET_BYTES: u64 = 50 * 1024 * 1024 * 1024;

/// Non-blocking exclusive flock on `path`. `Some(file)` holds the lock
/// until dropped; `None` = someone else (a live cargo build) holds it.
/// A missing lock file is created — holding it makes a cargo invocation
/// that starts mid-eviction block until we finish, instead of racing us.
fn try_exclusive_flock(path: &Path) -> Option<std::fs::File> {
    // Cross-platform advisory file lock via `fs2` (Unix: flock; Windows:
    // LockFileEx) — ONE code path on every platform. The lock is held until the
    // returned `File` is dropped, matching the previous flock-until-drop
    // semantics. `try_lock_exclusive` returns Err when another process (a live
    // cargo build) holds it → `None`.
    use fs2::FileExt;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .ok()?;
    file.try_lock_exclusive().ok().map(|_| file)
}

/// Budget-capped eviction owner for the shared cargo-target cache.
/// Shares its [`TrackedDir`] with the disk reporter — one measurement,
/// two consumers. Pressure = cached usage / budget, so this pool goes
/// over-budget (and the broker acts) long before the whole disk is
/// critical — the cache is bounded by policy, not by the disk filling.
pub struct CargoTargetPool {
    tracked: Arc<TrackedDir>,
    budget_bytes: u64,
}

impl CargoTargetPool {
    pub fn new(tracked: Arc<TrackedDir>, budget_bytes: u64) -> Self {
        Self {
            tracked,
            budget_bytes: budget_bytes.max(1),
        }
    }

    /// The eviction ladder, cheapest-regret first. Each rung is fully
    /// reproducible by the next build; order matters — incremental
    /// state is the biggest win with the smallest rebuild cost.
    fn ladder(root: &Path) -> [PathBuf; 3] {
        [
            root.join("debug/incremental"),
            root.join("tests"),
            root.join("debug"),
        ]
    }
}

impl ResourcePool for CargoTargetPool {
    fn tier_name(&self) -> &str {
        "disk-cargo-target"
    }

    fn capacity_bytes(&self) -> u64 {
        self.budget_bytes
    }

    fn usage_bytes(&self) -> u64 {
        self.tracked.bytes()
    }

    fn evict_at_least(&self, want_bytes: u64) -> u64 {
        let root = self.tracked.path().to_path_buf();
        if !root.exists() {
            return 0;
        }
        // Safety invariant 1: hold cargo's lock files exclusively for the
        // whole eviction, or do nothing. Guards held in scope until return.
        let _root_lock = match try_exclusive_flock(&root.join(".cargo-lock")) {
            Some(lock) => lock,
            None => return 0,
        };
        let debug_lock_path = root.join("debug/.cargo-lock");
        let _debug_lock = if debug_lock_path.parent().is_some_and(Path::exists) {
            match try_exclusive_flock(&debug_lock_path) {
                Some(lock) => Some(lock),
                None => return 0,
            }
        } else {
            None
        };

        let mut freed = 0u64;
        for rung in Self::ladder(&root) {
            if freed >= want_bytes.max(1) {
                break;
            }
            if !rung.exists() {
                continue;
            }
            let size = dir_size_bytes(&rung);
            if std::fs::remove_dir_all(&rung).is_ok() {
                freed = freed.saturating_add(size);
            }
        }
        if freed > 0 {
            self.tracked.record_freed(freed);
            crate::clog_warn!(
                "💾 cargo-target eviction freed {} GB (budget {} GB) — derived artifacts only; next build recreates them",
                freed / (1024 * 1024 * 1024),
                self.budget_bytes / (1024 * 1024 * 1024)
            );
        }
        freed
    }

    fn snapshot(&self) -> Vec<ResourcePoolEntry> {
        Vec::new()
    }
}

/// Governed reserve on the serving-tier volume: the slice of the drive the
/// serving tier must never claim, so the OS + working set never starve
/// behind model artifacts. DERIVED from the volume (#287-style), never a
/// user-tuned knob: 10% of the volume, floored at 32 GiB (a small SSD
/// still keeps a real working margin; a 4 TB NVMe reserves 400 GB — the
/// tier is generous exactly where the drive is).
pub fn serving_tier_reserve_bytes(volume_total_bytes: u64) -> u64 {
    (volume_total_bytes / 10).max(32 * 1024 * 1024 * 1024)
}

/// The serving tier's byte budget on its volume: total − governed reserve.
/// Saturating: a volume smaller than the reserve yields 0 capacity (the
/// tier is not offered there — resolution degrades, never gates).
pub fn serving_tier_capacity_bytes(volume_total_bytes: u64) -> u64 {
    volume_total_bytes.saturating_sub(serving_tier_reserve_bytes(volume_total_bytes))
}

/// The set of artifact paths serving is ACTIVELY paging per-token (the
/// resident model's GGUF, its expert container dir, device-fit overrides).
/// The eviction pool consults this before every migration — the #302
/// safety invariant is that an actively-paged artifact is NEVER moved out
/// from under the engine. Serving's reconcile (her half's
/// `ensure_hot_resident`) registers paths on spawn and releases on lane
/// teardown; registration is path-prefix aware so marking a model DIR
/// protects everything inside it.
#[derive(Default)]
pub struct ActiveArtifactSet {
    paths: RwLock<HashSet<PathBuf>>,
}

impl ActiveArtifactSet {
    pub fn register(&self, path: PathBuf) {
        if let Ok(mut set) = self.paths.write() {
            set.insert(path);
        }
    }

    pub fn release(&self, path: &Path) {
        if let Ok(mut set) = self.paths.write() {
            set.remove(path);
        }
    }

    /// Is `candidate` (an eviction target) protected? True when any
    /// registered active path IS the candidate, lives UNDER it, or the
    /// candidate lives under a registered dir — prefix containment both
    /// directions, so neither "marked the dir, evicting a file inside"
    /// nor "marked the file, evicting its parent dir" can slip through.
    pub fn protects(&self, candidate: &Path) -> bool {
        let Ok(set) = self.paths.read() else {
            // A poisoned lock means a panic mid-update — fail SAFE:
            // treat everything as protected rather than migrate blind.
            return true;
        };
        set.iter()
            .any(|active| active.starts_with(candidate) || candidate.starts_with(active))
    }
}

/// Process-wide active-artifact registry, one per process (same singleton
/// shape as `install_tracked_dirs`). Serving marks residency here; the
/// broker-registered pool reads it. Lazily created so tests and tools get
/// a working (empty) set without boot wiring.
static SERVING_ACTIVE_ARTIFACTS: std::sync::OnceLock<Arc<ActiveArtifactSet>> =
    std::sync::OnceLock::new();

pub fn serving_active_artifacts() -> Arc<ActiveArtifactSet> {
    SERVING_ACTIVE_ARTIFACTS
        .get_or_init(|| Arc::new(ActiveArtifactSet::default()))
        .clone()
}

/// NVMe serving-tier eviction owner (#302) — the decided story for the
/// `genome-models` cache class. The class holds the HOT serving set:
/// served GGUFs, expert containers, device-fit overrides — the artifacts
/// the engine pages per-token, which is why this pool MIGRATES to the
/// COLD/frozen drive instead of deleting (a model is hours of download /
/// forge work, not a derived artifact like cargo-target), and why it
/// refuses to touch anything in [`ActiveArtifactSet`].
///
/// ## Safety invariants (each pinned by a test)
///
/// 1. **Never the actively-paged artifact.** Anything `protects()` says
///    serving holds is skipped, even if it is the coldest entry.
/// 2. **Migrate, never blind-delete.** An entry leaves the hot tier only
///    after a byte-verified copy exists on the cold drive (copy → fsync →
///    verify → delete). A digest-verified twin already on cold = pure
///    drop of the hot copy. Verify failure removes the partial COPY,
///    never the source.
/// 3. **No cold drive ⇒ free nothing.** With nowhere safe to migrate,
///    the pool reports 0 and logs loudly — pressure stays visible to the
///    broker/operator instead of being "relieved" by destroying models.
///    (Composes with device-fit one tier down: Unfittable routes to the
///    grid, LOUD — never a silent HDD stream. See
///    docs/architecture/STORAGE-SERVING-TIER-GOVERNOR.md.)
pub struct NvmeServingTierPool {
    tracked: Arc<TrackedDir>,
    capacity_bytes: u64,
    cold_root: Option<PathBuf>,
    active: Arc<ActiveArtifactSet>,
}

impl NvmeServingTierPool {
    /// `volume_total_bytes` is the TOTAL size of the volume holding the
    /// hot tier (capacity derives from it — never a hand-tuned budget).
    /// `cold_root` is the migration target directory on the COLD drive
    /// (`None` ⇒ this box has no cold tier; eviction refuses, invariant 3).
    pub fn new(
        tracked: Arc<TrackedDir>,
        volume_total_bytes: u64,
        cold_root: Option<PathBuf>,
        active: Arc<ActiveArtifactSet>,
    ) -> Self {
        Self {
            tracked,
            capacity_bytes: serving_tier_capacity_bytes(volume_total_bytes).max(1),
            cold_root,
            active,
        }
    }

    /// Eviction candidates: top-level entries of the hot root (a served
    /// GGUF file or a per-model directory), coldest-first by mtime.
    /// Artifact granularity is the top-level entry — a model's dir moves
    /// as a unit, never half its files.
    fn candidates_coldest_first(root: &Path) -> Vec<(PathBuf, u64)> {
        let Ok(entries) = std::fs::read_dir(root) else {
            return Vec::new();
        };
        let mut list: Vec<(PathBuf, u64, std::time::SystemTime)> = entries
            .flatten()
            .filter_map(|e| {
                let meta = e.metadata().ok()?;
                if meta.is_symlink() {
                    return None;
                }
                let path = e.path();
                let size = if meta.is_dir() {
                    dir_size_bytes(&path)
                } else {
                    meta.len()
                };
                let mtime = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
                Some((path, size, mtime))
            })
            .collect();
        list.sort_by_key(|(_, _, mtime)| *mtime);
        list.into_iter().map(|(p, s, _)| (p, s)).collect()
    }
}

/// Byte-level equality of two files, streamed — the "digest-verified"
/// primitive without a hash dependency (a full byte compare is strictly
/// as strong as comparing digests of both sides).
fn files_identical(a: &Path, b: &Path) -> bool {
    use std::io::Read;
    let (Ok(ma), Ok(mb)) = (std::fs::metadata(a), std::fs::metadata(b)) else {
        return false;
    };
    if ma.len() != mb.len() {
        return false;
    }
    let (Ok(fa), Ok(fb)) = (std::fs::File::open(a), std::fs::File::open(b)) else {
        return false;
    };
    let mut ra = std::io::BufReader::new(fa);
    let mut rb = std::io::BufReader::new(fb);
    let mut ba = [0u8; 64 * 1024];
    let mut bb = [0u8; 64 * 1024];
    loop {
        let na = match ra.read(&mut ba) {
            Ok(n) => n,
            Err(_) => return false,
        };
        let nb = match rb.read(&mut bb) {
            Ok(n) => n,
            Err(_) => return false,
        };
        if na != nb || ba[..na] != bb[..nb] {
            return false;
        }
        if na == 0 {
            return true;
        }
    }
}

/// Recursive equality: files byte-compare; dirs compare entry sets then
/// recurse. Any unreadable piece = NOT identical (never "verified" on a
/// guess).
fn entries_identical(a: &Path, b: &Path) -> bool {
    let (Ok(ma), Ok(mb)) = (std::fs::metadata(a), std::fs::metadata(b)) else {
        return false;
    };
    match (ma.is_dir(), mb.is_dir()) {
        (false, false) => files_identical(a, b),
        (true, true) => {
            let Ok(entries) = std::fs::read_dir(a) else {
                return false;
            };
            let names_a: Vec<std::ffi::OsString> =
                entries.flatten().map(|e| e.file_name()).collect();
            let Ok(entries_b) = std::fs::read_dir(b) else {
                return false;
            };
            let names_b: HashSet<std::ffi::OsString> =
                entries_b.flatten().map(|e| e.file_name()).collect();
            names_a.len() == names_b.len()
                && names_a
                    .iter()
                    .all(|n| names_b.contains(n) && entries_identical(&a.join(n), &b.join(n)))
        }
        _ => false,
    }
}

/// Copy `src` (file or tree) to `dst`, fsyncing every file — the durable
/// half of migrate-then-delete. Cross-device safe (plain read/write copy,
/// no rename tricks).
fn copy_entry_durable(src: &Path, dst: &Path) -> std::io::Result<()> {
    let meta = std::fs::metadata(src)?;
    if meta.is_dir() {
        std::fs::create_dir_all(dst)?;
        for entry in std::fs::read_dir(src)?.flatten() {
            copy_entry_durable(&entry.path(), &dst.join(entry.file_name()))?;
        }
    } else {
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(src, dst)?;
        // Reopen for WRITE to fsync. `File::open` yields a read-only
        // handle, and on Windows `FlushFileBuffers` against one fails
        // with ERROR_ACCESS_DENIED — so every real migration errored
        // here, the caller deleted the partial cold copy, kept the hot
        // copy, and logged "failed verification". Net effect on Windows:
        // the serving tier could only ever evict an artifact that ALREADY
        // had a verified twin on cold (the pure-drop path, which never
        // calls this function). Under genuine disk pressure the pool
        // freed nothing while reporting that it tried. POSIX permits
        // fsync on a read-only fd, which is why this survived on
        // macOS/Linux — [[dir-opened-as-file-windows-only]] is the same
        // family: a file-API assumption that only one platform enforces.
        std::fs::OpenOptions::new()
            .write(true)
            .open(dst)?
            .sync_all()?;
    }
    Ok(())
}

fn remove_entry(path: &Path) -> std::io::Result<()> {
    if std::fs::metadata(path)?.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    }
}

impl ResourcePool for NvmeServingTierPool {
    fn tier_name(&self) -> &str {
        "disk-serving-tier"
    }

    fn capacity_bytes(&self) -> u64 {
        self.capacity_bytes
    }

    fn usage_bytes(&self) -> u64 {
        self.tracked.bytes()
    }

    fn evict_at_least(&self, want_bytes: u64) -> u64 {
        let root = self.tracked.path().to_path_buf();
        if !root.exists() {
            return 0;
        }
        // Invariant 3: no cold tier ⇒ refuse loudly, never blind-delete.
        let Some(cold_root) = self.cold_root.as_deref() else {
            crate::clog_warn!(
                "💾 serving-tier over budget but this box has NO cold drive — refusing to \
                 delete model artifacts; resolve by adding a cold tier or dropping models \
                 explicitly (models are re-fetch-hours, not derived artifacts)"
            );
            return 0;
        };

        let mut freed = 0u64;
        for (path, size) in Self::candidates_coldest_first(&root) {
            if freed >= want_bytes.max(1) {
                break;
            }
            // Invariant 1: never the actively-paged artifact.
            if self.active.protects(&path) {
                continue;
            }
            let Some(name) = path.file_name() else {
                continue;
            };
            let dest = cold_root.join(name);

            if dest.exists() {
                if entries_identical(&path, &dest) {
                    // Verified twin already frozen on cold: pure drop.
                    if remove_entry(&path).is_ok() {
                        freed = freed.saturating_add(size);
                    }
                } else {
                    // Name collision with DIFFERENT content — never
                    // clobber a cold artifact; skip and say so.
                    crate::clog_warn!(
                        "💾 serving-tier migrate skipped {:?}: cold copy exists with \
                         different content — refusing to overwrite",
                        name
                    );
                }
                continue;
            }

            // Invariant 2: copy → fsync → verify → delete source.
            if std::fs::create_dir_all(cold_root).is_err() {
                crate::clog_warn!(
                    "💾 serving-tier migrate failed: cannot create cold root {:?}",
                    cold_root
                );
                break;
            }
            match copy_entry_durable(&path, &dest) {
                Ok(()) if entries_identical(&path, &dest) => {
                    if remove_entry(&path).is_ok() {
                        freed = freed.saturating_add(size);
                    }
                }
                _ => {
                    // Verify failed or copy errored: remove the PARTIAL
                    // COPY, never the source.
                    let _ = remove_entry(&dest);
                    crate::clog_warn!(
                        "💾 serving-tier migrate of {:?} failed verification — hot copy \
                         kept, partial cold copy removed",
                        name
                    );
                }
            }
        }
        if freed > 0 {
            self.tracked.record_freed(freed);
            crate::clog_warn!(
                "💾 serving-tier migrated {} GB of frozen artifacts to cold storage \
                 (hot capacity {} GB) — nothing deleted without a verified cold copy",
                freed / (1024 * 1024 * 1024),
                self.capacity_bytes / (1024 * 1024 * 1024)
            );
        }
        freed
    }

    fn snapshot(&self) -> Vec<ResourcePoolEntry> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    fn seeded_target(tmp: &Path) -> Arc<TrackedDir> {
        std::fs::create_dir_all(tmp.join("debug/incremental")).expect("mkdir");
        std::fs::create_dir_all(tmp.join("tests")).expect("mkdir");
        std::fs::create_dir_all(tmp.join("release")).expect("mkdir");
        std::fs::write(tmp.join("debug/incremental/a.o"), vec![0u8; 4000]).expect("write");
        std::fs::write(tmp.join("debug/lib.rlib"), vec![0u8; 3000]).expect("write");
        std::fs::write(tmp.join("tests/t.bin"), vec![0u8; 2000]).expect("write");
        std::fs::write(tmp.join("release/keep.bin"), vec![0u8; 1000]).expect("write");
        let tracked = TrackedDir::new("cargo-target", tmp.to_path_buf());
        // Seed the cached size the way the scanner would.
        tracked.set_bytes(dir_size_bytes(tmp));
        tracked
    }

    // what this catches: the eviction ladder frees cheapest-regret first
    // (incremental before the whole debug tree), stops once `want` is met,
    // NEVER touches release/, and decrements the shared TrackedDir so the
    // broker doesn't re-fire against space already freed.
    #[test]
    fn ladder_frees_in_order_and_never_touches_release() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let tracked = seeded_target(tmp.path());
        let usage_before = tracked.bytes();
        let pool = CargoTargetPool::new(tracked.clone(), 1);

        // Small want: the incremental rung (4000 B) alone satisfies it.
        let freed = pool.evict_at_least(1000);
        assert_eq!(freed, 4000, "incremental rung is the first and only cut");
        assert!(!tmp.path().join("debug/incremental").exists());
        assert!(tmp.path().join("tests").exists(), "later rung untouched");
        assert!(tmp.path().join("release/keep.bin").exists());
        assert_eq!(
            tracked.bytes(),
            usage_before - 4000,
            "shared measurement reflects the delete immediately"
        );

        // Large want: remaining rungs go (tests 2000 + debug 3000);
        // release survives regardless.
        let freed = pool.evict_at_least(u64::MAX);
        assert_eq!(freed, 5000);
        assert!(!tmp.path().join("tests").exists());
        assert!(!tmp.path().join("debug").exists());
        assert!(tmp.path().join("release/keep.bin").exists());
    }

    // what this catches: safety invariant 1 — a held cargo lock (a build
    // in flight) makes eviction a no-op. Deleting deps out from under a
    // running cargo corrupts the build AND can race half-written files.
    #[test]
    fn held_cargo_lock_blocks_eviction_entirely() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let tracked = seeded_target(tmp.path());
        let pool = CargoTargetPool::new(tracked, 1);

        let _build_holds_lock = try_exclusive_flock(&tmp.path().join(".cargo-lock"))
            .expect("test takes the lock first");
        assert_eq!(pool.evict_at_least(u64::MAX), 0, "locked ⇒ untouched");
        assert!(tmp.path().join("debug/incremental").exists());
    }

    // what this catches: pressure is usage/BUDGET (policy-bounded), not
    // usage/disk — the pool must go over-budget and trigger the broker
    // while the disk itself is still healthy. An unscanned dir reads as
    // 0 pressure (not under management yet), never NaN.
    #[test]
    fn pressure_is_budget_relative() {
        let tracked = TrackedDir::new("cargo-target", PathBuf::from("/tmp/none"));
        let pool = CargoTargetPool::new(tracked.clone(), 50_000);
        assert_eq!(pool.pressure(), 0.0, "unscanned = not under management");

        tracked.set_bytes(75_000); // scanner measured 1.5× the budget
        assert!(pool.pressure() > 1.4 && pool.pressure() < 1.6);
        assert_eq!(pool.capacity_bytes(), 50_000);
    }

    // what this catches: TrackedDir.record_freed saturates at zero — an
    // over-reported eviction must never wrap the cached size to u64::MAX
    // (which would read as 17 exabytes of pressure and evict forever).
    #[test]
    fn record_freed_saturates() {
        let tracked = TrackedDir::new("cargo-target", PathBuf::from("/tmp/none"));
        tracked.set_bytes(5_000);
        tracked.record_freed(10_000);
        assert_eq!(tracked.bytes(), 0);
    }

    // what this catches: THE CHAIN, not the components. The 2026-07-13
    // incident happened with every unit green — the monitor ran with no
    // reporters and the broker ran with no eviction owner, so 460 GB
    // accumulated while every individual piece "worked." This test runs
    // the real PressureBroker against a real over-budget CargoTargetPool
    // on a real temp tree and asserts bytes actually leave the disk when
    // relieve() fires. If any link (pressure math, tier thresholds,
    // broker act_above, pool registration shape, eviction ladder)
    // regresses, this fails — the health component Joel called critical
    // is guarded end-to-end, not piecewise.
    #[test]
    fn broker_relieve_actually_deletes_from_an_over_budget_pool() {
        use crate::paging::{BrokerConfig, PressureBroker};

        let tmp = tempfile::tempdir().expect("tempdir");
        let tracked = seeded_target(tmp.path());
        // Budget below measured usage → pool pressure > 1.0 → Critical.
        let budget = tracked.bytes() / 2;
        let pool = Arc::new(CargoTargetPool::new(tracked.clone(), budget));

        let broker = PressureBroker::new(BrokerConfig::default());
        broker.register(pool as Arc<dyn ResourcePool>);

        let disk_before = dir_size_bytes(tmp.path());
        let report = broker.relieve();

        assert!(report.triggered, "over-budget pool must trigger relief");
        assert!(report.bytes_freed > 0, "relief must free real bytes");
        assert!(
            dir_size_bytes(tmp.path()) < disk_before,
            "bytes must actually leave the disk, not just the accounting"
        );
        assert!(
            tmp.path().join("release/keep.bin").exists(),
            "release survives even broker-driven eviction"
        );
    }

    // what this catches: an ownerless cache class — the exact shape of
    // the incident. Every class in standard_tracked_dirs must have a
    // DECIDED eviction story: either an owner pool exists, or the class
    // is explicitly listed here as deferred with the task that owns it.
    // Adding a sixth cache class without deciding makes this fail at
    // compile-adjacent cost instead of at a user's full disk.
    #[test]
    fn every_cache_class_has_a_decided_eviction_story() {
        // genome-models graduated from deferred to OWNED (#302): the
        // NvmeServingTierPool is exactly the "reference-aware, never
        // blind-delete a served model" owner the deferred entry demanded —
        // active-set aware, migrate-to-cold, verified-copy-before-delete.
        // logs + probes graduated straight to OWNED on registration
        // (2026-08-06) rather than through the deferred list: their
        // contents are rotation generations the writer has already
        // moved past, so `RotationLogPool` can drop the oldest with no
        // in-flight-set hazard to reason about. Deferring is for
        // classes where blind deletion is UNSAFE (a grading instance,
        // a served model); there is nothing unsafe here to defer for.
        let owned = ["cargo-target", "genome-models", "logs", "probes"];
        let deferred = [
            ("hf-hub", "#155: hub LRU keyed on last-access — downloads are re-fetchable"),
            (
                "eval-captures",
                "#155: age-based sweep — every file is a re-creatable diagnostic (kv-diag \
                 snapshots, wire-request jsonl from SERVING_WIRE_CAPTURE_DIR); writers are \
                 opt-in and quiet by default, so the class grows only while an operator is \
                 actively hunting. Owner when built: a capped appender like the log pool",
            ),
            ("citizens", "#155/#49: workspace CoW fix removes the bulk; stores are persona MEMORY, never auto-evicted"),
            // Sibling of `citizens` and inherits its rule: a LIVE mind's longterm.db and
            // working-set.json are MEMORY, never auto-evicted. What IS evictable is the
            // GHOST sub-class — a dir whose uuid appears in no roster and which never
            // recorded a turn, left by the spawn name-pool (#437). Measured 2026-08-20:
            // 295 dirs, 286 of them under 100 KB, 2 real citizens. Small in bytes, which is
            // exactly why it went unnoticed for so long — the hazard here is not capacity,
            // it is that ghost identities pollute the roster and the demand ceiling. An
            // owner pool must key on "has this uuid ever completed a turn", never on size.
            (
                "personas",
                "#155/#437: per-persona MEMORY, never blind-LRU'd. Evictable sub-class is \
                 GHOST dirs only — no roster entry AND no recorded turn — which needs a pool \
                 that can ask the roster, not a size heuristic",
            ),
            ("forge", "#155: export trimmer — intermediates only, published artifacts stay"),
            // Registered the day benchmark/swe-* landed, BEFORE a sweep ran. Everything under
            // it is re-creatable — repo clones from git, venvs from uv, the dataset from HF —
            // so eviction is safe by construction; what it must never do is delete an instance
            // dir mid-grade, which is why it wants a pool that knows the in-flight set rather
            // than a blind LRU. Until then it is tracked and REPORTED, so the class can never
            // be the silent one again.
            (
                "benchmarks",
                "#155: LRU over per-instance dirs, skipping the in-flight set — clones/venvs \
                 are re-creatable from git+uv, so only an active grade is at risk. NOT \
                 everything under it is re-creatable: `swe/captures/run-*/attempt-N.patch` \
                 is a citizen's actual diff, deleted from her workspace the moment the next \
                 attempt resets it (#379). An eviction pool here must treat captures as \
                 EVIDENCE — small, and the only thing that can answer what she wrote — and \
                 reclaim the bulky re-creatable clones/venvs instead. Corrected 2026-08-18: \
                 this entry read \"everything under it is re-creatable\", which the 25 \
                 patches already sitting there had falsified since before it was written",
            ),
            // Steady-state owner ALREADY EXISTS in-file: RAII drop on every in-process
            // return path + the provision-time orphan sweep for worlds a killed process
            // leaves behind (an eval run cannot survive its process, so any non-live
            // sibling is debris; everything inside is a CoW clone of the checkout —
            // re-creatable by construction). Deferred only for the broker seam: under
            // real disk pressure the broker cannot yet claw these bytes back BETWEEN
            // provisions — that wants a pool that consults `live_eval_roots()`, never
            // a blind LRU that could delete a mid-exam world.
            (
                "eval-roots",
                "#155: broker-reachable pool over cognition/eval::live_eval_roots(); \
                 sweep + RAII already own the steady state and the crash path",
            ),
        ];
        use super::super::disk_pressure::DiskReporter as _;
        for dir in super::super::disk_reporters::standard_tracked_dirs(Path::new("/h")) {
            let name = dir.report().name;
            let decided =
                owned.contains(&name.as_str()) || deferred.iter().any(|(n, _)| *n == name);
            assert!(
                decided,
                "cache class '{name}' has NO eviction decision — register an owner pool or \
                 add it to the deferred list above with the task that owns it (task #155; \
                 the 2026-07-13 incident was exactly an ownerless class filling the disk)"
            );
        }
    }

    mod serving_tier {
        use super::*;

        /// Set an mtime on a path that may be a FILE or a DIRECTORY.
        ///
        /// Windows will not hand you a handle to a directory through a
        /// plain `File::open` — it returns `PermissionDenied` (code 5)
        /// — so you must ask for `FILE_WRITE_ATTRIBUTES` access with
        /// `FILE_FLAG_BACKUP_SEMANTICS`, the documented way to open a
        /// directory handle. Both work for regular files too, so one
        /// helper covers both shapes on both platforms.
        ///
        /// what this fixes: these three `serving_tier` tests were RED on
        /// Windows and green everywhere else, because the helper opened
        /// `stale-model/` (a directory) as a file. Exactly the defect
        /// that also refused the probe sink's boot the same day — a
        /// directory opened as a file is invisible on macOS/Linux and
        /// fatal on Windows, so it survives review on the platform the
        /// author is using.
        fn set_mtime(path: &Path, t: std::time::SystemTime) {
            #[cfg(windows)]
            let f = {
                use std::os::windows::fs::OpenOptionsExt;
                const FILE_WRITE_ATTRIBUTES: u32 = 0x0100;
                const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
                std::fs::OpenOptions::new()
                    .access_mode(FILE_WRITE_ATTRIBUTES)
                    .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
                    .open(path)
                    .expect("open for mtime")
            };
            #[cfg(not(windows))]
            let f = std::fs::File::open(path).expect("open for mtime");
            f.set_modified(t).expect("mtime");
        }

        /// A hot-tier tree with two frozen model artifacts (one dir-shaped,
        /// one file-shaped) and one actively-served dir. mtimes are staged so
        /// `stale-model/` is coldest, then `old.gguf`, then the active dir.
        fn seeded_hot_tier(hot: &Path) -> Arc<TrackedDir> {
            std::fs::create_dir_all(hot.join("stale-model")).expect("mkdir");
            std::fs::write(hot.join("stale-model/model.gguf"), vec![1u8; 4000]).expect("write");
            std::fs::write(hot.join("old.gguf"), vec![2u8; 3000]).expect("write");
            std::fs::create_dir_all(hot.join("served-model")).expect("mkdir");
            std::fs::write(hot.join("served-model/model.gguf"), vec![3u8; 2000]).expect("write");
            // Stage mtimes: coldest first. filetime not in deps — touch via
            // set_modified (std, stable since 1.75).
            let t0 = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000);
            let t1 = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(2_000);
            set_mtime(&hot.join("stale-model"), t0);
            set_mtime(&hot.join("old.gguf"), t1);
            let tracked = TrackedDir::new("genome-models", hot.to_path_buf());
            tracked.set_bytes(dir_size_bytes(hot));
            tracked
        }

        // what this catches: #302 safety invariant 1 — the actively-paged
        // artifact is NEVER migrated, even when eviction wants unlimited
        // bytes. Losing the resident model's GGUF mid-serve is the engine
        // reading from a deleted file.
        #[test]
        fn actively_paged_artifact_is_never_migrated() {
            let hot = tempfile::tempdir().expect("tempdir");
            let cold = tempfile::tempdir().expect("tempdir");
            let tracked = seeded_hot_tier(hot.path());
            let active = Arc::new(ActiveArtifactSet::default());
            active.register(hot.path().join("served-model"));
            let pool = NvmeServingTierPool::new(
                tracked,
                1_000_000,
                Some(cold.path().to_path_buf()),
                active,
            );

            pool.evict_at_least(u64::MAX);
            assert!(
                hot.path().join("served-model/model.gguf").exists(),
                "the served model must survive unlimited eviction demand"
            );
            assert!(
                !hot.path().join("stale-model").exists(),
                "frozen artifacts migrate"
            );
            assert!(!hot.path().join("old.gguf").exists());
        }

        // what this catches: #302 safety invariant 2 — migration is
        // copy→verify→delete (the artifact EXISTS on cold before the hot
        // copy dies), coldest-first, and a verified twin already on cold is
        // a pure drop (no second copy). The shared TrackedDir decrements so
        // the broker doesn't re-fire on freed space.
        #[test]
        fn migrates_coldest_first_and_pure_drops_verified_twins() {
            let hot = tempfile::tempdir().expect("tempdir");
            let cold = tempfile::tempdir().expect("tempdir");
            let tracked = seeded_hot_tier(hot.path());
            let usage_before = tracked.bytes();
            // old.gguf already has a byte-identical frozen twin on cold.
            std::fs::write(cold.path().join("old.gguf"), vec![2u8; 3000]).expect("write");
            let pool = NvmeServingTierPool::new(
                tracked.clone(),
                1_000_000,
                Some(cold.path().to_path_buf()),
                Arc::new(ActiveArtifactSet::default()),
            );

            // Want only the coldest entry's worth: stale-model (4000 B).
            let freed = pool.evict_at_least(1000);
            assert_eq!(freed, 4000, "coldest entry goes first");
            assert!(
                cold.path().join("stale-model/model.gguf").exists(),
                "verified cold copy exists"
            );
            assert!(
                !hot.path().join("stale-model").exists(),
                "hot copy gone after verify"
            );
            assert!(
                hot.path().join("old.gguf").exists(),
                "later candidate untouched"
            );
            assert_eq!(tracked.bytes(), usage_before - 4000);

            // Second round: old.gguf's twin is already frozen — pure drop.
            let freed = pool.evict_at_least(1000);
            assert_eq!(freed, 3000);
            assert!(!hot.path().join("old.gguf").exists());
            assert!(cold.path().join("old.gguf").exists());
        }

        // what this catches: #302 safety invariant 3 — a box with no cold
        // drive frees NOTHING (models are hours of re-fetch, not derived
        // artifacts; pressure must stay visible, never "relieved" by
        // destroying them). Also: a cold-side name collision with DIFFERENT
        // content is never clobbered.
        #[test]
        fn no_cold_drive_frees_nothing_and_collisions_never_clobber() {
            let hot = tempfile::tempdir().expect("tempdir");
            let tracked = seeded_hot_tier(hot.path());
            let pool = NvmeServingTierPool::new(
                tracked.clone(),
                1_000_000,
                None,
                Arc::new(ActiveArtifactSet::default()),
            );
            assert_eq!(pool.evict_at_least(u64::MAX), 0, "no cold tier ⇒ refuse");
            assert!(hot.path().join("stale-model/model.gguf").exists());

            // Collision case: cold has a DIFFERENT old.gguf.
            let cold = tempfile::tempdir().expect("tempdir");
            std::fs::write(cold.path().join("old.gguf"), vec![9u8; 3000]).expect("write");
            let pool = NvmeServingTierPool::new(
                tracked,
                1_000_000,
                Some(cold.path().to_path_buf()),
                Arc::new(ActiveArtifactSet::default()),
            );
            pool.evict_at_least(u64::MAX);
            assert!(
                hot.path().join("old.gguf").exists(),
                "collision with different content: hot copy kept"
            );
            let cold_bytes = std::fs::read(cold.path().join("old.gguf")).expect("read");
            assert_eq!(
                cold_bytes,
                vec![9u8; 3000],
                "cold artifact never overwritten"
            );
        }

        // what this catches: the capacity derivation (#287-style) — 10% of
        // the volume floored at 32 GiB, saturating to 0 on a volume smaller
        // than the reserve (the tier degrades, never underflows).
        #[test]
        fn capacity_is_volume_minus_derived_reserve() {
            const GIB: u64 = 1024 * 1024 * 1024;
            // 4 TB NVMe: reserve = 10% = 400 GB-ish (> 32 GiB floor).
            assert_eq!(serving_tier_reserve_bytes(4000 * GIB), 400 * GIB);
            assert_eq!(serving_tier_capacity_bytes(4000 * GIB), 3600 * GIB);
            // 100 GiB SSD: 10% = 10 GiB < floor ⇒ reserve is 32 GiB.
            assert_eq!(serving_tier_reserve_bytes(100 * GIB), 32 * GIB);
            assert_eq!(serving_tier_capacity_bytes(100 * GIB), 68 * GIB);
            // Tiny volume: capacity saturates to 0, never wraps.
            assert_eq!(serving_tier_capacity_bytes(GIB), 0);
        }

        // what this catches: ActiveArtifactSet prefix containment BOTH
        // directions — marking a model dir protects files inside it, and
        // marking a file protects its parent dir from wholesale migration.
        #[test]
        fn active_set_protects_prefix_both_directions() {
            let set = ActiveArtifactSet::default();
            set.register(PathBuf::from("/hot/served-model"));
            assert!(set.protects(Path::new("/hot/served-model")));
            assert!(
                set.protects(Path::new("/hot/served-model/model.gguf")),
                "candidate under active dir is protected"
            );
            assert!(
                set.protects(Path::new("/hot")),
                "parent of active is protected"
            );
            assert!(!set.protects(Path::new("/hot/other-model")));
            set.release(Path::new("/hot/served-model"));
            assert!(!set.protects(Path::new("/hot/served-model")));
        }
    }
}
