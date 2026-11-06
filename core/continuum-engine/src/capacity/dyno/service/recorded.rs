//! The recorded 2026-11-04 whole-pool window (dyno 2 and 3) on the virtual clock, through the
//! production composition: the daemon's 5 s publication tick, Aiko's turns ending at her recorded
//! cadence and restarting between ticks, the three dark residents' exact needs, and scripted
//! physical receipts for every handoff ticket the owner issues.
//!
//! Recorded: the pool, the holder's grant, every need, the tick and Aiko's cadence
//! (`dyno/2026-11-04.json`). Scripted (SHAPE, not receipts): a granted waiter's own turns run at
//! Aiko's cadence (the dark residents had no recorded turns), and a physical receipt lands
//! `reclaim_after_ms` after its ticket. The bound is stated in feasible opportunities, so neither
//! value decides whether a waiter is served; the generated schedules vary both.
#![cfg(test)]

use std::collections::BTreeMap;

use super::clock::Clock;
use super::world::{Phase, Step, World};
use crate::capacity::dyno::recorded::{Recorded, RecordedNeed};
use crate::inference::planning::window_allocator::observed::Observed;
use crate::inference::planning::window_allocator::test_tokens as t;

#[derive(Debug, Clone, Copy)]
enum Event {
    Publish,
    TurnEnds(usize),
    Physical(usize),
}

/// What the recorded window measured through the production owner.
pub(crate) struct Replay {
    pub world: World,
    /// For each waiter (by demand index), the holder's completed turns when she was first
    /// granted her whole need.
    pub first_served_after_turns: BTreeMap<usize, u64>,
    pub holder: usize,
    pub holder_turns: u64,
}

/// When a mind's next turn starts after one ends: what the daemon's polled `turn_in_flight` can
/// see at the next tick (dyno 2 and 3 differ only in it; the demand carries the polled read).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Restart {
    /// At once: every tick reads her in flight (dyno 3).
    BetweenTicks,
    /// After the next tick: that tick reads her between turns (dyno 2).
    AfterNextTick,
}

/// Play the recorded window through the production composition.
pub(crate) fn replay(day: &Recorded, restart: Restart, reclaim_after_ms: u64) -> Replay {
    let grant = &day.whole_pool_grant.value;
    let pool = grant.pool;
    let residents: Vec<(&str, u32)> = grant
        .residents
        .iter()
        .map(|r| {
            let need = match r.need {
                RecordedNeed::Exact(n) => Observed::Exact(t(n)).within(pool),
                RecordedNeed::AtLeast(n) => Observed::AtLeast { at_least: t(n) }.within(pool),
            };
            (r.name.as_str(), need.get())
        })
        .collect();
    let holder = grant.residents.iter().position(|r| r.mind == grant.holder).expect("test: the holder is a recorded resident");
    let mut world = World::new(pool, pool, &residents);
    world.start_published(&[(holder, grant.held)]);
    let act_ms = day.aiko_turns.value.act_ms();
    let tick_ms = day.grant_tick_ms.value;
    let mut clock = Clock::new();
    for tick in 1..=day.aiko_turns.value.window_ms / tick_ms {
        clock.at(tick * tick_ms, Event::Publish);
    }
    let mut wants_turn = vec![false; world.len()];
    let start = |world: &mut World, clock: &mut Clock<Event>, wants: &mut Vec<bool>, i: usize| {
        for step in [Step::Quote(i), Step::Reserve(i), Step::Send(i)] {
            world.apply(step);
        }
        wants[i] = world.phase(i) != Phase::Sent;
        if !wants[i] {
            clock.after(act_ms, Event::TurnEnds(i));
        }
    };
    start(&mut world, &mut clock, &mut wants_turn, holder);
    let mut replay = Replay { world, first_served_after_turns: BTreeMap::new(), holder, holder_turns: 0 };
    while let Some(event) = clock.next() {
        let world = &mut replay.world;
        match event {
            Event::Publish => {
                let applied = world.apply(Step::Publish);
                for i in applied.tickets {
                    clock.after(reclaim_after_ms, Event::Physical(i));
                }
                for i in applied.newly_served {
                    replay.first_served_after_turns.entry(i).or_insert(replay.holder_turns);
                    wants_turn[i] = true;
                }
                for i in 0..world.len() {
                    if wants_turn[i] && world.published(i) > 0 {
                        start(world, &mut clock, &mut wants_turn, i);
                    }
                }
            }
            Event::TurnEnds(i) => {
                world.apply(Step::Terminal(i));
                if i == holder {
                    replay.holder_turns += 1;
                }
                match restart {
                    Restart::BetweenTicks => start(world, &mut clock, &mut wants_turn, i),
                    Restart::AfterNextTick => wants_turn[i] = true,
                }
            }
            Event::Physical(i) => {
                world.apply(Step::Reclaimed(i));
            }
        }
    }
    replay
}
