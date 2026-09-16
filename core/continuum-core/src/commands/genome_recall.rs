//! `genome/recall` — the demand-aligned recall engine's FIRST CALL SITE.
//!
//! The ranking engine (`genome/recall_impl` + `recall_scoring` + the candidate
//! sources) was fully built and fully tested with ZERO call sites (audited
//! 2026-08-22). This command wires it end-to-end over the real stores — the
//! adapter manifest, the signature sidecar, the eval-receipt fitness index —
//! and answers the substrate's default lookup: *"I need help with this; give me
//! a ranked pool."*
//!
//! Deliberately a COMMAND first, the cognition rung second: one discoverable
//! verb any driver (operator, citizen tool call, academy widget) can run and
//! read, with the verdict on the probe stream — the engine earns trust in the
//! open before it steers a live persona's model selection
//! ([[foolproof-over-instructions]]; the observation-before-control order the
//! exam-room doc uses for intervention verbs).

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::genome::recall_impl::{RecallDecision, RecallTraceStore};
use crate::genome::recall_trait::{RankedPool, RecallScoreWeights, RecallTrace};
use crate::genome::working_set::ArtifactId;
use crate::orm::OrmStore;
use crate::runtime::LateBound;
use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx};
use std::sync::Arc;

#[derive(Debug, Default, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/GenomeRecallParams.ts"
)]
pub struct GenomeRecallParams {
    /// What you need help with, in your own words ("refactor rust async code",
    /// "parse scheme s-expressions"). Embedded and matched by DISTANCE against
    /// every gene's minted signature — no keywords, no exact names.
    pub need: String,
    /// Max ranked genes to return (default 5).
    #[serde(default)]
    #[ts(optional)]
    pub limit: Option<u32>,
    /// Optional configuration of the versioned weighted recall policy.
    #[serde(default)]
    #[ts(optional)]
    pub policy: Option<GenomeRecallPolicy>,
}

/// Parameters for the existing weighted policy adapter. Policy identity is
/// versioned in every decision; this command does not bind a live persona.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/GenomeRecallPolicy.ts"
)]
pub struct GenomeRecallPolicy {
    /// Semantic, outcome, recency, tier proximity, provenance; nonnegative, sum 1.
    pub weights: [f32; 5],
    #[ts(type = "number")]
    pub half_life_ms: u64,
}
impl GenomeRecallPolicy {
    fn validated(&self) -> Result<RecallScoreWeights, CommandError> {
        let [s, o, r, t, p] = self.weights;
        RecallScoreWeights::new(s, o, r, t, p).map_err(|e| CommandError::Invalid(e.to_string()))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/GenomeRecallGene.ts"
)]
pub struct GenomeRecallGene {
    /// The gene's name (adapter alias — the id the page-in chain speaks).
    pub gene: String,
    /// Combined recall score (0..1), the ranking key.
    pub score: f32,
    /// Distance term: similarity of the need to the gene's minted signature
    /// (or its keyword fallback when unsigned).
    pub semantic: f32,
    /// Fitness term folded from eval receipts (neutral 0.5; >0.5 = measured
    /// lift, <0.5 = measured harm; includes the UCB audition bonus).
    pub fitness: f32,
    /// Whether a minted signature answered (vs the match-text fallback).
    pub signed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/GenomeRecallResult.ts"
)]
pub struct GenomeRecallResult {
    /// Durable decision UUID for genome/recall/replay. This is selection evidence,
    /// not a receipt that weights were loaded into an inference backend.
    pub trace_id: String,
    /// Ranked genes, best first. Empty = no genes registered on this node
    /// (the honest pre-first-adoption state, not an error).
    pub genes: Vec<GenomeRecallGene>,
    /// Genes registered in the manifest (the candidate universe size).
    #[ts(type = "number")]
    pub registered: u32,
    /// How many of those carry minted signatures.
    #[ts(type = "number")]
    pub signed: u32,
}

pub struct GenomeRecall {
    pub(crate) decisions: Arc<LateBound<OrmStore<RecallDecision>>>,
}

