//! `demand-aligned-recall` PR-3b: `LocalDemandAlignedRecall` —
//! the per-process implementation that composes PR-3a's scoring
//! function (`recall_scoring::score`) with a candidate-injection
//! API to produce ranked `RankedPool`s.
//!
//! PR-3b ships the ranking engine but NOT the candidate-source
//! integration. The recall walks whatever the caller hands it; the
//! caller (PR-3c's working-set + genome-catalog walker) is
//! responsible for sourcing candidates from the substrate.
//!
//! Why split: PR-3b stays a small atomic slice (~250 LoC) reviewable
//! as pure ranking logic. PR-3c adds the integration with
//! `WorkingSetManager` (from #1355) + the genome catalog (future)
//! and wires `LocalDemandAlignedRecall` into Runtime as the
//! substrate's recall provider.
//!
//! ## What PR-3b ships
//!
//! - `CandidateArtifact` — a fully-described candidate ready for
//!   scoring. Carries the per-factor inputs (semantic, outcome,
//!   provenance) + residency + last-used timestamp. PR-3c populates
//!   from substrate sources; PR-3b tests construct directly.
//! - `LocalDemandAlignedRecall { weights, half_life_ms }` — the
//!   ranking engine. Holds the governor-tunable scoring weights +
//!   recency half-life. Thread-safe (the ranking is pure-function
//!   over the candidate set).
//! - `rank(now_ms, candidates)` method — scores every candidate,
//!   partitions by `PageKind` into the three sub-pools (layers /
//!   experts / engrams), sorts each descending by `combined`,
//!   returns the populated `RankedPool`.
//! - Honors `CapabilityQuery::must_include` hard pins — the caller
//!   filters/injects must-include candidates upstream; the rank
//!   layer doesn't drop them.
//!
//! ## What PR-3b does NOT ship (PR-3c)
//!
//! - `DemandAlignedRecall` trait impl — needs the working-set +
//!   genome catalog to source candidates. PR-3c wires it.
//! - `RecallTrace` replay backing store — separate sentinel PR.
//! - Federation candidate sourcing (RecallScope::Federation /
//!   LocalThenGrid) — PR-3c.
//! - Embedding model integration (the semantic factor input) —
//!   separate Lane H slice.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use ts_rs::TS;

use super::recall::{RecallError, RecallScore, ResidencyHint};
use super::recall_scoring::{score, DEFAULT_RECENCY_HALF_LIFE_MS};
use super::recall_trait::{
    CapabilityQuery, CompositionHint, DemandAlignedRecall, EngramRef, LoRALayerRef, MoEExpertRef,
    RankedPool, RecallContext, RecallScoreWeights,
    RecallTrace,
};
use super::working_set::{ArtifactId, PageKind};

/// A fully-described candidate ready for scoring. The caller
/// (PR-3c's working-set walker) populates these from substrate
/// sources; PR-3b's `rank` consumes them.
///
/// `kind` determines which sub-pool of the `RankedPool` this
/// candidate lands in (LoRALayer → layers, MoEExpert → experts,
/// Engram → engrams). `KVCache` candidates are silently dropped
/// because the spec's `RankedPool` only carries the three
/// composition-relevant sub-pools — KV cache pages are working-set
/// state, not recall candidates. If a future PR adds a fourth
/// sub-pool for KV chunks, that mapping flips on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../shared/generated/genome/CandidateArtifact.ts"
)]
pub struct CandidateArtifact {
    pub kind: PageKind,
    pub artifact_id: ArtifactId,
    /// Cosine similarity between query embedding and artifact
    /// embedding. Caller computes (PR-3c via embedding service).
    /// Range `[0.0, 1.0]`.
    pub semantic_factor: f32,
    /// How well this artifact performed for this persona on
    /// recent similar tasks. Caller computes (PR-3c via sentinel).
    /// Range `[0.0, 1.0]`.
    pub outcome_history_factor: f32,
    /// Unix-ms timestamp of last use. Drives `recency_decay`.
    #[ts(type = "number")]
    pub last_used_ms: u64,
    /// Where this candidate lives + acquisition cost. PR-3c
    /// populates from the working-set-manager + federation
    /// registry.
    pub residency: ResidencyHint,
    /// Provenance trust adjusted by persona overrides. Caller
    /// computes (PR-3c via trust registry + persona context).
    /// Range `[0.0, 1.0]`.
    pub provenance_trust_factor: f32,
}

