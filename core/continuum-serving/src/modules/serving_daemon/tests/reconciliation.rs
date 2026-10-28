#![cfg(test)]

use super::*;

// what this catches (card c3c50e0d): a reconcile in flight is NAMED — its step and age
// — and past the bound it is a wedge, never a silent skip. The gate, the start and the
// step are one operation: acquire sets them, drop clears them, a second acquire fails.
#[test]
fn a_reconcile_in_flight_names_its_step_and_a_wedge_is_declared_past_the_bound() {
    use super::{reconcile_wedged, ReconcileStep, ServingOperation, RECONCILE_WEDGE_BOUND};
    use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
    use std::sync::Arc;
    let gate = Arc::new(AtomicBool::new(false));
    let started = Arc::new(AtomicU64::new(0));
    let step = Arc::new(AtomicU8::new(0));
    let op = ServingOperation::acquire(gate.clone(), started.clone(), step.clone())
        .expect("first acquire");
    assert!(ServingOperation::acquire(gate.clone(), started.clone(), step.clone()).is_none());
    assert!(started.load(Ordering::Acquire) > 0, "the start is stamped");
    op.step(ReconcileStep::Ensure);
    assert_eq!(
        ReconcileStep::from_u8(step.load(Ordering::Acquire)),
        ReconcileStep::Ensure
    );
    assert_eq!(
        ReconcileStep::from_u8(step.load(Ordering::Acquire)).name(),
        "ensure_serving"
    );
    drop(op);
    assert!(!gate.load(Ordering::Acquire) && started.load(Ordering::Acquire) == 0);
    assert_eq!(
        ReconcileStep::from_u8(step.load(Ordering::Acquire)),
        ReconcileStep::None
    );
    assert!(!reconcile_wedged(RECONCILE_WEDGE_BOUND));
    assert!(reconcile_wedged(
        RECONCILE_WEDGE_BOUND + std::time::Duration::from_secs(1)
    ));
    // Per step (Cormac on #4421): a teacher batch is busy for its own reasons, never wedged.
    let long = RECONCILE_WEDGE_BOUND * 10;
    assert!(super::reconcile_wedged_at(ReconcileStep::Ensure, long));
    assert!(!super::reconcile_wedged_at(
        ReconcileStep::AcademyBatch,
        long
    ));
}

// what this catches (2026-09-19, the M5 pinned at one lane across boots): the store is
// written on the tick a launch SETTLES — cooldown 1 → 0 with the lane serving — and on
// no other tick: not at spawn (cooling = full), not while cooling, not after (0), not
// for a lane that is not ready, not for zero lanes.
#[test]
fn only_the_tick_a_launch_settles_on_remembers_its_geometry() {
    use super::{settles_this_tick, REHOME_COOLDOWN_TICKS};
    assert!(settles_this_tick(1, true, 2));
    assert!(
        !settles_this_tick(REHOME_COOLDOWN_TICKS, true, 2),
        "spawn tick: cooling is full"
    );
    assert!(!settles_this_tick(2, true, 2), "still cooling");
    assert!(
        !settles_this_tick(0, true, 2),
        "already settled — no re-save every tick"
    );
    assert!(!settles_this_tick(1, false, 2), "not ready is not settled");
    assert!(
        !settles_this_tick(1, true, 0),
        "no lanes is nothing to remember"
    );
}

// what this catches (the M5, 2026-09-19 23:13–23:15Z): the spawn cooldown (18 ticks) was
// spent DURING the 27B's 90 s cold load — the old lane kept serving, so the ticks
// counted — and the new lane came ready with zero settle left; its predecessor's freed
// memory read as a 32 GB budget for a minute, the plan said 3 lanes, and a second
// relaunch fired 15 s after the first came ready (12 settles in the hour, acts 5). A
// launch settles AFTER it is ready: a ready stamp not yet settled on is the edge; the
// same stamp again is not; a not-ready snapshot never is.
#[test]
fn a_launch_settles_after_it_is_ready_not_after_it_was_spawned() {
    assert!(
        ready_edge(0, Some(1_000)),
        "the first ready stamp: settle from here"
    );
    assert!(
        !ready_edge(1_000, Some(1_000)),
        "the same stamp is the same launch"
    );
    assert!(
        ready_edge(1_000, Some(2_000)),
        "a new stamp is a new launch: settle again"
    );
    assert!(!ready_edge(1_000, None), "not ready: nothing to settle on");
}

