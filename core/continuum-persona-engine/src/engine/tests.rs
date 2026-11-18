//! The engine's contract: one decode, staging and retirement as fenced transfers through its tier,
//! and custody never lost to a refusal.

use super::*;
use crate::persona::{Epoch, PersonaParts, PersonaUnit};
use crate::residency::{Fence, FenceTag};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

type Unit = PersonaUnit<(), Vec<u32>, (), (), ()>;

fn id(n: u128) -> PersonaId {
    PersonaId(uuid::Uuid::from_u128(n))
}

fn mind(n: u128) -> Unit {
    PersonaUnit::new(id(n), Epoch(1), PersonaParts { blocks: (), tokens: Vec::new(), genome: (), learning: (), decode: () })
}

/// A transfer the test lands by hand.
struct Transfer {
    tag: FenceTag,
    landed: Rc<Cell<bool>>,
}
impl Fence for Transfer {
    fn tag(&self) -> FenceTag {
        self.tag
    }
    fn complete(&self) -> bool {
        self.landed.get()
    }
}

/// A tier that records every transfer it began, lands none until told, and refuses on request.
#[derive(Default)]
struct Recording {
    begun: Rc<RefCell<Vec<(FenceTag, Rc<Cell<bool>>)>>>,
    refuse: Rc<Cell<Option<Direction>>>,
}
impl Recording {
    fn handle(&self) -> (Rc<RefCell<Vec<(FenceTag, Rc<Cell<bool>>)>>>, Rc<Cell<Option<Direction>>>) {
        (Rc::clone(&self.begun), Rc::clone(&self.refuse))
    }
}
impl Tier for Recording {
    type Fence = Transfer;
    fn name(&self) -> &'static str {
        "recording"
    }
    fn begin(&mut self, what: FenceTag) -> Result<Transfer, TierRefusal> {
        if self.refuse.get() == Some(what.direction) {
            return Err(TierRefusal { tier: "recording", persona: what.persona, reason: "refused by the test" });
        }
        let landed = Rc::new(Cell::new(false));
        self.begun.borrow_mut().push((what, Rc::clone(&landed)));
        Ok(Transfer { tag: what, landed })
    }
}

type Begun = Rc<RefCell<Vec<(FenceTag, Rc<Cell<bool>>)>>>;

/// Land the most recent transfer the tier began.
fn land_last(begun: &Begun) {
    if let Some((_, landed)) = begun.borrow().last() {
        landed.set(true);
    }
}

fn engine() -> (PersonaEngine<Unit, Recording>, Begun, Rc<Cell<Option<Direction>>>) {
    let tier = Recording::default();
    let (begun, refuse) = tier.handle();
    (PersonaEngine::new(tier), begun, refuse)
}

// what this catches: a second mind decoding beside the first (the M5's --parallel 5, 2026-11-18).
// The decode reaches only the active mind; the staged one is a transfer, never a decode.
#[test]
fn decode_reaches_only_the_active_mind_while_the_next_streams_in() {
    let (mut engine, begun, _) = engine();
    assert!(engine.decode(|_, _| ()).is_none(), "nobody active, nothing decodes");
    assert!(engine.stage(mind(1)).is_ok());
    land_last(&begun);
    assert_eq!(engine.handoff(), Ok(None));
    assert!(engine.stage(mind(2)).is_ok());

    let decoded = engine.decode(|unit, _| {
        unit.parts();
        unit.id()
    });
    assert_eq!(decoded, Some(id(1)), "the active mind decodes");
    assert_eq!(engine.staged(), Some(id(2)), "the next is staged beside her, not decoding");
}

