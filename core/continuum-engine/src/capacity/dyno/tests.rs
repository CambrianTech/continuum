//! The dyno's engine-side runs: conservation (always on), the grant reproductions (2, 3) and the
//! view reproduction (7). Reproductions 1, 4, 5 and 6 live with the code they drive (`dyno.rs`).
#![cfg(test)]
use super::conservation::{Conservation, EngineResidency};
use super::expected::MissingSeam;
use super::recorded::*;

fn recorded() -> Recorded {
    Recorded::load().expect("test: the recorded 2026-11-04 data parses")
}

mod accounting {
    use super::*;

    fn footprint(day: &Recorded) -> crate::inference::planning::serving_plan::ModelFootprint {
        day.qwen36_on_m5().expect("test: the recorded weights are nonzero")
    }

    // what this catches: a launch pool the device cannot physically hold. Every pool the same
    // build launched with on 2026-11-04 (33,040 to 654,336, engine `-c`) is held by the M5's
    // 68,719,476,736 bytes beside the 20,419,565,568 B of weights and five sequences' staging,
    // counted with the planner's own footprint arithmetic.
    #[test]
    fn every_recorded_launch_pool_is_physically_conserved_on_the_m5() {
        let day = recorded();
        let footprint = footprint(&day);
        for &pool in &day.launch_pools_same_build.value {
            let engine = EngineResidency { footprint: &footprint, kv_tokens: u64::from(pool), sequences: day.m5_sequences.value };
            let accounting = Conservation::of(day.m5_device_bytes.value, &[engine]);
            assert!(accounting.holds(), "pool {pool}: {accounting:?}");
        }
    }

    // what this catches: a cold boot that counts only the new engine while the old one is still
    // wired (vm_stat 20:56Z: 21-24 GB reclaimable with it wired). Both engines' weights are
    // counted once each; the 125,640 launch beside the wired predecessor still fits the device,
    // so that launch was small, not unsafe: the defect is the reading it sized from (dyno 1).
    #[test]
    fn a_launch_beside_a_still_wired_predecessor_counts_both_engines_once() {
        let day = recorded();
        let footprint = footprint(&day);
        let cold_boot = day.cold_boot.value;
        let PreviousEngine::StillAttributed { wired_bytes } = cold_boot.reading.previous_engine else {
            panic!("test: the recorded cold boot read the old engine still wired");
        };
        assert_eq!(wired_bytes, footprint.weights_bytes, "test: the wired predecessor is the same model's weights");
        let predecessor = EngineResidency { footprint: &footprint, kv_tokens: 0, sequences: 0 };
        let successor = EngineResidency {
            footprint: &footprint,
            kv_tokens: u64::from(cold_boot.launched_pool),
            sequences: day.m5_sequences.value,
        };
        let both = Conservation::of(day.m5_device_bytes.value, &[predecessor, successor]);
        assert!(both.holds(), "{both:?}");
        assert_eq!(both.resident_bytes, predecessor.bytes() + successor.bytes(), "each engine counted once");
        assert_eq!(predecessor.bytes(), day.qwen36_weights_bytes.value, "a wired engine with no pool counts its weights only");
    }

    // what this catches: an accounting that can never fail. The trained window on every sequence
    // of a device-sized fleet (262,144 x 32 x 8,317 B = 69.8 GB of KV alone) is past the device:
    // it must read as overspent.
    #[test]
    fn an_overspent_device_reads_as_overspent() {
        let day = recorded();
        let footprint = footprint(&day);
        let sequences = 32; // test: enough sequences of the trained window to pass the device
        let engine = EngineResidency {
            footprint: &footprint,
            kv_tokens: u64::from(day.qwen36_trained_window.value) * u64::from(sequences),
            sequences,
        };
        let accounting = Conservation::of(day.m5_device_bytes.value, &[engine]);
        assert!(!accounting.holds(), "{accounting:?}");
        assert_eq!(accounting.headroom_bytes(), 0);
    }
}

mod grants {
    use super::*;
    use crate::capacity::dyno::service::recorded::{replay, Replay, Restart};
    use crate::capacity::window_grant::{Demand, DemandFollowing, WindowPolicy};
    use crate::inference::planning::window_allocator::observed::Observed;
    use crate::inference::planning::window_allocator::test_tokens as t;
    use std::collections::BTreeMap;

    /// A recorded resident's demand as `grants_now` builds it: a bound clamped to what one sequence
    /// holds (`Observed::within`), an exact need as measured; her target and wish are her need.
    fn demand(resident: &Resident, pool: u32, in_flight: bool) -> Demand<u32> {
        let need = match resident.need {
            RecordedNeed::Exact(n) => Observed::Exact(t(n)).within(pool),
            RecordedNeed::AtLeast(n) => Observed::AtLeast { at_least: t(n) }.within(pool),
        };
        Demand { mind: resident.mind, need, target: need, wish: need, in_flight, settled: true, ceiling: pool, growth: crate::inference::planning::window_allocator::Growth::Unmeasured }
    }

