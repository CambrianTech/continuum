#![cfg(test)]
//! Existing pure placement scenarios.
use super::*;

// regression for card 60d90c95 (Kimi on a 1.5B, 2026-09-26 16:4xZ; Delia IntelMac→5090
// ×22 on 2026-10-03, each move followed by a fall-home).
// what this catches: an opportunity never steps DOWN from her home capability, even
// while her home plan is absent (the moment Kimi was moved); unknown home with no
// current plan is no permission; a seat that starves her measured turn is never
// "better" (the fall-home predicate, both directions); a real step UP still moves her.
#[test]
fn an_opportunity_never_steps_down_and_never_offers_a_seat_fall_home_would_reject() {
    use crate::cognition::grid_allocation::{BetterBy, LanePlan, Requirement};
    const REQ: Requirement = Requirement { window: 0, target_window: None, min_capability: 0, decode_floor_tps: None };
    let plan = |cap: u8, window: u32| LanePlan { model_id: "m".into(), capability_rank: cap, window, lanes: 2, decode_tps_per_lane: None };
    fn base<'a>(current: Option<&'a LanePlan>, target: &'a LanePlan) -> OpportunityInputs<'a> {
        OpportunityInputs {
            current, current_holds_her: current.is_some(), target, requirement: &REQ, target_is_current: false,
            turn_in_flight: false, since_last_move_ms: 1_000, cooldown_ms: 1_000, home_capability: None, turn_requirement: None,
        }
    }
    let (small, big) = (plan(2, 32_768), plan(30, 45_470));
    // Kimi: home is a 27B, her home plan is momentarily absent, a 1.5B seat is offered.
    assert_eq!(decide_opportunity(&OpportunityInputs { home_capability: Some(30), ..base(None, &small) }), None);
    // Unknown home and no current plan: an absence is not permission.
    assert_eq!(decide_opportunity(&base(None, &small)), None);
    // A weak home stepping UP is still the point of the rule.
    assert_eq!(decide_opportunity(&OpportunityInputs { home_capability: Some(2), ..base(None, &big) }), Some(BetterBy::Requirement));
    // Delia, measured: the 5090 seat's window 67,840 against her sent floor 70,608. The
    // placer called it better by requirement; fall-home then rejected it, 22 times.
    let seat_5090 = plan(30, 67_840);
    assert_eq!(
        decide_opportunity(&OpportunityInputs { home_capability: Some(2), turn_requirement: Some(70_608), ..base(None, &seat_5090) }),
        None
    );
    assert!(seat_starves(Some(seat_5090.window), Some(70_608)), "the same predicate fall-home reads");
    // A mind that seat does hold still moves up to it.
    assert_eq!(
        decide_opportunity(&OpportunityInputs { home_capability: Some(2), turn_requirement: Some(56_557), ..base(None, &seat_5090) }),
        Some(BetterBy::Requirement)
    );
}

// what this catches (card 10bba591): the opportunity rule — a strictly better seat
// moves her, in the allocator's order (requirement, capability, lanes, window); an
// equal seat, her own seat, a turn in flight, or her cooldown holds her; and the
// cooldown is HER measured cadence (never under her turn), the failure rule's bound
// only while nothing is measured.
#[test]
fn a_strictly_better_seat_moves_her_between_turns_and_never_inside_her_cooldown() {
    use crate::cognition::grid_allocation::{BetterBy, LanePlan};
    use crate::cognition::turn_ledger::TurnShape;
    let plan = |cap: u8, window: u32, lanes: u32| LanePlan { model_id: "m".into(), capability_rank: cap, window, lanes, decode_tps_per_lane: None };
    let (home, wide, strong) = (plan(9, 67_072, 2), plan(9, 131_072, 2), plan(12, 32_768, 1));
    fn i<'a>(current: Option<&'a LanePlan>, target: &'a LanePlan) -> OpportunityInputs<'a> {
        const REQUIREMENT: crate::cognition::grid_allocation::Requirement = crate::cognition::grid_allocation::Requirement {
            window: 0, target_window: None, min_capability: 0, decode_floor_tps: None,
        };
        OpportunityInputs {
            requirement: &REQUIREMENT,
            current, current_holds_her: current.is_some(), target, target_is_current: false, turn_in_flight: false, since_last_move_ms: 1_000, cooldown_ms: 1_000,
            home_capability: Some(0), turn_requirement: None,
        }
    }
    assert_eq!(decide_opportunity(&i(Some(&home), &wide)), Some(BetterBy::Window));
    let target_requirement = crate::cognition::grid_allocation::Requirement {
        window: 16_384, target_window: Some(100_000), min_capability: 0, decode_floor_tps: None,
    };
    let narrow_many = plan(9, 32_768, 3);
    let mut toward_fit = i(Some(&narrow_many), &home);
    toward_fit.requirement = &target_requirement;
    assert_eq!(decide_opportunity(&toward_fit), Some(BetterBy::Window));
    let mut away_from_fit = i(Some(&home), &narrow_many);
    away_from_fit.requirement = &target_requirement;
    assert_eq!(decide_opportunity(&away_from_fit), None, "more lanes cannot reverse the allocator's target-fit choice");
    assert_eq!(decide_opportunity(&i(Some(&home), &strong)), Some(BetterBy::Capability));
    assert_eq!(decide_opportunity(&i(Some(&wide), &home)), None, "a worse seat is not an opportunity");
    assert_eq!(decide_opportunity(&i(Some(&home), &home)), None, "an equal seat is never a reason to move");
    assert_eq!(decide_opportunity(&i(None, &home)), Some(BetterBy::Requirement), "her node counts no plan: any seat that holds her is objective 1");
    let mut held = i(Some(&home), &wide);
    held.current_holds_her = false;
    assert_eq!(decide_opportunity(&held), Some(BetterBy::Requirement), "her node's plan seats nobody of her role: requirement before window");
    assert_eq!(decide_opportunity(&OpportunityInputs { target_is_current: true, ..i(Some(&home), &wide) }), None, "she already sits there");
    assert_eq!(decide_opportunity(&OpportunityInputs { turn_in_flight: true, ..i(Some(&home), &wide) }), None, "a turn in flight: stay");
    assert_eq!(decide_opportunity(&OpportunityInputs { since_last_move_ms: 999, ..i(Some(&home), &wide) }), None, "inside her cooldown: stay");
    assert_eq!(opportunity_cooldown_ms(None), MOVE_COOLDOWN_MS, "nothing measured: the failure rule's bound");
    assert_eq!(opportunity_cooldown_ms(Some(TurnShape { turn_ms: 40_000, cadence_ms: 180_000, turns: 3 })), 180_000, "her cadence");
    assert_eq!(opportunity_cooldown_ms(Some(TurnShape { turn_ms: 240_000, cadence_ms: 180_000, turns: 3 })), 240_000, "never under her turn");
    assert_eq!(opportunity_cooldown_ms(Some(TurnShape::default())), MOVE_COOLDOWN_MS, "an empty shape measures nothing");
    // The shape's fold: an EMA that settles, the first turn taken whole.
    let s = TurnShape::default().observe(60_000, None);
    assert_eq!((s.turn_ms, s.cadence_ms, s.turns), (60_000, 0, 1));
    let s = s.observe(20_000, Some(200_000));
    assert_eq!((s.turn_ms, s.cadence_ms, s.turns), (50_000, 200_000, 2), "a quarter step toward the new sample; the first gap is taken whole");
    let s = s.observe(20_000, Some(100_000));
    assert_eq!(s.cadence_ms, 175_000);
}

