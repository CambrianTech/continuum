#![cfg(test)]

use super::*;
use crate::inference::planning::window_allocator::test_tokens as t;

#[test]
fn a_refused_holder_can_grow_into_free_capacity_without_a_terminal_receipt() {
    let owner = Reservations::new();
    let a = uuid::Uuid::from_u128(1);
    let b = uuid::Uuid::from_u128(2);
    let mut fair = FairGrantState::default();
    let mut current = None;
    for floor in [40, 60] {
        let demands = [(a, floor), (b, 40)].map(|(mind, n)| Demand {
            mind, need: t(n), target: t(n), wish: t(n), in_flight: false, settled: true, ceiling: 100,
        });
        let before = current.clone();
        let (_, tickets) = owner.fair_publish(&mut fair, &demands, 100, 100, engine(),
            || before, |next| current = Some(next));
        assert!(tickets.is_empty(), "unused cells need no physical reclamation");
        assert_eq!(current.as_ref().expect("published").by_mind,
            BTreeMap::from([(a, floor), (b, 40)]),
            "an unsent capacity refusal must not require completed service to grow");
    }
}

#[test]
fn a_large_waiter_accumulates_reclaimed_capacity_across_two_holders() {
    let owner = Box::leak(Box::new(Reservations::new()));
    let (a, b, c) = (uuid::Uuid::from_u128(1), uuid::Uuid::from_u128(2), uuid::Uuid::from_u128(3));
    let demands = [(a, 20), (b, 60), (c, 90)].map(|(mind, n)| Demand {
        mind, need: t(n), target: t(n), wish: t(n), in_flight: false, settled: true, ceiling: 100,
    });
    let mut fair = FairGrantState::default();
    let mut current = None;
    let publish = |fair: &mut FairGrantState, current: &mut Option<PoolGrants>| {
        let before = current.clone();
        owner.fair_publish(fair, &demands, 100, 100, engine(), || before, |next| *current = Some(next)).1
    };
    publish(&mut fair, &mut current);
    for mind in [a, b] {
        let lease = current.as_ref().expect("published").lease_for(mind).expect("holder");
        let request = owner.reserve_against(mind, lease, 10, || Some(lease)).expect("request");
        settle(request, Some(&TerminalReceipt { finish: "stop".into() }));
    }
    let first = publish(&mut fair, &mut current).pop().expect("first reclaim");
    assert_eq!(first.persona(), a);
    owner.lock().reclaimed.insert(a, first.lease());
    let second = publish(&mut fair, &mut current).pop().expect("second reclaim");
    assert_eq!(second.persona(), b);
    assert!(!current.as_ref().expect("published").by_mind.contains_key(&a),
        "do not spend the first reclaimed cells while the oldest waiter needs both holders");
    owner.lock().reclaimed.insert(b, second.lease());
    assert!(publish(&mut fair, &mut current).is_empty());
    assert_eq!(current.as_ref().expect("published").by_mind, BTreeMap::from([(c, 90)]));
}

#[test]
fn refusal_restores_the_charged_holder_without_giving_capacity_to_the_waiter() {
    let owner = Box::leak(Box::new(Reservations::new()));
    let (a, b) = (uuid::Uuid::from_u128(1), uuid::Uuid::from_u128(2));
    let mut fair = FairGrantState::default();
    let mut current = None;
    tick(owner, &mut fair, &[a, b], &mut current);
    let lease = current.as_ref().expect("published").lease_for(a).expect("holder");
    let served = owner.reserve_against(a, lease, 50, || Some(lease)).expect("reserved");
    settle(served, Some(&TerminalReceipt { finish: "stop".into() }));
    let ticket = tick(owner, &mut fair, &[a, b], &mut current).pop().expect("handoff");
    owner.lock().refused.insert(a, ticket.lease());
    assert!(tick(owner, &mut fair, &[a, b], &mut current).is_empty());
    assert_eq!(current.as_ref().expect("restored").by_mind, BTreeMap::from([(a, 60)]));
    assert!(owner.lock().retiring.is_empty());
    let restored = current.as_ref().expect("published").lease_for(a).expect("holder");
    owner.reserve_against(a, restored, 50, || Some(restored)).expect("refusal is not permanent retirement").release_unsent();
    assert!(tick(owner, &mut fair, &[a, b], &mut current).is_empty(), "no immediate retry without a new completion");
}

