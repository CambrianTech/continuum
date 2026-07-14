//! `genome::candidate_source_store` — the LOCAL fetch-side adapter (#100 L5,
//! outlier A).
//!
//! The mirror of the publish side: publishing a layer is a [`Publisher`] adapter
//! (HF/grid); FINDING one is a [`CandidateSource`] adapter, and this is the local
//! store's impl. It surfaces the persona's own on-machine genome layers as
//! cosine-ranked candidates so [`LocalDemandAlignedRecall`] can pick the best-
//! fitting one for a task — the discovery half of "dynamic LoRA by need"
//! ([[dynamic-lora-by-project-directory-scope]]). A future `GridCandidateSource`
//! and an `HfCandidateSource` satisfy the SAME trait for peer/community layers, so
//! recall never learns where a candidate physically lives — exactly like inference
//! over base models and cloud APIs.
//!
//! Discovery ≠ residency: this names WHICH layers exist + how well each matches;
//! the paging engine decides which resident one to load. Cosine narrows to the
//! candidate set (this module); the five-factor `RecallScore` ranks within it.

use std::sync::Arc;

use async_trait::async_trait;

use super::recall::ResidencyHint;
use super::recall_impl::{CandidateArtifact, CandidateSource};
use super::recall_trait::{CapabilityQuery, RecallContext};
use super::tier::TierRole;
use super::working_set::{ArtifactId, PageKind};
use crate::cognition::embedding::{cosine_similarity, EmbeddingProvider};

/// One local genome layer as the recall source sees it — content-addressed
/// identity + the text that describes what it's for (its card/domain/keywords),
/// which is what gets embedded for the cosine match. The disk-walk /
/// paging-engine projection that produces these is the next wiring slice; this
/// module ranks whatever list it's given so the policy is testable in isolation.
#[derive(Debug, Clone, PartialEq)]
pub struct LocalGenomeLayer {
    /// Content-hash identity (the market's tag/line).
    pub artifact_id: ArtifactId,
    /// The layer's card/domain/keyword text — embedded for the cosine match.
    pub match_text: String,
    /// Epoch-ms of last use (recency factor).
    pub last_used_ms: u64,
    /// Provenance trust (local, refined layers score high).
    pub trust_factor: f32,
}

/// A [`CandidateSource`] over the local genome store. Holds the persona's layers
/// + an embedder; `fetch` embeds the query and each layer's match text and emits
/// candidates with the cosine as `semantic_factor` (the caller-computed field the
/// ranking engine expects).
pub struct GenomeStoreCandidateSource {
    layers: Vec<LocalGenomeLayer>,
    embedder: Arc<dyn EmbeddingProvider>,
}

impl GenomeStoreCandidateSource {
    pub fn new(layers: Vec<LocalGenomeLayer>, embedder: Arc<dyn EmbeddingProvider>) -> Self {
        Self { layers, embedder }
    }

    /// The query's text for embedding: its domain hints joined. (`task_kind` is a
    /// structured enum the RankedPool routes on; the domain hints carry the
    /// free-form "what I'm trying to do" the cosine matches against.)
    fn query_text(query: &CapabilityQuery) -> String {
        query
            .domain_hints
            .iter()
            .map(|h| h.0.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[async_trait]
impl CandidateSource for GenomeStoreCandidateSource {
    async fn fetch(
        &self,
        query: &CapabilityQuery,
        _context: &RecallContext,
    ) -> Vec<CandidateArtifact> {
        if self.layers.is_empty() {
            return Vec::new();
        }
        let q_emb = self.embedder.embed(&Self::query_text(query)).await;
        let mut out = Vec::with_capacity(self.layers.len());
        for layer in &self.layers {
            let l_emb = self.embedder.embed(&layer.match_text).await;
            let semantic_factor = cosine_similarity(&q_emb, &l_emb).clamp(0.0, 1.0);
            out.push(CandidateArtifact {
                kind: PageKind::LoRALayer,
                artifact_id: layer.artifact_id,
                semantic_factor,
                // No sentinel outcome history wired to the local store yet — the
                // ranker weights this at 0 here; the sentinel-attribution slice
                // fills it. Honest 0.0, never a fabricated score.
                outcome_history_factor: 0.0,
                last_used_ms: layer.last_used_ms,
                // On this machine, on SSD (Cold tier) — a load, not a page fault.
                residency: ResidencyHint::Local { role: TierRole::Cold },
                provenance_trust_factor: layer.trust_factor,
            });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cognition::embedding::LexicalEmbedder;
    use crate::genome::recall::{FreshnessTarget, RecallScope, TaskKind};
    use crate::genome::recall_trait::{DomainHint, RecallBudget};
    use uuid::Uuid;

    fn layer(text: &str) -> LocalGenomeLayer {
        LocalGenomeLayer {
            artifact_id: ArtifactId(Uuid::new_v4()),
            match_text: text.to_string(),
            last_used_ms: 1000,
            trust_factor: 0.9,
        }
    }

    fn query_for(domain: &str) -> CapabilityQuery {
        CapabilityQuery {
            task_kind: TaskKind::Chat,
            domain_hints: vec![DomainHint::new(domain)],
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

    // what this catches: the local source must surface layers as candidates whose
    // semantic_factor is the cosine of query↔layer text — so the layer that MATCHES
    // the task ranks higher. This is the discovery half of the market; if the cosine
    // isn't wired, recall can't tell a code LoRA from a poetry LoRA.
    #[tokio::test]
    async fn ranks_local_layers_by_cosine_to_the_query() {
        let embedder = Arc::new(LexicalEmbedder::new());
        let code = layer("rust code editing and refactoring");
        let code_id = code.artifact_id;
        let poetry = layer("lyrical poetry and creative verse");
        let poetry_id = poetry.artifact_id;
        let source = GenomeStoreCandidateSource::new(vec![code, poetry], embedder);

        let cands = source
            .fetch(&query_for("rust code refactoring"), &RecallContext::cold_start(crate::identity::PeerId::from_uuid(Uuid::nil())))
            .await;
        assert_eq!(cands.len(), 2, "both layers surface as candidates");

        let sem = |id: ArtifactId| {
            cands.iter().find(|c| c.artifact_id == id).unwrap().semantic_factor
        };
        assert!(
            sem(code_id) > sem(poetry_id),
            "the code layer matches the code query more closely (cosine): code={} poetry={}",
            sem(code_id),
            sem(poetry_id)
        );
        // Kind + residency are what the ranker/pager route on.
        assert!(cands.iter().all(|c| c.kind == PageKind::LoRALayer));
        assert!(cands
            .iter()
            .all(|c| matches!(c.residency, ResidencyHint::Local { .. })));
    }

    // what this catches: an empty store yields no candidates (recall then falls
    // back to grid/HF sources), never a panic or a fabricated hit.
    #[tokio::test]
    async fn empty_store_yields_no_candidates() {
        let source = GenomeStoreCandidateSource::new(vec![], Arc::new(LexicalEmbedder::new()));
        let cands = source
            .fetch(&query_for("anything"), &RecallContext::cold_start(crate::identity::PeerId::from_uuid(Uuid::nil())))
            .await;
        assert!(cands.is_empty());
    }
}
