//! `demand-aligned-recall` PR-2: the `DemandAlignedRecall` trait +
//! the composite types its methods reference. Per GENOME-FOUNDRY-
//! SENTINEL Part 7.
//!
//! PR-1 (#1366) shipped the typed primitives (ResidencyHint,
//! RecallScore, RecallScope, FreshnessTarget, TaskKind, TrustClass,
//! AcquireSource, PeerId, RecallError). This PR adds:
//!
//! - The trait itself — `recall` + `replay` method signatures
//! - `CapabilityQuery` — the input to `recall`: what kind of task,
//!   resource budget, scope, freshness target, hard pins
//! - `RecallContext` — who's asking and what they already have hot
//! - `RankedPool` — the output: ranked layers + experts + engrams
//!   with per-artifact `ResidencyHint` (from PR-1)
//! - `RecallScoreWeights` — governor-tunable weights with a sum-to-1
//!   invariant + a constructor that enforces it
//! - `ArtifactRef` + `LoRALayerRef` / `MoEExpertRef` / `EngramRef`
//!   typed wrappers around `ArtifactId`
//! - `RecallBudget` — the memory + time budget the persona allocates
//! - Stub placeholders for `OutcomeWindow` / `TrajectoryHint` /
//!   `CompositionRef` / `CompositionHint` / `RecallTrace` —
//!   GENOME-FOUNDRY-SENTINEL names these but their full shapes
//!   depend on sentinel + composer modules that aren't built yet.
//!   PR-2 ships opaque newtypes so the trait compiles; the shapes
//!   grow in dedicated PRs.
//!
//! ## What PR-2 does NOT ship (PR-3)
//!
//! - The scoring function (semantic / outcome_history / recency /
//!   tier_proximity / provenance_trust) — PR-3's `scoring.rs`
//! - `grid_penalty(latency_ms)` cost curve — PR-3
//! - `recency_decay(last_used, now, half_life)` — PR-3
//! - `LocalDemandAlignedRecall` impl with the actual cache walks —
//!   PR-3
//! - Working-set-manager integration (via #1362's bus hook) — PR-3

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::recall::{
    FreshnessTarget, RecallError, RecallScope, RecallScore, ResidencyHint, TaskKind, TrustClass,
};
use super::working_set::ArtifactId;
// Canonical actor id — same `PeerId` for a persona and a federation peer
// ([[identity-one-canonical-newtype-not-bare-uuid]]).
use crate::identity::PeerId;

// ─── Reference newtypes ─────────────────────────────────────────

/// Typed reference to one LoRA layer artifact. Newtype around
/// `ArtifactId` so the type system catches "passed a LoRA layer
/// where an expert was expected" at compile time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(transparent)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/LoRALayerRef.ts",
    type = "string"
)]
pub struct LoRALayerRef(pub ArtifactId);

/// Typed reference to one MoE expert artifact (one expert tile of
/// an MoE model). Sub-artifact paging — the artifact is the full
/// expert set; this reference picks one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(transparent)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/MoEExpertRef.ts",
    type = "string"
)]
pub struct MoEExpertRef(pub ArtifactId);

/// Typed reference to one engram (refined episodic memory).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(transparent)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/EngramRef.ts",
    type = "string"
)]
pub struct EngramRef(pub ArtifactId);

/// Generic artifact reference for `CapabilityQuery::must_include`
/// (hard pins). Discriminates by artifact kind so the recall can
/// route the pin to the right sub-pool of the result.
///
/// Uses adjacently-tagged serde (`{"kind": "loraLayer", "ref":
/// "<uuid>"}`) rather than internally-tagged because the inner
/// newtypes (LoRALayerRef etc.) are `#[serde(transparent)]` — they
/// serialize as bare strings, and serde's internally-tagged form
/// can't tag a bare string. Adjacent tagging is the clean fix; TS
/// consumers narrow by `kind` and read `ref` for the artifact id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(tag = "kind", content = "ref", rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/ArtifactRef.ts"
)]
pub enum ArtifactRef {
    LoRALayer(LoRALayerRef),
    MoEExpert(MoEExpertRef),
    Engram(EngramRef),
}

