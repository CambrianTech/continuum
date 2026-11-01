//! What a run costs. Execution and residency are separate claims on the one resource authority:
//! paused or yielded state still holds bytes after execution capacity is released.

use std::time::Duration;

/// The model tier a call needs; the resource authority maps it to a lane and a model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModelTier {
    Small,
    Mid,
    Large,
}

/// Executing: CPU time (with its measured estimate, never free) or a model call at a tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionCost {
    Cpu { estimated: Duration },
    Model { tier: ModelTier },
}

/// Keeping state resident across a pause or yield: bytes that stay charged, by kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ResidencyCost {
    pub host_bytes: u64,
    pub device_bytes: u64,
}

/// One run's claim: what executing costs, and what staying resident costs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cost {
    pub execution: ExecutionCost,
    pub residency: ResidencyCost,
}
