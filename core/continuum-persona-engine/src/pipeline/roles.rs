//! [`Roles`]: one Core's three places, one mind each. Exactly one mind is active, so one decodes
//! at a time on this Core by construction (the engine owns the list of Cores and the measured
//! admission of another, CONTINUUM-ENGINE 4.14).
//!
//! - **next** is staged as data under her copy-in fence while the active mind decodes;
//! - **handoff** binds next once her fence lands and leaves the previous mind *awaiting copy-out*:
//!   no read of her blocks starts before she has stopped being active;
//! - **begin_retire** attaches the copy-out the completion owner started after the bind;
//! - **release_retired** gives her back to her tier once that copy-out lands.
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
pub enum HandoffRefusal {
    #[error("nobody is staged")]
    NothingStaged,
    #[error("{staged:?} is staged but her copy-in has not landed")]
    NotReady { staged: PersonaId },
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

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BesideRefusal {
    #[error("{persona:?} is already in this Core")]
    AlreadyPresent { persona: PersonaId },
    #[error("{persona:?}'s copy-in has not landed")]
    NotLanded { persona: PersonaId },
    #[error("the fence moves {got:?}, not this unit's copy-in")]
    WrongFence { got: FenceTag },
    #[error("{persona:?} is not bound beside the active mind")]
    NotBeside { persona: PersonaId },
    #[error("nobody is active to bind beside")]
    NoneActive,
    #[error("{retiring:?} is still retiring; her copy-out must land first")]
    RetireInFlight { retiring: PersonaId },
}

struct InFlight<U, F> {
    unit: U,
    fence: F,
}

enum Retiring<U, F> {
    /// Bound out of active; no read of her blocks has begun.
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

/// One Core's places: the active mind, the next staged under her copy-in, the one retiring, and the minds bound beside
/// the active one in their own streams, who decode only inside a batch step with her (THE-LANE-ON-SAGE.md, open
/// question 1).
pub struct Roles<U, F> {
    active: Option<U>,
    next: Option<InFlight<U, F>>,
    retiring: Option<Retiring<U, F>>,
    beside: Vec<U>,
}

impl<U, F> Default for Roles<U, F> {
    fn default() -> Self {
        Self { active: None, next: None, retiring: None, beside: Vec::new() }
    }
}

fn expects<U: Identified>(unit: &U, direction: Direction) -> FenceTag {
    FenceTag { mind: unit.mind(), direction }
}

impl<U: Identified, F: Fence> Roles<U, F> {
    /// The mind decoding now.
    pub fn active(&self) -> Option<&U> {
        self.active.as_ref()
    }

    /// All units this Core owns, including those in flight.
    pub fn units(&self) -> impl Iterator<Item = &U> {
        self.active.iter()
            .chain(self.next.iter().map(|next| &next.unit))
            .chain(self.retiring.iter().map(Retiring::unit))
            .chain(self.beside.iter())
    }

