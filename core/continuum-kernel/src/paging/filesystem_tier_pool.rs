//! `FilesystemTierPool` — a reusable `ResourcePool` for any directory-
//! backed cache.
//!
//! Closes the broker → pool → real eviction loop end-to-end. The disk
//! pressure monitor (signal-only) tells the broker disk is hot; the
//! broker walks registered pools and asks each to `evict_at_least(n)`;
//! pools that own real files (this one) actually delete oldest-first
//! until `want_bytes` are freed.
//!
//! ## Why a generic filesystem pool primitive
//!
//! The substrate has multiple disk-backed caches with the same shape:
//! probe JSONL rotation (`~/.continuum/jtag/logs/probes/`), genome
//! cache (`~/.continuum/genome/`), model registry (`~/.continuum/models/`),
//! persona recorder outputs, sentinel checkpoint dirs, fixture archive.
//! Each one is "directory + soft capacity + oldest-first LRU eviction
//! by mtime." Writing N near-identical `ResourcePool` impls violates
//! the compression principle from CLAUDE.md — one primitive, N
//! instances. Same shape `PagedResourcePool<K, V>` does for in-memory
//! caches.
//!
//! Per `[[auto-clean-is-structural-not-operational]]`: substrate
//! writers that grow incrementally MUST auto-clean structurally.
//! Daily rotation handles the time axis; this pool handles the
//! space-pressure axis. Together they bound disk usage from both
//! directions.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use parking_lot::Mutex;

use crate::paging::pool::{ResourcePool, ResourcePoolEntry};

mod index;
use index::DirIndex;

/// What removing one file did. A guarded remover returns [`Removal::Protected`] for a file
/// something live still points at.
#[derive(Debug)]
pub enum Removal {
    Removed,
    Protected,
    Failed(std::io::Error),
}

impl From<std::io::Result<()>> for Removal {
    fn from(result: std::io::Result<()>) -> Self {
        match result {
            Ok(()) => Removal::Removed,
            Err(e) => Removal::Failed(e),
        }
    }
}

/// `ResourcePool` implementation backed by a single directory on the
/// host filesystem. Files in the directory are evicted oldest-first
/// (by mtime) when the broker asks for relief.
pub struct FilesystemTierPool {
    tier_name: String,
    dir: PathBuf,
    max_bytes: u64,
    /// Her directory's files, kept current by the kernel's change events (`index.rs`): walked once
    /// here, never per read.
    index: Arc<Mutex<DirIndex>>,
    /// Signalled after each change the kernel reported is in the index (what a waiter waits on).
    observed: Arc<parking_lot::Condvar>,
    /// Set when the kernel says events were lost (or the watch could not start): the next read
    /// walks once to rebuild the index, as the watch API requires.
    rescan: Arc<AtomicBool>,
    /// The kernel watch that keeps `index` current; dropped with the pool.
    watcher: Mutex<Option<RecommendedWatcher>>,
    /// How a file is removed, for a cache whose entries something live may still point at (the
    /// tool-output spill: a handle in a mind's working memory). The remover decides liveness
    /// AND deletes in one critical section of its owner, so nothing can claim the file between
    /// the check and the delete (Codex on #4982: a separate predicate released its lock before
    /// the delete). `None` deletes purely oldest-first.
    remove: Option<std::sync::Arc<dyn Fn(&Path) -> Removal + Send + Sync>>,
}

impl FilesystemTierPool {
    /// Create a new pool wrapping `dir`. `tier_name` shows up on the
    /// broker's `pools` array + every `PressureAlert` for this tier.
    /// `max_bytes` is the soft capacity — the broker tier-maps
    /// `usage / max_bytes` to pressure 0.0..1.0+ and acts when it
    /// crosses thresholds.
    pub fn new(tier_name: impl Into<String>, dir: PathBuf, max_bytes: u64) -> Self {
        let pool = Self {
            tier_name: tier_name.into(),
            index: Arc::new(Mutex::new(DirIndex::seeded(&dir))),
            observed: Arc::new(parking_lot::Condvar::new()),
            rescan: Arc::new(AtomicBool::new(false)),
            watcher: Mutex::new(None),
            dir,
            max_bytes,
            remove: None,
        };
        pool.watch();
        pool
    }

    /// Watch her directory: each change the kernel reports updates only the paths it names. A watch
    /// that cannot start (the directory is absent) leaves `rescan` set, so each read walks it until it
    /// exists and the watch starts.
    fn watch(&self) {
        let index = Arc::clone(&self.index);
        let observed = Arc::clone(&self.observed);
        let rescan = Arc::clone(&self.rescan);
        let started = notify::recommended_watcher(move |event: notify::Result<Event>| match event {
            Ok(event) if event.need_rescan() => rescan.store(true, Ordering::Release),
            Ok(event) => {
                let mut index = index.lock();
                for path in &event.paths {
                    index.observe(path);
                }
                observed.notify_all();
            }
            Err(_) => rescan.store(true, Ordering::Release),
        })
        .and_then(|mut watcher| watcher.watch(&self.dir, RecursiveMode::Recursive).map(|()| watcher));
        match started {
            Ok(watcher) => *self.watcher.lock() = Some(watcher),
            Err(e) => {
                continuum_base::probe!(class = "pool.watch_unavailable", tier = %self.tier_name, reason = %e,
                    "the pool's directory cannot be watched yet: reads walk it until the watch starts");
                self.rescan.store(true, Ordering::Release);
            }
        }
    }

