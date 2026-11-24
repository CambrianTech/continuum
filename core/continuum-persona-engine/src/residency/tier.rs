//! [`Tier`]: one contract for every residency tier (device VRAM, pinned host RAM, the unified
//! pool, flash). An adapter holds only the calls that differ; the pipeline never branches on
//! which tier it has.

use super::Direction;
use crate::persona::MindId;

/// A tier could not take or give a mind's bytes: named, so the engine keeps her where she is.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{tier}: {reason} for {mind:?}{native}")]
pub struct TierRefusal {
    pub tier: &'static str,
    pub mind: MindId,
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
    /// No room for her: every sequence is seated, or the pool has fewer empty cells than she needs. A
    /// seated mind evicted makes room; nothing of hers changed.
    NoRoom,
}

impl TierRefusal {
    /// Whether evicting a seated mind would make room for what was refused.
    pub fn wants_room(&self) -> bool {
        matches!(self.native, Native::NoRoom)
    }
}

impl std::fmt::Display for Native {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotAsked => Ok(()),
            Self::Refused { status, name } => write!(f, " (backend status {status}: {name})"),
            Self::Reported(message) => write!(f, " (backend: {message})"),
            Self::NoRoom => write!(f, " (no room: a seated mind must leave first)"),
        }
    }
}

/// A residency tier: the native moves of a mind's blocks. It is handed the blocks it moves (her unit
/// carries them, CONTINUUM-ENGINE 4.17) and keeps only which stream holds which mind; giving a mind up
/// is dropping her unit, so there is no release here.
pub trait Tier {
    type Fence: super::Fence;
    /// Her blocks in this tier's form.
    type Blocks;

    /// The tier's name, for refusals and receipts.
    fn name(&self) -> &'static str;

    /// New blocks for `mind` (a mind seen for the first time, or a new epoch of her), stamped with her
    /// owner: what her unit carries from then on.
    fn blocks_for(&mut self, mind: MindId) -> Result<Self::Blocks, TierRefusal>;

    /// Begin moving `mind`'s `blocks` in `direction` (into the execution stream, layer order behind
    /// compute; or out of it into her blocks). The fence it returns is tagged with her mind and the
    /// direction, so [`crate::pipeline::Roles`] accepts it only for that unit.
    fn begin(&mut self, mind: MindId, blocks: &mut Self::Blocks, direction: Direction) -> Result<Self::Fence, TierRefusal>;
}
