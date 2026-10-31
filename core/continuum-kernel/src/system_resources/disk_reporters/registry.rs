//! The existing measurement registry also admits roots selected by later writers.
//! Registration and boot binding share one lock; no writer can fall between them.
use std::path::Path;
use std::sync::{Arc, OnceLock, Weak};

use super::{TrackedDir, CARGO_TARGET_CLASSES};
use crate::orm::{adapter::StorageAdapter, OrmStore};
use crate::paging::PressureBroker;
use crate::system_resources::{CargoTargetPool, DiskPressureMonitor};
use futures::FutureExt;
use parking_lot::Mutex;
mod record;
use record::RootRecord;

pub(super) static TRACKED_DIRS: OnceLock<Registry> = OnceLock::new();

#[cfg(any(test, feature = "test-fixtures"))]
static TEST_OWNERSHIP: tokio::sync::OnceCell<(Registry, tempfile::TempDir)> =
    tokio::sync::OnceCell::const_new();

/// Explicit command-fixture setup, without installing a monitor or altering
/// unrelated module tests' production boot registration state.
#[cfg(any(test, feature = "test-fixtures"))]
pub async fn prepare_writer_fixture() {
    TEST_OWNERSHIP
        .get_or_init(|| async {
            let (adapter, directory) = crate::orm::store::fresh_adapter().await;
            let registry = Registry::new(Vec::new()).await;
            registry
                .open_records(adapter)
                .await
                .expect("writer fixture ownership");
            (registry, directory)
        })
        .await;
}

/// How long an ADMISSION (a shell's cargo-root grant, a configured root at startup) waits for
/// one filesystem resolution before it is refused with a named outcome: the one launch-path
/// filesystem bound ([`crate::runtime::daemon::LAUNCH_PATH_FS_BOUND`], measured there).
pub(crate) const ADMISSION_BOUND: std::time::Duration = crate::runtime::daemon::LAUNCH_PATH_FS_BOUND;

fn writer_registry() -> Result<&'static Registry, String> {
    if let Some(registry) = TRACKED_DIRS.get() {
        return Ok(registry);
    }
    #[cfg(any(test, feature = "test-fixtures"))]
    if let Some((registry, _)) = TEST_OWNERSHIP.get() {
        return Ok(registry);
    }
    Err("Cargo writer has no installed disk ownership registry".to_string())
}

struct Consumers {
    monitor: Weak<DiskPressureMonitor>,
    broker: Weak<PressureBroker>,
}
struct State {
    dirs: Vec<Arc<TrackedDir>>,
    unresolved_boot: Vec<Arc<TrackedDir>>,
    boot_reporters: usize,
    consumers: Option<Consumers>,
    active: std::collections::HashMap<std::path::PathBuf, usize>,
    permanent: std::collections::HashSet<std::path::PathBuf>,
    /// Target dirs an eviction is detaching. A writer admission that overlaps one is
    /// refused until the claim drops, and an eviction cannot claim a dir a lease overlaps:
    /// one lock decides both, so neither can slip between the other's check and act.
    evicting: std::collections::HashSet<std::path::PathBuf>,
}
#[derive(Clone, Debug)]
struct Observation {
    path: std::path::PathBuf,
    volume: Option<u64>,
    empty_custody: Option<Arc<Vec<std::fs::File>>>,
}
type Resolution =
    futures::future::Shared<futures::future::BoxFuture<'static, Result<Observation, String>>>;
#[derive(Clone, PartialEq, Eq, Hash)]
enum PathWork {
    Resolve,
    ResolveExisting,
    Validate(crate::system_resources::confinement::Confinement),
    Prepare(crate::system_resources::confinement::Confinement),
    Observe,
    Empty,
}
pub(super) struct Registry(
    Arc<Mutex<State>>,
    tokio::sync::Mutex<Option<DurableRoots>>,
    Mutex<std::collections::HashMap<(std::path::PathBuf, PathWork), (Arc<()>, Resolution)>>,
);

struct DurableRoots {
    store: OrmStore<RootRecord>,
    saved: std::collections::HashMap<std::path::PathBuf, RootRecord>,
}

