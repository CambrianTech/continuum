use super::*;

use crate::inference::planning::window_allocator::test_tokens as t;
use super::schedule::{play, Busy, Restart, Served};

/// One grant from a fresh policy: no rotation history.
fn grant_once(demands: &[Demand<u32>], held: &BTreeMap<u32, u32>, pool: u32) -> BTreeMap<u32, u32> {
    DemandFollowing::default().grant(demands, held, pool)
}

/// A mind whose need is her target (a wish with no held turn yet).
fn demand(mind: u32, need: u32, in_flight: bool, ceiling: u32) -> Demand<u32> {
    Demand { mind, need: Observed::Exact(t(need)), floor: Floor::Unmeasured, target: t(need), wish: t(need), in_flight, turns_ended: TurnsEnded::default(), ceiling }
}

// what this catches: a window divided by a count (Joel, 2026-11-03: "What a mind needs not what
// you dictate"). Minds that fit get exactly what they need, never an equal split, never past one
// sequence's ceiling; what is left stays in the pool, granted to no one.
#[test]
fn every_mind_gets_what_she_needs_and_the_rest_stays_in_the_pool() {
    let pool = 257_536;
    let demands = [demand(1, 70_000, false, pool), demand(2, 20_000, false, pool), demand(3, 10_000, false, pool)];
    let grants = grant_once(&demands, &BTreeMap::new(), pool);
    assert_eq!(grants, BTreeMap::from([(1, 70_000), (2, 20_000), (3, 10_000)]));
    let too_big = grant_once(&[demand(1, 40_000, false, 32_768)], &BTreeMap::new(), pool);
    assert!(too_big.is_empty(), "a need above what one sequence holds now waits, never trimmed: {too_big:?}");
}

// what this catches (the M5, 2026-11-03, 2bbfa56c9): a grant below need. The 2,048-token launch
// pool was shared in proportion, binding every resident to 409-819 tokens. A mind gets her whole
// need or no grant (she keeps her binding and waits for room), never a fraction.
#[test]
fn a_grant_is_never_below_need_a_mind_that_does_not_fit_waits() {
    let grants = grant_once(
        &[demand(1, 20_000, false, 2_048), demand(2, 30_000, false, 2_048)],
        &BTreeMap::new(),
        2_048,
    );
    assert!(grants.is_empty(), "a 2,048 pool holds no one's need: nobody is bound to a sliver: {grants:?}");
    let pool = 65_536;
    let grants = grant_once(
        &[demand(1, 40_000, false, pool), demand(2, 20_000, false, pool), demand(3, 30_000, false, pool)],
        &BTreeMap::new(),
        pool,
    );
    assert!(grants.iter().all(|(m, g)| *g == [0, 40_000, 20_000, 30_000][*m as usize]), "whole needs only: {grants:?}");
    assert!(grants.values().map(|&g| u64::from(g)).sum::<u64>() <= u64::from(pool));
    assert_eq!(grants.len(), 2, "the third need does not fit and waits: {grants:?}");
}

// what this catches (the M5, 2026-11-03): every resident rebound every 2-3 minutes as her estimate
// drifted, each rebind resizing her view and dropping her prompt cache. Holding at least her need,
// she keeps her grant; a turn in flight keeps what it holds even when room must be made.
#[test]
fn a_held_grant_survives_drift_and_a_running_turn_keeps_its_grant() {
    let pool = 257_536;
    let held = BTreeMap::from([(1, 50_000), (2, 30_000)]);
    let drift = grant_once(&[demand(1, 44_000, false, pool), demand(2, 31_000, false, pool)], &held, pool);
    assert_eq!(drift[&1], 50_000, "her need drifted down: she keeps what she holds");
    assert_eq!(drift[&2], 31_000, "her need rose above what she holds: she grows to it");
    let pool = 65_536;
    let held = BTreeMap::from([(1, 40_000), (2, 20_000)]);
    let tight = grant_once(
        &[demand(1, 20_000, true, pool), demand(2, 10_000, false, pool), demand(3, 30_000, false, pool)],
        &held,
        pool,
    );
    assert_eq!(tight[&1], 40_000, "a turn in flight keeps what it holds");
    assert_eq!(tight[&2], 10_000, "a mind not in a turn gives back her surplus to make room");
    assert!(!tight.contains_key(&3) || tight[&3] == 30_000, "a newcomer gets her whole need or waits");
}


