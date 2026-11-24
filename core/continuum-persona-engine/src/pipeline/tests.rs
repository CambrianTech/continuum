//! The Core's contract (4.14: each class has a test file of its contract). Units and fences are
//! non-Clone handles that count their drops, so a refusal that loses custody fails here.

use super::*;
use crate::persona::{Epoch, PersonaId, PersonaParts, PersonaUnit};
use crate::residency::{Direction, Fence, FenceTag};
use std::cell::Cell;
use std::rc::Rc;

/// Counts how many of the handles it was given have been dropped.
#[derive(Clone, Default)]
struct Drops(Rc<Cell<u32>>);
impl Drops {
    fn count(&self) -> u32 {
        self.0.get()
    }
}

/// Her blocks: not Clone; dropping it is counted.
struct Blocks(Drops);
impl Drop for Blocks {
    fn drop(&mut self) {
        self.0 .0.set(self.0 .0.get() + 1);
    }
}

/// A transfer the test lands by hand; not Clone, dropping it is counted.
struct Transfer {
    tag: FenceTag,
    landed: Rc<Cell<bool>>,
    drops: Drops,
}
impl Drop for Transfer {
    fn drop(&mut self) {
        self.drops.0.set(self.drops.0.get() + 1);
    }
}
impl Fence for Transfer {
    fn tag(&self) -> FenceTag {
        self.tag
    }
    fn complete(&self) -> bool {
        self.landed.get()
    }
}

type Unit = PersonaUnit<Blocks, Vec<u32>, u64, (), ()>;

fn id(n: u128) -> PersonaId {
    PersonaId(uuid::Uuid::from_u128(n))
}

fn mind(n: u128, drops: &Drops) -> Unit {
    PersonaUnit::new(
        id(n),
        Epoch(1),
        PersonaParts { blocks: Blocks(drops.clone()), tokens: vec![1, 2, 3], genome: 7, learning: (), decode: () },
    )
}

/// A transfer for persona `n`, epoch 1, in `direction`, and the switch that lands it.
fn transfer(n: u128, direction: Direction, drops: &Drops) -> (Transfer, Rc<Cell<bool>>) {
    let landed = Rc::new(Cell::new(false));
    let tag = FenceTag { mind: crate::persona::MindId { persona: id(n), epoch: Epoch(1) }, direction };
    (Transfer { tag, landed: Rc::clone(&landed), drops: drops.clone() }, landed)
}

/// Stage, land and seat persona `n` on `roles`.
fn seat(roles: &mut Roles<Unit, Transfer>, n: u128, drops: &Drops) {
    let (fence, landed) = transfer(n, Direction::In, drops);
    assert!(roles.stage(mind(n, drops), fence).is_ok(), "test: stage {n}");
    landed.set(true);
    assert_eq!(roles.seat(), Ok(id(n)), "test: seat {n}");
}

fn active(roles: &Roles<Unit, Transfer>) -> Option<PersonaId> {
    roles.active().map(Identified::persona)
}

// what this catches: every switch copying the active mind's whole state out and the next one in
// (63-259 ms per switch on the M5, 2026-11-24, one sequence for the whole Core). Seated minds stay in
// place; a switch between them only changes which one is active: no transfer begins and nothing moves.
// One is active at a time, so two minds never decode at once (the M5's --parallel 5, 2026-11-18).
#[test]
fn a_switch_between_seated_minds_moves_nothing_and_one_is_active() {
    let drops = Drops::default();
    let mut roles: Roles<Unit, Transfer> = Roles::default();
    seat(&mut roles, 1, &drops);
    seat(&mut roles, 2, &drops);
    assert_eq!(active(&roles), None, "seating never makes her active");
    assert_eq!(roles.activate(id(1)), Ok(None));
    assert_eq!(roles.activate(id(2)), Ok(Some(id(1))));
    assert_eq!(roles.activate(id(1)), Ok(Some(id(2))));
    assert_eq!(active(&roles), Some(id(1)));
    assert!(roles.is_seated(id(2)), "the other stays seated");
    assert_eq!((roles.staged(), roles.retiring(), roles.awaiting_copy_out()), (None, None, None), "no transfer");
    assert_eq!(drops.count(), 2, "only the two copy-in fences were dropped, by seating");
    assert_eq!(roles.activate(id(3)), Err(SeatedRefusal::NotSeated { persona: id(3) }), "an unseated mind is never active");
}

