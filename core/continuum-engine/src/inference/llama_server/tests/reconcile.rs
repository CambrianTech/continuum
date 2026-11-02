//! Reconcile regression fixtures.
#![cfg(test)]
use super::*;
// what this catches: the per-lane → total `-c` arithmetic. llama-server's
// `-c` is the WHOLE KV cache split across `--parallel` slots, so to give each
// of `lanes` slots a full per-lane `context_window` the spawn must request
// `context_window * lanes` and pass `--parallel lanes`. Launching `-c
// context_window` alone let llama.cpp default to 4 slots and quarter the
// per-request window, 500-ing live deliberations ("Context size exceeded").
// Regression here silently shrinks every persona's real prompt budget.
#[test]
fn served_total_ctx_is_per_lane_window_times_lanes() {
    let mut t = target("coder-4b");
    t.context_window = 16_384;
    t.lanes = 4;
    assert_eq!(t.parallel_lanes(), 4);
    assert_eq!(
        t.served_total_ctx(),
        65_536,
        "-c must be per-lane window × lanes so each slot holds one full window"
    );

    // lanes floored at 1: a single-lane plan serves the full window to one slot.
    t.lanes = 1;
    assert_eq!(t.served_total_ctx(), 16_384);

    // defensive floor: a zero lane count never collapses the cache to nothing.
    t.lanes = 0;
    assert_eq!(t.parallel_lanes(), 1);
    assert_eq!(t.served_total_ctx(), 16_384);
}

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

// what this catches: a same-model / same-genome lane whose live per-slot window
// is STARVED far below the plan target (the 2048 boot-floor vs a 31k plan) must
// RELAUNCH to grow — llama.cpp has no hot-resize. Regression here (returning
// AlreadyServing on a genome-set match alone) strands the lane at the boot floor
// forever while the daemon logs "re-homing starved" every tick and never grows
// it — glass-boxed 2026-07-20, blocked all benchmark runs (prompts overflowed
// 2048). The daemon's `starved` gate only sends a target ≥ 2× the served window,
// so the grow is unambiguous.
#[tokio::test]
async fn starved_window_relaunches_to_grow() {
    // Owned + decode-healthy: the ONLY thing wrong is the window (2048 ≪ 32768).
    let ctrl = FakeControl::probe(Ok(Some("coder-14b".into())))
        .owned()
        .with_served_window(2048);
    let outcome = ensure_model_serving(&ctrl, &target("coder-14b"), false).await;
    assert!(
        matches!(outcome, EnsureOutcome::Spawned { .. }),
        "a starved window must relaunch to grow, got {outcome:?}"
    );
    assert_eq!(
        ctrl.serves.load(Ordering::SeqCst),
        1,
        "exactly one relaunch to the larger window"
    );
}

