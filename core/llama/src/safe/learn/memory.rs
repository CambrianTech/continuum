//! What native code reports it would hold for learning, row by row, typed at this boundary: the one
//! owner of the measured roles (Codex on #6254), so a memory owner matches allocator scope without
//! parsing names.

use std::ffi::CStr;

/// What a measured row of memory is held for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MemoryRole {
    /// A shadow adapter's weights, from the adapter measurement.
    Weights,
    /// The optimizer's device state (its moments), static for the shadow's life.
    OptimizerStatic,
    /// The optimizer's host state.
    OptimizerHost,
    /// The shared training workspace a reply would need.
    SharedComputeRequired,
    /// Shared workspace the context already holds: evidence, never a further lease.
    SharedComputeHeld,
}

impl MemoryRole {
    /// The role native code names `kind`; an unknown name is `None`, refused by the caller, never
    /// charged under a guessed role.
    pub fn of(kind: &CStr) -> Option<Self> {
        match kind.to_bytes() {
            b"optimizer_static_required" => Some(Self::OptimizerStatic),
            b"optimizer_host_required" => Some(Self::OptimizerHost),
            b"shared_compute_required" => Some(Self::SharedComputeRequired),
            b"shared_compute_held" => Some(Self::SharedComputeHeld),
            _ => None,
        }
    }
}

/// One row native code reported: where it lives, what it is for, and its size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryRow {
    pub device: String,
    pub buffer_type: String,
    pub role: MemoryRole,
    pub bytes: u64,
    pub host: bool,
}
