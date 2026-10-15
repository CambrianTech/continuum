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
}
type Resolution = futures::future::Shared<
    futures::future::BoxFuture<'static, Result<std::path::PathBuf, String>>,
>;
#[derive(Clone, PartialEq, Eq, Hash)]
enum PathWork {
    Resolve,
    Validate(crate::code::confinement::Confinement),
    Prepare(crate::code::confinement::Confinement),
}
pub(super) struct Registry(
    Mutex<State>,
    tokio::sync::Mutex<Option<DurableRoots>>,
    Mutex<std::collections::HashMap<(std::path::PathBuf, PathWork), (Arc<()>, Resolution)>>,
);

struct DurableRoots {
    store: OrmStore<RootRecord>,
    saved: std::collections::HashSet<std::path::PathBuf>,
}

impl Registry {
    pub(super) async fn new(dirs: Vec<Arc<TrackedDir>>) -> Self {
        let registry = Self(
            Mutex::new(State {
                dirs: Vec::new(),
                unresolved_boot: Vec::new(),
                boot_reporters: 0,
                consumers: None,
            }),
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
                        eprintln!("Cargo ownership retains unavailable root: {error}");
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
        }
        registry
    }

    async fn resolve(&self, path: &Path) -> Result<std::path::PathBuf, String> {
        self.path_work(path, PathWork::Resolve).await
    }

    async fn path_work(&self, path: &Path, work: PathWork) -> Result<std::path::PathBuf, String> {
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
                            match &work {
                                PathWork::Resolve => crate::paths::cargo::physical_root(&path),
                                PathWork::Validate(policy) | PathWork::Prepare(policy) => {
                                    policy.validate_cargo_target(&path)
                                }
                            }
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
        Ok(self.register_resolved(physical))
    }

    fn register_resolved(&self, physical: std::path::PathBuf) -> Arc<TrackedDir> {
        let mut state = self.0.lock();
        if let Some(dir) = state
            .dirs
            .iter()
            .find(|dir| CARGO_TARGET_CLASSES.contains(&dir.class()) && dir.path() == physical)
        {
            return dir.clone();
        }
        let dir = TrackedDir::new("cargo-target", physical);
        if let Some(consumers) = &state.consumers {
            if let Some(monitor) = consumers.monitor.upgrade() {
                monitor.add_reporter(dir.clone());
            }
            if let Some(broker) = consumers.broker.upgrade() {
                register_pool(&broker, dir.clone());
            }
        }
        state.dirs.push(dir.clone());
        dir
    }

    async fn open_records(&self, adapter: Arc<dyn StorageAdapter>) -> Result<(), String> {
        // Retry unresolved boot configuration in the existing owner's tick. It
        // remains pending, never persisted or pooled as a guessed physical root.
        // Known durable roots below can be restored even while a mount is offline.
        let pending = self.0.lock().unresolved_boot.clone();
        for dir in pending {
            if let Ok(physical) = self.resolve(dir.path()).await {
                self.register_resolved(physical);
                self.0
                    .lock()
                    .unresolved_boot
                    .retain(|entry| !Arc::ptr_eq(entry, &dir));
            }
        }
        let mut durable = self.1.lock().await;
        if let Some(records) = durable.as_mut() {
            for dir in self
                .snapshot()
                .into_iter()
                .filter(|dir| CARGO_TARGET_CLASSES.contains(&dir.class()))
            {
                records.remember(dir.class(), dir.path()).await?;
            }
            return Ok(());
        }
        let store = OrmStore::<RootRecord>::new(adapter)
            .await
            .map_err(|error| error.to_string())?;
        let mut saved = std::collections::HashSet::new();
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
            self.register_resolved(path.clone());
            saved.insert(path);
        }
        let mut records = DurableRoots { store, saved };
        for dir in self
            .snapshot()
            .into_iter()
            .filter(|dir| CARGO_TARGET_CLASSES.contains(&dir.class()))
        {
            if !records.saved.contains(dir.path()) {
                records.remember(dir.class(), dir.path()).await?;
            }
        }
        *durable = Some(records);
        Ok(())
    }

    async fn admit(&self, path: &Path) -> Result<(), String> {
        let physical = self.resolve(path).await?;
        let mut durable = self.1.lock().await;
        let records = durable
            .as_mut()
            .ok_or_else(|| "disk root ownership has not restored yet".to_string())?;
        records.remember("cargo-target", &physical).await?;
        self.register_resolved(physical);
        Ok(())
    }
}

