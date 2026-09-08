//! Memento: per-persona episodic memory.
//!
//! - `store` — the on-disk store (JSONL append log + snapshots).
//! - `gate` — turn-admission gate that serializes "who may write" with
//!   "when a snapshot is taken", so a turn can never be admitted after a
//!   close and yet land after the save.

pub mod gate;
mod store;

pub use gate::TurnGate;
pub use store::{MementoStore, MementoStoreError};

use std::sync::Arc;

/// A single episodic record.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Episode {
    /// Stable id (persona-scoped).
    pub id: String,
    /// Coarse kind tag (`turn`, `recall`, `consolidation`, ...).
    pub kind: String,
    /// Text content.
    pub text: String,
    /// Unix millis at write time.
    pub ts_ms: u64,
}

/// Handle to an admitted turn; releasing it (drop) decrements the gate's
/// in-flight count exactly once.
#[derive(Debug)]
pub struct TurnPermit {
    gate: Arc<TurnGate>,
    /// `Some` if this permit was granted while the gate was open and must be
    /// released before the gate may drain; `None` for rejected admissions.
    live: Option<usize>,
}

impl TurnPermit {
    pub fn is_live(&self) -> bool {
        self.live.is_some()
    }
}

impl Drop for TurnPermit {
    fn drop(&mut self) {
        if let Some(slot) = self.live.take() {
            self.gate.release(slot);
        }
    }
}

/// The Memento facade: owns the store and the admission gate.
pub struct Memento {
    store: Arc<MementoStore>,
    pub(crate) gate: Arc<TurnGate>,
}

impl Memento {
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Arc<Self>, MementoStoreError> {
        Ok(Arc::new(Self {
            store: Arc::new(MementoStore::open(path)?),
            gate: Arc::new(TurnGate::new()),
        }))
    }

    /// Admit a turn. While the gate is open this reserves an in-flight slot
    /// and returns a live permit; after `close` it returns a dead permit.
    /// The check-and-reserve is one linearizable step (see [`TurnGate`]).
    pub fn admit(&self) -> TurnPermit {
        match self.gate.admit() {
            Some(slot) => TurnPermit { gate: Arc::clone(&self.gate), live: Some(slot) },
            None => TurnPermit { gate: Arc::clone(&self.gate), live: None },
        }
    }

    /// Close the gate: no further admissions. Returns `true` if this call
    /// performed the close (idempotent otherwise).
    pub fn close(&self) -> bool {
        self.gate.close()
    }

    /// Snapshot the store to disk. Only safe once the gate is closed and all
    /// admitted turns have released; [`Self::drain_ready`] states that.
    pub fn save(&self) -> Result<usize, MementoStoreError> {
        self.store.snapshot()
    }

    /// True iff the gate is closed AND no admitted turn is in flight — i.e.
    /// a save can be taken without racing an admission.
    pub fn drain_ready(&self) -> bool {
        self.gate.drain_ready()
    }

    /// Atomically claim the one-shot drain (close + empty), so concurrent
    /// callers cannot both take the post-close snapshot. Returns `true` for
    /// exactly one caller when the gate is closed and drained.
    pub fn claim_drain(&self) -> bool {
        self.gate.claim_drain()
    }

    /// Write an episode under a live permit; dead permits are dropped on the
    /// floor (they were admitted after close, by contract).
    pub fn record(&self, permit: &TurnPermit, ep: Episode) -> Result<(), MementoStoreError> {
        if !permit.is_live() {
            return Ok(());
        }
        self.store.append(ep)
    }

    /// Load all episodes (for consolidation / recall).
    pub fn load_all(&self) -> Vec<Episode> {
        self.store.load_all()
    }

    #[cfg(test)]
    pub(crate) fn gate(&self) -> &TurnGate {
        &self.gate
    }
}