// what this catches (5090, 2026-09-06 — BigMama's third bug): an empty plan
// publishing "idle" while the llama-server stays alive. At the live → empty
// transition the daemon takes the lane down exactly once; a second tick with
// the plan still empty does not idle again.
#[tokio::test]
async fn an_empty_plan_takes_the_live_lane_down_once() {
    let serves = Arc::new(AtomicUsize::new(0));
    let server = Arc::new(FakeServer::healthy(serves, true));
    let idles = server.idles.clone();
    // An EMPTY catalog, so the plan is None on every host — the real catalog would
    // find this Mac's own GGUFs and plan a model, which is not this test's question.
    let gpu = Arc::new(GpuMemoryManager::simulated("Apple M5 Pro", 53 * GB));
    let system = Arc::new(SystemResourceMonitor::new());
    let empty =
        crate::model_registry::Registry::from_catalog(vec![], vec![]).expect("empty registry");
    let daemon = ServingDaemonModule::with_control(
        gpu,
        system,
        test_resource_daemon(),
        server,
        Arc::new(ModelCatalog::from_registry(&empty)),
        test_pin_store(),
    );
    let mut live = ServingSnapshot::empty();
    live.active_model = Some("ornith-ai/Ornith-1.5-35B-A3B-GGUF".to_string());
    live.ready = true;
    let _ = daemon.serving_tx.send_replace(live);
    // Nothing on disk is servable in a test → the reconcile's empty arm.
    daemon.recompute();
    let queued = daemon.reconcile_to_plan().expect("empty-plan operation");
    assert!(daemon.reconcile_to_plan().is_none(), "one operation owner");
    queued.abort();
    assert!(queued.await.unwrap_err().is_cancelled());
    assert!(
        !daemon.reconciling.load(Ordering::Acquire),
        "abort before first poll releases gate"
    );
    assert_eq!(idles.load(Ordering::SeqCst), 0);
    let superseded = daemon.reconcile_to_plan().expect("queued idle");
    daemon.intent.set_pin(None, true);
    superseded.await.unwrap();
    assert_eq!(
        idles.load(Ordering::SeqCst),
        0,
        "newer intent supersedes queued idle"
    );
    daemon.recompute();
    daemon
        .reconcile_to_plan()
        .expect("current idle")
        .await
        .unwrap();
    assert_eq!(
        idles.load(Ordering::SeqCst),
        1,
        "the lane is taken down at live → empty"
    );
    daemon.recompute();
    assert!(daemon.reconcile_to_plan().is_none());
    assert_eq!(
        idles.load(Ordering::SeqCst),
        1,
        "still empty: no second teardown"
    );
    daemon
        .serving_tx
        .send_modify(|snapshot| snapshot.loading_model = Some("cancelled-boot".into()));
    daemon
        .reconcile_to_plan()
        .expect("empty intent retires an interrupted load")
        .await
        .unwrap();
    assert_eq!(idles.load(Ordering::SeqCst), 2);
    assert!(daemon.serving_tx.borrow().loading_model.is_none());
}