// what this catches (card 10bba591, the goal): a node with a WIDER holding plan
// joins. The coder whose 65k requirement her home's 32k box never held — dormant
// there — is seated on the new box and the rule moves her (requirement); the helper
// the home box holds stays home, no move; the sentinel whose 131k requirement the
// new box does not hold either has no seat and no move. Then the new box drops and
// the allocation sends nobody anywhere better.
#[test]
fn a_new_wider_node_moves_the_mind_it_holds_and_not_the_ones_it_does_not() {
    use crate::cognition::grid_allocation::{allocate, BetterBy, GridAllocation, GridInputs, LanePlan, Mind, NodeOffer, OfferTerms, Requirement, Role};
    let role = |name: &str, window: u32| Role { name: name.into(), requirement: Requirement { window, target_window: None, min_capability: 0, decode_floor_tps: None } };
    let plan = |window: u32, lanes: u32| LanePlan { model_id: "27b".into(), capability_rank: 9, window, lanes, decode_tps_per_lane: None };
    let (home, joined) = (Uuid::from_u128(0x40), Uuid::from_u128(0x5090));
    let (coder, helper, sentinel) = (Uuid::from_u128(1), Uuid::from_u128(2), Uuid::from_u128(3));
    let mind = |id, role| Mind { id, role, home: Some(home), owner: Uuid::nil() };
    let offer = |node, p: LanePlan| NodeOffer { node, owner: Uuid::nil(), terms: OfferTerms::open(), plans: vec![p] };
    let inputs = |nodes| GridInputs {
        roles: vec![role("coder", 65_536), role("helper", 8_192), role("sentinel", 131_072)],
        minds: vec![mind(coder, 0), mind(helper, 1), mind(sentinel, 2)],
        nodes,
        minds_per_lane: 2,
        holds: vec![],
        floors: vec![],
    };
    let alone = allocate(&inputs(vec![offer(home, plan(32_768, 2))]));
    assert_eq!(alone.seat_of(helper), Some(home));
    assert_eq!(alone.dormant, vec![coder, sentinel], "the 32k box holds the helper only");
    let grown = allocate(&inputs(vec![offer(home, plan(32_768, 2)), offer(joined, plan(70_000, 2))]));
    assert_eq!(grown.seat_of(coder), Some(joined), "the coder wakes on the wider box");
    assert_eq!(grown.seat_of(helper), Some(home), "the helper stays home");
    assert_eq!(grown.seat_of(sentinel), None, "the sentinel's 131k is held nowhere");
    // The rule, per mind, as the pass would read it: every mind sits at home now.
    let decide_for = |a: &GridAllocation, m: Uuid| -> Option<BetterBy> {
        let seat = a.seated.iter().find(|s| s.mind == m)?;
        let cur = a.node(home);
        decide_opportunity(&OpportunityInputs {
            current: cur.and_then(|n| n.plan.as_ref()),
            current_holds_her: cur.is_some_and(|n| n.holds.contains(&seat.role)),
            target: a.node(seat.node).and_then(|n| n.plan.as_ref())?,
            requirement: &inputs(vec![]).roles[seat.role].requirement,
            target_is_current: seat.node == home,
            turn_in_flight: false,
            since_last_move_ms: MOVE_COOLDOWN_MS,
            cooldown_ms: MOVE_COOLDOWN_MS,
            // As the pass reads it: her home capability is this node's own plan
            // (`note_home_capability`), so a move up still passes the never-step-down rule.
            home_capability: cur.and_then(|n| n.plan.as_ref()).map(|p| p.capability_rank),
            turn_requirement: None,
        })
    };
    assert_eq!(decide_for(&grown, coder), Some(BetterBy::Requirement), "moved: her home never held her");
    assert_eq!(decide_for(&grown, helper), None, "not moved: home holds her and seats her");
    assert_eq!(decide_for(&grown, sentinel), None, "not moved: no seat holds her");
    assert_eq!(decide_for(&alone, helper), None, "before the join, nobody moved either");
}

fn inputs(seat: Seat, age: Option<u64>, cold: bool, local: bool, since: u64) -> PlacementInputs {
    // peer_served_ok defaults TRUE here: these cases predate the return-gate service
    // term and assume a serving peer; `a_fresh_but_never_serving_seat_is_not_returned`
    // exercises the false case.
    PlacementInputs { seat, beacon_age_ms: age, lane_cold: cold, local_available: local, since_last_move_ms: since, peer_served_ok: true, seat_wait_p50_ms: None, home_wait_p50_ms: None, seat_is_self: false, seat_offers_slot: true, seat_window: None, requirement: None }
}

