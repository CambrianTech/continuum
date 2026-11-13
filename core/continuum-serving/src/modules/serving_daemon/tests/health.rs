#![cfg(test)]

use super::*;

// what this catches: a self-heal that renders as a dead node. `declare_lane_wedged`
// TAKES the reason as a parameter, logged it, then published `ServingSnapshot::empty()`
// — which hardcodes `degraded_reason: None`. For the whole kill+respawn window
// `serving/status` showed `ready:false, active_model:null, lanes:0,
// degraded_reason:null`: absence with no stated cause, byte-identical to a node that
// never had a brain. Measured 2026-09-05 on BigMama, where a wedge-heal recovering by
// itself inside 120s was read three times as a possible serving-is-dead defect, and
// the pre-flight guard's `degraded=` probe field (added 2026-08-15 precisely to carry
// this string) was handed "" on every wait. The reason is IN HAND at the call site;
// dropping it is the silent-failure class, not a formatting nit.
#[test]
fn declaring_a_wedge_publishes_the_reason_not_a_bare_empty_snapshot() {
    use super::*;

    let (tx, _rx) = watch::channel(ServingSnapshot::empty());
    let force = Arc::new(AtomicBool::new(false));

    ServingDaemonModule::declare_lane_wedged(
        &force,
        &tx,
        None,
        "consecutive REAL generations failed on the live lane",
    );

    let snap = tx.borrow();
    // the heal still fires: not-ready, nothing active, relaunch armed
    assert!(!snap.ready, "a declared wedge must flip the lane not-ready");
    assert!(snap.active_model.is_none());
    assert!(
        force.load(Ordering::Relaxed),
        "the relaunch flag is what makes this a HEAL — populating the reason must \
             not change the control path"
    );
    // ...and the operator is told WHY, verbatim
    assert_eq!(
        snap.degraded_reason.as_deref(),
        Some("consecutive REAL generations failed on the live lane"),
        "the wedge reason must reach the snapshot, not only the log"
    );
}

// what this catches: THE WIRING. The 2026-08-05 outage was not a missing detector — the
// reap actuator already existed and was tested. What was missing was anything that CALLED
// it, and a detector that reports into a void is indistinguishable from no detector at
// all. This drives the whole chain the live system uses: the stderr watcher raises the
// flag → the daemon tick takes it → the lane is published NOT-ready.
//
// The not-ready assertion is the load-bearing half. Arming `force_relaunch` alone is
// silently useless: `reconcile_to_plan` returns early on `ready && same model && same
// adapters` BEFORE it ever reads the flag, so a wedged-but-"ready" lane would sit there
// exactly as it did for four hours. regression for the 172 GB log outage
#[tokio::test]
async fn a_reported_wedge_flips_the_lane_not_ready() {
    let fake = Arc::new(FakeServer::healthy(Arc::new(AtomicUsize::new(0)), true));
    let daemon = daemon_with(fake.clone());
    let _ = daemon.serving_tx.send_replace(ready_snapshot());

    // Nothing reported yet: a healthy lane is left alone.
    daemon.take_reported_wedge();
    assert!(
        daemon.serving_tx.borrow().ready,
        "an unreported lane must never be reaped"
    );

    // The stderr watcher sees `progress = 1.10` four times and raises.
    fake.wedge.raise();
    daemon.take_reported_wedge();

    assert!(
        !daemon.serving_tx.borrow().ready,
        "a reported wedge must publish NOT-ready, or the reconcile's no-op guard \
             returns before it ever consumes the force-relaunch flag"
    );
    assert!(
        daemon.force_relaunch.load(Ordering::Acquire),
        "the reconcile must be told to re-prove decode on a child we own"
    );

    // The report is consumed exactly once — a second tick must not re-escalate a lane
    // that is already being relaunched.
    let _ = daemon.serving_tx.send_replace(ready_snapshot());
    daemon.take_reported_wedge();
    assert!(
        daemon.serving_tx.borrow().ready,
        "take clears the flag; one report is one escalation"
    );
}

