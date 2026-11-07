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
        Demand { mind: resident.mind, need, target: need, wish: need, in_flight, settled: true, ceiling: pool }
    }

    /// What one recorded window measured: for each waiter, the count of the holder's completed
    /// turns when she was first granted her need; and every tick's grants within the pool.
    struct Played {
        first_served_after_turns: BTreeMap<u32, u64>,
        over_pool_ticks: u32,
        last_grants: BTreeMap<u32, u32>,
    }

    /// Play the recorded window through the production grant policy, one regrant per recorded
    /// daemon tick, from the recorded grant. The holder's turns end at her recorded cadence;
    /// `polled_in_flight(ended_since_last_tick)` is what the daemon's `turn_in_flight` poll read
    /// at that tick. The waiters have no turn (they are dark).
    fn play(day: &Recorded, polled_in_flight: impl Fn(bool) -> bool) -> Played {
        let grant = &day.whole_pool_grant.value;
        let (pool, holder) = (grant.pool, grant.holder);
        let tick_ms = day.grant_tick_ms.value;
        let cadence = day.aiko_turns.value;
        let turns_ended = |tick: u64| tick * tick_ms / cadence.act_ms();
        let mut held = BTreeMap::from([(holder, grant.held)]);
        let mut played = Played { first_served_after_turns: BTreeMap::new(), over_pool_ticks: 0, last_grants: BTreeMap::new() };
        for tick in 1..=cadence.window_ms / tick_ms {
            let ended_since_last_tick = turns_ended(tick) > turns_ended(tick - 1);
            let demands: Vec<Demand<u32>> = grant
                .residents
                .iter()
                .map(|r| demand(r, pool, r.mind == holder && polled_in_flight(ended_since_last_tick)))
                .collect();
            let grants = DemandFollowing.grant(&demands, &held, pool);
            if grants.values().map(|&g| u64::from(g)).sum::<u64>() > u64::from(pool) {
                played.over_pool_ticks += 1;
            }
            for d in demands.iter().filter(|d| d.mind != holder) {
                if grants.get(&d.mind).is_some_and(|&g| g >= d.need.get()) {
                    played.first_served_after_turns.entry(d.mind).or_insert(turns_ended(tick));
                }
            }
            played.last_grants = grants.clone();
            held = grants;
        }
        played
    }

    /// The bound the design states (ONE-ACT stage 4): a waiter owed service is admitted at the
    /// next completed turn of a served holder, so with one holder the k-th waiter is served by the
    /// holder's k-th completed turn.
    fn assert_every_waiter_served_within_the_bound(day: &Recorded, played: &Played) {
        assert_eq!(played.over_pool_ticks, 0, "grants never exceed the pool");
        let grant = &day.whole_pool_grant.value;
        let waiters = grant.residents.iter().filter(|r| r.mind != grant.holder);
        for (k, waiter) in waiters.enumerate() {
            let bound = k as u64 + 1;
            let served = played.first_served_after_turns.get(&waiter.mind);
            assert!(
                served.is_some_and(|&turns| turns <= bound),
                "{} was not served by the holder's completed turn {bound} (served after {served:?} turns; \
                 {} of her turns completed in the window); last grants {:?}",
                waiter.name,
                day.aiko_turns.value.acts,
                played.last_grants,
            );
        }
    }

    // what this catches (the M5, 2026-11-04 20:15-20:35Z, `serving.grants` pool 125,696 ->
    // {Aiko: 114,592}, three residents dark 20 min): one censored bound holding the whole pool.
    // Aiko's turns visibly end (the poll reads her between turns on the tick after each), yet the
    // three exact ~40k needs are never granted, because her bound is kept as her floor.
    #[test]
    #[ignore = "ONE-ACT slice 5: every mind is served within the stated bound (one bound never takes the pool)"]
    fn one_bound_never_takes_the_pool_every_waiter_is_served_within_the_bound() {
        let day = recorded();
        let played = play(&day, |ended_since_last_tick| !ended_since_last_tick);
        assert_every_waiter_served_within_the_bound(&day, &played);
    }

    // what this catches (the M5, 2026-11-04: Aiko 28 acts in 20 min, one every ~43 s against the
    // 5 s grant tick): fairness read off a polled `in_flight`. Each of her turns completes and the
    // next starts between two ticks, so every tick reads her in flight and a turn in flight keeps
    // what it holds: the waiter is never served. The design consumes completion identities (the
    // turn ledger's ended-turn count), which `Demand` does not carry yet.
    #[test]
    #[ignore = "ONE-ACT slice 5: a waiter is served at the holder's next completed turn (completion identity, not a polled in_flight)"]
    fn a_turn_that_completes_and_restarts_between_ticks_still_yields_to_the_waiter() {
        let day = recorded();
        let played = play(&day, |_ended| true);
        assert_every_waiter_served_within_the_bound(&day, &played);
    }

    // what this catches: the reproductions above passing because the recorded start was not
    // today's. From the recorded grant, today's policy keeps {Aiko: 114,592} and grants no one
    // else: the receipt, reproduced.
    #[test]
    fn todays_policy_reproduces_the_recorded_whole_pool_grant() {
        let day = recorded();
        let played = play(&day, |_ended| true);
        let grant = &day.whole_pool_grant.value;
        assert_eq!(played.last_grants, BTreeMap::from([(grant.holder, grant.held)]));
        assert!(played.first_served_after_turns.is_empty());
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