// what this catches (Codex on #6130): a refused stage dropping the unit and its in-flight copy.
// Every refusal hands both back, and nothing is dropped by the pipeline.
#[test]
fn a_refused_stage_returns_the_unit_and_its_fence() {
    let drops = Drops::default();
    let mut roles: Roles<Unit, Transfer> = Roles::default();
    let (fence, _l) = transfer(1, Direction::In, &drops);
    assert!(roles.stage(mind(1, &drops), fence).is_ok());

    let (fence, _l) = transfer(2, Direction::In, &drops);
    let refused = roles.stage(mind(2, &drops), fence).err().expect("test: next is occupied"); // expect: the test's own precondition
    assert_eq!(refused.reason, StageRefusal::NextOccupied { staged: id(1) });
    assert_eq!(drops.count(), 0, "nothing was dropped by the refusal");
    let (unit, fence) = refused.returned;
    assert_eq!((unit.id(), fence.tag().mind.persona), (id(2), id(2)), "custody came back whole");
    drop((unit, fence));
    assert_eq!(drops.count(), 2, "the caller, not the pipeline, released them");

    let mut empty: Roles<Unit, Transfer> = Roles::default();
    let (wrong, _l) = transfer(9, Direction::In, &drops);
    let refused = empty.stage(mind(3, &drops), wrong).err().expect("test: another persona's fence"); // expect: the test's own precondition
    assert!(matches!(refused.reason, StageRefusal::WrongFence { .. }));
    let (out, _l) = transfer(3, Direction::Out, &drops);
    let (unit, _) = refused.returned;
    let refused = empty.stage(unit, out).err().expect("test: a copy-out is not a copy-in"); // expect: the test's own precondition
    assert!(matches!(refused.reason, StageRefusal::WrongFence { .. }));
}

// what this catches: a mind exposed as seated before her bytes landed (fence-before-ready).
#[test]
fn seating_waits_for_the_copy_in_and_changes_nothing_until_then() {
    let drops = Drops::default();
    let mut roles: Roles<Unit, Transfer> = Roles::default();
    let (fence, landed) = transfer(1, Direction::In, &drops);
    assert!(roles.stage(mind(1, &drops), fence).is_ok());
    assert_eq!(roles.seat(), Err(SeatRefusal::NotReady { staged: id(1) }));
    assert!(!roles.is_seated(id(1)));
    landed.set(true);
    assert_eq!(roles.seat(), Ok(id(1)));
    assert_eq!(roles.seat(), Err(SeatRefusal::NothingStaged));
}

// what this catches (Codex on #6130): a copy-out started before the mind left her seat, or a second
// eviction while one is moving. Evicting takes no fence: she leaves her seat (and stops being active)
// first, then her copy-out is attached; the release waits for it to land.
#[test]
fn an_eviction_leaves_the_seat_first_and_releases_only_when_landed() {
    let drops = Drops::default();
    let mut roles: Roles<Unit, Transfer> = Roles::default();
    seat(&mut roles, 1, &drops);
    seat(&mut roles, 2, &drops);
    assert_eq!(roles.activate(id(1)), Ok(None));
    assert_eq!(roles.evict(id(1)), Ok(()));
    assert_eq!((active(&roles), roles.is_seated(id(1)), roles.retiring()), (None, false, Some(id(1))), "she left her seat and is not active");
    assert_eq!(roles.release_retired().err(), Some(RetireRefusal::NotBegun { retiring: id(1) }));

    let (wrong, _l) = transfer(2, Direction::Out, &drops);
    let refused = roles.begin_retire(wrong).err().expect("test: the seated mind's copy-out is not hers"); // expect: the test's own precondition
    assert!(matches!(refused.reason, RetireRefusal::WrongFence { .. }));
    drop(refused.returned);

    let (out, landed) = transfer(1, Direction::Out, &drops);
    assert!(roles.begin_retire(out).is_ok());
    let (again, _l) = transfer(1, Direction::Out, &drops);
    assert!(matches!(roles.begin_retire(again).err().map(|r| r.reason), Some(RetireRefusal::AlreadyMoving { .. })));
    assert_eq!(roles.release_retired().err(), Some(RetireRefusal::NotLanded { retiring: id(1) }));
    assert_eq!(roles.evict(id(2)), Err(SeatedRefusal::RetireInFlight { retiring: id(1) }), "one copy-out at a time");

    landed.set(true);
    let released = roles.release_retired().expect("test: landed"); // expect: the test's own precondition
    assert_eq!((released.id(), released.parts().tokens.clone()), (id(1), vec![1, 2, 3]), "she travels whole");
    assert_eq!(roles.evict(id(1)), Err(SeatedRefusal::NotSeated { persona: id(1) }));
    assert!(roles.is_seated(id(2)), "the other was never touched");
}

