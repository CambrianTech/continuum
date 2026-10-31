#![cfg(test)]
use super::*;
use crate::paging::{BrokerConfig, PressureBroker};
use crate::system_resources::disk_eviction::tests::serving_tier::set_mtime;
use crate::system_resources::disk_reporters::Kept;
use crate::system_resources::drives::{DriveInfo, DriveRole};
use std::sync::atomic::{AtomicBool, Ordering};

const DAY: std::time::Duration = std::time::Duration::from_secs(86_400);

/// derived-or-floor: a liveness bound for waiting out a background job on a tempdir of a
/// few KB, never an assertion about speed; a job that outlives it fails the test loudly.
const LIVENESS: std::time::Duration = std::time::Duration::from_secs(30);

/// One worktree target under `root`: a debug profile with its build lock and one 1,000-byte
/// unit in `deps`, every entry stamped `days_ago` so the dir reads as last built then.
fn target(root: &Path, name: &str, days_ago: u32) -> PathBuf {
    let dir = root.join(name);
    std::fs::create_dir_all(dir.join("debug/deps")).expect("mkdir");
    std::fs::write(dir.join("debug/.cargo-lock"), b"").expect("lock");
    std::fs::write(dir.join("debug/deps/unit.rlib"), vec![0u8; 1000]).expect("unit");
    let at = std::time::SystemTime::now() - DAY * days_ago;
    for path in [
        dir.join("debug/deps/unit.rlib"),
        dir.join("debug/.cargo-lock"),
        dir.join("debug/deps"),
        dir.join("debug"),
        dir.clone(),
    ] {
        set_mtime(&path, at);
    }
    dir
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .build()
        .expect("runtime")
}

/// The blocking half with every host input the fixture's: no measured volume, no writer
/// lease, an empty serving custody set. A test about one of those sets it after this.
fn fixture_targets(root: &Path) -> Targets {
    let tracked = TrackedDir::new("wt-targets", root.canonicalize().expect("root"));
    tracked.set_bytes(dir_size_bytes(root));
    let mut targets = Targets::new(tracked);
    targets.volume_sample = |_| None;
    targets.claim = |_| Ok(Arc::new(()) as Kept);
    targets.active_artifacts = Arc::new(ActiveArtifactSet::default());
    targets
}

