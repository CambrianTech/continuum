#![cfg(test)]
//! The plan budget's credit fact (`own_credit`), from the pure arms to the real owner
//! (`host_budget_snapshot`) over a memory authority the test drives.

use super::*;

// what this catches (the M5, 2026-11-04 16:41Z): a core-only restart onto an inherited engine
// whose pool had grown to -c 654,295 read system memory below the Eco line, most of it that
// engine's own bytes, and planned Eco's 0.55 of a budget that was the engine alone
// (`serving.plan.branch credited_bytes=36,051,076,408 usable_bytes=19,828,092,024`), below the
// model's 20,419,565,568 bytes of weights: `candidate_cannot_speak`, the starved plan retired its
// record, and no resident was hosted for 9 minutes. The pressure is read in the budget's own
// view, so the credited engine is not an external squeeze: the mode stays Comfort and the
// inherited engine's model plans at its fit, above the residents' requirement, not the floor.
#[test]
fn a_core_restart_over_an_engine_larger_than_free_memory_plans_that_engine_not_the_floor() {
    use crate::capacity::power_mode::{PowerMode, ECO_ENTER_BYTES};
    use crate::gpu::monitor::MemoryMode;
    use crate::inference::planning::serving_plan::persona_lane_holds;
    use super::own_credit::{own_bytes_in_system_memory, plan_by_credit, pressure_mode};
    // The probes' numbers, 16:41:33Z.
    let credited = 36_051_076_408u64;
    let requirement = 16_521u32;
    let engine = ModelFootprint {
        model_id: "ggml-org/Qwen3.6-35B-A3B-GGUF".into(),
        weights_bytes: 20_419_565_568,
        kv_per_token: 8_317, // the M5 fixture's per-token cost (one_launch.rs)
        context_window: 262_144, // the catalog row's trained window
        capability_rank: continuum_kernel::capability::Capability::Proxy { score: 9 },
        fixed_per_lane_bytes: 0,
    };
    // The budget was exactly the credit: the board had nothing free beside the engine. System
    // memory free is not in the probes; any reading under the Eco line reproduces the case.
    let system_free = ECO_ENTER_BYTES / 2;
    let unified = own_bytes_in_system_memory(ResourceKind::Vram, Some(MemoryMode::Unified));
    assert!(unified, "on unified memory serving's bytes are system memory");
    let credit = OwnCredit::of_own_bytes(credited);
    let demand = ServingDemand::new(4, Some(requirement));
    let plan_at = |mode: PowerMode| {
        let budget = HostBudget { usable_bytes: plan_fill(credited, mode.serving_fraction()), perf_cores: 10 };
        plan_by_credit(credit, &budget, std::slice::from_ref(&engine), Some(&engine.model_id), &demand)
            .map(|t| t.plan)
    };

    // The defect, to the byte: the mode read without the credit is Eco, and Eco's fill of the
    // engine-only budget is the probe's usable_bytes, below the weights.
    let blind = pressure_mode(system_free, OwnCredit::ToApply, unified, Some(PowerMode::Comfort));
    assert_eq!(blind, PowerMode::Eco);
    assert_eq!(plan_fill(credited, blind.serving_fraction()), 19_828_092_024, "the probe's usable_bytes");
    assert!(
        !plan_at(blind).is_some_and(|p| p.fits_on_gpu && persona_lane_holds(p.served_context_window, Some(requirement))),
        "the negative control: Eco's fill cannot host a resident, so the assertion below is not vacuous"
    );

    // In the budget's view the engine's own bytes are not pressure, from either side of the band.
    for previous in [Some(PowerMode::Comfort), Some(PowerMode::Eco), None] {
        assert_eq!(pressure_mode(system_free, credit, unified, previous), PowerMode::Comfort, "from {previous:?}");
    }
    let plan = plan_at(PowerMode::Comfort).expect("the inherited engine's model plans"); // expect: the assertion under test
    assert_eq!(plan.base_model.model_id, engine.model_id, "the engine it inherited, never a flight to a smaller one");
    assert!(plan.fits_on_gpu);
    assert!(
        persona_lane_holds(plan.served_context_window, Some(requirement)),
        "window {} holds the residents' {requirement}: not the floor",
        plan.served_context_window
    );
}

