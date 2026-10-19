//! Durable per-grantee epoch watermark — the consumer-side anti-replay state for
//! capability grants.
//!
//! airc's `grid_auth` verifier is STATELESS by design: it checks a grant's
//! signature, key-binding, mesh, and expiry, but NOT whether a higher epoch has
//! already been accepted for the grantee. That anti-replay decision is the
//! consumer's responsibility (see [`grid_capability`](crate::routing::grid_capability)).
//! This module is that state, behind a trait so the policy depends on the SEAM:
//!
//! - [`InMemoryEpochWatermark`] — a `DashMap`, atomic per-grantee. The default for
//!   tests and any caller that hasn't opted into persistence.
//! - [`OrmEpochWatermark`] — durable, survives restart: one ORM entity row
//!   ([`GrantEpochWatermark`]) per grantee, no SQL outside the ORM.
//!
//! ## Why durable is a HARD GATE (adversarial review 2026-06-21)
//!
//! With an in-memory watermark, a node restart reopens the entire replay window:
//! a peer could re-present a grant the owner already SUPERSEDED (e.g. a revocation
//! raised the epoch, then the box restarted and forgot). The grid expects mundane
//! restarts, so the watermark MUST be durable before grants gate live traffic.
//! It must also be BOUNDED — the for-sale grid implies many transient grantees, so
//! [`evict_older_than`](EpochWatermarkStore::evict_older_than) drops entries no
//! live grant could still reference (expiry-aligned by `updated_at_ms`).
//!
//! Latest-epoch-authoritative (airc's model): any accepted epoch advances the
//! watermark, so a revocation — a higher-epoch grant with empty capabilities —
//! supersedes every older real-capability grant.

use std::sync::Arc;

use airc_core::PeerId;
use async_trait::async_trait;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::orm::query::{QueryBuilder, QueryOperator};
use crate::orm::{Entity, OrmEntity, OrmStore, OrmStoreError, SqliteAdapter, StorageAdapter};

/// The outcome of presenting `epoch` for a grantee against the watermark.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatermarkDecision {
    /// `epoch >= watermark`: the watermark advanced to `epoch`. The grant is current.
    Accepted,
    /// `epoch < watermark`: a replayed or superseded grant. Rejected.
    Superseded,
}

/// A watermark-store failure. Fail-CLOSED: the caller treats this as a deny, never
/// as "accept" — losing the anti-replay state must never open the replay window.
#[derive(Debug, Clone)]
pub struct WatermarkError(pub String);

impl std::fmt::Display for WatermarkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "epoch watermark store error: {}", self.0)
    }
}

impl std::error::Error for WatermarkError {}

/// Atomic, persistable per-grantee epoch high-water mark.
#[async_trait]
pub trait EpochWatermarkStore: Send + Sync + std::fmt::Debug {
    /// Atomically: if `epoch >= watermark(grantee)`, advance the watermark to
    /// `epoch` (stamping `now_ms`) and return [`Accepted`](WatermarkDecision::Accepted);
    /// otherwise leave it and return [`Superseded`](WatermarkDecision::Superseded).
    /// The check and the advance MUST be one critical section — a superseded epoch
    /// can never pass its check while a higher epoch commits in the gap.
    async fn check_and_advance(
        &self,
        grantee: PeerId,
        epoch: u64,
        now_ms: u64,
    ) -> Result<WatermarkDecision, WatermarkError>;

    /// Drop every entry whose last update is strictly older than `cutoff_ms` — the
    /// bound. Callers pass `now_ms - max_grant_lifetime_ms` so only entries no live
    /// grant could still reference are evicted. Returns the count removed.
    async fn evict_older_than(&self, cutoff_ms: u64) -> Result<usize, WatermarkError>;
}

/// In-memory watermark — atomic per-grantee via `DashMap::entry`. NOT durable; a
/// restart reopens the replay window. The default for tests and un-persisted use;
/// production gating live grant traffic MUST use [`OrmEpochWatermark`].
#[derive(Debug, Default)]
pub struct InMemoryEpochWatermark {
    /// grantee → (highest accepted epoch, last-update epoch-ms).
    seen: DashMap<PeerId, (u64, u64)>,
}

