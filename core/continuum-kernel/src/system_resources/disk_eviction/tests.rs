#![cfg(test)]
    // Broker fixtures own their volume input as well as their temp files. The
    // constructor budget is only a fallback: a concurrently published monitor
    // sample must not turn a custody test into a test of the runner's free disk.
    fn critical_fixture_volume(path: &Path) -> Option<crate::system_resources::drives::DriveInfo> {
        use crate::system_resources::drives::{DriveInfo, DriveRole};
        Some(DriveInfo { mount: path.into(), total_bytes: 100_000, available_bytes: 0, role: DriveRole::System })
    }

    #[test]
    fn cache_pressure_follows_its_own_mount_instead_of_the_home_mount() {
        use crate::system_resources::drives::{DriveInfo, DriveRole, drive_holding};
        let home = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let drives = [
            DriveInfo { mount: home.path().into(), total_bytes: 10_000, available_bytes: 100, role: DriveRole::System },
            DriveInfo { mount: other.path().into(), total_bytes: 100_000, available_bytes: 90_000, role: DriveRole::System },
        ];
        let legacy = CargoTargetPool::new(TrackedDir::new("cargo-target", home.path().join("cache")), 1_000);
        let relocated = CargoTargetPool::new(TrackedDir::new("cargo-target", other.path().join("cache")), 1_000);
        assert_eq!(legacy.pressure_for_volume(drive_holding(&drives, legacy.tracked.path())), 0.99);
        assert_eq!(relocated.pressure_for_volume(drive_holding(&drives, relocated.tracked.path())), 0.1);
        assert_eq!(relocated.budget_for_volume(drive_holding(&drives, relocated.tracked.path())), cargo_target_budget_bytes(100_000));
        assert_eq!(relocated.budget_for_volume(None), 1_000, "unmeasured volume retains the explicit fallback budget");
    }
    use super::*;

    /// A build-cache pool whose every host input is the fixture's: no measured volume
    /// (the explicit budget governs), an idle node, and its own empty serving custody.
    /// `CargoTargetPool::new` wires the live ones — the disk monitor's published sample
    /// of the runner's real volumes, the running core's citizens, and any pin another
    /// test published — so a fixture built on it tests the machine, not the pool (card
    /// c4d84be3: two of these failed on the M5 and passed on an idle box). A test that
    /// is ABOUT one of those inputs sets that field after this.
    fn fixture_pool(tracked: Arc<TrackedDir>, budget_bytes: u64) -> CargoTargetPool {
        let mut pool = CargoTargetPool::new(tracked, budget_bytes);
        pool.volume_sample = |_| None;
        pool.citizen_quiescence = |_, _| Ok(());
        pool.active_artifacts = Arc::new(ActiveArtifactSet::default());
        pool
    }

    /// Run an eviction that is expected to cut, waiting out a cargo lock held only by a
    /// spawn's fork window. On Linux `posix_spawn` is clone + exec, so every process this
    /// test binary starts (any test, any thread) holds a copy of our fd table until it
    /// execs — and with it any `.cargo-lock` description open at that instant, which
    /// keeps the flock alive after this pool dropped it. The next eviction then reads
    /// `BuildHoldsRootLock` on a tempdir nothing else owns (CI on #5200, run 39352761897:
    /// `remaining ladder: decline=1`, passed on rerun). macOS spawns atomically, so it
    /// never showed there. Only a lock decline that freed nothing is retried; any other
    /// outcome — a cut, or a different decline — returns at once and is asserted.
    fn past_spawn_windows<T>(pool: &CargoTargetPool, freed: impl Fn(&T) -> u64, mut op: impl FnMut() -> T) -> T {
        use std::sync::atomic::Ordering::Relaxed;
        let lock_held = [DeclinedEviction::BuildHoldsRootLock, DeclinedEviction::BuildHoldsDebugLock, DeclinedEviction::BuildHoldsReleaseLock]
            .map(|d| d.discriminant());
        // derived-or-floor: a liveness bound on a child's clone→exec window (microseconds
        // to a few ms), never the assertion: 200 × 5 ms = 1 s before the real outcome stands.
        for _ in 0..200 {
            pool.last_decline.store(u8::MAX, Relaxed);
            let out = op();
            if freed(&out) > 0 || !lock_held.contains(&pool.last_decline.load(Relaxed)) {
                return out;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        op()
    }

    // what this catches (2026-09-18, the 5090 at 95 % with both cargo caches "within
    // budget"): a cache pool on a critical volume reads AT LEAST the volume's
    // pressure, so the broker's act line is crossed by the volume filling — not only
    // by the pool outgrowing a budget cut from that volume's total. Under budget on a
    // calm volume it still reads its own number, so nothing evicts for no reason.
    #[test]
    fn a_cache_on_a_hot_volume_reads_the_volumes_pressure() {
        assert_eq!(cache_pool_pressure(0.34, 0.95), 0.95, "the volume brings the broker");
        assert_eq!(cache_pool_pressure(0.34, 0.40), 0.40);
        assert_eq!(cache_pool_pressure(0.90, 0.40), 0.90, "over budget still governs");
        assert_eq!(cache_pool_pressure(0.34, 0.0), 0.34, "no disk reading = own budget alone");
    }

    /// What the kernel's process table sees executing under `dir` right now (diagnostics).
    fn executing_now(dir: &Path) -> Option<(u32, String)> {
        crate::system_resources::process_tree::ProcessTable::read()
            .executing_under(dir)
            .map(|(process, exe)| (process.identity.pid, exe.display().to_string()))
    }

    // what this catches (card b958bed4, the IntelMac 2026-11-04: ~1.5 cores of the core): a
    // cargo pool with nothing left to reclaim read the whole process table on every broker
    // pass. The first pass that runs the ladder to the bottom reads it once; repeated passes at
    // the same measurement read nothing; the scanner's next observation reopens one pass.
    #[test]
    fn repeated_passes_with_nothing_reclaimable_read_the_process_table_once_per_measurement() {
        use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
        static READS: AtomicUsize = AtomicUsize::new(0);
        let tmp = tempfile::tempdir().expect("tempdir");
        let tracked = seeded_target(tmp.path());
        let mut pool = fixture_pool(tracked.clone(), 1);
        pool.process_table = || {
            READS.fetch_add(1, Relaxed);
            crate::system_resources::process_tree::ProcessTable::of(Vec::new())
        };
        // Everything reclaimable goes; release keeps its outputs, so the ladder ends short.
        assert!(past_spawn_windows(&pool, |f| *f, || pool.evict_at_least(u64::MAX)) > 0);
        tracked.set_bytes(dir_size_bytes(tmp.path()));
        let before = READS.load(Relaxed);
        assert_eq!(past_spawn_windows(&pool, |f| *f, || pool.evict_at_least(u64::MAX)), 0, "nothing reclaimable is left");
        assert_eq!(READS.load(Relaxed), before + 1, "the pass reads the table once for both profiles");
        for _ in 0..8 {
            assert_eq!(pool.evict_at_least(u64::MAX), 0);
        }
        assert_eq!(READS.load(Relaxed), before + 1, "repeated passes at one measurement read nothing");
        assert_eq!(pool.last_decline.load(Relaxed), DeclinedEviction::AwaitingMeasurement.discriminant());
        tracked.set_bytes(dir_size_bytes(tmp.path()));
        assert_eq!(past_spawn_windows(&pool, |f| *f, || pool.evict_at_least(u64::MAX)), 0);
        assert_eq!(READS.load(Relaxed), before + 2, "a new measurement reopens one pass");
    }

    // what this catches (Codex on card b958bed4): a decline whose cause ends on its own, latched
    // to the measurement, so reclaim waits for the next walk after the process is gone. While the
    // image under debug/ runs, passes wait on that exact process without reading the table; when
    // it exits the very next pass reclaims, with the measurement unchanged.
    #[test]
    fn a_profile_freed_by_its_process_exiting_is_reclaimed_without_a_new_measurement() {
        use crate::system_resources::process_tree::{ProcessRow, ProcessTable};
        use crate::system_resources::process_tree::ProcessIdentity;
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed};
        static READS: AtomicUsize = AtomicUsize::new(0);
        static RUNNING: AtomicBool = AtomicBool::new(true);
        static IMAGE: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
        let tmp = tempfile::tempdir().expect("tempdir");
        let tracked = seeded_target(tmp.path());
        IMAGE.set(tmp.path().join("debug/lib.rlib").canonicalize().expect("image")).expect("set once");
        let mut pool = fixture_pool(tracked.clone(), 1);
        pool.process_table = || {
            READS.fetch_add(1, Relaxed);
            let rows = RUNNING.load(Relaxed).then(|| ProcessRow {
                identity: ProcessIdentity { pid: 4242, start_time: 7 },
                name: "image".into(),
                exe: IMAGE.get().cloned(),
            });
            ProcessTable::of(rows.into_iter().collect())
        };
        pool.process_alive = |identity| RUNNING.load(Relaxed) && identity == ProcessIdentity { pid: 4242, start_time: 7 };
        let observation = tracked.observation();
        past_spawn_windows(&pool, |f| *f, || pool.evict_at_least(u64::MAX));
        assert!(tmp.path().join("debug/lib.rlib").exists(), "the running image's profile stands down");
        let reads = READS.load(Relaxed);
        for _ in 0..8 {
            assert_eq!(pool.evict_at_least(u64::MAX), 0);
        }
        assert_eq!(READS.load(Relaxed), reads, "while it runs, passes ask that one process, not the table");
        assert_eq!(pool.last_decline.load(Relaxed), DeclinedEviction::ProfileInUse { pid: 0, exe: String::new() }.discriminant());
        RUNNING.store(false, Relaxed);
        let freed = past_spawn_windows(&pool, |f| *f, || pool.evict_at_least(u64::MAX));
        assert!(freed > 0, "its exit reopens the pool at once: decline={} settled_at={} observation={}",
            pool.last_decline.load(Relaxed), pool.settled_at.load(Relaxed), tracked.observation());
        assert!(!tmp.path().join("debug/lib.rlib").exists());
        assert_eq!(tracked.observation(), observation, "with no new measurement");
    }

    fn seeded_target(tmp: &Path) -> Arc<TrackedDir> {
        std::fs::create_dir_all(tmp.join("debug/incremental")).expect("mkdir");
        std::fs::create_dir_all(tmp.join("tests")).expect("mkdir");
        std::fs::create_dir_all(tmp.join("release")).expect("mkdir");
        std::fs::write(tmp.join("debug/incremental/a.o"), vec![0u8; 4000]).expect("write");
        std::fs::write(tmp.join("debug/lib.rlib"), vec![0u8; 3000]).expect("write");
        std::fs::write(tmp.join("tests/t.bin"), vec![0u8; 2000]).expect("write");
        std::fs::write(tmp.join("release/keep.bin"), vec![0u8; 1000]).expect("write");
        let tracked = TrackedDir::new("cargo-target", tmp.canonicalize().unwrap());
        // Seed the cached size the way the scanner would.
        tracked.set_bytes(dir_size_bytes(tmp));
        tracked
    }

    // what this catches: the eviction ladder frees cheapest-regret first
    // (incremental before the whole debug tree), stops once `want` is met,
    // preserves release binaries, and decrements the shared TrackedDir so the
    // broker doesn't re-fire against space already freed.
    // what this catches (the M5, 2026-10-05): cargo has finished and is RUNNING what it
    // built, no lock held, and the debug rung deleted the executing test binary three
    // runs in a row. With a process executing an image under debug/, the earlier rungs
    // still cut and the debug rung stands down with the pid; once it exits, the rung cuts.
    #[cfg(unix)]
    #[test]
    fn profile_outputs_stand_down_while_a_process_executes_under_them() {
        for profile in ["debug", "release"] {
        let tmp = tempfile::tempdir().expect("tempdir");
        let tracked = seeded_target(tmp.path());
        let deps = tmp.path().join(profile).join(if profile == "release" { "build/unit/out/build" } else { "deps" });
        std::fs::create_dir_all(&deps).expect("mkdir");
        let sleeper = deps.join("continuum_core-test");
        // The sleeper is a copy of /bin/sh blocked on its builtin `read`: a shell is one
        // program whatever name it runs under, unlike a multi-call coreutils /bin/sleep (Ubuntu
        // 26.04, CI from 2026-10-23), which picks its applet by name and exited at once here.
        std::fs::copy("/bin/sh", &sleeper).expect("a sleeper to run from debug/deps");
        if profile == "release" {
            use continuum_cli_lifecycle::cargo_cache_custody as custody;
            let out = deps.parent().unwrap();
            std::fs::create_dir_all(out.join("lib")).unwrap();
            let generation = custody::publish_static_inputs(out).unwrap();
            std::fs::write(out.parent().unwrap().join("output"), format!("{}{generation}\n", custody::METADATA)).unwrap();
        }
        // The same fork window, on the image: `fs::copy` held `sleeper` open for write, and a
        // spawn on another thread that cloned our fd table in that instant keeps the write
        // fd until it execs, so exec of the image reads ETXTBSY (CI #5208, run
        // 39362437168). Only that error is retried, within the same 1 s liveness bound.
        let mut child = (0..200)
            .find_map(|_| match std::process::Command::new(&sleeper)
                .args(["-c", "read line"])
                .stdin(std::process::Stdio::piped())
                .spawn()
            {
                Err(e) if e.kind() == std::io::ErrorKind::ExecutableFileBusy => {
                    std::thread::sleep(std::time::Duration::from_millis(5));
                    None
                }
                other => Some(other),
            })
            .expect("the image stayed busy past the liveness bound")
            .expect("spawn the sleeper");
        let pool = fixture_pool(tracked.clone(), 1);
        // Ask for more than the first two rungs hold, so the ladder reaches debug/.
        let freed = past_spawn_windows(&pool, |f| *f, || pool.evict_at_least(100_000));
        assert!(!tmp.path().join("debug/incremental").exists(), "the first rung still cuts");
        assert!(!tmp.path().join("tests").exists(), "the second rung still cuts");
        assert!(
            sleeper.exists(),
            "the executing image survives (child still running: {:?}; executing under debug/: {:?})",
            child.try_wait().map(|status| status.is_none()),
            executing_now(&tmp.path().join(profile)),
        );
        assert_eq!(tmp.path().join("debug/lib.rlib").exists(), profile == "debug", "only the active profile stands down");
        assert_eq!(freed, if profile == "debug" { 6000 } else { 9000 }, "only safe rungs");
        child.kill().expect("stop the sleeper");
        let exited = child.wait();
        // The helper clears the decline tag per attempt, so the tag below is the SECOND call's alone.
        let freed_after = past_spawn_windows(&pool, |f| *f, || pool.evict_at_least(100_000));
        // The failure names its cause (Ubuntu CI run 38829445559 failed here with no
        // reason, card e37a54db): what the wait reported, what the rung freed, which typed
        // decline it last recorded, and what the process scan sees under debug/ now.
        assert!(
            !sleeper.exists() && freed_after > 0,
            "once nothing executes there, the rung cuts: wait={exited:?} freed_after={freed_after} \
             sleeper_exists={} last_decline_tag={} executing_now={:?}",
            sleeper.exists(),
            pool.last_decline.load(std::sync::atomic::Ordering::Relaxed),
            executing_now(&tmp.path().join(profile)),
        );
      }
    }

    #[test]
    fn ladder_frees_in_order_and_preserves_release_outputs() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let tracked = seeded_target(tmp.path());
        let usage_before = tracked.bytes();
        // The fixture owns its serving custody: another serving test may publish an
        // unresolved global pin, which correctly refuses every production candidate.
        let pool = fixture_pool(tracked.clone(), 1);

        // Small want: the incremental rung (4000 B) alone satisfies it.
        let freed = past_spawn_windows(&pool, |f| *f, || pool.evict_at_least(1000));
        assert_eq!(freed, 4000, "incremental rung is the first and only cut");
        assert!(!tmp.path().join("debug/incremental").exists());
        assert!(tmp.path().join("tests").exists(), "later rung untouched");
        assert!(tmp.path().join("release/keep.bin").exists());
        assert_eq!(
            tracked.bytes(),
            usage_before - 4000,
            "shared measurement reflects the delete immediately"
        );

        // Large want: remaining rungs go (tests 2000 + debug 3000);
        // release survives regardless.
        let freed = past_spawn_windows(&pool, |f| *f, || pool.evict_at_least(u64::MAX));
        assert_eq!(freed, 5000, "remaining ladder: decline={} active_pin={}",
            pool.last_decline.load(std::sync::atomic::Ordering::Relaxed),
            pool.active_artifacts.protects(tmp.path()));
        assert!(!tmp.path().join("tests").exists());
        assert!(tmp.path().join("release/keep.bin").exists());

        // `debug/` SURVIVES NOW, AND THAT IS THE POINT (@Astra, #3910 review).
        // This assertion used to be `!debug.exists()`. Deleting the directory also
        // unlinked `debug/.cargo-lock` — the file whose lock is this eviction's
        // claim to be running at all. On Unix that unlink SUCCEEDS while we hold
        // the descriptor: our fd keeps the old inode alive, the path is free, and
        // another cargo can create and lock a replacement while we are still
        // deleting underneath it. The lock stops being an ownership boundary at the
        // moment we delete the thing being locked.
        //
        // So the contract is: everything under `debug/` is reclaimed, and the lock
        // file and its parent stand. The bytes we decline to free are a zero-length
        // lock file.
        let debug = tmp.path().join("debug");
        assert!(debug.exists(), "the lock's parent must survive the rung");
        assert!(
            debug.join(".cargo-lock").exists(),
            "the lock we hold must still be at its PATH, not merely alive on an              unlinked inode — an unlinked lock guards nothing against the next cargo"
        );
        let survivors: Vec<String> = std::fs::read_dir(&debug)
            .expect("read debug")
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            survivors,
            vec![".cargo-lock".to_string()],
            "ONLY the lock survives — anything else means the rung under-reclaimed"
        );
    }

    // what this catches: safety invariant 1 — a held cargo lock (a build
    // in flight) makes eviction a no-op. Deleting deps out from under a
    // running cargo corrupts the build AND can race half-written files.
    #[test]
    fn held_cargo_lock_blocks_eviction_entirely() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let tracked = seeded_target(tmp.path());
        let mut pool = fixture_pool(tracked, 1);

        let build_holds_lock = continuum_host::durable::lock_path(&tmp.path().join(".cargo-lock"), false)
            .expect("test takes the lock first");
        assert_eq!(pool.evict_at_least(u64::MAX), 0, "locked ⇒ untouched");
        assert!(tmp.path().join("debug/incremental").exists());
        let peer = uuid::Uuid::new_v4();
        let in_flight = pool.tracked.eviction_custody();
        assert!(pool.tracked.bind_citizen(peer).is_err(),
            "discovery cannot change ownership during a generic eviction");
        assert_eq!(*in_flight, None);
        drop(in_flight);
        pool.tracked.bind_citizen(peer).unwrap();
        let hands = PersonaHandsLease::try_take(peer).unwrap();
        assert_eq!(pool.evict_at_least(u64::MAX), 0, "resident hands keep all derived rungs");
        assert_eq!(pool.last_decline.load(std::sync::atomic::Ordering::Relaxed),
            DeclinedEviction::CitizenBusy(String::new()).discriminant());
        assert!(tmp.path().join("debug/incremental").exists());
        drop(hands);
        drop(build_holds_lock);
        // Drive the existing owner observation, not the host running Cargo tests.
        pool.citizen_quiescence = |_, _| Err(crate::system_resources::citizen_workspace_pool::NotQuiescent::TurnInFlight(1));
        assert_eq!(pool.evict_at_least(u64::MAX), 0);
        assert!(tmp.path().join("debug/incremental").exists());
        pool.citizen_quiescence = |_, _| Ok(());
        assert!(past_spawn_windows(&pool, |f| *f, || pool.evict_at_least(1)) > 0, "a later idle broker attempt reaches the existing derived rung");
        assert!(tmp.path().join(".cargo-lock").exists(), "lock inode retained");
    }

    // what this catches: the real broker must reach release incremental after
    // compiler custody ends, even when the pool uses a measured volume instead
    // of its fallback budget (3d4a push CI previously depended on global timing).
    #[tokio::test]
    async fn broker_reclaims_release_incremental_only_after_profile_custody() {
        super::super::disk_reporters::prepare_writer_fixture().await;
        use crate::paging::{BrokerConfig, PressureBroker};
        use continuum_cli_lifecycle::cargo_cache_custody as custody;
        let tmp = tempfile::tempdir().unwrap();
        let release = tmp.path().join("release");
        std::fs::create_dir_all(release.join("incremental/unit")).unwrap();
        std::fs::create_dir_all(release.join("deps")).unwrap();
        std::fs::create_dir_all(release.join("build")).unwrap();
        std::fs::create_dir_all(release.join(".fingerprint")).unwrap();
        std::fs::write(release.join("incremental/unit/derived.o"), vec![0u8; 4096]).unwrap();
        let outputs = ["continuum", "deps/library.rlib", "build/output", ".fingerprint/unit"];
        for output in outputs { std::fs::write(release.join(output), b"preserve installed inputs").unwrap(); }
        let certified = release.join("build/producer/out");
        let unknown = release.join("build/foreign/out");
        for out in [&certified, &unknown] {
            for child in custody::CHILDREN {
                std::fs::create_dir_all(out.join(child)).unwrap();
                std::fs::write(out.join(child).join("input"), b"producer input").unwrap();
            }
        }
        let generation = custody::publish_static_inputs(&certified).unwrap();
        let cargo_output = certified.parent().unwrap().join("output");
        let declaration = format!("{}{generation}\n", custody::METADATA);
        assert!(!custody::declared_static_inputs(&certified), "receipt alone is not current producer provenance");
        std::fs::write(&cargo_output, &declaration).unwrap();
        assert!(custody::declared_static_inputs(&certified));
        std::fs::write(certified.join("build/input"), b"changed input!").unwrap();
        assert!(!custody::declared_static_inputs(&certified), "same-size changed contents invalidate generation custody");
        std::fs::write(certified.join("build/input"), b"producer input").unwrap();
        std::fs::write(certified.join("build/new-runtime-asset"), b"changed by legacy producer").unwrap();
        assert!(!custody::declared_static_inputs(&certified), "failed legacy build cannot leave stale output custody");
        std::fs::remove_file(certified.join("build/new-runtime-asset")).unwrap();
        std::fs::write(&cargo_output, b"cargo:rerun-if-changed=legacy.rs\n").unwrap();
        assert!(!custody::declared_static_inputs(&certified), "successful legacy producer supersedes declaration");
        std::fs::write(&cargo_output, &declaration).unwrap();
        // A malformed/partial receipt never grants inferred runtime ownership.
        std::fs::write(unknown.join(custody::RECEIPT), b"cargo-static-link-inputs-v1\n").unwrap();
        let closure_bytes = dir_size_bytes(&certified) + std::fs::metadata(&cargo_output).unwrap().len();
        let tracked = TrackedDir::new("cargo-target", tmp.path().canonicalize().unwrap());
        tracked.set_bytes(dir_size_bytes(tmp.path()));
        let mut pool = fixture_pool(tracked.clone(), 1);
        pool.volume_sample = critical_fixture_volume;
        let active = Arc::new(ActiveArtifactSet::default());
        pool.active_artifacts = active.clone();
        assert_eq!(pool.pressure(), 1.0, "the fixture supplies measured critical-volume pressure");
        assert!(pool.capacity_bytes() > tracked.bytes(), "pressure comes from the volume, not the fallback budget");
        let pool = Arc::new(pool);
        let broker = PressureBroker::new(BrokerConfig::default());
        broker.register(pool.clone());
        let lock_path = release.join(".cargo-lock");
        let build_lock = continuum_host::durable::lock_path(&lock_path, false).unwrap();
        assert_eq!(broker.relieve().bytes_freed, 0, "an active release compiler owns its profile");
        assert_eq!(pool.last_decline.load(std::sync::atomic::Ordering::Relaxed), DeclinedEviction::BuildHoldsReleaseLock.discriminant(), "the broker reached the real custody guard");
        assert!(release.join("incremental/unit/derived.o").exists());
        drop(build_lock);
        #[cfg(unix)]
        let inode = { use std::os::unix::fs::MetadataExt; std::fs::metadata(&lock_path).unwrap().ino() };
        let before = tracked.bytes();
        let raw_pin = certified.join("build/input");
        let physical_pin = ActiveArtifactSet::resolve(&raw_pin).await;
        assert!(physical_pin.is_some(), "bounded owner resolves the real Windows short alias");
        active.register_resolved(raw_pin, physical_pin);
        assert!(active.protects(&tracked.path().join("release")),
            "registered pin must protect the selected physical release: pin={:?}, pin_components={:?}, physical_pin={:?}, candidate={:?}, candidate_components={:?}",
            certified.join("build/input"), certified.join("build/input").components().collect::<Vec<_>>(),
            certified.join("build/input").canonicalize(), tracked.path().join("release"),
            tracked.path().join("release").components().collect::<Vec<_>>());
        assert_eq!(past_spawn_windows(&pool, |f| *f, || broker.relieve().bytes_freed), 4096,
            "the existing broker reaches release-only incremental pressure: decline={}, pressure={}, incremental_exists={}, incremental_pinned={}, release_pinned={}",
            pool.last_decline.load(std::sync::atomic::Ordering::Relaxed), pool.pressure(),
            release.join("incremental").exists(), active.protects(&tracked.path().join("release/incremental")),
            active.protects(&tracked.path().join("release")));
        assert_eq!(tracked.bytes(), before - 4096);
        assert!(!release.join("incremental").exists());
        assert!(lock_path.exists());
        #[cfg(unix)]
        { use std::os::unix::fs::MetadataExt; assert_eq!(std::fs::metadata(&lock_path).unwrap().ino(), inode); }
        for output in outputs { assert_eq!(std::fs::read(release.join(output)).unwrap(), b"preserve installed inputs"); }
        active.release(&certified.join("build/input"));
        let remaining = tracked.bytes();
        assert_eq!(past_spawn_windows(&pool, |f| *f, || broker.relieve().bytes_freed), closure_bytes,
            "release-only intermediate pressure has a real eviction owner: decline={}, observation={}, settled_at={}, active_pin={}",
            pool.last_decline.load(std::sync::atomic::Ordering::Relaxed), tracked.observation(),
            pool.settled_at.load(std::sync::atomic::Ordering::Relaxed), active.protects(&release));
        assert_eq!(tracked.bytes(), remaining - closure_bytes);
        for child in custody::CHILDREN { assert!(!certified.join(child).exists()); assert!(unknown.join(child).join("input").exists()); }
        assert!(!cargo_output.exists(), "withdraw Cargo's captured output so missing inputs regenerate");
        assert!(!certified.join(custody::RECEIPT).exists());
        for output in outputs { assert_eq!(std::fs::read(release.join(output)).unwrap(), b"preserve installed inputs"); }
        assert_eq!(broker.relieve().bytes_freed, 0, "unknown outputs cannot be certified by an unrelated producer");
        custody::invalidate(&unknown).unwrap();
        let generation = custody::publish_static_inputs(&unknown).unwrap();
        std::fs::write(unknown.parent().unwrap().join("output"), format!("{}{generation}\n", custody::METADATA)).unwrap();
        assert_eq!(broker.relieve().bytes_freed, 0, "new custody waits for the existing scanner; pressure ticks do not repeat tree hashing");
        assert_eq!(pool.last_decline.load(std::sync::atomic::Ordering::Relaxed), DeclinedEviction::AwaitingMeasurement.discriminant());
        assert!(unknown.join("build/input").exists());
        tracked.set_bytes(dir_size_bytes(tmp.path()));
        let declared_bytes = dir_size_bytes(&unknown) + std::fs::metadata(unknown.parent().unwrap().join("output")).unwrap().len();
        assert_eq!(past_spawn_windows(&pool, |f| *f, || broker.relieve().bytes_freed), declared_bytes, "only that producer's explicit declaration releases its own inputs");
        assert!(!custody::declared_static_inputs(&unknown));
        assert_eq!(std::fs::read(release.join("continuum")).unwrap(), b"preserve installed inputs", "a completed warm-build receipt can still name the root binary");
        assert_eq!(pool.last_decline.load(std::sync::atomic::Ordering::Relaxed), DeclinedEviction::ReleaseInputsRetained { bytes: 1 }.discriminant());
        #[cfg(unix)]
        { use std::os::unix::fs::MetadataExt; assert_eq!(std::fs::metadata(&lock_path).unwrap().ino(), inode); }
        let reacquired = try_exclusive_flock_detailed(&lock_path);
        assert!(matches!(reacquired, LockAttempt::Acquired(_)),
            "custody is released with the original lock path intact: path={}, result={reacquired:?}",
            lock_path.display());
    }

    #[test]
    fn redirected_profile_or_incremental_child_never_reaches_external_data() {
        #[cfg(unix)]
        use std::os::unix::fs::symlink;
        #[cfg(windows)]
        use std::os::windows::fs::symlink_dir as symlink;
        let tmp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("keep"), b"not cache ownership").unwrap();
        if let Err(error) = symlink(outside.path(), tmp.path().join("release")) {
            #[cfg(windows)]
            if error.raw_os_error() == Some(1314) {
                eprintln!("UNVERIFIED: Windows reparse fixture requires symbolic-link privilege: {error}");
                assert!(std::env::var_os("CONTINUUM_REQUIRE_WINDOWS_REPARSE_TEST").is_none(),
                    "this validation run requires actual Windows reparse execution");
                return;
            }
            panic!("create redirected profile fixture: {error}");
        }
        let tracked = TrackedDir::new("cargo-target", tmp.path().canonicalize().unwrap());
        let pool = fixture_pool(tracked, 1);
        assert_eq!(pool.evict_at_least(u64::MAX), 0);
        assert!(!outside.path().join(".cargo-lock").exists(), "refuse before creating custody through a redirect");
        #[cfg(unix)]
        std::fs::remove_file(tmp.path().join("release")).unwrap();
        #[cfg(windows)]
        std::fs::remove_dir(tmp.path().join("release")).unwrap();
        std::fs::create_dir_all(tmp.path().join("release/incremental")).unwrap();
        symlink(outside.path(), tmp.path().join("release/incremental/redirect")).unwrap();
        // The refused tree changed: the scanner's next observation is what reopens the pool.
        pool.tracked.set_bytes(dir_size_bytes(tmp.path()));
        assert_eq!(pool.evict_at_least(u64::MAX), 0);
        assert_eq!(std::fs::read(outside.path().join("keep")).unwrap(), b"not cache ownership");
        assert!(tmp.path().join("release/incremental/redirect").symlink_metadata().is_err(),
            "a link below a physical rung is unlinked, never followed (its target was kept above)");
        let parent = tmp.path().join("parent");
        std::fs::create_dir_all(parent.join("cache")).unwrap();
        let remembered = TrackedDir::new("cargo-target", parent.join("cache").canonicalize().unwrap());
        std::fs::rename(&parent, tmp.path().join("parent-before")).unwrap();
        std::fs::create_dir_all(outside.path().join("cache/release/incremental")).unwrap();
        symlink(outside.path(), &parent).unwrap();
        assert_eq!(fixture_pool(remembered, 1).evict_at_least(u64::MAX), 0,
            "a formerly physical root cannot acquire a new destination through its ancestor");
        assert!(!outside.path().join("cache/.cargo-lock").exists());
    }

    // what this catches: a link INSIDE the build tree refusing the whole eviction. Cargo's
    // cxxbridge writes `out/cxxbridge/include/<crate>/src/*.rs` as links back to source, so
    // refusing on any link below a rung meant the shared cache never evicted: the M5,
    // 2026-10-17 04:58Z to 10-18 09:28Z, 211 declines at 96% disk, every KV page save
    // refused, every resident turn re-prefilled cold. The real bytes beside the link go;
    // the link is unlinked; the source it names is untouched.
    #[cfg(unix)]
    #[test]
    fn a_link_inside_the_build_tree_is_unlinked_and_the_rest_is_reclaimed() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tempfile::tempdir().unwrap();
        std::fs::write(source.path().join("peer_connection_factory.rs"), b"source, never cache").unwrap();
        let include = tmp.path().join("debug/build/webrtc-sys-ac76/out/cxxbridge/include/webrtc-sys/src");
        std::fs::create_dir_all(&include).unwrap();
        std::os::unix::fs::symlink(source.path().join("peer_connection_factory.rs"), include.join("peer_connection_factory.rs")).unwrap();
        std::fs::create_dir_all(tmp.path().join("debug/deps")).unwrap();
        std::fs::write(tmp.path().join("debug/deps/libwebrtc_sys.rlib"), vec![7u8; 4096]).unwrap();
        let tracked = TrackedDir::new("cargo-target", tmp.path().canonicalize().unwrap());
        let pool = fixture_pool(tracked, 1);
        let freed = past_spawn_windows(&pool, |f| *f, || pool.evict_at_least(u64::MAX));
        assert!(freed >= 4096, "the real artifact beside the link is reclaimed: freed {freed}");
        assert!(!tmp.path().join("debug/deps/libwebrtc_sys.rlib").exists());
        assert!(include.join("peer_connection_factory.rs").symlink_metadata().is_err(), "the link itself is unlinked");
        assert_eq!(std::fs::read(source.path().join("peer_connection_factory.rs")).unwrap(), b"source, never cache");
    }

    // what this catches: pressure is usage/BUDGET (policy-bounded), not
    // usage/disk — the pool must go over-budget and trigger the broker
    // while the disk itself is still healthy. An unscanned dir reads as
    // 0 pressure (not under management yet), never NaN.
    #[test]
    fn pressure_is_budget_relative() {
        let tracked = TrackedDir::new("cargo-target", PathBuf::from("/tmp/none"));
        let pool = fixture_pool(tracked.clone(), 50_000);
        assert_eq!(pool.pressure(), 0.0, "unscanned = not under management");

        tracked.set_bytes(75_000); // scanner measured 1.5× the budget
        assert!(pool.pressure() > 1.4 && pool.pressure() < 1.6);
        assert_eq!(pool.capacity_bytes(), 50_000);
    }

    // what this catches: TrackedDir.record_freed saturates at zero — an
    // over-reported eviction must never wrap the cached size to u64::MAX
    // (which would read as 17 exabytes of pressure and evict forever).
    #[test]
    fn record_freed_saturates() {
        let tracked = TrackedDir::new("cargo-target", PathBuf::from("/tmp/none"));
        tracked.set_bytes(5_000);
        tracked.record_freed(10_000);
        assert_eq!(tracked.bytes(), 0);
    }

    // what this catches: THE CHAIN, not the components. The 2026-07-13
    // incident happened with every unit green — the monitor ran with no
    // reporters and the broker ran with no eviction owner, so 460 GB
    // accumulated while every individual piece "worked." This test runs
    // the real PressureBroker against a real over-budget CargoTargetPool
    // on a real temp tree and asserts bytes actually leave the disk when
    // relieve() fires. If any link (pressure math, tier thresholds,
    // broker act_above, pool registration shape, eviction ladder)
    // regresses, this fails — the health component Joel called critical
    // is guarded end-to-end, not piecewise.
    #[test]
    fn broker_relieve_actually_deletes_from_an_over_budget_pool() {
        use crate::paging::{BrokerConfig, PressureBroker};

        let tmp = tempfile::tempdir().expect("tempdir");
        let tracked = seeded_target(tmp.path());
        // Measured critical volume admits the broker independently of any
        // process-global monitor updates from concurrently running tests.
        let budget = tracked.bytes() / 2;
        let mut pool = fixture_pool(tracked.clone(), budget);
        pool.volume_sample = critical_fixture_volume;
        let pool = Arc::new(pool);

        let broker = PressureBroker::new(BrokerConfig::default());
        broker.register(pool.clone() as Arc<dyn ResourcePool>);

        let disk_before = dir_size_bytes(tmp.path());
        let report = past_spawn_windows(&pool, |r: &crate::paging::broker::ReliefReport| r.bytes_freed, || broker.relieve());

        assert!(report.triggered, "over-budget pool must trigger relief");
        assert!(report.bytes_freed > 0, "relief must free real bytes");
        assert!(
            dir_size_bytes(tmp.path()) < disk_before,
            "bytes must actually leave the disk, not just the accounting"
        );
        assert!(
            tmp.path().join("release/keep.bin").exists(),
            "release survives even broker-driven eviction"
        );
    }

    // what this catches: an ownerless cache class — the exact shape of
    // the incident. Every class in standard_tracked_dirs must have a
    // DECIDED eviction story: either an owner pool exists, or the class
    // is explicitly listed as deferred (eviction_decisions.rs) with the task that owns it.
    // Adding a sixth cache class without deciding makes this fail at
    // compile-adjacent cost instead of at a user's full disk.
    #[test]
    fn every_cache_class_has_a_decided_eviction_story() {
        // genome-models graduated from deferred to OWNED (#302): the
        // NvmeServingTierPool is exactly the "reference-aware, never
        // blind-delete a served model" owner the deferred entry demanded —
        // active-set aware, migrate-to-cold, verified-copy-before-delete.
        // logs + probes graduated straight to OWNED on registration
        // (2026-08-06) rather than through the deferred list: their
        // contents are rotation generations the writer has already
        // moved past, so `RotationLogPool` can drop the oldest with no
        // in-flight-set hazard to reason about. Deferring is for
        // classes where blind deletion is UNSAFE (a grading instance,
        // a served model); there is nothing unsafe here to defer for.
        use super::eviction_decisions::{DEFERRED, OWNED};
        use super::super::disk_pressure::DiskReporter as _;
        for dir in super::super::disk_reporters::standard_tracked_dirs(Path::new("/h"), Path::new("/h/.continuum")) {
            let name = dir.report().name;
            let decided =
                OWNED.contains(&name.as_str()) || DEFERRED.iter().any(|(n, _)| *n == name);
            assert!(
                decided,
                "cache class '{name}' has NO eviction decision — register an owner pool or \
                 add it to DEFERRED in disk_eviction/eviction_decisions.rs with the task that owns it (task #155; \
                 the 2026-07-13 incident was exactly an ownerless class filling the disk)"
            );
        }
    }

    mod serving_tier;
