//! Dream consolidation: distilling her episodic memories into durable facts, reviewing her
//! beliefs, and folding in what she was taught (`docs/cognition/DREAM-CONSOLIDATION.md`).
//!
//! Her dream is a part of her mind ([`DreamPart`], `part.rs`): started on her bus, woken by her
//! experience, her rest and what asks it, its need from what waits for it. A pass
//! (`consolidate.rs`) clusters her fresh experience and asks the distiller (`distiller.rs`) for
//! one fact per cluster; the passes (`passes.rs`) admit what she learned and apply its supersession
//! verdicts. Refinement is LEARNED cognition (the model distills), never a hand-written filter
//! that reads her output and puppets it (`[[no-hardcoded-heuristics-to-steer-cognition]]`).

use std::collections::{BTreeMap, HashSet, VecDeque};
use std::sync::Arc;

use thiserror::Error;
use uuid::Uuid;

use crate::ai::adapter::AIProviderAdapter;
use crate::ai::types::{ChatMessage, TextGenerationRequest};
use crate::cognition::admission_state::AdmissionState;
use crate::cognition::engram::{AdmissionDecision, Engram, EngramKind, EngramOrigin, TrustState};
use consolidate::Passed;
use her::Her;

mod consolidate;
mod distiller;
mod her;
mod lesson;
mod part;
mod passes;
mod region;
mod review_boundary_ledger;

pub use distiller::*;
pub use part::*;
pub use region::*;
pub use review_boundary_ledger::*;
use passes::*;
use lesson::DistilledLesson;
pub use lesson::UnreadableLesson;

/// Cluster episodics that share a recall key. Each engram lands in at most one cluster (its FIRST
/// recall key, deterministic), and only patterns survive: a pattern is two or more episodes, and a
/// single one is not yet anything to generalize into a fact. Mechanical grouping; the learned step
/// is the distillation, not this.
fn cluster_by_recall_key(episodics: &[Engram]) -> Vec<Vec<Engram>> {
    let mut buckets: BTreeMap<String, Vec<Engram>> = BTreeMap::new();
    for e in episodics {
        if let Some(key) = e.recall_keys.first() {
            buckets.entry(key.clone()).or_default().push(e.clone());
        }
    }
    buckets
        .into_values()
        .filter(|bucket| bucket.len() > 1)
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
/// a seeded in-memory hippocampus, and her dream over a stub reflection source driven by the
/// heuristic adapter (driven by [`DreamPart::step`]).
#[cfg(any(test, feature = "test-fixtures"))]
pub mod test_support {
    use super::*;
    use crate::ai::heuristic_adapter::HeuristicInferenceAdapter;
    use crate::cognition::admission_state::AdmissionState;
    use crate::cognition::engram::{Engram, EngramKind, EngramOrigin, TrustState};
    use crate::cognition::recall_metadata::RecallMetadataRegistry;

    /// `persona` with a measured act need on the process's working set, as a resident has after her
    /// first act, so her dream can size itself (an unmeasured mind's distillation defers,
    /// `distiller::lens_terms`). 400 output tokens, 120 of them thought.
    pub fn measured(persona: Uuid) -> Uuid {
        crate::cognition::working_set::global().record_emission_in_memory(
            persona,
            400,
            120,
            crate::cognition::working_set::EmissionStop::Landed,
            1,
        );
        persona
    }

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

    /// Her dream over `admission`, with the heuristic adapter and no credit store.
    pub fn part_over(persona_id: Uuid, admission: Arc<AdmissionState>) -> DreamPart {
        part_over_with_store(persona_id, admission, None)
    }

    /// Her dream over `admission`, recovering accepted reviews from `boundary_store`.
    pub fn part_over_with_store(
        persona_id: Uuid,
        admission: Arc<AdmissionState>,
        boundary_store: Option<Arc<dyn ReviewBoundaryLedger>>,
    ) -> DreamPart {
        let mut part = DreamPart::new(
            persona_id,
            Arc::new(StubReflectionSource {
                persona_id,
                admission,
                adapter: Arc::new(HeuristicInferenceAdapter::new()),
                boundary_store,
                credit_unavailable: false,
            }),
        );
        part.her.lived_from = Some(0); // a test seeds her store first: her dream reads all of it
        part
    }
}
