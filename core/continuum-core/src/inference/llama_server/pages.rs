//! Pages responsibility of the llama-server process owner.
use super::*;
/// The KV disk-page dir for one serve GEOMETRY (restore economy): a page saved
/// under one model + per-slot window must never restore into another. The
/// geometry is IN THE PATH so mismatch is structurally impossible — a new
/// geometry simply reads an empty dir. Rooted under `~/.continuum/cache/`
/// (tracked in `system_resources::disk_reporters`, eviction decided in
/// `disk_eviction` — the no-new-cache-dir-without-an-eviction-decision law).
pub fn kv_page_dir(model_id: &str, per_slot_ctx: u32) -> PathBuf {
    let sanitized: String = model_id
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' { c } else { '_' })
        .collect();
    kv_pages_root().join(format!("{sanitized}--c{}", page_geometry_key(per_slot_ctx)))
}


/// The per-slot window is derived from LIVE memory at every plan, so it jitters by
/// a few hundred tokens between relaunches (measured 2026-09-15 on the M5:
/// 59171 → 58997 → 58273 → 57758 → 57112 in five hours). Keying the page dir on
/// the exact number minted a NEW generation on every relaunch and swept the
/// previous one — every resident's page gone, every eviction a 24–66 s re-prefill
/// while a restore of the same state costs ~0.36 s. The key is the window rounded
/// DOWN to [`PAGE_GEOMETRY_STEP`]: relaunches within a step share pages. A page
/// restores into any slot large enough to hold it; the rare page from a slightly
/// larger slot that no longer fits fails the restore once (`note_page_lost`) and
/// that turn re-prefills — the same cost as today's worst case, paid rarely.
/// 16k: the five-hour band above (57112–59171) is one key; the 8-lane band
/// (25104–26531) another; a 4096 step still split the measured jitter.
pub const PAGE_GEOMETRY_STEP: u32 = 16_384;


pub fn page_geometry_key(per_slot_ctx: u32) -> u32 {
    (per_slot_ctx / PAGE_GEOMETRY_STEP).max(1) * PAGE_GEOMETRY_STEP
}


/// The root every geometry dir lives under — the ONE path the disk reporter
/// tracks and the spawn sweep walks.
pub fn kv_pages_root() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(std::env::temp_dir) // JUSTIFIED unwrap_or_else: no home dir = containerized oddity; pages in tmp still work, and the lane must not fail to spawn over cache placement
        .join(".continuum")
        .join("cache")
        .join("kv-pages")
}


/// Sweep sibling GENERATIONS of the current geometry dir: same model prefix,
/// different `--c<n>` suffix. Their pages can never be restored again (the
/// geometry is gone), so the spawn — the one place that knows the new truth —
/// deletes them. This is the eviction story `disk_eviction` records for
/// `cache/kv-pages`; cross-model dirs are left for their own lanes' spawns.
pub fn sweep_stale_page_generations(current: &Path, protect: &[PathBuf]) -> usize {
    let mut swept = 0usize;
    let (Some(parent), Some(name)) = (current.parent(), current.file_name().and_then(|n| n.to_str()))
    else {
        return 0;
    };
    // Protected dirs are siblings in this same root by construction, so compare the
    // DIRECTORY NAME, never the full path: a registry path deserialized from an older
    // core (a different HOME resolution, macOS `/Users` vs `/private/var`, a trailing
    // component) would miss on PathBuf equality and the miss would look exactly like a
    // correct sweep — the five-hour silent failure this fix exists to end (Cormac's
    // review of #4069, finding 1).
    let protected_names: Vec<&std::ffi::OsStr> = protect.iter().filter_map(|p| p.file_name()).collect();
    let Some((model_prefix, _)) = name.rsplit_once("--c") else {
        return 0;
    };
    let Ok(entries) = std::fs::read_dir(parent) else {
        return 0;
    };
    for entry in entries.flatten() {
        let entry_name = entry.file_name();
        let Some(entry_name) = entry_name.to_str() else {
            continue;
        };
        if entry_name != name
            && entry_name.starts_with(model_prefix)
            && entry_name[model_prefix.len()..].starts_with("--c")
        {
            let stale = entry.path();
            if protected_names.iter().any(|p| *p == entry.file_name().as_os_str()) {
                // Another live lane's pages: a relaunch that later fails leaves that
                // lane serving, and its residents restoring from here.
                continue;
            }
            let removed = std::fs::remove_dir_all(&stale).is_ok();
            swept += usize::from(removed);
            crate::probe!(target: "continuum_core::inference::llama_server", class = "inference.kv_page.generation_swept",
                dir = %stale.display(),
                removed,
                protected = protected_names.len() as u64,
                "stale KV page generation removed — its geometry no longer exists to restore into",
            );
        }
    }
    swept
}


/// THE PAGE-DIR GUARD (Astra's re-review of #4069): one mutex owns the reservation
/// list AND is held across the sweep's census + deletion AND across the
/// reservation→record handoff. So, for any two lanes A (sweeping) and B (starting):
///   - B's reservation insert and A's census+delete are mutually exclusive — B is
///     either fully in the census or fully after the deletion (in which case the
///     dir A deleted was unowned by definition, and B creates it fresh);
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
    });
    guard.collect_exited();
}


/// Poll the existing retained-child collector from the serving owner's tick.
/// Unknown status retains both page ownership and the child's resource lease.
pub(crate) fn collect_retired_engines() {
    PAGE_DIR_GUARD.lock().collect_exited();
}


/// RAII reservation of a page dir for a spawn in flight, taken the instant the dir is
/// decided — BEFORE it is created or used ([`Self::take_and_create`]), so no sibling
/// sweep can delete it between `mkdir` and the reservation. Released by
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
    /// serve and the caller neither trims nor sweeps).
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


/// Census + destructive sweep under the guard: the protected set (live records ∪
/// reservations) is computed and consumed while no reservation can be inserted or
/// handed off. `None` = nothing swept: the inventory was incomplete (a live record
/// without a dir) or could not be established at all (an unreadable or malformed
/// record — the census fails CLOSED; a lane the sweep cannot see is a lane whose
/// pages it must not delete).
pub(super) fn sweep_stale_page_generations_guarded(current: &Path) -> Option<usize> {
    let registry = crate::inference::lane_registry::lanes_dir()?;
    sweep_stale_page_generations_guarded_in(current, &registry)
}


/// [`sweep_stale_page_generations_guarded`] against an explicit registry dir (the
/// test seam).
pub(super) fn sweep_stale_page_generations_guarded_in(current: &Path, registry_dir: &Path) -> Option<usize> {
    let mut guard = PAGE_DIR_GUARD.lock();
    guard.collect_exited();
    if guard.has_unknown_retiring_dir() {
        crate::probe!(target: "continuum_core::inference::llama_server", class = "inference.kv_page.sweep_skipped",
            reason = "owned child exit unproven and page directory unknown",
            retiring_children = guard.retiring.len(),
            "stale-generation sweep skipped until owned child exit is observed"
        );
        return None;
    }
    let records = match crate::inference::lane_registry::records_checked_in(registry_dir) {
        Ok(r) => r,
        Err(e) => {
            crate::probe!(target: "continuum_core::inference::llama_server", class = "inference.kv_page.sweep_skipped",
                reason = "lane registry unreadable",
                error = e.to_string().as_str(),
                "stale-generation sweep skipped — the live-lane inventory could not be established"
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
    Some(sweep_stale_page_generations(current, &protect))
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
