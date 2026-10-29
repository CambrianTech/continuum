#![cfg(test)]

use super::*;

// what this catches (the M5, 2026-10-11 01:41Z: the plan moved 2,2,1,1,1,1 lanes in 40 s,
// same engine pid, and the admission gate followed every move): admission sized by a plan
// the engine never took. A serving engine's own lane count is what the gate counts; the
// plan only stands in before an engine is up.
#[test]
fn the_admission_gate_counts_the_engines_lanes_not_the_plans() {
    assert_eq!(
        admitted_lanes(true, 2, 1),
        2,
        "a 2-lane engine is admitted as two lanes whatever the plan says"
    );
    assert_eq!(
        admitted_lanes(true, 1, 2),
        1,
        "and a 1-lane engine as one, until the reconcile relaunches it"
    );
    assert_eq!(
        admitted_lanes(false, 0, 2),
        2,
        "cold boot: the plan is all there is"
    );
    assert_eq!(
        admitted_lanes(true, 0, 2),
        2,
        "a ready snapshot without a lane count is no evidence"
    );
}

// what this catches (2026-09-01 live): the re-home guard measured only the
// per-slot window, so a plan that GREW total token-space by adding lanes
// while trimming per-slot (2×57,600 → 4×47,426, +65% space, +2 warm slots)
// computed a negative gain and never relaunched — five residents thrashed
// two slots at hit_rate 0.0 until an operator cycled the server by hand.
// The evidence currency is window × lanes.
#[test]
fn rehome_evidence_counts_lanes_not_just_per_slot_window() {
    use super::rehome_gain_evidence;
    // The live incident shape: more lanes, smaller per-slot, bigger total.
    let (live, plan, gain, worth_it) = rehome_gain_evidence(57_600, 2, 47_426, 4);
    assert_eq!(live, 115_200);
    assert_eq!(plan, 189_704);
    assert_eq!(gain, 74_504);
    assert!(
        worth_it,
        "65% more total space must qualify as re-home evidence"
    );
    // Same total space re-sliced: not evidence (a relaunch kills in-flight turns).
    let (_, _, _, worth_it) = rehome_gain_evidence(40_000, 2, 20_000, 4);
    assert!(!worth_it, "re-slicing the same space is not a gain");
    // Sub-threshold growth (<15%): not evidence.
    let (_, _, _, worth_it) = rehome_gain_evidence(40_000, 2, 44_000, 2);
    assert!(!worth_it, "10% is under the 15% margin");
    // A dead lane (0 window) never qualifies — recovery is a different path.
    let (_, _, _, worth_it) = rehome_gain_evidence(0, 1, 40_000, 2);
    assert!(
        !worth_it,
        "live_space=0 is the recovery arm's job, not re-home"
    );
}