// what this catches (Cormac on #5745): ordered admission overspending the pool. A mind left out
// keeps her binding, so what is bound (grants, plus the bindings of minds without one) never
// exceeds the pool. A holds 40k rising to 50k, B holds 25k, pool 65,536: B's 25k is a floor, A
// grows only into what is left.
#[test]
fn what_is_bound_never_exceeds_the_pool() {
    let pool = 65_536;
    let held = BTreeMap::from([(1, 40_000), (2, 25_000)]);
    let demands = [demand(1, 50_000, false, pool), demand(2, 25_000, false, pool), demand(3, 20_000, false, pool)];
    let grants = grant_once(&demands, &held, pool);
    let bound: u64 = demands
        .iter()
        .map(|d| u64::from(grants.get(&d.mind).copied().or_else(|| held.get(&d.mind).copied()).unwrap_or(0)))
        .sum();
    assert!(bound <= u64::from(pool), "bound {bound} on a {pool} pool: {grants:?}");
    assert_eq!(grants.get(&2), Some(&25_000), "B keeps her binding as a floor");
    assert!(grants.get(&1).is_some_and(|&g| g >= 40_000), "A keeps at least what she holds: {grants:?}");
    assert!(!grants.contains_key(&3), "the newcomer waits: no room for her whole need");
}


// what this catches (Cormac on #5745): a holder whose need outgrew one sequence dropped from the
// grants while keeping her binding, so a newcomer's grant bound past the pool. She keeps what she
// holds, counted; the newcomer is admitted only into what is left.
#[test]
fn a_holder_whose_need_outgrew_the_sequence_keeps_her_counted_binding() {
    let pool = 65_536;
    let held = BTreeMap::from([(1, 40_000)]);
    let demands = [demand(1, 70_000, false, pool), demand(2, 60_000, false, pool)];
    let grants = grant_once(&demands, &held, pool);
    assert_eq!(grants.get(&1), Some(&40_000), "she keeps what she holds: {grants:?}");
    let bound: u64 = demands
        .iter()
        .map(|d| u64::from(grants.get(&d.mind).copied().or_else(|| held.get(&d.mind).copied()).unwrap_or(0)))
        .sum();
    assert!(bound <= u64::from(pool), "bound {bound} on a {pool} pool: {grants:?}");
}


// what this catches (the M5, 2026-11-03, b7c815484): grants sized from her wish instead of her
// need. Benchy's wish took the model's whole 262,144 of a 524,288 pool, Aiko's 104,015 and
// Aris's 88,380, and Atlas got nothing. Needs are granted first; targets only when every target
// fits.
#[test]
fn needs_are_granted_before_any_target_and_targets_only_when_all_fit() {
    let pool = 524_288;
    let mind = |m, need, target| Demand { mind: m, need: Observed::Exact(t(need)), floor: Floor::Unmeasured, target: t(target), wish: t(target), in_flight: false, turns_ended: TurnsEnded::default(), ceiling: 262_144 };
    let wishes = [mind(1, 70_000, 262_144), mind(2, 60_000, 104_015), mind(3, 50_000, 88_380), mind(4, 40_000, 90_000)];
    let grants = grant_once(&wishes, &BTreeMap::new(), pool);
    assert_eq!(grants, BTreeMap::from([(1, 70_000), (2, 60_000), (3, 50_000), (4, 40_000)]), "every need, no target: the targets sum past the pool");
    let modest = [mind(1, 70_000, 100_000), mind(2, 60_000, 104_015), mind(3, 50_000, 88_380), mind(4, 40_000, 90_000)];
    let grants = grant_once(&modest, &BTreeMap::new(), pool);
    assert_eq!(grants, BTreeMap::from([(1, 100_000), (2, 104_015), (3, 88_380), (4, 90_000)]), "every target fits: each reads whole");
}