    /// Mutable access to the active mind only: decoding touches nobody else.
    pub fn active_mut(&mut self) -> Option<&mut U> {
        self.active.as_mut()
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
        self.active.as_ref().is_some_and(|a| a.persona() == persona)
            || self.staged() == Some(persona)
            || self.retiring() == Some(persona)
            || self.beside.iter().any(|unit| unit.persona() == persona)
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

    /// Bind the staged mind as active once her copy-in has landed. The previous active mind (if
    /// any) moves to retiring, awaiting the copy-out the completion owner begins after this
    /// returns. Refused, changing nothing, until next has landed and the previous retirement is
    /// released.
    pub fn handoff(&mut self) -> Result<Option<PersonaId>, HandoffRefusal> {
        if let Some(retiring) = self.retiring() {
            return Err(HandoffRefusal::RetireInFlight { retiring });
        }
        match self.next.as_ref() {
            None => return Err(HandoffRefusal::NothingStaged),
            Some(next) if !next.fence.complete() => {
                return Err(HandoffRefusal::NotReady { staged: next.unit.persona() })
            }
            Some(_) => {}
        }
        let Some(next) = self.next.take() else {
            return Err(HandoffRefusal::NothingStaged);
        };
        let previous = self.active.replace(next.unit);
        let retired = previous.as_ref().map(Identified::persona);
        self.retiring = previous.map(Retiring::AwaitingCopyOut);
        Ok(retired)
    }

    /// Retire the active mind with nobody to follow her (a stop's suspend, CONTINUUM-ENGINE 4.16):
    /// she awaits her copy-out as after a handoff. Returns who retired (`None` when nobody was
    /// active). Refused while another retirement is in flight.
    pub fn retire(&mut self) -> Result<Option<PersonaId>, HandoffRefusal> {
        if let Some(retiring) = self.retiring() {
            return Err(HandoffRefusal::RetireInFlight { retiring });
        }
        let previous = self.active.take();
        let retired = previous.as_ref().map(Identified::persona);
        self.retiring = previous.map(Retiring::AwaitingCopyOut);
        Ok(retired)
    }

    /// Attach the copy-out begun after the handoff. A refusal returns the fence.
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

    /// The minds bound beside the active one, each in her own stream.
    pub fn beside(&self) -> impl Iterator<Item = &U> {
        self.beside.iter()
    }

    /// A bound mind's own state: the active one or one beside her (a batch step records each of theirs).
    pub fn bound_mut(&mut self, persona: PersonaId) -> Option<&mut U> {
        match self.active.as_mut() {
            Some(active) if active.persona() == persona => Some(active),
            _ => self.beside.iter_mut().find(|unit| unit.persona() == persona),
        }
    }

    /// Bind `unit` beside the active mind under `fence`, her copy-in, once it has landed: she decodes only inside a
    /// batch step with the active mind. A refusal returns both.
    pub fn bind_beside(&mut self, unit: U, fence: F) -> Result<(), Refused<BesideRefusal, (U, F)>> {
        let reason = if self.active.is_none() {
            Some(BesideRefusal::NoneActive)
        } else if self.present(unit.persona()) {
            Some(BesideRefusal::AlreadyPresent { persona: unit.persona() })
        } else if fence.tag() != expects(&unit, Direction::In) {
            Some(BesideRefusal::WrongFence { got: fence.tag() })
        } else if !fence.complete() {
            Some(BesideRefusal::NotLanded { persona: unit.persona() })
        } else {
            None
        };
        match reason {
            Some(reason) => Err(Refused { reason, returned: (unit, fence) }),
            None => {
                self.beside.push(unit);
                Ok(())
            }
        }
    }

    /// Make `persona`, bound beside, the active mind; the active one moves beside her. No transfer: both stay bound.
    pub fn swap_active(&mut self, persona: PersonaId) -> Result<(), BesideRefusal> {
        let at = self.beside.iter().position(|unit| unit.persona() == persona).ok_or(BesideRefusal::NotBeside { persona })?;
        let Some(active) = self.active.take() else { return Err(BesideRefusal::NoneActive) };
        let next = std::mem::replace(&mut self.beside[at], active);
        self.active = Some(next);
        Ok(())
    }

    /// Unbind `persona` from beside the active mind: she awaits her copy-out as a retired mind does, through the one
    /// copy-out path. Refused while another retirement is in flight.
    pub fn unbind_beside(&mut self, persona: PersonaId) -> Result<(), BesideRefusal> {
        if let Some(retiring) = self.retiring() {
            return Err(BesideRefusal::RetireInFlight { retiring });
        }
        let at = self.beside.iter().position(|unit| unit.persona() == persona).ok_or(BesideRefusal::NotBeside { persona })?;
        self.retiring = Some(Retiring::AwaitingCopyOut(self.beside.swap_remove(at)));
        Ok(())
    }
}