// what this catches (the M5, d6c0dd892, 2026-11-14 09:37:41Z): Atlas rendered her act against an
// 88,509 grant; her need grew and the next publication grew her grant to 89,513 (a new revision),
// so her dispatch was refused "her grant changed since this request was rendered (88509 ->
// Some(89513))" and re-rendered. An act holds its grant: the growth waits for her next act, the
// request dispatches on the lease it was priced on, and the grant grows once the act ends.
#[test]
fn a_need_that_grows_under_a_rendered_act_moves_her_grant_at_the_next_act() {
    let owner = Box::leak(Box::new(Reservations::new()));
    let a = uuid::Uuid::from_u128(1);
    let mut fair = FairGrantState::default();
    let mut current = None;
    let mut publish = |need: u32, current: &mut Option<PoolGrants>| {
        let demands = [Demand { mind: a, need: t(need), target: t(need), wish: t(need), in_flight: false, settled: true, ceiling: 200 }];
        let before = current.clone();
        owner.fair_publish(&mut fair, &demands, 200, 200, engine(), || before, |next| *current = Some(next)).1
    };
    publish(88, &mut current);
    let rendered = current.as_ref().expect("published").lease_for(a).expect("granted");
    let act = owner.hold_act(a, || Some(rendered)).expect("her act holds her grant");
    assert!(publish(90, &mut current).is_empty());
    assert_eq!(current.as_ref().expect("published").lease_for(a), Some(rendered), "the same grant, the same revision, under the act");
    let request = owner.reserve_against(a, rendered, 80, || current.as_ref().and_then(|c| c.lease_for(a)))
        .expect("dispatched on the lease she was priced on");
    settle(request, Some(&TerminalReceipt { finish: "stop".into() }));
    drop(act);
    publish(90, &mut current);
    assert_eq!(current.as_ref().expect("published").grant_for(a), Some(90), "her grant grows at her next act");
}

fn engine() -> Option<EngineIncarnation> {
    Some(EngineIncarnation {
        pid: 7,
        started_s: 123,
        port: 8085,
    })
}

#[tokio::test]
async fn http_reclamation_proof_publishes_the_waiter_without_freeing_early() {
    use crate::inference::slots::grant_handoff::{fixture_for_publication, reclaim_for_publication};
    let (root, pool, key, server) = fixture_for_publication().await;
    let on = Some(EngineIncarnation { pid: 7, started_s: 123,
        port: reqwest::Url::parse(&root).expect("fixture URL").port().expect("fixture port") });
    let owner = Box::leak(Box::new(Reservations::new()));
    let a = key.persona();
    let b = uuid::Uuid::new_v4();
    let demands = [a, b].map(|mind| Demand {
        mind, need: t(60), target: t(60), wish: t(60), in_flight: false, settled: true, ceiling: 100,
    });
    let mut fair = FairGrantState::default();
    let mut current = None;
    let publish = |fair: &mut FairGrantState, current: &mut Option<PoolGrants>| {
        let before = current.clone();
        owner.fair_publish(fair, &demands, 100, 100, on, || before, |next| *current = Some(next)).1
    };
    assert!(publish(&mut fair, &mut current).is_empty());
    let lease = current.as_ref().expect("published").lease_for(a).expect("holder");
    let turn = owner.reserve_against(a, lease, 50, || Some(lease)).expect("reserved");
    settle(turn, Some(&TerminalReceipt { finish: "stop".into() }));
    let ticket = publish(&mut fair, &mut current).pop().expect("handoff");
    assert_eq!(owner.lock().retiring[&a].tokens, 60);
    assert!(!current.as_ref().expect("published").by_mind.contains_key(&b));
    let proof = tokio::time::timeout(std::time::Duration::from_secs(5), reclaim_for_publication(&root, ticket))
        .await.expect("reclaim completes").expect("verified HTTP proof");
    owner.record_reclaimed(proof).expect("retained proof");
    assert_eq!(owner.lock().retiring[&a].tokens, 60, "proof alone has not published replacement authority");
    assert!(pool.confirms(&key), "resident save remains available after erase");
    assert!(publish(&mut fair, &mut current).is_empty());
    let next = current.as_ref().expect("published");
    assert_eq!(next.by_mind, BTreeMap::from([(b, 60)]));
    assert!(owner.reserve_against(a, lease, 50, || next.lease_for(a)).is_err());
    let lease_b = next.lease_for(b).expect("waiter granted");
    owner.reserve_against(b, lease_b, 50, || Some(lease_b)).expect("waiter may dispatch").release_unsent();
    server.abort();
}

