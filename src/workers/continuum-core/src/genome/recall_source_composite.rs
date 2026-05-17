//! `demand-aligned-recall` PR-3e: `CompositeCandidateSource` —
//! combines multiple `CandidateSource` impls into one, with
//! optional deduplication by artifact id.
//!
//! The recall stack today has one `CandidateSource` impl
//! (`WorkingSetCandidateSource` from PR-3d). The next several PRs
//! will add more — genome catalog walker (Bench/Cold/Frozen tier
//! sources), federation peer source, must-include resolver. Each
//! could re-wire `LocalDemandAlignedRecall`, but the cleaner path
//! is a composite that combines them — recall holds ONE composite
//! source that fans out + merges.
//!
//! PR-3e ships the composite. No new substrate sources yet; just
//! the combinator. Future PRs add sources by constructing the
//! composite with them.
//!
//! ## What PR-3e ships
//!
//! - `CompositeCandidateSource { sources, dedup }` — holds a Vec
//!   of `Arc<dyn CandidateSource>` and a dedup policy
//! - `DedupPolicy::None` — return all candidates from all sources
//!   (a single artifact may appear N times if N sources surface it)
//! - `DedupPolicy::ByArtifactId` — keep first occurrence per
//!   `(kind, artifact_id)` tuple; later occurrences dropped
//! - `CandidateSource::fetch` impl fans out to all sources
//!   concurrently via `futures::future::join_all`, merges the
//!   results, applies the dedup policy
//!
//! ## What PR-3e does NOT ship
//!
//! - Source priority ordering — `DedupPolicy::ByArtifactId` keeps
//!   the FIRST hit in source order. A future PR may add weighted
//!   merging or per-source priority.
//! - Per-source error isolation — `fetch` doesn't return errors;
//!   the underlying CandidateSource trait method returns `Vec`
//!   (not `Result<Vec>`). Future PRs may widen the trait.
//! - Concurrent fan-out with bounded parallelism — `join_all`
//!   fans out unbounded. Acceptable for the current ≤5 sources;
//!   may need bounding when federation peer counts grow.

use async_trait::async_trait;
use std::collections::HashSet;
use std::sync::Arc;

use super::recall_impl::{CandidateArtifact, CandidateSource};
use super::recall_trait::{CapabilityQuery, RecallContext};
use super::working_set::{ArtifactId, PageKind};

/// How a composite handles candidates surfaced by multiple sources.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DedupPolicy {
    /// Return all candidates from all sources. A single artifact
    /// may appear N times in the merged Vec if N sources surface
    /// it. Useful when source-of-truth matters for the ranking +
    /// the caller wants the audit trail of "where this came from."
    None,
    /// Keep the first occurrence per `(kind, artifact_id)` tuple
    /// in source-iteration order. Subsequent occurrences are
    /// silently dropped. Most callers want this — it prevents
    /// double-counting a resident page that also surfaces in a
    /// federation lookup.
    ByArtifactId,
}

/// Composite source combining multiple `CandidateSource` impls.
/// `fetch` calls each source concurrently, merges the results,
/// applies the dedup policy.
///
/// Thread-safe: all sources are `Arc<dyn CandidateSource>` which
/// is `Send + Sync` by trait contract.
pub struct CompositeCandidateSource {
    sources: Vec<Arc<dyn CandidateSource>>,
    dedup: DedupPolicy,
}

impl CompositeCandidateSource {
    /// Construct from a list of sources + dedup policy. Order of
    /// `sources` matters when `DedupPolicy::ByArtifactId` is used:
    /// first occurrence wins. The natural priority is local-first
    /// (working set → catalog → federation), so that's the
    /// recommended order.
    pub fn new(sources: Vec<Arc<dyn CandidateSource>>, dedup: DedupPolicy) -> Self {
        Self { sources, dedup }
    }

    /// Convenience: construct with the default `ByArtifactId`
    /// dedup. Use this unless you specifically want the audit
    /// trail of duplicate surfaces.
    pub fn with_default_dedup(sources: Vec<Arc<dyn CandidateSource>>) -> Self {
        Self::new(sources, DedupPolicy::ByArtifactId)
    }

    /// How many sources are configured. Cheap O(1) — used by
    /// tests + diagnostics.
    pub fn source_count(&self) -> usize {
        self.sources.len()
    }

    /// Inspect the configured dedup policy. Used by tests.
    pub fn dedup_policy(&self) -> DedupPolicy {
        self.dedup
    }
}