// what this catches (the M5, 2026-11-04 07:40Z): three minds settled at their needs beside one
// holding most of the pool, so their views kept 81-88 tokens of history and turned over on every
// act (55 turnovers in 66 acts, prefix reuse 0 on 46 of 69). Every target fits once the large
// holder gives back what she holds above hers: everyone is sized to her target. When everything
// fits as held, nobody shrinks.
#[test]
fn every_mind_grows_to_her_target_when_every_target_fits() {
    let pool = 323_840;
    let mind = |m, need, target| Demand { mind: m, need: Observed::Exact(t(need)), floor: Floor::Unmeasured, target: t(target), wish: t(target), in_flight: false, turns_ended: TurnsEnded::default(), ceiling: 262_144 };
    let demands = [mind(1, 19_384, 78_000), mind(2, 14_499, 35_000), mind(3, 16_004, 51_000), mind(4, 14_477, 42_000)];
    let held = BTreeMap::from([(1, 262_144), (2, 14_445), (3, 18_213), (4, 14_242)]);
    let grants = grant_once(&demands, &held, pool);
    assert_eq!(grants, BTreeMap::from([(1, 78_000), (2, 35_000), (3, 51_000), (4, 42_000)]), "every target: {grants:?}");
    let roomy = grant_once(&demands, &BTreeMap::from([(1, 100_000)]), pool);
    assert_eq!(roomy[&1], 100_000, "everything fits as held: she keeps her surplus, no rebind");
    assert_eq!(grant_once(&demands, &grants, pool), grants, "a steady demand is a steady grant");
}

// what this catches (Cormac on #5758): growing on every upward move of a target recomputed every
// act, which regranted most acts as an append-only view grew. Her wish rises 300 tokens a call
// and her target is the wish x 1.25: the grant holds while it covers the wish and regrants only
// when the wish passes it, about once per 25% of growth.
#[test]
fn a_rising_wish_regrants_only_when_it_passes_the_grant() {
    let pool = 323_840;
    let at = |wish: u32| Demand { mind: 1u32, need: Observed::Exact(t(10_000)), floor: Floor::Unmeasured, target: t(wish + wish / 4), wish: t(wish), in_flight: false, turns_ended: TurnsEnded::default(), ceiling: 262_144 };
    let mut held = BTreeMap::new();
    let mut regrants = 0;
    for call in 0..100u32 {
        let wish = 20_000 + 300 * call;
        let grants = grant_once(&[at(wish)], &held, pool);
        if held.get(&1) != grants.get(&1) {
            regrants += 1;
        }
        assert!(grants[&1] >= wish, "the grant always covers her wish: {} < {wish}", grants[&1]);
        held = grants;
    }
    // 20,000 -> 49,700 is about 2.5x: a grant regrows by 25% each time, so ~5 regrants, not ~100.
    assert!((3..=8).contains(&regrants), "{regrants} regrants over 100 calls of a rising wish");
}

// what this catches (the M5, 2026-11-03, d47c5c2a6: 22 rebinds in 15 minutes): a grant that moves
// when the regime flips. Every target fits, so each mind is granted her target; then one mind's
// need grows past what the targets leave. She is granted her need from the room, and every other
// mind, still holding at least her need, keeps exactly her grant: no rebind. What is bound stays
// within the pool.
#[test]
fn a_regime_flip_never_rebinds_a_mind_whose_grant_still_fits() {
    let pool = 200_000;
    let mind = |m, need, target| Demand { mind: m, need: Observed::Exact(t(need)), floor: Floor::Unmeasured, target: t(target), wish: t(target), in_flight: false, turns_ended: TurnsEnded::default(), ceiling: pool };
    let first = [mind(1, 14_000, 47_000), mind(2, 20_000, 53_000), mind(3, 20_000, 51_000), mind(4, 14_000, 45_000)];
    let held = grant_once(&first, &BTreeMap::new(), pool);
    assert_eq!(held, BTreeMap::from([(1, 47_000), (2, 53_000), (3, 51_000), (4, 45_000)]), "every target fits");
    // Mind 4's need grows to 49,000: the targets no longer all fit (47+53+51+60 > 200), but her need
    // fits in what the others hold above theirs (47+53+51+49 = 200).
    let flipped = [mind(1, 14_000, 47_000), mind(2, 20_000, 53_000), mind(3, 20_000, 51_000), mind(4, 49_000, 60_000)];
    let next = grant_once(&flipped, &held, pool);
    for m in [1, 2, 3] {
        assert_eq!(next.get(&m), held.get(&m), "mind {m} still holds at least her need: no rebind ({next:?})");
    }
    assert_eq!(next.get(&4), Some(&49_000), "the grower gets her need from what the others leave");
    assert!(next.values().map(|&g| u64::from(g)).sum::<u64>() <= u64::from(pool), "bound within the pool: {next:?}");
    // And the same grants again: nothing moves.
    assert_eq!(grant_once(&flipped, &next, pool), next, "a steady demand is a steady grant");
}

