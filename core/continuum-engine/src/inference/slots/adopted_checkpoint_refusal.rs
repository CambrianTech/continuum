//! Why an adopted engine's checkpoint before its replacement was refused: a typed fact every caller
//! branches on by variant, its text only its `Display` (Joel, 2026-11-04: "always give me enums and
//! ints ... String matching is insane").

/// Why checkpointing an adopted engine before its replacement was refused. Nothing was signalled.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AdoptedCheckpointRefusal {
    #[error("adopted checkpoint URL does not identify the held endpoint")]
    EndpointMismatch,
    /// This core owns a generation on the endpoint, or its paging is uncertain.
    #[error("adopted checkpoint requires an unowned, certain pool")]
    OwnedOrUncertain,
    #[error("adopted checkpoint requires a known pool")]
    UnknownPool,
    #[error("adopted checkpoint requires a verified ledger")]
    UnverifiedLedger,
    /// The ledger names another process than the recorded lane's.
    #[error("adopted checkpoint ledger names a different process")]
    OtherProcess,
    #[error("adopted checkpoint engine exited or PID was reused")]
    EngineGone,
    #[error("adopted checkpoint engine identity is unverified")]
    IdentityUnverified,
    #[error("adopted checkpoint intent changed")]
    IntentChanged,
    /// The save itself failed; its detail is display-only.
    #[error("adopted checkpoint save failed: {0}")]
    Save(String),
    #[error("adopted replacement intent changed after checkpoint")]
    IntentChangedAfterCheckpoint,
}
