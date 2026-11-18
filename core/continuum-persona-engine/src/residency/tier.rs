//! [`Tier`]: one contract for every residency tier (device VRAM, pinned host RAM, the unified
//! pool, flash). An adapter holds only the calls that differ; the pipeline never branches on
//! which tier it has.

use super::FenceTag;
use crate::persona::PersonaId;

/// A tier could not take or give a persona's bytes: named, so the engine keeps her where she is.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{tier}: {reason} for persona {persona:?}")]
pub struct TierRefusal {
    pub tier: &'static str,
    pub persona: PersonaId,
    pub reason: &'static str,
}

/// A residency tier.
pub trait Tier {
    type Fence: super::Fence;

    /// The tier's name, for refusals and receipts.
    fn name(&self) -> &'static str;

    /// Begin the transfer `what` names: her blocks of that exact epoch, in that direction (into the
    /// execution stream, layer order behind compute; or out of it into this tier). The fence it
    /// returns carries the same tag, so [`crate::pipeline::Roles`] accepts it only for that unit.
    fn begin(&mut self, what: FenceTag) -> Result<Self::Fence, TierRefusal>;
}