/// The registry's writer hold starts before durable admission and is retained by
/// the existing child-process authority until termination is confirmed.
pub struct CargoAdmission {
    grant: Option<crate::system_resources::confinement::CargoTargetGrant>,
    lease: Arc<RootLease>,
}
impl CargoAdmission {
    pub fn grant(&self) -> Option<&crate::system_resources::confinement::CargoTargetGrant> {
        self.grant.as_ref()
    }
    pub fn kept(&self) -> super::Kept {
        self.lease.clone()
    }
}
struct RootLease {
    state: Arc<Mutex<State>>,
    path: std::path::PathBuf,
}
impl std::fmt::Debug for RootLease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("CargoRootLease").field(&self.path).finish()
    }
}
impl Drop for RootLease {
    fn drop(&mut self) {
        let mut state = self.state.lock();
        if let Some(count) = state.active.get_mut(&self.path) {
            *count -= 1;
            if *count == 0 {
                state.active.remove(&self.path);
            }
        }
    }
}

/// An eviction's exclusive claim on one target dir: held from before its lock scan until
/// it is detached, so no writer is admitted to it in between (see `State::evicting`).
struct EvictionClaim {
    state: Arc<Mutex<State>>,
    path: std::path::PathBuf,
}
impl Drop for EvictionClaim {
    fn drop(&mut self) {
        self.state.lock().evicting.remove(&self.path);
    }
}

/// Either path at, beneath or above the other.
fn overlaps(a: &Path, b: &Path) -> bool {
    crate::paths::has_path_prefix(a, b) || crate::paths::has_path_prefix(b, a)
}

impl Registry {
    /// Claim `dir` for eviction, or say why not: an admitted writer's lease overlaps it, or
    /// another eviction already holds it.
    fn claim_for_eviction(&self, dir: &Path) -> Result<super::Kept, String> {
        let mut state = self.0.lock();
        if state.active.keys().any(|root| overlaps(root, dir)) {
            return Err("an admitted cargo writer holds its lease".into());
        }
        if state.evicting.iter().any(|claimed| overlaps(claimed, dir)) {
            return Err("another eviction holds it".into());
        }
        state.evicting.insert(dir.to_path_buf());
        Ok(Arc::new(EvictionClaim { state: self.0.clone(), path: dir.to_path_buf() }))
    }

    pub(super) async fn new(dirs: Vec<Arc<TrackedDir>>) -> Self {
        let registry = Self(
            Arc::new(Mutex::new(State {
                dirs: Vec::new(),
                unresolved_boot: Vec::new(),
                boot_reporters: 0,
                consumers: None,
                active: Default::default(),
                permanent: Default::default(),
                evicting: Default::default(),
            })),
            tokio::sync::Mutex::new(None),
            Mutex::new(Default::default()),
        );
        let mut physical_roots = std::collections::HashSet::new();
        for dir in dirs {
            let dir = if CARGO_TARGET_CLASSES.contains(&dir.class()) {
                match registry.resolve(dir.path()).await {
                    Ok(root) if physical_roots.insert(root.clone()) => {
                        TrackedDir::new(dir.class(), root)
                    }
                    Ok(_) => continue,
                    Err(error) => {
                        crate::clog_warn!("Cargo ownership retains unavailable root: {error}");
                        registry.0.lock().unresolved_boot.push(dir);
                        continue;
                    }
                }
            } else {
                dir
            };
            registry.0.lock().dirs.push(dir);
        }
        {
            let mut state = registry.0.lock();
            state.boot_reporters = state.dirs.len();
            state.permanent = state
                .dirs
                .iter()
                .filter(|dir| CARGO_TARGET_CLASSES.contains(&dir.class()))
                .map(|dir| dir.path().to_path_buf())
                .collect();
        }
        registry
    }

