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
    let candidates = vec![crate::inference::planning::serving_plan::footprint::calibrated_from_parts("coder-14b", 9 * GB, 8192, true, None).unwrap()];
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
    let launch_pool = super::super::launch::launch_pool;
    let none = super::super::launch::Seated { need: 0, proven: None };
    let m5_trained = 262_144;
    let mut m5 = crate::inference::planning::serving_plan::footprint::calibrated_from_parts("ggml-org/Qwen3.6-35B-A3B-GGUF", 20_690_000_000, m5_trained, true, None)
        .expect("a footprint from the weights");
    m5.kv_per_token = 8_317;
    let m5_live = 23 * GB;
    assert_eq!(m5.window_within(m5_live, 5), 0, "the M5's live budget fits no window at five sequences beside the plan");
    assert_eq!(launch_pool(Some(&m5), m5_live, 128_768, 2, 5, m5_trained, none), 257_536, "the M5 launches its plan's 2 x 128,768");
    assert!(launch_pool(Some(&m5), m5_live, 128_768, 2, 5, m5_trained, none) < 5 * u64::from(m5_trained), "never 5 x the trained window (590df4bc3)");

    let intel_trained = 32_768;
    let mut intel = crate::inference::planning::serving_plan::footprint::calibrated_from_parts("intelmac", 5 * GB, intel_trained, true, None).expect("a footprint");
    intel.kv_per_token = 8_192;
    assert_eq!(launch_pool(Some(&intel), 30 * GB, 32_768, 2, 9, intel_trained, none), 294_912, "the IntelMac: its fit, 9 x 32,768");
    assert_eq!(launch_pool(None, 30 * GB, 32_768, 2, 9, intel_trained, none), 65_536, "no footprint: the plan's own window x lanes");
}

// what this catches (the M5, 2026-11-07 16:14Z, card 9ffcb1b5): a relaunch after an engine exit read
// a gap budget (the dead engine credited nothing: usable 0) and launched its plan's 2 lanes, 176,640
// split over 5 sequences = 35,328 each, while the five minds held 44-70k: every mind refused for 45
// minutes. The floor is the seated held needs, each capped at the trained window, and capped in sum
// by the pool this model already ran at, never by the gap reading and never above what was proven.
#[test]
fn a_relaunch_from_a_gap_budget_holds_every_seated_need_up_to_the_proven_pool() {
    use super::super::launch::{launch_pool, Seated};
    let trained = 262_144;
    let mut m5 = crate::inference::planning::serving_plan::footprint::calibrated_from_parts("ggml-org/Qwen3.6-35B-A3B-GGUF", 20_690_000_000, trained, true, None)
        .expect("a footprint from the weights");
    m5.kv_per_token = 8_317;
    let gap = 0;
    // Benchy, Aris, Aiko (her 902,158 is a measurement defect, capped at the trained window), and two at their turns' sizes.
    let held: u64 = [136_694_u64, 70_538, 902_158, 56_431, 44_252].iter().map(|need| (*need).min(u64::from(trained))).sum();
    assert_eq!(held, 570_059, "test: the capped held needs");
    let none = Seated { need: 0, proven: None };
    assert_eq!(launch_pool(Some(&m5), gap, 88_320, 2, 5, trained, none), 176_640, "today's relaunch, reproduced: 5 x 35,328");
    let proven = 997_390;
    assert_eq!(launch_pool(Some(&m5), gap, 88_320, 2, 5, trained, Seated { need: held, proven: Some(proven) }), held, "every seated need is held");
    assert_eq!(launch_pool(Some(&m5), gap, 88_320, 2, 5, trained, Seated { need: held, proven: Some(300_000) }), 300_000, "never above the proven pool");
    assert_eq!(launch_pool(Some(&m5), gap, 88_320, 2, 5, trained, Seated { need: held, proven: None }), 176_640, "nothing proven, nothing floored: the plan's own pool");
    let steady = 60 * GB;
    let fit = launch_pool(Some(&m5), steady, 88_320, 2, 5, trained, none);
    assert_eq!(launch_pool(Some(&m5), steady, 88_320, 2, 5, trained, Seated { need: held, proven: None }), fit, "a steady budget: the floor (proven 0) never lifts the pool past its live fit");
}

// what this catches (the M5, 2026-11-07 19:08Z, #5835 -> #5837): engine_pool held a planned_budget
// guard as a temporary in launch_pool's argument list while seated_for, another argument, locked
// the same (non-reentrant) mutex: the serving reconcile deadlocked and the core never bound its
// socket. engine_pool on a real daemon with a planned budget set, under both credit facts, returns;
// a deadlock fails here in seconds instead of wedging a node.
#[tokio::test(flavor = "multi_thread")]
async fn engine_pool_returns_with_a_planned_budget_set() {
    for credit in [OwnCredit::Included { own_bytes: 20 * GB }, OwnCredit::ToApply] {
        let daemon = Arc::new(daemon_with(Arc::new(FakeServer::healthy(Arc::new(AtomicUsize::new(0)), true))));
        *daemon.planned_budget.lock() = Some(PlannedBudget { usable_bytes: 23 * GB, credit });
        let (tx, rx) = std::sync::mpsc::channel();
        let sized = Arc::clone(&daemon);
        std::thread::spawn(move || {
            let pool = sized.engine_pool(&fake_model("m"), 32_768, 2);
            let _ = tx.send(pool); // the receiver may have given up; the assertion below says so
        });
        let pool = rx.recv_timeout(std::time::Duration::from_secs(10));
        assert!(pool.is_ok(), "engine_pool did not return with a planned budget set ({credit:?}): a lock held across its own relock");
    }
}
