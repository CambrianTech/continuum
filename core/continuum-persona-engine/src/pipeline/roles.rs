//! [`Roles`]: one Core's minds by place (CONTINUUM-ENGINE 4.3, 4.14). Several minds are **seated**: each
//! holds her own state in place in the Core's pool, and switching between seated minds moves nothing. One
//! of them is **active**, so one decodes at a time on this Core by construction. A mind moves only to be
//! seated or to leave:
//!
//! - **next** is a mind coming in, staged as data under her copy-in fence (her blocks paged back in, or a
//!   new mind claiming a sequence) while the active mind decodes;
//! - **seat** seats next once her fence lands; she may then be made active;
//! - **evict** takes a seated mind out (the pool needs her room): she awaits the copy-out the completion
//!   owner begins next, so no read of her blocks starts before she has stopped being seated;
//! - **begin_retire** attaches that copy-out; **release_retired** gives her back to her tier once it lands.
//!
//! Every refusal that receives owned values returns them ([`Refused`]): nothing is dropped.

use crate::persona::{Epoch, MindId, PersonaId, PersonaUnit};
use crate::residency::{Direction, Fence, FenceTag};

/// What the pipeline needs to know of a unit: which mind it is. Presence is by persona (one of her
/// at a time, whatever epoch); a move's fence is by the whole id.
pub trait Identified {
    fn mind(&self) -> MindId;
    fn persona(&self) -> PersonaId {
        self.mind().persona
    }
    fn epoch(&self) -> Epoch {
        self.mind().epoch
    }
}

impl<B, T, G, L, D> Identified for PersonaUnit<B, T, G, L, D> {
    fn mind(&self) -> MindId {
        MindId { persona: self.id(), epoch: PersonaUnit::epoch(self) }
    }
}

