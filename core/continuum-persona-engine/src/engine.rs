//! [`PersonaEngine`]: the one owner of an engine's decode (CONTINUUM-ENGINE 4.14; MODULE-ARCHITECTURE
//! 9.4: "a shared resource is a dependency too: the engine's decode is one resource, so the minds'
//! decodes are ordered children of the engine, serialized by that edge").
//!
//! The edge is the engine itself. [`PersonaEngine::submit`] hands the tier the active mind's work and
//! nothing else: the tier decodes on the stream she is bound to, so a step cannot name another
//! sequence or move a mind. Its completion is held, and [`handoff`](PersonaEngine::handoff) is refused
//! until it has landed, so a decode still running on the device is never cut off by a switch (Codex
//! on #6139). Only one decode is in flight at a time; no count, lane or launch parameter exists to ask
//! for more. Staging the next mind and retiring the previous one are transfers through the same tier,
//! fenced, and proceed while a decode runs.
//!
//! - [`submit`](PersonaEngine::submit): decode a step of the active mind, after the previous step landed;
//! - [`stage`](PersonaEngine::stage): begin her copy-in, and hold her as next under its fence;
//! - [`handoff`](PersonaEngine::handoff): once the active mind's decode and the next mind's copy-in have
//!   landed, bind next, then begin the previous mind's copy-out;
//! - [`release_retired`](PersonaEngine::release_retired): give the previous mind back once her copy-out
//!   landed.
//!
//! Every refusal that receives a unit returns it: custody never ends in a refusal.
//!
//! The active mind's own state ([`active_mut`](PersonaEngine::active_mut)) cannot be held across a
//! submit:
//!
//! ```compile_fail,E0499
//! use continuum_persona_engine::engine::PersonaEngine;
//! use continuum_persona_engine::pipeline::Identified;
//! use continuum_persona_engine::residency::Execute;
//!
//! fn step_while_holding_her<U: Identified, T: Execute>(engine: &mut PersonaEngine<U, T>, work: T::Work) {
//!     let her = engine.active_mut();
//!     let _ = engine.submit(work);
//!     drop(her);
//! }
//! ```

#[cfg(test)]
mod tests;

use crate::persona::PersonaId;
use crate::pipeline::{HandoffRefusal, Identified, Refused, RetireRefusal, Roles, StageRefusal};
use crate::residency::{Completion, Direction, Execute, FenceTag, Native, TierRefusal};