// what this catches (card 2500d2f1, the M5 2026-09-19): a "remote" seat that is one
// of this node's own ids. Bound to it she must come home NOW — not after the cooldown,
// not after 90 s of "silence" from a seat that is herself — and at home a fresh beacon
// from that id (her own node's) must never read as a seat to return to.
#[test]
fn a_seat_that_is_this_node_is_never_a_remote_seat() {
    let mut bound = inputs(Seat::Remote, Some(1_000), false, true, 0);
    bound.seat_is_self = true;
    assert_eq!(
        decide(bound),
        PlacementMove::FallHome { reason: SELF_SEAT_REASON },
        "fresh beacon, no cooldown: still home, it is a loopback lane"
    );
    let mut no_lane = bound;
    no_lane.local_available = false;
    assert_eq!(decide(no_lane), PlacementMove::Park { reason: SELF_SEAT_REASON });
    let mut home = inputs(Seat::Home, Some(1_000), false, true, MOVE_COOLDOWN_MS);
    assert_eq!(decide(home), PlacementMove::ReturnRemote, "a real fresh seat returns");
    home.seat_is_self = true;
    assert_eq!(decide(home), PlacementMove::Stay, "her own node is never a seat to return to");
}

// what this catches: the 2026-09-07 hand moves, as the rule — a dark seat (no
// beacon for 90 s, or the breaker cold) with a home lane available → fall home;
// a fresh seat while home → return. The two silent failure shapes are named.
#[test]
fn a_dark_seat_falls_home_and_a_fresh_seat_returns() {
    assert_eq!(
        decide(inputs(Seat::Remote, Some(REMOTE_SEAT_SILENT_MS + 1), false, true, MOVE_COOLDOWN_MS)),
        PlacementMove::FallHome { reason: "seat silent: no capacity beacon" }
    );
    assert_eq!(
        decide(inputs(Seat::Remote, Some(5_000), true, true, MOVE_COOLDOWN_MS)),
        PlacementMove::FallHome { reason: "breaker cold: three deadline misses in a row" }
    );
    assert_eq!(
        decide(inputs(Seat::Remote, None, false, true, MOVE_COOLDOWN_MS)),
        PlacementMove::FallHome { reason: "seat silent: no capacity beacon" },
        "never heard = dark"
    );
    assert_eq!(decide(inputs(Seat::Home, Some(10_000), false, true, MOVE_COOLDOWN_MS)), PlacementMove::ReturnRemote);
}

// what this catches (card ad96f5d1, the flap): a fresh beacon proves the peer is
// ALIVE, not that it will ANSWER. A seat that beacons perfectly but has never served
// (peer_served_ok = false) must NOT be returned to — otherwise a rescued citizen is
// sent straight back the moment the 5-minute cold window lapses. Same inputs that
// returned above, only the service term flipped, must now Stay.
#[test]
fn a_fresh_but_never_serving_seat_is_not_returned() {
    let mut i = inputs(Seat::Home, Some(10_000), false, true, MOVE_COOLDOWN_MS);
    assert_eq!(decide(i.clone()), PlacementMove::ReturnRemote, "serving peer: return");
    i.peer_served_ok = false;
    assert_eq!(
        decide(i),
        PlacementMove::Stay,
        "alive-but-never-served peer: hold home, do not feed the dead seat"
    );
}

// what this catches: a node with no lane for her PARKS (loud, no move) — never the
// silent local downgrade [[fallbacks-are-illegal-fail-loud]] forbids (the Intel case).
#[test]
fn a_node_without_a_home_lane_parks_instead_of_downgrading() {
    assert_eq!(
        decide(inputs(Seat::Remote, None, false, false, MOVE_COOLDOWN_MS)),
        PlacementMove::Park { reason: "seat silent: no capacity beacon" }
    );
}

// what this catches: the pooled-demand rule — 16 minds on 7 lanes borrow a fresh
// peer's two free lanes (27B measured 42 ≥ local proxy 40) for the two least-served
// minds; a stale beacon, a lower-ranked model, or a full peer lends nothing; and a
// node with lanes to spare moves nobody.
#[test]
fn a_node_with_more_minds_than_lanes_borrows_a_peers_free_lanes() {
    let gpu = Uuid::from_u128(0x5090);
    let rank = |m: &str| -> Option<u8> { match m { "qwen-27b" => Some(42), "tiny" => Some(10), _ => None } };
    let peers = vec![
        PeerOffer { peer: gpu, served_model: Some("qwen-27b".into()), lanes: 2, residents: 0, beacon_age_ms: 5_000, free_slots_live: 2, lane_wait_p50_ms: 0, lane_wait_samples: 0, served_context_window: None },
        PeerOffer { peer: Uuid::from_u128(1), served_model: Some("tiny".into()), lanes: 4, residents: 0, beacon_age_ms: 5_000, free_slots_live: 4, lane_wait_p50_ms: 0, lane_wait_samples: 0, served_context_window: None },
        PeerOffer { peer: Uuid::from_u128(2), served_model: Some("qwen-27b".into()), lanes: 2, residents: 2, beacon_age_ms: 5_000, free_slots_live: 0, lane_wait_p50_ms: 0, lane_wait_samples: 0, served_context_window: None },
        PeerOffer { peer: Uuid::from_u128(3), served_model: Some("qwen-27b".into()), lanes: 8, residents: 0, beacon_age_ms: REMOTE_SEAT_FRESH_MS + 1, free_slots_live: 8, lane_wait_p50_ms: 0, lane_wait_samples: 0, served_context_window: None },
    ];
    let minds: Vec<(Uuid, u64)> = (1..=16u128).map(|i| (Uuid::from_u128(0x100 + i), 20 - (i as u64 % 5))).collect();
    let moves = choose_offloads(LocalShape { resident: 16, lanes: 7, rank: 40, lane_wait_p50_ms: None }, &peers, &rank, &minds, &|_| None);
    assert_eq!(moves.len(), 2, "two free lanes on the only eligible peer: {moves:?}");
    assert!(moves.iter().all(|(_, p, m)| *p == gpu && m == "qwen-27b"));
    let least = minds.iter().min_by_key(|(_, g)| *g).map(|(m, _)| *m).expect("a mind");
    assert_eq!(moves[0].0, least, "the least-served mind goes first");
    assert!(choose_offloads(LocalShape { resident: 5, lanes: 7, rank: 40, lane_wait_p50_ms: None }, &peers, &rank, &minds, &|_| None).is_empty());
}

// what this catches (card 73eefbbb): which decided moves the workspace must be
// carried across first — a fall-home or a return between machines; never Stay,
// never Park, and never a "remote" seat that is this very node (a loopback lane:
// her workspace is already here, a push would gate nothing).
#[test]
fn only_a_move_between_machines_waits_on_the_workspace() {
    assert!(moves_between_machines(&PlacementMove::ReturnRemote, false));
    assert!(moves_between_machines(&PlacementMove::FallHome { reason: "seat silent: no capacity beacon" }, false));
    assert!(!moves_between_machines(&PlacementMove::FallHome { reason: SELF_SEAT_REASON }, true));
    assert!(!moves_between_machines(&PlacementMove::ReturnRemote, true));
    assert!(!moves_between_machines(&PlacementMove::Stay, false));
    assert!(!moves_between_machines(&PlacementMove::Park { reason: "seat silent: no capacity beacon" }, false));
}