// what this catches (card 29e4ab34, the M5 2026-09-19 17:20:35Z): the thirteen-second
// boot window in which no lane has reported a model. The knee lookup must key on the
// remembered model then — otherwise the roster (17 runtimes) goes unclamped into the
// demand, the plan says 8 lanes under the transient budget, and the warm adopted lane
// is relaunched twice (8 × 49k, then 2 × 67k). A reported model still wins; a box
// that remembers nothing plans the roster, as before.
#[test]
fn a_boot_before_any_lane_reports_its_model_reads_the_remembered_models_knee() {
    use crate::inference::decode_knee::knee_lanes;
    let remembered = Some("ggml-org/Qwen3.8-27B-GGUF".to_string());
    assert_eq!(
        knee_model(None, || remembered.clone()),
        remembered,
        "the boot window plans the last model"
    );
    let read = std::cell::Cell::new(false);
    assert_eq!(
        knee_model(Some("other".into()), || {
            read.set(true);
            remembered.clone()
        })
        .as_deref(),
        Some("other"),
        "a serving lane's model wins"
    );
    assert!(
        !read.get(),
        "with a lane serving, the remembered geometry (a file) is never read"
    );
    assert_eq!(
        knee_model(None, || None),
        None,
        "nothing remembered: the roster rules, as before"
    );
    // With the remembered model's knee (2 on the M5) the roster of 17 is clamped at boot;
    // with none it is not — the first stair of the staircase.
    assert_eq!(knee_lanes(17, Some(2)), 2);
    assert_eq!(knee_lanes(17, None), 17);
    // Exercise the actual plan as well as the lookup: remembering a one-lane
    // collapse cannot defeat today's two-lane knee, even before a slot reports.
    use crate::inference::planning::serving_plan::{
        plan_serving_at_rest, HostBudget, ModelFootprint, ServingDemand,
    };
    let model = ModelFootprint {
        model_id: remembered.clone().unwrap(),
        weights_bytes: 19 << 30,
        kv_per_token: 33_000,
        context_window: 262_144,
        capability_rank: 9,
        fixed_per_lane_bytes: 0,
    };
    let host = HostBudget {
        usable_bytes: 30 << 30,
        perf_cores: 8,
    };
    let demand = ServingDemand::new(
        knee_lanes(resident_lane_demand(1, 17, false), Some(2)),
        Some(60_000),
    )
    .with_sticky_window(Some(67_340));
    let cold = plan_serving_at_rest(&host, std::slice::from_ref(&model), None, &demand).unwrap();
    let settled = plan_serving_at_rest(
        &host,
        std::slice::from_ref(&model),
        Some(&model.model_id),
        &demand,
    )
    .unwrap();
    assert_eq!(
        (cold.lanes, cold.served_context_window),
        (settled.lanes, settled.served_context_window)
    );
    assert_eq!(
        cold.lanes, 2,
        "remembered model applies its knee before boot, not the eight-core maximum"
    );
    assert!(
        roster_short_of_slots(1, cold.lanes),
        "a prior one-lane collapse can recover the second slot"
    );
}

// what this catches (#287 slice 2): the governed host-cache lease inputs are
// assembled from the HOST-RESIDENT mass — expert bytes (cache-funded, or hot in
// VRAM) are subtracted BEFORE any weight-scaled heuristic runs, KV tracks the
// LIVE window × lanes, and the OS floor derives from the box. K3-on-63GB
// incident shape: full-file mass would derive a permanent ZERO lease (68GB KV
// + 41GB buffer estimates off 663GB); the split derives a real budget.
#[test]
fn moe_lease_inputs_use_host_resident_mass_not_file_mass() {
    let file = 663 * GB;
    let experts = 640 * GB;
    let physical = 63 * GB;
    let inputs =
        moe_host_cache_lease_inputs("k3", file, experts, 262_144, 4096, 2, physical, None, 4096)
            .expect("host share is real");
    assert_eq!(
        inputs.weights_host_bytes,
        23 * GB,
        "dense/attention share only"
    );
    assert_eq!(
        inputs.live_kv_bytes,
        (23 * GB / 80_000) * 4096 * 2,
        "KV law over the HOST mass × live window × lanes"
    );
    assert_eq!(
        inputs.os_floor_bytes,
        physical / 8,
        "floor derives from the box"
    );
    let lease = crate::capacity::host_cache_lease::host_cache_lease_bytes(&inputs);
    assert!(
        lease > 20 * GB && lease < 30 * GB,
        "governed lease lands in a sane band on the incident shape, got {lease}"
    );

    // Blind to the expert split, the working set "costs" the whole file → the
    // lease saturates to zero and the cache is permanently off. The subtraction
    // is load-bearing, not cosmetic.
    let blind =
        moe_host_cache_lease_inputs("k3", file, 0, 262_144, 4096, 2, physical, None, 4096).unwrap();
    assert_eq!(
        crate::capacity::host_cache_lease::host_cache_lease_bytes(&blind),
        0,
        "full-file mass must starve the lease — proves why the split exists"
    );

    // Degenerate all-expert read → no honest host footprint → publish nothing.
    assert!(
        moe_host_cache_lease_inputs("k3", file, file, 262_144, 4096, 2, physical, None, 4096)
            .is_none()
    );
}

