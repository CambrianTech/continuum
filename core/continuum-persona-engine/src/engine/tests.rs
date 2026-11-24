//! The engine's contract: one decode in flight, submitted only for the active mind; a switch between
//! seated minds moves nothing; seating and eviction as fenced transfers through its tier; no switch while
//! a decode is on the device; and custody never lost to a refusal.

use super::*;
use crate::persona::{Epoch, MindId, PersonaParts, PersonaUnit};
use crate::residency::{Fence, FenceTag, Tier};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

type Unit = PersonaUnit<(), Vec<u32>, (), (), ()>;

fn id(n: u128) -> PersonaId {
    PersonaId(uuid::Uuid::from_u128(n))
}

fn mind(n: u128) -> Unit {
    PersonaUnit::new(id(n), Epoch(1), PersonaParts { blocks: (), tokens: Vec::new(), genome: (), learning: (), decode: () })
}

/// A transfer or a decode the test lands by hand.
struct Pending {
    tag: FenceTag,
    landed: Rc<Cell<bool>>,
}
impl Fence for Pending {
    fn tag(&self) -> FenceTag {
        self.tag
    }
    fn complete(&self) -> bool {
        self.landed.get()
    }
}
impl Completion for Pending {
    fn complete(&self) -> bool {
        self.landed.get()
    }
}

type Log = Rc<RefCell<Vec<(FenceTag, Rc<Cell<bool>>)>>>;

/// A tier that records every transfer and decode it began, lands none until told, and refuses moves
/// on request. A decode is logged with the persona and epoch it was asked to run for.
#[derive(Default)]
struct Recording {
    moves: Log,
    decodes: Log,
    refuse: Rc<Cell<Option<Direction>>>,
}
impl Tier for Recording {
    type Fence = Pending;
    type Blocks = ();
    fn name(&self) -> &'static str {
        "recording"
    }
    fn blocks_for(&mut self, _mind: MindId) -> Result<(), TierRefusal> {
        Ok(())
    }
    fn begin(&mut self, mind: MindId, (): &mut (), direction: Direction) -> Result<Pending, TierRefusal> {
        if self.refuse.get() == Some(direction) {
            return Err(TierRefusal { tier: "recording", mind, reason: "refused by the test", native: Native::NotAsked });
        }
        let tag = FenceTag { mind, direction };
        let landed = Rc::new(Cell::new(false));
        self.moves.borrow_mut().push((tag, Rc::clone(&landed)));
        Ok(Pending { tag, landed })
    }
}
impl Execute for Recording {
    type Work = ();
    type Done = Pending;
    fn submit(&mut self, mind: MindId, (): &()) -> Result<Pending, TierRefusal> {
        let tag = FenceTag { mind, direction: Direction::In };
        let landed = Rc::new(Cell::new(false));
        self.decodes.borrow_mut().push((tag, Rc::clone(&landed)));
        Ok(Pending { tag, landed })
    }
    fn settle(&mut self) -> Result<(), TierRefusal> {
        for (_, landed) in self.decodes.borrow().iter() {
            landed.set(true);
        }
        Ok(())
    }
    type Checkpoint = Option<u32>;
    fn checkpoint(&mut self, _mind: MindId, into: &mut Option<u32>) -> Result<u32, TierRefusal> {
        let at = self.decodes.borrow().len() as u32; // a test log never holds 4 billion steps
        *into = Some(at);
        Ok(at)
    }
    fn rewind(&mut self, mind: MindId, from: &Option<u32>) -> Result<(), TierRefusal> {
        from.map(|_| ()).ok_or(TierRefusal { tier: "recording", mind, reason: "nothing taken", native: Native::NotAsked })
    }
}

/// Land the most recent entry of `log`.
fn land_last(log: &Log) {
    if let Some((_, landed)) = log.borrow().last() {
        landed.set(true);
    }
}

struct Harness {
    engine: PersonaEngine<Unit, Recording>,
    moves: Log,
    decodes: Log,
    refuse: Rc<Cell<Option<Direction>>>,
}

fn engine() -> Harness {
    let tier = Recording::default();
    let (moves, decodes, refuse) = (Rc::clone(&tier.moves), Rc::clone(&tier.decodes), Rc::clone(&tier.refuse));
    Harness { engine: PersonaEngine::new(tier), moves, decodes, refuse }
}

/// Bring `n` in: stage her, land her copy-in, seat her.
fn seat(h: &mut Harness, n: u128) {
    assert!(h.engine.stage(mind(n)).is_ok(), "test: stage {n}");
    land_last(&h.moves);
    assert_eq!(h.engine.seat(), Ok(id(n)), "test: seat {n}");
}

