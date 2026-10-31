#![cfg(test)]

use super::*;

// what this catches (Cormac on #4298): a receipt that fires on PERSISTENCE rather than
// CHANGE. `serving.prompt_cache.divergence` sits on the 5-second ready path, and the M5
// gap lasted three hours — firing per tick is ~2,160 identical lines carrying the same
// two numbers, a rising count that is not rising evidence and drowns the very ledger the
// card asks a reader to open. Both EDGES owe a receipt; the middle owes silence.
#[test]
fn a_divergence_speaks_when_it_appears_and_when_it_closes_never_while_it_persists() {
    use super::say_on_change;
    let mut cell: Option<(&str, u32, u32)> = None;

    // Quiet stays quiet — a node whose engine already holds what the decision wants
    // must never emit a line just for being healthy.
    assert_eq!(say_on_change(&mut cell, None), None);

    // APPEARS: speak, and the previous state is "nothing had been said".
    let m5 = ("qwen27b", 480u32, 2_517u32);
    assert_eq!(say_on_change(&mut cell, Some(m5)), Some(None));

    // PERSISTS: three hours of identical ticks, one receipt total.
    for _ in 0..2_160 {
        assert_eq!(say_on_change(&mut cell, Some(m5)), None);
    }

    // MOVES: the target drifted (3889 -> 2916 -> 2517 happens within minutes), so the
    // fact changed and owes a new receipt carrying the old numbers.
    let moved = ("qwen27b", 480u32, 3_889u32);
    assert_eq!(say_on_change(&mut cell, Some(moved)), Some(Some(m5)));

    // A MODEL SWITCH that happens to produce the same pair is a different fact — this is
    // why the cell is keyed by model id and not by the numbers alone.
    let other = ("ornith35b", 480u32, 3_889u32);
    assert_eq!(say_on_change(&mut cell, Some(other)), Some(Some(moved)));

    // CLOSES: one receipt at the far edge, carrying what it was, then silence again.
    assert_eq!(say_on_change(&mut cell, None), Some(Some(other)));
    assert_eq!(say_on_change(&mut cell, None), None);
}

// what this catches: the cache is sized to the ACTIVE roster's states at the served
// window, never to every seed on disk at its unclamped peak. Regression for the M5
// 2026-09-20 (card 7b01ed65): 23 seeded minds × 225k–505k peaks made the want
// astronomical, so the grant was always "everything affordable" (26,985 MiB on a
// fresh 2 × 67k engine), the cache filled the box into swap, the plan shed a lane and
// every deploy went dark. Two lanes → four states of at most the window; the largest
// are kept; a mind that never sent falls back to its peak; an unknown bound keeps all.
#[test]
fn the_cache_holds_the_active_rosters_states_not_every_seed_on_disk() {
    // 23 minds: sent ~30k, unclamped peaks 225k–505k, one that never sent.
    let mut population: Vec<(u32, u32)> = (0..22)
        .map(|i| (28_000 + i * 500, 225_000 + i * 12_000))
        .collect();
    population.push((0, 505_000));
    let active = crate::persona::spawner_module::bounded_by_warm_slots(population.len(), Some(2));
    assert_eq!(active, 4, "two lanes seat four active minds");
    let states = active_cache_states(&population, 67_072, active);
    assert_eq!(
        states.len(),
        4,
        "one state per active mind, not one per seed"
    );
    assert!(
        states.iter().all(|t| *t <= 67_072),
        "a state never exceeds the served window: {states:?}"
    );
    assert_eq!(
        states[0], 67_072,
        "the never-sent mind's peak, clamped to the window, is the largest kept"
    );
    assert_eq!(states[1], 38_500, "then the largest sent prompts");
    let want: u64 = states.iter().map(|t| 32_888u64 * *t as u64).sum();
    assert!(
        want < 8 * 1024 * 1024 * 1024,
        "four states at 32.9 KB/token stay under 8 GiB: {want}"
    );
    // No bound known (a derivation before any plan) keeps the FLOOR, never everything:
    // unknown is not "all 23 at their peaks" — that is the want this law deletes.
    assert_eq!(
        active_cache_states(&population, 67_072, 0).len(),
        crate::cognition::health_ledger::MINDLESS_RESIDENT_FLOOR as usize
    );
    // Through the decision: the same population on a 2-lane serve asks for a
    // demand-sized cache, not the whole afford.
    let fp = crate::inference::planning::serving_plan::ModelFootprint {
        model_id: "cache-fixture".into(),
        weights_bytes: 19 * 1024 * 1024 * 1024,
        kv_per_token: 32_888,
        context_window: 262_144,
        capability_rank: 1,
        fixed_per_lane_bytes: 0,
    };
    let physical = 64u64 << 30;
    let decision = prompt_cache_decision(
        Some(&fp),
        &states,
        23,
        67_072,
        2,
        physical,
        fp.peak_resident_bytes(67_072, 2),
        40u64 << 30,
    );
    assert_eq!(decision.reason, "resident_demand_kv_estimate");
    let afford_mib = decision.affordable_bytes.expect("afford") / (1024 * 1024);
    assert!(
        (decision.desired_mib as u64) < afford_mib && decision.desired_mib < 9_000,
        "demand-sized ({} MiB), not the afford ({afford_mib} MiB)",
        decision.desired_mib
    );
}

