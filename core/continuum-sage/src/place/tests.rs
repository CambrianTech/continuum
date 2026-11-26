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
    let Answer::Waiting { .. } = ledger.prepare(Owner(3), FIRST, &[Need { place: Tier::Vram, amount: Bytes(MB) }]) else { panic!("test: behind the older Vram wait, though it fits") };
    let Answer::Waiting { .. } = ledger.prepare(Owner(4), FIRST, &[Need { place: Tier::Vram, amount: Bytes(MB) }, Need { place: Tier::Pinned, amount: Bytes(GB) }]) else { panic!("test: waits on Vram") };
    ledger.place(Tier::Vram, Bytes(32 * GB), Bytes(5_700 * MB), &mut Vec::new());
    let mut out = Vec::new();
    ledger.grant_waiting(&mut out);
    assert_eq!(out.iter().map(Claim::id).collect::<Vec<_>>(), vec![seat], "the oldest wins its place; the 1 MB waits are then short");
    assert_eq!(ledger.waiting(), 2);
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

// what this catches (Codex's repro on 5ea6e2edbc): two generations of one owner each spending the
// same 100 bytes, because the restart credited the old claim without revoking it; and a restart racing
// a spend on another thread charging nobody.
#[test]
fn a_retired_claim_cannot_be_spent_and_a_racing_spend_stays_charged() {
    let mut ledger = Ledger::default();
    ledger.place(Tier::Vram, Bytes(100), Bytes(100), &mut Vec::new());
    let old = granted(ledger.prepare(TRAINER, FIRST, &[Need { place: Tier::Vram, amount: Bytes(100) }]));
    let new = granted(ledger.prepare(TRAINER, Generation(2), &[Need { place: Tier::Vram, amount: Bytes(100) }]));
    assert!(!old.consume(Tier::Vram, Bytes(100)), "the retired incarnation's claim is revoked");
    assert!(new.consume(Tier::Vram, Bytes(100)));

    for _ in 0..200 {
        let mut ledger = Ledger::default();
        ledger.place(Tier::Vram, Bytes(100), Bytes(100), &mut Vec::new());
        let old = granted(ledger.prepare(TRAINER, FIRST, &[Need { place: Tier::Vram, amount: Bytes(100) }]));
        let spender = old.clone();
        let spend = std::thread::spawn(move || spender.consume(Tier::Vram, Bytes(60)));
        assert!(matches!(ledger.prepare(TRAINER, Generation(2), &[]), Answer::Granted(_)), "the restart's own ask");
        let spent = spend.join().expect("test: the spending thread"); // expect: the test's own precondition
        let held = ledger.accounts(Tier::Vram).map(|a| a.held);
        assert_eq!(held, Some(Bytes(if spent { 60 } else { 0 })), "a spend that won the race is charged; one that lost spent nothing");
    }
}

// what this catches (the kernel authority's lease terms, moving onto place): a lapsed lease still spent
// by native code, a lapsed lease credited as free before its holder released it, and a lapsed lease
// renewed back to life.
#[test]
fn a_lapsed_claim_reads_nothing_and_stays_charged_until_released() {
    let mut ledger = Ledger::default();
    ledger.place(Tier::Vram, Bytes(8 * GB), Bytes(8 * GB), &mut Vec::new());
    let claim = granted(ledger.prepare(TRAINER, FIRST, &[Need { place: Tier::Vram, amount: Bytes(2 * GB) }]));
    let now = std::time::Instant::now();
    assert_eq!(ledger.renew(claim.id(), now, now), Ok(()), "a live claim takes a term");
    assert_eq!(claim.left(Tier::Vram), Some(Bytes(0)), "past its term it reads nothing");
    assert!(!claim.consume(Tier::Vram, Bytes(1)), "and cannot be spent");
    let mut overdue = Vec::new();
    ledger.overdue(std::time::Instant::now(), &mut overdue);
    assert_eq!(overdue, vec![claim.id()], "listed for reclaim");
    assert_eq!(ledger.accounts(Tier::Vram).map(|a| a.free()), Some(Bytes(6 * GB)), "lapsing frees nothing");
    assert_eq!(ledger.renew(claim.id(), now + std::time::Duration::from_secs(60), std::time::Instant::now()), Err(Refusal::Lapsed(claim.id())));
    assert_eq!(ledger.commit(claim.id()), Ok(()));
    assert_eq!(ledger.release(claim.id()), Ok(()));
    assert_eq!(ledger.accounts(Tier::Vram).map(|a| a.free()), Some(Bytes(8 * GB)), "its release frees it");
}

// what this catches (the kernel's shrink): a holder that gave back part of its memory keeping the
// whole charge, and a claim shrunk to nothing lingering.
#[test]
fn a_partial_release_frees_what_was_given_back() {
    let mut ledger = Ledger::default();
    ledger.place(Tier::Vram, Bytes(8 * GB), Bytes(8 * GB), &mut Vec::new());
    let claim = granted(ledger.prepare(SERVING, FIRST, &[Need { place: Tier::Vram, amount: Bytes(4 * GB) }]));
    assert!(claim.consume(Tier::Vram, Bytes(4 * GB)));
    assert_eq!(ledger.commit(claim.id()), Ok(()));
    assert_eq!(ledger.shrink(claim.id(), Tier::Vram, Bytes(GB)), Ok(Bytes(3 * GB)));
    assert_eq!(ledger.accounts(Tier::Vram).map(|a| (a.held, a.free())), Some((Bytes(3 * GB), Bytes(5 * GB))));
    assert_eq!(ledger.shrink(claim.id(), Tier::Pinned, Bytes(1)), Err(Refusal::NotOnPlace(claim.id())));
    assert_eq!(ledger.shrink(claim.id(), Tier::Vram, Bytes(9 * GB)), Ok(Bytes(0)), "giving back more than held frees what is held");
    assert_eq!(ledger.release(claim.id()), Err(Refusal::UnknownClaim(claim.id())), "shrunk to nothing, it is gone");
    assert_eq!(ledger.accounts(Tier::Vram).map(|a| a.free()), Some(Bytes(8 * GB)));
}

