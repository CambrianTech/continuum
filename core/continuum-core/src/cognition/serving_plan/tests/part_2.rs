#![cfg(test)]
use super::*;

    // what this catches: lanes DEGRADE on a tight host — MAX_LANES is a sanity backstop,
    // the fit math is the real cap. A 4-persona demand on a budget that can't feed 4 warm
    // slots must serve FEWER (well-fed) lanes, never 4 starving ones that OOM. The window
    // still fits every lane it does serve. Guards the 2→4 raise from re-introducing the
    // OOM on a small box.
    #[test]
    pub(super) fn lanes_degrade_on_a_tight_host_never_overcommitting() {
        // ~18GB usable — fits the 14GB weights + a couple lanes' KV, not four.
        let host = HostBudget {
            usable_bytes: 18 * GB,
            perf_cores: 10,
        };
        let devstral = fp("devstral-24b", 14, 112 * 1024, 131_072, 3);
        let plan = plan_serving(
            &host,
            std::slice::from_ref(&devstral),
            &ServingDemand::new(4, None))
        .unwrap();
        assert!(plan.fits_on_gpu, "{}", plan.rationale);
        assert!(
            plan.lanes < 4,
            "tight host must serve fewer than the 4 demanded: got {}",
            plan.lanes
        );
        assert!(plan.lanes >= 1);
        let lanes = plan.lanes as u64;
        let c = plan.served_context_window as u64;
        let compute_floor = devstral.compute_buffer_per_lane();
        let compute_rate = devstral.kv_per_token / PREFILL_COMPUTE_KV_DIVISOR;
        let footprint = devstral.weights_bytes
            + devstral.kv_at(c as u32) * lanes
            + (compute_floor + compute_rate * c) * lanes;
        let effective = host.usable_bytes;
        assert!(
            footprint <= effective,
            "degraded plan still overcommits: {footprint} > {effective}"
        );
    }

    // what this catches: an 8GB Air must NOT be handed the 14B (won't fit) and
    // must NOT be left with nothing — it picks the most capable model that
    // actually fits on the GPU budget. "Figure out something to run."
    #[test]
    pub(super) fn tiny_box_picks_most_capable_that_fits_not_the_biggest() {
        // ~5.5GB usable after OS headroom on an 8GB Air.
        let host = HostBudget {
            usable_bytes: 5 * GB + 500 * 1_000_000,
            perf_cores: 4,
        };
        let plan = plan_serving(&host, &candidates(), &ServingDemand::new(MAX_LANES, None)).unwrap();
        assert!(
            plan.fits_on_gpu,
            "must fit a real model on GPU: {}",
            plan.rationale
        );
        assert_eq!(
            plan.base_model.model_id, "qwen3.5-4b",
            "14B can't fit 5.5GB; 4B is the most capable that does"
        );
        assert!(plan.lanes >= 1);
    }

    // what this catches: lanes come from DEMAND, never maximum concurrency.
    // 2 personas on a pressured budget must get 2 well-fed lanes, not 4
    // starving ones — the 2026-07-10 starvation served 2 minds through 4 slots
    // at 3633 tokens each and the room degenerated into a greeting loop. Same
    // budget, demand honored → each mind's window roughly doubles.
    #[test]
    pub(super) fn lanes_track_demand_and_every_unneeded_lane_stops_costing_window() {
        // A pressured budget (benchmark servers breathing next door). Demand a budget-bound
        // window (Some(u32::MAX)) so the "unneeded lane costs window" invariant is visible:
        // under the default demand cap both counts would hit the same cap and the window
        // difference would be masked — the invariant lives in the budget-bound regime.
        let host = HostBudget {
            usable_bytes: 20 * GB,
            perf_cores: 6,
        };
        let greedy = plan_serving(
            &host,
            &candidates(),
            &ServingDemand::new(MAX_LANES, Some(u32::MAX)))
        .unwrap();
        let demand2 =
            plan_serving(&host, &candidates(), &ServingDemand::new(2, Some(u32::MAX))).unwrap();
        assert_eq!(
            demand2.lanes, 2,
            "2 minds → 2 lanes, never the MAX_LANES ceiling"
        );
        if greedy.lanes > 2 {
            assert!(
                demand2.served_context_window > greedy.served_context_window,
                "fewer lanes must buy every mind a bigger window: {} lanes @ {} vs {} lanes @ {}",
                greedy.lanes,
                greedy.served_context_window,
                demand2.lanes,
                demand2.served_context_window,
            );
        }
        // Demand can never exceed the physical caps (kv/perf/MAX_LANES)…
        let demand99 = plan_serving(&host, &candidates(), &ServingDemand::new(99, None)).unwrap();
        assert!(demand99.lanes <= MAX_LANES);
        // …and a zero demand is defensively floored at one lane.
        assert_eq!(
            plan_serving(&host, &candidates(), &ServingDemand::new(0, None))
                .unwrap()
                .lanes,
            1
        );
    }

    // what this catches: #213 — the daemon must not chase a lane-count target off a cliff.
    // When a mid-session VRAM squeeze (an eval lane spinning up, a game) leaves room for
    // only ONE real window, demand for 2 concurrent lanes must SHED a lane (queue) rather
    // than collapse BOTH minds to the MIN_SERVE_CTX floor — a 2048-token mind can't hold its
    // own memory + the task. Live specimen 2026-07-20: Devstral re-homed to 2 lanes @ 2048
    // with the eval lane resident, while 1 lane would have served ~30k. The old math capped
    // lanes by what fit at the FLOOR window, so it admitted 2 and let the window collapse;
    // the fix picks the largest lane count whose per-lane window still clears the floor.
    #[test]
    pub(super) fn a_squeeze_sheds_a_lane_rather_than_flooring_every_mind() {
        let devstral = fp("devstral-24b", 14, 175_000, 131_072, 3);
        // Budget where ONE lane serves a real window but TWO would floor (eval lane resident).
        let squeezed = HostBudget {
            usable_bytes: 19 * GB,
            perf_cores: 6,
        };
        let plan = plan_serving(
            &squeezed,
            std::slice::from_ref(&devstral),
            &ServingDemand::new(2, None))
        .unwrap();
        assert_eq!(
            plan.lanes, 1,
            "2 lanes would floor → shed to 1 real lane, not 2 @ 2048"
        );
        assert!(
            plan.served_context_window > 4096,
            "the surviving lane gets a window big enough to think in, got {}",
            plan.served_context_window,
        );
        // And when there IS room, demand is honored: concurrency preserved AND each
        // mind at a real, DEMAND-sized window — capped at the working-set bootstrap
        // (2026-07-26), NOT maximized to fill RAM (the 94k→swap/wedge bug). A roomy
        // box buys more LANES (concurrent minds), not a bloated per-lane KV cache.
        let roomy = HostBudget {
            usable_bytes: 45 * GB,
            perf_cores: 6,
        };
        let plan2 = plan_serving(
            &roomy,
            std::slice::from_ref(&devstral),
            &ServingDemand::new(2, None))
        .unwrap();
        assert_eq!(plan2.lanes, 2, "roomy host serves both minds concurrently");
        assert!(
            plan2.served_context_window > 4096
                && plan2.served_context_window <= BOOTSTRAP_WORKING_SET,
            "each mind at a real window, demand-capped not budget-maxed, got {}",
            plan2.served_context_window,
        );
    }

    // what this catches: the M5 Pro must USE its headroom — pick the most capable
    // model (the 14B coding sentinel) and run multiple concurrent lanes. But the
    // per-lane WINDOW sizes to DEMAND, not to the budget (M5+BigMama 2026-07-26):
    // a roomy box buys more MINDS (lanes), not a bloated per-lane KV cache that
    // pre-allocates ~33GB and swaps (the 94k bug). This REPLACES the old
    // "size the window UP from the budget" contract — window scales with demand
    // (BOOTSTRAP_WORKING_SET cold-start, measured p95 later, #234), never RAM.
    // (Supersedes the #216 "don't cheapen the window" framing: right-sizing to a
    // 16k+ working set is not cheapening — flooring below the ~9k turn was. The
    // coding-headroom question — a bigger BOOTSTRAP for coders — is #234 A-next.)
    #[test]
    pub(super) fn big_box_picks_most_capable_runs_lanes_but_sizes_window_to_demand() {
        // ~45GB usable on a 64GB M5 Pro after headroom.
        let host = HostBudget {
            usable_bytes: 45 * GB,
            perf_cores: 6,
        };
        let plan = plan_serving(&host, &candidates(), &ServingDemand::new(MAX_LANES, None)).unwrap();
        assert_eq!(
            plan.base_model.model_id, "coder-sentinel-14b",
            "most capable, fits easily"
        );
        assert!(
            plan.lanes >= 2,
            "M5 Pro has the budget for multiple lanes, got {}",
            plan.lanes
        );
        // Window sized to DEMAND (capped at the working-set bootstrap), NOT maximized
        // to fill the 45GB budget — the fix for the cross-node swap/wedge. Still a
        // real window (well above the ~9k persona turn), never exceeds the ceiling.
        assert!(
            plan.served_context_window > 4096
                && plan.served_context_window <= BOOTSTRAP_WORKING_SET,
            "served window sizes to demand (≤ bootstrap cap), not budget-maxed, got {}",
            plan.served_context_window,
        );
        assert!(
            plan.served_context_window <= 262_144,
            "never exceed the model's trained ceiling, got {}",
            plan.served_context_window,
        );
    }

    // what this catches: the MAX_LANES ceiling holds even with absurd budget —
    // a single node doesn't run unbounded lanes (grid shares load past that).
    #[test]
    pub(super) fn lanes_capped_at_max() {
        let host = HostBudget {
            usable_bytes: 500 * GB,
            perf_cores: 64,
        };
        let plan = plan_serving(&host, &candidates(), &ServingDemand::new(MAX_LANES, None)).unwrap();
        assert_eq!(plan.lanes, MAX_LANES);
    }

    // what this catches: footprint-awareness — a model with a fatter per-token
    // KV cache yields fewer lanes on the same budget than a lean one (lanes are
    // sized at the MIN window; a fatter floor lane fits fewer times).
    #[test]
    pub(super) fn fatter_kv_means_fewer_lanes() {
        // Budget chosen so KV (not the MAX_LANES backstop or perf cores) is the binding
        // constraint: each warm slot must clear the full-turn window floor (16384, #266), so a
        // fatter per-token KV rate makes fewer slots clear it. 16GB total, 2GB weights → ~11.6GB
        // (after co-consumer headroom) for (KV + compute buffer) across lanes at a full-turn
        // window: lean (150k/tok) clears the floor at 3 slots; fat (450k/tok, 3× the KV) clears
        // it at only 1. (A 4GB host would floor BOTH to 1 lane — too small to show the effect.)
        let host = HostBudget {
            usable_bytes: 16 * GB,
            perf_cores: 8,
        };
        let lean = plan_serving(
            &host,
            &[fp("lean", 2, 150_000, 32_768, 5)],
            &ServingDemand::new(MAX_LANES, None))
        .unwrap();
        let fat = plan_serving(
            &host,
            &[fp("fat", 2, 450_000, 32_768, 5)],
            &ServingDemand::new(MAX_LANES, None))
        .unwrap();
        assert!(
            lean.lanes > fat.lanes,
            "lean {} should beat fat {}",
            lean.lanes,
            fat.lanes
        );
    }

    // what this catches: the plan CAPS lane count at the binding constraint — the full-turn
    // window floor (#266), not the raised MAX_LANES backstop — AND reserves the concurrent
    // compute buffers, so resident KV + those buffers fit the budget (no OOM by construction).
    // The transient prefill compute buffer scales with n_ctx, not weights, so a
    // window-independent reserve under-provisions and 4 concurrent large-window prefills
    // overflow. Whatever the cap, the fit invariant below must hold.
    #[test]
    pub(super) fn lane_count_respects_the_window_floor_and_reserves_compute_buffers() {
        // 24B-class: 13.6GB weights, kv_per_token ~156KB/token (measured), ~26GB usable.
        let m = fp("devstral-24b", 13, 156_000, 131_072, 9);
        let host = HostBudget {
            // 22.1 GB — what the old 15% plan reserve left of 26 GB; the pins below were
            // measured there (#4230: the plan sizes to the budget as handed).
            usable_bytes: 22_100_000_000,
            perf_cores: 10,
        };

        // 4 personas demand 4 lanes, but only 2 slots clear the full-turn window floor on this
        // budget — the window floor (below MAX_LANES=8) is the binding cap. The other 2 minds
        // surface as grid_overflow rather than thrashing 4 minds across 2 clobbering slots.
        let plan =
            plan_serving(&host, std::slice::from_ref(&m), &ServingDemand::new(4, None)).unwrap();
        assert_eq!(
            plan.lanes, 2,
            "the full-turn window floor (not the MAX_LANES backstop) caps warm slots here: {}",
            plan.rationale
        );
        assert_eq!(
            plan.grid_overflow_lanes, 2,
            "the 2 unslotted minds are surfaced for grid placement"
        );

        // The plan FITS: weights + lanes×KV@window + lanes×compute buffer ≤ budget — the
        // invariant the KV-only math violated (it left no room for the buffers).
        let kv = m.kv_at(plan.served_context_window) * plan.lanes as u64;
        let compute = m.compute_buffer_per_lane() * plan.lanes as u64;
        assert!(
            m.weights_bytes + kv + compute <= host.usable_bytes,
            "resident KV + concurrent compute buffers must fit the budget (no OOM)"
        );
    }

    // what this catches: GPU-residency-first — when nothing fits, the plan
    // says so (fits_on_gpu=false) and names the smallest, instead of silently
    // claiming a CPU plan. The caller owns the CPU/grid decision.
    #[test]
    pub(super) fn nothing_fits_degrades_honestly_no_silent_cpu() {
        let host = HostBudget {
            usable_bytes: 300 * 1_000_000,
            perf_cores: 2,
        }; // 0.3GB
        let plan = plan_serving(&host, &candidates(), &ServingDemand::new(MAX_LANES, None)).unwrap();
        assert!(
            !plan.fits_on_gpu,
            "must report the GPU budget can't hold any candidate"
        );
        assert_eq!(
            plan.base_model.model_id, "qwen2.5-0.5b",
            "names the smallest as the only option"
        );
        assert_eq!(plan.lanes, 1);
    }

    // what this catches: a persona seat holds what the residents REQUIRE — a comparison
    // against the measured-or-declared requirement, never a constant. The 5090's 2 × 2,048
    // against residents sending 56k (× 1.25 headroom = 70k) is not a seat; a 67k window
    // against a 30k typical is; with nothing measured or declared, only the engine's
    // runnable floor applies.
    #[test]
    pub(super) fn a_persona_lane_holds_what_the_residents_require() {
        assert_eq!(prompt_floor_of(56_057), 70_071, "the one arithmetic behind typical_prompt_floor and the placement rule");
        assert_eq!(prompt_floor_of(1), MIN_SERVE_CTX, "never under the serve floor");
        let demand = ServingDemand::new(2, Some(200_000)).with_sent_median(Some(56_057));
        let requirement = demand.typical_prompt_floor();
        assert_eq!(requirement, Some(70_071));
        assert!(!persona_lane_holds(2_048, requirement));
        assert!(!persona_lane_holds(67_072, requirement), "67k does not hold a 56k typical with headroom");
        assert!(persona_lane_holds(70_071, requirement));
        let thirty = ServingDemand::new(2, Some(200_000)).with_sent_median(Some(30_000)).typical_prompt_floor();
        assert!(persona_lane_holds(67_072, thirty));
        assert!(persona_lane_holds(MIN_SERVE_CTX, None), "nothing measured or declared → only the runnable floor");
        assert!(!persona_lane_holds(MIN_SERVE_CTX - 1, None));
    }

    // what this catches: no candidates → no plan (caller must supply a registry).
    #[test]
    pub(super) fn no_candidates_is_none() {
        let host = HostBudget {
            usable_bytes: 45 * GB,
            perf_cores: 6,
        };
        assert!(plan_serving(&host, &[], &ServingDemand::new(MAX_LANES, None)).is_none());
    }

    // small chat (rank 1, ~1GB) vs big coder (rank 3, ~9.7GB) — the pair that
    // exercises switch-up/down decisions.
    pub(super) fn pair() -> Vec<ModelFootprint> {
        vec![
            fp("small", 1, 4_000, 32_768, 1),
            fp("big", 9, 90_000, 262_144, 3),
        ]
    }

    // what this catches: no incumbent → identical to plain plan_serving (boot).
    #[test]
    pub(super) fn stable_with_no_incumbent_equals_plain() {
        let host = HostBudget {
            usable_bytes: 20 * GB,
            perf_cores: 6,
        };
        assert_eq!(
            plan_serving_stable(&host, &pair(), None, &ServingDemand::new(MAX_LANES, None)),
            plan_serving(&host, &pair(), &ServingDemand::new(MAX_LANES, None))
        );
    }

    // what this catches: THE thrash guard — fresh prefers `big` (more capable,
    // fits) but it only fits without headroom, so we KEEP the incumbent `small`
    // rather than flap the served model on a transient budget bump.
    #[test]
    pub(super) fn stable_keeps_incumbent_when_upgrade_lacks_headroom() {
        // LOCAL fixture, not `pair()`. This test needs `big` to be a LEGITIMATE upgrade
        // target (clears the full-turn window floor, so selection would really pick it)
        // that nonetheless fails the switch-UP headroom bar. With the shared `pair()` at
        // 10GB those two are unsatisfiable together: big (9GB + 90k/tok) needs 10.47GB to
        // clear one full turn, so at any budget where it lacks 0.9x headroom it also
        // isn't full-turn viable — and a model that can only be served blind is not an
        // upgrade worth flapping for. Before the model-choice floor landed, the old
        // MIN_SERVE_CTX bar admitted big here on 184MB of KV and this fixture read as
        // "fresh would pick big" when what fresh actually had was a 2048-token 9GB model.
        // PREMISE CHANGE (2026-08-19), stated as such: this fixture was sized against the
        // UN-CREDITED budget, because the switch-up bar used to be `0.9 * host`. The bar now
        // sits on the POST-EVICTION budget (`host` + the incumbent's own weights), since
        // `serve()` kills the incumbent before launching the candidate and the two are never
        // co-resident. Crediting `small`'s 1GB moved the bar 18.0 → 18.9, and the old `big`
        // (18GB + 0.18 KV = 18.18) slid under it — so the test began asserting the OPPOSITE of
        // its own name. The INVARIANT is untouched: a model that lacks headroom must not be
        // adopted. Only the fixture is restated for the corrected budget.
        //
        // Re-sized so big still clears a full turn at the at-rest budget (19 + 1.47 = 20.47 <=
        // 21) — a legitimate upgrade target selection would really pick — yet still exceeds the
        // headroom bar (19 + 0.18 = 19.18 > 0.9 * 21 = 18.9).
        let host = HostBudget {
            usable_bytes: 20 * GB,
            perf_cores: 6,
        };
        let models = vec![
            fp("small", 1, 4_000, 32_768, 1),
            fp("big", 19, 90_000, 262_144, 3),
        ];
        // Setup assertion: big IS what selection would choose, so the refusal below is a real
        // headroom refusal and not "big was never a candidate". This asks the question at the
        // budget `fresh` is now computed against — the at-rest one — because that is where the
        // stabilizer does its selecting. Asking it at `host` would make the setup vacuous: big
        // (19 + 1.47 = 20.47) does not clear a full turn in 20GB at all, so the test would pass
        // for the wrong reason, asserting a refusal of something never on offer.
        let at_rest_for_setup = HostBudget {
            usable_bytes: host.usable_bytes + 1 * GB, // small's credited weights
            perf_cores: host.perf_cores,
        };
        // PREMISE CHANGE (2026-08-30), stated as such: demand here is now SOLO.
        // The single-lane gate (the Flash-Next lesson) makes a 19GB-on-21GB
        // model ineligible under MULTI-lane demand by design — near-budget
        // weights and fleet parallelism are mutually exclusive premises at this
        // scale, so under fleet demand this fixture's "big" is correctly never
        // on offer and the headroom refusal would be vacuous. The INVARIANT
        // under test — an upgrade lacking headroom must not be adopted — is
        // demand-independent; solo demand is where both premises coexist.
        assert_eq!(
            plan_serving(&at_rest_for_setup, &models, &ServingDemand::new(1, None))
                .unwrap()
                .base_model
                .model_id,
            "big",
            "fresh would pick big at the post-eviction budget — so the refusal below is real"
        );
        let stable = plan_serving_stable(
            &host,
            &models,
            Some("small"),
            &ServingDemand::new(1, None))
        .unwrap();
        assert_eq!(
            stable.base_model.model_id, "small",
            "hysteresis keeps incumbent — no flap"
        );
        assert!(
            stable.lanes >= 1,
            "lanes still re-tracked for the kept model"
        );
    }

    // what this catches: THE LANE FLAP — a serving model self-evicting its own lane.
    // `plan_serving_stable` used to early-return the FRESH plan whenever fresh chose the
    // incumbent, and fresh is computed against the LIVE budget, which the incumbent's own
    // weights + KV depress while it serves. So the planner kept re-deciding it could not
    // afford the lane it was already running, dropped to 1, then re-added it when the KV
    // freed. Measured on an idle host: 718 replans in ONE solve, lanes oscillating 1↔2, each
    // flip resizing the live admission semaphore and prefill throttle under in-flight
    // requests — the `no response headers for 300s` wedge that killed three benchmark runs.
    //
    // Same budget, same model, only difference is whether the incumbent is declared: the
    // stable plan must NOT serve fewer lanes than the plan that boot would have made at rest.
    #[test]
    pub(super) fn a_serving_model_never_sheds_its_own_lane_to_its_own_residency() {
        // 9GB model on an 18GB host — the measured flap zone. At rest it plans (2 lanes,
        // 16384). Once it is SERVING, its own 9GB reads as "used", and a fresh plan against
        // that depressed budget returns (1 lane, 2048): its own residency costs it a lane AND
        // 87% of its window, so the next tick (KV freed) plans it straight back up. That is
        // the oscillation, and every flip resizes the live admission semaphore + prefill
        // throttle under in-flight requests.
        let models = vec![fp("big", 9, 90_000, 262_144, 3)];
        let at_rest = HostBudget {
            usable_bytes: 18 * GB,
            perf_cores: 6,
        };
        let boot = plan_serving(&at_rest, &models, &ServingDemand::new(MAX_LANES, None))
            .expect("servable at rest");

        let live = HostBudget {
            usable_bytes: at_rest.usable_bytes - models[0].weights_bytes,
            perf_cores: 6,
        };
        let fresh = plan_serving(&live, &models, &ServingDemand::new(MAX_LANES, None))
            .expect("still servable");
        // Guard the guard: if this ever stops being a flap, the test below proves nothing.
        assert!(
            fresh.lanes < boot.lanes,
            "fixture no longer reproduces the flap (boot={} fresh={}) — re-derive the budget",
            boot.lanes,
            fresh.lanes
        );

        let stable = plan_serving_stable(
            &live,
            &models,
            Some("big"),
            &ServingDemand::new(MAX_LANES, None))
        .expect("incumbent still servable");
        assert_eq!(stable.base_model.model_id, "big");
        assert_eq!(
            stable.lanes, boot.lanes,
            "a model's OWN residency must not shrink its OWN lane count (stable={} boot={} \
             fresh={})",
            stable.lanes, boot.lanes, fresh.lanes
        );
        assert_eq!(
            stable.served_context_window, boot.served_context_window,
            "nor its own window (stable={} boot={} fresh={})",
            stable.served_context_window, boot.served_context_window, fresh.served_context_window
        );
    }

    // what this catches: a genuine upgrade DOES happen when the better model
    // fits with headroom — hysteresis isn't a permanent lock-in.
    #[test]
    pub(super) fn stable_upgrades_when_better_model_fits_with_headroom() {
        let host = HostBudget {
            usable_bytes: 20 * GB,
            perf_cores: 6,
        }; // big 9.7 << 0.9*20=18
        // Demand 1: the roster fits either model, so this test isolates the HEADROOM
        // rule. (With MAX_LANES the bigger model would leave minds cold that were warm
        // on the small one, and the switch-up guard refuses — its own test below.)
        let stable = plan_serving_stable(&host, &pair(), Some("small"), &ServingDemand::new(1, None))
            .unwrap();
        assert_eq!(
            stable.base_model.model_id, "big",
            "more capable + ample headroom → upgrade"
        );
    }

    // what this catches: forced switch when the incumbent has DROPPED OFF DISK
    // (no longer among candidates) — we can't keep serving a model whose weights
    // vanished, so we fall to whatever is actually present. This is the genuine
    // forced-down path now that a RESIDENT incumbent is credited its own weights
    // (memory-pressure eviction of a resident model for a smaller one is provably
    // unreachable: room for the smaller model's full weights implies room for the
    // incumbent's tiny KV floor).
    // what this catches: a switch-up that makes more minds cold. Measured 2026-09-06
    // on the M5: a 5-lane MoE incumbent was replaced by a 3-lane dense model on
    // measured capability alone with 12 minds resident (ACT 0 / QUE 12, one act in
    // 30 min). While the roster is oversubscribed at the incumbent, a candidate that
    // serves FEWER lanes is not an upgrade; once the roster fits, capability decides.
    #[test]
    pub(super) fn a_switch_up_never_makes_more_minds_cold() {
        // 64 GB usable: the MoE (22 GB, tiny KV) hosts the whole roster, the dense model
        // (19 GB, KV ten times larger) hosts part of it. Ratios, not exact counts, carry
        // the test.
        let host = HostBudget {
            usable_bytes: 64 * GB,
            perf_cores: 8,
        };
        let moe = fp("moe", 22, 40_000, 131_072, 23);
        let dense = fp("dense", 19, 400_000, 131_072, 42);
        let both = vec![moe.clone(), dense.clone()];

        let inc_only = plan_serving(&host, std::slice::from_ref(&moe), &ServingDemand::new(12, None))
            .expect("the incumbent plans");
        let dense_only =
            plan_serving(&host, std::slice::from_ref(&dense), &ServingDemand::new(12, None))
                .expect("the candidate plans");
        assert!(
            dense_only.lanes < inc_only.lanes && (dense_only.lanes as u32) < 12,
            "fixture: dense {} < moe {} and dense < roster 12",
            dense_only.lanes,
            inc_only.lanes
        );

        let oversubscribed =
            plan_serving_stable(&host, &both, Some("moe"), &ServingDemand::new(12, None))
                .expect("a plan");
        assert_eq!(
            oversubscribed.base_model.model_id, "moe",
            "12 minds warm on the MoE: the higher-ranked dense model would leave some cold \
             — not an upgrade"
        );

        let roster_fits = plan_serving_stable(&host, &both, Some("moe"), &ServingDemand::new(1, None))
            .expect("a plan");
        assert_eq!(
            roster_fits.base_model.model_id, "dense",
            "one mind: nobody goes cold, so the measured capability decides"
        );
    }

    #[test]
    pub(super) fn stable_forced_down_when_incumbent_gone_from_disk() {
        let host = HostBudget {
            usable_bytes: 20 * GB,
            perf_cores: 6,
        };
        let only_small = vec![fp("small", 1, 4_000, 32_768, 1)]; // "big" no longer on disk
        let stable = plan_serving_stable(
            &host,
            &only_small,
            Some("big"),
            &ServingDemand::new(MAX_LANES, None))
        .unwrap();
        assert_eq!(
            stable.base_model.model_id, "small",
            "incumbent gone from disk → serve what's present"
        );
    }

    // what this catches: THE boot-load flap (this session's live bug). While the
    // incumbent loads, its own ~9GB of weights read as "used" in live free memory,
    // so the reported budget craters (35GB → 8GB) and a plain plan would drop the
    // incumbent for a smaller model that "fits" the depressed budget — tearing down
    // the healthy lane mid-deliberation (14b → 4b → 14b). Crediting the incumbent's
    // OWN weights back (it is already resident) keeps it selected across its own
    // load. regression for the 14b→4b→14b flap at boot.
    #[test]
    pub(super) fn stable_survives_its_own_load_dip_no_flap() {
        // Mid-load the monitor reports only 8GB free because the 9GB incumbent's
        // weights are paging in; steady-state would be far higher.
        let dipped = HostBudget {
            usable_bytes: 8 * GB,
            perf_cores: 6,
        };
        // Plain plan at the depressed budget WOULD flap: big (9GB) no longer "fits"
        // 8GB, so fresh prefers the smaller model.
        assert_eq!(
            plan_serving(&dipped, &pair(), &ServingDemand::new(MAX_LANES, None))
                .unwrap()
                .base_model
                .model_id,
            "small",
            "depressed-budget plain plan would flap to the smaller model"
        );
        // With the incumbent credited its own weights back, the resident big stays.
        let stable = plan_serving_stable(
            &dipped,
            &pair(),
            Some("big"),
            &ServingDemand::new(MAX_LANES, None))
        .unwrap();
        assert_eq!(
            stable.base_model.model_id, "big",
            "incumbent survives its OWN load dip — no flap"
        );
        assert!(stable.lanes >= 1, "kept model still gets ≥1 lane");
    }

    // what this catches: the autonomic planner being structurally unable to reach a BETTER
    // model that fits — because it measured the candidate against a budget the incumbent was
    // still occupying, while `serve()` evicts before it launches.
    //
    // These are THIS MacBook's real numbers on 2026-08-19, not invented ones. The live
    // `serving.plan` chose Devstral (24B, ~14 GB) at `usable_gb: 18` on a 64 GB box, so
    // Qwen3.8-27B (18.97 GB) "did not fit" and never entered the plan. `serving/pin`, same
    // instant, credited the incumbent back and reported `budget_gb: 34.09` — the 27B fit with
    // 15 GB spare and served at 17.2 tok/s. Two answers to one question; only the operator
    // path could reach the better model.
    #[test]
    pub(super) fn a_more_capable_model_that_fits_after_eviction_is_adopted_not_starved() {
        // 18 GB free WITH the ~14 GB incumbent resident — the measured live condition.
        let live = HostBudget {
            usable_bytes: 18 * GB,
            perf_cores: 10,
        };
        let models = vec![
            fp("devstral-24b", 14, 100_000, 131_072, 5),
            fp("qwen3.8-27b", 19, 100_000, 262_144, 9), // strictly more capable
        ];

        // The un-credited question — "fits BESIDE the incumbent?" — answers the wrong thing.
        assert_eq!(
            plan_serving(&live, &models, &ServingDemand::new(2, None))
                .unwrap()
                .base_model
                .model_id,
            "devstral-24b",
            "against the live budget the 27B cannot fit — this is the state that stranded it"
        );

        // The stabilizer credits the eviction and takes the upgrade.
        let plan = plan_serving_stable(
            &live,
            &models,
            Some("devstral-24b"),
            &ServingDemand::new(2, None))
        .expect("a plan exists");
        assert_eq!(
            plan.base_model.model_id, "qwen3.8-27b",
            "a strictly more capable model that fits POST-EVICTION must be adopted — the swap \
             kills the incumbent before launching, so the co-resident test was never the right \
             question"
        );
    }