// what this catches: the EnsureOutcome → ServingSnapshot mapping. A live or
// spawned model is ready with the served base url AND the real per-slot window
// it carries through to personas; a degraded reconcile — OR a ready server
// whose served window was unreadable (0) — publishes "nothing live", never a
// half-true ready-with-no-model or ready-with-zero-window (the drift fix:
// budgeting against a zero/guessed window is exactly what we refuse).
#[test]
fn snapshot_mapping_is_honest() {
    let genes = vec!["/genes/a.gguf".to_string()];
    let up = snapshot_from_outcome(
        &EnsureOutcome::Spawned { model: "m".into() },
        "coder-14b",
        &genes,
        11008,
        4,
        8_704,
        Some(crate::inference::vision_sidecar::SidecarLane {
            base_url: "http://127.0.0.1:58091/v1".to_string(),
            model_id: "vl-7b".to_string(),
        }),
    );
    assert_eq!(up.active_model.as_deref(), Some("coder-14b"));
    assert!(up.ready);
    assert!(up.base_url.ends_with("/v1"));
    assert_eq!(
        up.adapters, genes,
        "live snapshot carries the loaded genome set"
    );
    assert_eq!(
        up.served_context_window, 11008,
        "ready snapshot carries the real per-slot window personas budget to"
    );
    assert_eq!(
        up.lanes, 4,
        "ready snapshot carries the --parallel lane count for total-KV accounting"
    );
    assert_eq!(
        up.host_prompt_cache_mib, 8_704,
        "ready snapshot carries the grant the engine was launched with"
    );
    assert!(
        up.vision_ready,
        "the reconcile's verified multimodal verdict must survive into the snapshot \
             — the observe path gates on THIS field (#106)"
    );
    assert_eq!(
        up.vision_base_url.as_deref(),
        Some("http://127.0.0.1:58091/v1"),
        "the ONE vision value projects the routing address with the flag — an \
             address can never publish without verified readiness"
    );
    assert_eq!(up.vision_model.as_deref(), Some("vl-7b"));

    let already = snapshot_from_outcome(
        &EnsureOutcome::AlreadyServing,
        "coder-14b",
        &genes,
        11008,
        4,
        0,
        None,
    );
    assert_eq!(already.active_model.as_deref(), Some("coder-14b"));
    assert!(already.ready);
    assert_eq!(already.served_context_window, 11008);
    assert_eq!(already.lanes, 4);
    assert!(
        !already.vision_ready,
        "a text lane (no verified endpoint) must never read as sighted"
    );
    assert!(
        already.vision_base_url.is_none() && already.vision_model.is_none(),
        "no verified endpoint → no address, no model (None-iff-not-ready)"
    );

    // Ready outcome but the served window was unreadable (0) → publish the gap,
    // NOT a ready snapshot with a zero window a persona would budget against.
    let windowless = snapshot_from_outcome(
        &EnsureOutcome::Spawned { model: "m".into() },
        "coder-14b",
        &genes,
        0,
        4,
        8_704,
        None,
    );
    assert_eq!(
        windowless.active_model, None,
        "ready-but-no-window → nothing live"
    );
    assert!(!windowless.ready);
    assert_eq!(windowless.served_context_window, 0);
    assert_eq!(windowless.lanes, 0, "empty snapshot carries no lanes");
    assert_eq!(
        windowless.host_prompt_cache_mib, 0,
        "nothing live → no grant to charge"
    );
    let degraded = snapshot_from_outcome(
        &EnsureOutcome::Degraded { reason: "x".into() },
        "coder-14b",
        &genes,
        11008,
        4,
        8_704,
        None,
    );
    assert_eq!(degraded.active_model, None, "degraded → nothing live");
    assert!(!degraded.ready);
    assert!(degraded.adapters.is_empty(), "degraded → no genome claimed");
    // regression for the 2026-07-24 Windows repro: the spawn-failure reason
    // must SURVIVE into the published snapshot — serving/status saying only
    // null/false while spawn fails every tick is the silent-failure lie.
    assert_eq!(
        degraded.degraded_reason.as_deref(),
        Some("x"),
        "degraded reason must reach the snapshot"
    );
    assert_eq!(
        windowless.degraded_reason, None,
        "a windowless-ready snapshot is not degraded — no reason claimed"
    );
}