// what this catches (the M5's unified memory, the kernel's set_backing): a VRAM claim and a host claim
// both granted out of one physical memory, as two counters would; and a VRAM need past the memory it
// draws on not answered "never here".
#[test]
fn a_place_backed_by_a_pool_is_charged_once_on_that_physical_memory() {
    let mut ledger = Ledger::default();
    ledger.place(Tier::Vram, Bytes(32 * GB), Bytes(32 * GB), &mut Vec::new());
    ledger.place(Tier::Pinned, Bytes(16 * GB), Bytes(16 * GB), &mut Vec::new());
    ledger.back(Tier::Vram, Some(Tier::Pinned));
    let gpu = granted(ledger.prepare(SERVING, FIRST, &[Need { place: Tier::Vram, amount: Bytes(10 * GB) }]));
    assert_eq!(ledger.accounts(Tier::Pinned).map(|a| a.prepared), Some(Bytes(10 * GB)), "the pool carries the VRAM claim");
    assert_eq!(ledger.room_for(TRAINER, Tier::Vram), Some(Bytes(6 * GB)), "VRAM room is the pool's room");
    assert!(matches!(ledger.prepare(TRAINER, FIRST, &[Need { place: Tier::Pinned, amount: Bytes(10 * GB) }]), Answer::Waiting { .. }), "host and VRAM share one memory");
    assert!(matches!(ledger.prepare(SERVING, FIRST, &[Need { place: Tier::Vram, amount: Bytes(20 * GB) }]), Answer::Never { capacity: Bytes(c), .. } if c == 16 * GB), "past the physical memory: never here");
    assert_eq!(ledger.unwind(gpu.id()), Ok(()));
    assert_eq!(ledger.accounts(Tier::Pinned).map(|a| a.prepared), Some(Bytes(0)), "returned to the pool too");
}

// what this catches (the kernel's reservations, a live call's floor): inference granted into the
// bytes a call is guaranteed, and the call itself kept out of its own floor.
#[test]
fn a_floor_is_kept_for_its_owner_and_others_are_granted_around_it() {
    const CALL: Owner = Owner(3);
    let mut ledger = Ledger::default();
    ledger.place(Tier::Vram, Bytes(8 * GB), Bytes(8 * GB), &mut Vec::new());
    ledger.floor(CALL, Tier::Vram, Bytes(4 * GB));
    assert_eq!(ledger.room_for(TRAINER, Tier::Vram), Some(Bytes(4 * GB)), "others are granted around the floor");
    assert_eq!(ledger.room_for(CALL, Tier::Vram), Some(Bytes(8 * GB)), "its owner may fill it");
    assert!(matches!(ledger.prepare(TRAINER, FIRST, &[Need { place: Tier::Vram, amount: Bytes(5 * GB) }]), Answer::Waiting { .. }));
    let call = granted(ledger.prepare(CALL, FIRST, &[Need { place: Tier::Vram, amount: Bytes(3 * GB) }]));
    assert_eq!(ledger.room_for(TRAINER, Tier::Vram), Some(Bytes(4 * GB)), "a floor partly filled still owes the rest");
    assert_eq!(ledger.unwind(call.id()), Ok(()));
    ledger.floor(CALL, Tier::Vram, Bytes(0));
    let mut out = Vec::new();
    ledger.grant_waiting(&mut out);
    assert_eq!(out.len(), 1, "a withdrawn floor's room is granted to the wait");
}

// what this catches (Cormac, 09:24Z): a seat needing Vram and Ram, short on Vram, while a stream of
// later Ram asks spends the Ram it will need, so when Vram frees it is short on Ram behind them.
// The first wait in line holds its fitting places too.
#[test]
fn a_two_place_wait_is_granted_when_its_short_place_frees_despite_asks_on_its_other() {
    let mut ledger = Ledger::default();
    ledger.place(Tier::Vram, Bytes(32 * GB), Bytes(GB), &mut Vec::new());
    ledger.place(Tier::Pinned, Bytes(16 * GB), Bytes(8 * GB), &mut Vec::new());
    let seat = [Need { place: Tier::Vram, amount: Bytes(6 * GB) }, Need { place: Tier::Pinned, amount: Bytes(6 * GB) }];
    let Answer::Waiting { id, .. } = ledger.prepare(TRAINER, FIRST, &seat) else { panic!("test: short on Vram") };
    for owner in 10..14 {
        let ask = ledger.prepare(Owner(owner), FIRST, &[Need { place: Tier::Pinned, amount: Bytes(GB) }]);
        assert!(matches!(ask, Answer::Waiting { .. }), "a later Pinned ask does not spend the seat's Pinned");
    }
    let other = ledger.prepare(SERVING, FIRST, &[Need { place: Tier::Vram, amount: Bytes(1) }]);
    assert!(matches!(other, Answer::Waiting { .. }), "nor its Vram");
    ledger.place(Tier::Vram, Bytes(32 * GB), Bytes(7 * GB), &mut Vec::new());
    let mut out = Vec::new();
    ledger.grant_waiting(&mut out);
    assert_eq!(out.first().map(Claim::id), Some(id), "the seat first, once its short place frees");
    assert_eq!(out.len(), 1 + 2 + 1, "then the Pinned asks that still fit (2 of 4) and the 1-byte Vram ask");
}

/// Bytes on the test's two tiers, as one placer's domain.
struct Memory;

impl Domain for Memory {
    type Place = Tier;
    type Quantity = Bytes;
    const NAME: &'static str = "test-placer";
    const ASKS: crate::dataflow::TopicName = crate::dataflow::TopicName("test/place/asks");
    const SHORT: crate::dataflow::TopicName = crate::dataflow::TopicName("test/place/short");
}