    /// The SUPERSEDED composition (before #5793): the bare demand policy regranted on every tick
    /// from what was held, with fairness read off a polled `in_flight`. Kept as the evidence of the
    /// 2026-11-04 receipt: from the recorded grant it reproduces {Aiko: 114,592} and serves no one.
    struct Played {
        first_served_after_turns: BTreeMap<u32, u64>,
        last_grants: BTreeMap<u32, u32>,
        holder_turns: u64,
    }

    fn play_superseded(day: &Recorded) -> Played {
        let grant = &day.whole_pool_grant.value;
        let (pool, holder) = (grant.pool, grant.holder);
        let tick_ms = day.grant_tick_ms.value;
        let cadence = day.aiko_turns.value;
        let turns_ended = |tick: u64| tick * tick_ms / cadence.act_ms();
        let mut held = BTreeMap::from([(holder, grant.held)]);
        let mut played = Played { first_served_after_turns: BTreeMap::new(), last_grants: BTreeMap::new(), holder_turns: 0 };
        for tick in 1..=cadence.window_ms / tick_ms {
            let demands: Vec<Demand<u32>> = grant.residents.iter().map(|r| demand(r, pool, r.mind == holder)).collect();
            let grants = DemandFollowing.grant(&demands, &held, pool);
            for d in demands.iter().filter(|d| d.mind != holder) {
                if grants.get(&d.mind).is_some_and(|&g| g >= d.need.get()) {
                    played.first_served_after_turns.entry(d.mind).or_insert(turns_ended(tick));
                }
            }
            played.holder_turns = turns_ended(tick);
            played.last_grants = grants.clone();
            held = grants;
        }
        played
    }

    /// The bound, reconciled with ONE-ACT section 4: each completed turn of a holder that owns no
    /// request is a feasible reclaim opportunity, and the replay asserts at EVERY publication that
    /// the owner took the opportunity it had (`service::world`, `Violation::MissedOpportunity`).
    /// With one holder, the k-th waiter is therefore served by the holder's k-th completed turn:
    /// her completion is the only event that can free the cells, and the pool holds all three
    /// needs once hers are reclaimed. Assumptions: the recorded needs are feasible (each fits the
    /// pool), her preceding work is finite (her turns end at her recorded cadence) and the backend
    /// delivers its receipts (scripted).
    fn assert_every_waiter_served_within_the_bound(day: &Recorded, replay: &Replay) {
        let world = &replay.world;
        assert!(world.violations.is_empty(), "violations {:?}\n{}", world.violations, world.log.join("\n"));
        let grant = &day.whole_pool_grant.value;
        let waiters = (0..world.len()).filter(|i| *i != replay.holder);
        for (k, waiter) in waiters.enumerate() {
            let bound = k as u64 + 1;
            let served = replay.first_served_after_turns.get(&waiter);
            assert!(
                served.is_some_and(|&turns| turns <= bound),
                "{} was not served by the holder's completed turn {bound} (served after {served:?} turns; {} of her turns \
                 completed in the window of {} recorded acts)\n{}",
                world.name(waiter),
                replay.holder_turns,
                day.aiko_turns.value.acts,
                world.log.join("\n"),
            );
        }
        assert_eq!(world.len(), grant.residents.len());
    }

    // what this catches (the M5, 2026-11-04 20:15-20:35Z, `serving.grants` pool 125,696 ->
    // {Aiko: 114,592}, three residents dark 20 min): one censored bound holding the whole pool.
    // Aiko's turns visibly end (the poll reads her between turns on the tick after each); her
    // completion is a feasible opportunity, and the production owner must hand her cells to the
    // three exact ~40k needs through verified physical reclamation.
    #[test]
    fn one_bound_never_takes_the_pool_every_waiter_is_served_within_the_bound() {
        let day = recorded();
        let replay = replay(&day, Restart::AfterNextTick, day.grant_tick_ms.value);
        assert_every_waiter_served_within_the_bound(&day, &replay);
    }

    // what this catches (the M5, 2026-11-04: Aiko 28 acts in 20 min, one every ~43 s against the
    // 5 s grant tick): fairness read off a polled `in_flight`. Each of her turns completes and the
    // next starts between two ticks, so every tick reads her in flight. The owner consumes her
    // terminal receipt (completion identity), bars her next reserve at that boundary and hands her
    // cells off at the next publication.
    #[test]
    fn a_turn_that_completes_and_restarts_between_ticks_still_yields_to_the_waiter() {
        let day = recorded();
        for receipt_after_ticks in [1, 2, 7] {
            let replay = replay(&day, Restart::BetweenTicks, receipt_after_ticks * day.grant_tick_ms.value);
            assert_every_waiter_served_within_the_bound(&day, &replay);
        }
    }

