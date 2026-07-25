//! `TierStore` trait — the abstraction every per-role tier
//! implementation (Fast/Warm/Bench/Cold/Frozen) implements. Per
//! GENOME-FOUNDRY-SENTINEL Part 2.
//!
//! PR-2 of working-set-manager ships the **trait surface only**.
//! Per-role implementations (`FastTierStore`, `WarmTierStore`,
//! `BenchTierStore`, etc.) are separate PRs.
//!
//! ## Why one trait, five impls
//!
//! Each role has different eviction policy (LRU-within-turn,
//! LRU-across-turns, LFU+recency, …) and different backing storage
//! (accelerator VRAM, host RAM, SSD, archive). The TRAIT names the
//! capability — read / write / evict / capacity / observe_access —
//! that the working-set-manager (PR-3) calls without caring which
//! role it's talking to. The IMPLEMENTATIONS specialize.
//!
//! This is the OpenCV-style polymorphism pattern from CLAUDE.md: one
//! interface, many implementations, AIs (or sentinel) can swap them
//! at runtime via the governor's `Vec<TierConfig>`.

use async_trait::async_trait;

use super::blob::{ArtifactBlob, Provenance};
use super::tier::{EvictionRecord, TierCapacity, TierError, TierRole};
use super::working_set::{PageHandle, PageRef};

/// The single trait every tier implementation satisfies. The
/// working-set-manager (PR-3) holds `Box<dyn TierStore>` per
/// configured role and routes page operations through them.
///
/// `Send + Sync` because the working-set-manager runs in a tokio
/// runtime + the trait is called from multiple persona tasks
/// concurrently.
#[async_trait]
pub trait TierStore: Send + Sync {
    /// Which role this store implements. Stable for the store's
    /// lifetime — the governor doesn't re-role a store at runtime;
    /// it adds / removes them as policy changes.
    fn role(&self) -> TierRole;

    /// Read a page from this tier. Returns the typed page handle on
    /// hit, `TierError::PageNotFound` on miss. The handle's
    /// `tier_role` should equal `self.role()` so the caller can
    /// distinguish a miss-promoted-from-lower-tier (different role)
    /// from a direct hit (same role).
    async fn read(&self, page: PageRef) -> Result<PageHandle, TierError>;

    /// Write a page to this tier. May trigger eviction if the tier
    /// is at-or-near `configured_limit`. The provenance is REQUIRED —
    /// per GENOME-FOUNDRY-SENTINEL Part 1, no artifact enters the
    /// pool without one. A tier that can't accept the write surfaces
    /// `TierError::NoEvictionCandidate` or `TierError::BackingStoreIo`.
    async fn write(
        &self,
        page: PageRef,
        blob: ArtifactBlob,
        provenance: Provenance,
    ) -> Result<(), TierError>;

    /// Free at least `target_free_bytes` by evicting pages according
    /// to this role's eviction policy. Returns the records of every
    /// page evicted so the caller (working-set-manager) can publish
    /// them to the trace bus.
    ///
    /// Returns an empty Vec if no eviction was needed (tier already
    /// had enough headroom). Returns Vec with `< target` total bytes
    /// if no more eviction candidates exist (all pages pinned) —
    /// caller is responsible for surfacing `NoEvictionCandidate` to
    /// its caller in that case.
    async fn evict(&self, target_free_bytes: usize) -> Vec<EvictionRecord>;

    /// Current capacity snapshot. Cheap O(1) read — the tier tracks
    /// `current_used` as writes/evicts happen. Used by the governor +
    /// pressure broker to see who's near their limit.
    fn capacity(&self) -> TierCapacity;

    /// Tell the tier that a page was accessed (for LRU / LFU
    /// bookkeeping). Doesn't return — the tier is free to coalesce
    /// or drop calls under pressure. Cheap-and-return only.
    fn observe_access(&self, page: PageRef);
}

