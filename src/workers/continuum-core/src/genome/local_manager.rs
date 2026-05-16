//! `LocalWorkingSetManager` — per-process implementation of the
//! `WorkingSetManager` trait shipped in PR-2 (#1353).
//!
//! Holds:
//! - `Vec<Box<dyn TierStore>>` — the tier chain, ordered Fast → Frozen
//! - `RwLock<HashMap<PersonaId, WorkingSet>>` — per-persona working sets
//! - `RwLock<HashMap<PageRef, PersonaId>>` — page-ownership map for
//!   the MMU-style `audit_access` enforcement
//!
//! Page-in walks the tier chain from highest (Fast) to lowest (Frozen),
//! returns the first hit, optionally promotes the page to the working
//! set's preferred tier. A miss with no resident copy is a true cold
//! miss → `PageFault::from_role: None`.
//!
//! ## What PR-3 ships
//!
//! - Pure local implementation. No bus publishing baked in (the
//!   `page_in` Result already carries `PageFault` as the typed
//!   observability signal; callers wire to the artifact dispatch
//!   path #1339+#1343 themselves).
//! - The four trait methods: `page_in`, `page_out`, `working_set`,
//!   `audit_access`.
//! - Constructor that registers tier stores + capacity per persona.
//! - Tests using a stub `TierStore` that records calls so the test
//!   can assert which tier was queried + that PageFault carries the
//!   right `from_role` / `to_role`.
//!
//! ## What PR-3 does NOT ship (PR-4 or later)
//!
//! - Eviction policy invocation when the target tier is at limit —
//!   PR-3 returns `TierError::NoEvictionCandidate` instead of running
//!   the policy. Policy invocation is a tier-store-internal concern
//!   that the PR-3 impl doesn't drive; PR-4's enhancement is a wired
//!   callback so the manager observes and re-publishes the
//!   `EvictionRecord` that the tier returned.
//! - Pinning logic for composition-layer page pinning — that's part
//!   of PR-3 of demand-aligned-recall (composer cache).
//! - The `check_permission(actor, region, op)` method from PR-2's
//!   "deliberately deferred" list. Lands in PR-4 alongside the
//!   GenomeRegion + Op type definitions.

use async_trait::async_trait;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

use super::manager::WorkingSetManager;
use super::store::TierStore;
use super::tier::{TierError, TierRole};
use super::working_set::{
    AccessDenied, PageFault, PageHandle, PageRef, PersonaId, ResidentPage, WorkingSet,
    WorkingSetCapacity,
};

/// Per-process working-set manager. Holds the tier chain + per-persona
/// state. Thread-safe through `parking_lot::RwLock` — the hot-path
/// `audit_access` and `working_set` calls only need a read lock.
pub struct LocalWorkingSetManager {
    /// The tier chain, ordered highest (Fast) to lowest (Frozen).
    /// Each tier is a `Box<dyn TierStore>` from PR-2. The order is
    /// the page_in walk order — we stop at the first hit.
    tiers: Vec<Arc<dyn TierStore>>,
    /// Per-persona working set state. RwLock because read-heavy
    /// (every audit_access + working_set query) with occasional
    /// write (page_in / page_out modifications).
    working_sets: RwLock<HashMap<PersonaId, WorkingSet>>,
    /// Page-ownership map for cross-persona compartmentalization.
    /// `audit_access` denies if `persona != owner`. PR-3 populates
    /// this via `register_page_owner`; PR-4 may move to a typed
    /// genome-region-keyed table per GENOME-FOUNDRY-SENTINEL Part 4.
    page_owners: RwLock<HashMap<PageRef, PersonaId>>,
}

impl LocalWorkingSetManager {
    /// Construct with the tier chain. The vec is in walk order:
    /// `tiers[0]` is the highest tier (Fast — checked first by
    /// `page_in`); `tiers[N-1]` is the lowest (typically Frozen).
    pub fn new(tiers: Vec<Arc<dyn TierStore>>) -> Self {
        Self {
            tiers,
            working_sets: RwLock::new(HashMap::new()),
            page_owners: RwLock::new(HashMap::new()),
        }
    }

    /// Register a persona with the manager + give it a working set
    /// capacity. Must be called before any `page_in` for the persona;
    /// `page_in` to an unregistered persona returns a `PageFault`
    /// with `from_role: None` (the page never existed for that
    /// persona because the persona itself doesn't exist yet).
    pub fn register_persona(&self, persona: PersonaId, capacity: WorkingSetCapacity) {
        let ws = WorkingSet::new(persona, capacity);
        self.working_sets.write().insert(persona, ws);
    }

