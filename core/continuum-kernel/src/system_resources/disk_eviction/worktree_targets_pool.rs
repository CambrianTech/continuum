//! Eviction owner for `wt-targets`: the directory of per-worktree cargo target dirs.
//!
//! The M5, 2026-10-29 23:13Z: `~/.continuum/cache/wt-targets` held 448 GB (apparent) in one
//! target dir per git worktree, a convention adopted on 2026-10-21 because one shared target
//! ran a stale binary across worktrees. No row tracked it and no pool owned it, so the volume
//! reached ENOSPC with the broker seeing nothing to reclaim; a test window and the residents'
//! core went down with it.
//!
//! Each child is a whole, derived cargo target, so the unit of eviction is the child: least
//! recently built first, never a held one. A child is HELD when any of these says so, each the
//! same check the shared cargo-target owner already makes:
//! 1. cargo's own build locks (`.cargo-lock` at the root and in every profile dir, including
//!    a target-triple's profile dirs) cannot all be taken exclusively;
//! 2. a process is executing an image under it (a test binary, a core started from it);
//! 3. a writer admitted through `admit_cargo_target` still holds its root lease there;
//! 4. serving has registered an artifact under it.
//!
//! Removal first DETACHES the child (one rename to `.evicting-<name>` under the same root)
//! while the locks are held, so a cargo that starts afterwards builds a fresh tree at the old
//! name instead of into one being deleted. The detached tree is then removed with the same
//! redirect-safe walk the shared owner uses. A detached tree a crash left behind is reclaimed
//! first on the next eviction: nothing builds into a detached name.

use super::*;
use crate::system_resources::drives::DriveInfo;

/// The prefix a detached child carries until its removal completes.
const DETACHED_PREFIX: &str = ".evicting-";

pub struct WorktreeTargetsPool {
    /// Class plus physical root, the same identity rule as [`CargoTargetPool`]: the broker
    /// dedups by name, so a hardcoded name would let one registration replace another.
    tier: String,
    pub(super) tracked: Arc<TrackedDir>,
    pub(super) active_artifacts: Arc<ActiveArtifactSet>,
    /// Read the owning volume's already-published sample; never probe here.
    pub(super) volume_sample: fn(&Path) -> Option<DriveInfo>,
    /// The writer-lease check (hold 3). A field so a test can stand for a held lease.
    pub(super) leased: fn(&Path) -> bool,
    /// Whether "every target is held" was already said for the current run of such
    /// evictions, so a standing condition speaks once instead of once per broker tick.
    all_held_reported: std::sync::atomic::AtomicBool,
}

/// One child target dir, with when a build last wrote into it.
#[derive(Debug)]
pub(super) struct Candidate {
    pub(super) path: PathBuf,
    pub(super) last_built: std::time::SystemTime,
}

