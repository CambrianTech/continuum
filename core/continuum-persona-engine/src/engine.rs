//! [`PersonaEngine`]: the one owner of an engine's decode (CONTINUUM-ENGINE 4.14; MODULE-ARCHITECTURE
//! 9.4: "a shared resource is a dependency too: the engine's decode is one resource, so the minds'
//! decodes are ordered children of the engine, serialized by that edge").
//!
//! The edge is the borrow. [`PersonaEngine::decode`] takes `&mut self` and hands out only the active
//! mind, so two decodes through one engine cannot overlap: the compiler refuses it, and no count, lane
//! or launch parameter exists to ask for it. Staging the next mind and retiring the previous one are
//! transfers through the engine's [`Tier`], fenced, never decodes.
//!
//! - [`stage`](PersonaEngine::stage): begin her copy-in, and hold her as next under its fence;
//! - [`handoff`](PersonaEngine::handoff): bind next once her copy-in landed, then begin the previous
//!   mind's copy-out (only after she stopped being active);
//! - [`release_retired`](PersonaEngine::release_retired): give the previous mind back once her copy-out
//!   landed.
//!
//! Every refusal that receives a unit returns it: custody never ends in a refusal.
//!
//! A second decode while one runs does not compile:
//!
//! ```compile_fail,E0499
//! use continuum_persona_engine::engine::PersonaEngine;
//! use continuum_persona_engine::pipeline::Identified;
//! use continuum_persona_engine::residency::Tier;
//!
//! fn two_decodes_at_once<U: Identified, T: Tier>(engine: &mut PersonaEngine<U, T>) {
//!     engine.decode(|_, _| engine.decode(|_, _| ()));
//! }
//! ```

#[cfg(test)]
mod tests;

use crate::persona::PersonaId;
use crate::pipeline::{HandoffRefusal, Identified, Refused, RetireRefusal, Roles, StageRefusal};
use crate::residency::{Direction, FenceTag, Tier, TierRefusal};

/// Why a mind was not staged. Either way she is handed back.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StageError {
    #[error(transparent)]
    Pipeline(#[from] StageRefusal),
    #[error(transparent)]
    Tier(#[from] TierRefusal),
}

/// Why a handoff did not complete.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HandoffError {
    /// Nothing changed: next is not ready, or the previous retirement is still in flight.
    #[error(transparent)]
    Pipeline(#[from] HandoffRefusal),
    /// The bind happened (next is active); the previous mind's copy-out could not begin. She stays
    /// retiring, awaiting a copy-out, in the engine's custody: [`PersonaEngine::retry_retire`].
    #[error("{previous:?} is bound out but her copy-out did not begin: {refusal}")]
    CopyOut { previous: PersonaId, refusal: TierRefusal },
}

/// One engine's decode, its three places, and the tier that moves minds between them.
pub struct PersonaEngine<U, T: Tier> {
    roles: Roles<U, T::Fence>,
    tier: T,
}

impl<U: Identified, T: Tier> PersonaEngine<U, T> {
    pub fn new(tier: T) -> Self {
        Self { roles: Roles::default(), tier }
    }

    /// The mind decoding now.
    pub fn active(&self) -> Option<PersonaId> {
        self.roles.active().map(Identified::persona)
    }

    pub fn staged(&self) -> Option<PersonaId> {
        self.roles.staged()
    }

    pub fn retiring(&self) -> Option<PersonaId> {
        self.roles.retiring()
    }

    /// The engine's one decode. `step` gets the active mind and the tier her blocks are bound in; it
    /// runs only with `&mut self`, so no other decode, stage or handoff runs beside it. `None` when no
    /// mind is active.
    pub fn decode<R>(&mut self, step: impl FnOnce(&mut U, &mut T) -> R) -> Option<R> {
        let unit = self.roles.active_mut()?;
        Some(step(unit, &mut self.tier))
    }

    /// Begin `unit`'s copy-in and hold her as next. Refused before any transfer starts when next is
    /// occupied or she is already here, so a refusal never leaves a copy-in running for nobody.
    pub fn stage(&mut self, unit: U) -> Result<(), Refused<StageError, U>> {
        let persona = unit.persona();
        let occupied = match self.roles.staged() {
            Some(staged) => Some(StageRefusal::NextOccupied { staged }),
            None if self.present(persona) => Some(StageRefusal::AlreadyPresent { persona }),
            None => None,
        };
        if let Some(reason) = occupied {
            return Err(Refused { reason: reason.into(), returned: unit });
        }
        let fence = match self.tier.begin(FenceTag { persona, epoch: unit.epoch(), direction: Direction::In }) {
            Ok(fence) => fence,
            Err(refusal) => return Err(Refused { reason: refusal.into(), returned: unit }),
        };
        self.roles
            .stage(unit, fence)
            .map_err(|refused| Refused { reason: refused.reason.into(), returned: refused.returned.0 })
    }

    /// Bind next once her copy-in has landed, then begin the previous mind's copy-out. Returns who
    /// retired (`None` when nobody was active). Changes nothing until next has landed and the
    /// previous retirement is released.
    pub fn handoff(&mut self) -> Result<Option<PersonaId>, HandoffError> {
        let previous = self.roles.handoff()?;
        if previous.is_some() {
            self.retry_retire()?;
        }
        Ok(previous)
    }

    /// Begin the copy-out of the mind awaiting it (after a handoff whose copy-out was refused). Does
    /// nothing when no copy-out awaits.
    pub fn retry_retire(&mut self) -> Result<(), HandoffError> {
        let Some(tag) = self.roles.awaiting_copy_out() else { return Ok(()) };
        let fence = self
            .tier
            .begin(tag)
            .map_err(|refusal| HandoffError::CopyOut { previous: tag.persona, refusal })?;
        // The tag came from the pipeline itself, so the fence it carries is the one it expects.
        self.roles.begin_retire(fence).map_err(|refused| HandoffError::CopyOut {
            previous: tag.persona,
            refusal: TierRefusal { tier: self.tier.name(), persona: tag.persona, reason: refusal_reason(&refused.reason) },
        })
    }

    /// Give the previous mind back once her copy-out has landed.
    pub fn release_retired(&mut self) -> Result<U, RetireRefusal> {
        self.roles.release_retired()
    }

    fn present(&self, persona: PersonaId) -> bool {
        self.active() == Some(persona) || self.retiring() == Some(persona)
    }
}

fn refusal_reason(refusal: &RetireRefusal) -> &'static str {
    match refusal {
        RetireRefusal::NothingRetiring => "nobody is retiring",
        RetireRefusal::AlreadyMoving { .. } => "her copy-out was already begun",
        RetireRefusal::NotBegun { .. } => "her copy-out has not begun",
        RetireRefusal::NotLanded { .. } => "her copy-out has not landed",
        RetireRefusal::WrongFence { .. } => "the tier returned another unit's fence",
    }
}