    /// Remove each file through `remove`, which may protect it. Oldest-first still orders them.
    pub fn removing_with(mut self, remove: std::sync::Arc<dyn Fn(&Path) -> Removal + Send + Sync>) -> Self {
        self.remove = Some(remove);
        self
    }

    /// Wait until `ready` holds of her usage, or `deadline` passes: woken by each change the kernel
    /// reports, never polled.
    #[cfg(test)]
    fn wait_for_usage(&self, ready: impl Fn(u64) -> bool, deadline: std::time::Instant) -> bool {
        let mut index = self.index.lock();
        while !ready(index.total()) {
            if self.observed.wait_until(&mut index, deadline).timed_out() {
                return ready(index.total());
            }
        }
        true
    }

    /// Rebuild the index once when the kernel said it lost events (or the watch has not started): the
    /// one walk the watch API requires, never one per read.
    fn rescan_if_lost(&self) {
        if !self.rescan.swap(false, Ordering::AcqRel) {
            return;
        }
        *self.index.lock() = DirIndex::seeded(&self.dir);
        if self.watcher.lock().is_none() {
            self.watch();
        }
    }
}

impl ResourcePool for FilesystemTierPool {
    fn tier_name(&self) -> &str {
        &self.tier_name
    }

    fn capacity_bytes(&self) -> u64 {
        self.max_bytes
    }

    fn usage_bytes(&self) -> u64 {
        self.rescan_if_lost();
        self.index.lock().total()
    }

    fn evict_at_least(&self, want_bytes: u64) -> u64 {
        if want_bytes == 0 {
            return 0;
        }
        self.rescan_if_lost();
        let oldest = self.index.lock().oldest_first();
        let mut freed: u64 = 0;
        for (path, size) in &oldest {
            if freed >= want_bytes {
                break;
            }
            let removal = match &self.remove {
                Some(remove) => remove(path),
                None => fs::remove_file(path).into(),
            };
            match removal {
                Removal::Protected => {
                    continuum_base::probe!(
                        class = "pool.evict_protected",
                        tier = %self.tier_name,
                        path = %path.display(),
                        "a file something live still points at is not evicted"
                    );
                }
                Removal::Removed => {
                    // Reconciled, never assumed gone: a file recreated at this path (and already recorded by
                    // the watch) keeps its accounting.
                    self.index.lock().observe(path);
                    freed = freed.saturating_add(*size);
                    continuum_base::probe!(
                        class = "pool.evicted",
                        tier = %self.tier_name,
                        path = %path.display(),
                        bytes = *size
                    );
                }
                Removal::Failed(e) => {
                    // Don't bail — operator may have locked a file; try
                    // the next-oldest. Logging the failure keeps the
                    // evidence trail per
                    // `[[every-error-is-an-opportunity-to-battle-harden]]`.
                    tracing::warn!(
                        tier = %self.tier_name,
                        path = %path.display(),
                        error = %e,
                        "FilesystemTierPool: eviction skip"
                    );
                    continuum_base::probe!(
                        class = "pool.evict_failed",
                        tier = %self.tier_name,
                        path = %path.display(),
                        reason = %e
                    );
                }
            }
        }
        freed
    }