// what this catches: the daemon publishes the classifier's decision to its
// watch channel — Some(plan) with the most-capable fitting model when there
// are candidates, None (no silent serve) when there are none.
#[tokio::test]
async fn publish_plan_drives_the_watch() {
    let gpu = Arc::new(GpuMemoryManager::simulated("Apple M5 Pro", 53 * GB));
    let system = Arc::new(SystemResourceMonitor::new());
    let mut daemon = ServingDaemonModule::new(
        gpu,
        system,
        test_resource_daemon(),
        test_catalog(),
        test_pin_store(),
    );
    daemon.working_set = crate::cognition::working_set::WorkingSetRegistry::new();
    daemon.set_leased_in_sent_source(Arc::new(Vec::new));
    let rx = daemon.subscribe();
    assert!(rx.borrow().is_none(), "starts unpublished");
    // A measured demand, as every live node has (the seat's leased-in sample ring is
    // process-global across tests; a requirement it carries needs a ceiling above it).
    daemon.working_set.record_in_memory(uuid::Uuid::new_v4(), 200_000, 1);

    let budget = HostBudget {
        usable_bytes: 45 * GB,
        perf_cores: 6,
    };
    // The candidates carry real trained windows (2eec3977: a persona seat holds what the
    // residents require; an 8k-window model cannot).
    let candidates = vec![
        footprint_from_parts("small", GB, 4096, false, None).unwrap(),
        footprint_from_parts("coder-14b", 9 * GB, 262_144, true, None).unwrap(),
    ];
    daemon.publish_plan(budget, &candidates, &candidates);
    let plan = rx.borrow().clone().expect("plan published");
    assert_eq!(
        plan.base_model.model_id, "coder-14b",
        "most capable that fits"
    );
    assert!(plan.fits_on_gpu);

    // No candidates → None published (no silent serve).
    daemon.publish_plan(budget, &[], &[]);
    assert!(rx.borrow().is_none(), "empty candidates → no plan");
    // What this catches: publication must retain the revision used to select
    // candidates, never borrow newer intent and falsely stamp the old plan.
    let selected_revision = daemon.intent.snapshot().revision;
    daemon.intent.set_pin(Some("new-user-choice".into()), true);
    daemon.publish_plan_for_intent(budget, &candidates, &candidates, selected_revision);
    assert_eq!(daemon.plan_tx.borrow().intent_revision, selected_revision);
    assert!(
        daemon.reconcile_to_plan().is_none(),
        "stale plan cannot dispatch"
    );
    assert_ne!(
        daemon.plan_tx.borrow().intent_revision,
        daemon.intent.snapshot().revision
    );
}