// what this catches: the snapshot describes the ENGINE, not the plan. Regression for
// the M5 at 2026-09-20 07:52Z — the plan fell to 1 lane, the 2-slot engine was adopted
// (slots ≥ target never relaunch), and the snapshot published the target's count while
// the footprint measurement, the ambient budget and the roster bound all read it as the
// engine's. A 2-slot engine under a 1-lane target publishes 2; an engine that names no
// slot count falls back to the target's; and the grant rides the snapshot from the
// launch, so a target re-derived under a running engine cannot move the number the
// measurement subtracts.
#[test]
fn the_snapshot_reports_the_engines_lanes_and_grant_not_the_plans() {
    assert_eq!(
        published_lanes(Some(2), 1),
        2,
        "the engine's slots, not the plan's"
    );
    assert_eq!(
        published_lanes(Some(0), 1),
        1,
        "no slot count named → the target's"
    );
    assert_eq!(published_lanes(None, 3), 3, "unreadable → the target's");
    let snap = snapshot_from_outcome(
        &EnsureOutcome::AlreadyServing,
        "qwen-27b",
        &[],
        67_072,
        published_lanes(Some(2), 1),
        14_396,
        None,
    );
    assert_eq!(snap.lanes, 2);
    assert_eq!(snap.host_prompt_cache_mib, 14_396);
    assert_eq!(
        attribute_serving(20 * GB, snap.lanes, snap.host_prompt_cache_mib),
        20 * GB + 14_396 * 1024 * 1024,
        "the board credits the grant the engine holds"
    );
}

// what this catches (#363 wedge-heal floor): a wedge declaration EMPTIES the live
// snapshot, which disarms the sticky floor above — so the very relaunch the wedge
// detector triggers used to spawn at a teardown-transient plan (measured live: plan
// dipped to 9,984 while the dying 34,048 predecessor still held its memory; every
// subsequent 12k prompt was rejected). The successor must inherit the last HEALTHY
// window when lanes are unchanged; a live-ready lane defers to sticky; a lane-count
// change resizes legitimately; and no-history (cold first boot) passes the plan.
#[test]
fn wedge_heal_floor_holds_the_last_healthy_window_across_the_empty_snapshot() {
    // The measured blackout shape: not ready (wedge emptied it), same lanes,
    // transient plan far below the last healthy window → floored.
    assert_eq!(wedge_heal_floor(9_984, 2, false, 34_048, 2), 34_048);
    // Live lane still ready → sticky owns the decision; floor stands down.
    assert_eq!(wedge_heal_floor(9_984, 2, true, 34_048, 2), 9_984);
    // Lane-count change legitimately resizes KV → plan through.
    assert_eq!(wedge_heal_floor(9_984, 4, false, 34_048, 2), 9_984);
    // Plan already >= last healthy → never held down.
    assert_eq!(wedge_heal_floor(41_216, 2, false, 34_048, 2), 41_216);
    // No healthy lane ever observed (cold boot) → plan through.
    assert_eq!(wedge_heal_floor(9_984, 2, false, 0, 0), 9_984);
}

