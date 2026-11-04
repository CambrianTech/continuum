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

// what this catches (the M5, 2026-11-04 20:15:16-18Z, 87eccb3a7, no growth relaunch): the cold
// boot's first reading, `serving.plan.branch credited_bytes=0 usable_bytes=22,785,477,836` on a
// 128 GB machine, sized the one launch two seconds into the boot (`-c 125640 --parallel 5`); the
// engine kept 125,696 for its life and three residents starved. A reading inside its view's first
// window is transient: it sizes no launch however it moves, nothing launches on it, and only an
// engine already serving the model keeps its own pool.
#[test]
fn a_cold_boot_transient_reading_never_sizes_the_launch_pool() {
    use super::super::launch_pool::launch_pool;
    use super::super::own_credit::CreditedReadings;
    let window = crate::inference::llama_server::READY_TIMEOUT;
    let window_ms = window.as_millis() as u64;
    let m5 = m5_qwen36();
    let first = 22_785_477_836u64;
    // The plan's window x lanes was the launch's 125,640 (shown as 5 x 25,128); four residents + scratch.
    let (plan_window, plan_lanes, sequences) = (25_128u32, 5u32, 5u32);
    // The negative control, the defect to the token: that reading fits no window at five
    // sequences (weights + five compute floors exceed it), so the launch took the plan's 125,640.
    assert_eq!(m5.window_within(first, sequences), 0);
    assert_eq!(launch_pool(Some(&m5), first, plan_window, plan_lanes, sequences, m5.context_window), 125_640);

    let boot = 1_000_000u64;
    let mut readings = CreditedReadings::new(window);
    let observed = readings.observe(boot, OwnCredit::ToApply, first);
    assert_eq!(observed.sustained_min, first, "the plan still plans from it");
    assert_eq!(observed.launch, BudgetReading::Transient, "credited 0, two seconds into the boot");
    // Rising through the first window (2026-10-18's 22.9 -> 41.9 GB in 22 s) changes nothing.
    for (after_ms, bytes) in [(5_000, 29_800_000_000u64), (22_000, 41_900_000_000), (window_ms - 1, 41_900_000_000)] {
        assert_eq!(readings.observe(boot + after_ms, OwnCredit::ToApply, bytes).launch, BudgetReading::Transient, "+{after_ms} ms");
    }

    let pool = LaunchPool::from_reading(observed.launch, Some(&m5), plan_window, plan_lanes, sequences, m5.context_window);
    assert_eq!(pool, LaunchPool::AwaitSettled { sequences });
    assert_eq!(pool.commit(None), None, "no engine of the model runs: nothing launches on a transient");
    assert_eq!(pool.commit(Some(0)), None, "an unreadable running pool is no pool to keep");
    assert_eq!(pool.commit(Some(523_520)), Some((104_704, sequences)), "a running engine keeps its own pool");
}

// what this catches (the same 20:15Z boot, the other side): once the view has outlived its first
// window, the launch reading is the sustained minimum of only the readings after it, so the boot
// trough cannot fix the pool, while the plan's own minimum is unchanged. Inputs: the 20:15Z first
// reading (22,785,477,836), the M5's 2026-10-18 rise to a steady 41.9 GB (the ledger flat at
// 41.68 GB; the 11-04 steady reading itself is not in the receipts), the M5 model above and five
// sequences. The pool is at least the 523,520 the 18:49Z steady plan regrew to.
#[test]
fn a_settled_reading_sizes_the_launch_pool_from_the_fit() {
    use super::super::own_credit::CreditedReadings;
    let window = crate::inference::llama_server::READY_TIMEOUT;
    let window_ms = window.as_millis() as u64;
    let m5 = m5_qwen36();
    let (first, steady) = (22_785_477_836u64, 41_900_000_000u64);
    let boot = 1_000_000u64;
    let mut readings = CreditedReadings::new(window);
    readings.observe(boot, OwnCredit::ToApply, first);
    for second in 1..(window_ms / 1_000) {
        readings.observe(boot + second * 1_000, OwnCredit::ToApply, steady);
    }
    let at_window = readings.observe(boot + window_ms, OwnCredit::ToApply, steady);
    assert_eq!(at_window.sustained_min, first, "the plan's minimum still holds the boot reading");
    assert_eq!(at_window.launch, BudgetReading::Settled(steady), "the launch reading excludes the first window");

    let fit = m5.window_within(steady, 5).min(m5.context_window);
    let pool = LaunchPool::from_reading(at_window.launch, Some(&m5), 25_128, 5, 5, m5.context_window);
    assert_eq!(pool, LaunchPool::Sized { per_sequence: fit, sequences: 5 }, "the fit, not the plan's 25,128");
    assert!(u64::from(fit) * 5 >= 523_520, "{fit} x 5 below the 18:49Z 523,520");
    assert_eq!(pool.commit(None), Some((fit, 5)), "a settled pool launches with no engine running");

    // Within the settled view a fall still shows at once, and a new view starts transient again.
    let fallen = readings.observe(boot + window_ms + 1_000, OwnCredit::ToApply, 30_000_000_000);
    assert_eq!(fallen.launch, BudgetReading::Settled(30_000_000_000));
    let engine_up = readings.observe(boot + window_ms + 2_000, OwnCredit::Included { own_bytes: 36 * GB }, steady);
    assert_eq!(engine_up.launch, BudgetReading::Transient, "an engine appearing is a new view's first window");
}