// what this catches (Cormac on #5773): a censored need above the ceiling dropped a mind holding
// nothing (`need > ceiling => holds = 0`). On fixed slots or a small pool she was then never
// granted, never took a turn, and her `AtLeast` bound never aged out of her ring: starvation
// forever. A bound is clamped to the ceiling, so she is granted all one sequence holds; an exact
// need above it still waits for room, as before.
#[test]
fn a_bound_above_the_ceiling_is_granted_the_ceiling_and_an_exact_need_waits() {
    let (pool, ceiling) = (654_336, 8_192);
    let at = |need: Observed<Tokens>| Demand { mind: 1u32, need, floor: Floor::Unmeasured, target: need.value(), wish: need.value(), in_flight: false, turns_ended: TurnsEnded::default(), ceiling };
    let bound = Observed::AtLeast { at_least: t(65_672) };
    let grants = grant_once(&[at(bound)], &BTreeMap::new(), pool);
    assert_eq!(grants, BTreeMap::from([(1, ceiling)]), "a bound past one sequence is granted one sequence, never dropped");
    let exact = grant_once(&[at(Observed::Exact(t(65_672)))], &BTreeMap::new(), pool);
    assert!(exact.is_empty(), "an exact need past the ceiling still waits for room: {exact:?}");
    // Negative control, the pre-fix read: the bound's value taken as her need drops her.
    let unclamped = grant_once(&[at(Observed::Exact(bound.value()))], &BTreeMap::new(), pool);
    assert!(unclamped.is_empty(), "precondition: the unclamped bound is the drop this fixes: {unclamped:?}");
    assert_eq!(Observed::AtLeast { at_least: t(4_096) }.within(ceiling), t(4_096), "a bound within the ceiling is untouched");
}


/// The M5 at 2026-11-04 20:15-20:35Z (87eccb3a7): a 125,696-token pool, Aiko's need a censored
/// bound of 114,592 and held, Aris, Atlas and Benchy each needing ~40k. Floors are this test's
/// inputs (each below her need), not a receipt. `floors: false` is the same minds unmeasured.
const RECEIPT_POOL: u32 = 125_696;
fn receipt_minds(floors: bool) -> ([Demand<u32>; 4], BTreeMap<u32, u32>) {
    let mind = |m, need: Observed<Tokens>, floor| Demand {
        mind: m,
        need,
        floor: match floors {
            true => Floor::Measured(t(floor)),
            false => Floor::Unmeasured,
        },
        target: need.value(),
        wish: need.value(),
        in_flight: false,
        turns_ended: TurnsEnded::default(),
        ceiling: RECEIPT_POOL,
    };
    let minds = [
        mind(1, Observed::AtLeast { at_least: t(114_592) }, 15_000),
        mind(2, Observed::Exact(t(41_000)), 18_000),
        mind(3, Observed::Exact(t(38_000)), 16_000),
        mind(4, Observed::Exact(t(42_000)), 20_000),
    ];
    (minds, BTreeMap::from([(1, 114_592)]))
}

// what this catches (the M5, 2026-11-04 20:15-20:35Z, 87eccb3a7): starvation. `serving.grants`
// granted Aiko's censored bound 114,592 of a 125,696 pool and nothing to Aris, Atlas and Benchy;
// in 20 minutes only Aiko acted (28 acts), three residents dark. Every mind is seated at her floor
// (69,000), exact needs grow whole from the 56,696 left (Aris +23,000, Atlas +22,000; Benchy's
// +22,000 no longer fits, she keeps her floor), and the bound takes the last 11,696 above hers.
#[test]
fn every_mind_holds_her_floor_and_a_bound_takes_only_what_the_floors_leave() {
    let (minds, held) = receipt_minds(true);
    let grants = grant_once(&minds, &held, RECEIPT_POOL);
    for d in &minds {
        let floor = match d.floor {
        Floor::Measured(floor) => floor.get(),
        Floor::Unmeasured => 0,
    };
        let granted = grants.get(&d.mind).copied();
        assert!(granted.is_some_and(|g| g >= floor), "mind {} starved below her floor {floor}: {grants:?}", d.mind);
    }
    for d in &minds[1..] {
        let g = grants[&d.mind];
        assert!(d.floor == Floor::Measured(t(g)) || g == d.need.value().get(), "an exact need is her floor or whole: {g}");
    }
    let others: u32 = minds[1..].iter().map(|d| grants[&d.mind]).sum();
    assert_eq!(grants[&1], RECEIPT_POOL - others, "the bound takes exactly what the others leave: {grants:?}");
    assert_eq!(grants, BTreeMap::from([(1, 26_696), (2, 41_000), (3, 38_000), (4, 20_000)]));
    assert_eq!(grant_once(&minds, &grants, RECEIPT_POOL), grants, "a steady demand is a steady grant");
    // Her next turn starts: a running turn keeps what it holds and the others keep their seats.
    let mut running = minds;
    running[0].in_flight = true;
    assert_eq!(grant_once(&running, &grants, RECEIPT_POOL), grants, "a running turn shrinks no one");
}

