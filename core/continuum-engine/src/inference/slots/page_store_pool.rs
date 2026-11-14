//! The governed eviction owner of the KV page DISK tier (`kv-pages`).
//!
//! A mind's warm KV lives in three tiers: a slot on the GPU, the engine's host cache in RAM,
//! and her saved page on disk (`<kv-pages>/<model dir>/a-<persona>.bin` with its `.ckpt` and
//! `.lora` sidecars). The disk tier is the cheap one, so it is generous; it is never
//! ungoverned. Measured on the M5, 2026-11-10: the class held 30 GB of which ~3.1 GB was
//! restorable (four confirmed minds); 19.4 GB were pages no ledger confirmed and 7.4 GB a
//! model that was not served, and nothing could take a byte back under disk pressure because
//! the class was DEFERRED (`disk_eviction/eviction_decisions.rs`).
//!
//! This pool is that owner. The broker asks it for bytes; it frees them in VALUE order, the
//! cheapest regret first:
//!
//! 1. pages in a model dir no live slot pool serves (another model, an old geometry dir):
//!    nothing on this node can restore them now;
//! 2. in a served dir, pages the slot pool does not confirm (a mind its ledger forgot, a page
//!    from before a restart that never re-confirmed): a restore would refuse them anyway;
//! 3. only then confirmed pages of minds not resident in a slot, coldest (file mtime) first,
//!    each FORGOTTEN through its owner first (`note_page_lost`) so the ledger never confirms a
//!    page that is gone: her next return re-prefills once. This rung answers only pressure the
//!    store itself causes (its own budget) or a volume about to run out ([`ColdPages`]): a
//!    confirmed page is a mind's warm context, and a volume other classes filled is theirs to
//!    relieve. On the M5, 2026-11-13 22:40Z-00:50Z, the volume sat at 85% with 143 GB free,
//!    the store held ~3 GB of a 99 GB budget, and the broker's High-tier request took 29 minds'
//!    pages (one each time a mind left her slot): at the 23:45Z deploy no mind had a page, and
//!    every first act after it was a full prefill (`cold_turn cause="no_page"`).
//!
//! What it never removes: `saved-ledger.json` (only `a-*.bin` pages and their sidecars are
//! candidates), a resident mind's page, and any served page while page IO is in flight, the
//! node is stopping or an endpoint is mid-transition (a save in progress writes a page its
//! ledger has just forgotten; deleting under it would cost the save). Unserved dirs wait while
//! an engine is launching or before any engine serves, because the dir it will serve is not
//! known yet. The save-side trim (`trim_page_store_after_save`) still bounds the one live dir;
//! this pool is what the broker can lower.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use super::citizen_slots::{pages_in, PAGE_DIR, PAGE_SIDECARS};
use super::{page_filename, KvSlotPool, PageKey, Probed};
use crate::paging::broker::PressureTier;
use crate::paging::pool::{ResourcePool, ResourcePoolEntry};
use crate::runtime::admission_gate::AdmissionGate;
use crate::system_resources::disk_eviction::{
    cache_pool_pressure, cargo_target_budget_bytes, DEFAULT_CARGO_TARGET_BUDGET_BYTES,
};
use crate::system_resources::drives::DriveInfo;
use crate::system_resources::{DiskPressureMonitor, TrackedDir};

/// The broker's owner of the `kv-pages` class. Shares its [`TrackedDir`] with the disk
/// reporter: one measurement, two consumers (as `CargoTargetPool` and `RotationLogPool` do).
pub struct KvPageStorePool {
    tracked: Arc<TrackedDir>,
    tier: String,
    /// The last pass's decline: a pass that frees nothing for the same reason as the one before
    /// is not news (the broker asks every tick while the volume stays hot).
    last_decline: parking_lot::Mutex<Option<Decline>>,
}

impl KvPageStorePool {
    pub fn new(tracked: Arc<TrackedDir>) -> Self {
        let tier = format!("disk-{}", tracked.name());
        Self { tracked, tier, last_decline: parking_lot::Mutex::new(None) }
    }