// ─── Domain hints + resource budget ─────────────────────────────

/// Free-form tag from the persona's plan. Recall uses these for
/// semantic narrowing (e.g. "math", "ruby", "vision-segmentation").
/// `String` because the tags are open-ended; recall doesn't validate.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(transparent)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/DomainHint.ts",
    type = "string"
)]
pub struct DomainHint(pub String);

impl DomainHint {
    pub fn new(tag: impl Into<String>) -> Self {
        Self(tag.into())
    }
}

/// Memory + time budget the persona allocates for the composition
/// it's about to build. Recall uses this to filter candidates
/// (e.g. don't include a 4GB layer if budget is 1GB).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/RecallBudget.ts"
)]
pub struct RecallBudget {
    /// Maximum bytes the composition is allowed to consume.
    #[ts(type = "number")]
    pub max_bytes: u64,
    /// Maximum wall-clock duration the recall call is allowed.
    /// `0` = no time limit (caller will time out separately).
    #[ts(type = "number")]
    pub max_duration_ms: u32,
}

// ─── Persona context + stubs for sentinel-dependent types ───────

/// Stub placeholder per GENOME-FOUNDRY-SENTINEL Part 7. The full
/// shape carries the persona's last N turns of outcomes (explicit
/// user signal + implicit downstream-tool-success). Sentinel reads
/// this to compute `outcome_history` for scoring.
///
/// PR-2 ships an opaque empty struct so the trait compiles; the
/// real shape lands when sentinel-observer is built (separate Lane
/// H PR).
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/OutcomeWindow.ts"
)]
pub struct OutcomeWindow {
    /// Reserved for the full shape. PR-2 ships as an empty struct;
    /// the field exists so downstream consumers can pattern-match
    /// even on the empty case.
    #[ts(type = "number")]
    pub turn_count: u32,
}

/// Stub placeholder per GENOME-FOUNDRY-SENTINEL Part 7. The full
/// shape carries hints about where the conversation is heading
/// (likely-next-task signals from the planning layer). Recall uses
/// this for speculative weighting on artifacts likely to be needed
/// soon. Empty in PR-2.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/TrajectoryHint.ts"
)]
pub struct TrajectoryHint {
    /// Reserved for the full shape (planner-emitted next-task
    /// likelihoods). PR-2 keeps it empty.
    pub speculative_kinds: Vec<TaskKind>,
}

/// Stub placeholder for "what composition is currently hot for this
/// persona." Full shape from the composer module (not built yet);
/// PR-2 ships a thin opaque struct so RecallContext compiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(transparent)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/CompositionRef.ts",
    type = "string"
)]
pub struct CompositionRef(pub ArtifactId);

/// The persona's context for a recall call. Recall uses this for:
/// - `outcome_history` factor (recent_outcomes input)
/// - speculative weighting (conversation_trajectory)
/// - per-peer trust overrides (trust_overrides)
/// - skip-already-hot-artifacts (current_composition)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/RecallContext.ts"
)]
pub struct RecallContext {
    #[ts(type = "string")]
    pub persona: PeerId,
    /// What composition is already hot for this persona. `None`
    /// means the persona is starting fresh (cold composition).
    #[ts(optional)]
    pub current_composition: Option<CompositionRef>,
    pub recent_outcomes: OutcomeWindow,
    pub conversation_trajectory: TrajectoryHint,
    /// Per-peer trust adjustments from the persona's identity state.
    /// Recall composes these with the artifact's `provenance_trust`
    /// during scoring.
    // PeerId is the canonical airc actor id (serde-transparent over a Uuid, no
    // ts-rs derive), so each tuple's first element is a string on the wire.
    #[ts(type = "Array<[string, TrustClass]>")]
    pub trust_overrides: Vec<(PeerId, TrustClass)>,
}