// what this catches (the M5, 2026-11-04 20:15-20:35Z, 87eccb3a7): a bound displacing minds whose
// floors are not measured yet. Unknown is a type: an unmeasured exact need is admitted by her need,
// ahead of every bound. The three exact needs are admitted whole (41,000 + 38,000 + 42,000) and
// Aiko's bound, whose held 114,592 is surplus and no seat, takes the 4,696 left: below her floor
// her turn is refused, which measures it, and the next grant seats it (the test above).
// Contrast, documented: the receipt's grant for these same minds was Aiko alone.
#[test]
fn without_measured_floors_exact_needs_are_admitted_before_a_bound() {
    const RECEIPT_GRANT: [(u32, u32); 1] = [(1, 114_592)];
    let (minds, held) = receipt_minds(false);
    let grants = grant_once(&minds, &held, RECEIPT_POOL);
    assert_eq!(grants, BTreeMap::from([(1, 4_696), (2, 41_000), (3, 38_000), (4, 42_000)]), "no bound starves a mind");
    assert_ne!(grants, BTreeMap::from(RECEIPT_GRANT), "the 20:15Z grant, Aiko alone, is gone");
    assert_eq!(grant_once(&minds, &grants, RECEIPT_POOL), grants, "a steady demand is a steady grant");
}

// what this catches: seating floors leaking into the case where everything fits. On a 654,336 pool
// (the size the 8,192-ceiling test uses) every mind's target fits, so each is granted her target,
// exactly as without floors: the floor pass runs only when the needs do not all fit.
#[test]
fn when_everything_fits_measured_floors_change_no_grant() {
    let pool = 654_336;
    let (minds, _) = receipt_minds(true);
    let fits = minds.map(|d| Demand { ceiling: 262_144, target: d.need.value().saturating_add(d.need.value().get() / 4), ..d });
    let unmeasured = fits.map(|d| Demand { floor: Floor::Unmeasured, ..d });
    let grants = grant_once(&fits, &BTreeMap::new(), pool);
    assert_eq!(grants, BTreeMap::from([(1, 143_240), (2, 51_250), (3, 47_500), (4, 52_500)]), "every target: {grants:?}");
    assert_eq!(grants, grant_once(&unmeasured, &BTreeMap::new(), pool), "floors move nothing when all fits");
}

/// Busy minds `(id, exact need, floor if measured, turn ticks)` on a shared pool of `pool`, granted
/// straight by the policy every tick and admitted at each turn's start through the published grants.
fn serve(minds: &[(u128, u32, Option<u32>, u32)], pool: u32, restart: Restart) -> Served {
    let busy: Vec<Busy> = minds.iter().map(|&(id, _, _, turn_ticks)| Busy { mind: uuid::Uuid::from_u128(id), turn_ticks }).collect();
    let shape: BTreeMap<uuid::Uuid, (u32, Option<u32>)> = minds.iter().map(|&(id, need, floor, _)| (uuid::Uuid::from_u128(id), (need, floor))).collect();
    let mut policy = DemandFollowing::default();
    let mut held = BTreeMap::new();
    play(&busy, restart, 300, |seen| {
        let demands: Vec<Demand<uuid::Uuid>> = seen
            .iter()
            .map(|s| {
                let (need, floor) = shape[&s.mind];
                Demand {
                    mind: s.mind,
                    need: Observed::Exact(t(need)),
                    floor: Floor::of(floor.map(t)),
                    target: t(need),
                    wish: t(need),
                    in_flight: s.in_flight,
                    turns_ended: s.turns_ended,
                    ceiling: pool,
                }
            })
            .collect();
        let by_mind = policy.grant(&demands, &held, pool);
        held.clone_from(&by_mind);
        PoolGrants { pool_tokens: pool, per_sequence_cap: pool, by_mind, lapses: policy.lapses().clone() }
    })
}

