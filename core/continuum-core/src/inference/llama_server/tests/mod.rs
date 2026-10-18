#![cfg(test)]

// what this catches (card 6f3218ed, the act-bound half): the per-request rule that keeps
// a lost request from riding someone else's progress. The first checkpoint needs only a
// processing slot; a later one needs the SAME task with its work moved; a new task on the
// slot, an idle slot, or no reading at all ends the act.
#[test]
fn an_act_extends_only_while_its_own_slot_advances() {
    let body = serde_json::json!([
        {"id": 0, "id_task": 7, "is_processing": true, "n_prompt_tokens_processed": 4096, "next_token": [{"n_decoded": 0}]},
        {"id": 3, "id_task": 9, "is_processing": false, "n_prompt_tokens_processed": 2048, "next_token": [{"n_decoded": 0}]}
    ]);
    let ours = slot_work_of(&body, 0).expect("slot 0");
    assert_eq!((ours.task, ours.processing, ours.work), (7, true, 4096));
    assert_eq!(slot_work_of(&body, 5), None, "no such slot");
    let idle = slot_work_of(&body, 3);
    assert!(slot_advanced(None, Some(ours)), "first checkpoint: processing is enough");
    assert!(!slot_advanced(None, idle), "an idle slot is not progress, whatever its counters");
    assert!(!slot_advanced(None, None), "no reading is no evidence");
    let moved = SlotWork { work: 6144, ..ours };
    assert!(slot_advanced(Some(ours), Some(moved)), "same task, work moved");
    assert!(!slot_advanced(Some(ours), Some(ours)), "same task, nothing moved: stuck");
    assert!(!slot_advanced(Some(ours), Some(SlotWork { task: 8, ..moved })), "another task on the slot: ours is gone");
}

// what this catches (card 6f3218ed, IntelMac 2026-09-27): a generation's header wait
// killing a request that is queued or prefilling. With quiet_checkpoints = 1 (the header
// wait's setting), an engine whose work fingerprint keeps moving holds the wait across
// many checkpoints and the response is returned; an engine that goes quiet (nothing
// moved, nothing processing) ends it after ONE grace checkpoint, not the page switch's 8;
// and an engine processing with nothing moving is stuck at the first checkpoint.
#[tokio::test(start_paused = true)]
async fn a_header_wait_rides_a_moving_engine_and_ends_on_a_quiet_one() {
    let bound = std::time::Duration::from_secs(300);
    // c610ef8f: only counters from this exact restore hold its waiter. A
    // completed snapshot cannot replace the pending terminal HTTP receipt.
    let operation = uuid::Uuid::new_v4();
    let snapshot = |bytes, phase| serde_json::json!({
        "operation_id": operation, "id_slot": 0, "action": "restore",
        "phase": phase, "state_bytes": bytes, "sidecar_bytes": 0, "checkpoints": 0,
    });
    assert!(restore_progress(&snapshot(1, 1), uuid::Uuid::new_v4(), 0).is_none());
    assert!(restore_progress(&snapshot(1, 1), operation, 1).is_none());
    // Replaced/unknown operations cannot inherit another slot's moving
    // counters. Legacy compatibility is restricted to a successful array.
    let legacy = serde_json::json!([{"id_task": 9, "is_processing": true}]);
    assert!(matches!(restore_probe_response(reqwest::StatusCode::OK, &legacy, operation, 0), EngineProbe::Progress(_)));
    for (status, body) in [
        (reqwest::StatusCode::NOT_FOUND, legacy),
        (reqwest::StatusCode::OK, snapshot(900, 2)),
    ] {
        let unknown = restore_probe_response(status, &body, uuid::Uuid::new_v4(), 0);
        assert_eq!(unknown, EngineProbe::Busy);
        assert_eq!(wait_while_engine_progresses(std::future::pending::<()>(), bound, 1,
            || async { unknown }, |_| {}).await, None);
    }
    let completed = restore_progress(&snapshot(100, 3), operation, 0).unwrap();
    assert_eq!(wait_while_engine_progresses(std::future::pending::<()>(), bound, 1,
        || async { completed }, |_| {}).await, None);
    for phase in [4, 5] {
        let ended = restore_progress(&snapshot(100, phase), operation, 0).unwrap();
        let started = tokio::time::Instant::now();
        assert_eq!(wait_while_engine_progresses(std::future::pending::<()>(), bound, 1,
            || async { ended }, |_| {}).await, None);
        assert_eq!(started.elapsed(), Duration::ZERO);
    }
    let mut bytes = 0;
    let progressing = || {
        bytes += 1;
        let receipt = restore_progress(&snapshot(bytes, 2), operation, 0).unwrap();
        async move { receipt }
    };
    let terminal = async { tokio::time::sleep(bound * 5).await; "HTTP receipt" };
    assert_eq!(wait_while_engine_progresses(terminal, bound, 1, progressing, |_| {}).await,
        Some("HTTP receipt"));
    // Card 198ffe08: a slow initial/checkpoint probe must not stop polling
    // work or discard its terminal receipt when the probe later reports dead.
    for slow_initial in [true, false] {
        let mut calls = 0;
        let slow_probe = || {
            calls += 1;
            let slow = slow_initial || calls > 1;
            async move {
                if slow {
                    tokio::time::sleep(bound * 3).await;
                    EngineProbe::Unreachable
                } else {
                    EngineProbe::Busy
                }
            }
        };
        let completes_after = if slow_initial { bound / 2 } else { bound + bound / 2 };
        let work = async {
            tokio::time::sleep(completes_after).await;
            "terminal receipt"
        };
        let started = tokio::time::Instant::now();
        assert_eq!(wait_while_engine_progresses(work, bound, 1, slow_probe, |_| {}).await,
            Some("terminal receipt"));
        assert_eq!(started.elapsed(), completes_after,
            "completion must not wait for the unrelated probe deadline");
    }
    let moving = std::sync::atomic::AtomicU64::new(0);
    let advancing = || {
        let n = moving.fetch_add(2048, std::sync::atomic::Ordering::Relaxed);
        async move { EngineProbe::Progress(EngineProgress { fingerprint: n, any_processing: true }) }
    };
    let late = async {
        tokio::time::sleep(bound * 5).await;
        "headers"
    };
    let mut checkpoints = 0;
    let got = wait_while_engine_progresses(late, bound, 1, advancing, |n| checkpoints = n).await;
    assert_eq!(got, Some("headers"), "a moving engine is busy, not dead, for five bounds");
    assert!(checkpoints >= 4, "each checkpoint was a busy one: {checkpoints}");

    let started = tokio::time::Instant::now();
    let quiet = || async { EngineProbe::Progress(EngineProgress { fingerprint: 7, any_processing: false }) };
    let never = wait_while_engine_progresses(std::future::pending::<&str>(), bound, 1, quiet, |_| {}).await;
    assert_eq!(never, None);
    assert!(started.elapsed() <= bound * 2, "one grace checkpoint, then dead: {:?}", started.elapsed());

    let stuck = || async { EngineProbe::Progress(EngineProgress { fingerprint: 7, any_processing: true }) };
    let started = tokio::time::Instant::now();
    assert_eq!(wait_while_engine_progresses(std::future::pending::<&str>(), bound, 1, stuck, |_| {}).await, None);
    assert!(started.elapsed() <= bound, "processing with nothing moving is stuck at the first checkpoint");

    // A CONDEMNED engine ends the wait at its checkpoint even while it keeps moving: the
    // daemon is replacing it, and its relaunch's drain waits on this very turn (IntelMac
    // 2026-09-27: 25 and then 46 min dark while header waits rode a prefilling engine).
    let mut calls = 0u64;
    let condemned_later = || {
        calls += 1;
        let c = calls;
        async move {
            if c == 1 {
                EngineProbe::Progress(EngineProgress { fingerprint: c, any_processing: true })
            } else {
                EngineProbe::Condemned
            }
        }
    };
    let started = tokio::time::Instant::now();
    assert_eq!(wait_while_engine_progresses(std::future::pending::<&str>(), bound, 1, condemned_later, |_| {}).await, None);
    assert!(started.elapsed() <= bound, "condemned at the first checkpoint ends it there: {:?}", started.elapsed());
}

// what this catches (2026-09-27, the M5 at 10.4 GiB free beside the 27B): a node whose
// lane fills its memory refused every deploy. Between the floor and plenty the build takes
// fewer jobs; with plenty, cargo's own count; the floor itself is one job plus the reserve.
// In the lib so CI runs it (the CLI's own tests run only by name).
#[test]
fn a_warm_build_takes_the_jobs_its_free_memory_holds() {
    let gib = 1024 * 1024 * 1024;
    // measured: continuum_core's front end alone peaked at 6.4 GB, so one job needs it
    // plus the reserve; a second job only past another 2.5 GiB
    assert_eq!(WARM_BUILD_MIN_FREE_BYTES, 4 * gib + 6656 * 1024 * 1024);
    assert_eq!(warm_build_jobs_for_memory(WARM_BUILD_MIN_FREE_BYTES), Some(1));
    assert_eq!(warm_build_jobs_for_memory(WARM_BUILD_MIN_FREE_BYTES + 2560 * 1024 * 1024 - 1), Some(1));
    assert_eq!(warm_build_jobs_for_memory(WARM_BUILD_MIN_FREE_BYTES + 2560 * 1024 * 1024), None, "where a second job would start, the build is already uncapped");
    assert_eq!(warm_build_jobs_for_memory(12 * gib), Some(1), "under the measured uncapped need");
    assert_eq!(warm_build_jobs_for_memory(13 * gib), None);
    assert!(WARM_BUILD_UNCAPPED_FREE_BYTES >= WARM_BUILD_RESERVE_BYTES + 8_700_000_000, "uncapped covers its measured peak plus the reserve");
}

// what this catches (card 7c5f139d): an adopted lane left on last deploy's engine
// forever — the M5 and IntelMac served b10765-965d38a90 (no /train) while the stamp
// moved on. A lane whose /props build names another commit than the installed stamp is
// stale; the same commit in short or full form is current; an unknown on either side
// is never a relaunch.
#[test]
fn a_lane_on_an_older_engine_than_the_installed_one_is_stale() {
    assert!(!lane_runs_installed_engine("b10765-965d38a90", "9733aca6c"), "older engine: relaunch");
    assert!(lane_runs_installed_engine("b10765-965d38a90", "965d38a90"), "same commit");
    assert!(lane_runs_installed_engine("b10765-965d38a90", "965d38a900bc82e28900683f9e30f7333cbf3936"), "short vs full SHA");
    assert!(lane_runs_installed_engine("", "9733aca6c"), "a lane naming no build is not stale");
    assert!(lane_runs_installed_engine("b10765-965d38a90", ""), "no stamp: nothing to compare");
    assert!(lane_runs_installed_engine("b10765-unknown", "9733aca6c"), "a non-SHA build is not evidence");
}

// what this catches: an engine older than --train-dir being handed the flag (it refuses to
// start: the lane goes dark). Only an engine whose help LISTS the option accepts it; a help
// text that merely mentions the words, or none at all, does not.
#[test]
fn train_dir_is_accepted_only_when_the_engines_help_lists_it() {
    use super::engine_help_lists_train_dir;
    let new = "  -lv, --verbosity N\n--train-dir DIR                         enable POST /train\n";
    let old = "  -lv, --verbosity N\n  --lora FNAME   path to LoRA adapter\n";
    let mention = "  --lora FNAME   adapters are not --train-dir aware\n";
    assert!(engine_help_lists_train_dir(new));
    assert!(!engine_help_lists_train_dir(old));
    assert!(!engine_help_lists_train_dir(mention));
    assert!(!engine_help_lists_train_dir(""));
}

