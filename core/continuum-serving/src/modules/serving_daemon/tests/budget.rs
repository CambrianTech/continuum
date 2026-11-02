#![cfg(test)]

use super::*;

// what this catches (card c3c50e0d): a refused demotion with NO previous plan must
// still produce a plan for the inherited engine — the incumbent alone, at its running
// geometry, against the physical budget — never nothing. An incumbent that is not a
// candidate, or cannot fit one lane physically, is a named None.
#[test]
fn a_refused_demotion_with_no_plan_adopts_the_inherited_engine_at_its_geometry() {
    use super::incumbent_adoption_plan;
    use crate::inference::planning::serving_plan::{HostBudget, ModelFootprint, ServingDemand};
    const GB: u64 = 1_000_000_000;
    let qwen27b = ModelFootprint {
        model_id: "ggml-org/Qwen3.8-27B-GGUF".into(),
        weights_bytes: 17 * GB,
        kv_per_token: 55_000,
        context_window: 131_072,
        capability_rank: 18,
        fixed_per_lane_bytes: 0,
    };
    let small = ModelFootprint {
        model_id: "continuum-ai/qwen2.5-coder-14b-instruct-GGUF".into(),
        weights_bytes: 9 * GB,
        kv_per_token: 30_000,
        context_window: 32_768,
        capability_rank: 11,
        fixed_per_lane_bytes: 0,
    };
    let physical = HostBudget {
        usable_bytes: 44 * GB,
        perf_cores: 12,
    };
    let demand = ServingDemand::new(2, Some(30_000));
    let plan = incumbent_adoption_plan(
        &physical,
        &[small.clone(), qwen27b.clone()],
        &qwen27b.model_id,
        &demand,
        Some((41_728, 2)),
    )
    .expect("the incumbent fits physically");
    assert_eq!(
        plan.base_model.model_id, qwen27b.model_id,
        "the incumbent, never the smaller candidate"
    );
    assert!(plan.lanes >= 1 && plan.served_context_window >= 8_192);
    assert!(
        incumbent_adoption_plan(&physical, &[small], &qwen27b.model_id, &demand, None).is_none(),
        "an incumbent that is not a candidate is a named None"
    );
}

// what this catches (card 628dc958): the plan grew into free memory while the host
// was pushing other processes' pages out to make it, then fed on the memory its own
// relaunch freed — six relaunches in 75 min on the M5 at HIGH. Pure decision.
#[test]
fn a_plan_never_grows_while_the_host_pages_out() {
    use super::grow_refused_while_paging;
    let gib = |n: u64| n << 30;
    // paging out + a bigger plan = refused; flat or falling swap, a shrink, or no
    // previous reading (boot on a node that always has swap) flows.
    assert!(!grow_refused_while_paging(
        80_000,
        120_000,
        u64::MAX,
        gib(2)
    ));
    assert!(grow_refused_while_paging(80_000, 120_000, gib(1), gib(2)));
    assert!(!grow_refused_while_paging(80_000, 120_000, gib(2), gib(2)));
    assert!(!grow_refused_while_paging(80_000, 120_000, gib(2), gib(1)));
    assert!(
        !grow_refused_while_paging(120_000, 80_000, gib(1), gib(2)),
        "a shrink while paging is the remedy, never refused"
    );
}

// what this catches: a pin SWAPS — serve() kills the incumbent llama-server
// child, THEN launches the candidate (never co-resident) — so the fit-check must
// credit the outgoing model's weights back into the budget. Without it, swapping
// DOWN to a model that fits ALONE but not ALONGSIDE the incumbent is falsely
// denied (glass-boxed 2026-07-21: pin Devstral 14GB refused while a 20GB 32B
// teacher was resident, though evicting it frees enough). Regression for the
// stronger-teacher swap-and-back the Academy needs.
// what this catches: the swap/restore interleaving bug in the measurement demand
// override (regression for the #2238 follow-up). With TWO overlapping quiesce
// leases, releasing in ACQUISITION order under the old swap/restore scheme first
// restored the pre-lease base over the still-held second lease, then restored the
// FIRST lease's override — leaving the fleet's warm-slot demand stuck at the
// measurement value with nobody quiesced. The authority must recompute from what
// remains: any-order release, base restored only when the LAST override lifts.
#[test]
// what this catches: 16 resident minds planned for a boot floor of 2 (3 lanes with
// 40 GB idle, 2026-09-14) — the roster IS the demand; the floor only bounds it
// from below; a measurement lease still holds it down while held.
fn the_lane_demand_follows_the_live_roster_above_the_boot_floor() {
    assert_eq!(resident_lane_demand(2, 16, false), 16);
    assert_eq!(
        resident_lane_demand(16, 5, false),
        16,
        "the floor is a lower bound"
    );
    assert_eq!(
        resident_lane_demand(2, 0, false),
        2,
        "no residents yet = the floor"
    );
    assert_eq!(
        resident_lane_demand(2, 16, true),
        2,
        "a held quiesce lease wins"
    );
    assert_eq!(resident_lane_demand(0, 0, false), 1);
}