/// Minds 1 and 2 seated, 1 active, nothing in flight.
fn two_seated_one_active() -> Harness {
    let mut h = engine();
    seat(&mut h, 1);
    seat(&mut h, 2);
    assert_eq!(h.engine.activate(id(1)), Ok(None));
    h
}

fn copy_out_of(n: u128) -> FenceTag {
    FenceTag { mind: MindId { persona: id(n), epoch: Epoch(1) }, direction: Direction::Out }
}

// what this catches: a decode run for a mind that is not the active one (the caller naming a sequence,
// Codex on #6139), or two decodes in flight (the M5's --parallel 5, 2026-11-18). The tier is asked
// only for the active mind, and a second step waits for the first to land.
#[test]
fn a_step_runs_only_for_the_active_mind_and_one_at_a_time() {
    assert_eq!(engine().engine.submit(&()), Err(DecodeError::NoneActive), "nobody active, nothing decodes");
    let mut h = two_seated_one_active();
    assert_eq!(h.engine.submit(&()), Ok(()));
    let asked = h.decodes.borrow().last().map(|(tag, _)| tag.mind.persona);
    assert_eq!(asked, Some(id(1)), "the tier decodes the active mind, never another seated one");
    assert_eq!(h.engine.submit(&()), Err(DecodeError::InFlight { active: id(1) }), "one decode in flight");
    land_last(&h.decodes);
    assert_eq!(h.engine.submit(&()), Ok(()), "the next step once the first landed");
    assert_eq!(h.decodes.borrow().len(), 2);
}

// what this catches: every switch copying the active mind out and the next one in (63-259 ms on the
// M5, 2026-11-24), and a switch cutting off a decode still on the device (Codex on #6139). Between
// seated minds a switch begins no transfer at all; with her step not landed it is refused and changes
// nothing.
#[test]
fn a_switch_between_seated_minds_moves_nothing_and_waits_for_the_active_decode() {
    let mut h = two_seated_one_active();
    let moves = h.moves.borrow().len();
    assert_eq!(h.engine.submit(&()), Ok(()));
    assert_eq!(h.engine.activate(id(2)), Err(SwitchError::DecodeInFlight { active: id(1) }));
    assert_eq!(h.engine.active(), Some(id(1)), "she is still active");

    assert_eq!(h.engine.settle(), Ok(()), "the tier attests the device finished");
    assert_eq!(h.engine.activate(id(2)), Ok(Some(id(1))));
    assert_eq!(h.engine.activate(id(1)), Ok(Some(id(2))));
    assert_eq!(h.moves.borrow().len(), moves, "no copy-out or copy-in for any switch");
    assert!(h.engine.decode_landed(), "the new mind starts with nothing in flight");
    assert_eq!(h.engine.seated().count(), 2);
}

// what this catches: a seated mind exposed before her bytes landed (fence before ready).
#[test]
fn seating_before_the_copy_in_lands_changes_nothing() {
    let mut h = engine();
    assert!(h.engine.stage(mind(1)).is_ok());
    assert_eq!(h.engine.seat(), Err(SwitchError::Seat(SeatRefusal::NotReady { staged: id(1) })));
    assert!(!h.engine.is_seated(id(1)));
    assert_eq!(h.engine.staged(), Some(id(1)));
}

// what this catches: a page-out as suspend-then-resume (MODULE-ARCHITECTURE 9.4). Her copy-out begins
// only once she has left her seat, she is given back only once it lands, and the seated mind beside
// her is untouched. The active mind is evicted only once her decode landed.
#[test]
fn an_evicted_mind_leaves_first_and_is_released_once_her_copy_out_lands() {
    let mut h = two_seated_one_active();
    assert_eq!(h.engine.evict(id(2)), Ok(()));
    assert_eq!(h.moves.borrow().last().map(|(tag, _)| *tag), Some(copy_out_of(2)), "her copy-out began after she left");
    assert!(matches!(h.engine.release_retired(), Err(RetireRefusal::NotLanded { .. })));
    land_last(&h.moves);
    assert_eq!(h.engine.release_retired().map(|unit| unit.id()), Ok(id(2)));
    assert_eq!((h.engine.active(), h.engine.retiring()), (Some(id(1)), None), "the active mind was not touched");

    assert_eq!(h.engine.submit(&()), Ok(()));
    assert_eq!(h.engine.evict(id(1)), Err(SwitchError::DecodeInFlight { active: id(1) }));
    assert!(h.engine.is_seated(id(1)), "nothing changed");
    assert_eq!(h.engine.settle(), Ok(()));
    assert_eq!(h.engine.evict(id(1)), Ok(()));
    assert_eq!(h.engine.active(), None);
}