// what this catches: a flapping tower cannot move her twice inside the cooldown, and
// a live seat is left alone (the common tick).
#[test]
fn cooldown_and_a_live_seat_both_hold_still() {
    assert_eq!(decide(inputs(Seat::Remote, None, false, true, MOVE_COOLDOWN_MS - 1)), PlacementMove::Stay);
    assert_eq!(decide(inputs(Seat::Home, Some(1_000), false, true, MOVE_COOLDOWN_MS - 1)), PlacementMove::Stay);
    assert_eq!(decide(inputs(Seat::Remote, Some(4_000), false, true, MOVE_COOLDOWN_MS)), PlacementMove::Stay);
    // Back home, the seat is heard but only just (not yet three beacons): stay.
    assert_eq!(decide(inputs(Seat::Home, Some(REMOTE_SEAT_FRESH_MS), false, true, MOVE_COOLDOWN_MS)), PlacementMove::Stay);
    // A fresh beacon while the breaker is still cold: not yet.
    assert_eq!(decide(inputs(Seat::Home, Some(1_000), true, true, MOVE_COOLDOWN_MS)), PlacementMove::Stay);
}

fn offer(peer: u128, free: u32, wait_p50_ms: u64, wait_samples: u32) -> PeerOffer {
    PeerOffer { peer: Uuid::from_u128(peer), served_model: Some("qwen-27b".into()), lanes: 4, residents: 4, beacon_age_ms: 5_000, free_slots_live: free, lane_wait_p50_ms: wait_p50_ms, lane_wait_samples: wait_samples, served_context_window: None }
}
fn local(resident: u32, lanes: u32, home_wait: Option<u64>) -> LocalShape {
    LocalShape { resident, lanes, rank: 40, lane_wait_p50_ms: home_wait }
}
fn rank27(m: &str) -> Option<u8> { (m == "qwen-27b").then_some(42) }
fn minds(n: u128) -> Vec<(Uuid, u64)> { (1..=n).map(|i| (Uuid::from_u128(0x100 + i), i as u64)).collect() }

// what this catches (card c84d885a, S1b — Cormac's "NO RESERVATION" location): `free`
// is the seat's LIVE grant-able count, never `lanes − residents`. A seat with 4 lanes
// and 4 residents that has 2 slots free lends 2; a seat with 4 lanes and no residents
// that has 0 free (every slot in flight or already granted) lends nothing; an older
// beacon (field absent → 0) offers nothing until it says otherwise.
#[test]
fn free_is_the_seats_live_grant_not_roster_arithmetic() {
    let peers = vec![offer(1, 2, 0, 0), PeerOffer { free_slots_live: 0, residents: 0, ..offer(2, 0, 0, 0) }];
    let moves = choose_offloads(local(8, 2, None), &peers, &rank27, &minds(8), &|_| None);
    assert_eq!(moves.len(), 2, "{moves:?}");
    assert!(moves.iter().all(|(_, p, _)| *p == Uuid::from_u128(1)), "only the seat with live free slots: {moves:?}");
}

#[test]
fn offloads_fit_each_mind_and_keep_unfitted_minds_for_wider_peers() {
    let large = Uuid::from_u128(101);
    let small = Uuid::from_u128(102);
    let unmeasured = Uuid::from_u128(103);
    let candidates = vec![(large, 0), (small, 1), (unmeasured, 2)];
    let requirement = |mind| {
        if mind == large { Some(70_071) }
        else if mind == small { Some(20_000) }
        else { None }
    };
    let narrow = PeerOffer { served_context_window: Some(32_768), ..offer(1, 2, 0, 0) };
    let wide = PeerOffer { served_context_window: Some(131_072), ..offer(2, 1, 0, 0) };
    let moves = choose_offloads(local(4, 1, None), &[narrow.clone(), wide.clone()], &rank27, &candidates, &requirement);
    assert_eq!(moves.iter().map(|(m, p, _)| (*m, *p)).collect::<Vec<_>>(),
        vec![(small, narrow.peer), (unmeasured, narrow.peer), (large, wide.peer)]);
    // Peer order can change the assignment, but not admit the large mind to a
    // narrow lane or spend a slot twice. Least-served eligible still goes first.
    let reversed = choose_offloads(local(4, 1, None), &[wide.clone(), narrow.clone()], &rank27, &candidates, &requirement);
    assert_eq!(reversed.iter().map(|(m, p, _)| (*m, *p)).collect::<Vec<_>>(),
        vec![(large, wide.peer), (small, narrow.peer), (unmeasured, narrow.peer)]);
    let narrow_only = choose_offloads(local(4, 1, None), &[narrow], &rank27, &candidates, &requirement);
    assert_eq!(narrow_only.len(), 2);
    assert!(narrow_only.iter().all(|(m, _, _)| *m != large));
}

