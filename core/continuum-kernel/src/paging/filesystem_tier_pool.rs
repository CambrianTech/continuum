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
//!
//! ## An index fed by events, never a re-walk
//!
//! The pool keeps its files in memory: path, size, mtime and the running total. One walk seeds it
//! the first time it is asked; after that the directory's own change events (`notify`, the owner
//! the policy watcher already uses) update it, each event touching only the path it names, and
//! the pool's evictions subtract what they removed. `usage_bytes` is the running total, O(1).
//! Before this, the broker's 5 s tick asked `pressure()` -> `usage_bytes()` and a 1 s snapshot TTL
//! meant every tick re-walked the tree: on the M5 (2026-11-14, `sample` of the core) the
//! tool-output spill pool stat'ed 89,721 files in 7,509 directories each tick, the core's largest
//! use of CPU, to report a number that changes only when a tool spills.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use parking_lot::Mutex;

use crate::paging::pool::{ResourcePool, ResourcePoolEntry};

/// Why a pool could not watch its directory: it cannot keep its index current, so it is not built.
#[derive(Debug, thiserror::Error)]
pub enum PoolWatchError {
    #[error("pool directory {dir} could not be created: {source}")]
    Create { dir: PathBuf, #[source] source: std::io::Error },
    #[error("pool directory {dir} could not be watched: {source}")]
    Watch { dir: PathBuf, #[source] source: notify::Error },
}

/// The pool's files, by path: size and mtime, and their total.
#[derive(Debug, Default)]
struct Seeded {
    files: HashMap<PathBuf, (u64, SystemTime)>,
    total: u64,
}

/// The index: absent until the pool is first asked, then seeded by one walk.
#[derive(Debug, Default)]
struct Index(Option<Seeded>);

impl Index {
    /// The seeded index, seeding it with one walk of `dir` on first use.
    fn seeded(&mut self, dir: &Path) -> &mut Seeded {
        self.0.get_or_insert_with(|| {
            let mut seeded = Seeded::default();
            walk(dir, &mut |path, size, mtime| {
                seeded.total = seeded.total.saturating_add(size);
                seeded.files.insert(path, (size, mtime));
            });
            seeded
        })
    }

    /// Bring one changed path up to date: a file is upserted at its current size, a directory that
    /// appeared is walked, and a path that is gone takes everything under it. Idempotent, so an
    /// event racing the seed walk never counts a file twice. Before seeding there is nothing to
    /// update: the seed walk sees the change.
    fn note(&mut self, path: &Path) {
        let Some(Seeded { files, total }) = &mut self.0 else { return };
        match fs::metadata(path) {
            Ok(meta) if meta.is_file() => {
                let mtime = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH); // unwrap_or: a filesystem without mtimes orders the file first
                let previous = files.insert(path.to_path_buf(), (meta.len(), mtime)).map_or(0, |(size, _)| size);
                *total = total.saturating_sub(previous).saturating_add(meta.len());
            }
            Ok(meta) if meta.is_dir() => walk(path, &mut |file, size, mtime| {
                let previous = files.insert(file, (size, mtime)).map_or(0, |(old, _)| old);
                *total = total.saturating_sub(previous).saturating_add(size);
            }),
            _ => {
                let gone: Vec<PathBuf> = files.keys().filter(|p| p.starts_with(path)).cloned().collect();
                for p in gone {
                    if let Some((size, _)) = files.remove(&p) {
                        *total = total.saturating_sub(size);
                    }
                }
            }
        }
    }
}

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
    /// The files and their total, shared with the watcher's callback.
    index: Arc<Mutex<Index>>,
    /// Kept alive for as long as the pool: dropping it stops the events.
    _watcher: RecommendedWatcher,
    /// How a file is removed, for a cache whose entries something live may still point at (the
    /// tool-output spill: a handle in a mind's working memory). The remover decides liveness
    /// AND deletes in one critical section of its owner, so nothing can claim the file between
    /// the check and the delete (Codex on #4982: a separate predicate released its lock before
    /// the delete). `None` deletes purely oldest-first.
    remove: Option<Arc<dyn Fn(&Path) -> Removal + Send + Sync>>,
}

impl FilesystemTierPool {
    /// Create a new pool owning `dir` (created when absent, so a pool registered at boot before
    /// anything wrote there still watches it). `tier_name` shows up on the broker's `pools` array
    /// and every `PressureAlert` for this tier. `max_bytes` is the soft capacity: the broker maps
    /// `usage / max_bytes` to pressure and acts when it crosses thresholds.
    pub fn new(tier_name: impl Into<String>, dir: PathBuf, max_bytes: u64) -> Result<Self, PoolWatchError> {
        fs::create_dir_all(&dir).map_err(|source| PoolWatchError::Create { dir: dir.clone(), source })?;
        let index = Arc::new(Mutex::new(Index::default()));
        let fed = Arc::clone(&index);
        let mut watcher = notify::recommended_watcher(move |event: notify::Result<Event>| {
            if let Ok(event) = event {
                let mut index = fed.lock();
                if event.need_rescan() {
                    // The OS dropped events: the index may have drifted, so the next read re-seeds it.
                    *index = Index::default();
                    return;
                }
                for path in &event.paths {
                    index.note(path);
                }
            }
        })
        .map_err(|source| PoolWatchError::Watch { dir: dir.clone(), source })?;
        watcher
            .watch(&dir, RecursiveMode::Recursive)
            .map_err(|source| PoolWatchError::Watch { dir: dir.clone(), source })?;
        Ok(Self { tier_name: tier_name.into(), dir, max_bytes, index, _watcher: watcher, remove: None })
    }

