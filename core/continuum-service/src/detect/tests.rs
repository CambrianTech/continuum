use super::*;
use crate::sample::{Builds, Demand, EngineIncarnation, Prediction, ServingConfig, Window, Workload};

const MIN: u64 = 60_000;

fn policy() -> ServicePolicy {
    ServicePolicy { history: 8, min_evidence: 3, sustained: 2 }
}

fn config(lanes: u32, ctx: u32) -> ServingConfig {
    ServingConfig {
        lanes,
        ctx_per_lane: ctx,
        cache_ram_mib: 4365,
        kv_type: "f16".into(),
        backend: "metal".into(),
        expert_residency: String::new(),
    }
}

/// One 10-minute window on the M5 serving the 35B MoE.
fn window(at_min: u64, config: &ServingConfig, pid: u32, p50_ms: u64, predicted_ms: u64) -> ServiceSample {
    ServiceSample {
        schema: SCHEMA,
        node: "m5".into(),
        model: "qwen3.6-35b-a3b".into(),
        builds: Builds { core: "8e38ada8d".into(), engine: "b0".into() },
        config: config.clone(),
        incarnation: EngineIncarnation { pid, started_ms: u64::from(pid) },
        mixed: false,
        window: Window { start_ms: at_min * MIN, end_ms: (at_min + 10) * MIN },
        workload: Workload { residents_active: 3, prompt_tokens_p50: 24_000, output_tokens_p50: 300, cached_ratio_p50: 0.7 },
        demand: Demand { offered: 12, queued_ms_p50: 2_000 },
        generations: 12,
        acts: 4,
        failures: 0,
        latency_p50_ms: p50_ms,
        latency_p90_ms: p50_ms * 2,
        prediction: Some(Prediction { p50_ms: predicted_ms, calibration: "dyno-v1".into() }),
        warmup: Warmup::Warm,
        memory_ceiling_bytes: 40_300_000_000,
    }
}

// what this catches: the M5 on 2026-10-26. A configuration first seen DEGRADED (the 2-lane plan
// went live in a relaunch after a Compute-error burst) has no history of its own, so a
// per-configuration baseline alone would learn 174 s as normal and never fire. The node's
// healthy prediction ratios catch it, the slow windows never join any baseline, and the same
// configuration after a clean restart reads healthy.
#[test]
fn a_configuration_first_seen_degraded_is_caught_by_the_nodes_prediction_ratios() {
    let (mut state, policy) = (ServiceState::default(), policy());
    let five = config(5, 50_432);
    for (i, p50) in [62_000, 66_000, 64_000, 63_000].into_iter().enumerate() {
        let v = observe(&mut state, &policy, &window(i as u64 * 10, &five, 100, p50, 60_000));
        assert!(matches!(v, Verdict::Healthy | Verdict::Insufficient(Insufficient::NoReference)), "{v:?}");
    }

    let two = config(2, 58_880);
    let mut degraded = window(200, &two, 200, 174_000, 80_000);
    degraded.failures = 0;
    assert_eq!(observe(&mut state, &policy, &degraded), Verdict::Suspect { streak: 1, sustained: 2 }, "one slow window is a wobble");
    let mut next = window(210, &two, 200, 180_000, 80_000);
    next.failures = 3;
    let Verdict::Regressed(r) = observe(&mut state, &policy, &next) else { panic!("sustained slowdown on a new configuration must regress") };
    assert!(matches!(r.against, Reference::PredictionRatio { .. }), "judged by the ratios: {r:?}");
    assert!(r.causes.contains(&CauseHint::Failures(3)), "{r:?}");
    assert!(r.causes.iter().any(|c| matches!(c, CauseHint::SlowerThanItsRates { .. })), "{r:?}");
    assert!(state.baselines.values().all(|b| b.references.iter().all(|x| x.latency_p50_ms < 100_000)), "no slow window became a reference");

    // The clean restart: same configuration, new incarnation, at its measured speed.
    assert_eq!(observe(&mut state, &policy, &window(400, &two, 300, 86_000, 80_000)), Verdict::Healthy);
}