#[async_trait]
impl ActionCommand for GenomeRecall {
    const NAME: &'static str = "genome/recall";
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Ask the genome which genes help with a need, ranked by DISTANCE (embedding similarity \
         to each gene's minted signature) x FITNESS (folded from real eval receipts, with a \
         small exploration bonus for young genes) x recency/residency/trust — the demand-aligned \
         recall the resolver doctrine specifies (GENOME-REPOSITORY-ON-HF.md §2b). No keywords, \
         no exact names: 'parse scheme s-expressions' finds the functional-programming gene by \
         proximity. Empty result = no genes registered yet (run a training job to mint one). \
         Examples: `continuum genome/recall --need \"refactor rust async code\"`, \
         `continuum genome/recall --need \"design a landing page\" --limit 3`.";
    type Params = GenomeRecallParams;
    type Output = GenomeRecallResult;

    async fn run(
        &self,
        ctx: &Ctx,
        p: GenomeRecallParams,
    ) -> Result<GenomeRecallResult, CommandError> {
        use crate::genome::recall::{FreshnessTarget, RecallScope, TaskKind};
        use crate::genome::recall_trait::{
            CapabilityQuery, DomainHint, RecallBudget, RecallContext,
        };

        let policy_weights = p
            .policy
            .as_ref()
            .map(GenomeRecallPolicy::validated)
            .transpose()?;
        let decisions = Arc::clone(self.decisions.require().map_err(CommandError::Internal)?);
        let manifest = crate::forge::adapter_manifest::load().map_err(CommandError::Invalid)?;
        let sig_path =
            crate::genome::signature::signature_store_path().map_err(CommandError::Invalid)?;
        let signatures = crate::genome::signature::SignatureStore::load_at(&sig_path)
            .map_err(CommandError::Invalid)?;
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0); // pre-epoch clock: fitness decay saturates rather than the command failing
        let fitness = match crate::genome::fitness_ledger::GeneFitnessIndex::default_dir() {
            Some(dir) => crate::genome::fitness_ledger::GeneFitnessIndex::load(&dir, now_ms),
            None => Default::default(), // no HOME: empty index, every gene ranks neutral
        };

        // alias↔id map + which genes are signed, BEFORE the source consumes them.
        let id_of = |alias: &str| crate::genome::candidate_source_store::stable_local_id(alias);
        let signed_paths: std::collections::HashSet<String> =
            signatures.by_path.keys().cloned().collect();
        let alias_by_id: std::collections::HashMap<_, _> = manifest
            .iter()
            .map(|a| {
                (
                    id_of(&a.alias),
                    (
                        a.alias.clone(),
                        signed_paths.contains(&a.path.display().to_string()),
                    ),
                )
            })
            .collect();
        let registered = manifest.len() as u32;
        let signed_count = alias_by_id.values().filter(|(_, s)| *s).count() as u32;

        let embedder = crate::cognition::embedding::resolve_recall_embedder_local().await;
        let source = std::sync::Arc::new(
            crate::genome::candidate_source_store::GenomeStoreCandidateSource::from_manifest(
                &manifest,
                &signatures,
                &fitness,
                embedder,
            ),
        );
        let engine = match (&p.policy, policy_weights) {
            (Some(policy), Some(weights)) => {
                crate::genome::recall_impl::LocalDemandAlignedRecall::with_config_and_source(
                    weights,
                    policy.half_life_ms,
                    source,
                )
            }
            _ => crate::genome::recall_impl::LocalDemandAlignedRecall::with_source(source),
        }
        .with_trace_store(decisions);

        let query = CapabilityQuery {
            task_kind: TaskKind::Other, // the need's own words carry the domain; the enum routes later slices
            domain_hints: vec![DomainHint::new(&p.need)],
            budget: RecallBudget {
                max_bytes: 4_000_000_000,
                max_duration_ms: 2_000,
            },
            must_include: vec![],
            prefer_refined: true,
            scope: RecallScope::Local,
            freshness_target: FreshnessTarget::BestEffort,
        };
        let context = RecallContext::cold_start(
            ctx.caller
                .as_ref()
                .map(|c| c.peer_id)
                .unwrap_or_else(|| crate::identity::PeerId::from_uuid(uuid::Uuid::nil())),
        );
        use crate::genome::recall_trait::DemandAlignedRecall as _;
        let pool = engine
            .recall(&query, &context)
            .await
            .map_err(|e| CommandError::Invalid(format!("recall failed: {e:?}")))?;

        let limit = p.limit.unwrap_or(5) as usize; // documented default in DESCRIPTION — a display bound, nothing budgets on it
        let genes: Vec<GenomeRecallGene> = pool
            .layers
            .iter()
            .take(limit)
            .filter_map(|(layer_ref, score, _residency)| {
                alias_by_id
                    .get(&layer_ref.0)
                    .map(|(alias, signed)| GenomeRecallGene {
                        gene: alias.clone(),
                        score: score.combined,
                        semantic: score.semantic,
                        fitness: score.outcome_history,
                        signed: *signed,
                    })
            })
            .collect();

        // The verdict rides the event stream too — widgets and citizen
        // perception watch events, not command results.
        crate::probe!(
            class = "genome.recall",
            trace_id = %pool.trace_ref.0.as_uuid(),
            need = %p.need,
            registered = %registered,
            signed = %signed_count,
            top = %genes.first().map(|g| g.gene.as_str()).unwrap_or("-"), // probe display only: "-" = empty pool, a real state
            top_score = %genes.first().map(|g| g.score).unwrap_or(0.0), // probe display only: 0.0 alongside top="-" reads as no-pool
            "demand-aligned recall answered"
        );

        Ok(GenomeRecallResult {
            trace_id: pool.trace_ref.0.as_uuid().to_string(),
            genes,
            registered,
            signed: signed_count,
        })
    }
}

