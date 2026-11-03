#![cfg(test)]

use super::*;

// what this catches: the daemon relaunching a ready engine because the plan wants a different
// window or lane count. The M5's plan read 1, 2 and 3 lanes of four residents and its engine
// reloaded 40 times (2026-11-02/03). A ready lane serving the right model and genome to the
// seated population is never relaunched, however far the plan's window drifts; when the
// population moves, the launch path republishes each mind's share without a relaunch.
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
    let _ = daemon.serving_tx.send_replace(ready(daemon.seated()));
    for tick in 0..10 {
        assert!(
            daemon.reconcile_to_plan().is_none(),
            "tick {tick}: a quarter of the plan's window is not a relaunch"
        );
    }
    let _ = daemon.serving_tx.send_replace(ready(daemon.seated() + 1));
    if let Some(republish) = daemon.reconcile_to_plan() {
        republish.await.unwrap();
    }
    assert_eq!(
        serves.load(Ordering::SeqCst),
        0,
        "the population moved: republished, never relaunched"
    );
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