/// Why a mind was not staged. Either way she is handed back.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StageError {
    #[error(transparent)]
    Pipeline(#[from] StageRefusal),
    #[error(transparent)]
    Tier(#[from] TierRefusal),
}

/// Why a decode step was not submitted. Nothing was submitted.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DecodeError {
    #[error("no mind is active")]
    NoneActive,
    /// One decode is in flight at a time: the previous step has not landed.
    #[error("{active:?}'s previous decode has not landed")]
    InFlight { active: PersonaId },
    #[error(transparent)]
    Tier(#[from] TierRefusal),
}

/// Why a handoff did not complete.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HandoffError {
    /// Nothing changed: the active mind's decode is still on the device.
    #[error("{active:?}'s decode has not landed")]
    DecodeInFlight { active: PersonaId },
    /// Nothing changed: next is not ready, or the previous retirement is still in flight.
    #[error(transparent)]
    Pipeline(#[from] HandoffRefusal),
    /// The bind happened (next is active); the previous mind's copy-out could not begin. She stays
    /// retiring, awaiting a copy-out, in the engine's custody: [`PersonaEngine::retry_retire`].
    #[error("{previous:?} is bound out but her copy-out did not begin: {refusal}")]
    CopyOut { previous: PersonaId, refusal: TierRefusal },
}

/// Why a retirement did not finish. She stays retiring, in the engine's custody.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RetireError {
    /// Her copy-out could not begin.
    #[error(transparent)]
    CopyOut(#[from] HandoffError),
    /// Her copy-out has not landed, or the pipeline refused the release.
    #[error(transparent)]
    Release(#[from] RetireRefusal),
}

/// One engine's decode, its three places, and the tier that moves minds between them and runs them.
pub struct PersonaEngine<U, T: Execute> {
    roles: Roles<U, T::Fence>,
    tier: T,
    /// The active mind's last submitted step, until a handoff after it landed.
    decoding: Option<T::Done>,
}

impl<U: Identified, T: Execute> PersonaEngine<U, T> {
    pub fn new(tier: T) -> Self {
        Self { roles: Roles::default(), tier, decoding: None }
    }

    /// The mind decoding now.
    pub fn active(&self) -> Option<PersonaId> {
        self.roles.active().map(Identified::persona)
    }

    /// The active mind's own state (her tokens, her position), never the tier she runs on.
    pub fn active_mut(&mut self) -> Option<&mut U> {
        self.roles.active_mut()
    }

    pub fn staged(&self) -> Option<PersonaId> {
        self.roles.staged()
    }

    pub fn retiring(&self) -> Option<PersonaId> {
        self.roles.retiring()
    }

    /// The tier, for what it reports (her logits, her position); moves and decodes go through the
    /// engine.
    pub fn tier(&self) -> &T {
        &self.tier
    }

    /// Whether the active mind's last submitted step has landed (`true` when none was submitted).
    pub fn decode_landed(&self) -> bool {
        self.decoding.as_ref().map_or(true, Completion::complete)
    }

    /// Wait until the active mind's submitted steps have finished on the device (the tier's own
    /// attestation, never an assumption that a returned submit has landed).
    pub fn settle(&mut self) -> Result<(), TierRefusal> {
        self.tier.settle()
    }

    /// Submit one step of the active mind's decode. Refused while her previous step is still on the
    /// device: one decode is in flight at a time.
    pub fn submit(&mut self, work: T::Work) -> Result<(), DecodeError> {
        let (persona, epoch) = match self.roles.active() {
            Some(unit) => (unit.persona(), unit.epoch()),
            None => return Err(DecodeError::NoneActive),
        };
        if !self.decode_landed() {
            return Err(DecodeError::InFlight { active: persona });
        }
        self.decoding = Some(self.tier.submit(persona, epoch, work)?);
        Ok(())
    }

    /// Checkpoint the active mind where she stands (CONTINUUM-ENGINE 4.15); her last position it
    /// holds through. Waits for her submitted step to land first, so it reads what she decoded.
    pub fn checkpoint_active(&mut self, into: &mut T::Checkpoint) -> Result<u32, DecodeError> {
        let (persona, epoch) = self.active_identity()?;
        self.tier.settle()?;
        self.decoding = None;
        Ok(self.tier.checkpoint(persona, epoch, into)?)
    }

    /// Put the active mind back at `from`'s position: what she holds past it is cut. A refusal
    /// leaves her as she was.
    pub fn rewind_active(&mut self, from: &T::Checkpoint) -> Result<(), DecodeError> {
        let (persona, epoch) = self.active_identity()?;
        self.tier.settle()?;
        self.decoding = None;
        Ok(self.tier.rewind(persona, epoch, from)?)
    }

    fn active_identity(&self) -> Result<(PersonaId, crate::persona::Epoch), DecodeError> {
        self.roles.active().map(|unit| (unit.persona(), unit.epoch())).ok_or(DecodeError::NoneActive)
    }

    /// Begin `unit`'s copy-in and hold her as next. Refused before any transfer starts when next is
    /// occupied or she is already here, so a refusal never leaves a copy-in running for nobody. Runs
    /// while the active mind decodes.
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

    /// Bind next once the active mind's decode and next's copy-in have landed, then begin the
    /// previous mind's copy-out. Returns who retired (`None` when nobody was active). Changes nothing
    /// until both have landed and the previous retirement is released.
    pub fn handoff(&mut self) -> Result<Option<PersonaId>, HandoffError> {
        if !self.decode_landed() {
            if let Some(active) = self.active() {
                return Err(HandoffError::DecodeInFlight { active });
            }
        }
        let previous = self.roles.handoff()?;
        self.decoding = None;
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
            refusal: TierRefusal {
                tier: self.tier.name(),
                persona: tag.persona,
                reason: refusal_reason(&refused.reason),
                native: Native::NotAsked,
            },
        })
    }

    /// Give the previous mind back once her copy-out has landed.
    pub fn release_retired(&mut self) -> Result<U, RetireRefusal> {
        self.roles.release_retired()
    }

    /// Bring a retirement to its end: begin her copy-out if a refusal left it unbegun, then give her
    /// back once it landed. `Ok(None)` when nobody is retiring. A driver calls this before it stages,
    /// so a refused copy-out is retried rather than skipped, and the retiring mind is never handed
    /// out twice (Codex on #6140).
    pub fn finish_retirement(&mut self) -> Result<Option<U>, RetireError> {
        if self.retiring().is_none() {
            return Ok(None);
        }
        self.retry_retire()?;
        Ok(Some(self.release_retired()?))
    }

    /// Give up the blocks of a mind that has left the pipeline (a one-off whose turn ended). Refused
    /// while she is active, staged or retiring: custody of a present mind is never dropped.
    pub fn forget(&mut self, persona: PersonaId) -> Result<(), TierRefusal> {
        if self.present(persona) || self.staged() == Some(persona) {
            return Err(TierRefusal { tier: self.tier.name(), persona, reason: "she is still in the pipeline", native: Native::NotAsked });
        }
        self.tier.release(persona)
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
