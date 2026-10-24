//! Dream consolidation — distilling episodic memories into durable facts.
//!
//! This is the **consolidation/dream region** — "outlier B" of the
//! RAG-as-persistent-cache architecture (`docs/cognition/RAG-AS-PERSISTENT-CACHE.md`
//! + `docs/cognition/DREAM-CONSOLIDATION.md`). Where the live `ChannelDigestRegion`
//! (outlier A) is an *event-driven, no-LLM* servicer, this one is the most
//! different servicer: *intermittent, LLM-driven*. If both fit the same
//! `BrainRegion`/`ReadyBuffer` interface without forcing, the remaining slices
//! slot in (the methodical-process outlier-validation strategy).
//!
//! ## What this file is (slice 1: the distiller)
//!
//! The smallest, self-contained, deterministically-testable unit: given N
//! related episodic [`Engram`]s and an inference adapter, ask the model to
//! consolidate them into ONE durable semantic fact ([`DistilledFact`]).
//!
//! It deliberately does NOT:
//! - decide WHICH engrams to consolidate (the region's clustering job — a later
//!   slice),
//! - admit the result into the engram store (the self-admission path — a later
//!   slice; the engram store IS the `facts` persistence, not a bespoke buffer).
//!
//! It is pure: engrams in, one distilled fact out, source provenance preserved.
//! Refinement is LEARNED cognition (the model distills), never a hand-written
//! filter that reads the persona's output and puppets it — that would be the
//! exact anti-pattern this codebase forbids
//! (`[[no-hardcoded-heuristics-to-steer-cognition]]`).

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use thiserror::Error;
use uuid::Uuid;

use crate::ai::adapter::AIProviderAdapter;
use crate::ai::types::{ChatMessage, TextGenerationRequest};
use crate::cognition::admission_state::AdmissionState;
use crate::cognition::engram::{AdmissionDecision, Engram, EngramKind, EngramOrigin, TrustState};
use crate::runtime::brain_region::{BrainRegion, CadenceHint, ComputeClass, MemoryClass, Orientation, PressureProfile, PressureSignalKind, RegionContext, RegionId, TickOutcome};

mod consolidate;
mod distiller;
mod passes;
mod region;
mod review_boundary_ledger;

pub use distiller::*;
pub use region::*;
pub use review_boundary_ledger::*;
use passes::*;

#[async_trait]
impl BrainRegion for DreamConsolidationRegion {
    fn id(&self) -> RegionId {
        RegionId::from_static("dream-consolidation")
    }

    fn pressure_profile(&self) -> PressureProfile {
        PressureProfile {
            // Holds Arc to the source + per-persona sets of consolidated engram
            // ids (bounded by episodic count). Light.
            memory_class: MemoryClass::Light,
            // Runs LLM distillation in the tick — declared truthfully so the
            // governor can gate placement on leases/pressure once it hosts
            // inference (slice 4). This is the field that says "do not schedule
            // me like the digest region."
            compute_class: ComputeClass::InferenceHeavy,
            // Back off when the backend is saturated or the user is actively
            // engaged (a proxy for "not at rest" until `RegionContext` carries a
            // real sleep signal).
            responds_to: vec![
                PressureSignalKind::InferenceQueueDepth,
                PressureSignalKind::VramHigh,
                PressureSignalKind::SystemMemHigh,
                PressureSignalKind::UserActive,
            ],
        }
    }

    /// The dream draws from the **interiority** budget, never the reactive one.
    /// Consolidation is the being's own inner work (the [`Orientation`] doc names
    /// "dream/consolidation" as the canonical `SelfDirected` example), not
    /// stimulus it owes a response. This is what keeps the subconscious off the
    /// foreground path at the budget level: the governor's floored share means a
    /// dream tick can never preempt a reactive (responding) tick under
    /// contention, and a flood of reactive work can never fully starve the dream.
    fn orientation(&self) -> Orientation {
        Orientation::SelfDirected
    }

    /// The dream is per-persona, like the digest. A global tick (no persona
    /// scope) has nothing to consolidate — sleep until scoped.
    async fn tick(&self, ctx: &RegionContext) -> TickOutcome {
        match ctx.persona_scope {
            // A consolidation has nobody waiting on it (card 7496ed9d).
            Some(persona_id) => {
                crate::cognition::audience::with(
                    crate::inference::prefill_rate::Audience::Unattended,
                    self.consolidate(persona_id),
                )
                .await
            }
            None => sleep(),
        }
    }
}