    /// The store's own budget pressure and its volume's, read once for one decision.
    fn pressures(&self) -> StorePressure {
        let volume = DiskPressureMonitor::volume_for(self.tracked.path());
        let own = self.tracked.bytes() as f64 / budget_bytes(volume.as_ref()).max(1) as f64;
        let volume = volume
            .filter(|volume| volume.total_bytes > 0)
            .map_or(0.0, |volume| volume.total_bytes.saturating_sub(volume.available_bytes) as f64 / volume.total_bytes as f64);
        StorePressure { own, volume }
    }
}

/// The store's own budget pressure and the pressure of the volume it sits on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct StorePressure {
    pub(crate) own: f64,
    pub(crate) volume: f64,
}

/// Whether a pass may give back confirmed minds' pages (rung 3). Losing one costs her a full
/// re-prefill of her context (43k tokens: ~69 s at the M5's four-mind prefill rate, against a
/// ~1 s restore of its 1.3 GB page: `fixtures/dyno/2026-11-13-restart.json`), so it yields only
/// to the store's own pressure or a volume at the broker's critical tier, never to a hot volume
/// filled by other classes. Dead pages (rungs 1 and 2) are given back under any pressure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ColdPages {
    /// The store is over its own act threshold, or the volume is critical: the coldest
    /// confirmed pages go, each forgotten first.
    Yield,
    /// Kept: the pressure is the volume's, below critical, and other classes fill it.
    Keep,
}

impl ColdPages {
    /// The decision for one pass, on the broker's own tiers: the store acts on its own bytes
    /// from the tier the broker acts at (High), and on the volume's only at Critical, where the
    /// broker relieves every pool at once.
    pub(crate) fn under(pressure: StorePressure) -> Self {
        let own = PressureTier::for_pressure(pressure.own);
        let volume = PressureTier::for_pressure(pressure.volume);
        if matches!(own, PressureTier::High | PressureTier::Critical) || volume == PressureTier::Critical {
            Self::Yield
        } else {
            Self::Keep
        }
    }
}

/// The class budget, DERIVED from the volume that holds it: the derived-cache policy every
/// re-creatable disk cache on this node already uses (`cargo_target_budget_bytes`: 10% of the
/// volume, floored at `DEFAULT_CARGO_TARGET_BUDGET_BYTES`). Pages are re-creatable (a lost page
/// costs one re-prefill), so they share that policy rather than a number of their own. A volume
/// the monitor has not sampled yet takes the floor, the same rule the cargo pools' registration
/// applies.
fn budget_bytes(volume: Option<&DriveInfo>) -> u64 {
    volume.map_or(DEFAULT_CARGO_TARGET_BUDGET_BYTES, |volume| cargo_target_budget_bytes(volume.total_bytes))
}

impl ResourcePool for KvPageStorePool {
    fn tier_name(&self) -> &str {
        &self.tier
    }

    fn capacity_bytes(&self) -> u64 {
        budget_bytes(DiskPressureMonitor::volume_for(self.tracked.path()).as_ref())
    }

    fn usage_bytes(&self) -> u64 {
        self.tracked.bytes()
    }

    /// A cache on a hot volume is a hot pool (`cache_pool_pressure`): the class's own budget
    /// cannot see a volume other classes filled. An unsampled volume adds no pressure of its
    /// own; the class's budget pressure still counts.
    fn pressure(&self) -> f64 {
        let pressure = self.pressures();
        cache_pool_pressure(pressure.own, pressure.volume)
    }

    fn evict_at_least(&self, want_bytes: u64) -> u64 {
        let gate = super::page_io::gate();
        let pressure = self.pressures();
        let cold_pages = ColdPages::under(pressure);
        let evicted = evict_page_store(self.tracked.path(), &StoreView::now(), gate, want_bytes, cold_pages);
        let freed = evicted.freed();
        if freed > 0 {
            // The scanner walks every few minutes and the broker ticks every 5 s: without this
            // the pool reads its pre-eviction size and is asked again for bytes already gone.
            self.tracked.record_freed(freed);
        }
        let repeated = std::mem::replace(&mut *self.last_decline.lock(), evicted.declined) == evicted.declined;
        if freed > 0 || (evicted.declined.is_some() && !repeated) {
            continuum_base::probe!(
                class = "inference.kv_page.store_evicted",
                want_bytes,
                own_pressure = pressure.own,
                volume_pressure = pressure.volume,
                cold_pages = ?cold_pages,
                unserved_bytes = evicted.unserved,
                unconfirmed_bytes = evicted.unconfirmed,
                cold_bytes = evicted.cold,
                cold_minds_forgotten = evicted.forgotten as u64,
                declined = evicted.declined.map_or("", Decline::as_str),
                "the KV page store gave bytes back in value order: unserved model dirs, then pages no \
                 ledger confirms, then (only under its own pressure or a critical volume) the coldest \
                 non-resident confirmed pages, each forgotten first"
            );
        }
        freed
    }

