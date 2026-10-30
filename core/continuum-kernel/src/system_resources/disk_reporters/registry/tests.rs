#![cfg(test)]
    use super::*;

    // what this catches: an admission refused on the reporter budget. The shell's cargo-root
    // grant went through the 100 ms pressure-reporter bound, and on a box at 91% swap one
    // resolution took longer than that: three of Benchy's commands failed outright with
    // "Cargo root resolution is still pending" (M5, 2026-10-16), none of them touching cargo.
    // Deterministic (Cormac on #5112: a zero bound races the first poll): the shared worker
    // is held on a gate the test releases; the reporter bound sees "still pending", and the
    // admission bound, once the gate opens, gets the answer from that same worker.
    #[tokio::test]
    async fn an_admission_waits_its_own_bound_not_the_reporters() {
        let registry = Registry::new(Vec::new()).await;
        let root = tempfile::tempdir().unwrap();
        let key = (root.path().to_path_buf(), PathWork::Resolve);
        let (release, gate) = tokio::sync::oneshot::channel::<Observation>();
        let held = async move { gate.await.map_err(|error| error.to_string()) }.boxed().shared();
        registry.2.lock().insert(key.clone(), (Arc::new(()), held));
        let pending = registry
            .path_work_within(root.path(), PathWork::Resolve, crate::system_resources::disk_pressure::REPORTER_TIMEOUT)
            .await;
        assert!(pending.unwrap_err().contains("still pending"), "the reporter bound is the refusal while the worker is held");
        assert!(registry.2.lock().contains_key(&key), "the held worker is kept, never duplicated");
        let physical = root.path().canonicalize().unwrap();
        release
            .send(Observation { path: physical.clone(), volume: None, empty_custody: None })
            .unwrap();
        let resolved = registry.path_work_within(root.path(), PathWork::Resolve, ADMISSION_BOUND).await.unwrap();
        assert_eq!(resolved.path, physical, "the admission bound gets the same worker's answer once it is free");
        assert!(!registry.2.lock().contains_key(&key), "nothing stays quarantined once it answered");
    }

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
            (identity.clone(), request.clone()),
        );
        let existing_key = (unavailable.path().to_path_buf(), PathWork::ResolveExisting);
        registry.2.lock().insert(existing_key.clone(), (identity.clone(), request));
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
            assert!(registry.path_work(unavailable.path(), PathWork::ResolveExisting)
                .await.unwrap_err().contains("still pending"));
            assert!(Arc::ptr_eq(&registry.2.lock().get(&existing_key).unwrap().0, &identity));
        }
        registry.admit(available.path()).await.unwrap();
        assert_eq!(registry.snapshot().len(), 1);
        assert_eq!(
            registry.2.lock().len(),
            3,
            "the original unresolved requests remain quarantined"
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
        assert_eq!(crate::system_resources::DiskReporter::report(observed.as_ref()).bytes, 0,
            "an already registered generic reporter stops double charging its inclusive parent");
        let in_flight = observed.eviction_custody();
        assert!(first.observe_citizen_target(later.path(), citizen).await.is_err(),
            "a later scan can fail to publish while the restored child remains owned");
        assert_eq!(crate::system_resources::DiskReporter::report(observed.as_ref()).bytes, 0);
        assert_eq!(observed.bytes(), 4096, "broker measurement survives publication failure");
        drop(in_flight);
        assert!(first.observe_citizen_target(later.path(), uuid::Uuid::new_v4()).await.is_err(),
            "a historic target cannot be reassigned to different hands");
        drop(first.admit(later.path()).await.unwrap());
        assert_eq!(observed.citizen(), Some(citizen), "later writer admission preserves resident custody");
        // Cormac's 5058 P2: the actual bounded preparation path must refuse
        // sensitive roots before mkdir, even when lexically hidden behind '..'.
        let home = tempfile::tempdir().unwrap();
        let policy = crate::system_resources::confinement::Confinement::for_caller(
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
        assert_eq!(crate::system_resources::DiskReporter::report(restored_child.as_ref()).bytes, 0,
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

    // what this catches (Codex on 9a5882d2d): a lease CHECK is not custody. An eviction that
    // checked for leases, released the lock and then renamed could detach a target a writer
    // was admitted to in between. One authority decides both, with the guards as explicit
    // barriers: (a) while an eviction holds its claim, admission to that target is refused;
    // once the target is detached and the claim dropped, a build is admitted to a fresh dir;
    // (b) while a writer's lease is held, the eviction claim is refused.
    // Mutation checks: dropping the `evicting` check in admit_within admits under (a);
    // dropping the lease check in claim_for_eviction claims under (b).
    #[tokio::test]
    async fn eviction_and_writer_admission_exclude_each_other_through_one_authority() {
        let (adapter, _database) = crate::orm::store::fresh_adapter().await;
        let registry = Registry::new(Vec::new()).await;
        registry.open_records(adapter).await.unwrap();
        let parent = tempfile::tempdir().unwrap();
        let parent = parent.path().canonicalize().unwrap();
        let target = parent.join("wt");
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("old.rlib"), b"stale").unwrap();

        let claim = registry.claim_for_eviction(&target).unwrap();
        let refused = registry.admit(&target).await.unwrap_err();
        assert!(refused.contains("being evicted"), "(a) admission waits out the claim: {refused}");
        assert!(registry.claim_for_eviction(&target).is_err(), "one eviction per target");
        std::fs::rename(&target, parent.join(".evicting-wt")).unwrap();
        drop(claim);
        std::fs::create_dir(&target).unwrap();
        let lease = registry.admit(&target).await.unwrap();
        assert!(std::fs::read_dir(&target).unwrap().next().is_none(), "the build gets a fresh target");

        let refused = match registry.claim_for_eviction(&target) {
            Ok(_) => panic!("(b) a leased target was claimed for eviction"),
            Err(why) => why,
        };
        assert!(refused.contains("lease"), "{refused}");
        drop(lease);
        assert!(registry.claim_for_eviction(&target).is_ok(), "the lease's end frees the target");
    }