    // what this catches: the reproductions above passing because the recorded start was not the
    // day's. The superseded composition (bare demand policy, polled `in_flight`) keeps
    // {Aiko: 114,592} through every one of her 28 completed turns and grants no one else: the
    // receipt, reproduced, and the starvation the 2026-11-06 15:25Z audit read as 28 unserved
    // completions, preserved as evidence.
    #[test]
    fn the_superseded_composition_reproduces_the_recorded_whole_pool_grant() {
        let day = recorded();
        let played = play_superseded(&day);
        let grant = &day.whole_pool_grant.value;
        assert_eq!(played.last_grants, BTreeMap::from([(grant.holder, grant.held)]));
        assert!(played.first_served_after_turns.is_empty());
        assert_eq!(played.holder_turns, day.aiko_turns.value.acts, "every recorded completion passed unserved");
    }
}

mod view {
    use super::*;

    // what this catches (the M5, 2026-11-04 18:49-19:22Z: Aris appended 17-43k chars per act on a
    // ~44k-token view, ~89k-token prompts, 9 acts in 30 min, against Aiko's ~20k prompts and 28
    // acts in 20 min): history kept verbatim. Older items must collapse to handles so prompt
    // tokens per act stay at the working set; at the M5's measured 148 tok/s a cold re-prefill of
    // Aris's prompt costs ~600 s, of Aiko's ~135 s.
    #[test]
    #[ignore = "ONE-ACT slice 2: older items collapse to handles; prompt tokens per act stay at the working set"]
    fn an_overflowing_view_collapses_older_items_to_handles() {
        let day = recorded();
        let view = day.aris_view.value;
        let [low, high] = view.append_chars_per_act;
        let cold_s = |tokens: u64| tokens / day.m5_prefill_tok_per_s_large.value;
        panic!(
            "{} (Aris: {low}-{high} chars appended per act on a {}-token view, {} prompt tokens, ~{} s cold; \
             Aiko: {}, ~{} s)",
            MissingSeam::ViewTurnoverPolicy.describe(),
            view.view_tokens,
            view.prompt_tokens,
            cold_s(view.prompt_tokens),
            view.aiko_prompt_tokens,
            cold_s(view.aiko_prompt_tokens),
        );
    }
}

mod censored_demand {
    //! Idle pool, one mind, censored demand (Aris on the M5, 2026-11-13, canary 2b17c41df): alone on
    //! a 247,040-token pool, her grant fell 109,745 -> 43,807 in 50 minutes, every step 1.25 x a
    //! demand measured from her collapsed view, while her priced need read at least 56,902-99,441.
    //! Her real requirement path drives each publish (`LaneRequirement` -> `Demand::of` ->
    //! `DemandFollowing`); the measurement is the arm: her view as rendered after its last
    //! collapse (before), or at full detail (after: `View::collapsed_growth`).
    use crate::capacity::window_grant::{Demand, DemandFollowing, WindowPolicy};
    use crate::inference::planning::window_allocator::observed::Observed;
    use crate::inference::planning::window_allocator::{test_tokens as t, Growth, LaneRequirement};
    use std::collections::BTreeMap;

    /// `serving.grants` pool_tokens and per_sequence_cap, 21:40:00Z.
    const POOL: u32 = 247_040;
    /// Qwen3.6-35B-A3B's trained context.
    const TRAINED: u32 = 262_144;
    /// Her grant at 20:42Z, the start of the recorded window.
    const FIRST_GRANT: u32 = 109_745;
    /// Her largest live priced bound (`persona.need.censored` at_least, 21:39:06Z) and her
    /// irreducible window (`working-set.json` floor_tokens): what her need ring reads.
    const BOUND: u32 = 99_441;
    const FLOOR: u32 = 26_802;
    /// What her act holds at full detail: her demand on the act before the window's first collapse
    /// (`delib.turn.demand` 20:47:48Z, 67,828), inside her recorded priced bounds.
    const FULL_DETAIL: u32 = 67_828;
    /// Her `delib.turn.demand` per act, 20:42-21:44Z, as her collapsed view measured it.
    const RECORDED: [u32; 30] = [
        49_943, 54_914, 59_643, 67_828, 47_165, 35_046, 39_144, 25_870, 34_135, 37_960, 36_960, 34_219, 34_709, 37_668, 36_287,
        15_592, 15_592, 34_508, 34_508, 37_283, 41_123, 41_123, 54_885, 44_309, 45_891, 47_018, 45_128, 40_944, 49_415, 54_792,
    ];