// what this catches: a placer answering by a promise or an event a lagging consumer can lose (a lost
// grant leaks memory). Asks go in by its teller; each owner reads its whole table as state, so the
// seat that waited sees its grant once room appears, and a later ask past a shrunk capacity sees
// never-here, with nothing awaited and nothing held but the claim.
#[tokio::test]
async fn an_owner_reads_its_asks_as_state_waiting_then_granted_then_never_here() {
    let bus: &'static crate::component::Bus = Box::leak(Box::new(crate::component::Bus::new()));
    let placer = Placer::<Memory>::new(bus).expect("test: the topics are free"); // expect: the test's own precondition
    let running = crate::component::start(placer, bus, crate::dataflow::OwnerId(uuid::Uuid::from_u128(7)), 1);
    let mut asks = bus.reader::<Asks<Tier, Bytes>>(Memory::ASKS, key_of(TRAINER));
    let seat = AskId { owner: TRAINER, seq: 1 };
    assert!(running.tell(Act::Room { place: Tier::Vram, capacity: Bytes(32 * GB), budget: Bytes(5 * MB) }).await);
    assert!(running.tell(Act::Prepare { ask: seat, generation: FIRST, needs: vec![Need { place: Tier::Vram, amount: Bytes(5_700 * MB) }] }).await);
    let waiting = asks.changed().await.expect("test: the placer runs"); // expect: the test's own precondition
    assert!(matches!(waiting.value.0.as_slice(), [(1, Status::Waiting { .. })]), "it waits, naming its shortfall: {:?}", waiting.value);

    assert!(running.tell(Act::Room { place: Tier::Vram, capacity: Bytes(32 * GB), budget: Bytes(8 * GB) }).await);
    let granted = asks.changed().await.expect("test: the placer runs"); // expect: the test's own precondition
    let [(1, Status::Granted(claim))] = granted.value.0.as_slice() else { panic!("test: granted, got {:?}", granted.value) };
    assert!(claim.consume(Tier::Vram, Bytes(GB)), "spent straight from the claim, nothing awaited");

    let later = AskId { owner: TRAINER, seq: 2 };
    assert!(running.tell(Act::Prepare { ask: later, generation: FIRST, needs: vec![Need { place: Tier::Vram, amount: Bytes(20 * GB) }] }).await);
    asks.changed().await.expect("test: the placer runs"); // expect: the test's own precondition
    assert!(running.tell(Act::Room { place: Tier::Vram, capacity: Bytes(16 * GB), budget: Bytes(8 * GB) }).await);
    let never = asks.changed().await.expect("test: the placer runs"); // expect: the test's own precondition
    assert!(matches!(never.value.0.as_slice(), [(1, Status::Granted(_)), (2, Status::Never { capacity: Bytes(c), .. })] if *c == 16 * GB), "{:?}", never.value);
    assert!(running.tell(Act::Forget(later)).await);
    let forgot = asks.changed().await.expect("test: the placer runs"); // expect: the test's own precondition
    assert_eq!(forgot.value.0.len(), 1, "a read terminal answer leaves the table");
}

// what this catches (Codex on 9bf241b3b): one ask for 60 on a place and 60 on the pool of 100 it draws
// on, each checked alone against free room, granted as 120 of one physical memory.
#[test]
fn an_ask_is_summed_on_each_physical_memory_it_draws_on() {
    let mut ledger = Ledger::default();
    ledger.place(Tier::Pinned, Bytes(100), Bytes(100), &mut Vec::new());
    ledger.place(Tier::Vram, Bytes(100), Bytes(100), &mut Vec::new());
    ledger.back(Tier::Vram, Some(Tier::Pinned));
    let both = [Need { place: Tier::Vram, amount: Bytes(60) }, Need { place: Tier::Pinned, amount: Bytes(60) }];
    assert!(matches!(ledger.prepare(TRAINER, FIRST, &both), Answer::Never { place: Tier::Pinned, need: Bytes(120), capacity: Bytes(100) }), "past the memory itself: never here");
    let held = granted(ledger.prepare(SERVING, FIRST, &[Need { place: Tier::Pinned, amount: Bytes(30) }]));
    let fits = [Need { place: Tier::Vram, amount: Bytes(40) }, Need { place: Tier::Pinned, amount: Bytes(40) }];
    let Answer::Waiting { short, .. } = ledger.prepare(TRAINER, FIRST, &fits) else { panic!("test: 80 of the 70 left waits") };
    assert_eq!(short, vec![Need { place: Tier::Pinned, amount: Bytes(10) }], "the memory names what the ask lacks on it");
    assert_eq!(ledger.unwind(held.id()), Ok(()));
    let mut out = Vec::new();
    ledger.grant_waiting(&mut out);
    assert_eq!(out.len(), 1, "granted once the memory has room for both");
    assert_eq!(ledger.accounts(Tier::Pinned).map(|a| a.prepared), Some(Bytes(80)), "charged once on the memory");
}

// what this catches (the kernel's replace-myself budget, #438/#214): a place already over its budget
// read as zero free before what an owner would give back is added, so the add-back hides the debt; and
// an add-back past the place's capacity.
#[test]
fn room_if_released_repays_an_over_budget_place_and_never_passes_capacity() {
    let mut ledger = Ledger::default();
    ledger.place(Tier::Vram, Bytes(100), Bytes(100), &mut Vec::new());
    let peer = granted(ledger.prepare(SERVING, FIRST, &[Need { place: Tier::Vram, amount: Bytes(20) }]));
    assert!(peer.consume(Tier::Vram, Bytes(20)));
    assert_eq!(ledger.commit(peer.id()), Ok(()));
    ledger.place(Tier::Vram, Bytes(100), Bytes(10), &mut Vec::new()); // 90 resident beside it: 10 over
    assert_eq!(ledger.room_for(TRAINER, Tier::Vram), Some(Bytes(0)));
    assert_eq!(ledger.room_if(TRAINER, Tier::Vram, &[(Tier::Vram, Bytes(40))]), Some(Bytes(30)), "the 10 over is repaid first");
    assert_eq!(ledger.room_if(TRAINER, Tier::Vram, &[(Tier::Vram, Bytes(500))]), Some(Bytes(100)), "never past capacity");
}