// what this catches (the 5090, 2026-10-11: --cache-ram 4288 against hybrid-27B states of
// 4.3-5.8 GB; 104 states skipped and 220 evicted over the lane's life, two minds on one
// slot found nothing): a host prompt cache sized from the KV estimate alone, when a
// saved state also carries the recurrent state and the context checkpoints. Each mind's
// demand is raised to the measured state, so the cache holds every rotating mind's
// state; with nothing measured the KV estimate stands unchanged.
#[test]
fn the_prompt_cache_holds_each_rotating_minds_measured_state() {
    let kv = 32_768u64;
    let mib = 1024 * 1024;
    let measured = Some(5_511 * mib);
    let demands = vec![41_318u32, 21_525];
    assert_eq!(
        with_measured_state(demands.clone(), Some(kv), None),
        demands,
        "nothing measured: the estimate stands"
    );
    assert_eq!(
        with_measured_state(demands.clone(), None, measured),
        demands,
        "no KV rate: nothing to convert"
    );
    let raised = with_measured_state(demands.clone(), Some(kv), measured);
    let plenty = 64 * 1024 * mib;
    let sized = u64::from(crate::inference::lane_args::host_prompt_cache_mib(
        &raised, kv, plenty,
    )) * mib;
    assert!(
        sized >= 2 * 5_511 * mib,
        "both minds' measured states fit: {} MiB",
        sized / mib
    );
    let before = u64::from(crate::inference::lane_args::host_prompt_cache_mib(
        &demands, kv, plenty,
    )) * mib;
    assert!(
        before < 5_511 * mib,
        "the KV estimate alone held less than ONE measured state: {} MiB",
        before / mib
    );
    let afford = 8 * 1024 * mib;
    assert!(
        u64::from(crate::inference::lane_args::host_prompt_cache_mib(
            &raised, kv, afford
        )) * mib
            <= afford,
        "still capped by what the host can afford"
    );
}

// what this catches (card 58ba9752): the cache sized for lanes x 2 while more minds
// rotate. The IntelMac, 2026-10-10: 7 minds turning through 2 lanes against 4 states,
// so every turn re-prefilled cold. Minds seen within the rotation window count; the
// M5's dormant seeds (never seen, or seen hours ago) do not, so 7b01ed65 stays shut.
#[test]
fn the_cache_counts_the_minds_that_rotate_not_the_seeds_on_disk() {
    let hour = 60 * 60 * 1000u64;
    let minute = 60_000u64;
    let now = 1000 * hour;
    // The IntelMac (card 56d3c76f): residents last seen 6, 41, 77, 225, 246, 250 and
    // 366 minutes ago, each returning every 4-7 hours, plus one never seen.
    let intelmac: Vec<(u64, u64)> = [6u64, 41, 77, 225, 246, 250, 366]
        .iter()
        .zip([4u64, 5, 6, 4, 5, 6, 7])
        .map(|(ago, every)| (now - ago * minute, now - ago * minute - every * hour))
        .chain([(0, 0)])
        .collect();
    let window = rotation_window_ms(&intelmac).expect("seven measured returns");
    assert_eq!(window, 7 * hour, "the p90 return interval");
    assert_eq!(
        rotating_minds(&intelmac, now, Some(window)),
        7,
        "the whole rotation is held; the never-seen mind is not"
    );
    assert_eq!(
        rotating_minds(&intelmac, now, Some(hour)),
        2,
        "a declared hour counted two and lost to the floor of four: the no-op"
    );
    // The M5 shape: 23 seeds, three returning every 19 minutes, twenty dormant for days.
    let m5: Vec<(u64, u64)> = (0..23)
        .map(|i| {
            if i < 3 {
                (now - minute, now - 20 * minute)
            } else {
                (now - 72 * hour, 0)
            }
        })
        .collect();
    let w5 = rotation_window_ms(&m5);
    assert_eq!(w5, Some(19 * minute));
    assert_eq!(
        rotating_minds(&m5, now, w5),
        3,
        "the dormant seeds stay out: 7b01ed65 stays shut"
    );
    let floor = crate::persona::spawner_module::bounded_by_warm_slots(23, Some(2));
    assert_eq!(
        rotating_minds(&m5, now, w5).max(floor),
        floor,
        "the warm-slot floor still holds"
    );
    // Nothing measured yet: nothing counts, and the floor alone sizes the cache.
    assert_eq!(rotation_window_ms(&[(now, 0)]), None);
    assert_eq!(rotating_minds(&[(now, 0)], now, None), 0);
}

