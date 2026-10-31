#![cfg(test)]

use super::*;

// what this catches: #3751 threw this throttle at `rehome_streak` — the
// RE-HOME GAIN counter — which is stored to 0 for exactly as long as the lane
// is below plan with no qualifying gain. `streak <= 1` was therefore true on
// every tick of the declined episode and the probe throttled NOTHING: 411
// identical lines at one per 5 s measured on BigMama 2026-09-05, burying a
// persona's first_token underneath them. The clock a cadence uses must be
// owned by the cadence; a domain counter's reset rule belongs to the domain.
#[test]
fn decline_probe_speaks_on_entry_then_about_once_a_minute() {
    assert!(
        super::decline_should_log(1),
        "entry into the declined state must always speak — the silence was the original defect"
    );
    for quiet in [2, 3, 7, 11, 13, 23] {
        assert!(
            !super::decline_should_log(quiet),
            "tick {quiet} adds nothing the 1st line lacked"
        );
    }
    for loud in [12, 24, 120] {
        assert!(
            super::decline_should_log(loud),
            "tick {loud} is the ~1/min heartbeat"
        );
    }

    // A ten-minute episode is 120 ticks: entry plus one per minute, not 120.
    let spoken = (1..=120u32)
        .filter(|&t| super::decline_should_log(t))
        .count();
    assert_eq!(spoken, 11, "entry + 10 minutes of heartbeat over 120 ticks");

    // The defect itself, stated as a property: a clock that never advances
    // must be SILENT, not infinitely chatty. `0 % 12 == 0` would make the
    // naive form log forever, which is how the first fix passed review.
    assert!(
            !super::decline_should_log(0),
            "a counter that never advanced means we have not entered the state, not that we should shout"
        );
}

// what this catches (2026-09-17, the M5 at 8 lanes / 7.9 t/s all morning): a plan
// that trades lanes for width is the decode knee clamping the roster, and it is
// evidence even when token-space calls it a wash or a loss — otherwise the knee
// only ever applies at boot. Re-slicing the same space at the SAME lane count, or a
// plan with more lanes, still needs the space margin.
#[test]
fn a_lane_over_its_knee_is_rehome_evidence_whatever_the_token_space_says() {
    use super::{lane_over_its_knee, rehome_gain_evidence};
    // The live shape: 8 × 51,712 → 7 × 67,340 (+14%, under the margin) qualifies.
    let (_, _, _, worth_it) = rehome_gain_evidence(51_712, 8, 67_340, 7);
    assert!(
        worth_it,
        "one lane over the knee is a relaunch, not a rounding error"
    );
    // The knee at 4: 8 × 51,712 → 4 × 67,340 is a LOSS of space and still qualifies.
    let (_, _, _, worth_it) = rehome_gain_evidence(51_712, 8, 67_340, 4);
    assert!(
        worth_it,
        "fewer, wider lanes is the knee — space is not the currency here"
    );
    // Fewer lanes but NARROWER is a pressure shrink, not a knee: not this arm.
    assert!(!lane_over_its_knee(51_712, 8, 40_000, 4));
    // Same lanes / more lanes: the space margin still rules.
    assert!(!lane_over_its_knee(40_000, 2, 44_000, 2));
    assert!(!lane_over_its_knee(57_600, 2, 47_426, 4));
    let (_, _, _, worth_it) = rehome_gain_evidence(40_000, 2, 44_000, 2);
    assert!(
        !worth_it,
        "10% at the same lane count is still under the margin"
    );
}

// what this catches (2026-09-19 15:4xZ, the M5 stuck at one lane): the knee's mirror.
// A cold boot launched 1 × 161,792 under a transient budget; the steady plan said
// 2 × 67,340 — a 17% LOSS of token-space — and token-space alone declined it every
// tick, so the roster's second coder lane could never appear at runtime, and the
// settled 1-lane geometry then capped every later boot. A plan with more lanes than
// are live is minds without a warm slot; that is evidence whatever the space says.
// Fewer or equal lanes is not this arm (the knee and the margin rule those), and a
// dead lane (0 live lanes) is recovery's job, never re-home's.
#[test]
fn a_roster_short_of_warm_slots_is_rehome_evidence_whatever_the_token_space_says() {
    use super::{rehome_gain_evidence, roster_short_of_slots};
    assert!(
        roster_short_of_slots(1, 2),
        "1 × 161k → 2 × 67k: a second warm slot for the roster"
    );
    let (_, _, _, by_space) = rehome_gain_evidence(161_792, 1, 67_340, 2);
    assert!(
        !by_space,
        "token-space alone calls the second lane a loss — the arm above is what carries it"
    );
    assert!(
        roster_short_of_slots(2, 4),
        "2 × 40k → 4 × 20k: two more slots the roster asked for"
    );
    assert!(
        !roster_short_of_slots(2, 2),
        "the same count is the margin's business"
    );
    assert!(
        !roster_short_of_slots(8, 4),
        "fewer lanes is the knee's business"
    );
    assert!(
        !roster_short_of_slots(0, 2),
        "a dead lane is recovery, not re-home"
    );
}

