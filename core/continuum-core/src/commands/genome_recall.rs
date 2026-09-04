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

use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx};

#[derive(Debug, Default, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/genome/GenomeRecallParams.ts")]
pub struct GenomeRecallParams {
    /// What you need help with, in your own words ("refactor rust async code",
    /// "parse scheme s-expressions"). Embedded and matched by DISTANCE against
    /// every gene's minted signature — no keywords, no exact names.
    pub need: String,
    /// Max ranked genes to return (default 5).
    #[serde(default)]
    #[ts(optional)]
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/genome/GenomeRecallGene.ts")]
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
#[ts(export, export_to = "../../../protocol/typescript/genome/GenomeRecallResult.ts")]
pub struct GenomeRecallResult {
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

#[derive(Default)]
pub struct GenomeRecall;

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
        _ctx: &Ctx,
        p: GenomeRecallParams,
    ) -> Result<GenomeRecallResult, CommandError> {
        use crate::genome::recall::{FreshnessTarget, RecallScope, TaskKind};
        use crate::genome::recall_trait::{
            CapabilityQuery, DomainHint, RecallBudget, RecallContext,
        };

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
                    (a.alias.clone(), signed_paths.contains(&a.path.display().to_string())),
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
        let engine = crate::genome::recall_impl::LocalDemandAlignedRecall::with_source(source);

        let query = CapabilityQuery {
            task_kind: TaskKind::Other, // the need's own words carry the domain; the enum routes later slices
            domain_hints: vec![DomainHint::new(&p.need)],
            budget: RecallBudget { max_bytes: 4_000_000_000, max_duration_ms: 2_000 },
            must_include: vec![],
            prefer_refined: true,
            scope: RecallScope::Local,
            freshness_target: FreshnessTarget::BestEffort,
        };
        let context =
            RecallContext::cold_start(crate::identity::PeerId::from_uuid(uuid::Uuid::nil()));
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
                alias_by_id.get(&layer_ref.0).map(|(alias, signed)| GenomeRecallGene {
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
            need = %p.need,
            registered = %registered,
            signed = %signed_count,
            top = %genes.first().map(|g| g.gene.as_str()).unwrap_or("-"), // probe display only: "-" = empty pool, a real state
            top_score = %genes.first().map(|g| g.score).unwrap_or(0.0), // probe display only: 0.0 alongside top="-" reads as no-pool
            "demand-aligned recall answered"
        );

        Ok(GenomeRecallResult { genes, registered, signed: signed_count })
    }
}

crate::register_stateless_command!(GenomeRecall);

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
}