impl WorktreeTargetsPool {
    pub fn new(tracked: Arc<TrackedDir>) -> Self {
        use sha2::{Digest, Sha256};
        let root_id = Sha256::digest(tracked.path().as_os_str().as_encoded_bytes());
        Self {
            tier: format!("disk-{}:{root_id:x}", tracked.class()),
            tracked,
            active_artifacts: serving_active_artifacts(),
            volume_sample: super::super::DiskPressureMonitor::volume_for,
            leased: super::super::disk_reporters::cargo_root_leased,
            all_held_reported: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// The same volume-derived budget every build cache takes; an unmeasured volume reads
    /// the budget's documented floor, which is also what the derived budget is on a small one.
    fn budget(volume: Option<&DriveInfo>) -> u64 {
        volume.map_or(DEFAULT_CARGO_TARGET_BUDGET_BYTES, |volume| {
            cargo_target_budget_bytes(volume.total_bytes)
        })
    }

    /// When a build last wrote into `dir`: the newest mtime among the dir, its entries, and
    /// its entries' entries. A build that compiles anything adds files to `<profile>/deps`,
    /// `<profile>/.fingerprint` and `<profile>/incremental` (each a depth-2 directory whose
    /// mtime moves when an entry is added or replaced) and relinks `<profile>/<bin>`; under
    /// `--target <triple>` the profile dir is itself at depth 2 and moves when its outputs
    /// are relinked. A no-op build writes nothing and so does not refresh the rank. Access times are not used: `noatime`/`relatime` mounts make
    /// them unreliable. `None` when the dir's own mtime cannot be read: its recency is
    /// unknown, so it is not ranked at all rather than guessed old or new.
    pub(super) fn last_built(dir: &Path) -> Option<std::time::SystemTime> {
        fn visit(path: &Path, descend: bool, newest: &mut std::time::SystemTime) -> Vec<PathBuf> {
            let Ok(entries) = std::fs::read_dir(path) else {
                return Vec::new();
            };
            let mut dirs = Vec::new();
            for entry in entries.flatten() {
                let Ok(metadata) = entry.metadata() else {
                    continue;
                };
                if let Ok(modified) = metadata.modified() {
                    *newest = (*newest).max(modified);
                }
                if descend && metadata.is_dir() && !metadata.is_symlink() {
                    dirs.push(entry.path());
                }
            }
            dirs
        }
        let mut newest = std::fs::symlink_metadata(dir).ok()?.modified().ok()?;
        for child in visit(dir, true, &mut newest) {
            visit(&child, false, &mut newest);
        }
        Some(newest)
    }

    /// The children to consider, least recently built first, plus any detached trees a
    /// crashed eviction left behind. Files and links at the root are never candidates.
    pub(super) fn candidates(root: &Path) -> (Vec<PathBuf>, Vec<Candidate>) {
        let Ok(entries) = std::fs::read_dir(root) else {
            return (Vec::new(), Vec::new());
        };
        let mut detached = Vec::new();
        let mut live = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(metadata) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            if !metadata.is_dir() || CargoTargetPool::is_redirect(&metadata) {
                continue;
            }
            if entry
                .file_name()
                .to_string_lossy()
                .starts_with(DETACHED_PREFIX)
            {
                detached.push(path);
            } else if let Some(last_built) = Self::last_built(&path) {
                live.push(Candidate { path, last_built });
            }
        }
        live.sort_by_key(|candidate| candidate.last_built);
        (detached, live)
    }

    /// Every cargo build lock under `dir`: its root `.cargo-lock` (created if missing, so a
    /// cargo starting mid-eviction blocks on it) and every existing `.cargo-lock` one or two
    /// levels down (`<profile>/` and `<triple>/<profile>/`). `Err` = some lock is held,
    /// redirected or cannot be queried, so the dir is held.
    pub(super) fn take_build_locks(dir: &Path) -> Result<Vec<std::fs::File>, String> {
        let mut paths = vec![dir.join(".cargo-lock")];
        let mut level = vec![dir.to_path_buf()];
        for _ in 0..2 {
            let mut next = Vec::new();
            for parent in &level {
                let Ok(entries) = std::fs::read_dir(parent) else {
                    continue;
                };
                for entry in entries.flatten() {
                    let path = entry.path();
                    let Ok(metadata) = std::fs::symlink_metadata(&path) else {
                        continue;
                    };
                    if metadata.is_dir() && !CargoTargetPool::is_redirect(&metadata) {
                        if std::fs::symlink_metadata(path.join(".cargo-lock")).is_ok() {
                            paths.push(path.join(".cargo-lock"));
                        }
                        next.push(path);
                    }
                }
            }
            level = next;
        }
        let mut locks = Vec::with_capacity(paths.len());
        for path in paths {
            if let Ok(metadata) = std::fs::symlink_metadata(&path) {
                if CargoTargetPool::is_redirect(&metadata) {
                    return Err(format!("redirected build lock: {}", path.display()));
                }
            }
            match try_exclusive_flock_detailed(&path) {
                LockAttempt::Acquired(lock) => locks.push(lock),
                LockAttempt::HeldByAnother => {
                    return Err(format!("a live build holds {}", path.display()))
                }
                LockAttempt::Unavailable(why) => return Err(format!("{}: {why}", path.display())),
            }
        }
        Ok(locks)
    }

    /// Why `dir` is held, or `None` when it may be evicted. The caller holds `_locks`.
    fn held(&self, dir: &Path) -> Option<String> {
        if let Some((pid, exe)) = process_executing_under(dir) {
            return Some(format!("pid {pid} is executing {exe}"));
        }
        if (self.leased)(dir) {
            return Some("an admitted cargo writer holds its lease".into());
        }
        // An identity that cannot be resolved cannot be cleared of serving custody.
        let physical = match std::fs::canonicalize(dir) {
            Ok(physical) => physical,
            Err(error) => return Some(format!("serving custody unverifiable: {error}")),
        };
        self.active_artifacts
            .protects(&physical)
            .then(|| "serving holds an artifact under it".into())
    }

    /// Remove a detached tree with the shared owner's redirect-safe walk; bytes are the
    /// removed files' apparent lengths, the measure the scanner reports for this class.
    fn remove_detached(detached: &Path) -> u64 {
        CargoTargetPool::free_owned_tree(detached, detached, None)
    }

    /// Detach and remove one unheld child; 0 when it is held or cannot be detached.
    fn evict_one(&self, root: &Path, candidate: &Candidate) -> u64 {
        let dir = &candidate.path;
        if let Err(error) = CargoTargetPool::owned_metadata(dir, root) {
            crate::clog_warn!("💾 {} skipped {}: {error}", self.tier, dir.display());
            return 0;
        }
        let _locks = match Self::take_build_locks(dir) {
            Ok(locks) => locks,
            Err(why) => {
                tracing::debug!(probe_class = "disk.wt_targets.held", dir = %dir.display(), %why, "held: build lock");
                return 0;
            }
        };
        if let Some(why) = self.held(dir) {
            tracing::debug!(probe_class = "disk.wt_targets.held", dir = %dir.display(), %why, "held");
            return 0;
        }
        let Some(name) = dir.file_name() else {
            return 0;
        };
        let mut detached_name = std::ffi::OsString::from(DETACHED_PREFIX);
        detached_name.push(name);
        let detached = root.join(detached_name);
        if let Err(error) = std::fs::rename(dir, &detached) {
            crate::clog_warn!(
                "💾 {} could not detach {}: {error}",
                self.tier,
                dir.display()
            );
            return 0;
        }
        Self::remove_detached(&detached)
    }
}

impl ResourcePool for WorktreeTargetsPool {
    fn tier_name(&self) -> &str {
        &self.tier
    }

