#![cfg(test)]
    use super::*;

    /// Set an mtime on a path that may be a FILE or a DIRECTORY.
    ///
    /// Windows will not hand you a handle to a directory through a
    /// plain `File::open` — it returns `PermissionDenied` (code 5)
    /// — so you must ask for `FILE_WRITE_ATTRIBUTES` access with
    /// `FILE_FLAG_BACKUP_SEMANTICS`, the documented way to open a
    /// directory handle. Both work for regular files too, so one
    /// helper covers both shapes on both platforms.
    ///
    /// what this fixes: these three `serving_tier` tests were RED on
    /// Windows and green everywhere else, because the helper opened
    /// `stale-model/` (a directory) as a file. Exactly the defect
    /// that also refused the probe sink's boot the same day — a
    /// directory opened as a file is invisible on macOS/Linux and
    /// fatal on Windows, so it survives review on the platform the
    /// author is using.
    fn set_mtime(path: &Path, t: std::time::SystemTime) {
        #[cfg(windows)]
        let f = {
            use std::os::windows::fs::OpenOptionsExt;
            const FILE_WRITE_ATTRIBUTES: u32 = 0x0100;
            const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
            std::fs::OpenOptions::new()
                .access_mode(FILE_WRITE_ATTRIBUTES)
                .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
                .open(path)
                .expect("open for mtime")
        };
        #[cfg(not(windows))]
        let f = std::fs::File::open(path).expect("open for mtime");
        f.set_modified(t).expect("mtime");
    }

    /// A hot-tier tree with two frozen model artifacts (one dir-shaped,
    /// one file-shaped) and one actively-served dir. mtimes are staged so
    /// `stale-model/` is coldest, then `old.gguf`, then the active dir.
    fn seeded_hot_tier(hot: &Path) -> Arc<TrackedDir> {
        std::fs::create_dir_all(hot.join("stale-model")).expect("mkdir");
        std::fs::write(hot.join("stale-model/model.gguf"), vec![1u8; 4000]).expect("write");
        std::fs::write(hot.join("old.gguf"), vec![2u8; 3000]).expect("write");
        std::fs::create_dir_all(hot.join("served-model")).expect("mkdir");
        std::fs::write(hot.join("served-model/model.gguf"), vec![3u8; 2000]).expect("write");
        // Stage mtimes: coldest first. filetime not in deps — touch via
        // set_modified (std, stable since 1.75).
        let t0 = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000);
        let t1 = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(2_000);
        set_mtime(&hot.join("stale-model"), t0);
        set_mtime(&hot.join("old.gguf"), t1);
        let tracked = TrackedDir::new("genome-models", hot.to_path_buf());
        tracked.set_bytes(dir_size_bytes(hot));
        tracked
    }

    // what this catches: #302 safety invariant 1 — the actively-paged
    // artifact is NEVER migrated, even when eviction wants unlimited
    // bytes. Losing the resident model's GGUF mid-serve is the engine
    // reading from a deleted file.
    #[tokio::test]
    async fn actively_paged_artifact_is_never_migrated() {
        crate::system_resources::disk_reporters::prepare_writer_fixture().await;
        let hot = tempfile::tempdir().expect("tempdir");
        let cold = tempfile::tempdir().expect("tempdir");
        let tracked = seeded_hot_tier(hot.path());
        let active = Arc::new(ActiveArtifactSet::default());
        let raw = hot.path().join("served-model");
        active.register_resolved(raw.clone(), ActiveArtifactSet::resolve(&raw).await);
        let pool = NvmeServingTierPool::new(
            tracked,
            1_000_000,
            Some(cold.path().to_path_buf()),
            active,
        );

        pool.evict_at_least(u64::MAX);
        assert!(
            hot.path().join("served-model/model.gguf").exists(),
            "the served model must survive unlimited eviction demand"
        );
        assert!(
            !hot.path().join("stale-model").exists(),
            "frozen artifacts migrate"
        );
        assert!(!hot.path().join("old.gguf").exists());
    }

    // what this catches: #302 safety invariant 2 — migration is
    // copy→verify→delete (the artifact EXISTS on cold before the hot
    // copy dies), coldest-first, and a verified twin already on cold is
    // a pure drop (no second copy). The shared TrackedDir decrements so
    // the broker doesn't re-fire on freed space.
    #[test]
    fn migrates_coldest_first_and_pure_drops_verified_twins() {
        let hot = tempfile::tempdir().expect("tempdir");
        let cold = tempfile::tempdir().expect("tempdir");
        let tracked = seeded_hot_tier(hot.path());
        let usage_before = tracked.bytes();
        // old.gguf already has a byte-identical frozen twin on cold.
        std::fs::write(cold.path().join("old.gguf"), vec![2u8; 3000]).expect("write");
        let pool = NvmeServingTierPool::new(
            tracked.clone(),
            1_000_000,
            Some(cold.path().to_path_buf()),
            Arc::new(ActiveArtifactSet::default()),
        );

        // Want only the coldest entry's worth: stale-model (4000 B).
        let freed = pool.evict_at_least(1000);
        assert_eq!(freed, 4000, "coldest entry goes first");
        assert!(
            cold.path().join("stale-model/model.gguf").exists(),
            "verified cold copy exists"
        );
        assert!(
            !hot.path().join("stale-model").exists(),
            "hot copy gone after verify"
        );
        assert!(
            hot.path().join("old.gguf").exists(),
            "later candidate untouched"
        );
        assert_eq!(tracked.bytes(), usage_before - 4000);

        // Second round: old.gguf's twin is already frozen — pure drop.
        let freed = pool.evict_at_least(1000);
        assert_eq!(freed, 3000);
        assert!(!hot.path().join("old.gguf").exists());
        assert!(cold.path().join("old.gguf").exists());
    }

    // what this catches: #302 safety invariant 3 — a box with no cold
    // drive frees NOTHING (models are hours of re-fetch, not derived
    // artifacts; pressure must stay visible, never "relieved" by
    // destroying them). Also: a cold-side name collision with DIFFERENT
    // content is never clobbered.
    #[test]
    fn no_cold_drive_frees_nothing_and_collisions_never_clobber() {
        let hot = tempfile::tempdir().expect("tempdir");
        let tracked = seeded_hot_tier(hot.path());
        let pool = NvmeServingTierPool::new(
            tracked.clone(),
            1_000_000,
            None,
            Arc::new(ActiveArtifactSet::default()),
        );
        assert_eq!(pool.evict_at_least(u64::MAX), 0, "no cold tier ⇒ refuse");
        assert!(hot.path().join("stale-model/model.gguf").exists());

        // Collision case: cold has a DIFFERENT old.gguf.
        let cold = tempfile::tempdir().expect("tempdir");
        std::fs::write(cold.path().join("old.gguf"), vec![9u8; 3000]).expect("write");
        let pool = NvmeServingTierPool::new(
            tracked,
            1_000_000,
            Some(cold.path().to_path_buf()),
            Arc::new(ActiveArtifactSet::default()),
        );
        pool.evict_at_least(u64::MAX);
        assert!(
            hot.path().join("old.gguf").exists(),
            "collision with different content: hot copy kept"
        );
        let cold_bytes = std::fs::read(cold.path().join("old.gguf")).expect("read");
        assert_eq!(
            cold_bytes,
            vec![9u8; 3000],
            "cold artifact never overwritten"
        );
    }

    // what this catches: @Astra, twice. First that `Option<File>` collapsed
    // "a build holds it" and "I could not open it" into one `None`; then that
    // my fix still misread contention ON WINDOWS, where fs2 reports
    // ERROR_LOCK_VIOLATION (33) rather than anything std maps to
    // `ErrorKind::WouldBlock`. Matching on the KIND classified a genuinely held
    // cargo lock as `Unavailable` — the exact misclassification the tri-state
    // exists to prevent, inverted, on the platform I run on.
    //
    // So this exercises REAL lock files rather than asserting the mapping:
    // acquire one, then attempt it again while the first guard is alive, on
    // whatever platform the test is running.
    #[test]
    fn a_held_lock_reads_as_contention_and_not_as_an_io_failure() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let lock = tmp.path().join(".cargo-lock");

        let first = try_exclusive_flock_detailed(&lock);
        let held = match first {
            LockAttempt::Acquired(f) => f,
            other => panic!("an uncontended lock must be Acquired, got {other:?}"),
        };

        // Same process, same file, while the first guard is alive.
        match try_exclusive_flock_detailed(&lock) {
            LockAttempt::HeldByAnother => {}
            LockAttempt::Acquired(_) => {
                panic!("a held lock must NOT be re-acquirable — the guard would be a no-op")
            }
            LockAttempt::Unavailable(why) => panic!(
                "a held lock was reported as an IO failure ({why}) — this is the                      Windows misclassification: contention is ERROR_LOCK_VIOLATION,                      which std does not map to WouldBlock"
            ),
        }

        drop(held);
        // And once released it is acquirable again, so the contention above was
        // the lock and not some permanent property of the path.
        assert!(
            matches!(
                try_exclusive_flock_detailed(&lock),
                LockAttempt::Acquired(_)
            ),
            "non-degeneracy: if this path could never be acquired, the assertion                  above would pass for the wrong reason"
        );
    }

    // what this catches: the third state must be reachable and distinct — a path
    // that cannot be opened is NOT evidence a build was protected.
    #[test]
    fn an_unopenable_lock_path_is_unavailable_not_contention() {
        let tmp = tempfile::tempdir().expect("tempdir");
        // A lock file whose PARENT does not exist cannot be created or opened.
        let impossible = tmp.path().join("no-such-dir").join(".cargo-lock");
        match try_exclusive_flock_detailed(&impossible) {
            LockAttempt::Unavailable(_) => {}
            other => panic!(
                "an unopenable path must be Unavailable, never mistaken for a                      live build holding the lock, got {other:?}"
            ),
        }
    }

    // what this catches: a LIVE regression that #3907 shipped to canary, found by
    // S6 via @Astra. `CargoTargetPool::tier_name` returned a hardcoded
    // "disk-cargo-target" for every instance, and `PressureBroker::register`
    // dedups by name (broker.rs: `pools.retain(|p| p.tier_name() != name)`).
    // So when #3907 began registering a SECOND cache in a loop, the second
    // registration REPLACED the first and the shared cargo-target became
    // ungoverned — while two boot log lines claimed both were registered.
    //
    // Two governed caches must be two POOLS, and the only thing that makes them
    // two is a distinct name.
    #[test]
    fn two_build_caches_are_two_pools_and_not_one_replacing_the_other() {
        let tmp = tempfile::tempdir().expect("tempdir");
        #[cfg(windows)]
        let names = {
            use std::os::windows::ffi::OsStringExt;
            [std::ffi::OsString::from_wide(&[0xd800]), std::ffi::OsString::from_wide(&[0xd801])]
        };
        #[cfg(not(windows))]
        let names = {
            use std::os::unix::ffi::OsStringExt;
            [std::ffi::OsString::from_vec(vec![0xfe]), std::ffi::OsString::from_vec(vec![0xff])]
        };
        assert_eq!(names[0].to_string_lossy(), names[1].to_string_lossy(), "display cannot identify these distinct native roots");
        let shared = TrackedDir::new("cargo-target", tmp.path().join(&names[0]));
        let worktree = TrackedDir::new("cargo-target", tmp.path().join(&names[1]));

        let a = fixture_pool(shared, DEFAULT_CARGO_TARGET_BUDGET_BYTES);
        let b = fixture_pool(worktree, DEFAULT_CARGO_TARGET_BUDGET_BYTES);

        assert!(a.tier_name().starts_with("disk-cargo-target:"));
        assert!(b.tier_name().starts_with("disk-cargo-target:"));
        assert_ne!(
            a.tier_name(),
            b.tier_name(),
            "non-degeneracy: identical names are exactly what let the broker's                  dedup silently drop one of the two caches"
        );

        // AND THROUGH THE BROKER, because the getters are not where this broke.
        // @Astra: "getter inequality alone misses the actual integration
        // boundary that failed." The failure was `register`'s dedup-by-name
        // (broker.rs: `pools.retain(|p| p.tier_name() != name)`), so the
        // assertion has to survive that call — a future change that makes the
        // names equal again would pass the getter check above and still lose a
        // cache here.
        let broker = crate::paging::broker::PressureBroker::new(Default::default());
        broker.register(Arc::new(a) as Arc<dyn ResourcePool>);
        broker.register(Arc::new(b) as Arc<dyn ResourcePool>);
        let names: Vec<String> = broker
            .snapshot()
            .pools
            .into_iter()
            .map(|p| p.name)
            .filter(|n| n.starts_with("disk-cargo-target"))
            .collect();
        assert_eq!(
            names.len(),
            2,
            "BOTH build caches must survive registration — one row here is the                  regression: the shared cache silently lost its eviction owner while                  two boot log lines claimed otherwise. Got: {names:?}"
        );
    }

    // what this catches: @Astra and @IntelMac both rejected the first version of
    // this, which warned after 8 evictions in a process lifetime. A COUNT cannot
    // tell 36-in-a-day (the measured defect) from 36-over-a-week (housekeeping),
    // and IntelMac's own box would have been named loudly for the latter. The
    // signal is whether the cache survived long enough to be REUSED, which is a
    // gap, not a tally.
    #[test]
    fn thrash_is_a_cadence_not_a_count() {
        // A build cannot reuse a cache emptied minutes ago.
        assert!(gap_is_thrash(60_000));
        assert!(gap_is_thrash(THRASH_GAP_MS - 1));
        // Hours apart is housekeeping, however many times it has happened.
        assert!(!gap_is_thrash(THRASH_GAP_MS));
        assert!(!gap_is_thrash(6 * 60 * 60 * 1000));

        // One fast gap is a coincidence; a run is a pattern.
        assert!(!thrash_warning_due(1));
        assert!(thrash_warning_due(CONSECUTIVE_FAST_GAPS_BEFORE_WARNING));
        // Then periodic, never per-eviction noise.
        let n = CONSECUTIVE_FAST_GAPS_BEFORE_WARNING;
        assert!(!thrash_warning_due(n + 1));
        assert!(thrash_warning_due(n * 2));
    }

    // what this catches: regression for #3906 — the cargo-target budget was a
    // FLAT 50 GiB, which sits BELOW this workspace's debug tree on a
    // workstation. Measured consequence on a 1.9 TB box: 36 evictions and
    // 356 GB of compilation destroyed in one day, median 13.5 minutes apart,
    // at a 13.5-minute median, and three agents blamed the toolchain. (The
    // frequency is what was measured; whether any build was denied a warm
    // cache was never observed — see note_eviction.)
    //
    // The derivation must (a) actually raise the budget on a big volume —
    // a fix that returned the floor everywhere would pass a weaker test while
    // changing nothing — and (b) never LOWER it on a small one, or this
    // regresses the machines it was already correct for.
    #[test]
    fn the_cargo_target_budget_is_derived_from_the_volume_and_never_below_the_floor() {
        const GIB: u64 = 1024 * 1024 * 1024;
        let floor = DEFAULT_CARGO_TARGET_BUDGET_BYTES;

        // Small machines: unchanged. This is what makes the change safe to
        // ship everywhere rather than only where it was measured.
        assert_eq!(cargo_target_budget_bytes(256 * GIB), floor);
        // @IntelMac's box: a "500 GB" container is 465 GiB, so 10% = 46.5 GiB
        // and the FLOOR wins — their budget is unchanged by this PR. Asserted
        // in the units drives are actually SOLD in, because my first table
        // said "500 GiB, the crossover", which is true and misleading: nobody
        // owns a 500 GiB drive.
        const GB: u64 = 1_000_000_000;
        assert_eq!(cargo_target_budget_bytes(500 * GB), floor);
        // The crossover in advertised units is ~537 GB; below it, floor.
        assert_eq!(cargo_target_budget_bytes(530 * GB), floor);
        // Above it the derivation takes over.
        assert!(cargo_target_budget_bytes(600 * GB) > floor);
        // A volume smaller than the floor still yields the floor, never 0 and
        // never the volume — a budget of nothing is a permanent `cargo clean`.
        assert_eq!(cargo_target_budget_bytes(8 * GIB), floor);
        assert_eq!(cargo_target_budget_bytes(0), floor);

        // The measured machine: 1.9 TB. This is the assertion that would have
        // failed before the fix, and it is the only one that matters to the
        // defect.
        let bigmama = cargo_target_budget_bytes(1900 * GIB);
        assert_eq!(bigmama, 190 * GIB);
        // And in advertised units, the 2 TB class this PR was measured on.
        assert!(cargo_target_budget_bytes(2000 * GB) > 180 * GIB);
        assert!(
            bigmama > floor,
            "non-degeneracy: on the volume that produced #3906 the derived                  budget MUST exceed the old constant, or nothing changed"
        );

        // Monotonic in the volume — a bigger drive never gets a smaller cache.
        let mut prev = 0;
        for tb in [0u64, 256, 512, 1000, 1900, 4000, 8000] {
            let b = cargo_target_budget_bytes(tb * GIB);
            assert!(b >= prev, "budget must not shrink as the volume grows");
            prev = b;
        }
    }

    // what this catches: the capacity derivation (#287-style) — 10% of
    // the volume floored at 32 GiB, saturating to 0 on a volume smaller
    // than the reserve (the tier degrades, never underflows).
    #[test]
    fn capacity_is_volume_minus_derived_reserve() {
        const GIB: u64 = 1024 * 1024 * 1024;
        // 4 TB NVMe: reserve = 10% = 400 GB-ish (> 32 GiB floor).
        assert_eq!(serving_tier_reserve_bytes(4000 * GIB), 400 * GIB);
        assert_eq!(serving_tier_capacity_bytes(4000 * GIB), 3600 * GIB);
        // 100 GiB SSD: 10% = 10 GiB < floor ⇒ reserve is 32 GiB.
        assert_eq!(serving_tier_reserve_bytes(100 * GIB), 32 * GIB);
        assert_eq!(serving_tier_capacity_bytes(100 * GIB), 68 * GIB);
        // Tiny volume: capacity saturates to 0, never wraps.
        assert_eq!(serving_tier_capacity_bytes(GIB), 0);
    }

    // what this catches: ActiveArtifactSet prefix containment BOTH
    // directions — marking a model dir protects files inside it, and
    // marking a file protects its parent dir from wholesale migration.
    #[tokio::test]
    async fn active_set_protects_prefix_both_directions() {
        crate::system_resources::disk_reporters::prepare_writer_fixture().await;
        let set = ActiveArtifactSet::default();
        let root = tempfile::tempdir().unwrap();
        // Unix temporary roots may themselves be aliases (/var on macOS).
        // This fixture tests Windows prefix spelling, not alias resolution.
        #[cfg(windows)]
        let root_path = root.path().to_path_buf();
        #[cfg(not(windows))]
        let root_path = root.path().canonicalize().unwrap();
        let logical = root_path.join("served-model");
        std::fs::create_dir(&logical).unwrap();
        let physical = logical.canonicalize().unwrap();
        set.register(logical.clone());
        assert!(set.protects(&root_path.join("other-model")), "unresolved pins fail closed without IO");
        set.register_resolved(logical.clone(), ActiveArtifactSet::resolve(&logical).await);
        assert!(set.protects(&physical), "normal and canonical Windows spellings share custody");
        assert!(
            set.protects(&physical.join("model.gguf")),
            "candidate under active dir is protected"
        );
        assert!(
            set.protects(&root_path.canonicalize().unwrap()),
            "parent of active is protected"
        );
        assert!(!set.protects(&root_path.join("other-model")));
        std::fs::remove_dir(&logical).unwrap();
        assert!(set.protects(&physical), "missing leaf retains registered custody");
        set.release(&logical);
        assert!(!set.protects(&physical));
        let unresolved = PathBuf::from("relative-model");
        set.register(unresolved.clone());
        assert!(set.protects(&physical), "relative custody cannot grant reclamation permission");
        set.release(&unresolved);
        #[cfg(windows)]
        {
            let unc = PathBuf::from(r"\\server\share\model");
            set.register_resolved(unc.clone(), Some(unc.clone()));
            assert!(set.protects(Path::new(r"\\?\UNC\server\share\model\weights")));
            assert!(!set.protects(Path::new(r"\\?\UNC\server\different\model")));
            assert!(!set.protects(Path::new(r"\\?\UNC\server\share\model-other")));
            set.release(&unc);
            assert!(!set.protects(Path::new(r"\\?\UNC\server\share\model")));
        }
    }
