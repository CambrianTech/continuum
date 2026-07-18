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

use std::path::{Path, PathBuf};
use std::sync::Arc;

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
    use std::os::unix::io::AsRawFd;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .ok()?;
    let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    (rc == 0).then_some(file)
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
        let owned = ["cargo-target"];
        let deferred = [
            ("genome-models", "#155: reference-aware LRU — never blind-delete a model being served"),
            ("hf-hub", "#155: hub LRU keyed on last-access — downloads are re-fetchable"),
            ("citizens", "#155/#49: workspace CoW fix removes the bulk; stores are persona MEMORY, never auto-evicted"),
            ("forge", "#155: export trimmer — intermediates only, published artifacts stay"),
        ];
        use super::super::disk_pressure::DiskReporter as _;
        for dir in super::super::disk_reporters::standard_tracked_dirs(Path::new("/h")) {
            let name = dir.report().name;
            let decided = owned.contains(&name.as_str())
                || deferred.iter().any(|(n, _)| *n == name);
            assert!(
                decided,
                "cache class '{name}' has NO eviction decision — register an owner pool or \
                 add it to the deferred list above with the task that owns it (task #155; \
                 the 2026-07-13 incident was exactly an ownerless class filling the disk)"
            );
        }
    }
}