// what this catches: an engine that comes back slower than its OWN configuration's healthy
// windows, judged against that history, with the restart named as the candidate cause.
#[test]
fn the_same_configuration_slower_after_a_restart_regresses_and_names_the_restart() {
    let (mut state, policy) = (ServiceState::default(), policy());
    let two = config(2, 58_880);
    for (i, p50) in [86_000, 88_000, 90_000].into_iter().enumerate() {
        observe(&mut state, &policy, &window(i as u64 * 10, &two, 300, p50, 80_000));
    }
    assert!(matches!(observe(&mut state, &policy, &window(100, &two, 400, 170_000, 80_000)), Verdict::Suspect { .. }));
    let Verdict::Regressed(r) = observe(&mut state, &policy, &window(110, &two, 400, 175_000, 80_000)) else { panic!("must regress") };
    assert_eq!(r.against, Reference::OwnHistory { fastest_ms: 86_000, slowest_ms: 90_000 });
    assert!(r.causes.contains(&CauseHint::EngineRestartedSinceBaseline), "{r:?}");
}

// what this catches: windows that cannot describe the engine's speed being judged anyway, or
// learned. Warm-up, unknown warm-up, a window spanning two incarnations and an idle window
// are each Insufficient, never Healthy, and none of them becomes a reference.
#[test]
fn warmup_unknown_mixed_and_idle_windows_are_never_judged_or_learned() {
    let (mut state, policy) = (ServiceState::default(), policy());
    let two = config(2, 58_880);
    let mut warming = window(0, &two, 1, 300_000, 80_000);
    warming.warmup = Warmup::WarmingUp;
    let mut unknown = window(10, &two, 1, 80_000, 80_000);
    unknown.warmup = Warmup::Unknown;
    let mut mixed = window(20, &two, 1, 80_000, 80_000);
    mixed.mixed = true;
    let mut idle = window(30, &two, 1, 0, 80_000);
    idle.demand.offered = 0;
    idle.generations = 0;
    assert_eq!(observe(&mut state, &policy, &warming), Verdict::Insufficient(Insufficient::WarmingUp));
    assert_eq!(observe(&mut state, &policy, &unknown), Verdict::Insufficient(Insufficient::UnknownWarmup));
    assert_eq!(observe(&mut state, &policy, &mixed), Verdict::Insufficient(Insufficient::Mixed));
    assert_eq!(observe(&mut state, &policy, &idle), Verdict::Insufficient(Insufficient::NoDemand));
    assert!(state.baselines.is_empty() && state.ratios.is_empty(), "nothing learned: {state:?}");
}

// what this catches: an upgrade read as a sick engine. New builds are a new baseline and new
// ratios, so a slower window right after an upgrade is not enough evidence, never a regression.
#[test]
fn new_builds_start_their_own_baseline_instead_of_regressing() {
    let (mut state, policy) = (ServiceState::default(), policy());
    let two = config(2, 58_880);
    for i in 0..3 {
        observe(&mut state, &policy, &window(i * 10, &two, 1, 86_000, 80_000));
    }
    let mut upgraded = window(100, &two, 2, 140_000, 80_000);
    upgraded.builds.core = "f00dfeed".into();
    assert_eq!(observe(&mut state, &policy, &upgraded), Verdict::Insufficient(Insufficient::NoReference));
}

// what this catches: a heavier hour (twice the fresh tokens per act) read as a slower engine. It
// lands in its own stratum, where the node's prediction ratio, which normalizes the workload,
// judges it healthy.
#[test]
fn a_heavier_workload_is_its_own_stratum_and_judged_by_prediction() {
    let (mut state, policy) = (ServiceState::default(), policy());
    let two = config(2, 58_880);
    for i in 0..3 {
        observe(&mut state, &policy, &window(i * 10, &two, 1, 86_000, 80_000));
    }
    let mut heavy = window(100, &two, 1, 160_000, 150_000);
    heavy.workload.prompt_tokens_p50 = 48_000;
    assert_ne!(BaselineKey::of(&heavy), BaselineKey::of(&window(0, &two, 1, 86_000, 80_000)));
    assert_eq!(observe(&mut state, &policy, &heavy), Verdict::Healthy);
}