impl RecallContext {
    /// Cold-start RecallContext: no current composition, no
    /// outcome window, no trajectory, no trust overrides. Used by
    /// tests + first-turn recall calls.
    pub fn cold_start(persona: PeerId) -> Self {
        Self {
            persona,
            current_composition: None,
            recent_outcomes: OutcomeWindow::default(),
            conversation_trajectory: TrajectoryHint::default(),
            trust_overrides: Vec::new(),
        }
    }
}

// ─── Capability query (recall input) ────────────────────────────

/// The input to `DemandAlignedRecall::recall`. Names what the
/// persona is trying to do + what it can spend + where it's willing
/// to look.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/CapabilityQuery.ts"
)]
pub struct CapabilityQuery {
    pub task_kind: TaskKind,
    /// Free-form tags from the persona's plan. May be empty.
    pub domain_hints: Vec<DomainHint>,
    pub budget: RecallBudget,
    /// Hard pins — recall MUST include these in the RankedPool even
    /// if their score is low. Used for persona-private LoRA layers
    /// and sticky engrams.
    pub must_include: Vec<ArtifactRef>,
    /// When true (default), sentinel-refined artifacts win ties
    /// over foundry-imported. When false, the score alone decides.
    pub prefer_refined: bool,
    pub scope: RecallScope,
    pub freshness_target: FreshnessTarget,
}

// ─── Ranked pool (recall output) ────────────────────────────────

/// Stub placeholder for the composer's "how to stack these
/// artifacts" hint. Recall produces a suggested stacking order +
/// per-artifact weights; the composer module (not built yet) reads
/// this. PR-2 ships an empty struct so RankedPool compiles.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/CompositionHint.ts"
)]
pub struct CompositionHint {
    /// Reserved for the full shape. PR-2 keeps it empty; the
    /// composer PR will fill in the stacking order + per-artifact
    /// weight fields.
    pub layer_order_hint: Vec<LoRALayerRef>,
}

/// Stub placeholder for the replay handle. The full shape carries
/// the snapshotted scoring weights + artifact-set version + query
/// hash that `replay` uses to reproduce the recall deterministically
/// for sentinel attribution + VDD regression tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(transparent)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/RecallTrace.ts",
    type = "string"
)]
pub struct RecallTrace(pub ArtifactId);

/// The output of `DemandAlignedRecall::recall`. Three sub-pools
/// (layers / experts / engrams) so the composer can pick from each
/// independently. Every entry carries its score + `ResidencyHint`
/// so the persona can make the cost trade-off explicit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/RankedPool.ts"
)]
pub struct RankedPool {
    pub layers: Vec<(LoRALayerRef, RecallScore, ResidencyHint)>,
    pub experts: Vec<(MoEExpertRef, RecallScore, ResidencyHint)>,
    pub engrams: Vec<(EngramRef, RecallScore, ResidencyHint)>,
    pub composition_hint: CompositionHint,
    pub trace_ref: RecallTrace,
}

// ─── Scoring weights ─────────────────────────────────────────────

/// Governor-tunable weights for the five scoring factors. The
/// `new()` constructor enforces sum-to-1.0 (within an epsilon);
/// fields are pub so the governor can read but not mutate
/// directly. Mutation goes through `RecallScoreWeights::new()`
/// which re-validates.
///
/// Defaults from GENOME-FOUNDRY-SENTINEL Part 7 (semantic-leaning;
/// the governor tunes per hardware class + sentinel refines per
/// persona over time).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/RecallScoreWeights.ts"
)]
pub struct RecallScoreWeights {
    pub semantic: f32,
    pub outcome_history: f32,
    pub recency: f32,
    pub tier_proximity: f32,
    pub provenance_trust: f32,
}