#[test]
fn pin_swap_down_credits_the_evicted_incumbents_weights() {
    let base = HostBudget {
        usable_bytes: 12 * GB,
        perf_cores: 10,
    };
    let footprint = |id: &str, weights_gb: u64, rank: u8| ModelFootprint {
        model_id: id.into(),
        weights_bytes: weights_gb * GB,
        kv_per_token: 100_000, // ~0.2GB KV at 2048 ctx — small, not the binding term
        context_window: 32768,
        capability_rank: rank,
        fixed_per_lane_bytes: 0,
    };
    let candidate = footprint("devstral-24b", 14, 8);
    let incumbent = footprint("qwen-32b", 20, 10);

    // No incumbent credited → the 14GB candidate does NOT fit the raw 12GB budget.
    let no_credit = pin_fit_decision(base, Some(candidate.clone()), None);
    assert!(
        no_credit.plan.map(|p| !p.fits_on_gpu).unwrap_or(true),
        "candidate must not fit the raw 12GB budget"
    );

    // Crediting the evicted 20GB incumbent lifts the budget to 32GB → it fits.
    let credited = pin_fit_decision(base, Some(candidate), Some(&incumbent));
    assert_eq!(
        credited.budget_bytes,
        12 * GB + 20 * GB,
        "the evicted incumbent's weights are credited back into the pin budget"
    );
    assert!(
        credited.plan.expect("a plan is produced").fits_on_gpu,
        "candidate must fit its lane once the incumbent is evicted"
    );
}

// what this catches: the ServingSteadyHold RAII gauge — acquiring sets
// serving_held_steady() true; dropping clears it; nesting is reference-counted so a
// concurrent second eval doesn't release the first's hold. This is the gate that stops
// the grow-back re-home from relaunching the lane under a running eval (hard-rs 0/8).
// regression for the 2026-07-20 shared-lane bounce.
// what this catches (2026-09-17, the M5): a steady hold pinning a boot shape of
// 8 × 12,800 against a plan of 3 × 67,340 for 255 ticks. The hold yields to a
// shortfall of REHOME_HOLD_OVERRIDE_PCT or more; a few-percent grow-back stays held.
// what this catches: a machine-specific constant sizing every plan. The plan fills the mode's
// fraction of the sustained budget, the same on unified and discrete memory; the M5's 2026-10-29
// plan withheld a flat 12 GiB on top (three lanes for four residents, 17 s median slot wait).
#[test]
fn a_plan_fills_the_modes_fraction_and_nothing_flat() {
    use crate::capacity::power_mode::PowerMode;
    for live in [20_000_000_000u64, 40_300_000_000, 120_000_000_000] {
        for mode in [PowerMode::Eco, PowerMode::Comfort, PowerMode::Performance] {
            let expected = (live as f64 * mode.serving_fraction()) as u64;
            assert_eq!(plan_fill(live, mode.serving_fraction()), expected, "{live} {mode:?}");
        }
    }
}

// what this catches (#4230, BigMama's condition): with the plan's own reserve gone, a
// foundry's headroom of 1.0 must still stop at what Performance keeps — the serving
// process's fixed overhead is not in the footprint, and 100% of the ceiling is an OOM.
#[test]
fn a_foundry_headroom_of_one_still_keeps_the_process_overhead() {
    use crate::capacity::power_mode::PowerMode;
    assert_eq!(
        plan_fraction(1.0),
        PowerMode::Performance.serving_fraction()
    );
    assert!(plan_fraction(1.0) < 1.0);
    assert_eq!(
        plan_fraction(0.8),
        0.8,
        "the everyday default is the operator's own number"
    );
}