// what this catches: c8a8829b / the 4096 MiB incident hid all prior branches
// and treated a desired KV estimate as observed serialized cache capacity.
#[test]
fn prompt_cache_diagnostics_preserve_priors_and_distinguish_measurements() {
    use super::*;
    let fp = crate::inference::planning::serving_plan::ModelFootprint {
        model_id: "cache-fixture".into(),
        weights_bytes: 1024 * 1024 * 1024,
        kv_per_token: 65536,
        context_window: 65536,
        capability_rank: 1,
        fixed_per_lane_bytes: 0,
    };
    let missing = prompt_cache_decision(None, &[60000], 1, 65536, 1, 32 << 30, 0, 0);
    assert_eq!(missing.reason, "prior_missing_footprint");
    assert_eq!(missing.estimated_kv_bytes, None);
    assert_eq!(missing.affordable_bytes, None);
    // Empty demand is TWO states now, and they must not share a name. A cold node
    // (no seeds) and a node whose residents will resume but have never recorded
    // demand are different facts; the old single reason read benign for both — and
    // for a third state, "residents not yet up at launch", that the caller no longer
    // produces (it sizes from the resume set). Regression for the 5-minute turn.
    let cold = prompt_cache_decision(
        Some(&fp),
        &[],
        0,
        65536,
        1,
        32 << 30,
        fp.peak_resident_bytes(65536, 1),
        0,
    );
    assert_eq!(cold.reason, "prior_no_seeds_on_disk");
    let empty = prompt_cache_decision(
        Some(&fp),
        &[],
        2,
        65536,
        1,
        32 << 30,
        fp.peak_resident_bytes(65536, 1),
        0,
    );
    assert_eq!(empty.reason, "prior_seeds_without_recorded_demand");
    assert_eq!(
        empty.citizens, 0,
        "planned is not demand — citizens counts demand rows"
    );
    let unknown = prompt_cache_decision(
        Some(&fp),
        &[60000],
        1,
        65536,
        1,
        0,
        fp.peak_resident_bytes(65536, 1),
        0,
    );
    assert_eq!(unknown.reason, "prior_unknown_physical_memory");
    assert_eq!(
        missing.desired_mib,
        crate::inference::lane_args::CACHE_RAM_MIB
    );
    for prior in [&missing, &cold, &empty, &unknown] {
        assert_eq!(prior.affordable_bytes, None);
    }
    // what this catches: a prior that knows the geometry must hold ONE FULL STATE.
    // The fixture above sits on the crossover (65536 × 65536 B = exactly 4096 MiB),
    // where the constant and the floor coincide and an `assert_eq!(…, CACHE_RAM_MIB)`
    // could not tell them apart — the coincidence that let a first-ever boot ship an
    // undersized cache (Cormac, #4128). So drive the priors across geometries with
    // one full state BELOW the constant and ABOVE it, and assert the invariant.
    let mib = 1024u64 * 1024;
    for (kv_per_token, served_ctx) in [(16_384u64, 65_536u32), (27_360, 172_436), (65_536, 262_144)]
    {
        let geo = crate::inference::planning::serving_plan::ModelFootprint {
            kv_per_token,
            context_window: served_ctx,
            ..fp.clone()
        };
        let one_state_bytes = kv_per_token * served_ctx as u64;
        let one_state_mib = one_state_bytes.div_ceil(mib) as u32;
        let constant = crate::inference::lane_args::CACHE_RAM_MIB;
        for (name, prior) in [
            (
                "cold",
                prompt_cache_decision(
                    Some(&geo),
                    &[],
                    0,
                    served_ctx,
                    1,
                    32 << 30,
                    geo.peak_resident_bytes(served_ctx, 1),
                    0,
                ),
            ),
            (
                "seeds",
                prompt_cache_decision(
                    Some(&geo),
                    &[],
                    2,
                    served_ctx,
                    1,
                    32 << 30,
                    geo.peak_resident_bytes(served_ctx, 1),
                    0,
                ),
            ),
            (
                "unknown",
                prompt_cache_decision(
                    Some(&geo),
                    &[60_000],
                    1,
                    served_ctx,
                    1,
                    0,
                    geo.peak_resident_bytes(served_ctx, 1),
                    0,
                ),
            ),
        ] {
            // The engine's check is in BYTES (`state_size > limit_size`), so the
            // invariant is asserted in bytes: a limit floored to the MiB below a
            // full state still refuses that state.
            assert!(
                    (prior.desired_mib as u64) * mib >= one_state_bytes,
                    "{name} prior at kv {kv_per_token} × ctx {served_ctx}: {} MiB < one full state {} B",
                    prior.desired_mib,
                    one_state_bytes
                );
            assert_eq!(
                prior.desired_mib,
                constant.max(one_state_mib),
                "{name}: the floor lifts the prior, never lowers it"
            );
        }
        // The one prior that cannot know the geometry keeps the constant.
        assert_eq!(
            prompt_cache_decision(None, &[], 2, served_ctx, 1, 32 << 30, 0, 0).desired_mib,
            constant
        );
    }
    let measured = prompt_cache_decision(
        Some(&fp),
        &[32768, 32768],
        2,
        65536,
        1,
        32 << 30,
        fp.peak_resident_bytes(65536, 1),
        0,
    );
    // Same 4096 MiB result as the prior, but materially different evidence.
    assert_eq!(measured.desired_mib, missing.desired_mib);
    assert_ne!(measured, missing);
    assert_eq!(measured.estimated_kv_bytes, Some(4 << 30));
    assert_eq!(measured.reason, "resident_demand_kv_estimate");
    let above_floor =
        (32u64 << 30) - fp.peak_resident_bytes(65536, 1) - host_os_floor_bytes(32 << 30);
    // The grant leaves the planner's co-consumer headroom — a cache fills to its
    // grant, so a grant of everything above the floor ends at the floor (the M5,
    // 2026-09-19: 6 GB of swap under a 19.5 GB grant).
    let afford =
        (above_floor as f64 * (1.0 - crate::inference::planning::serving_plan::CACHE_GRANT_HEADROOM)) as u64;
    assert!(
        afford < above_floor,
        "the cache never gets everything above the floor"
    );
    assert_eq!(measured.affordable_bytes, Some(afford));
    assert_eq!(
        measured.desired_mib,
        crate::inference::lane_args::host_prompt_cache_mib(
            &[32768, 32768],
            fp.kv_per_token,
            afford
        )
    );
    let squeezed = prompt_cache_decision(
        Some(&fp),
        &[60000],
        1,
        65536,
        1,
        1,
        fp.peak_resident_bytes(65536, 1),
        0,
    );
    assert_eq!(squeezed.affordable_bytes, Some(0));
    assert_eq!(
        squeezed.desired_mib,
        crate::inference::lane_args::host_prompt_cache_mib(&[60000], fp.kv_per_token, 0)
    );
}

