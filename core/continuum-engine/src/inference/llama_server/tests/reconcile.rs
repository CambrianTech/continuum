//! Reconcile regression fixtures.
#![cfg(test)]
use super::*;
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
    // card 22aa3379: an adopted lane records its placement, so training can name its pool.
    assert_eq!(ctrl.placements_recorded.load(Ordering::SeqCst), 1, "adoption records the placement");
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

// what this catches: a window, lane count or cache grant that differs from the plan
// relaunching an engine on the shared pool. The M5 loaded its model 40 times as the plan's
// slot count moved 1 -> 2 -> 1 (2026-11-02/03), every reload wiping every mind's cache; a
// starved 2,048 window, one slot under a four-lane plan, and a grant that holds no state are
// all runtime matters now (Joel: "Slot counts should never require shutdown").
#[tokio::test]
async fn a_window_or_lane_change_never_relaunches_an_engine_on_the_shared_pool() {
    for (served_lanes, served_window, plan_lanes, plan_window) in
        [(1, 32_768, 4, 32_768), (2, 2_048, 2, 32_768), (5, 58_112, 1, 87_410), (2, 81_920, 1, 87_410)]
    {
        let ctrl = FakeControl::probe(Ok(Some("coder-14b".into())))
            .owned()
            .launched_with_cache(1)
            .with_served_lanes(served_lanes)
            .with_served_window(served_window);
        let mut t = target("coder-14b");
        t.lanes = plan_lanes;
        t.context_window = plan_window;
        let outcome = ensure_model_serving(&ctrl, &t, false).await;
        assert_eq!(outcome, EnsureOutcome::AlreadyServing, "served {served_lanes} x {served_window}, plan {plan_lanes} x {plan_window}");
        assert_eq!(ctrl.serves.load(Ordering::SeqCst), 0, "served {served_lanes} x {served_window}, plan {plan_lanes} x {plan_window}");
    }
}

// what this catches: an engine launched with fixed slots (every node before 2026-11-03) left
// on them, so its slot count still needs a relaunch to change. It moves to the shared pool by
// one relaunch; the test above proves no further one follows.
#[tokio::test]
async fn an_engine_on_fixed_slots_moves_to_the_shared_pool_once() {
    let ctrl = FakeControl::probe(Ok(Some("coder-14b".into())))
        .owned()
        .launched_partitioned();
    let outcome = ensure_model_serving(&ctrl, &target("coder-14b"), false).await;
    assert!(matches!(outcome, EnsureOutcome::Spawned { .. }), "got {outcome:?}");
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
    // A respawned lane's launch stamps its placement; the rejected orphan's engine is not asked.
    assert_eq!(ctrl.placements_recorded.load(Ordering::SeqCst), 0, "no adoption, no observed stamp");
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
    let planned = crate::inference::kv_cache_plan::resolve();
    let other = if planned.cache_type == crate::inference::kv_cache_plan::Q8_0 { None } else { Some(crate::inference::kv_cache_plan::Q8_0) };
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

// what this catches (card 931cfd22; enrico 2026-10-25, 5 hours empty): a refused ELECTIVE
// replacement condemning a working engine. The incumbent serves the target and decodes; its
// retirement is refused before anything is signalled ("adopted checkpoint requires a known
// pool"): it keeps serving. A condemned incumbent (fails its decode proof) and an incumbent on
// another model stay degraded, so neither is ever advertised ready for the target.
#[tokio::test]
async fn a_refused_replacement_keeps_a_verified_incumbent_and_only_that() {
    let healthy = FakeControl::probe(Ok(Some("coder-14b".into())))
        .launched_partitioned()
        .retirement_refused();
    assert_eq!(ensure_model_serving(&healthy, &target("coder-14b"), false).await, EnsureOutcome::AlreadyServing);
    assert_eq!(healthy.serves.load(Ordering::SeqCst), 1, "the replacement was attempted, and refused");

    let condemned = FakeControl::probe(Ok(Some("coder-14b".into())))
        .decode_wedged()
        .retirement_refused();
    assert!(
        matches!(ensure_model_serving(&condemned, &target("coder-14b"), false).await, EnsureOutcome::Degraded { .. }),
        "a wedged incumbent is never kept as ready"
    );

    let other_model = FakeControl::probe(Ok(Some("other-model".into()))).retirement_refused();
    assert!(
        matches!(ensure_model_serving(&other_model, &target("coder-14b"), false).await, EnsureOutcome::Degraded { .. }),
        "another model is never reported as the target"
    );

    // Codex on #5413: a replacement wanted for the GENOME. Keeping the incumbent would publish
    // the target's genes as loaded; a different or unreadable set stays degraded.
    let other_genes = FakeControl::probe(Ok(Some("coder-14b".into())))
        .with_active_adapters(vec!["/genes/old.gguf".into()])
        .retirement_refused();
    assert!(
        matches!(ensure_model_serving(&other_genes, &target("coder-14b"), false).await, EnsureOutcome::Degraded { .. }),
        "the target's genes are never reported as loaded on an incumbent that holds others"
    );
    let unknown_genes = FakeControl::probe(Ok(Some("coder-14b".into())))
        .adapters_unreadable()
        .retirement_refused();
    assert!(
        matches!(ensure_model_serving(&unknown_genes, &target("coder-14b"), false).await, EnsureOutcome::Degraded { .. }),
        "an unknown gene set is never assumed to be the target's"
    );
}