// what this catches: #175 self-heal (detection + recovery). A lane the daemon believes
// is `ready` but whose COMPUTE path is wedged — the decode heartbeat fails while the
// control plane still 200s, the exact Metal-OOM-poison shape — is flipped NOT-ready
// after HEALTH_FAILS_TO_RELAUNCH consecutive heartbeats (which the reconcile then turns
// into a kill+respawn). Before this the owned lane was trusted forever and the persona
// substrate stayed bricked until a human reboot. regression for #175
#[tokio::test]
async fn health_heartbeat_flips_ready_after_sustained_decode_failure() {
    let serves = Arc::new(AtomicUsize::new(0));
    let smoke = Arc::new(AtomicBool::new(false)); // wedged compute path
    let daemon = daemon_with(Arc::new(FakeServer {
        smoke_ok: smoke.clone(),
        ..FakeServer::healthy(serves, true)
    }));
    let _ = daemon.serving_tx.send_replace(ready_snapshot());

    // First heartbeat (tick 0) fails once — hysteresis holds, still ready.
    if let Some(h) = daemon.spawn_health_heartbeat_if_due() {
        h.await.unwrap();
    }
    assert!(
        daemon.serving_tx.borrow().ready,
        "one failed probe is not enough — a merely-busy lane must not be reaped"
    );

    // Force the NEXT call to be a heartbeat tick; the second failure reaches threshold.
    daemon.health_ticks.store(0, Ordering::Relaxed);
    if let Some(h) = daemon.spawn_health_heartbeat_if_due() {
        h.await.unwrap();
    }
    let s = daemon.serving_tx.borrow();
    assert!(
        !s.ready,
        "sustained decode failure flips the lane not-ready so the reconcile respawns it (#175)"
    );
    assert!(
        s.active_model.is_none(),
        "not-ready is published as the empty gap the reconcile relaunches from"
    );
}

// what this catches: hysteresis RESET. A lane that fails a heartbeat once then RECOVERS
// (probe passes) must have its failure streak cleared — so an isolated failure minutes
// later never accumulates with a stale one into a spurious reap.
#[tokio::test]
async fn health_heartbeat_streak_resets_on_a_passing_probe() {
    let serves = Arc::new(AtomicUsize::new(0));
    let smoke = Arc::new(AtomicBool::new(false));
    let daemon = daemon_with(Arc::new(FakeServer {
        smoke_ok: smoke.clone(),
        ..FakeServer::healthy(serves, true)
    }));
    let _ = daemon.serving_tx.send_replace(ready_snapshot());

    // Fail once.
    if let Some(h) = daemon.spawn_health_heartbeat_if_due() {
        h.await.unwrap();
    }
    assert!(daemon.serving_tx.borrow().ready);

    // Lane recovers; next heartbeat passes → streak resets.
    smoke.store(true, Ordering::Relaxed);
    daemon.health_ticks.store(0, Ordering::Relaxed);
    if let Some(h) = daemon.spawn_health_heartbeat_if_due() {
        h.await.unwrap();
    }
    assert!(daemon.serving_tx.borrow().ready);

    // Fail once more — because the streak reset, this is the FIRST failure again, so
    // it must NOT reap.
    smoke.store(false, Ordering::Relaxed);
    daemon.health_ticks.store(0, Ordering::Relaxed);
    if let Some(h) = daemon.spawn_health_heartbeat_if_due() {
        h.await.unwrap();
    }
    assert!(
        daemon.serving_tx.borrow().ready,
        "a passing probe reset the streak — one later failure is not 'sustained'"
    );
}

