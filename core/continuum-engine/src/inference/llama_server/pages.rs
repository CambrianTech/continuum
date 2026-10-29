//! Pages responsibility of the llama-server process owner.
use super::*;
/// The KV disk-page dir for one MODEL: every page of every mind served by it, kept
/// across relaunches at any window or slot count. Whether a page still fits the slot it
/// is restored into is the engine's check (`Restored prompt does not fit in the slot
/// context`, refused loudly, and the turn re-prefills); whether its KV belongs to this
/// model, cache type and engine is the ledger's ([`crate::inference::slots::KvPageContract::fingerprint`]).
///
/// It used to be one dir per window step (`<model>--c<step>`), and a relaunch into
/// another step swept every other step's dir: the M5 at 2026-10-23 04:06Z went from a
/// 112,384- to a 129,024-token window and deleted 98 GB of pages, every resident's. Rooted
/// under `~/.continuum/cache/` (tracked in `system_resources::disk_reporters`, trimmed by
/// the save-side store budget).
pub fn kv_page_dir(model_id: &str) -> PathBuf {
    let sanitized: String = model_id
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' { c } else { '_' })
        .collect();
    kv_pages_root().join(sanitized)
}


/// The root every page dir lives under — the ONE path the disk reporter tracks.
pub fn kv_pages_root() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(std::env::temp_dir) // JUSTIFIED unwrap_or_else: no home dir = containerized oddity; pages in tmp still work, and the lane must not fail to spawn over cache placement
        .join(".continuum")
        .join("cache")
        .join("kv-pages")
}


/// The per-window dirs a model's pages lived in before one dir per model:
/// `<model dir name>--c<digits>`, siblings of `current`.
fn legacy_page_dirs(current: &Path) -> Vec<PathBuf> {
    let (Some(parent), Some(name)) = (current.parent(), current.file_name().and_then(|n| n.to_str())) else {
        return Vec::new();
    };
    let prefix = format!("{name}--c");
    let Ok(entries) = std::fs::read_dir(parent) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|e| {
            e.file_name().to_str().and_then(|n| n.strip_prefix(&prefix)).is_some_and(|step| {
                !step.is_empty() && step.bytes().all(|b| b.is_ascii_digit())
            }) && e.path().is_dir()
        })
        .map(|e| e.path())
        .collect()
}


/// Whether `dir` already holds pages or a ledger of its own (then nothing moves into it).
fn holds_pages(dir: &Path) -> bool {
    std::fs::read_dir(dir).is_ok_and(|entries| {
        entries.flatten().any(|e| {
            let name = e.file_name();
            let name = name.to_string_lossy();
            name == "saved-ledger.json" || (name.starts_with("a-") && name.contains(".bin"))
        })
    })
}


/// Move the newest legacy per-window dir's files (pages, sidecars, ledger) into the model's
/// dir, once: when that dir holds no pages yet. The legacy dir is chosen by its ledger's
/// write time (the last one a lane used); every other legacy dir is left in place, and a
/// dir another live lane was launched with (`protect`, by NAME, as the census records it)
/// is never touched. Nothing is deleted: files are renamed, and only an emptied legacy dir
/// is removed. Returns the files moved.
pub fn migrate_legacy_page_dir(current: &Path, protect: &[PathBuf]) -> usize {
    if holds_pages(current) {
        return 0;
    }
    let protected: Vec<&std::ffi::OsStr> = protect.iter().filter_map(|p| p.file_name()).collect();
    let written = |dir: &Path| {
        std::fs::metadata(dir.join("saved-ledger.json"))
            .or_else(|_| std::fs::metadata(dir))
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH) // unwrap_or: an undatable dir sorts oldest, the last one chosen
    };
    let legacy = legacy_page_dirs(current);
    let left_in_place = legacy.len().saturating_sub(1) as u64;
    let Some(source) = legacy
        .into_iter()
        .filter(|d| !d.file_name().is_some_and(|n| protected.contains(&n)))
        .max_by_key(|d| written(d))
    else {
        return 0;
    };
    let mut moved = 0usize;
    let mut failed = 0u64;
    if let Ok(entries) = std::fs::read_dir(&source) {
        for entry in entries.flatten() {
            let to = current.join(entry.file_name());
            if !entry.path().is_file() || to.exists() {
                continue;
            }
            match std::fs::rename(entry.path(), &to) {
                Ok(()) => moved += 1,
                Err(_) => failed += 1,
            }
        }
    }
    let removed = std::fs::remove_dir(&source).is_ok();
    continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "inference.kv_page.dir_migrated",
        from = %source.display(),
        to = %current.display(),
        moved = moved as u64,
        failed,
        source_removed = removed,
        other_legacy_dirs = left_in_place,
        "a per-window page dir's pages moved into the model's one page dir: they stay restorable at any window"
    );
    moved
}