// what this catches (the M5, 2026-11-04 20:15-20:35Z, 87eccb3a7): a cold boot launched its one pool
// from the first, transient budget reading (`credited_bytes=0 usable_bytes=22,785,477,836` on a
// 128 GB machine) as `-c 125640 --parallel 5`, and for twenty minutes three of the four residents
// were dark: no grant, no request reaching the engine. The Directed gate fixture cannot see that
// (it never launches a pool and never grants). This runs the cold boot end to end on the real
// owners: the budget owner's reading over a memory authority the test drives, the planner, the
// reconcile's launch, the grant tick over four minds' requirements built from their working set
// the way their turns record them, and each mind's request admitted at the engine's endpoint and
// completed by the fake engine's completion route.
#[tokio::test]
async fn a_cold_boot_launches_from_the_settled_reading_and_every_resident_works() {
    use super::super::launch_pool::LaunchPool;
    use super::credit::until_the_board;
    use crate::inference::planning::window_allocator::observed::Observed;
    use axum::{routing::post, Router};

    // The M5: 128 GB of unified memory. The 20:15Z first reading is the everyday (Comfort, 0.80)
    // fill of 28,481,847,295 free bytes: 22,785,477,836. Settled: the boot's leftovers gone, 10 GB
    // of others staying resident.
    let capacity = 128 * GB;
    let transient_free = 28_481_847_295u64;
    let settled_free = capacity - 10 * GB;
    let source = Arc::new(crate::resources::MockCapacitySource::new(serving_pool_kind(), capacity));
    source.set_used(capacity - transient_free);
    let authority = ticking_resource_daemon(source.clone());
    let mut server = FakeServer::healthy(Arc::new(AtomicUsize::new(0)), true);
    server.shared_pool = true;
    let launched = server.launched.clone();
    let serves = server.serves.clone();
    let mut daemon = daemon_on_authority(Arc::new(server), capacity, authority.clone());
    // One relaunch's duration in a test's time: the view's first window is the boot's transient.
    let window = Duration::from_millis(300);
    daemon.set_budget_window(window);
    // The M5's model: its weights, its trained window (`fake_model`'s 262,144), and for the plan its
    // header KV rate (8,317 B/token). The launch's fit reads `footprint_for` of the resolved row.
    let model_id = "cold-boot-fixture/Qwen3.6-35B-A3B-GGUF";
    let resolve = |id: &str| {
        let mut model = fake_model(id);
        model.weights_bytes = Some(20_419_565_568);
        model
    };
    daemon.set_model_resolver(Arc::new(move |id: &str| Some(resolve(id))));
    daemon.set_engine_pid(Arc::new(|| Some(4_242)));
    let candidate = ModelFootprint { model_id: model_id.into(), ..m5_qwen36() };

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

    // (1) The boot's first reading is transient: whatever the plan makes of it, no pool is
    // committed from it and, with no engine running, nothing launches.
    until_the_board(&authority, |r| r.bytes == transient_free).await;
    let first = plan(&daemon);
    assert_eq!(first.credit, OwnCredit::ToApply, "a cold boot: no engine, nothing credited");
    assert_eq!(first.launch, BudgetReading::Transient, "the boot's first reading");
    assert!(
        matches!(daemon.engine_pool(&resolve(model_id), 25_128, 5), LaunchPool::AwaitSettled { .. }),
        "the 20:15Z launch's plan (5 x 25,128) sizes no pool from it"
    );
    if let Some(reconcile) = daemon.reconcile_to_plan() {
        reconcile.await.expect("the reconcile task completes"); // expect: the fixture's own task
    }
    assert_eq!(serves.load(Ordering::SeqCst), 0, "nothing launched on the transient");
    assert!(launched.lock().is_empty());

    // (2) The memory comes back and the view outlives its first window: the reading settles, and
    // the launch commits its pool from it.
    source.set_used(capacity - settled_free);
    until_the_board(&authority, |r| r.bytes == settled_free).await;
    tokio::time::sleep(window * 2).await;
    let settled = plan(&daemon);
    let BudgetReading::Settled(settled_bytes) = settled.launch else {
        panic!("past the first window the reading settles: {:?}", settled.launch); // panic: the assertion under test
    };
    assert_eq!(settled.host.usable_bytes, settled_bytes, "the boot reading has aged out of the plan's minimum too");
    daemon
        .reconcile_to_plan()
        .expect("the settled plan reconciles") // expect: the assertion under test
        .await
        .expect("the reconcile task completes"); // expect: the fixture's own task
    assert_eq!(serves.load(Ordering::SeqCst), 1, "one launch");
    let (per_sequence, sequences) = launched.lock().first().copied().expect("the launch's -c and --parallel"); // expect: asserted one launch above
    let pool = u64::from(per_sequence) * u64::from(sequences);
    assert!(sequences >= 5, "four residents and the scratch sequence: --parallel {sequences}");
    let fit = footprint_for(&resolve(model_id)).expect("the M5 row has a footprint"); // expect: weights are set above
    let fit_pool = u64::from(fit.window_within(settled_bytes, sequences).min(fit.context_window)) * u64::from(sequences);
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