    /// A reporter's read of a root: the pressure-reporter budget, never a launch path's.
    async fn resolve(&self, path: &Path) -> Result<std::path::PathBuf, String> {
        self.resolve_within(path, super::super::disk_pressure::REPORTER_TIMEOUT).await
    }
    async fn resolve_within(&self, path: &Path, bound: std::time::Duration) -> Result<std::path::PathBuf, String> {
        Ok(self.path_work_within(path, PathWork::Resolve, bound).await?.path)
    }
    /// Reporter-bounded, see [`Self::path_work_within`].
    async fn path_work(&self, path: &Path, work: PathWork) -> Result<Observation, String> {
        self.path_work_within(path, work, super::super::disk_pressure::REPORTER_TIMEOUT).await
    }
    /// One filesystem resolution of `path`, shared with every other request for the same
    /// work while it is in flight, waited for at most `bound`. A reporter tick waits the
    /// 100 ms reporter budget and reads "still pending" as its named outcome; an ADMISSION
    /// (a citizen's shell, a configured root at startup) waits [`ADMISSION_BOUND`]: on the
    /// M5 at 91% swap (2026-10-16) the reporter budget on a shell's cargo-root grant refused
    /// three of Benchy's commands outright, none of which touched cargo, with "Cargo cache
    /// ownership: Cargo root resolution is still pending" (shell_session.rs fails the whole
    /// command before any child exists).

    async fn path_work_within(&self, path: &Path, work: PathWork, bound: std::time::Duration) -> Result<Observation, String> {
        use crate::runtime::daemon::{guarded, Guarded};
        let key = (path.to_path_buf(), work.clone());
        let (identity, request) = {
            let mut pending = self.2.lock();
            pending
                .entry(key.clone())
                .or_insert_with(|| {
                    let path = path.to_path_buf();
                    let kind = match &work {
                        PathWork::Resolve => "resolve",
                        PathWork::ResolveExisting => "resolve_existing",
                        PathWork::Observe => "observe",
                        PathWork::Empty => "empty",
                        PathWork::Validate(_) => "validate",
                        PathWork::Prepare(_) => "prepare",
                    };
                    let request = async move {
                        // Queue vs work: a refusal at the bound says which was slow, so the
                        // bound can be checked against a number (Cormac on #5112).
                        let queued = std::time::Instant::now();
                        let probe_path = path.clone();
                        let outcome = tokio::task::spawn_blocking(move || {
                            let started = std::time::Instant::now();
                            let result = (|| {
                            if let PathWork::Prepare(policy) = &work {
                                policy.validate_cargo_target(&path)?;
                            }
                            if matches!(work, PathWork::Prepare(_)) {
                                std::fs::create_dir_all(&path).map_err(|error| {
                                    format!(
                                        "cannot prepare configured Cargo root {}: {error}",
                                        path.display()
                                    )
                                })?;
                            }
                            let physical = match &work {
                                PathWork::ResolveExisting => std::fs::canonicalize(&path)
                                    .map_err(|error| format!("cannot resolve existing storage path: {error}")),
                                PathWork::Resolve | PathWork::Observe | PathWork::Empty => {
                                    crate::paths::cargo::physical_root(&path)
                                }
                                PathWork::Validate(policy) | PathWork::Prepare(policy) => {
                                    policy.validate_cargo_target(&path)
                                }
                            }?;
                            let volume = if matches!(work, PathWork::Observe | PathWork::Empty) {
                                Some(if matches!(work, PathWork::Empty) {
                                    crate::paths::cargo::volume_id(&path)?
                                } else {
                                    crate::paths::cargo::existing_volume_id(&path)?
                                })
                            } else {
                                None
                            };
                            let empty_custody = if matches!(work, PathWork::Empty) {
                                if physical != path {
                                    return Err("stored physical root was redirected".into());
                                }
                                CargoTargetPool::empty_root_custody(&path)?.map(Arc::new)
                            } else {
                                None
                            };
                            Ok(Observation {
                                path: physical,
                                volume,
                                empty_custody,
                            })
                            })();
                            (result, started.elapsed())
                        })
                        .await
                        .map_err(|error| format!("Cargo root resolution worker failed: {error}"))?;
                        let (result, worked) = outcome;
                        continuum_base::probe!(
                            class = "disk.root.resolution",
                            path = %probe_path.display(),
                            kind = %kind,
                            queue_ms = (queued.elapsed().saturating_sub(worked)).as_millis() as u64,
                            work_ms = worked.as_millis() as u64,
                            ok = result.is_ok(),
                            "one filesystem resolution of a tracked root: how long it queued, how long it worked"
                        );
                        result
                    }
                    .boxed()
                    .shared();
                    (Arc::new(()), request)
                })
                .clone()
        };
        match guarded(bound, request).await {
            Guarded::Completed(result) => {
                let mut pending = self.2.lock();
                if pending
                    .get(&key)
                    .is_some_and(|(current, _)| Arc::ptr_eq(current, &identity))
                {
                    pending.remove(&key);
                }
                result
            }
            Guarded::Panicked => Err(format!(
                "Cargo root resolution panicked: {}",
                path.display()
            )),
            // Keep the shared in-flight worker: another request must not create
            // another stuck filesystem thread for the same unavailable root.
            Guarded::TimedOut => {
                continuum_base::probe!(
                    class = "disk.root.resolution.pending",
                    path = %path.display(),
                    bound_ms = bound.as_millis() as u64,
                    "a tracked root did not resolve inside this caller's bound; the shared worker stays"
                );
                Err(format!(
                    "Cargo root resolution is still pending: {}",
                    path.display()
                ))
            }
        }
    }
    pub(super) fn snapshot(&self) -> Vec<Arc<TrackedDir>> {
        self.0.lock().dirs.clone()
    }
    async fn register(&self, path: &Path) -> Result<Arc<TrackedDir>, String> {
        let physical = match self.resolve_within(path, ADMISSION_BOUND).await {
            Ok(physical) => physical,
            Err(error) => {
                let mut state = self.0.lock();
                if !state.unresolved_boot.iter().any(|dir| dir.path() == path) {
                    state
                        .unresolved_boot
                        .push(TrackedDir::new("cargo-target", path.to_path_buf()));
                }
                return Err(error);
            }
        };
        self.0.lock().permanent.insert(physical.clone());
        self.register_resolved(physical, None)
    }