// what this catches (Codex CHANGES at a95afe3d1, counterexample 1): pool 100,000, two exact needs of
// 60,000 with unmeasured floors, A holding 60,000 and B nothing. The seats fit, so rotation never ran:
// stage 2 kept A and could not fit B, forever, however often A completed. B is owed service: A's
// running turn keeps its grant, which now ends with that turn, and at A's first turn boundary (even
// one she ends and restarts between ticks) B is granted her whole need.
#[test]
fn an_unmeasured_waiter_is_served_at_the_holders_next_turn_boundary() {
    let (a, b) = (1u32, 2u32);
    let mind = |m, ended, in_flight| Demand {
        mind: m,
        need: Observed::Exact(t(60_000)),
        floor: Floor::Unmeasured,
        target: t(60_000),
        wish: t(60_000),
        in_flight,
        turns_ended: TurnsEnded(ended),
        ceiling: 100_000,
    };
    let mut policy = DemandFollowing::default();
    let first = policy.grant(&[mind(a, 0, true), mind(b, 0, true)], &BTreeMap::from([(a, 60_000)]), 100_000);
    assert_eq!(first, BTreeMap::from([(a, 60_000)]), "A's running turn is never preempted");
    assert_eq!(policy.lapses(), &BTreeMap::from([(a, TurnsEnded(0))]), "A's grant ends with the turn it serves");
    // A ends her turn and starts the next between ticks: her count moved, so she re-enters behind B.
    let second = policy.grant(&[mind(a, 1, true), mind(b, 0, true)], &first, 100_000);
    assert_eq!(second, BTreeMap::from([(b, 60_000)]), "B is served at A's first turn boundary");
}

// what this catches (Codex CHANGES at a95afe3d1, both counterexamples, and the M5 live grant at
// 21:28Z on a95afe3d1: a 35,584 pool seated Aiko 16,150 and Atlas 14,394, Aris and Benchy nothing):
// a mind owed service never served. Played against the policy with turns ending after a tick and
// (counterexample 2) ending and restarting between ticks, every mind is served again and again, each
// wait at most (M − S)·S turn endings (the bound on `DemandFollowing`), no running turn shrinks, and
// nothing is granted past the pool: two unmeasured 60,000 needs on 100,000 (bound 1); the 21:28Z
// shape, four unmeasured minds of which two fit at once (bound 4; needs are inputs: the two granted
// at 21:28Z and two that did not fit beside them); four measured floors of 30,000 on 100,000, three
// seats (bound 3, Codex on the fairness change: "pool below the sum of floors").
#[test]
fn every_mind_owed_service_is_served_within_the_bound() {
    for restart in [Restart::AfterATick, Restart::BetweenTicks] {
        let cases: [(&str, Vec<(u128, u32, Option<u32>, u32)>, u32, u64); 3] = [
            ("two unmeasured", vec![(1, 60_000, None, 2), (2, 60_000, None, 3)], 100_000, 1),
            ("the 21:28Z shape", vec![(1, 16_150, None, 2), (2, 14_394, None, 3), (3, 17_500, None, 2), (4, 15_800, None, 4)], 35_584, 4),
            ("below the sum of floors", vec![(1, 40_000, Some(30_000), 2), (2, 40_000, Some(30_000), 3), (3, 40_000, Some(30_000), 4), (4, 40_000, Some(30_000), 2)], 100_000, 3),
        ];
        for (name, minds, pool, bound) in cases {
            let served = serve(&minds, pool, restart);
            assert!(served.longest_wait <= bound, "{name}, {restart:?}: waited {} turn endings, bound {bound}", served.longest_wait);
            assert!(served.turns.len() == minds.len() && served.turns.values().all(|&n| n >= 10), "{name}, {restart:?}: {:?}", served.turns);
            assert_eq!((served.over_pool, served.shrunk), (0, 0), "{name}, {restart:?}: within the pool, no running turn shrunk");
        }
    }
}
