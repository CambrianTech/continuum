//! Seats' contract: a switch to a seated mind moves nothing; a mind not seated comes in and the coldest
//! seated mind leaves only when there is no room, the active one included at a switch and never during
//! her own decode; and no refusal on any path loses a mind.

use super::*;
use crate::persona::{Epoch, MindId, PersonaParts, PersonaUnit};
use crate::residency::{Completion, Direction, Fence, FenceTag, Native, Tier};
use std::cell::Cell;
use std::rc::Rc;

/// A mind whose history the test checks survives every move.
type Unit = PersonaUnit<(), Vec<u32>, (), (), ()>;

fn id(n: u128) -> PersonaId {
    PersonaId(uuid::Uuid::from_u128(n))
}

fn mind(n: u128) -> Unit {
    PersonaUnit::new(id(n), Epoch(1), PersonaParts { blocks: (), tokens: vec![n as u32; 3], genome: (), learning: (), decode: () })
}

/// Every move and decode lands at once.
struct Landed(FenceTag);
impl Fence for Landed {
    fn tag(&self) -> FenceTag {
        self.0
    }
    fn complete(&self) -> bool {
        true
    }
}
impl Completion for Landed {
    fn complete(&self) -> bool {
        true
    }
}

/// A pool of `seats` sequences: a move in past them is refused for room; copy-outs refused on request.
struct Pool {
    seats: usize,
    bound: Rc<Cell<usize>>,
    refuse_out: Rc<Cell<bool>>,
    moves: Rc<Cell<usize>>,
}
impl Tier for Pool {
    type Fence = Landed;
    type Blocks = ();
    fn name(&self) -> &'static str {
        "pool"
    }
    fn blocks_for(&mut self, _mind: MindId) -> Result<(), TierRefusal> {
        Ok(())
    }
    fn begin(&mut self, mind: MindId, (): &mut (), direction: Direction) -> Result<Landed, TierRefusal> {
        let refused = |native| TierRefusal { tier: "pool", mind, reason: "refused by the test", native };
        match direction {
            Direction::In if self.bound.get() == self.seats => return Err(refused(Native::NoRoom)),
            Direction::In => self.bound.set(self.bound.get() + 1),
            Direction::Out if self.refuse_out.get() => return Err(refused(Native::NotAsked)),
            Direction::Out => self.bound.set(self.bound.get() - 1),
        }
        self.moves.set(self.moves.get() + 1);
        Ok(Landed(FenceTag { mind, direction }))
    }
}
impl Execute for Pool {
    type Work = ();
    type Done = Landed;
    fn submit(&mut self, mind: MindId, (): &()) -> Result<Landed, TierRefusal> {
        Ok(Landed(FenceTag { mind, direction: Direction::In }))
    }
    fn settle(&mut self) -> Result<(), TierRefusal> {
        Ok(())
    }
    type Checkpoint = ();
    fn checkpoint(&mut self, _mind: MindId, (): &mut ()) -> Result<u32, TierRefusal> {
        Ok(0)
    }
    fn rewind(&mut self, _mind: MindId, (): &()) -> Result<(), TierRefusal> {
        Ok(())
    }
}

struct Core {
    engine: PersonaEngine<Unit, Pool>,
    seats: Seats<Unit>,
    refuse_out: Rc<Cell<bool>>,
    moves: Rc<Cell<usize>>,
}

fn core(seats: usize) -> Core {
    let (refuse_out, moves) = (Rc::new(Cell::new(false)), Rc::new(Cell::new(0)));
    let pool = Pool { seats, bound: Rc::new(Cell::new(0)), refuse_out: Rc::clone(&refuse_out), moves: Rc::clone(&moves) };
    Core { engine: PersonaEngine::new(pool), seats: Seats::new(HashMap::new()), refuse_out, moves }
}

/// Keeps `only` (every mind when `None`) once she leaves her seat; starts a mind with her own history.
struct Keeps {
    only: Option<PersonaId>,
}

const KEEPS_ALL: Keeps = Keeps { only: None };

impl Owner<Unit, Pool> for Keeps {
    fn keeps(&self, unit: &Unit) -> bool {
        self.only.map_or(true, |only| unit.id() == only)
    }
    fn start(&self, _engine: &mut PersonaEngine<Unit, Pool>, id: PersonaId) -> Result<Unit, TierRefusal> {
        Ok(mind(id.0.as_u128()))
    }
}

impl Core {
    fn switch(&mut self, n: u128) -> Result<Vec<PersonaId>, SeatsError> {
        self.seats.switch_to(&mut self.engine, id(n), &KEEPS_ALL)
    }