// what this catches: a plan whose window is below the residents' REQUIREMENT is never PUBLISHED —
// the previous plan stands. Regression for the 5090 (2026-09-20, 2 × 2,048 all
// afternoon, 2,932 prompts refused) and the M5's 1 × 2,048 boot: a starved geometry
// must not become the served target, whatever the planner's arithmetic says.
#[tokio::test]
async fn a_plan_below_one_coding_turn_is_not_published_and_the_previous_stands() {
    let gpu = Arc::new(GpuMemoryManager::simulated("Apple M5 Pro", 53 * GB));
    let system = Arc::new(SystemResourceMonitor::new());
    let mut daemon = ServingDaemonModule::new(
        gpu,
        system,
        test_resource_daemon(),
        test_catalog(),
        test_pin_store(),
    );
    // This test OWNS its demand pool: a private working set and an empty leased-in ring.
    // Both are process globals in production, shared by every test in the binary.
    daemon.working_set = crate::cognition::working_set::WorkingSetRegistry::new();
    daemon.set_leased_in_sent_source(Arc::new(Vec::new));
    let rx = daemon.subscribe();
    let candidates = vec![footprint_from_parts("coder-14b", 9 * GB, 262_144, true, None).unwrap()];
    // The residents' requirement: a 56k typical prompt (the 5090's p50), with headroom.
    // No residents are live in this harness, so the seat's leased-in samples carry it —
    // the same ring #4256 votes refused prompts into.
    // …in the daemon's OWN working set, never the process-global leased-in ring: that
    // ring is shared by every test in the binary, and eight 56k samples there re-homed
    // a 65k lane in `a_jittering_gain_never_accumulates_into_a_re_home` (CI, 2026-09-20).
    let resident = uuid::Uuid::new_v4();
    for i in 0..8 {
        daemon.working_set.record_sent_in_memory(resident, 56_057, 1 + i);
    }
    // …and a measured demand ceiling above it (the assembled context a turn wanted),
    // as every live node has — without it the demand cap is the bootstrap prior and no
    // plan can hold a 56k requirement, which is a harness artifact, not a law.
    daemon.working_set.record_in_memory(uuid::Uuid::new_v4(), 200_000, 1);
    // A roomy budget: a real window is published.
    daemon.publish_plan(
        HostBudget {
            usable_bytes: 45 * GB,
            perf_cores: 6,
        },
        &candidates,
        &candidates,
    );
    let good = rx.borrow().clone().expect("a real plan is published");
    assert!(
        good.served_context_window >= 70_071,
        "the roomy plan holds the residents' requirement (56k × 1.25): {}",
        good.served_context_window
    );
    // A squeezed budget: the planner's arithmetic yields a window below one coding
    // turn — the daemon refuses to publish it and the good plan stands.
    daemon
        .intent
        .set_pin(Some(good.base_model.model_id.clone()), true);
    daemon.publish_plan(
        HostBudget {
            usable_bytes: 9 * GB + 200 * 1_000_000,
            perf_cores: 6,
        },
        &candidates,
        &candidates,
    );
    let after = rx.borrow().clone().expect("the previous plan still stands");
    assert_eq!(
        daemon.plan_tx.borrow().intent_revision,
        daemon.intent.snapshot().revision,
        "held geometry is revalidated under same-value explicit intent"
    );
    assert_eq!(
        after.served_context_window, good.served_context_window,
        "a 2k plan never replaces a real one"
    );
    assert_eq!(after.lanes, good.lanes);
    let held_revision = daemon.plan_tx.borrow().intent_revision;
    daemon.intent.set_pin(Some("different-model".into()), true);
    let different = vec![footprint_from_parts("different-model", GB, 8192, true, None).unwrap()];
    daemon.publish_plan_for_intent(
        HostBudget {
            usable_bytes: 2 * GB,
            perf_cores: 6,
        },
        &different,
        &different,
        daemon.intent.snapshot().revision,
    );
    assert_eq!(
        daemon.plan_tx.borrow().intent_revision,
        held_revision,
        "refusing a new candidate cannot relabel the excluded old plan as current intent"
    );
    assert!(
        daemon.reconcile_to_plan().is_none(),
        "excluded retained model cannot dispatch"
    );
    // With NO plan published (a cold boot) and a requirement the box cannot hold, the
    // best runnable plan is still published — a dark node is worse than a starved seat
    // (Cormac's condition on #4257: IntelMac's 32k-trained 1.5B against a 58k typical).
    let mut cold = ServingDaemonModule::new(
        Arc::new(GpuMemoryManager::simulated("Intel", 16 * GB)),
        Arc::new(SystemResourceMonitor::new()),
        test_resource_daemon(),
        test_catalog(),
        test_pin_store(),
    );
    cold.working_set = crate::cognition::working_set::WorkingSetRegistry::new();
    cold.set_leased_in_sent_source(Arc::new(Vec::new));
    cold.working_set.record_in_memory(uuid::Uuid::new_v4(), 200_000, 1);
    let rx_cold = cold.subscribe();
    let small = vec![footprint_from_parts("coder-1.5b", GB, 32_768, true, None).unwrap()];
    cold.publish_plan(
        HostBudget {
            usable_bytes: 12 * GB,
            perf_cores: 4,
        },
        &small,
        &small,
    );
    let starved = rx_cold
        .borrow()
        .clone()
        .expect("a cold boot publishes the best runnable plan, starved or not");
    assert!(starved.served_context_window <= 32_768);
    assert!(
        !crate::inference::planning::serving_plan::persona_lane_holds(
            starved.served_context_window,
            Some(70_071)
        ),
        "it IS below the requirement — published and said, never dark"
    );
}

// what this catches: the `serving/*` surface (plan · status · load · unload) is
// migrated to the typed registry; the legacy handle_command is gone, so for any
// command name it must fail loud — never silently fall back to an empty result.
#[tokio::test]
async fn legacy_handle_command_fails_loud() {
    let gpu = Arc::new(GpuMemoryManager::simulated("Apple M5 Pro", 53 * GB));
    let system = Arc::new(SystemResourceMonitor::new());
    let daemon = ServingDaemonModule::new(
        gpu,
        system,
        test_resource_daemon(),
        test_catalog(),
        test_pin_store(),
    );
    let err = daemon
        .handle_command("serving/plan", serde_json::json!({}))
        .await
        .expect_err("legacy handler must fail loud after migration");
    assert!(
        err.contains("migrated to the typed registry"),
        "error must name the migration: {err}"
    );
}

