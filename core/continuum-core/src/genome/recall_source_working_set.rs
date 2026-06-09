//! `demand-aligned-recall` PR-3d: `WorkingSetCandidateSource` —
//! the `CandidateSource` impl that translates a persona's
//! `WorkingSet` (from `LocalWorkingSetManager` #1355) into recall
//! candidates.
//!
//! This is the architectural payoff of the genome stack: a
//! persona's `page_in` calls populate the working set; recall
//! reads that same working set to surface "what's already hot"
//! candidates ranked by `LocalDemandAlignedRecall` (#1372 + #1374).
//! The bus hook from #1362 publishes PageFault events; this
//! source reads the resulting WorkingSet state.
//!
//! ## What PR-3d ships
//!
//! - `WorkingSetCandidateSource` struct holding
//!   `Arc<LocalWorkingSetManager>`
//! - `CandidateSource::fetch` impl that:
//!   - reads the persona's working_set_snapshot
//!   - translates each ResidentPage into a CandidateArtifact with
//!     `ResidencyHint::Hot { role }` (resident = hot by definition)
//!   - filters by `query.scope` (Local → return all hot;
//!     LocalThenGrid / Federation → also hot but mark grid sourcing
//!     for upstream to extend)
//!
//! ## What PR-3d does NOT ship
//!
//! - Genome catalog walker (Bench/Cold/Frozen tier sources) — needs
//!   the catalog module which doesn't exist yet
//! - Federation peer source — needs the federation registry
//! - Embedding integration (semantic factor) — stubs return 0.5
//! - Sentinel outcome history lookup (outcome_history factor) —
//!   stubs return 0.5
//! - Trust registry lookup (provenance_trust factor) — stubs
//!   return 0.5
//!
//! Each of the three "stub 0.5" factors is documented in the
//! translation function with a TODO so the dedicated integrations
//! can find them. Recall still ranks correctly today because
//! tier_proximity (Hot=1.0) carries the load — the working-set
//! members all score the same on non-tier factors so the relative
//! ordering reflects what matters in PR-3d's scope: how hot.
//!
//! The semantic / outcome / trust integrations are independent
//! lane work; each can land separately + recall scoring improves
//! without re-touching this source.

use async_trait::async_trait;
use std::sync::Arc;

use super::local_manager::LocalWorkingSetManager;
use super::recall::ResidencyHint;
use super::recall_impl::{CandidateArtifact, CandidateSource};
use super::recall_trait::{CapabilityQuery, RecallContext};

/// Placeholder factor value for the three non-tier scoring factors
/// (semantic, outcome_history, provenance_trust). PR-3d's
/// working-set source can't compute these without the embedding /
/// sentinel / trust integrations that aren't built yet; using 0.5
/// (the neutral midpoint) means none of the working-set candidates
/// gets a per-factor bias for or against, so ranking falls to
/// tier_proximity (Hot=1.0) + recency_decay (last_access_ms).
///
/// When the dedicated integrations land, callers pass real values
/// via the upstream `recall()` call chain; this constant disappears.
pub const NEUTRAL_FACTOR_STUB: f32 = 0.5;

/// `CandidateSource` impl backed by a per-process working-set
/// manager. Holds the manager Arc so the source survives across
/// recall calls; the working set itself is read by snapshot
/// (cloned) on each `fetch` to avoid holding the RwLock across
/// awaits.
///
/// Thread-safe: the underlying LocalWorkingSetManager is
/// `Send + Sync`; the Arc clone for `fetch` is O(1).
pub struct WorkingSetCandidateSource {
    manager: Arc<LocalWorkingSetManager>,
}