#[test]
fn a_discrete_gpu_does_not_charge_vram_residency_against_host_ram() {
    use super::*;
    use crate::gpu::monitor::MemoryMode;
    let fp = crate::inference::planning::serving_plan::ModelFootprint {
        model_id: "27b-on-a-5090".into(),
        weights_bytes: 17 << 30,
        kv_per_token: 262_144, // ~256 KiB/token: a 144k window is ~36 GiB of KV
        context_window: 262_144,
        capability_rank: 1,
        fixed_per_lane_bytes: 0,
    };
    let physical = 63u64 << 30;
    let (ctx, lanes) = (144_530u32, 1u32);
    let demands = [43_000u32, 12_000];
    let unified = serve_host_bytes(Some(&fp), ctx, lanes, Some(MemoryMode::Unified));
    let discrete = serve_host_bytes(Some(&fp), ctx, lanes, Some(MemoryMode::Discrete));
    let cpu_only = serve_host_bytes(Some(&fp), ctx, lanes, None);
    assert_eq!(unified, fp.peak_resident_bytes(ctx, lanes));
    assert_eq!(cpu_only, unified, "no GPU = everything host-resident");
    assert_eq!(discrete, 0, "VRAM residency costs the host nothing");
    assert!(
        unified + host_os_floor_bytes(physical) > physical,
        "the fixture reproduces the squeeze"
    );

    let on_unified =
        prompt_cache_decision(Some(&fp), &demands, 2, ctx, lanes, physical, unified, 0);
    let on_discrete =
        prompt_cache_decision(Some(&fp), &demands, 2, ctx, lanes, physical, discrete, 0);
    assert_eq!(
        on_unified.affordable_bytes,
        Some(0),
        "unified: honestly nothing left"
    );
    assert_eq!(
        on_unified.desired_mib,
        crate::inference::lane_args::CACHE_RAM_HARD_FLOOR_MIB,
        "the 256 MiB the 5090 actually launched with"
    );
    let want_bytes = demands
        .iter()
        .map(|t| fp.kv_per_token * *t as u64)
        .sum::<u64>();
    assert_eq!(
        on_discrete.affordable_bytes,
        Some(grantable(physical - host_os_floor_bytes(physical)))
    );
    assert_eq!(
        on_discrete.desired_mib as u64,
        want_bytes / (1024 * 1024),
        "discrete: sized from the citizens' demand, not squeezed to the floor"
    );
    assert_eq!(on_discrete.serve_host_bytes, 0);
}