    /// Her history, wherever she is.
    fn history(&self, n: u128) -> Option<&[u32]> {
        self.engine.units().chain(self.seats.parked().values()).find(|unit| unit.id() == id(n)).map(|unit| unit.parts().tokens.as_slice())
    }
}

// what this catches (Codex on #6258 at 42a1c5de7e): with one seat, a switch could never make room,
// because the active mind was never a victim, so A -> B refused forever. At a switch the active mind
// leaves like any other once her decode landed, and A -> B -> A brings each back holding what she held.
#[test]
fn one_seat_switches_by_paging_the_active_mind_out_and_back() {
    let mut c = core(1);
    assert_eq!(c.switch(1), Ok(vec![]));
    assert_eq!(c.switch(2), Ok(vec![id(1)]), "the active mind leaves for the one coming in");
    assert_eq!((c.engine.active(), c.seats.parked().contains_key(&id(1))), (Some(id(2)), true));
    assert_eq!(c.switch(1), Ok(vec![id(2)]));
    assert_eq!(c.engine.active(), Some(id(1)));
    assert_eq!((c.history(1), c.history(2)), (Some(&[1u32; 3][..]), Some(&[2u32; 3][..])), "each holds what she held");
}

// what this catches: every switch moving minds (63-259 ms per switch on the M5, 2026-11-24). Between
// seated minds a switch begins no move at all.
#[test]
fn a_switch_between_seated_minds_moves_nothing() {
    let mut c = core(2);
    assert_eq!(c.switch(1), Ok(vec![]));
    assert_eq!(c.switch(2), Ok(vec![]));
    let moves = c.moves.get();
    for n in [1, 2, 1, 2] {
        assert_eq!(c.switch(n), Ok(vec![]));
    }
    assert_eq!(c.moves.get(), moves, "no copy-in or copy-out");
}

// what this catches: a page-out that takes a mind still in use while an older one sits idle. The
// seated mind used least recently is the one that leaves.
#[test]
fn the_seated_mind_used_least_recently_leaves() {
    let mut c = core(2);
    for n in [1, 2, 1] {
        assert_eq!(c.switch(n), Ok(vec![]));
    }
    assert_eq!(c.switch(3), Ok(vec![id(2)]), "2 is the coldest");
    assert!(c.engine.is_seated(id(1)) && c.engine.is_seated(id(3)));
}

// what this catches: room made for a step by evicting the mind decoding it. During her own decode the
// active mind never leaves; with nobody else seated there is no room, said as None.
#[test]
fn room_during_a_decode_never_takes_the_active_mind() {
    let mut c = core(2);
    assert_eq!(c.switch(1), Ok(vec![]));
    assert_eq!(c.seats.make_room_during(&mut c.engine, &KEEPS_ALL), Ok(None), "she is the only one seated");
    assert_eq!(c.switch(2), Ok(vec![]));
    assert_eq!(c.seats.make_room_during(&mut c.engine, &KEEPS_ALL), Ok(Some(id(1))), "the other seated mind leaves");
    assert_eq!(c.engine.active(), Some(id(2)));
}

// what this catches (Codex on #6258 at 42a1c5de7e): the mind coming in dropped when the room-making
// refused (her victim's copy-out refused). She is parked again, the victim stays in custody, and once
// the copy-out can begin the same switch completes with both holding what they held.
#[test]
fn a_refused_copy_out_parks_the_mind_coming_in_and_loses_nobody() {
    let mut c = core(1);
    assert_eq!(c.switch(1), Ok(vec![]));
    c.seats.parked.insert(id(2), mind(2));
    c.refuse_out.set(true);
    assert!(c.switch(2).is_err(), "no room while her copy-out is refused");
    assert!(c.seats.parked().contains_key(&id(2)), "the mind coming in is parked again");
    assert_eq!(c.engine.retiring(), Some(id(1)), "the victim stays in the engine's custody");
    c.refuse_out.set(false);
    assert_eq!(c.switch(2), Ok(vec![]), "her retirement finishes first, and the seat is free");
    assert_eq!(c.engine.active(), Some(id(2)));
    assert_eq!((c.history(1), c.history(2)), (Some(&[1u32; 3][..]), Some(&[2u32; 3][..])));
}

// what this catches: a one-off kept forever after her turn. A mind her owner no longer keeps is dropped
// as she leaves her seat, never parked.
#[test]
fn a_mind_her_owner_no_longer_keeps_is_dropped_as_she_leaves() {
    let mut c = core(1);
    assert_eq!(c.switch(1), Ok(vec![]));
    let only_two = Keeps { only: Some(id(2)) };
    assert_eq!(c.seats.switch_to(&mut c.engine, id(2), &only_two), Ok(vec![id(1)]));
    assert_eq!(c.history(1), None, "dropped, blocks and all");
}
