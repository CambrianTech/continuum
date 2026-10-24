use super::*;

impl ResourcePool for CargoTargetPool {
    fn tier_name(&self) -> &str {
        &self.tier
    }

    fn capacity_bytes(&self) -> u64 {
        self.budget_for_volume((self.volume_sample)(self.tracked.path()).as_ref())
    }

    fn usage_bytes(&self) -> u64 {
        self.tracked.bytes()
    }

    /// A CACHE ON A CRITICAL VOLUME IS A CRITICAL POOL. The broker relieves a pool
    /// only when ITS pressure crosses `act_above`; this pool's own pressure is
    /// usage over a budget derived from the volume's TOTAL, so it cannot see a
    /// volume filled by other classes. Measured 2026-09-18 on the 5090 the moment
    /// the disk daemon could see at all: volume `critical` (95 %+, 92 GB of
    /// 1.9 TB free), cargo-target 0.34 of budget, cargo-target-wt 0.65 — both
    /// below the act line, nothing evicted, the disk-root signal pool alerting
    /// "hot and stuck" every tick. A derived artifact cache is exactly what a
    /// critical volume should shed first, so the volume's pressure is this pool's
    /// floor: over budget OR on a hot volume, either one brings the broker.
    fn pressure(&self) -> f64 {
        self.pressure_for_volume((self.volume_sample)(self.tracked.path()).as_ref())
    }

