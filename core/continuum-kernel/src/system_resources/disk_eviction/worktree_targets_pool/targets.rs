//! The blocking half of the `wt-targets` owner: every filesystem read and write it makes
//! (ranking, measuring, lock checks, detaching, deleting). Nothing here runs on the broker's
//! tick; [`super::WorktreeTargetsPool`] runs it on the blocking pool and publishes what it
//! measured as a [`Snapshot`] the tick reads.

use super::super::*;
use super::DETACHED_PREFIX;
use crate::system_resources::drives::DriveInfo;

/// Everything the blocking work needs, shared by the pool and its background jobs.
pub(super) struct Targets {
    /// Class plus physical root, the same identity rule as [`CargoTargetPool`]: the broker
    /// dedups by name, so a hardcoded name would let one registration replace another.
    pub(super) tier: String,
    pub(super) tracked: Arc<TrackedDir>,
    pub(super) active_artifacts: Arc<ActiveArtifactSet>,
    /// Read the owning volume's already-published sample; never probe here.
    pub(super) volume_sample: fn(&Path) -> Option<DriveInfo>,
    /// The root authority's exclusive eviction claim (hold 3): refused while an admitted
    /// writer's lease overlaps the target, and, while held, refuses any writer admission
    /// that overlaps it. Held from before the lock scan until the target is detached. A
    /// field so a test can stand for a held lease.
    pub(super) claim: fn(&Path) -> Result<crate::system_resources::disk_reporters::Kept, String>,
    /// The class budget for a volume sample: the cargo-target budget in production. A field
    /// so a test can stand for a volume small enough to put a fixture over it.
    pub(super) budget_for: fn(Option<&DriveInfo>) -> u64,
    /// One child's bytes: `dir_size_bytes` in production; a field so a test can make the
    /// walk slow and prove the tick never waits on it.
    pub(super) walk: fn(&Path) -> u64,
    /// Whether "every target is held" was already said for the current run of such
    /// evictions, so a standing condition speaks once instead of once per job.
    pub(super) all_held_reported: std::sync::atomic::AtomicBool,
    /// Each child's measured size, keyed by the `last_built` it was measured at: a build
    /// that changes the size moves that mark, so an unchanged mark needs no second walk.
    pub(super) sizes:
        parking_lot::Mutex<std::collections::HashMap<PathBuf, (std::time::SystemTime, u64)>>,
}

/// One child target dir, with when a build last wrote into it.
#[derive(Debug)]
pub(super) struct Candidate {
    pub(super) path: PathBuf,
    pub(super) last_built: std::time::SystemTime,
}

/// What one background measurement saw: the class's physical root, the scanner observation
/// it was taken against, detached trees awaiting removal, and every live target with its
/// size, least recently built first. The tick plans from this and never reads the disk.
#[derive(Debug)]
pub(super) struct Snapshot {
    pub(super) root: PathBuf,
    pub(super) observation: u64,
    pub(super) detached: Vec<PathBuf>,
    pub(super) live: Vec<(Candidate, u64)>,
}

/// One eviction's instructions, decided on the tick from a snapshot (see `Targets::evict`).
#[derive(Debug, Clone, Copy)]
pub(super) struct Plan {
    /// How many of the oldest targets fall outside the keep-recent budget.
    pub(super) beyond: usize,
    /// The broker's ask, honoured past the budget trim only when the class fit its budget
    /// (the volume brought the broker); 0 otherwise.
    pub(super) pressure_ask: u64,
}

