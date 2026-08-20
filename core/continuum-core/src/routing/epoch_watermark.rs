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
//! - [`SqliteEpochWatermark`] — durable, survives restart.
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
use rusqlite::{params, Connection};
use tokio::sync::Mutex;

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
/// production gating live grant traffic MUST use [`SqliteEpochWatermark`].
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

/// Durable SQLite-backed watermark — survives restart. One row per grantee in a
/// dedicated table; the check-and-advance runs in a write transaction on a single
/// serialized writer connection, so it is atomic across concurrent callers. All
/// blocking rusqlite work runs on `spawn_blocking` (off the async executor), per
/// the substrate concurrency style.
#[derive(Debug)]
pub struct SqliteEpochWatermark {
    writer: Arc<Mutex<Connection>>,
}

impl SqliteEpochWatermark {
    /// The table backing the watermark. Self-contained (not the generic JSON ORM)
    /// because this is a typed, hot, security-critical row.
    const TABLE_DDL: &'static str = "CREATE TABLE IF NOT EXISTS grant_epoch_watermark (\
         grantee TEXT PRIMARY KEY, \
         epoch INTEGER NOT NULL, \
         updated_at_ms INTEGER NOT NULL\
     )";

    /// Open (creating if absent) the watermark database at `path`. WAL mode so a
    /// background eviction reader never blocks the writer. Synchronous open is
    /// fine — it runs once at boot, not on the dispatch path.
    pub fn open(path: &std::path::Path) -> Result<Self, WatermarkError> {
        let conn = Connection::open(path)
            .map_err(|e| WatermarkError(format!("open {}: {e}", path.display())))?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")
            .map_err(|e| WatermarkError(format!("set pragmas: {e}")))?;
        conn.execute(Self::TABLE_DDL, [])
            .map_err(|e| WatermarkError(format!("create table: {e}")))?;
        Ok(Self {
            writer: Arc::new(Mutex::new(conn)),
        })
    }

    /// In-memory SQLite (`:memory:`) — for tests that want the REAL transaction
    /// path without a tempfile.
    pub fn in_memory() -> Result<Self, WatermarkError> {
        let conn = Connection::open_in_memory()
            .map_err(|e| WatermarkError(format!("open in-memory: {e}")))?;
        conn.execute(Self::TABLE_DDL, [])
            .map_err(|e| WatermarkError(format!("create table: {e}")))?;
        Ok(Self {
            writer: Arc::new(Mutex::new(conn)),
        })
    }
}

#[async_trait]
impl EpochWatermarkStore for SqliteEpochWatermark {
    async fn check_and_advance(
        &self,
        grantee: PeerId,
        epoch: u64,
        now_ms: u64,
    ) -> Result<WatermarkDecision, WatermarkError> {
        let writer = Arc::clone(&self.writer);
        let key = grantee.0.to_string();
        // rusqlite is blocking; run the transaction off the async executor.
        let handle = tokio::task::spawn_blocking(move || -> Result<WatermarkDecision, String> {
            // The single writer Mutex serializes transactions, so the
            // SELECT-then-UPSERT below is atomic across concurrent callers.
            let mut conn = writer.blocking_lock();
            let tx = conn.transaction().map_err(|e| e.to_string())?;
            let current: Option<i64> = tx
                .query_row(
                    "SELECT epoch FROM grant_epoch_watermark WHERE grantee = ?1",
                    params![key],
                    |row| row.get(0),
                )
                .map_err(|e| match e {
                    rusqlite::Error::QueryReturnedNoRows => "no-row".to_string(),
                    other => other.to_string(),
                })
                .ok();
            // SQLite INTEGER is i64; epochs are u64 but monotonic counters that
            // never approach i64::MAX in practice. Compare as u64 after a checked
            // cast so a corrupt negative row reads as 0 (fail-forward to overwrite),
            // never panics.
            let current_epoch = current.and_then(|v| u64::try_from(v).ok()).unwrap_or(0);
            if current.is_some() && epoch < current_epoch {
                return Ok(WatermarkDecision::Superseded);
            }
            let epoch_i64 =
                i64::try_from(epoch).map_err(|_| format!("epoch {epoch} exceeds i64 range"))?;
            let now_i64 =
                i64::try_from(now_ms).map_err(|_| format!("now_ms {now_ms} exceeds i64 range"))?;
            tx.execute(
                "INSERT INTO grant_epoch_watermark (grantee, epoch, updated_at_ms) \
                 VALUES (?1, ?2, ?3) \
                 ON CONFLICT(grantee) DO UPDATE SET epoch = excluded.epoch, \
                 updated_at_ms = excluded.updated_at_ms",
                params![key, epoch_i64, now_i64],
            )
            .map_err(|e| e.to_string())?;
            tx.commit().map_err(|e| e.to_string())?;
            Ok(WatermarkDecision::Accepted)
        });
        handle
            .await
            .map_err(|e| WatermarkError(format!("join: {e}")))?
            .map_err(WatermarkError)
    }

