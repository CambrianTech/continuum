//! Genome — the substrate's cache hierarchy and paging data layer.
//!
//! The cache is a sequence of **tier roles** parameterized by hardware
//! class. Discrete-GPU hardware has five distinct tiers; unified-memory
//! hardware collapses the top two into one (Warm is omitted). The Rust
//! code is identical across hardware; only the `Vec<TierConfig>`
//! per-policy differs.
//!
//! PR-1 of working-set-manager (per MODULE-CATALOG §VII +
//! GENOME-FOUNDRY-SENTINEL Parts 2/3/4) ships the **data layer only**:
//! the typed surface that downstream PRs (trait + impl + dispatch
//! wiring) will hang behaviors on. No I/O, no async, no traits — just
//! the structs/enums + ts-rs exports + serde + a small unit-test pin
//! for each invariant the type system guarantees.
//!
//! This mirrors the shape that worked for CBAR-PIECE-2 PR-1 (#1321 —
//! ArtifactKey/Selector/Cadence types) + PIECE-5 PR-1 (#1331 — gate
//! types): land the data shape first, hang behaviors on it incrementally
//! across later PRs. Each subsequent PR is reviewable independently.
//!
//! ## PR-1 scope (this PR)
//!
//! - `TierRole` — Fast / Warm (discrete-GPU-only) / Bench / Cold / Frozen
//! - `EvictionPolicy` — per-role policy enum
//! - `TierCapacity` — current_used + configured_limit, both bytes
//! - `EvictionRecord` — typed event emitted when a page is evicted
//! - `PageKind` — LoRALayer / MoEExpert / KVCache / Engram
//! - `PageOffset` — sub-artifact offset (for MoE experts, KV chunks)
//! - `PageRef` — fully-qualified page address (kind + artifact + offset)
//! - `ResidentPage` — a page currently in some persona's working set
//! - `WorkingSetCapacity` — per-persona budget the governor sets
//! - `WorkingSet` — a persona's currently-resident pages
//! - `PageFault` — typed event when a page must be paged in
//! - `AccessDenied` — typed refusal from the MMU-style permission check
//!
//! ## PR-1 scope (NOT this PR — explicitly deferred)
//!
//! - `WorkingSetManager` trait — PR-2 of this stack
//! - `TierStore` trait + role-specific impls (5 of them) — separate PR set
//! - MMU permission table enforcement — PR-2 or PR-3 of this stack
//! - Wiring `PageFault` / `EvictionRecord` to the trace bus via my
//!   just-shipped artifact dispatch (#1339 + #1343) — PR-3 of this stack
//! - Hardware-anchor `Vec<TierConfig>` from the governor — separate PR
//!   (substrate-governor lane, codex's territory if they want it)
//!
//! ## Why types-only first
//!
//! Two reasons that compound:
//!
//! 1. **Compiler-enforced contract.** Naming a `TierRole` enum makes
//!    "L1→L2 eviction on UMA" structurally impossible because there is
//!    no `Warm` tier to evict to. The type system removes the need for
//!    runtime checks. Get the names right before the behaviors land.
//!
//! 2. **Multi-author shipping.** Codex + I are racing the MODULE-CATALOG
//!    queue. Naming the types first locks the seam every downstream PR
//!    builds against — codex's threat-detector + my working-set-manager
//!    impl + the next persona-cognition slice all subscribe to the same
//!    `PageFault` / `AccessDenied` shapes. PR-1's types are the
//!    coordination substrate.

pub mod blob;
pub mod bus;
pub mod candidate_source_store;
pub mod eviction;
pub mod expert_ingest;
pub mod expert_layout;
pub mod fine_tuning;
pub mod fitness;
pub mod gate_magnitude;
pub mod local_manager;
pub mod manager;
pub mod recall;
pub mod recall_trait;
pub mod residency;
pub mod store;
pub mod tier;
pub mod working_set;

pub use blob::{ArtifactBlob, Provenance};
pub use bus::{
    all_genome_artifact_selectors, publish_access_denied, publish_eviction_record,
    publish_page_fault, subscribe_to_genome_events, ACCESS_DENIED_KEY, EVICTION_RECORD_KEY,
    PAGE_FAULT_KEY,
};
pub use eviction::rank_pages_for_eviction;
pub use local_manager::LocalWorkingSetManager;
pub use manager::WorkingSetManager;
pub use recall::{
    AcquireSource, FreshnessTarget, RecallError, RecallScope, RecallScore, ResidencyHint, TaskKind,
    TrustClass,
};
pub use recall_trait::{
    ArtifactRef, CapabilityQuery, CompositionHint, CompositionRef, DemandAlignedRecall, DomainHint,
    EngramRef, LoRALayerRef, MoEExpertRef, OutcomeWindow, RankedPool, RecallBudget, RecallContext,
    RecallScoreWeights, RecallTrace, TrajectoryHint, WeightSumOutOfBounds,
};
pub use residency::GenomeResidencyModule;
pub use store::TierStore;
pub use tier::{EvictionPolicy, EvictionRecord, TierCapacity, TierError, TierRole};
pub use working_set::{
    AccessDenied, ArtifactId, PageFault, PageHandle, PageKind, PageOffset, PageRef, ResidentPage,
    WorkingSet, WorkingSetCapacity,
};
pub mod recall_scoring;
pub use recall_scoring::{
    grid_penalty, local_role_score, recency_decay, score as recall_score, tier_proximity_for,
    DEFAULT_RECENCY_HALF_LIFE_MS,
};
pub mod recall_impl;
pub use recall_impl::{CandidateArtifact, CandidateSource, LocalDemandAlignedRecall};
pub mod recall_source_working_set;
pub use recall_source_working_set::{WorkingSetCandidateSource, NEUTRAL_FACTOR_STUB};
pub mod recall_source_composite;
pub use recall_source_composite::{CompositeCandidateSource, DedupPolicy};
pub mod recall_source_must_include;
pub use recall_source_must_include::MustIncludeCandidateSource;