fn tick(
    owner: &Reservations,
    fair: &mut FairGrantState,
    minds: &[uuid::Uuid],
    current: &mut Option<PoolGrants>,
) -> Vec<GrantHandoff> {
    let demands: Vec<_> = minds
        .iter()
        .map(|mind| Demand {
            // The service loop can still report the old holder active after engine completion.
            mind: *mind,
            need: t(60),
            target: t(60),
            wish: t(60),
            in_flight: *mind == minds[0],
            settled: true,
            ceiling: 100,
        })
        .collect();
    let before = current.clone();
    owner
        .fair_publish(
            fair,
            &demands,
            100,
            100,
            engine(),
            || before,
            |next| {
                assert!(
                    owner.held.try_lock().is_err(),
                    "publication and dispatch reservation share one transaction"
                );
                *current = Some(next);
            },
        )
        .1
}

#[test]
fn waiter_publication_requires_terminal_then_physical_receipt() {
    let owner = Box::leak(Box::new(Reservations::new()));
    let (a, b) = (uuid::Uuid::from_u128(1), uuid::Uuid::from_u128(2));
    let mut fair = FairGrantState::default();
    let mut current = None;
    assert!(tick(owner, &mut fair, &[a, b], &mut current).is_empty());
    let lease = current
        .as_ref()
        .expect("published")
        .lease_for(a)
        .expect("first holder");
    let reservation = owner
        .reserve_against(a, lease, 50, || Some(lease))
        .expect("reserved");
    assert!(tick(owner, &mut fair, &[a, b], &mut current).is_empty());
    assert_eq!(
        current.as_ref().expect("published").by_mind.get(&a),
        Some(&60)
    );
    settle(
        reservation,
        Some(&TerminalReceipt {
            finish: "stop".into(),
        }),
    );
    let mut tickets = tick(owner, &mut fair, &[a, b], &mut current);
    assert_eq!(tickets.len(), 1);
    let ticket = tickets.pop().expect("retirement");
    assert_eq!(ticket.persona(), a);
    assert!(
        current.as_ref().expect("published").by_mind.is_empty(),
        "neither resident may spend capacity awaiting erase"
    );
    assert!(
        owner.reserve_against(a, lease, 50, || Some(lease)).is_err(),
        "even a stale publication cannot reacquire"
    );
    assert!(tick(owner, &mut fair, &[a, b], &mut current).is_empty());
    assert_eq!(owner.lock().retiring[&a].tokens, 60);
    // Model ONLY the physical receipt here; real HTTP save/erase ordering is covered by
    // slots::grant_handoff tests. No public API can fabricate this sealed proof.
    owner.lock().reclaimed.insert(a, ticket.lease());
    assert!(tick(owner, &mut fair, &[a, b], &mut current).is_empty());
    let published = current.as_ref().expect("published");
    assert_eq!(published.by_mind.get(&b), Some(&60));
    assert_eq!(published.by_mind.get(&a), None);
    assert!(owner.lock().retiring.is_empty());
    assert!(owner
        .reserve_against(a, lease, 50, || published.lease_for(a))
        .is_err());
}

