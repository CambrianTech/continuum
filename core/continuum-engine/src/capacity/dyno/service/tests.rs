//! The interleavings section 4 lists, and seeded generated schedules, through the production
//! grant owner. The recorded window (dyno 2 and 3) is asserted with the other reproductions in
//! `dyno/tests.rs`.
#![cfg(test)]

use super::clock::{shrink, Draws};
use super::opportunity::{NoOpportunity, Opportunity};
use super::world::{Phase, Step, Violation, World};
use crate::capacity::window_grant::ReserveRefused;

/// Two minds whose needs do not fit together: A is admitted first, B waits.
fn two_on_a_small_pool() -> World {
    World::new(100, 100, &[("A", 60), ("B", 60)])
}

fn run(world: &mut World, steps: &[Step]) -> Vec<super::world::Applied> {
    steps.iter().map(|step| world.apply(*step)).collect()
}

fn turn(i: usize) -> [Step; 3] {
    [Step::Quote(i), Step::Reserve(i), Step::Send(i)]
}

fn assert_clean(world: &World) {
    assert!(world.violations.is_empty(), "violations {:?}\n{}", world.violations, world.log.join("\n"));
}

/// Characterization of the missing preparation interval, NOT the desired service contract.
/// Windows 2026-11-12T23:14:32Z measured a 21,566 ms restore, followed by an
/// ungranted verdict and immediate save. Only that duration is recorded here;
/// the two 10 s publications and the small pool are scripted interleavings.
/// This drives the real grant owner, but does not simulate copies or claim that
/// a quoted lease accounts for physical paging custody.
#[test]
fn slow_preparation_can_lose_its_grant_before_dispatch() {
    use super::clock::Clock;

    for (preparation_ms, loses_grant) in [(1, false), (21_566, true)] {
        let mut world = two_on_a_small_pool();
        world.apply(Step::Publish);
        world.apply(Step::Quote(0));
        assert_eq!(world.phase(0), Phase::Quoted);
        let mut clock = Clock::new();
        clock.at(10_000, Step::Publish);
        clock.at(20_000, Step::Publish);
        clock.at(preparation_ms, Step::Reserve(0));
        while let Some(step) = clock.next() {
            let applied = world.apply(step);
            if step == Step::Reserve(0) {
                assert_eq!(applied.refused.is_some(), loses_grant, "{}", world.log.join("\n"));
                assert_eq!(clock.virtual_ms(), preparation_ms);
                world.apply(Step::Send(0));
                assert_eq!(world.phase(0) == Phase::Sent, !loses_grant);
                assert_clean(&world);
                break;
            }
        }
    }
}

/// Safety only: for a scenario about something else that crosses an engine replacement, where
/// the known missed opportunity (`an_engine_replacement_keeps_the_oldest_waiters_place`) recurs.
fn assert_safe(world: &World) {
    let unsafe_: Vec<_> = world.violations.iter().filter(|(_, _, v)| !matches!(v, Violation::MissedOpportunity { .. })).collect();
    assert!(unsafe_.is_empty(), "violations {unsafe_:?}\n{}", world.log.join("\n"));
}

mod interleavings {
    use super::*;

    // what this catches: a receipt from a replaced engine counted as service on its successor,
    // and a physical proof for the old engine's handoff freeing cells on the new one. A's turn is
    // sent on engine 7; engine 7 is replaced at the same URL; A's terminal arrives late, then the
    // old handoff's physical receipt. Nothing of either reaches the new engine's publication.
    #[test]
    fn late_receipts_from_a_replaced_engine_credit_nothing_on_its_successor() {
        let mut world = two_on_a_small_pool();
        run(&mut world, &[Step::Publish]);
        run(&mut world, &turn(0));
        run(&mut world, &[Step::Terminal(0), Step::Publish]);
        let ticketed = world.observe().retiring.len();
        assert_eq!(ticketed, 1, "A completed with B waiting: the owner begins her handoff\n{}", world.log.join("\n"));
        run(&mut world, &turn(0));
        run(&mut world, &[Step::ReplaceEngine, Step::Terminal(0), Step::Reclaimed(0), Step::Publish]);
        assert!(world.observe().retiring.is_empty(), "the old engine's handoff is fenced");
        assert_eq!(world.observe().charged.values().sum::<u32>(), 0, "nothing of engine 7 is charged on engine 8");
        assert_safe(&world);
    }

