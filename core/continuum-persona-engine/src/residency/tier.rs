//! [`Tier`]: one contract for every residency tier (device VRAM, pinned host RAM, the unified
//! pool, flash). An adapter holds only the calls that differ; the pipeline never branches on
//! which tier it has.

use super::FenceTag;
use crate::persona::PersonaId;

/// A tier could not take or give a persona's bytes: named, so the engine keeps her where she is.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{tier}: {reason} for persona {persona:?}{native}")]
pub struct TierRefusal {
    pub tier: &'static str,
    pub persona: PersonaId,
    pub reason: &'static str,
    /// The backend's own refusal, kept rather than erased.
    pub native: Native,
}

/// What the backend itself said when the tier refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Native {
    /// The refusal is the tier's own (a precondition it checked); the backend was not asked.
    NotAsked,
    /// The backend refused, by its status and the name it gives that status.
    Refused { status: i32, name: String },
    /// The backend refused with a message and no status.
    Reported(String),
}

impl std::fmt::Display for Native {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotAsked => Ok(()),
            Self::Refused { status, name } => write!(f, " (backend status {status}: {name})"),
            Self::Reported(message) => write!(f, " (backend: {message})"),
        }
    }
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

    /// Give up `persona`'s blocks held in this tier (a mind that will not come back, such as a one-off
    /// probe). Refused while she is in the execution stream.
    fn release(&mut self, persona: PersonaId) -> Result<(), TierRefusal>;
}