    async fn evict_older_than(&self, cutoff_ms: u64) -> Result<usize, WatermarkError> {
        let writer = Arc::clone(&self.writer);
        let handle = tokio::task::spawn_blocking(move || -> Result<usize, String> {
            let cutoff_i64 = i64::try_from(cutoff_ms).unwrap_or(i64::MAX);
            let conn = writer.blocking_lock();
            let removed = conn
                .execute(
                    "DELETE FROM grant_epoch_watermark WHERE updated_at_ms < ?1",
                    params![cutoff_i64],
                )
                .map_err(|e| e.to_string())?;
            Ok(removed)
        });
        handle
            .await
            .map_err(|e| WatermarkError(format!("join: {e}")))?
            .map_err(WatermarkError)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peer(n: u128) -> PeerId {
        PeerId::from_uuid(uuid::Uuid::from_u128(n))
    }

    // what this catches: the core anti-replay contract on BOTH impls — first epoch
    // accepted, a higher epoch accepted (advances), a lower epoch superseded, and a
    // revocation (higher epoch) supersedes the old one. Run against both the
    // in-memory and the REAL SQLite transaction path so they can't drift.
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
    async fn sqlite_enforces_anti_replay() {
        assert_anti_replay(&SqliteEpochWatermark::in_memory().expect("open")).await;
    }

    // what this catches: durability — a watermark written by one SQLite handle is
    // seen by a fresh handle opening the SAME file. This is the whole point of the
    // hard gate: a restart must NOT reopen the replay window.
    #[tokio::test]
    async fn sqlite_watermark_survives_reopen() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("watermark.sqlite");
        let g = peer(7);
        {
            let store = SqliteEpochWatermark::open(&path).expect("open");
            assert_eq!(
                store.check_and_advance(g, 9, 100).await.unwrap(),
                WatermarkDecision::Accepted
            );
        }
        // Reopen — simulating a node restart.
        let reopened = SqliteEpochWatermark::open(&path).expect("reopen");
        assert_eq!(
            reopened.check_and_advance(g, 8, 200).await.unwrap(),
            WatermarkDecision::Superseded,
            "a superseded epoch stays superseded across restart — the replay window did NOT reopen"
        );
    }

    // what this catches: the BOUND — eviction drops only entries older than the
    // cutoff, keeping live ones. Without this the store grows unbounded as transient
    // grantees come and go.
    #[tokio::test]
    async fn evicts_only_stale_entries() {
        for store in [
            Box::new(InMemoryEpochWatermark::new()) as Box<dyn EpochWatermarkStore>,
            Box::new(SqliteEpochWatermark::in_memory().expect("open")),
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

    /// Concurrency proof for the atomic check-and-advance on the REAL SQLite path.
    /// Gated behind `stress-tests` per the test doctrine.
    #[cfg(feature = "stress-tests")]
    mod stress {
        use super::*;

        // what this catches: under heavy concurrent presentation of many epochs for
        // the SAME grantee, the serialized writer transaction keeps the watermark
        // monotonic — every outcome is a clean Accepted/Superseded and the final
        // watermark equals the max epoch presented.
        #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
        async fn concurrent_same_grantee_epochs_stay_monotonic() {
            const N: u64 = 200;
            let store = Arc::new(SqliteEpochWatermark::in_memory().expect("open"));
            let g = peer(42);
            let mut handles = Vec::new();
            for epoch in 1..=N {
                let store = Arc::clone(&store);
                handles.push(tokio::spawn(async move {
                    let out = store.check_and_advance(g, epoch, 100).await.unwrap();
                    assert!(matches!(
                        out,
                        WatermarkDecision::Accepted | WatermarkDecision::Superseded
                    ));
                }));
            }
            for h in handles {
                h.await.expect("task");
            }
            // The max epoch is now the watermark: a replay below it is superseded,
            // and re-presenting the max is still accepted.
            assert_eq!(
                store.check_and_advance(g, N - 1, 200).await.unwrap(),
                WatermarkDecision::Superseded
            );
            assert_eq!(
                store.check_and_advance(g, N, 201).await.unwrap(),
                WatermarkDecision::Accepted
            );
        }
    }
}
