#![cfg(test)]
use super::*;
use crate::system_resources::disk_eviction::tests::serving_tier::set_mtime;
use crate::system_resources::drives::DriveRole;

const DAY: std::time::Duration = std::time::Duration::from_secs(86_400);

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

/// A pool whose every host input is the fixture's: no measured volume, no writer lease,
/// an empty serving custody set. A test about one of those sets it after this.
fn fixture_pool(root: &Path) -> WorktreeTargetsPool {
    let tracked = TrackedDir::new("wt-targets", root.canonicalize().expect("root"));
    tracked.set_bytes(dir_size_bytes(root));
    let mut pool = WorktreeTargetsPool::new(tracked);
    pool.volume_sample = |_| None;
    pool.leased = |_| false;
    pool.active_artifacts = Arc::new(ActiveArtifactSet::default());
    pool
}

fn detached_remnants(root: &Path) -> Vec<String> {
    std::fs::read_dir(root)
        .expect("root")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(DETACHED_PREFIX))
        .collect()
}

// what this catches (the M5, 2026-10-29: 448 GB of per-worktree targets, ENOSPC): an
// over-budget wt-targets pool evicts the least recently built UNHELD target first, whole,
// and stops once the ask is met; a target a live build locks or an admitted writer leases is
// never touched even when it is the oldest. Mutation checks: dropping the lease check evicts
// `leased` first; sorting newest-first evicts `new`; ignoring profile locks evicts `building`.
#[test]
fn evicts_the_least_recently_built_unheld_target_first_and_never_a_held_one() {
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
    let mut pool = fixture_pool(&root);
    pool.leased = |dir| dir.ends_with("leased");

    assert_eq!(
        pool.evict_at_least(1),
        1000,
        "one whole target, the least recently built unheld one"
    );
    assert!(
        !old.exists(),
        "the least recently built unheld target goes first"
    );
    assert!(building.exists() && leased.exists() && new.exists());

    assert_eq!(
        pool.evict_at_least(u64::MAX),
        1000,
        "only `new` is left unheld"
    );
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
    let (_, live) = WorktreeTargetsPool::candidates(&root);
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
// volume brings the broker, relieve() actually removes bytes from disk, and a detached tree
// a crashed eviction left behind is reclaimed with it. Before 2026-10-29 no pool existed for
// this class, so relieve() reached nothing while the volume filled.
#[test]
fn broker_relieve_actually_deletes_from_an_over_budget_wt_targets_pool() {
    use crate::paging::{BrokerConfig, PressureBroker};
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path().join("wt-targets");
    let old = target(&root, "old", 2);
    let remnant = root.join(format!("{DETACHED_PREFIX}crashed"));
    std::fs::create_dir_all(remnant.join("debug")).expect("remnant");
    std::fs::write(remnant.join("debug/left.rlib"), vec![0u8; 500]).expect("remnant bytes");
    let mut pool = fixture_pool(&root);
    pool.volume_sample = |path| {
        Some(DriveInfo {
            mount: path.into(),
            total_bytes: 100_000,
            available_bytes: 0,
            role: DriveRole::System,
        })
    };
    let pool = Arc::new(pool);
    let broker = PressureBroker::new(BrokerConfig::default());
    broker.register(pool.clone() as Arc<dyn ResourcePool>);

    let before = dir_size_bytes(&root);
    let report = broker.relieve();

    assert!(
        report.triggered,
        "a wt-targets pool on a critical volume must bring the broker"
    );
    assert!(
        report.bytes_freed >= 1500,
        "relief frees the target and the remnant: {}",
        report.bytes_freed
    );
    assert!(
        dir_size_bytes(&root) < before,
        "bytes leave the disk, not just the accounting"
    );
    assert!(!old.exists() && !remnant.exists());
}