#[test]
fn a_steady_hold_yields_to_a_large_shortfall_and_keeps_a_small_one() {
    let (live, _plan, gain, _) = rehome_gain_evidence(12_800, 8, 67_340, 3);
    assert!(
        hold_overridden_by_shortfall(live, gain),
        "8 × 12.8k vs 3 × 67k is starvation, not a slot to protect"
    );
    let (live, _plan, gain, worth) = rehome_gain_evidence(56_000, 5, 67_000, 5);
    assert!(worth, "a ~20% grow-back is worth a re-home in general");
    assert!(
        !hold_overridden_by_shortfall(live, gain),
        "but not worth dropping a held slot for"
    );
    assert!(
        !hold_overridden_by_shortfall(0, 1),
        "no live space = nothing to compare"
    );
}

#[test]
fn serving_steady_hold_is_refcounted_and_raii() {
    assert!(!serving_held_steady(), "no hold at rest");
    {
        let _h1 = ServingSteadyHold::acquire("test");
        assert!(serving_held_steady(), "one hold ⇒ steady");
        {
            let _h2 = ServingSteadyHold::acquire("test");
            assert!(serving_held_steady(), "two holds ⇒ still steady");
        }
        assert!(
            serving_held_steady(),
            "inner drop must not release the outer hold"
        );
    }
    assert!(
        !serving_held_steady(),
        "all holds dropped ⇒ grow-back resumes"
    );
}

// what this catches: the ServingLudicrousHold RAII gauge — while held, serving plans at
// PowerMode::Performance (the whole GPU, biggest window), overriding the timid pressure
// read; refcounted so concurrent benchmarks compose; RAII-released so serving reverts to
// the live pressure-adaptive mode. This is the "extreme mode for benchmarks" gate that
// stops a starved eco boot-window from being what the exam is measured on.
// [[serving-mode-follows-activity-ludicrous-to-dream]]
#[test]
fn serving_ludicrous_hold_is_refcounted_and_raii() {
    assert!(!serving_ludicrous_active(), "no ludicrous demand at rest");
    {
        let _h1 = ServingLudicrousHold::acquire();
        assert!(
            serving_ludicrous_active(),
            "one hold ⇒ Performance override active"
        );
        {
            let _h2 = ServingLudicrousHold::acquire();
            assert!(serving_ludicrous_active(), "two holds ⇒ still active");
        }
        assert!(
            serving_ludicrous_active(),
            "inner drop must not release the outer hold"
        );
    }
    assert!(
        !serving_ludicrous_active(),
        "all holds dropped ⇒ back to pressure-adaptive mode"
    );
}

// what this catches (card 48f5438a): a below-floor plan is refused ONLY when there
// is an incumbent to preserve. A COLD BOOT (no incumbent) must SERVE the plan rather
// than refuse into a gap and boot the node DARK — measured on the Intel tier
// 2026-09-17: #4169 without #4174 sized a CPU host below its capability floor, the
// gate refused into "the previous plan stands" with no previous plan, and the node
// reported resident 0 / active_model None while a servable 1.5B sat on disk. The
// floor guards SINKING from an incumbent; on a cold boot there is nothing to sink
// from. Against the pre-fix unconditional refuse the cold-boot assert fails.
#[test]
fn a_cold_boot_below_the_floor_serves_rather_than_booting_dark() {
    let below = FloorVerdict::Below {
        floor: "qwen2.5-coder-4b".to_string(),
        wanted: "qwen2.5-coder-1.5b".to_string(),
    };
    use std::time::Duration;
    let grace = COLD_BOOT_BELOW_FLOOR_GRACE;
    // Below the floor WITH an incumbent → refuse, however long (a host never sinks).
    assert!(
        below_floor_refuses(&below, true, Duration::ZERO),
        "with an incumbent, hold the floor"
    );
    assert!(
        below_floor_refuses(&below, true, grace * 10),
        "…for as long as it takes"
    );
    // Below the floor with NO incumbent: a fresh reading is a teardown transient →
    // refuse (the 5090's 53 GB card must not latch a 4B on one 6 GB reading)…
    assert!(
        below_floor_refuses(&below, false, Duration::ZERO),
        "a first below-floor reading with nothing serving is a transient, not a shortfall"
    );
    // …and a shortfall that has outlived the grace → SERVE, never stay dark.
    assert!(
        !below_floor_refuses(&below, false, grace),
        "a cold boot held below the floor past the grace serves the viable plan (48f5438a)"
    );
    // At or above the floor never refuses, incumbent or not.
    assert!(!below_floor_refuses(
        &FloorVerdict::AtOrAbove,
        true,
        Duration::ZERO
    ));
    assert!(!below_floor_refuses(&FloorVerdict::AtOrAbove, false, grace));
}

