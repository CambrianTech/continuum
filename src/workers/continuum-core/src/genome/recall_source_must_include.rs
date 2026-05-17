//! `demand-aligned-recall` PR-3f: `MustIncludeCandidateSource` —
//! resolves `CapabilityQuery::must_include` hard pins into
//! candidates.
//!
//! Per GENOME-FOUNDRY-SENTINEL Part 7: "Hard pins — recall MUST
//! include these in the RankedPool even if their score is low. Used
//! for persona-private LoRA layers and sticky engrams."
//!
//! This source ensures every entry in `query.must_include` shows up
//! as a CandidateArtifact, even if no other source surfaces it. The
//! composite pattern (PR-3e) handles deduplication: when wired AFTER
//! a resident source like WorkingSetCandidateSource with
//! ByArtifactId dedup, must-include items that ARE resident get the
//! resident source's Hot residency + factor data; must-include
//! items NOT resident get this source's NotResident placeholder
//! (still ranked, just lower combined score).
//!
//! ## What PR-3f ships
//!
//! - `MustIncludeCandidateSource` (zero-state singleton — no Arc
//!   state needed; the source is pure-function over the query)
//! - `CandidateSource::fetch` impl that:
//!   - reads `query.must_include` Vec<ArtifactRef>
//!   - maps each variant (LoRALayer / MoEExpert / Engram) to a
//!     CandidateArtifact with the appropriate `PageKind`
//!   - marks every must-include candidate as `ResidencyHint::
//!     NotResident { acquirable_from: SentinelRefinement }` —
//!     placeholder; if working set has a Hot version it wins via
//!     dedup
//!   - uses `NEUTRAL_FACTOR_STUB` (0.5) for the three non-tier
//!     factors, same as WorkingSetCandidateSource (PR-3d)
//!
//! ## Composition pattern
//!
//! Recommended wiring for production recall:
//!
//! ```ignore
//! let composite = CompositeCandidateSource::with_default_dedup(vec![
//!     Arc::new(WorkingSetCandidateSource::new(mgr)),     // Hot pages first
//!     Arc::new(MustIncludeCandidateSource),              // Pins second
//!     // future: catalog walker, federation source
//! ]);
//! ```
//!
//! With this ordering + `DedupPolicy::ByArtifactId`:
//! - Hot resident pages keep their tier_proximity=1.0 score
//! - Non-resident must-includes get added at tier_proximity=0.0
//!   but still appear in the RankedPool (per the spec's hard-pin
//!   contract)
//! - The ranking surfaces hot stuff at the top + pinned-but-cold
//!   stuff at the bottom of each sub-pool, which matches what
//!   composition expects.

use async_trait::async_trait;

use super::recall::{AcquireSource, ResidencyHint};
use super::recall_impl::{CandidateArtifact, CandidateSource};
use super::recall_source_working_set::NEUTRAL_FACTOR_STUB;
use super::recall_trait::{ArtifactRef, CapabilityQuery, RecallContext};
use super::working_set::PageKind;

/// Zero-state source that resolves `query.must_include` into
/// candidates. Stateless — every instance is interchangeable;
/// the construction-time cost is zero.
pub struct MustIncludeCandidateSource;

impl MustIncludeCandidateSource {
    /// Construct. Returns a unit struct because all the state
    /// lives in the query — there's nothing per-source to hold.
    pub fn new() -> Self {
        Self
    }
}