// what this catches: the swap as suspend-then-resume (MODULE-ARCHITECTURE 9.4). The previous mind's
// copy-out begins only after the next is bound, and she is given back only once it lands.
#[test]
fn the_previous_mind_retires_after_the_bind_and_is_released_once_her_copy_out_lands() {
    let (mut engine, begun, _) = engine();
    assert!(engine.stage(mind(1)).is_ok());
    land_last(&begun);
    assert_eq!(engine.handoff(), Ok(None));
    assert!(engine.stage(mind(2)).is_ok());
    assert_eq!(begun.borrow().len(), 2, "two copy-ins, no copy-out yet");

    land_last(&begun);
    assert_eq!(engine.handoff(), Ok(Some(id(1))));
    assert_eq!(engine.active(), Some(id(2)));
    let last = begun.borrow().last().map(|(tag, _)| *tag);
    assert_eq!(last, Some(FenceTag { persona: id(1), epoch: Epoch(1), direction: Direction::Out }), "her copy-out began after the bind");

    assert!(matches!(engine.release_retired(), Err(RetireRefusal::NotLanded { .. })));
    land_last(&begun);
    assert_eq!(engine.release_retired().map(|unit| unit.id()), Ok(id(1)));
    assert_eq!(engine.retiring(), None);
}

// what this catches: a bind exposed before her bytes landed (fence before ready).
#[test]
fn a_handoff_before_the_copy_in_lands_changes_nothing() {
    let (mut engine, _, _) = engine();
    assert!(engine.stage(mind(1)).is_ok());
    assert_eq!(engine.handoff(), Err(HandoffError::Pipeline(HandoffRefusal::NotReady { staged: id(1) })));
    assert_eq!(engine.active(), None);
    assert_eq!(engine.staged(), Some(id(1)));
}

// what this catches: a refusal that drops a mind. A refused stage starts no transfer and hands her back.
#[test]
fn a_refused_stage_starts_no_transfer_and_returns_her() {
    let (mut engine, begun, refuse) = engine();
    assert!(engine.stage(mind(1)).is_ok());
    let refused = engine.stage(mind(2)).err();
    assert!(matches!(
        refused.as_ref().map(|r| &r.reason),
        Some(StageError::Pipeline(StageRefusal::NextOccupied { .. }))
    ));
    assert_eq!(refused.map(|r| r.returned.id()), Some(id(2)), "she came back");
    assert_eq!(begun.borrow().len(), 1, "no copy-in began for the refused stage");

    let (mut fresh, fresh_begun, fresh_refuse) = self::engine();
    drop((refuse, begun));
    fresh_refuse.set(Some(Direction::In));
    let refused = fresh.stage(mind(3)).err();
    assert!(matches!(refused.as_ref().map(|r| &r.reason), Some(StageError::Tier(_))));
    assert_eq!(refused.map(|r| r.returned.id()), Some(id(3)));
    assert_eq!(fresh.staged(), None);
    assert!(fresh_begun.borrow().is_empty());
}

// what this catches: a refused copy-out losing the retiring mind. She stays retiring in the engine's
// custody, the next stays active, and the copy-out begins on retry.
#[test]
fn a_refused_copy_out_keeps_her_retiring_until_it_begins() {
    let (mut engine, begun, refuse) = engine();
    assert!(engine.stage(mind(1)).is_ok());
    land_last(&begun);
    assert_eq!(engine.handoff(), Ok(None));
    assert!(engine.stage(mind(2)).is_ok());
    land_last(&begun);

    refuse.set(Some(Direction::Out));
    assert!(matches!(engine.handoff(), Err(HandoffError::CopyOut { previous, .. }) if previous == id(1)));
    assert_eq!(engine.active(), Some(id(2)), "the bind happened");
    assert_eq!(engine.retiring(), Some(id(1)), "she is still held, awaiting her copy-out");
    assert!(matches!(engine.release_retired(), Err(RetireRefusal::NotBegun { .. })));

    refuse.set(None);
    assert_eq!(engine.retry_retire(), Ok(()));
    land_last(&begun);
    assert_eq!(engine.release_retired().map(|unit| unit.id()), Ok(id(1)));
}
