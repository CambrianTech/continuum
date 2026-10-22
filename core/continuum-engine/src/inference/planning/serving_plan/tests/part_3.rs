#![cfg(test)]
use super::*;

    // what this catches: the credit-back turning into a thrash engine. Crediting eviction must
    // NOT hand the upgrade gate a blank cheque — a marginally-fitting or no-better model still
    // has to lose, or the planner swaps models on budget noise and every flip bounces the live
    // lane under in-flight requests (the wedge that killed three benchmark runs).
    #[test]
    pub(super) fn the_eviction_credit_still_refuses_a_marginal_or_no_better_swap() {
        let live = HostBudget {
            usable_bytes: 18 * GB,
            perf_cores: 10,
        };

        // (a) Equal capability → never swap, however well it fits.
        let equal = vec![
            fp("devstral-24b", 14, 100_000, 131_072, 5),
            fp("sibling-24b", 14, 100_000, 131_072, 5),
        ];
        assert_eq!(
            plan_serving_stable(&live, &equal, Some("devstral-24b"), &ServingDemand::new(2, None))
                .unwrap()
                .base_model
                .model_id,
            "devstral-24b",
            "equal capability is not an upgrade — the incumbent holds"
        );

        // (b) More capable but it consumes the ENTIRE post-eviction budget, leaving nothing
        // for KV — SWITCH_UP_HEADROOM must reject it rather than swap into a lane that
        // cannot hold a window.
        let marginal = vec![
            fp("devstral-24b", 14, 100_000, 131_072, 5),
            fp("hog-70b", 32, 100_000, 262_144, 9),
        ];
        assert_eq!(
            plan_serving_stable(
                &live,
                &marginal,
                Some("devstral-24b"),
                &ServingDemand::new(2, None))
            .unwrap()
            .base_model
            .model_id,
            "devstral-24b",
            "a better model with no headroom left is refused — the credit is not a blank cheque"
        );
    }

    // what this catches: the served window re-keying the KV page geometry every boot
    // (2026-09-13: four windows in four boots on one host + model; no page survived a
    // reboot). A remembered window that fits and covers the demand is served verbatim;
    // one the demand has outgrown, or that no longer fits, gives way to the demand.
    #[test]
    pub(super) fn the_served_window_holds_still_across_boots_while_it_covers_the_demand() {
        assert_eq!(choose_served_window(138_240, 90_000, Some(102_656)), 102_656, "sticky fits and covers: served verbatim");
        assert_eq!(choose_served_window(65_536, 90_000, Some(102_656)), 65_536, "sticky no longer fits: the fit bounds the demand");
        assert_eq!(choose_served_window(138_240, 110_000, Some(102_656)), 110_000, "demand outgrew the sticky window: serve the demand (pages re-key once)");
        assert_eq!(choose_served_window(138_240, 90_000, None), 90_000, "no memory: the demand, as before");
        assert_eq!(choose_served_window(138_240, 100, None), MIN_SERVE_CTX, "never below the runnable floor");
        let host = HostBudget { usable_bytes: 48 * GB, perf_cores: 10 };
        let m = fp("m", 14, 112 * 1024, 131_072, 3);
        let first = plan_serving(&host, std::slice::from_ref(&m), &ServingDemand::new(4, Some(40_000))).unwrap();
        let again = plan_serving(&host, std::slice::from_ref(&m), &ServingDemand::new(4, Some(33_000)).with_sticky_window(Some(first.served_context_window))).unwrap();
        assert_eq!(again.served_context_window, first.served_context_window, "the next boot, with demand drifted DOWN, serves the SAME window — the pages carry over");
    }

    // what this catches: the window sized from the untrimmed context (225k–505k measured
    // 2026-09-13) instead of what turns actually send — a 25k working set provisioned a
    // 137k slot, starving the lane count and the RAM prompt-cache tier, and the sticky
    // window could never "cover" a demand that large.
    // what this catches (2026-09-17, the 5090): the ONE window arithmetic, callable from
    // the launch path. A 144,640 window kept by the sticky / wedge-heal laws must not
    // survive onto a budget that fits ~57k; the same method the plan sizes with says so.
    // what this catches (2026-09-17, the 5090): the daemon's budget is the ledger's
    // replace-myself budget — ALREADY at rest — and `plan_serving_stable` credited the
    // incumbent's weights a second time, planning 2 × 70,093 on a card whose fit was
    // 2 × 47,098. The re-home law then relaunched the lane three times mid-solve
    // chasing a window the launch bound kept turning back into the live one. At rest,
    // the plan's window IS the fit: no shortfall exists to chase.
    #[test]
    pub(super) fn an_at_rest_plan_never_asks_for_more_window_than_the_fit() {
        let m = ModelFootprint {
            model_id: "qwen3.8-27b".into(),
            weights_bytes: 17 * GB,
            kv_per_token: 65_536,
            context_window: 262_144,
            capability_rank: 9,
            fixed_per_lane_bytes: 0,
        };
        let at_rest = HostBudget {
            usable_bytes: 32 * GB, // the ledger's replace-myself budget, capped at the card
            perf_cores: 8,
        };
        let demand = ServingDemand::new(2, Some(120_000));
        let plan = plan_serving_at_rest(
            &at_rest,
            std::slice::from_ref(&m),
            Some("qwen3.8-27b"),
            &demand)
        .expect("plan");
        let fit = m.window_within(at_rest.usable_bytes, plan.lanes);
        assert_eq!(plan.served_context_window, fit, "at rest, the plan's window is the fit");
        // The live-budget path still credits: handed the same number as a LIVE reading it
        // plans a bigger window — which is exactly why the daemon must not use it.
        let credited = plan_serving_stable(
            &at_rest,
            std::slice::from_ref(&m),
            Some("qwen3.8-27b"),
            &demand)
        .expect("plan");
        assert!(credited.served_context_window > plan.served_context_window);
    }

    #[test]
    pub(super) fn window_within_is_the_plans_own_arithmetic_and_bounds_a_kept_window() {
        let m = ModelFootprint {
            model_id: "qwen3.8-27b".into(),
            weights_bytes: 17 * GB,
            kv_per_token: 262_144,
            context_window: 262_144,
            capability_rank: 9,
            fixed_per_lane_bytes: 0,
        };
        let physical = HostBudget {
            usable_bytes: (33.6 * GB as f64 * 0.80) as u64,
            perf_cores: 8,
        };
        let fits = m.window_within(physical.usable_bytes, 1);
        assert!(fits > MIN_SERVE_CTX && fits < 144_640, "fits={fits}");
        // The plan's own choice at that budget IS window_within (the closure delegates).
        let plan = plan_serving(&physical, std::slice::from_ref(&m), &ServingDemand::new(1, Some(200_000)))
            .expect("plan");
        assert_eq!(plan.served_context_window, fits);
        assert_eq!(m.window_within(physical.usable_bytes, 0), 0);
        let kv_free = ModelFootprint { kv_per_token: 0, ..m.clone() };
        assert_eq!(kv_free.window_within(physical.usable_bytes, 1), kv_free.context_window);
    }

    #[test]
    pub(super) fn the_window_follows_the_sent_prompt_with_headroom_and_the_sticky_window_can_cover_it() {
        let d = ServingDemand::new(4, Some(505_342)).with_sent_tokens(Some(30_000));
        assert_eq!(d.window_target(), 37_500, "sent × 1.25, not the untrimmed 505k");
        // The M5's boot, 2026-09-19 19:37Z: peak 144,891 (one prompt, ever), typical 51,242.
        let m5 = ServingDemand::new(5, Some(505_342)).with_sent_tokens(Some(144_891)).with_sent_median(Some(51_242));
        assert_eq!(m5.window_target(), 64_052, "the typical prompt with headroom, not the outlier's 181,114");
        assert_eq!(choose_served_window(505_342, 64_052, Some(67_584)), 67_584, "and the remembered 67,584 covers it — served verbatim, no boot relaunch");
        assert_eq!(ServingDemand::new(4, Some(505_342)).window_target(), 505_342, "unmeasured sent: the untrimmed demand, as before");
        assert_eq!(ServingDemand::new(4, Some(20_000)).with_sent_tokens(Some(30_000)).window_target(), 20_000, "the untrimmed demand is the upper bound");
        assert_eq!(choose_served_window(137_222, 37_500, Some(47_280)), 47_280, "the remembered 47k window covers a 37.5k target: served verbatim — pages carry over");
        let host = HostBudget { usable_bytes: 48 * GB, perf_cores: 10 };
        let m = fp("m", 14, 112 * 1024, 131_072, 3);
        let plan = plan_serving(&host, std::slice::from_ref(&m), &d).unwrap();
        assert!(plan.served_context_window <= 37_500 + 4_096 && plan.served_context_window >= 37_500 - 4_096, "served near the target: {}", plan.served_context_window);
    }

    // what this catches (card eb8d1839; the 5090, 2026-10-12, build 6160): a one-lane plan that
    // could not say WHICH term chose one lane. `bound_by=host-fit` meant only "the window came out
    // below demand", so the GPU accounting, the per-lane floor and a boot's memory all read alike.
    // The plan now carries the term from its own decision: the floor when one more lane would
    // starve each window, demand when every lane was granted, cores when the host capped it, and
    // the host core budget when it limits lanes.
    #[test]
    pub(super) fn a_plan_names_the_term_that_chose_its_lane_count() {
        const GB: u64 = 1_000_000_000;
        let model = ModelFootprint {
            model_id: "native-27b".into(),
            weights_bytes: 16 * GB,
            kv_per_token: 32_768,
            context_window: 262_144,
            capability_rank: 42,
            fixed_per_lane_bytes: 922_859_520,
        };
        let models = std::slice::from_ref(&model);
        // A typical prompt of ~60k: two lanes on a tight host cannot each clear that floor.
        let demand = ServingDemand::new(2, Some(60_000)).with_sent_median(Some(60_000));
        let tight = HostBudget { usable_bytes: 23 * GB, perf_cores: 8 };
        let plan = plan_serving(&tight, models, &demand).expect("plan");
        assert_eq!(plan.lanes, 1, "{}", plan.rationale);
        match plan.lanes_bound {
            LanesBound::PerLaneFloor { floor_tokens, floor_term, rejected_lanes, rejected_window } => {
                assert_eq!(rejected_lanes, 2, "the candidate it rejected was a second lane");
                assert!(rejected_window < floor_tokens, "and that lane's window fell below the floor: {rejected_window} < {floor_tokens}");
                let typical = demand.typical_prompt_floor().expect("a sent median");
                assert_eq!(floor_tokens, typical.max(BOOTSTRAP_WORKING_SET));
                assert_eq!(floor_term, FloorTerm::TypicalPrompt, "the typical prompt set the floor");
            }
            other => panic!("one lane on a tight host is the floor's decision, not {other:?}"),
        }
        // A roomy host grants every demanded lane.
        let roomy = HostBudget { usable_bytes: 200 * GB, perf_cores: 8 };
        let plan = plan_serving(&roomy, models, &demand).expect("plan");
        assert_eq!((plan.lanes, plan.lanes_bound), (2, LanesBound::Demand));
        // One performance core caps it below demand.
        let one_core = HostBudget { usable_bytes: 200 * GB, perf_cores: 1 };
        let plan = plan_serving(&one_core, models, &demand).expect("plan");
        assert_eq!((plan.lanes, plan.lanes_bound), (1, LanesBound::Cores));
    }