// what this catches: owned bytes credited twice. A budget whose read already holds serving's own
// bytes plans the at-rest arm at its fit, never that budget plus the weights again: the 5090,
// 2026-09-17, planned 2 x 70,093 on a card whose fit was 2 x 47,098 that way. The M5 16:41Z
// repair adds the credit to the pressure reading, and only once: never for a budget that does
// not hold it, never VRAM to system memory.
#[test]
fn owned_bytes_are_never_credited_twice() {
    use crate::gpu::monitor::MemoryMode;
    use super::own_credit::{own_bytes_in_system_memory, plan_by_credit};
    let m = ModelFootprint {
        model_id: "qwen3.8-27b".into(),
        weights_bytes: 17 * GB,
        kv_per_token: 65_536,
        context_window: 262_144,
        capability_rank: continuum_kernel::capability::Capability::Proxy { score: 9 },
        fixed_per_lane_bytes: 0,
    };
    let at_rest = HostBudget { usable_bytes: 32 * GB, perf_cores: 8 };
    let demand = ServingDemand::new(2, Some(120_000));
    let included = OwnCredit::of_own_bytes(m.weights_bytes);
    assert_eq!(included, OwnCredit::Included { own_bytes: m.weights_bytes });
    assert_eq!(OwnCredit::of_own_bytes(0), OwnCredit::ToApply, "nothing reported is nothing credited");

    let once = plan_by_credit(included, &at_rest, std::slice::from_ref(&m), Some(&m.model_id), &demand)
        .expect("an at-rest plan") // expect: the assertion under test
        .plan;
    assert_eq!(
        once.served_context_window,
        m.window_within(at_rest.usable_bytes, once.lanes),
        "a budget that holds the credit plans its own fit"
    );
    let applied = plan_by_credit(OwnCredit::ToApply, &at_rest, std::slice::from_ref(&m), Some(&m.model_id), &demand)
        .expect("a stable plan") // expect: the assertion under test
        .plan;
    assert!(
        applied.served_context_window > once.served_context_window,
        "the negative control: `ToApply` credits the weights on top, the second credit `Included` must never get"
    );

    let system = 5 * GB;
    let own = 30 * GB;
    let unified = own_bytes_in_system_memory(ResourceKind::Vram, Some(MemoryMode::Unified));
    let discrete = own_bytes_in_system_memory(ResourceKind::Vram, Some(MemoryMode::Discrete));
    assert!(!discrete && own_bytes_in_system_memory(ResourceKind::Ram, None));
    let held = OwnCredit::Included { own_bytes: own };
    assert_eq!(held.pressure_available(system, unified), system + own, "added back exactly once");
    assert_eq!(OwnCredit::ToApply.pressure_available(system, unified), system, "no credit in the budget, none in the pressure");
    assert_eq!(held.pressure_available(system, discrete), system, "a discrete card's VRAM never was system memory");
}

/// Serving registered with `authority` as the measured consumer production registers
/// (`register_as_consumer`), its residency what `measured` holds (0 = nothing reported yet).
fn serving_measured_by(
    daemon: &ServingDaemonModule,
    authority: &ResourceDaemon,
    measured: Arc<std::sync::atomic::AtomicU64>,
) {
    use crate::modules::serving_consumer::{MeasuredFn, ServingConsumer};
    let footprint_of: FootprintFn = Arc::new(|_: &str, _: u32, _: u32, _: u32| 0u64);
    let measured_of: MeasuredFn = Arc::new(move |_: &str| Some(measured.load(Ordering::SeqCst)));
    authority.add_consumer(Arc::new(
        ServingConsumer::new(
            daemon.subscribe_serving(),
            daemon.intent(),
            footprint_of,
            serving_pool_kind(),
            Arc::new(crate::modules::serving_tier_down::DeclineTierDown),
        )
        .with_inherited_lane(Arc::new(|| None))
        .with_measured(measured_of),
    ));
}

/// Wait until the authority's replace-myself read satisfies `settled` (its tick is 20 ms).
async fn until_the_board(
    authority: &ResourceDaemon,
    settled: impl Fn(&crate::resources::ReplacementBudget) -> bool,
) {
    let waited = tokio::time::timeout(Duration::from_secs(10), async {
        let mut tick = tokio::time::interval(Duration::from_millis(10));
        loop {
            tick.tick().await;
            if settled(&authority.replacement_budget_snapshot(SERVING_CONSUMER_ID, serving_pool_kind())) {
                return;
            }
        }
    })
    .await;
    assert!(waited.is_ok(), "the authority's board never settled");
}

/// The M5's engine model, as `a_core_restart_over_an_engine_larger_than_free_memory_...` sizes it.
fn m5_engine_footprint() -> ModelFootprint {
    ModelFootprint {
        model_id: "ggml-org/Qwen3.6-35B-A3B-GGUF".into(),
        weights_bytes: 20_419_565_568,
        kv_per_token: 8_317,
        context_window: 262_144,
        capability_rank: continuum_kernel::capability::Capability::Proxy { score: 9 },
        fixed_per_lane_bytes: 0,
    }
}