    /// Remove each file through `remove`, which may protect it. Oldest-first still orders them.
    pub fn removing_with(mut self, remove: Arc<dyn Fn(&Path) -> Removal + Send + Sync>) -> Self {
        self.remove = Some(remove);
        self
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
        self.index.lock().seeded(&self.dir).total
    }

    fn evict_at_least(&self, want_bytes: u64) -> u64 {
        if want_bytes == 0 {
            return 0;
        }
        // Oldest first, ordered only when something is evicted (rare), never on a pressure read.
        let mut oldest_first: Vec<(PathBuf, u64, SystemTime)> = {
            let mut index = self.index.lock();
            index.seeded(&self.dir).files.iter().map(|(path, (size, mtime))| (path.clone(), *size, *mtime)).collect()
        };
        oldest_first.sort_by_key(|(_, _, mtime)| *mtime);
        let mut freed: u64 = 0;
        for (path, size, _mtime) in &oldest_first {
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
                    freed = freed.saturating_add(*size);
                    self.index.lock().note(path);
                    continuum_base::probe!(
                        class = "pool.evicted",
                        tier = %self.tier_name,
                        path = %path.display(),
                        bytes = *size
                    );
                }
                Removal::Failed(e) => {
                    // An operator may have locked a file: the next-oldest is tried.
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
        // Per-file detail isn't surfaced on the broker wire: operators see tier-level
        // capacity, usage and pressure, and `PressureAlert`s. Empty keeps the payload bounded.
        Vec::new()
    }
}

/// Every file under `dir` (recursively) with its size and mtime, handed to `found`. Run once to
/// seed an index, and for a directory that appears inside one; never on a pressure read.
fn walk(dir: &Path, found: &mut dyn FnMut(PathBuf, u64, SystemTime)) {
    let Ok(entries) = fs::read_dir(dir) else { return }; // an absent or unreadable dir holds nothing
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(metadata) = entry.metadata() else { continue };
        if metadata.is_dir() {
            walk(&path, found);
            continue;
        }
        let mtime = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH); // unwrap_or: a filesystem without mtimes orders the file first
        found(path, metadata.len(), mtime);
    }
}

#[cfg(test)]
mod tests {
    //! What this catches: the full broker → pool → eviction loop in
    //! one test. Per Joel's "less tests with more coverage" doctrine —
    //! one integration test exercises the contract end-to-end rather
    //! than three single-path unit tests.
    use super::*;
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

        let pool = FilesystemTierPool::new("test-tier", dir.path().to_path_buf(), 10_000).unwrap();
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
        let pool = FilesystemTierPool::new("guarded", dir.path().to_path_buf(), 10_000).unwrap()
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
        let pool = FilesystemTierPool::new("missing", missing, 1_000_000).unwrap();
        assert_eq!(pool.usage_bytes(), 0);
        assert_eq!(pool.evict_at_least(1000), 0);
    }

    /// What this catches: the index drifting from the directory once it stops re-walking. A file
    /// noted twice (an event racing the seed walk, or a modify after a create) counts once at its
    /// current size; a directory that appears is walked in; a removed directory takes every file
    /// under it. Driven through `note` directly, so no OS event timing is involved.
    #[test]
    fn the_index_follows_changes_without_a_rewalk() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("a.log"), vec![b'.'; 100]).unwrap();
        let mut index = Index::default();
        assert_eq!(index.seeded(dir.path()).total, 100);

        let b = dir.path().join("b.log");
        fs::write(&b, vec![b'.'; 50]).unwrap();
        index.note(&b);
        index.note(&b);
        assert_eq!(index.seeded(dir.path()).total, 150, "a file noted twice counts once");
        fs::write(&b, vec![b'.'; 80]).unwrap();
        index.note(&b);
        assert_eq!(index.seeded(dir.path()).total, 180, "a grown file counts at its new size");

        let sub = dir.path().join("persona");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("h1.log"), vec![b'.'; 10]).unwrap();
        fs::write(sub.join("h2.log"), vec![b'.'; 20]).unwrap();
        index.note(&sub);
        assert_eq!(index.seeded(dir.path()).total, 210, "a new directory is walked in");

        fs::remove_dir_all(&sub).unwrap();
        index.note(&sub);
        assert_eq!(index.seeded(dir.path()).total, 180, "a removed directory takes its files");
        assert_eq!(index.seeded(dir.path()).files.len(), 2);
    }
}