// what this catches (Codex on 7e348e827): a backing that changes under live claims. Each tier holds 40;
// the GPU tier is then backed by the host pool and its claim released: the release must come off where
// the charge was posted, never off the pool's own owner's 40.
#[test]
fn a_backing_change_moves_what_is_charged_with_it() {
    let mut ledger = Ledger::default();
    ledger.place(Tier::Vram, Bytes(100), Bytes(100), &mut Vec::new());
    ledger.place(Tier::Pinned, Bytes(100), Bytes(100), &mut Vec::new());
    let held = |ledger: &mut Ledger<Tier, Bytes>, owner, place| {
        let claim = granted(ledger.prepare(owner, FIRST, &[Need { place, amount: Bytes(40) }]));
        assert!(claim.consume(place, Bytes(40)));
        assert_eq!(ledger.commit(claim.id()), Ok(()));
        claim.id()
    };
    let gpu = held(&mut ledger, TRAINER, Tier::Vram);
    held(&mut ledger, SERVING, Tier::Pinned);
    ledger.back(Tier::Vram, Some(Tier::Pinned));
    assert_eq!(ledger.accounts(Tier::Pinned).map(|a| a.held), Some(Bytes(80)), "the pool carries the GPU tier's charge once backed");
    assert_eq!(ledger.release(gpu), Ok(()));
    assert_eq!(ledger.accounts(Tier::Pinned).map(|a| a.held), Some(Bytes(40)), "the pool's own owner still holds its 40");
    ledger.back(Tier::Vram, None);
    assert_eq!(ledger.accounts(Tier::Pinned).map(|a| a.held), Some(Bytes(40)), "unbacking moves nothing that is not there");
}

// what this catches (Codex on 66c1fb471): a stage carved from a reserve by releasing and taking again,
// where a wait or another taker can come between, or the taking fails after the release. A split moves
// the bytes as one transition: the place's and its pool's charges never move, and no wait is granted.
#[test]
fn a_split_moves_held_bytes_without_freeing_them() {
    let mut ledger = Ledger::default();
    ledger.place(Tier::Vram, Bytes(100), Bytes(100), &mut Vec::new());
    ledger.place(Tier::Pinned, Bytes(100), Bytes(100), &mut Vec::new());
    ledger.back(Tier::Vram, Some(Tier::Pinned));
    let reserve = granted(ledger.prepare(TRAINER, FIRST, &[Need { place: Tier::Vram, amount: Bytes(100) }]));
    assert!(reserve.consume(Tier::Vram, Bytes(100)));
    assert_eq!(ledger.commit(reserve.id()), Ok(()));
    assert!(matches!(ledger.prepare(SERVING, FIRST, &[Need { place: Tier::Vram, amount: Bytes(10) }]), Answer::Waiting { .. }));
    let before = (ledger.accounts(Tier::Vram), ledger.accounts(Tier::Pinned));
    let stage = ledger.split(reserve.id(), Need { place: Tier::Vram, amount: Bytes(60) }, TRAINER, FIRST).expect("test: split"); // expect: the test's own precondition
    assert_eq!((ledger.accounts(Tier::Vram), ledger.accounts(Tier::Pinned)), before, "every charge unchanged");
    let mut out = Vec::new();
    ledger.grant_waiting(&mut out);
    assert!(out.is_empty(), "nothing was freed for the wait");
    assert_eq!(ledger.split(reserve.id(), Need { place: Tier::Vram, amount: Bytes(41) }, TRAINER, FIRST), Err(Refusal::NotOnPlace(reserve.id())), "never past what it holds");
    assert_eq!(ledger.release(stage), Ok(()));
    assert_eq!(ledger.accounts(Tier::Pinned).map(|a| a.held), Some(Bytes(40)), "the stage releases its own 60");
}

// what this catches (UNIFIED-PAGING.md, "Room by priority"): the one flexbox sharing a place's bytes as it
// shares a prompt's tokens, past u32, with floors first. The 5090's split at 15:41Z (MiB, Codex): weights
// fixed (floor == max), the Core's KV and workspace variable above a floor, a seat's 12 MB optimizer at
// a lower priority. With the card short, the seat gets its floor before any variable holder grows.
#[test]
fn the_flexbox_shares_bytes_floors_first_then_by_priority() {
    use super::{Bytes, Flex, Flexbox, Landed};
    const MIB: u64 = 1 << 20;
    let card = Bytes(32_607 * MIB);
    let weights = Bytes(17_402 * MIB);
    let claims = [
        Flex { priority: 10, floor: weights, min: weights, max: weights, required: true },
        Flex { priority: 8, floor: Bytes(2_048 * MIB), min: Bytes(4_096 * MIB), max: Bytes(8_704 * MIB), required: true }, // the stream's KV
        Flex { priority: 6, floor: Bytes(1_024 * MIB), min: Bytes(2_048 * MIB), max: Bytes(3_124 * MIB), required: true }, // the workspace
        Flex { priority: 3, floor: Bytes(12_058_624), min: Bytes(12_058_624), max: Bytes(12_058_624), required: false }, // a seat
    ];
    let (mut flexbox, mut shares) = (Flexbox::default(), Vec::new());
    let shared = flexbox.share(card, &claims, |a, b| a.cmp(&b), &mut shares);
    assert!(!shared.escalation_needed);
    assert_eq!(shares[0].amount, weights);
    assert_eq!((shares[3].amount, shares[3].landed), (Bytes(12_058_624), Landed::Satisfied), "the seat is granted before anything grows");
    let total: u64 = shares.iter().map(|share| share.amount.0).sum::<u64>() + shared.unallocated.0;
    assert_eq!(total, card.0, "every byte accounted once");
    assert!(shares[1].amount.0 > 4_096 * MIB && shares[1].amount.0 <= 8_704 * MIB, "the KV grows into what is left, never past its want");
}

mod room_by_priority {
    use super::*;

    const CORE: Owner = Owner(3);

    /// A committed holding of `owner` of `amount` on the card.
    fn hold(ledger: &mut Ledger<Tier, Bytes>, owner: Owner, amount: u64) -> ClaimId {
        let claim = granted(ledger.prepare(owner, FIRST, &[Need { place: Tier::Vram, amount: Bytes(amount) }]));
        assert!(claim.consume(Tier::Vram, Bytes(amount)));
        assert_eq!(ledger.commit(claim.id()), Ok(()));
        claim.id()
    }

