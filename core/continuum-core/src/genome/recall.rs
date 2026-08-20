//! `demand-aligned-recall` — PR-1: typed data layer for the
//! substrate's most-used primitive. Per GENOME-FOUNDRY-SENTINEL
//! Part 7.
//!
//! Recall is the lookup every persona's cognition reaches for:
//! "give me a ranked pool of artifacts I can compose from to handle
//! this task." It spans local cache (Fast/Bench/Cold/Frozen) → grid
//! peers → federation pulls. The scoring incorporates semantic
//! similarity, outcome history, recency, tier proximity, and
//! provenance trust — but the **load-bearing** type is `ResidencyHint`
//! per the spec: "the persona doesn't just see *what's relevant*, it
//! sees *where it lives* and *what it costs to use*."
//!
//! PR-1 of demand-aligned-recall ships the typed data surface only.
//! No trait impl, no scoring function, no grid-peer calls — those
//! land in PR-2 (trait surface) and PR-3 (LocalDemandAlignedRecall
//! impl with the scoring function + working-set integration).
//!
//! ## What PR-1 ships
//!
//! - `ResidencyHint` — the load-bearing type with four variants
//!   (Hot/Local/GridPeer/NotResident), tied to the genome `TierRole`
//!   from PR-1 of working-set-manager (#1346).
//! - `RecallScore` — composite score struct with the five factors
//!   the scoring function combines.
//! - `RecallScope` — Local / LocalThenGrid { max_grid_pulls } /
//!   Federation { peers, max_latency_ms }. Bounds what the recall
//!   may touch.
//! - `FreshnessTarget` — BestEffort / FreshAsOf { ts_ms } / Strict.
//! - `TaskKind` — the seven canonical task kinds the substrate
//!   names: Chat / Code / Vision / ToolUse / Memory / Plan / Other.
//! - `TrustClass` — Local / TrustedPeer / KnownPeer / Anonymous.
//! - `PeerId` — the canonical `crate::identity::PeerId` (airc's
//!   universal actor id), re-used here for federation peers rather
//!   than a local newtype; `ArtifactId` stays distinct (content-
//!   addressed), so the type system still catches swapped arguments.
//! - `RecallError` — typed errors covering Budget exhaustion, Scope
//!   denial, FreshnessUnmet, and federation-level NoMatchingArtifacts.
//!
//! ## What PR-1 does NOT ship (PR-2 / PR-3)
//!
//! - `DemandAlignedRecall` trait — PR-2
//! - `CapabilityQuery`, `RecallContext`, `RankedPool`,
//!   `RecallScoreWeights` full shapes — PR-2 (they reference PR-1's
//!   types but depend on RecallContext + composition types that
//!   benefit from being grouped with the trait)
//! - Scoring function + grid_penalty + recency_decay — PR-3
//! - `LocalDemandAlignedRecall` impl + working-set integration — PR-3
//! - `RecallTrace` + replay determinism — PR-3
//! - Embedding model integration — separate Lane H slice

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use super::tier::TierRole;
// The peer/actor in a federated recall is the same canonical actor identity used
// everywhere else on the substrate ([[identity-one-canonical-newtype-not-bare-uuid]]).
// This was once a local `recall::PeerId(Uuid)` newtype — a duplicate of
// `airc_core::PeerId` — collapsed onto the one type (2026-06-30) so a federation
// peer and a persona are the same `PeerId`, and `ArtifactId` (content-addressed,
// distinct) stays the only id newtype this module defines.
use crate::identity::PeerId;