// what this catches: the daemon contributes the full typed serving surface,
// sharing its OWN watch receivers + catalog (so the read surfaces report the
// daemon's live decision/snapshot). A regression that drops a verb is caught.
#[tokio::test]
async fn contributes_the_full_serving_surface() {
    let gpu = Arc::new(GpuMemoryManager::simulated("Apple M5 Pro", 53 * GB));
    let system = Arc::new(SystemResourceMonitor::new());
    let daemon = ServingDaemonModule::new(
        gpu,
        system,
        test_resource_daemon(),
        test_catalog(),
        test_pin_store(),
    );
    let mut names: Vec<&str> = daemon.commands().iter().map(|c| c.name()).collect();
    names.sort_unstable();
    assert_eq!(
        names,
        vec![
            "serving/cache-probe",
            "serving/load",
            "serving/pin",
            "serving/plan",
            "serving/status",
            "serving/unload",
            "serving/unpin"
        ]
    );
}

/// A resource authority for daemon construction in tests — primed with a
/// generous mock VRAM ceiling so that if a test ever reaches `host_budget()`
/// it reads a real governed number rather than the fail-closed 0. Calls
/// `ResourceDaemon::start`, which spawns its tick task, so every test that
/// constructs a daemon through `daemon_with` must be `#[tokio::test]`.
/// A pin store under a tempdir the process owns for its lifetime: a test daemon
/// must never read the machine's real pin (it would plan under whatever the box
/// last pinned) nor write it (2026-09-16: `cargo test` re-pinned every dev box).
pub(super) fn test_pin_store() -> crate::modules::serving_pin_store::ServingPinStore {
    static DIR: std::sync::LazyLock<tempfile::TempDir> =
        std::sync::LazyLock::new(|| tempfile::tempdir().expect("tempdir"));
    crate::modules::serving_pin_store::ServingPinStore::under_home(DIR.path())
}

// what this catches: Kimi's attempt 2 (5090, 2026-09-28 12:29Z). The plan wanted a different
// model while a training run was bound to the live engine, the reconcile replaced the
// engine, and the run died with it. With the run's binding recorded against THIS engine
// incarnation (here, this test process: same pid, same start time), the same plan that
// would otherwise serve launches nothing; once the binding is released it proceeds.
#[tokio::test]
async fn an_engine_bound_to_live_work_is_never_replaced_under_it() {
    let _home = crate::paths::test_home::HomeGuard::preserve().await; // reads the process home a guarded fixture may be swapping (card cb3221a5)
    use crate::inference::engine_residency::{
        record, release, store_path, EngineIncarnation, ResidentWork,
    };
    let serves = Arc::new(AtomicUsize::new(0));
    let mut daemon = daemon_with(Arc::new(FakeServer::healthy(serves.clone(), true)));
    let pid = std::process::id();
    let engine = EngineIncarnation::of(pid, 61347).expect("test: this process has a start time");
    let lane = LaneRecord {
        pid,
        port: 61347,
        started_s: engine.started_s,
        ..inherited_27b()
    };
    daemon.set_inherited_lane(Arc::new(move || Some(lane.clone())));
    let budget = HostBudget {
        usable_bytes: 45 * GB,
        perf_cores: 6,
    };
    let candidates = vec![footprint_from_parts("coder-14b", 9 * GB, 8192, true, None).unwrap()];
    daemon.publish_plan(budget, &candidates, &candidates);

    let store = store_path(&crate::paths::continuum_home().expect("test: a test home"));
    let job = uuid::Uuid::from_u128(0xef9d_f13b);
    let bound = ResidentWork {
        job,
        out: "run.gguf".into(),
        engine,
        base_model: "qwen3-27b".into(),
        created_ms: 1,
        consumer: "genome-train:test".into(),
        reserved_bytes: 1 << 30,
        interrupted: None,
        job_spec: None,
        memory: None,
    };
    record(&store, bound.clone()).expect("test: record");
    assert!(
        daemon.reconcile_to_plan().is_none(),
        "an engine bound to live work is not reconciled"
    );
    assert_eq!(
        serves.load(Ordering::SeqCst),
        0,
        "nothing was launched over the run"
    );

    release(&store, &bound).expect("test: release");
    let handle = daemon
        .reconcile_to_plan()
        .expect("released: the plan proceeds");
    handle.await.unwrap();
    assert_eq!(serves.load(Ordering::SeqCst), 1);
}