    fn register_resolved(&self, physical: std::path::PathBuf, citizen: Option<uuid::Uuid>) -> Result<Arc<TrackedDir>, String> {
        let mut state = self.0.lock();
        if let Some(dir) = state
            .dirs
            .iter()
            .find(|dir| CARGO_TARGET_CLASSES.contains(&dir.class()) && dir.path() == physical)
        {
            if let Some(peer) = citizen {
                dir.bind_citizen(peer)?;
                if let Some(monitor) = state.consumers.as_ref().and_then(|consumers| consumers.monitor.upgrade()) {
                    let reporter: Arc<dyn super::super::disk_pressure::DiskReporter> = dir.clone();
                    monitor.remove_reporter(&reporter);
                }
            }
            return Ok(dir.clone());
        }
        let dir = TrackedDir::new("cargo-target", physical);
        if let Some(peer) = citizen { dir.bind_citizen(peer)?; }
        if let Some(consumers) = &state.consumers {
            if let Some(monitor) = consumers.monitor.upgrade().filter(|_| citizen.is_none()) {
                monitor.add_reporter(dir.clone());
            }
            if let Some(broker) = consumers.broker.upgrade() {
                register_pool(&broker, dir.clone());
            }
        }
        state.dirs.push(dir.clone());
        Ok(dir)
    }

    async fn open_records(&self, adapter: Arc<dyn StorageAdapter>) -> Result<(), String> {
        // Retry unresolved boot configuration in the existing owner's tick. It
        // remains pending, never persisted or pooled as a guessed physical root.
        // Known durable roots below can be restored even while a mount is offline.
        let pending = self.0.lock().unresolved_boot.clone();
        for dir in pending {
            if let Ok(physical) = self.resolve(dir.path()).await {
                self.0.lock().permanent.insert(physical.clone());
                self.register_resolved(physical, dir.citizen())?;
                self.0
                    .lock()
                    .unresolved_boot
                    .retain(|entry| !Arc::ptr_eq(entry, &dir));
            }
        }
        let mut observations = Vec::new();
        for dir in self
            .snapshot()
            .into_iter()
            .filter(|dir| CARGO_TARGET_CLASSES.contains(&dir.class()))
        {
            let volume = self
                .path_work(dir.path(), PathWork::Observe)
                .await
                .ok()
                .and_then(|observation| observation.volume);
            observations.push((dir, volume));
        }
        let mut durable = self.1.lock().await;
        if let Some(records) = durable.as_mut() {
            for (dir, volume) in observations {
                // Historical roots retain their recorded device; only an actual
                // writer admission can replace that ownership observation.
                if !records.saved.contains_key(dir.path()) {
                    records.remember(dir.class(), dir.path(), volume).await?;
                }
            }
            return Ok(());
        }
        let store = OrmStore::<RootRecord>::new(adapter)
            .await
            .map_err(|error| error.to_string())?;
        let mut saved = std::collections::HashMap::new();
        for (_, record) in store.find_all().await.map_err(|error| error.to_string())? {
            if !CARGO_TARGET_CLASSES.contains(&record.class.as_str()) {
                return Err(format!("unknown tracked Cargo class {}", record.class));
            }
            let path = record.path.decode()?;
            if !path.is_absolute() {
                return Err("stored cache root is not absolute".into());
            }
            // Stored paths already name physical roots. Offline volumes do not
            // need a filesystem probe to recover their durable ownership.
            self.register_resolved(path.clone(), record.citizen)?;
            saved.insert(path, record);
        }
        let mut records = DurableRoots { store, saved };
        for (dir, volume) in observations {
            if !records.saved.contains_key(dir.path()) {
                records.remember(dir.class(), dir.path(), volume).await?;
            }
        }
        *durable = Some(records);
        Ok(())
    }