    /// The 5090 at 15:41Z (Codex, MiB): weights fixed, KV and workspace elastic, the card all but full.
    fn the_5090() -> (Ledger<Tier, Bytes>, ClaimId, ClaimId) {
        let mut ledger = Ledger::default();
        let held = (17_402 + 8_704 + 3_124) * MB;
        ledger.place(Tier::Vram, Bytes(32_607 * MB), Bytes(held + 400 * 1024), &mut Vec::new());
        hold(&mut ledger, CORE, 17_402 * MB);
        let kv = hold(&mut ledger, CORE, 8_704 * MB);
        let workspace = hold(&mut ledger, CORE, 3_124 * MB);
        assert_eq!(ledger.elastic(kv, Tier::Vram, Bytes(2_048 * MB), 8), Ok(()));
        assert_eq!(ledger.elastic(workspace, Tier::Vram, Bytes(1_024 * MB), 6), Ok(()));
        (ledger, kv, workspace)
    }

    const SEAT: u64 = 12_058_624; // the 5090's asks, 13 of them in one window

    // what this catches (Joel, 2026-11-26: "make room by priority"): a wait shrinking a holding that
    // outranks it, or asking anything with no priority of its own. Below both elastic holdings, the seat
    // waits for what frees by itself.
    #[test]
    fn a_wait_below_every_holding_or_with_no_priority_shrinks_no_one() {
        let (mut ledger, _, _) = the_5090();
        let Answer::Waiting { id, .. } = ledger.prepare(TRAINER, FIRST, &[Need { place: Tier::Vram, amount: Bytes(SEAT) }]) else { panic!("test: the seat waits") };
        let mut shrinks = Vec::new();
        ledger.shrinks(&mut shrinks);
        assert!(shrinks.is_empty(), "no priority: it asks nothing");
        assert_eq!(ledger.prioritize(id, 3), Ok(()));
        ledger.shrinks(&mut shrinks);
        assert!(shrinks.is_empty(), "below the KV and the workspace: it asks nothing of either");
    }

    // what this catches: a wait that outranks a holding still never granted (the 5090's 13 waits), or
    // taking more than it lacks, or from a holding above it, or before the holder's release. Aged past the
    // workspace, the seat asks the workspace alone for exactly what it lacks, and is granted only once the
    // workspace has shrunk.
    #[test]
    fn a_wait_that_outranks_a_holding_shrinks_it_by_what_it_lacks_and_is_granted_after_the_release() {
        let (mut ledger, kv, workspace) = the_5090();
        let Answer::Waiting { id, short } = ledger.prepare(TRAINER, FIRST, &[Need { place: Tier::Vram, amount: Bytes(SEAT) }]) else { panic!("test: the seat waits") };
        let lacks = short[0].amount.0;
        assert_eq!(ledger.prioritize(id, 7), Ok(()));
        let mut shrinks = Vec::new();
        ledger.shrinks(&mut shrinks);
        assert_eq!(shrinks, vec![ShrinkTo { id: workspace, place: Tier::Vram, to: Bytes(3_124 * MB - lacks) }], "the workspace alone, by what the seat lacks; the KV outranks it");
        assert!(shrinks.iter().all(|shrink| shrink.id != kv));
        let mut granted_now = Vec::new();
        ledger.grant_waiting(&mut granted_now);
        assert!(granted_now.is_empty(), "a shrink asked is not room: nothing is granted before the release");
        assert_eq!(ledger.shrink(workspace, Tier::Vram, Bytes(lacks)), Ok(Bytes(3_124 * MB - lacks)), "the workspace released what it was asked, fenced");
        ledger.grant_waiting(&mut granted_now);
        assert_eq!(granted_now.iter().map(Claim::id).collect::<Vec<_>>(), vec![id], "then the seat is granted");
    }

    // what this catches: shrinking for a wait that could not fit even with every elastic holding at its
    // floor, giving up room for nothing.
    #[test]
    fn a_wait_that_could_not_fit_even_at_every_floor_asks_no_one() {
        let (mut ledger, _, _) = the_5090();
        let Answer::Waiting { id, .. } = ledger.prepare(TRAINER, FIRST, &[Need { place: Tier::Vram, amount: Bytes(10_000 * MB) }]) else { panic!("test: it waits") };
        assert_eq!(ledger.prioritize(id, 9), Ok(()));
        let mut shrinks = Vec::new();
        ledger.shrinks(&mut shrinks);
        assert!(shrinks.is_empty());
    }
}

// what this catches (Joel, 2026-11-26: "make room by priority"; "everything memory related needs to feed
// back"): the shrink reaching a holder only through a side channel, or a wait granted before the holder's
// fenced release. Through the component: the Core reads Shrink on its own table, says Shrunk after its
// fence, and the seat then reads Granted on its own.
#[tokio::test]
async fn a_holder_reads_its_shrink_and_the_seat_its_grant_after_the_fenced_release() {
    let bus: &'static crate::component::Bus = Box::leak(Box::new(crate::component::Bus::new()));
    let placer = Placer::<Memory>::new(bus).expect("test: the topics are free"); // expect: the test's own precondition
    let running = crate::component::start(placer, bus, crate::dataflow::OwnerId(uuid::Uuid::from_u128(8)), 1);
    let mut core = bus.reader::<Asks<Tier, Bytes>>(Memory::ASKS, key_of(SERVING));
    let mut seat = bus.reader::<Asks<Tier, Bytes>>(Memory::ASKS, key_of(TRAINER));
    assert!(running.tell(Act::Room { place: Tier::Vram, capacity: Bytes(32 * GB), budget: Bytes(4 * GB) }).await);
    let workspace = AskId { owner: SERVING, seq: 1 };
    assert!(running.tell(Act::Prepare { ask: workspace, generation: FIRST, needs: vec![Need { place: Tier::Vram, amount: Bytes(4 * GB) }] }).await);
    let held = core.changed().await.expect("test: the placer runs"); // expect: the test's own precondition
    let [(1, Status::Granted(claim))] = held.value.0.as_slice() else { panic!("test: granted, got {:?}", held.value) };
    assert!(claim.consume(Tier::Vram, Bytes(4 * GB)));
    assert!(running.tell(Act::Commit(workspace)).await);
    assert!(running.tell(Act::Elastic { ask: workspace, place: Tier::Vram, floor: Bytes(GB), priority: 6 }).await);

    let lesson = AskId { owner: TRAINER, seq: 1 };
    assert!(running.tell(Act::Prepare { ask: lesson, generation: FIRST, needs: vec![Need { place: Tier::Vram, amount: Bytes(12 * MB) }] }).await);
    assert!(running.tell(Act::Prioritize { ask: lesson, priority: 7 }).await);
    let asked = loop {
        let now = core.changed().await.expect("test: the placer runs"); // expect: the test's own precondition
        if let [(1, Status::Shrink { place: Tier::Vram, to })] = now.value.0.as_slice() {
            break *to;
        }
    };
    assert_eq!(asked, Bytes(4 * GB - 12 * MB), "asked for exactly what the seat lacks");
    let waiting = seat.changed().await.expect("test: the placer runs"); // expect: the test's own precondition
    assert!(matches!(waiting.value.0.as_slice(), [(1, Status::Waiting { .. })]), "no grant before the release: {:?}", waiting.value);

    assert!(running.tell(Act::Shrunk { ask: workspace, place: Tier::Vram, by: Bytes(12 * MB) }).await);
    let granted = loop {
        let now = seat.changed().await.expect("test: the placer runs"); // expect: the test's own precondition
        if let [(1, Status::Granted(claim))] = now.value.0.as_slice() {
            break claim.clone();
        }
    };
    assert!(granted.consume(Tier::Vram, Bytes(12 * MB)), "the seat spends what the workspace gave back");
}

