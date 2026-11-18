//! The pipeline's contract (4.14 "each class has its own test file of its contract").

use super::*;
use crate::persona::{Epoch, PersonaId, PersonaParts, PersonaUnit};
use crate::residency::Fence;
use std::cell::Cell;
use std::rc::Rc;

/// A transfer the test lands by hand.
#[derive(Clone, Default)]
struct Landing(Rc<Cell<bool>>);
impl Landing {
    fn land(&self) {
        self.0.set(true);
    }
}
impl Fence for Landing {
    fn complete(&self) -> bool {
        self.0.get()
    }
}

type Unit = PersonaUnit<&'static str, Vec<u32>, u64, (), ()>;

fn mind(n: u128) -> Unit {
    PersonaUnit::new(
        PersonaId(uuid::Uuid::from_u128(n)),
        Epoch(1),
        PersonaParts { blocks: "kv", tokens: vec![1, 2, 3], genome: 7, learning: (), decode: () },
    )
}

fn id(n: u128) -> PersonaId {
    PersonaId(uuid::Uuid::from_u128(n))
}

// what this catches: two minds decoding at once (the M5's --parallel 5, 2026-11-18). The
// pipeline holds one active mind; a second only arrives by handoff, which moves the first out.
#[test]
fn one_mind_is_active_and_the_next_is_staged_not_decoding() {
    let mut roles: Roles<Unit, Landing> = Roles::default();
    let first = Landing::default();
    roles.stage(mind(1), first.clone()).expect("test: stage");
    first.land();
    assert_eq!(roles.handoff(Landing::default()), Ok(None), "the first mind binds with nobody to retire");
    roles.stage(mind(2), Landing::default()).expect("test: stage the next while she decodes");
    assert_eq!(roles.active().map(Identified::persona), Some(id(1)), "staging never makes a second mind active");
    assert_eq!(roles.staged(), Some(id(2)));
    assert_eq!(
        roles.stage(mind(3), Landing::default()),
        Err(StageRefusal::NextOccupied { staged: id(2) }),
        "one staged mind at a time"
    );
}

// what this catches: a mind exposed as ready before her bytes landed (fence-before-ready).
#[test]
fn handoff_waits_for_the_stage_fence_and_changes_nothing_until_then() {
    let mut roles: Roles<Unit, Landing> = Roles::default();
    let staged = Landing::default();
    roles.stage(mind(1), staged.clone()).expect("test: stage");
    assert_eq!(roles.handoff(Landing::default()), Err(HandoffRefusal::NotReady { staged: id(1) }));
    assert!(roles.active().is_none(), "nothing bound before the fence");
    staged.land();
    assert_eq!(roles.handoff(Landing::default()), Ok(None));
    assert_eq!(roles.active().map(Identified::persona), Some(id(1)));
}

// what this catches: the previous mind freed or reused before her move out landed
// (retire-after-handoff), and a second handoff stacking a retirement on an unfinished one.
#[test]
fn the_previous_mind_retires_after_handoff_and_is_released_only_when_landed() {
    let mut roles: Roles<Unit, Landing> = Roles::default();
    let (a, b, c) = (Landing::default(), Landing::default(), Landing::default());
    roles.stage(mind(1), a.clone()).expect("test: stage");
    a.land();
    roles.handoff(Landing::default()).expect("test: first bind");
    roles.stage(mind(2), b.clone()).expect("test: stage");
    b.land();
    let out = Landing::default();
    assert_eq!(roles.handoff(out.clone()), Ok(Some(id(1))), "she retires as the next binds");
    assert_eq!(roles.active().map(Identified::persona), Some(id(2)));
    assert!(matches!(roles.release_retired(), Err(RetireRefusal::NotLanded { .. })));
    roles.stage(mind(3), c.clone()).expect("test: stage");
    c.land();
    assert_eq!(
        roles.handoff(Landing::default()),
        Err(HandoffRefusal::RetireInFlight { retiring: id(1) }),
        "a handoff waits for the previous retirement to land"
    );
    out.land();
    let released = roles.release_retired().expect("test: landed");
    assert_eq!(released.id(), id(1));
    assert_eq!(released.parts().tokens, vec![1, 2, 3], "her token history travels with her, whole");
    assert_eq!(roles.handoff(Landing::default()), Ok(Some(id(2))));
}

// what this catches: a mind assembled from some of her parts (move-whole, refuse-partial).
#[test]
fn a_unit_missing_any_part_is_refused_whole() {
    let refused = Unit::assemble(id(9), Epoch(1), Some("kv"), Some(vec![1]), None, Some(()), Some(()));
    assert_eq!(
        refused,
        Err(crate::persona::PartialMove { persona: id(9), missing: "genome generation" })
    );
    let whole = Unit::assemble(id(9), Epoch(1), Some("kv"), Some(vec![1]), Some(3), Some(()), Some(()));
    assert!(whole.is_ok());
}

// what this catches: the same mind in two places at once (staged while active).
#[test]
fn a_mind_already_in_the_pipeline_is_not_staged_again() {
    let mut roles: Roles<Unit, Landing> = Roles::default();
    let a = Landing::default();
    roles.stage(mind(1), a.clone()).expect("test: stage");
    a.land();
    roles.handoff(Landing::default()).expect("test: bind");
    assert_eq!(roles.stage(mind(1), Landing::default()), Err(StageRefusal::AlreadyPresent { persona: id(1) }));
}