// what this catches: regression #4146 on the Intel tier — a CPU-placed host
// (`--n-gpu-layers 0`) sized against its decorative GPU instead of the system RAM
// the KV actually lives in, which bound the kept window to MIN_SERVE_CTX (2,048),
// hard-refused every ~2,300-token prompt and produced 36/36 inference_failed with
// ZERO acts across eight citizens. The CPU arm must read RAM and the GPU arm must
// read VRAM; a VRAM-only implementation (the pre-fix code) fails the first assert.
#[test]
fn physical_capacity_follows_the_placement_not_the_installed_device() {
    use crate::inference::llama_server::LanePlacement;
    // The measured Intel tier: 3.89 GB Radeon Pro 560X at used_mb 0.0, 32 GB RAM.
    let vram = 3_891_000_000u64;
    let ram = 32 * GB;

    let cpu = physical_usable_bytes(LanePlacement::Cpu, vram, ram, 0.80);
    let gpu = physical_usable_bytes(LanePlacement::Gpu, vram, ram, 0.80);

    // THE REGRESSION ASSERT: a CPU-placed host must not be bounded by a card that
    // holds zero bytes. Pre-fix both arms returned the VRAM figure, so cpu == gpu.
    assert!(
        cpu > gpu,
        "CPU placement must size against RAM, not the idle card: cpu={cpu} gpu={gpu}"
    );
    assert_eq!(cpu, (ram as f64 * 0.80) as u64, "CPU arm reads system RAM");
    assert_eq!(gpu, (vram as f64 * 0.80) as u64, "GPU arm still reads VRAM");

    // And the fix must not hand a GPU host its (often larger) RAM figure — that
    // would over-commit the card, which is the 2026-07-20 error in reverse.
    assert!(
        gpu < ram,
        "GPU placement must stay bounded by the device: {gpu} vs {ram} RAM"
    );
}

// what this catches: the budget is LIVE (tracks free memory, not capacity),
// capped at physical VRAM, with headroom, cores floored at 1 — the organic
// "free memory drops → budget drops" behavior.
#[test]
fn host_budget_tracks_live_free_memory() {
    // 40GB free on a 53GB device → budget from the 40, with headroom.
    let b = host_budget_from(&HostBudgetInputs {
        available_bytes: 40 * GB,
        total_vram_bytes: 53 * GB,
        perf_cores: 6,
        budget_fraction: 0.80,
    });
    assert!(b.usable_bytes < 40 * GB, "must reserve headroom");
    assert!(
        b.usable_bytes >= 30 * GB,
        "but most of free is ours: {}",
        b.usable_bytes
    );

    // Organic: less free memory → smaller budget (a game grabbed memory).
    let busy = host_budget_from(&HostBudgetInputs {
        available_bytes: 6 * GB,
        total_vram_bytes: 53 * GB,
        perf_cores: 6,
        budget_fraction: 0.80,
    });
    assert!(
        busy.usable_bytes < b.usable_bytes,
        "less free → smaller budget"
    );

    // Never plan above physical VRAM even if the OS reports more free RAM
    // (unified memory: free RAM can exceed the VRAM serving ceiling).
    let capped = host_budget_from(&HostBudgetInputs {
        available_bytes: 100 * GB,
        total_vram_bytes: 53 * GB,
        perf_cores: 6,
        budget_fraction: 0.80,
    });
    assert!(capped.usable_bytes <= 53 * GB, "capped at physical VRAM");

    assert_eq!(
        host_budget_from(&HostBudgetInputs {
            available_bytes: 8 * GB,
            total_vram_bytes: 8 * GB,
            perf_cores: 0,
            budget_fraction: 0.80,
        })
        .perf_cores,
        1,
        "cores floored at 1"
    );
}

