//! Completion of a transfer, tied to what it moves. Readiness is never exposed before its fence
//! completes, and a fence for another persona, epoch or direction is never accepted.

use crate::persona::{Epoch, PersonaId};

/// Which way a transfer moves her blocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Into the execution stream (staging her as next).
    In,
    /// Out of the execution stream to her tier (retiring her).
    Out,
}

/// What a fence moves: whose blocks, which generation of them, which way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FenceTag {
    pub persona: PersonaId,
    pub epoch: Epoch,
    pub direction: Direction,
}

/// A transfer's completion, as the tier that issued it reports it.
pub trait Fence {
    /// What this transfer moves.
    fn tag(&self) -> FenceTag;
    /// Whether every byte of the transfer has landed.
    fn complete(&self) -> bool;
}