    /// Her wish ring as the working set keeps it (`working_set::requirement_tokens`: the upper
    /// median of her last 16 demands, at least her newest when it overflowed her window; her
    /// growth: `Growth::of` over the same ring, measured once it holds 16).
    const RING: usize = 16;
    #[derive(Default)]
    struct Ring {
        demands: Vec<u32>,
        overflow: Option<u32>,
    }

    impl Ring {
        fn record(&mut self, demand: u32, served: u32) {
            self.demands.push(demand);
            if self.demands.len() > RING {
                self.demands.remove(0);
            }
            self.overflow = (demand > served).then_some(demand);
        }

        fn wish(&self) -> u32 {
            let mut sorted = self.demands.clone();
            sorted.sort_unstable();
            let typical = sorted[sorted.len() / 2];
            self.overflow.map_or(typical, |overflow| typical.max(overflow)) // map_or: a turn that fit leaves her typical turn
        }

        fn growth(&self) -> Growth {
            Growth::of(&self.demands, RING)
        }
    }

    /// One publish on the idle pool: her requirement, her demand, her grant from what she holds.
    fn publish(ring: &Ring, held: u32) -> u32 {
        let aris = uuid::Uuid::from_u128(0x8044b75b);
        let need = Observed::AtLeast { at_least: t(BOUND) }.over(Some(t(FLOOR)));
        let requirement = LaneRequirement::wished(aris, t(ring.wish()), ring.growth()).held(need);
        let demand = Demand::of(&requirement, t(TRAINED), POOL, false).expect("test: a held requirement is a demand"); // expect: wished and held, so measured
        let grants = DemandFollowing.grant(&[Demand { settled: true, ..demand }], &BTreeMap::from([(aris, held)]), POOL);
        grants[&aris]
    }

    /// Her grants act by act, and how many acts overflowed (her full-detail act past her grant:
    /// her view collapsed at its next start), measuring each act with `measure(grant, act)`.
    fn play(first: u32, ring: &mut Ring, measure: impl Fn(u32, usize) -> u32) -> (Vec<u32>, usize) {
        let (mut grant, mut grants, mut overflows) = (first, Vec::new(), 0);
        for act in 0..RECORDED.len() {
            overflows += usize::from(FULL_DETAIL > grant);
            ring.record(measure(grant, act), grant);
            grant = publish(ring, grant);
            grants.push(grant);
        }
        (grants, overflows)
    }

    // what this catches: the recorded loop, reproduced through the real grant path. Measured from
    // her collapsed render, her requirement follows the collapse down: her window falls on an idle
    // pool, and she overflows act after act. With the recorded 1.25 headroom her last grant read
    // 57,750, below her whole act; grown by her own ring's growth (p95 over median, measured from
    // the 16th act) it ends near 73k, above it, and she still overflows 17 of 30 acts: the growth
    // does not cure a requirement measured from a collapse, the full-detail measure does.
    #[test]
    fn measured_from_her_collapsed_view_her_window_falls_and_she_overflows() {
        let (grants, overflows) = play(FIRST_GRANT, &mut Ring::default(), |_, act| RECORDED[act]);
        let last = *grants.last().expect("test: a recorded act publishes"); // expect: RECORDED is nonempty
        assert!(last < FIRST_GRANT, "her window fell on an idle pool: {grants:?}");
        assert!(overflows >= RECORDED.len() / 2, "{overflows} of {} acts overflowed: {grants:?}", RECORDED.len());
    }

    // what this catches: the fix. Measured at full detail, no act overflows while the pool holds her
    // need, and her window never falls below it on an idle pool, from the grant she held.
    #[test]
    fn measured_at_full_detail_her_window_holds_her_whole_act_on_an_idle_pool() {
        let (grants, overflows) = play(FIRST_GRANT, &mut Ring::default(), |_, _| FULL_DETAIL);
        assert_eq!(overflows, 0, "{grants:?}");
        assert!(grants.iter().all(|&g| g >= FULL_DETAIL), "{grants:?}");
        assert!(grants.windows(2).all(|pair| pair[1] >= pair[0]), "no fall on an idle pool: {grants:?}");
    }

    // what this catches: the upward probe. From her recorded 43,807 with a ring her collapses filled
    // (the recorded demands), one act measured at full detail overflows, and the next grant holds it:
    // she overflows once, never again while the pool holds her need.
    #[test]
    fn one_overflow_at_full_detail_is_granted_at_the_next_publish() {
        let mut ring = Ring::default();
        for demand in &RECORDED[RECORDED.len() - 16..] {
            ring.record(*demand, 43_807);
        }
        let (grants, overflows) = play(43_807, &mut ring, |_, _| FULL_DETAIL);
        assert_eq!(overflows, 1, "{grants:?}");
        assert!(grants[0] >= FULL_DETAIL, "the next publish holds what overflowed: {grants:?}");
    }
}