    fn snapshot(&self) -> Vec<ResourcePoolEntry> {
        let view = StoreView::now();
        let groups = view.groups();
        let Ok(dirs) = page_dirs(self.tracked.path()) else {
            return Vec::new(); // an unreadable root lists nothing; eviction names the refusal
        };
        dirs.iter()
            .flat_map(|dir| {
                let resident = groups.iter().find(|g| g.dir == *dir).map(|g| g.resident_pages()).unwrap_or_default(); // unwrap_or_default: an unserved dir has no resident page
                listed(dir).into_iter().map(move |page| ResourcePoolEntry {
                    key: page.dir.join(&page.name).display().to_string(),
                    size_bytes: page.bytes,
                    pinned_count: u32::from(resident.contains(&page.name)),
                    // The page's mtime orders eviction; it is not restated as a load or access
                    // clock it never was. 0 = not a clock-derived value.
                    loaded_at: 0,
                    last_access_at: 0,
                    access_count: 0,
                })
            })
            .collect()
    }
}

/// What the slot directory says about the page store at one moment.
#[derive(Default)]
pub(super) struct StoreView {
    /// Every live slot pool with the page dir it saves into.
    pub(super) served: Vec<(PathBuf, Arc<KvSlotPool>)>,
    /// Dirs kept whatever the pools say: the live geometry dir the last ready engine noted.
    pub(super) protected: Vec<PathBuf>,
    /// An engine is launching (its page dir is not known until it is ready), or no engine
    /// serves yet: no dir can be proven unserved.
    pub(super) unproven: bool,
    /// An endpoint is mid-transition (checkpoint saves run outside the page-IO gate).
    pub(super) busy: bool,
}

impl StoreView {
    /// The process's slot directory now. Endpoints are collected before any state lock is
    /// taken, so no shard lock of the directory is held across one.
    fn now() -> Self {
        let mut view = StoreView::default();
        if let Some(dir) = PAGE_DIR.lock().clone() {
            view.protected.push(dir);
        }
        let endpoints: Vec<Arc<super::EndpointSlots>> =
            super::directory().endpoints.iter().map(|entry| Arc::clone(entry.value())).collect();
        for endpoint in endpoints {
            let state = endpoint.state.lock();
            if !*state.ready.borrow() {
                view.busy = true;
            }
            if state.generation().is_some_and(|generation| !generation.has_exited() && state.owned_by(generation.id).is_none()) {
                view.unproven = true;
            }
            let pool = match state.pool() {
                Probed::Known(pool) => pool,
                Probed::NotAsked | Probed::Absent(_) => continue,
            };
            let dir = pool
                .ledger
                .get()
                .and_then(|ledger| ledger.path.parent().map(Path::to_path_buf))
                .or_else(|| state.owned().and_then(|owned| owned.contract.page_dir.clone()));
            if let Some(dir) = dir {
                view.served.push((dir, Arc::clone(pool)));
            }
        }
        if view.served.is_empty() {
            view.unproven = true;
        }
        view
    }

    /// The served dirs, each with every pool saving into it (two lanes of one model share a dir).
    fn groups(&self) -> Vec<DirGroup> {
        let mut groups: Vec<DirGroup> = Vec::new();
        for (dir, pool) in &self.served {
            match groups.iter_mut().find(|g| g.dir == *dir) {
                Some(group) => group.pools.push(Arc::clone(pool)),
                None => groups.push(DirGroup { dir: dir.clone(), pools: vec![Arc::clone(pool)] }),
            }
        }
        groups
    }
}

/// One served page dir and the pools that save into it.
struct DirGroup {
    dir: PathBuf,
    pools: Vec<Arc<KvSlotPool>>,
}

