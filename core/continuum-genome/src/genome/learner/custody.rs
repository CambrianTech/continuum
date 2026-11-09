//! The one owner of every learning lease (card 4114a2c6).
//!
//! A residency's leases enter custody synchronously, the moment they are granted and before the
//! engine is told anything, so no await (and no cancelled future) sits between leasing and custody.
//! A seat, an optimizer or a workspace holds only a [`Claim`]. Dropping a claim for any reason (an
//! error, a cancelled future, an unwind) marks its entry orphaned and never drops a lease. An entry
//! leaves custody, and its leases with it, only when the engine confirms the residency released,
//! through the claim or through [`Custody::settle_orphans`], which the owning task runs on its tick.
//!
//! Confirmation is fenced by incarnation (STAGE-CONTRACT 2): each entry records the engine
//! incarnation its residency was registered with. A receipt counts only from that incarnation; any
//! other answer at the same address (a relaunch) proves nothing about the old process's memory, so
//! the entry stays until that process is verifiably dead (`EngineIncarnation::liveness`, pid and OS
//! start time, an unreadable process never counting as dead).
//! Custody is one per process ([`Custody::global`]), so it outlives any task that drives it.

use crate::inference::engine_residency::Liveness;
use crate::inference::llama_server::{LearningEngine, LedgerAnswer, LedgerEngine, ResidencyId, ResidencyRefusal};
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
    /// The engine incarnation the residency was registered with: the only one whose receipts count.
    admitted: LedgerEngine,
    _leases: Vec<LeaseGuard>,
    orphaned: bool,
}

static GLOBAL: std::sync::OnceLock<Arc<Custody>> = std::sync::OnceLock::new();

impl Custody {
    /// This process's custody: every learning lease the core holds, for as long as the process lives.
    pub fn global() -> Arc<Self> {
        Arc::clone(GLOBAL.get_or_init(|| Arc::new(Self::default())))
    }

    /// A custody of its own, for a test that inspects one in isolation.
    #[cfg(any(test, feature = "test-fixtures"))]
    pub fn isolated() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Take `leases` into custody for `residency`, registered with `engine` as incarnation
    /// `admitted`; the caller keeps only the claim.
    pub(super) fn hold(self: &Arc<Self>, engine: &LearningEngine, admitted: &LedgerEngine, residency: ResidencyId, leases: Vec<LeaseGuard>) -> Claim {
        self.entries.lock().insert(residency, Entry { engine: engine.clone(), admitted: admitted.clone(), _leases: leases, orphaned: false });
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
        let orphans: Vec<(ResidencyId, LearningEngine, LedgerEngine)> = self
            .entries
            .lock()
            .iter()
            .filter(|(_, entry)| entry.orphaned)
            .map(|(id, entry)| (*id, entry.engine.clone(), entry.admitted.clone()))
            .collect();
        let mut remaining = 0;
        for (residency, engine, admitted) in orphans {
            match settle(&engine, &admitted, &residency).await {
                Ok(()) => {
                    self.entries.lock().remove(&residency);
                }
                Err(_unconfirmed) => remaining += 1,
            }
        }
        remaining
    }

    fn entry(&self, residency: &ResidencyId) -> Option<(LearningEngine, LedgerEngine)> {
        self.entries.lock().get(residency).map(|entry| (entry.engine.clone(), entry.admitted.clone()))
    }
}

/// Settle `residency`, registered with `engine` as incarnation `admitted`: unload any adapter the
/// ledger shows bound inside it, then release it. `Ok` only on proof the engine holds nothing there:
/// a release (or an unknown residency) confirmed by the admitted incarnation, or that incarnation's
/// process verifiably dead. An answer from any other incarnation at the address proves nothing.
async fn settle(engine: &LearningEngine, admitted: &LedgerEngine, residency: &ResidencyId) -> Result<(), String> {
    let unproven = |why: String| match engine.process().liveness() {
        Liveness::Dead => Ok(()), // the process that held the memory is gone, and the memory with it
        Liveness::Alive | Liveness::Unknown => Err(why),
    };
    let id = residency.to_string();
    let ledger = match engine.learn_ledger().await {
        Ok(LedgerAnswer::Ledger(ledger)) => ledger,
        Ok(LedgerAnswer::NoLedger) => return unproven("the engine at that address has no learning ledger".into()),
        Err(error) => return unproven(format!("the ledger did not answer: {error}")),
    };
    if &ledger.engine != admitted {
        return unproven(format!("another incarnation answers ({:?}); the admitted one ({admitted:?}) is not proven dead", ledger.engine));
    }
    // The unload names a lora id read from the admitted incarnation; binding the unload itself to
    // that incarnation (so a relaunch between the read and the unload is refused) is the engine's,
    // asked of Codex 2026-11-09.
    if let Some(lora) = ledger.residencies.iter().find(|held| held.id == id).map(|held| held.lora_id).filter(|lora| *lora >= 0) {
        engine.unseat(lora).await.map_err(|error| format!("unload of adapter {lora} refused: {error}"))?;
    }
    match engine.release_residency(&id).await {
        Ok(LedgerAnswer::Ledger(released)) if &released.ledger.engine != admitted => {
            unproven(format!("the release was answered by another incarnation ({:?})", released.ledger.engine))
        }
        Ok(LedgerAnswer::Ledger(released)) => match released.refused {
            None | Some(ResidencyRefusal::UnknownResidency) => Ok(()),
            Some(why) => Err(format!("residency {residency} release refused: {why:?}")),
        },
        Ok(LedgerAnswer::NoLedger) => unproven("the engine at that address lost its learning ledger".into()),
        Err(error) => unproven(format!("residency {residency} release failed: {error}")),
    }
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

    /// Release the residency (unloading any adapter still bound inside it), fenced by incarnation; on
    /// proof the engine holds nothing there it leaves custody with its leases. Unproven, the claim is
    /// handed back.
    pub async fn release(mut self) -> Result<(), (Self, String)> {
        let Some((engine, admitted)) = self.custody.entry(&self.residency) else {
            return Err((self, "the residency is not in custody".into()));
        };
        match settle(&engine, &admitted, &self.residency).await {
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