// what this catches (2026-08-15 14:13, round-killer L10 / #438 live): a plan tick
// whose usable_bytes sample was transiently bogus named the SMALLEST candidate
// (qwen2.5-0.5B) via the nothing-fits arm, and the reconcile swapped the READY
// 27B lane on that single tick — citizens spoke template-token salad into the
// bench room as durable messages every healthy peer then perceived as
// conversation. A base-model swap re-homes minds; it must earn the same
// sustained streak as the (milder) window re-home, target changes must restart
// the count, and recovery (no ready incumbent) must never wait.
#[test]
fn model_swap_on_a_ready_lane_needs_a_sustained_streak() {
    use super::{model_change_gate, ModelChangeGate};
    use std::sync::atomic::AtomicU32;
    let streak = AtomicU32::new(0);
    let mut pending = None;
    // One bogus tick wanting the 0.5B: deferred, never actuates.
    assert_eq!(
        model_change_gate(true, Some("27b"), "0.5b", &mut pending, &streak, 3),
        ModelChangeGate::Defer { streak: 1 }
    );
    // Next tick the sample healed and the plan wants the incumbent again:
    // pending clears, nothing happened.
    assert_eq!(
        model_change_gate(true, Some("27b"), "27b", &mut pending, &streak, 3),
        ModelChangeGate::NotAChange
    );
    assert!(pending.is_none());
    // Sustained genuine disagreement commits on the Nth consecutive tick.
    assert_eq!(
        model_change_gate(true, Some("27b"), "48b", &mut pending, &streak, 3),
        ModelChangeGate::Defer { streak: 1 }
    );
    assert_eq!(
        model_change_gate(true, Some("27b"), "48b", &mut pending, &streak, 3),
        ModelChangeGate::Defer { streak: 2 }
    );
    assert_eq!(
        model_change_gate(true, Some("27b"), "48b", &mut pending, &streak, 3),
        ModelChangeGate::Commit { streak: 3 }
    );
    // A flapping target restarts the count — 0.5b/48b alternation never commits.
    assert_eq!(
        model_change_gate(true, Some("27b"), "0.5b", &mut pending, &streak, 3),
        ModelChangeGate::Defer { streak: 1 }
    );
    assert_eq!(
        model_change_gate(true, Some("27b"), "48b", &mut pending, &streak, 3),
        ModelChangeGate::Defer { streak: 1 }
    );
    // No ready incumbent (boot / post-wedge): commit immediately — recovery
    // must not wait out a streak.
    let mut p2 = None;
    assert_eq!(
        model_change_gate(false, Some("27b"), "48b", &mut p2, &streak, 3),
        ModelChangeGate::NotAChange
    );
    assert_eq!(
        model_change_gate(true, None, "48b", &mut p2, &streak, 3),
        ModelChangeGate::NotAChange
    );
}

// what this catches: the failure the OLD single-sample `live * 2 <= plan` rule
// could not see. Measured live 2026-08-06: plan 26,323 vs lane 16,384 — a 62%
// lane, comfortably above half — held indefinitely while every persona's binding
// read the stale window and the demand loop's output was silently discarded.
// A real, PERSISTENT gain must eventually re-home no matter where it sits in the
// 50-99% band. (BigMama's call; she owns the guard the ratio came from.)
#[tokio::test]
async fn a_sustained_gain_above_the_old_half_bar_eventually_re_homes() {
    let serves = Arc::new(AtomicUsize::new(0));
    // 62% of plan — the live incident's exact ratio, comfortably above the old
    // half bar and therefore invisible to the rule this replaces.
    let (daemon, plan_window) = lane_under_plan(serves.clone(), 65_536, 62);
    // Guard the FIXTURE, not just the guard: if a measured demand had not raised
    // the plan above the cold-start prior, plan == live, there would be no gain to
    // sustain, and every assertion below would pass for the wrong reason. That is
    // exactly how this test failed its first time out.
    assert!(
        plan_window > daemon.serving_tx.borrow().served_context_window,
        "fixture is vacuous: the plan must actually exceed the lane"
    );

    // Below the streak it must hold — a relaunch kills in-flight turns, so an
    // instant re-home on first sight is exactly the thrash we are avoiding.
    for tick in 1..REHOME_SUSTAINED_TICKS {
        assert!(
            daemon.reconcile_to_plan().is_none(),
            "tick {tick}: must not re-home before the gain has proven itself sustained"
        );
    }
    assert_eq!(
        serves.load(Ordering::SeqCst),
        0,
        "no relaunch during the streak"
    );

    assert!(
        daemon.reconcile_to_plan().is_some(),
        "a gain that persisted for a full lane-readiness window MUST re-home — \
             this is the 62%-forever case the 2x ratio never caught"
    );
}