// what this catches (S1b, Cormac's shape 3): a seat whose MEASURED queue is not under
// this node's own is not capacity, whatever its rank — and an unmeasured seat is not
// read as fast, but is not refused on its history either (its live free count and
// the grant bound it). Every number is relative to the one bound, not a grid value.
// what this catches (2026-09-20 13-14Z, the M5; card beea5a7a): a seat whose beacon
// offers free slots at a window that cannot hold her turn — Demetri on the 5090's
// 2,048-token seat, 172 refused prompts an hour, two 67k lanes idle at home. A known
// narrow seat sends her home NOW (no cooldown: it is not a flap, it cannot serve),
// is never returned to, and is never chosen for a spill; an unknown width (an older
// core's beacon) changes nothing; a wide seat is capacity as before.
// what this catches (2026-09-21, the IntelMac): `decide` refuses to RETURN her to a
// starving seat, but that refusal lived only in the running process — the durable
// override on disk survived, so each deploy re-bound Benchy, Aiko and Atlas to seat
// e85a5bb3 and the first tick walked them home again. Eight `fell_home` events across
// four deploys against ZERO returns, two clusters landing 28 s and 69 s after a
// deploy. The tick now asks THIS predicate — the same one `decide` asks, never a
// second reading of the same two numbers — to decide whether the record survives.
#[test]
fn the_predicate_that_refuses_a_seat_is_the_one_that_retires_its_record() {
    // The starving case `decide` acts on is exactly the case the tick retires.
    assert!(seat_starves(Some(2_048), Some(70_071)));
    assert_eq!(
        decide(PlacementInputs { seat_window: Some(2_048), requirement: Some(70_071), ..inputs(Seat::Remote, Some(1_000), false, true, 0) }),
        PlacementMove::FallHome { reason: SEAT_STARVES_REASON },
        "the move and the retirement must never disagree about what starves",
    );
    // A seat wide enough keeps its binding — retiring a good record would cost her
    // the seat she is correctly on.
    assert!(!seat_starves(Some(131_072), Some(70_071)));
    // UNKNOWN WIDTH IS NOT A REFUSAL, so it is not a retirement either: an older
    // core's beacon publishes no width, and an absence is not a number. Before this
    // was one predicate, a second hand-rolled reading here is exactly where that
    // asymmetry would have crept back in.
    assert!(!seat_starves(None, Some(70_071)));
    assert!(!seat_starves(None, None));
    // Nothing measured of her turn: the serve floor is the bar, on both sides.
    assert!(!seat_starves(Some(crate::inference::planning::MIN_SERVE_CTX), None));
    assert!(seat_starves(Some(crate::inference::planning::MIN_SERVE_CTX - 1), None));
}

#[test]
fn a_seat_whose_window_cannot_hold_her_turn_is_no_seat() {
    let starves = |i: PlacementInputs| PlacementInputs { seat_window: Some(2_048), requirement: Some(70_071), ..i };
    let wide = |i: PlacementInputs| PlacementInputs { seat_window: Some(131_072), requirement: Some(70_071), ..i };
    // Remote, fresh, warm, inside cooldown: home now.
    assert_eq!(decide(starves(inputs(Seat::Remote, Some(1_000), false, true, 0))), PlacementMove::FallHome { reason: SEAT_STARVES_REASON });
    assert_eq!(decide(starves(inputs(Seat::Remote, Some(1_000), false, false, 0))), PlacementMove::Park { reason: SEAT_STARVES_REASON });
    assert_eq!(decide(wide(inputs(Seat::Remote, Some(1_000), false, true, 0))), PlacementMove::Stay);
    assert_eq!(decide(inputs(Seat::Remote, Some(1_000), false, true, 0)), PlacementMove::Stay, "unknown width: not a refusal");
    // Home, seat fresh + served + offering a slot, cooled: never returned to a narrow seat.
    assert_eq!(decide(starves(inputs(Seat::Home, Some(1_000), false, true, MOVE_COOLDOWN_MS))), PlacementMove::Stay);
    assert_eq!(decide(wide(inputs(Seat::Home, Some(1_000), false, true, MOVE_COOLDOWN_MS))), PlacementMove::ReturnRemote);
    // No requirement measured yet: the serve floor is the bar — a seat exactly at it
    // holds (nothing measured refuses nothing), one token under it does not.
    assert_eq!(decide(PlacementInputs { seat_window: Some(2_048), ..inputs(Seat::Remote, Some(1_000), false, true, 0) }), PlacementMove::Stay);
    assert_eq!(decide(PlacementInputs { seat_window: Some(2_047), ..inputs(Seat::Remote, Some(1_000), false, true, 0) }), PlacementMove::FallHome { reason: SEAT_STARVES_REASON });
    // The chooser: a narrow seat with free slots is passed over for a wide one.
    let narrow = PeerOffer { served_context_window: Some(2_048), ..offer(1, 2, 0, 0) };
    let wide_seat = PeerOffer { served_context_window: Some(131_072), ..offer(2, 2, 0, 0) };
    let unknown = offer(3, 2, 0, 0);
    let go = |peers: &[PeerOffer]| -> Vec<Uuid> {
        let shape = local(8, 2, None);
        choose_offloads(shape, peers, &rank27, &minds(1), &|_| Some(70_071)).into_iter().map(|(_, p, _)| p).collect()
    };
    assert!(go(&[narrow.clone()]).is_empty(), "a 2k seat is not capacity for a 70k turn");
    assert_eq!(go(&[narrow, wide_seat.clone()]), vec![wide_seat.peer]);
    assert_eq!(go(&[unknown.clone()]), vec![unknown.peer], "unknown width is not a refusal");
}

#[test]
fn a_queued_seat_is_not_capacity_whatever_its_rank() {
    let b = SEAT_QUEUE_BOUND_MS;
    let queued = offer(1, 2, b + 1, 30);
    let quick = offer(2, 2, b / 4, 30);
    let unmeasured = offer(3, 2, 0, 0);
    let go = |home: Option<u64>, peers: &[PeerOffer]| -> Vec<Uuid> {
        choose_offloads(local(8, 2, home), peers, &rank27, &minds(1), &|_| None).into_iter().map(|(_, p, _)| p).collect()
    };
    // Home unmeasured: the budget is the bound.
    assert!(go(None, &[queued.clone()]).is_empty(), "past the budget: skipped");
    assert_eq!(go(None, &[quick.clone()]), vec![quick.peer], "under the budget: capacity");
    assert_eq!(go(None, &[unmeasured.clone()]), vec![unmeasured.peer], "unmeasured is not judged on history");
    // Home measured: the remote must be FASTER than home.
    let home = Some(b / 2);
    assert!(go(home, &[offer(4, 2, b / 2, 30)]).is_empty(), "equal to home: not faster, skipped");
    assert_eq!(go(home, &[quick.clone()]), vec![quick.peer], "faster than home: capacity");
    assert!(go(Some(b / 8), &[quick.clone()]).is_empty(), "slower than a quick home: skipped");
    // No queue at home at all: nobody moves, however many minds share the lanes.
    assert!(go(Some(0), &[quick.clone(), unmeasured.clone()]).is_empty(), "a median under a millisecond is no queue");
    // The queued seat is passed over FOR the quick one, not just skipped.
    assert_eq!(go(None, &[queued, quick.clone()]), vec![quick.peer]);
}