// what this catches: a published plan drives a reconcile that brings the
// server up and publishes a ready ServingSnapshot for that model — the
// plan→reality wiring. Regression here = the daemon decides but never acts.
#[tokio::test]
async fn reconcile_brings_planned_model_up() {
    let serves = Arc::new(AtomicUsize::new(0));
    let mut daemon = daemon_with(Arc::new(FakeServer::healthy(serves.clone(), true)));

    // Publish a plan (most-capable fitting model = coder-14b).
    let budget = HostBudget {
        usable_bytes: 45 * GB,
        perf_cores: 6,
    };
    let candidates = vec![footprint_from_parts("coder-14b", 9 * GB, 8192, true, None).unwrap()];
    daemon.publish_plan(budget, &candidates, &candidates);

    // What this catches: an intent change during target preparation must not
    // unprotect the incumbent or attribute an unlaunched resident on refusal.
    let paths = tempfile::tempdir().expect("isolated artifact paths");
    let incumbent = paths.path().join("incumbent.gguf");
    let candidate = paths.path().join("candidate.gguf");
    daemon.set_active_artifact(Some(incumbent.clone()));
    *daemon.served_resident.lock().unwrap() = Some(incumbent.clone());
    let resolver = daemon.model_resolver.clone();
    let preparing = resolver.clone();
    let intent = daemon.intent.clone();
    daemon.set_model_resolver(Arc::new(move |id| {
        intent.set_pin(Some(id.to_string()), true);
        preparing(id).map(|mut model| {
            model.gguf_local_path = Some(candidate.clone());
            model
        })
    }));
    assert!(daemon.reconcile_to_plan().is_none());
    assert_eq!(
        daemon.active_artifact.lock().unwrap().as_ref(),
        Some(&incumbent)
    );
    assert_eq!(
        daemon.served_resident.lock().unwrap().as_ref(),
        Some(&incumbent)
    );
    assert_eq!(serves.load(Ordering::SeqCst), 0);
    daemon.set_active_artifact(None);
    daemon.set_model_resolver(resolver);
    daemon.publish_plan(budget, &candidates, &candidates);

    let handle = daemon
        .reconcile_to_plan()
        .expect("a reconcile should be spawned");
    handle.await.unwrap();

    let snap = daemon.subscribe_serving().borrow().clone();
    assert_eq!(snap.active_model.as_deref(), Some("coder-14b"));
    assert!(snap.ready);
    assert_eq!(serves.load(Ordering::SeqCst), 1, "served exactly once");
    assert!(
        daemon.verified_serving_target().is_none(),
        "an unowned fixture cannot invent launch provenance"
    );

    // A no-op probe never crosses lifecycle admission. New intent during
    // that probe must prevent downstream publication/sidecar actions too.
    let intent = ServingIntent::new(None);
    let changed = intent.clone();
    let mut server = FakeServer::healthy(serves.clone(), true);
    server.active = Some("coder-14b".into());
    server.probe_effect = Some(Arc::new(move || {
        changed.set_pin(Some("coder-14b".into()), true);
    }));
    let mut stale = daemon_with(Arc::new(server));
    stale.intent = intent;
    stale.publish_plan(budget, &candidates, &candidates);
    stale.plan_tx.send_modify(|snapshot| {
        let plan = snapshot.plan.as_mut().unwrap();
        plan.served_context_window = 11008;
        plan.lanes = 1;
    });
    let queued = stale.reconcile_to_plan().expect("queued serve");
    queued.abort();
    assert!(queued.await.unwrap_err().is_cancelled());
    assert!(!stale.reconciling.load(Ordering::Acquire));
    stale
        .reconcile_to_plan()
        .expect("probe reconcile")
        .await
        .unwrap();
    assert!(
        stale.serving_tx.borrow().active_model.is_none(),
        "superseded no-op cannot publish"
    );
    assert_eq!(
        serves.load(Ordering::SeqCst),
        1,
        "no new lifecycle admission"
    );
    assert!(stale.active_artifact.lock().unwrap().is_none());

    // Stop inside the real operation future after admission. The old ready
    // incumbent must already be unavailable, and cancellation releases ownership.
    let entered = Arc::new(tokio::sync::Notify::new());
    let mut server = FakeServer::healthy(serves.clone(), true);
    server.serve_started = Some(entered.clone());
    let cancelled = daemon_with(Arc::new(server));
    cancelled.publish_plan(budget, &candidates, &candidates);
    let mut incumbent = ready_snapshot();
    incumbent.active_model = Some("previous-model".into());
    let _ = cancelled.serving_tx.send_replace(incumbent);
    *cancelled.pending_model_change.lock().unwrap() = Some("coder-14b".into());
    cancelled
        .model_change_streak
        .store(REHOME_SUSTAINED_TICKS, Ordering::Relaxed);
    cancelled.force_relaunch.store(true, Ordering::Release);
    let admitted = cancelled.reconcile_to_plan().expect("admitted serve");
    tokio::time::timeout(Duration::from_secs(5), entered.notified())
        .await
        .expect("operation reaches controlled await");
    assert!(!cancelled.serving_tx.borrow().ready);
    assert_eq!(
        cancelled.serving_tx.borrow().active_model.as_deref(),
        Some("previous-model")
    );
    assert_eq!(
        cancelled.serving_tx.borrow().loading_model.as_deref(),
        Some("coder-14b")
    );
    admitted.abort();
    assert!(admitted.await.unwrap_err().is_cancelled());
    assert!(!cancelled.reconciling.load(Ordering::Acquire));
    assert!(
        cancelled.force_relaunch.load(Ordering::Acquire),
        "cancelled forced probe is still owed"
    );
    let retry = cancelled
        .reconcile_to_plan()
        .expect("cancellation cannot strand recovery");
    retry.abort();
    assert!(retry.await.unwrap_err().is_cancelled());

    // The same forced-probe claim survives cancellation during preflight,
    // before any lifecycle admission or readiness mutation is permitted.
    let entered = Arc::new(tokio::sync::Notify::new());
    let mut server = FakeServer::healthy(serves, true);
    server.probe_started = Some(entered.clone());
    let probing = daemon_with(Arc::new(server));
    probing.publish_plan(budget, &candidates, &candidates);
    probing.force_relaunch.store(true, Ordering::Release);
    let task = probing.reconcile_to_plan().expect("preflight probe");
    tokio::time::timeout(Duration::from_secs(5), entered.notified())
        .await
        .expect("operation reaches controlled await");
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert!(probing.force_relaunch.load(Ordering::Acquire));
    assert!(!probing.reconciling.load(Ordering::Acquire));
    assert!(
        probing.serving_tx.borrow().loading_model.is_none(),
        "no admission before probe completes"
    );
}

