//! The cache manager — reconcile a demand plan against finite misfit-hardware disk.
//!
//! The artifact store is a CACHE ([[resource-authority-is-a-system-concern]]): the
//! currently-needed set (active personas' + lanes' models/avatars/voices) is PINNED
//! and guaranteed present ("we need what we need"), everything else is evictable. When
//! the pinned set + the disk already in use exceeds the budget, evict unpinned cache;
//! when even the pinned set alone won't fit, that's a hard truth about this machine —
//! report the shortfall, don't silently thrash ([[fallbacks-are-illegal-fail-loud]],
//! [[model-fit-is-the-priority-single-machine-first]]).
//!
//! Pure logic over `DiskState` (slice 1's primitive) — the `Downloader` (fetch) and
//! the eviction I/O (delete) are later slices that ACT on this decision.

use super::DiskState;

/// What the currently-active personas + lanes require to be on disk. These ids are
/// PINNED — never evicted, always fetched if absent.
#[derive(Debug, Clone, Default)]
pub struct ProvisionPlan {
    pub needed: Vec<String>,
}

/// One artifact's cache-relevant state: is it on disk (+ how big), and is it pinned by
/// the current plan. Callers build these from the `ArtifactSource`s × the plan.
#[derive(Debug, Clone)]
pub struct CacheEntry {
    pub id: String,
    pub disk: DiskState,
    pub pinned: bool,
}

impl CacheEntry {
    pub fn new(id: impl Into<String>, disk: DiskState, pinned: bool) -> Self {
        Self {
            id: id.into(),
            disk,
            pinned,
        }
    }
}

/// The reconcile outcome — what to fetch, what to evict, and whether the pinned set
/// simply won't fit (a fail-loud signal, not a fallback).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheDecision {
    /// Pinned artifacts not yet on disk — the provisioner must download these.
    pub fetch: Vec<String>,
    /// Unpinned cached artifacts to evict (largest-first) to fit the budget.
    pub evict: Vec<String>,
    /// Bytes by which the PINNED set exceeds the budget even after evicting ALL
    /// unpinned cache. `0` = it fits; `>0` = fail loud (too many big artifacts for
    /// this disk — name it).
    pub shortfall_bytes: u64,
}

impl CacheDecision {
    /// True when the pinned/needed set cannot fit on this machine's disk budget.
    pub fn is_shortfall(&self) -> bool {
        self.shortfall_bytes > 0
    }
}

/// Reconcile the cache: keep the pinned set, evict unpinned (largest-first) to fit
/// `budget_bytes`, fetch pinned-but-absent, and report any shortfall. Pure — no I/O.
pub fn reconcile(entries: &[CacheEntry], budget_bytes: u64) -> CacheDecision {
    let mut fetch = Vec::new();
    for e in entries {
        if e.pinned && !e.disk.is_present() {
            fetch.push(e.id.clone());
        }
    }

    // Bytes currently on disk (everything present, pinned or not).
    let mut used: u64 = entries
        .iter()
        .filter(|e| e.disk.is_present())
        .map(|e| e.disk.bytes())
        .sum();

    // Evict unpinned present artifacts, LARGEST first — frees the most space with the
    // fewest evictions. (Access-time LRU is a refinement once we track last-use.)
    let mut unpinned: Vec<&CacheEntry> = entries
        .iter()
        .filter(|e| !e.pinned && e.disk.is_present())
        .collect();
    unpinned.sort_by(|a, b| b.disk.bytes().cmp(&a.disk.bytes()).then(a.id.cmp(&b.id)));

    let mut evict = Vec::new();
    for e in unpinned {
        if used <= budget_bytes {
            break;
        }
        evict.push(e.id.clone());
        used -= e.disk.bytes();
    }

    // Whatever's still on disk after evicting all we could is the pinned set (we never
    // evict pinned). If that still exceeds the budget, it's a hard shortfall.
    let shortfall_bytes = used.saturating_sub(budget_bytes);

    CacheDecision {
        fetch,
        evict,
        shortfall_bytes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn present(bytes: u64) -> DiskState {
        DiskState::Present {
            path: PathBuf::from("/x"),
            bytes,
        }
    }

    fn entry(id: &str, disk: DiskState, pinned: bool) -> CacheEntry {
        CacheEntry::new(id, disk, pinned)
    }

    // what this catches: under budget, nothing is touched — no evict, no fetch, no
    // shortfall.
    #[test]
    fn under_budget_is_a_noop() {
        let e = vec![
            entry("a", present(10), true),
            entry("b", present(20), false),
        ];
        let d = reconcile(&e, 100);
        assert!(d.evict.is_empty() && d.fetch.is_empty() && !d.is_shortfall());
    }

    // what this catches: over budget, UNPINNED cache is evicted largest-first to fit,
    // and PINNED artifacts are never evicted.
    #[test]
    fn evicts_unpinned_largest_first_never_pinned() {
        let e = vec![
            entry("pinned-model", present(50), true),
            entry("cache-big", present(40), false),
            entry("cache-small", present(20), false),
        ];
        // used=110, budget=70 → must free 40. Largest unpinned (cache-big, 40) does it.
        let d = reconcile(&e, 70);
        assert_eq!(d.evict, vec!["cache-big".to_string()]);
        assert!(
            !d.is_shortfall(),
            "70 fits the 50 pinned + 20 small after eviction"
        );
        assert!(!d.evict.contains(&"pinned-model".to_string()));
    }

    // what this catches: pinned-but-absent artifacts land in `fetch`.
    #[test]
    fn needed_absent_is_fetched() {
        let e = vec![
            entry("have-it", present(10), true),
            entry("need-it", DiskState::Absent, true),
        ];
        let d = reconcile(&e, 100);
        assert_eq!(d.fetch, vec!["need-it".to_string()]);
    }

    // what this catches: the FAIL-LOUD case — the pinned set alone exceeds the budget
    // even after evicting all unpinned cache. shortfall names the byte deficit.
    #[test]
    fn pinned_set_over_budget_is_a_shortfall() {
        let e = vec![
            entry("big-brain-1", present(60), true),
            entry("big-brain-2", present(60), true),
            entry("evictable", present(30), false),
        ];
        // Evict the 30 → 120 pinned left, budget 100 → 20 short. Fail loud, don't thrash.
        let d = reconcile(&e, 100);
        assert_eq!(d.evict, vec!["evictable".to_string()]);
        assert_eq!(d.shortfall_bytes, 20);
        assert!(d.is_shortfall());
    }
}