// what this catches (S1b, Cormac's shape 2): a bound switch whose seat's measured wait
// is past the bound falls home when home is faster (measured) or when the seat is past
// the budget (home unmeasured); a queued seat is never RETURNED to; cooldown still
// holds; and an unmeasured seat wait changes nothing.
#[test]
fn a_bound_seat_whose_measured_wait_passes_the_bound_falls_home_and_is_not_returned_to() {
    let b = SEAT_QUEUE_BOUND_MS;
    let mut i = inputs(Seat::Remote, Some(5_000), false, true, MOVE_COOLDOWN_MS);
    i.seat_wait_p50_ms = Some(b + 1);
    assert_eq!(decide(i.clone()), PlacementMove::FallHome { reason: SEAT_QUEUED_REASON }, "past the budget, home unmeasured");
    i.seat_wait_p50_ms = Some(b / 2);
    assert_eq!(decide(i.clone()), PlacementMove::Stay, "under the budget, home unmeasured: stay");
    i.home_wait_p50_ms = Some(b / 4);
    assert_eq!(decide(i.clone()), PlacementMove::FallHome { reason: SEAT_QUEUED_REASON }, "home is faster: come home");
    i.home_wait_p50_ms = Some(b);
    assert_eq!(decide(i.clone()), PlacementMove::Stay, "home is slower: the seat is still the better lane");
    i.home_wait_p50_ms = Some(b / 4);
    i.since_last_move_ms = MOVE_COOLDOWN_MS - 1;
    assert_eq!(decide(i.clone()), PlacementMove::Stay, "cooldown holds");
    i.since_last_move_ms = MOVE_COOLDOWN_MS;
    i.local_available = false;
    assert_eq!(decide(i.clone()), PlacementMove::Park { reason: SEAT_QUEUED_REASON }, "no home lane: parked, loud");
    // Home-seated, the seat fresh and serving — but queued: no return.
    let mut h = inputs(Seat::Home, Some(10_000), false, true, MOVE_COOLDOWN_MS);
    assert_eq!(decide(h.clone()), PlacementMove::ReturnRemote);
    h.seat_wait_p50_ms = Some(b + 1);
    assert_eq!(decide(h.clone()), PlacementMove::Stay, "a queued seat is not returned to");
    h.seat_wait_p50_ms = None;
    assert_eq!(decide(h), PlacementMove::ReturnRemote, "unmeasured changes nothing");
}

// what this catches (the M5, 2026-09-19 22:3xZ): fifteen home-bound minds, their
// seat fresh, serving, faster than home and past the cooldown — and FULL. Every tick
// each one asked the 5090 for its slot back and was told "no free slot: 0 free": 30
// wire messages a minute buying nothing. A spill only asks a seat whose beacon
// offers a slot (`choose_offloads`); the return is its mirror. Nothing else about
// the return rule changes: a free slot on a fresh, serving, unqueued seat still
// returns her, and a remote-seated mind is untouched by the offer.
#[test]
fn a_return_is_asked_only_of_a_seat_whose_beacon_offers_a_slot() {
    let mut h = inputs(Seat::Home, Some(10_000), false, true, MOVE_COOLDOWN_MS);
    assert_eq!(decide(h.clone()), PlacementMove::ReturnRemote, "the seat offers a slot: ask");
    h.seat_offers_slot = false;
    assert_eq!(decide(h.clone()), PlacementMove::Stay, "a full seat is not asked");
    let mut r = inputs(Seat::Remote, Some(10_000), false, true, MOVE_COOLDOWN_MS);
    r.seat_offers_slot = false;
    assert_eq!(decide(r), PlacementMove::Stay, "seated there already: the offer is not hers to need");
}

// what this catches (6780e79b): placement changes cannot exchange the concrete
// adapter between admission and dispatch; the next call resolves the new seat.
#[tokio::test]
async fn admitted_target_survives_fall_home_and_next_call_uses_home() {
    use crate::ai::adapter::{AIProviderAdapter, GenerationAdmissionOwner};
    use crate::ai::heuristic_adapter::HeuristicInferenceAdapter;
    use crate::ai::types::TextGenerationRequest;
    use crate::inference::airc_remote::{AircRemoteInferenceAdapter, RemoteInferenceError};
    use crate::inference::airc_remote::transport::StubInferenceTransport;
    use crate::persona::inference_profile::{PersonaInferenceProfile, SamplingProfile};
    use crate::persona::hw_tier_descriptor::HwTierCategory;
    use crate::persona::scripted_adapter_factory::ScriptedPersonaAdapterFactory;
    use std::sync::Arc;
    use uuid::Uuid;

    let home: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let factory_home = Arc::clone(&home);
    let factory = Arc::new(ScriptedPersonaAdapterFactory::custom(move |_| Ok(Arc::clone(&factory_home))));
    let profile = PersonaInferenceProfile {
        persona_id: Uuid::new_v4(), persona_name: "route fixture".into(),
        model_id: "fixture".into(), gguf_local_path: None,
        tier_category: HwTierCategory::Compat, tier_id: "fixture".into(),
        context_length: 8192, n_ubatch: 512, n_batch: 512, n_seq_max: 1, n_gpu_layers: 0,
        sampling: SamplingProfile::chat_defaults(), chat_template: None, stop_sequences: vec![],
    };
    let remote = Arc::new(AircRemoteInferenceAdapter::new(
        StubInferenceTransport::always_failing(RemoteInferenceError::Transport {
            message: "selected remote receipt".into(),
        }),
    ));
    let switch = PlacementSwitch::new(profile.persona_id, profile.persona_name.clone(),
        Uuid::new_v4(), remote, factory, profile, None, None);
    let selected = switch.generation_target().expect("concrete remote target");
    assert_eq!(selected.generation_admission(), GenerationAdmissionOwner::Adapter);
    let moved = switch.apply(&PlacementMove::FallHome { reason: "fixture transition" },
        1, SeatFit { window: None, requirement: None }).await;
    assert!(moved.is_some());
    let (sink, _receiver) = tokio::sync::mpsc::unbounded_channel();
    let result = selected.generate_stream_checked(TextGenerationRequest::default(), sink).await;
    assert!(result.expect_err("the admitted remote still dispatches remotely").to_string().contains("selected remote receipt"));
    let next = switch.generation_target().expect("concrete home target");
    assert!(Arc::ptr_eq(&next, &home), "fall-home is effective on the next call");
}