// what this catches: the SLOW cadence. The heartbeat must NOT probe every tick (that
// would burn one GPU decode per 5s tick, the load-per-tick the trust short-circuit
// exists to avoid); it fires only every HEALTH_PROBE_EVERY_TICKS.
#[tokio::test]
async fn health_heartbeat_only_fires_on_the_slow_cadence() {
    let serves = Arc::new(AtomicUsize::new(0));
    let smoke = Arc::new(AtomicBool::new(false));
    let daemon = daemon_with(Arc::new(FakeServer {
        smoke_ok: smoke.clone(),
        ..FakeServer::healthy(serves, true)
    }));
    let _ = daemon.serving_tx.send_replace(ready_snapshot());

    // Tick 0 is due.
    let first = daemon.spawn_health_heartbeat_if_due();
    assert!(first.is_some(), "tick 0 probes");
    if let Some(h) = first {
        h.await.unwrap();
    }
    // The next HEALTH_PROBE_EVERY_TICKS-1 ticks must NOT probe, so one failure can't be
    // compounded to the reap threshold faster than the cadence.
    for _ in 1..HEALTH_PROBE_EVERY_TICKS {
        assert!(
            daemon.spawn_health_heartbeat_if_due().is_none(),
            "non-heartbeat ticks skip the probe"
        );
    }
    assert!(
        daemon.serving_tx.borrow().ready,
        "only one probe ran across the cadence window → hysteresis holds"
    );
}