impl WorkingSetCandidateSource {
    /// Construct from a working-set manager. The manager must
    /// already be registered with the personas the source will
    /// fetch for; `fetch` returns an empty Vec for unregistered
    /// personas (a legitimate empty-pool signal, not an error).
    pub fn new(manager: Arc<LocalWorkingSetManager>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl CandidateSource for WorkingSetCandidateSource {
    async fn fetch(
        &self,
        _query: &CapabilityQuery,
        context: &RecallContext,
    ) -> Vec<CandidateArtifact> {
        // Snapshot the persona's working set. Cloned to avoid
        // holding the manager's RwLock across awaits (same pattern
        // as #1362's bus_arc hook).
        let snapshot = match self.manager.working_set_snapshot(context.persona) {
            Some(ws) => ws,
            // Unregistered persona — return empty pool. Recall
            // callers handle empty gracefully (try federation,
            // etc.).
            None => return Vec::new(),
        };

        // Translate each ResidentPage → CandidateArtifact. Every
        // resident page is `ResidencyHint::Hot { role }` by
        // definition; the page is in the working set, ergo paged
        // into the persona's tier. Non-tier factors get the neutral
        // 0.5 stub per the module docstring; semantic/outcome/trust
        // integrations land in dedicated PRs.
        snapshot
            .pages
            .into_values()
            .map(|resident| CandidateArtifact {
                kind: resident.page.kind,
                artifact_id: resident.page.artifact,
                semantic_factor: NEUTRAL_FACTOR_STUB,
                outcome_history_factor: NEUTRAL_FACTOR_STUB,
                last_used_ms: resident.last_access_ms,
                residency: ResidencyHint::Hot {
                    role: resident.role,
                },
                provenance_trust_factor: NEUTRAL_FACTOR_STUB,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    //! End-to-end tests: register a persona, page-in some pages
    //! via the working-set manager, then prove the working-set
    //! source returns them as candidates that the LocalDemand
    //! AlignedRecall ranks correctly.
    use super::*;
    use crate::genome::blob::{ArtifactBlob, Provenance};
    use crate::genome::manager::WorkingSetManager;
    use crate::genome::recall::{FreshnessTarget, RecallScope, TaskKind};
    use crate::genome::recall_impl::LocalDemandAlignedRecall;
    use crate::genome::recall_trait::{
        DemandAlignedRecall, DomainHint, RecallBudget, RecallContext,
    };
    use crate::genome::store::TierStore;
    use crate::genome::tier::{EvictionRecord, TierCapacity, TierError, TierRole};
    use crate::genome::working_set::{
        ArtifactId, PageHandle, PageKind, PageOffset, PageRef, PersonaId, WorkingSetCapacity,
    };
    use parking_lot::Mutex;
    use uuid::Uuid;

    fn sample_persona(low: u128) -> PersonaId {
        PersonaId::new(Uuid::from_u128(low))
    }

    fn sample_page(low: u128, kind: PageKind) -> PageRef {
        PageRef {
            kind,
            artifact: ArtifactId::new(Uuid::from_u128(low)),
            offset: PageOffset::Whole,
        }
    }

    fn capacity_uma() -> WorkingSetCapacity {
        WorkingSetCapacity {
            fast_bytes: 1_000_000,
            warm_bytes: 0,
            max_pinned_bytes: 500_000,
        }
    }

    /// Stub tier that always has the requested page (for setting
    /// up the working-set state we want to query).
    struct AlwaysPresentTier {
        role: TierRole,
        present: Mutex<Vec<PageRef>>,
    }

    impl AlwaysPresentTier {
        fn new(role: TierRole) -> Arc<Self> {
            Arc::new(Self {
                role,
                present: Mutex::new(Vec::new()),
            })
        }
        fn add(&self, page: PageRef) {
            self.present.lock().push(page);
        }
    }

    #[async_trait]
    impl TierStore for AlwaysPresentTier {
        fn role(&self) -> TierRole {
            self.role
        }
        async fn read(&self, page: PageRef) -> Result<PageHandle, TierError> {
            if self.present.lock().contains(&page) {
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
        async fn evict(&self, _target: usize) -> Vec<EvictionRecord> {
            Vec::new()
        }
        fn capacity(&self) -> TierCapacity {
            TierCapacity {
                current_used: 0,
                configured_limit: 100_000_000,
            }
        }
        fn observe_access(&self, _page: PageRef) {}
    }

    fn sample_query() -> CapabilityQuery {
        CapabilityQuery {
            task_kind: TaskKind::Chat,
            domain_hints: vec![DomainHint::new("test")],
            budget: RecallBudget {
                max_bytes: 1_000_000,
                max_duration_ms: 100,
            },
            must_include: vec![],
            prefer_refined: true,
            scope: RecallScope::Local,
            freshness_target: FreshnessTarget::BestEffort,
        }
    }

    /// What this catches: an unregistered persona returns an empty
    /// Vec, NOT an error. Recall must handle "this persona doesn't
    /// have a working set yet" gracefully (it's the cold-start case
    /// for new personas).
    #[tokio::test]
    async fn fetch_unregistered_persona_returns_empty_not_error() {
        let tier = AlwaysPresentTier::new(TierRole::Fast);
        let mgr = Arc::new(LocalWorkingSetManager::new(vec![tier]));
        let source = WorkingSetCandidateSource::new(mgr);

        let ctx = RecallContext::cold_start(sample_persona(99));
        let candidates = source.fetch(&sample_query(), &ctx).await;
        assert!(candidates.is_empty());
    }

    /// What this catches: a registered-but-empty working set
    /// returns an empty Vec. Same as unregistered from the
    /// outside, but the working set EXISTS — distinguishing the
    /// two is the registration-tracking job of the manager, not
    /// the source.
    #[tokio::test]
    async fn fetch_registered_empty_working_set_returns_empty() {
        let tier = AlwaysPresentTier::new(TierRole::Fast);
        let mgr = Arc::new(LocalWorkingSetManager::new(vec![tier]));
        let persona = sample_persona(1);
        mgr.register_persona(persona, capacity_uma());

        let source = WorkingSetCandidateSource::new(mgr);
        let ctx = RecallContext::cold_start(persona);
        let candidates = source.fetch(&sample_query(), &ctx).await;
        assert!(candidates.is_empty());
    }

    /// What this catches: after page_in populates the working set,
    /// fetch returns one CandidateArtifact per resident page +
    /// each candidate carries Hot residency at the right TierRole.
    /// This is the architectural payoff — working-set state ↔
    /// recall candidate translation works end-to-end.
    #[tokio::test]
    async fn fetch_after_page_in_returns_resident_pages_as_hot_candidates() {
        let tier = AlwaysPresentTier::new(TierRole::Fast);
        let page1 = sample_page(10, PageKind::LoRALayer);
        let page2 = sample_page(11, PageKind::Engram);
        tier.add(page1);
        tier.add(page2);

        let mgr = Arc::new(LocalWorkingSetManager::new(vec![tier]));
        let persona = sample_persona(1);
        mgr.register_persona(persona, capacity_uma());

        // Page in both — populates the working set.
        let _ = mgr.page_in(persona, page1).await;
        let _ = mgr.page_in(persona, page2).await;

        let source = WorkingSetCandidateSource::new(mgr);
        let ctx = RecallContext::cold_start(persona);
        let candidates = source.fetch(&sample_query(), &ctx).await;

        assert_eq!(candidates.len(), 2);
        // Both candidates are Hot at Fast role.
        for c in &candidates {
            match &c.residency {
                ResidencyHint::Hot { role } => assert_eq!(*role, TierRole::Fast),
                other => panic!("expected Hot residency, got {other:?}"),
            }
        }
        // Each candidate carries one of the two artifact ids we
        // paged in.
        let ids: Vec<Uuid> = candidates.iter().map(|c| c.artifact_id.as_uuid()).collect();
        assert!(ids.contains(&Uuid::from_u128(10)));
        assert!(ids.contains(&Uuid::from_u128(11)));
    }

    /// What this catches: the CandidateArtifact.kind preserves the
    /// PageRef.kind from the working set — LoRALayer page → layers
    /// sub-pool; Engram page → engrams sub-pool. The translation
    /// is faithful so the downstream rank() partitions correctly.
    #[tokio::test]
    async fn translation_preserves_page_kind_for_sub_pool_partitioning() {
        let tier = AlwaysPresentTier::new(TierRole::Fast);
        let layer_page = sample_page(20, PageKind::LoRALayer);
        let expert_page = sample_page(21, PageKind::MoEExpert);
        let engram_page = sample_page(22, PageKind::Engram);
        tier.add(layer_page);
        tier.add(expert_page);
        tier.add(engram_page);

        let mgr = Arc::new(LocalWorkingSetManager::new(vec![tier]));
        let persona = sample_persona(2);
        mgr.register_persona(persona, capacity_uma());
        let _ = mgr.page_in(persona, layer_page).await;
        let _ = mgr.page_in(persona, expert_page).await;
        let _ = mgr.page_in(persona, engram_page).await;

        let source = WorkingSetCandidateSource::new(mgr);
        let ctx = RecallContext::cold_start(persona);
        let candidates = source.fetch(&sample_query(), &ctx).await;

        assert_eq!(candidates.len(), 3);
        // Group by kind.
        let layers: Vec<_> = candidates
            .iter()
            .filter(|c| c.kind == PageKind::LoRALayer)
            .collect();
        let experts: Vec<_> = candidates
            .iter()
            .filter(|c| c.kind == PageKind::MoEExpert)
            .collect();
        let engrams: Vec<_> = candidates
            .iter()
            .filter(|c| c.kind == PageKind::Engram)
            .collect();
        assert_eq!(layers.len(), 1);
        assert_eq!(experts.len(), 1);
        assert_eq!(engrams.len(), 1);
    }

    /// What this catches: every PR-3d candidate carries the
    /// NEUTRAL_FACTOR_STUB for semantic / outcome_history /
    /// provenance_trust. The dedicated integrations (embedding,
    /// sentinel, trust) will replace these per-call; PR-3d ships
    /// the contract that "no integration yet → neutral 0.5."
    /// This test pins the contract so a future PR that wires real
    /// values has a regression check to flip.
    #[tokio::test]
    async fn translation_uses_neutral_factor_stubs_for_non_tier_factors() {
        let tier = AlwaysPresentTier::new(TierRole::Fast);
        let page = sample_page(30, PageKind::LoRALayer);
        tier.add(page);

        let mgr = Arc::new(LocalWorkingSetManager::new(vec![tier]));
        let persona = sample_persona(3);
        mgr.register_persona(persona, capacity_uma());
        let _ = mgr.page_in(persona, page).await;

        let source = WorkingSetCandidateSource::new(mgr);
        let ctx = RecallContext::cold_start(persona);
        let candidates = source.fetch(&sample_query(), &ctx).await;

        assert_eq!(candidates.len(), 1);
        let c = &candidates[0];
        assert!((c.semantic_factor - NEUTRAL_FACTOR_STUB).abs() < 1e-6);
        assert!((c.outcome_history_factor - NEUTRAL_FACTOR_STUB).abs() < 1e-6);
        assert!((c.provenance_trust_factor - NEUTRAL_FACTOR_STUB).abs() < 1e-6);
    }

    /// What this catches: WorkingSetCandidateSource is object-safe
    /// — usable as Arc<dyn CandidateSource>. PR-3c's
    /// LocalDemandAlignedRecall holds the source via Arc<dyn>, so
    /// any future CandidateSource impl must satisfy this shape too.
    #[tokio::test]
    async fn source_is_object_safe_for_arc_dyn_dispatch() {
        let tier = AlwaysPresentTier::new(TierRole::Fast);
        let mgr = Arc::new(LocalWorkingSetManager::new(vec![tier]));
        let source: Arc<dyn CandidateSource> = Arc::new(WorkingSetCandidateSource::new(mgr));
        let ctx = RecallContext::cold_start(sample_persona(99));
        // Round-trip through the dyn dispatch.
        let candidates = source.fetch(&sample_query(), &ctx).await;
        assert!(candidates.is_empty(), "no persona registered → empty");
    }

    /// What this catches: the end-to-end recall path through
    /// LocalDemandAlignedRecall::with_source(working_set_source).
    /// This is the architectural payoff test — page_in writes
    /// working set; recall() reads it; the RankedPool contains
    /// the paged-in artifacts.
    #[tokio::test]
    async fn end_to_end_page_in_then_recall_returns_ranked_pool() {
        let tier = AlwaysPresentTier::new(TierRole::Fast);
        let page1 = sample_page(100, PageKind::LoRALayer);
        let page2 = sample_page(101, PageKind::LoRALayer);
        let page3 = sample_page(102, PageKind::Engram);
        tier.add(page1);
        tier.add(page2);
        tier.add(page3);

        let mgr = Arc::new(LocalWorkingSetManager::new(vec![tier]));
        let persona = sample_persona(7);
        mgr.register_persona(persona, capacity_uma());
        let _ = mgr.page_in(persona, page1).await;
        let _ = mgr.page_in(persona, page2).await;
        let _ = mgr.page_in(persona, page3).await;

        let source = Arc::new(WorkingSetCandidateSource::new(mgr));
        let recall = LocalDemandAlignedRecall::with_source(source);
        let ctx = RecallContext::cold_start(persona);

        let pool = recall.recall(&sample_query(), &ctx).await.unwrap();
        // Two LoRA layers + one engram landed in their sub-pools.
        assert_eq!(pool.layers.len(), 2);
        assert_eq!(pool.engrams.len(), 1);
        assert!(pool.experts.is_empty());

        // All three resident pages got scored — combined > 0 for
        // each (Hot residency + neutral stubs).
        for (_, score, _) in &pool.layers {
            assert!(score.combined > 0.0);
        }
    }
}