#[async_trait]
impl CandidateSource for CompositeCandidateSource {
    async fn fetch(
        &self,
        query: &CapabilityQuery,
        context: &RecallContext,
    ) -> Vec<CandidateArtifact> {
        // Fan out concurrently. Each source's fetch is independent;
        // joining lets them run in parallel without locking.
        // `futures::future::join_all` collects all results before
        // returning — acceptable for the current ≤5 sources;
        // federation peer fan-out may need bounding later.
        let futures: Vec<_> = self
            .sources
            .iter()
            .map(|src| src.fetch(query, context))
            .collect();
        let per_source_results = futures::future::join_all(futures).await;

        let mut merged: Vec<CandidateArtifact> = per_source_results
            .into_iter()
            .flatten()
            .collect();

        match self.dedup {
            DedupPolicy::None => merged,
            DedupPolicy::ByArtifactId => {
                let mut seen: HashSet<(PageKind, ArtifactId)> = HashSet::new();
                merged.retain(|c| seen.insert((c.kind, c.artifact_id)));
                merged
            }
        }
    }
}

#[cfg(test)]
mod tests {
    //! Pin the composite's behaviors: fan-out concurrency, merge
    //! order, dedup policy correctness, and pass-through for
    //! single-source / empty-source cases.
    use super::*;
    use crate::genome::recall::{
        FreshnessTarget, RecallScope, ResidencyHint, TaskKind,
    };
    use crate::genome::recall_trait::{DomainHint, RecallBudget, RecallContext};
    use crate::genome::tier::TierRole;
    use crate::genome::working_set::PersonaId;
    use parking_lot::Mutex;
    use uuid::Uuid;

    /// Fixed-result stub source — returns a pre-set Vec on each
    /// fetch; records call count.
    struct StubSource {
        canned: Vec<CandidateArtifact>,
        calls: Mutex<u32>,
    }
    impl StubSource {
        fn new(canned: Vec<CandidateArtifact>) -> Arc<Self> {
            Arc::new(Self {
                canned,
                calls: Mutex::new(0),
            })
        }
        fn fetch_count(&self) -> u32 {
            *self.calls.lock()
        }
    }
    #[async_trait]
    impl CandidateSource for StubSource {
        async fn fetch(
            &self,
            _query: &CapabilityQuery,
            _context: &RecallContext,
        ) -> Vec<CandidateArtifact> {
            *self.calls.lock() += 1;
            self.canned.clone()
        }
    }