crate::register_command!(GenomeRecall);

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/GenomeRecallReplayParams.ts"
)]
pub struct GenomeRecallReplayParams {
    pub trace_id: String,
    /// Omit for exact replay; supply to compare a counterfactual configuration.
    #[serde(default)]
    #[ts(optional)]
    pub policy: Option<GenomeRecallPolicy>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/GenomeRecallReplayResult.ts"
)]
pub struct GenomeRecallReplayResult {
    #[schemars(with = "serde_json::Value")]
    pub decision: RecallDecision,
    #[schemars(with = "serde_json::Value")]
    pub replayed: RankedPool,
    pub counterfactual: bool,
    pub evaluated_policy: GenomeRecallPolicy,
}

pub struct GenomeRecallReplay {
    pub(crate) decisions: Arc<LateBound<OrmStore<RecallDecision>>>,
}

#[async_trait]
impl ActionCommand for GenomeRecallReplay {
    const NAME: &'static str = "genome/recall/replay";
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str = "Replay a retained genome recall decision without live catalog, embedding, or clock access. Optional policy changes evaluate a counterfactual against the same inputs; neither operation changes a persona or the original record. Example: continuum genome/recall/replay --traceId <UUID>. This replays ranking, not inference or weight acquisition.";
    type Params = GenomeRecallReplayParams;
    type Output = GenomeRecallReplayResult;

