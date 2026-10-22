#![cfg(test)]

    use super::*;

    // what this catches (card 2eec3977, the 5090 at 2 × 2048 on 2026-09-20): with the
    // roster's REQUIREMENTS on the demand, the plan's lanes × window are the
    // allocator's — two coders at 70k on a 30 GB budget serve at 2 × 70k, never a
    // starved window and never the sent-median path; a squeeze sheds a mind (grid
    // overflow), not the window; a sticky window stands in only while it covers the
    // requirement; and when no candidate holds one lane at the smallest requirement
    // the plan says so with fits_on_gpu=false instead of planning a 2k lane.
    #[test]
    pub(super) fn with_requirements_the_plan_is_the_allocators_never_a_starved_lane() {
        use crate::inference::planning::window_allocator::LaneRequirement;
        const GB: u64 = 1 << 30;
        let qwen = ModelFootprint {
            model_id: "qwen-27b".into(),
            weights_bytes: 19 * GB,
            kv_per_token: 33_000,
            context_window: 262_144,
            capability_rank: 9,
            fixed_per_lane_bytes: 0,
        };
        let host = HostBudget { usable_bytes: 30 * GB, perf_cores: 8 };
        let two = vec![
            LaneRequirement::declared(uuid::Uuid::new_v4(), 70_000),
            LaneRequirement::declared(uuid::Uuid::new_v4(), 70_000),
        ];
        // The poisoned inputs the 5090 had: a sent median of 828 and a sticky 2048.
        let demand = ServingDemand::new(2, Some(56_057))
            .with_sent_median(Some(828))
            .with_sticky_window(Some(2048))
            .with_requirements(two.clone());
        let plan = plan_serving(&host, std::slice::from_ref(&qwen), &demand).expect("plan");
        assert_eq!((plan.lanes, plan.served_context_window), (2, 70_000), "{}", plan.rationale);
        assert!(plan.fits_on_gpu);

        let five: Vec<_> = (0..5).map(|_| LaneRequirement::declared(uuid::Uuid::new_v4(), 70_000)).collect();
        let squeezed = plan_serving(&host, std::slice::from_ref(&qwen), &ServingDemand::new(5, Some(56_057)).with_requirements(five)).expect("plan");
        assert_eq!((squeezed.lanes, squeezed.served_context_window), (2, 70_000), "minds shed, window kept");
        assert_eq!(squeezed.grid_overflow_lanes, 3);

        // Regression: the 5090's publication gate retained a 26,880-token lane
        // after rejecting improved plans against a historical 176,730 floor.
        // The allocator's per-mind requirements, not that second owner, govern.
        let stale_history = demand.clone().with_sent_median(Some(141_384));
        assert_eq!(stale_history.typical_prompt_floor(), Some(176_730));
        let governed = plan_serving(&host, std::slice::from_ref(&qwen), &stale_history).expect("governed plan");
        assert!(persona_lane_holds(governed.served_context_window, stale_history.publication_prompt_floor(&governed)),
            "publication must accept the allocator's feasible per-mind window");
        let legacy = ServingDemand::new(2, None).with_sent_median(Some(141_384));
        assert!(!persona_lane_holds(governed.served_context_window, legacy.publication_prompt_floor(&governed)),
            "without per-mind allocation, the legacy safety floor must still apply");

        let covering = plan_serving(&host, std::slice::from_ref(&qwen), &demand.clone().with_sticky_window(Some(72_000))).expect("plan");
        assert_eq!(covering.served_context_window, 72_000, "a sticky window that covers the requirement and fits stands");

        // The no-fit early return precedes typed allocation: it must not inherit
        // the successful allocator's exemption and replace a working incumbent.
        let impossible = HostBudget { usable_bytes: GB, perf_cores: 8 };
        let fallback = plan_serving(&impossible, std::slice::from_ref(&qwen), &stale_history).expect("fallback");
        assert!(!fallback.fits_on_gpu);
        assert!(!persona_lane_holds(fallback.served_context_window, stale_history.publication_prompt_floor(&fallback)),
            "unallocated no-fit fallback must retain the historical guard");

        // 21 GB: the weights fit and a 2048 lane would "fit" — the legacy path served
        // exactly that; the governed path refuses to plan a lane below the requirement.
        let tiny = HostBudget { usable_bytes: 21 * GB, perf_cores: 8 };
        assert!(qwen.window_within(tiny.usable_bytes, 1) >= MIN_SERVE_CTX, "precondition: a starved lane was plannable here");
        let none = plan_serving(&tiny, std::slice::from_ref(&qwen), &demand).expect("a plan that says why");
        assert!(!none.fits_on_gpu);
        assert_eq!(none.lanes, 0);
        assert!(none.rationale.contains("smallest requirement"), "{}", none.rationale);
    }

    pub(super) const GB: u64 = 1_000_000_000;

    // what this catches: a bootstrap prior being reported as measured demand. Live
    // 2026-08-20 the plan served 16384 with `bound_by=demand` and 33 GB of VRAM free —
    // reading as "the minds asked for exactly this", when no turn had been measured at
    // all. The number is fine as a PRIOR; laundering it into a measurement is what made
    // the window's failure to grow unfalsifiable. Pins provenance to survive `unwrap_or`.
    #[test]
    pub(super) fn a_bootstrap_window_never_claims_to_be_measured_demand() {
        let cold = ServingDemand::new(4, None);
        assert_eq!(cold.window_tokens, BOOTSTRAP_WORKING_SET);
        assert!(
            !cold.measured,
            "no turn has been assembled, so the window is a prior — not something a mind asked for"
        );

        let warm = ServingDemand::new(4, Some(31_834));
        assert_eq!(warm.window_tokens, 31_834);
        assert!(warm.measured);

        // The degenerate case that makes the flag load-bearing rather than cosmetic: a
        // real measurement that happens to EQUAL the bootstrap value is still measured,
        // so the two states can never be told apart by comparing the number alone.
        let coincidence = ServingDemand::new(1, Some(BOOTSTRAP_WORKING_SET));
        assert_eq!(coincidence.window_tokens, cold.window_tokens);
        assert!(
            coincidence.measured && !cold.measured,
            "identical windows, opposite provenance — the value cannot carry this fact"
        );
    }

    // kv_per_token in BYTES; ctx is the model's trained ceiling. The served
    // window is DERIVED from (footprint ∩ host) by the planner, never passed in.
    pub(super) fn fp(id: &str, weights_gb: u64, kv_per_token: u64, ctx: u32, rank: u8) -> ModelFootprint {
        ModelFootprint {
            model_id: id.to_string(),
            weights_bytes: weights_gb * GB,
            kv_per_token,
            context_window: ctx,
            capability_rank: rank,
            fixed_per_lane_bytes: 0,
        }
    }

    // what this catches: the lane-axis twin of the window-collapse selection bug
    // (glass-boxed 2026-08-30). A higher-ranked model that can only ever serve
    // ONE lane (Flash-Next: giant weights, one slot) must not be crowned over a
    // slightly lower-ranked model that serves every demanded lane — four minds
    // spent hours single-file through one slot while per-act numbers looked
    // healthy. Parallelism is capability; selection maximizes achievable lanes
    // toward demand FIRST, capability second. Degrade honesty is pinned too:
    // when demand is one lane, the capability order stands unchanged.
    // what this catches: 16 residents planned onto 3 lanes because ONE mind's 96k
    // sent prompt became every lane's size (2026-09-14, 40 GB of KV idle). With the
    // floor at the residents' TYPICAL prompt the same host multiplies lanes toward
    // the roster; the ceiling shapes the window only when it still fits.
    #[test]
    pub(super) fn lanes_multiply_to_the_roster_at_the_typical_prompt_not_the_outlier() {
        // 51.85 GB — what the old 15% plan reserve left of 61 GB; the outlier/typical
        // contrast below was measured there (#4230: the plan sizes to the budget as handed).
        let host = HostBudget { usable_bytes: 51_850_000_000, perf_cores: 12 };
        let ornith = fp("ornith", 21, 41_984, 262_144, 200);
        let c = vec![ornith];
        let outlier_only = ServingDemand::new(17, Some(120_000)).with_sent_tokens(Some(96_000));
        let typical = ServingDemand::new(17, Some(120_000))
            .with_sent_tokens(Some(96_000))
            .with_sent_median(Some(40_000));
        let a = plan_serving(&host, &c, &outlier_only).expect("plan");
        let b = plan_serving(&host, &c, &typical).expect("plan");
        assert!(b.lanes >= 5, "the roster gets its lanes at the typical prompt: {}", b.lanes);
        assert!(
            b.served_context_window >= 50_000,
            "each lane holds a typical turn with headroom (40k × 1.25): {}",
            b.served_context_window
        );
        // Without the median the planner packs the most lanes at the bootstrap floor —
        // more lanes, each too small for a real turn. The floor trades at most a lane or
        // two for windows the roster can actually use. (Unchanged by the target following
        // the typical prompt, card 29e4ab34: the floor, not the target, is what packs.)
        assert!(a.served_context_window < b.served_context_window, "{} vs {}", a.served_context_window, b.served_context_window);
        assert!(b.lanes + 2 >= a.lanes, "the floor costs at most two lanes: {} vs {}", b.lanes, a.lanes);
    }

    #[test]
    pub(super) fn a_one_lane_giant_never_outranks_a_model_that_serves_the_fleet() {
        // 60GB budget. "giant" rank 5 fits ONE bootstrap lane and no more;
        // "team" rank 3 serves 3+ lanes comfortably.
        let host = HostBudget {
            usable_bytes: 60 * GB,
            perf_cores: 8,
        };
        let giant = fp("giant-flash", 50, 200_000, 262_144, 5);
        let team = fp("team-brain", 20, 60_000, 262_144, 3);
        let c = vec![giant.clone(), team.clone()];

        let plan = plan_serving(&host, &c, &ServingDemand::new(4, None)).expect("plan");
        assert_eq!(
            plan.base_model.model_id, "team-brain",
            "fleet demand 4: the model serving the fleet wins over the one-lane giant"
        );
        assert!(plan.lanes >= 2, "the chosen plan actually multiplies lanes: {}", plan.lanes);

        // Demand 1: old capability order stands — the giant is honestly best.
        let solo = plan_serving(&host, &c, &ServingDemand::new(1, None)).expect("plan");
        assert_eq!(
            solo.base_model.model_id, "giant-flash",
            "solo demand: capability preference unchanged (degrade honesty)"
        );
    }

    pub(super) fn candidates() -> Vec<ModelFootprint> {
        vec![
            fp("qwen2.5-0.5b", 1, 4_000, 32_768, 1),         // tiny chat
            fp("qwen3.5-4b", 3, 30_000, 262_144, 2),         // good general (47 tok/s on M5)
            fp("coder-sentinel-14b", 9, 90_000, 262_144, 3), // rich coding model — more RAM each
        ]
    }

    // what this catches: resident_bytes folds weights + per-lane KV × lanes into
    // ONE honest residency — the number serving reports as its footprint(). If it
    // dropped the lane multiply (or the KV term entirely, the pre-#79 bug), the
    // board would under-count serving and the missing bytes would masquerade as
    // external contention. 9GB weights + 4 lanes × (90_000 × 11_008) KV.
    #[test]
    pub(super) fn resident_folds_weights_plus_per_lane_kv() {
        let f = fp("coder-sentinel-14b", 9, 90_000, 262_144, 3);
        let per_lane_kv = 90_000u64 * 11_008; // kv_at(11_008)
        assert_eq!(f.kv_at(11_008), per_lane_kv);
        assert_eq!(f.resident_bytes(11_008, 4), 9 * GB + per_lane_kv * 4);
        // A lane-less snapshot (lanes == 0) still charges one lane's KV, never zero.
        assert_eq!(f.resident_bytes(11_008, 0), 9 * GB + per_lane_kv);
        // No window served yet → weights only (KV term is zero).
        assert_eq!(f.resident_bytes(0, 4), 9 * GB);
    }

    // what this catches: #56/G5 — the footprint serving REPORTS to the board is the PEAK
    // (weights + per-lane KV + per-lane prefill compute reserve), and it equals the plan's
    // `chosen_cost` EXACTLY via ONE shared `prefill_compute_reserve` formula. If peak dropped
    // the compute term (the pre-G5 bug: footprint == resident_bytes), the board would
    // over-report free VRAM by the prefill buffer and a second consumer (eval lane, train
    // job) would grab the bytes serving needs for its next prefill → the concurrent-OOM.
    #[test]
    pub(super) fn peak_resident_equals_plan_chosen_cost_including_window_scaled_compute() {
        let f = fp("coder-sentinel-14b", 9, 90_000, 262_144, 3);
        let c = 11_008u32;
        let lanes = 4u32;
        // The reserve is window-SCALED: floor + compute_rate·C, times lanes.
        let compute_rate = 90_000u64 / PREFILL_COMPUTE_KV_DIVISOR;
        let expect_reserve = (f.compute_buffer_per_lane() + compute_rate * c as u64) * lanes as u64;
        assert_eq!(f.prefill_compute_reserve(c, lanes), expect_reserve);
        // Peak = resident + reserve — strictly greater than resident (the pre-G5 report).
        assert_eq!(
            f.peak_resident_bytes(c, lanes),
            f.resident_bytes(c, lanes) + expect_reserve
        );
        assert!(f.peak_resident_bytes(c, lanes) > f.resident_bytes(c, lanes));

        // The keystone invariant: what serving REPORTS (peak) is what the plan SIZED
        // against (chosen_cost) — never two figures that drift. Plan a real serving shape
        // and assert peak_resident_bytes at the plan's own (window, lanes) reproduces the
        // budget the fixpoint consumed for the chosen model.
        let host = HostBudget {
            usable_bytes: 48 * GB,
            perf_cores: 10,
        };
        let devstral = fp("devstral-24b", 14, 112 * 1024, 131_072, 3);
        let plan = plan_serving(
            &host,
            std::slice::from_ref(&devstral),
            &ServingDemand::new(4, None))
        .unwrap();
        let chosen_cost = devstral.weights_bytes
            + devstral.kv_at(plan.served_context_window) * plan.lanes as u64
            + devstral.prefill_compute_reserve(plan.served_context_window, plan.lanes);
        assert_eq!(
            devstral.peak_resident_bytes(plan.served_context_window, plan.lanes),
            chosen_cost,
            "the footprint serving reports == the plan's chosen_cost (one formula)"
        );
        // A lane-less / no-window snapshot still charges the per-lane compute floor.
        assert_eq!(
            f.prefill_compute_reserve(0, 0),
            f.compute_buffer_per_lane(),
            "zero window + zero lanes → one lane's compute floor, never zero"
        );
    }

    // what this catches: the governor serving a mind a window too small to think in
    // because MODEL CHOICE was floored at bare survival while LANE COUNT was floored at
    // a full turn. Glass-boxed live 2026-08-17: a 27B picked at usable_gb=5 →
    // served_window=2048 against a measured demand_window=63817, and every SWE act ran
    // against a context that could not hold the task statement. The pre-fix filter asked
    // "can this model hold MIN_SERVE_CTX (2048)?", so the biggest model always won and
    // then starved the window to that trivial bar. Shedding a lane and shedding
    // capability are the same move for the same reason (1 lane @ 30k beats 2 lanes @ 2k;
    // a 14B @ 30k beats a 27B @ 2k) — only the first had been implemented.
    #[test]
    pub(super) fn model_choice_sheds_capability_rather_than_starve_the_window() {
        // Big model clears 2048 but NOT a full turn; small model clears a full turn.
        let big = fp("big-27b", 28, 256 * 1024, 131_072, 5);
        let small = fp("small-14b", 10, 64 * 1024, 131_072, 3);
        let candidates = [big.clone(), small.clone()];
        let demand = ServingDemand::new(1, None);

        // 30GB: big fits ONLY at the survival floor (28 + 0.5 ≤ 30, but 28 + 4.3 > 30).
        // Pre-fix this crowned `big` and served 2048. It must now pick `small`.
        let plan = plan_serving(
            &HostBudget { usable_bytes: 30 * GB, perf_cores: 10 },
            &candidates,
            &demand)
        .expect("a model fits");
        assert_eq!(
            plan.base_model.model_id, "small-14b",
            "a model that fits only at the 2048 survival floor is not a serving option \
             on this host — shed capability, never starve the window"
        );
        assert!(
            plan.served_context_window >= BOOTSTRAP_WORKING_SET,
            "the served window must clear one full turn ({BOOTSTRAP_WORKING_SET}), got {}",
            plan.served_context_window
        );

        // NEGATIVE CONTROL — the floor must not cost capability when the host can afford
        // it. On a roomy host the 27B clears a full turn and must still win, otherwise
        // this fix would have silently downgraded every capable box.
        let roomy = plan_serving(
            &HostBudget { usable_bytes: 64 * GB, perf_cores: 10 },
            &candidates,
            &demand)
        .expect("a model fits");
        assert_eq!(
            roomy.base_model.model_id, "big-27b",
            "when the budget clears a full turn, the MOST capable model still wins"
        );

        // DEGRADE PATH — when NO candidate clears a full turn, fall back to the old
        // survival bar so a genuinely tiny host serves something rather than nothing.
        // Honest starvation is then real, not an artifact of the selection rule.
        let tiny = plan_serving(
            &HostBudget { usable_bytes: 11 * GB, perf_cores: 4 },
            &candidates,
            &demand)
        .expect("the survival fallback still yields a model");
        assert_eq!(tiny.base_model.model_id, "small-14b");
        assert!(tiny.fits_on_gpu, "the fallback still serves on GPU, just narrowly");
    }

    // what this catches: the "alive" OOM (2026-07-16). The served window's FULL live
    // footprint — weights + lanes·KV(C) + lanes·(compute_floor + compute_rate·C), every
    // lane prefilling at once — must fit within the EFFECTIVE budget (usable − co-consumer
    // headroom). The pre-fix math reserved a WINDOW-INDEPENDENT compute buffer, so a roomy
    // snapshot sized a huge window whose real (window-scaled) prefill buffer then blew the
    // shared Metal pool (`kIOGPU…OutOfMemory`). Devstral-24B-class model on a 64GB-class
    // budget — the exact shape that picked the OOMing 53k window.
    #[test]
    pub(super) fn served_window_footprint_fits_effective_budget_including_window_scaled_compute() {
        let host = HostBudget {
            usable_bytes: 48 * GB,
            perf_cores: 10,
        };
        let devstral = fp("devstral-24b", 14, 112 * 1024, 131_072, 3); // ~112 KiB/token KV
                                                                       // Demand = 4 personas. This roomy 48GB host clears the full-turn window floor at 4
                                                                       // slots (#266: a warm slot per resident persona), so all 4 are served — the window is
                                                                       // derived to fit whatever lane count is served, and the fit invariant below holds at
                                                                       // any count.
        let plan = plan_serving(
            &host,
            std::slice::from_ref(&devstral),
            &ServingDemand::new(4, None))
        .unwrap();
        assert!(plan.fits_on_gpu, "{}", plan.rationale);
        assert_eq!(
            plan.lanes, 4,
            "roomy host gives each of the 4 resident personas its own warm slot"
        );
        let c = plan.served_context_window as u64;
        let lanes = plan.lanes as u64;
        let compute_floor = devstral.compute_buffer_per_lane();
        let compute_rate = devstral.kv_per_token / PREFILL_COMPUTE_KV_DIVISOR;
        let footprint = devstral.weights_bytes
            + devstral.kv_at(c as u32) * lanes
            + (compute_floor + compute_rate * c) * lanes;
        let effective = host.usable_bytes;
        assert!(
            footprint <= effective,
            "served window {c} overcommits: footprint {footprint} > effective {effective}"
        );
        // And it's strictly smaller than the compute-blind math would pick (weights + KV
        // filling the FULL usable budget) — the compute buffers demonstrably shrink it.
        let naive = (host.usable_bytes - devstral.weights_bytes) / lanes / devstral.kv_per_token;
        assert!(
            c < naive,
            "window-scaled compute must shrink the window ({c} < {naive})"
        );
    }

    // what this catches: the governor now SURFACES demand it can't serve locally instead
    // of silently cramming it into `lanes` and thrashing (the log-proven 65%-LRU / ~8s
    // KV-cache-swap-per-turn bug, 2026-07-27). Demand 4 > 2 local lanes → the excess 2 are
    // reported as `grid_overflow_lanes` — the honest "over local capacity by N" signal the
    // pooled governor routes to a grid peer (Σdemand vs Σresource, one decision; a single
    // box is the pool-of-one case). Demand within capacity → zero overflow. Regression for
    // #234/#56 — the good governor recognizing its own overload, never a silent clamp.
    #[test]
    pub(super) fn demand_over_local_capacity_surfaces_grid_overflow_not_silent_cram() {
        // 26GB: enough for the 14GB weights + 2 warm slots at a full-turn window, but NOT 4 —
        // the window floor (#266) caps warm slots at 2, so 2 of the 4 resident personas can't
        // get a persistent slot locally. That excess is the honest overflow, surfaced (not
        // crammed onto shared slots to thrash) for the governor to place off-box.
        let host = HostBudget {
            // 22.1 GB — what the old 15% plan reserve left of 26 GB; the pins below were
            // measured there (#4230: the plan sizes to the budget as handed).
            usable_bytes: 22_100_000_000,
            perf_cores: 10,
        };
        let devstral = fp("devstral-24b", 14, 112 * 1024, 131_072, 3);
        let over = plan_serving(
            &host,
            std::slice::from_ref(&devstral),
            &ServingDemand::new(4, None))
        .unwrap();
        assert_eq!(
            over.lanes, 2,
            "precondition: the full-turn window floor caps warm slots at 2 here"
        );
        assert_eq!(
            over.grid_overflow_lanes,
            4 - over.lanes,
            "demand the local warm slots couldn't absorb must be surfaced for grid placement, not crammed"
        );
        // Demand within local capacity → zero overflow (nothing to place off-box).
        let fits = plan_serving(
            &host,
            std::slice::from_ref(&devstral),
            &ServingDemand::new(1, None))
        .unwrap();
        assert_eq!(
            fits.lanes, 1,
            "precondition: single demand fits one local lane"
        );
        assert_eq!(
            fits.grid_overflow_lanes, 0,
            "demand ≤ local warm slots → no overflow"
        );
    }

    // what this catches: #266 — the slot count sizes to the RESIDENT PERSONA POPULATION so
    // each mind keeps a persistent warm slot (its prefilled KV survives across turns), clamped
    // by the full-turn window floor. The pre-fix `MAX_LANES = 2` clamp forced 4 resident minds
    // onto 2 slots, and the per-slot LRU eviction re-prefilled a cold ~10k prefix every turn
    // (measured: 96% prefill, two of four citizens at 0% cache reuse). Two branches, both pinned:
    //   (a) a budget that clears the floor at 4 slots → all 4 resident minds get a warm slot;
    //   (b) a budget that clears it at only 2 → 2 warm slots + the excess VISIBLE as overflow,
    //       never silently 4-crammed-onto-2 and never shrunk below the floor to fit more.
    #[test]
    pub(super) fn slots_size_to_resident_population_capped_by_the_full_turn_window_floor() {
        let devstral = fp("devstral-24b", 14, 112 * 1024, 131_072, 3);

        // (a) Roomy: 48GB clears the full-turn floor at 4 slots → a warm slot per resident mind,
        // zero overflow, no thrash. This is the win the `MAX_LANES = 2` clamp used to forbid.
        let roomy = HostBudget {
            usable_bytes: 48 * GB,
            perf_cores: 10,
        };
        let warm = plan_serving(
            &roomy,
            std::slice::from_ref(&devstral),
            &ServingDemand::new(4, None))
        .unwrap();
        assert_eq!(
            warm.lanes, 4,
            "roomy host: one warm slot per resident persona"
        );
        assert_eq!(
            warm.grid_overflow_lanes, 0,
            "all 4 minds hosted locally → nothing over-subscribed"
        );

        // (b) Floor-limited: 26GB clears the floor at only 2 slots. The plan yields 2 (never 4
        // crammed onto 2), surfaces the 2 unslotted minds as overflow (the non-silent
        // over-subscription signal a probe also names at the decision), and — critically — does
        // NOT drop the per-slot window below the floor to squeeze 4 in.
        let tight = HostBudget {
            // 22.1 GB — what the old 15% plan reserve left of 26 GB; the pins below were
            // measured there (#4230: the plan sizes to the budget as handed).
            usable_bytes: 22_100_000_000,
            perf_cores: 10,
        };
        let capped = plan_serving(
            &tight,
            std::slice::from_ref(&devstral),
            &ServingDemand::new(4, None))
        .unwrap();
        assert_eq!(
            capped.lanes, 2,
            "window floor caps warm slots at 2 — the honest ceiling, not a silent cram"
        );
        assert_eq!(
            capped.grid_overflow_lanes, 2,
            "the 2 minds that couldn't get a warm slot are SURFACED (probe + grid_overflow), never absorbed"
        );
        assert!(
            capped.served_context_window >= BOOTSTRAP_WORKING_SET,
            "each served slot keeps a full-turn window ({}) — the floor is never breached to fit more, got {}",
            BOOTSTRAP_WORKING_SET,
            capped.served_context_window,
        );
    }

    // what this catches: the cap that outlived its own TODO. `BOOTSTRAP_WORKING_SET`
    // was written as a cold-start PRIOR to be superseded by measurement ("measured p95
    // later, #234"); the measurement never arrived, so on a host that could serve 94k
    // of a 131k-capable model every citizen got 16384/lanes = **8192 tokens**, and
    // measured 2026-08-06 that left a median context budget of 55 after framing and
    // conversation — the work board reached a prompt 0 times in 495. A measured demand
    // ABOVE the bootstrap prior must now actually raise the served window, or the
    // constant is still silently in charge and this whole seam is decoration.
    #[test]
    pub(super) fn a_measured_demand_above_the_cold_start_prior_actually_raises_the_window() {
        let host = HostBudget {
            usable_bytes: 48 * GB,
            perf_cores: 10,
        };
        let devstral = fp("devstral-24b", 14, 112 * 1024, 131_072, 3);

        let cold = plan_serving(
            &host,
            std::slice::from_ref(&devstral),
            &ServingDemand::new(1, None))
        .expect("servable");
        assert_eq!(
            cold.served_context_window, BOOTSTRAP_WORKING_SET,
            "with NO measurement the cold-start prior is the honest answer"
        );

        // One mind measured wanting 48k — well past the prior, well under the ceiling.
        let measured = plan_serving(
            &host,
            std::slice::from_ref(&devstral),
            &ServingDemand::new(1, Some(48_000)))
        .expect("servable");
        assert!(
            measured.served_context_window > cold.served_context_window,
            "measured demand ({}) must RAISE the window above the cold-start prior ({}), \
             got {} — if this is equal, the constant is still the authority",
            48_000,
            cold.served_context_window,
            measured.served_context_window
        );
        assert!(
            measured.served_context_window <= 48_000,
            "…but never above what was actually demanded (got {})",
            measured.served_context_window
        );
    }

    // what this catches (the M5, 2026-09-19 23:14Z): the decode knee clamped the ROSTER
    // (17 → 2) and then `leased_in` was added on top, so the plan served 3 slots against a
    // knee of 2 — "AT THE KNEE: 16 minds on 3 lanes, the measured decode knee is 2" in the
    // [health] hour. The knee is a property of the box's decode and bounds every stream it
    // decodes, leased-in included. Same roomy host and model as the test below: 1 resident
    // + 3 leased-in is 4 slots without a knee and 2 with a knee of 2; a knee never RAISES.
    #[test]
    pub(super) fn the_knee_bounds_the_served_total_leased_in_included() {
        let roomy = HostBudget {
            usable_bytes: 48 * GB,
            perf_cores: 10,
        };
        let devstral = fp("devstral-24b", 14, 112 * 1024, 131_072, 3);
        let unclamped = plan_serving(
            &roomy,
            std::slice::from_ref(&devstral),
            &ServingDemand::new(1, None).with_leased_in(3))
        .expect("servable");
        assert_eq!(unclamped.lanes, 4, "no knee: four minds served, four slots");
        let kneed = plan_serving(
            &roomy,
            std::slice::from_ref(&devstral),
            &ServingDemand::new(1, None).with_leased_in(3).with_knee(Some(2)))
        .expect("servable");
        assert_eq!(kneed.lanes, 2, "a knee of 2 bounds the served total, leased-in included");
        let roomy_knee = plan_serving(
            &roomy,
            std::slice::from_ref(&devstral),
            &ServingDemand::new(1, None).with_leased_in(3).with_knee(Some(8)))
        .expect("servable");
        assert_eq!(roomy_knee.lanes, 4, "a knee above the demand changes nothing");
    }

    // what this catches (card c84d885a, 2026-09-18): a seat that plans for the minds it
    // HOSTS and not the minds it SERVES. The 5090's 27B seat had one resident of its own and
    // served twelve coders leased in from the M5 and IntelMac — on one slot, because the
    // plan's lane demand was its roster. A seat with a roomy budget and 3 leased-in minds
    // beside 1 resident must plan 4 warm slots, exactly as it would for 4 residents; and a
    // solve-lease override that pins resident demand to 1 must not erase the leased-in three.
    #[test]
    pub(super) fn a_seat_plans_for_the_minds_it_serves_not_only_the_minds_it_hosts() {
        let roomy = HostBudget {
            usable_bytes: 48 * GB,
            perf_cores: 10,
        };
        let devstral = fp("devstral-24b", 14, 112 * 1024, 131_072, 3);
        let hosts_only = plan_serving(
            &roomy,
            std::slice::from_ref(&devstral),
            &ServingDemand::new(1, None))
        .expect("servable");
        assert_eq!(hosts_only.lanes, 1, "one resident, nothing leased in: one slot");

        let serves = plan_serving(
            &roomy,
            std::slice::from_ref(&devstral),
            &ServingDemand::new(1, None).with_leased_in(3))
        .expect("servable");
        assert_eq!(
            serves.lanes, 4,
            "one resident plus three leased-in minds is FOUR minds served: four warm slots"
        );
        assert_eq!(serves.grid_overflow_lanes, 0, "the roomy host seats all four");

        // the same seat, read as if a solve lease had overridden resident demand to 1
        // (the 5090's shape): the leased-in three are still served here and still count
        let overridden_but_leased = plan_serving(
            &roomy,
            std::slice::from_ref(&devstral),
            &ServingDemand::new(1, Some(40_000)).with_leased_in(3))
        .expect("servable");
        assert_eq!(overridden_but_leased.lanes, 4);

        // a tight host still says so honestly: it plans what fits and names the rest
        let tight = HostBudget {
            // 22.1 GB — what the old 15% plan reserve left of 26 GB; the pins below were
            // measured there (#4230: the plan sizes to the budget as handed).
            usable_bytes: 22_100_000_000,
            perf_cores: 10,
        };
        let capped = plan_serving(
            &tight,
            std::slice::from_ref(&devstral),
            &ServingDemand::new(1, None).with_leased_in(3))
        .expect("servable");
        assert_eq!(capped.lanes, 2, "the floor caps warm slots at 2 on 26 GB");
        assert_eq!(
            capped.grid_overflow_lanes, 2,
            "two of the four minds this seat serves are visibly unslotted, never silently crammed"
        );
    }

    // what this catches: the other direction — demand is a CAP, not a request the host
    // must honor. A mind that wants more than the machine has must receive what fits,
    // never a window the host cannot back with real KV, which is the swap/wedge the
    // demand cap exists to prevent in the first place.
    #[test]
    pub(super) fn demand_beyond_the_host_is_bounded_by_what_actually_fits() {
        let host = HostBudget {
            usable_bytes: 48 * GB,
            perf_cores: 10,
        };
        let devstral = fp("devstral-24b", 14, 112 * 1024, 131_072, 3);
        let greedy = plan_serving(
            &host,
            std::slice::from_ref(&devstral),
            &ServingDemand::new(4, Some(10_000_000)))
        .expect("servable");
        assert!(
            greedy.served_context_window <= devstral.context_window,
            "never above the model's trained ceiling (got {})",
            greedy.served_context_window
        );
        let unbounded_fit = plan_serving(
            &host,
            std::slice::from_ref(&devstral),
            &ServingDemand::new(4, Some(u32::MAX)))
        .expect("servable");
        assert_eq!(
            greedy.served_context_window, unbounded_fit.served_context_window,
            "past the host's real fit, MORE demand changes nothing — the fit is the bound"
        );
    }

    // what this catches: the cross-node serving-QUALITY bug (2026-07-26). On a roomy
    // host `window_for` maximizes the served window to fill the memory budget — tens
    // of thousands of tokens (the pre-fix devstral shape sized ~53k; live it was 94k),
    // pre-allocating ~33GB of KV × lanes → SWAP on a 64GB Mac (5M swapouts, 1.77 tok/s)
    // and DECODE-WEDGE on a 32GB 5090 — while personas fill only ~9k. The DEMAND cap
    // (BOOTSTRAP_WORKING_SET now, measured p95 later, #234) must hold the served window
    // at the working set, provisioned for USE not for RAM. The ceiling here (131k) is
    // far above the cap, so a served window ≤ BOOTSTRAP proves the DEMAND cap held it
    // (not the ceiling). Regression for M5+BigMama's 94k→swap/wedge.
    #[test]
    pub(super) fn demand_cap_holds_served_window_at_working_set_not_budget_max() {
        let host = HostBudget {
            usable_bytes: 48 * GB,
            perf_cores: 10,
        };
        let devstral = fp("devstral-24b", 14, 112 * 1024, 131_072, 3);
        assert!(
            devstral.context_window > BOOTSTRAP_WORKING_SET,
            "precondition: model ceiling must exceed the demand cap, else the ceiling (not the cap) could explain the result"
        );
        let plan = plan_serving(
            &host,
            std::slice::from_ref(&devstral),
            &ServingDemand::new(4, None))
        .unwrap();
        assert!(
            plan.served_context_window <= BOOTSTRAP_WORKING_SET,
            "served window {} must be capped to working-set demand ({}), never budget-maxed toward the ceiling",
            plan.served_context_window,
            BOOTSTRAP_WORKING_SET
        );

        // The cap NEVER inflates a small-ceiling model UP toward BOOTSTRAP — a model
        // whose trained ceiling is below the demand cap is still served at/below its
        // own ceiling (`.min` only ever caps DOWN).
        let tiny = fp("tiny-4k", 3, 30_000, 4_096, 2);
        let plan_tiny = plan_serving(
            &host,
            std::slice::from_ref(&tiny),
            &ServingDemand::new(1, None))
        .unwrap();
        assert!(
            plan_tiny.served_context_window <= 4_096,
            "demand cap must not inflate a 4k-ceiling model past its ceiling; got {}",
            plan_tiny.served_context_window
        );
    }

mod part_2;
mod part_3;