    // what this catches: custody released on cancellation, or held where nothing can be running.
    // Cancelled after the quote: nothing is reserved. Cut after the POST: the reservation stays
    // held, unresolved, and blocks a handoff, until a fence ends it.
    #[test]
    fn cancellation_after_quote_holds_nothing_and_after_post_holds_custody_until_a_fence() {
        let mut world = two_on_a_small_pool();
        run(&mut world, &[Step::Publish, Step::Quote(0), Step::CancelQuoted(0)]);
        assert!(world.observe().charged.is_empty(), "a cancelled quote reserves nothing");
        run(&mut world, &turn(0));
        run(&mut world, &[Step::Disconnect(0)]);
        assert_eq!(world.observe().unresolved.get(&world.mind(0)), Some(&1), "a cut stream is no terminal receipt");
        let applied = world.apply(Step::Publish);
        assert!(applied.tickets.is_empty(), "unresolved custody is never handed off");
        run(&mut world, &[Step::ReplaceEngine]);
        assert!(world.observe().unresolved.is_empty(), "a fence ends unresolved custody");
        assert_safe(&world);
    }

    // what this catches (ONE-ACT 4a validator matrix: "cancellation ... before POST: proven-unsent
    // release"): a reservation dropped before its POST is held unresolved like a request the engine
    // may be running, so the holder's cells stay charged until an engine fence. No request was
    // sent; the dispatch path has no settlement for "proven unsent" except a retry's re-reserve.
    #[test]
    fn a_cancellation_before_post_releases_its_proven_unsent_reservation() {
        let mut world = two_on_a_small_pool();
        run(&mut world, &[Step::Publish, Step::Quote(0), Step::Reserve(0), Step::CancelReserved(0)]);
        let held = world.observe();
        assert!(held.unresolved.is_empty() && held.charged.is_empty(), "nothing was sent, nothing is held: {held:?}");
    }

    // what this catches: a lease from a replaced engine reserving on its successor (same URL, same
    // port, a new pid and start time). The quote made on engine 7 is refused typed on engine 8:
    // nothing is POSTed under it.
    #[test]
    fn a_same_url_replacement_refuses_the_old_lease() {
        let mut world = two_on_a_small_pool();
        run(&mut world, &[Step::Publish, Step::Quote(0), Step::ReplaceEngine, Step::Publish]);
        let applied = world.apply(Step::Reserve(0));
        assert!(matches!(applied.refused, Some(ReserveRefused::Changed { .. })), "{:?}", applied.refused);
        assert_eq!(world.phase(0), Phase::Idle, "re-render against the new grant; nothing sent");
        assert_safe(&world);
    }

    // what this catches: a waiter granted cells a pinned holder may still be decoding into, or a
    // pinned wait reported as an ordinary one. Every holder's request is unresolved: the boundary
    // names the custody, the waiter is not granted, and conservation holds.
    #[test]
    fn an_all_pinned_wait_names_its_custody_and_grants_nothing() {
        let mut world = two_on_a_small_pool();
        run(&mut world, &[Step::Publish]);
        run(&mut world, &turn(0));
        run(&mut world, &[Step::Disconnect(0)]);
        let applied = world.apply(Step::Publish);
        assert_eq!(
            applied.opportunity,
            Some(Err(NoOpportunity::CustodyOutstanding { waiter: world.mind(1), holders: vec![world.mind(0)] })),
        );
        assert_eq!(world.published(1), 0, "the waiter is not granted cells under unresolved custody");
        assert_clean(&world);
    }

    // what this catches (found by this harness, 2026-11-06): a fence that forgets the queue. B has
    // waited since the first publication behind A's unresolved custody; the engine is replaced,
    // the fence frees the pool, and the next publication's feasible opportunity is B's (the oldest
    // waiter, and the pool now holds her need). The owner's fair state resets on a new incarnation
    // (`FairGrantState` is rebuilt when `fair.engine != engine`), so admission restarts in demand
    // order and A is granted first.
    #[test]
    #[ignore = "ONE-ACT 4: an engine replacement keeps the oldest waiter's place (fair state resets on a new incarnation)"]
    fn an_engine_replacement_keeps_the_oldest_waiters_place() {
        let mut world = two_on_a_small_pool();
        run(&mut world, &[Step::Publish]);
        run(&mut world, &turn(0));
        run(&mut world, &[Step::Disconnect(0), Step::Publish, Step::ReplaceEngine]);
        let applied = world.apply(Step::Publish);
        assert!(matches!(applied.opportunity, Some(Ok(Opportunity::Grant { waiter, .. })) if waiter == world.mind(1)), "{:?}", applied.opportunity);
        assert_clean(&world);
    }

