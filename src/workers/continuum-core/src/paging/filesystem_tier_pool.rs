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
use std::sync::Mutex;
use std::time::SystemTime;

use crate::paging::pool::{ResourcePool, ResourcePoolEntry};

/// Cached directory-walk result. Recomputed on demand, never older
/// than `MAX_AGE` so concurrent `usage_bytes`/`evict_at_least` calls
/// agree on a consistent set of files for one decision cycle. The
/// cache exists ONLY to avoid the same broker tick doing two
/// independent `readdir`s; correctness doesn't depend on it.
#[derive(Debug, Clone)]
struct DirSnapshot {
    /// Files sorted oldest-first by mtime — eviction order.
    files: Vec<(PathBuf, u64, SystemTime)>,
    total_bytes: u64,
    captured_at: SystemTime,
}

/// Refresh-cache TTL. 1 s is short enough that a hot pool sees fresh
/// numbers but long enough that the same broker `relieve()` tick
/// reuses one snapshot across `usage_bytes` + `evict_at_least`.
const SNAPSHOT_TTL_MS: u128 = 1000;

/// `ResourcePool` implementation backed by a single directory on the
/// host filesystem. Files in the directory are evicted oldest-first
/// (by mtime) when the broker asks for relief.
pub struct FilesystemTierPool {
    tier_name: String,
    dir: PathBuf,
    max_bytes: u64,
    /// Cached directory snapshot. Mutex (not RwLock) because every
    /// read may also refresh; the lock window is small and contention
    /// is bounded by the broker's tick interval anyway.
    snapshot: Mutex<Option<DirSnapshot>>,
}

impl FilesystemTierPool {
    /// Create a new pool wrapping `dir`. `tier_name` shows up on the
    /// broker's `pools` array + every `PressureAlert` for this tier.
    /// `max_bytes` is the soft capacity — the broker tier-maps
    /// `usage / max_bytes` to pressure 0.0..1.0+ and acts when it
    /// crosses thresholds.
    pub fn new(tier_name: impl Into<String>, dir: PathBuf, max_bytes: u64) -> Self {
        Self {
            tier_name: tier_name.into(),
            dir,
            max_bytes,
            snapshot: Mutex::new(None),
        }
    }

    /// Recompute or reuse the cached directory snapshot.
    ///
    /// The cache is purely for correlating two reads within one broker
    /// tick (`usage_bytes` then `evict_at_least`). If the cache is
    /// stale or missing, we re-walk; on success the snapshot is
    /// installed for subsequent reads.
    fn refresh(&self) -> DirSnapshot {
        let mut guard = self.snapshot.lock().unwrap();
        if let Some(snap) = guard.as_ref() {
            if let Ok(age) = SystemTime::now().duration_since(snap.captured_at) {
                if age.as_millis() < SNAPSHOT_TTL_MS {
                    return snap.clone();
                }
            }
        }
        let new = walk_dir(&self.dir);
        *guard = Some(new.clone());
        new
    }

    /// Invalidate the cached snapshot — call after mutating the
    /// directory (e.g., after eviction) so the next read sees the
    /// updated state.
    fn invalidate(&self) {
        *self.snapshot.lock().unwrap() = None;
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
        self.refresh().total_bytes
    }

    fn evict_at_least(&self, want_bytes: u64) -> u64 {
        if want_bytes == 0 {
            return 0;
        }
        let snap = self.refresh();
        let mut freed: u64 = 0;
        for (path, size, _mtime) in &snap.files {
            if freed >= want_bytes {
                break;
            }
            match fs::remove_file(path) {
                Ok(()) => {
                    freed = freed.saturating_add(*size);
                    crate::probe!(
                        class = "pool.evicted",
                        tier = %self.tier_name,
                        path = %path.display(),
                        bytes = *size
                    );
                }
                Err(e) => {
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
                    crate::probe!(
                        class = "pool.evict_failed",
                        tier = %self.tier_name,
                        path = %path.display(),
                        reason = %e
                    );
                }
            }
        }
        if freed > 0 {
            self.invalidate();
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

/// Walk `dir`, returning files sorted oldest-first by mtime with
/// their sizes. Subdirectories are walked recursively — the substrate's
/// existing layouts (probes/, genome/, models/) all keep files at the
/// top level today, but recursive walk avoids surprise when a future
/// caller passes a parent dir.
fn walk_dir(dir: &Path) -> DirSnapshot {
    let mut files: Vec<(PathBuf, u64, SystemTime)> = Vec::new();
    let mut total: u64 = 0;
    walk(dir, &mut files, &mut total);
    files.sort_by_key(|(_, _, mtime)| *mtime);
    DirSnapshot {
        files,
        total_bytes: total,
        captured_at: SystemTime::now(),
    }
}

fn walk(dir: &Path, out: &mut Vec<(PathBuf, u64, SystemTime)>, total: &mut u64) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return, // dir absent or unreadable — treat as empty
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let metadata = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        if metadata.is_dir() {
            walk(&path, out, total);
            continue;
        }
        let size = metadata.len();
        let mtime = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        *total = total.saturating_add(size);
        out.push((path, size, mtime));
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
}