    fn snapshot(&self) -> Vec<ResourcePoolEntry> {
        // Per-file detail isn't surfaced on the broker wire today —
        // operators see tier-level capacity/usage/pressure and
        // `PressureAlert`s. Returning empty keeps the broker payload
        // bounded regardless of how many files live in the dir.
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    //! What this catches: the full broker → pool → eviction loop in
    //! one test. Per Joel's "less tests with more coverage" doctrine —
    //! one integration test exercises the contract end-to-end rather
    //! than three single-path unit tests.
    use super::*;
    use std::time::SystemTime;
    use std::time::Duration;
    use tempfile::tempdir;

    /// What this catches: `evict_at_least` actually deletes files
    /// oldest-first by mtime, stops once it has freed enough bytes,
    /// updates the snapshot so the NEXT `usage_bytes` reflects the
    /// freed state, and emits the eviction probe. Walks the whole
    /// production-shape contract in one test.
    #[test]
    fn evicts_oldest_first_until_target_bytes_freed() {
        let dir = tempdir().unwrap();
        // Write 3 files with controlled mtimes (oldest → newest).
        // Use fs::set_modified on a stable file rather than relying on
        // creation order so the test doesn't race with fs timestamp
        // resolution (some filesystems round mtime to the second).
        let paths: Vec<PathBuf> = (0..3)
            .map(|i| {
                let path = dir.path().join(format!("evt-{i}.jsonl"));
                fs::write(&path, vec![b'.'; 1000]).unwrap();
                path
            })
            .collect();
        // Force mtimes: 100s ago, 50s ago, now.
        let now = SystemTime::now();
        let stamps = [
            now - Duration::from_secs(100),
            now - Duration::from_secs(50),
            now,
        ];
        for (p, t) in paths.iter().zip(stamps.iter()) {
            let file = fs::File::open(p).unwrap();
            file.set_modified(*t).unwrap();
        }

        let pool = FilesystemTierPool::new("test-tier", dir.path().to_path_buf(), 10_000);
        // Sanity: all three files counted.
        assert_eq!(pool.usage_bytes(), 3_000);
        assert_eq!(pool.tier_name(), "test-tier");
        assert_eq!(pool.capacity_bytes(), 10_000);

        // Ask for 1500 bytes — should evict 2 files (the two oldest,
        // 1000+1000=2000 ≥ 1500), leaving the newest in place.
        let freed = pool.evict_at_least(1500);
        assert!(
            freed >= 1500,
            "must free at least the requested amount, got {freed}"
        );
        assert_eq!(freed, 2000, "evicts whole files until target is met");

        // Snapshot updated — usage now reflects the surviving file.
        assert_eq!(pool.usage_bytes(), 1_000);
        // Only the newest file survives.
        assert!(
            !paths[0].exists() && !paths[1].exists(),
            "oldest two files deleted"
        );
        assert!(paths[2].exists(), "newest file preserved");
    }

    /// What this catches: eviction deleting a file something live still points at (card
    /// 9197cd01: a spill a mind's working memory references). The protected OLDEST file
    /// survives; eviction moves on to the next-oldest instead.
    #[test]
    fn a_protected_file_is_never_evicted_however_old() {
        let dir = tempdir().unwrap();
        let now = SystemTime::now();
        let paths: Vec<PathBuf> = (0..3)
            .map(|i| {
                let path = dir.path().join(format!("h{i}.log"));
                fs::write(&path, vec![b'.'; 1000]).unwrap();
                fs::File::open(&path)
                    .unwrap()
                    .set_modified(now - Duration::from_secs(100 - i * 40))
                    .unwrap();
                path
            })
            .collect();
        let oldest = paths[0].clone();
        let pool = FilesystemTierPool::new("guarded", dir.path().to_path_buf(), 10_000)
            .removing_with(std::sync::Arc::new(move |p: &Path| {
                if p == oldest { Removal::Protected } else { fs::remove_file(p).into() }
            }));
        assert_eq!(pool.evict_at_least(1000), 1000);
        assert!(paths[0].exists(), "the protected oldest file survives");
        assert!(!paths[1].exists(), "the next-oldest goes instead");
        assert!(paths[2].exists());
    }

    /// What this catches: a pool pointed at a non-existent dir
    /// reports zero usage and no-op evicts cleanly rather than
    /// erroring. Important for the "register pools at boot, dirs
    /// may not exist yet" pattern.
    #[test]
    fn missing_dir_reports_zero_and_evicts_zero() {
        let dir = tempdir().unwrap();
        let missing = dir.path().join("does-not-exist");
        let pool = FilesystemTierPool::new("missing", missing, 1_000_000);
        assert_eq!(pool.usage_bytes(), 0);
        assert_eq!(pool.evict_at_least(1000), 0);
    }

    /// What this catches: usage read by re-walking her directory every broker tick (the M5,
    /// 2026-11-17: ~615 of ~6,900 active core samples under `walk`). Files written AFTER the pool is
    /// made reach its usage through the kernel's change events, and removing one is seen the same
    /// way; no read walks.
    #[test]
    fn files_written_after_the_pool_is_made_reach_its_usage_by_kernel_events() {
        let dir = tempdir().unwrap();
        let pool = FilesystemTierPool::new("watched", dir.path().to_path_buf(), 1_000_000);
        assert_eq!(pool.usage_bytes(), 0);
        let deadline = || std::time::Instant::now() + Duration::from_secs(10);
        fs::create_dir(dir.path().join("persona")).unwrap();
        fs::write(dir.path().join("persona/a.log"), vec![b'.'; 700]).unwrap();
        fs::write(dir.path().join("b.log"), vec![b'.'; 300]).unwrap();
        assert!(pool.wait_for_usage(|usage| usage == 1_000, deadline()), "the kernel reported both writes, nested included");
        fs::remove_file(dir.path().join("b.log")).unwrap();
        assert!(pool.wait_for_usage(|usage| usage == 700, deadline()), "and the removal");
    }
}
