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
    let tag = FenceTag { persona: id(n), epoch: Epoch(1), direction };
    (Transfer { tag, landed: Rc::clone(&landed), drops: drops.clone() }, landed)
}

/// Stage, land and bind persona `n` on `roles`.
fn bind(roles: &mut Roles<Unit, Transfer>, n: u128, drops: &Drops) -> Option<PersonaId> {
    let (fence, landed) = transfer(n, Direction::In, drops);
    assert!(roles.stage(mind(n, drops), fence).is_ok(), "test: stage {n}");
    landed.set(true);
    roles.handoff().expect("test: handoff")
}

// what this catches: two minds decoding at once (the M5's --parallel 5, 2026-11-18). A Core holds
// one active mind; staging the next never makes her active.
#[test]
fn one_mind_is_active_and_the_next_is_staged_not_decoding() {
    let drops = Drops::default();
    let mut roles: Roles<Unit, Transfer> = Roles::default();
    assert_eq!(bind(&mut roles, 1, &drops), None);
    let (fence, _landed) = transfer(2, Direction::In, &drops);
    assert!(roles.stage(mind(2, &drops), fence).is_ok());
    assert_eq!(roles.active().map(Identified::persona), Some(id(1)));
    assert_eq!(roles.staged(), Some(id(2)));
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
    let refused = roles.stage(mind(2, &drops), fence).err().expect("test: next is occupied");
    assert_eq!(refused.reason, StageRefusal::NextOccupied { staged: id(1) });
    assert_eq!(drops.count(), 0, "nothing was dropped by the refusal");
    let (unit, fence) = refused.returned;
    assert_eq!((unit.id(), fence.tag().persona), (id(2), id(2)), "custody came back whole");
    drop((unit, fence));
    assert_eq!(drops.count(), 2, "the caller, not the pipeline, released them");

    let mut empty: Roles<Unit, Transfer> = Roles::default();
    let (wrong, _l) = transfer(9, Direction::In, &drops);
    let refused = empty.stage(mind(3, &drops), wrong).err().expect("test: another persona's fence");
    assert!(matches!(refused.reason, StageRefusal::WrongFence { .. }));
    let (out, _l) = transfer(3, Direction::Out, &drops);
    let (unit, _) = refused.returned;
    let refused = empty.stage(unit, out).err().expect("test: a copy-out is not a copy-in");
    assert!(matches!(refused.reason, StageRefusal::WrongFence { .. }));
}

// what this catches: a mind exposed as ready before her bytes landed (fence-before-ready).
#[test]
fn handoff_waits_for_the_copy_in_and_changes_nothing_until_then() {
    let drops = Drops::default();
    let mut roles: Roles<Unit, Transfer> = Roles::default();
    let (fence, landed) = transfer(1, Direction::In, &drops);
    assert!(roles.stage(mind(1, &drops), fence).is_ok());
    assert_eq!(roles.handoff(), Err(HandoffRefusal::NotReady { staged: id(1) }));
    assert!(roles.active().is_none());
    landed.set(true);
    assert_eq!(roles.handoff(), Ok(None));
    assert_eq!(roles.active().map(Identified::persona), Some(id(1)));
}

// what this catches (Codex on #6130): a copy-out started before the mind stopped being active.
// Handoff takes no fence: she leaves active first, then her copy-out is attached; the release
// waits for it to land, and the next handoff waits for the release.
#[test]
fn the_copy_out_begins_after_the_bind_and_releases_only_when_landed() {
    let drops = Drops::default();
    let mut roles: Roles<Unit, Transfer> = Roles::default();
    bind(&mut roles, 1, &drops);
    assert_eq!(bind(&mut roles, 2, &drops), Some(id(1)), "she leaves active as the next binds");
    assert_eq!(roles.release_retired().err(), Some(RetireRefusal::NotBegun { retiring: id(1) }));

    let (wrong, _l) = transfer(2, Direction::Out, &drops);
    let refused = roles.begin_retire(wrong).err().expect("test: the active mind's copy-out is not hers");
    assert!(matches!(refused.reason, RetireRefusal::WrongFence { .. }));
    drop(refused.returned);

    let (out, landed) = transfer(1, Direction::Out, &drops);
    assert!(roles.begin_retire(out).is_ok());
    let (again, _l) = transfer(1, Direction::Out, &drops);
    assert!(matches!(roles.begin_retire(again).err().map(|r| r.reason), Some(RetireRefusal::AlreadyMoving { .. })));
    assert_eq!(roles.release_retired().err(), Some(RetireRefusal::NotLanded { retiring: id(1) }));

    let (fence, l3) = transfer(3, Direction::In, &drops);
    assert!(roles.stage(mind(3, &drops), fence).is_ok());
    l3.set(true);
    assert_eq!(roles.handoff(), Err(HandoffRefusal::RetireInFlight { retiring: id(1) }));

    landed.set(true);
    let released = roles.release_retired().expect("test: landed");
    assert_eq!((released.id(), released.parts().tokens.clone()), (id(1), vec![1, 2, 3]), "she travels whole");
    assert_eq!(roles.handoff(), Ok(Some(id(2))));
}

// what this catches: the same mind in two places at once in one Core.
#[test]
fn a_mind_already_in_the_core_is_not_staged_again() {
    let drops = Drops::default();
    let mut roles: Roles<Unit, Transfer> = Roles::default();
    bind(&mut roles, 1, &drops);
    let (fence, _l) = transfer(1, Direction::In, &drops);
    let refused = roles.stage(mind(1, &drops), fence).err().expect("test: already active");
    assert_eq!(refused.reason, StageRefusal::AlreadyPresent { persona: id(1) });
}
