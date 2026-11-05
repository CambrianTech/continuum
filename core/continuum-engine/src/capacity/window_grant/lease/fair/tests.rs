use super::*;
use crate::inference::planning::window_allocator::test_tokens as t;

fn engine() -> Option<EngineIncarnation> {
    Some(EngineIncarnation {
        pid: 7,
        started_s: 123,
        port: 8085,
    })
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
    settle(
        finished,
        Some(&TerminalReceipt {
            finish: "stop".into(),
        }),
    );
    let unresolved = owner
        .reserve_against(a, lease, 50, || Some(lease))
        .expect("second request");
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
