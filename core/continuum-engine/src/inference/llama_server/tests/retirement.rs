//! Retirement regression fixtures.
#![cfg(test)]
#[cfg(windows)]
use super::*;
// Regression for #4069: failed signalling and unknown wait must not free
// pages while the actual owned process remains alive. Generation-bound receipts
// must refuse foreign/stale requests and survive requester/controller drop.
// All files are isolated; no serving endpoint or live model is contacted.
#[tokio::test]
async fn retiring_page_owner_requires_observed_child_exit() {
    use super::*;
    const CHILD_ENV: &str = "CONTINUUM_PAGE_OWNER_TEST_CHILD";
    if std::env::var_os(CHILD_ENV).is_some() {
        use std::io::Read;
        let mut byte = [0];
        let _ = std::io::stdin().read(&mut byte);
        return;
    }
    let resources = crate::resources::ResourceDaemon::start(
        vec![Arc::new(crate::resources::MockCapacitySource::new(
            crate::resources::ResourceKind::Vram,
            10_000,
        ))],
        vec![],
        crate::resources::DaemonConfig::default(),
    );
    let reserve = || {
        resources
            .acquire_guarded(&crate::resources::LeaseRequest {
                consumer_id: "owned-child-fixture".into(),
                kind: crate::resources::ResourceKind::Vram,
                bytes: 6_000,
                ttl_ms: 60_000,
                reclaim_policy: crate::resources::ReclaimPolicy::Pinned,
            })
            .expect("fixture capacity grant")
    };
    let held = || {
        resources
            .board()
            .leases
            .iter()
            .map(|l| l.bytes)
            .sum::<u64>()
    };
    let root = tempfile::tempdir().expect("isolated pages");
    let registry = tempfile::tempdir().expect("isolated registry");
    let old = root.path().join("retirement-model--c32768");
    let current = root.path().join("retirement-model");
    let (reservation, ready) = PageDirReservation::take_and_create(&old);
    assert!(ready);
    std::fs::create_dir_all(&current).expect("current directory");
    std::fs::write(old.join("page"), b"owned").expect("owned page");
    let mut command =
        tokio::process::Command::new(std::env::current_exe().expect("test executable"));
    command
        .args([
            "--exact",
            "inference::llama_server::tests::retirement::retiring_page_owner_requires_observed_child_exit",
        ])
        .env(CHILD_ENV, "1")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000); // CREATE_NO_WINDOW: fixture never opens a console.
    let mut child = command.spawn().expect("owned fixture child");
    let pid = child.id().expect("running child PID");
    let input = child.stdin.take().expect("child input held open");
    assert!(child.try_wait().expect("child status").is_none());
    let mut process = LlamaServerProcess::with_root("http://127.0.0.1:1".into());
    // The fixture's engine program: never this host's slots, which `engine_bin` holds to the pin.
    process.bin_pin = Some("llama-server".into());
    *process.pending_resource_reservation.lock() = Some(reserve());
    let endpoint = crate::inference::slots::directory().endpoint(&process.root);
    let transition = endpoint.transition().await;
    let generation = transition
        .start_generation()
        .expect("fixture engine generation");
    process.install_child(child, generation.clone(), None, None);
    drop(transition);
    *process.retained_page_dir.lock() = Some(reservation);
    #[cfg(windows)]
    {
        use sha2::Digest;
        // Relative adapter paths are valid in the core cwd. A receipted
        // command must refuse them BEFORE touching this real incumbent.
        let engine = root.path().canonicalize().unwrap().join("receipt-engine");
        std::fs::create_dir(&engine).unwrap();
        let program = engine.join("llama-server.exe");
        std::fs::write(&program, b"fixture-engine").unwrap();
        let descriptor = serde_json::json!({
            "schema": 1, "source_revision": "0123456789012345678901234567890123456789",
            "backend": "cpu", "build_contract": "static-local-backends-v1",
            "runtime_origin": "installed-toolkit-bin-snapshot",
            "platform_contract": "windows-system32-nvidia-driver-v1",
            "files": {"llama-server.exe": format!("{:x}", sha2::Sha256::digest(b"fixture-engine"))}
        });
        std::fs::write(
            engine.join("engine-install.json"),
            serde_json::to_vec(&descriptor).unwrap(),
        )
        .unwrap();
        let relative_root = tempfile::tempdir_in(std::env::current_dir().unwrap()).unwrap();
        std::fs::write(relative_root.path().join("adapter.gguf"), b"adapter").unwrap();
        let weights = engine.join("model.gguf");
        std::fs::write(&weights, b"model").unwrap();
        let mut requested = target("relative-receipted-input");
        requested.model.gguf_local_path = Some(weights);
        requested.adapters.push(AdapterEntry {
            alias: "relative".into(),
            path: PathBuf::from(relative_root.path().file_name().unwrap()).join("adapter.gguf"),
        });
        let original =
            process.bin_pin.replace(program.to_string_lossy().into_owned());
        let admitted = std::sync::atomic::AtomicBool::new(false);
        std::fs::write(engine.join("engine-install.pending"), b"interrupted copy").unwrap();
        let incomplete = process
            .serve_if_current(&requested, &|| {
                admitted.store(true, std::sync::atomic::Ordering::SeqCst);
                true
            })
            .await;
        assert!(
            matches!(incomplete, Err(LlamaServerError::Spawn(ref why)) if why.contains("publication is incomplete"))
        );
        assert!(!admitted.load(std::sync::atomic::Ordering::SeqCst));
        std::fs::remove_file(engine.join("engine-install.pending")).unwrap();
        let result = process
            .serve_if_current(&requested, &|| {
                admitted.store(true, std::sync::atomic::Ordering::SeqCst);
                true
            })
            .await;
        assert!(
            matches!(result, Err(LlamaServerError::Spawn(ref why)) if why.contains("requires absolute model/config"))
        );
        // A real installed child must remain untouched by a refused exact
        // restore. Build its private input fixture with the production capture
        // path; no teacher/model inference or manufactured validation callback.
        let input = engine.join("pinned-input.jinja");
        std::fs::write(&input, b"original template").unwrap();
        let local_inputs = crate::inference::launch_inputs::LocalLaunchInputs::capture(
            Vec::new(),
            vec![input.clone()],
        )
        .await
        .unwrap();
        let engine_install = crate::inference::engine_install::EngineInstallReceipt::prepare(
            process.checked_engine().unwrap().program().to_string(),
        )
        .await
        .unwrap();
        let invocation = crate::inference::lane_args::base_invocation(
            requested.model.gguf_local_path.as_ref().unwrap(),
            requested.model_id(),
            "127.0.0.1",
            1,
            requested.parallel_lanes(),
            requested.served_total_ctx(),
            requested.host_prompt_cache_mib,
            &engine.join("pages"),
        );
        let prepared = Arc::new(PreparedLocalLaunch {
            target: requested.clone(),
            engine: process.checked_engine().unwrap(),
            engine_custody: Default::default(),
            endpoint: process.root.clone(),
            invocation: invocation.clone(),
            engine_install: engine_install.clone(),
            gguf: requested.model.gguf_local_path.clone().unwrap(),
            slot_save_dir: engine.join("pages"),
            page_cache_type: None,
            page_flash_attn: None,
            mtp_draft: None,
            mmproj: false,
            local_inputs: Ok(local_inputs.clone()),
            managed_environment: Vec::new(),
            expected_geometry: None,
        });
        process.child.lock().unwrap().as_mut().unwrap().local_inputs = Some(local_inputs);
        process.record_verified_target(
            &generation,
            &requested,
            &invocation,
            Some(prepared),
            process.checked_engine().unwrap().program(),
            11008,
            4,
        );
        drop(engine_install);
        let captured = process.owned_serving_target().unwrap();
        let recipe = captured.prepared.as_ref().unwrap();
        assert_eq!(recipe.expected_geometry, Some((11008, 4)));
        recipe.verify_geometry(11008, 4).unwrap();
        assert!(recipe.verify_geometry(5504, 2).is_err());
        let command = recipe.command().unwrap();
        assert_eq!(
            command
                .as_std()
                .get_args()
                .map(|v| v.to_os_string())
                .collect::<Vec<_>>(),
            invocation
                .args
                .iter()
                .map(std::ffi::OsString::from)
                .collect::<Vec<_>>()
        );
        assert_eq!(command.as_std().get_program(), program.as_os_str());
        // Missing original evidence refuses without even invoking admission.
        let mut refused = captured.clone();
        let mut recipe = refused.prepared.as_ref().unwrap().as_ref().clone();
        recipe.local_inputs = Err("fixture input is no longer certified".into());
        refused.prepared = Some(Arc::new(recipe));
        let refusal = process
            .restore_owned_launch(&refused, &|| {
                admitted.store(true, std::sync::atomic::Ordering::SeqCst);
                true
            })
            .await;
        assert!(
            matches!(refusal, Err(LlamaServerError::Spawn(ref why)) if why.contains("no longer certified"))
        );
        assert!(!admitted.load(std::sync::atomic::Ordering::SeqCst));
        assert!(!generation.has_exited());
        drop(refused);
        drop(captured);
        assert!(std::fs::write(&input, b"must remain pinned by actual child").is_err());
        // Existing retirement assertions below drop this owner, inject failed
        // kill/status, and only release its retained guard after observed exit.
        process
            .child
            .lock()
            .unwrap()
            .as_mut()
            .unwrap()
            .verified_target = None;
        process.bin_pin = original;
        assert!(!admitted.load(std::sync::atomic::Ordering::SeqCst));
        assert!(!generation.has_exited());
        assert!(process
            .child
            .lock()
            .unwrap()
            .as_mut()
            .unwrap()
            .child
            .try_wait()
            .unwrap()
            .is_none());
    }
    let launch_target = target("fixture-owned-launch");
    assert!(
        process.owned_serving_target().is_none(),
        "unverified generation has no launch receipt"
    );
    let mut invocation = crate::inference::lane_args::base_invocation(
        Path::new("/fixture/model.gguf"),
        launch_target.model_id(),
        "127.0.0.1",
        1,
        launch_target.parallel_lanes(),
        launch_target.served_total_ctx(),
        launch_target.host_prompt_cache_mib,
        Path::new("/fixture/pages"),
    )
    .with_options(&crate::inference::lane_args::LaneOptions::default());
    invocation.inherit_or_default_env(
        "GGML_MOE_PLAN_FILE",
        Path::new("/fixture/unused-plan").as_os_str(),
        true,
    );
    let launched = invocation.clone();
    process.record_verified_target(&generation, &launch_target, &invocation, None, process.checked_engine().unwrap().program(), 11008, 4);
    // A later resolved choice cannot mutate the already verified generation.
    invocation.args.push("--different-future-policy".into());
    let recorded = process
        .owned_serving_target()
        .expect("verified owned provenance");
    assert_eq!(recorded.target.context_window, launch_target.context_window);
    assert_eq!(recorded.observed_context_window, 11008);
    assert_eq!(recorded.observed_lanes, 4);
    assert_eq!(recorded.engine_program, process.checked_engine().unwrap().program().to_string());
    assert_eq!(recorded.invocation, launched);
    let restore_admitted = std::sync::atomic::AtomicBool::new(false);
    let no_original = process
        .restore_owned_launch(&recorded, &|| {
            restore_admitted.store(true, std::sync::atomic::Ordering::SeqCst);
            true
        })
        .await;
    assert!(
        matches!(no_original, Err(LlamaServerError::Spawn(ref why)) if why.contains("not captured"))
    );
    assert!(!restore_admitted.load(std::sync::atomic::Ordering::SeqCst));
    assert!(!generation.has_exited());
    // Feed the actual owner record through the existing reconcile fixture.
    // An accepted smaller plan must not replace its original launch geometry.
    let mut control = FakeControl::probe(Ok(Some(launch_target.model.id.clone())))
        .with_served_window(11008)
        .with_served_lanes(4)
        .owned();
    control.verified_launch = Some(recorded.clone());
    let mut drifted = launch_target.clone();
    // Downsize from what the engine actually serves, not the larger original
    // request: half of 32768 still exceeds this fixture's 11008-token lane.
    drifted.context_window = recorded.observed_context_window / 2;
    drifted.host_prompt_cache_mib = launch_target.host_prompt_cache_mib.saturating_add(1);
    let admissions = AtomicUsize::new(0);
    let admit = || {
        admissions.fetch_add(1, Ordering::SeqCst);
        true
    };
    assert_eq!(
        ensure_model_serving_if_current(&control, &drifted, false, &admit).await,
        EnsureOutcome::AlreadyServing
    );
    assert_eq!(
        admissions.load(Ordering::SeqCst),
        0,
        "already serving never admits a relaunch"
    );
    let kept = control
        .owned_serving_target()
        .expect("original verified launch");
    assert_eq!(kept.target.context_window, launch_target.context_window);
    assert_eq!(
        kept.target.host_prompt_cache_mib,
        launch_target.host_prompt_cache_mib
    );
    assert!(kept.identity == recorded.identity);
    assert_eq!(
        kept.invocation, launched,
        "AlreadyServing keeps original resolved invocation"
    );
    assert_eq!(kept.engine_program, process.checked_engine().unwrap().program().to_string());
    assert!(matches!(
        process.idle_if_current(&|| false).await,
        Err(LlamaServerError::Superseded)
    ));
    assert!(process.owned_engine().unwrap() == recorded.identity);
    let identity = process.owned_engine().expect("owned identity");
    let foreign = LlamaServerProcess::with_root("http://127.0.0.1:2".into());
    assert!(foreign.owned_engine().is_none());
    assert!(matches!(
        foreign.retire_owned_engine(&identity).await,
        Err(EngineRetirementError::NotOwned)
    ));
    assert!(process
        .child
        .lock()
        .unwrap()
        .as_mut()
        .unwrap()
        .child
        .try_wait()
        .unwrap()
        .is_none());
    let receipt = process
        .retire_owned_engine_with(&identity, |_| {
            Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "injected signal failure",
            ))
        })
        .await
        .expect("retirement remains owned after signal refusal");
    assert_eq!(
        process.observe_engine_retirement(&receipt),
        Ok(EngineRetirementStatus::Pending)
    );
    assert_eq!(
        foreign.observe_engine_retirement(&receipt),
        Err(EngineRetirementError::NotOwned)
    );
    assert!(
        process.owned_serving_target().is_none(),
        "retired ownership cannot be advertised as current launch"
    );
    assert_eq!(
        held(),
        6_000,
        "signal failure must retain the real resource grant"
    );
    drop(receipt.clone()); // Dropping a requester's observation cannot abandon the child.
    {
        let mut guard = PAGE_DIR_GUARD.lock();
        guard.collect_exited_with(|_| Err(std::io::Error::other("injected unknown status")));
        assert!(guard
            .retiring
            .iter()
            .any(|owner| owner.child.id() == Some(pid)));
        assert!(guard.reservations.contains(&old));
        assert!(
            !generation.has_exited(),
            "uncertain polling cannot acknowledge engine exit"
        );
    }
    assert_eq!(
        migrate_legacy_page_dir_guarded_in(&current, registry.path()),
        Some(0)
    );
    assert_eq!(held(), 6_000, "unknown status cannot release capacity");
    #[cfg(windows)]
    assert!(std::fs::write(
        root.path().join("receipt-engine/pinned-input.jinja"),
        b"still in use"
    )
    .is_err());
    assert!(old.join("page").is_file());
    {
        let mut guard = PAGE_DIR_GUARD.lock();
        let index = guard
            .retiring
            .iter()
            .position(|owner| owner.child.id() == Some(pid))
            .expect("owned child");
        let dir = guard.retiring[index].reserved_dir.take();
        assert!(
            guard.has_unknown_retiring_dir(),
            "missing directory evidence must close the migration gate"
        );
        guard.retiring[index].reserved_dir = dir;
    }
    // Keep the reservation listed while awaiting the fixture outside the lock.
    let mut owner = {
        let mut guard = PAGE_DIR_GUARD.lock();
        let index = guard
            .retiring
            .iter()
            .position(|owner| owner.child.id() == Some(pid))
            .expect("retained child");
        guard.retiring.swap_remove(index)
    };
    drop(input);
    let status = tokio::time::timeout(std::time::Duration::from_secs(10), owner.child.wait())
        .await
        .expect("fixture exits after stdin closes")
        .expect("observed exit");
    assert!(status.success());
    {
        let mut guard = PAGE_DIR_GUARD.lock();
        guard.retiring.push(owner);
        guard.collect_exited();
        assert!(!guard.reservations.contains(&old));
        assert!(
            generation.has_exited(),
            "actual child exit supplies the generation receipt"
        );
    }
    assert_eq!(
        migrate_legacy_page_dir_guarded_in(&current, registry.path()),
        Some(1)
    );
    assert!(!old.exists(), "the emptied legacy dir is removed");
    assert!(current.join("page").is_file(), "its page moved into the model's dir, never deleted");
    assert_eq!(held(), 0, "observed child exit releases its grant");
    #[cfg(windows)]
    std::fs::write(
        root.path().join("receipt-engine/pinned-input.jinja"),
        b"released after observed exit",
    )
    .unwrap();
    assert_eq!(
        process.observe_engine_retirement(&receipt),
        Ok(EngineRetirementStatus::Exited)
    );
    assert_eq!(
        process.observe_engine_retirement(&receipt),
        Ok(EngineRetirementStatus::Exited)
    );

    // A receipt for the old child cannot authorize retiring its replacement.
    let mut replacement = command.spawn().expect("replacement fixture child");
    let replacement_input = replacement.stdin.take().expect("replacement input");
    let transition = endpoint.transition().await;
    let replacement_generation = transition
        .start_generation()
        .expect("verified predecessor exit");
    process.install_child(replacement, replacement_generation.clone(), None, None);
    drop(transition);
    assert!(matches!(
        process.retire_owned_engine(&identity).await,
        Err(EngineRetirementError::StaleGeneration)
    ));
    assert_eq!(
        process.observe_engine_retirement(&receipt),
        Ok(EngineRetirementStatus::Exited)
    );
    assert!(!replacement_generation.has_exited());
    assert!(process
        .child
        .lock()
        .unwrap()
        .as_mut()
        .unwrap()
        .child
        .try_wait()
        .unwrap()
        .is_none());
    let replacement_identity = process.owned_engine().expect("replacement identity");
    let replacement_receipt = process
        .retire_owned_engine_with(&replacement_identity, |_| {
            Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "injected replacement signal failure",
            ))
        })
        .await
        .expect("exact replacement retirement remains owned after signal refusal");
    drop(process); // The retained process owner survives the controller and requester.
    drop(replacement_input);
    let mut owner = {
        let mut guard = PAGE_DIR_GUARD.lock();
        // Closing stdin may already have produced a terminal receipt.
        guard.collect_exited();
        guard
            .retiring
            .iter()
            .position(|owner| owner.generation.same_engine(&replacement_generation))
            .map(|index| guard.retiring.swap_remove(index))
    };
    if let Some(owner) = owner.as_mut() {
        tokio::time::timeout(std::time::Duration::from_secs(10), owner.child.wait())
            .await
            .expect("replacement exits")
            .expect("replacement exit status");
    }
    if let Some(owner) = owner {
        let mut guard = PAGE_DIR_GUARD.lock();
        guard.retiring.push(owner);
        guard.collect_exited();
    }
    assert!(replacement_receipt.identity.generation.has_exited());

    // What this catches: unwinding the real ephemeral owner after child
    // installation (error, timeout, cancellation or normal end) must move
    // BOTH child and grant into retirement, not merely signal then free it.
    for outcome in ["error", "timeout", "cancel", "complete", "finish"] {
        let process =
            LlamaServerProcess::with_root(format!("test://borrower-{}", uuid::Uuid::new_v4()));
        let endpoint = crate::inference::slots::directory().endpoint(&process.root);
        let transition = endpoint.transition().await;
        let generation = transition.start_generation().expect("borrower generation");
        let lane = EphemeralServingLane::with_reservation(process, 1, Some(reserve()));
        let mut child = command.spawn().expect("borrower child");
        let input = child.stdin.take().expect("keep borrower alive");
        lane.proc
            .install_child(child, generation.clone(), None, None);
        drop(transition);
        assert_eq!(held(), 6_000);
        let operation = async move {
            let mut owner = lane;
            match outcome {
                "error" => Err::<(), _>("readiness refused"),
                "complete" => Ok(()),
                "finish" => {
                    tokio::time::timeout(Duration::from_secs(10), async {
                        let mut tick = tokio::time::interval(Duration::from_millis(10));
                        while owner.finish().await.unwrap() != EngineRetirementStatus::Exited {
                            tick.tick().await;
                        }
                        assert_eq!(
                            owner.finish().await.unwrap(),
                            EngineRetirementStatus::Exited,
                            "terminal finish remains observable without surrendering the owner"
                        );
                    })
                    .await
                    .unwrap();
                    Ok(())
                }
                _ => std::future::pending().await,
            }
        };
        match outcome {
            "error" => assert!(operation.await.is_err()),
            "complete" | "finish" => assert!(operation.await.is_ok()),
            "timeout" => assert!(tokio::time::timeout(
                std::time::Duration::from_millis(1),
                operation
            )
            .await
            .is_err()),
            "cancel" => {
                let mut operation = Box::pin(operation);
                assert!(futures::poll!(&mut operation).is_pending());
                drop(operation);
            }
            _ => unreachable!(),
        }
        let mut owner = {
            let mut guard = PAGE_DIR_GUARD.lock();
            // A very fast child can already have exited during Drop. Only
            // that actual receipt permits release; no elapsed-time assertion.
            assert_eq!(
                held(),
                if generation.has_exited() { 0 } else { 6_000 },
                "{outcome} must not free capacity before observed exit"
            );
            guard
                .retiring
                .iter()
                .position(|owner| owner.generation.same_engine(&generation))
                .map(|index| guard.retiring.swap_remove(index))
        };
        drop(input);
        if let Some(owner) = owner.as_mut() {
            tokio::time::timeout(std::time::Duration::from_secs(10), owner.child.wait())
                .await
                .expect("borrower exits")
                .expect("borrower exit status");
        }
        if let Some(owner) = owner {
            PAGE_DIR_GUARD.lock().retiring.push(owner);
        }
        collect_retired_engines();
        assert!(generation.has_exited());
        assert_eq!(
            held(),
            0,
            "collector releases after actual {outcome} child exit"
        );
    }

    // Cancellation before OS spawn has no child to retain the reservation.
    let lane = EphemeralServingLane::with_reservation(
        LlamaServerProcess::with_root("test://unspawned-borrower".into()),
        1,
        Some(reserve()),
    );
    let mut unspawned = Box::pin(async move {
        let _owner = lane;
        std::future::pending::<()>().await;
    });
    assert!(futures::poll!(&mut unspawned).is_pending());
    drop(unspawned);
    assert_eq!(held(), 0, "no child exists before OS spawn");

}