impl InMemoryEpochWatermark {
    pub fn new() -> Self {
        Self {
            seen: DashMap::new(),
        }
    }
}

#[async_trait]
impl EpochWatermarkStore for InMemoryEpochWatermark {
    async fn check_and_advance(
        &self,
        grantee: PeerId,
        epoch: u64,
        now_ms: u64,
    ) -> Result<WatermarkDecision, WatermarkError> {
        use dashmap::mapref::entry::Entry;
        // The check (epoch < watermark?) and the advance happen inside ONE entry
        // critical section, so a superseded epoch can never pass its check while a
        // higher epoch commits in the gap.
        match self.seen.entry(grantee) {
            Entry::Occupied(mut o) => {
                if epoch < o.get().0 {
                    return Ok(WatermarkDecision::Superseded);
                }
                *o.get_mut() = (epoch, now_ms);
            }
            Entry::Vacant(v) => {
                v.insert((epoch, now_ms));
            }
        }
        Ok(WatermarkDecision::Accepted)
    }

    async fn evict_older_than(&self, cutoff_ms: u64) -> Result<usize, WatermarkError> {
        let before = self.seen.len();
        self.seen.retain(|_, (_, updated)| *updated >= cutoff_ms);
        Ok(before - self.seen.len())
    }
}

/// One grantee's watermark row: the highest epoch accepted for them and when it was last
/// advanced. Keyed by the grantee, so a grantee has exactly one row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Entity)]
#[serde(rename_all = "camelCase")]
#[entity(collection = "grant_epoch_watermarks")]
pub struct GrantEpochWatermark {
    /// The grantee this watermark belongs to.
    #[entity(primary_key)]
    pub id: PeerId,
    /// The highest epoch accepted for the grantee.
    pub epoch: u64,
    /// When the watermark last advanced (epoch-ms); eviction keys on it.
    #[entity(indexed)]
    pub updated_at_ms: u64,
}

/// The largest epoch a watermark row stores exactly. A `Number` column is a double on
/// Postgres, exact only up to 2^53; above that two distinct epochs could read back as
/// one and a superseded grant could pass. Such an epoch is refused (fail closed), never
/// rounded.
const MAX_EXACT_EPOCH: u64 = 1 << 53;

/// Durable ORM-backed watermark — survives restart. One [`GrantEpochWatermark`] row per
/// grantee. The ORM has no compare-and-swap, so the read-then-write of
/// [`check_and_advance`](EpochWatermarkStore::check_and_advance) (and eviction, which
/// must not delete a row an accept just advanced) runs under one in-process lock: the
/// check and the advance are one critical section across concurrent callers.
pub struct OrmEpochWatermark {
    store: Mutex<OrmStore<GrantEpochWatermark>>,
}

impl std::fmt::Debug for OrmEpochWatermark {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OrmEpochWatermark")
            .field("collection", &GrantEpochWatermark::COLLECTION)
            .finish()
    }
}

impl OrmEpochWatermark {
    /// Open (creating if absent) the watermark database at `path` — the grant
    /// watermark of one airc home. Runs once at boot, not on the dispatch path.
    pub async fn open(path: &std::path::Path) -> Result<Self, WatermarkError> {
        let adapter = SqliteAdapter::open(path)
            .await
            .map_err(|e| WatermarkError(format!("open {}: {e}", path.display())))?;
        Self::over(Arc::new(adapter)).await
    }

    /// The watermark over an already-open ORM adapter.
    pub async fn over(adapter: Arc<dyn StorageAdapter>) -> Result<Self, WatermarkError> {
        let store = OrmStore::new(adapter).await.map_err(store_error)?;
        Ok(Self {
            store: Mutex::new(store),
        })
    }
}

fn store_error(e: OrmStoreError) -> WatermarkError {
    WatermarkError(e.to_string())
}