// regression for card 8c06f778 (BigMama, 2026-09-26 14:24:55Z): a /slots read that
// timed out mid-restore was read as a dead engine and cost the node an engine
// replacement.
// what this catches: the three outcomes are told apart by the SOCKET, not by
// "did we get a value" — an open socket that never answers is Busy, a socket
// nothing listens on is Unreachable, and a real slots array is Progress.
#[tokio::test]
async fn a_probe_that_times_out_is_busy_and_only_a_missing_socket_is_unreachable() {
    use super::{engine_probe_within, EngineProbe};
    let client = reqwest::Client::new();
    let short = std::time::Duration::from_millis(200);
    // Busy: a listener that accepts and holds the connection without answering.
    let silent = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("test: bind");
    let root = format!("http://{}", silent.local_addr().expect("test: addr"));
    let hold = tokio::spawn(async move {
        let (stream, _) = silent.accept().await.expect("test: accept");
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        drop(stream);
    });
    assert_eq!(engine_probe_within(&root, &client, short).await, EngineProbe::Busy);
    hold.abort();
    // Unreachable: a port that was ours a moment ago and now has no listener.
    let gone = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("test: bind");
    let root = format!("http://{}", gone.local_addr().expect("test: addr"));
    drop(gone);
    assert_eq!(engine_probe_within(&root, &client, short).await, EngineProbe::Unreachable);
    // Progress: a slots array is a fingerprint.
    let app = axum::Router::new().route(
        "/slots",
        axum::routing::get(|| async {
            axum::Json(serde_json::json!([{"id_task": 7, "n_prompt_tokens_processed": 100, "next_token": [{"n_decoded": 5}], "is_processing": true}]))
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("test: bind");
    let root = format!("http://{}", listener.local_addr().expect("test: addr"));
    let server = tokio::spawn(async move { axum::serve(listener, app).await });
    let probe = engine_probe_within(&root, &client, std::time::Duration::from_secs(2)).await;
    assert!(matches!(probe, EngineProbe::Progress(p) if p.any_processing && p.fingerprint == 112), "{probe:?}");
    server.abort();
}
// what this catches (Codex on #4467): two retires where the engine frees the first and
// refuses the second (a turn holds it) left the core advertising BOTH, though the engine
// had freed one. The served set must name exactly what the engine holds after a busy
// stop, and the new gene must not have been loaded ahead of the refused retire.
#[tokio::test]
async fn a_busy_second_retire_leaves_the_served_set_naming_what_the_engine_holds() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let loads = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&loads);
    let app = axum::Router::new()
        .route(
            "/lora-adapters",
            axum::routing::get(|| async {
                axum::Json(serde_json::json!([
                    {"id": 0, "path": "/genes/a.gguf"},
                    {"id": 1, "path": "/genes/b.gguf"}
                ]))
            }),
        )
        .route(
            "/lora-adapters/unload",
            axum::routing::post(|axum::Json(body): axum::Json<serde_json::Value>| async move {
                if body["id"] == 0 {
                    axum::http::StatusCode::OK
                } else {
                    axum::http::StatusCode::SERVICE_UNAVAILABLE
                }
            }),
        )
        .route(
            "/lora-adapters/load",
            axum::routing::post(move || {
                seen.fetch_add(1, Ordering::SeqCst);
                async { axum::http::StatusCode::OK }
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("test: bind");
    let root = format!("http://{}", listener.local_addr().expect("test: addr"));
    let server = tokio::spawn(async move { axum::serve(listener, app).await });
    let process = LlamaServerProcess::with_root(root);
    *process.served_adapters.lock().unwrap() = vec!["/genes/a.gguf".into(), "/genes/b.gguf".into()]; // JUSTIFIED: test-owned lock, never poisoned
    let dir = tempfile::tempdir().expect("test: tempdir");
    let got = process
        .converge_genome(dir.path(), &target_with_adapters("coder-14b", &["/genes/c.gguf"]))
        .await
        .expect("test: the engine answered");
    assert_eq!(got, GenomeInPlace::Busy);
    assert_eq!(
        *process.served_adapters.lock().unwrap(), // JUSTIFIED: test-owned lock, never poisoned
        vec!["/genes/b.gguf".to_string()],
        "A was freed and B still held: the record says exactly that"
    );
    assert_eq!(loads.load(Ordering::SeqCst), 0, "no new gene joins ahead of a refused retire");
    server.abort();
}

// The service-host's verified engine must win over any older default install.
// A child process isolates the launch environment from parallel tests.
#[test]
fn serving_uses_the_supervisors_engine() {
    const CHILD: &str = "CONTINUUM_ENGINE_CONTRACT_TEST_CHILD";
    const ENGINE: &str = "prepared release/engine-b/llama-server.exe";
    if std::env::var_os(CHILD).is_some() {
        assert_eq!(super::server_bin().unwrap(), ENGINE);
        return;
    }
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "inference::llama_server::tests::serving_uses_the_supervisors_engine",
        ])
        .env(CHILD, "1")
        .env("LLAMA_SERVER_BIN", ENGINE);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    assert!(command.status().unwrap().success());
}

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
    let current = root.path().join("retirement-model--c49152");
    let (reservation, ready) = PageDirReservation::take_and_create(&old);
    assert!(ready);
    std::fs::create_dir_all(&current).expect("current directory");
    std::fs::write(old.join("page"), b"owned").expect("owned page");
    let mut command =
        tokio::process::Command::new(std::env::current_exe().expect("test executable"));
    command
        .args([
            "--exact",
            "inference::llama_server::tests::retiring_page_owner_requires_observed_child_exit",
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
    let process = LlamaServerProcess::with_root("http://127.0.0.1:1".into());
    *process.pending_resource_reservation.lock() = Some(reserve());
    #[cfg(windows)]
    let mut process = process;
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
            process.engine_bin().unwrap(),
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
            engine_program: process.engine_bin().unwrap(),
            endpoint: process.root.clone(),
            invocation: invocation.clone(),
            engine_install: engine_install.clone(),
            gguf: requested.model.gguf_local_path.clone().unwrap(),
            slot_save_dir: engine.join("pages"),
            page_cache_type: None,
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
            &process.engine_bin().unwrap(),
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
    invocation.constrain_to_cpu();
    let launched = invocation.clone();
    process.record_verified_target(&generation, &launch_target, &invocation, None, &process.engine_bin().unwrap(), 11008, 4);
    // A later resolved choice cannot mutate the already verified generation.
    invocation.args.push("--different-future-policy".into());
    let recorded = process
        .owned_serving_target()
        .expect("verified owned provenance");
    assert_eq!(recorded.target.context_window, launch_target.context_window);
    assert_eq!(recorded.observed_context_window, 11008);
    assert_eq!(recorded.observed_lanes, 4);
    assert_eq!(recorded.engine_program, process.engine_bin().unwrap());
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
    assert_eq!(kept.engine_program, process.engine_bin().unwrap());
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
        sweep_stale_page_generations_guarded_in(&current, registry.path()),
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
            "missing directory evidence must close the sweep gate"
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
        sweep_stale_page_generations_guarded_in(&current, registry.path()),
        Some(1)
    );
    assert!(!old.exists());
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

    #[cfg(windows)]
    {
        use sha2::Digest;
        // Drive real preparation and the shared restore->OS-spawn path with
        // a non-model Windows child. Readiness must fail honestly; this does
        // not claim that a fixture command is a ready inference engine.
        let fixture = tempfile::tempdir().unwrap();
        let home = fixture.path().canonicalize().unwrap();
        let _home = crate::test_env::HomeGuard::set(&home).await;
        let engine = home.join("engine");
        std::fs::create_dir(&engine).unwrap();
        let program = engine.join("llama-server.exe");
        let system =
            PathBuf::from(std::env::var_os("SystemRoot").expect("Windows fixture system root"))
                .join("System32");
        std::fs::copy(system.join("where.exe"), &program).unwrap();
        let bytes = std::fs::read(&program).unwrap();
        std::fs::write(
            engine.join("engine-install.json"),
            serde_json::to_vec(&serde_json::json!({
                "schema": 1, "source_revision": "0123456789012345678901234567890123456789",
                "backend": "cpu", "build_contract": "static-local-backends-v1",
                "runtime_origin": "installed-toolkit-bin-snapshot",
                "platform_contract": "windows-system32-nvidia-driver-v1",
                "files": {"llama-server.exe": format!("{:x}", sha2::Sha256::digest(bytes))}
            }))
            .unwrap(),
        )
        .unwrap();
        let weights = engine.join("weights.gguf");
        candle_core::quantized::gguf_file::write(
            &mut std::fs::File::create(&weights).unwrap(),
            &[],
            &[],
        )
        .unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let mut process = LlamaServerProcess::with_root(format!("http://127.0.0.1:{port}"));
        process.bin_pin = Some(program.to_string_lossy().into_owned());
        let mut requested = target("prepared-launch-fixture");
        requested.model.gguf_local_path = Some(weights);
        requested.placement = LanePlacement::Cpu;
        let prepared = Arc::new(process.prepare_local_launch(&requested).await.unwrap());
        assert!(
            prepared.local_inputs.is_ok(),
            "fixture must obtain actual original input evidence"
        );
        let original_args = prepared
            .command()
            .unwrap()
            .as_std()
            .get_args()
            .map(|v| v.to_os_string())
            .collect::<Vec<_>>();
        std::fs::write(
            engine.join("chat_template.jinja"),
            b"a later template must not be selected",
        )
        .unwrap();
        requested.context_window = 4096;
        let mut later = prepared.as_ref().clone();
        let mut changed_trace = prepared
            .managed_environment
            .iter()
            .find(|(key, _)| key == "LLAMA_TRACE")
            .and_then(|(_, value)| value.clone())
            .unwrap_or_default();
        changed_trace.push("-different-fixture-choice");
        later
            .managed_environment
            .push(("LLAMA_TRACE".into(), Some(changed_trace)));
        let replay_command = prepared.command().unwrap();
        assert_eq!(
            replay_command
                .as_std()
                .get_args()
                .map(|v| v.to_os_string())
                .collect::<Vec<_>>(),
            original_args
        );
        assert!(!prepared
            .invocation
            .args
            .iter()
            .any(|arg| arg == "--chat-template-file"));
        assert_ne!(
            later
                .command()
                .unwrap()
                .as_std()
                .get_envs()
                .find(|(k, _)| *k == "LLAMA_TRACE")
                .map(|(_, v)| v.map(|v| v.to_os_string())),
            replay_command
                .as_std()
                .get_envs()
                .find(|(k, _)| *k == "LLAMA_TRACE")
                .map(|(_, v)| v.map(|v| v.to_os_string()))
        );
        drop(later);
        let endpoint = crate::inference::slots::directory().endpoint(&process.root);
        let transition = endpoint.transition().await;
        let generation = transition.start_generation().unwrap();
        let mut child = command.spawn().unwrap();
        let child_input = child.stdin.take().unwrap();
        process.install_child(
            child,
            generation.clone(),
            prepared.engine_install.clone(),
            prepared.local_inputs.as_ref().ok().cloned(),
        );
        process.record_verified_target(
            &generation,
            &prepared.target,
            &prepared.invocation,
            Some(prepared.clone()),
            &process.engine_bin().unwrap(),
            11008,
            4,
        );
        drop(transition);
        let captured = process.owned_serving_target().unwrap();
        let admitted = std::sync::atomic::AtomicBool::new(false);
        let live_refusal = process
            .restore_owned_launch(&captured, &|| {
                admitted.store(true, std::sync::atomic::Ordering::SeqCst);
                true
            })
            .await;
        assert!(
            matches!(live_refusal, Err(LlamaServerError::Spawn(ref why)) if why.contains("exit is not confirmed"))
        );
        assert!(!admitted.load(std::sync::atomic::Ordering::SeqCst));
        let retired = process
            .retire_owned_engine(&captured.identity)
            .await
            .unwrap();
        drop(child_input);
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            let mut tick = tokio::time::interval(std::time::Duration::from_millis(10));
            while process.observe_engine_retirement(&retired).unwrap()
                != EngineRetirementStatus::Exited
            {
                tick.tick().await;
            }
        })
        .await
        .unwrap();
        drop(listener);
        let session = process.begin_owned_restore(&captured).await.unwrap();
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            process.restore_owned_session(&session, None, &|| {
                admitted.store(true, std::sync::atomic::Ordering::SeqCst);
                true
            }),
        )
        .await
        .unwrap();
        assert!(admitted.load(std::sync::atomic::Ordering::SeqCst));
        assert!(
            matches!(result, Err(LlamaServerError::Spawn(ref why)) if why.contains("exited during bring-up")),
            "actual non-model child must spawn and then fail readiness"
        );
        assert!(
            process.child.lock().unwrap().as_ref().is_some(),
            "shared spawn installed the child in the real owner"
        );
        assert!(
            process.owned_serving_target().is_none(),
            "failed readiness cannot mint a new restoration receipt"
        );
        // A subsequent generation prevents replay of the earlier receipt even
        // with a caller that says intent is current. Its callback is not run.
        admitted.store(false, std::sync::atomic::Ordering::SeqCst);
        let stale = process
            .restore_owned_launch(&captured, &|| {
                admitted.store(true, std::sync::atomic::Ordering::SeqCst);
                true
            })
            .await;
        assert!(matches!(stale, Err(LlamaServerError::Superseded)));
        assert!(!admitted.load(std::sync::atomic::Ordering::SeqCst));
        // The batch's original session retains its own failed-startup successor,
        // unlike a fresh replay of the old capture. Retry requires actual exit.
        let first_attempt = session.attempt_identity();
        let retired = process.retire_owned_engine(&first_attempt).await.unwrap();
        tokio::time::timeout(Duration::from_secs(10), async {
            let mut tick = tokio::time::interval(Duration::from_millis(10));
            while process.observe_engine_retirement(&retired).unwrap()
                != EngineRetirementStatus::Exited
            {
                tick.tick().await;
            }
        })
        .await
        .unwrap();
        let retry = tokio::time::timeout(
            Duration::from_secs(10),
            process.restore_owned_session(&session, None, &|| true),
        )
        .await
        .unwrap();
        assert!(
            matches!(retry, Err(LlamaServerError::Spawn(ref why)) if why.contains("exited during bring-up"))
        );
        assert!(
            session.attempt_identity() != first_attempt,
            "shared spawn alone advances retry authority"
        );
        let retired = process
            .retire_owned_engine(&session.attempt_identity())
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(10), async {
            let mut tick = tokio::time::interval(Duration::from_millis(10));
            while process.observe_engine_retirement(&retired).unwrap()
                != EngineRetirementStatus::Exited
            {
                tick.tick().await;
            }
        })
        .await
        .unwrap();
        let transition = endpoint.transition().await;
        let foreign_generation = transition.start_generation().unwrap();
        let mut foreign_child = command.spawn().unwrap();
        let foreign_input = foreign_child.stdin.take().unwrap();
        process.install_child(
            foreign_child,
            foreign_generation.clone(),
            prepared.engine_install.clone(),
            prepared.local_inputs.as_ref().ok().cloned(),
        );
        drop(transition);
        admitted.store(false, std::sync::atomic::Ordering::SeqCst);
        let foreign = process
            .restore_owned_session(&session, None, &|| {
                admitted.store(true, std::sync::atomic::Ordering::SeqCst);
                true
            })
            .await;
        assert!(matches!(foreign, Err(LlamaServerError::Superseded)));
        assert!(!admitted.load(std::sync::atomic::Ordering::SeqCst));

        // Drive the connected daemon transaction with this actual child and
        // original local inputs. The helper cancels AFTER checkpoint/exit;
        // where.exe is an actual restore spawn, never claimed model readiness.
        process.record_verified_target(
            &foreign_generation,
            &prepared.target,
            &prepared.invocation,
            Some(prepared.clone()),
            &process.engine_bin().unwrap(),
            11008,
            4,
        );
        let transition = endpoint.transition().await;
        transition
            .ready(
                &process.root,
                &foreign_generation,
                crate::inference::slots::KvPageContract {
                    model_id: prepared.target.model.id.clone(),
                    model: prepared.gguf.clone(),
                    adapters: vec![],
                    page_dir: Some(prepared.slot_save_dir.clone()),
                    context: 11008,
                    slots: 4,
                    cache_type: Some("q8_0".into()),
                    engine: process.engine_bin().unwrap(),
                    revisions: Some(vec![]),
                },
            )
            .unwrap();
        drop(transition);
        let pool = endpoint.admit().await.unwrap().pool.unwrap();
        let key = crate::inference::slots::ActivityKey::new(
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
        )
        .unwrap();
        let sem = Arc::new(tokio::sync::Semaphore::new(4));
        let mut turn = crate::inference::turn_admission::admit_turn(
            &sem,
            Some(key),
            Some(pool),
            &process.client,
            &process.root,
            10,
            std::time::Duration::ZERO,
        )
        .await
        .unwrap();
        turn.generation_completed();
        drop(turn);
        let saves = Arc::new(AtomicUsize::new(0));
        let app = axum::Router::new().route("/slots/{slot}", axum::routing::post({
            let saves = saves.clone();
            move |axum::extract::Path(slot): axum::extract::Path<u32>,
                axum::extract::Query(query): axum::extract::Query<std::collections::HashMap<String, String>>,
                axum::Json(body): axum::Json<serde_json::Value>| {
                let saves = saves.clone();
                async move {
                    assert_eq!(query.get("action").map(String::as_str), Some("save"));
                    saves.fetch_add(1, Ordering::SeqCst);
                    axum::Json(serde_json::json!({"id_slot":slot,"filename":body["filename"],"n_saved":10}))
                }
            }
        }));
        let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{port}"))
            .await
            .unwrap();
        let http = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let root = process.root.clone();
        let process = Arc::new(process);
        crate::modules::serving_daemon::tests::exercise_cancelled_exclusive_teacher(
            process.clone(),
            foreign_generation,
            &root,
            http,
            saves,
        )
        .await;
        drop(foreign_input);
        drop(process);
        collect_retired_engines();
    }
}