mod residents {
    use super::*;
    use std::time::Duration;

    /// The one method an elastic part writes (the levels do the rest): its allocation moves where it is asked.
    struct Allocation;

    impl Resize<Bytes> for Allocation {
        fn resize(&mut self, ask: ResizeTo<Bytes>) -> Moved<Bytes> {
            Moved::Done { now: ask.to, took: Duration::ZERO }
        }
    }

    fn facts(held: u64, qos: crate::Qos) -> Facts<Tier, Bytes> {
        Facts { place: Tier::Vram, held: Bytes(held), qos, waited: Duration::ZERO }
    }

    /// A holding nothing can shrink: no code of its own.
    fn fixed(held: u64, qos: crate::Qos, why: &'static str) -> Box<dyn Resident<Tier, Bytes> + Send> {
        Box::new(Fixed { facts: facts(held, qos), why })
    }

    /// A holding that resizes from `floor` up to what it holds now.
    fn resizing(held: u64, floor: u64, qos: crate::Qos) -> Box<dyn Resident<Tier, Bytes> + Send> {
        Box::new(Resizing { facts: facts(held, qos), floor: Bytes(floor), want: Bytes(held), cost: Cost::default(), leaf: Allocation })
    }
    /// The law's priority in these tests: the class alone (the system's point of view).
    fn by_class(row: &Row<Tier, Bytes>) -> u8 {
        row.qos as u8 * 64
    }

    /// A Core: the library's composite over mixed parts, nothing written here.
    fn core(parts: Vec<Box<dyn Resident<Tier, Bytes> + Send>>) -> Composite<Tier, Bytes> {
        let mut core = Composite::new(by_class);
        for part in parts {
            core.push(part);
        }
        core
    }

    // what this catches (Joel, 2026-11-26: "you do this with objects"; "A host of a base model can't just
    // shrink"): a composite that cannot hold mixed children, a shrink that reaches a fixed holding, or the
    // system's need ignored (an Interactive turn's KV giving before a Background workspace). The Core folds
    // its parts' reports and shares a shrink so the lowest class gives first and the weights are never asked.
    #[test]
    fn a_composite_shares_a_shrink_lowest_class_first_and_never_asks_the_fixed() {
        let mut core = core(vec![
                fixed(17_402 * MB, crate::Qos::Interactive, "the base model serves live minds"),
                resizing(8_704 * MB, 2_048 * MB, crate::Qos::Interactive),
                resizing(3_124 * MB, 1_024 * MB, crate::Qos::Background),
        ]);
        let mut report = Report::default();
        core.report(&mut report);
        assert_eq!(report.rows().len(), 3, "the parent's report is its children's, folded");
        let total: u64 = report.rows().iter().map(|row| row.held.0).sum();
        let given = core.shrink(ShrinkAsk { place: Tier::Vram, to: Bytes(total - 512 * MB), for_qos: crate::Qos::Utility });
        assert_eq!(given, Given::Done { way: Way::Resize, amount: Bytes(512 * MB), took: Duration::ZERO });
        report.clear();
        core.report(&mut report);
        let held: Vec<u64> = report.rows().iter().map(|row| row.held.0 / MB).collect();
        assert_eq!(held, vec![17_402, 8_704, 3_124 - 512], "the Background workspace gave it all; the weights and the turn's KV kept theirs");
    }

    /// A live call: its video at a level (elastic down to audio's floor), needing the encoder's memory
    /// beside it for as long as it holds any.
    struct Call {
        held: u64,
        encoder: u64,
    }

    impl Resident<Tier, Bytes> for Call {
        fn report(&self, into: &mut Report<Tier, Bytes>) {
            let ways = Ways { resize: None, cache_out: None, collapse: Some(Cost::default()) };
            let row = into.row(Row { place: Tier::Vram, held: Bytes(self.held), want: Bytes(self.held), varies: Varies::Elastic { floor: Bytes(64 * MB), ways }, qos: crate::Qos::Interactive, waited: Duration::ZERO });
            into.requires(row, Tier::Vram, Bytes(self.encoder), "the encoder its level streams through");
        }
        fn shrink(&mut self, ask: ShrinkAsk<Tier, Bytes>) -> Given<Bytes> {
            let freed = self.held.saturating_sub(ask.to.0);
            self.held -= freed;
            Given::Done { way: Way::Collapse, amount: Bytes(freed), took: Duration::ZERO } // a lower level, never a dropped call
        }
        fn grow(&mut self, _: GrowGrant<Tier, Bytes>) -> Given<Bytes> {
            Given::Refused { why: "not in this test" }
        }
    }