// what this catches: once the desired model is ready, a subsequent reconcile
// is a NO-OP (no relaunch) — otherwise every tick would thrash the
// GPU-warm server.
#[tokio::test]
async fn reconcile_is_noop_when_already_serving() {
    let serves = Arc::new(AtomicUsize::new(0));
    let server = FakeServer::healthy(serves.clone(), true);
    let verified_reads = server.verified_reads.clone();
    let daemon = daemon_with(Arc::new(server));

    // Pretend coder-14b is already up and ready.
    let _ = daemon.serving_tx.send_replace(ServingSnapshot {
        // test fixture: no live readiness was ever CONFIRMED here.
        ready_verified_at_ms: None,
        active_model: Some("coder-14b".into()),
        ready: true,
        base_url: serving_v1_url(),
        adapters: Vec::new(),
        served_context_window: 11008,
        lanes: 4,
        host_prompt_cache_mib: 0,
        degraded_reason: None,
        vision_ready: false,
        vision_base_url: None,
        vision_model: None,
        loading_model: None,
        held_for: None,
    });
    let budget = HostBudget {
        usable_bytes: 45 * GB,
        perf_cores: 6,
    };
    let candidates = vec![footprint_from_parts("coder-14b", 9 * GB, 8192, true, None).unwrap()];
    daemon.publish_plan(budget, &candidates, &candidates);

    assert!(
        daemon.reconcile_to_plan().is_none(),
        "already serving → no reconcile"
    );
    assert_eq!(serves.load(Ordering::SeqCst), 0, "no relaunch");
    let reads = verified_reads.load(Ordering::SeqCst);
    assert!(
        reads > 0,
        "same-model retention considers original provenance"
    );
    daemon.intent.set_pin(Some("other-model".into()), true);
    let revision = daemon.intent.snapshot().revision;
    daemon.plan_tx.send_modify(|snapshot| {
        snapshot.intent_revision = revision;
        snapshot.plan.as_mut().unwrap().base_model.model_id = "other-model".into();
    });
    assert!(
        daemon.reconcile_to_plan().is_none(),
        "first model change is deferred"
    );
    assert_eq!(
        verified_reads.load(Ordering::SeqCst),
        reads,
        "a deferred different model cannot restamp the incumbent capture"
    );
    assert!(daemon.verified_serving_target().is_none());
}