// Cold-route preparation regressions: before prompt sizing and admission.
use crate::ai::adapter::{AIProviderAdapter, GenerationAdmissionOwner};
use crate::ai::types::{TextGenerationRequest, TextGenerationResponse};
use crate::inference::airc_remote::peer_breaker::COLD_AFTER_DEADLINES;
use crate::inference::airc_remote::{
    AircRemoteInferenceAdapter, RemoteInferenceError, StubInferenceTransport,
};
use crate::persona::hw_tier_descriptor::HwTierCategory;
use crate::persona::inference_profile::{PersonaInferenceProfile, SamplingProfile};
use crate::persona::scripted_adapter_factory::ScriptedPersonaAdapterFactory;

// An admission/window contract fixture: preparation must never generate itself.
struct HomeLane(u32);

#[async_trait::async_trait]
impl AIProviderAdapter for HomeLane {
    fn provider_id(&self) -> &str {
        "home-lane-fixture"
    }
    fn name(&self) -> &str {
        "home lane"
    }
    fn default_model(&self) -> &str {
        "fixture"
    }
    fn generation_admission(&self) -> GenerationAdmissionOwner {
        GenerationAdmissionOwner::SharedLocal
    }
    fn live_served_window(&self) -> Option<u32> {
        Some(self.0)
    }
    async fn generate_text(
        &self,
        _request: TextGenerationRequest,
    ) -> Result<TextGenerationResponse, String> {
        Err("preparation must not generate".into())
    }
}

fn switch(
    factory: Arc<ScriptedPersonaAdapterFactory>,
    remote: Arc<AircRemoteInferenceAdapter>,
) -> PlacementSwitch {
    let profile = PersonaInferenceProfile {
        persona_id: Uuid::new_v4(),
        persona_name: "cold route fixture".into(),
        model_id: "fixture".into(),
        gguf_local_path: None,
        tier_category: HwTierCategory::Compat,
        tier_id: "fixture".into(),
        context_length: 8192,
        n_ubatch: 512,
        n_batch: 512,
        n_seq_max: 1,
        n_gpu_layers: 0,
        sampling: SamplingProfile::chat_defaults(),
        chat_template: None,
        stop_sequences: vec![],
    };
    PlacementSwitch::new(
        profile.persona_id,
        profile.persona_name.clone(),
        Uuid::new_v4(),
        remote,
        factory,
        profile,
        None,
        None,
    )
}

fn remote() -> Arc<AircRemoteInferenceAdapter> {
    Arc::new(AircRemoteInferenceAdapter::new(
        StubInferenceTransport::always_failing(RemoteInferenceError::Timeout { elapsed_ms: 1 }),
    ))
}

async fn trip(remote: &AircRemoteInferenceAdapter) {
    for _ in 0..COLD_AFTER_DEADLINES {
        assert!(remote
            .generate_text(TextGenerationRequest::default())
            .await
            .is_err());
    }
    assert!(remote.is_cold());
}

#[tokio::test]
async fn cold_route_prepares_home_once_and_preserves_breaker(
) -> Result<(), Box<dyn std::error::Error>> {
    let home: Arc<dyn AIProviderAdapter> = Arc::new(HomeLane(4096));
    let home_for_factory = Arc::clone(&home);
    let factory = Arc::new(ScriptedPersonaAdapterFactory::custom(move |_| {
        Ok(Arc::clone(&home_for_factory))
    }));
    let remote = remote();
    let switch = switch(Arc::clone(&factory), Arc::clone(&remote));
    let prior_target = switch.generation_target().ok_or("missing remote target")?;
    assert_eq!(
        prior_target.generation_admission(),
        GenerationAdmissionOwner::Adapter
    );
    // The remote has not answered yet; the bound profile budgets 8192. Home
    // reports 4096, so preparation must expose the reduction before prompt fit.
    assert_eq!(switch.live_served_window(), None);
    trip(&remote).await;
    switch.prepare_generation().await;
    let target = switch.generation_target().ok_or("missing home target")?;
    assert_eq!(switch.seat(), Seat::Home);
    assert!(Arc::ptr_eq(&target, &home));
    assert_eq!(
        target.generation_admission(),
        GenerationAdmissionOwner::SharedLocal
    );
    assert_eq!(switch.live_served_window(), Some(4096));
    assert!(
        remote.is_cold(),
        "preparation must not reset or replace the breaker"
    );
    assert!(
        !Arc::ptr_eq(&prior_target, &home),
        "already selected calls keep their target"
    );
    switch.prepare_generation().await;
    assert_eq!(factory.build_count(), 1);
    Ok(())
}

async fn refused_as_overloaded() -> Arc<AircRemoteInferenceAdapter> {
    let remote = Arc::new(AircRemoteInferenceAdapter::new(
        StubInferenceTransport::always_failing(RemoteInferenceError::SeatOverloaded {
            message: "2 generations outstanding for 2 served lanes".to_string(),
        }),
    ));
    assert!(remote
        .generate_text(TextGenerationRequest::default())
        .await
        .is_err());
    assert!(!remote.is_cold(), "a refusal is an answer, not a deadline miss");
    assert!(remote.seat_overloaded());
    remote
}

// what this catches (card 03660a77, IntelMac 10-23): a seat that refuses her as
// overloaded answered at once, so it is no breaker strike, but her NEXT act must go home
// instead of asking the full seat again (Paige asked 13 times). Once she has moved, the
// refusal is cleared, so a later return starts clean.
#[tokio::test]
async fn an_overload_refusal_sends_her_next_act_home_without_a_breaker_strike(
) -> Result<(), Box<dyn std::error::Error>> {
    let home: Arc<dyn AIProviderAdapter> = Arc::new(HomeLane(4096));
    let home_for_factory = Arc::clone(&home);
    let factory = Arc::new(ScriptedPersonaAdapterFactory::custom(move |_| {
        Ok(Arc::clone(&home_for_factory))
    }));
    let remote = refused_as_overloaded().await;
    let switch = switch(Arc::clone(&factory), Arc::clone(&remote));
    switch.prepare_generation().await;
    assert_eq!(switch.seat(), Seat::Home);
    let target = switch.generation_target().ok_or("missing home target")?;
    assert!(Arc::ptr_eq(&target, &home));
    assert!(!remote.seat_overloaded(), "cleared once she moved");
    Ok(())
}