// what this catches: a debug llama-server hosting a lane silently (BigMama's
// 5090 served one for a day, 2026-09-04) — the door reads the server's own
// warning line; a release version line passes.
#[test]
fn a_debug_build_is_refused_at_the_door_and_a_release_build_passes() {
    assert!(super::is_debug_build("warning: DEBUG BUILD (asserts enabled) -- performance numbers from this process are not valid\nversion: 10229 (a28ee566c)"));
    assert!(!super::is_debug_build("version: 0.3.0-dev (build 10751, commit 0a637ba22)\nbuilt with AppleClang 21.0.0.21000101 for Darwin arm64"));
}

// what this catches: the work-scaled patience rule (2026-08-23, three
// MirrorCode-baseline kills): a lane holding a window-scale in-flight
// prompt earns DOUBLE the miss threshold (a kill destroys minutes of
// re-prefill and can loop); a quiet or small-investment lane keeps base
// patience and still dies fast. Relative quarter-window bound — no
// absolute token constant to drift.
#[test]
fn wedge_patience_scales_with_inflight_investment() {
    assert_eq!(wedge_miss_threshold(2, None, 65_024), 2, "no in-flight = base");
    assert_eq!(wedge_miss_threshold(2, Some(1_000), 65_024), 2, "small investment = base");
    assert_eq!(wedge_miss_threshold(2, Some(16_256), 65_024), 4, "quarter-window doubles");
    assert_eq!(wedge_miss_threshold(2, Some(82_668), 163_072), 4, "the live case doubles");
    assert_eq!(wedge_miss_threshold(2, Some(82_668), 0), 2, "unknown window = base, never a guess");
}

// what this catches (2026-08-15, round-killer #2 of the day): a never-started
// stream timeout stamped `note_real_decode_failure` unconditionally, so an
// OVERSUBSCRIBED lane (4 demand lanes, 1-2 slots, multi-minute thinking turns)
// hit the relaunch threshold from queue starvation alone — the health monitor
// killed a lane whose own log showed 11k-token completions at 17.5 t/s, every
// 2 minutes, forever. The classifier reads the lane's own delivery record:
// tokens for ANYONE within the wait = Starved (alive, no stamp); no delivery
// across the whole wait, or none ever observed = WedgeEvidence (stamp — the
// #363 undersized/dead class this counter exists for).
#[test]
fn never_started_timeout_on_a_delivering_lane_is_starvation_not_wedge_evidence() {
    use super::{classify_never_started_timeout, NeverStartedClass};
    // Busy lane: someone got tokens 30s into our 120s wait — alive.
    assert_eq!(
        classify_never_started_timeout(Some(30_000), 120_000),
        NeverStartedClass::Starved
    );
    // Dead lane: last delivery predates our whole wait — genuine evidence.
    assert_eq!(
        classify_never_started_timeout(Some(300_000), 120_000),
        NeverStartedClass::WedgeEvidence
    );
    // Fresh boot / unreadable clock: no delivery record — do not excuse the
    // timeout on faith; the wedge path (and its relaunch) is the safe read.
    assert_eq!(
        classify_never_started_timeout(None, 120_000),
        NeverStartedClass::WedgeEvidence
    );
    // Boundary: delivery exactly as old as the wait means nothing came out
    // DURING it — evidence, not starvation.
    assert_eq!(
        classify_never_started_timeout(Some(120_000), 120_000),
        NeverStartedClass::WedgeEvidence
    );
}

// what this catches (2026-08-16, the 23-minute boot kill-loop): a smoke miss on
// a lane doing GHOST WORK (a dead core's in-flight turns, no adapter stream →
// no L9 stamps) counted as wedge evidence, so 2 misses killed a lane llama's
// own log showed grinding flat out — three times in one boot. The verdict must
// exonerate a miss when the server's own /slots state ADVANCED between misses,
// keep a FROZEN fingerprint counting (the 2026-08-05 wedge signature holds one
// task at one impossible progress forever), never exonerate on a first miss
// (nothing to compare), and never exonerate on a fetch error.
#[test]
fn smoke_miss_on_an_advancing_lane_is_contention_not_wedge_evidence() {
    use super::{judge_smoke_miss, SmokeMissVerdict};
    // Slots advanced between the two misses: real work held the probe out.
    assert_eq!(
        judge_smoke_miss(Some(1), Some(2)),
        SmokeMissVerdict::AliveViaSlotProgress
    );
    // Frozen fingerprint: the wedge signature — count it.
    assert_eq!(judge_smoke_miss(Some(7), Some(7)), SmokeMissVerdict::CountMiss);
    // First miss: nothing to compare — count it (threshold ≥ 2 keeps the
    // detection latency identical to pre-L11).
    assert_eq!(judge_smoke_miss(None, Some(7)), SmokeMissVerdict::CountMiss);
    // /slots unreadable: a fetch error is never exoneration.
    assert_eq!(judge_smoke_miss(Some(7), None), SmokeMissVerdict::CountMiss);
    assert_eq!(judge_smoke_miss(None, None), SmokeMissVerdict::CountMiss);
}

// what this catches: the fingerprint must move with WORK (prefill advance, task
// turnover, decode count) and must NOT move with volatile per-request params —
// hashing the params blob would churn every poll and exonerate a dead lane
// forever. JSON shape pinned from a live /slots response (2026-08-16, :58057).
#[test]
fn slots_fingerprint_moves_with_work_and_ignores_params() {
    use super::slots_activity_fingerprint_of;
    let slot = |processed: u64, temp: f64| {
        serde_json::json!([{
            "id": 0, "n_ctx": 32512, "is_processing": true, "id_task": 55395,
            "n_prompt_tokens": 143, "n_prompt_tokens_processed": processed,
            "params": { "temperature": temp, "top_k": 20 }
        }])
    };
    let base = slots_activity_fingerprint_of(&slot(4, 0.0)).expect("array parses");
    // Prefill advanced → fingerprint moves.
    assert_ne!(
        base,
        slots_activity_fingerprint_of(&slot(90, 0.0)).unwrap(),
        "prefill advance must change the fingerprint"
    );
    // Only volatile params changed → fingerprint stable.
    assert_eq!(
        base,
        slots_activity_fingerprint_of(&slot(4, 0.9)).unwrap(),
        "params churn must NOT change the fingerprint"
    );
    // Non-array body (error page, wrong endpoint) → None, never a fake hash.
    assert_eq!(
        slots_activity_fingerprint_of(&serde_json::json!({"error": "nope"})),
        None
    );
}

// what this catches (2026-08-24, the 1-lane kill-loop): current llama-server
// wraps decode state in an ARRAY (`next_token: [{n_decoded: N}]`); the old
// string-key read returned None on it, so during a long DECODE (prefill done →
// `n_prompt_tokens_processed` frozen, one task → `id_task` frozen) the
// fingerprint was static and L11 judged a lane decoding at 31 tok/s as WEDGED —
// two smoke misses killed a 35k-token in-flight turn. Shape pinned from the live
// /slots response (:58057). Decode advance must move the fingerprint through
// BOTH the array shape and the older object shape.
#[test]
fn slots_fingerprint_sees_decode_through_the_array_shaped_next_token() {
    use super::slots_activity_fingerprint_of;
    let decoding = |n_decoded: u64, n_prompt: u64| {
        serde_json::json!([{
            "id": 0, "n_ctx": 166_400, "is_processing": true, "id_task": 350,
            "n_prompt_tokens": n_prompt, "n_prompt_tokens_processed": 35_144,
            "next_token": [{ "has_next_token": true, "n_remain": 697, "n_decoded": n_decoded }]
        }])
    };
    // Pure decode: processed/id_task static, only n_decoded (+ context growth) move.
    assert_ne!(
        slots_activity_fingerprint_of(&decoding(148, 47_211)).unwrap(),
        slots_activity_fingerprint_of(&decoding(400, 47_211)).unwrap(),
        "array-shaped n_decoded advance must change the fingerprint"
    );
    // Same story via the context-growth belt alone (build hides n_decoded).
    assert_ne!(
        slots_activity_fingerprint_of(&decoding(148, 47_211)).unwrap(),
        slots_activity_fingerprint_of(&decoding(148, 47_704)).unwrap(),
        "n_prompt_tokens growth during decode must change the fingerprint"
    );
    // Truly frozen (the 2026-08-05 4-hour wedge shape): identical looks stay equal.
    assert_eq!(
        slots_activity_fingerprint_of(&decoding(148, 47_211)).unwrap(),
        slots_activity_fingerprint_of(&decoding(148, 47_211)).unwrap(),
        "a frozen slot must keep a frozen fingerprint"
    );
    // Older builds: object-shaped next_token still read.
    let object_shape = |n: u64| {
        serde_json::json!([{
            "id": 0, "is_processing": true, "id_task": 1,
            "n_prompt_tokens": 100, "n_prompt_tokens_processed": 100,
            "next_token": { "n_decoded": n }
        }])
    };
    assert_ne!(
        slots_activity_fingerprint_of(&object_shape(5)).unwrap(),
        slots_activity_fingerprint_of(&object_shape(6)).unwrap(),
        "object-shaped n_decoded advance must change the fingerprint"
    );
}

// what this catches (2026-08-15, the round-killer): the smoke verdict judged a
// THINKING model's healthy decode as a dead lane because the whole quick budget
// went to the reasoning channel and `content` came back empty — serving was
// marked degraded forever and every turn waited on a transition that never
// resolved. The verdict must read that shape as ThinkStarved (retry), keep the
// wedged-lane shape (~2 tokens) Dead, keep the embedding-model-on-the-port
// shape (garbage in the VISIBLE channel, no reasoning) Dead, and pass a real
// answer regardless of whether reasoning is present.
#[test]
fn smoke_verdict_separates_thinking_from_wedged_from_garbage() {
    use super::{smoke_verdict, SmokeVerdict};
    // Healthy non-thinking model: answer in the visible channel.
    assert_eq!(
        smoke_verdict(20, "1 2 3 4 5 6 7 8 9 10", 0),
        SmokeVerdict::Alive
    );
    // Healthy THINKING model after the big-budget retry: reasoning present AND
    // the visible answer counts — reasoning must not disqualify a real answer.
    assert_eq!(
        smoke_verdict(120, "1 2 3 4 5 6 7 8 9 10", 400),
        SmokeVerdict::Alive
    );
    // Thinking model starved by the quick budget: decode proven, all of it
    // private, visible empty — retry, never "dead lane".
    assert_eq!(smoke_verdict(24, "", 300), SmokeVerdict::ThinkStarved);
    assert_eq!(smoke_verdict(24, "   ", 300), SmokeVerdict::ThinkStarved);
    // Wedged lane: ~2 tokens then stop. Dead on either budget.
    assert_eq!(smoke_verdict(2, "", 0), SmokeVerdict::Dead);
    assert_eq!(smoke_verdict(2, "", 50), SmokeVerdict::Dead);
    // Embedding model bound to the port: garbage in the VISIBLE channel, no
    // reasoning channel at all (the BigMama incident) — Dead, never a retry.
    assert_eq!(
        smoke_verdict(24, "user interface interface UIUUIUUIuiui", 0),
        SmokeVerdict::Dead
    );
    // Empty visible with NO reasoning: nothing proves a mind is attached.
    assert_eq!(smoke_verdict(24, "", 0), SmokeVerdict::Dead);
}

/// what this catches (2026-08-11, the solve-prelude keyhole): the settle wait
/// must resolve on the FIRST ready snapshot whose layout differs from the
/// pre-quiesce one (event-gated on the watch, no polling), and a planner that
/// keeps its layout must end as `Unchanged` at the bound — a legitimate
/// outcome that proceeds, never a park. A non-ready layout change must NOT
/// resolve it (mid-relaunch snapshots are transient, not a settled plan).
#[tokio::test]
async fn await_snapshot_resettle_resolves_on_ready_layout_change_and_bounds_out_otherwise() {
    use super::{await_snapshot_resettle, ServingSnapshot, SnapshotSettle};
    use std::time::Duration;

    // Resettle: a ready snapshot with a NEW layout resolves the wait.
    let (tx, rx) = tokio::sync::watch::channel(ServingSnapshot::empty());
    let waiter = tokio::spawn(await_snapshot_resettle(
        rx,
        4,
        16384,
        Duration::from_secs(5),
    ));
    // Transient mid-relaunch publish (not ready) must be ignored…
    let mut transitional = ServingSnapshot::empty();
    transitional.lanes = 1;
    transitional.served_context_window = 32768;
    transitional.ready = false;
    tx.send(transitional.clone()).unwrap();
    // …and the settled ready layout resolves it.
    transitional.ready = true;
    tx.send(transitional).unwrap();
    assert_eq!(
        waiter.await.unwrap(),
        SnapshotSettle::Resettled {
            lanes: 1,
            window: 32768
        }
    );

    // Unchanged, EARLY: two ready publishes with the same layout mean the
    // planner ran and held — resolves well before the (long) bound, so a
    // no-change solve pays reconcile-tick latency, never the full backstop.
    let (tx2, rx2) = tokio::sync::watch::channel(ServingSnapshot::empty());
    let waiter2 = tokio::spawn(await_snapshot_resettle(
        rx2,
        4,
        16384,
        Duration::from_secs(30),
    ));
    let mut same = ServingSnapshot::empty();
    same.lanes = 4;
    same.served_context_window = 16384;
    same.ready = true;
    tx2.send(same.clone()).unwrap();
    tokio::task::yield_now().await;
    tx2.send(same).unwrap();
    let early = tokio::time::timeout(Duration::from_secs(2), waiter2)
        .await
        .expect("two unchanged ready publishes must resolve early, not ride the bound")
        .unwrap();
    assert_eq!(early, SnapshotSettle::Unchanged);

    // Unchanged, BACKSTOP: a daemon that stops publishing ends at the bound.
    let (_tx3, rx3) = tokio::sync::watch::channel(ServingSnapshot::empty());
    let waiter3 = tokio::spawn(await_snapshot_resettle(
        rx3,
        4,
        16384,
        Duration::from_millis(80),
    ));
    assert_eq!(waiter3.await.unwrap(), SnapshotSettle::Unchanged);
}

