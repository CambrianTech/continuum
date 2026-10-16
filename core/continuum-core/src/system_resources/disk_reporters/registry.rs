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

#[cfg(test)]
static TEST_OWNERSHIP: tokio::sync::OnceCell<(Registry, tempfile::TempDir)> =
    tokio::sync::OnceCell::const_new();

/// Explicit command-fixture setup, without installing a monitor or altering
/// unrelated module tests' production boot registration state.
#[cfg(test)]
pub(crate) async fn prepare_writer_fixture() {
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

fn writer_registry() -> Result<&'static Registry, String> {
    if let Some(registry) = TRACKED_DIRS.get() {
        return Ok(registry);
    }
    #[cfg(test)]
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
    Validate(crate::code::confinement::Confinement),
    Prepare(crate::code::confinement::Confinement),
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
    grant: Option<crate::code::confinement::CargoTargetGrant>,
    lease: Arc<RootLease>,
}
impl CargoAdmission {
    pub fn grant(&self) -> Option<&crate::code::confinement::CargoTargetGrant> {
        self.grant.as_ref()
    }
    pub fn kept(&self) -> crate::inference::lane_process::Kept {
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

impl Registry {
    pub(super) async fn new(dirs: Vec<Arc<TrackedDir>>) -> Self {
        let registry = Self(
            Arc::new(Mutex::new(State {
                dirs: Vec::new(),
                unresolved_boot: Vec::new(),
                boot_reporters: 0,
                consumers: None,
                active: Default::default(),
                permanent: Default::default(),
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

    async fn resolve(&self, path: &Path) -> Result<std::path::PathBuf, String> {
        Ok(self.path_work(path, PathWork::Resolve).await?.path)
    }

    async fn path_work(&self, path: &Path, work: PathWork) -> Result<Observation, String> {
        use crate::runtime::daemon::{guarded, Guarded};
        let key = (path.to_path_buf(), work.clone());
        let (identity, request) = {
            let mut pending = self.2.lock();
            pending
                .entry(key.clone())
                .or_insert_with(|| {
                    let path = path.to_path_buf();
                    let request = async move {
                        tokio::task::spawn_blocking(move || {
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
                        })
                        .await
                        .map_err(|error| format!("Cargo root resolution worker failed: {error}"))?
                    }
                    .boxed()
                    .shared();
                    (Arc::new(()), request)
                })
                .clone()
        };
        match guarded(super::super::disk_pressure::REPORTER_TIMEOUT, request).await {
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
            Guarded::TimedOut => Err(format!(
                "Cargo root resolution is still pending: {}",
                path.display()
            )),
        }
    }
    pub(super) fn snapshot(&self) -> Vec<Arc<TrackedDir>> {
        self.0.lock().dirs.clone()
    }
    async fn register(&self, path: &Path) -> Result<Arc<TrackedDir>, String> {
        let physical = match self.resolve(path).await {
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

    async fn admit(&self, path: &Path) -> Result<Arc<RootLease>, String> {
        let observation = self.path_work(path, PathWork::Observe).await?;
        let physical = observation.path;
        let mut durable = self.1.lock().await;
        let records = durable
            .as_mut()
            .ok_or_else(|| "disk root ownership has not restored yet".to_string())?;
        *self.0.lock().active.entry(physical.clone()).or_default() += 1;
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
        let _hands = crate::modules::work::HandsLease::try_take(peer)
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
pub(crate) async fn resolve_existing_storage_path(path: &Path) -> Result<std::path::PathBuf, String> {
    Ok(writer_registry()?.path_work(path, PathWork::ResolveExisting).await?.path)
}

/// Durable admission before a writer can launch. Failure to save refuses that
/// write rather than produce an unowned tree after the next restart.
pub async fn admit_cargo_target(
    path: &Path,
    cwd: &Path,
    confinement: Option<&crate::code::confinement::Confinement>,
) -> Result<CargoAdmission, String> {
    let registry = writer_registry()?;
    let policy = match confinement {
        Some(policy) => policy.clone(),
        None => crate::code::confinement::Confinement::for_caller(
            &dirs::home_dir()
                .ok_or_else(|| "Cargo target validation requires the native home".to_string())?,
            &crate::paths::continuum_home()?,
            "",
            cwd,
        ),
    };
    registry
        .path_work(path, PathWork::Validate(policy.clone()))
        .await?;
    let lease = registry.admit(path).await?;
    // Only operator configuration grants creation outside a citizen workspace.
    // Session overrides acquire ownership but gain no filesystem privileges.
    let configured = crate::paths::cargo::target_dir(cwd)?;
    if registry.resolve(path).await? == registry.resolve(&configured).await? {
        let prepared = registry
            .path_work(path, PathWork::Prepare(policy.clone()))
            .await?;
        return Ok(CargoAdmission {
            grant: Some(crate::code::confinement::CargoTargetGrant::admitted(
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

/// Called while holding this citizen's hands, which also excludes discovery's
/// ownership publication. A whole-workspace owner must not bypass child pools.
pub(crate) fn citizen_has_cargo_owner(peer: uuid::Uuid) -> Result<bool, String> {
    Ok(writer_registry()?.snapshot().iter().any(|dir| dir.citizen() == Some(peer)))
}

#[cfg(test)]
mod tests {
    use super::*;

    // d86: an offline root must neither multiply blocking workers nor hold the
    // registry/durable lock while another volume admits a writer.
    #[tokio::test]
    async fn unavailable_root_reuses_pending_work_without_blocking_other_volume() {
        let registry = Registry::new(Vec::new()).await;
        let (adapter, _database) = crate::orm::store::fresh_adapter().await;
        let unavailable = tempfile::tempdir().unwrap();
        let available = tempfile::tempdir().unwrap();
        let key = (unavailable.path().to_path_buf(), PathWork::Resolve);
        let identity = Arc::new(());
        let (complete, completion) = tokio::sync::oneshot::channel::<Observation>();
        let request = async move { completion.await.map_err(|error| error.to_string()) }
            .boxed()
            .shared();
        registry
            .2
            .lock()
            .insert(key.clone(), (identity.clone(), request.clone()));
        registry.2.lock().insert(
            (unavailable.path().to_path_buf(), PathWork::Observe),
            (identity.clone(), request),
        );
        registry.0.lock().unresolved_boot.push(TrackedDir::new(
            "cargo-target",
            unavailable.path().to_path_buf(),
        ));
        registry.open_records(adapter.clone()).await.unwrap();
        assert!(
            registry.snapshot().is_empty(),
            "unresolved boot paths are not guessed physical roots"
        );
        for _ in 0..2 {
            assert!(registry
                .admit(unavailable.path())
                .await
                .unwrap_err()
                .contains("still pending"));
            assert!(Arc::ptr_eq(
                &registry.2.lock().get(&key).unwrap().0,
                &identity
            ));
        }
        registry.admit(available.path()).await.unwrap();
        assert_eq!(registry.snapshot().len(), 1);
        assert_eq!(
            registry.2.lock().len(),
            2,
            "the original unresolved request remains quarantined"
        );
        // Simulate the original filesystem operation becoming available. The
        // existing restore tick consumes the pending configuration and persists it.
        complete
            .send(Observation {
                path: unavailable.path().canonicalize().unwrap(),
                volume: Some(crate::paths::cargo::volume_id(unavailable.path()).unwrap()),
                empty_custody: None,
            })
            .unwrap();
        registry.open_records(adapter.clone()).await.unwrap();
        assert!(registry.0.lock().unresolved_boot.is_empty());
        let restarted = Registry::new(Vec::new()).await;
        restarted.open_records(adapter).await.unwrap();
        assert_eq!(
            restarted.snapshot().len(),
            2,
            "both physical roots survive restart"
        );
    }

    // 5058: retirement must coordinate persistent ownership and the live pool,
    // retain a writer and unknown/replaced volumes, and stay retired on restart.
    #[tokio::test]
    async fn empty_root_retirement_respects_writer_custody_and_native_volume() {
        let (adapter, _database) = crate::orm::store::fresh_adapter().await;
        let empty = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let unknown = tempfile::tempdir().unwrap();
        let replaced = tempfile::tempdir().unwrap();
        std::fs::write(other.path().join("artifact"), b"keep").unwrap();
        let registry = Registry::new(Vec::new()).await;
        registry.open_records(adapter.clone()).await.unwrap();
        let broker = Arc::new(PressureBroker::new(Default::default()));
        registry.0.lock().consumers = Some(Consumers {
            monitor: Weak::new(),
            broker: Arc::downgrade(&broker),
        });
        let hold = registry.admit(empty.path()).await.unwrap();
        drop(registry.admit(other.path()).await.unwrap());
        drop(registry.admit(unknown.path()).await.unwrap());
        drop(registry.admit(replaced.path()).await.unwrap());
        {
            let mut durable = registry.1.lock().await;
            let records = durable.as_mut().unwrap();
            records
                .remember(
                    "cargo-target",
                    &unknown.path().canonicalize().unwrap(),
                    None,
                )
                .await
                .unwrap();
            let volume = crate::paths::cargo::volume_id(replaced.path()).unwrap();
            records
                .remember(
                    "cargo-target",
                    &replaced.path().canonicalize().unwrap(),
                    Some(volume.wrapping_add(1)),
                )
                .await
                .unwrap();
        }
        registry.retire_empty().await.unwrap();
        assert_eq!(
            registry.snapshot().len(),
            4,
            "writer, content and uncertain devices remain owned"
        );
        assert_eq!(broker.snapshot().pools.len(), 4);
        drop(hold);
        // The probe may finish after a new writer entered. The durable transaction
        // must recheck admission, not trust the earlier candidate list.
        let physical = empty.path().canonicalize().unwrap();
        let (complete, completion) = tokio::sync::oneshot::channel::<Observation>();
        registry.2.lock().insert(
            (physical.clone(), PathWork::Empty),
            (
                Arc::new(()),
                async move { completion.await.map_err(|error| error.to_string()) }
                    .boxed()
                    .shared(),
            ),
        );
        let retirement = registry.retire_empty();
        tokio::pin!(retirement);
        assert!(futures::poll!(&mut retirement).is_pending());
        let late = registry.admit(empty.path()).await.unwrap();
        complete
            .send(Observation {
                path: physical.clone(),
                volume: Some(crate::paths::cargo::volume_id(&physical).unwrap()),
                empty_custody: CargoTargetPool::empty_root_custody(&physical)
                    .unwrap()
                    .map(Arc::new),
            })
            .unwrap();
        retirement.await.unwrap();
        assert_eq!(
            registry.snapshot().len(),
            4,
            "a late admission prevents retirement"
        );
        drop(late);
        registry.retire_empty().await.unwrap();
        assert_eq!(registry.snapshot().len(), 3);
        assert_eq!(broker.snapshot().pools.len(), 3);
        assert!(
            empty.path().is_dir(),
            "retiring metadata does not delete a root"
        );
        let restarted = Registry::new(Vec::new()).await;
        restarted.open_records(adapter).await.unwrap();
        assert_eq!(restarted.snapshot().len(), 3);
        assert!(!restarted
            .snapshot()
            .iter()
            .any(|dir| dir.path() == empty.path().canonicalize().unwrap()));
    }

    #[tokio::test]
    async fn writer_admission_is_durable_before_restart_with_changed_configuration() {
        let (adapter, _database) = crate::orm::store::fresh_adapter().await;
        let old = tempfile::tempdir().unwrap();
        let later = tempfile::tempdir().unwrap();
        let boot = || vec![TrackedDir::new("cargo-target", old.path().to_path_buf())];
        let first = Registry::new(boot()).await;
        assert!(
            first.admit(later.path()).await.is_err(),
            "writes wait for ownership replay"
        );
        // A failed boot open must leave the same owner retryable, not mark an
        // empty catalog ready or require a second registry after disk recovery.
        let unavailable = Arc::new(crate::orm::sqlite::SqliteAdapter::new());
        assert!(first.open_records(unavailable).await.is_err());
        assert!(first.admit(later.path()).await.is_err());
        first.open_records(adapter.clone()).await.unwrap();
        first.admit(later.path()).await.unwrap();
        first.admit(&later.path().join("unused/..")).await.unwrap();
        let citizen = uuid::Uuid::new_v4();
        let observed = first.observe_citizen_target(later.path(), citizen).await.unwrap();
        assert_eq!(observed.citizen(), Some(citizen));
        observed.set_bytes(4096);
        assert_eq!(super::super::disk_pressure::DiskReporter::report(observed.as_ref()).bytes, 0,
            "an already registered generic reporter stops double charging its inclusive parent");
        let in_flight = observed.eviction_custody();
        assert!(first.observe_citizen_target(later.path(), citizen).await.is_err(),
            "a later scan can fail to publish while the restored child remains owned");
        assert_eq!(super::super::disk_pressure::DiskReporter::report(observed.as_ref()).bytes, 0);
        assert_eq!(observed.bytes(), 4096, "broker measurement survives publication failure");
        drop(in_flight);
        assert!(first.observe_citizen_target(later.path(), uuid::Uuid::new_v4()).await.is_err(),
            "a historic target cannot be reassigned to different hands");
        drop(first.admit(later.path()).await.unwrap());
        assert_eq!(observed.citizen(), Some(citizen), "later writer admission preserves resident custody");
        // Cormac's 5058 P2: the actual bounded preparation path must refuse
        // sensitive roots before mkdir, even when lexically hidden behind '..'.
        let home = tempfile::tempdir().unwrap();
        let policy = crate::code::confinement::Confinement::for_caller(
            home.path(),
            &home.path().join(".continuum"),
            "",
            later.path(),
        );
        for forbidden in [
            home.path().to_path_buf(),
            home.path().join("unused/../.local/share/keyrings/derived"),
            home.path().join(".continuum/citizens/peers/other/derived"),
        ] {
            assert!(first
                .path_work(&forbidden, PathWork::Prepare(policy.clone()))
                .await
                .is_err());
        }
        assert!(!home.path().join(".local").exists());
        assert!(!home.path().join(".continuum").exists());
        drop(first);
        let restarted = Registry::new(boot()).await;
        restarted.open_records(adapter).await.unwrap();
        let restored_child = restarted.snapshot().into_iter()
            .find(|dir| dir.path() == later.path().canonicalize().unwrap()).unwrap();
        assert_eq!(restored_child.citizen(), Some(citizen), "restart retains the eviction's hands gate");
        restored_child.set_bytes(4096);
        let eviction = restored_child.eviction_custody();
        assert!(restarted.observe_citizen_target(later.path(), citizen).await.is_err());
        assert_eq!(super::super::disk_pressure::DiskReporter::report(restored_child.as_ref()).bytes, 0,
            "a failed update of an already restored child cannot double charge the inclusive parent");
        assert_eq!(restored_child.bytes(), 4096);
        drop(eviction);
        assert_eq!(
            restarted.snapshot().len(),
            2,
            "a session-only override remains owned when absent from new configuration"
        );
        assert!(restarted
            .snapshot()
            .iter()
            .any(|dir| dir.path() == later.path().canonicalize().unwrap()));
        let broker = PressureBroker::new(Default::default());
        for dir in restarted.snapshot() {
            register_pool(&broker, dir);
        }
        assert_eq!(broker.snapshot().pools.len(), 2);
    }

    #[tokio::test]
    async fn later_writer_roots_keep_legacy_ownership_and_deduplicate_aliases() {
        let legacy = tempfile::tempdir().unwrap();
        let configured = tempfile::tempdir().unwrap();
        let old = TrackedDir::new("cargo-target", legacy.path().to_path_buf());
        let registry = Registry::new(vec![old.clone()]).await;
        let new = registry
            .register(&configured.path().join("target"))
            .await
            .unwrap();
        let alias = registry
            .register(&configured.path().join("unused/../target"))
            .await
            .unwrap();
        assert!(Arc::ptr_eq(&new, &alias));
        assert!(Arc::ptr_eq(
            &registry.snapshot()[0],
            &registry.register(legacy.path()).await.unwrap()
        ));
        assert_eq!(
            registry.snapshot().len(),
            2,
            "scanner must see roots added after boot"
        );
        let broker = PressureBroker::new(Default::default());
        for dir in registry.snapshot() {
            register_pool(&broker, dir);
        }
        assert_eq!(
            broker.snapshot().pools.len(),
            2,
            "same-class roots both retain eviction owners"
        );
    }
}