#[async_trait]
impl EpochWatermarkStore for OrmEpochWatermark {
    async fn check_and_advance(
        &self,
        grantee: PeerId,
        epoch: u64,
        now_ms: u64,
    ) -> Result<WatermarkDecision, WatermarkError> {
        if epoch > MAX_EXACT_EPOCH {
            return Err(WatermarkError(format!(
                "epoch {epoch} exceeds the exactly-stored range (2^53)"
            )));
        }
        let store = self.store.lock().await;
        let id = grantee.as_uuid();
        let row = GrantEpochWatermark {
            id: grantee,
            epoch,
            updated_at_ms: now_ms,
        };
        match store.find_by_id(id).await.map_err(store_error)? {
            Some(current) if epoch < current.epoch => return Ok(WatermarkDecision::Superseded),
            Some(_) => store.update(id, &row).await.map_err(store_error)?,
            None => store.save(id, &row).await.map_err(store_error)?,
        }
        Ok(WatermarkDecision::Accepted)
    }

    async fn evict_older_than(&self, cutoff_ms: u64) -> Result<usize, WatermarkError> {
        let store = self.store.lock().await;
        let stale = store
            .find_matching(
                QueryBuilder::new(GrantEpochWatermark::COLLECTION)
                    .filter("updatedAtMs", QueryOperator::Lt(cutoff_ms.into()))
                    .build(),
            )
            .await
            .map_err(store_error)?;
        store
            .delete_all(stale.iter().map(|(id, _)| *id))
            .await
            .map_err(store_error)?;
        Ok(stale.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peer(n: u128) -> PeerId {
        PeerId::from_uuid(uuid::Uuid::from_u128(n))
    }

    /// The durable store over a fresh ORM database; the `TempDir` owns the file.
    async fn orm_store() -> (OrmEpochWatermark, tempfile::TempDir) {
        let (adapter, dir) = crate::orm::store::fresh_adapter().await;
        (OrmEpochWatermark::over(adapter).await.expect("open"), dir)
    }

    // what this catches: the core anti-replay contract on BOTH impls — first epoch
    // accepted, a higher epoch accepted (advances), a lower epoch superseded, and a
    // revocation (higher epoch) supersedes the old one. Run against both the
    // in-memory and the REAL ORM path so they can't drift.
    async fn assert_anti_replay(store: &dyn EpochWatermarkStore) {
        let g = peer(1);
        assert_eq!(
            store.check_and_advance(g, 5, 100).await.unwrap(),
            WatermarkDecision::Accepted
        );
        // re-presenting the same epoch is still current (>=), accepted
        assert_eq!(
            store.check_and_advance(g, 5, 101).await.unwrap(),
            WatermarkDecision::Accepted
        );
        // lower epoch → superseded
        assert_eq!(
            store.check_and_advance(g, 3, 102).await.unwrap(),
            WatermarkDecision::Superseded
        );
        // higher epoch (e.g. a revocation) → accepted, advances
        assert_eq!(
            store.check_and_advance(g, 6, 103).await.unwrap(),
            WatermarkDecision::Accepted
        );
        // the old epoch is now superseded
        assert_eq!(
            store.check_and_advance(g, 5, 104).await.unwrap(),
            WatermarkDecision::Superseded
        );
        // a DIFFERENT grantee is independent
        assert_eq!(
            store.check_and_advance(peer(2), 1, 105).await.unwrap(),
            WatermarkDecision::Accepted
        );
    }

    #[tokio::test]
    async fn in_memory_enforces_anti_replay() {
        assert_anti_replay(&InMemoryEpochWatermark::new()).await;
    }

    #[tokio::test]
    async fn orm_enforces_anti_replay() {
        let (store, _dir) = orm_store().await;
        assert_anti_replay(&store).await;
    }

    // what this catches: durability — a watermark written by one store is seen by a
    // fresh store opening the SAME database file. This is the whole point of the hard
    // gate: a restart must NOT reopen the replay window.
    #[tokio::test]
    async fn orm_watermark_survives_reopen() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("grant_watermark.sqlite");
        let g = peer(7);
        {
            let store = OrmEpochWatermark::open(&path).await.expect("open");
            assert_eq!(
                store.check_and_advance(g, 9, 100).await.unwrap(),
                WatermarkDecision::Accepted
            );
        }
        // Reopen — simulating a node restart.
        let reopened = OrmEpochWatermark::open(&path).await.expect("reopen");
        assert_eq!(
            reopened.check_and_advance(g, 8, 200).await.unwrap(),
            WatermarkDecision::Superseded,
            "a superseded epoch stays superseded across restart — the replay window did NOT reopen"
        );
    }

    // what this catches: an epoch the ORM's Number column cannot hold exactly is
    // refused (fail closed), never rounded into a value a replay could slip under.
    #[tokio::test]
    async fn orm_refuses_an_epoch_it_cannot_store_exactly() {
        let (store, _dir) = orm_store().await;
        assert!(store.check_and_advance(peer(3), MAX_EXACT_EPOCH + 1, 100).await.is_err());
        assert_eq!(
            store.check_and_advance(peer(3), MAX_EXACT_EPOCH, 100).await.unwrap(),
            WatermarkDecision::Accepted
        );
    }

    // what this catches: the BOUND — eviction drops only entries older than the
    // cutoff, keeping live ones. Without this the store grows unbounded as transient
    // grantees come and go.
    #[tokio::test]
    async fn evicts_only_stale_entries() {
        let (orm, _dir) = orm_store().await;
        for store in [
            Box::new(InMemoryEpochWatermark::new()) as Box<dyn EpochWatermarkStore>,
            Box::new(orm),
        ] {
            store.check_and_advance(peer(1), 1, 1_000).await.unwrap(); // old
            store.check_and_advance(peer(2), 1, 5_000).await.unwrap(); // fresh
            let removed = store.evict_older_than(3_000).await.unwrap();
            assert_eq!(removed, 1, "only the stale entry is evicted");
            // the fresh grantee's watermark is intact (a replay is still superseded)
            assert_eq!(
                store.check_and_advance(peer(2), 0, 6_000).await.unwrap(),
                WatermarkDecision::Superseded
            );
            // the evicted grantee is forgotten (a fresh low epoch is accepted again)
            assert_eq!(
                store.check_and_advance(peer(1), 0, 6_000).await.unwrap(),
                WatermarkDecision::Accepted
            );
        }
    }

    // what this catches: under concurrent presentation of many epochs for the SAME
    // grantee, the store's critical section keeps the watermark monotonic — every
    // outcome is a clean Accepted/Superseded, the final watermark is the max epoch
    // presented, and it is still the max after a reopen. Without the lock two
    // read-then-write accepts interleave and a lower epoch overwrites a higher one.
    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    async fn concurrent_same_grantee_epochs_stay_monotonic() {
        const N: u64 = 64;
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("grant_watermark.sqlite");
        let store = Arc::new(OrmEpochWatermark::open(&path).await.expect("open"));
        let g = peer(42);
        // Interleave high and low epochs so a lost update would leave a low epoch.
        let order: Vec<u64> = (1..=N / 2).flat_map(|i| [N + 1 - i, i]).collect();
        let mut handles = Vec::new();
        for epoch in order {
            let store = Arc::clone(&store);
            handles.push(tokio::spawn(async move {
                store.check_and_advance(g, epoch, 100).await.expect("decision")
            }));
        }
        for h in handles {
            let out = h.await.expect("task");
            assert!(matches!(
                out,
                WatermarkDecision::Accepted | WatermarkDecision::Superseded
            ));
        }
        drop(store);
        let reopened = OrmEpochWatermark::open(&path).await.expect("reopen");
        assert_eq!(
            reopened.check_and_advance(g, N - 1, 200).await.unwrap(),
            WatermarkDecision::Superseded,
            "the max epoch is the durable watermark"
        );
        assert_eq!(
            reopened.check_and_advance(g, N, 201).await.unwrap(),
            WatermarkDecision::Accepted
        );
    }
}
