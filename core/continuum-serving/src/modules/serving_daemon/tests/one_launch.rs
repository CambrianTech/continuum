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

/// Serving's own footprint on the board, as `ServingConsumer` reports it once the engine is up:
/// the plan's budget is then "steady" (`serving_footprint_reported`), the state the pool-growth
/// bridge required before it quiesced admission.
struct SteadyServing(u64);

#[async_trait::async_trait]
impl crate::resources::ResourceConsumer for SteadyServing {
    fn consumer_id(&self) -> &str {
        SERVING_CONSUMER_ID
    }
    fn footprint(&self) -> Vec<crate::resources::ConsumerFootprint> {
        vec![crate::resources::ConsumerFootprint {
            kind: serving_pool_kind(),
            bytes: self.0,
            detail: "test: the running engine".into(),
        }]
    }
    async fn reclaim(&self, _request: crate::resources::ReclaimRequest) -> crate::resources::ReclaimOutcome {
        crate::resources::ReclaimOutcome::refused("test: serving holds its engine")
    }
}

// what this catches (the M5, 2026-11-04 18:00:46Z-18:16Z): an admission hold whose release
// depended on a relaunch the launch planner refused. With serving's footprint on the board (a
// steady plan) and the running pool (523,520-654,336 tokens) below the plan's full lanes x window,
// the pool-growth bridge quiesced admission (`serving.pool.grow_quiesced drained_ms=289855`) and
// waited on an ensure the planner answered only with `serving.launch.plan` every 5 s: four
// residents, `turn_ingress in_flight=4`, zero acts for 16 minutes until an operator stopped
// serving (also 14:56Z). The same steady plan against the same-model engine (a known incarnation,
// a quarter of the pool it wants) now never relaunches, never drains, and a resident's next
// request through the real lane gate is admitted at once.
//
// Isolation: the process-wide lane gate (`resource_admission::LANES`) is the one the bridge held,
// so it is the one asserted. Nothing else in this crate's test binary takes a serving lane (only
// planning sets the count, floored at 1), so one directed request cannot be starved by a
// parallel test; the bound is the old failure's shape, not a race window.
#[tokio::test]
async fn a_steady_plan_wanting_more_pool_than_the_engine_holds_leaves_admission_open() {
    let serves = Arc::new(AtomicUsize::new(0));
    let mut server = FakeServer::healthy(serves.clone(), true);
    server.active = Some("coder-14b".into());
    server.incarnation = Some(crate::inference::engine_residency::EngineIncarnation {
        pid: 4_000_000_002,
        started_s: 1,
        port: 1,
    });
    let idles = server.idles.clone();
    let authority = ticking_resource_daemon(Arc::new(crate::resources::MockCapacitySource::new(
        serving_pool_kind(),
        53 * GB,
    )));
    authority.add_consumer(Arc::new(SteadyServing(12 * GB)));
    let mut board = authority.subscribe();
    tokio::time::timeout(
        std::time::Duration::from_secs(10),
        board.wait_for(|b| {
            b.attributions
                .iter()
                .any(|a| a.consumer_id == SERVING_CONSUMER_ID && a.bytes > 0)
        }),
    )
    .await
    .expect("the authority ticks serving's footprint onto the board")
    .expect("the board channel stays open");
    let daemon = daemon_on_authority(Arc::new(server), 53 * GB, authority);
    assert!(daemon.serving_footprint_reported(), "the plan is steady: the receipt's state");

    let budget = HostBudget {
        usable_bytes: 45 * GB,
        perf_cores: 6,
    };
    let candidates = vec![footprint_from_parts("coder-14b", 9 * GB, 8192, true, None).unwrap()];
    daemon.publish_plan(budget, &candidates, &candidates);
    let (plan_window, plan_lanes) = {
        let published = daemon.plan_tx.borrow();
        let plan = published.plan.as_ref().expect("plan published");
        (plan.served_context_window, plan.lanes)
    };
    // The engine holds a quarter of what the plan's window x lanes asks for.
    let held_pool = (plan_window.saturating_mul(plan_lanes) / 4).max(1);
    daemon.served_pool.store(held_pool, Ordering::Relaxed);
    let mut ready = ready_snapshot();
    ready.lanes = plan_lanes;
    ready.served_context_window = held_pool / plan_lanes.max(1);
    let _ = daemon.serving_tx.send_replace(ready);
    for tick in 0..10 {
        assert!(
            daemon.reconcile_to_plan().is_none(),
            "tick {tick}: a plan wanting a larger pool is not a relaunch"
        );
        // Let anything a tick spawned (the bridge's quiesce task was one) run before the next.
        tokio::task::yield_now().await;
    }
    assert_eq!(serves.load(Ordering::SeqCst), 0, "never relaunched into a larger pool");
    assert_eq!(idles.load(Ordering::SeqCst), 0, "never drained for one");
    assert_eq!(daemon.served_pool.load(Ordering::Relaxed), held_pool, "the pool stays what the launch sized");

    // A resident's next generation enters admission at once: nothing holds the gate.
    let permit = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        crate::inference::resource_admission::acquire_serving_lane(
            Some(uuid::Uuid::from_u128(0x75ece751)),
            crate::inference::resource_admission::LanePriority::Directed,
            None,
        ),
    )
    .await
    .expect("admission stays open: a resident's request is admitted, never held for a relaunch");
    drop(permit);
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