impl DurableRoots {
    async fn remember(&mut self, class: &str, path: &Path) -> Result<(), String> {
        // Boot/replay retain unavailable volumes as named ownership. New writes
        // already require successful physical resolution in admit().
        let physical = path.to_path_buf();
        if !physical.is_absolute() {
            return Err("cache ownership requires an absolute root".into());
        }
        if self.saved.contains(&physical) {
            return Ok(());
        }
        let record = RootRecord::new(class, &physical);
        let id = uuid::Uuid::parse_str(&record.base.id).map_err(|error| error.to_string())?;
        self.store
            .save(id, &record)
            .await
            .map_err(|error| error.to_string())?;
        self.saved.insert(physical);
        Ok(())
    }
}

pub async fn restore_cargo_roots(adapter: Arc<dyn StorageAdapter>) -> Result<(), String> {
    if let Some(registry) = TRACKED_DIRS.get() {
        registry.open_records(adapter).await?;
    }
    Ok(())
}

/// Durable admission before a writer can launch. Failure to save refuses that
/// write rather than produce an unowned tree after the next restart.
pub async fn admit_cargo_target(
    path: &Path,
    cwd: &Path,
    confinement: Option<&crate::code::confinement::Confinement>,
) -> Result<Option<crate::code::confinement::CargoTargetGrant>, String> {
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
    registry.admit(path).await?;
    // Only operator configuration grants creation outside a citizen workspace.
    // Session overrides acquire ownership but gain no filesystem privileges.
    let configured = crate::paths::cargo::target_dir(cwd)?;
    if registry.resolve(path).await? == registry.resolve(&configured).await? {
        let prepared = registry
            .path_work(path, PathWork::Prepare(policy.clone()))
            .await?;
        return Ok(Some(crate::code::confinement::CargoTargetGrant::admitted(
            prepared, policy,
        )));
    }
    Ok(None)
}

fn register_pool(broker: &PressureBroker, dir: Arc<TrackedDir>) {
    let budget = DiskPressureMonitor::volume_for(dir.path())
        .map(|volume| super::super::cargo_target_budget_bytes(volume.total_bytes))
        .unwrap_or(super::super::DEFAULT_CARGO_TARGET_BUDGET_BYTES);
    broker.register(Arc::new(CargoTargetPool::new(dir, budget)));
}

/// Connect the existing boot consumers once. Monitor already owns the boot
/// reporters; roots registered before binding are included in the same snapshot.
pub fn bind_cargo_owners(monitor: &Arc<DiskPressureMonitor>, broker: &Arc<PressureBroker>) {
    let Some(registry) = TRACKED_DIRS.get() else {
        return;
    };
    let mut state = registry.0.lock();
    assert!(state.consumers.is_none(), "Cargo owners bound twice");
    for (index, dir) in state.dirs.iter().enumerate() {
        if index >= state.boot_reporters {
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
        let (complete, completion) = tokio::sync::oneshot::channel::<std::path::PathBuf>();
        let request = async move { completion.await.map_err(|error| error.to_string()) }
            .boxed()
            .shared();
        registry
            .2
            .lock()
            .insert(key.clone(), (identity.clone(), request));
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
            1,
            "the original unresolved request remains quarantined"
        );
        // Simulate the original filesystem operation becoming available. The
        // existing restore tick consumes the pending configuration and persists it.
        complete
            .send(unavailable.path().canonicalize().unwrap())
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
