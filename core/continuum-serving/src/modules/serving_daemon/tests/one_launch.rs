#![cfg(test)]

use super::*;

// what this catches: the daemon relaunching a ready engine because the plan wants a different
// window or lane count. The M5's plan read 1, 2 and 3 lanes of four residents and its engine
// reloaded 40 times (2026-11-02/03). A ready lane serving the right model and genome to the
// population is never relaunched, however far the plan's window or lane count drifts.
#[tokio::test]
async fn a_ready_lane_is_never_relaunched_for_its_window_or_lanes() {
    let serves = Arc::new(AtomicUsize::new(0));
    let mut server = FakeServer::healthy(serves.clone(), true);
    server.active = Some("coder-14b".into());
    let daemon = daemon_with(Arc::new(server));
    let budget = HostBudget {
        usable_bytes: 45 * GB,
        perf_cores: 6,
    };
    let candidates = vec![footprint_from_parts("coder-14b", 9 * GB, 8192, true, None).unwrap()];
    daemon.publish_plan(budget, &candidates, &candidates);
    let plan_window = daemon
        .plan_tx
        .borrow()
        .plan
        .as_ref()
        .expect("plan published")
        .served_context_window;
    let ready = |lanes: u32| ServingSnapshot {
        // test fixture: no live readiness was ever CONFIRMED here.
        ready_verified_at_ms: None,
        active_model: Some("coder-14b".into()),
        ready: true,
        base_url: serving_v1_url(),
        adapters: Vec::new(),
        served_context_window: plan_window / 4,
        lanes,
        host_prompt_cache_mib: 0,
        degraded_reason: None,
        vision_ready: false,
        vision_base_url: None,
        vision_model: None,
        loading_model: None,
        held_for: None,
    };
    let _ = daemon.serving_tx.send_replace(ready(4));
    for tick in 0..10 {
        assert!(
            daemon.reconcile_to_plan().is_none(),
            "tick {tick}: a quarter of the plan's window is not a relaunch"
        );
    }
    assert_eq!(serves.load(Ordering::SeqCst), 0, "never relaunched");
}

// what this catches (2026-08-15 14:13, round-killer L10 / #438 live): a plan tick
// whose usable_bytes sample was transiently bogus named the SMALLEST candidate
// (qwen2.5-0.5B) via the nothing-fits arm, and the reconcile swapped the READY
// 27B lane on that single tick — citizens spoke template-token salad into the
// bench room as durable messages every healthy peer then perceived as
// conversation. A base-model swap re-homes minds; it must earn the same
// sustained streak as the (milder) window re-home, target changes must restart
// the count, and recovery (no ready incumbent) must never wait.
#[test]
fn model_swap_on_a_ready_lane_needs_a_sustained_streak() {
    use super::{model_change_gate, ModelChangeGate};
    use std::sync::atomic::AtomicU32;
    let streak = AtomicU32::new(0);
    let mut pending = None;
    // One bogus tick wanting the 0.5B: deferred, never actuates.
    assert_eq!(
        model_change_gate(true, Some("27b"), "0.5b", &mut pending, &streak, 3),
        ModelChangeGate::Defer { streak: 1 }
    );
    // Next tick the sample healed and the plan wants the incumbent again:
    // pending clears, nothing happened.
    assert_eq!(
        model_change_gate(true, Some("27b"), "27b", &mut pending, &streak, 3),
        ModelChangeGate::NotAChange
    );
    assert!(pending.is_none());
    // Sustained genuine disagreement commits on the Nth consecutive tick.
    assert_eq!(
        model_change_gate(true, Some("27b"), "48b", &mut pending, &streak, 3),
        ModelChangeGate::Defer { streak: 1 }
    );
    assert_eq!(
        model_change_gate(true, Some("27b"), "48b", &mut pending, &streak, 3),
        ModelChangeGate::Defer { streak: 2 }
    );
    assert_eq!(
        model_change_gate(true, Some("27b"), "48b", &mut pending, &streak, 3),
        ModelChangeGate::Commit { streak: 3 }
    );
    // A flapping target restarts the count — 0.5b/48b alternation never commits.
    assert_eq!(
        model_change_gate(true, Some("27b"), "0.5b", &mut pending, &streak, 3),
        ModelChangeGate::Defer { streak: 1 }
    );
    assert_eq!(
        model_change_gate(true, Some("27b"), "48b", &mut pending, &streak, 3),
        ModelChangeGate::Defer { streak: 1 }
    );
    // No ready incumbent (boot / post-wedge): commit immediately — recovery
    // must not wait out a streak.
    let mut p2 = None;
    assert_eq!(
        model_change_gate(false, Some("27b"), "48b", &mut p2, &streak, 3),
        ModelChangeGate::NotAChange
    );
    assert_eq!(
        model_change_gate(true, None, "48b", &mut p2, &streak, 3),
        ModelChangeGate::NotAChange
    );
}