    /// Reporter-bounded, see [`Self::admit_within`].
    async fn admit(&self, path: &Path) -> Result<Arc<RootLease>, String> {
        self.admit_within(path, super::super::disk_pressure::REPORTER_TIMEOUT).await
    }
    async fn admit_within(&self, path: &Path, bound: std::time::Duration) -> Result<Arc<RootLease>, String> {
        let observation = self.path_work_within(path, PathWork::Observe, bound).await?;
        let physical = observation.path;
        let mut durable = self.1.lock().await;
        let records = durable
            .as_mut()
            .ok_or_else(|| "disk root ownership has not restored yet".to_string())?;
        {
            let mut state = self.0.lock();
            if state.evicting.iter().any(|dir| overlaps(dir, &physical)) {
                return Err(format!(
                    "{} is being evicted; admit again once it is detached (the build then gets a fresh target)",
                    physical.display()
                ));
            }
            *state.active.entry(physical.clone()).or_default() += 1;
        }
        let lease = Arc::new(RootLease {
            state: self.0.clone(),
            path: physical.clone(),
        });
        records
            .remember("cargo-target", &physical, observation.volume)
            .await?;
        self.register_resolved(physical, None)?;
        Ok(lease)
    }

    async fn observe_citizen_target(&self, path: &Path, peer: uuid::Uuid) -> Result<Arc<TrackedDir>, String> {
        let observation = self.path_work(path, PathWork::Observe).await?;
        let mut durable = self.1.lock().await;
        let records = durable.as_mut().ok_or("disk root ownership has not restored yet")?;
        let _hands = crate::system_resources::hands_lease::PersonaHandsLease::try_take(peer)
            .ok_or("citizen hands are busy; Cargo observation deferred")?;
        // Binding is conservative even if persistence fails: an already exposed
        // generic pool must stop using generic custody before the durable write.
        let dir = self.register_resolved(observation.path.clone(), Some(peer))?;
        records.remember_owned("cargo-target", &observation.path, observation.volume, Some(peer)).await?;
        Ok(dir)
    }

