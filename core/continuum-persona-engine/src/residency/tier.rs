//! [`Tier`]: one contract for every residency tier (device VRAM, pinned host RAM, the unified
//! pool, flash). An adapter holds only the calls that differ; the pipeline never branches on
//! which tier it has.

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

    /// Begin moving her blocks INTO the execution stream (layer order, behind compute).
    fn copy_in(&mut self, persona: PersonaId) -> Result<Self::Fence, TierRefusal>;

    /// Begin moving her blocks OUT of the execution stream into this tier.
    fn copy_out(&mut self, persona: PersonaId) -> Result<Self::Fence, TierRefusal>;
}