// what this catches: the footprint resolver serving hands the authority reads
// the SAME live catalog + estimator the serving plan uses (one footprint
// authority) — a Ready model resolves to its real on-disk weights, and an id
// the catalog doesn't know resolves to 0 (nothing resident to attribute, never
// a phantom the governor would over-account). If this drifted from the plan's
// footprint the board would attribute a different number than the planner sized
// against.
#[test]
fn serving_footprint_fn_resolves_live_catalog_weights() {
    use crate::model_registry::live::ModelStatus;
    use std::io::Write;

    let catalog = test_catalog();
    let id = "footprint-probe";
    catalog.register(
        fake_model(id),
        ModelStatus {
            availability: Availability::NotDownloaded,
            verified: None,
        },
    );
    let resolve = serving_footprint_fn(catalog.clone());

    // NotDownloaded (no on-disk weights) → nothing resident yet. Window/lanes
    // are the live serving shape; with no weights they resolve to 0 anyway.
    assert_eq!(
        resolve(id, 8192, 2, 0),
        0,
        "no weights on disk → nothing to attribute"
    );
    // An id the catalog has never heard of → 0, never a phantom.
    assert_eq!(resolve("never-registered", 8192, 2, 0), 0);

    // Land the artifact: real bytes on disk, flips Ready.
    let mut gguf = tempfile::Builder::new()
        .suffix(".gguf")
        .tempfile()
        .expect("temp gguf");
    gguf.write_all(&[0u8; 4096]).expect("write weights");
    gguf.flush().expect("flush");
    assert!(catalog.attach_local_artifact(id, gguf.path().to_path_buf(), None));

    // Same resolver, same live catalog → now reports the real PEAK resident bytes:
    // weights (4096) + the KV of every lane at the served window + the concurrent-
    // prefill compute reserve of every lane (#56/G5). kv_per_token floors at 20_000,
    // so 2 lanes × 20_000 × 8192 tokens of KV on top of 4096 weights, PLUS the
    // window-scaled compute reserve. Charging the KV stops it masquerading as
    // external (#79); charging the compute reserve stops the board over-reporting
    // free by the prefill buffer (G5). The compute-reserve term comes from the
    // footprint's own method so this expectation can't drift from the plan's sizing.
    // Build the expectation through the SAME quant transform the resolver applies
    // (`apply_kv_quantization` rides the live resolved KV cache decision):
    // with f16/unset this is byte-identical to raw parts; with q8_0 configured the
    // rate halves on BOTH sides. Before this the expectation assumed f16 and the
    // test failed on any box actually serving quantized KV while CI stayed green.
    let fp = apply_kv_quantization(
        footprint_from_parts(id, 4096, 8192, false, None).expect("footprint"),
    );
    let kv_per_token = fp.kv_per_token; // 20_000 raw, ÷ live quant divisor
    let expect = 4096 + 2 * kv_per_token * 8192 + fp.prefill_compute_reserve(8192, 2);
    assert_eq!(
        resolve(id, 8192, 2, 0),
        expect,
        "resolves real weights + per-lane KV + prefill compute reserve (peak), no reboot"
    );
    assert_eq!(
        resolve(id, 8192, 2, 0),
        fp.peak_resident_bytes(8192, 2),
        "resolver reports peak_resident_bytes — the plan's chosen_cost, not resident-only"
    );
    // A no-window snapshot (nothing served yet) still charges weights + every lane's
    // compute-buffer FLOOR (the reserve exists even before a window is chosen).
    assert_eq!(
        resolve(id, 0, 2, 0),
        4096 + fp.prefill_compute_reserve(0, 2),
        "no served window → weights + per-lane compute floor",
    );
    // The grant the ENGINE was launched with (off the snapshot, beside window and
    // lanes) is credited on top of the peak — the 2026-09-20 attribution — and only
    // while lanes are resident (no lanes → nothing to charge a cache to).
    assert_eq!(
        resolve(id, 8192, 2, 1_024),
        fp.peak_resident_bytes(8192, 2) + 1_024 * 1024 * 1024,
        "the running engine's --cache-ram grant is serving's, not external"
    );
    assert_eq!(
        resolve(id, 8192, 0, 1_024),
        fp.peak_resident_bytes(8192, 0),
        "no lanes → no cache charged"
    );
}

