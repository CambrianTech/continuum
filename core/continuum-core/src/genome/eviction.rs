//! Pure tier-eviction page ranking.
//!
//! `TierStore::evict` (genome/store.rs) must drop resident pages under
//! memory pressure, but the trait defines no policy — each role runs its
//! canonical [`EvictionPolicy`](crate::genome::tier::EvictionPolicy)
//! (tier.rs). This is the **single definition** of "under THIS policy, which
//! pages are least valuable to keep?", mirroring the
//! `select_leases_to_revoke` classifier the paging layer uses: a pure,
//! snapshot-in / order-out decision so every role-specific store delegates
//! to ONE policy implementation instead of re-encoding the ordering inline.
//!
//! Keeping the decision pure means it is unit-testable in microseconds and
//! a `TierStore::evict` stays a thin "walk the order, drop until enough
//! bytes are freed" loop with no policy logic of its own.

use crate::genome::tier::EvictionPolicy;
use crate::genome::working_set::{PageRef, ResidentPage};

/// Rank `pages` by eviction priority under `policy` — least-valuable-to-keep
/// FIRST. Pure: no I/O, no mutation. Pinned pages are excluded entirely (the
/// composition layer protects them from mid-turn eviction). The caller (a
/// `TierStore::evict`) walks the returned order, dropping pages until it has
/// freed enough bytes.
///
/// Per-policy ordering, by the signal [`ResidentPage`] carries
/// (`last_access_ms`, `access_count_window`):
/// - `LruWithinTurn` / `LruAcrossTurns` — oldest `last_access_ms` first. (The
///   `LruAcrossTurns` window is a manager-side retention concern; for
///   ordering both are plain LRU.)
/// - `LfuPlusRecency` — fewest `access_count_window` first, oldest
///   `last_access_ms` as the recency tiebreak.
/// - `DemandAlignedWithRefinedPreference` — least-demanded
///   (`access_count_window`) first, sharing `LfuPlusRecency`'s
///   ResidentPage-level order. Its distinguishing "prefer evicting imported
///   pages over sentinel-refined pages of equal demand" needs blob
///   provenance, which `ResidentPage` does not carry — the `TierStore`
///   layers that preference on top of this order from its own provenance.
/// - `AppendOnlyGcOnSleep` — empty: the Frozen tier never evicts on the hot
///   path (GC happens opportunistically during sleep).
///
/// Deterministic for a given input ordering (the sort is stable); exact ties
/// (equal keys) follow input order. A caller needing cross-run determinism
/// passes a pre-sorted slice.
pub fn rank_pages_for_eviction(pages: &[ResidentPage], policy: &EvictionPolicy) -> Vec<PageRef> {
    let mut candidates: Vec<&ResidentPage> = pages.iter().filter(|p| !p.pinned).collect();

    match policy {
        // Frozen tier: never evicts on the hot path. The pinned filter above
        // is computed-then-discarded (negligible) so the policy arms read
        // uniformly.
        EvictionPolicy::AppendOnlyGcOnSleep => return Vec::new(),

        EvictionPolicy::LruWithinTurn | EvictionPolicy::LruAcrossTurns { .. } => {
            candidates.sort_by(|a, b| a.last_access_ms.cmp(&b.last_access_ms));
        }

        // Least-used / least-demanded first; oldest access breaks ties.
        // `DemandAlignedWithRefinedPreference` shares this order at the
        // ResidentPage level (see fn doc); the refined-vs-imported preference
        // is layered on by the TierStore.
        EvictionPolicy::LfuPlusRecency
        | EvictionPolicy::DemandAlignedWithRefinedPreference => {
            candidates.sort_by(|a, b| {
                a.access_count_window
                    .cmp(&b.access_count_window)
                    .then(a.last_access_ms.cmp(&b.last_access_ms))
            });
        }
    }

    candidates.into_iter().map(|p| p.page).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::tier::TierRole;
    use crate::genome::working_set::{ArtifactId, PageKind, PageOffset, PageRef};
    use uuid::Uuid;

    /// Build a resident page with a distinct `PageRef` (keyed off `tag`) and
    /// the given eviction-relevant metadata.
    fn page(tag: u128, last_access_ms: u64, access_count_window: u32, pinned: bool) -> ResidentPage {
        ResidentPage {
            page: PageRef {
                kind: PageKind::LoRALayer,
                artifact: ArtifactId::new(Uuid::from_u128(tag)),
                offset: PageOffset::Whole,
            },
            role: TierRole::Fast,
            last_access_ms,
            access_count_window,
            pinned,
        }
    }

    fn ref_of(tag: u128) -> PageRef {
        PageRef {
            kind: PageKind::LoRALayer,
            artifact: ArtifactId::new(Uuid::from_u128(tag)),
            offset: PageOffset::Whole,
        }
    }

    /// what this catches: LRU must order oldest-access first. Sorting by the
    /// wrong key (or descending) would evict the freshest page — the exact
    /// opposite of LRU.
    #[test]
    fn lru_orders_oldest_access_first() {
        // tag 1 newest, tag 3 oldest.
        let pages = [
            page(1, 300, 9, false),
            page(2, 200, 9, false),
            page(3, 100, 9, false),
        ];
        for policy in [
            EvictionPolicy::LruWithinTurn,
            EvictionPolicy::LruAcrossTurns { window_turns: 100 },
        ] {
            let order = rank_pages_for_eviction(&pages, &policy);
            assert_eq!(order, vec![ref_of(3), ref_of(2), ref_of(1)], "{policy:?}");
        }
    }

    /// what this catches: LFU must order least-frequently-used first, using
    /// recency (oldest access) only to break frequency ties. Ordering by
    /// recency alone (ignoring frequency) would be plain LRU, not LFU.
    #[test]
    fn lfu_orders_least_used_first_then_oldest() {
        let pages = [
            page(1, 100, 5, false), // most used → kept longest
            page(2, 300, 1, false), // least used, fresh
            page(3, 200, 1, false), // least used, older → evict before tag 2
        ];
        let order = rank_pages_for_eviction(&pages, &EvictionPolicy::LfuPlusRecency);
        // freq 1 group first (older-recency first within it), then freq 5.
        assert_eq!(order, vec![ref_of(3), ref_of(2), ref_of(1)]);
    }

    /// what this catches: DemandAligned shares LFU's ResidentPage-level order
    /// (least-demanded first). If the two policies diverged here without the
    /// provenance layer that justifies it, this pins the shared contract.
    #[test]
    fn demand_aligned_shares_lfu_order_at_residentpage_level() {
        let pages = [
            page(1, 100, 5, false),
            page(2, 300, 1, false),
            page(3, 200, 1, false),
        ];
        let lfu = rank_pages_for_eviction(&pages, &EvictionPolicy::LfuPlusRecency);
        let demand = rank_pages_for_eviction(
            &pages,
            &EvictionPolicy::DemandAlignedWithRefinedPreference,
        );
        assert_eq!(lfu, demand);
    }

    /// what this catches: a pinned page must NEVER appear in the ranking,
    /// under any policy. A pinned page leaking into the evict order is the
    /// mid-turn-eviction-of-in-use-page failure the pin flag exists to stop.
    #[test]
    fn pinned_pages_are_never_ranked() {
        let pages = [
            page(1, 100, 1, true), // pinned — oldest + least-used, yet protected
            page(2, 200, 2, false),
        ];
        for policy in [
            EvictionPolicy::LruWithinTurn,
            EvictionPolicy::LfuPlusRecency,
            EvictionPolicy::DemandAlignedWithRefinedPreference,
            EvictionPolicy::LruAcrossTurns { window_turns: 4 },
        ] {
            let order = rank_pages_for_eviction(&pages, &policy);
            assert_eq!(order, vec![ref_of(2)], "{policy:?} must skip the pinned page");
        }
    }

    /// what this catches: the Frozen tier's append-only policy never evicts
    /// on the hot path — it returns empty even with many unpinned pages.
    /// Returning a ranking here would let pressure drop frozen provenance.
    #[test]
    fn frozen_append_only_never_evicts() {
        let pages = [page(1, 100, 1, false), page(2, 200, 2, false)];
        let order = rank_pages_for_eviction(&pages, &EvictionPolicy::AppendOnlyGcOnSleep);
        assert!(order.is_empty());
    }

    /// what this catches: empty input is a clean no-op (no panic, empty out)
    /// across policies — the broker can call evict on an empty tier.
    #[test]
    fn empty_input_returns_empty() {
        for policy in [
            EvictionPolicy::LruWithinTurn,
            EvictionPolicy::LfuPlusRecency,
            EvictionPolicy::AppendOnlyGcOnSleep,
        ] {
            assert!(rank_pages_for_eviction(&[], &policy).is_empty(), "{policy:?}");
        }
    }
}