/// The revision evidence a saved KV page is keyed by: size and mtime of every file the
/// launch resolved (model, adapters, engine). All of it, or the first path that could not
/// be read and why: a page saved under unknown revisions is never restorable, and the
/// caller names the file instead of a silent `None`.
pub(super) fn revision_evidence(
    paths: impl Iterator<Item = PathBuf>,
) -> Result<Vec<(PathBuf, u64, std::time::SystemTime)>, (PathBuf, std::io::Error)> {
    paths
        .map(|path| match std::fs::metadata(&path).and_then(|m| m.modified().map(|at| (m.len(), at))) {
            Ok((len, modified)) => Ok((path, len, modified)),
            Err(error) => Err((path, error)),
        })
        .collect()
}

/// THE PAGE-DIR GUARD (Astra's re-review of #4069): one mutex owns the reservation
/// list AND is held across the legacy migration's census + move AND across the
/// reservation→record handoff. So, for any two lanes A (migrating) and B (starting):
///   - B's reservation insert and A's census+move are mutually exclusive — B is
///     either fully in the census or fully after the move (in which case the
///     dir A emptied was unowned by definition, and B creates it fresh);
///   - B's "write the record, then drop the reservation" is one critical section,
///     so a census never lands between the two and finds B in neither list.
/// A cloned list outside the lock was the time-of-check/time-of-use hole. Held
/// briefly, never across an await; `parking_lot` so a panic never poisons it.
/// Process-local by design: two CORES spawning the same model at once is the
/// split-brain class (card 27fd5870), not this seam.
pub(super) static PAGE_DIR_GUARD: parking_lot::Mutex<PageDirState> = parking_lot::Mutex::new(PageDirState {
    reservations: Vec::new(),
    retiring: Vec::new(),
});


pub(super) struct PageDirState {
    pub(super) reservations: Vec<PathBuf>,
    pub(super) retiring: Vec<RetiringPageOwner>,
}


pub(super) struct RetiringPageOwner {
    pub(super) child: tokio::process::Child,
    pub(super) generation: crate::inference::slots::EngineGeneration,
    // None means the registry carried the directory, but its current readability
    // and OS identity lookup are not proof that this owned child has exited.
    pub(super) reserved_dir: Option<PathBuf>,
    // Released only by collect_exited after the actual child reports termination.
    pub(super) _resource_reservation: Option<crate::resources::LeaseGuard>,
    pub(super) _engine_install: Option<crate::inference::engine_install::EngineInstallReceipt>,
    pub(super) _local_inputs: Option<crate::inference::launch_inputs::LocalLaunchInputs>,
    // An engine slot claim the lane record could not take over: released at exit.
    pub(super) _engine_slot: Option<crate::inference::engine_slots::SlotClaim>,
}


impl PageDirState {
    pub(super) fn has_unknown_retiring_dir(&self) -> bool {
        self.retiring
            .iter()
            .any(|owner| owner.reserved_dir.is_none())
    }

    pub(super) fn collect_exited(&mut self) {
        self.collect_exited_with(tokio::process::Child::try_wait);
    }