// what this catches: a launch pool sized from anything but the published plan's LIVE budget. One
// launch keeps its pool and `--kv-unified` allocates it up front, so b4be7bc59 (the cold boot's
// first-tick 2,048 plan: 5 x 2,048) and 590df4bc3 (the host's physical total: 5 x 262,144, memory
// pressure 91%) were both rolled back on the M5 on 2026-11-03. Fixed inputs, never the host's RAM
// (Cormac on #5739): the pool is the larger of the plan's window x lanes and the fit at the live
// budget, so the M5 at its 23 GB live budget gets its plan's 257,536 and the IntelMac its fit.
#[test]
fn the_launch_pool_is_the_live_plans_never_the_physical_totals() {
    let launch_pool = super::super::launch_pool::launch_pool;
    let m5_trained = 262_144;
    let mut m5 = footprint_from_parts("ggml-org/Qwen3.6-35B-A3B-GGUF", 20_690_000_000, m5_trained, true, None)
        .expect("a footprint from the weights");
    m5.kv_per_token = 8_317;
    let m5_live = 23 * GB;
    assert_eq!(m5.window_within(m5_live, 5), 0, "the M5's live budget fits no window at five sequences beside the plan");
    assert_eq!(launch_pool(Some(&m5), m5_live, 128_768, 2, 5, m5_trained), 257_536, "the M5 launches its plan's 2 x 128,768");
    assert!(launch_pool(Some(&m5), m5_live, 128_768, 2, 5, m5_trained) < 5 * u64::from(m5_trained), "never 5 x the trained window (590df4bc3)");

    let intel_trained = 32_768;
    let mut intel = footprint_from_parts("intelmac", 5 * GB, intel_trained, true, None).expect("a footprint");
    intel.kv_per_token = 8_192;
    assert_eq!(launch_pool(Some(&intel), 30 * GB, 32_768, 2, 9, intel_trained), 294_912, "the IntelMac: its fit, 9 x 32,768");
    assert_eq!(launch_pool(None, 30 * GB, 32_768, 2, 9, intel_trained), 65_536, "no footprint: the plan's own window x lanes");
}


/// The M5's engine model on 2026-11-04 (`credit.rs` sizes it the same): Qwen3.6-35B-A3B, its
/// 20,419,565,568 bytes of weights, the fixture's 8,317 B/token and the catalog's trained window.
fn m5_qwen36() -> ModelFootprint {
    ModelFootprint {
        model_id: "ggml-org/Qwen3.6-35B-A3B-GGUF".into(),
        weights_bytes: 20_419_565_568,
        kv_per_token: 8_317,
        context_window: 262_144,
        capability_rank: 9,
        fixed_per_lane_bytes: 0,
    }
}

/// The engine the previous core left serving (pid, OS start time, port of its lane record).
fn old_engine() -> crate::inference::engine_residency::EngineIncarnation {
    crate::inference::engine_residency::EngineIncarnation { pid: 41_207, started_s: 1_762_282_000, port: 8_181 }
}