// what this catches: the property the 2x ratio was RIGHT about, which the
// replacement must not lose. A plan that jitters above the bar and falls back
// must never accumulate its way to a relaunch — the outage this guard exists for
// was a lane relaunching against a plan wandering 3.6k<->22k.
#[tokio::test]
async fn a_jittering_gain_never_accumulates_into_a_re_home() {
    let serves = Arc::new(AtomicUsize::new(0));
    let (daemon, plan_window) = lane_under_plan(serves.clone(), 65_536, 62);
    let live_window = daemon.serving_tx.borrow().served_context_window;
    let budget = HostBudget {
        usable_bytes: 45 * GB,
        perf_cores: 6,
    };

    // Alternate: plenty of headroom, then none. Far MORE ticks than the streak
    // needs in total — but never consecutively.
    for _ in 0..(REHOME_SUSTAINED_TICKS * 3) {
        assert!(
            daemon.reconcile_to_plan().is_none(),
            "wanting: still below streak"
        );
        // A dip: the plan momentarily affords no more than the lane already has.
        let small =
            vec![footprint_from_parts("coder-14b", 9 * GB, live_window, true, None).unwrap()];
        daemon.publish_plan(budget, &small, &small);
        assert!(daemon.reconcile_to_plan().is_none(), "dip: nothing to gain");
        // …and back up.
        let big = vec![footprint_from_parts("coder-14b", 9 * GB, plan_window, true, None).unwrap()];
        daemon.publish_plan(budget, &big, &big);
    }
    assert_eq!(
        serves.load(Ordering::SeqCst),
        0,
        "jitter must NEVER re-home, however long it goes on — one dip resets the streak"
    );
}

// what this catches (2026-10-07, the 5090): a lane serving 2,304 under a plan that
// alternated 33k <-> 70k never re-homed for an hour. Every rise reset the streak,
// although BOTH plan values wanted a far bigger lane, so Kimi, needing 52k, could
// not fit while the lane read ready. A plan moving below a high it already reached
// is settled enough: the streak counts and the lane re-homes within a few ticks.
#[tokio::test]
async fn a_lane_far_below_an_alternating_plan_re_homes() {
    let serves = Arc::new(AtomicUsize::new(0));
    let (daemon, full) = lane_under_plan(serves.clone(), 65_536, 4);
    let live_window = daemon.serving_tx.borrow().served_context_window;
    assert!(
        live_window * 4 < full / 2,
        "fixture: the lane must sit far below both plan values"
    );
    let budget = HostBudget {
        usable_bytes: 45 * GB,
        perf_cores: 6,
    };
    let half = vec![footprint_from_parts("coder-14b", 9 * GB, full / 2, true, None).unwrap()];
    let whole = vec![footprint_from_parts("coder-14b", 9 * GB, full, true, None).unwrap()];
    let mut fired_at = None;
    for tick in 0..(REHOME_SUSTAINED_TICKS * 4) {
        let plan = if tick % 2 == 0 { &half } else { &whole };
        daemon.publish_plan(budget, plan, plan);
        if daemon.reconcile_to_plan().is_some() {
            fired_at = Some(tick);
            break;
        }
    }
    let fired_at =
        fired_at.expect("a lane far below both values of an alternating plan must re-home");
    assert!(
        fired_at <= REHOME_SUSTAINED_TICKS + 3,
        "re-homed only at tick {fired_at}: the alternation still resets the streak"
    );
    // and it lands at the HIGH value (the relaunch takes the fire tick's plan): a re-home
    // on a low tick would bring her back under her need and owe a second relaunch
    assert_eq!(
        fired_at % 2,
        1,
        "fired on a low tick (half the plan), not the high one"
    );
}