impl Default for MustIncludeCandidateSource {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CandidateSource for MustIncludeCandidateSource {
    async fn fetch(
        &self,
        query: &CapabilityQuery,
        _context: &RecallContext,
    ) -> Vec<CandidateArtifact> {
        // Map each must_include ArtifactRef into a CandidateArtifact
        // with NotResident residency. The composite (PR-3e) handles
        // dedup against other sources — if a more-residency-aware
        // source surfaces the same artifact_id first, that one wins.
        query
            .must_include
            .iter()
            .map(|aref| {
                let (kind, artifact_id) = match aref {
                    ArtifactRef::LoRALayer(r) => (PageKind::LoRALayer, r.0),
                    ArtifactRef::MoEExpert(r) => (PageKind::MoEExpert, r.0),
                    ArtifactRef::Engram(r) => (PageKind::Engram, r.0),
                };
                CandidateArtifact {
                    kind,
                    artifact_id,
                    semantic_factor: NEUTRAL_FACTOR_STUB,
                    outcome_history_factor: NEUTRAL_FACTOR_STUB,
                    // Placeholder timestamp — must-include items
                    // don't carry last-used metadata in the query.
                    // The recency_decay over this will be ~0 (long
                    // time ago) so the recency factor contributes
                    // minimally; tier_proximity (0 for NotResident)
                    // is the dominant signal.
                    last_used_ms: 0,
                    residency: ResidencyHint::NotResident {
                        acquirable_from: AcquireSource::SentinelRefinement,
                    },
                    provenance_trust_factor: NEUTRAL_FACTOR_STUB,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    //! End-to-end tests: construct a CapabilityQuery with
    //! must_include entries, verify MustIncludeCandidateSource
    //! surfaces them as candidates with the right shape. Then
    //! verify the composite-with-dedup pattern works as expected
    //! when a working-set source has overlapping artifacts.
    use super::*;
    use crate::genome::local_manager::LocalWorkingSetManager;
    use crate::genome::manager::WorkingSetManager;
    use crate::genome::recall::{FreshnessTarget, RecallScope, TaskKind};
    use crate::genome::recall_source_composite::{
        CompositeCandidateSource, DedupPolicy,
    };
    use crate::genome::recall_source_working_set::WorkingSetCandidateSource;
    use crate::genome::recall_trait::{
        DomainHint, EngramRef, LoRALayerRef, MoEExpertRef, RecallBudget, RecallContext,
    };
    use crate::genome::store::TierStore;
    use crate::genome::tier::{EvictionRecord, TierCapacity, TierError, TierRole};
    use crate::genome::blob::{ArtifactBlob, Provenance};
    use crate::genome::working_set::{
        ArtifactId, PageHandle, PageOffset, PageRef, PersonaId, WorkingSetCapacity,
    };
    use parking_lot::Mutex;
    use std::sync::Arc;
    use uuid::Uuid;

    fn art(low: u128) -> ArtifactId {
        ArtifactId::new(Uuid::from_u128(low))
    }
    fn persona() -> PersonaId {
        PersonaId::new(Uuid::nil())
    }
    fn ctx() -> RecallContext {
        RecallContext::cold_start(persona())
    }
    fn base_query() -> CapabilityQuery {
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

    /// What this catches: an empty must_include list yields an
    /// empty candidate Vec. No-error contract: empty pins are
    /// legitimate, not a failure.
    #[tokio::test]
    async fn empty_must_include_returns_empty_candidates() {
        let src = MustIncludeCandidateSource::new();
        let candidates = src.fetch(&base_query(), &ctx()).await;
        assert!(candidates.is_empty());
    }

    /// What this catches: each ArtifactRef variant maps to the
    /// correct PageKind. If a future PR adds a variant, this test
    /// fails (forces author to extend the mapping).
    #[tokio::test]
    async fn variant_mapping_preserves_page_kind() {
        let src = MustIncludeCandidateSource::new();
        let mut query = base_query();
        query.must_include = vec![
            ArtifactRef::LoRALayer(LoRALayerRef(art(1))),
            ArtifactRef::MoEExpert(MoEExpertRef(art(2))),
            ArtifactRef::Engram(EngramRef(art(3))),
        ];
        let candidates = src.fetch(&query, &ctx()).await;
        assert_eq!(candidates.len(), 3);

        let layers: Vec<_> = candidates.iter().filter(|c| c.kind == PageKind::LoRALayer).collect();
        let experts: Vec<_> = candidates.iter().filter(|c| c.kind == PageKind::MoEExpert).collect();
        let engrams: Vec<_> = candidates.iter().filter(|c| c.kind == PageKind::Engram).collect();
        assert_eq!(layers.len(), 1);
        assert_eq!(experts.len(), 1);
        assert_eq!(engrams.len(), 1);
        assert_eq!(layers[0].artifact_id, art(1));
        assert_eq!(experts[0].artifact_id, art(2));
        assert_eq!(engrams[0].artifact_id, art(3));
    }

    /// What this catches: every must-include candidate carries
    /// `ResidencyHint::NotResident { SentinelRefinement }`. The
    /// composite pattern lets a more-residency-aware source (like
    /// WorkingSetCandidateSource) override via dedup. PR-3f's
    /// contract is "I make sure these surface; you decide where
    /// they live by source ordering."
    #[tokio::test]
    async fn must_include_marks_candidates_as_not_resident() {
        let src = MustIncludeCandidateSource::new();
        let mut query = base_query();
        query.must_include = vec![ArtifactRef::LoRALayer(LoRALayerRef(art(7)))];

        let candidates = src.fetch(&query, &ctx()).await;
        assert_eq!(candidates.len(), 1);
        match &candidates[0].residency {
            ResidencyHint::NotResident { acquirable_from } => {
                assert_eq!(*acquirable_from, AcquireSource::SentinelRefinement);
            }
            other => panic!("expected NotResident, got {other:?}"),
        }
    }

    /// What this catches: non-tier factors get NEUTRAL_FACTOR_STUB
    /// (0.5) — same convention as WorkingSetCandidateSource (PR-3d).
    /// Consistency lets the scoring weights work uniformly across
    /// sources.
    #[tokio::test]
    async fn factors_use_neutral_stubs_consistent_with_working_set_source() {
        let src = MustIncludeCandidateSource::new();
        let mut query = base_query();
        query.must_include = vec![ArtifactRef::LoRALayer(LoRALayerRef(art(7)))];

        let candidates = src.fetch(&query, &ctx()).await;
        assert_eq!(candidates.len(), 1);
        let c = &candidates[0];
        assert!((c.semantic_factor - NEUTRAL_FACTOR_STUB).abs() < 1e-6);
        assert!((c.outcome_history_factor - NEUTRAL_FACTOR_STUB).abs() < 1e-6);
        assert!((c.provenance_trust_factor - NEUTRAL_FACTOR_STUB).abs() < 1e-6);
    }

    /// What this catches: object-safety. MustIncludeCandidateSource
    /// works through `Arc<dyn CandidateSource>` (the wiring shape
    /// the composite expects).
    #[tokio::test]
    async fn source_is_object_safe_for_dyn_dispatch() {
        let src: Arc<dyn CandidateSource> = Arc::new(MustIncludeCandidateSource::new());
        let mut query = base_query();
        query.must_include = vec![ArtifactRef::Engram(EngramRef(art(99)))];
        let candidates = src.fetch(&query, &ctx()).await;
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].kind, PageKind::Engram);
    }

    // ─── Composite integration: the load-bearing test ──────────

    /// Stub tier helper for the composite-integration test.
    struct AlwaysPresentTier {
        present: Mutex<Vec<PageRef>>,
    }
    impl AlwaysPresentTier {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                present: Mutex::new(Vec::new()),
            })
        }
        fn add(&self, p: PageRef) {
            self.present.lock().push(p);
        }
    }
    #[async_trait]
    impl TierStore for AlwaysPresentTier {
        fn role(&self) -> TierRole {
            TierRole::Fast
        }
        async fn read(&self, page: PageRef) -> Result<PageHandle, TierError> {
            if self.present.lock().contains(&page) {
                Ok(PageHandle {
                    page,
                    tier_role: TierRole::Fast,
                    size_bytes: 1024,
                })
            } else {
                Err(TierError::PageNotFound { page })
            }
        }
        async fn write(
            &self,
            _: PageRef,
            _: ArtifactBlob,
            _: Provenance,
        ) -> Result<(), TierError> {
            Ok(())
        }
        async fn evict(&self, _: usize) -> Vec<EvictionRecord> {
            Vec::new()
        }
        fn capacity(&self) -> TierCapacity {
            TierCapacity {
                current_used: 0,
                configured_limit: 100_000_000,
            }
        }
        fn observe_access(&self, _: PageRef) {}
    }