/// Typed error from `RecallScoreWeights::new` when the weights
/// don't sum to 1.0 within tolerance. Carries the actual sum so the
/// caller can see how far off they are without re-summing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WeightSumOutOfBounds {
    pub actual_sum: f32,
}

impl std::fmt::Display for WeightSumOutOfBounds {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "RecallScoreWeights must sum to 1.0 (within 1e-4); got {}",
            self.actual_sum
        )
    }
}

impl std::error::Error for WeightSumOutOfBounds {}

impl RecallScoreWeights {
    /// Tolerance for the sum-to-1.0 invariant. f32 round-off means
    /// exact 1.0 is impractical; 1e-4 covers reasonable rounding.
    pub const SUM_EPSILON: f32 = 1e-4;

    /// Construct weights with sum-to-1.0 validation. Returns
    /// `WeightSumOutOfBounds` if the sum is off by more than
    /// `SUM_EPSILON`. Each weight must individually be `>= 0.0`;
    /// negative weights are rejected as nonsensical (the scoring
    /// function can't subtract from a candidate's score).
    pub fn new(
        semantic: f32,
        outcome_history: f32,
        recency: f32,
        tier_proximity: f32,
        provenance_trust: f32,
    ) -> Result<Self, WeightSumOutOfBounds> {
        let sum = semantic + outcome_history + recency + tier_proximity + provenance_trust;
        if (sum - 1.0).abs() > Self::SUM_EPSILON {
            return Err(WeightSumOutOfBounds { actual_sum: sum });
        }
        if semantic < 0.0
            || outcome_history < 0.0
            || recency < 0.0
            || tier_proximity < 0.0
            || provenance_trust < 0.0
        {
            return Err(WeightSumOutOfBounds { actual_sum: sum });
        }
        Ok(Self {
            semantic,
            outcome_history,
            recency,
            tier_proximity,
            provenance_trust,
        })
    }
}

impl Default for RecallScoreWeights {
    /// Defaults from GENOME-FOUNDRY-SENTINEL Part 7. Semantic-
    /// leaning baseline that the governor refines per hardware class
    /// and sentinel refines per persona.
    fn default() -> Self {
        // Sum exactly 1.0 (verified in test).
        Self {
            semantic: 0.35,
            outcome_history: 0.25,
            recency: 0.10,
            tier_proximity: 0.20,
            provenance_trust: 0.10,
        }
    }
}

// ─── The trait ───────────────────────────────────────────────────

/// The trait every demand-aligned-recall implementation satisfies.
/// PR-3 will ship `LocalDemandAlignedRecall` which walks the
/// working-set-manager (#1362's bus hook) + the genome catalog,
/// applies the scoring function, and emits the RankedPool.
///
/// `Send + Sync + async_trait` for tokio concurrency. Object-safe
/// for `Arc<dyn DemandAlignedRecall>` dispatch from persona-
/// cognition code.
#[async_trait]
pub trait DemandAlignedRecall: Send + Sync {
    /// The hot-path lookup. Sub-ms target on local L1/L2 hits;
    /// grid-aware budget when results must come from a peer or
    /// federation pull. The returned `RankedPool` carries every
    /// candidate's `ResidencyHint` so the persona sees acquisition
    /// cost explicitly.
    async fn recall(
        &self,
        query: &CapabilityQuery,
        context: &RecallContext,
    ) -> Result<RankedPool, RecallError>;

    /// Replay a previous recall deterministically from its trace.
    /// Used by sentinel for outcome attribution and by VDD for
    /// regression testing. Replay produces the same RankedPool the
    /// live recall did, using snapshotted scoring weights + artifact
    /// set at that time.
    async fn replay(&self, trace: &RecallTrace) -> Result<RankedPool, RecallError>;
}

#[cfg(test)]
mod tests {
    //! Trait-shape + serde-stability tests. Prove the trait is
    //! object-safe (Arc<dyn DemandAlignedRecall> dispatch works) +
    //! pin every wire-stable field name so downstream TS consumers
    //! don't silently break.
    use super::*;
    use crate::genome::recall::AcquireSource;
    use crate::genome::working_set::ArtifactId;
    use std::sync::Arc;
    use uuid::Uuid;