impl DirGroup {
    /// Page filename -> key, for every page a pool confirms (minds and forks).
    fn confirmed(&self) -> HashMap<String, PageKey> {
        self.pools
            .iter()
            .flat_map(|pool| pool.saved.lock().iter().map(|key| (page_filename(key), *key)).collect::<Vec<_>>())
            .collect()
    }

    /// The page filenames of the minds a slot holds now.
    fn resident_pages(&self) -> HashSet<String> {
        self.pools
            .iter()
            .flat_map(|pool| pool.holders.lock().values().map(page_filename).collect::<Vec<_>>())
            .collect()
    }
}

/// Why a pass stopped before its want.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Decline {
    /// The store's root could not be listed.
    Unreadable,
    /// No dir could be proven unserved (an engine launching, or none serving yet).
    Unproven,
    /// Page IO in flight, the node stopping or an endpoint mid-transition: no served page is
    /// settled enough to judge.
    PageIoBusy,
    /// What is left is confirmed minds' pages, and the pressure is not theirs to answer
    /// ([`ColdPages::Keep`]).
    MindsPagesKept,
}

impl Decline {
    fn as_str(self) -> &'static str {
        match self {
            Self::Unreadable => "unreadable",
            Self::Unproven => "unproven",
            Self::PageIoBusy => "page_io_busy",
            Self::MindsPagesKept => "minds_pages_kept",
        }
    }
}

/// What one pass freed, by rung.
#[derive(Debug, Default)]
pub(super) struct Evicted {
    pub(super) unserved: u64,
    pub(super) unconfirmed: u64,
    pub(super) cold: u64,
    pub(super) forgotten: usize,
    pub(super) declined: Option<Decline>,
}

impl Evicted {
    pub(super) fn freed(&self) -> u64 {
        self.unserved.saturating_add(self.unconfirmed).saturating_add(self.cold)
    }
}

/// One page (with its sidecars) on disk.
struct Page {
    dir: PathBuf,
    name: String,
    mtime: SystemTime,
    bytes: u64,
}

/// Free at least `want` bytes from the store under `root`, in value order (module doc); confirmed
/// minds' pages only when `cold_pages` yields. Bytes reported are bytes actually removed.
pub(super) fn evict_page_store(root: &Path, view: &StoreView, gate: &AdmissionGate, want: u64, cold_pages: ColdPages) -> Evicted {
    let mut out = Evicted::default();
    let dirs = match page_dirs(root) {
        Ok(dirs) => dirs,
        Err(error) => {
            continuum_base::probe!(class = "inference.kv_page.store_unreadable", dir = %root.display(), error = %error,
                "the KV page store root could not be listed; nothing evicted");
            out.declined = Some(Decline::Unreadable);
            return out;
        }
    };
    let groups = view.groups();
    let served = |dir: &Path| groups.iter().any(|g| g.dir == dir) || view.protected.iter().any(|p| p.as_path() == dir);

    // 1. Dirs nothing serves.
    if view.unproven {
        out.declined = Some(Decline::Unproven);
    } else {
        let mut pages: Vec<Page> = dirs.iter().filter(|dir| !served(dir.as_path())).flat_map(|dir| listed(dir)).collect();
        pages.sort_by_key(|page| page.mtime);
        for page in pages {
            if out.freed() >= want {
                break;
            }
            out.unserved = out.unserved.saturating_add(remove_page(&page));
        }
    }
    if out.freed() >= want {
        return out;
    }

    // 2 and 3. Served dirs: only while no page IO can be writing beneath us.
    let settled = || !view.busy && gate.is_open() && gate.in_flight() == 0;
    if !settled() {
        out.declined = Some(Decline::PageIoBusy);
        return out;
    }
    let mut unconfirmed: Vec<(usize, Page)> = Vec::new();
    let mut cold: Vec<(usize, Page, PageKey)> = Vec::new();
    for (index, group) in groups.iter().enumerate() {
        let confirmed = group.confirmed();
        let resident = group.resident_pages();
        for page in listed(&group.dir) {
            if resident.contains(&page.name) {
                continue;
            }
            match confirmed.get(&page.name) {
                Some(key) => cold.push((index, page, *key)),
                None => unconfirmed.push((index, page)),
            }
        }
    }
    unconfirmed.sort_by_key(|(_, page)| page.mtime);
    for (index, page) in unconfirmed {
        if out.freed() >= want {
            return out;
        }
        if !settled() {
            out.declined = Some(Decline::PageIoBusy);
            return out;
        }
        // A save may have confirmed it, or a lease made it resident, since the listing.
        let group = &groups[index];
        if group.confirmed().contains_key(&page.name) || group.resident_pages().contains(&page.name) {
            continue;
        }
        out.unconfirmed = out.unconfirmed.saturating_add(remove_page(&page));
    }
    if cold_pages == ColdPages::Keep {
        if !cold.is_empty() && out.freed() < want {
            out.declined = Some(Decline::MindsPagesKept);
        }
        return out;
    }
    cold.sort_by_key(|(_, page, _)| page.mtime);
    for (index, page, key) in cold {
        if out.freed() >= want {
            return out;
        }
        if !settled() {
            out.declined = Some(Decline::PageIoBusy);
            return out;
        }
        let group = &groups[index];
        if group.resident_pages().contains(&page.name) {
            continue;
        }
        // Forget first: from here no plan restores this page, so deleting it cannot fail a turn.
        for pool in &group.pools {
            pool.note_page_lost(&key);
        }
        out.forgotten += 1;
        out.cold = out.cold.saturating_add(remove_page(&page));
    }
    out
}