    async fn retire_empty(&self) -> Result<(), String> {
        let candidates = {
            let durable = self.1.lock().await;
            let Some(records) = durable.as_ref() else {
                return Ok(());
            };
            let state = self.0.lock();
            records
                .saved
                .iter()
                .filter(|(path, record)| {
                    record.volume_id.is_some()
                        && !state.permanent.contains(*path)
                        && !state.active.contains_key(*path)
                        && state
                            .dirs
                            .iter()
                            .any(|dir| dir.path() == path.as_path() && dir.bytes() == 0)
                })
                .map(|(path, record)| (path.clone(), record.clone()))
                .collect::<Vec<_>>()
        };
        for (path, before) in candidates {
            let Ok(observation) = self.path_work(&path, PathWork::Empty).await else {
                continue;
            };
            if observation.empty_custody.is_none() || observation.volume != before.volume_id {
                continue;
            }
            // The filesystem custody guards stay in observation through the
            // transaction. Admission takes this same durable lock before its hold.
            let mut durable = self.1.lock().await;
            let Some(records) = durable.as_mut() else {
                continue;
            };
            if !records.saved.get(&path).is_some_and(|record| {
                record.base.id == before.base.id && record.volume_id == before.volume_id
            }) {
                continue;
            }
            {
                let state = self.0.lock();
                if state.active.contains_key(&path) || state.permanent.contains(&path) {
                    continue;
                }
            }
            records
                .store
                .delete(uuid::Uuid::parse_str(&before.base.id).map_err(|error| error.to_string())?)
                .await
                .map_err(|error| error.to_string())?;
            records.saved.remove(&path);
            let mut state = self.0.lock();
            if let Some(index) = state.dirs.iter().position(|dir| dir.path() == path) {
                let dir = state.dirs.remove(index);
                if let Some(consumers) = &state.consumers {
                    if let Some(monitor) = consumers.monitor.upgrade() {
                        let reporter: Arc<
                            dyn crate::system_resources::disk_pressure::DiskReporter,
                        > = dir.clone();
                        monitor.remove_reporter(&reporter);
                    }
                    if let Some(broker) = consumers.broker.upgrade() {
                        let pool = CargoTargetPool::new(dir, 0);
                        broker.unregister(crate::paging::ResourcePool::tier_name(&pool));
                    }
                }
            }
        }
        Ok(())
    }
}

impl DurableRoots {
    async fn remember(
        &mut self,
        class: &str,
        path: &Path,
        volume_id: Option<u64>,
    ) -> Result<(), String> {
        self.remember_owned(class, path, volume_id, None).await
    }

    async fn remember_owned(&mut self, class: &str, path: &Path, volume_id: Option<u64>, citizen: Option<uuid::Uuid>) -> Result<(), String> {
        // Boot/replay retain unavailable volumes as named ownership. New writes
        // already require successful physical resolution in admit().
        let physical = path.to_path_buf();
        if !physical.is_absolute() {
            return Err("cache ownership requires an absolute root".into());
        }
        if self
            .saved
            .get(&physical)
            .is_some_and(|record| record.volume_id == volume_id && (citizen.is_none() || record.citizen == citizen))
        {
            return Ok(());
        }
        let mut record = self
            .saved
            .get(&physical)
            .cloned()
            .unwrap_or_else(|| RootRecord::new(class, &physical));
        record.volume_id = volume_id;
        if let Some(peer) = citizen {
            if record.citizen.is_some_and(|prior| prior != peer) {
                return Err("stored Cargo root belongs to another citizen".into());
            }
            record.citizen = Some(peer);
        }
        let id = uuid::Uuid::parse_str(&record.base.id).map_err(|error| error.to_string())?;
        // save creates a row; a newly observed volume updates the existing owner
        // rather than trying to insert its stable identity a second time.
        if self.saved.contains_key(&physical) {
            self.store.update(id, &record).await
        } else {
            self.store.save(id, &record).await
        }
        .map_err(|error| error.to_string())?;
        self.saved.insert(physical, record);
        Ok(())
    }
}

pub async fn restore_cargo_roots(adapter: Arc<dyn StorageAdapter>) -> Result<(), String> {
    if let Some(registry) = TRACKED_DIRS.get() {
        registry.open_records(adapter).await?;
        registry.retire_empty().await?;
    }
    Ok(())
}

/// Existing-path custody uses the registry's bounded, retained path worker;
/// an unavailable mount cannot create a new orphan worker on every request.
pub async fn resolve_existing_storage_path(path: &Path) -> Result<std::path::PathBuf, String> {
    Ok(writer_registry()?.path_work(path, PathWork::ResolveExisting).await?.path)
}