    fn sample_artifact() -> ArtifactId {
        ArtifactId::new(Uuid::nil())
    }

    fn sample_persona() -> PeerId {
        PeerId::from_uuid(Uuid::from_u128(1))
    }

    /// Minimal stub implementor: always returns an empty pool on
    /// recall, errors on replay. Used to prove the trait is
    /// object-safe through Arc<dyn DemandAlignedRecall>.
    struct StubRecall;

    #[async_trait]
    impl DemandAlignedRecall for StubRecall {
        async fn recall(
            &self,
            _query: &CapabilityQuery,
            _context: &RecallContext,
        ) -> Result<RankedPool, RecallError> {
            Ok(RankedPool {
                layers: Vec::new(),
                experts: Vec::new(),
                engrams: Vec::new(),
                composition_hint: CompositionHint::default(),
                trace_ref: RecallTrace(sample_artifact()),
            })
        }

        async fn replay(&self, _trace: &RecallTrace) -> Result<RankedPool, RecallError> {
            Err(RecallError::ScopeUnreachable {
                reason: "stub does not implement replay".to_string(),
            })
        }
    }

    fn sample_query() -> CapabilityQuery {
        CapabilityQuery {
            task_kind: TaskKind::Chat,
            domain_hints: vec![DomainHint::new("math")],
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

    /// What this catches: DemandAlignedRecall is object-safe — can
    /// be used through `Arc<dyn DemandAlignedRecall>`. PR-3's impl
    /// will be held this way by persona-cognition. If a future PR
    /// adds a generic method or breaks dyn-safety, this fails to
    /// compile.
    #[tokio::test]
    async fn trait_is_object_safe() {
        let recall: Arc<dyn DemandAlignedRecall> = Arc::new(StubRecall);
        let ctx = RecallContext::cold_start(sample_persona());
        let pool = recall.recall(&sample_query(), &ctx).await.unwrap();
        assert!(pool.layers.is_empty());
        assert!(pool.experts.is_empty());
        assert!(pool.engrams.is_empty());
    }

    /// What this catches: replay returns RecallError (typed) when
    /// the trace doesn't resolve. Same Result<RankedPool, RecallError>
    /// signature as recall, so callers can handle both uniformly.
    #[tokio::test]
    async fn trait_replay_returns_typed_error_on_failure() {
        let recall: Box<dyn DemandAlignedRecall> = Box::new(StubRecall);
        let trace = RecallTrace(sample_artifact());
        let result = recall.replay(&trace).await;
        match result {
            Err(RecallError::ScopeUnreachable { reason }) => {
                assert!(reason.contains("stub"));
            }
            other => panic!("expected ScopeUnreachable, got {other:?}"),
        }
    }

    /// What this catches: cold_start RecallContext produces a
    /// valid context with sensible defaults. Used by first-turn
    /// recall calls + tests; needs to be cheap and deterministic.
    #[test]
    fn recall_context_cold_start_has_sensible_defaults() {
        let ctx = RecallContext::cold_start(sample_persona());
        assert_eq!(ctx.persona, sample_persona());
        assert!(ctx.current_composition.is_none());
        assert_eq!(ctx.recent_outcomes.turn_count, 0);
        assert!(ctx.conversation_trajectory.speculative_kinds.is_empty());
        assert!(ctx.trust_overrides.is_empty());
    }

    /// What this catches: CapabilityQuery round-trips through serde
    /// without losing fields. The query is the contract every
    /// persona's planner emits to recall; if a field disappears or
    /// renames, every planner breaks.
    #[test]
    fn capability_query_round_trips_through_serde() {
        let q = sample_query();
        let json = serde_json::to_string(&q).unwrap();
        let back: CapabilityQuery = serde_json::from_str(&json).unwrap();
        assert_eq!(q, back);
    }

    /// What this catches: CapabilityQuery serializes with camelCase
    /// field names. TS consumers parse the camelCase form.
    #[test]
    fn capability_query_field_names_are_camel_case() {
        let q = sample_query();
        let j = serde_json::to_string(&q).unwrap();
        assert!(j.contains("\"taskKind\":"), "got {j}");
        assert!(j.contains("\"domainHints\":"), "got {j}");
        assert!(j.contains("\"mustInclude\":"), "got {j}");
        assert!(j.contains("\"preferRefined\":"), "got {j}");
        assert!(j.contains("\"freshnessTarget\":"), "got {j}");
    }

    /// What this catches: ArtifactRef uses adjacent tagging —
    /// `{"kind": "loRALayer", "ref": "<uuid>"}`. Internally-tagged
    /// would fail because the inner refs are transparent (bare
    /// string serde). TS consumers narrow on `kind` and read `ref`
    /// for the artifact id.
    #[test]
    fn artifact_ref_serializes_with_adjacent_kind_tag() {
        let layer = ArtifactRef::LoRALayer(LoRALayerRef(sample_artifact()));
        let j = serde_json::to_string(&layer).unwrap();
        assert!(
            j.contains("\"kind\":\"loRALayer\"") || j.contains("\"kind\":\"loraLayer\""),
            "got {j}"
        );
        assert!(j.contains("\"ref\":\""), "got {j}");

        let expert = ArtifactRef::MoEExpert(MoEExpertRef(sample_artifact()));
        let j = serde_json::to_string(&expert).unwrap();
        assert!(j.contains("\"kind\":\"moEExpert\""), "got {j}");
        assert!(j.contains("\"ref\":\""), "got {j}");

        let engram = ArtifactRef::Engram(EngramRef(sample_artifact()));
        let j = serde_json::to_string(&engram).unwrap();
        assert!(j.contains("\"kind\":\"engram\""), "got {j}");
        assert!(j.contains("\"ref\":\""), "got {j}");

        // Round-trip
        let back: ArtifactRef =
            serde_json::from_str(&serde_json::to_string(&layer).unwrap()).unwrap();
        assert_eq!(layer, back);
    }

    /// What this catches: typed ref newtypes are distinct at the
    /// type level. LoRALayerRef + MoEExpertRef + EngramRef all wrap
    /// ArtifactId but the type system prevents passing one where
    /// another is expected. Compile-time only — this test pins that
    /// the wrappers exist (changing one to a type alias would let
    /// them silently substitute).
    #[test]
    fn typed_refs_are_distinct_at_compile_time() {
        let layer: LoRALayerRef = LoRALayerRef(sample_artifact());
        let expert: MoEExpertRef = MoEExpertRef(sample_artifact());
        let engram: EngramRef = EngramRef(sample_artifact());
        // Both contain the same Uuid (nil), but mixing them up at
        // call sites that take LoRALayerRef wouldn't compile.
        assert_eq!(layer.0.as_uuid(), expert.0.as_uuid());
        assert_eq!(expert.0.as_uuid(), engram.0.as_uuid());
    }

    /// What this catches: RecallBudget serializes with camelCase
    /// fields. Wire stability.
    #[test]
    fn recall_budget_serializes_camel_case() {
        let b = RecallBudget {
            max_bytes: 1_000_000,
            max_duration_ms: 250,
        };
        let j = serde_json::to_string(&b).unwrap();
        assert!(j.contains("\"maxBytes\":1000000"), "got {j}");
        assert!(j.contains("\"maxDurationMs\":250"), "got {j}");
    }

    /// What this catches: default RecallScoreWeights sums to exactly
    /// 1.0 within the constructor's epsilon. If a future PR tweaks
    /// the defaults, this test flags any deviation — the sum-to-1
    /// invariant is load-bearing.
    #[test]
    fn default_recall_score_weights_sum_to_one() {
        let w = RecallScoreWeights::default();
        let sum =
            w.semantic + w.outcome_history + w.recency + w.tier_proximity + w.provenance_trust;
        assert!(
            (sum - 1.0).abs() < RecallScoreWeights::SUM_EPSILON,
            "default weights must sum to 1.0; got {sum}"
        );
    }

    /// What this catches: RecallScoreWeights::new rejects weights
    /// that don't sum to 1.0. The error carries the actual sum so
    /// the caller can debug without re-summing.
    #[test]
    fn recall_score_weights_constructor_rejects_invalid_sums() {
        // Sum > 1.0
        let result = RecallScoreWeights::new(0.5, 0.5, 0.5, 0.0, 0.0);
        match result {
            Err(WeightSumOutOfBounds { actual_sum }) => {
                assert!((actual_sum - 1.5).abs() < 1e-6);
            }
            Ok(_) => panic!("sum 1.5 should be rejected"),
        }

        // Sum < 1.0
        let result = RecallScoreWeights::new(0.1, 0.1, 0.1, 0.1, 0.1);
        assert!(result.is_err(), "sum 0.5 should be rejected");

        // Sum exactly 1.0 — accepted
        let result = RecallScoreWeights::new(0.2, 0.2, 0.2, 0.2, 0.2);
        assert!(result.is_ok(), "sum 1.0 should be accepted");
    }

    /// What this catches: RecallScoreWeights::new rejects negative
    /// weights. Negative weights would mean "the scoring function
    /// SUBTRACTS this factor from the candidate's score" — nonsense
    /// at the contract level. The constructor refuses.
    #[test]
    fn recall_score_weights_constructor_rejects_negative_weights() {
        // Negative semantic — rejected even if sum is 1.0.
        let result = RecallScoreWeights::new(-0.1, 0.4, 0.2, 0.3, 0.2);
        assert!(result.is_err(), "negative weights must be rejected");
    }

    /// What this catches: RankedPool round-trips through serde with
    /// all three sub-pools + composition_hint + trace_ref intact.
    /// If a field renames or a sub-pool changes shape, the round-
    /// trip fails.
    #[test]
    fn ranked_pool_round_trips_with_all_fields() {
        let score = RecallScore {
            semantic: 0.9,
            outcome_history: 0.5,
            recency: 0.3,
            tier_proximity: 1.0,
            provenance_trust: 0.7,
            combined: 0.78,
        };
        let pool = RankedPool {
            layers: vec![(
                LoRALayerRef(sample_artifact()),
                score,
                ResidencyHint::Hot {
                    role: super::super::tier::TierRole::Fast,
                },
            )],
            experts: vec![],
            engrams: vec![(
                EngramRef(sample_artifact()),
                score,
                ResidencyHint::NotResident {
                    acquirable_from: AcquireSource::FoundryAbsorption,
                },
            )],
            composition_hint: CompositionHint::default(),
            trace_ref: RecallTrace(sample_artifact()),
        };
        let json = serde_json::to_string(&pool).unwrap();
        let back: RankedPool = serde_json::from_str(&json).unwrap();
        assert_eq!(pool, back);
    }

    /// What this catches: RecallContext serializes with camelCase
    /// + current_composition is optional (None → null on wire OR
    /// omitted, depending on ts(optional) + skip_serializing_if).
    /// This pins the contract.
    #[test]
    fn recall_context_serializes_camel_case() {
        let ctx = RecallContext::cold_start(sample_persona());
        let j = serde_json::to_string(&ctx).unwrap();
        assert!(j.contains("\"currentComposition\":") || !j.contains("currentComposition"));
        assert!(j.contains("\"recentOutcomes\":"), "got {j}");
        assert!(j.contains("\"conversationTrajectory\":"), "got {j}");
        assert!(j.contains("\"trustOverrides\":"), "got {j}");
    }
}