    // what this catches (Joel, 2026-11-26: "ask you what other parts you need (live video example)"): a
    // shrink freeing what a held row requires, which saves nothing and breaks the row, or dropping the call
    // instead of lowering its level. With 3 GiB to find and a Background workspace that can give 1.5 GiB of
    // its 2 GiB, the call gives the rest by a lower level, and the encoder it requires is never counted as
    // room.
    #[test]
    fn a_shrink_keeps_what_a_held_row_requires_and_lowers_a_level_instead_of_dropping() {
        let mut core = core(vec![
            Box::new(Call { held: 4 * GB, encoder: 512 * MB }),
            resizing(2 * GB, 512 * MB, crate::Qos::Background),
        ]);
        // what the card holds of them: their rows and the encoder the call requires
        let card = 4 * GB + 2 * GB + 512 * MB;
        let given = core.shrink(ShrinkAsk { place: Tier::Vram, to: Bytes(card - 3 * GB), for_qos: crate::Qos::Interactive });
        assert!(matches!(given, Given::Done { amount: Bytes(moved), .. } if moved == 3 * GB), "3 GiB moved in all: {given:?}"); // each part's own way is its own receipt
        let mut report = Report::default();
        core.report(&mut report);
        let held: Vec<u64> = report.rows().iter().map(|row| row.held.0 / MB).collect();
        assert_eq!(held, vec![4_096 - 1_536, 512], "the workspace to its floor first, then the call a level lower; the encoder untouched");
        assert_eq!(report.requirements(0).map(|requires| requires.amount.0 / MB).collect::<Vec<_>>(), vec![512]);
    }

    /// A holder whose cache-out lands later: it answers Pending, and its owner reports the fence.
    struct Paging {
        held: u64,
        want: u64,
    }

    impl Resident<Tier, Bytes> for Paging {
        fn report(&self, into: &mut Report<Tier, Bytes>) {
            let ways = Ways { resize: None, cache_out: Some((Tier::Pinned, Cost::default())), collapse: None };
            into.row(Row { place: Tier::Vram, held: Bytes(self.held), want: Bytes(self.want), varies: Varies::Elastic { floor: Bytes(0), ways }, qos: crate::Qos::Utility, waited: Duration::ZERO });
        }
        fn shrink(&mut self, ask: ShrinkAsk<Tier, Bytes>) -> Given<Bytes> {
            self.held = ask.to.0; // the copy is started; its bytes are released when its fence lands
            Given::Pending { way: Way::CacheOut }
        }
        fn grow(&mut self, grant: GrowGrant<Tier, Bytes>) -> Given<Bytes> {
            let took = grant.to.0.saturating_sub(self.held);
            self.held = grant.to.0;
            Given::Done { way: Way::Restore, amount: Bytes(took), took: Duration::ZERO }
        }
    }

    // what this catches (SAGE.md, the Resident parent): a holder that has to publish, track its pending
    // fences or keep its own receipts. The parent does all three: it publishes only on change, holds the
    // Pending ask until the fence of its own ticket lands, and keeps every exchange as a receipt for the law. A late
    // or duplicate completion, or the fence of another ask on the same place, never settles the wrong one (Codex).
    #[test]
    fn the_parent_keeps_receipts_holds_a_pending_ask_until_its_fence_and_publishes_only_on_change() {
        let mut holding = Holding::new(Paging { held: GB, want: GB });
        assert_eq!(holding.report().1, 1, "its first report is read at once");
        assert!(!holding.refresh(), "nothing changed: nothing to publish");
        let first = holding.shrink(ShrinkAsk { place: Tier::Vram, to: Bytes(512 * MB), for_qos: crate::Qos::Interactive });
        let second = holding.shrink(ShrinkAsk { place: Tier::Vram, to: Bytes(256 * MB), for_qos: crate::Qos::Interactive });
        assert_eq!((first.given, second.given), (Given::Pending { way: Way::CacheOut }, Given::Pending { way: Way::CacheOut }));
        assert_ne!(first.ticket, second.ticket, "two asks on one place, two tickets");
        assert_eq!(holding.report().1, 3, "each shrink changed its report");
        // the second fence lands first: it settles its own ask, never the older one on the same place
        assert_eq!(holding.landed(second.ticket, Way::CacheOut, Bytes(256 * MB), Duration::from_millis(2)), Landing::Settled);
        assert!(holding.pending(), "the first ask still waits on its own fence");
        assert_eq!(holding.landed(second.ticket, Way::CacheOut, Bytes(256 * MB), Duration::from_millis(2)), Landing::Unknown, "a duplicate completion changes nothing");
        assert_eq!(holding.landed(first.ticket, Way::CacheOut, Bytes(512 * MB), Duration::from_millis(3)), Landing::Settled);
        assert!(!holding.pending());
        let mut receipts = Vec::new();
        holding.receipts(&mut receipts);
        assert_eq!(receipts.len(), 4, "two Pending answers and their two fences' Done");
        assert!(matches!(receipts[2], Receipt { ticket, given: Given::Done { amount: Bytes(a), .. }, .. } if ticket == second.ticket && a == 256 * MB));
        holding.receipts(&mut receipts);
        assert!(receipts.is_empty(), "drained once");
    }

    // what this catches: room returning and nobody growing back, or the lowest class growing first. Granted
    // room, the composite grows its parts toward their wants highest class first; a fixed part never grows.
    #[test]
    fn a_composite_grows_its_parts_toward_their_wants_highest_class_first() {
        let mut core = core(vec![
            fixed(2 * GB, crate::Qos::Interactive, "weights"),
            Box::new(Paging { held: GB, want: 3 * GB }),
            resizing(GB, GB, crate::Qos::Background),
        ]);
        let given = core.grow(GrowGrant { place: Tier::Vram, to: Bytes(2 * GB + GB + GB + 2 * GB) });
        assert!(matches!(given, Given::Done { amount: Bytes(a), .. } if a == 2 * GB), "{given:?}");
        let mut report = Report::default();
        core.report(&mut report);
        let held: Vec<u64> = report.rows().iter().map(|row| row.held.0 / GB).collect();
        assert_eq!(held, vec![2, 3, 1], "the Utility part grew to its want; the fixed and the Background part kept theirs");
    }

    /// A resize its owner cannot make now.
    struct Busy;

    impl Resize<Bytes> for Busy {
        fn resize(&mut self, _: ResizeTo<Bytes>) -> Moved<Bytes> {
            Moved::Refused { why: "a graph is in flight" }
        }
    }