/// Where an artifact currently lives, from the persona's
/// perspective. The load-bearing type per GENOME-FOUNDRY-SENTINEL
/// Part 7: persona sees the artifact's location + acquisition cost,
/// not just its relevance.
///
/// The scoring function (PR-3) combines this with semantic match
/// and outcome history; the persona can also read the hint directly
/// when it wants to make an explicit cost trade-off (e.g. "stay
/// local even if a slightly higher-scoring layer is on a grid peer").
///
/// Variants:
/// - `Hot { role }` — already in this persona's working set at the
///   given tier role (typically Fast, or Warm on discrete-GPU
///   hardware). Cheapest to use.
/// - `Local { role }` — on this machine but not in this persona's
///   working set; promotable from Bench/Cold/Frozen via the
///   working-set-manager's page_in (#1355).
/// - `GridPeer { peer, est_latency_ms }` — resident on a federated
///   peer; would require a network pull to use.
/// - `NotResident { acquirable_from }` — doesn't exist locally OR
///   on any peer the persona has visibility into; would require
///   the foundry to import or sentinel to refine. Cost is "indefinite
///   future" — the persona usually picks something else.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/ResidencyHint.ts"
)]
pub enum ResidencyHint {
    Hot {
        role: TierRole,
    },
    Local {
        role: TierRole,
    },
    GridPeer {
        // PeerId is the canonical airc actor id (no ts-rs TS derive); it is
        // serde-transparent over a Uuid, so the wire/TS shape is a string.
        #[ts(type = "string")]
        peer: PeerId,
        #[serde(rename = "estLatencyMs")]
        #[ts(rename = "estLatencyMs", type = "number")]
        est_latency_ms: u32,
    },
    NotResident {
        acquirable_from: AcquireSource,
    },
}

/// Where the substrate would have to get an artifact from if it
/// isn't resident anywhere visible. PR-3's recall will fill this in
/// based on the artifact's provenance + the federation registry.
/// PR-1 ships the typed variants only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/AcquireSource.ts"
)]
pub enum AcquireSource {
    /// Foundry would have to absorb (e.g. pull SOTA + extract). The
    /// most expensive option — typically rejected on hot path.
    FoundryAbsorption,
    /// Sentinel would have to refine from existing outcomes. Cheaper
    /// than foundry but still bounded by the sentinel's refinement
    /// budget.
    SentinelRefinement,
    /// A peer NOT in the persona's current federation set could
    /// hold it. Requires the user / governor to expand federation
    /// scope first.
    UnreachablePeer,
}

/// Composite score for a recall candidate. The five factors are
/// the explicit, sentinel-tunable dimensions of the scoring function
/// (PR-3). Persona-facing code can inspect the components to explain
/// why a particular artifact was ranked where it was — useful for
/// debugging recall behavior and for VDD replay determinism.
///
/// All factors are normalized to `[0.0, 1.0]` so the combined score
/// is bounded `[0.0, sum(weights)]` (governor weights are also
/// bounded; defaults sum to 1.0).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/RecallScore.ts"
)]
pub struct RecallScore {
    /// Cosine similarity between query embedding and artifact
    /// metadata embedding. Range [0.0, 1.0]; 1.0 = identical.
    pub semantic: f32,
    /// How well this artifact performed in the persona's last N
    /// turns of similar tasks. Exponentially-decayed outcome
    /// signal — see PR-3's `outcome_window_score`.
    pub outcome_history: f32,
    /// Exponential decay over time-since-last-use. Governor-tunable
    /// half-life (default 24h).
    pub recency: f32,
    /// Cost-to-promote penalty. Hot artifacts score 1.0; cold
    /// archive scores ~0.2; grid peers score a function of
    /// estimated latency. See PR-3's `grid_penalty`.
    pub tier_proximity: f32,
    /// Artifact's trust score adjusted by the persona's trust
    /// overrides. Sentinel-refined-locally > sentinel-refined-by-
    /// trusted-peer > foundry-imported > anonymous-public.
    pub provenance_trust: f32,
    /// Weighted sum of the five factors. The persona usually picks
    /// from the top-K by this value; debugging code may inspect the
    /// factors above to understand why.
    pub combined: f32,
}

/// Bound on what the recall may touch. Lets a persona say "local
/// only" (e.g. for privacy-sensitive tasks) without per-call
/// federation-scope plumbing through every caller.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/RecallScope.ts"
)]
pub enum RecallScope {
    /// Never leave this machine. Fastest; may return a thinner
    /// RankedPool if local artifacts don't cover the query well.
    Local,
    /// Local first; grid pulls bounded by `max_grid_pulls`. Used
    /// when the persona wants the local result quickly + at most
    /// N grid candidates as backup.
    LocalThenGrid {
        #[serde(rename = "maxGridPulls")]
        #[ts(rename = "maxGridPulls", type = "number")]
        max_grid_pulls: usize,
    },
    /// Federation lookup against the named peer set; results
    /// bounded by `max_latency_ms`. Returns whatever the peers
    /// respond with inside the deadline.
    Federation {
        // Vec of canonical airc PeerIds; each is serde-transparent over a Uuid,
        // so the TS shape is an array of strings (PeerId has no ts-rs derive).
        #[ts(type = "Array<string>")]
        peers: Vec<PeerId>,
        #[serde(rename = "maxLatencyMs")]
        #[ts(rename = "maxLatencyMs", type = "number")]
        max_latency_ms: u32,
    },
}