#[test]
fn cancellation_does_not_authorize_retirement_or_waiter_admission() {
    let owner = Box::leak(Box::new(Reservations::new()));
    let (a, b) = (uuid::Uuid::from_u128(1), uuid::Uuid::from_u128(2));
    let mut fair = FairGrantState::default();
    let mut current = None;
    tick(owner, &mut fair, &[a, b], &mut current);
    let lease = current
        .as_ref()
        .expect("published")
        .lease_for(a)
        .expect("holder");
    let finished = owner
        .reserve_against(a, lease, 50, || Some(lease))
        .expect("first request");
    let mut unresolved = owner
        .reserve_against(a, lease, 50, || Some(lease))
        .expect("overlapping second request");
    settle(
        finished,
        Some(&TerminalReceipt {
            finish: "stop".into(),
        }),
    );
    unresolved.mark_dispatched();
    drop(unresolved);
    assert!(tick(owner, &mut fair, &[a, b], &mut current).is_empty());
    assert_eq!(
        current.as_ref().expect("published").by_mind.get(&a),
        Some(&60)
    );
    assert_eq!(current.as_ref().expect("published").by_mind.get(&b), None);
    assert!(owner.lock().retiring.is_empty());
}

#[test]
fn completion_bars_reacquisition_before_the_next_publisher_tick() {
    let owner = Box::leak(Box::new(Reservations::new()));
    let (a, b) = (uuid::Uuid::from_u128(1), uuid::Uuid::from_u128(2));
    let mut fair = FairGrantState::default();
    let mut current = None;
    tick(owner, &mut fair, &[a, b], &mut current);
    let lease = current.as_ref().expect("published").lease_for(a).expect("holder");
    let unsent = owner.reserve_against(a, lease, 50, || Some(lease)).expect("first attempt");
    unsent.release_unsent();
    let served = owner.reserve_against(a, lease, 50, || Some(lease)).expect("unsent did not spend turn");
    settle(served, Some(&TerminalReceipt { finish: "stop".into() }));
    assert!(matches!(owner.reserve_against(a, lease, 50, || Some(lease)), Err(ReserveRefused::Retiring { .. })),
        "no publisher tick has run, but the terminal boundary already bars reacquisition");
    assert_eq!(current.as_ref().expect("published").by_mind.get(&a), Some(&60), "capacity remains charged");
    // A departed waiter disarms the obligation at publication without physical reclamation.
    assert!(tick(owner, &mut fair, &[a], &mut current).is_empty());
    let refreshed = current.as_ref().expect("published").lease_for(a).expect("holder");
    owner.reserve_against(a, refreshed, 50, || Some(refreshed)).expect("no remaining waiter").release_unsent();
}

#[test]
fn a_reclaimed_holder_queues_behind_existing_waiters() {
    let owner = Box::leak(Box::new(Reservations::new()));
    let (a, b, c) = (
        uuid::Uuid::from_u128(1),
        uuid::Uuid::from_u128(2),
        uuid::Uuid::from_u128(3),
    );
    let mut fair = FairGrantState::default();
    let mut current = None;
    tick(owner, &mut fair, &[a, b, c], &mut current);
    for (holder, next) in [(a, b), (b, c), (c, a)] {
        let lease = current
            .as_ref()
            .expect("published")
            .lease_for(holder)
            .expect("holder admitted");
        let request = owner
            .reserve_against(holder, lease, 50, || Some(lease))
            .expect("reserved");
        settle(
            request,
            Some(&TerminalReceipt {
                finish: "stop".into(),
            }),
        );
        let mut tickets = tick(owner, &mut fair, &[a, b, c], &mut current);
        assert_eq!(tickets.len(), 1);
        let ticket = tickets.pop().expect("retirement");
        assert_eq!(ticket.persona(), holder);
        owner.lock().reclaimed.insert(holder, ticket.lease());
        assert!(tick(owner, &mut fair, &[a, b, c], &mut current).is_empty());
        assert_eq!(
            current.as_ref().expect("published").by_mind,
            BTreeMap::from([(next, 60)])
        );
    }
}

