//! ONE ACT AT A TIME PER MIND: the gate every generation that warms a page passes.
//!
//! A mind has one page ([`PageKey::Mind`]), so two of her generations running at once would
//! fight over it: the second leases the same slot, pins it, and waits for the first's slot
//! permit holding a LANE — a lane another mind could have used (the M5 serves two). This
//! gate puts that wait BEFORE the lane: a second act of the same mind queues behind the
//! running one, FIFO (tokio's semaphore is fair), holding nothing, and is never dropped;
//! its only bound is the turn's own deadline.
//!
//! Why here and not in the mind's crate: this is the one seam every page-warming generation
//! crosses, whichever door dispatched it — the persona service loop, `persona/turn-execute`
//! and `ai/should-respond` driving the same live cycle, `cognition/respond`, the RAG
//! inspector's decide-and-respond. `turn_ingress` is the process-wide stop door (open or
//! closed, a count), not a per-mind order; a gate on the mind's side would need every door
//! to remember it, and the next door added would not. A fork ([`PageKey::Fork`]) has its own
//! gate per room, so an exam never waits behind the mind it copies.
//!
//! The gates are weak: a mind with no act running or queued holds no entry.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Weak};

use parking_lot::Mutex;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use super::PageKey;

static GATES: LazyLock<Mutex<HashMap<PageKey, Weak<Semaphore>>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// The right to run one act of a mind. Dropping it lets the next queued act of that mind run.
pub(crate) struct MindTurn {
    _permit: OwnedSemaphorePermit,
    _custody: tokio::sync::OwnedRwLockReadGuard<()>,
}

static CUSTODY: LazyLock<Mutex<HashMap<uuid::Uuid, Weak<tokio::sync::RwLock<()>>>>> = LazyLock::new(|| Mutex::new(HashMap::new()));
fn custody(persona: uuid::Uuid) -> Arc<tokio::sync::RwLock<()>> {
    let mut owners = CUSTODY.lock();
    if let Some(owner) = owners.get(&persona).and_then(Weak::upgrade) { return owner; }
    owners.retain(|_, owner| owner.strong_count() > 0);
    let owner = Arc::new(tokio::sync::RwLock::new(()));
    owners.insert(persona, Arc::downgrade(&owner));
    owner
}

/// The gate of `key`, made on first use; dead entries are swept on the way.
fn gate(key: PageKey) -> Arc<Semaphore> {
    let mut gates = GATES.lock();
    if let Some(gate) = gates.get(&key).and_then(Weak::upgrade) {
        return gate;
    }
    gates.retain(|_, gate| gate.strong_count() > 0);
    let gate = Arc::new(Semaphore::new(1));
    gates.insert(key, Arc::downgrade(&gate));
    gate
}

impl MindTurn {
    /// Wait for `key`'s running act (and every act queued before this one) to finish.
    /// Nothing closes a gate, so the error is unreachable; it is a refusal, never a panic.
    pub(crate) async fn begin(key: PageKey) -> Result<MindTurn, String> {
        let read = custody(key.persona()).read_owned().await;
        if crate::capacity::window_grant::is_grant_retiring(key.persona()) {
            return Err("resident grant is transferring physical custody".into());
        }
        let permit = gate(key).acquire_owned().await.map_err(|_| format!("the act gate of {key} was closed"))?;
        if crate::capacity::window_grant::is_grant_retiring(key.persona()) {
            return Err("resident grant began transferring while queued".into());
        }
        Ok(MindTurn { _permit: permit, _custody: read })
    }

    /// All page users of this persona (including forks) drain before physical reclamation.
    pub(crate) async fn reclaim(persona: uuid::Uuid) -> tokio::sync::OwnedRwLockWriteGuard<()> {
        custody(persona).write_owned().await
    }

    /// The act now, or `None` when the mind is mid-act (best-effort callers such as
    /// warm-ahead: a mind mid-act already has her KV in a slot).
    pub(crate) fn try_begin(key: PageKey) -> Option<MindTurn> {
        let read = custody(key.persona()).try_read_owned().ok()?;
        if crate::capacity::window_grant::is_grant_retiring(key.persona()) { return None; }
        gate(key).try_acquire_owned().ok().map(|permit| MindTurn { _permit: permit, _custody: read })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn reclaim_drains_forks_and_bars_new_page_users_without_stopping_other_minds() {
        let persona = uuid::Uuid::new_v4();
        let fork = PageKey::fork(persona, uuid::Uuid::new_v4()).expect("fork");
        let active = MindTurn::begin(fork).await.expect("active fork");
        let reclaim = MindTurn::reclaim(persona);
        tokio::pin!(reclaim);
        assert!(tokio::time::timeout(std::time::Duration::from_millis(10), &mut reclaim).await.is_err());
        let other = PageKey::mind(uuid::Uuid::new_v4()).expect("other mind");
        assert!(MindTurn::try_begin(other).is_some(), "unrelated residents keep running");
        drop(active);
        let reclaimed = tokio::time::timeout(std::time::Duration::from_secs(1), reclaim).await.expect("writer progresses");
        assert!(MindTurn::try_begin(fork).is_none());
        assert!(MindTurn::try_begin(PageKey::mind(persona).expect("mind")).is_none());
        drop(reclaimed);
        assert!(MindTurn::try_begin(fork).is_some());
    }
}