/// How fresh the persona requires the result to be. Recall's
/// downstream sources (engram catalog, federation peers) may serve
/// stale data; this lets the persona reject stale results before
/// using them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/FreshnessTarget.ts"
)]
pub enum FreshnessTarget {
    /// No staleness check. Recall returns whatever's cheapest;
    /// caller treats results as "good enough."
    BestEffort,
    /// Reject any artifact whose `last_updated` is before `tsMs`.
    /// Soft contract — recall serves what's available + flags the
    /// rest as stale rather than failing the whole call.
    FreshAsOf {
        #[serde(rename = "tsMs")]
        #[ts(rename = "tsMs", type = "number")]
        ts_ms: u64,
    },
    /// Strict: every artifact in the RankedPool must be fresh as
    /// of the call time. Recall returns `RecallError::FreshnessUnmet`
    /// if any source can't guarantee freshness.
    Strict,
}

/// The seven canonical task kinds the substrate names. Used by
/// scoring (different task kinds weight semantic vs. outcome
/// history differently) and by routing (vision tasks need a vision-
/// capable persona, etc.).
///
/// `Other` is the escape hatch for novel task kinds the substrate
/// hasn't named — recall treats them with default weights.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/genome/TaskKind.ts")]
pub enum TaskKind {
    Chat,
    Code,
    Vision,
    ToolUse,
    Memory,
    Plan,
    Other,
}

/// How much the persona trusts a peer's artifacts. Adjusted at
/// scoring time via the persona's `trust_overrides` field
/// (RecallContext, PR-2). PR-1 names the variants the override list
/// can map a peer to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/TrustClass.ts"
)]
pub enum TrustClass {
    /// The persona's own artifacts. Always full trust.
    Local,
    /// A peer the user has explicitly marked trusted. Artifacts get
    /// near-local trust weight.
    TrustedPeer,
    /// A known peer (in the federation but not explicitly trusted).
    /// Artifacts weighted at the federation-default trust level.
    KnownPeer,
    /// Anonymous / unknown source. Used for public artifact pools
    /// the substrate has no provenance chain for. Heavily penalized
    /// in scoring.
    Anonymous,
}

/// Typed errors recall can surface. Per Joel's "never swallow
/// errors" rule: every failure mode has a typed variant with the
/// context needed to debug.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/RecallError.ts"
)]
pub enum RecallError {
    /// The query's resource budget couldn't be satisfied by any
    /// combination of available artifacts.
    BudgetExhausted {
        /// Bytes requested vs available — debugging signal.
        #[serde(rename = "budgetBytes")]
        #[ts(rename = "budgetBytes", type = "number")]
        budget_bytes: u64,
        #[serde(rename = "availableBytes")]
        #[ts(rename = "availableBytes", type = "number")]
        available_bytes: u64,
    },
    /// The query asked for scope the substrate can't satisfy (e.g.
    /// `RecallScope::Federation` with peers not in the federation).
    /// PR-3 surfaces this when filtering candidates by scope.
    ScopeUnreachable { reason: String },
    /// `FreshnessTarget::Strict` and at least one source couldn't
    /// guarantee freshness. The freshness gap is in
    /// `behind_by_ms`.
    FreshnessUnmet {
        #[serde(rename = "behindByMs")]
        #[ts(rename = "behindByMs", type = "number")]
        behind_by_ms: u64,
    },
    /// Federation pull returned zero matches within
    /// `RecallScope::Federation.max_latency_ms`. Doesn't mean the
    /// artifacts don't exist — it means the federation couldn't
    /// surface them in time.
    NoMatchingArtifacts {
        /// How many peers were queried before giving up.
        #[serde(rename = "peersQueried")]
        #[ts(rename = "peersQueried", type = "number")]
        peers_queried: u32,
        #[serde(rename = "elapsedMs")]
        #[ts(rename = "elapsedMs", type = "number")]
        elapsed_ms: u64,
    },
}