// what this catches: paging quarantine must bypass the unchanged healthy
// snapshot shortcut and reach the existing reconcile/serve owner.
#[tokio::test]
async fn paging_quarantine_reconciles_an_unchanged_healthy_plan() {
    let serves = Arc::new(AtomicUsize::new(0));
    let root = format!("test://reconcile-paging-{}", uuid::Uuid::new_v4());
    let endpoint = crate::inference::slots::directory().endpoint(&root);
    let mut server = FakeServer::healthy(serves.clone(), false);
    assert!(
        server.smoke_ok.load(Ordering::Relaxed),
        "control-plane health cannot heal paging uncertainty"
    );
    server.paging_endpoint = Some(endpoint.clone());
    let daemon = daemon_with(Arc::new(server));
    let budget = HostBudget {
        usable_bytes: 45 * GB,
        perf_cores: 6,
    };
    let candidates = vec![footprint_from_parts("coder-14b", 9 * GB, 8192, true, None).unwrap()];
    daemon.publish_plan(budget, &candidates, &candidates);
    let plan = daemon
        .plan_tx
        .borrow()
        .plan
        .clone()
        .expect("published plan");
    let mut live = ready_snapshot();
    live.active_model = Some("coder-14b".into());
    live.served_context_window = plan.served_context_window;
    live.lanes = plan.lanes;
    live.base_url = format!("{root}/v1");
    let _ = daemon.serving_tx.send_replace(live);
    assert!(
        daemon.reconcile_to_plan().is_none(),
        "healthy unchanged plan stays resident"
    );
    let admission = endpoint.admit().await.expect("initial endpoint admission");
    admission.quarantine_paging();
    drop(admission);
    let replacement = daemon
        .reconcile_to_plan()
        .expect("quarantine enters normal replacement");
    assert!(
        !daemon.serving_tx.borrow().ready,
        "publish refusal before replacement awaits"
    );
    replacement.await.expect("normal reconcile task");
    assert_eq!(serves.load(Ordering::SeqCst), 1);
    assert!(
        !daemon.serving_tx.borrow().ready,
        "failed replacement cannot publish ready"
    );
    assert!(
        endpoint.paging_recovery_required(),
        "only verified generation replacement clears quarantine"
    );
}

// what this catches: no servable plan (empty registry) publishes the empty
// snapshot so readers (and a grid allocator) see "nothing live here" and can
// route the lease elsewhere — the Intel-Mac/weak-node path.
#[tokio::test]
async fn no_plan_publishes_empty_snapshot() {
    let serves = Arc::new(AtomicUsize::new(0));
    let daemon = daemon_with(Arc::new(FakeServer::healthy(serves.clone(), true)));

    // Seed a live snapshot, then publish an empty plan → must clear to empty.
    let _ = daemon.serving_tx.send_replace(ServingSnapshot {
        // test fixture: no live readiness was ever CONFIRMED here.
        ready_verified_at_ms: None,
        active_model: Some("stale".into()),
        ready: true,
        base_url: serving_v1_url(),
        adapters: Vec::new(),
        served_context_window: 11008,
        lanes: 4,
        host_prompt_cache_mib: 0,
        degraded_reason: None,
        vision_ready: false,
        vision_base_url: None,
        vision_model: None,
        loading_model: None,
        held_for: None,
    });
    let budget = HostBudget {
        usable_bytes: 45 * GB,
        perf_cores: 6,
    };
    daemon.publish_plan(budget, &[], &[]); // no candidates → plan None

    daemon
        .reconcile_to_plan()
        .expect("empty plan uses the operation owner")
        .await
        .unwrap();
    let snap = daemon.subscribe_serving().borrow().clone();
    assert_eq!(snap.active_model, None, "stale snapshot cleared to empty");
    assert!(!snap.ready);
    assert_eq!(serves.load(Ordering::SeqCst), 0);
}

