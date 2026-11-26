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