// what this catches: THE 2026-09-03 SLOW-RESUME defect — a warm lane REAPED for a
// trivial window drift. On reboot the boot plan re-computes the window against live
// memory, which climbs ~7% once the old core frees its share (measured 147712 →
// 158107). A flat 512-token tolerance (0.35% of a 147k lane) tripped a full ~7-10
// min 35B RELAUNCH on EVERY restart — the dominant "resume is slow" cost. Tolerance
// is now a percentage (max(512, served/8) ≈ 12.5%), so a warm lane within ~12.5% of
// the target is ADOPTED (warm-now beats cold-in-10-min); a genuinely starved lane
// (the 2× case above) still relaunches, and the runtime re-home grows a persistent
// gain later.
#[tokio::test]
async fn a_small_window_drift_adopts_the_warm_lane_not_a_slow_reload() {
    // Owned + decode-healthy, serving 147712; the boot plan drifted to 158107 (~7%).
    let ctrl = FakeControl::probe(Ok(Some("coder-14b".into())))
        .owned()
        .with_served_window(147_712);
    let mut t = target("coder-14b");
    t.context_window = 158_107;
    let outcome = ensure_model_serving(&ctrl, &t, false).await;
    assert_eq!(
        outcome,
        EnsureOutcome::AlreadyServing,
        "a ~7% window drift must ADOPT the warm lane, not reload it; got {outcome:?}"
    );
    assert_eq!(
        ctrl.serves.load(Ordering::SeqCst),
        0,
        "zero relaunches — the warm 35B lane is reused, not reloaded"
    );
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

// what this catches: the fix above must not disable lane grow-back (#214's sibling).
// A lane genuinely serving FEWER slots than the plan wants still has to relaunch —
// llama.cpp cannot hot-resize n_seq_max. Without this, "adopt more eagerly" would
// silently freeze every citizen at the squeezed slot count forever.
#[tokio::test]
async fn a_lane_serving_fewer_slots_than_planned_still_relaunches_to_grow() {
    let ctrl = FakeControl::probe(Ok(Some("coder-14b".into())))
        .owned()
        .with_served_lanes(1)
        .with_served_window(32768); // window is FINE — only the lane count is short
    let mut t = target("coder-14b");
    t.lanes = 4;
    let outcome = ensure_model_serving(&ctrl, &t, false).await;
    assert!(
        matches!(outcome, EnsureOutcome::Spawned { .. }),
        "1 served slot against a 4-lane plan must relaunch to grow, got {outcome:?}"
    );
}

// what this catches (card d9d747b2, the M5 2026-09-21 09:29-09:36Z): a plan wanting
// MORE lanes at a NARROWER window than the lane serves (5 × 36,810 over 3 × 78,592)
// must not relaunch — following it thrashed the engine every ~3 min against the
// window grow-back and reaped 35 generations in 20 min. Both directions: the trade is
// refused and the lane stays; a pure lane grow at the served window still relaunches
// (the test above), and a WIDER window at fewer lanes still relaunches (the window is
// the requirement, lanes are throughput).
// what this catches: the grow ladder allocating through a CLOSED memory gate. The M5
// settled at 8 lanes × 35,328 during 39 consecutive Critical samples (2026-09-23), and
// fseventsd has twice held over a quarter of the machine. A grow on either axis is
// refused while the gate is closed; a decision-only relaunch (KV type, sight) is not,
// and nothing is refused while the gate is open.
#[test]
fn a_closed_memory_gate_refuses_a_lane_or_window_grow() {
    assert!(grow_refused_by_memory_gate(true, true, false, true), "more lanes through a closed gate");
    assert!(grow_refused_by_memory_gate(true, false, true, true), "a wider window through a closed gate");
    assert!(grow_refused_by_memory_gate(true, false, false, true), "both");
    assert!(grow_refused_by_memory_gate(true, true, true, false), "a bigger host cache through a closed gate (card d7294f0b)");
    assert!(!grow_refused_by_memory_gate(true, true, true, true), "geometry matches: a KV/sight relaunch is not a grow");
    assert!(!grow_refused_by_memory_gate(false, false, false, false), "an open gate refuses nothing");
}

#[tokio::test]
async fn a_lane_grow_that_would_narrow_the_window_is_refused_as_a_trade() {
    // The predicate, at the measured numbers and at the edges.
    assert!(lane_grow_would_narrow_the_window(5, 36_810, 3, 78_592), "the M5 loop's first step");
    assert!(lane_grow_would_narrow_the_window(4, 34_176, 3, 78_592), "and its second");
    assert!(!lane_grow_would_narrow_the_window(3, 78_592, 5, 36_810), "wider at fewer lanes is a grow-back");
    assert!(!lane_grow_would_narrow_the_window(5, 78_592, 3, 78_592), "a pure lane grow");
    assert!(!lane_grow_would_narrow_the_window(5, 70_000, 3, 78_592), "within the window tolerance (78,592 / 8)");
    assert!(!lane_grow_would_narrow_the_window(3, 31_000, 1, 2_048), "the 2k → 31k starved case grows both axes");
    assert!(!lane_grow_would_narrow_the_window(5, 36_810, 0, 0), "an unreadable served side is not a trade");
    // Fable's 45 s time series on the M5 (09:28-09:42Z): the DOWN steps are crash
    // respawns under a trough budget, not this ladder (grow-only), and from any of
    // them a wider plan must still climb out — lanes up or down.
    assert!(!lane_grow_would_narrow_the_window(4, 34_176, 5, 36_810), "fewer lanes is never a refused trade");
    assert!(!lane_grow_would_narrow_the_window(1, 19_173, 4, 34_176));
    assert!(!lane_grow_would_narrow_the_window(3, 78_592, 1, 19_173), "out of the bottom: wider at more lanes");
    assert!(!lane_grow_would_narrow_the_window(2, 31_141, 1, 19_173), "out of the bottom: wider at more lanes");

    // The ladder on the fake control: served 3 × 78,592, plan 5 × 36,810 → no relaunch.
    let ctrl = FakeControl::probe(Ok(Some("coder-14b".into())))
        .owned()
        .with_served_lanes(3)
        .with_served_window(78_592);
    let mut t = target("coder-14b");
    t.lanes = 5;
    t.context_window = 36_810;
    let outcome = ensure_model_serving(&ctrl, &t, false).await;
    assert_eq!(outcome, EnsureOutcome::AlreadyServing, "a trade is refused, the lane keeps its window");
    assert_eq!(ctrl.serves.load(Ordering::SeqCst), 0, "no relaunch toward the narrower geometry");

    // The other direction: served 5 × 36,810, plan 3 × 78,592 → relaunches (window grow-back).
    let ctrl = FakeControl::probe(Ok(Some("coder-14b".into())))
        .owned()
        .with_served_lanes(5)
        .with_served_window(36_810);
    let mut t = target("coder-14b");
    t.lanes = 3;
    t.context_window = 78_592;
    let outcome = ensure_model_serving(&ctrl, &t, false).await;
    assert!(matches!(outcome, EnsureOutcome::Spawned { .. }), "a wider window at fewer lanes still grows, got {outcome:?}");
}

// what this catches: the window-grow check must NOT fire on llama.cpp's 256-pad
// (served slightly BELOW target after internal rounding is normal) — only a real
// step-change grow relaunches. A served window one pad under target stays
// AlreadyServing; regression would relaunch every tick on padding noise.
#[tokio::test]
async fn padded_window_within_tolerance_does_not_relaunch() {
    let ctrl = FakeControl::probe(Ok(Some("coder-14b".into())))
        .owned()
        .with_served_window(32768 - 256); // one 256-pad under the 32768 target
    let outcome = ensure_model_serving(&ctrl, &target("coder-14b"), false).await;
    assert_eq!(outcome, EnsureOutcome::AlreadyServing);
    assert_eq!(
        ctrl.serves.load(Ordering::SeqCst),
        0,
        "padding noise must not relaunch"
    );
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

// what this catches (card d7294f0b, Kimi's node 2026-10-15 07:37-07:39Z): the daemon decided a
// cache re-home for an adopted lane whose --cache-ram held no measured state, and this rail
// answered AlreadyServing because it never compared the grant. With a measured state between
// the running grant and the target's, the lane relaunches; an unknown grant, or a target that
// could not hold the state either, still adopts.
#[tokio::test]
async fn an_adopted_lane_whose_cache_holds_no_state_relaunches_onto_one_that_does() {
    const MIB: u64 = 1024 * 1024;
    let id = "cache-grant-d7294f0b"; // its own page dir: the state samples are process-wide
    let t = target(id);
    crate::inference::slots::note_state_bytes(&kv_page_dir(id), 3_800 * MIB);
    let starved = FakeControl::probe(Ok(Some(id.into()))).owned().launched_with_cache(3_248);
    assert_eq!(
        ensure_model_serving(&starved, &t, false).await,
        EnsureOutcome::Spawned { model: id.into() },
        "a 3,248 MiB grant holds no 3,800 MiB state and the target's 4,096 does"
    );
    assert_eq!(starved.serves.load(Ordering::SeqCst), 1);
    let unknown = FakeControl::probe(Ok(Some(id.into()))).owned();
    assert_eq!(ensure_model_serving(&unknown, &t, false).await, EnsureOutcome::AlreadyServing, "no argv: an unknown grant is not a mismatch");
    let futile = ServingTarget { host_prompt_cache_mib: 3_500, ..t.clone() };
    let starved = FakeControl::probe(Ok(Some(id.into()))).owned().launched_with_cache(3_248);
    assert_eq!(
        ensure_model_serving(&starved, &futile, false).await,
        EnsureOutcome::AlreadyServing,
        "a relaunch whose grant holds no state either is not owed"
    );
    assert_eq!(starved.serves.load(Ordering::SeqCst), 0);
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
        .with_served_window(2048)
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

// what this catches (Codex on #5535): a relaunch on a node with no slot holding its pin either
// launching the stale `current` binary or condemning the incumbent for a refusal raised before
// any transition. The incumbent that serves the target and decodes keeps serving; with nothing
// serving, the lane stays degraded with the named refusal, and no rollback retries a launch.
#[tokio::test]
async fn an_unpinned_node_refuses_a_new_launch_and_keeps_its_incumbent() {
    let incumbent = FakeControl::probe(Ok(Some("coder-14b".into())))
        .with_served_window(2048)
        .engine_unpinned();
    assert_eq!(ensure_model_serving(&incumbent, &target("coder-14b"), false).await, EnsureOutcome::AlreadyServing);
    assert_eq!(incumbent.serves.load(Ordering::SeqCst), 1, "the relaunch was asked once and refused");

    let nothing_serves = FakeControl::probe(Ok(None)).engine_unpinned();
    match ensure_model_serving(&nothing_serves, &target("coder-14b"), false).await {
        EnsureOutcome::Degraded { reason } => assert!(reason.contains("no engine slot holds pin"), "reason: {reason}"),
        other => panic!("expected Degraded, got {other:?}"),
    }
    assert_eq!(nothing_serves.serves.load(Ordering::SeqCst), 1, "no rollback relaunch onto another engine");
}
