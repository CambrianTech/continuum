//! Completion of a transfer. Readiness is never exposed before its fence completes.

/// A transfer's completion, as the tier that issued it reports it.
pub trait Fence {
    /// Whether every byte of the transfer has landed.
    fn complete(&self) -> bool;
}