// what this catches (#175 sticky window): the LoRA-load relaunch cascade must not
// ratchet the per-slot window down and strand earlier-pinned personas. A same-lane
// genome relaunch KEEPS the larger incumbent window; a lane-count change lets the
// plan window through (resizing KV is legitimate; keeping the old window across a
// lane increase would OOM); a larger plan window is never held down (the
// starved-grow check owns growing); nothing-serving-yet → the plan window stands.
#[test]
fn sticky_window_holds_the_incumbent_only_on_a_same_lane_relaunch() {
    let live = |ready: bool, window: u32, lanes: u32| ServingSnapshot {
        ready,
        served_context_window: window,
        lanes,
        ..ServingSnapshot::empty()
    };
    assert_eq!(
        sticky_served_window(31_744, 2, &live(true, 49_664, 2)),
        49_664
    );
    assert_eq!(
        sticky_served_window(31_744, 2, &live(true, 49_664, 1)),
        31_744
    );
    assert_eq!(
        sticky_served_window(60_000, 2, &live(true, 49_664, 2)),
        60_000
    );
    assert_eq!(sticky_served_window(31_744, 2, &live(false, 0, 0)), 31_744);
}

// what this catches: THE #438 boot downshift, measured live 2026-08-19. A successor core
// boots while its predecessor's 27B still holds ~45 GB, so the first plan honestly samples
// ~6 GB free and — with no incumbent — plainly selects the biggest thing that fits: a 14B at
// the floor. The predecessor was then reclaimed 1.2s later, freeing memory the plan had
// already spent. Counting the inherited lane as the incumbent re-arms the at-rest credit
// that exists for exactly this ("a model's OWN residency can never flap it out"), so the
// 27B holds. Regression here = every crash-and-restart silently demotes the served model.
#[tokio::test]
async fn a_lane_inherited_from_a_previous_generation_is_the_incumbent_the_first_plan_defends() {
    let mut daemon = daemon_with(Arc::new(FakeServer::healthy(
        Arc::new(AtomicUsize::new(0)),
        true,
    )));
    daemon.set_inherited_lane(Arc::new(|| Some(inherited_27b())));

    let (budget, candidates) = boot_squeeze();
    daemon.publish_plan(budget, &candidates, &candidates);

    let plan = daemon.plan_tx.borrow().plan.clone().expect("a plan");
    assert_eq!(
        plan.base_model.model_id, "qwen3-27b",
        "the inherited 27B is ours and its bytes are ours to reclaim — the successor must \
             not flee its own predecessor onto a 14B"
    );
}

// what this catches: the NEGATIVE control for the test above — proof the fixture really is
// a squeeze and the assertion is not vacuous. Same budget, same candidates, no inherited
// lane: the planner correctly downshifts, because on a genuinely 6 GB machine a 27B does
// not fit. If this ever also returns the 27B, the test above proves nothing.
#[tokio::test]
async fn without_a_past_form_of_ourself_a_six_gigabyte_box_really_does_downshift() {
    // A genuinely 8 GB card: 6 GB live is not a squeeze, it is the box. The host
    // floor (physical × 0.80 = 6.4 GB) is honestly the 4B, so the downshift stands.
    let daemon = daemon_on_card(
        Arc::new(FakeServer::healthy(Arc::new(AtomicUsize::new(0)), true)),
        8 * GB,
    );
    let (budget, candidates) = boot_squeeze();
    daemon.publish_plan(budget, &candidates, &candidates);
    let plan = daemon.plan_tx.borrow().plan.clone().expect("a plan");
    assert_eq!(
        plan.base_model.model_id, "coder-4b",
        "with no incumbent to credit, 6 GB genuinely cannot hold a 19 GB model"
    );
}