// what this catches (2026-09-17, the 5090): the discrete footprint arm. WDDM
// attributes VRAM to no process, so the lane's bytes beyond its weights are the
// device delta across the spawn. The 5090's two points: 752 MiB idle → 31,914 MiB
// at a 144k window and 22,078 MiB at 9.4k, 17 GB of weights; and an adopted lane
// (no baseline) must say could-not-look, never charge the desktop to the model.
#[test]
fn the_discrete_footprint_arm_is_the_device_delta_beyond_the_weights() {
    use super::*;
    const MIB: u64 = 1024 * 1024;
    let weights = 17 << 30;
    let big = device_delta_beyond_weights(752 * MIB, 31_914 * MIB, weights).expect("read");
    let small = device_delta_beyond_weights(752 * MIB, 22_078 * MIB, weights).expect("read");
    assert!(big > small, "a bigger window costs more beyond the weights");
    // Through the ONE consumer: per-token from the plan's own decomposition.
    let per_token_big =
        crate::inference::lane_footprint::per_token_from(big, 1, 144_640, 0, 0).expect("rate");
    assert!(
        per_token_big > 60_000 && per_token_big < 110_000,
        "≈74 KiB/token measured on the 5090, got {per_token_big}"
    );
    assert_eq!(
        device_delta_beyond_weights(0, 31_914 * MIB, weights),
        None,
        "no baseline = could not look"
    );
    assert_eq!(
        device_delta_beyond_weights(752 * MIB, 0, weights),
        None,
        "no reading = could not look"
    );
    assert_eq!(
        device_delta_beyond_weights(u64::MAX, 31_914 * MIB, weights),
        None,
        "armed but never lowered = could not look"
    );
    // card b5230492 (the 5090 2026-10-11: anon_bytes=0 per_token_measured=0 every minute
    // for a lane serving 67,584 tokens): a trough baseline that already held the weights
    // makes the delta saturate. Zero or less beyond the weights is not a reading; it
    // falls through to the engine's own breakdown.
    assert_eq!(
        device_delta_beyond_weights(18_000 * MIB, 31_914 * MIB, weights),
        None,
        "the baseline held the weights: the delta is below them, so it is no reading"
    );
    assert_eq!(
        device_delta_beyond_weights(752 * MIB, 752 * MIB + weights, weights),
        None,
        "exactly the weights beyond the baseline: nothing measured beyond them"
    );
}