/// Idle outcome that asks the governor to let the region sleep until experience
/// accrues — the organism resting, not a clock ticking.
fn sleep() -> TickOutcome {
    TickOutcome {
        cadence_hint: Some(CadenceHint::Sleep),
        ..TickOutcome::idle()
    }
}

/// Cluster episodics that share a recall key. Each engram lands in at most one
/// cluster (its FIRST recall key — deterministic), and only clusters of at least
/// `min_cluster` survive. Mechanical grouping; the learned step is the
/// distillation, not this.
fn cluster_by_recall_key(episodics: &[Engram], min_cluster: usize) -> Vec<Vec<Engram>> {
    let mut buckets: BTreeMap<String, Vec<Engram>> = BTreeMap::new();
    for e in episodics {
        if let Some(key) = e.recall_keys.first() {
            buckets.entry(key.clone()).or_default().push(e.clone());
        }
    }
    buckets
        .into_values()
        .filter(|bucket| bucket.len() >= min_cluster)
        .collect()
}

/// Build the `Semantic` engram for a distilled fact. Records provenance as
/// `EngramOrigin::SelfReflection` with the FIRST source as `parent_engram_id`
/// (the engram model carries a single parent today; multi-source provenance —
/// `parent_engram_ids: Vec<Uuid>` — is a named follow-up slice). The fact stays
/// retrievable by every source's keys via `recall_keys = fact.tags` (the union
/// the distiller already computed), so the single-parent gap does not cost
/// recall reach. SelfTrust: this is the persona's own cognition.
fn semantic_engram(fact: &DistilledFact) -> Engram {
    Engram {
        id: Uuid::new_v4(),
        context_id: None,
        kind: EngramKind::Semantic,
        content: fact.content.clone(),
        origin: EngramOrigin::SelfReflection {
            parent_engram_id: fact.source_ids.first().copied().unwrap_or_else(Uuid::nil), // JUSTIFIED: a fact with no recorded sources has no parent; nil is the engram model's no-parent id.
        },
        recall_keys: fact.tags.clone(),
        admitted_at_ms: continuum_base::time::EpochMs::now().millis(),
        trust_state_at_admission: TrustState::SelfTrust,
        admission_trace_id: None,
    }
}

/// A wanderer thought as an engram: `SelfReflection` KIND (inner speech about
/// her own history, not distilled world-knowledge — the kind axis separates it
/// from the consolidator's `Semantic` facts), same `SelfReflection` origin +
/// `SelfTrust` as any self-produced cognition. The content already carries its
/// `[thought:<lens>]` provenance tag (prefixed at synthesis in `distill_with`);
/// `thought:<lens>` is also added as a recall key so introspection can query
/// one lens's stream directly.
fn thought_engram(fact: &DistilledFact, lens: Lens) -> Engram {
    let mut engram = semantic_engram(fact);
    engram.kind = EngramKind::SelfReflection;
    engram.recall_keys.push(format!("thought:{}", lens.name));
    engram
}

/// Production reflection source: the process-global persona-workspace registry.
/// Every live mind's hippocampus + adapter are already retained there as the
/// fork-template handles — the dream resolves them per tick, never stores a
/// parallel persona→adapter map (the wiring contract documented on
/// [`PersonaReflectionSource`]). Mirrors persona's `impl PersonaChannelReader for
/// PersonaAircRuntimeRegistry`.
impl PersonaReflectionSource for crate::cognition::persona_workspace::PersonaWorkspaceRegistry {
    fn review_boundary_store(
        &self,
        persona_id: Uuid,
    ) -> Result<Option<Arc<dyn ReviewBoundaryLedger>>, continuum_client::ClientError> {
        review_boundary_ledger::resident_review_boundaries(persona_id)
    }
    fn live_personas(&self) -> Vec<Uuid> {
        self.roster().into_iter().map(|(id, _)| id).collect()
    }

    fn reflector_for(&self, persona_id: Uuid) -> Option<PersonaReflector> {
        self.reflector_handles(&persona_id)
            .map(|(admission, adapter, model)| PersonaReflector {
                admission,
                adapter,
                model,
            })
    }
}