// what this catches (2026-09-16): the same 6 GB reading on a 53 GB card is a
// TRANSIENT, and with no incumbent to defend (a fresh boot, an unload that
// suppressed the 27B) every other guard is disarmed. The host floor is not: the
// card's physical capacity serves the 27B, so a plan naming the 4B is refused and
// NOTHING is published — the host waits for its budget rather than sinking. This is
// the crash-restart demotion of #438 and the unload demotion of #4145, closed at
// the layer both share.
#[tokio::test]
async fn a_transient_six_gigabytes_on_a_big_card_refuses_to_sink_below_the_floor() {
    let daemon = daemon_with(Arc::new(FakeServer::healthy(
        Arc::new(AtomicUsize::new(0)),
        true,
    )));
    let (budget, candidates) = boot_squeeze();
    daemon.publish_plan(budget, &candidates, &candidates);
    assert!(
        daemon.plan_tx.borrow().plan.is_none(),
        "a 53 GB card whose floor is the 27B must not publish a 4B plan on a 6 GB reading"
    );
    // And when the budget comes back, the 27B is adopted at once.
    let recovered = HostBudget {
        usable_bytes: 40 * GB,
        perf_cores: budget.perf_cores,
    };
    daemon.publish_plan(recovered, &candidates, &candidates);
    let plan = daemon
        .plan_tx
        .borrow()
        .plan
        .clone()
        .expect("a plan once the budget returns");
    assert_eq!(plan.base_model.model_id, "qwen3-27b");
}

// what this catches: precedence. Once THIS core has published a decision, its own plan is
// the incumbent and a stale registry record must never override it — otherwise a leftover
// .lane file would pin the model forever against every later replan.
#[test]
fn a_published_plan_outranks_an_inherited_lane() {
    assert_eq!(
        incumbent_for_plan(Some("coder-14b".into()), Some(&inherited_27b())).as_deref(),
        Some("coder-14b"),
    );
    // …and with nothing published, the inherited lane speaks.
    assert_eq!(
        incumbent_for_plan(None, Some(&inherited_27b())).as_deref(),
        Some("qwen3-27b"),
    );
    // …and a true cold boot still has no incumbent at all.
    assert_eq!(incumbent_for_plan(None, None), None);
}

// what this catches (2026-09-16, the 5090): a host sinking BELOW what its physical
// capacity serves. `serving/unload` suppressed the 27B, so it left `candidates`;
// the debounce read that as "incumbent gone from disk" (the case just below) and
// adopted the 0.5B on a 32 GB card at a transient 24 GB — 35 silent relaunches
// (#4145). The floor is the plan's OWN selection law at the PHYSICAL budget over
// everything on disk, suppression ignored: on this card that is the 27B, so a plan
// naming anything less is refused. On a card that cannot hold the 27B at all the
// floor is honestly the smaller model and the same plan passes.
#[test]
fn a_host_never_sinks_below_what_its_physical_capacity_serves() {
    let footprint = |id: &str, weights_gb: u64, rank: u8| ModelFootprint {
        model_id: id.into(),
        weights_bytes: weights_gb * GB,
        kv_per_token: 100_000,
        context_window: 32768,
        capability_rank: rank,
        fixed_per_lane_bytes: 0,
    };
    let big = footprint("qwen3.8-27b", 17, 9);
    let tiny = footprint("qwen-0.5b", 1, 1);
    let on_disk = vec![big.clone(), tiny.clone()];
    let demand = ServingDemand::new(1, Some(8192));
    let plan_for = |m: &ModelFootprint| ServingPlan {
        base_model: m.clone(),
        served_context_window: 2048,
        lanes: 1,
        grid_overflow_lanes: 0,
        resident_models: 1,
        fits_on_gpu: true,
        rationale: String::new(),
        lanes_bound: crate::inference::planning::serving_plan::LanesBound::Demand,
    };
    // The 5090: 32 GB × 0.80 physical. The floor is the 27B — even though the
    // plan that asked was computed over a candidate list the unload had emptied
    // of it (the floor looks at what is ON DISK, not what is currently permitted).
    let card_5090 = HostBudget {
        usable_bytes: (32.0 * GB as f64 * 0.80) as u64,
        perf_cores: 8,
    };
    let floor = host_floor_of(card_5090, &on_disk, &demand).expect("something on disk");
    assert_eq!(floor.model_id, "qwen3.8-27b");
    assert_eq!(
        floor_gate(&plan_for(&tiny), Some(&floor)),
        FloorVerdict::Below {
            floor: "qwen3.8-27b".into(),
            wanted: "qwen-0.5b".into()
        },
        "the 0.5B on a 5090 is a demotion below physical capacity, whatever the live budget said"
    );
    assert_eq!(
        floor_gate(&plan_for(&big), Some(&floor)),
        FloorVerdict::AtOrAbove
    );
    // A card the 27B cannot fit at all (16 GB × 0.80 < 17 GB weights): the floor is
    // honestly the small model, and serving it is not a demotion.
    let small_card = HostBudget {
        usable_bytes: (16.0 * GB as f64 * 0.80) as u64,
        perf_cores: 4,
    };
    let floor = host_floor_of(small_card, &on_disk, &demand).expect("something on disk");
    assert_eq!(floor.model_id, "qwen-0.5b");
    assert_eq!(
        floor_gate(&plan_for(&tiny), Some(&floor)),
        FloorVerdict::AtOrAbove
    );
    // THE PIN IS THE FLOOR'S CEILING (Fable, #4146 review): the M5 pins a lower-ranked
    // model with a higher-ranked one on disk. The floor is judged over the PINNED
    // universe, so the pinned model IS the floor and serving it is never a demotion.
    let ornith = footprint("ornith-35b-a3b", 20, 5);
    let pinned_universe = vec![ornith.clone()]; // what servable_candidates yields under the pin
    let floor = host_floor_of(card_5090, &pinned_universe, &demand).expect("pinned model on disk");
    assert_eq!(floor.model_id, "ornith-35b-a3b");
    assert_eq!(
        floor_gate(&plan_for(&ornith), Some(&floor)),
        FloorVerdict::AtOrAbove
    );
    // No floor (nothing on disk / no GPU) never refuses.
    assert_eq!(floor_gate(&plan_for(&tiny), None), FloorVerdict::AtOrAbove);
}

