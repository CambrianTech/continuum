//! The minds an engine preserved across a restart (CONTINUUM-ENGINE 4.16, restore is the norm), as a
//! kind of the one contract for state that outlives a process (`continuum-saved-state`). The envelope
//! gives the atomic save, the generation and digest, and the typed load verdict; the contract
//! (the model) is compared field by field, so another model's minds load as `Diverged`, never as
//! hers. Each mind's blocks are written by the fork beside it (`<persona>.cpb`, raw rows; the
//! fork checks owner, epoch and memory layout on read) until the store's `ExternalBlob` medium
//! records them by receipt.
//!
//! One generation per node, replaced at every stop: a mind's file is her only copy, never aged out.

use std::path::PathBuf;

use continuum_saved_state::{OwnedFile, Retention, SavedState, Singleton, Store};

/// One preserved mind: who she is, her genes, and her token history (exactly what her blocks hold).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(super) struct PreservedMind {
    pub persona: uuid::Uuid,
    pub epoch: u64,
    pub basis: String,
    pub held: Vec<i32>,
}

/// What a preserved mind is valid against: the model she ran on.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub(super) struct ServedModel {
    pub model: String,
}

/// The kind: the minds one engine preserved at its last stop.
pub(super) struct PreservedMinds;

impl SavedState for PreservedMinds {
    const KIND: &'static str = "preserved-minds";
    const FORMAT: u32 = 1;
    const RETENTION: Retention = Retention::PerKey { bound: "one per node, replaced at every stop; a mind's only copy" };
    type Key = Singleton;
    type Contract = ServedModel;
    type Value = Vec<PreservedMind>;
    type Medium = OwnedFile;
}

/// Where preserved minds live: the store, and her block file beside it.
pub(super) struct Preserved {
    pub store: Store<PreservedMinds>,
    dir: PathBuf,
}

impl Preserved {
    /// The node's preserved minds, under its continuum home.
    pub(super) fn at_home() -> Result<Self, String> {
        let dir = continuum_kernel::paths::continuum_home()?.join("persona-blocks");
        std::fs::create_dir_all(&dir).map_err(|e| format!("persona-blocks {}: {e}", dir.display()))?;
        Ok(Self { store: Store::at(dir.clone()), dir })
    }

    /// Her block file, written and read by the fork.
    pub(super) fn blocks(&self, persona: uuid::Uuid) -> PathBuf {
        self.dir.join(format!("{persona}.cpb"))
    }
}
