//! Completion of a transfer, tied to what it moves. Readiness is never exposed before its fence
//! completes, and a fence for another mind or direction is never accepted.

use crate::persona::MindId;

/// Which way a transfer moves her blocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Into the execution stream (staging her as next).
    In,
    /// Out of the execution stream to her tier (retiring her).
    Out,
}

/// What a fence moves: whose blocks (her mind: persona and generation), which way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FenceTag {
    pub mind: MindId,
    pub direction: Direction,
}

/// A transfer's completion, as the tier that issued it reports it.
pub trait Fence {
    /// What this transfer moves.
    fn tag(&self) -> FenceTag;
    /// Whether every byte of the transfer has landed.
    fn complete(&self) -> bool;
}

/// A unit that carries her own blocks: the tier moves the blocks it is handed and holds no copy of
/// them (CONTINUUM-ENGINE 4.17).
pub trait Carries<B> {
    fn blocks_mut(&mut self) -> &mut B;
}