// what this catches: a refusal that drops a mind. A refused stage starts no transfer and hands her back.
#[test]
fn a_refused_stage_starts_no_transfer_and_returns_her() {
    let mut h = engine();
    assert!(h.engine.stage(mind(1)).is_ok());
    let refused = h.engine.stage(mind(2)).err();
    assert!(matches!(refused.as_ref().map(|r| &r.reason), Some(StageError::Pipeline(StageRefusal::NextOccupied { .. }))));
    assert_eq!(refused.map(|r| r.returned.id()), Some(id(2)), "she came back");
    assert_eq!(h.moves.borrow().len(), 1, "no copy-in began for the refused stage");

    let mut fresh = engine();
    fresh.refuse.set(Some(Direction::In));
    let refused = fresh.engine.stage(mind(3)).err();
    assert!(matches!(refused.as_ref().map(|r| &r.reason), Some(StageError::Tier(_))));
    assert_eq!(refused.map(|r| r.returned.id()), Some(id(3)));
    assert_eq!(fresh.engine.staged(), None);
    assert!(fresh.moves.borrow().is_empty());
}

// what this catches: a refused copy-out losing the evicted mind. She stays retiring in the engine's
// custody, and the copy-out begins on retry.
#[test]
fn a_refused_copy_out_keeps_her_retiring_until_it_begins() {
    let mut h = two_seated_one_active();
    h.refuse.set(Some(Direction::Out));
    assert!(matches!(h.engine.evict(id(2)), Err(SwitchError::CopyOut { previous, .. }) if previous == id(2)));
    assert_eq!(h.engine.retiring(), Some(id(2)), "she is still held, awaiting her copy-out");
    assert!(matches!(h.engine.release_retired(), Err(RetireRefusal::NotBegun { .. })));

    h.refuse.set(None);
    assert_eq!(h.engine.retry_retire(), Ok(()));
    land_last(&h.moves);
    assert_eq!(h.engine.release_retired().map(|unit| unit.id()), Ok(id(2)));
}

// what this catches (Codex on #6140 at 17c208cf5): a refused copy-out skipped by the next move, or the
// retiring mind handed out twice. finish_retirement keeps her retiring while her copy-out is refused or
// still moving, gives her back exactly once, and she can then be seated again.
#[test]
fn a_refused_copy_out_is_finished_before_the_next_move_and_given_back_once() {
    let mut h = two_seated_one_active();
    h.refuse.set(Some(Direction::Out));
    assert!(matches!(h.engine.evict(id(2)), Err(SwitchError::CopyOut { .. })));

    assert!(matches!(h.engine.finish_retirement(), Err(RetireError::CopyOut(_))), "still refused");
    assert_eq!(h.engine.retiring(), Some(id(2)), "she stays in custody");
    let held: Vec<_> = h.engine.units().map(|mind| mind.id()).collect();
    assert_eq!(held, [id(1), id(2)], "both the seated and the retiring mind remain in custody");
    h.refuse.set(None);
    assert!(matches!(h.engine.finish_retirement(), Err(RetireError::Release(RetireRefusal::NotLanded { .. }))), "begun, not landed");
    land_last(&h.moves);
    assert_eq!(h.engine.finish_retirement().map(|unit| unit.map(|unit| unit.id())), Ok(Some(id(2))));
    assert_eq!(h.engine.finish_retirement().map(|unit| unit.is_none()), Ok(true), "given back once");

    seat(&mut h, 2);
    assert_eq!(h.engine.activate(id(2)), Ok(Some(id(1))));
    assert_eq!((h.engine.active(), h.engine.staged(), h.engine.retiring()), (Some(id(2)), None, None));
}

// what this catches (CONTINUUM-ENGINE 4.16, restore is the norm): a stop that cannot take the active
// mind out without staging another, so her suspend would move her blocks behind the pipeline's back.
// She is evicted through the same copy-out a page-out begins, and is given back once it lands.
#[test]
fn the_active_mind_retires_with_nobody_to_follow_through_the_same_copy_out() {
    let mut h = two_seated_one_active();
    assert_eq!(h.engine.retire_active(), Ok(Some(id(1))));
    assert_eq!(h.engine.active(), None);
    assert_eq!(h.moves.borrow().last().map(|(tag, _)| *tag), Some(copy_out_of(1)), "her copy-out began");
    assert!(matches!(h.engine.release_retired(), Err(RetireRefusal::NotLanded { .. })));
    land_last(&h.moves);
    assert_eq!(h.engine.release_retired().map(|unit| unit.id()), Ok(id(1)));
    assert_eq!(h.engine.retire_active(), Ok(None), "nobody active: nothing retires");
}