// what this catches (2026-09-17, the 5090 after #4144): with VRAM residency no
// longer charged to the host, afford read 55 GB on a 63 GB box that had 31 GB
// free — a `--cache-ram` that fills to a pagefile thrash once two 115k-token
// states are saved. The second ceiling is what is AVAILABLE now, less the floor.
// And it is not the commit charge: the first cut used it and read 75.5 GB
// committed on that same box with 43 GB free → afford 0 → the 256 MiB floor again.
// what this catches (2026-09-19, the M5 relaunched at one lane): a grant sized to
// "everything above the OS floor" fills into the Eco pressure band and steps the
// serving mode down — the cache throttles its own planner. For every available
// reading the box can report, what is left after the grant fills stays above the
// Eco EXIT band, so a full cache never enters Eco by itself; and the grant is still
// a real cache where there is room (the 46 GB reading gets more than the 20 GB one).
#[test]
fn a_full_cache_never_steps_the_serving_mode_down_by_itself() {
    use crate::capacity::power_mode::ECO_EXIT_BYTES;
    let fp = crate::inference::planning::serving_plan::ModelFootprint {
        model_id: "eco-fixture".into(),
        weights_bytes: 16 << 30,
        kv_per_token: 32_768,
        context_window: 67_340,
        capability_rank: 1,
        fixed_per_lane_bytes: 0,
    };
    let physical = 64u64 << 30;
    let demands = [67_340u32; 16]; // sixteen seeds on disk: the want is far past any box
    let mut last = 0u64;
    for available_gib in [13u64, 20, 29, 46] {
        let available = available_gib << 30;
        let d = prompt_cache_decision(
            Some(&fp),
            &demands,
            16,
            67_340,
            2,
            physical,
            fp.peak_resident_bytes(67_340, 2),
            available,
        );
        let grant = (d.desired_mib as u64) << 20;
        assert!(
                available - grant >= ECO_EXIT_BYTES,
                "available {available_gib} GiB: a full {} MiB cache would leave {} GiB — inside the Eco band",
                d.desired_mib,
                (available - grant) >> 30
            );
        assert!(
            grant >= last,
            "more room, no smaller grant: {available_gib} GiB → {} MiB",
            d.desired_mib
        );
        last = grant;
    }
    assert!(
        last > (8u64 << 30),
        "with 46 GiB free the cache is still a real cache: {} MiB",
        last >> 20
    );
}

#[test]
fn the_cache_never_promises_past_what_is_available_now() {
    use super::*;
    let fp = crate::inference::planning::serving_plan::ModelFootprint {
        model_id: "27b-on-a-5090".into(),
        weights_bytes: 17 << 30,
        kv_per_token: 262_144,
        context_window: 262_144,
        capability_rank: 1,
        fixed_per_lane_bytes: 0,
    };
    let physical = 63u64 << 30;
    let demands = [115_000u32, 115_000]; // ~58 GB wanted
    let unknown = prompt_cache_decision(Some(&fp), &demands, 2, 144_640, 1, physical, 0, 0);
    assert_eq!(
        unknown.affordable_bytes,
        Some(grantable(physical - host_os_floor_bytes(physical))),
        "no availability reading: headroom alone, less the co-consumer share"
    );
    let available = 31u64 << 30;
    let bound = prompt_cache_decision(Some(&fp), &demands, 2, 144_640, 1, physical, 0, available);
    assert_eq!(
        bound.affordable_bytes,
        Some(grantable_from_available(physical, available)),
        "bounded by what the box can give now, less the Eco exit band and the co-consumer share"
    );
    assert!(bound.desired_mib < unknown.desired_mib);
    assert_eq!(bound.available_bytes, available);
    // Less available than the floor → nothing to promise → the hard floor, no wrap.
    let starved = prompt_cache_decision(Some(&fp), &demands, 2, 144_640, 1, physical, 0, 1);
    assert_eq!(starved.affordable_bytes, Some(0));
    assert_eq!(
        starved.desired_mib,
        crate::inference::lane_args::CACHE_RAM_HARD_FLOOR_MIB
    );
}

// what this catches (the M5, 2026-09-20 02:4xZ): the board attributing 22.8 GB to a
// lane whose process held 27.5 GB — the 8.7 GB `--cache-ram` the daemon itself granted
// read as another holder's bytes, and the plan lost the second lane to it. The
// attribution is the plan's peak plus the granted cache; nothing resident, or no
// lanes, charges no cache.
#[test]
fn the_serving_attribution_includes_the_prompt_cache_it_granted() {
    let mib = 1024 * 1024;
    assert_eq!(
        attribute_serving(22_802_000_000, 1, 8_343),
        22_802_000_000 + 8_343 * mib
    );
    assert_eq!(
        attribute_serving(22_802_000_000, 2, 0),
        22_802_000_000,
        "no grant, no charge"
    );
    assert_eq!(
        attribute_serving(0, 2, 8_343),
        0,
        "nothing resident: the grant is not yet held"
    );
    assert_eq!(
        attribute_serving(19_000_000_000, 0, 8_343),
        19_000_000_000,
        "no lanes: weights only"
    );
}