/// The model dirs directly under `root`. A root that does not exist holds none.
fn page_dirs(root: &Path) -> Result<Vec<PathBuf>, String> {
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.to_string()),
    };
    let mut dirs = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| error.to_string())?;
        match entry.file_type() {
            Ok(kind) if kind.is_dir() => dirs.push(entry.path()),
            Ok(_) => continue,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(format!("{}: {error}", entry.path().display())),
        }
    }
    Ok(dirs)
}

/// The pages in one dir; an unreadable dir is named and contributes none (deleting on a partial
/// listing could remove what the read did not see).
fn listed(dir: &Path) -> Vec<Page> {
    match pages_in(dir) {
        Ok(pages) => pages
            .into_iter()
            .map(|(name, (mtime, bytes))| Page { dir: dir.to_path_buf(), name, mtime, bytes })
            .collect(),
        Err(error) => {
            continuum_base::probe!(class = "inference.kv_page.store_unreadable", dir = %dir.display(), error = %error,
                "a KV page dir could not be read; none of its pages are candidates");
            Vec::new()
        }
    }
}

/// Remove a page, then its sidecars; the bytes actually removed.
fn remove_page(page: &Page) -> u64 {
    let page_file = remove_counted(&page.dir.join(&page.name));
    PAGE_SIDECARS
        .iter()
        .map(|sidecar| remove_counted(&page.dir.join(format!("{}{sidecar}", page.name))))
        .fold(page_file, u64::saturating_add)
}