// what this catches (the M5, 2026-11-04 16:41Z, through the real owner `host_budget_snapshot`):
// a core restarted over an inherited engine plans before serving's attribution lands, so the
// budget's sustained-minimum history holds a reading of free memory alone. When the attribution
// lands, the next budget must be the credited one (free + the engine, exactly once), not that
// uncredited floor for another relaunch's window; and before it lands nothing may be invented.
// The mode fraction is the host's, so every bound below holds at any fraction in (0, 1).
#[tokio::test]
async fn the_credit_lands_once_through_the_budget_owner_and_the_uncredited_history_does_not_hold_it() {
    use super::own_credit::plan_by_credit;
    let capacity = 53 * GB;
    let engine = 36 * GB; // the inherited engine (the M5's 36.05 GB)
    let others = 9 * GB;
    let free = capacity - engine - others;
    let source = Arc::new(crate::resources::MockCapacitySource::new(serving_pool_kind(), capacity));
    source.set_used(engine + others);
    let authority = ticking_resource_daemon(source);
    let daemon = daemon_on_authority(
        Arc::new(FakeServer::healthy(Arc::new(AtomicUsize::new(0)), true)),
        capacity,
        authority.clone(),
    );
    let measured = Arc::new(std::sync::atomic::AtomicU64::new(0));
    serving_measured_by(&daemon, &authority, measured.clone());

    // Before the attribution: the board scanned, serving reported nothing.
    until_the_board(&authority, |r| r.capacity_bytes == capacity && r.bytes == free).await;
    let before = daemon.host_budget_snapshot();
    assert_eq!(before.credit, OwnCredit::ToApply, "nothing reported is nothing credited");
    assert!(
        before.host.usable_bytes <= free,
        "nothing fabricated before the attribution: {} > the {free} free",
        before.host.usable_bytes
    );

    // The attribution lands.
    measured.store(engine, Ordering::SeqCst);
    until_the_board(&authority, |r| r.own_bytes == engine).await;
    let after = daemon.host_budget_snapshot();
    assert_eq!(after.credit, OwnCredit::Included { own_bytes: engine });
    assert!(
        after.host.usable_bytes > free,
        "the uncredited reading in the history does not hold the credited budget at its floor: {}",
        after.host.usable_bytes
    );
    assert!(
        after.host.usable_bytes <= free + engine,
        "the engine credited once: {} > free + engine",
        after.host.usable_bytes
    );

    // The planner takes the at-rest arm on that budget: its window is the budget's own fit,
    // never the budget plus the weights again.
    let m = m5_engine_footprint();
    let plan = plan_by_credit(
        after.credit,
        &after.host,
        std::slice::from_ref(&m),
        Some(&m.model_id),
        &ServingDemand::new(4, Some(16_521)),
    )
    .expect("the credited budget holds the engine's model") // expect: the assertion under test
    .plan;
    assert!(
        plan.served_context_window <= m.window_within(after.host.usable_bytes, plan.lanes),
        "window {} beyond the credited budget's fit",
        plan.served_context_window
    );
}

// what this catches (Codex on #5770, the M5 16:41Z repair's edge): a pool whose every byte is
// leased away (a training run took it) reads a MEASURED zero budget. That zero used to fall to
// the substitute-ceiling ladder, whose first rung is `budget_for_replacing` without the other
// holders' leases: serving's own leased-away bytes came back as capacity and the leases that
// took them were dropped. A measured zero is the answer; the credit says the engine was counted
// (so the plan adds nothing of its own on top) and the training lease stays charged.
#[tokio::test]
async fn a_fully_leased_pool_budgets_nothing_and_the_credit_resurrects_nothing() {
    use super::own_credit::plan_by_credit;
    let capacity = 53 * GB;
    let engine = 36 * GB;
    let source = Arc::new(crate::resources::MockCapacitySource::new(serving_pool_kind(), capacity));
    let authority = ticking_resource_daemon(source.clone());
    let daemon = daemon_on_authority(
        Arc::new(FakeServer::healthy(Arc::new(AtomicUsize::new(0)), true)),
        capacity,
        authority.clone(),
    );
    until_the_board(&authority, |r| r.capacity_bytes == capacity).await;
    let training = authority.acquire_guarded(&crate::resources::LeaseRequest {
        consumer_id: "training".into(),
        kind: serving_pool_kind(),
        bytes: capacity,
        ttl_ms: u64::MAX,
        reclaim_policy: crate::resources::ReclaimPolicy::Pinned,
    });
    assert!(training.is_ok(), "an empty pool leases whole");
    source.set_used(engine);
    serving_measured_by(&daemon, &authority, Arc::new(std::sync::atomic::AtomicU64::new(engine)));
    until_the_board(&authority, |r| r.own_bytes == engine).await;

    // The negative control: the ladder's first rung would have handed the engine back.
    assert_eq!(governed_vram_ceiling(&authority), Some(engine), "the rung that resurrected leased-away bytes");

    let budget = daemon.host_budget_snapshot();
    assert_eq!(budget.host.usable_bytes, 0, "a fully leased pool budgets nothing: no prior, no own bytes back");
    assert_eq!(budget.credit, OwnCredit::Included { own_bytes: engine }, "the engine was counted; nothing more to credit");
    assert!(
        budget.leases.iter().any(|l| l.consumer_id == "training" && l.bytes == capacity),
        "the lease that took the pool stays charged"
    );
    let m = m5_engine_footprint();
    assert!(
        !plan_by_credit(budget.credit, &budget.host, std::slice::from_ref(&m), Some(&m.model_id), &ServingDemand::new(4, Some(16_521)))
            .is_some_and(|t| t.plan.fits_on_gpu),
        "no plan is built on bytes a lease holds"
    );
    drop(training);
}
