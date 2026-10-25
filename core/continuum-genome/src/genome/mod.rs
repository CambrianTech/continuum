//! Genome: a mind's learned skills (GENE-REUSE-FORK-MINT.md, GENOME-FOUNDRY-SENTINEL.md).
//!
//! - What genes EXIST: training (`fine_tuning`, `teach`), the decision to reuse / fork / mint
//!   (`competence`), each gene's identity in embedding space (`signature`), its trial and
//!   verdict (`gene_trial`), fitness (`fitness`, `fitness_ledger`).
//! - Which genes shape a TURN, and at what scale: `composer` (the phenotype, by distance).
//! - Where a node's loaded genes LIVE (device or host RAM), within the one device budget:
//!   `gene_residency`.
//! - Finding a gene or artifact to reuse: `recall*`, `candidate_source_store`, `tier::TierRole`.

pub mod adapter_manifest;
pub mod candidate_source_store;
pub mod fine_tuning;
pub mod fitness;
pub mod competence;
pub mod corpus_ref;
pub mod gym_grader;
pub mod fitness_ledger;
pub mod examiner;
pub mod gene_residency;
pub mod gene_trial;
pub mod composer;
pub mod signature;
pub mod recall;
pub mod recall_trait;
pub mod teach;
pub mod tier;
pub mod training_outcome;

pub use recall::{
    AcquireSource, FreshnessTarget, RecallError, RecallScope, RecallScore, ResidencyHint, TaskKind,
    TrustClass,
};
pub use recall_trait::{
    ArtifactRef, CapabilityQuery, CompositionHint, CompositionRef, DemandAlignedRecall, DomainHint,
    EngramRef, LoRALayerRef, MoEExpertRef, OutcomeWindow, RankedPool, RecallBudget, RecallContext,
    RecallScoreWeights, RecallTrace, TrajectoryHint, WeightSumOutOfBounds,
};
pub use tier::TierRole;
pub mod recall_scoring;
pub use recall_scoring::{
    grid_penalty, local_role_score, recency_decay, score as recall_score, tier_proximity_for,
    DEFAULT_RECENCY_HALF_LIFE_MS,
};
pub mod recall_impl;
pub use recall_impl::{CandidateArtifact, CandidateSource, LocalDemandAlignedRecall};
pub mod recall_source_composite;
pub use recall_source_composite::{CompositeCandidateSource, DedupPolicy};