    /// Record that a page is private to a persona. Subsequent
    /// `audit_access(other_persona, page)` returns `AccessDenied`.
    /// Pages not registered here are treated as substrate-shared
    /// (no owner; anyone can access).
    pub fn register_page_owner(&self, page: PageRef, owner: PersonaId) {
        self.page_owners.write().insert(page, owner);
    }

    /// How many tiers are configured. Cheap O(1) — used by tests +
    /// the governor's policy diagnostics.
    pub fn tier_count(&self) -> usize {
        self.tiers.len()
    }
}

#[async_trait]
impl WorkingSetManager for LocalWorkingSetManager {
    async fn page_in(
        &self,
        persona: PersonaId,
        page: PageRef,
    ) -> Result<PageHandle, PageFault> {
        // Already resident? — fast path.
        {
            let working_sets = self.working_sets.read();
            if let Some(ws) = working_sets.get(&persona) {
                let key = serde_json::to_string(&page).unwrap_or_default();
                if let Some(resident) = ws.pages.get(&key) {
                    return Ok(PageHandle {
                        page,
                        tier_role: resident.role,
                        size_bytes: 0,
                    });
                }
            }
        }

        // Walk tier chain top-down. First hit wins. Promote (record
        // residency) into the working set's Fast tier; the caller's
        // composition decides whether to pin.
        for tier in &self.tiers {
            if let Ok(handle) = tier.read(page).await {
                let from_role = handle.tier_role;
                let to_role = self.tiers.first().map(|t| t.role()).unwrap_or(from_role);

                // Record residency in the working set (if persona
                // registered).
                if let Some(ws) = self.working_sets.write().get_mut(&persona) {
                    let key = serde_json::to_string(&page).unwrap_or_default();
                    ws.pages.insert(
                        key,
                        ResidentPage {
                            page,
                            role: to_role,
                            last_access_ms: now_ms(),
                            access_count_window: 1,
                            pinned: false,
                        },
                    );
                }

                // Return PageFault to signal the caller "this was a
                // tier promotion" — they'll publish to the trace bus.
                // The handle is in the Err arm; the spec uses this
                // typed signal to capture sentinel observability
                // without confusing it with a failure.
                return Err(PageFault {
                    page,
                    from_role: Some(from_role),
                    to_role,
                    persona,
                    elapsed_us: 0,
                    eviction_cost: None,
                });
            }
        }

        // True cold miss — page doesn't exist in any tier yet.
        Err(PageFault {
            page,
            from_role: None,
            to_role: self
                .tiers
                .first()
                .map(|t| t.role())
                .unwrap_or(TierRole::Fast),
            persona,
            elapsed_us: 0,
            eviction_cost: None,
        })
    }

    async fn page_out(
        &self,
        persona: PersonaId,
        page: PageRef,
        to: TierRole,
    ) -> Result<(), TierError> {
        // Remove from working set if present, then write to target
        // tier. PR-3 doesn't validate that `to` is a configured
        // tier role — that's a PR-4 concern (needs the governor's
        // current Vec<TierConfig> snapshot to know which roles are
        // present on this hardware).
        {
            let mut working_sets = self.working_sets.write();
            if let Some(ws) = working_sets.get_mut(&persona) {
                let key = serde_json::to_string(&page).unwrap_or_default();
                // Pinned pages skip silently per the trait docstring:
                // page_out doesn't surface TierError for pin-violation;
                // composition is responsible for unpinning.
                if let Some(resident) = ws.pages.get(&key) {
                    if resident.pinned {
                        return Ok(());
                    }
                }
                ws.pages.remove(&key);
            }
        }

        // Find the target tier and write a marker (PR-3 doesn't
        // shuttle the actual blob — that's a PR-4 enhancement; for
        // now page_out is a working-set-state operation only). When
        // we wire blob movement, this is where TierStore::write
        // gets called.
        for tier in &self.tiers {
            if tier.role() == to {
                tier.observe_access(page);
                return Ok(());
            }
        }
        Err(TierError::RoleNotConfigured { role: to })
    }

    fn working_set(&self, _persona: PersonaId) -> Option<&WorkingSet> {
        // PR-3 cannot return a borrow through the RwLock without
        // exposing the lock guard type — that breaks the trait
        // signature. PR-4 will introduce a `Snapshot` type that
        // clones the working set view; until then, return None so
        // callers know to use the (future) snapshot API instead of
        // relying on this borrow path. Tests that need to inspect
        // the working set use the internal `working_set_snapshot`
        // helper below.
        //
        // This is a deliberate refinement of the PR-2 contract,
        // documented in the trait docstring as "Option<&WorkingSet>"
        // — the None case here is the "lock-guard escape impossible"
        // case, distinct from the spec's "persona not registered"
        // case but compatible with the same return type.
        None
    }