    fn capacity_uma() -> WorkingSetCapacity {
        WorkingSetCapacity {
            fast_bytes: 1_000_000,
            warm_bytes: 0,
            max_pinned_bytes: 500_000,
        }
    }

    /// What this catches (the architectural payoff): with the
    /// recommended composite wiring (working-set FIRST,
    /// must-include SECOND, ByArtifactId dedup), an artifact that
    /// IS resident gets the working-set's Hot residency; an
    /// artifact that is must-include-but-not-resident gets the
    /// must-include's NotResident residency; both appear in the
    /// merged Vec. This is the spec's "hard pin MUST surface"
    /// contract met with proper residency semantics.
    #[tokio::test]
    async fn composite_with_dedup_resident_wins_must_include_for_pinned_hot_artifact() {
        let p = persona();
        let resident_page = PageRef {
            kind: PageKind::LoRALayer,
            artifact: art(100),
            offset: PageOffset::Whole,
        };

        // Set up working set with one resident page.
        let tier = AlwaysPresentTier::new();
        tier.add(resident_page);
        let mgr = Arc::new(LocalWorkingSetManager::new(vec![tier]));
        mgr.register_persona(p, capacity_uma());
        let _ = mgr.page_in(p, resident_page).await;

        // Compose: working-set FIRST (Hot wins), must-include SECOND.
        let composite = CompositeCandidateSource::new(
            vec![
                Arc::new(WorkingSetCandidateSource::new(mgr)),
                Arc::new(MustIncludeCandidateSource::new()),
            ],
            DedupPolicy::ByArtifactId,
        );

        // Query pins artifact 100 (also resident) + artifact 200
        // (not resident anywhere).
        let mut query = base_query();
        query.must_include = vec![
            ArtifactRef::LoRALayer(LoRALayerRef(art(100))),
            ArtifactRef::LoRALayer(LoRALayerRef(art(200))),
        ];

        let candidates = composite.fetch(&query, &RecallContext::cold_start(p)).await;

        // Two candidates total: resident artifact 100 (Hot) +
        // non-resident artifact 200 (NotResident).
        assert_eq!(candidates.len(), 2);

        let c_100 = candidates.iter().find(|c| c.artifact_id == art(100)).unwrap();
        match &c_100.residency {
            ResidencyHint::Hot { role } => assert_eq!(*role, TierRole::Fast),
            other => panic!("artifact 100 should be Hot (working-set won dedup), got {other:?}"),
        }

        let c_200 = candidates.iter().find(|c| c.artifact_id == art(200)).unwrap();
        match &c_200.residency {
            ResidencyHint::NotResident { acquirable_from } => {
                assert_eq!(*acquirable_from, AcquireSource::SentinelRefinement);
            }
            other => panic!("artifact 200 should be NotResident, got {other:?}"),
        }
    }
}