/// Remove one file; its size when it was removed, 0 when it was already gone or refused.
fn remove_counted(path: &Path) -> u64 {
    let bytes = match std::fs::metadata(path) {
        Ok(metadata) => metadata.len(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return 0,
        Err(error) => {
            continuum_base::probe!(class = "inference.kv_page.evict_refused", path = %path.display(), error = %error,
                "a KV page file could not be read for eviction; kept");
            return 0;
        }
    };
    match std::fs::remove_file(path) {
        Ok(()) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
        Err(error) => {
            continuum_base::probe!(class = "inference.kv_page.evict_refused", path = %path.display(), error = %error,
                "a KV page file could not be removed; kept");
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::SavedLedger;
    use std::time::Duration;

    fn mind(n: u128) -> PageKey {
        PageKey::mind(uuid::Uuid::from_u128(n)).expect("test: a non-nil persona")
    }

    /// Write a page (and a checkpoint sidecar) with an mtime `mtime_s` seconds after the epoch,
    /// so the order is explicit and never read from the clock.
    fn page(dir: &Path, name: &str, mtime_s: u64) {
        std::fs::create_dir_all(dir).expect("test: page dir");
        let path = dir.join(name);
        std::fs::write(&path, vec![b'k'; 100]).expect("test: page");
        std::fs::write(dir.join(format!("{name}.ckpt")), vec![b'c'; 10]).expect("test: sidecar");
        std::fs::File::options()
            .write(true)
            .open(&path)
            .expect("test: open page")
            .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(mtime_s))
            .expect("test: set mtime");
    }

    struct Fixture {
        _home: tempfile::TempDir,
        root: PathBuf,
        served: PathBuf,
        other: PathBuf,
        pool: Arc<KvSlotPool>,
        resident: PageKey,
        cold: PageKey,
        orphan: PageKey,
    }

    /// A served dir whose ledger confirms two minds (one resident in slot 0, one cold), an
    /// orphan page no ledger confirms, and another model's dir. Mtimes run AGAINST value order
    /// (the other model's page is newest, the confirmed cold page oldest), so a pass that sorted
    /// by age alone would take them in the wrong order.
    fn fixture() -> Fixture {
        let home = tempfile::tempdir().expect("test: home");
        let root = home.path().join("kv-pages");
        let served = root.join("served-model");
        let other = root.join("other-model");
        let (resident, cold, orphan) = (mind(1), mind(2), mind(3));
        std::fs::create_dir_all(&served).expect("test: served dir");
        let pool = Arc::new(KvSlotPool::new("test://page-store", 3));
        pool.attach_ledger(SavedLedger::in_dir(&served, "fixture".into()), false);
        for key in [resident, cold] {
            pool.note_saved(key);
        }
        pool.holders.lock().insert(0, resident);
        page(&served, &page_filename(&cold), 1_000);
        page(&served, &page_filename(&resident), 2_000);
        page(&served, &page_filename(&orphan), 3_000);
        page(&other, &page_filename(&cold), 4_000);
        Fixture { _home: home, root, served, other, pool, resident, cold, orphan }
    }

    impl Fixture {
        fn view(&self) -> StoreView {
            StoreView { served: vec![(self.served.clone(), Arc::clone(&self.pool))], ..StoreView::default() }
        }

        fn exists(&self, dir: &Path, key: &PageKey) -> bool {
            dir.join(page_filename(key)).is_file()
        }
    }

    // what this catches: an owner that frees by age instead of value (a confirmed mind's page
    // going before a dead model's), one that deletes a confirmed page the ledger still offers
    // (her next restore fails instead of re-prefilling), and one that touches the ledger file
    // or a resident's page however much is asked.
    #[test]
    fn eviction_takes_unserved_then_unconfirmed_then_the_coldest_confirmed_and_forgets_it() {
        static GATE: AdmissionGate = AdmissionGate::new();
        let f = fixture();
        let view = f.view();
        let one_page = 110;

        let first = evict_page_store(&f.root, &view, &GATE, 1, ColdPages::Yield);
        assert_eq!((first.unserved, first.unconfirmed, first.cold), (one_page, 0, 0), "another model's page goes first");
        assert!(!f.exists(&f.other, &f.cold), "the unserved dir's page is gone");
        assert!(f.exists(&f.served, &f.orphan), "nothing served is touched while unserved bytes remain");

        let second = evict_page_store(&f.root, &view, &GATE, 1, ColdPages::Yield);
        assert_eq!((second.unserved, second.unconfirmed, second.cold), (0, one_page, 0), "then the unconfirmed page");
        assert!(!f.exists(&f.served, &f.orphan));
        assert!(f.pool.confirms(&f.cold), "a confirmed page outlives every unconfirmed one");

        let third = evict_page_store(&f.root, &view, &GATE, 1, ColdPages::Yield);
        assert_eq!((third.unserved, third.unconfirmed, third.cold, third.forgotten), (0, 0, one_page, 1));
        assert!(!f.exists(&f.served, &f.cold), "then the cold confirmed page");
        assert!(!f.pool.confirms(&f.cold), "and its ledger entry is forgotten with it");

        let everything = evict_page_store(&f.root, &view, &GATE, u64::MAX, ColdPages::Yield);
        assert_eq!(everything.freed(), 0, "nothing else is the pool's to give");
        assert!(f.exists(&f.served, &f.resident), "a resident's page is never removed");
        assert!(f.pool.confirms(&f.resident));
        assert!(f.served.join("saved-ledger.json").is_file(), "the ledger survives every rung");
    }

    // what this catches: deleting a served page while a save may be writing it (page IO in
    // flight, the node stopping, an endpoint mid-transition), and wiping every dir while an
    // engine is launching, before the dir it will serve is known.
    #[test]
    fn page_io_in_flight_or_a_launch_keeps_what_it_could_be_using() {
        static GATE: AdmissionGate = AdmissionGate::new();
        let f = fixture();
        let permit = GATE.admit().expect("test: an open gate admits");
        let busy = evict_page_store(&f.root, &f.view(), &GATE, u64::MAX, ColdPages::Yield);
        assert_eq!(busy.declined, Some(Decline::PageIoBusy));
        assert_eq!((busy.unconfirmed, busy.cold), (0, 0), "no served page while page IO is in flight");
        assert!(f.exists(&f.served, &f.orphan) && f.exists(&f.served, &f.cold));
        assert!(!f.exists(&f.other, &f.cold), "an unserved dir is no save's target");
        drop(permit);

        page(&f.other, &page_filename(&f.cold), 4_000);
        let launching = StoreView { unproven: true, ..f.view() };
        let held = evict_page_store(&f.root, &launching, &GATE, 1, ColdPages::Yield);
        assert_eq!(held.declined, Some(Decline::Unproven));
        assert!(f.exists(&f.other, &f.cold), "no dir is proven unserved while an engine launches");

        assert!(!f.exists(&f.served, &f.orphan), "a launch leaves the served dirs' value order as it was");

        let transition = StoreView { busy: true, ..f.view() };
        let kept = evict_page_store(&f.root, &transition, &GATE, u64::MAX, ColdPages::Yield);
        assert_eq!(kept.declined, Some(Decline::PageIoBusy));
        assert!(f.exists(&f.served, &f.cold), "a transition's checkpoint saves run outside the gate");
        assert!(f.pool.confirms(&f.cold));
    }

    // what this catches (the M5, 2026-11-13 22:40Z-00:50Z: volume 0.85, the store ~3 GB of a
    // 99 GB budget, 29 minds' confirmed pages taken; at the 23:45Z deploy every first act was a
    // full prefill): confirmed minds' pages given up for a volume other classes filled. Under
    // that pressure the dead bytes still go, her page stays confirmed on disk, and a restarted
    // core's ledger offers it again (she restores instead of re-prefilling).
    #[test]
    fn a_hot_volume_takes_dead_pages_but_keeps_minds_pages_across_a_restart() {
        static GATE: AdmissionGate = AdmissionGate::new();
        let f = fixture();
        let m5 = StorePressure { own: 3.0 / 99.0, volume: 0.85 };
        assert_eq!(ColdPages::under(m5), ColdPages::Keep, "the M5's recorded pressure is the volume's, not the store's");

        let pass = evict_page_store(&f.root, &f.view(), &GATE, u64::MAX, ColdPages::under(m5));
        assert_eq!((pass.unserved, pass.unconfirmed, pass.cold, pass.forgotten), (110, 110, 0, 0), "only dead pages go");
        assert_eq!(pass.declined, Some(Decline::MindsPagesKept), "what it kept is named");
        assert!(f.exists(&f.served, &f.cold) && f.pool.confirms(&f.cold), "the paged-out mind's page stays, confirmed");

        let restarted = KvSlotPool::new("test://page-store-restarted", 3);
        assert_eq!(restarted.attach_ledger(SavedLedger::in_dir(&f.served, "fixture".into()), true), 2);
        assert!(restarted.confirms(&f.cold), "after a restart her page is restorable: no re-prefill");
    }

    // what this catches: a store that stops giving back confirmed pages altogether (the disk
    // rule: every class keeps an eviction). Its own budget and a critical volume still take them.
    #[test]
    fn the_stores_own_pressure_or_a_critical_volume_still_takes_minds_pages() {
        assert_eq!(ColdPages::under(StorePressure { own: 0.85, volume: 0.10 }), ColdPages::Yield, "its own bytes");
        assert_eq!(ColdPages::under(StorePressure { own: 0.03, volume: 0.96 }), ColdPages::Yield, "a volume about to run out");
        assert_eq!(ColdPages::under(StorePressure { own: 0.79, volume: 0.94 }), ColdPages::Keep, "below both tiers");
    }
}
