//! Why retiring the incumbent engine was refused before anything was signalled: a typed fact the
//! reconcile branches on by variant, its text only its `Display` (Joel, 2026-11-04: "always give me
//! enums and ints ... String matching is insane").

use crate::inference::slots::AdoptedCheckpointRefusal;

/// Why the incumbent engine could not be retired safely; it was not signalled and still runs.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RetirementRefusal {
    #[error("recorded live lane belongs to another endpoint")]
    OtherEndpoint,
    #[error("adopted engine identity is unverified")]
    IdentityUnverified,
    #[error("adopted engine identity became unverified after checkpoint")]
    IdentityUnverifiedAfterCheckpoint,
    /// Its residents' checkpoint custody refused.
    #[error(transparent)]
    Checkpoint(#[from] AdoptedCheckpointRefusal),
    /// The OS refused to signal the exact process; its detail is display-only.
    #[error("signalling the adopted engine was refused: {0}")]
    Signal(String),
}