// what this catches: the same mind in two places at once in one Core.
#[test]
fn a_mind_already_in_the_core_is_not_staged_again() {
    let drops = Drops::default();
    let mut roles: Roles<Unit, Transfer> = Roles::default();
    seat(&mut roles, 1, &drops);
    let (fence, _l) = transfer(1, Direction::In, &drops);
    let refused = roles.stage(mind(1, &drops), fence).err().expect("test: already seated"); // expect: the test's own precondition
    assert_eq!(refused.reason, StageRefusal::AlreadyPresent { persona: id(1) });
}

/// A tier that issues tagged transfers for whatever it is asked to begin.
struct TestTier {
    drops: Drops,
}
impl crate::residency::Tier for TestTier {
    type Fence = Transfer;
    type Blocks = Blocks;
    fn name(&self) -> &'static str {
        "test"
    }
    fn blocks_for(&mut self, _mind: crate::persona::MindId) -> Result<Blocks, crate::residency::TierRefusal> {
        Ok(Blocks(self.drops.clone()))
    }
    fn begin(&mut self, mind: crate::persona::MindId, _blocks: &mut Blocks, direction: Direction) -> Result<Transfer, crate::residency::TierRefusal> {
        Ok(Transfer { tag: FenceTag { mind, direction }, landed: Rc::new(Cell::new(true)), drops: self.drops.clone() })
    }
}

// what this catches (Codex on #6130 at 9446c2be7): the copy-out begun for a persona by id alone,
// which can name another generation of her. The completion owner begins exactly the transfer the
// Core describes, and a transfer of another epoch is refused with the fence returned.
#[test]
fn the_completion_owner_begins_exactly_the_described_copy_out() {
    use crate::residency::Tier;
    let drops = Drops::default();
    let mut tier = TestTier { drops: drops.clone() };
    let mut roles: Roles<Unit, Transfer> = Roles::default();
    assert_eq!(roles.awaiting_copy_out(), None, "nobody awaits a copy-out yet");
    seat(&mut roles, 1, &drops);
    seat(&mut roles, 2, &drops);
    assert_eq!(roles.evict(id(1)), Ok(()));

    let described = roles.awaiting_copy_out().expect("test: she awaits her copy-out"); // expect: the test's own precondition
    assert_eq!(described, FenceTag { mind: crate::persona::MindId { persona: id(1), epoch: Epoch(1) }, direction: Direction::Out });

    let mut blocks = Blocks(drops.clone());
    let older = crate::persona::MindId { epoch: Epoch(0), ..described.mind };
    let stale = tier.begin(older, &mut blocks, described.direction).expect("test: the tier begins what it is told"); // expect: the test's own precondition
    let refused = roles.begin_retire(stale).err().expect("test: another generation's copy-out"); // expect: the test's own precondition
    assert!(matches!(refused.reason, RetireRefusal::WrongFence { .. }));

    let copy_out = tier.begin(described.mind, &mut blocks, described.direction).expect("test: begin the described transfer"); // expect: the test's own precondition
    assert!(roles.begin_retire(copy_out).is_ok());
    assert_eq!(roles.awaiting_copy_out(), None, "begun: no longer awaiting");
    assert_eq!(roles.release_retired().map(|u| u.epoch()), Ok(Epoch(1)));
}