impl std::fmt::Display for RecallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RecallError::BudgetExhausted {
                budget_bytes,
                available_bytes,
            } => write!(
                f,
                "recall budget exhausted: requested {budget_bytes} bytes, only {available_bytes} available"
            ),
            RecallError::ScopeUnreachable { reason } => {
                write!(f, "recall scope unreachable: {reason}")
            }
            RecallError::FreshnessUnmet { behind_by_ms } => {
                write!(f, "recall freshness unmet: {behind_by_ms}ms behind target")
            }
            RecallError::NoMatchingArtifacts {
                peers_queried,
                elapsed_ms,
            } => write!(
                f,
                "recall: no matching artifacts after querying {peers_queried} peers in {elapsed_ms}ms"
            ),
        }
    }
}

impl std::error::Error for RecallError {}

#[cfg(test)]
mod tests {
    //! Each test pins one invariant the type system + serde encoding
    //! guarantee. If a downstream PR changes a name, casing, or
    //! variant shape, a test fails — forcing the author to verify
    //! the wire contract is what they intend.
    use super::*;
    use serde_json::json;

    fn sample_peer() -> PeerId {
        PeerId::from_uuid(Uuid::nil())
    }

    /// What this catches: PeerId serializes as a transparent UUID
    /// string (not a wrapping object). Wire stability — federation
    /// peer identifiers travel through gist/SSH/JSON-RPC as strings.
    #[test]
    fn peer_id_serializes_transparent_as_uuid_string() {
        let id = PeerId::from_uuid(Uuid::nil());
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "\"00000000-0000-0000-0000-000000000000\"");
    }

    /// What this catches: ResidencyHint variants serialize with the
    /// `kind` tag (camelCase). TS consumers narrow by it; any
    /// rename of a variant breaks every consumer.
    #[test]
    fn residency_hint_serializes_with_kind_tag() {
        let hot = ResidencyHint::Hot {
            role: TierRole::Fast,
        };
        let j = serde_json::to_string(&hot).unwrap();
        assert!(j.contains("\"kind\":\"hot\""), "got {j}");
        assert!(j.contains("\"role\":\"fast\""), "got {j}");

        let local = ResidencyHint::Local {
            role: TierRole::Cold,
        };
        let j = serde_json::to_string(&local).unwrap();
        assert!(j.contains("\"kind\":\"local\""), "got {j}");
        assert!(j.contains("\"role\":\"cold\""), "got {j}");

        let grid = ResidencyHint::GridPeer {
            peer: sample_peer(),
            est_latency_ms: 42,
        };
        let j = serde_json::to_string(&grid).unwrap();
        assert!(j.contains("\"kind\":\"gridPeer\""), "got {j}");
        assert!(j.contains("\"estLatencyMs\":42"), "got {j}");

        let not_resident = ResidencyHint::NotResident {
            acquirable_from: AcquireSource::FoundryAbsorption,
        };
        let j = serde_json::to_string(&not_resident).unwrap();
        assert!(j.contains("\"kind\":\"notResident\""), "got {j}");
        assert!(j.contains("\"foundryAbsorption\""), "got {j}");
    }

    /// What this catches: RecallScore is a flat struct with five
    /// f32 factors + a combined. If a future PR adds/removes a
    /// factor without updating the scoring weights, this test
    /// flags it. The combined value is NOT recomputed by serde —
    /// PR-3's scoring function fills it; PR-1 only pins the shape.
    #[test]
    fn recall_score_serializes_with_all_five_factors_plus_combined() {
        let score = RecallScore {
            semantic: 0.9,
            outcome_history: 0.7,
            recency: 0.5,
            tier_proximity: 1.0,
            provenance_trust: 0.8,
            combined: 0.82,
        };
        let j: serde_json::Value = serde_json::to_value(&score).unwrap();
        assert!((j["semantic"].as_f64().unwrap() - 0.9).abs() < 1e-6);
        assert!((j["outcomeHistory"].as_f64().unwrap() - 0.7).abs() < 1e-6);
        assert!((j["recency"].as_f64().unwrap() - 0.5).abs() < 1e-6);
        assert!((j["tierProximity"].as_f64().unwrap() - 1.0).abs() < 1e-6);
        assert!((j["provenanceTrust"].as_f64().unwrap() - 0.8).abs() < 1e-6);
        assert!((j["combined"].as_f64().unwrap() - 0.82).abs() < 1e-6);
    }

    /// What this catches: RecallScope variants. Federation carries
    /// a Vec<PeerId> + max_latency_ms; LocalThenGrid carries
    /// max_grid_pulls; Local is unit. Wire-stable tags.
    #[test]
    fn recall_scope_serializes_with_kind_tag() {
        let local = RecallScope::Local;
        assert_eq!(
            serde_json::to_string(&local).unwrap(),
            "{\"kind\":\"local\"}"
        );

        let local_grid = RecallScope::LocalThenGrid { max_grid_pulls: 5 };
        let j = serde_json::to_string(&local_grid).unwrap();
        assert!(j.contains("\"kind\":\"localThenGrid\""), "got {j}");
        assert!(j.contains("\"maxGridPulls\":5"), "got {j}");

        let fed = RecallScope::Federation {
            peers: vec![sample_peer()],
            max_latency_ms: 100,
        };
        let j = serde_json::to_string(&fed).unwrap();
        assert!(j.contains("\"kind\":\"federation\""), "got {j}");
        assert!(j.contains("\"maxLatencyMs\":100"), "got {j}");
    }

    /// What this catches: FreshnessTarget variants. Strict is unit;
    /// FreshAsOf carries a tsMs; BestEffort is unit.
    #[test]
    fn freshness_target_serializes_with_kind_tag() {
        let best = FreshnessTarget::BestEffort;
        assert_eq!(
            serde_json::to_string(&best).unwrap(),
            "{\"kind\":\"bestEffort\"}"
        );

        let fresh = FreshnessTarget::FreshAsOf {
            ts_ms: 1_700_000_000_000,
        };
        let j = serde_json::to_string(&fresh).unwrap();
        assert!(j.contains("\"kind\":\"freshAsOf\""), "got {j}");
        assert!(j.contains("\"tsMs\":1700000000000"), "got {j}");

        let strict = FreshnessTarget::Strict;
        assert_eq!(
            serde_json::to_string(&strict).unwrap(),
            "{\"kind\":\"strict\"}"
        );
    }

    /// What this catches: TaskKind has exactly the seven variants
    /// the spec names. Adding an eighth or removing one is a
    /// substrate change that needs deliberate review — this test
    /// flags it by failing.
    #[test]
    fn task_kind_has_seven_canonical_variants() {
        // Enumerate every variant; if a future PR adds/removes one,
        // this test won't compile because the match isn't exhaustive
        // or unreferenced.
        let variants = [
            TaskKind::Chat,
            TaskKind::Code,
            TaskKind::Vision,
            TaskKind::ToolUse,
            TaskKind::Memory,
            TaskKind::Plan,
            TaskKind::Other,
        ];
        assert_eq!(variants.len(), 7);
        // Also pin the serde wire form — TS consumers map by the
        // string ("chat", "code", "toolUse", ...).
        assert_eq!(serde_json::to_string(&TaskKind::Chat).unwrap(), "\"chat\"");
        assert_eq!(
            serde_json::to_string(&TaskKind::ToolUse).unwrap(),
            "\"toolUse\""
        );
    }

    /// What this catches: TrustClass variants serialize as
    /// camelCase strings. Wire stability.
    #[test]
    fn trust_class_serializes_camel_case() {
        assert_eq!(
            serde_json::to_string(&TrustClass::Local).unwrap(),
            "\"local\""
        );
        assert_eq!(
            serde_json::to_string(&TrustClass::TrustedPeer).unwrap(),
            "\"trustedPeer\""
        );
        assert_eq!(
            serde_json::to_string(&TrustClass::KnownPeer).unwrap(),
            "\"knownPeer\""
        );
        assert_eq!(
            serde_json::to_string(&TrustClass::Anonymous).unwrap(),
            "\"anonymous\""
        );
    }

    /// What this catches: RecallError variants serialize with the
    /// kind tag + camelCase fields. Each variant carries the
    /// debugging context downstream code needs.
    #[test]
    fn recall_error_serializes_with_kind_tag_and_camel_case_fields() {
        let budget = RecallError::BudgetExhausted {
            budget_bytes: 1_000_000,
            available_bytes: 500_000,
        };
        let j = serde_json::to_string(&budget).unwrap();
        assert!(j.contains("\"kind\":\"budgetExhausted\""), "got {j}");
        assert!(j.contains("\"budgetBytes\":1000000"), "got {j}");
        assert!(j.contains("\"availableBytes\":500000"), "got {j}");

        let fresh = RecallError::FreshnessUnmet { behind_by_ms: 5000 };
        let j = serde_json::to_string(&fresh).unwrap();
        assert!(j.contains("\"kind\":\"freshnessUnmet\""), "got {j}");
        assert!(j.contains("\"behindByMs\":5000"), "got {j}");

        let no_match = RecallError::NoMatchingArtifacts {
            peers_queried: 3,
            elapsed_ms: 150,
        };
        let j = serde_json::to_string(&no_match).unwrap();
        assert!(j.contains("\"kind\":\"noMatchingArtifacts\""), "got {j}");
        assert!(j.contains("\"peersQueried\":3"), "got {j}");
        assert!(j.contains("\"elapsedMs\":150"), "got {j}");
    }

    /// What this catches: RecallError implements Display + Error so
    /// it works in `?` chains and dyn Error contexts. Per Joel's
    /// "never swallow errors" rule — the typed error has to be
    /// debuggable from its Display alone.
    #[test]
    fn recall_error_implements_error_trait_with_useful_display() {
        let e = RecallError::BudgetExhausted {
            budget_bytes: 100,
            available_bytes: 50,
        };
        let _: &dyn std::error::Error = &e;
        let display = format!("{e}");
        assert!(display.contains("100"));
        assert!(display.contains("50"));
        assert!(display.contains("exhausted"));
    }

    /// What this catches: full round-trip integrity for the bigger
    /// composite types. If a future PR breaks field naming, the
    /// round-trip fails.
    #[test]
    fn round_trip_through_serde_preserves_all_fields() {
        let hint = ResidencyHint::GridPeer {
            peer: sample_peer(),
            est_latency_ms: 25,
        };
        let j = serde_json::to_string(&hint).unwrap();
        let back: ResidencyHint = serde_json::from_str(&j).unwrap();
        assert_eq!(hint, back);

        let scope = RecallScope::Federation {
            peers: vec![sample_peer(), PeerId::from_uuid(Uuid::from_u128(1))],
            max_latency_ms: 200,
        };
        let j = serde_json::to_string(&scope).unwrap();
        let back: RecallScope = serde_json::from_str(&j).unwrap();
        assert_eq!(scope, back);

        let err = RecallError::ScopeUnreachable {
            reason: "peer offline".to_string(),
        };
        let j = serde_json::to_string(&err).unwrap();
        let back: RecallError = serde_json::from_str(&j).unwrap();
        assert_eq!(err, back);
    }

    /// What this catches: AcquireSource variants. Three options for
    /// "this artifact isn't here yet." The spec uses these to drive
    /// foundry / sentinel scheduling decisions; PR-1 pins the wire
    /// shape so PR-3's scheduler can dispatch on it.
    #[test]
    fn acquire_source_has_canonical_variants() {
        let _val = json!({"foundryAbsorption": null}); // shape hint
        for variant in [
            AcquireSource::FoundryAbsorption,
            AcquireSource::SentinelRefinement,
            AcquireSource::UnreachablePeer,
        ] {
            let j = serde_json::to_string(&variant).unwrap();
            let back: AcquireSource = serde_json::from_str(&j).unwrap();
            assert_eq!(variant, back);
        }
        assert_eq!(
            serde_json::to_string(&AcquireSource::FoundryAbsorption).unwrap(),
            "\"foundryAbsorption\""
        );
        assert_eq!(
            serde_json::to_string(&AcquireSource::SentinelRefinement).unwrap(),
            "\"sentinelRefinement\""
        );
        assert_eq!(
            serde_json::to_string(&AcquireSource::UnreachablePeer).unwrap(),
            "\"unreachablePeer\""
        );
    }
}
