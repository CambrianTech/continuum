//! [`Roles`]: three places, one mind each. Exactly one mind is active, so one decodes at a time
//! by construction: there is no count to configure and no way to hold a second active mind.
//!
//! - **next** is staged as data while the active mind decodes (a transfer, never a decode);
//! - **handoff** binds next as active only once her stage fence completes, and moves the
//!   previous active to retiring under the retire fence it is given;
//! - **retiring** is released to its tier only after that fence completes.

use crate::persona::{PersonaId, PersonaUnit};
use crate::residency::Fence;

/// What the pipeline needs to know of a unit: whose it is.
pub trait Identified {
    fn persona(&self) -> PersonaId;
}

impl<B, T, G, L, D> Identified for PersonaUnit<B, T, G, L, D> {
    fn persona(&self) -> PersonaId {
        self.id()
    }
}

/// Staging refused: the next place is taken (one staged mind at a time), or she is already here.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StageRefusal {
    #[error("{staged:?} is already staged next")]
    NextOccupied { staged: PersonaId },
    #[error("{persona:?} is already in the pipeline")]
    AlreadyPresent { persona: PersonaId },
}

/// Handoff refused, with the reason the active mind keeps decoding.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HandoffRefusal {
    #[error("nobody is staged")]
    NothingStaged,
    #[error("{staged:?} is staged but her transfer has not completed")]
    NotReady { staged: PersonaId },
    #[error("{retiring:?} is still retiring; the previous move must land first")]
    RetireInFlight { retiring: PersonaId },
}

/// Release refused: nobody retiring, or her move out has not landed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RetireRefusal {
    #[error("nobody is retiring")]
    NothingRetiring,
    #[error("{retiring:?} is still moving out")]
    NotLanded { retiring: PersonaId },
}

struct InFlight<U, F> {
    unit: U,
    fence: F,
}

/// The pipeline's three places.
pub struct Roles<U, F> {
    active: Option<U>,
    next: Option<InFlight<U, F>>,
    retiring: Option<InFlight<U, F>>,
}

impl<U, F> Default for Roles<U, F> {
    fn default() -> Self {
        Self { active: None, next: None, retiring: None }
    }
}

impl<U: Identified, F: Fence> Roles<U, F> {
    /// The mind decoding now.
    pub fn active(&self) -> Option<&U> {
        self.active.as_ref()
    }

    /// Mutable access to the active mind only: decoding touches nobody else.
    pub fn active_mut(&mut self) -> Option<&mut U> {
        self.active.as_mut()
    }

    pub fn staged(&self) -> Option<PersonaId> {
        self.next.as_ref().map(|n| n.unit.persona())
    }

    pub fn retiring(&self) -> Option<PersonaId> {
        self.retiring.as_ref().map(|r| r.unit.persona())
    }

    fn present(&self, persona: PersonaId) -> bool {
        self.active.as_ref().is_some_and(|a| a.persona() == persona)
            || self.staged() == Some(persona)
            || self.retiring() == Some(persona)
    }

    /// Stage `unit` as next while the active mind decodes; `fence` is her copy-in.
    pub fn stage(&mut self, unit: U, fence: F) -> Result<(), StageRefusal> {
        if let Some(staged) = self.staged() {
            return Err(StageRefusal::NextOccupied { staged });
        }
        if self.present(unit.persona()) {
            return Err(StageRefusal::AlreadyPresent { persona: unit.persona() });
        }
        self.next = Some(InFlight { unit, fence });
        Ok(())
    }

    /// Bind the staged mind as active. The previous active (if any) moves to retiring under
    /// `retire_fence`, her copy-out. Refused, changing nothing, until next has landed and the
    /// previous retirement has been released.
    pub fn handoff(&mut self, retire_fence: F) -> Result<Option<PersonaId>, HandoffRefusal> {
        let Some(next) = self.next.as_ref() else {
            return Err(HandoffRefusal::NothingStaged);
        };
        if !next.fence.complete() {
            return Err(HandoffRefusal::NotReady { staged: next.unit.persona() });
        }
        if let Some(retiring) = self.retiring() {
            if self.active.is_some() {
                return Err(HandoffRefusal::RetireInFlight { retiring });
            }
        }
        let Some(next) = self.next.take() else {
            return Err(HandoffRefusal::NothingStaged);
        };
        let previous = self.active.replace(next.unit);
        let retired = previous.as_ref().map(Identified::persona);
        if let Some(unit) = previous {
            self.retiring = Some(InFlight { unit, fence: retire_fence });
        }
        Ok(retired)
    }

    /// Give the retired mind back to her tier once her move out has landed.
    pub fn release_retired(&mut self) -> Result<U, RetireRefusal> {
        let Some(retiring) = self.retiring.as_ref() else {
            return Err(RetireRefusal::NothingRetiring);
        };
        if !retiring.fence.complete() {
            return Err(RetireRefusal::NotLanded { retiring: retiring.unit.persona() });
        }
        match self.retiring.take() {
            Some(done) => Ok(done.unit),
            None => Err(RetireRefusal::NothingRetiring),
        }
    }
}