    async fn run(&self, ctx: &Ctx, p: Self::Params) -> Result<Self::Output, CommandError> {
        let id =
            uuid::Uuid::parse_str(&p.trace_id).map_err(|e| CommandError::Invalid(e.to_string()))?;
        let weights = p
            .policy
            .as_ref()
            .map(GenomeRecallPolicy::validated)
            .transpose()?;
        let store = self.decisions.require().map_err(CommandError::Internal)?;
        let decision = store
            .load_decision(RecallTrace(ArtifactId::new(id)))
            .await
            .map_err(|e| CommandError::Invalid(e.to_string()))?;
        // Capture includes the caller's need/context. Verified peers can inspect
        // their own decisions; local substrate/operator retains mechanic access.
        if crate::routing::grid_trust_policy::caller_trust(ctx.caller.as_ref())
            != crate::modules::grid::node::TrustLevel::Owner
            && ctx
                .caller
                .as_ref()
                .is_some_and(|c| c.peer_id != decision.context.persona)
        {
            return Err(CommandError::Denied(
                "recall decision belongs to a different caller".into(),
            ));
        }
        let original = decision
            .replay()
            .map_err(|e| CommandError::Invalid(e.to_string()))?;
        let replayed = match (&p.policy, weights) {
            (Some(policy), Some(weights)) => decision
                .evaluate(weights, policy.half_life_ms)
                .map_err(|e| CommandError::Invalid(e.to_string()))?,
            _ => original,
        };
        let counterfactual = p.policy.is_some();
        let evaluated_policy = p.policy.unwrap_or(GenomeRecallPolicy {
            weights: [
                decision.weights.semantic,
                decision.weights.outcome_history,
                decision.weights.recency,
                decision.weights.tier_proximity,
                decision.weights.provenance_trust,
            ],
            half_life_ms: decision.half_life_ms,
        });
        Ok(GenomeRecallReplayResult {
            decision,
            replayed,
            counterfactual,
            evaluated_policy,
        })
    }
}
crate::register_command!(GenomeRecallReplay);

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the command's identity + its help contract. The verb is
    // the engine's ONE discoverable door ([[foolproof-over-instructions]]) — the
    // description must carry runnable examples and name the distance doctrine, or
    // the next driver reinvents the lookup by hand.
    #[test]
    fn the_verb_is_discoverable_with_examples_and_distance_doctrine() {
        assert_eq!(GenomeRecall::NAME, "genome/recall");
        assert!(GenomeRecall::DESCRIPTION.contains("genome/recall --need"));
        assert!(GenomeRecall::DESCRIPTION.contains("DISTANCE"));
        assert!(GenomeRecall::DESCRIPTION.contains("FITNESS"));
    }
    // What this catches: serialized or programmatically supplied invalid policy
    // weights cannot poison captured rankings (NaN previously passed sum checks).
    #[test]
    fn policy_rejects_nonfinite_and_negative_weights() {
        for weights in [
            [f32::NAN, 0.0, 0.0, 0.0, 0.0],
            [f32::INFINITY, 0.0, 0.0, 0.0, 0.0],
            [-1.0, 2.0, 0.0, 0.0, 0.0],
        ] {
            assert!(GenomeRecallPolicy {
                weights,
                half_life_ms: 1000
            }
            .validated()
            .is_err());
        }
    }
    // What this catches: two callers sharing a database cannot replay each
    // other's private input; an authorized caller gets the real retained record.
    #[tokio::test]
    async fn replay_command_scopes_capture_to_verified_caller() {
        use crate::genome::recall::{FreshnessTarget, RecallScope, TaskKind};
        use crate::genome::recall_impl::LocalDemandAlignedRecall;
        use crate::genome::recall_trait::{
            CapabilityQuery, DemandAlignedRecall, DomainHint, RecallBudget, RecallContext,
        };
        use crate::identity::PeerId;
        let (adapter, _tmp) = crate::orm::store::fresh_adapter().await;
        let store = Arc::new(OrmStore::<RecallDecision>::new(adapter).await.unwrap());
        let owner = PeerId::from_uuid(uuid::Uuid::new_v4());
        let other = PeerId::from_uuid(uuid::Uuid::new_v4());
        let query = CapabilityQuery {
            task_kind: TaskKind::Other,
            domain_hints: vec![DomainHint::new("private project planning")],
            budget: RecallBudget {
                max_bytes: 1024,
                max_duration_ms: 100,
            },
            must_include: vec![],
            prefer_refined: true,
            scope: RecallScope::Local,
            freshness_target: FreshnessTarget::BestEffort,
        };
        let pool = LocalDemandAlignedRecall::new()
            .with_trace_store(store.clone())
            .recall(&query, &RecallContext::cold_start(owner))
            .await
            .unwrap();
        let decisions = Arc::new(LateBound::new("replay test"));
        decisions.install(store);
        let command = GenomeRecallReplay { decisions };
        let params = GenomeRecallReplayParams {
            trace_id: pool.trace_ref.0.as_uuid().to_string(),
            policy: None,
        };
        let ctx = |peer| Ctx {
            caller: Some(crate::routing::CallerIdentity::local_persona(peer)),
            ..Ctx::default()
        };
        assert!(command.run(&ctx(other), params.clone()).await.is_err());
        let result = command.run(&ctx(owner), params.clone()).await.unwrap();
        assert_eq!(result.replayed, pool);
        assert!(!result.counterfactual);
        assert_eq!(result.decision.query, query);
        let operator = Ctx {
            caller: Some(crate::routing::CallerIdentity::local(other)),
            ..Ctx::default()
        };
        assert!(
            command.run(&operator, params).await.is_ok(),
            "the authenticated local owner retains glass-box inspection"
        );
    }
}