// what this catches: THE slice-1 production-wiring gap — ServingConsumer was
// defined + unit-tested but never CONSTRUCTED in the boot path, so the
// authority polled nothing and stayed blind to serving's multi-GB residency.
// register_as_consumer (called from initialize) must register a consumer under
// SERVING_CONSUMER_ID with the one per-machine authority. Regresses deleting
// that registration — which would silently reopen the granted:0-while-full hole.
#[tokio::test]
async fn register_as_consumer_wires_serving_into_the_authority() {
    let gpu = Arc::new(GpuMemoryManager::simulated("Apple M5 Pro", 53 * GB));
    let system = Arc::new(SystemResourceMonitor::new());
    let daemon = ServingDaemonModule::new(
        gpu,
        system,
        test_resource_daemon(),
        test_catalog(),
        test_pin_store(),
    );

    assert!(
        !daemon
            .resource_daemon
            .consumer_ids()
            .contains(&SERVING_CONSUMER_ID.to_string()),
        "not registered until the daemon wires itself in"
    );
    daemon.register_as_consumer();
    assert!(
        daemon
            .resource_daemon
            .consumer_ids()
            .contains(&SERVING_CONSUMER_ID.to_string()),
        "serving must register itself as a measured consumer with the authority"
    );
}

// what this catches (card a18452d6): a held engine published as an unexplained empty node
// (enrico 2026-10-27: Delia's training held the only engine for hours, read as dead), a hold
// whose start resets when another publisher replaces the snapshot whole, a hold carried over to
// a relaunched engine, and a hold that outlives its release.
#[tokio::test]
async fn a_hold_is_published_keeps_its_start_and_is_cleared_on_release() {
    use crate::inference::engine_residency::{EngineIncarnation, Occupancy, ResidentWork};
    let daemon = daemon_with(Arc::new(FakeServer::healthy(Arc::new(AtomicUsize::new(0)), true)));
    let work = |started_s: u64| ResidentWork {
        job: uuid::Uuid::from_u128(0xf633bd36),
        out: "f633bd36.gguf".into(),
        engine: EngineIncarnation { pid: 33_971, started_s, port: 58_057 },
        base_model: "continuum-ai/qwen2.5-coder-1.5b-instruct-GGUF".into(),
        created_ms: 1_793_120_435_262,
        consumer: "genome-train:f633bd36".into(),
        reserved_bytes: 13_130_575_872,
        interrupted: None,
        job_spec: None,
        memory: None,
    };
    let held = Occupancy::Resident(vec![work(1_793_118_524)]);
    daemon.publish_hold(&held);
    let first = daemon.serving_tx.borrow().held_for.clone().expect("a held engine says what holds it");
    let _ = daemon.serving_tx.send_replace(ServingSnapshot::empty()); // another publisher, whole
    daemon.publish_hold(&held);
    let again = daemon.serving_tx.borrow().held_for.clone().expect("still held");
    assert_eq!(again.since_ms, first.since_ms, "the hold's start survives a replaced snapshot");
    daemon.publish_hold(&Occupancy::Resident(vec![work(1_793_200_000)]));
    let relaunched = daemon.serving_tx.borrow().held_for.clone().expect("held by the relaunched engine's work");
    assert_ne!(relaunched.by, first.by, "a changed incarnation is a different hold, never the old one carried over");
    daemon.publish_hold(&Occupancy::Free);
    assert_eq!(daemon.serving_tx.borrow().held_for, None, "a released engine is not held");
}