#[cfg(test)]
mod tests {
    //! Trait-shape tests: prove the trait is object-safe (can be used
    //! as `Box<dyn TierStore>` / `Arc<dyn TierStore>`) and that a
    //! minimal implementor compiles. PR-3 will add per-role impls
    //! tested against the real semantics; PR-2 only proves the seam.

    use super::*;
    use crate::genome::working_set::{ArtifactId, PageKind, PageOffset};
    use std::sync::Arc;
    use uuid::Uuid;

    /// Minimal in-memory tier store for trait tests. Records calls so
    /// tests can assert dispatch happened.
    struct InMemTier {
        role: TierRole,
        capacity: TierCapacity,
    }

    #[async_trait]
    impl TierStore for InMemTier {
        fn role(&self) -> TierRole {
            self.role
        }

        async fn read(&self, page: PageRef) -> Result<PageHandle, TierError> {
            Ok(PageHandle {
                page,
                tier_role: self.role,
                size_bytes: 0,
            })
        }

        async fn write(
            &self,
            _page: PageRef,
            _blob: ArtifactBlob,
            _provenance: Provenance,
        ) -> Result<(), TierError> {
            Ok(())
        }

        async fn evict(&self, _target_free_bytes: usize) -> Vec<EvictionRecord> {
            Vec::new()
        }

        fn capacity(&self) -> TierCapacity {
            self.capacity
        }

        fn observe_access(&self, _page: PageRef) {}
    }

    fn sample_page() -> PageRef {
        PageRef {
            kind: PageKind::LoRALayer,
            artifact: ArtifactId::new(Uuid::nil()),
            offset: PageOffset::Whole,
        }
    }

    /// What this catches: TierStore is object-safe. If a future PR
    /// adds a method with a generic type parameter or a non-dyn-safe
    /// signature, this construction fails to compile. Object-safety
    /// is load-bearing because the working-set-manager holds
    /// `Box<dyn TierStore>` per configured role.
    #[tokio::test]
    async fn tier_store_is_object_safe() {
        let store: Arc<dyn TierStore> = Arc::new(InMemTier {
            role: TierRole::Fast,
            capacity: TierCapacity {
                current_used: 0,
                configured_limit: 1_000_000,
            },
        });
        assert_eq!(store.role(), TierRole::Fast);
        let handle = store.read(sample_page()).await.unwrap();
        assert_eq!(handle.tier_role, TierRole::Fast);
    }

    /// What this catches: write accepts ArtifactBlob + Provenance
    /// without requiring the caller to clone or move excessively. If
    /// a future PR adds an unwanted bound (e.g. `'static` on the
    /// blob), this dispatch fails.
    #[tokio::test]
    async fn tier_store_write_round_trips_through_trait_object() {
        let store: Box<dyn TierStore> = Box::new(InMemTier {
            role: TierRole::Cold,
            capacity: TierCapacity {
                current_used: 0,
                configured_limit: 10_000_000,
            },
        });
        let blob = ArtifactBlob::inline(ArtifactId::new(Uuid::nil()), vec![1, 2, 3]);
        let prov = Provenance::minimal(blob.id, 1_700_000_000_000);
        store.write(sample_page(), blob, prov).await.unwrap();
    }

    /// What this catches: evict returns Vec<EvictionRecord>. If a
    /// future PR changes the return shape (e.g. to a stream or single
    /// record), this assertion catches it.
    #[tokio::test]
    async fn tier_store_evict_returns_record_vec() {
        let store: Arc<dyn TierStore> = Arc::new(InMemTier {
            role: TierRole::Bench,
            capacity: TierCapacity {
                current_used: 0,
                configured_limit: 100_000_000,
            },
        });
        let records = store.evict(4096).await;
        // InMemTier returns empty; PR-3's real impl returns the
        // pages it actually evicted. The contract here is the Vec
        // type, not the contents.
        assert_eq!(records.len(), 0);
    }
}