    // what this catches: a grant revoked between the quote and the reserve still dispatching. A's
    // turn ends with B waiting; A renders her next act against the grant still published; the
    // publication hands her grant off; her reserve is refused typed and nothing is sent.
    #[test]
    fn a_grant_revoked_during_the_quote_refuses_the_reserve() {
        let mut world = two_on_a_small_pool();
        run(&mut world, &[Step::Publish]);
        run(&mut world, &turn(0));
        run(&mut world, &[Step::Terminal(0), Step::Quote(0), Step::Publish]);
        assert_eq!(world.published(0), 0, "her publication is retiring");
        let applied = world.apply(Step::Reserve(0));
        assert!(matches!(applied.refused, Some(ReserveRefused::Changed { current: None, .. } | ReserveRefused::Retiring { .. })), "{:?}", applied.refused);
        assert_eq!(world.phase(0), Phase::Idle);
        run(&mut world, &[Step::Reclaimed(0), Step::Publish]);
        assert_eq!(world.published(1), 60, "the waiter is served once the cells are physically reclaimed");
        assert_clean(&world);
    }
}

mod generated {
    use super::*;

    /// One seeded schedule over three minds whose needs never all fit. `replacements` decides
    /// whether a same-URL engine replacement may be drawn.
    fn schedule(seed: u64, len: usize, replacements: bool) -> Vec<Step> {
        let mut draws = Draws::new(seed);
        (0..len)
            .map(|_| {
                let i = draws.below(3) as usize;
                match draws.below(if replacements { 13 } else { 12 }) {
                    0 | 1 => Step::Publish,
                    2 => Step::Quote(i),
                    3 => Step::Reserve(i),
                    4 => Step::Send(i),
                    5 | 6 => Step::Terminal(i),
                    7 => Step::Disconnect(i),
                    8 => Step::CancelQuoted(i),
                    9 => Step::Reclaimed(i),
                    10 => Step::RefusedHandoff(i),
                    11 => Step::CancelReserved(i),
                    _ => Step::ReplaceEngine,
                }
            })
            .collect()
    }

    fn violations(steps: &[Step]) -> Vec<(u64, Step, Violation)> {
        let mut world = World::new(100, 100, &[("A", 60), ("B", 40), ("C", 30)]);
        run(&mut world, steps);
        world.violations
    }

    /// The shortest failing schedule over `seeds`, shrunk; `None` when every seed holds.
    fn shortest_failure(seeds: std::ops::Range<u64>, replacements: bool) -> Option<(u64, Vec<Step>, Vec<(u64, Step, Violation)>)> {
        let mut shortest: Option<(u64, Vec<Step>, Vec<(u64, Step, Violation)>)> = None;
        for seed in seeds {
            let steps = schedule(seed, 80, replacements);
            if violations(&steps).is_empty() {
                continue;
            }
            let shrunk = shrink(steps, |s| !violations(s).is_empty());
            if shortest.as_ref().is_none_or(|(_, s, _)| shrunk.len() < s.len()) {
                let found = violations(&shrunk);
                shortest = Some((seed, shrunk, found));
            }
        }
        shortest
    }

    // what this catches: any interleaving of publications, quotes, reserves, sends, terminal and
    // cut streams, cancellations before and after the reserve, and physical receipts (granted or refused) that breaks
    // conservation, custody, a legal transition or the feasible-opportunity contract. 300 seeded
    // schedules of 80 steps; a failure is reported as its shortest schedule.
    #[test]
    fn seeded_schedules_keep_every_invariant_on_one_engine() {
        if let Some((seed, steps, found)) = shortest_failure(0..300, false) {
            panic!("seed {seed}: shortest failing schedule {steps:?}\nviolations {found:?}");
        }
    }

    // what this catches: the same, with same-URL engine replacements drawn among the steps.
    #[test]
    #[ignore = "ONE-ACT 4: an engine replacement keeps the oldest waiter's place (fair state resets on a new incarnation)"]
    fn seeded_schedules_keep_every_invariant_across_engine_replacements() {
        if let Some((seed, steps, found)) = shortest_failure(0..300, true) {
            panic!("seed {seed}: shortest failing schedule {steps:?}\nviolations {found:?}");
        }
    }
}
