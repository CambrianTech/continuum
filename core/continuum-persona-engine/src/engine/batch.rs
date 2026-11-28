//! Minds bound beside the active one, and the batch step they decode in with her (THE-LANE-ON-SAGE.md, open
//! question 1). One step is still in flight at a time: a batch is one step for several bound minds, so every rule
//! that guards a decode on the device guards it too. A mind beside joins through her tier's copy-in like a staged
//! one, and leaves through the one copy-out path a retired mind takes.

use super::{DecodeError, HandoffError, PersonaEngine, StageError};
use crate::persona::{Carries, MindId, PersonaId};
use crate::pipeline::{BesideRefusal, Identified, Refused};
use crate::residency::{Direction, Execute, TierRefusal};

/// Why minds were not bound, swapped or batched. Nothing changed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BatchError {
    #[error(transparent)]
    Beside(#[from] BesideRefusal),
    #[error(transparent)]
    Tier(#[from] TierRefusal),
    /// A decode is on the device: nothing moves, and no second step goes in behind it.
    #[error("{active:?}'s decode has not landed")]
    DecodeInFlight { active: PersonaId },
    #[error(transparent)]
    Handoff(#[from] HandoffError),
    #[error("{persona:?} is not bound here")]
    NotBound { persona: PersonaId },
}

impl<U, T> PersonaEngine<U, T>
where
    U: Identified + Carries<T::Blocks>,
    T: Execute,
{
    /// The minds bound beside the active one.
    pub fn beside(&self) -> impl Iterator<Item = PersonaId> + '_ {
        self.roles.beside().map(Identified::persona)
    }

    /// A bound mind's own state (the active one or one beside her), never the tier she runs on.
    pub fn bound_mut(&mut self, persona: PersonaId) -> Option<&mut U> {
        self.roles.bound_mut(persona)
    }

    fn landed_or(&self) -> Result<(), BatchError> {
        match self.active() {
            Some(active) if !self.decode_landed() => Err(BatchError::DecodeInFlight { active }),
            _ => Ok(()),
        }
    }

    /// Bind `unit` beside the active mind in a stream of her own: her copy-in through the tier, then her place beside
    /// once it landed. A refusal returns her.
    pub fn bind_beside(&mut self, mut unit: U) -> Result<(), Refused<StageError, U>> {
        let mind = unit.mind();
        let fence = match self.tier.begin(mind, unit.blocks_mut(), Direction::In) {
            Ok(fence) => fence,
            Err(refusal) => return Err(Refused { reason: refusal.into(), returned: unit }),
        };
        self.roles.bind_beside(unit, fence).map_err(|refused| Refused { reason: StageError::Beside(refused.reason), returned: refused.returned.0 })
    }

    /// Make `persona`, bound beside, the active mind. No transfer; refused while a decode is on the device.
    pub fn swap_active(&mut self, persona: PersonaId) -> Result<(), BatchError> {
        self.landed_or()?;
        self.roles.swap_active(persona)?;
        self.decoding = None;
        Ok(())
    }

    /// Unbind `persona` from beside the active mind and begin her copy-out, as a retired mind's; she is given back by
    /// [`Self::finish_retirement`]. Refused while a decode is on the device or another retirement is in flight.
    pub fn unbind_beside(&mut self, persona: PersonaId) -> Result<(), BatchError> {
        self.landed_or()?;
        self.roles.unbind_beside(persona)?;
        self.retry_retire()?;
        Ok(())
    }

    /// Submit one step for several bound minds (the active one and minds beside her), each her own work: one decode,
    /// in flight like any other. Refused, submitting nothing, while the previous step is on the device or a named
    /// mind is not bound here.
    pub fn submit_batch(&mut self, work: &[(PersonaId, &T::Work)]) -> Result<(), DecodeError> {
        let active = self.roles.active().map(Identified::persona).ok_or(DecodeError::NoneActive)?;
        if !self.decode_landed() {
            return Err(DecodeError::InFlight { active });
        }
        let mut steps: Vec<(MindId, &T::Work)> = Vec::with_capacity(work.len());
        for &(persona, step) in work {
            let mind = self.roles.bound_mut(persona).map(|unit| unit.mind()).ok_or(DecodeError::NotBound { persona })?;
            steps.push((mind, step));
        }
        self.decoding = Some(self.tier.submit_batch(&steps)?);
        Ok(())
    }
}
