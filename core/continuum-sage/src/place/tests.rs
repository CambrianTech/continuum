use super::*;

const GB: u64 = 1 << 30;
const MB: u64 = 1 << 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tier {
    Vram,
    Pinned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Node {
    Awake,
}

const TRAINER: Owner = Owner(1);
const SERVING: Owner = Owner(2);
const FIRST: Generation = Generation(1);

fn granted<P: std::fmt::Debug, Q: std::fmt::Debug>(answer: Answer<P, Q>) -> Claim<P, Q> {
    match answer {
        Answer::Granted(claim) => claim,
        other => panic!("test: expected a grant, got {other:?}"),
    }
}

// what this catches: the 5090 on 2026-11-26 (5.18 MB of VRAM free, a seat needing 5.7 GB of a 32 GB
// card) refused outright; it must wait, naming its shortfall, and be granted when room appears. Only a
// need past the card itself is "never here".
#[test]
fn a_seat_short_of_room_waits_and_only_a_need_past_capacity_is_never_here() {
    let mut ledger = Ledger::default();
    ledger.place(Tier::Vram, Bytes(32 * GB), Bytes(32 * GB), &mut Vec::new());
    let serving = granted(ledger.prepare(SERVING, FIRST, &[Need { place: Tier::Vram, amount: Bytes(32 * GB - 5 * MB) }]));
    assert!(serving.consume(Tier::Vram, Bytes(32 * GB - 5 * MB)));
    assert_eq!(ledger.commit(serving.id()), Ok(()));

    let seat = Need { place: Tier::Vram, amount: Bytes(5_700 * MB) };
    let Answer::Waiting { id, short } = ledger.prepare(TRAINER, FIRST, &[seat]) else { panic!("test: a seat short of room waits") };
    assert_eq!(short, vec![Need { place: Tier::Vram, amount: Bytes(5_695 * MB) }], "the wait names what it lacks");
    assert_eq!(ledger.shortfall(Tier::Vram), Some(Bytes(5_695 * MB)));

    let mut out = Vec::new();
    ledger.grant_waiting(&mut out);
    assert!(out.is_empty(), "no room yet");
    assert_eq!(ledger.release(serving.id()), Ok(()), "serving's allocation released");
    ledger.grant_waiting(&mut out);
    assert_eq!(out.len(), 1, "granted when room appears");
    assert_eq!(out[0].id(), id);

    let past = ledger.prepare(TRAINER, FIRST, &[Need { place: Tier::Vram, amount: Bytes(33 * GB) }]);
    assert!(matches!(past, Answer::Never { need: Bytes(n), capacity: Bytes(c), .. } if n == 33 * GB && c == 32 * GB), "past the card: never here");
}

// what this catches: the engine thread waiting on, or overspending, a claim; and a commit charging what
// was prepared rather than what was spent (the night's gross-versus-growth counters).
#[test]
fn a_claim_is_spent_without_waiting_and_only_what_was_spent_is_held() {
    let mut ledger = Ledger::default();
    ledger.place(Tier::Vram, Bytes(8 * GB), Bytes(8 * GB), &mut Vec::new());
    ledger.place(Tier::Pinned, Bytes(16 * GB), Bytes(16 * GB), &mut Vec::new());
    let claim = granted(ledger.prepare(TRAINER, FIRST, &[Need { place: Tier::Vram, amount: Bytes(2 * GB) }, Need { place: Tier::Pinned, amount: Bytes(GB) }]));
    assert!(claim.consume(Tier::Vram, Bytes(GB)));
    assert!(!claim.consume(Tier::Vram, Bytes(2 * GB)), "past what is left: refused, nothing spent");
    assert_eq!(claim.left(Tier::Vram), Some(Bytes(GB)));
    assert!(claim.consume(Tier::Pinned, Bytes(1)), "each place spends from its own remainder");
    assert_eq!(claim.left(Tier::Pinned), Some(Bytes(GB - 1)));
    assert_eq!(ledger.commit(claim.id()), Ok(()));
    let vram = ledger.accounts(Tier::Vram).expect("test: told of it"); // expect: the test's own precondition
    assert_eq!((vram.held, vram.prepared, vram.free()), (Bytes(GB), Bytes(0), Bytes(7 * GB)), "the unspent gigabyte is free again");
}

// what this catches: a seat whose allocation failed, or was cancelled, keeping its room.
#[test]
fn an_unwound_claim_returns_everything_and_a_withdrawn_wait_leaves_the_queue() {
    let mut ledger = Ledger::default();
    ledger.place(Tier::Vram, Bytes(4 * GB), Bytes(4 * GB), &mut Vec::new());
    let claim = granted(ledger.prepare(TRAINER, FIRST, &[Need { place: Tier::Vram, amount: Bytes(3 * GB) }]));
    let Answer::Waiting { id, .. } = ledger.prepare(SERVING, FIRST, &[Need { place: Tier::Vram, amount: Bytes(2 * GB) }]) else { panic!("test: waits") };
    assert_eq!(ledger.unwind(claim.id()), Ok(()));
    assert_eq!(ledger.unwind(id), Ok(()), "a wait withdrawn");
    assert_eq!(ledger.waiting(), 0);
    assert_eq!(ledger.accounts(Tier::Vram).map(|a| a.free()), Some(Bytes(4 * GB)));
    assert_eq!(ledger.release(claim.id()), Err(Refusal::UnknownClaim(claim.id())));
}

// what this catches: an owner restarted mid-seat spending or holding its old incarnation's room.
#[test]
fn a_restarted_owner_retires_its_earlier_claims_and_cannot_spend_them() {
    let mut ledger = Ledger::default();
    ledger.place(Tier::Vram, Bytes(4 * GB), Bytes(4 * GB), &mut Vec::new());
    let old = granted(ledger.prepare(TRAINER, FIRST, &[Need { place: Tier::Vram, amount: Bytes(3 * GB) }]));
    let _new = granted(ledger.prepare(TRAINER, Generation(2), &[Need { place: Tier::Vram, amount: Bytes(3 * GB) }]));
    assert_eq!(ledger.commit(old.id()), Err(Refusal::UnknownClaim(old.id())), "the earlier incarnation's claim is gone");
    assert!(matches!(ledger.prepare(TRAINER, FIRST, &[]), Answer::Stale { .. }));
    assert_eq!(ledger.accounts(Tier::Vram).map(|a| a.prepared), Some(Bytes(3 * GB)), "only the new claim is counted");
}

// what this catches: a later, smaller ask passing an older wait (starvation), and pressure from
// outside read as anything but a smaller budget with what must be demoted.
#[test]
fn waits_are_granted_in_order_and_a_shrinking_budget_names_what_must_go() {
    let mut ledger = Ledger::default();
    ledger.place(Tier::Vram, Bytes(8 * GB), Bytes(4 * GB), &mut Vec::new());
    let held = granted(ledger.prepare(SERVING, FIRST, &[Need { place: Tier::Vram, amount: Bytes(3 * GB) }]));
    let Answer::Waiting { id: older, .. } = ledger.prepare(TRAINER, FIRST, &[Need { place: Tier::Vram, amount: Bytes(2 * GB) }]) else { panic!("test: waits") };
    let Answer::Waiting { .. } = ledger.prepare(Owner(3), FIRST, &[Need { place: Tier::Vram, amount: Bytes(1) }]) else { panic!("test: queued behind the older wait, though it fits") };
    ledger.place(Tier::Vram, Bytes(8 * GB), Bytes(6 * GB), &mut Vec::new());
    let mut out = Vec::new();
    ledger.grant_waiting(&mut out);
    assert_eq!(out.iter().map(Claim::id).collect::<Vec<_>>().first(), Some(&older), "the older wait first");
    assert_eq!(out.len(), 2);

    ledger.place(Tier::Vram, Bytes(8 * GB), Bytes(GB), &mut Vec::new());
    assert_eq!(ledger.accounts(Tier::Vram).map(|a| a.over()), Some(Bytes(4 * GB + 1)), "pressure from outside: this much must be demoted");
    drop(held);
}

// what this catches: the outlier the contract must fit without forcing: minds awake on a node are
// slots, not bytes, and the same ledger places them.
#[test]
fn minds_awake_are_slots_in_the_same_ledger() {
    let mut ledger: Ledger<Node, Slots> = Ledger::default();
    ledger.place(Node::Awake, Slots(4), Slots(2), &mut Vec::new());
    let aiko = granted(ledger.prepare(Owner(10), FIRST, &[Need { place: Node::Awake, amount: Slots(1) }]));
    let _benchy = granted(ledger.prepare(Owner(11), FIRST, &[Need { place: Node::Awake, amount: Slots(1) }]));
    assert!(matches!(ledger.prepare(Owner(12), FIRST, &[Need { place: Node::Awake, amount: Slots(1) }]), Answer::Waiting { .. }), "a third mind waits to wake");
    assert!(aiko.consume(Node::Awake, Slots(1)));
    assert_eq!(ledger.commit(aiko.id()), Ok(()));
    assert_eq!(ledger.release(aiko.id()), Ok(()), "she goes to sleep");
    let mut out = Vec::new();
    ledger.grant_waiting(&mut out);
    assert_eq!(out.len(), 1, "the waiting mind wakes");
    assert!(matches!(ledger.prepare(Owner(13), FIRST, &[Need { place: Node::Awake, amount: Slots(5) }]), Answer::Never { .. }));
}

// what this catches (Cormac's review of 5ea6e2edbc, 1): the 5090's Vram seat waiting with 5 MB free
// held up every later claim, a Pinned one with plenty free included. Order is per place.
#[test]
fn a_wait_short_on_one_place_never_holds_up_a_claim_on_another() {
    let mut ledger = Ledger::default();
    ledger.place(Tier::Vram, Bytes(32 * GB), Bytes(5 * MB), &mut Vec::new());
    ledger.place(Tier::Pinned, Bytes(64 * GB), Bytes(64 * GB), &mut Vec::new());
    let Answer::Waiting { id: seat, .. } = ledger.prepare(TRAINER, FIRST, &[Need { place: Tier::Vram, amount: Bytes(5_700 * MB) }]) else { panic!("test: the seat waits") };
    let pinned = ledger.prepare(SERVING, FIRST, &[Need { place: Tier::Pinned, amount: Bytes(GB) }]);
    assert!(matches!(pinned, Answer::Granted(_)), "Pinned has room and nobody waits on it");
    let Answer::Waiting { id: later, .. } = ledger.prepare(Owner(3), FIRST, &[Need { place: Tier::Vram, amount: Bytes(MB) }]) else { panic!("test: behind the older Vram wait, though it fits") };
    let Answer::Waiting { id: both, .. } = ledger.prepare(Owner(4), FIRST, &[Need { place: Tier::Vram, amount: Bytes(MB) }, Need { place: Tier::Pinned, amount: Bytes(GB) }]) else { panic!("test: waits on Vram") };
    ledger.place(Tier::Vram, Bytes(32 * GB), Bytes(5_700 * MB), &mut Vec::new());
    let mut out = Vec::new();
    ledger.grant_waiting(&mut out);
    assert_eq!(out.iter().map(Claim::id).collect::<Vec<_>>(), vec![seat], "the oldest wins its place; the 1 MB waits are then short");
    assert_eq!(ledger.waiting(), 2);
    let _ = (later, both);
}

// what this catches (review 2): a restarted owner's committed memory credited the moment it restarted,
// while its old process or GPU context may still hold it.
#[test]
fn a_restart_keeps_the_earlier_incarnations_holding_charged_until_its_release() {
    let mut ledger = Ledger::default();
    ledger.place(Tier::Vram, Bytes(4 * GB), Bytes(4 * GB), &mut Vec::new());
    let old = granted(ledger.prepare(TRAINER, FIRST, &[Need { place: Tier::Vram, amount: Bytes(3 * GB) }]));
    assert!(old.consume(Tier::Vram, Bytes(2 * GB)));
    assert_eq!(ledger.commit(old.id()), Ok(()));
    assert!(matches!(ledger.prepare(TRAINER, Generation(2), &[Need { place: Tier::Vram, amount: Bytes(3 * GB) }]), Answer::Waiting { .. }), "the old 2 GB still counts");
    assert_eq!(ledger.accounts(Tier::Vram).map(|a| a.held), Some(Bytes(2 * GB)));
    assert_eq!(ledger.release(old.id()), Ok(()), "the old context's allocation released, fenced");
    let mut out = Vec::new();
    ledger.grant_waiting(&mut out);
    assert_eq!(out.len(), 1, "only now is the room the new incarnation's");
}

// what this catches (review 3): a cloned claim spending after its commit, charged to nobody.
#[test]
fn a_consume_after_the_commit_fails() {
    let mut ledger = Ledger::default();
    ledger.place(Tier::Vram, Bytes(4 * GB), Bytes(4 * GB), &mut Vec::new());
    let claim = granted(ledger.prepare(TRAINER, FIRST, &[Need { place: Tier::Vram, amount: Bytes(2 * GB) }]));
    let late = claim.clone();
    assert!(claim.consume(Tier::Vram, Bytes(GB)));
    assert_eq!(ledger.commit(claim.id()), Ok(()));
    assert!(!late.consume(Tier::Vram, Bytes(1)), "nothing is left to spend once committed");
    assert_eq!(ledger.accounts(Tier::Vram).map(|a| (a.held, a.free())), Some((Bytes(GB), Bytes(3 * GB))));
}

// what this catches (review 4): a wait left queued forever after the place shrank below its need.
#[test]
fn a_capacity_that_falls_below_a_wait_answers_it_never_here() {
    let mut ledger = Ledger::default();
    ledger.place(Tier::Vram, Bytes(32 * GB), Bytes(GB), &mut Vec::new());
    let Answer::Waiting { id, .. } = ledger.prepare(TRAINER, FIRST, &[Need { place: Tier::Vram, amount: Bytes(20 * GB) }]) else { panic!("test: waits") };
    let mut never = Vec::new();
    ledger.place(Tier::Vram, Bytes(16 * GB), Bytes(GB), &mut never);
    assert_eq!(never, vec![NeverNow { id, owner: TRAINER, place: Tier::Vram, need: Bytes(20 * GB), capacity: Bytes(16 * GB) }]);
    assert_eq!(ledger.waiting(), 0);
}
