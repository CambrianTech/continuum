//! Why a checkpoint of a drained pool's residents did not confirm every save: typed, so a caller
//! branches on the variant and a probe records the variant, never a formatted sentence (the
//! review of #5987: three save outcomes had collapsed into one `String`).

use super::{EndpointSlots, PageKey};
use std::sync::Arc;

/// How one resident's save ended without a confirmed page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveFailure {
    /// The engine refused the save with its typed request error: nothing was written.
    Rejected,
    /// The engine kept working with no receipt: the request is held by the endpoint until it
    /// answers, and the endpoint is quarantined.
    Unresolved,
    /// No receipt and no proof the engine kept working: the endpoint is quarantined.
    Uncertain,
}

/// Why a checkpoint ended before confirming every resident. `E` is the caller's engine
/// authority refusal (an owned generation's, or an adopted process's).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CheckpointFailure<E: std::error::Error> {
    #[error(transparent)]
    Engine(E),
    /// The page-write negotiation with the engine failed before any save (its own detail).
    #[error("page admission before the checkpoint: {0}")]
    Admission(String),
    #[error("checkpoint save of slot {slot}: {kind:?}")]
    Save { slot: u32, kind: SaveFailure },
}

/// Why a checkpoint of THIS core's owned generation was refused or failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum OwnedCheckpointRefusal {
    #[error("checkpoint URL does not identify the held endpoint")]
    EndpointMismatch,
    #[error("paging completion remains uncertain")]
    PagingUncertain,
    #[error("no owned generation")]
    NotOwned,
    #[error("the generation exited or its pool is not the verified install")]
    PoolUnverified,
    #[error("no verified page directory")]
    NoPageDir,
    #[error("the checkpoint's engine exited")]
    EngineExited,
}

/// One resident's save in flight. Dropped unanswered, it quarantines the endpoint: a save with
/// no receipt may still be writing, so no turn is admitted until a verified replacement. Its
/// answer is given by consuming it, so no path can leave it answered by mistake.
#[must_use = "an unanswered save quarantines its endpoint"]
pub(super) struct CheckpointSave {
    endpoint: Option<Arc<EndpointSlots>>,
}

impl CheckpointSave {
    pub(super) fn begin(endpoint: Arc<EndpointSlots>) -> Self {
        Self { endpoint: Some(endpoint) }
    }

    /// The engine confirmed the save: `key`'s page is restorable.
    pub(super) fn confirmed(mut self, pool: &super::KvSlotPool, key: PageKey) {
        self.endpoint = None;
        pool.note_saved(key);
    }

    /// The engine refused the save with its typed error: nothing was written, nothing in flight.
    pub(super) fn rejected(mut self) {
        self.endpoint = None;
    }
}

impl Drop for CheckpointSave {
    fn drop(&mut self) {
        if let Some(endpoint) = self.endpoint.take() {
            endpoint.quarantine_paging();
        }
    }
}
