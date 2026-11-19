//! The engine's contract: one decode in flight, submitted only for the bound mind; staging and
//! retirement as fenced transfers through its tier; no handoff while a decode is on the device; and
//! custody never lost to a refusal.

use super::*;
use crate::persona::{Epoch, PersonaParts, PersonaUnit};
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
    released: Rc<RefCell<Vec<PersonaId>>>,
}
impl Tier for Recording {
    type Fence = Pending;
    fn name(&self) -> &'static str {
        "recording"
    }
    fn begin(&mut self, what: FenceTag) -> Result<Pending, TierRefusal> {
        if self.refuse.get() == Some(what.direction) {
            return Err(TierRefusal { tier: "recording", persona: what.persona, reason: "refused by the test", native: Native::NotAsked });
        }
        let landed = Rc::new(Cell::new(false));
        self.moves.borrow_mut().push((what, Rc::clone(&landed)));
        Ok(Pending { tag: what, landed })
    }
    fn release(&mut self, persona: PersonaId) -> Result<(), TierRefusal> {
        self.released.borrow_mut().push(persona);
        Ok(())
    }
}
impl Execute for Recording {
    type Work = ();
    type Done = Pending;
    fn submit(&mut self, persona: PersonaId, epoch: Epoch, (): ()) -> Result<Pending, TierRefusal> {
        let tag = FenceTag { persona, epoch, direction: Direction::In };
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
    fn checkpoint(&mut self, _persona: PersonaId, _epoch: Epoch, into: &mut Option<u32>) -> Result<u32, TierRefusal> {
        let at = u32::try_from(self.decodes.borrow().len()).unwrap_or(u32::MAX); // unwrap_or: a test log never holds 4 billion steps
        *into = Some(at);
        Ok(at)
    }
    fn rewind(&mut self, persona: PersonaId, _epoch: Epoch, from: &Option<u32>) -> Result<(), TierRefusal> {
        from.map(|_| ()).ok_or(TierRefusal { tier: "recording", persona, reason: "nothing taken", native: Native::NotAsked })
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

/// Mind 1 active (her copy-in landed, handed off), nothing in flight.
fn with_one_active() -> Harness {
    let mut h = engine();
    assert!(h.engine.stage(mind(1)).is_ok());
    land_last(&h.moves);
    assert_eq!(h.engine.handoff(), Ok(None));
    h
}

// what this catches: a decode run for a mind that is not the bound one (the caller naming a sequence,
// Codex on #6139), or two decodes in flight (the M5's --parallel 5, 2026-11-18). The tier is asked
// only for the active mind, and a second step waits for the first to land.
#[test]
fn a_step_runs_only_for_the_active_mind_and_one_at_a_time() {
    assert_eq!(engine().engine.submit(()), Err(DecodeError::NoneActive), "nobody active, nothing decodes");
    let mut h = with_one_active();
    assert!(h.engine.stage(mind(2)).is_ok());

    assert_eq!(h.engine.submit(()), Ok(()));
    let asked = h.decodes.borrow().last().map(|(tag, _)| tag.persona);
    assert_eq!(asked, Some(id(1)), "the tier decodes the active mind, never the staged one");
    assert_eq!(h.engine.submit(()), Err(DecodeError::InFlight { active: id(1) }), "one decode in flight");
    land_last(&h.decodes);
    assert_eq!(h.engine.submit(()), Ok(()), "the next step once the first landed");
    assert_eq!(h.decodes.borrow().len(), 2);
}

// what this catches: a switch cutting off a decode still on the device (Codex on #6139). With her step
// submitted and not landed, the next mind's copy-in still begins and lands, but the handoff is refused
// and changes nothing; once her step lands, the handoff binds next.
#[test]
fn a_handoff_waits_for_the_active_decode_while_the_next_mind_streams_in() {
    let mut h = with_one_active();
    assert_eq!(h.engine.submit(()), Ok(()));
    assert!(h.engine.stage(mind(2)).is_ok(), "staging progresses while she decodes");
    land_last(&h.moves);

    assert_eq!(h.engine.handoff(), Err(HandoffError::DecodeInFlight { active: id(1) }));
    assert_eq!(h.engine.active(), Some(id(1)), "she is still bound");
    assert_eq!(h.engine.staged(), Some(id(2)), "next is still staged");
    assert_eq!(h.moves.borrow().len(), 2, "no copy-out began for her");

    assert_eq!(h.engine.settle(), Ok(()), "the tier attests the device finished");
    assert!(h.engine.decode_landed());
    assert_eq!(h.engine.handoff(), Ok(Some(id(1))));
    assert_eq!(h.engine.active(), Some(id(2)));
    assert!(h.engine.decode_landed(), "the new mind starts with nothing in flight");
}

// what this catches: the swap as suspend-then-resume (MODULE-ARCHITECTURE 9.4). The previous mind's
// copy-out begins only after the next is bound, and she is given back only once it lands.
#[test]
fn the_previous_mind_retires_after_the_bind_and_is_released_once_her_copy_out_lands() {
    let mut h = with_one_active();
    assert!(h.engine.stage(mind(2)).is_ok());
    assert_eq!(h.moves.borrow().len(), 2, "two copy-ins, no copy-out yet");

    land_last(&h.moves);
    assert_eq!(h.engine.handoff(), Ok(Some(id(1))));
    assert_eq!(h.engine.active(), Some(id(2)));
    let last = h.moves.borrow().last().map(|(tag, _)| *tag);
    assert_eq!(last, Some(FenceTag { persona: id(1), epoch: Epoch(1), direction: Direction::Out }), "her copy-out began after the bind");

    assert!(matches!(h.engine.release_retired(), Err(RetireRefusal::NotLanded { .. })));
    land_last(&h.moves);
    assert_eq!(h.engine.release_retired().map(|unit| unit.id()), Ok(id(1)));
    assert_eq!(h.engine.retiring(), None);
}

// what this catches: a bind exposed before her bytes landed (fence before ready).
#[test]
fn a_handoff_before_the_copy_in_lands_changes_nothing() {
    let mut h = engine();
    assert!(h.engine.stage(mind(1)).is_ok());
    assert_eq!(h.engine.handoff(), Err(HandoffError::Pipeline(HandoffRefusal::NotReady { staged: id(1) })));
    assert_eq!(h.engine.active(), None);
    assert_eq!(h.engine.staged(), Some(id(1)));
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

// what this catches: a refused copy-out losing the retiring mind. She stays retiring in the engine's
// custody, the next stays active, and the copy-out begins on retry.
#[test]
fn a_refused_copy_out_keeps_her_retiring_until_it_begins() {
    let mut h = with_one_active();
    assert!(h.engine.stage(mind(2)).is_ok());
    land_last(&h.moves);

    h.refuse.set(Some(Direction::Out));
    assert!(matches!(h.engine.handoff(), Err(HandoffError::CopyOut { previous, .. }) if previous == id(1)));
    assert_eq!(h.engine.active(), Some(id(2)), "the bind happened");
    assert_eq!(h.engine.retiring(), Some(id(1)), "she is still held, awaiting her copy-out");
    assert!(matches!(h.engine.release_retired(), Err(RetireRefusal::NotBegun { .. })));

    h.refuse.set(None);
    assert_eq!(h.engine.retry_retire(), Ok(()));
    land_last(&h.moves);
    assert_eq!(h.engine.release_retired().map(|unit| unit.id()), Ok(id(1)));
}

// what this catches (Codex on #6140 at 17c208cf5): a refused copy-out skipped by the next switch, or
// the retiring mind handed out twice. finish_retirement keeps her retiring while her copy-out is
// refused or still moving, gives her back exactly once, and an A/B/A switch then holds one of each.
#[test]
fn a_refused_copy_out_is_finished_before_the_next_switch_and_given_back_once() {
    let mut h = with_one_active();
    assert!(h.engine.stage(mind(2)).is_ok());
    land_last(&h.moves);
    h.refuse.set(Some(Direction::Out));
    assert!(matches!(h.engine.handoff(), Err(HandoffError::CopyOut { .. })));

    assert!(matches!(h.engine.finish_retirement(), Err(RetireError::CopyOut(_))), "still refused");
    assert_eq!(h.engine.retiring(), Some(id(1)), "she stays in custody");
    h.refuse.set(None);
    assert!(matches!(h.engine.finish_retirement(), Err(RetireError::Release(RetireRefusal::NotLanded { .. }))), "begun, not landed");
    land_last(&h.moves);
    assert_eq!(h.engine.finish_retirement().map(|unit| unit.map(|unit| unit.id())), Ok(Some(id(1))));
    assert_eq!(h.engine.finish_retirement().map(|unit| unit.is_none()), Ok(true), "given back once");

    assert!(h.engine.stage(mind(1)).is_ok(), "A comes back after B");
    land_last(&h.moves);
    assert_eq!(h.engine.handoff(), Ok(Some(id(2))));
    land_last(&h.moves);
    assert_eq!(h.engine.finish_retirement().map(|unit| unit.map(|unit| unit.id())), Ok(Some(id(2))));
    assert_eq!((h.engine.active(), h.engine.staged(), h.engine.retiring()), (Some(id(1)), None, None));
}

// what this catches: dropping the blocks of a mind still in custody. A one-off's blocks are given up
// only once she has left the pipeline; while active or retiring the release is refused and the tier
// is not asked.
#[test]
fn a_mind_is_forgotten_only_after_she_left_the_pipeline() {
    let mut h = with_one_active();
    let released = Rc::clone(&h.engine.tier().released);
    assert!(h.engine.forget(id(1)).is_err(), "active");
    assert!(h.engine.stage(mind(2)).is_ok());
    land_last(&h.moves);
    assert_eq!(h.engine.handoff(), Ok(Some(id(1))));
    assert!(h.engine.forget(id(1)).is_err(), "retiring");
    land_last(&h.moves);
    assert_eq!(h.engine.release_retired().map(|unit| unit.id()), Ok(id(1)));
    assert_eq!(h.engine.forget(id(1)), Ok(()));
    assert_eq!(*released.borrow(), vec![id(1)], "the tier was asked once, after she left");
}

// what this catches (CONTINUUM-ENGINE 4.16, restore is the norm): a stop that cannot take the last
// mind out of the stream without staging another, so her suspend would move her blocks behind the
// pipeline's back. With nobody to follow, the active mind retires through the same copy-out a
// handoff begins, and is given back once it lands; nobody is active after.
#[test]
fn the_active_mind_retires_with_nobody_to_follow_through_the_same_copy_out() {
    let mut h = with_one_active();
    assert_eq!(h.engine.retire_active(), Ok(Some(id(1))));
    assert_eq!(h.engine.active(), None);
    let last = h.moves.borrow().last().map(|(tag, _)| *tag);
    assert_eq!(last, Some(FenceTag { persona: id(1), epoch: Epoch(1), direction: Direction::Out }), "her copy-out began");
    assert!(matches!(h.engine.release_retired(), Err(RetireRefusal::NotLanded { .. })));
    land_last(&h.moves);
    assert_eq!(h.engine.release_retired().map(|unit| unit.id()), Ok(id(1)));
    assert_eq!(h.engine.retire_active(), Ok(None), "nobody active: nothing retires");
}