// what this catches: DELIVERY BEATS PROBING — the trust short-circuit that was
// untestable until the decode-age evidence became a per-daemon seam. A lane that
// delivered real tokens inside the probe window is PROVEN alive by work that already
// happened, so the heartbeat must not spend a slot re-proving it; a lane whose last
// decode is older than the window has no live evidence and must probe. Regresses the
// measured SWE-bench kill (v13): a busy lane failed two synthetic probes purely by
// losing the slot race, the relaunch left every downstream generate refusing with
// `serving: <none>`, and the run died — the recovery being scored as a capability zero
// ([[a-benchmark-zero-is-a-claim-about-the-harness-until-proven-otherwise]]).
#[tokio::test]
async fn a_lane_proven_alive_by_real_decode_is_not_probed() {
    let window_ms = TICK.as_millis() as u64 * HEALTH_PROBE_EVERY_TICKS;
    let serves = Arc::new(AtomicUsize::new(0));

    // Fresh decode INSIDE the window → trusted, no probe.
    let slots_reads = Arc::new(AtomicUsize::new(0));
    let mut busy = daemon_with(Arc::new(FakeServer {
        smoke_ok: Arc::new(AtomicBool::new(false)),
        slots_reads: slots_reads.clone(),
        ..FakeServer::healthy(serves.clone(), true)
    }));
    busy.set_decode_age_source(Arc::new(move || Some(window_ms / 2)));
    let _ = busy.serving_tx.send_replace(ready_snapshot());
    assert!(
        busy.spawn_health_heartbeat_if_due().is_none(),
        "tokens came out half a window ago — the compute path is already proven, so the \
             heartbeat must not contend for a slot with the work proving it"
    );
    // And the trust RESETS the streak: evidence of life is evidence, not a skipped verdict.
    assert_eq!(busy.health_fails.load(Ordering::Relaxed), 0);
    // The busy lane is exactly the one the prefill knee must read (card e370a673): its
    // `/slots` read runs on its own task whether or not the smoke probe is skipped. Read
    // inside the probe, it never saw a busy lane and never published on the M5.
    tokio::time::timeout(Duration::from_secs(5), async {
        while slots_reads.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect(
        "a busy lane's /slots is read for the prefill knee even when the smoke probe is skipped",
    );

    // Stale decode OUTSIDE the window → no live evidence, probe as usual.
    let mut quiet = daemon_with(Arc::new(FakeServer {
        smoke_ok: Arc::new(AtomicBool::new(true)),
        ..FakeServer::healthy(serves, true)
    }));
    quiet.set_decode_age_source(Arc::new(move || Some(window_ms + 1)));
    let _ = quiet.serving_tx.send_replace(ready_snapshot());
    let probe = quiet.spawn_health_heartbeat_if_due();
    assert!(
        probe.is_some(),
        "the last token predates the probe window — nothing proves this lane alive, so it \
             must be probed rather than trusted"
    );
    if let Some(h) = probe {
        h.await.unwrap();
    }
}

// what this catches: the #363 blackout class — a lane that REJECTS the fleet's
// real prompts (undersized slot / mid-stream death) while both trust paths stay
// green: recent-decode trust says alive, the 1-token smoke probe would pass.
// Sustained REAL-turn failure must outrank both and declare the wedge, flipping
// the snapshot not-ready + arming force_relaunch — without ever spending a probe.
// Regression for the 2026-08-07 25-minute room blackout (serving/status
// ready:true throughout, 4 citizens' turns all dying mid-stream).
#[tokio::test]
async fn sustained_real_turn_failures_outrank_a_passing_probe() {
    let serves = Arc::new(AtomicUsize::new(0));
    let mut d = daemon_with(Arc::new(FakeServer {
        // The smoke probe WOULD pass — that is the point: it must not get the
        // chance to vouch for a lane the real workload proves broken.
        smoke_ok: Arc::new(AtomicBool::new(true)),
        ..FakeServer::healthy(serves, true)
    }));
    // Fresh decode trust too (a partial stream can stamp it) — must ALSO be outranked.
    let window_ms = TICK.as_millis() as u64 * HEALTH_PROBE_EVERY_TICKS;
    d.set_decode_age_source(Arc::new(move || Some(window_ms / 2)));
    d.set_real_fails_source(Arc::new(|| HEALTH_FAILS_TO_RELAUNCH as u64));
    let _ = d.serving_tx.send_replace(ready_snapshot());

    assert!(
        d.spawn_health_heartbeat_if_due().is_none(),
        "the wedge verdict is immediate — no probe task is spawned"
    );
    assert!(
        !d.serving_tx.borrow().ready,
        "sustained real failures must flip the snapshot not-ready even though the \
             smoke probe would pass and decode trust is fresh"
    );
    assert!(
        d.force_relaunch.load(Ordering::Acquire),
        "the reconcile must be forced to re-prove the lane, not re-adopt it"
    );
}

// what this catches (L11 — the 2026-08-16 23-minute boot kill-loop): after a
// reboot, the adopted lane ground through the dead core's GHOST turns — real
// work no adapter stream observes, so decode-age trust (L9) was blind, two
// smoke misses counted as wedge evidence, and a healthy lane was killed three
// times. The heartbeat must consult the server's OWN /slots between misses:
// an ADVANCING fingerprint exonerates the miss (streak resets, lane stays
// ready); a FROZEN fingerprint keeps counting and wedges at the same
// threshold/latency as before.
#[tokio::test]
async fn a_smoke_miss_on_a_lane_with_advancing_slots_is_not_wedge_evidence() {
    let serves = Arc::new(AtomicUsize::new(0));
    let slots_fp = Arc::new(AtomicU64::new(1));
    let mut d = daemon_with(Arc::new(FakeServer {
        // Every smoke probe MISSES — the ghost work holds the slots.
        smoke_ok: Arc::new(AtomicBool::new(false)),
        slots_fp: slots_fp.clone(),
        ..FakeServer::healthy(serves, true)
    }));
    // Pin both process-global evidence sources to inert test values — the
    // globals are stamped by unrelated tests under full-suite parallelism
    // (the #7 isolation class), which would silently skip the probe path.
    d.set_decode_age_source(Arc::new(|| None));
    d.set_real_fails_source(Arc::new(|| 0));
    let _ = d.serving_tx.send_replace(ready_snapshot());

    // Drive a heartbeat probe to completion (the cadence gate only fires
    // every Nth call; bounded by that cadence).
    let probe_once = |d: &ServingDaemonModule| {
        let mut h = None;
        for _ in 0..(HEALTH_PROBE_EVERY_TICKS as usize + 1) {
            if let Some(handle) = d.spawn_health_heartbeat_if_due() {
                h = Some(handle);
                break;
            }
        }
        h.expect("a heartbeat probe must fire within one cadence window")
    };

    // Miss #1: first miss has nothing to compare — it counts (streak = 1).
    probe_once(&d).await.unwrap();
    assert_eq!(d.health_fails.load(Ordering::Relaxed), 1);
    // The serve loop ADVANCES between misses (ghost work / other clients).
    slots_fp.store(2, Ordering::Relaxed);
    // Miss #2: fingerprint moved → exonerated, streak resets, still ready.
    probe_once(&d).await.unwrap();
    assert_eq!(
        d.health_fails.load(Ordering::Relaxed),
        0,
        "slot progress between misses proves the serve loop alive — the miss \
             must reset the streak, not feed the relaunch threshold"
    );
    assert!(d.serving_tx.borrow().ready, "lane must stay ready");
    assert!(!d.force_relaunch.load(Ordering::Acquire));

    // Now FREEZE the fingerprint (the 2026-08-05 wedge signature): two more
    // misses with no slot movement must wedge exactly as before L11.
    probe_once(&d).await.unwrap();
    assert_eq!(d.health_fails.load(Ordering::Relaxed), 1);
    probe_once(&d).await.unwrap();
    assert!(
        !d.serving_tx.borrow().ready,
        "a frozen /slots across the miss window is the real wedge — detection \
             latency must be unchanged"
    );
    assert!(d.force_relaunch.load(Ordering::Acquire));

    // The processing-slot path must establish its own task/generation interval,
    // even when adapter activity is stale after an idle period or core restart.
    let now = std::time::Instant::now();
    let interval = Duration::from_millis(FROZEN_ENGINE_MS);
    let generation = uuid::Uuid::new_v4();
    let processing = serde_json::json!([{"id": 0, "id_task": 17, "is_processing": true,
            "n_prompt_tokens_processed": 200, "next_token": [{"n_decoded": 4}]}]);
    let mut observed = None;
    assert!(!FrozenSlots::observe(
        &mut observed,
        generation,
        Some(&processing),
        now
    ));
    assert!(!FrozenSlots::observe(
        &mut observed,
        generation,
        Some(&processing),
        now + interval / 2
    ));
    let mut advancing = processing.clone();
    advancing[0]["n_prompt_tokens_processed"] = serde_json::json!(201);
    assert!(!FrozenSlots::observe(
        &mut observed,
        generation,
        Some(&advancing),
        now + interval / 2
    ));
    advancing[0]["next_token"][0]["n_decoded"] = serde_json::json!(5);
    assert!(!FrozenSlots::observe(
        &mut observed,
        generation,
        Some(&advancing),
        now + interval
    ));
    let mut replacement = advancing.clone();
    replacement[0]["id_task"] = serde_json::json!(18);
    assert!(!FrozenSlots::observe(
        &mut observed,
        generation,
        Some(&replacement),
        now + interval * 2
    ));
    let successor = uuid::Uuid::new_v4();
    assert!(!FrozenSlots::observe(
        &mut observed,
        successor,
        Some(&replacement),
        now + interval * 3
    ));
    assert!(FrozenSlots::observe(
        &mut observed,
        successor,
        Some(&replacement),
        now + interval * 4
    ));
    assert!(!FrozenSlots::observe(
        &mut observed,
        successor,
        None,
        now + interval * 5
    ));
    assert!(
        observed.is_none(),
        "an unreadable interval cannot support a frozen verdict"
    );
    assert!(!FrozenSlots::observe(
        &mut observed,
        successor,
        Some(&serde_json::json!([])),
        now
    ));
    assert!(!FrozenSlots::observe(
        &mut observed,
        successor,
        Some(&serde_json::json!([{"is_processing": true}])),
        now
    ));

    // Exercise the actual heartbeat with the existing endpoint generation owner.
    let root = format!("test://frozen-{}", uuid::Uuid::new_v4());
    let directory = crate::inference::slots::directory();
    let endpoint = directory.endpoint(&root);
    let launch = endpoint.transition().await;
    let engine = launch.start_generation().unwrap();
    launch
        .ready(
            &root,
            &engine,
            crate::inference::slots::KvPageContract {
                model_id: "fixture".into(),
                model: "fixture.gguf".into(),
                adapters: vec![],
                page_dir: None,
                context: 11008,
                slots: 1,
                cache_type: None,
                flash_attn: None,
                revisions: None,
            },
        )
        .unwrap();
    drop(launch);
    let reply = processing.clone();
    let mut frozen = daemon_with(Arc::new(FakeServer {
        slots_reply: Some(Arc::new(move || reply.clone())),
        ..FakeServer::healthy(Arc::new(AtomicUsize::new(0)), true)
    }));
    let age = Arc::new(AtomicU64::new(FROZEN_ENGINE_MS * 10));
    let sampled_age = age.clone();
    frozen.set_decode_age_source(Arc::new(move || Some(sampled_age.load(Ordering::Acquire))));
    frozen.set_real_fails_source(Arc::new(|| 0));
    let mut snapshot = ready_snapshot();
    snapshot.base_url = format!("{root}/v1");
    frozen.serving_tx.send_replace(snapshot.clone());
    probe_once(&frozen).await.unwrap();
    assert!(
        frozen.serving_tx.borrow().ready,
        "a newly observed task is not frozen"
    );
    frozen.frozen_slots.lock().as_mut().unwrap().unchanged_since -= interval;
    age.store(
        TICK.as_millis() as u64 * HEALTH_PROBE_EVERY_TICKS + 1,
        Ordering::Release,
    );
    for _ in 0..HEALTH_PROBE_EVERY_TICKS {
        if let Some(probe) = frozen.spawn_health_heartbeat_if_due() {
            probe.await.unwrap();
        }
    }
    assert!(
        frozen.frozen_slots.lock().is_none(),
        "intervening activity beyond the fast trust window still invalidates the interval"
    );
    age.store(FROZEN_ENGINE_MS * 10, Ordering::Release);
    probe_once(&frozen).await.unwrap();
    assert!(
        !frozen.force_relaunch.load(Ordering::Acquire),
        "eligibility returning requires a fresh baseline"
    );
    frozen.frozen_slots.lock().as_mut().unwrap().unchanged_since -= interval;
    probe_once(&frozen).await.unwrap();
    assert!(
        frozen.force_relaunch.load(Ordering::Acquire),
        "same task unchanged across the interval is evidence"
    );

    // An engine exit during the awaited read invalidates an otherwise mature observation.
    let reply = processing.clone();
    let exiting = engine.clone();
    let mut stale = daemon_with(Arc::new(FakeServer {
        slots_reply: Some(Arc::new(move || {
            exiting.observed_exit();
            reply.clone()
        })),
        ..FakeServer::healthy(Arc::new(AtomicUsize::new(0)), true)
    }));
    stale.set_decode_age_source(Arc::new(|| Some(FROZEN_ENGINE_MS * 10)));
    stale.set_real_fails_source(Arc::new(|| 0));
    stale.prefill_knee_reading.store(true, Ordering::Release);
    stale.serving_tx.send_replace(snapshot);
    *stale.frozen_slots.lock() = Some(FrozenSlots {
        generation: directory.page_discovery_generation(&root).unwrap(),
        fingerprint: crate::inference::llama_server::slots_activity_fingerprint_of(&processing)
            .unwrap(),
        unchanged_since: std::time::Instant::now() - interval,
    });
    probe_once(&stale).await.unwrap();
    assert!(
        !stale.force_relaunch.load(Ordering::Acquire),
        "post-await generation validity is required"
    );
    assert!(stale.frozen_slots.lock().is_none());
}