/// what this catches (#350): the two states an EMPTY serving snapshot can mean
/// collapsing back into one. `ServingSnapshot::empty()` is published from process
/// start (install_serving_state runs in the daemon's `initialize()`, before the first
/// reconcile), so "we have not looked yet" and "we looked and nothing is serving" are
/// byte-identical. Readers that cannot tell them apart shout about a serving fault
/// during every boot: measured 116 false alarms in 38 bursts over 3 days.
///
/// CORRECTION (2026-08-07) to what this comment first claimed. It said the daemon
/// "published `active=<none>` ZERO times in 12 hours", which read as: empty means boot,
/// full stop. It does not. `serving_daemon` publishes `empty()` on EVERY lane teardown
/// — no servable plan, a re-home, and `declare_lane_wedged` (#175 self-heal) — the
/// original count simply had no probe on those publish sites to see them. Live receipts
/// the same day: three personas read `serving: <none>` 59s into a wedge relaunch, 350s
/// after this latch was set. So `has_reconciled()` answers "has a lane EVER come up",
/// never "is one up now" — for the latter, read `ServingSnapshot::ready` live.
///
/// The empty snapshot deliberately stays unchanged — it is still the honest "nothing
/// live" value. What must exist is a SEPARATE signal for whether anyone has looked.
#[test]
fn an_empty_snapshot_cannot_itself_distinguish_startup_from_a_serving_fault() {
    let boot = super::ServingSnapshot::empty();
    assert_eq!(boot.active_model, None);
    assert!(!boot.ready);
    // The point: this value is IDENTICAL whether the daemon has reconciled or not,
    // which is exactly why `has_reconciled()` is a separate signal and not something
    // a reader can infer from the snapshot. If someone later adds a field that makes
    // the two distinguishable here, this assertion documents why they must ALSO keep
    // `has_reconciled()` correct rather than silently replacing it.
    assert_eq!(boot, super::ServingSnapshot::empty());
}

/// what this catches: `mark_first_reconcile` becoming non-idempotent or
/// `has_reconciled` reading the wrong cell. A OnceLock set twice must not panic —
/// the daemon calls it on EVERY reconcile, not just the first.
///
/// NOTE this test can only observe the post-mark state: `FIRST_RECONCILE` is a
/// process-global OnceLock, so once any test in this binary marks it, it stays
/// marked. Asserting `!has_reconciled()` first would be order-dependent — the exact
/// flake shape that made tests pass locally and fail in CI
/// ([[a-process-global-read-inside-a-decision-makes-tests-order-dependent]]).
#[test]
fn marking_the_first_reconcile_is_idempotent_and_observable() {
    super::mark_first_reconcile();
    assert!(super::has_reconciled());
    super::mark_first_reconcile(); // must not panic
    assert!(super::has_reconciled());
}

/// what this catches: the health heartbeat going back to probe-always. `decode_smoke_ok`
/// is a REAL generation through the live slots, so on a saturated lane it cannot get one,
/// the miss counts as "no decode", and two misses relaunch a lane that is merely busy —
/// which is what killed SWE run v13 (`serving.health {ok:false} x2 -> relaunch`, then every
/// generate refused with `serving: <none>`). Real tokens are the honest liveness evidence;
/// this pins the accessor the heartbeat gates on.
#[test]
fn real_token_delivery_is_recorded_as_liveness_evidence() {
    // Fresh process: nothing observed yet ⇒ None, so the caller falls through and probes
    // (a boot with no traffic must still be verified — absence of evidence is not health).
    // NB: process-global, so only assert the post-record contract if a prior test recorded.
    note_real_decode();
    let since = ms_since_real_decode().expect("a recorded decode must be visible");
    assert!(
        since < 5_000,
        "a decode recorded just now must read as recent, got {since}ms"
    );
}

use super::*;
use super::reconcile::{grow_refused_by_memory_gate, lane_grow_would_narrow_the_window};
use std::sync::atomic::{AtomicUsize, Ordering};

// what this catches: the decode smoke-probe passing a lane that cannot chat at all. The gate
// used to assert only `completion_tokens >= 5`, which measures VOLUME, not correctness — so
// when an EMBEDDING model was bound to the port the persona lane was configured for
// (BigMama 2026-08-04, qwen3-embedding-0.6b on :8090), it sailed through and every persona on
// the node "thought" through it. The literal reply below is the real one that passed.
//
// Mutation checks: accepting any digit instead of a run of three passes the embedding string
// ("UIUUIU..." has none, but "1 interface 7 ui 3" would); requiring exact formatting fails the
// legitimate preamble/comma cases a real model produces.
#[test]
fn decode_probe_content_check_rejects_a_lane_that_cannot_count() {
    // The measured failure — fluent-looking noise, well past any token-count gate.
    assert!(
        !counts_upward("user interface interface user interface UIUUIUUIuiuiuiuiuiuiuiui"),
        "the embedding model's real reply must NOT pass the decode probe"
    );
    // Scattered digits are not comprehension.
    assert!(!counts_upward("7 interface 3 ui 9"));
    assert!(!counts_upward("1 1 1 1"));
    assert!(!counts_upward(""));
    // Healthy answers, in the shapes real models actually emit.
    assert!(counts_upward("1 2 3 4 5 6 7 8 9 10"));
    assert!(counts_upward("Sure! 1, 2, 3, 4, 5"));
    assert!(counts_upward("1\n2\n3"));
    // A truncated-but-correct answer (max_tokens cut it off) is still healthy.
    assert!(counts_upward("1 2 3 4"));
}