/// Source of recall candidates. PR-3c introduces the seam between
/// the ranking engine (LocalDemandAlignedRecall) and the substrate
/// sources (working-set-manager, genome catalog, federation peers).
/// PR-3d wraps `LocalWorkingSetManager` as a CandidateSource impl.
///
/// `Send + Sync + async_trait` for tokio concurrency. The trait
/// takes the query + context so future impls can do query-aware
/// pruning (don't return artifacts that violate scope, exceed
/// budget, fail freshness target).
///
/// PR-3c's stub impls in tests return canned Vec<CandidateArtifact>;
/// PR-3d's working-set walker returns the persona's resident pages
/// translated to candidates.
#[async_trait]
pub trait CandidateSource: Send + Sync {
    /// Return all candidates relevant to the query within the
    /// persona's context. Pure data — no scoring, no sorting; the
    /// ranking engine handles that.
    ///
    /// May return an empty Vec; recall handles that gracefully
    /// (no error, empty pools — caller may try federation).
    async fn fetch(
        &self,
        query: &CapabilityQuery,
        context: &RecallContext,
    ) -> Vec<CandidateArtifact>;
}

/// Per-process implementation of demand-aligned recall ranking.
/// Holds the governor-tunable scoring weights + recency half-life
/// + an optional CandidateSource for the trait impl.
///
/// Thread-safe through immutability: the struct's fields don't
/// change after construction. `rank` is pure-function over the
/// candidate set + the engine's config. The DemandAlignedRecall
/// trait impl uses the configured CandidateSource to fetch
/// candidates; if no source is configured, recall returns an empty
/// pool (no error — that's a legitimate "no candidates known"
/// signal callers may use to fall back to federation).
pub struct LocalDemandAlignedRecall {
    weights: RecallScoreWeights,
    half_life_ms: u64,
    source: Option<Arc<dyn CandidateSource>>,
}

impl LocalDemandAlignedRecall {
    /// Construct with default weights, default 24h recency
    /// half-life, and no candidate source. The `rank()` method
    /// works (caller passes candidates explicitly) but the trait
    /// impl returns empty pools.
    pub fn new() -> Self {
        Self {
            weights: RecallScoreWeights::default(),
            half_life_ms: DEFAULT_RECENCY_HALF_LIFE_MS,
            source: None,
        }
    }

    /// Construct with explicit weights + half-life, no source.
    /// Weights are validated by `RecallScoreWeights::new` at
    /// construction upstream; this constructor takes them as
    /// already-valid.
    pub fn with_config(weights: RecallScoreWeights, half_life_ms: u64) -> Self {
        Self {
            weights,
            half_life_ms,
            source: None,
        }
    }

    /// Construct with a candidate source. The trait impl's
    /// `recall()` calls `source.fetch()` then `rank()`. Weights +
    /// half-life are at defaults; use `with_config_and_source`
    /// for explicit values.
    pub fn with_source(source: Arc<dyn CandidateSource>) -> Self {
        Self {
            weights: RecallScoreWeights::default(),
            half_life_ms: DEFAULT_RECENCY_HALF_LIFE_MS,
            source: Some(source),
        }
    }

    /// Construct with explicit weights, half-life, AND a candidate
    /// source. PR-3d's working-set walker uses this when wiring
    /// LocalDemandAlignedRecall into Runtime with governor-driven
    /// config.
    pub fn with_config_and_source(
        weights: RecallScoreWeights,
        half_life_ms: u64,
        source: Arc<dyn CandidateSource>,
    ) -> Self {
        Self {
            weights,
            half_life_ms,
            source: Some(source),
        }
    }