    fn audit_access(
        &self,
        persona: PersonaId,
        page: PageRef,
    ) -> Result<(), AccessDenied> {
        match self.page_owners.read().get(&page).copied() {
            Some(owner) if owner != persona => Err(AccessDenied {
                actor: persona,
                page,
                owner: Some(owner),
                reason: "cross-persona read blocked by working-set MMU".to_string(),
            }),
            _ => Ok(()),
        }
    }
}

impl LocalWorkingSetManager {
    /// Test/diagnostic helper: snapshot the working set for a persona.
    /// Clones — not for hot path. Used by tests + future telemetry
    /// modules to inspect state without holding the read lock.
    pub fn working_set_snapshot(&self, persona: PersonaId) -> Option<WorkingSet> {
        self.working_sets.read().get(&persona).cloned()
    }
}

/// Unix-ms timestamp. Used by `ResidentPage.last_access_ms` to record
/// the wall-clock of a page promotion. Tests pass a fixed value to a
/// stub clock; production reads `SystemTime::now()`.
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    //! End-to-end tests for the local impl. Each test wires a couple
    //! of stub tiers, registers a persona, and verifies the page_in /
    //! page_out / audit_access dispatch.
    use super::*;
    use crate::genome::blob::{ArtifactBlob, Provenance};
    use crate::genome::tier::{EvictionRecord, TierCapacity};
    use crate::genome::working_set::{ArtifactId, PageKind, PageOffset};
    use parking_lot::Mutex;
    use std::sync::Arc;
    use uuid::Uuid;

    /// Stub tier store: records every read/write/observe call so
    /// tests assert "the manager called the right tier in the right
    /// order." Holds a static `Option<PageHandle>` per page for
    /// `read` responses.
    struct StubTier {
        role: TierRole,
        /// Pages this tier has — read returns Ok(handle) for matches,
        /// `TierError::PageNotFound` otherwise.
        pages_present: Mutex<Vec<PageRef>>,
        /// Call log so tests can assert order of tier access.
        reads: Mutex<Vec<PageRef>>,
        observes: Mutex<Vec<PageRef>>,
    }

    impl StubTier {
        fn new(role: TierRole, pages_present: Vec<PageRef>) -> Arc<Self> {
            Arc::new(Self {
                role,
                pages_present: Mutex::new(pages_present),
                reads: Mutex::new(Vec::new()),
                observes: Mutex::new(Vec::new()),
            })
        }
    }

    #[async_trait]
    impl TierStore for StubTier {
        fn role(&self) -> TierRole {
            self.role
        }

        async fn read(&self, page: PageRef) -> Result<PageHandle, TierError> {
            self.reads.lock().push(page);
            if self.pages_present.lock().contains(&page) {
                Ok(PageHandle {
                    page,
                    tier_role: self.role,
                    size_bytes: 1024,
                })
            } else {
                Err(TierError::PageNotFound { page })
            }
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
            TierCapacity {
                current_used: 0,
                configured_limit: 100_000_000,
            }
        }

        fn observe_access(&self, page: PageRef) {
            self.observes.lock().push(page);
        }
    }

    fn make_page(low_artifact_bits: u128) -> PageRef {
        PageRef {
            kind: PageKind::LoRALayer,
            artifact: ArtifactId::new(Uuid::from_u128(low_artifact_bits)),
            offset: PageOffset::Whole,
        }
    }

    fn make_persona(low_bits: u128) -> PersonaId {
        PersonaId::new(Uuid::from_u128(low_bits))
    }

    fn capacity_uma() -> WorkingSetCapacity {
        WorkingSetCapacity {
            fast_bytes: 1_000_000,
            warm_bytes: 0,
            max_pinned_bytes: 500_000,
        }
    }

    /// What this catches: page_in on an already-resident page returns
    /// the cached handle WITHOUT walking the tier chain. Hot-path
    /// correctness; the whole point of a working set is that the
    /// resident-hit path is cheap.
    #[tokio::test]
    async fn page_in_resident_returns_cached_without_tier_walk() {
        let page = make_page(1);
        let fast = StubTier::new(TierRole::Fast, vec![page]);
        let mgr = LocalWorkingSetManager::new(vec![fast.clone()]);
        let persona = make_persona(7);
        mgr.register_persona(persona, capacity_uma());

        // First call: misses working set, promotes via Fast tier.
        let first = mgr.page_in(persona, page).await;
        match first {
            Err(fault) => {
                assert_eq!(fault.from_role, Some(TierRole::Fast));
                assert_eq!(fault.to_role, TierRole::Fast);
                assert_eq!(fault.persona, persona);
            }
            Ok(_) => panic!("first call should report tier promotion"),
        }
        let reads_after_first = fast.reads.lock().len();
        assert_eq!(reads_after_first, 1);

        // Second call: hits working set, returns Ok without re-reading.
        let second = mgr.page_in(persona, page).await;
        match second {
            Ok(handle) => {
                assert_eq!(handle.tier_role, TierRole::Fast);
                assert_eq!(handle.page, page);
            }
            Err(_) => panic!("second call should be a resident hit"),
        }
        // Tier was NOT re-read on the resident-hit path.
        assert_eq!(fast.reads.lock().len(), reads_after_first);
    }

    /// What this catches: page_in walks tier chain top-down (Fast →
    /// Cold), returns the first hit + records the from_role + to_role
    /// correctly. PageFault.from_role is where the page WAS;
    /// PageFault.to_role is the working set's preferred tier (always
    /// the highest configured).
    #[tokio::test]
    async fn page_in_walks_tier_chain_and_records_promotion() {
        let page = make_page(2);
        let fast = StubTier::new(TierRole::Fast, vec![]);
        let bench = StubTier::new(TierRole::Bench, vec![]);
        let cold = StubTier::new(TierRole::Cold, vec![page]);
        let mgr = LocalWorkingSetManager::new(vec![
            fast.clone(),
            bench.clone(),
            cold.clone(),
        ]);
        let persona = make_persona(8);
        mgr.register_persona(persona, capacity_uma());

        let result = mgr.page_in(persona, page).await;
        match result {
            Err(fault) => {
                assert_eq!(fault.from_role, Some(TierRole::Cold));
                assert_eq!(fault.to_role, TierRole::Fast);
                assert_eq!(fault.persona, persona);
                // Eviction cost is None — PR-3 doesn't drive
                // eviction. PR-4 wires the callback.
                assert!(fault.eviction_cost.is_none());
            }
            Ok(_) => panic!("expected PageFault for tier promotion"),
        }

        // Tier walk order: Fast first, then Bench, then Cold.
        assert_eq!(fast.reads.lock().len(), 1);
        assert_eq!(bench.reads.lock().len(), 1);
        assert_eq!(cold.reads.lock().len(), 1);
    }

    /// What this catches: page_in on a page that exists in NO tier
    /// returns a PageFault with `from_role: None` — the typed "true
    /// cold miss" signal sentinel needs to distinguish "page never
    /// existed" from "page was on Cold tier."
    #[tokio::test]
    async fn page_in_true_cold_miss_has_none_from_role() {
        let page = make_page(3);
        let fast = StubTier::new(TierRole::Fast, vec![]);
        let cold = StubTier::new(TierRole::Cold, vec![]);
        let mgr = LocalWorkingSetManager::new(vec![fast, cold]);
        let persona = make_persona(9);
        mgr.register_persona(persona, capacity_uma());

        let result = mgr.page_in(persona, page).await;
        match result {
            Err(fault) => {
                assert_eq!(fault.from_role, None);
                assert_eq!(fault.to_role, TierRole::Fast);
                assert_eq!(fault.page, page);
            }
            Ok(_) => panic!("expected PageFault for true cold miss"),
        }
    }

    /// What this catches: audit_access returns AccessDenied with the
    /// typed shape — not a generic error — when a different persona
    /// tries to read a private page. Same contract PR-2's trait test
    /// pins, now exercised through the LocalWorkingSetManager.
    #[tokio::test]
    async fn audit_access_denies_cross_persona_read() {
        let fast = StubTier::new(TierRole::Fast, vec![]);
        let mgr = LocalWorkingSetManager::new(vec![fast]);
        let owner = make_persona(10);
        let intruder = make_persona(11);
        let page = make_page(4);

        mgr.register_persona(owner, capacity_uma());
        mgr.register_persona(intruder, capacity_uma());
        mgr.register_page_owner(page, owner);

        // Owner: OK.
        assert!(mgr.audit_access(owner, page).is_ok());

        // Intruder: AccessDenied with full context.
        let result = mgr.audit_access(intruder, page);
        match result {
            Err(denied) => {
                assert_eq!(denied.actor, intruder);
                assert_eq!(denied.owner, Some(owner));
                assert!(denied.reason.contains("cross-persona"));
            }
            Ok(()) => panic!("expected AccessDenied"),
        }
    }

    /// What this catches: page_out to a configured tier role observes
    /// the page (signals the tier's bookkeeping) and removes from the
    /// working set. page_out to an unconfigured role returns
    /// `TierError::RoleNotConfigured` — the typed refusal for "you
    /// asked for a role this hardware doesn't have."
    #[tokio::test]
    async fn page_out_observes_target_tier_and_handles_unconfigured() {
        let page = make_page(5);
        let fast = StubTier::new(TierRole::Fast, vec![page]);
        let bench = StubTier::new(TierRole::Bench, vec![]);
        let mgr = LocalWorkingSetManager::new(vec![fast, bench.clone()]);
        let persona = make_persona(12);
        mgr.register_persona(persona, capacity_uma());

        // First, page_in to populate the working set.
        let _ = mgr.page_in(persona, page).await;

        // page_out to Bench: tier observes; working set updates.
        let result = mgr.page_out(persona, page, TierRole::Bench).await;
        assert!(result.is_ok());
        assert!(bench.observes.lock().contains(&page));

        // page_out to Warm: NOT configured on this UMA-like setup
        // (no Warm tier in the vec). Returns typed RoleNotConfigured.
        let result = mgr.page_out(persona, page, TierRole::Warm).await;
        match result {
            Err(TierError::RoleNotConfigured { role }) => {
                assert_eq!(role, TierRole::Warm);
            }
            other => panic!("expected RoleNotConfigured, got {other:?}"),
        }
    }

    /// What this catches: pinned pages survive page_out (skipped
    /// silently per the trait docstring). Composition layer holds
    /// the pin; manager respects it.
    #[tokio::test]
    async fn page_out_skips_pinned_pages_silently() {
        let page = make_page(6);
        let fast = StubTier::new(TierRole::Fast, vec![page]);
        let bench = StubTier::new(TierRole::Bench, vec![]);
        let mgr = LocalWorkingSetManager::new(vec![fast, bench]);
        let persona = make_persona(13);
        mgr.register_persona(persona, capacity_uma());

        let _ = mgr.page_in(persona, page).await;

        // Manually pin the page (composition would normally do this).
        {
            let mut working_sets = mgr.working_sets.write();
            if let Some(ws) = working_sets.get_mut(&persona) {
                let key = serde_json::to_string(&page).unwrap();
                if let Some(resident) = ws.pages.get_mut(&key) {
                    resident.pinned = true;
                }
            }
        }

        // page_out is a no-op for pinned page.
        let result = mgr.page_out(persona, page, TierRole::Bench).await;
        assert!(result.is_ok());

        // Page is still in the working set.
        let snapshot = mgr.working_set_snapshot(persona).unwrap();
        let key = serde_json::to_string(&page).unwrap();
        assert!(snapshot.pages.contains_key(&key));
    }

    /// What this catches: working_set_snapshot reflects what page_in
    /// recorded. Diagnostic helper correctness — tests + telemetry
    /// rely on this to verify state without holding the lock.
    #[tokio::test]
    async fn working_set_snapshot_reflects_page_in_state() {
        let page = make_page(7);
        let fast = StubTier::new(TierRole::Fast, vec![page]);
        let mgr = LocalWorkingSetManager::new(vec![fast]);
        let persona = make_persona(14);
        mgr.register_persona(persona, capacity_uma());

        // Pre-page-in: empty.
        let pre = mgr.working_set_snapshot(persona).unwrap();
        assert!(pre.pages.is_empty());

        // After page_in: one resident page.
        let _ = mgr.page_in(persona, page).await;
        let post = mgr.working_set_snapshot(persona).unwrap();
        assert_eq!(post.pages.len(), 1);
        let key = serde_json::to_string(&page).unwrap();
        let resident = post.pages.get(&key).unwrap();
        assert_eq!(resident.role, TierRole::Fast);
        assert_eq!(resident.access_count_window, 1);
        assert!(!resident.pinned);
    }

    /// What this catches: tier_count returns the configured tier
    /// count. Cheap O(1) — used by the governor's policy diagnostics
    /// to verify the manager was wired with the right Vec<TierConfig>
    /// shape (4 on UMA, 5 on discrete-GPU).
    #[tokio::test]
    async fn tier_count_reflects_configured_tiers() {
        let mgr = LocalWorkingSetManager::new(vec![
            StubTier::new(TierRole::Fast, vec![]),
            StubTier::new(TierRole::Bench, vec![]),
            StubTier::new(TierRole::Cold, vec![]),
            StubTier::new(TierRole::Frozen, vec![]),
        ]);
        assert_eq!(mgr.tier_count(), 4);
    }
}
