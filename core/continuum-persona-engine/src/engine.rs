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

use crate::persona::{MindId, PersonaId};
use crate::pipeline::{HandoffRefusal, Identified, Refused, RetireRefusal, Roles, StageRefusal};
use crate::residency::{Completion, Direction, Execute, Native, TierRefusal};
use crate::persona::Carries;

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

impl<U, T> PersonaEngine<U, T>
where
    U: Identified + Carries<T::Blocks>,
    T: Execute,
{
    pub fn new(tier: T) -> Self {
        Self { roles: Roles::default(), tier, decoding: None }
    }

    /// The mind decoding now.
    pub fn active(&self) -> Option<PersonaId> {
        self.roles.active().map(Identified::persona)
    }

    /// The units held across the active, staged and retiring roles.
    pub fn units(&self) -> impl Iterator<Item = &U> {
        self.roles.units()
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

    /// New blocks for `mind`, for the unit that will carry them (the tier allocates them; the engine
    /// never hands the tier itself out, so nothing moves a mind behind the pipeline).
    pub fn blocks_for(&mut self, mind: MindId) -> Result<T::Blocks, TierRefusal> {
        self.tier.blocks_for(mind)
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
        let mind = self.active_mind()?;
        if !self.decode_landed() {
            return Err(DecodeError::InFlight { active: mind.persona });
        }
        self.decoding = Some(self.tier.submit(mind, work)?);
        Ok(())
    }

    /// Checkpoint the active mind where she stands (CONTINUUM-ENGINE 4.15); her last position it
    /// holds through. Waits for her submitted step to land first, so it reads what she decoded.
    pub fn checkpoint_active(&mut self, into: &mut T::Checkpoint) -> Result<u32, DecodeError> {
        let mind = self.active_mind()?;
        self.tier.settle()?;
        self.decoding = None;
        Ok(self.tier.checkpoint(mind, into)?)
    }

    /// Put the active mind back at `from`'s position: what she holds past it is cut. A refusal
    /// leaves her as she was.
    pub fn rewind_active(&mut self, from: &T::Checkpoint) -> Result<(), DecodeError> {
        let mind = self.active_mind()?;
        self.tier.settle()?;
        self.decoding = None;
        Ok(self.tier.rewind(mind, from)?)
    }

    fn active_mind(&self) -> Result<MindId, DecodeError> {
        self.roles.active().map(Identified::mind).ok_or(DecodeError::NoneActive)
    }

    /// Begin `unit`'s copy-in (the tier is handed the blocks she carries) and hold her as next.
    /// Refused before any transfer starts when next is occupied or she is already here, so a refusal
    /// never leaves a copy-in running for nobody. Runs while the active mind decodes.
    pub fn stage(&mut self, mut unit: U) -> Result<(), Refused<StageError, U>> {
        if let Some(reason) = self.roles.admits(unit.persona()) {
            return Err(Refused { reason: reason.into(), returned: unit });
        }
        let mind = unit.mind();
        let fence = match self.tier.begin(mind, unit.blocks_mut(), Direction::In) {
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
        self.require_landed()?;
        let previous = self.roles.handoff()?;
        self.retired(previous)
    }

    /// Retire the active mind with nobody to follow her (a stop's suspend): once her decode has
    /// landed, begin her copy-out. Returns who retired (`None` when nobody was active); bring it to
    /// its end with [`Self::finish_retirement`].
    pub fn retire_active(&mut self) -> Result<Option<PersonaId>, HandoffError> {
        self.require_landed()?;
        let previous = self.roles.retire()?;
        self.retired(previous)
    }

    /// A move out of the active role is refused while her decode is on the device.
    fn require_landed(&self) -> Result<(), HandoffError> {
        match self.active() {
            Some(active) if !self.decode_landed() => Err(HandoffError::DecodeInFlight { active }),
            _ => Ok(()),
        }
    }

    /// After the active role changed hands: her decode is over, and whoever left begins her copy-out.
    fn retired(&mut self, previous: Option<PersonaId>) -> Result<Option<PersonaId>, HandoffError> {
        self.decoding = None;
        if previous.is_some() {
            self.retry_retire()?;
        }
        Ok(previous)
    }

    /// Begin the copy-out of the mind awaiting it (after a handoff whose copy-out was refused): the
    /// tier is handed her own blocks. Does nothing when no copy-out awaits.
    pub fn retry_retire(&mut self) -> Result<(), HandoffError> {
        let tier_name = self.tier.name();
        let Some((tag, unit)) = self.roles.awaiting_copy_out_mut() else { return Ok(()) };
        let fence = self
            .tier
            .begin(tag.mind, unit.blocks_mut(), Direction::Out)
            .map_err(|refusal| HandoffError::CopyOut { previous: tag.mind.persona, refusal })?;
        // The tag came from the pipeline itself, so the fence it carries is the one it expects.
        self.roles.begin_retire(fence).map_err(|refused| HandoffError::CopyOut {
            previous: tag.mind.persona,
            refusal: TierRefusal { tier: tier_name, mind: tag.mind, reason: refusal_reason(&refused.reason), native: Native::NotAsked },
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