// what this catches (#205 unmask, glass-boxed 2026-07-28): an ephemeral eval lane
// whose child was OS-killed (SIGKILL/137) before it emitted anything produced an
// EMPTY stderr log and masked as a bare 240s /health timeout. `tail_or_hang_marker`
// must turn an empty log into the "no output before it stopped" fingerprint naming
// the OOM-kill vs. early-init-hang split (so the error names the real memory-wall
// failure mode) while a non-empty log yields its last lines in order. Mutation
// checks: dropping the empty-branch fails the OOM/jetsam assert; a non-reversed tail
// fails the ordering assert.
// what this catches (the 5090, 2026-10-06): a verified new engine whose first load
// listened at 92s was declared failed at the 90s live budget, and the slot rollback put
// the lanes back on the old engine. A server of ours saying "loading" earns the cold-load
// ceiling; every other state keeps the tight budget, so a wedge still fails fast.
#[test]
fn a_loading_engine_of_ours_is_not_a_failed_one() {
    assert_eq!(ready_budget(READY_TIMEOUT, true), EPHEMERAL_READY_TIMEOUT);
    assert!(
        ready_budget(READY_TIMEOUT, true) > Duration::from_secs(92),
        "the measured first load fits"
    );
    assert_eq!(
        ready_budget(READY_TIMEOUT, false),
        READY_TIMEOUT,
        "silence or an adopted process keeps the tight budget"
    );
    assert_eq!(
        ready_budget(EPHEMERAL_READY_TIMEOUT, true),
        EPHEMERAL_READY_TIMEOUT,
        "never shorter than the caller's own budget"
    );
    // Regression: total cold load may exceed 240s while measured work advances.
    // Repeated 503, stage labels, wrong children and warmup silence earn no time.
    assert!(
        serde_json::from_str::<LoadingHealth>(r#"{"error":{"code":503}}"#)
            .expect("older engine health shape")
            .load_progress
            .is_none()
    );
    assert!(
        serde_json::from_str::<LoadingHealth>(r#"{"load_progress":{"pid":"wrong"}}"#).is_err()
    );
    let mut progress = ReadyLoadProgress::default();
    let sample = |pid, current, value| {
        serde_json::from_value::<LoadProgressSample>(serde_json::json!({
            "pid": pid, "stages": ["weights", "vision"], "current": current, "value": value,
        }))
        .expect("native snapshot shape")
    };
    let seconds = Duration::from_secs;
    assert!(!progress.observe(&sample(7, "weights", 0.0), 7, seconds(100)));
    assert!(progress.stalled(seconds(240), EPHEMERAL_READY_TIMEOUT));
    assert!(progress.observe(&sample(7, "weights", 0.5), 7, seconds(200)));
    assert!(!progress.stalled(seconds(300), EPHEMERAL_READY_TIMEOUT));
    assert!(!progress.observe(&sample(7, "weights", 0.5), 7, seconds(300)));
    assert!(!progress.observe(&sample(7, "weights", 0.4), 7, seconds(300)));
    assert!(!progress.observe(&sample(8, "weights", 1.0), 7, seconds(300)));
    assert!(!progress.observe(&sample(7, "vision", 0.0), 7, seconds(300)));
    assert!(!progress.observe(&sample(7, "vision", 1.1), 7, seconds(300)));
    let mut changed = sample(7, "vision", 0.5);
    changed.stages.reverse();
    assert!(!progress.observe(&changed, 7, seconds(300)));
    assert_eq!(
        progress.last_advanced,
        seconds(200),
        "rejected samples cannot renew the allowance"
    );
    assert!(progress.observe(&sample(7, "vision", 1.0), 7, seconds(400)));
    assert!(!progress.stalled(seconds(639), EPHEMERAL_READY_TIMEOUT));
    assert!(
        progress.stalled(seconds(640), EPHEMERAL_READY_TIMEOUT),
        "unmeasured warmup stays bounded"
    );
}

#[test]
fn tail_or_hang_marker_fingerprints_empty_and_tails_nonempty() {
    assert!(
        tail_or_hang_marker("").contains("OOM/jetsam"),
        "an empty log is the no-output fingerprint naming the OOM-kill cause, not a silent blank"
    );
    assert!(
        tail_or_hang_marker("   \n\n \t \n").contains("OOM/jetsam"),
        "whitespace-only counts as empty"
    );
    let log = (1..=30)
        .map(|i| format!("line{i}"))
        .collect::<Vec<_>>()
        .join("\n");
    let tail = tail_or_hang_marker(&log);
    assert!(
        tail.contains("line30") && tail.contains("line11") && !tail.contains("line10"),
        "keeps the LAST 20 lines in original order, not the first: {tail}"
    );
    // sanity: order preserved (line11 appears before line30)
    assert!(tail.find("line11") < tail.find("line30"));
}

// what this catches: the slice-1 K3 -ot builder — the LAYER-granular buft-override that
// physically pages MoE experts. Cold = (0..n_layers) minus hot; the value keys on the
// real blk.N with `\.` anchors (so blk.3 never matches blk.31), covers all four stacked
// ffn_*_exps projections, and targets CPU. None when nothing is cold (empty -ot is
// rejected by llama-server). A regression here = the wrong experts offloaded (a stale
// plan degrading throughput) or a spawn that 500s on a malformed override.
#[test]
fn cold_expert_offload_ot_emits_inverse_of_hot_layers() {
    // Mixed hot/cold: layers 0,2,4 fit GPU → 1,3,5 offload to CPU.
    let v = cold_expert_offload_ot(&[0, 2, 4], 6).expect("some cold → an override");
    assert_eq!(v, r"blk\.(1|3|5)\.ffn.*_exps=CPU");
    // Anchoring intent: the block index is fenced by `\.` on both sides.
    assert!(
        v.contains(r"blk\.(1|3|5)\.ffn"),
        "block index must be dot-anchored"
    );

    // Everything hot → no override at all (NOT an empty string, which llama-server rejects).
    assert_eq!(cold_expert_offload_ot(&[0, 1, 2, 3], 4), None);
    // Zero layers → nothing to place.
    assert_eq!(cold_expert_offload_ot(&[], 0), None);
    // Empty hot set → offload every block (the all-cold extreme).
    assert_eq!(
        cold_expert_offload_ot(&[], 3),
        Some(r"blk\.(0|1|2)\.ffn.*_exps=CPU".to_string())
    );
    // A hot index outside the block range is ignored, never forcing a bogus pattern.
    assert_eq!(
        cold_expert_offload_ot(&[99], 2),
        Some(r"blk\.(0|1)\.ffn.*_exps=CPU".to_string())
    );
}

/// Fake control: scripted probe result + a counter of serve() calls, so the
/// pure reconcile decision is tested without a live process.
struct FakeControl {
    probe: Result<Option<String>, &'static str>,
    verified_launch: Option<OwnedServingTarget>,
    /// The genome set the fake reports as currently loaded (sorted paths).
    active_adapters: Vec<String>,
    serve_ok: bool,
    serves: AtomicUsize,
    /// Whether the fake's decode smoke-probe succeeds. `true` = a healthy lane
    /// (adoptable); `false` = a compute-wedged orphan (must be rejected).
    decode_ok: bool,
    /// `Some(_)` = the fake's `/slots` fingerprint ADVANCES on every look (a busy
    /// lane); `None` = no slots account (the default: no exoneration, as before).
    slots_busy: Option<AtomicUsize>,
    /// The KV decision the fake's running engine was launched with; `None` = unknown
    /// (the default, so every pre-existing case adopts on the other axes alone).
    launched_kv: Option<crate::inference::lane_process::LaunchKv>,
    /// Whether the fake "owns" the running child (we spawned it). `false` =
    /// an adopted orphan (the conservative default that exercises the
    /// smoke-probe gate).
    owns: bool,
    /// The slot count the fake's `/props` reports (`total_slots`). Defaults to
    /// 0 = "the server named no slot count, nothing to compare", which is the
    /// same lanes-OK verdict these tests had before the lane operand was probed
    /// — so every pre-existing case still exercises the WINDOW axis alone.
    served_lanes: u32,
    /// The per-slot window the fake's `/props` reports. Defaults to the tests'
    /// target window so the window-grow relaunch check (which only fires when
    /// target > served + tolerance) is a no-op for the model/adapter/decode
    /// tests; a grow-relaunch test sets it BELOW the target explicitly.
    served_window: u32,
    /// What an in-place genome change comes to. Defaults to `Unsupported`, the
    /// engine every pre-S1 test modelled: a genome change relaunches.
    in_place: GenomeInPlace,
}

impl FakeControl {
    fn probe(probe: Result<Option<String>, &'static str>) -> Self {
        Self {
            probe,
            verified_launch: None,
            active_adapters: Vec::new(),
            serve_ok: true,
            serves: AtomicUsize::new(0),
            decode_ok: true,
            slots_busy: None,
            launched_kv: None,
            owns: false,
            served_lanes: 0,
            served_window: 32768,
            in_place: GenomeInPlace::Unsupported,
        }
    }
    fn with_in_place(mut self, outcome: GenomeInPlace) -> Self {
        self.in_place = outcome;
        self
    }
    /// Model a lane whose live per-slot window is SMALLER than the plan target —
    /// the starved boot-floor case the window-grow relaunch must catch.
    fn with_served_window(mut self, n: u32) -> Self {
        self.served_window = n;
        self
    }
    /// Model a lane whose live `/props total_slots` is what it is — the LANE
    /// axis of the same grow-vs-adopt decision. Used to pin that a past form of
    /// ourself already serving enough slots is ADOPTED, not reaped.
    fn with_served_lanes(mut self, n: u32) -> Self {
        self.served_lanes = n;
        self
    }
    fn serve_fails(mut self) -> Self {
        self.serve_ok = false;
        self
    }
    /// Report a genome set as already loaded (the live server's catalog).
    fn with_active_adapters(mut self, adapters: Vec<String>) -> Self {
        self.active_adapters = adapters;
        self
    }
    /// Model the compute-wedged orphan: answers `/v1/models` but every decode
    /// 500s.
    fn decode_wedged(mut self) -> Self {
        self.decode_ok = false;
        self
    }
    /// Model a lane whose `/slots` account advances on every look — a busy lane
    /// (mid-prefill for a client this core never knew, or under a build's load).
    fn slots_busy(mut self) -> Self {
        self.slots_busy = Some(AtomicUsize::new(0));
        self
    }
    /// Model an engine launched with this KV decision (read off its argv in production).
    fn launched_with(mut self, cache_type: Option<&str>, flash_attn: bool) -> Self {
        self.launched_kv = Some(crate::inference::lane_process::LaunchKv { cache_type: cache_type.map(str::to_string), flash_attn, cache_ram_mib: None });
        self
    }
    /// Model an engine launched on THIS KV decision with a `--cache-ram` grant of `mib`.
    fn launched_with_cache(mut self, mib: u32) -> Self {
        let planned = crate::cognition::kv_cache_plan::resolve();
        self.launched_kv = Some(crate::inference::lane_process::LaunchKv {
            cache_type: planned.launcher_cache_type().map(str::to_string),
            flash_attn: planned.flash_attn,
            cache_ram_mib: Some(mib),
        });
        self
    }
    /// Model a child we spawned ourselves (trusted without a per-tick probe).
    fn owned(mut self) -> Self {
        self.owns = true;
        self
    }
}

#[async_trait]
impl LlamaServerControl for FakeControl {
    fn owned_serving_target(&self) -> Option<OwnedServingTarget> {
        self.verified_launch.clone()
    }
    fn owned_engine(&self) -> Option<OwnedEngineIdentity> {
        self.verified_launch
            .as_ref()
            .map(|launch| launch.identity.clone())
    }
    async fn active_model(&self) -> Result<Option<String>, LlamaServerError> {
        match &self.probe {
            Ok(v) => Ok(v.clone()),
            Err(m) => Err(LlamaServerError::Unreachable(m.to_string())),
        }
    }
    async fn active_adapters(&self) -> Result<Vec<String>, LlamaServerError> {
        Ok(self.active_adapters.clone())
    }
    async fn adopt_genome_in_place(
        &self,
        _target: &ServingTarget,
    ) -> Result<GenomeInPlace, LlamaServerError> {
        Ok(self.in_place)
    }
    async fn serve(&self, _target: &ServingTarget) -> Result<(), LlamaServerError> {
        self.serves.fetch_add(1, Ordering::SeqCst);
        if self.serve_ok {
            Ok(())
        } else {
            Err(LlamaServerError::Spawn("boom".into()))
        }
    }
    async fn served_context_window(&self) -> Result<u32, LlamaServerError> {
        // The per-slot window the fake's /props reports (configurable via
        // `with_served_window`). Non-zero so a ready outcome publishes a ready
        // snapshot; defaults to the target window so the window-grow relaunch
        // check is a no-op unless a test deliberately floors it.
        Ok(self.served_window)
    }
    async fn served_lanes(&self) -> Result<u32, LlamaServerError> {
        // The slot count the fake's /props reports (configurable via
        // `with_served_lanes`). Defaults to 0 = "nothing to compare", so the
        // lane axis is inert unless a test deliberately sets it.
        Ok(self.served_lanes)
    }
    async fn decode_smoke_ok(&self) -> bool {
        self.decode_ok
    }
    async fn slots_activity_fingerprint(&self) -> Option<u64> {
        self.slots_busy
            .as_ref()
            .map(|n| n.fetch_add(1, Ordering::SeqCst) as u64)
    }
    fn launch_kv(&self) -> Option<crate::inference::lane_process::LaunchKv> {
        self.launched_kv.clone()
    }
    fn owns_child(&self) -> bool {
        self.owns
    }
}

/// Minimal serving target for the reconcile-logic tests: only `model.id` is
/// load-bearing here (it drives the relaunch decision); the rest is filler so
/// the grouped `Model` constructs. The real launcher path is covered by the
/// live serving daemon, not these pure-decision tests.
fn target(id: &str) -> ServingTarget {
    use crate::model_registry::types::{Arch, MultiPartyChatStrategy};
    ServingTarget {
        host_prompt_cache_mib: crate::inference::lane_args::CACHE_RAM_MIB,
        model: Model {
            weights_bytes: None,
            mmproj_bytes: None,
            id: id.to_string(),
            name: None,
            provider: "llamacpp-local".to_string(),
            arch: Arch::Qwen2,
            context_window: 32768,
            max_output_tokens: 4096,
            tokens_per_second: 0.0,
            capabilities: std::collections::BTreeSet::new(),
            cost_input_per_1k: 0.0,
            cost_output_per_1k: 0.0,
            gguf_hint: None,
            hf_source: None,
            gguf_local_path: None,
            chat_template: None,
            stop_sequences: Vec::new(),
            multi_party_strategy: MultiPartyChatStrategy::ProperChatMlSingleParty,
            mmproj_local_path: None,
            parameter_count: 0,
            sampling: crate::model_registry::types::ModelSampling::default(),
            persona_serving_eligible: true,
            serving: Default::default(), // test/fixture literal: substrate defaults (text-only main lane, unverified kv-shift)
        },
        context_window: 32768,
        lanes: 1,
        adapters: Vec::new(),
        placement: LanePlacement::Gpu,
        expert_placement: None,
        resident_override: None,
        vision_sidecar: false,
    }
}

/// Same as [`target`] but with a genome set loaded — for the relaunch-on-
/// genome-change tests. Paths are the load identity the reconcile compares.
fn target_with_adapters(id: &str, paths: &[&str]) -> ServingTarget {
    let mut t = target(id);
    t.adapters = paths
        .iter()
        .map(|p| AdapterEntry {
            alias: format!("gene-{p}"),
            path: PathBuf::from(p),
        })
        .collect();
    t
}

// what this catches: the per-lane → total `-c` arithmetic. llama-server's
// `-c` is the WHOLE KV cache split across `--parallel` slots, so to give each
// of `lanes` slots a full per-lane `context_window` the spawn must request
// `context_window * lanes` and pass `--parallel lanes`. Launching `-c
// context_window` alone let llama.cpp default to 4 slots and quarter the
// per-request window, 500-ing live deliberations ("Context size exceeded").
// Regression here silently shrinks every persona's real prompt budget.
#[test]
fn served_total_ctx_is_per_lane_window_times_lanes() {
    let mut t = target("coder-4b");
    t.context_window = 16_384;
    t.lanes = 4;
    assert_eq!(t.parallel_lanes(), 4);
    assert_eq!(
        t.served_total_ctx(),
        65_536,
        "-c must be per-lane window × lanes so each slot holds one full window"
    );

    // lanes floored at 1: a single-lane plan serves the full window to one slot.
    t.lanes = 1;
    assert_eq!(t.served_total_ctx(), 16_384);

    // defensive floor: a zero lane count never collapses the cache to nothing.
    t.lanes = 0;
    assert_eq!(t.parallel_lanes(), 1);
    assert_eq!(t.served_total_ctx(), 16_384);
}

// what this catches: the desired model already being served is a NO-OP — we
// must not relaunch (which would kill the GPU-warm server). Regression here
// would thrash the served model every tick.
#[tokio::test]
async fn already_serving_does_not_relaunch() {
    let ctrl = FakeControl::probe(Ok(Some("coder-14b".into())));
    let outcome = ensure_model_serving(&ctrl, &target("coder-14b"), false).await;
    assert_eq!(outcome, EnsureOutcome::AlreadyServing);
    assert_eq!(
        ctrl.serves.load(Ordering::SeqCst),
        0,
        "no relaunch when already serving"
    );
}

// what this catches: a same-model / same-genome lane whose live per-slot window
// is STARVED far below the plan target (the 2048 boot-floor vs a 31k plan) must
// RELAUNCH to grow — llama.cpp has no hot-resize. Regression here (returning
// AlreadyServing on a genome-set match alone) strands the lane at the boot floor
// forever while the daemon logs "re-homing starved" every tick and never grows
// it — glass-boxed 2026-07-20, blocked all benchmark runs (prompts overflowed
// 2048). The daemon's `starved` gate only sends a target ≥ 2× the served window,
// so the grow is unambiguous.
#[tokio::test]
async fn starved_window_relaunches_to_grow() {
    // Owned + decode-healthy: the ONLY thing wrong is the window (2048 ≪ 32768).
    let ctrl = FakeControl::probe(Ok(Some("coder-14b".into())))
        .owned()
        .with_served_window(2048);
    let outcome = ensure_model_serving(&ctrl, &target("coder-14b"), false).await;
    assert!(
        matches!(outcome, EnsureOutcome::Spawned { .. }),
        "a starved window must relaunch to grow, got {outcome:?}"
    );
    assert_eq!(
        ctrl.serves.load(Ordering::SeqCst),
        1,
        "exactly one relaunch to the larger window"
    );
}

// what this catches: THE 2026-09-03 SLOW-RESUME defect — a warm lane REAPED for a
// trivial window drift. On reboot the boot plan re-computes the window against live
// memory, which climbs ~7% once the old core frees its share (measured 147712 →
// 158107). A flat 512-token tolerance (0.35% of a 147k lane) tripped a full ~7-10
// min 35B RELAUNCH on EVERY restart — the dominant "resume is slow" cost. Tolerance
// is now a percentage (max(512, served/8) ≈ 12.5%), so a warm lane within ~12.5% of
// the target is ADOPTED (warm-now beats cold-in-10-min); a genuinely starved lane
// (the 2× case above) still relaunches, and the runtime re-home grows a persistent
// gain later.
#[tokio::test]
async fn a_small_window_drift_adopts_the_warm_lane_not_a_slow_reload() {
    // Owned + decode-healthy, serving 147712; the boot plan drifted to 158107 (~7%).
    let ctrl = FakeControl::probe(Ok(Some("coder-14b".into())))
        .owned()
        .with_served_window(147_712);
    let mut t = target("coder-14b");
    t.context_window = 158_107;
    let outcome = ensure_model_serving(&ctrl, &t, false).await;
    assert_eq!(
        outcome,
        EnsureOutcome::AlreadyServing,
        "a ~7% window drift must ADOPT the warm lane, not reload it; got {outcome:?}"
    );
    assert_eq!(
        ctrl.serves.load(Ordering::SeqCst),
        0,
        "zero relaunches — the warm 35B lane is reused, not reloaded"
    );
}

// what this catches: THE 2026-08-19 DEFECT — a PAST FORM OF OURSELF being reaped
// instead of adopted. A lane this core did not spawn (crash survivor on the
// canonical port, named in our own custody record) already serves 4 slots at the
// target window. It must be ADOPTED with zero relaunches. Regression here — reading
// the lane count from `current_serving()` instead of the lane's own `/props` — makes
// a fresh core see 0 lanes for a lane it didn't spawn, decline, reap, and rebuild:
// measured live at ~110s of downtime, returning at 3 lanes and a SMALLER window than
// the lane that was already running.
#[tokio::test]
async fn a_past_form_of_ourself_serving_enough_lanes_is_adopted_not_reaped() {
    // NOT owned (we did not spawn it) + decode-healthy + already at the target
    // shape: the exact live crash-survivor case.
    let ctrl = FakeControl::probe(Ok(Some("coder-14b".into())))
        .with_served_lanes(4)
        .with_served_window(32768);
    let mut t = target("coder-14b");
    t.lanes = 4;
    let outcome = ensure_model_serving(&ctrl, &t, false).await;
    assert_eq!(
        outcome,
        EnsureOutcome::AlreadyServing,
        "a healthy past form of ourself already at the target shape must be adopted"
    );
    assert_eq!(
        ctrl.serves.load(Ordering::SeqCst),
        0,
        "adoption is FREE — reaping and reloading a lane that already fits is the bug"
    );
}

// what this catches: the fix above must not disable lane grow-back (#214's sibling).
// A lane genuinely serving FEWER slots than the plan wants still has to relaunch —
// llama.cpp cannot hot-resize n_seq_max. Without this, "adopt more eagerly" would
// silently freeze every citizen at the squeezed slot count forever.
#[tokio::test]
async fn a_lane_serving_fewer_slots_than_planned_still_relaunches_to_grow() {
    let ctrl = FakeControl::probe(Ok(Some("coder-14b".into())))
        .owned()
        .with_served_lanes(1)
        .with_served_window(32768); // window is FINE — only the lane count is short
    let mut t = target("coder-14b");
    t.lanes = 4;
    let outcome = ensure_model_serving(&ctrl, &t, false).await;
    assert!(
        matches!(outcome, EnsureOutcome::Spawned { .. }),
        "1 served slot against a 4-lane plan must relaunch to grow, got {outcome:?}"
    );
}

// what this catches (card d9d747b2, the M5 2026-09-21 09:29-09:36Z): a plan wanting
// MORE lanes at a NARROWER window than the lane serves (5 × 36,810 over 3 × 78,592)
// must not relaunch — following it thrashed the engine every ~3 min against the
// window grow-back and reaped 35 generations in 20 min. Both directions: the trade is
// refused and the lane stays; a pure lane grow at the served window still relaunches
// (the test above), and a WIDER window at fewer lanes still relaunches (the window is
// the requirement, lanes are throughput).
// what this catches: the grow ladder allocating through a CLOSED memory gate. The M5
// settled at 8 lanes × 35,328 during 39 consecutive Critical samples (2026-09-23), and
// fseventsd has twice held over a quarter of the machine. A grow on either axis is
// refused while the gate is closed; a decision-only relaunch (KV type, sight) is not,
// and nothing is refused while the gate is open.
#[test]
fn a_closed_memory_gate_refuses_a_lane_or_window_grow() {
    assert!(grow_refused_by_memory_gate(true, true, false, true), "more lanes through a closed gate");
    assert!(grow_refused_by_memory_gate(true, false, true, true), "a wider window through a closed gate");
    assert!(grow_refused_by_memory_gate(true, false, false, true), "both");
    assert!(grow_refused_by_memory_gate(true, true, true, false), "a bigger host cache through a closed gate (card d7294f0b)");
    assert!(!grow_refused_by_memory_gate(true, true, true, true), "geometry matches: a KV/sight relaunch is not a grow");
    assert!(!grow_refused_by_memory_gate(false, false, false, false), "an open gate refuses nothing");
}

#[tokio::test]
async fn a_lane_grow_that_would_narrow_the_window_is_refused_as_a_trade() {
    // The predicate, at the measured numbers and at the edges.
    assert!(lane_grow_would_narrow_the_window(5, 36_810, 3, 78_592), "the M5 loop's first step");
    assert!(lane_grow_would_narrow_the_window(4, 34_176, 3, 78_592), "and its second");
    assert!(!lane_grow_would_narrow_the_window(3, 78_592, 5, 36_810), "wider at fewer lanes is a grow-back");
    assert!(!lane_grow_would_narrow_the_window(5, 78_592, 3, 78_592), "a pure lane grow");
    assert!(!lane_grow_would_narrow_the_window(5, 70_000, 3, 78_592), "within the window tolerance (78,592 / 8)");
    assert!(!lane_grow_would_narrow_the_window(3, 31_000, 1, 2_048), "the 2k → 31k starved case grows both axes");
    assert!(!lane_grow_would_narrow_the_window(5, 36_810, 0, 0), "an unreadable served side is not a trade");
    // Fable's 45 s time series on the M5 (09:28-09:42Z): the DOWN steps are crash
    // respawns under a trough budget, not this ladder (grow-only), and from any of
    // them a wider plan must still climb out — lanes up or down.
    assert!(!lane_grow_would_narrow_the_window(4, 34_176, 5, 36_810), "fewer lanes is never a refused trade");
    assert!(!lane_grow_would_narrow_the_window(1, 19_173, 4, 34_176));
    assert!(!lane_grow_would_narrow_the_window(3, 78_592, 1, 19_173), "out of the bottom: wider at more lanes");
    assert!(!lane_grow_would_narrow_the_window(2, 31_141, 1, 19_173), "out of the bottom: wider at more lanes");

    // The ladder on the fake control: served 3 × 78,592, plan 5 × 36,810 → no relaunch.
    let ctrl = FakeControl::probe(Ok(Some("coder-14b".into())))
        .owned()
        .with_served_lanes(3)
        .with_served_window(78_592);
    let mut t = target("coder-14b");
    t.lanes = 5;
    t.context_window = 36_810;
    let outcome = ensure_model_serving(&ctrl, &t, false).await;
    assert_eq!(outcome, EnsureOutcome::AlreadyServing, "a trade is refused, the lane keeps its window");
    assert_eq!(ctrl.serves.load(Ordering::SeqCst), 0, "no relaunch toward the narrower geometry");

    // The other direction: served 5 × 36,810, plan 3 × 78,592 → relaunches (window grow-back).
    let ctrl = FakeControl::probe(Ok(Some("coder-14b".into())))
        .owned()
        .with_served_lanes(5)
        .with_served_window(36_810);
    let mut t = target("coder-14b");
    t.lanes = 3;
    t.context_window = 78_592;
    let outcome = ensure_model_serving(&ctrl, &t, false).await;
    assert!(matches!(outcome, EnsureOutcome::Spawned { .. }), "a wider window at fewer lanes still grows, got {outcome:?}");
}

// what this catches: the window-grow check must NOT fire on llama.cpp's 256-pad
// (served slightly BELOW target after internal rounding is normal) — only a real
// step-change grow relaunches. A served window one pad under target stays
// AlreadyServing; regression would relaunch every tick on padding noise.
#[tokio::test]
async fn padded_window_within_tolerance_does_not_relaunch() {
    let ctrl = FakeControl::probe(Ok(Some("coder-14b".into())))
        .owned()
        .with_served_window(32768 - 256); // one 256-pad under the 32768 target
    let outcome = ensure_model_serving(&ctrl, &target("coder-14b"), false).await;
    assert_eq!(outcome, EnsureOutcome::AlreadyServing);
    assert_eq!(
        ctrl.serves.load(Ordering::SeqCst),
        0,
        "padding noise must not relaunch"
    );
}

// what this catches: an orphan that answers /v1/models with the RIGHT model
// but is COMPUTE-WEDGED (every llama_decode 500s "Compute error" — observed
// live in a 47-min-old orphan reclaimed from a SIGKILLed core). Trusting
// /models alone re-adopted the wedged lane every 5s tick and BOTH personas
// abstained forever. The decode smoke-probe must reject it and fall through
// to serve() → reap + respawn fresh. Regression here silently re-adopts a
// brain that cannot think.
#[tokio::test]
async fn wedged_orphan_rejected_and_respawned() {
    let ctrl = FakeControl::probe(Ok(Some("coder-14b".into()))).decode_wedged();
    let outcome = ensure_model_serving(&ctrl, &target("coder-14b"), false).await;
    assert_eq!(
        outcome,
        EnsureOutcome::Spawned {
            model: "coder-14b".into()
        },
        "a compute-wedged orphan must be rejected and respawned, never adopted"
    );
    assert_eq!(
        ctrl.serves.load(Ordering::SeqCst),
        1,
        "wedged orphan → fresh spawn"
    );
}

// what this catches (card 59052747, 2026-09-20 20:44Z on the IntelMac): an adoptee
// that misses the decode smoke-probe because it is BUSY — its `/slots` account
// advanced during the probe's own window — was reaped as "compute-wedged", a cold
// prefill for every seated mind. Busy is not dead: it is adopted; the frozen one
// (the case above) is still reaped.
#[tokio::test]
async fn a_busy_adoptee_that_misses_the_probe_is_adopted_not_reaped() {
    let ctrl = FakeControl::probe(Ok(Some("coder-14b".into())))
        .decode_wedged()
        .slots_busy();
    let outcome = ensure_model_serving(&ctrl, &target("coder-14b"), false).await;
    assert_eq!(outcome, EnsureOutcome::AlreadyServing, "its slots moved during the probe: alive by its own account");
    assert_eq!(ctrl.serves.load(Ordering::SeqCst), 0, "nothing respawned over a working lane");
    // The same shape on an OWNED child the heartbeat flagged (force_probe): still busy, still kept.
    let ctrl = FakeControl::probe(Ok(Some("coder-14b".into())))
        .owned()
        .decode_wedged()
        .slots_busy();
    assert_eq!(ensure_model_serving(&ctrl, &target("coder-14b"), true).await, EnsureOutcome::AlreadyServing);
}

// what this catches (card 977842fd, the 5090 2026-09-21 00:33Z): a healthy engine
// launched on last deploy's KV decision, adopted by a core whose plan now decides
// differently — the decision never reached the process. An adopted lane whose launch
// KV differs from what a spawn would produce NOW relaunches once; the same decision
// (or an unknown one) still adopts, so the #4284 warm-lane economy stands.
#[tokio::test]
async fn an_adopted_lane_on_last_deploys_kv_decision_relaunches_onto_this_ones() {
    let planned = crate::cognition::kv_cache_plan::resolve();
    let other = if planned.cache_type == crate::cognition::kv_cache_plan::Q8_0 { None } else { Some(crate::cognition::kv_cache_plan::Q8_0) };
    // Launched on the OTHER decision: stale, relaunched.
    let stale = FakeControl::probe(Ok(Some("coder-14b".into()))).owned().launched_with(other, !planned.flash_attn);
    assert_eq!(
        ensure_model_serving(&stale, &target("coder-14b"), false).await,
        EnsureOutcome::Spawned { model: "coder-14b".into() },
        "a different KV decision is a stale lane"
    );
    assert_eq!(stale.serves.load(Ordering::SeqCst), 1);
    // Launched on THIS decision: adopted, nothing respawned.
    let same_type = planned.launcher_cache_type();
    let fresh = FakeControl::probe(Ok(Some("coder-14b".into()))).owned().launched_with(same_type, planned.flash_attn);
    assert_eq!(ensure_model_serving(&fresh, &target("coder-14b"), false).await, EnsureOutcome::AlreadyServing);
    assert_eq!(fresh.serves.load(Ordering::SeqCst), 0);
    // Unknown launch (no argv): not a mismatch.
    let unknown = FakeControl::probe(Ok(Some("coder-14b".into()))).owned();
    assert_eq!(ensure_model_serving(&unknown, &target("coder-14b"), false).await, EnsureOutcome::AlreadyServing);
}

// what this catches (card d7294f0b, Kimi's node 2026-10-15 07:37-07:39Z): the daemon decided a
// cache re-home for an adopted lane whose --cache-ram held no measured state, and this rail
// answered AlreadyServing because it never compared the grant. With a measured state between
// the running grant and the target's, the lane relaunches; an unknown grant, or a target that
// could not hold the state either, still adopts.
#[tokio::test]
async fn an_adopted_lane_whose_cache_holds_no_state_relaunches_onto_one_that_does() {
    const MIB: u64 = 1024 * 1024;
    let id = "cache-grant-d7294f0b"; // its own page dir: the state samples are process-wide
    let t = target(id);
    crate::inference::slots::note_state_bytes(&kv_page_dir(id, t.context_window), 3_800 * MIB);
    let starved = FakeControl::probe(Ok(Some(id.into()))).owned().launched_with_cache(3_248);
    assert_eq!(
        ensure_model_serving(&starved, &t, false).await,
        EnsureOutcome::Spawned { model: id.into() },
        "a 3,248 MiB grant holds no 3,800 MiB state and the target's 4,096 does"
    );
    assert_eq!(starved.serves.load(Ordering::SeqCst), 1);
    let unknown = FakeControl::probe(Ok(Some(id.into()))).owned();
    assert_eq!(ensure_model_serving(&unknown, &t, false).await, EnsureOutcome::AlreadyServing, "no argv: an unknown grant is not a mismatch");
    let futile = ServingTarget { host_prompt_cache_mib: 3_500, ..t.clone() };
    let starved = FakeControl::probe(Ok(Some(id.into()))).owned().launched_with_cache(3_248);
    assert_eq!(
        ensure_model_serving(&starved, &futile, false).await,
        EnsureOutcome::AlreadyServing,
        "a relaunch whose grant holds no state either is not owed"
    );
    assert_eq!(starved.serves.load(Ordering::SeqCst), 0);
}

// what this catches: #175 self-heal. A child WE OWN that is compute-wedged (decode
// probe fails while /v1/models still 200s — the Metal-GPU-OOM poison) is normally
// TRUSTED forever ("decode-verified at wait_ready, trusted thereafter, no per-tick
// decode load"). Once the serving daemon's liveness heartbeat has flagged it,
// `force_probe=true` makes `ensure_model_serving` re-prove decode EVEN on an owned
// child, so the wedged lane is reaped + respawned instead of re-adopted. Regression
// here re-bricks the persona substrate on any transient GPU OOM. regression for #175
#[tokio::test]
async fn force_probe_relaunches_an_owned_but_wedged_lane() {
    let ctrl = FakeControl::probe(Ok(Some("coder-14b".into())))
        .owned()
        .decode_wedged();
    // Baseline (the blindness we fix): an owned child is trusted despite the wedge.
    let trusted = ensure_model_serving(&ctrl, &target("coder-14b"), false).await;
    assert_eq!(
        trusted,
        EnsureOutcome::AlreadyServing,
        "an owned child is trusted without force_probe (the pre-#175 blindness)"
    );
    assert_eq!(
        ctrl.serves.load(Ordering::SeqCst),
        0,
        "no relaunch without force"
    );
    // With force_probe: re-prove decode → fails → reap + respawn a fresh backend.
    let healed = ensure_model_serving(&ctrl, &target("coder-14b"), true).await;
    assert_eq!(
        healed,
        EnsureOutcome::Spawned {
            model: "coder-14b".into()
        },
        "force_probe relaunches an owned wedged lane — the #175 self-heal"
    );
    assert_eq!(ctrl.serves.load(Ordering::SeqCst), 1, "exactly one respawn");
}

// what this catches: a child WE spawned (owns_child) is decode-verified once
// at wait_ready and trusted thereafter — the adopt path must NOT run a decode
// smoke-probe against it every 5s tick (that would burn a GPU decode per tick
// and contend with the personas' real inference slots). owns_child
// short-circuits the probe: even a `decode_wedged` fake adopts, proving the
// probe was skipped for an owned child.
#[tokio::test]
async fn owned_child_adopted_without_smoke_probe() {
    let ctrl = FakeControl::probe(Ok(Some("coder-14b".into())))
        .owned()
        .decode_wedged();
    let outcome = ensure_model_serving(&ctrl, &target("coder-14b"), false).await;
    assert_eq!(
        outcome,
        EnsureOutcome::AlreadyServing,
        "our own decode-verified child is trusted without a per-tick re-probe"
    );
    assert_eq!(
        ctrl.serves.load(Ordering::SeqCst),
        0,
        "no relaunch for an owned child"
    );
}

// what this catches: a different model live → relaunch to the desired one.
#[tokio::test]
async fn wrong_model_triggers_relaunch() {
    let ctrl = FakeControl::probe(Ok(Some("general-4b".into())));
    let outcome = ensure_model_serving(&ctrl, &target("coder-14b"), false).await;
    assert_eq!(
        outcome,
        EnsureOutcome::Spawned {
            model: "coder-14b".into()
        }
    );
    assert_eq!(ctrl.serves.load(Ordering::SeqCst), 1);
}

// what this catches: SAME model + SAME genome set live → no relaunch. The
// adapter-set comparison must not spuriously relaunch a correctly-genomed
// server (which would kill the GPU-warm catalog every tick).
#[tokio::test]
async fn same_model_same_genome_does_not_relaunch() {
    let ctrl = FakeControl::probe(Ok(Some("coder-14b".into())))
        .with_active_adapters(vec!["/genes/a.gguf".into(), "/genes/b.gguf".into()]);
    // Desired set equal but given out of order — adapter_paths() sorts, so it matches.
    let outcome = ensure_model_serving(
        &ctrl,
        &target_with_adapters("coder-14b", &["/genes/b.gguf", "/genes/a.gguf"]),
        false,
    )
    .await;
    assert_eq!(outcome, EnsureOutcome::AlreadyServing);
    assert_eq!(
        ctrl.serves.load(Ordering::SeqCst),
        0,
        "no relaunch when genome set matches"
    );
}

// what this catches: SAME model but a NEW gene trained (set grew) → relaunch.
// This is the wire that closes the genome learning loop — without the adapter
// comparison a freshly-trained gene with an unchanged model id would return
// AlreadyServing and never load into the catalog.
#[tokio::test]
async fn new_gene_triggers_relaunch_on_same_model() {
    let ctrl = FakeControl::probe(Ok(Some("coder-14b".into())))
        .with_active_adapters(vec!["/genes/a.gguf".into()]);
    let outcome = ensure_model_serving(
        &ctrl,
        &target_with_adapters("coder-14b", &["/genes/a.gguf", "/genes/b.gguf"]),
        false,
    )
    .await;
    assert_eq!(
        outcome,
        EnsureOutcome::Spawned {
            model: "coder-14b".into()
        }
    );
    assert_eq!(
        ctrl.serves.load(Ordering::SeqCst),
        1,
        "new gene must relaunch to repopulate the catalog"
    );
}

// what this catches (charter S1, card 49b5e806): a new gene on an engine that loads at
// runtime relaunching anyway, killing every turn in flight; and a retire refused
// because a turn holds the gene turning into that same relaunch instead of a wait
// (the set stands this tick; the checks after it still run).
#[tokio::test]
async fn a_new_gene_joins_a_capable_engine_in_place_and_a_busy_retire_waits() {
    for outcome in [GenomeInPlace::Adopted, GenomeInPlace::Busy] {
        let ctrl = FakeControl::probe(Ok(Some("coder-14b".into())))
            .with_active_adapters(vec!["/genes/a.gguf".into()])
            .with_in_place(outcome);
        let got = ensure_model_serving(
            &ctrl,
            &target_with_adapters("coder-14b", &["/genes/a.gguf", "/genes/b.gguf"]),
            false,
        )
        .await;
        assert_eq!(got, EnsureOutcome::AlreadyServing, "{outcome:?}");
        assert_eq!(ctrl.serves.load(Ordering::SeqCst), 0, "{outcome:?}: no relaunch");
    }
    // The staged name: a bare .gguf the engine's confinement accepts, and two genes
    // sharing a file name in different folders never collide.
    let a = gene_stage_name(std::path::Path::new("/genome/kimi/adapter.gguf")).expect("stageable");
    let b = gene_stage_name(std::path::Path::new("/genome/asha/adapter.gguf")).expect("stageable");
    assert_ne!(a, b);
    assert!(a.starts_with("gene-adapter-") && a.ends_with(".gguf"));
    assert!(!a.contains('/') && !a.contains("..") && a.len() <= 200);
}

// what this catches: nothing running (Unreachable) is NOT an error — it's
// the normal pre-spawn state, so we serve, not degrade.
#[tokio::test]
async fn unreachable_means_spawn_not_degrade() {
    let ctrl = FakeControl::probe(Err("connection refused"));
    let outcome = ensure_model_serving(&ctrl, &target("coder-14b"), false).await;
    assert_eq!(
        outcome,
        EnsureOutcome::Spawned {
            model: "coder-14b".into()
        }
    );
    assert_eq!(ctrl.serves.load(Ordering::SeqCst), 1);
}

// what this catches: no model live and serve() fails → Degraded with the
// named reason, NEVER a panic on the daemon tick (degrade-never-panic).
#[tokio::test]
async fn serve_failure_degrades_with_reason() {
    let ctrl = FakeControl::probe(Ok(None)).serve_fails();
    let outcome = ensure_model_serving(&ctrl, &target("coder-14b"), false).await;
    match outcome {
        EnsureOutcome::Degraded { reason } => {
            assert!(reason.contains("boom"), "reason: {reason}")
        }
        other => panic!("expected Degraded, got {other:?}"),
    }
}

// what this catches: host:port parsing for the launch args, including the
// /v1-stripped root and a missing port falling back to the default.
#[test]
fn split_host_port_parses_root() {
    assert_eq!(
        split_host_port("http://127.0.0.1:58057"),
        ("127.0.0.1".into(), 58057)
    );
    assert_eq!(
        split_host_port("http://0.0.0.0:9090"),
        ("0.0.0.0".into(), 9090)
    );
    assert_eq!(
        split_host_port("http://localhost"),
        ("localhost".into(), DEFAULT_PORT)
    );
}

// what this catches: serving_root normalizes a configured `.../v1` back to
// the bare root so /health and /v1 are derived from one source of truth.
// (No env set in test → default; the strip logic is exercised by the unit
// below feeding the same normalization the reader uses.)
#[test]
fn root_normalization_strips_v1() {
    // Mirror the normalization serving_root applies to a configured value.
    let normalize = |raw: &str| {
        let trimmed = raw.trim().trim_end_matches('/');
        trimmed.strip_suffix("/v1").unwrap_or(trimmed).to_string()
    };
    assert_eq!(normalize("http://h:1/v1"), "http://h:1");
    assert_eq!(normalize("http://h:1/v1/"), "http://h:1");
    assert_eq!(normalize("http://h:1"), "http://h:1");
}

// what this catches: first_free_port must SKIP a port already bound and
// return a different, bindable one — the whole point of "scan, never assume
// free". A regression to "return base unconditionally" would wedge the bind
// whenever the preferred port is taken (the Studio-squats-58057 case).
#[test]
fn first_free_port_skips_a_taken_port() {
    // Bind an ephemeral port so we KNOW one is taken, then scan from it.
    let taken = std::net::TcpListener::bind((DEFAULT_HOST, 0)).expect("bind ephemeral");
    let taken_port = taken.local_addr().unwrap().port();
    let chosen = first_free_port(taken_port);
    assert_ne!(chosen, taken_port, "must not return the held port");
    // And the returned port must itself be bindable right now.
    assert!(
        std::net::TcpListener::bind((DEFAULT_HOST, chosen)).is_ok(),
        "scanned port {chosen} should be free"
    );
}

// what this catches: an ephemeral lane must derive its `/v1` url from the
// EXPLICIT root it was handed (not the global serving_root), and must not
// double up the scheme or the `/v1` segment — the same `/v1/v1` class of bug
// that muted Asha. `with_root` is the seam EphemeralServingLane builds on.
#[test]
fn with_root_derives_v1_url_from_explicit_root() {
    let proc = LlamaServerProcess::with_root("http://127.0.0.1:59123".to_string());
    assert_eq!(proc.root, "http://127.0.0.1:59123");
    assert_eq!(proc.v1_url, "http://127.0.0.1:59123/v1");
    // a trailing slash on the root must not yield `//v1`.
    let proc = LlamaServerProcess::with_root("http://127.0.0.1:59123/".to_string());
    assert_eq!(proc.v1_url, "http://127.0.0.1:59123/v1");
}

// what this catches: THE seam that binds the orphan-reclaim machinery to
// exactly one process. The live constructors (`new`/`with_client`) must mark
// `is_live_lane` so they pin the canonical port + own the reclaim pidfile;
// `with_root` (the EphemeralServingLane seam) must NOT — an ephemeral lane that
// wrote the canonical pidfile or reaped the live port would kill the living
// persona's own server. A regression flipping either default reintroduces the
// two-servers-on-one-GPU OOM (live not pinning) or self-reaping (ephemeral
// claiming the live role).
#[test]
fn live_constructors_are_live_lane_ephemeral_is_not() {
    assert!(
        LlamaServerProcess::new().is_live_lane,
        "new() is THE live lane — must pin the canonical port + own the pidfile"
    );
    assert!(
        LlamaServerProcess::with_client(reqwest::Client::new()).is_live_lane,
        "with_client() is a live constructor"
    );
    assert!(
        !LlamaServerProcess::with_root("http://127.0.0.1:59123".to_string()).is_live_lane,
        "with_root() is ephemeral — must never write the canonical pidfile or reclaim the live port"
    );
}

// what this catches: the live lane PINS the canonical port — it must never
// scan away from DEFAULT_PORT. Scanning past a held canonical port was the bug
// (spawned a GPU competitor AND blinded the daemon's own probe). The pin is
// what makes a crashed predecessor's orphan reclaimable on a known port. The
// ephemeral scan (first_free_port) is tested separately above and stays.
#[test]
fn live_lane_pins_the_canonical_port() {
    assert_eq!(
        chosen_port(),
        DEFAULT_PORT,
        "the live lane must bind the canonical port, never a scanned one"
    );
}

// what this catches: the readiness gate `await_ready_serving` waits on must
// require BOTH `ready` AND an `active_model` — a regression to `ready` alone
// (or `ready || ...`) would resolve a lease against a `None` model and bind
// a persona to nothing. Exercised on a LOCAL watch so it doesn't mutate the
// process-global SERVING_STATE (set-once → would race other tests).
#[tokio::test]
async fn ready_predicate_requires_ready_and_a_model() {
    let pred = |s: &ServingSnapshot| s.ready && s.active_model.is_some();
    let (tx, mut rx) = watch::channel(ServingSnapshot::empty());

    // empty: not ready, no model → unsatisfied.
    assert!(!pred(&rx.borrow()));
    // ready but no model → STILL unsatisfied (the bug we guard against).
    tx.send_replace(ServingSnapshot {
        // test fixture: no live readiness was ever CONFIRMED here.
        ready_verified_at_ms: None,
        active_model: None,
        ready: true,
        base_url: "x".into(),
        adapters: Vec::new(),
        served_context_window: 0,
        lanes: 0,
        host_prompt_cache_mib: 0,
        degraded_reason: None,
        vision_ready: false,
        vision_base_url: None,
        vision_model: None,
        loading_model: None,
    });
    assert!(!pred(&rx.borrow()));
    // not-ready but has a model → unsatisfied.
    tx.send_replace(ServingSnapshot {
        // test fixture: no live readiness was ever CONFIRMED here.
        ready_verified_at_ms: None,
        active_model: Some("coder".into()),
        ready: false,
        base_url: "x".into(),
        adapters: Vec::new(),
        served_context_window: 0,
        lanes: 0,
        host_prompt_cache_mib: 0,
        degraded_reason: None,
        vision_ready: false,
        vision_base_url: None,
        vision_model: None,
        loading_model: None,
    });
    assert!(!pred(&rx.borrow()));
    // ready AND a model → satisfied, and wait_for resolves to it at once.
    tx.send_replace(ServingSnapshot {
        // test fixture: no live readiness was ever CONFIRMED here.
        ready_verified_at_ms: None,
        active_model: Some("coder".into()),
        ready: true,
        base_url: "x".into(),
        adapters: Vec::new(),
        served_context_window: 0,
        lanes: 0,
        host_prompt_cache_mib: 0,
        degraded_reason: None,
        vision_ready: false,
        vision_base_url: None,
        vision_model: None,
        loading_model: None,
    });
    let got = tokio::time::timeout(Duration::from_millis(100), rx.wait_for(pred))
        .await
        .expect("should not time out")
        .expect("sender live");
    assert_eq!(got.active_model.as_deref(), Some("coder"));
}

// what this catches (2026-09-18, the 5090 sightless with Qwen3.8-27B's projector on
// disk): the sidecar is a prosthesis for text-only minds. A row that prefers a
// sidecar withholds its projector ONLY when another VL candidate is on the node;
// with none, the main lane wears its own eyes — never blind because of a cache
// knob. Opt-in rows and the sidecar lane itself always wear; nothing resolved is
// nothing to decide; a row that prefers a sidecar but was not consulted errs toward
// sight.
#[test]
fn the_only_eyes_on_the_node_go_on_the_main_lane() {
    use MainLaneMmproj::*;
    assert_eq!(main_lane_mmproj_decision(false, false, true, Some(true)), Withhold);
    assert_eq!(main_lane_mmproj_decision(false, false, true, Some(false)), WearAsOnlyEyes);
    assert_eq!(main_lane_mmproj_decision(false, false, true, None), WearAsOnlyEyes);
    assert_eq!(main_lane_mmproj_decision(true, false, true, Some(true)), Wear);
    assert_eq!(main_lane_mmproj_decision(false, true, true, Some(true)), Wear);
    assert_eq!(main_lane_mmproj_decision(false, false, false, Some(false)), NoneResolved);
    // And the readiness check names a withheld/absent projector as what it is —
    // never "--mmproj was passed" about a lane that was given none.
    let err = vision_lane_ready(true, false, None).unwrap_err();
    assert!(err.contains("carries no mmproj"), "{err}");
    assert!(!err.contains("was passed"), "{err}");
}

// what this catches: the #106 vision-readiness verdict must only claim sight when
// ALL THREE facts line up — the row declares Vision, an mmproj resolved at spawn,
// AND the running server's own /props confirms modalities.vision. A regression that
// returns Ok(true) from a weaker combination publishes `vision_ready: true` for a
// lane that silently drops image parts — the exact capability lie the observe path
// gates on ([[fallbacks-are-illegal-fail-loud]]).
#[test]
fn vision_lane_ready_requires_declaration_projector_and_props_confirmation() {
    let sees = MultimodalSupport {
        vision: true,
        audio: false,
    };
    let blind = MultimodalSupport {
        vision: false,
        audio: true,
    };
    // Text lane: never declared Vision → not vision-ready, and NOT an error.
    assert_eq!(vision_lane_ready(false, false, None), Ok(false));
    assert_eq!(vision_lane_ready(false, true, Some(sees)), Ok(false));
    // Declared Vision but no projector resolved → loud error naming the gap.
    let err = vision_lane_ready(true, false, None).unwrap_err();
    assert!(err.contains("carries no mmproj projector"), "{err}");
    // Projector passed, server confirms vision → the ONLY Ok(true) path.
    assert_eq!(vision_lane_ready(true, true, Some(sees)), Ok(true));
    // Projector passed but the server says it can't see (wrong/failed mmproj).
    let err = vision_lane_ready(true, true, Some(blind)).unwrap_err();
    assert!(err.contains("modalities.vision=false"), "{err}");
    // Server build reports no modalities block: UNVERIFIED must not read as sight.
    let err = vision_lane_ready(true, true, None).unwrap_err();
    assert!(err.contains("cannot VERIFY"), "{err}");
}

// what this catches: a WEDGED server (port held open, never answers HTTP — a
// GPU hang, OOM-thrash, deadlock, or a SIGSTOP'd orphan from a crashed
// predecessor) must not hang `active_model` forever. Without the per-request
// PROBE_TIMEOUT the probe blocks indefinitely: the TCP connect succeeds against
// the held socket, then the read never returns, so `ensure_model_serving` never
// returns and the reconcile that would reclaim the sick lane never runs — the
// exact silent-hang that wedged a successor core. The bound converts "wedged"
// into a fast `Unreachable`, which serve()+reclaim acts on. We assert it
// returns (Err) well under an unbounded "forever" — if the timeout regressed to
// unbounded this test would hang and the runner would kill it. Regression for
// the unbounded control-plane probe (#90 reliability follow-up).
#[tokio::test]
async fn wedged_server_probe_times_out_instead_of_hanging() {
    use tokio::io::AsyncReadExt;
    use tokio::net::TcpListener;

    // A listener that ACCEPTS then holds the socket silent — the precise wedge a
    // frozen llama-server produces (connect ok, read never answered).
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let _holder = tokio::spawn(async move {
        if let Ok((mut sock, _)) = listener.accept().await {
            // Drain quietly and never reply; hold the conn open past the probe.
            let mut buf = [0u8; 1024];
            let _ = sock.read(&mut buf).await;
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
    });

    let proc = LlamaServerProcess::with_root(format!("http://127.0.0.1:{port}"));
    let started = Instant::now();
    let result = proc.active_model().await;
    let elapsed = started.elapsed();

    assert!(
        matches!(result, Err(LlamaServerError::Unreachable(_))),
        "wedged server must surface as Unreachable, got {result:?}"
    );
    assert!(
        elapsed < PROBE_TIMEOUT + Duration::from_secs(4),
        "probe must be bounded by PROBE_TIMEOUT, took {elapsed:?}"
    );
}
// what this catches: the host's CPU placement decision must be one config key, read
// once, defaulting to the GPU — never a raw env var and never a spelling guess.
#[test]
fn main_lane_placement_reads_the_config_decision() {
    assert_eq!(placement_from_config(Some("cpu")), LanePlacement::Cpu);
    assert_eq!(placement_from_config(Some(" CPU ")), LanePlacement::Cpu);
    assert_eq!(placement_from_config(Some("gpu")), LanePlacement::Gpu);
    assert_eq!(placement_from_config(None), LanePlacement::Gpu);
}

// what this catches (2026-09-15, the M5): the page dir keyed on the EXACT per-slot
// window minted a new generation on every relaunch (59171 → 58997 → … in five
// hours) and the spawn-time sweep deleted the live lane's pages — every eviction a
// 24–66 s re-prefill against a 0.36 s restore. (a) windows within one step share a
// key; (b) the sweep leaves the previous live lane's dir alone.
#[test]
fn relaunch_jitter_shares_a_page_generation_and_the_live_dir_survives_the_sweep() {
    assert_eq!(page_geometry_key(59_171), page_geometry_key(57_112), "the measured band is one key");
    assert_eq!(page_geometry_key(59_171), 49_152);
    assert_eq!(page_geometry_key(25_104), page_geometry_key(26_531));
    assert_ne!(page_geometry_key(59_171), page_geometry_key(25_104));
    assert_eq!(page_geometry_key(100), PAGE_GEOMETRY_STEP, "never a zero key");

    let root = tempfile::tempdir().expect("tempdir");
    let mk = |k: &str| {
        let d = root.path().join(format!("some-model--c{k}"));
        std::fs::create_dir_all(&d).unwrap(); // JUSTIFIED unwrap: test scaffolding
        std::fs::write(d.join("a-page.bin"), b"kv").unwrap(); // JUSTIFIED unwrap: test scaffolding
        d
    };
    let live_previous = mk("57344");
    let stale = mk("24576");
    let other_model = root.path().join("other-model--c57344");
    std::fs::create_dir_all(&other_model).unwrap(); // JUSTIFIED unwrap: test scaffolding
    let current = root.path().join("some-model--c61440");
    std::fs::create_dir_all(&current).unwrap(); // JUSTIFIED unwrap: test scaffolding

    // The registry's copy of the path may differ textually from what read_dir yields
    // (an older core's HOME resolution, /Users vs /private/var on macOS, `./`) —
    // protection is by directory NAME (Cormac, finding 1).
    let other_form = PathBuf::from("/some/other/root/./").join(live_previous.file_name().unwrap()); // JUSTIFIED unwrap: a dir we just made has a name
    sweep_stale_page_generations(&current, &[other_form]);
    assert!(live_previous.join("a-page.bin").exists(), "the previous live lane's pages survive");
    assert!(!stale.exists(), "an unprotected stale generation is swept");
    assert!(other_model.exists(), "another model's dir is not this lane's to sweep");
    assert!(current.exists());
}

// what this catches (Astra's block, re-block and third review on #4069), with
// deterministic barriers and the PRODUCTION ordering (reserve, THEN mkdir):
//  (a) B tries to reserve+create while A holds the guard mid-sweep (after census,
//      before deletion): B blocks until A's sweep is done, so B's insert can never
//      land between A's census and A's deletion — and because B creates its dir
//      only AFTER its reservation, A's sweep cannot delete a dir B has made.
#[test]
fn a_reservation_cannot_slip_between_a_siblings_census_and_deletion() {
    use std::sync::{mpsc, Arc, Barrier};
    let root = tempfile::tempdir().expect("tempdir");
    // a STALE generation of B's geometry from an earlier run: unowned at A's census
    let stale = root.path().join("race-model--c32768");
    std::fs::create_dir_all(&stale).unwrap(); // JUSTIFIED unwrap: test scaffolding
    std::fs::write(stale.join("old-page.bin"), b"kv").unwrap(); // JUSTIFIED unwrap: test scaffolding
    let b_dir = stale.clone(); // B relaunches into the same geometry
    let a_dir = root.path().join("race-model--c49152");
    std::fs::create_dir_all(&a_dir).unwrap(); // JUSTIFIED unwrap: test scaffolding

    let at_census = Arc::new(Barrier::new(2));
    let (b_reserved_tx, b_reserved_rx) = mpsc::channel::<bool>();
    let b_dir2 = b_dir.clone();
    let at_census_b = Arc::clone(&at_census);
    let b = std::thread::spawn(move || {
        at_census_b.wait(); // A is mid-sweep now
        // THE PRODUCTION ORDERING: reserve first, create second. Must BLOCK on the guard.
        let (r, ready) = PageDirReservation::take_and_create(&b_dir2);
        let _ = b_reserved_tx.send(ready);
        r
    });
    {
        let guard = PAGE_DIR_GUARD.lock(); // A's census+deletion critical section
        at_census.wait();
        assert!(
            b_reserved_rx.recv_timeout(std::time::Duration::from_millis(300)).is_err(),
            "B's reservation must not land while A holds the guard"
        );
        // A's census saw no reservation for B's dir → A deletes it under the guard.
        let protect: Vec<PathBuf> = guard.reservations.iter().cloned().collect();
        assert_eq!(sweep_stale_page_generations(&a_dir, &protect), 1);
        assert!(!b_dir.exists(), "the stale generation is gone");
    }
    // Guard released → B's reservation lands, THEN B creates its dir: it exists,
    // it is protected, and nothing A did could have touched it.
    let b_res = b.join().expect("B joins");
    let ready = b_reserved_rx
        .recv_timeout(std::time::Duration::from_secs(2))
        .expect("B reserved after A released");
    assert!(ready && b_dir.is_dir(), "B's dir was created after its reservation");
    assert!(b_res.is_held());
    assert!(
        PAGE_DIR_GUARD
            .lock()
            .reservations
            .iter()
            .any(|d| d == &b_dir),
        "B is protected from here on"
    );
    // A second sweep now, with the guard's contents as the protect set, keeps B's dir.
    let protect: Vec<PathBuf> = PAGE_DIR_GUARD.lock().reservations.iter().cloned().collect();
    assert_eq!(sweep_stale_page_generations(&a_dir, &protect), 0);
    assert!(b_dir.is_dir());
}

//  (b) the handoff (record written + reservation dropped) is ONE critical section:
//      concurrent censuses and ordered observations on both sides see the
//      reservation or record, never neither. The registry is a temp
//      dir (the test seam), never the canonical `~/.continuum/run/lanes/`.
#[test]
fn a_census_under_the_guard_never_finds_a_lane_in_neither_list_during_handoff() {
    use crate::inference::lane_registry::{records_checked_in, LaneRecord, LaneRole};
    use std::sync::mpsc;
    use std::time::{Duration, Instant};
    let root = tempfile::tempdir().expect("tempdir");
    let registry = tempfile::tempdir().expect("tempdir");
    let b_dir = root.path().join("handoff-model--c32768");
    let (mut res, _ready) = PageDirReservation::take_and_create(&b_dir);
    let rec = LaneRecord {
        pid: 4_000_001, // never a live pid; the census here reads lists, not liveness
        port: 8123,
        role: LaneRole::Ephemeral,
        model: "handoff-model".into(),
        context_window: 32768,
        lanes: 1,
        page_dir: Some(b_dir.clone()),
        engine_bin: None,
        started_s: 0,
    };
    let (before_tx, before_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    let reg = registry.path().to_path_buf();
    let dir = b_dir.clone();
    // Contend during handoff, and observe both sides explicitly. A fixed
    // iteration count could exhaust before the handoff thread ran.
    let census = std::thread::spawn(move || {
        let mut seen_before = 0usize;
        let mut seen_after = 0usize;
        // Initial BEFORE census, concurrent censuses, then explicit AFTER.
        let observe = |seen_before: &mut usize, seen_after: &mut usize| {
            let g = PAGE_DIR_GUARD.lock();
            let in_res = g.reservations.iter().any(|d| d == &dir);
            let in_rec = records_checked_in(&reg)
                .expect("temp registry readable")
                .iter()
                .any(|r| r.page_dir.as_deref() == Some(dir.as_path()));
            drop(g);
            assert!(in_res || in_rec, "a census found the lane in NEITHER list");
            assert!(!(in_res && in_rec), "a census found the lane in BOTH lists");
            if in_res { *seen_before += 1 } else { *seen_after += 1 }
        };
        observe(&mut seen_before, &mut seen_after);
        before_tx.send(()).expect("before census notification");
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match done_rx.try_recv() {
                Ok(()) => break,
                Err(mpsc::TryRecvError::Empty) => {
                    assert!(Instant::now() < deadline, "handoff completion deadline");
                    observe(&mut seen_before, &mut seen_after);
                    std::thread::yield_now();
                }
                Err(mpsc::TryRecvError::Disconnected) => panic!("handoff owner disconnected"),
            }
        }
        observe(&mut seen_before, &mut seen_after);
        (seen_before, seen_after)
    });
    before_rx.recv_timeout(Duration::from_secs(5)).expect("initial census completed");
    res.handoff_in(&rec, registry.path()).expect("handoff writes the temp registry");
    assert!(!res.is_held(), "a successful handoff releases the reservation");
    done_tx.send(()).expect("handoff complete");
    let (before, after) = census.join().expect("census joins");
    assert!(before > 0 && after > 0, "censuses ran on both sides of the handoff: {before}/{after}");
    // And a FAILED handoff keeps the reservation: an unwritable registry dir.
    let (mut res2, _) = PageDirReservation::take_and_create(&root.path().join("handoff-model--c49152"));
    let not_a_dir = registry.path().join("file-not-dir");
    std::fs::write(&not_a_dir, b"x").unwrap(); // JUSTIFIED unwrap: test scaffolding
    assert!(res2.handoff_in(&rec, &not_a_dir).is_err());
    assert!(res2.is_held(), "a failed record write keeps the dir protected");
}

// what this catches (Astra's third review): a destructive census fails CLOSED. With
// a malformed `.lane` file in the registry the guarded sweep deletes NOTHING, even
// though a stale sibling generation is right there.
#[test]
fn an_unreadable_registry_skips_the_sweep_entirely() {
    let root = tempfile::tempdir().expect("tempdir");
    let registry = tempfile::tempdir().expect("tempdir");
    let stale = root.path().join("closed-model--c32768");
    std::fs::create_dir_all(&stale).unwrap(); // JUSTIFIED unwrap: test scaffolding
    let current = root.path().join("closed-model--c49152");
    std::fs::create_dir_all(&current).unwrap(); // JUSTIFIED unwrap: test scaffolding
    std::fs::write(registry.path().join("garbage.lane"), "{not json").unwrap(); // JUSTIFIED unwrap: test scaffolding
    assert_eq!(sweep_stale_page_generations_guarded_in(&current, registry.path()), None);
    assert!(stale.is_dir(), "nothing deleted on an unreadable inventory");
    std::fs::remove_file(registry.path().join("garbage.lane")).unwrap(); // JUSTIFIED unwrap: test scaffolding
    assert_eq!(sweep_stale_page_generations_guarded_in(&current, registry.path()), Some(1));
    assert!(!stale.exists());
}

// what this catches: the live-lane inventory — a live record contributes its dir, a
// live record without one (written before the field existed) makes the inventory
// INCOMPLETE so the sweep stays its hand, and a dead record is ignored either way.
#[test]
fn the_live_lane_inventory_is_complete_only_when_every_live_record_names_its_dir() {
    use crate::inference::lane_registry::{LaneRecord, LaneRole};
    let rec = |pid: u32, page_dir: Option<&str>| LaneRecord {
        pid,
        port: 8000 + pid as u16,
        role: LaneRole::Live,
        model: "m".into(),
        context_window: 4096,
        lanes: 1,
        page_dir: page_dir.map(PathBuf::from),
        engine_bin: None,
        started_s: 0,
    };
    let alive = |pid: u32| pid != 3;
    let (dirs, complete) = page_dirs_of(vec![rec(1, Some("/p/a")), rec(2, Some("/p/b")), rec(3, None)], alive);
    assert_eq!(dirs, vec![PathBuf::from("/p/a"), PathBuf::from("/p/b")]);
    assert!(complete, "a DEAD record without a dir does not spoil the inventory");
    let (dirs, complete) = page_dirs_of(vec![rec(1, Some("/p/a")), rec(2, None)], alive);
    assert_eq!(dirs, vec![PathBuf::from("/p/a")]);
    assert!(!complete, "a LIVE record without a dir → incomplete → no sweep");
}

// what this catches: the latched-backend wedge class, AND the empty that must NOT
// trigger it. A Metal compute context that has gone bad still answers 200 and streams
// a well-formed SSE body with ZERO deltas, so no transport-failure stamp fires and the
// lane reads healthy while every citizen on it gets an empty turn. Measured on the M5
// 2026-09-22: 2 root Metal OOMs latched the backend, 113 tasks ran prefill and never
// generated, 73 empty-completion faults landed on six citizens — and `REAL_DECODE_FAILS`
// stayed 0 the whole time.
//
// The finish reason is load-bearing and was missing from the first cut of this fix
// (Cormac's review of #4344). The IntelMac's empties the same hour were `Length` on a
// lane with 27 successful generations in the same window: the cap ended the turn, the
// backend is fine, and two of them would have bounced a healthy lane for nothing. Same
// sentence in the receipt, opposite cure.
#[test]
fn only_an_empty_that_stopped_is_the_lanes_failure() {
    use super::{classify_real_decode, RealDecodeOutcome};
    use crate::ai::types::FinishReason;

    // Nothing produced at all: the relaunch is the cure.
    assert_eq!(
        classify_real_decode(true, false, FinishReason::Stop),
        RealDecodeOutcome::DeliveredNothing,
        "an empty local-lane generation that STOPPED must count AGAINST the lane — \
         this is the evidence the heartbeat relaunches on"
    );
    // The cap ended it. The backend generated; the allowance ran out. Relaunching
    // would bounce a working lane and fix nothing — the output allowance owns this.
    assert_eq!(
        classify_real_decode(true, false, FinishReason::Length),
        RealDecodeOutcome::CappedBeforeOutput,
        "an empty that hit the OUTPUT CAP is an allowance fault on healthy hardware, \
         never lane wedge evidence"
    );
    // Real output is still proof of life whatever ended it, and still ends the streak.
    assert_eq!(
        classify_real_decode(true, true, FinishReason::Stop),
        RealDecodeOutcome::ProofOfLife
    );
    assert_eq!(
        classify_real_decode(true, true, FinishReason::Length),
        RealDecodeOutcome::ProofOfLife,
        "a truncated turn still DELIVERED — it is not a wedge, whatever else it is"
    );
    // A cloud provider's empty turn says nothing about our hardware. Every finish
    // reason, so a future refactor cannot let remote traffic smear the local record.
    for fr in [
        FinishReason::Stop,
        FinishReason::Length,
        FinishReason::ToolUse,
        FinishReason::Error,
    ] {
        assert_eq!(classify_real_decode(false, false, fr), RealDecodeOutcome::NotOurLane);
        assert_eq!(classify_real_decode(false, true, fr), RealDecodeOutcome::NotOurLane);
    }
}