    pub(super) fn collect_exited_with(
        &mut self,
        mut poll: impl FnMut(
            &mut tokio::process::Child,
        ) -> std::io::Result<Option<std::process::ExitStatus>>,
    ) {
        let mut index = 0;
        while index < self.retiring.len() {
            match poll(&mut self.retiring[index].child) {
                Ok(Some(_)) => {
                    let owner = self.retiring.swap_remove(index);
                    owner.generation.observed_exit();
                    if let Some(dir) = owner.reserved_dir {
                        if let Some(i) = self.reservations.iter().position(|d| d == &dir) {
                            self.reservations.swap_remove(i);
                        }
                    }
                }
                Ok(None) | Err(_) => index += 1,
            }
        }
    }
}


pub(super) fn retire_page_owner(
    owned: OwnedEngine,
    reservation: Option<PageDirReservation>,
    engine_slot: Option<crate::inference::engine_slots::SlotClaim>,
    signal: impl FnOnce(&mut tokio::process::Child) -> std::io::Result<()>,
) {
    let OwnedEngine {
        mut child,
        generation,
        resource_reservation,
        engine_install,
        local_inputs,
        ..
    } = owned;
    generation.retiring();
    let mut guard = PAGE_DIR_GUARD.lock();
    if let Err(e) = signal(&mut child) {
        tracing::warn!(target: "continuum_core::inference::llama_server", error = %e, "could not signal llama-server child; retaining page ownership until verified exit");
    }
    // Disarm RAII while holding the same guard: no unlocked interval, and no
    // recursive lock in PageDirReservation::drop. The reservation stays listed.
    let reserved_dir = reservation.and_then(|mut reservation| reservation.0.take());
    guard.retiring.push(RetiringPageOwner {
        child,
        generation,
        reserved_dir,
        _resource_reservation: resource_reservation,
        _engine_install: engine_install,
        _local_inputs: local_inputs,
        _engine_slot: engine_slot,
    });
    guard.collect_exited();
}


/// Poll the existing retained-child collector from the serving owner's tick.
/// Unknown status retains both page ownership and the child's resource lease.
pub fn collect_retired_engines() {
    PAGE_DIR_GUARD.lock().collect_exited();
}


/// RAII reservation of a page dir for a spawn in flight, taken the instant the dir is
/// decided — BEFORE it is created or used ([`Self::take_and_create`]), so no sibling's
/// legacy migration can move its pages between `mkdir` and the reservation. Released by
/// [`Self::handoff`] once the lane's registry record carries the dir, or on drop if
/// the spawn unwinds first. A lane whose record write FAILED keeps its reservation
/// for the CHILD's lifetime: the spawn parks it beside `child` on the process
/// ([`LlamaServerProcess::retained_page_dir`]) and retirement releases it only
/// after the owned child reports exit — so
/// registry persistence failing never unprotects a running lane, and a `serve()`
/// that returns (or is cancelled) does not drop the protection its child still needs.
pub(super) struct PageDirReservation(pub(super) Option<PathBuf>);


impl PageDirReservation {
    pub(super) fn take(dir: &Path) -> Self {
        PAGE_DIR_GUARD.lock().reservations.push(dir.to_path_buf());
        Self(Some(dir.to_path_buf()))
    }

    /// The production ordering, in one place so the regression can exercise it:
    /// reserve FIRST, then create. Returns the reservation and whether the dir is a
    /// directory now (`false` = mkdir failed; the paging tier is amputated for this
    /// serve and the caller neither trims nor migrates).
    pub(super) fn take_and_create(dir: &Path) -> (Self, bool) {
        let reservation = Self::take(dir);
        if let Err(e) = std::fs::create_dir_all(dir) {
            tracing::warn!(target: "continuum_core::inference::llama_server", probe_class = "inference.kv_page.dir_failed",
                dir = %dir.display(),
                error = %e,
                "could not create the KV page dir — the lane still serves, but rotation \
                 pays full re-prefill (the paging tier is amputated for this serve)"
            );
        }
        let ready = std::fs::metadata(dir).is_ok_and(|m| m.is_dir());
        (reservation, ready)
    }