fn wait_idle(pool: &WorktreeTargetsPool) {
    let start = std::time::Instant::now();
    while pool.busy() {
        assert!(
            start.elapsed() < LIVENESS,
            "a background job outlived the liveness bound"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

/// The blocking half over `root` with one more fixture input set.
fn pool_over(
    rt: &tokio::runtime::Runtime,
    root: &Path,
    budget: fn(Option<&DriveInfo>) -> u64,
    volume: fn(&Path) -> Option<DriveInfo>,
) -> WorktreeTargetsPool {
    let mut targets = fixture_targets(root);
    targets.budget_for = budget;
    targets.volume_sample = volume;
    WorktreeTargetsPool::from_targets(targets, rt.handle().clone(), WorktreeEvictionMode::Delete)
}

/// One direct tick and the job it started, waited out. Returns what that job freed.
fn tick(pool: &WorktreeTargetsPool, want: u64) -> u64 {
    pool.evict_at_least(want);
    wait_idle(pool);
    pool.freed.swap(0, Ordering::AcqRel)
}

/// `n` broker passes, each job waited out; returns the bytes the broker was told of.
fn broker_ticks(broker: &PressureBroker, pool: &WorktreeTargetsPool, n: usize) -> u64 {
    let mut reported = 0;
    for _ in 0..n {
        reported += broker.relieve().bytes_freed;
        wait_idle(pool);
    }
    reported
}

fn calm_volume(path: &Path) -> Option<DriveInfo> {
    Some(DriveInfo {
        mount: path.into(),
        total_bytes: 100_000,
        available_bytes: 90_000,
        role: DriveRole::System,
    })
}

fn critical_volume(path: &Path) -> Option<DriveInfo> {
    Some(DriveInfo {
        mount: path.into(),
        total_bytes: 100_000,
        available_bytes: 0,
        role: DriveRole::System,
    })
}

fn detached_remnants(root: &Path) -> Vec<String> {
    std::fs::read_dir(root)
        .expect("root")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(DETACHED_PREFIX))
        .collect()
}

// what this catches (the M5, 2026-10-29: 448 GB of per-worktree targets, ENOSPC): the
// least recently built UNHELD target goes first, whole, and the eviction stops once the ask
// is met; a target a live build locks or an admitted writer leases is never touched even
// when it is the oldest. Mutation checks: dropping the lease check evicts `leased` first;
// sorting newest-first evicts `new`; ignoring profile locks evicts `building`.
#[test]
fn evicts_the_least_recently_built_unheld_target_first_and_never_a_held_one() {
    let rt = runtime();
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path().join("wt-targets");
    let building = target(&root, "building", 4);
    let leased = target(&root, "leased", 3);
    let old = target(&root, "old", 2);
    let new = target(&root, "new", 1);
    let held = try_exclusive_flock_detailed(&building.join("debug/.cargo-lock"));
    assert!(
        matches!(held, LockAttempt::Acquired(_)),
        "the fixture build holds its profile lock"
    );
    let mut targets = fixture_targets(&root);
    targets.claim = |dir| {
        if dir.ends_with("leased") {
            Err("an admitted cargo writer holds its lease".into())
        } else {
            Ok(Arc::new(()) as Kept)
        }
    };
    let pool = WorktreeTargetsPool::from_targets(
        targets,
        rt.handle().clone(),
        WorktreeEvictionMode::Delete,
    );

    assert_eq!(tick(&pool, 1), 0, "the first tick only measures");
    assert_eq!(
        tick(&pool, 1),
        1000,
        "one whole target, the least recently built unheld one"
    );
    assert!(
        !old.exists(),
        "the least recently built unheld target goes first"
    );
    assert!(building.exists() && leased.exists() && new.exists());

    assert_eq!(tick(&pool, u64::MAX), 1000, "only `new` is left unheld");
    assert!(!new.exists());
    assert!(
        building.join("debug/deps/unit.rlib").exists(),
        "a locked build's target is never evicted"
    );
    assert!(
        leased.join("debug/deps/unit.rlib").exists(),
        "a leased writer's target is never evicted"
    );
    assert!(
        detached_remnants(&root).is_empty(),
        "an eviction leaves no detached tree behind"
    );
    drop(held);
}

// what this catches: the LRU basis. A build writes new units into `<profile>/deps`, which
// moves that directory's mtime; the target must then rank as recently built even though its
// root and profile dirs are old. Mutation check: reading only the root's own mtime ranks
// `rebuilt` (older root) first.
#[test]
fn a_build_writing_into_deps_makes_its_target_recent() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path().join("wt-targets");
    let rebuilt = target(&root, "rebuilt", 5);
    target(&root, "idle", 2);
    std::fs::write(rebuilt.join("debug/deps/fresh.rlib"), b"unit").expect("a build lands a unit");
    let (_, live) = Targets::candidates(&root);
    let order: Vec<String> = live
        .iter()
        .map(|c| {
            c.path
                .file_name()
                .expect("name")
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    assert_eq!(
        order,
        ["idle", "rebuilt"],
        "least recently built first, by what the build wrote"
    );
}

// what this catches: the chain, end to end, on a real tree. A wt-targets pool on a critical
// volume brings the broker, relief actually removes bytes from disk, the broker is told of
// them, and a detached tree a crashed eviction left behind is reclaimed with it. Before
// 2026-10-29 no pool existed for this class, so relief reached nothing while the volume filled.
#[test]
fn broker_relieve_actually_deletes_from_an_over_budget_wt_targets_pool() {
    let rt = runtime();
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path().join("wt-targets");
    let old = target(&root, "old", 2);
    let remnant = root.join(format!("{DETACHED_PREFIX}crashed"));
    std::fs::create_dir_all(remnant.join("debug")).expect("remnant");
    std::fs::write(remnant.join("debug/left.rlib"), vec![0u8; 500]).expect("remnant bytes");
    let mut targets = fixture_targets(&root);
    targets.volume_sample = critical_volume;
    let pool = Arc::new(WorktreeTargetsPool::from_targets(
        targets,
        rt.handle().clone(),
        WorktreeEvictionMode::Delete,
    ));
    let broker = PressureBroker::new(BrokerConfig::default());
    broker.register(pool.clone() as Arc<dyn ResourcePool>);

    let before = dir_size_bytes(&root);
    // Measure, evict, then the tick that reports what the eviction freed.
    let reported = broker_ticks(&broker, &pool, 3);

    assert!(
        reported >= 1500,
        "the broker is told of the target and the remnant: {reported}"
    );
    assert!(
        dir_size_bytes(&root) < before,
        "bytes leave the disk, not just the accounting"
    );
    assert!(!old.exists() && !remnant.exists());
}

// what this catches: KEEP RECENT, with no disk pressure (Joel, 2026-10-29: build caches
// rebuild, so old ones are disposable at any time). On a calm volume an over-budget class
// keeps its newest targets that fit the budget and evicts the older ones through the broker's
// ordinary tick, a held one never; later ticks do not eat into what fits. Mutation checks:
// dropping the trim leaves `older` (the calm volume never asks for it); ranking oldest first
// keeps `older` and evicts `newest`; letting the broker's 60% ask apply over budget evicts `newer`.
#[test]
fn a_calm_volume_keeps_the_newest_targets_within_budget_and_evicts_the_older_ones() {
    assert_eq!(Targets::beyond_budget(&[1000, 1000, 1000, 1000], 2500), 2);
    assert_eq!(
        Targets::beyond_budget(&[1000, 1000], 2500),
        0,
        "within budget, nothing goes"
    );
    assert_eq!(
        Targets::beyond_budget(&[10, 5000], 100),
        1,
        "the newest is kept even alone over budget"
    );

    let rt = runtime();
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path().join("wt-targets");
    let oldest_held = target(&root, "oldest-held", 4);
    let older = target(&root, "older", 3);
    let newer = target(&root, "newer", 2);
    let newest = target(&root, "newest", 1);
    let held = try_exclusive_flock_detailed(&oldest_held.join("debug/.cargo-lock"));
    assert!(
        matches!(held, LockAttempt::Acquired(_)),
        "the fixture build holds its profile lock"
    );
    let mut targets = fixture_targets(&root);
    targets.budget_for = |_| 2500;
    targets.volume_sample = calm_volume;
    let pool = Arc::new(WorktreeTargetsPool::from_targets(
        targets,
        rt.handle().clone(),
        WorktreeEvictionMode::Delete,
    ));
    let broker = PressureBroker::new(BrokerConfig::default());
    broker.register(pool.clone() as Arc<dyn ResourcePool>);

    assert_eq!(
        broker_ticks(&broker, &pool, 3),
        1000,
        "exactly the unheld target outside the budget"
    );
    assert!(
        !older.exists(),
        "the older unheld target outside the budget goes"
    );
    assert!(
        newest.exists() && newer.exists(),
        "the newest targets that fit the budget stay"
    );
    assert!(
        oldest_held.join("debug/deps/unit.rlib").exists(),
        "a held target is never evicted"
    );

    broker_ticks(&broker, &pool, 2);
    assert!(
        newest.exists() && newer.exists(),
        "later ticks keep what fits the budget"
    );
    assert!(oldest_held.join("debug/deps/unit.rlib").exists());
    drop(held);
}

static WALK_ENTERED: AtomicBool = AtomicBool::new(false);
static WALK_RELEASED: AtomicBool = AtomicBool::new(false);

/// A walk that blocks until the test releases it: the stand-in for 448 GB.
fn blocking_walk(path: &Path) -> u64 {
    WALK_ENTERED.store(true, Ordering::Release);
    while !WALK_RELEASED.load(Ordering::Acquire) {
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    dir_size_bytes(path)
}

// what this catches (CONCURRENCY-STYLE-GUIDE: a tick never waits on a filesystem walk):
// with the class's measurement blocked mid-walk, the broker's relief pass and the pool's
// pressure reading both return, and the walk is still blocked when they do; once the walk
// finishes, the eviction it measured for goes through. Mutation check: measuring inside
// `evict_at_least` hangs the first relief pass until the liveness bound fails it.
#[test]
fn a_slow_walk_never_delays_the_brokers_tick_or_the_pressure_reading() {
    let rt = runtime();
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path().join("wt-targets");
    let older = target(&root, "older", 2);
    target(&root, "newest", 1);
    let mut targets = fixture_targets(&root);
    targets.budget_for = |_| 1500;
    targets.volume_sample = calm_volume;
    targets.walk = blocking_walk;
    let pool = Arc::new(WorktreeTargetsPool::from_targets(
        targets,
        rt.handle().clone(),
        WorktreeEvictionMode::Delete,
    ));
    let broker = Arc::new(PressureBroker::new(BrokerConfig::default()));
    broker.register(pool.clone() as Arc<dyn ResourcePool>);

    let (done, returned) = std::sync::mpsc::channel();
    let (tick_broker, tick_pool) = (broker.clone(), pool.clone());
    std::thread::spawn(move || {
        tick_broker.relieve();
        let start = std::time::Instant::now();
        while !WALK_ENTERED.load(Ordering::Acquire) {
            assert!(start.elapsed() < LIVENESS, "the measurement never started");
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        tick_broker.relieve();
        let pressure = tick_pool.pressure();
        done.send(pressure).expect("report");
    });
    let pressure = returned
        .recv_timeout(LIVENESS)
        .expect("relief and pressure returned while the walk was blocked");
    assert!(
        !WALK_RELEASED.load(Ordering::Acquire),
        "they returned before the walk did"
    );
    assert!(
        pressure > 1.0,
        "the over-budget reading is the scanner's, available mid-walk"
    );

    WALK_RELEASED.store(true, Ordering::Release);
    wait_idle(&pool);
    broker_ticks(&broker, &pool, 2);
    assert!(
        !older.exists(),
        "the measured eviction goes through once the walk finishes"
    );
}

// what this catches (Codex on 9a5882d2d): keep-recent must yield to real volume pressure.
// One target alone over the budget is the newest, which keep-recent keeps; on a hot volume
// the broker's ask applies past the trim and takes it, unless it is held. Mutation check:
// gating the ask on "within budget" alone leaves `only` on a full disk.
#[test]
fn a_hot_volume_reclaims_even_the_newest_target_over_budget_but_never_a_held_one() {
    let rt = runtime();
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path().join("wt-targets");
    let only = target(&root, "only", 1);
    let pool = pool_over(&rt, &root, |_| 500, critical_volume);
    assert_eq!(tick(&pool, 1), 0, "the first tick only measures");
    assert_eq!(
        tick(&pool, 1),
        1000,
        "the hot volume takes the one target over the budget"
    );
    assert!(!only.exists());

    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path().join("wt-targets");
    let only = target(&root, "only", 1);
    let held = try_exclusive_flock_detailed(&only.join("debug/.cargo-lock"));
    assert!(
        matches!(held, LockAttempt::Acquired(_)),
        "the fixture build holds its profile lock"
    );
    let pool = pool_over(&rt, &root, |_| 500, critical_volume);
    tick(&pool, 1);
    assert_eq!(
        tick(&pool, 1),
        0,
        "a held target is protected even on a full disk"
    );
    assert!(only.join("debug/deps/unit.rlib").exists());
    drop(held);
}

// what this catches (Codex on 9a5882d2d): a profile a build creates AFTER the snapshot's
// scan, holding its own `.cargo-lock`, must stop the eviction: the lock scan runs after the
// root authority's claim, not from the snapshot. Mutation check: taking the lock paths from
// the measurement evicts `target` while the release build holds its lock.
#[test]
fn a_profile_lock_created_after_the_snapshot_is_respected() {
    let rt = runtime();
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path().join("wt-targets");
    let target = target(&root, "target", 2);
    let pool = pool_over(&rt, &root, |_| u64::MAX, |_| None);
    assert_eq!(
        tick(&pool, 1),
        0,
        "the snapshot is taken before the new profile exists"
    );
    std::fs::create_dir_all(target.join("release")).expect("a build creates a profile");
    let held = try_exclusive_flock_detailed(&target.join("release/.cargo-lock"));
    assert!(
        matches!(held, LockAttempt::Acquired(_)),
        "the new build holds its profile lock"
    );
    assert_eq!(
        tick(&pool, 1),
        0,
        "the eviction finds the new lock held and stands down"
    );
    assert!(
        target.join("debug/deps/unit.rlib").exists() && target.join("release/.cargo-lock").exists()
    );
    drop(held);
}

// what this catches (Codex on #5601): production deletes nothing until build admission
// exists. In observe mode an over-budget class on a hot volume is measured and planned, the
// plan is published naming each target it would take, and every byte stays on disk; the
// broker neither asks it to free nor alerts on it, on a hot volume, every pass. Mutation
// checks: carrying the plan out in observe mode removes `older`; leaving observe pools in
// the relief candidates puts one "0 bytes freed" alert per pass into `alerts`.
#[test]
fn observe_mode_publishes_what_it_would_take_and_deletes_nothing() {
    let rt = runtime();
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path().join("wt-targets");
    let older = target(&root, "older", 2);
    let newest = target(&root, "newest", 1);
    let mut targets = fixture_targets(&root);
    targets.budget_for = |_| 1500;
    targets.volume_sample = critical_volume;
    let pool = Arc::new(WorktreeTargetsPool::from_targets(
        targets,
        rt.handle().clone(),
        WorktreeEvictionMode::Observe,
    ));
    let broker = PressureBroker::new(BrokerConfig::default());
    broker.register(pool.clone() as Arc<dyn ResourcePool>);
    let alerts = Arc::new(parking_lot::Mutex::new(Vec::new()));
    let sink = alerts.clone();
    broker.add_alert_sink(Arc::new(move |alert: crate::paging::PressureAlert| {
        sink.lock().push(alert.tier_name)
    }));

    assert_eq!(
        broker_ticks(&broker, &pool, 3),
        0,
        "nothing is freed in observe mode"
    );
    assert!(
        older.join("debug/deps/unit.rlib").exists() && newest.join("debug/deps/unit.rlib").exists()
    );
    let would: Vec<String> = pool
        .would_take
        .lock()
        .iter()
        .map(|(path, _)| {
            path.file_name()
                .expect("name")
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    assert_eq!(
        would,
        ["older", "newest"],
        "the budget trim, then the hot volume's ask, oldest first"
    );
    assert!(
        alerts.lock().is_empty(),
        "no relief request and no alert for an observe-only pool: {:?}",
        alerts.lock()
    );
}