    // what this catches (Joel, 2026-11-26: "only this part needs the abstract method"): a leaf that has to
    // police its own floor and want, or build its own row. The level does both: an ask below the floor stops at
    // the floor, a grant past the want stops at the want, and the leaf sees only from and to.
    #[test]
    fn the_resizing_level_keeps_floor_and_want_and_its_leaf_writes_one_method() {
        let mut workspace = Resizing { facts: facts(3 * GB, crate::Qos::Background), floor: Bytes(GB), want: Bytes(4 * GB), cost: Cost::default(), leaf: Allocation };
        let given = workspace.shrink(ShrinkAsk { place: Tier::Vram, to: Bytes(0), for_qos: crate::Qos::Interactive });
        assert_eq!(given, Given::Done { way: Way::Resize, amount: Bytes(2 * GB), took: Duration::ZERO }, "down to its floor, never below");
        assert_eq!(workspace.facts.held, Bytes(GB));
        let given = workspace.grow(GrowGrant { place: Tier::Vram, to: Bytes(10 * GB) });
        assert_eq!(given, Given::Done { way: Way::Resize, amount: Bytes(3 * GB), took: Duration::ZERO }, "up to its want, never past");
        assert!(matches!(workspace.shrink(ShrinkAsk { place: Tier::Pinned, to: Bytes(0), for_qos: crate::Qos::Interactive }), Given::Refused { .. }), "not its place");
        let mut busy = Resizing { facts: facts(3 * GB, crate::Qos::Background), floor: Bytes(GB), want: Bytes(3 * GB), cost: Cost::default(), leaf: Busy };
        assert_eq!(busy.shrink(ShrinkAsk { place: Tier::Vram, to: Bytes(GB), for_qos: crate::Qos::Interactive }), Given::Refused { why: "a graph is in flight" });
        assert_eq!(busy.facts.held, Bytes(3 * GB), "a refused resize holds what it held");
    }
}

// what this catches (Cormac's step 3: about 28 kernel sites call the authority synchronously and cannot
// await a teller): the placer's tables reachable only through the bus. The core alone, applied and settled
// in one call each, answers the same tables the component says.
#[test]
fn the_core_is_driven_synchronously_with_no_bus() {
    let mut core = PlacerCore::<Memory>::default();
    core.apply(Act::Room { place: Tier::Vram, capacity: Bytes(32 * GB), budget: Bytes(5 * MB) });
    let seat = AskId { owner: TRAINER, seq: 1 };
    core.apply(Act::Prepare { ask: seat, generation: FIRST, needs: vec![Need { place: Tier::Vram, amount: Bytes(12 * MB) }] });
    assert!(core.settle(), "the shortfall changed: the seat lacks 7 MB");
    let mut changed = Vec::new();
    core.changed(&mut changed);
    assert!(matches!(changed.as_slice(), [(TRAINER, Asks(rows))] if matches!(rows.as_slice(), [(1, Status::Waiting { .. })])));
    core.apply(Act::Room { place: Tier::Vram, capacity: Bytes(32 * GB), budget: Bytes(GB) });
    core.settle();
    core.changed(&mut changed);
    assert!(matches!(changed.as_slice(), [(TRAINER, Asks(rows))] if matches!(rows.as_slice(), [(1, Status::Granted(_))])), "granted once room is told");

}

// what this catches (Joel, 2026-11-26: "take what you know in the field and diagnose or even simulate"): a field
// run that can only be read from probes. The core records its acts; replayed into a fresh core anywhere, they give
// the same tables, grants and shrink asks; a ring that rolled off refuses to pass for the field.
#[test]
fn a_field_recording_replays_into_the_same_tables_anywhere() {
    let mut field = PlacerCore::<Memory>::recording(64);
    let workspace = AskId { owner: SERVING, seq: 1 };
    let seat = AskId { owner: TRAINER, seq: 1 };
    let acts = vec![
        Act::Room { place: Tier::Vram, capacity: Bytes(32 * GB), budget: Bytes(4 * GB) },
        Act::Prepare { ask: workspace, generation: FIRST, needs: vec![Need { place: Tier::Vram, amount: Bytes(4 * GB) }] },
    ];
    for act in acts {
        field.apply(act);
        field.settle();
    }
    let mut changed = Vec::new();
    field.changed(&mut changed);
    let Some((_, Asks(rows))) = changed.iter().find(|(owner, _)| *owner == SERVING) else { panic!("test: the workspace's table") };
    let [(1, Status::Granted(claim))] = rows.as_slice() else { panic!("test: granted, got {rows:?}") };
    assert!(claim.consume(Tier::Vram, Bytes(4 * GB)));
    for act in [
        Act::Commit(workspace),
        Act::Elastic { ask: workspace, place: Tier::Vram, floor: Bytes(GB), priority: 6 },
        Act::Prepare { ask: seat, generation: FIRST, needs: vec![Need { place: Tier::Vram, amount: Bytes(12 * MB) }] },
        Act::Prioritize { ask: seat, priority: 7 },
    ] {
        field.apply(act);
        field.settle();
    }
    let mut simulated = replay::<Memory>(field.recorded()).expect("test: a whole recording replays"); // expect: the test's own precondition
    let mut tables = Vec::new();
    simulated.changed(&mut tables);
    assert_eq!(tables.len(), 2, "both owners' tables, rebuilt whole by the replay");
    assert!(tables.iter().any(|(owner, Asks(rows))| *owner == SERVING && matches!(rows.as_slice(), [(1, Status::Shrink { to: Bytes(t), .. })] if *t == 4 * GB - 12 * MB)), "the field's shrink ask, simulated: {tables:?}");
    assert!(tables.iter().any(|(owner, Asks(rows))| *owner == TRAINER && matches!(rows.as_slice(), [(1, Status::Waiting { .. })])), "the seat waits, as in the field");
    assert_eq!(field.shortfall(), simulated.shortfall(), "the same shortfall");
    let mut short = PlacerCore::<Memory>::recording(2);
    for act in [Act::Room { place: Tier::Vram, capacity: Bytes(GB), budget: Bytes(GB) }, Act::Forget(seat), Act::Forget(seat)] {
        short.apply(act);
    }
    assert_eq!(replay::<Memory>(short.recorded()).err(), Some(Partial { first: 1 }), "a rolled-off ring never passes for the field");
}