// what this catches: a configuration's identity drifting (two runs disagreeing on its
// fingerprint, so restarts never match), or two different configurations colliding.
#[test]
fn a_configurations_fingerprint_is_stable_and_tells_settings_apart() {
    assert_eq!(config(2, 58_880).fingerprint(), config(2, 58_880).fingerprint());
    assert_ne!(config(2, 58_880).fingerprint(), config(5, 50_432).fingerprint());
    let mut q8 = config(2, 58_880);
    q8.kv_type = "q8_0".into();
    assert_ne!(q8.fingerprint(), config(2, 58_880).fingerprint());
}

// what this catches (Codex on #5477): the same window fed twice. A repeated slow window must
// not grow a streak into a "sustained" regression, and a repeated healthy one must not pad the
// evidence; anything that does not start after the node's last window is refused untouched.
#[test]
fn a_duplicate_or_out_of_order_window_is_refused_before_it_is_judged_or_learned() {
    let (mut state, policy) = (ServiceState::default(), policy());
    let two = config(2, 58_880);
    for i in 0..3 {
        observe(&mut state, &policy, &window(i * 10, &two, 1, 86_000, 80_000));
    }
    let slow = window(30, &two, 1, 170_000, 80_000);
    assert!(matches!(observe(&mut state, &policy, &slow), Verdict::Suspect { streak: 1, .. }));
    assert_eq!(
        observe(&mut state, &policy, &slow),
        Verdict::Insufficient(Insufficient::NotAfterLast { start_ms: 30 * MIN, last_end_ms: 40 * MIN }),
        "the same window again is refused, not a second slow window"
    );
    assert!(matches!(observe(&mut state, &policy, &window(5, &two, 1, 86_000, 80_000)), Verdict::Insufficient(Insufficient::NotAfterLast { .. })));
    assert_eq!(state.baselines.values().map(|b| b.references.len()).sum::<usize>(), 3, "nothing refused was learned");
}

// what this catches (Codex on #5477): a streak counted across windows that are not consecutive
// time on one engine. Slow, then a warm-up window, then slow is two separate slow windows; so is
// slow, a gap, slow; and a restart between them starts the run again.
#[test]
fn a_streak_needs_adjacent_judged_windows_on_one_engine() {
    let two = config(2, 58_880);
    let warmed = |state: &mut ServiceState| {
        for i in 0..3 {
            observe(state, &policy(), &window(i * 10, &two, 1, 86_000, 80_000));
        }
    };
    let (mut state, policy) = (ServiceState::default(), policy());
    warmed(&mut state);
    assert!(matches!(observe(&mut state, &policy, &window(30, &two, 1, 170_000, 80_000)), Verdict::Suspect { streak: 1, .. }));
    let mut warming = window(40, &two, 1, 170_000, 80_000);
    warming.warmup = Warmup::WarmingUp;
    observe(&mut state, &policy, &warming);
    assert!(matches!(observe(&mut state, &policy, &window(50, &two, 1, 170_000, 80_000)), Verdict::Suspect { streak: 1, .. }), "an ineligible window ends the run");

    let mut state = ServiceState::default();
    warmed(&mut state);
    observe(&mut state, &policy, &window(30, &two, 1, 170_000, 80_000));
    assert!(matches!(observe(&mut state, &policy, &window(60, &two, 1, 170_000, 80_000)), Verdict::Suspect { streak: 1, .. }), "a gap ends the run");

    let mut state = ServiceState::default();
    warmed(&mut state);
    observe(&mut state, &policy, &window(30, &two, 1, 170_000, 80_000));
    assert!(matches!(observe(&mut state, &policy, &window(40, &two, 2, 170_000, 80_000)), Verdict::Suspect { streak: 1, .. }), "a restart ends the run");
}