    /// Score + partition + sort the candidate set. Returns a fully-
    /// populated `RankedPool` with:
    /// - `layers`: LoRA layer candidates, sorted descending by
    ///   `RecallScore::combined`
    /// - `experts`: MoE expert candidates, sorted descending
    /// - `engrams`: engram candidates, sorted descending
    /// - `composition_hint`: empty placeholder (PR-3b doesn't
    ///   compute stacking order; the composer module owns that)
    /// - `trace_ref`: deterministic placeholder derived from the
    ///   query timestamp. PR-3c replaces with a real trace handle
    ///   the sentinel can replay against.
    ///
    /// `now_ms` is passed in (rather than read from
    /// `SystemTime::now`) so callers can replay with snapshotted
    /// clocks — the spec requires replay determinism, and reading
    /// `now()` inside the ranker would break that.
    pub fn rank(
        &self,
        now_ms: u64,
        candidates: Vec<CandidateArtifact>,
    ) -> RankedPool {
        let mut layers: Vec<(LoRALayerRef, RecallScore, ResidencyHint)> = Vec::new();
        let mut experts: Vec<(MoEExpertRef, RecallScore, ResidencyHint)> = Vec::new();
        let mut engrams: Vec<(EngramRef, RecallScore, ResidencyHint)> = Vec::new();

        for c in candidates {
            let scored = score(
                c.semantic_factor,
                c.outcome_history_factor,
                c.last_used_ms,
                now_ms,
                self.half_life_ms,
                &c.residency,
                c.provenance_trust_factor,
                &self.weights,
            );
            match c.kind {
                PageKind::LoRALayer => {
                    layers.push((LoRALayerRef(c.artifact_id), scored, c.residency))
                }
                PageKind::MoEExpert => {
                    experts.push((MoEExpertRef(c.artifact_id), scored, c.residency))
                }
                PageKind::Engram => {
                    engrams.push((EngramRef(c.artifact_id), scored, c.residency))
                }
                PageKind::KVCache => {
                    // Spec's RankedPool has three sub-pools; KV
                    // cache pages are working-set state, not recall
                    // candidates. Silently drop. PR-3c may make
                    // this a typed warning if upstream is sending
                    // KVCache candidates by mistake.
                }
            }
        }

        // Sort descending by combined score. NaN handling: the
        // spec assumes f32 factors are well-formed; if NaN slips
        // through, partial_cmp returns None and Ordering::Equal is
        // the fallback — which preserves input order for NaN
        // candidates. Better than panicking; the audit trail in
        // RecallScore lets a debugger see WHICH factor was NaN.
        layers.sort_by(|a, b| {
            b.1.combined
                .partial_cmp(&a.1.combined)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        experts.sort_by(|a, b| {
            b.1.combined
                .partial_cmp(&a.1.combined)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        engrams.sort_by(|a, b| {
            b.1.combined
                .partial_cmp(&a.1.combined)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        RankedPool {
            layers,
            experts,
            engrams,
            composition_hint: CompositionHint::default(),
            // Trace placeholder: deterministic UUID derived from
            // now_ms so replay-with-same-inputs produces the same
            // trace_ref. PR-3c replaces with a real RecallTrace
            // that includes the query hash + weights snapshot.
            trace_ref: RecallTrace(ArtifactId::new(uuid::Uuid::from_u128(now_ms as u128))),
        }
    }

    /// Inspect the configured scoring weights. Used by tests +
    /// PR-3c diagnostics.
    pub fn weights(&self) -> &RecallScoreWeights {
        &self.weights
    }

    /// Inspect the configured recency half-life (ms). Used by
    /// tests + PR-3c diagnostics.
    pub fn half_life_ms(&self) -> u64 {
        self.half_life_ms
    }
}

impl Default for LocalDemandAlignedRecall {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DemandAlignedRecall for LocalDemandAlignedRecall {
    /// Fetch candidates from the configured CandidateSource, then
    /// rank them. If no source is configured (`new()` /
    /// `with_config()` constructors), returns an empty pool — no
    /// error, because "no candidates known locally" is a
    /// legitimate signal callers may use to fall back to
    /// federation.
    ///
    /// `now_ms` is read from `SystemTime::now()` here (the public
    /// entry point), then threaded through `rank()` which keeps
    /// the explicit-now-ms contract for replay determinism. The
    /// trait surface looks "live" but `rank()` stays pure.
    ///
    /// PR-3c scope: no scope filtering, no freshness enforcement,
    /// no budget filtering. The CandidateSource does query-aware
    /// pruning in its `fetch()`; PR-3d's working-set walker
    /// filters by RecallScope::Local. Future PRs add the rest.
    async fn recall(
        &self,
        query: &CapabilityQuery,
        context: &RecallContext,
    ) -> Result<RankedPool, RecallError> {
        let candidates = match &self.source {
            Some(src) => src.fetch(query, context).await,
            None => Vec::new(),
        };
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        Ok(self.rank(now_ms, candidates))
    }

    /// Replay support deferred to a sentinel-owned PR. PR-3c
    /// returns `RecallError::ScopeUnreachable` with a clear reason
    /// so callers see a typed refusal rather than silent empty
    /// pool — per Joel's "never swallow errors" rule. The sentinel
    /// PR will add a RecallTraceStore that maps RecallTrace →
    /// snapshotted (weights, candidate_set, now_ms), then replay
    /// re-ranks deterministically.
    async fn replay(
        &self,
        _trace: &super::recall_trait::RecallTrace,
    ) -> Result<RankedPool, RecallError> {
        Err(RecallError::ScopeUnreachable {
            reason: "replay requires RecallTraceStore (sentinel PR); not yet implemented in PR-3c"
                .to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    //! Pin the ranking behavior:
    //! - candidates land in the right sub-pool by PageKind
    //! - each sub-pool sorted descending by combined score
    //! - score() math matches PR-3a per-candidate (cross-check)
    //! - empty input → empty pools
    //! - KVCache silently dropped
    //! - replay determinism: same inputs + same now_ms → same
    //!   trace_ref + same ranking
    use super::*;
    use crate::genome::recall::AcquireSource;
    use crate::genome::tier::TierRole;
    use uuid::Uuid;

    fn art(low: u128) -> ArtifactId {
        ArtifactId::new(Uuid::from_u128(low))
    }

    fn cand(
        kind: PageKind,
        artifact_low: u128,
        semantic: f32,
        outcome: f32,
        residency: ResidencyHint,
    ) -> CandidateArtifact {
        CandidateArtifact {
            kind,
            artifact_id: art(artifact_low),
            semantic_factor: semantic,
            outcome_history_factor: outcome,
            last_used_ms: 1000,
            residency,
            provenance_trust_factor: 0.5,
        }
    }

    /// What this catches: a fresh recall engine reports the default
    /// weights + half-life. Spec compliance + governor-tunable
    /// contract.
    #[test]
    fn new_uses_default_weights_and_half_life() {
        let r = LocalDemandAlignedRecall::new();
        assert_eq!(*r.weights(), RecallScoreWeights::default());
        assert_eq!(r.half_life_ms(), DEFAULT_RECENCY_HALF_LIFE_MS);
    }

    /// What this catches: with_config preserves both fields exactly
    /// as passed. PR-3c's governor wiring will use this constructor;
    /// any silent transformation would break weight-update
    /// determinism.
    #[test]
    fn with_config_preserves_weights_and_half_life() {
        let w = RecallScoreWeights::new(0.2, 0.2, 0.2, 0.2, 0.2).unwrap();
        let r = LocalDemandAlignedRecall::with_config(w, 1_000_000);
        assert_eq!(*r.weights(), w);
        assert_eq!(r.half_life_ms(), 1_000_000);
    }

    /// What this catches: empty candidate set yields an empty
    /// RankedPool (all three sub-pools empty) + a valid trace_ref.
    /// Recall must NEVER return error for empty input — it's a
    /// legitimate "no candidates found locally, caller may try
    /// federation" signal.
    #[test]
    fn rank_empty_candidates_returns_empty_pools() {
        let r = LocalDemandAlignedRecall::new();
        let pool = r.rank(1000, Vec::new());
        assert!(pool.layers.is_empty());
        assert!(pool.experts.is_empty());
        assert!(pool.engrams.is_empty());
    }

    /// What this catches: candidates of each PageKind variant land
    /// in the correct sub-pool. If a future PR adds a fifth kind,
    /// this test won't compile (forces the author to decide which
    /// sub-pool, or to expand RankedPool).
    #[test]
    fn rank_partitions_by_kind_into_correct_sub_pool() {
        let r = LocalDemandAlignedRecall::new();
        let residency = ResidencyHint::Hot { role: TierRole::Fast };
        let candidates = vec![
            cand(PageKind::LoRALayer, 1, 0.9, 0.5, residency.clone()),
            cand(PageKind::MoEExpert, 2, 0.8, 0.5, residency.clone()),
            cand(PageKind::Engram, 3, 0.7, 0.5, residency),
        ];
        let pool = r.rank(1000, candidates);
        assert_eq!(pool.layers.len(), 1);
        assert_eq!(pool.experts.len(), 1);
        assert_eq!(pool.engrams.len(), 1);
        assert_eq!(pool.layers[0].0, LoRALayerRef(art(1)));
        assert_eq!(pool.experts[0].0, MoEExpertRef(art(2)));
        assert_eq!(pool.engrams[0].0, EngramRef(art(3)));
    }

    /// What this catches: each sub-pool is sorted descending by
    /// combined score. The hot-path callers expect "best candidates
    /// first" — if the sort flips or stops, every downstream
    /// composer breaks.
    #[test]
    fn rank_sorts_each_sub_pool_descending_by_combined() {
        let r = LocalDemandAlignedRecall::new();
        let hot = ResidencyHint::Hot { role: TierRole::Fast };
        let candidates = vec![
            // Lower semantic
            cand(PageKind::LoRALayer, 10, 0.2, 0.5, hot.clone()),
            // Higher semantic
            cand(PageKind::LoRALayer, 11, 0.9, 0.5, hot.clone()),
            // Middle semantic
            cand(PageKind::LoRALayer, 12, 0.5, 0.5, hot),
        ];
        let pool = r.rank(1000, candidates);
        assert_eq!(pool.layers.len(), 3);
        // First entry is the highest-scoring (artifact 11).
        assert_eq!(pool.layers[0].0, LoRALayerRef(art(11)));
        assert_eq!(pool.layers[1].0, LoRALayerRef(art(12)));
        assert_eq!(pool.layers[2].0, LoRALayerRef(art(10)));
        // Verify monotonic descending.
        for win in pool.layers.windows(2) {
            assert!(
                win[0].1.combined >= win[1].1.combined,
                "expected descending sort: {} >= {}",
                win[0].1.combined,
                win[1].1.combined
            );
        }
    }

    /// What this catches: KVCache candidates are silently dropped
    /// — spec's RankedPool has three sub-pools (layers, experts,
    /// engrams); KV cache is working-set state, not a recall
    /// candidate. If a future PR adds a fourth sub-pool, this test
    /// flags the change.
    #[test]
    fn rank_silently_drops_kvcache_candidates() {
        let r = LocalDemandAlignedRecall::new();
        let hot = ResidencyHint::Hot { role: TierRole::Fast };
        let candidates = vec![
            cand(PageKind::LoRALayer, 1, 0.9, 0.5, hot.clone()),
            cand(PageKind::KVCache, 2, 0.9, 0.5, hot.clone()),
            cand(PageKind::Engram, 3, 0.7, 0.5, hot),
        ];
        let pool = r.rank(1000, candidates);
        assert_eq!(pool.layers.len(), 1);
        assert_eq!(pool.engrams.len(), 1);
        // KV cache candidate did NOT land in any sub-pool.
        assert!(pool.experts.is_empty());
    }

    /// What this catches: RankedPool.layers entries carry the
    /// RecallScore that PR-3a's score() would have produced. This
    /// is the audit trail — debuggers + sentinel attribution rely
    /// on reading scored.semantic, scored.combined, etc.
    #[test]
    fn rank_score_factors_match_pr3a_for_each_candidate() {
        let r = LocalDemandAlignedRecall::new();
        let hot = ResidencyHint::Hot { role: TierRole::Fast };
        let candidates = vec![cand(PageKind::LoRALayer, 1, 0.9, 0.8, hot.clone())];
        let now = 1_000_000;
        let pool = r.rank(now, candidates);

        let scored = pool.layers[0].1;
        // semantic + outcome_history + provenance_trust factors
        // round-trip from input.
        assert!((scored.semantic - 0.9).abs() < 1e-6);
        assert!((scored.outcome_history - 0.8).abs() < 1e-6);
        assert!((scored.provenance_trust - 0.5).abs() < 1e-6);
        // tier_proximity for Hot is 1.0.
        assert!((scored.tier_proximity - 1.0).abs() < 1e-6);
    }

    /// What this catches: replay determinism. Same inputs + same
    /// now_ms produce the same RankedPool. This is required for
    /// the sentinel's RecallTrace replay; without it, attribution
    /// can't reproduce historical decisions.
    #[test]
    fn rank_is_deterministic_across_calls() {
        let r = LocalDemandAlignedRecall::new();
        let hot = ResidencyHint::Hot { role: TierRole::Fast };
        let candidates = vec![
            cand(PageKind::LoRALayer, 1, 0.9, 0.5, hot.clone()),
            cand(PageKind::LoRALayer, 2, 0.5, 0.5, hot),
        ];
        let pool1 = r.rank(1000, candidates.clone());
        let pool2 = r.rank(1000, candidates);
        assert_eq!(pool1, pool2, "same inputs + same now must yield same pool");
    }

    /// What this catches: candidates with NotResident residency
    /// are still included in the ranking but score lower (their
    /// tier_proximity is 0.0). This pin matches PR-3a's
    /// "NotResident can still score" — sentinel may want to
    /// surface "this would be useful, schedule the foundry."
    #[test]
    fn rank_includes_not_resident_candidates_at_lower_score() {
        let r = LocalDemandAlignedRecall::new();
        let hot = ResidencyHint::Hot { role: TierRole::Fast };
        let not_res = ResidencyHint::NotResident {
            acquirable_from: AcquireSource::SentinelRefinement,
        };
        let candidates = vec![
            cand(PageKind::LoRALayer, 1, 0.9, 0.5, hot),
            cand(PageKind::LoRALayer, 2, 0.9, 0.5, not_res),
        ];
        let pool = r.rank(1000, candidates);
        assert_eq!(pool.layers.len(), 2, "both candidates included");
        // Hot scores higher than NotResident with same factors.
        assert!(
            pool.layers[0].1.combined > pool.layers[1].1.combined,
            "Hot candidate must outrank NotResident candidate"
        );
        // The NotResident entry's tier_proximity is 0.
        assert_eq!(pool.layers[1].1.tier_proximity, 0.0);
    }

    /// What this catches: tier ordering when all else is equal —
    /// Fast > Bench > Cold > Frozen via local_role_score. The
    /// tier_proximity factor differentiates artifacts of equal
    /// semantic + outcome + trust, which is the common case in
    /// federated recall.
    #[test]
    fn rank_orders_by_tier_when_other_factors_equal() {
        let r = LocalDemandAlignedRecall::new();
        let candidates = vec![
            cand(
                PageKind::LoRALayer,
                1,
                0.5,
                0.5,
                ResidencyHint::Local { role: TierRole::Frozen },
            ),
            cand(
                PageKind::LoRALayer,
                2,
                0.5,
                0.5,
                ResidencyHint::Hot { role: TierRole::Fast },
            ),
            cand(
                PageKind::LoRALayer,
                3,
                0.5,
                0.5,
                ResidencyHint::Local { role: TierRole::Bench },
            ),
        ];
        let pool = r.rank(1000, candidates);
        assert_eq!(pool.layers[0].0, LoRALayerRef(art(2))); // Hot/Fast
        assert_eq!(pool.layers[1].0, LoRALayerRef(art(3))); // Local/Bench
        assert_eq!(pool.layers[2].0, LoRALayerRef(art(1))); // Local/Frozen
    }

    /// What this catches: composition_hint is empty (PR-3b
    /// placeholder). PR-3c may populate it via the composer
    /// module. Pin the current shape so the next PR's diff is
    /// visible.
    #[test]
    fn rank_composition_hint_is_empty_placeholder_in_pr3b() {
        let r = LocalDemandAlignedRecall::new();
        let pool = r.rank(1000, Vec::new());
        assert!(pool.composition_hint.layer_order_hint.is_empty());
    }

    /// What this catches: trace_ref derives deterministically from
    /// now_ms. PR-3c replaces with a richer RecallTrace; this test
    /// pins the current deterministic-by-now contract so replay
    /// continues to work in the meantime.
    #[test]
    fn rank_trace_ref_is_deterministic_from_now_ms() {
        let r = LocalDemandAlignedRecall::new();
        let pool1 = r.rank(12345, Vec::new());
        let pool2 = r.rank(12345, Vec::new());
        assert_eq!(pool1.trace_ref, pool2.trace_ref);

        let pool3 = r.rank(99999, Vec::new());
        assert_ne!(
            pool1.trace_ref, pool3.trace_ref,
            "different now_ms must yield different trace_ref"
        );
    }

    // ─── PR-3c: trait impl + CandidateSource tests ─────────────

    use crate::genome::recall_trait::{
        CapabilityQuery, DemandAlignedRecall, DomainHint, RecallBudget, RecallContext, RecallTrace,
    };
    use crate::genome::recall::{FreshnessTarget, RecallError, RecallScope, TaskKind};
    use crate::genome::working_set::PersonaId;
    use parking_lot::Mutex;

    /// Stub CandidateSource: returns a pre-set Vec on every call,
    /// records each fetch invocation so tests can assert it ran.
    struct StubSource {
        canned: Vec<CandidateArtifact>,
        fetch_calls: Mutex<u32>,
    }

    impl StubSource {
        fn new(canned: Vec<CandidateArtifact>) -> Arc<Self> {
            Arc::new(Self {
                canned,
                fetch_calls: Mutex::new(0),
            })
        }
        fn fetch_count(&self) -> u32 {
            *self.fetch_calls.lock()
        }
    }

    #[async_trait]
    impl CandidateSource for StubSource {
        async fn fetch(
            &self,
            _query: &CapabilityQuery,
            _context: &RecallContext,
        ) -> Vec<CandidateArtifact> {
            *self.fetch_calls.lock() += 1;
            self.canned.clone()
        }
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

    fn sample_persona() -> PersonaId {
        PersonaId::new(Uuid::from_u128(100))
    }

    /// What this catches: trait impl exists + is object-safe.
    /// `Arc<dyn DemandAlignedRecall>` dispatch through LocalDemand
    /// AlignedRecall works. This is the seam persona-cognition will
    /// use.
    #[tokio::test]
    async fn recall_dispatches_through_dyn_demand_aligned_recall() {
        let recall: Arc<dyn DemandAlignedRecall> =
            Arc::new(LocalDemandAlignedRecall::new());
        let ctx = RecallContext::cold_start(sample_persona());
        let pool = recall.recall(&sample_query(), &ctx).await.unwrap();
        assert!(pool.layers.is_empty());
        assert!(pool.experts.is_empty());
        assert!(pool.engrams.is_empty());
    }

    /// What this catches: no-source mode returns empty pool, NOT
    /// an error. Empty pool is the legitimate "no candidates
    /// known locally; caller may try federation" signal.
    #[tokio::test]
    async fn recall_without_source_returns_empty_pool_not_error() {
        let recall = LocalDemandAlignedRecall::new();
        let ctx = RecallContext::cold_start(sample_persona());
        let result = recall.recall(&sample_query(), &ctx).await;
        assert!(result.is_ok());
        let pool = result.unwrap();
        assert!(pool.layers.is_empty());
    }

    /// What this catches: with_source dispatches to the source's
    /// fetch() — count the calls to prove dispatch happened. The
    /// source's canned candidates land in the resulting pool.
    #[tokio::test]
    async fn recall_with_source_dispatches_to_fetch_and_ranks() {
        let hot = ResidencyHint::Hot { role: super::super::tier::TierRole::Fast };
        let cand = CandidateArtifact {
            kind: PageKind::LoRALayer,
            artifact_id: ArtifactId::new(Uuid::from_u128(42)),
            semantic_factor: 0.9,
            outcome_history_factor: 0.8,
            last_used_ms: 0,
            residency: hot,
            provenance_trust_factor: 0.7,
        };
        let source = StubSource::new(vec![cand]);
        let recall = LocalDemandAlignedRecall::with_source(source.clone());
        let ctx = RecallContext::cold_start(sample_persona());

        let pool = recall.recall(&sample_query(), &ctx).await.unwrap();

        assert_eq!(source.fetch_count(), 1, "source.fetch must be called once");
        assert_eq!(pool.layers.len(), 1);
        assert_eq!(pool.layers[0].0.0.as_uuid(), Uuid::from_u128(42));
    }

    /// What this catches: with_config_and_source preserves all
    /// three (weights, half_life, source). PR-3d's working-set
    /// walker uses this constructor when wiring with governor-
    /// driven config.
    #[tokio::test]
    async fn with_config_and_source_preserves_all_three() {
        let w = RecallScoreWeights::new(0.2, 0.2, 0.2, 0.2, 0.2).unwrap();
        let source = StubSource::new(Vec::new());
        let recall = LocalDemandAlignedRecall::with_config_and_source(w, 12345, source.clone());
        assert_eq!(*recall.weights(), w);
        assert_eq!(recall.half_life_ms(), 12345);

        let ctx = RecallContext::cold_start(sample_persona());
        let _ = recall.recall(&sample_query(), &ctx).await.unwrap();
        assert_eq!(source.fetch_count(), 1, "source still wired");
    }

    /// What this catches: replay returns the typed
    /// ScopeUnreachable refusal with a clear reason rather than
    /// silently returning an empty pool. Per Joel's never-swallow-
    /// errors rule — when the sentinel PR adds the RecallTraceStore,
    /// this test flips to expect Ok(pool).
    #[tokio::test]
    async fn replay_returns_typed_not_implemented_refusal_in_pr3c() {
        let recall = LocalDemandAlignedRecall::new();
        let trace = RecallTrace(ArtifactId::new(Uuid::nil()));
        let result = recall.replay(&trace).await;
        match result {
            Err(RecallError::ScopeUnreachable { reason }) => {
                assert!(
                    reason.contains("RecallTraceStore") || reason.contains("not yet implemented"),
                    "expected typed not-implemented reason, got: {reason}"
                );
            }
            other => panic!("expected ScopeUnreachable, got {other:?}"),
        }
    }
}