#[cfg(test)]
pub(crate) mod tests;

/// The dream fixtures this module's tests and core's credit tests share: an episodic engram,
/// a seeded in-memory hippocampus, and a region over a stub reflection source driven by the
/// heuristic adapter.
#[cfg(any(test, feature = "test-fixtures"))]
pub mod test_support {
    use super::*;
    use crate::ai::heuristic_adapter::HeuristicInferenceAdapter;
    use crate::cognition::admission_state::AdmissionState;
    use crate::cognition::engram::{Engram, EngramKind, EngramOrigin, TrustState};
    use crate::cognition::recall_metadata::RecallMetadataRegistry;

    /// Build an episodic engram with a given id, content, and recall keys.
    pub fn episodic(id: Uuid, content: &str, recall_keys: &[&str]) -> Engram {
        Engram {
            id,
            context_id: None,
            kind: EngramKind::Episodic,
            content: content.to_string(),
            origin: EngramOrigin::SelfReflection {
                parent_engram_id: Uuid::nil(),
            },
            recall_keys: recall_keys.iter().map(|k| k.to_string()).collect(),
            admitted_at_ms: 1_000,
            trust_state_at_admission: TrustState::SelfTrust,
            admission_trace_id: None,
        }
    }

    /// A stub `PersonaReflectionSource` over one persona's in-memory
    /// hippocampus + the deterministic heuristic adapter. Mirrors the
    /// `StubChannels`/`StubReader` fixtures of `channel_digest_region`.
    pub struct StubReflectionSource {
        pub persona_id: Uuid,
        pub admission: Arc<AdmissionState>,
        pub adapter: Arc<dyn AIProviderAdapter>,
        pub boundary_store: Option<Arc<dyn ReviewBoundaryLedger>>,
        pub credit_unavailable: bool,
    }

    impl PersonaReflectionSource for StubReflectionSource {
        fn review_boundary_store(
            &self,
            _persona_id: Uuid,
        ) -> Result<Option<Arc<dyn ReviewBoundaryLedger>>, continuum_client::ClientError> {
            if self.credit_unavailable {
                return Err(continuum_client::ClientError::Transport(
                    "credit storage unavailable".into(),
                ));
            }
            Ok(self.boundary_store.clone())
        }
        fn live_personas(&self) -> Vec<Uuid> {
            vec![self.persona_id]
        }
        fn reflector_for(&self, persona_id: Uuid) -> Option<PersonaReflector> {
            (persona_id == self.persona_id).then(|| PersonaReflector {
                admission: self.admission.clone(),
                adapter: self.adapter.clone(),
                model: None,
            })
        }
    }

    /// Build a fresh in-memory hippocampus and seed it with the given
    /// episodics (via `admit_reflection`, which pushes whatever kind it's
    /// handed — the cleanest way to populate the real store in a test).
    pub fn seeded_admission(episodics: &[Engram]) -> Arc<AdmissionState> {
        let admission = Arc::new(AdmissionState::new(Arc::new(RecallMetadataRegistry::new())));
        for e in episodics {
            admission
                .admit_reflection(e.clone())
                .expect("seed episodic admits");
        }
        admission
    }

    /// Wait for the spawned dream pass to finish (the heuristic adapter is
    /// instant; this is scheduling latency only). Panics if it never does.
    pub async fn drain(region: &DreamConsolidationRegion) {
        for _ in 0..400 {
            if !region.dreaming() {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        panic!("dream pass did not complete");
    }

    pub fn region_over(
        persona_id: Uuid,
        admission: Arc<AdmissionState>,
    ) -> DreamConsolidationRegion {
        region_over_with_store(persona_id, admission, None)
    }

    pub fn region_over_with_store(
        persona_id: Uuid,
        admission: Arc<AdmissionState>,
        boundary_store: Option<Arc<dyn ReviewBoundaryLedger>>,
    ) -> DreamConsolidationRegion {
        DreamConsolidationRegion::new(Arc::new(StubReflectionSource {
            persona_id,
            admission,
            adapter: Arc::new(HeuristicInferenceAdapter::new()),
            boundary_store,
            credit_unavailable: false,
        }))
    }
}