    /// Write the record and release the reservation under ONE guard: a census in
    /// another lane sees the reservation or the record, never neither. A failed
    /// record write keeps the reservation (returned `Err`, reservation intact).
    pub(super) fn handoff(
        &mut self,
        rec: &crate::inference::lane_registry::LaneRecord,
    ) -> std::io::Result<()> {
        let dir = crate::inference::lane_registry::lanes_dir().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, "no home dir for lane registry")
        })?;
        self.handoff_in(rec, &dir)
    }

    /// [`Self::handoff`] against an explicit registry dir — the test seam, so a
    /// regression never writes the canonical `~/.continuum/run/lanes/`.
    pub(super) fn handoff_in(
        &mut self,
        rec: &crate::inference::lane_registry::LaneRecord,
        registry_dir: &Path,
    ) -> std::io::Result<()> {
        let mut guard = PAGE_DIR_GUARD.lock();
        crate::inference::lane_registry::record_in(registry_dir, rec)?;
        if let Some(dir) = self.0.take() {
            if let Some(i) = guard.reservations.iter().position(|d| d == &dir) {
                guard.reservations.swap_remove(i);
            }
        }
        Ok(())
    }

    /// Still protecting a dir (the record write failed, or no handoff yet)?
    pub(super) fn is_held(&self) -> bool {
        self.0.is_some()
    }
}


impl Drop for PageDirReservation {
    fn drop(&mut self) {
        if let Some(dir) = self.0.take() {
            let mut guard = PAGE_DIR_GUARD.lock();
            if let Some(i) = guard.reservations.iter().position(|d| d == &dir) {
                guard.reservations.swap_remove(i);
            }
        }
    }
}


/// Census + migration under the guard: the protected set (live records ∪ reservations)
/// is computed and consumed while no reservation can be inserted or handed off. `None` =
/// nothing moved: the inventory was incomplete (a live record without a dir) or could not
/// be established at all (an unreadable or malformed record — the census fails CLOSED; a
/// lane the census cannot see is a lane whose pages must not move under it).
pub(super) fn migrate_legacy_page_dir_guarded(current: &Path) -> Option<usize> {
    let registry = crate::inference::lane_registry::lanes_dir()?;
    migrate_legacy_page_dir_guarded_in(current, &registry)
}


/// [`migrate_legacy_page_dir_guarded`] against an explicit registry dir (the test seam).
pub(super) fn migrate_legacy_page_dir_guarded_in(current: &Path, registry_dir: &Path) -> Option<usize> {
    let mut guard = PAGE_DIR_GUARD.lock();
    guard.collect_exited();
    if guard.has_unknown_retiring_dir() {
        continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "inference.kv_page.migration_skipped",
            reason = "owned child exit unproven and page directory unknown",
            retiring_children = guard.retiring.len(),
            "legacy page dir migration skipped until owned child exit is observed"
        );
        return None;
    }
    let records = match crate::inference::lane_registry::records_checked_in(registry_dir) {
        Ok(r) => r,
        Err(e) => {
            continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "inference.kv_page.migration_skipped",
                reason = "lane registry unreadable",
                error = e.to_string().as_str(),
                "legacy page dir migration skipped — the live-lane inventory could not be established"
            );
            return None;
        }
    };
    let (mut protect, complete) =
        page_dirs_of(records, crate::inference::lane_process::is_llama_server);
    protect.extend(guard.reservations.iter().cloned());
    if !complete {
        return None;
    }
    Some(migrate_legacy_page_dir(current, &protect))
}


/// The pure half of the guarded census: of `records`, those `alive` contribute their
/// dir; a live record WITHOUT one makes the inventory incomplete; dead records are
/// ignored whether or not they carry a dir.
pub fn page_dirs_of(
    records: Vec<crate::inference::lane_registry::LaneRecord>,
    alive: impl Fn(u32) -> bool,
) -> (Vec<PathBuf>, bool) {
    let mut dirs = Vec::new();
    let mut complete = true;
    for rec in records {
        if !alive(rec.pid) {
            continue;
        }
        match rec.page_dir {
            Some(d) => dirs.push(d),
            None => complete = false,
        }
    }
    (dirs, complete)
}