// what this catches (#368, 2nd occurrence): a ONE-tick budget collapse must
// classify as a Downshift (so publish_plan HOLDS the incumbent plan until
// the squeeze proves sustained) — while same-model, upshift, and
// incumbent-gone cases adopt immediately. The live failure: usable_gb hit 0
// for ~6 seconds at solve launch, the fresh plan named the 0.5B, and the
// reconcile re-brained every citizen onto it before the dip had even passed.
#[test]
fn downshift_gate_classifies_the_transient_lobotomy() {
    let footprint = |id: &str, weights_gb: u64, rank: u8| ModelFootprint {
        model_id: id.into(),
        weights_bytes: weights_gb * GB,
        kv_per_token: 100_000,
        context_window: 32768,
        capability_rank: rank,
        fixed_per_lane_bytes: 0,
    };
    let devstral = footprint("devstral-24b", 14, 8);
    let tiny = footprint("qwen-0.5b", 1, 1);
    let plan_for = |id: &str| ServingPlan {
        base_model: footprint(id, 1, 1),
        served_context_window: 2048,
        lanes: 1,
        grid_overflow_lanes: 0,
        resident_models: 1,
        fits_on_gpu: true,
        rationale: String::new(),
        lanes_bound: crate::inference::planning::serving_plan::LanesBound::Demand,
    };
    let both = vec![devstral.clone(), tiny.clone()];

    // The incident shape: incumbent Devstral, fresh wants the 0.5B → HOLD.
    assert_eq!(
        downshift_gate(&plan_for("qwen-0.5b"), Some("devstral-24b"), &both),
        DownshiftVerdict::Downshift,
    );
    // Same model → nothing to debounce.
    assert_eq!(
        downshift_gate(&plan_for("devstral-24b"), Some("devstral-24b"), &both),
        DownshiftVerdict::NotADownshift,
    );
    // UPSHIFT (0.5B incumbent, fresh wants Devstral) must adopt immediately —
    // holding it would have kept the citizens lobotomized on purpose.
    assert_eq!(
        downshift_gate(&plan_for("devstral-24b"), Some("qwen-0.5b"), &both),
        DownshiftVerdict::NotADownshift,
    );
    // Incumbent vanished from disk → holding a plan naming a ghost is worse
    // than any downshift; adopt what is actually present.
    assert_eq!(
        downshift_gate(&plan_for("qwen-0.5b"), Some("devstral-24b"), &[tiny]),
        DownshiftVerdict::NotADownshift,
    );
    // No incumbent (boot) → plain adoption.
    assert_eq!(
        downshift_gate(&plan_for("qwen-0.5b"), None, &both),
        DownshiftVerdict::NotADownshift,
    );
}