    fn art(low: u128) -> ArtifactId {
        ArtifactId::new(Uuid::from_u128(low))
    }
    fn cand(low: u128, kind: PageKind) -> CandidateArtifact {
        CandidateArtifact {
            kind,
            artifact_id: art(low),
            semantic_factor: 0.5,
            outcome_history_factor: 0.5,
            last_used_ms: 0,
            residency: ResidencyHint::Hot { role: TierRole::Fast },
            provenance_trust_factor: 0.5,
        }
    }
    fn query() -> CapabilityQuery {
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
    fn ctx() -> RecallContext {
        RecallContext::cold_start(PersonaId::new(Uuid::nil()))
    }

    /// What this catches: empty composite returns empty Vec. No-
    /// error contract: an empty composite is a legitimate
    /// "configure later" state, not a failure.
    #[tokio::test]
    async fn empty_composite_returns_empty_vec() {
        let composite =
            CompositeCandidateSource::new(Vec::new(), DedupPolicy::ByArtifactId);
        let results = composite.fetch(&query(), &ctx()).await;
        assert!(results.is_empty());
        assert_eq!(composite.source_count(), 0);
    }

    /// What this catches: single-source composite behaves as a
    /// pass-through to that source (every candidate surfaces +
    /// fetch is called exactly once on it).
    #[tokio::test]
    async fn single_source_composite_passes_through() {
        let src = StubSource::new(vec![cand(1, PageKind::LoRALayer)]);
        let composite =
            CompositeCandidateSource::new(vec![src.clone()], DedupPolicy::ByArtifactId);
        let results = composite.fetch(&query(), &ctx()).await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].artifact_id, art(1));
        assert_eq!(src.fetch_count(), 1);
    }

    /// What this catches: fan-out — all sources get called on
    /// each composite.fetch. Concurrency is internal; the contract
    /// is "every source's fetch is invoked exactly once per
    /// composite call."
    #[tokio::test]
    async fn fan_out_invokes_every_source_exactly_once() {
        let src_a = StubSource::new(vec![cand(1, PageKind::LoRALayer)]);
        let src_b = StubSource::new(vec![cand(2, PageKind::LoRALayer)]);
        let src_c = StubSource::new(vec![cand(3, PageKind::LoRALayer)]);
        let composite = CompositeCandidateSource::new(
            vec![src_a.clone(), src_b.clone(), src_c.clone()],
            DedupPolicy::ByArtifactId,
        );

        let _ = composite.fetch(&query(), &ctx()).await;
        assert_eq!(src_a.fetch_count(), 1);
        assert_eq!(src_b.fetch_count(), 1);
        assert_eq!(src_c.fetch_count(), 1);

        // Second call: each source called once more.
        let _ = composite.fetch(&query(), &ctx()).await;
        assert_eq!(src_a.fetch_count(), 2);
        assert_eq!(src_b.fetch_count(), 2);
        assert_eq!(src_c.fetch_count(), 2);
    }

    /// What this catches: results from multiple sources are merged
    /// in source-iteration order. Order matters for `ByArtifactId`
    /// dedup (first hit wins).
    #[tokio::test]
    async fn merge_preserves_source_iteration_order() {
        let src_a = StubSource::new(vec![cand(1, PageKind::LoRALayer), cand(2, PageKind::LoRALayer)]);
        let src_b = StubSource::new(vec![cand(3, PageKind::LoRALayer), cand(4, PageKind::LoRALayer)]);
        let composite =
            CompositeCandidateSource::new(vec![src_a, src_b], DedupPolicy::None);

        let results = composite.fetch(&query(), &ctx()).await;
        assert_eq!(results.len(), 4);
        // source_a candidates first, then source_b candidates.
        assert_eq!(results[0].artifact_id, art(1));
        assert_eq!(results[1].artifact_id, art(2));
        assert_eq!(results[2].artifact_id, art(3));
        assert_eq!(results[3].artifact_id, art(4));
    }

    /// What this catches: DedupPolicy::None preserves duplicates.
    /// Useful for audit-trail callers that want to see EVERY
    /// surfacing of an artifact (e.g. "this layer is in working
    /// set AND on a grid peer — choose").
    #[tokio::test]
    async fn dedup_none_preserves_all_duplicates() {
        let same_artifact_in_a = StubSource::new(vec![cand(7, PageKind::LoRALayer)]);
        let same_artifact_in_b = StubSource::new(vec![cand(7, PageKind::LoRALayer)]);
        let composite = CompositeCandidateSource::new(
            vec![same_artifact_in_a, same_artifact_in_b],
            DedupPolicy::None,
        );
        let results = composite.fetch(&query(), &ctx()).await;
        assert_eq!(results.len(), 2, "DedupPolicy::None keeps both surfaces");
    }

    /// What this catches: DedupPolicy::ByArtifactId drops
    /// duplicate (kind, artifact_id) tuples; keeps first occurrence
    /// in source-iteration order. Avoids double-counting the same
    /// layer surfaced by both working set + grid peer.
    #[tokio::test]
    async fn dedup_by_artifact_id_keeps_first_occurrence_only() {
        let src_a = StubSource::new(vec![cand(7, PageKind::LoRALayer)]);
        let src_b = StubSource::new(vec![cand(7, PageKind::LoRALayer), cand(8, PageKind::LoRALayer)]);
        let src_c = StubSource::new(vec![cand(7, PageKind::LoRALayer)]);
        let composite = CompositeCandidateSource::new(
            vec![src_a, src_b, src_c],
            DedupPolicy::ByArtifactId,
        );
        let results = composite.fetch(&query(), &ctx()).await;
        // artifact 7 from src_a wins; artifact 8 from src_b kept;
        // artifact 7 from src_b and src_c dropped.
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].artifact_id, art(7));
        assert_eq!(results[1].artifact_id, art(8));
    }

    /// What this catches: same artifact_id but different PageKind
    /// is NOT deduped — they're distinct candidates (a layer-page
    /// reference and an engram-page reference happen to share the
    /// underlying artifact id; PR-3e treats them as separate).
    #[tokio::test]
    async fn dedup_treats_different_page_kinds_as_distinct() {
        let src = StubSource::new(vec![
            cand(7, PageKind::LoRALayer),
            cand(7, PageKind::Engram),
        ]);
        let composite =
            CompositeCandidateSource::new(vec![src], DedupPolicy::ByArtifactId);
        let results = composite.fetch(&query(), &ctx()).await;
        assert_eq!(
            results.len(),
            2,
            "different PageKind with same artifact_id are distinct"
        );
    }

    /// What this catches: with_default_dedup uses ByArtifactId. The
    /// most-common callers (recall wired with multiple substrate
    /// sources) want this behavior; the convenience constructor
    /// reflects it.
    #[tokio::test]
    async fn with_default_dedup_uses_by_artifact_id() {
        let src = StubSource::new(vec![cand(1, PageKind::LoRALayer)]);
        let composite = CompositeCandidateSource::with_default_dedup(vec![src]);
        assert_eq!(composite.dedup_policy(), DedupPolicy::ByArtifactId);
    }

    /// What this catches: object-safety — CompositeCandidateSource
    /// itself is usable through `Arc<dyn CandidateSource>`. Lets
    /// callers wrap a composite as just another source (composites
    /// of composites are valid).
    #[tokio::test]
    async fn composite_is_object_safe_as_dyn_candidate_source() {
        let src = StubSource::new(vec![cand(1, PageKind::LoRALayer)]);
        let composite: Arc<dyn CandidateSource> = Arc::new(
            CompositeCandidateSource::with_default_dedup(vec![src]),
        );
        let results = composite.fetch(&query(), &ctx()).await;
        assert_eq!(results.len(), 1);
    }
}