// what this catches (the M5, 2026-11-10: Aiko and Benchy queued for ten minutes behind two holders):
// a seat that never turns because its holder is idle. A holder granted but idle across two
// evaluations, who never served on this engine, holds no KV to reclaim: her seat goes to the waiter
// with no physical handoff. One evaluation of idleness is not enough (she may be building her prompt).
#[test]
fn an_unused_seat_yields_to_a_waiter_after_two_idle_evaluations() {
    let owner = Box::leak(Box::new(Reservations::new()));
    let (a, b) = (uuid::Uuid::from_u128(1), uuid::Uuid::from_u128(2));
    let mut fair = FairGrantState::default();
    let mut current = None;
    let publish = |fair: &mut FairGrantState, current: &mut Option<PoolGrants>, minds: &[uuid::Uuid]| {
        let demands: Vec<_> = minds.iter().map(|mind| Demand {
            mind: *mind, need: t(60), target: t(60), wish: t(60), in_flight: false, settled: true, ceiling: 100,
        }).collect();
        let before = current.clone();
        owner.fair_publish(fair, &demands, 100, 100, engine(), || before, |next| *current = Some(next)).1
    };
    assert!(publish(&mut fair, &mut current, &[a]).is_empty());
    let holds = |current: &Option<PoolGrants>| current.as_ref().expect("published").by_mind.clone();
    assert_eq!(holds(&current), BTreeMap::from([(a, 60)]), "she holds the seat");
    assert!(publish(&mut fair, &mut current, &[a, b]).is_empty());
    assert_eq!(holds(&current), BTreeMap::from([(a, 60)]), "idle once: her seat stands");
    let tickets = publish(&mut fair, &mut current, &[a, b]);
    assert!(tickets.is_empty(), "no KV of hers on this engine: nothing to reclaim");
    assert_eq!(holds(&current).get(&b), Some(&60), "idle twice: the waiter is seated");
    assert!(!holds(&current).contains_key(&a), "her unused seat went to the waiter");
}

// what this catches: the yield armed by id order, pinning one holder (the lowest id) whether or not
// she was the one using her seat. The holder that served most since her grant is armed.
#[test]
fn the_holder_that_served_most_is_armed_to_yield() {
    let owner = Box::leak(Box::new(Reservations::new()));
    let (a, b, c) = (uuid::Uuid::from_u128(1), uuid::Uuid::from_u128(2), uuid::Uuid::from_u128(3));
    let mut fair = FairGrantState::default();
    let mut current = None;
    let publish = |fair: &mut FairGrantState, current: &mut Option<PoolGrants>, minds: &[uuid::Uuid]| {
        let demands: Vec<_> = minds.iter().map(|mind| Demand {
            mind: *mind, need: t(50), target: t(50), wish: t(50), in_flight: false, settled: true, ceiling: 100,
        }).collect();
        let before = current.clone();
        owner.fair_publish(fair, &demands, 100, 100, engine(), || before, |next| *current = Some(next)).1
    };
    publish(&mut fair, &mut current, &[a, b]);
    // b serves twice, a once, both mid-act at the evaluation that sees c waiting.
    for (holder, times) in [(a, 1), (b, 2)] {
        let lease = current.as_ref().expect("published").lease_for(holder).expect("holder admitted");
        for _ in 0..times {
            let request = owner.reserve_against(holder, lease, 40, || Some(lease)).expect("reserved");
            settle(request, Some(&TerminalReceipt { finish: "stop".into() }));
        }
    }
    let lease_a = current.as_ref().expect("published").lease_for(a).expect("a");
    let lease_b = current.as_ref().expect("published").lease_for(b).expect("b");
    let _in_flight_a = owner.reserve_against(a, lease_a, 40, || Some(lease_a)).expect("a mid-act");
    let _in_flight_b = owner.reserve_against(b, lease_b, 40, || Some(lease_b)).expect("b mid-act");
    publish(&mut fair, &mut current, &[a, b, c]);
    let armed: Vec<_> = owner.lock().yield_after.keys().copied().collect();
    assert_eq!(armed, vec![b], "the holder that served most yields next, not the lowest id");
}
