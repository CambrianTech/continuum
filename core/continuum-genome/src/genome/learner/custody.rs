//! The one owner of every learning lease (card 4114a2c6).
//!
//! A residency's leases enter custody synchronously, the moment they are granted and before the
//! engine is told anything, so no await (and no cancelled future) sits between leasing and custody.
//! A seat, an optimizer or a workspace holds only a [`Claim`]. Dropping a claim for any reason (an
//! error, a cancelled future, an unwind) marks its entry orphaned and never drops a lease. An entry
//! leaves custody, and its leases with it, only when the engine confirms the residency released,
//! through the claim or through [`Custody::settle_orphans`], which the owning task runs on its tick.

use super::seat::confirm_release;
use crate::inference::llama_server::{LearningEngine, LedgerAnswer, ResidencyId};
use crate::resources::LeaseGuard;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;

/// Every learning residency this core leased and the engine has not confirmed released.
#[derive(Default)]
pub struct Custody {
    entries: Mutex<HashMap<ResidencyId, Entry>>,
}

struct Entry {
    engine: LearningEngine,
    _leases: Vec<LeaseGuard>,
    orphaned: bool,
}

impl Custody {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Take `leases` into custody for `residency` in `engine`; the caller keeps only the claim.
    pub(super) fn hold(self: &Arc<Self>, engine: &LearningEngine, residency: ResidencyId, leases: Vec<LeaseGuard>) -> Claim {
        self.entries.lock().insert(residency, Entry { engine: engine.clone(), _leases: leases, orphaned: false });
        Claim { custody: Arc::clone(self), residency, released: false }
    }

    /// How many residencies are held, and how many of them nobody claims.
    pub fn held(&self) -> (usize, usize) {
        let entries = self.entries.lock();
        (entries.len(), entries.values().filter(|entry| entry.orphaned).count())
    }

    /// Settle every orphaned residency: unload whatever adapter the engine bound inside it, then
    /// release it; an entry the engine confirms leaves custody with its leases, the rest stay for the
    /// next pass. Returns how many orphans remain.
    pub async fn settle_orphans(&self) -> usize {
        let orphans: Vec<(ResidencyId, LearningEngine)> =
            self.entries.lock().iter().filter(|(_, entry)| entry.orphaned).map(|(id, entry)| (*id, entry.engine.clone())).collect();
        let mut remaining = 0;
        for (residency, engine) in orphans {
            match settle(&engine, &residency).await {
                Ok(()) => {
                    self.entries.lock().remove(&residency);
                }
                Err(_unconfirmed) => remaining += 1,
            }
        }
        remaining
    }

    fn engine(&self, residency: &ResidencyId) -> Option<LearningEngine> {
        self.entries.lock().get(residency).map(|entry| entry.engine.clone())
    }
}

/// Unload any adapter the engine's ledger shows bound inside `residency`, then release it.
async fn settle(engine: &LearningEngine, residency: &ResidencyId) -> Result<(), String> {
    let id = residency.to_string();
    let bound = match engine.learn_ledger().await {
        Ok(LedgerAnswer::Ledger(ledger)) => ledger.residencies.iter().find(|held| held.id == id).map(|held| held.lora_id).filter(|lora| *lora >= 0),
        Ok(LedgerAnswer::NoLedger) => return Err("the engine lost its learning ledger".into()),
        Err(error) => return Err(format!("the ledger did not answer: {error}")),
    };
    if let Some(lora) = bound {
        engine.unseat(lora).await.map_err(|error| format!("unload of adapter {lora} refused: {error}"))?;
    }
    confirm_release(engine, residency).await
}

/// A live hold on one residency in custody. Dropped without a confirmed release, its entry becomes an
/// orphan for the owner to settle; its leases never drop with it.
pub struct Claim {
    custody: Arc<Custody>,
    residency: ResidencyId,
    released: bool,
}

impl Claim {
    pub fn residency(&self) -> ResidencyId {
        self.residency
    }

    /// The engine refused to register the residency, its word that it holds nothing there: the
    /// entry leaves custody and its leases drop.
    pub(super) fn refused(mut self) {
        self.custody.entries.lock().remove(&self.residency);
        self.released = true;
    }

    /// Release the residency; on the engine's confirmation it leaves custody with its leases. An
    /// unconfirmed release hands the claim back.
    pub async fn release(mut self) -> Result<(), (Self, String)> {
        let Some(engine) = self.custody.engine(&self.residency) else {
            return Err((self, "the residency is not in custody".into()));
        };
        match confirm_release(&engine, &self.residency).await {
            Ok(()) => {
                self.custody.entries.lock().remove(&self.residency);
                self.released = true;
                Ok(())
            }
            Err(why) => Err((self, why)),
        }
    }

    /// Settle the residency the way an orphan is settled (unload any adapter bound inside it, then
    /// release); used when a load's answer was lost.
    pub async fn settle(mut self) -> Result<(), (Self, String)> {
        let Some(engine) = self.custody.engine(&self.residency) else {
            return Err((self, "the residency is not in custody".into()));
        };
        match settle(&engine, &self.residency).await {
            Ok(()) => {
                self.custody.entries.lock().remove(&self.residency);
                self.released = true;
                Ok(())
            }
            Err(why) => Err((self, why)),
        }
    }
}

impl std::fmt::Debug for Claim {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Claim({})", self.residency)
    }
}

impl Drop for Claim {
    fn drop(&mut self) {
        if !self.released {
            if let Some(entry) = self.custody.entries.lock().get_mut(&self.residency) {
                entry.orphaned = true;
            }
        }
    }
}