    fn evict_at_least(&self, want_bytes: u64) -> u64 {
        let root = self.tracked.path().to_path_buf();
        let citizen_custody = self.tracked.eviction_custody();
        let _hands = if let Some(peer) = *citizen_custody {
            let Some(hands) = PersonaHandsLease::try_take(peer) else {
                return self.declined(DeclinedEviction::CitizenBusy("hands are in use".into()));
            };
            let home = match crate::paths::continuum_home_trial_renamed() {
                Ok(home) => home,
                Err(error) => return self.declined(DeclinedEviction::CitizenBusy(error.to_string())),
            };
            if let Err(error) = (self.citizen_quiescence)(&home) {
                return self.declined(DeclinedEviction::CitizenBusy(error));
            }
            Some((hands, home))
        } else { None };
        if !root.exists() {
            return self.declined(DeclinedEviction::NoSuchDirectory);
        }
        // Validate lock locations before opening them: a profile or lock symlink
        // must not redirect custody to an unrelated file outside this cache.
        for path in [&root, &root.join(".cargo-lock"), &root.join("debug"),
            &root.join("debug/.cargo-lock"), &root.join("release"), &root.join("release/.cargo-lock")] {
            match std::fs::symlink_metadata(path) {
                Ok(_) => if let Err(error) = Self::owned_metadata(path, &root) {
                    return self.declined(DeclinedEviction::UnsafePath(error));
                },
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {},
                Err(error) => return self.declined(DeclinedEviction::UnsafePath(error.to_string())),
            }
        }
        // Safety invariant 1: hold cargo's lock files exclusively for the
        // whole eviction, or do nothing. Guards held in scope until return.
        let _root_lock = match try_exclusive_flock_detailed(&root.join(".cargo-lock")) {
            LockAttempt::Acquired(lock) => lock,
            LockAttempt::HeldByAnother => {
                return self.declined(DeclinedEviction::BuildHoldsRootLock)
            }
            LockAttempt::Unavailable(why) => {
                return self.declined(DeclinedEviction::LockUnavailable(why))
            }
        };
        let mut profile_locks = Vec::new();
        for (profile, held_reason) in [
            ("debug", DeclinedEviction::BuildHoldsDebugLock),
            ("release", DeclinedEviction::BuildHoldsReleaseLock),
        ] {
            let lock_path = root.join(profile).join(".cargo-lock");
            if lock_path.parent().is_some_and(Path::exists) {
                match try_exclusive_flock_detailed(&lock_path) {
                    LockAttempt::Acquired(lock) => profile_locks.push(lock),
                    LockAttempt::HeldByAnother => return self.declined(held_reason),
                    LockAttempt::Unavailable(why) => return self.declined(DeclinedEviction::LockUnavailable(why)),
                }
            }
        }

        let mut freed = 0u64;
        let debug = root.join("debug");
        let release = root.join("release");
        for rung in Self::ladder(&root) {
            if let Some((_, home)) = &_hands {
                if let Err(error) = (self.citizen_quiescence)(home) {
                    self.declined(DeclinedEviction::CitizenBusy(error));
                    break;
                }
            }
            if freed >= want_bytes.max(1) {
                break;
            }
            if !rung.exists() {
                continue;
            }
            let observation = self.tracked.observation();
            if rung == release && self.release_observation.load(std::sync::atomic::Ordering::Relaxed) == observation {
                self.declined(DeclinedEviction::AwaitingReleaseMeasurement);
                continue;
            }
            if let Err(error) = Self::validate_tree(&rung, &root) {
                self.declined(DeclinedEviction::UnsafePath(error));
                continue;
            }
            // Cargo releases its locks before running the produced image. Protect
            // the whole profile, including native build outputs/dependencies, while
            // any image from it executes; the same rule applies to both profiles.
            if rung == debug || rung == release {
                if let Some((pid, exe)) = process_executing_under(&rung) {
                    self.declined(DeclinedEviction::ProfileInUse { pid, exe });
                    continue;
                }
            }
            if self.active_artifacts.protects(&rung) {
                self.declined(DeclinedEviction::ActiveArtifact { path: rung });
                continue;
            }
            if rung == release {
                self.release_observation.store(observation, std::sync::atomic::Ordering::Relaxed);
            }
            freed = freed.saturating_add(Self::free_rung(&rung, &root));
            if rung == release && freed < want_bytes.max(1) {
                let bytes = dir_size_bytes(&release);
                if bytes > 0 { self.declined(DeclinedEviction::ReleaseInputsRetained { bytes }); }
            }
        }
        if freed > 0 {
            self.tracked.record_freed(freed);
            crate::clog_warn!(
                "💾 cargo-target eviction freed {} GB (budget {} GB) — derived artifacts only; next build recreates them",
                freed / (1024 * 1024 * 1024),
                self.capacity_bytes() / (1024 * 1024 * 1024)
            );
            self.note_eviction();
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
    pub(super) paths: RwLock<std::collections::HashMap<PathBuf, Option<PathBuf>>>,
}

impl ActiveArtifactSet {
    /// Raw identities without physical proof cannot authorize any reclamation.
    pub fn register(&self, path: PathBuf) {
        if let Ok(mut set) = self.paths.write() {
            set.insert(path, None);
        }
    }

    /// Resolve before the synchronous serving admission, through the shared bounded
    /// path worker. Failed resolution is published as unresolved custody, never guessed.
    pub async fn resolve(path: &Path) -> Option<PathBuf> {
        if !path.is_absolute() { return None; }
        match super::super::disk_reporters::resolve_existing_storage_path(path).await {
            Ok(physical) => Some(physical),
            Err(error) => {
                crate::clog_warn!("active artifact identity unresolved: {}", error);
                None
            }
        }
    }

    pub fn register_resolved(&self, path: PathBuf, physical: Option<PathBuf>) {
        if let Ok(mut set) = self.paths.write() {
            let physical = physical.filter(|resolved| path.is_absolute() && resolved.is_absolute());
            set.insert(path, physical);
        }
    }

    pub fn replace(&self, old: Option<&Path>, new: Option<(PathBuf, Option<PathBuf>)>) {
        if let Ok(mut set) = self.paths.write() {
            if let Some(old) = old { set.remove(old); }
            if let Some((raw, physical)) = new {
                let physical = physical.filter(|resolved| raw.is_absolute() && resolved.is_absolute());
                set.insert(raw, physical);
            }
        }
    }

    pub fn release(&self, path: &Path) {
        if let Ok(mut set) = self.paths.write() {
            set.remove(path);
        }
    }

    /// `candidate` must be resolved by its eviction owner before this lock-only check.
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
        set.values().any(|physical| match physical {
            None => true,
            Some(active) => !active.is_absolute() || !candidate.is_absolute()
                || crate::paths::has_path_prefix(active, candidate)
                || crate::paths::has_path_prefix(candidate, active),
        })
    }
}

/// Process-wide active-artifact registry, one per process (same singleton
/// shape as `install_tracked_dirs`). Serving marks residency here; the
/// broker-registered pool reads it. Lazily created so tests and tools get
/// a working (empty) set without boot wiring.
pub(super) static SERVING_ACTIVE_ARTIFACTS: std::sync::OnceLock<Arc<ActiveArtifactSet>> =
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
    pub(super) tracked: Arc<TrackedDir>,
    pub(super) capacity_bytes: u64,
    pub(super) cold_root: Option<PathBuf>,
    pub(super) active: Arc<ActiveArtifactSet>,
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
    pub(super) fn candidates_coldest_first(root: &Path) -> Vec<(PathBuf, u64)> {
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
pub(super) fn files_identical(a: &Path, b: &Path) -> bool {
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
pub(super) fn entries_identical(a: &Path, b: &Path) -> bool {
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
pub(super) fn copy_entry_durable(src: &Path, dst: &Path) -> std::io::Result<()> {
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

pub(super) fn remove_entry(path: &Path) -> std::io::Result<()> {
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
            let Ok(physical) = std::fs::canonicalize(&path) else { continue };
            if self.active.protects(&physical) {
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