// what this catches (card 5bac4968, Kimi's node 2026-10-15): one transient high stranding a
// lane. The plan read 109,768 at the post-deploy relaunch tick while memory was still being
// freed, then settled at 33k-87k; the fire tick had to reach that one-off high, never did, and
// her lane served 6,144 until an operator relaunched it. The high is the best of the last
// REHOME_SUSTAINED_TICKS plans, so a spike ages out and the lane re-homes to the settled
// plan; the rule is also pinned on its own, alternating and staircase cases kept above/below.
#[tokio::test]
async fn a_transient_plan_spike_does_not_strand_the_lane_below_the_settled_plan() {
    let serves = Arc::new(AtomicUsize::new(0));
    let (daemon, full) = lane_under_plan(serves.clone(), 65_536, 4);
    let live_window = daemon.serving_tx.borrow().served_context_window;
    let settled_window = full * 3 / 4;
    assert!(
        live_window * 4 < settled_window / 2,
        "fixture: the lane must sit far below the settled plan"
    );
    let budget = HostBudget {
        usable_bytes: 45 * GB,
        perf_cores: 6,
    };
    let spike = vec![footprint_from_parts("coder-14b", 9 * GB, full, true, None).unwrap()];
    let settled =
        vec![footprint_from_parts("coder-14b", 9 * GB, settled_window, true, None).unwrap()];
    daemon.publish_plan(budget, &spike, &spike);
    assert!(
        daemon.reconcile_to_plan().is_none(),
        "one spike tick is not sustained evidence"
    );
    let mut fired_at = None;
    for tick in 1..(REHOME_SUSTAINED_TICKS * 4) {
        daemon.publish_plan(budget, &settled, &settled);
        if daemon.reconcile_to_plan().is_some() {
            fired_at = Some(tick);
            break;
        }
    }
    let fired_at =
        fired_at.expect("a lane far below a settled plan must re-home once the spike ages out");
    assert!(
        fired_at <= REHOME_SUSTAINED_TICKS + 1,
        "re-homed only at tick {fired_at}: the spike outlived the evidence span"
    );

    let mut recent = std::collections::VecDeque::new();
    assert!(
        at_recent_plan_high(&mut recent, 109_768),
        "the first plan is its own high"
    );
    for (plan, expect) in [
        (87_000, false),
        (84_000, false),
        (86_000, false),
        (87_000, true),
    ] {
        assert_eq!(
            at_recent_plan_high(&mut recent, plan),
            expect,
            "plan {plan} against {recent:?}"
        );
    }
    assert!(
        recent.len() <= REHOME_SUSTAINED_TICKS as usize,
        "bounded by the evidence span"
    );
}

// what this catches: the boot staircase (2026-09-02) the streak's plan-high rule
// exists for. Personas register serially at boot, so the plan climbs in stairs;
// a re-home per stair was sixteen model loads in one lane's log. While every tick
// is a new high, nothing fires; once the climb stops, exactly one re-home at the top.
#[tokio::test]
async fn a_climbing_plan_coalesces_into_one_re_home_at_its_top() {
    let serves = Arc::new(AtomicUsize::new(0));
    let (daemon, top) = lane_under_plan(serves.clone(), 65_536, 4);
    let budget = HostBudget {
        usable_bytes: 45 * GB,
        perf_cores: 6,
    };
    let at = |w: u32| vec![footprint_from_parts("coder-14b", 9 * GB, w, true, None).unwrap()];
    for (stair, w) in [
        top / 4,
        top * 3 / 8,
        top / 2,
        top * 5 / 8,
        top * 3 / 4,
        top * 7 / 8,
        top,
    ]
    .into_iter()
    .enumerate()
    {
        let plan = at(w);
        daemon.publish_plan(budget, &plan, &plan);
        assert!(
            daemon.reconcile_to_plan().is_none(),
            "stair {stair}: a climbing plan must not re-home"
        );
    }
    let plan = at(top);
    for tick in 1..REHOME_SUSTAINED_TICKS {
        daemon.publish_plan(budget, &plan, &plan);
        assert!(
            daemon.reconcile_to_plan().is_none(),
            "settled tick {tick}: still proving the plan stopped"
        );
    }
    daemon.publish_plan(budget, &plan, &plan);
    assert!(
        daemon.reconcile_to_plan().is_some(),
        "the climb stopped: one re-home, at its top (no stair above re-homed)"
    );
}