// what this catches (the M5 Pro, 64 GB, 2026-11-04 20:15:16-18Z, 87eccb3a7): two seconds into a
// boot, with the old engine still holding its memory and not yet credited (`credited_bytes=0
// usable_bytes=22,785,477,836`), the one launch sized `-c 125640 --parallel 5` and kept it for
// its life; three residents starved. A reading taken while an engine of ours that this daemon does
// not serve is registered sizes no launch: nothing launches on it, and only an engine already
// serving the model keeps its own pool.
#[test]
fn an_old_engine_still_holding_memory_never_sizes_the_launch_pool() {
    use super::super::engine_release::{EngineFacts, EngineRelease};
    use super::super::launch_pool::launch_pool;
    let m5 = m5_qwen36();
    let first = 22_785_477_836u64;
    // The plan's window x lanes was the launch's 125,640 (shown as 5 x 25,128); four residents + scratch.
    let (plan_window, plan_lanes, sequences) = (25_128u32, 5u32, 5u32);
    // The negative control, the defect to the token: that reading fits no window at five
    // sequences (weights + five staging buffers exceed it), so the launch took the plan's 125,640.
    assert_eq!(m5.window_within(first, sequences), 0);
    assert_eq!(launch_pool(Some(&m5), first, plan_window, plan_lanes, sequences, m5.context_window), 125_640);

    let held = EngineFacts { registered: Some(old_engine()), verified: None };
    let reading = EngineRelease::default().read(OwnCredit::ToApply, held, 1, first);
    assert_eq!(reading, BudgetReading::Transient, "credited 0, the old engine registered and not ours");
    let pool = LaunchPool::from_reading(reading, Some(&m5), plan_window, plan_lanes, sequences, m5.context_window);
    assert_eq!(pool, LaunchPool::AwaitSettled { sequences });
    assert_eq!(pool.commit(None), None, "no engine of the model runs: nothing launches on a transient");
    assert_eq!(pool.commit(Some(0)), None, "an unreadable running pool is no pool to keep");
    assert_eq!(pool.commit(Some(523_520)), Some((104_704, sequences)), "a running engine keeps its own pool");
}

// what this catches (the same boot, the other side): once the old engine is gone from the
// registry, its attribution off the board and a later scan taken, the reading settles and the
// fit sizes the pool against the M5 Pro's shared physical pool: weights + each sequence's staging
// buffer + KV. Inputs (2026-11-04): 45.6 GB reclaimable at 21:00Z after the old engine released
// (other apps holding ~15 GB compressed), 21-24 GB while it was still wired, the everyday
// (Comfort, 0.80) fill, the model above, five sequences. Derived, not asserted from memory: 36.48
// GB less the weights and five staging buffers leaves 9.68 GB of KV + prefill at 10,396 B/token,
// 186,212 tokens a sequence, so the pool clears the 523,520 the 18:49Z steady plan regrew to;
// while the old engine is still wired nothing fits at all.
#[test]
fn a_released_old_engine_settles_and_the_fit_sizes_the_pool_on_the_m5_pro() {
    use super::super::engine_release::{EngineFacts, EngineRelease};
    use crate::capacity::power_mode::PowerMode;
    let m5 = m5_qwen36();
    let fill = |reclaimable: u64| plan_fill(reclaimable, PowerMode::Comfort.serving_fraction());
    for still_wired in [21_000_000_000u64, 24_000_000_000] {
        assert_eq!(m5.window_within(fill(still_wired), 5), 0, "{still_wired} reclaimable: the old engine's memory holds no new pool");
    }
    let settled = fill(45_600_000_000);
    let mut release = EngineRelease::default();
    let held = EngineFacts { registered: Some(old_engine()), verified: None };
    let gone = EngineFacts { registered: None, verified: None };
    assert_eq!(release.read(OwnCredit::ToApply, held, 1, fill(24_000_000_000)), BudgetReading::Transient);
    assert_eq!(release.read(OwnCredit::ToApply, gone, 2, settled), BudgetReading::Transient, "the exit's own scan");
    let reading = release.read(OwnCredit::ToApply, gone, 3, settled);
    assert_eq!(reading, BudgetReading::Settled(settled), "a scan after the release");

    let fit = m5.window_within(settled, 5).min(m5.context_window);
    assert!((186_000..=186_300).contains(&fit), "the derivation above: {fit} a sequence");
    let pool = LaunchPool::from_reading(reading, Some(&m5), 25_128, 5, 5, m5.context_window);
    assert_eq!(pool, LaunchPool::Sized { per_sequence: fit, sequences: 5 }, "the fit, not the plan's 25,128");
    assert!(u64::from(fit) * 5 >= 523_520, "{fit} x 5 below the 18:49Z 523,520");
    assert_eq!(pool.commit(None), Some((fit, 5)), "a settled pool launches with no engine running");
}