// what this catches (Codex on #5477): an engine that completes nothing while work waits. That
// is the worst slowdown, and it must never read as an idle (NoDemand) or healthy window.
#[test]
fn demand_with_no_completions_is_a_stall_never_idle() {
    let (mut state, policy) = (ServiceState::default(), policy());
    let mut stalled = window(0, &config(2, 58_880), 1, 0, 80_000);
    stalled.generations = 0;
    stalled.failures = 12;
    assert_eq!(observe(&mut state, &policy, &stalled), Verdict::Stalled { offered: 12, failures: 12 });
    assert!(state.baselines.is_empty(), "a stall is never learned");

    // what this catches: Kimi's shape, refused before inference, so her window has no reuse to
    // call warm. Warm-up and a mixed window qualify a speed; they never excuse zero completions.
    let mut unwarmed = window(10, &config(2, 58_880), 1, 0, 80_000);
    unwarmed.generations = 0;
    unwarmed.failures = 12;
    unwarmed.warmup = Warmup::Unknown;
    unwarmed.mixed = true;
    assert_eq!(observe(&mut state, &policy, &unwarmed), Verdict::Stalled { offered: 12, failures: 12 });
}

// what this catches (Codex on #5477): a recalibrated predictor read as a change in the engine. A
// new calibration is a new ratio baseline, so a configuration without history waits for evidence
// under it instead of being judged by ratios the old predictor produced.
#[test]
fn a_recalibrated_prediction_starts_new_ratios_instead_of_judging_against_the_old_ones() {
    let (mut state, policy) = (ServiceState::default(), policy());
    let five = config(5, 50_432);
    for i in 0..3 {
        observe(&mut state, &policy, &window(i * 10, &five, 1, 62_000, 60_000));
    }
    let mut recalibrated = window(30, &config(2, 58_880), 1, 170_000, 80_000);
    if let Some(p) = recalibrated.prediction.as_mut() {
        p.calibration = "dyno-v2".into();
    }
    assert_eq!(observe(&mut state, &policy, &recalibrated), Verdict::Insufficient(Insufficient::NoReference));
}

// what this catches (Codex on #5477): windows with no duration. A zero-length window at the
// last end would pass every time, training the baseline and growing a streak; a reversed one
// would rewind the stream. Both are refused before anything changes.
#[test]
fn a_zero_length_or_reversed_window_is_refused_before_any_state_changes() {
    let (mut state, policy) = (ServiceState::default(), policy());
    let two = config(2, 58_880);
    observe(&mut state, &policy, &window(0, &two, 1, 86_000, 80_000));
    let before = state.clone();
    let mut empty = window(10, &two, 1, 86_000, 80_000);
    empty.window.end_ms = empty.window.start_ms;
    let mut reversed = window(10, &two, 1, 86_000, 80_000);
    reversed.window.end_ms = reversed.window.start_ms - MIN;
    assert!(matches!(observe(&mut state, &policy, &empty), Verdict::Insufficient(Insufficient::EmptyWindow { .. })));
    assert!(matches!(observe(&mut state, &policy, &reversed), Verdict::Insufficient(Insufficient::EmptyWindow { .. })));
    assert_eq!(state, before, "nothing learned, no streak, no rewind");
}

// what this catches (Codex on #5477): two different comparisons merged into one sustained run.
// A slow window under one configuration, then an adjacent slow one under another, are two runs
// of one, never a regression.
#[test]
fn slow_windows_under_different_configurations_are_separate_runs() {
    let (mut state, policy) = (ServiceState::default(), policy());
    let (two, three) = (config(2, 58_880), config(3, 39_253));
    for (i, c) in [&two, &two, &two, &three, &three, &three].into_iter().enumerate() {
        observe(&mut state, &policy, &window(i as u64 * 10, c, 1, 86_000, 80_000));
    }
    assert!(matches!(observe(&mut state, &policy, &window(60, &two, 1, 170_000, 80_000)), Verdict::Suspect { streak: 1, .. }));
    assert!(
        matches!(observe(&mut state, &policy, &window(70, &three, 1, 170_000, 80_000)), Verdict::Suspect { streak: 1, .. }),
        "a different configuration starts its own run"
    );
}