// what this catches: the cooldown collapsing into the sustained-delta test. The
// two answer different questions — "is the gain real?" vs "may we pay for it
// yet?" — and a relaunch kills every in-flight turn on the lane, so the rate
// limit has to hold even when the evidence is perfect. If the cooldown were
// merely "reset the streak", 3 more qualifying ticks would re-fire in 15s
// instead of 90. (BigMama's requirement 1 of 2, 2026-08-06.)
#[tokio::test]
async fn a_second_re_home_waits_out_the_cooldown_however_good_the_evidence_is() {
    let serves = Arc::new(AtomicUsize::new(0));
    let (daemon, _plan) = lane_under_plan(serves.clone(), 65_536, 62);
    let starved = daemon.serving_tx.borrow().clone();

    let mut decision = None;
    for _ in 0..REHOME_SUSTAINED_TICKS {
        decision = daemon.reconcile_to_plan();
    }
    decision
        .expect("evidence qualifies by the third tick")
        .await
        .unwrap();
    assert_eq!(
        serves.load(Ordering::SeqCst),
        1,
        "the first re-home fires on evidence"
    );

    // Re-pin the lane to its starved window: the relaunch is faked, so nothing
    // actually resized. That makes the gain as compelling on every following tick
    // as it was on the first — only the cooldown stands between us and a storm.
    let _ = daemon.serving_tx.send_replace(starved);
    for tick in 0..REHOME_COOLDOWN_TICKS {
        assert!(
            daemon.reconcile_to_plan().is_none(),
            "cooldown tick {tick}: perfect evidence must still not buy a second relaunch"
        );
    }
    assert_eq!(
        serves.load(Ordering::SeqCst),
        1,
        "exactly one relaunch per cooldown window"
    );

    // Cooldown spent — the gain must now re-prove itself from zero before firing.
    for tick in 0..(REHOME_SUSTAINED_TICKS - 1) {
        assert!(
            daemon.reconcile_to_plan().is_none(),
            "post-cooldown tick {tick}: the streak was lost while cooling, re-prove it"
        );
    }
    assert!(
        daemon.reconcile_to_plan().is_some(),
        "once cooled AND re-proven, the still-stranded lane re-homes again"
    );
}

// what this catches: a margin expressed as an absolute token count. The same
// 2,048-token shortfall is a third of a small lane and rounding error on a large
// one — an absolute bar means something different at every window size, so the
// relative margin is the property, not an implementation detail of it.
#[tokio::test]
async fn the_margin_is_relative_so_the_same_shortfall_decides_differently_by_lane_size() {
    // A lane at 93% of plan: the shortfall is ~7.5% of what it serves — real
    // tokens, but under the bar however long it persists.
    let quiet = Arc::new(AtomicUsize::new(0));
    let (near_plan, _) = lane_under_plan(quiet.clone(), 65_536, 93);
    for _ in 0..(REHOME_SUSTAINED_TICKS * 3) {
        near_plan.reconcile_to_plan();
    }
    assert_eq!(
        quiet.load(Ordering::SeqCst),
        0,
        "a 7.5% shortfall is not worth killing in-flight turns for, however long it holds"
    );

    // A lane at 67% of the SAME plan: the shortfall is ~49% of what it serves —
    // half a mind again, a step change.
    let loud = Arc::new(AtomicUsize::new(0));
    let (far_below, _) = lane_under_plan(loud.clone(), 65_536, 67);
    let mut decision = None;
    for _ in 0..REHOME_SUSTAINED_TICKS {
        decision = far_below.reconcile_to_plan();
    }
    decision.expect("a 49% shortfall qualifies").await.unwrap();
    assert_eq!(
        loud.load(Ordering::SeqCst),
        1,
        "half the lane's own window again IS a step change — re-home"
    );
}