/// A refusal that hands back everything it was given.
#[derive(Debug)]
pub struct Refused<R, V> {
    pub reason: R,
    pub returned: V,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StageRefusal {
    #[error("{staged:?} is already staged next")]
    NextOccupied { staged: PersonaId },
    #[error("{persona:?} is already in this Core")]
    AlreadyPresent { persona: PersonaId },
    #[error("the fence moves {got:?}, not this unit's copy-in")]
    WrongFence { got: FenceTag },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SeatRefusal {
    #[error("nobody is staged")]
    NothingStaged,
    #[error("{staged:?} is staged but her copy-in has not landed")]
    NotReady { staged: PersonaId },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SeatedRefusal {
    #[error("{persona:?} is not seated")]
    NotSeated { persona: PersonaId },
    #[error("{retiring:?} is still retiring; her copy-out must land first")]
    RetireInFlight { retiring: PersonaId },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RetireRefusal {
    #[error("nobody is retiring")]
    NothingRetiring,
    #[error("{retiring:?}'s copy-out was already begun")]
    AlreadyMoving { retiring: PersonaId },
    #[error("{retiring:?} awaits a copy-out that has not begun")]
    NotBegun { retiring: PersonaId },
    #[error("{retiring:?} is still moving out")]
    NotLanded { retiring: PersonaId },
    #[error("the fence moves {got:?}, not this unit's copy-out")]
    WrongFence { got: FenceTag },
}

struct InFlight<U, F> {
    unit: U,
    fence: F,
}

enum Retiring<U, F> {
    /// Out of her seat; no read of her blocks has begun.
    AwaitingCopyOut(U),
    Moving(InFlight<U, F>),
}

impl<U, F> Retiring<U, F> {
    fn unit(&self) -> &U {
        match self {
            Self::AwaitingCopyOut(unit) => unit,
            Self::Moving(moving) => &moving.unit,
        }
    }
}

/// One Core's minds by place.
pub struct Roles<U, F> {
    seated: Vec<U>,
    active: Option<PersonaId>,
    next: Option<InFlight<U, F>>,
    retiring: Option<Retiring<U, F>>,
}

impl<U, F> Default for Roles<U, F> {
    fn default() -> Self {
        Self { seated: Vec::new(), active: None, next: None, retiring: None }
    }
}

fn expects<U: Identified>(unit: &U, direction: Direction) -> FenceTag {
    FenceTag { mind: unit.mind(), direction }
}

impl<U: Identified, F: Fence> Roles<U, F> {
    /// The mind decoding now.
    pub fn active(&self) -> Option<&U> {
        let active = self.active?;
        self.seated.iter().find(|unit| unit.persona() == active)
    }

    /// Mutable access to the active mind only: decoding touches nobody else.
    pub fn active_mut(&mut self) -> Option<&mut U> {
        let active = self.active?;
        self.seated.iter_mut().find(|unit| unit.persona() == active)
    }

    /// The minds seated in the pool, the active one among them.
    pub fn seated(&self) -> impl Iterator<Item = &U> {
        self.seated.iter()
    }

    pub fn is_seated(&self, persona: PersonaId) -> bool {
        self.seated.iter().any(|unit| unit.persona() == persona)
    }

    /// All units this Core owns, including those in flight.
    pub fn units(&self) -> impl Iterator<Item = &U> {
        self.seated.iter()
            .chain(self.next.iter().map(|next| &next.unit))
            .chain(self.retiring.iter().map(Retiring::unit))
    }

    pub fn staged(&self) -> Option<PersonaId> {
        self.next.as_ref().map(|n| n.unit.persona())
    }

    pub fn retiring(&self) -> Option<PersonaId> {
        self.retiring.as_ref().map(|role| role.unit().persona())
    }

    /// The exact copy-out to begin for the mind awaiting it (her persona, epoch, direction out): the
    /// descriptor the completion owner hands its tier, so no second lookup by id can name another
    /// generation. `None` when nobody awaits a copy-out.
    pub fn awaiting_copy_out(&self) -> Option<FenceTag> {
        match self.retiring.as_ref() {
            Some(Retiring::AwaitingCopyOut(unit)) => Some(expects(unit, Direction::Out)),
            _ => None,
        }
    }

    /// The copy-out to begin and the unit it moves: the engine hands her own blocks to the tier, so
    /// no tier holds a second copy of who she is (CONTINUUM-ENGINE 4.17).
    pub fn awaiting_copy_out_mut(&mut self) -> Option<(FenceTag, &mut U)> {
        match self.retiring.as_mut() {
            Some(Retiring::AwaitingCopyOut(unit)) => Some((expects(unit, Direction::Out), unit)),
            _ => None,
        }
    }

    fn present(&self, persona: PersonaId) -> bool {
        self.is_seated(persona) || self.staged() == Some(persona) || self.retiring() == Some(persona)
    }

    /// Whether `persona` may be staged now: next is free and she is not already here. The one check,
    /// asked before her copy-in begins (so a refusal never leaves a transfer running for nobody) and
    /// again when she is staged.
    pub fn admits(&self, persona: PersonaId) -> Option<StageRefusal> {
        if let Some(staged) = self.staged() {
            Some(StageRefusal::NextOccupied { staged })
        } else if self.present(persona) {
            Some(StageRefusal::AlreadyPresent { persona })
        } else {
            None
        }
    }

    /// Stage `unit` as next under `fence`, her copy-in. A refusal returns both.
    pub fn stage(&mut self, unit: U, fence: F) -> Result<(), Refused<StageRefusal, (U, F)>> {
        let reason = if let Some(refusal) = self.admits(unit.persona()) {
            Some(refusal)
        } else if fence.tag() != expects(&unit, Direction::In) {
            Some(StageRefusal::WrongFence { got: fence.tag() })
        } else {
            None
        };
        match reason {
            Some(reason) => Err(Refused { reason, returned: (unit, fence) }),
            None => {
                self.next = Some(InFlight { unit, fence });
                Ok(())
            }
        }
    }

    /// Seat the staged mind once her copy-in has landed. Returns who was seated.
    pub fn seat(&mut self) -> Result<PersonaId, SeatRefusal> {
        match self.next.as_ref() {
            None => return Err(SeatRefusal::NothingStaged),
            Some(next) if !next.fence.complete() => return Err(SeatRefusal::NotReady { staged: next.unit.persona() }),
            Some(_) => {}
        }
        let Some(next) = self.next.take() else {
            return Err(SeatRefusal::NothingStaged);
        };
        let seated = next.unit.persona();
        self.seated.push(next.unit);
        Ok(seated)
    }

    /// Make a seated mind active: nothing moves. Returns who was active before (`None` when nobody was).
    pub fn activate(&mut self, persona: PersonaId) -> Result<Option<PersonaId>, SeatedRefusal> {
        if !self.is_seated(persona) {
            return Err(SeatedRefusal::NotSeated { persona });
        }
        Ok(self.active.replace(persona))
    }

    /// Take a seated mind out of her seat (active or not: the active one stops being active). She awaits
    /// her copy-out. Refused while another retirement is in flight, so one copy-out runs at a time.
    pub fn evict(&mut self, persona: PersonaId) -> Result<(), SeatedRefusal> {
        if let Some(retiring) = self.retiring() {
            return Err(SeatedRefusal::RetireInFlight { retiring });
        }
        let at = self.seated.iter().position(|unit| unit.persona() == persona).ok_or(SeatedRefusal::NotSeated { persona })?;
        if self.active == Some(persona) {
            self.active = None;
        }
        self.retiring = Some(Retiring::AwaitingCopyOut(self.seated.swap_remove(at)));
        Ok(())
    }

    /// Attach the copy-out begun after an eviction. A refusal returns the fence.
    pub fn begin_retire(&mut self, fence: F) -> Result<(), Refused<RetireRefusal, F>> {
        let check = match self.retiring.as_ref() {
            None => Err(RetireRefusal::NothingRetiring),
            Some(Retiring::Moving(moving)) => Err(RetireRefusal::AlreadyMoving { retiring: moving.unit.persona() }),
            Some(Retiring::AwaitingCopyOut(unit)) if fence.tag() != expects(unit, Direction::Out) => {
                Err(RetireRefusal::WrongFence { got: fence.tag() })
            }
            Some(Retiring::AwaitingCopyOut(_)) => Ok(()),
        };
        if let Err(reason) = check {
            return Err(Refused { reason, returned: fence });
        }
        match self.retiring.take() {
            Some(Retiring::AwaitingCopyOut(unit)) => {
                self.retiring = Some(Retiring::Moving(InFlight { unit, fence }));
                Ok(())
            }
            other => {
                self.retiring = other;
                Err(Refused { reason: RetireRefusal::NothingRetiring, returned: fence })
            }
        }
    }

    /// Give the retired mind back to her tier once her copy-out has landed.
    pub fn release_retired(&mut self) -> Result<U, RetireRefusal> {
        match self.retiring.as_ref() {
            None => return Err(RetireRefusal::NothingRetiring),
            Some(Retiring::AwaitingCopyOut(unit)) => return Err(RetireRefusal::NotBegun { retiring: unit.persona() }),
            Some(Retiring::Moving(moving)) if !moving.fence.complete() => {
                return Err(RetireRefusal::NotLanded { retiring: moving.unit.persona() })
            }
            Some(Retiring::Moving(_)) => {}
        }
        match self.retiring.take() {
            Some(Retiring::Moving(done)) => Ok(done.unit),
            _ => Err(RetireRefusal::NothingRetiring),
        }
    }
}