impl Targets {
    pub(super) fn new(tracked: Arc<TrackedDir>) -> Self {
        use sha2::{Digest, Sha256};
        let root_id = Sha256::digest(tracked.path().as_os_str().as_encoded_bytes());
        Self {
            tier: format!("disk-{}:{root_id:x}", tracked.class()),
            tracked,
            active_artifacts: serving_active_artifacts(),
            volume_sample: super::super::super::DiskPressureMonitor::volume_for,
            claim: super::super::super::disk_reporters::claim_for_eviction,
            budget_for: Self::budget,
            walk: dir_size_bytes,
            all_held_reported: std::sync::atomic::AtomicBool::new(false),
            sizes: parking_lot::Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// The same volume-derived budget every build cache takes; an unmeasured volume reads
    /// the budget's documented floor, which is also what the derived budget is on a small one.
    pub(super) fn budget(volume: Option<&DriveInfo>) -> u64 {
        volume.map_or(DEFAULT_CARGO_TARGET_BUDGET_BYTES, |volume| {
            cargo_target_budget_bytes(volume.total_bytes)
        })
    }

    /// KEEP RECENT (Joel, 2026-10-29: build caches rebuild, so old ones are disposable at
    /// any time, not only under disk pressure). Given sizes ordered least recently built
    /// first, how many of the oldest fall outside the budget: newest first, a target is kept
    /// while the running total fits. The newest is always kept, even alone over the budget:
    /// it is the one a build is most likely about to reuse, and evicting it every tick
    /// between two builds would make every build cold.
    pub(super) fn beyond_budget(sizes_oldest_first: &[u64], budget: u64) -> usize {
        let mut kept = 0u64;
        for (from_newest, size) in sizes_oldest_first.iter().rev().enumerate() {
            kept = kept.saturating_add(*size);
            if from_newest > 0 && kept > budget {
                return sizes_oldest_first.len() - from_newest;
            }
        }
        0
    }

    /// A child's size, walked only when a build has written into it since the last walk.
    pub(super) fn measured_size(&self, candidate: &Candidate) -> u64 {
        if let Some((at, size)) = self.sizes.lock().get(&candidate.path) {
            if *at == candidate.last_built {
                return *size;
            }
        }
        let size = (self.walk)(&candidate.path);
        self.sizes
            .lock()
            .insert(candidate.path.clone(), (candidate.last_built, size));
        size
    }

    /// When a build last wrote into `dir`: the newest mtime among its entries and its
    /// entries' entries. A build that compiles anything adds files to `<profile>/deps`,
    /// `<profile>/.fingerprint` and `<profile>/incremental` (each a depth-2 directory whose
    /// mtime moves when an entry is added or replaced) and relinks `<profile>/<bin>`; under
    /// `--target <triple>` the profile dir is itself at depth 2 and moves when its outputs
    /// are relinked. A no-op build writes nothing and so does not refresh the rank.
    ///
    /// The dir's OWN mtime and every `.cargo-lock` are not read: this pool creates the root
    /// lock when it checks a target, and that would rank the target it just declined as the
    /// newest (only an empty target, with nothing else to read, is ranked by its own mtime).
    /// Access times are not used: `noatime`/`relatime` mounts make them unreliable. `None`
    /// when no mtime can be read: its recency is unknown, so it is not ranked at all rather
    /// than guessed old or new.
    pub(super) fn last_built(dir: &Path) -> Option<std::time::SystemTime> {
        fn visit(
            path: &Path,
            descend: bool,
            newest: &mut Option<std::time::SystemTime>,
        ) -> Vec<PathBuf> {
            let Ok(entries) = std::fs::read_dir(path) else {
                return Vec::new();
            };
            let mut dirs = Vec::new();
            for entry in entries.flatten() {
                if entry.file_name() == ".cargo-lock" {
                    continue;
                }
                let Ok(metadata) = entry.metadata() else {
                    continue;
                };
                if let Ok(modified) = metadata.modified() {
                    *newest = Some(newest.map_or(modified, |seen| seen.max(modified)));
                }
                if descend && metadata.is_dir() && !metadata.is_symlink() {
                    dirs.push(entry.path());
                }
            }
            dirs
        }
        let mut newest = None;
        for child in visit(dir, true, &mut newest) {
            visit(&child, false, &mut newest);
        }
        match newest {
            Some(newest) => Some(newest),
            None => std::fs::symlink_metadata(dir).ok()?.modified().ok(),
        }
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

    /// Every cargo build lock path under `dir`: its root `.cargo-lock` and every existing
    /// `.cargo-lock` one or two levels down (`<profile>/` and `<triple>/<profile>/`).
    fn build_lock_paths(dir: &Path) -> Result<Vec<PathBuf>, String> {
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
                        let lock = path.join(".cargo-lock");
                        if let Ok(metadata) = std::fs::symlink_metadata(&lock) {
                            if CargoTargetPool::is_redirect(&metadata) {
                                return Err(format!("redirected build lock: {}", lock.display()));
                            }
                            paths.push(lock);
                        }
                        next.push(path);
                    }
                }
            }
            level = next;
        }
        Ok(paths)
    }

    /// Take every cargo build lock under `dir` exclusively: the root `.cargo-lock` (created
    /// if missing, so a cargo starting mid-eviction blocks on it) and every profile lock.
    /// The scan repeats after each round of acquisitions until it finds no new lock, so a
    /// profile a build created during the previous round is covered too; the locks are kept
    /// by the caller across the detach. `Err` = some lock is held, redirected or cannot be
    /// queried, so the dir is held. Residual window, stated rather than claimed closed: a
    /// profile created after the last scan and before the rename is not locked; the rename
    /// moves it with the tree, so that build writes a fresh tree at the old path.
    pub(super) fn take_build_locks(dir: &Path) -> Result<Vec<std::fs::File>, String> {
        let mut taken = std::collections::HashSet::new();
        let mut locks = Vec::new();
        loop {
            let fresh: Vec<PathBuf> = Self::build_lock_paths(dir)?
                .into_iter()
                .filter(|path| !taken.contains(path))
                .collect();
            if fresh.is_empty() {
                return Ok(locks);
            }
            for path in fresh {
                match try_exclusive_flock_detailed(&path) {
                    LockAttempt::Acquired(lock) => locks.push(lock),
                    LockAttempt::HeldByAnother => {
                        return Err(format!("a live build holds {}", path.display()))
                    }
                    LockAttempt::Unavailable(why) => {
                        return Err(format!("{}: {why}", path.display()))
                    }
                }
                taken.insert(path);
            }
        }
    }

    /// Why `dir` is held, or `None` when it may be evicted. The caller holds the root
    /// authority's claim and the build locks.
    fn held(&self, dir: &Path) -> Option<String> {
        if let Some((pid, exe)) = process_executing_under(dir) {
            return Some(format!("pid {pid} is executing {exe}"));
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
    pub(super) fn remove_detached(detached: &Path) -> u64 {
        CargoTargetPool::free_owned_tree(detached, detached, None)
    }

    /// Detach and remove one unheld child; 0 when it is held or cannot be detached.
    pub(super) fn evict_one(&self, root: &Path, candidate: &Candidate) -> u64 {
        let dir = &candidate.path;
        if let Err(error) = CargoTargetPool::owned_metadata(dir, root) {
            crate::clog_warn!("💾 {} skipped {}: {error}", self.tier, dir.display());
            return 0;
        }
        // The root authority first: from here until the detach, no writer is admitted to
        // this target, and a writer already admitted refuses the claim.
        let claim = match (self.claim)(dir) {
            Ok(claim) => claim,
            Err(why) => {
                tracing::debug!(probe_class = "disk.wt_targets.held", dir = %dir.display(), %why, "held: root authority");
                return 0;
            }
        };
        // Scanned after the claim, so a profile a build created since the snapshot is seen.
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
        // The original path no longer exists: a writer admitted from here builds a fresh
        // target there. The build locks stay held until the detached tree is gone.
        drop(claim);
        Self::remove_detached(&detached)
    }
}

impl Targets {
    /// Measure the class (blocking): rank its children and size each one a build changed.
    /// `None` when the root does not exist yet (nothing to own) or cannot be resolved.
    pub(super) fn measure(&self) -> Option<Snapshot> {
        let observation = self.tracked.observation();
        // The class row names the configured spelling; custody checks run on the physical
        // root, so an aliased ancestor (a linked home) is resolved once here, not refused.
        let root = match std::fs::canonicalize(self.tracked.path()) {
            Ok(root) => root,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
            Err(error) => {
                crate::clog_warn!(
                    "💾 {} measurement refused: root unresolved: {error}",
                    self.tier
                );
                return None;
            }
        };
        if let Err(error) = CargoTargetPool::owned_metadata(&root, &root) {
            crate::clog_warn!("💾 {} measurement refused: {error}", self.tier);
            return None;
        }
        let (detached, live) = Self::candidates(&root);
        self.sizes
            .lock()
            .retain(|path, _| live.iter().any(|c| &c.path == path));
        let live = live
            .into_iter()
            .map(|candidate| {
                let size = self.measured_size(&candidate);
                (candidate, size)
            })
            .collect();
        Some(Snapshot {
            root,
            observation,
            detached,
            live,
        })
    }

    /// Carry out a plan (blocking): remove detached trees, then the targets outside the
    /// budget oldest first, then, if the plan carries a pressure ask, the oldest kept targets
    /// until it is met. Every target passes every hold check at the moment it is reached.
    pub(super) fn evict(&self, snapshot: &Snapshot, plan: Plan) -> u64 {
        let root = &snapshot.root;
        // What the filesystem actually frees, beside the apparent bytes: per-worktree targets
        // can be APFS clones sharing blocks, so the two differ and both are reported.
        let available_before = fs2::available_space(root).ok();
        let mut freed = 0u64;
        let mut evicted = 0usize;
        for tree in &snapshot.detached {
            freed = freed.saturating_add(Self::remove_detached(tree));
        }
        for (index, (candidate, _)) in snapshot.live.iter().enumerate() {
            let asking = plan.pressure_ask > 0 && freed < plan.pressure_ask;
            if index >= plan.beyond && !asking {
                break;
            }
            let bytes = self.evict_one(root, candidate);
            if bytes > 0 {
                evicted += 1;
                self.sizes.lock().remove(&candidate.path);
            }
            freed = freed.saturating_add(bytes);
        }
        if freed > 0 {
            self.tracked.record_freed(freed);
            self.all_held_reported
                .store(false, std::sync::atomic::Ordering::Relaxed);
            let volume_freed = match (available_before, fs2::available_space(root).ok()) {
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
        } else if !snapshot.live.is_empty()
            && !self
                .all_held_reported
                .swap(true, std::sync::atomic::Ordering::Relaxed)
        {
            crate::clog_warn!(
                "💾 {} freed nothing: the {} worktree target(s) it reached are held (a build lock, a \
                 running image, a writer lease or serving custody)",
                self.tier,
                snapshot.live.len()
            );
        }
        freed
    }
}