// what this catches (Codex review of 9b3ad4f4a): the refusal is never erased by a move
// that did not happen. With no home lane she is parked on the seat and the refusal stands;
// with the move dropped mid-build (the act cancelled) it stands too, for her next act.
// That the parked lane then makes no round trip to the full seat is pinned at the lane
// (`capacity_tests::a_lane_refused_as_overloaded_does_not_ask_again_until_the_seat_offers_a_slot`).
#[tokio::test]
async fn an_overload_refusal_stands_until_she_has_actually_moved() {
    let no_home = Arc::new(ScriptedPersonaAdapterFactory::always_fails("no home lane"));
    let remote = refused_as_overloaded().await;
    let parked = switch(Arc::clone(&no_home), Arc::clone(&remote));
    parked.prepare_generation().await;
    assert_eq!(parked.seat(), Seat::Remote, "no home: parked, not moved");
    assert!(remote.seat_overloaded(), "no move happened, so the refusal stands");

    let held = Arc::new(ScriptedPersonaAdapterFactory::held());
    let remote = refused_as_overloaded().await;
    let building = switch(Arc::clone(&held), Arc::clone(&remote));
    let mut preparing = Box::pin(building.prepare_generation());
    assert!(futures::poll!(&mut preparing).is_pending(), "building her home lane");
    assert_eq!(held.build_count(), 1, "the build started");
    drop(preparing);
    assert_eq!(building.seat(), Seat::Remote);
    assert!(remote.seat_overloaded(), "a cancelled move leaves the refusal for her next act");
}

#[tokio::test]
async fn warm_route_does_not_materialize_home() {
    let factory = Arc::new(ScriptedPersonaAdapterFactory::heuristic());
    let switch = switch(Arc::clone(&factory), remote());
    switch.prepare_generation().await;
    assert_eq!(switch.seat(), Seat::Remote);
    assert_eq!(factory.build_count(), 0);
}

#[tokio::test]
async fn cold_route_without_home_keeps_honest_refusal() {
    let factory = Arc::new(ScriptedPersonaAdapterFactory::always_fails("no home lane"));
    let remote = remote();
    let switch = switch(Arc::clone(&factory), Arc::clone(&remote));
    trip(&remote).await;
    switch.prepare_generation().await;
    switch.prepare_generation().await;
    assert_eq!(switch.seat(), Seat::Remote);
    assert!(remote.is_cold());
    assert_eq!(
        factory.build_count(),
        1,
        "the failed home build is cached; cold act refusals remain an open limitation"
    );
}

#[tokio::test]
async fn capacity_park_resumes_when_local_1_5b_routes_to_larger_remote_seat() {
    use crate::cognition::model_binding::{ModelBinding, ModelBindingState};
    use crate::cognition::workspace::CapacityReadiness;
    use continuum_engine::capacity::gossip::{global_ledger, CapacityOffer};
    let peer = Uuid::new_v4();
    let now = continuum_base::time::EpochMs::now().millis();
    let offer: CapacityOffer = serde_json::from_value(serde_json::json!({
        "gpuTotalBytes": 1, "gpuFreeBytesLive": 0, "systemRamFreeBytes": 0,
        "atMs": now, "servedModel": "m5-capacity-fixture",
        "servedContextWindow": 65536, "capabilities": ["served_window_stamp"]
    })).unwrap();
    global_ledger().hear(peer, offer, now);
    let remote = Arc::new(AircRemoteInferenceAdapter::new(StubInferenceTransport::new(|_| {
        panic!("capacity readiness never dispatches inference")
    })).with_target_peer(peer.to_string()).with_model("m5-capacity-fixture"));
    let home: Arc<dyn AIProviderAdapter> = Arc::new(HomeLane(32768));
    let factory = Arc::new(ScriptedPersonaAdapterFactory::custom(move |_| Ok(home.clone())));
    let route = Arc::new(switch(factory, remote));
    route.apply(&PlacementMove::FallHome { reason: "local 1.5B fixture" }, now, SeatFit::default()).await;
    let binding = Arc::new(ModelBindingState::new(ModelBinding {
        adapter: route.clone(), model: Some("local-1.5B".into()), context_window: 32768,
    }));
    let refusal = CapacityReadiness::new(binding.clone(), binding.load_full());
    let local_target = route.generation_target().unwrap();
    assert_eq!(refusal.observed_window(), 32768);
    assert!(!refusal.changed());
    let waiting = refusal.wait();
    route.apply(&PlacementMove::ReturnRemote, now, SeatFit::default()).await;
    assert!(!Arc::ptr_eq(&local_target, &route.generation_target().unwrap()));
    assert_eq!(route.live_served_window(), Some(65536));
    assert_eq!(refusal.change_reason(), Some("placement"));
    tokio::time::timeout(std::time::Duration::from_secs(1), waiting).await
        .expect("the actual forwarding target changed before the waiter was first polled");
    assert_eq!(binding.load().requested_model(), "local-1.5B");
}

// A home lane on the fork's gateway: it takes a learned tool head.
struct TailLane;

#[async_trait::async_trait]
impl AIProviderAdapter for TailLane {
    fn provider_id(&self) -> &str {
        "tail-lane-fixture"
    }
    fn name(&self) -> &str {
        "tail lane"
    }
    fn default_model(&self) -> &str {
        "fixture"
    }
    fn accepts_tail_tools(&self, _model: &str) -> bool {
        true
    }
    async fn generate_text(
        &self,
        _request: TextGenerationRequest,
    ) -> Result<TextGenerationResponse, String> {
        Err("capability fixture must not generate".into())
    }
}

// what this catches: the switch every resident is bound through answering the trait default
// (`false`) for `accepts_tail_tools`, so no resident's learned tool head is ever sent even
// when her home lane takes it (M5 2026-10-27, after #5491 deployed). The answer must follow
// the seat: true at a home lane that takes tails, false once she routes to a remote seat.
#[tokio::test]
async fn the_switch_asks_the_seat_she_routes_to_whether_it_takes_her_tool_head() {
    let now = continuum_base::time::EpochMs::now().millis();
    let home: Arc<dyn AIProviderAdapter> = Arc::new(TailLane);
    let factory = Arc::new(ScriptedPersonaAdapterFactory::custom(move |_| Ok(home.clone())));
    let route = Arc::new(switch(factory, remote()));
    assert!(!route.accepts_tail_tools("fixture"), "routed to the remote seat, no tail");
    route.apply(&PlacementMove::FallHome { reason: "tail fixture" }, now, SeatFit::default()).await;
    assert!(route.accepts_tail_tools("fixture"), "her home lane takes the tail");
}