    fn capacity_bytes(&self) -> u64 {
        Self::budget((self.volume_sample)(self.tracked.path()).as_ref())
    }

    fn usage_bytes(&self) -> u64 {
        self.tracked.bytes()
    }

    /// Over its budget OR on a hot volume, the same rule as every build cache.
    fn pressure(&self) -> f64 {
        let volume = (self.volume_sample)(self.tracked.path());
        cache_pressure_on_volume(
            self.tracked.bytes(),
            Self::budget(volume.as_ref()),
            volume.as_ref(),
        )
    }

    fn evict_at_least(&self, want_bytes: u64) -> u64 {
        // The class row names the configured spelling; custody checks run on the physical
        // root, so an aliased ancestor (a linked home) is resolved once here, not refused.
        let root = match std::fs::canonicalize(self.tracked.path()) {
            Ok(root) => root,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return 0,
            Err(error) => {
                crate::clog_warn!(
                    "💾 {} eviction refused: root unresolved: {error}",
                    self.tier
                );
                return 0;
            }
        };
        if let Err(error) = CargoTargetPool::owned_metadata(&root, &root) {
            crate::clog_warn!("💾 {} eviction refused: {error}", self.tier);
            return 0;
        }
        // What the filesystem actually frees, beside the apparent bytes: per-worktree targets
        // can be APFS clones sharing blocks, so the two differ and both are reported.
        let available_before = fs2::available_space(&root).ok();
        let (detached, live) = Self::candidates(&root);
        let mut freed = 0u64;
        let mut evicted = 0usize;
        for tree in detached {
            freed = freed.saturating_add(Self::remove_detached(&tree));
        }
        for candidate in &live {
            if freed >= want_bytes.max(1) {
                break;
            }
            let bytes = self.evict_one(&root, candidate);
            if bytes > 0 {
                evicted += 1;
            }
            freed = freed.saturating_add(bytes);
        }
        if freed > 0 {
            self.tracked.record_freed(freed);
            self.all_held_reported
                .store(false, std::sync::atomic::Ordering::Relaxed);
            let volume_freed = match (available_before, fs2::available_space(&root).ok()) {
                (Some(before), Some(after)) => {
                    format!("{} GB", after.saturating_sub(before) / (1024 * 1024 * 1024))
                }
                _ => "an unmeasured amount (statfs failed)".to_string(),
            };
            crate::clog_warn!(
                "💾 {} evicted {evicted} worktree target(s), least recently built first: {} GB apparent, \
                 volume free space grew by {volume_freed} (clones share blocks; the volume number is what \
                 the disk got back); the next build in each worktree recreates its target",
                self.tier,
                freed / (1024 * 1024 * 1024)
            );
        } else if !live.is_empty()
            && !self
                .all_held_reported
                .swap(true, std::sync::atomic::Ordering::Relaxed)
        {
            crate::clog_warn!(
                "💾 {} freed nothing: all {} worktree targets are held (a build lock, a running image, \
                 a writer lease or serving custody)",
                self.tier,
                live.len()
            );
        }
        freed
    }

    fn snapshot(&self) -> Vec<ResourcePoolEntry> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests;