// what this catches (#225, Joel 2026-08-08 "the budgeter just has all its
// parts figure it out"): serving's plan budget (`governed_vram_ceiling`)
// reads serving's OWN view of the board — global available minus every
// OTHER consumer's unmet reservation floor — so the plan can no longer size
// the window into the embed lane's slice and starve a faculty cognition
// needs every turn. Regression: reading the reservation-blind global
// `available_bytes` again would make this budget ignore the floor.
#[tokio::test]
async fn serving_budget_plans_around_other_consumers_floors() {
    use crate::resources::{DaemonConfig, GovernorConfig, MockCapacitySource, ResourceDaemon};
    let src = Arc::new(MockCapacitySource::new(
        crate::resources::ResourceKind::Vram,
        10_000,
    ));
    let daemon = ResourceDaemon::start(
        vec![src],
        vec![],
        DaemonConfig {
            tick_interval: std::time::Duration::from_millis(20),
            min_reclaim_budget: std::time::Duration::from_millis(100),
            governor: GovernorConfig {
                min_dwell_ms: 0,
                graceful_grace_ms: 50,
            },
        },
    );
    // No floors → serving sees the whole board.
    assert_eq!(governed_vram_ceiling(&daemon), Some(10_000));
    // The embed lane claims its standing floor → serving's plannable view
    // shrinks by exactly that slice; the board's Vram row still exists so
    // the None ("governor hasn't reported") semantics are untouched.
    daemon.reserve("embed", crate::resources::ResourceKind::Vram, 1_800);
    assert_eq!(governed_vram_ceiling(&daemon), Some(8_200));
    // Serving's own hypothetical floor would NOT count against itself.
    daemon.reserve(
        SERVING_CONSUMER_ID,
        crate::resources::ResourceKind::Vram,
        3_000,
    );
    assert_eq!(governed_vram_ceiling(&daemon), Some(8_200));
}

// what this catches (2026-09-18 21:56:03Z, the M5, card below): the live free read
// compared RAW admitted a 24.1 GB sidecar into 26.7 GB "free" beside the 27B, and
// the box paged (swap 6.7 GB, compressor 20 GB). With the same OS floor the planned
// term keeps, every measured tick of that boot refuses it, the tighter term still
// wins, and a small VL sidecar is still admitted — sight is not refused, a mind-sized
// sidecar is. Numbers are the measured ones; the floor is the rule's own.
#[test]
fn the_live_free_read_keeps_the_same_os_floor_the_planned_term_keeps() {
    const GB: u64 = 1024 * 1024 * 1024;
    let physical = 64 * GB;
    let floor = host_os_floor_bytes(physical);
    let need = 24_078_233_892u64;
    // 21:55:27Z: live 14.2 GB, planned 16.6 GB → refused either way.
    assert!(sidecar_admissible_bytes(14_215_921_664, 16_647_885_296, physical) < need);
    // 21:56:03Z: live 26.7 GB, planned 31.8 GB → admitted before; REFUSED now.
    let admitted_before = 26_689_634_304u64.min(31_805_592_074);
    assert!(
        admitted_before >= need,
        "the raw comparison said yes — that is the defect"
    );
    let now = sidecar_admissible_bytes(26_689_634_304, 31_805_592_074, physical);
    assert!(
        now < need,
        "with the floor ({} GB) the same tick refuses: {} < {}",
        floor / GB,
        now,
        need
    );
    assert_eq!(
        now,
        26_689_634_304 - floor,
        "the live term after the floor is the tighter one here"
    );
    // The planned term still wins when it is the tighter one.
    assert_eq!(
        sidecar_admissible_bytes(40 * GB, 10_374_868_340, physical),
        10_374_868_340
    );
    // A small VL sidecar (4 GB) is still admitted on the same tick.
    assert!(
        now >= 4 * GB,
        "sight is not refused, a mind-sized sidecar is"
    );
    // An over-full box yields zero, never a wrap.
    assert_eq!(sidecar_admissible_bytes(floor / 2, 31 * GB, physical), 0);
}