/// Durable admission before a writer can launch. Failure to save refuses that
/// write rather than produce an unowned tree after the next restart.
pub async fn admit_cargo_target(
    path: &Path,
    cwd: &Path,
    confinement: Option<&crate::system_resources::confinement::Confinement>,
) -> Result<CargoAdmission, String> {
    let registry = writer_registry()?;
    let policy = match confinement {
        Some(policy) => policy.clone(),
        None => crate::system_resources::confinement::Confinement::for_caller(
            &dirs::home_dir()
                .ok_or_else(|| "Cargo target validation requires the native home".to_string())?,
            &crate::paths::continuum_home()?,
            "",
            cwd,
        ),
    };
    registry
        .path_work_within(path, PathWork::Validate(policy.clone()), ADMISSION_BOUND)
        .await?;
    let lease = registry.admit_within(path, ADMISSION_BOUND).await?;
    // Only operator configuration grants creation outside a citizen workspace.
    // Session overrides acquire ownership but gain no filesystem privileges.
    let configured = crate::paths::cargo::target_dir(cwd)?;
    if registry.resolve_within(path, ADMISSION_BOUND).await? == registry.resolve_within(&configured, ADMISSION_BOUND).await? {
        let prepared = registry
            .path_work_within(path, PathWork::Prepare(policy.clone()), ADMISSION_BOUND)
            .await?;
        return Ok(CargoAdmission {
            grant: Some(crate::system_resources::confinement::CargoTargetGrant::admitted(
                prepared.path,
                policy,
            )),
            lease,
        });
    }
    Ok(CargoAdmission { grant: None, lease })
}

fn register_pool(broker: &PressureBroker, dir: Arc<TrackedDir>) {
    let budget = DiskPressureMonitor::volume_for(dir.path())
        .map(|volume| super::super::cargo_target_budget_bytes(volume.total_bytes))
        .unwrap_or(super::super::DEFAULT_CARGO_TARGET_BUDGET_BYTES);
    broker.register(Arc::new(CargoTargetPool::new(dir, budget)));
}

/// Connect the existing boot consumers once. Monitor already owns the boot
/// reporters; roots registered before binding are included in the same snapshot.
pub fn bind_cargo_owners(
    monitor: &Arc<DiskPressureMonitor>,
    broker: &Arc<PressureBroker>,
) -> Result<(), String> {
    let Some(registry) = TRACKED_DIRS.get() else {
        return Err("Cargo owners have no installed measurement registry".into());
    };
    let mut state = registry.0.lock();
    if let Some(consumers) = &state.consumers {
        if Weak::ptr_eq(&consumers.monitor, &Arc::downgrade(monitor))
            && Weak::ptr_eq(&consumers.broker, &Arc::downgrade(broker))
        {
            return Ok(());
        }
        return Err("Cargo roots already belong to different monitor/broker owners".into());
    }
    for (index, dir) in state.dirs.iter().enumerate() {
        if dir.citizen().is_some() {
            let reporter: Arc<dyn super::super::disk_pressure::DiskReporter> = dir.clone();
            monitor.remove_reporter(&reporter);
        } else if index >= state.boot_reporters {
            monitor.add_reporter(dir.clone());
        }
        if CARGO_TARGET_CLASSES.contains(&dir.class()) {
            register_pool(broker, dir.clone());
        }
    }
    state.consumers = Some(Consumers {
        monitor: Arc::downgrade(monitor),
        broker: Arc::downgrade(broker),
    });
    Ok(())
}

/// Tools without the running substrate do not create a second registry/scanner.
/// Their path is resolved identically; the running core registers configured roots
/// at boot and all later in-process writer overrides here before launching work.
pub async fn register_cargo_target(path: &Path) -> Result<(), String> {
    if let Some(registry) = TRACKED_DIRS.get() {
        registry.register(path).await?;
    }
    Ok(())
}

pub(super) async fn observe_citizen_target(path: &Path, peer: uuid::Uuid) -> Result<Arc<TrackedDir>, String> {
    writer_registry()?.observe_citizen_target(path, peer).await
}

/// The root authority's eviction claim on `dir` (a physical target dir), held by the caller
/// until the dir is detached. Refused while an admitted writer's lease overlaps it; while
/// it is held, [`admit_cargo_target`] for an overlapping root is refused. No installed
/// registry means no authority to exclude writers, so the claim is refused (fail closed).
pub(crate) fn claim_for_eviction(dir: &Path) -> Result<super::Kept, String> {
    writer_registry()?.claim_for_eviction(dir)
}

/// Called while holding this citizen's hands, which also excludes discovery's
/// ownership publication. A whole-workspace owner must not bypass child pools.
pub(crate) fn citizen_has_cargo_owner(peer: uuid::Uuid) -> Result<bool, String> {
    Ok(writer_registry()?.snapshot().iter().any(|dir| dir.citizen() == Some(peer)))
}

#[cfg(test)]
mod tests;