// what this catches: a degraded reconcile (serve fails) publishes "nothing
// live" and clears the in-flight gate, so the NEXT reconcile can retry
// rather than being permanently locked out.
#[tokio::test]
async fn degraded_reconcile_clears_gate_and_publishes_empty() {
    let serves = Arc::new(AtomicUsize::new(0));
    let daemon = daemon_with(Arc::new(FakeServer::healthy(serves.clone(), false)));

    let budget = HostBudget {
        usable_bytes: 45 * GB,
        perf_cores: 6,
    };
    let candidates = vec![footprint_from_parts("coder-14b", 9 * GB, 8192, true, None).unwrap()];
    daemon.publish_plan(budget, &candidates, &candidates);

    daemon.reconcile_to_plan().expect("spawned").await.unwrap();
    let snap = daemon.subscribe_serving().borrow().clone();
    assert!(!snap.ready, "degraded → not ready");
    assert_eq!(snap.active_model, None);
    assert!(
        !daemon.reconciling.load(Ordering::SeqCst),
        "gate cleared for retry"
    );

    // Gate cleared → a retry actually spawns again.
    daemon
        .reconcile_to_plan()
        .expect("retry spawned")
        .await
        .unwrap();
    assert_eq!(serves.load(Ordering::SeqCst), 2, "retried after degrade");
}

// what this catches: a reconcile emits the live snapshot on the BUS (topic
// serving.snapshot), not just the in-process watch — the cbar fan-out that
// lets any subscriber (and a remote grid allocator) get serving state pushed
// without point-to-point plumbing. Regression = silent watch-only updates
// that no subscriber ever sees.
#[tokio::test]
async fn reconcile_emits_snapshot_on_the_bus() {
    let serves = Arc::new(AtomicUsize::new(0));
    let daemon = daemon_with(Arc::new(FakeServer::healthy(serves.clone(), true)));
    let bus = Arc::new(MessageBus::new());
    let _ = daemon.bus.set(bus.clone());

    let budget = HostBudget {
        usable_bytes: 45 * GB,
        perf_cores: 6,
    };
    let candidates = vec![footprint_from_parts("coder-14b", 9 * GB, 8192, true, None).unwrap()];
    daemon.publish_plan(budget, &candidates, &candidates);
    daemon.reconcile_to_plan().expect("spawned").await.unwrap();

    let event = bus
        .find_recent_event(SERVING_SNAPSHOT_EVENT)
        .expect("serving.snapshot must be emitted on the bus");
    let snap: ServingSnapshot = serde_json::from_value((*event.payload).clone()).unwrap(); // test-only decode of the shared bus payload
    assert_eq!(snap.active_model.as_deref(), Some("coder-14b"));
    assert!(snap.ready, "emitted snapshot reflects the live model");
    // find_recent_event consumes newest first. A fast completion must not
    // coalesce with admission, or remote consumers remain falsely unavailable.
    let admission = bus
        .find_recent_event(SERVING_SNAPSHOT_EVENT)
        .expect("admission must reach bus consumers before completion");
    let admission: ServingSnapshot = serde_json::from_value((*admission.payload).clone()).unwrap();
    assert!(!admission.ready);
    assert_eq!(admission.loading_model.as_deref(), Some("coder-14b"));
    assert!(
        bus.find_recent_event(SERVING_SNAPSHOT_EVENT).is_none(),
        "one admission and one completion"
    );
}

// what this catches (card 4d2a64a2; Fable on #5500): the one place the daemon commits to an
// engine transition (`publish_lifecycle_admission`: relaunch, quarantine replacement,
// convergence, retirement) must condemn the engine, so acts in flight end before the drain
// instead of at their next checkpoint. Readiness alone (a transient 0 `/props` window under
// AlreadyServing) is published elsewhere and never condemns: `until_condemned` reads no snapshot.
#[test]
fn an_admitted_lifecycle_transition_condemns_the_engine() {
    let condemned = crate::inference::llama_server::condemnations();
    let (serving, _) = tokio::sync::watch::channel(ServingSnapshot::empty());
    let (verified, _) = tokio::sync::watch::channel(None);
    ServingDaemonModule::publish_lifecycle_admission(&serving, &verified, None, Some("coder-14b"));
    assert!(condemned.has_changed().unwrap(), "an admitted transition condemns the engine"); // unwrap: the sender is a process-lifetime static
    assert!(!serving.borrow().ready, "and publishes it not-ready, as before");
}