// what this catches: a lane whose per-slot window froze at ≤ HALF the
// currently-planned window gets RE-HOMED (relaunched) even though model +
// genome match. A server spawned under transient memory pressure (a scratch
// benchmark server) otherwise keeps its starved window forever — 2026-07-10:
// the living Devstral froze at 3.8k/slot on a 131k model while 14,700 plan
// recomputes wandered above it, and the room degenerated into a greeting
// loop. Within 2× of the plan → hysteresis holds, no relaunch churn.
#[tokio::test]
async fn reconcile_re_homes_a_starved_window_but_holds_within_hysteresis() {
    let serves = Arc::new(AtomicUsize::new(0));
    let daemon = daemon_with(Arc::new(FakeServer::healthy(serves.clone(), true)));

    let budget = HostBudget {
        usable_bytes: 45 * GB,
        perf_cores: 6,
    };
    let candidates = vec![footprint_from_parts("coder-14b", 9 * GB, 8192, true, None).unwrap()];
    daemon.publish_plan(budget, &candidates, &candidates);
    let (plan_window, plan_lanes) = {
        let plan = daemon.plan_tx.borrow();
        let plan = plan.plan.as_ref().expect("plan published");
        (plan.served_context_window, plan.lanes)
    };

    // Same model + genome, but the live slot froze far below the plan.
    let _ = daemon.serving_tx.send_replace(ServingSnapshot {
        // test fixture: no live readiness was ever CONFIRMED here.
        ready_verified_at_ms: None,
        active_model: Some("coder-14b".into()),
        ready: true,
        base_url: serving_v1_url(),
        adapters: Vec::new(),
        served_context_window: plan_window / 4,
        lanes: plan_lanes,
        host_prompt_cache_mib: 0,
        degraded_reason: None,
        vision_ready: false,
        vision_base_url: None,
        vision_model: None,
        loading_model: None,
        held_for: None,
    });
    // A quarter of the plan is a 300% shortfall — far past the margin — but the
    // gain must still PERSIST before it buys a relaunch (BigMama's sustained-delta
    // replacement for the original 2x ratio, 2026-08-06).
    for tick in 1..REHOME_SUSTAINED_TICKS {
        assert!(
            daemon.reconcile_to_plan().is_none(),
            "tick {tick}: even a starved lane must prove the gain is sustained"
        );
    }
    daemon
        .reconcile_to_plan()
        .expect("starved window must trigger a re-home")
        .await
        .unwrap();
    assert_eq!(serves.load(Ordering::SeqCst), 1, "one relaunch");

    // Within hysteresis (> half the plan) → hold, no churn.
    let _ = daemon.serving_tx.send_replace(ServingSnapshot {
        // test fixture: no live readiness was ever CONFIRMED here.
        ready_verified_at_ms: None,
        active_model: Some("coder-14b".into()),
        ready: true,
        base_url: serving_v1_url(),
        adapters: Vec::new(),
        served_context_window: plan_window / 2 + 256,
        lanes: plan_lanes,
        host_prompt_cache_mib: 0,
        degraded_reason: None,
        vision_ready: false,
        vision_base_url: None,
        vision_model: None,
        loading_model: None,
        held_for: None,
    });
    assert!(
        daemon.reconcile_to_plan().is_none(),
        "within 2\u{d7} of the plan the lane holds — no relaunch churn"
    );
    assert_eq!(serves.load(Ordering::SeqCst), 1, "still one relaunch");
}

// what this catches (card 7c72c0f0, Intel Mac 2026-09-17): the plan's budget is the
// row the lane's PLACEMENT puts its bytes in. A CPU-placed lane budgets against the
// Ram row — the 30 GB the KV actually fills — not the 4 GB Vram row of a card that
// holds nothing of it (nine citizens sized against 5.7 GB on a 34.4 GB box: one warm
// slot, eight exiled to a remote lane). The GPU arm is byte-identical to before.
#[tokio::test]
async fn a_cpu_placed_lane_budgets_against_the_ram_row_not_the_card() {
    use crate::inference::llama_server::LanePlacement;
    use crate::resources::{DaemonConfig, GovernorConfig, MockCapacitySource, ResourceDaemon};
    let vram = Arc::new(MockCapacitySource::new(
        crate::resources::ResourceKind::Vram,
        4_000,
    ));
    let ram = Arc::new(MockCapacitySource::new(
        crate::resources::ResourceKind::Ram,
        30_000,
    ));
    let daemon = ResourceDaemon::start(
        vec![vram, ram],
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
    assert_eq!(
        LanePlacement::Cpu.pool_kind(),
        crate::resources::ResourceKind::Ram
    );
    assert_eq!(
        LanePlacement::Gpu.pool_kind(),
        crate::resources::ResourceKind::Vram
    );
    assert_eq!(
        governed_pool_ceiling(&daemon, LanePlacement::Cpu.pool_kind()),
        Some(30_000),
        "a CPU-placed lane plans against the RAM its KV lives in"
    );
    assert_eq!(
        governed_pool_ceiling(&daemon, LanePlacement::Gpu.pool_kind()),
        Some(4_000),
        "a GPU-placed lane plans against the card"
    );
}