// what this catches (the M5 Pro, 2026-11-04 20:15-20:35Z, 87eccb3a7): a cold boot launched its
// one pool from a reading taken while the old engine still held its memory (`credited_bytes=0
// usable_bytes=22,785,477,836`) as `-c 125640 --parallel 5`, and for twenty minutes three of the
// four residents were dark: no grant, no request reaching the engine. The Directed gate fixture
// cannot see that (it never launches a pool and never grants). This runs the cold boot end to end
// on the real owners: the budget owner's reading over a memory authority the test drives, the
// old engine's release observed by that authority, the planner, the reconcile's launch, the grant
// tick over four minds' requirements built from their working set the way their turns record
// them, and each mind's request admitted at the engine's endpoint and completed by the fake
// engine's completion route.
#[tokio::test]
async fn a_cold_boot_launches_after_the_old_engine_is_released_and_every_resident_works() {
    use super::credit::until_the_board;
    use crate::inference::planning::window_allocator::observed::Observed;
    use axum::{routing::post, Router};

    // The M5 Pro: hw.memsize 68,719,476,736 bytes of unified memory. At 20:15Z, the old engine
    // still wired, the board read 28,481,847,295 reclaimable (Comfort fill 22,785,477,836); after
    // it released, 45.6 GB (21:00Z).
    let capacity = 68_719_476_736u64;
    let still_wired_free = 28_481_847_295u64;
    let released_free = 45_600_000_000u64;
    let source = Arc::new(crate::resources::MockCapacitySource::new(serving_pool_kind(), capacity));
    source.set_used(capacity - still_wired_free);
    let authority = ticking_resource_daemon(source.clone());
    let mut server = FakeServer::healthy(Arc::new(AtomicUsize::new(0)), true);
    server.shared_pool = true;
    let launched = server.launched.clone();
    let serves = server.serves.clone();
    let mut daemon = daemon_on_authority(Arc::new(server), capacity, authority.clone());
    // The PLAN's sustained minimum in a test's time, so the still-wired reading ages out of the
    // plan; the launch's settledness is the release, not this window.
    let window = Duration::from_millis(300);
    daemon.set_budget_window(window);
    // The M5's model; the plan's candidate carries its header KV rate (8,317 B/token), and the
    // launch sizes with the footprint the plan published.
    let model_id = "cold-boot-fixture/Qwen3.6-35B-A3B-GGUF";
    let resolve = |id: &str| {
        let mut model = fake_model(id);
        model.weights_bytes = Some(20_419_565_568);
        model
    };
    daemon.set_model_resolver(Arc::new(move |id: &str| Some(resolve(id))));
    daemon.set_engine_pid(Arc::new(|| Some(4_242)));
    let candidate = ModelFootprint { model_id: model_id.into(), ..m5_qwen36() };
    // The old engine the previous core left: registered (pidfile + record + a live llama-server).
    let old = old_engine();
    let old_lane = Arc::new(parking_lot::Mutex::new(Some(LaneRecord {
        pid: old.pid,
        port: old.port,
        role: crate::inference::lane_registry::LaneRole::Live,
        model: model_id.into(),
        context_window: 104_704,
        lanes: 5,
        page_dir: None,
        engine_bin: None,
        weights: None,
        started_s: old.started_s,
        placement: None,
    })));
    let registry = old_lane.clone();
    daemon.set_inherited_lane(Arc::new(move || registry.lock().clone()));

    // Four residents, measured the way their turns record them: Aiko's view overflowed her grant,
    // so her need is a censored lower bound; three exact needs near 40k (the shape of the M5's
    // three; their exact values are not in the 20:15Z receipts).
    let aiko = uuid::Uuid::new_v4();
    let others = [(uuid::Uuid::new_v4(), 40_960u32), (uuid::Uuid::new_v4(), 39_424), (uuid::Uuid::new_v4(), 41_216)];
    let minds: Vec<(uuid::Uuid, u32)> = std::iter::once((aiko, 114_592)).chain(others).collect();
    let roster: Vec<uuid::Uuid> = minds.iter().map(|(mind, _)| *mind).collect();
    daemon.set_live_residents(Arc::new(move || roster.clone()));
    daemon.working_set.record_need_in_memory(aiko, Observed::AtLeast { at_least: 114_592 }, 1);
    for (mind, need) in others {
        daemon.working_set.record_in_memory(mind, need, 1);
        daemon.working_set.record_need_in_memory(mind, Observed::Exact(need), 1);
    }
    let needs: u64 = minds.iter().map(|(_, need)| u64::from(*need)).sum();
    let plan = |daemon: &ServingDaemonModule| {
        let budget = daemon.host_budget_snapshot();
        let revision = daemon.intent.snapshot().revision;
        daemon.plan_until_decided(&budget, revision, || (vec![candidate.clone()], vec![candidate.clone()]));
        budget
    };

    // (1) The old engine still holds its memory: the reading is transient however long it holds,
    // no pool is committed from it and, with no engine of the model answering, nothing launches.
    until_the_board(&authority, |r| r.bytes == still_wired_free).await;
    for _ in 0..3 {
        let held = plan(&daemon);
        assert_eq!(held.credit, OwnCredit::ToApply, "the old engine's bytes not credited yet");
        assert_eq!(held.launch, BudgetReading::Transient, "the old engine is registered and not ours");
        tokio::time::sleep(window).await;
    }
    assert!(
        matches!(daemon.engine_pool(&resolve(model_id), 25_128, 5), LaunchPool::AwaitSettled { .. }),
        "the 20:15Z launch's plan (5 x 25,128) sizes no pool from it"
    );
    if let Some(reconcile) = daemon.reconcile_to_plan() {
        reconcile.await.expect("the reconcile task completes"); // expect: the fixture's own task
    }
    assert_eq!(serves.load(Ordering::SeqCst), 0, "nothing launched while the old engine held memory");
    assert!(launched.lock().is_empty());

    // (2) The old engine exits and releases: gone from the registry, its bytes back on the board.
    // The reading that sees it gone is still transient; one at a later scan settles, and the launch
    // commits its pool from it.
    *old_lane.lock() = None;
    source.set_used(capacity - released_free);
    until_the_board(&authority, |r| r.bytes == released_free).await;
    let exited = plan(&daemon);
    assert_eq!(exited.launch, BudgetReading::Transient, "the exit's own scan is not yet a release");
    let seen = authority.replacement_budget_snapshot(SERVING_CONSUMER_ID, serving_pool_kind()).scan;
    until_the_board(&authority, |r| r.scan > seen).await;
    tokio::time::sleep(window * 2).await;
    let settled = plan(&daemon);
    let BudgetReading::Settled(settled_bytes) = settled.launch else {
        panic!("released and scanned, the reading settles: {:?}", settled.launch); // panic: the assertion under test
    };
    assert_eq!(settled.host.usable_bytes, settled_bytes, "the still-wired reading has aged out of the plan too");
    daemon
        .reconcile_to_plan()
        .expect("the settled plan reconciles") // expect: the assertion under test
        .await
        .expect("the reconcile task completes"); // expect: the fixture's own task
    assert_eq!(serves.load(Ordering::SeqCst), 1, "one launch");
    let (per_sequence, sequences) = launched.lock().first().copied().expect("the launch's -c and --parallel"); // expect: asserted one launch above
    let pool = u64::from(per_sequence) * u64::from(sequences);
    assert!(sequences >= 5, "four residents and the scratch sequence: --parallel {sequences}");
    let fit_pool = u64::from(candidate.window_within(settled_bytes, sequences).min(candidate.context_window)) * u64::from(sequences);
    assert!(pool >= fit_pool, "-c {pool} below the settled reading's fit {fit_pool}");
    assert!(pool >= needs, "-c {pool} cannot hold the four needs ({needs})");

    // (3) The grant tick grants every mind at least her need from the launched pool.
    assert!(daemon.serving_tx.borrow().ready, "the launched engine serves");
    daemon.publish_grants();
    let grants = daemon.subscribe_grants().borrow().clone().expect("grants from the launched pool"); // expect: the assertion under test
    assert_eq!(u64::from(grants.pool_tokens), pool, "the grants' pool is the engine's own");
    for (mind, need) in &minds {
        assert!(
            grants.by_mind.get(mind).is_some_and(|granted| granted >= need),
            "{mind} (need {need}) not granted it: {:?}",
            grants.by_mind
        );
    }

    // (4) Each mind's request is admitted at the engine's endpoint and its generation completes.
    let completed = Arc::new(parking_lot::Mutex::new(Vec::<Option<String>>::new()));
    let app = Router::new().route(
        "/v1/chat/completions",
        post({
            let completed = completed.clone();
            move |axum::Json(body): axum::Json<serde_json::Value>| {
                let completed = completed.clone();
                async move {
                    completed.lock().push(body["user"].as_str().map(str::to_owned));
                    axum::Json(serde_json::json!({
                        "choices": [{"index": 0, "message": {"role": "assistant", "content": "done"}, "finish_reason": "stop"}],
                        "usage": {"prompt_tokens": 8, "completion_tokens": 1, "total_tokens": 9}
                    }))
                }
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("a loopback port"); // expect: fixture setup
    let root = format!("http://{}", listener.local_addr().expect("its address")); // expect: fixture setup
    let engine = tokio::spawn(async move { axum::serve(listener, app).await });
    let endpoint = crate::inference::slots::directory().endpoint(&root);
    let client = reqwest::Client::new();
    for (mind, _) in &minds {
        let admitted = endpoint.admit().await.expect("her request is admitted"); // expect: the assertion under test
        let reply: serde_json::Value = client
            .post(format!("{root}/v1/chat/completions"))
            .json(&serde_json::json!({
                "model": model_id,
                "user": mind.to_string(),
                "messages": [{"role": "user", "content": "work"}],
                "max_tokens": 1,
                "n_ctx": grants.by_mind.get(mind),
            }))
            .send()
            .await
            .expect("the engine answers") // expect: the assertion under test
            .json()
            .await
            .expect("a completion body"); // expect: the assertion under test
        assert_eq!(reply["choices"][0]["finish_reason"], "stop", "{mind}'s generation completes");
        drop(admitted);
    }
    let expected: Vec<Option<String>> = minds.iter().map(|(mind, _)| Some(mind.to_string())).collect();
    assert_eq!(*completed.lock(), expected, "every resident's work reached the engine and completed");
    engine.abort();
}
