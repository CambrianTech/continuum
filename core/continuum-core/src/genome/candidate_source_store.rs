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
    /// The layer's card/domain/keyword text — embedded for the cosine match
    /// WHEN no minted signature exists (the fallback path).
    pub match_text: String,
    /// Epoch-ms of last use (recency factor).
    pub last_used_ms: u64,
    /// Provenance trust (local, refined layers score high).
    pub trust_factor: f32,
    /// The gene's MINTED embedding-space identity, when the adoption chain
    /// stamped one ([`crate::genome::signature`]). Preferred over `match_text`:
    /// computed from the actual training corpus (centroid + subspaces, so
    /// tangential reach survives), and it kills the per-call re-embed of every
    /// layer that the fallback path pays.
    pub signature: Option<crate::genome::signature::GeneSignature>,
}

/// A [`CandidateSource`] over the local genome store. Holds the persona's layers
/// + an embedder; `fetch` embeds the query and each layer's match text and emits
/// candidates with the cosine as `semantic_factor` (the caller-computed field the
/// ranking engine expects).
pub struct GenomeStoreCandidateSource {
    layers: Vec<LocalGenomeLayer>,
    embedder: Arc<dyn EmbeddingProvider>,
}

/// Derive a stable local `ArtifactId` for a layer the forge hasn't content-hashed
/// yet — the sha256 of its name, first 16 bytes → UUID (the same
/// "sha256-derived-uuid" convention as forge `ArtifactBlob`). Deterministic, so a
/// local layer keeps the same id across boots and cosine recall can dedup it; a
/// real forge content-hash supersedes it once the layer is published to the market.
fn stable_local_id(name: &str) -> ArtifactId {
    let hash = crate::persona::inbox_admission::content_hash_sha256(name);
    let hex = hash.strip_prefix("sha256:").unwrap_or(&hash);
    let mut bytes = [0u8; 16];
    for (i, b) in bytes.iter_mut().enumerate() {
        *b = hex
            .get(i * 2..i * 2 + 2)
            .and_then(|s| u8::from_str_radix(s, 16).ok())
            .unwrap_or(0);
    }
    ArtifactId(uuid::Uuid::from_bytes(bytes))
}

impl GenomeStoreCandidateSource {
    pub fn new(layers: Vec<LocalGenomeLayer>, embedder: Arc<dyn EmbeddingProvider>) -> Self {
        Self { layers, embedder }
    }

    /// Build the source from the persona's LIVE paging-engine adapters — the LOCAL
    /// LoRA intelligence, no network, no HF. Each known adapter becomes a rankable
    /// candidate (stable id from its name, match text from its domain), so recall
    /// can cosine-pick the best-fitting local layer for a task entirely on-machine.
    /// HF/grid are just additional sources layered in later; the intelligence is
    /// here now ([[dynamic-lora-by-project-directory-scope]]).
    pub fn from_local_adapters(
        adapters: &[crate::persona::genome_paging::GenomeAdapterInfo],
        embedder: Arc<dyn EmbeddingProvider>,
    ) -> Self {
        let layers = adapters
            .iter()
            .map(|a| LocalGenomeLayer {
                artifact_id: stable_local_id(&a.name),
                match_text: a.domain.clone(),
                last_used_ms: a.last_used_ms,
                // Local, persona-owned layers are trusted; a refined layer scores
                // higher still once the sentinel-attribution slice lands.
                trust_factor: 0.9,
                // The paging-engine view carries no path to join the sidecar on;
                // signatures arrive via `from_manifest` (the manifest IS keyed by
                // path). This constructor stays the signature-less fallback.
                signature: None,
            })
            .collect();
        Self::new(layers, embedder)
    }

    /// Build the source from the durable adapter MANIFEST joined with the
    /// signature sidecar — the store the adoption chain stamps
    /// ([`crate::genome::signature::SignatureStore`]). Every registered gene
    /// becomes a candidate; the ones the sentinel adopted since the signature
    /// slice landed carry their minted identity and rank by it.
    pub fn from_manifest(
        manifest: &[crate::forge::adapter_manifest::TrainedAdapter],
        signatures: &crate::genome::signature::SignatureStore,
        embedder: Arc<dyn EmbeddingProvider>,
    ) -> Self {
        let layers = manifest
            .iter()
            .map(|a| LocalGenomeLayer {
                artifact_id: stable_local_id(&a.alias),
                match_text: a.alias.clone(),
                // The manifest carries no recency; 0 = "never used" and the
                // recency term decays it honestly rather than inventing warmth.
                last_used_ms: 0,
                trust_factor: 0.9,
                signature: signatures
                    .by_path
                    .get(&a.path.display().to_string())
                    .cloned(),
            })
            .collect();
        Self::new(layers, embedder)
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
            // A minted signature answers first: corpus-derived centroid+subspaces
            // (max over both, so tangential reach counts), zero per-layer embeds.
            // Space mismatch (different embedder/dim) returns None and the layer
            // falls back to its match_text — honest degrade, never a lying 0.0.
            let semantic_factor = match layer
                .signature
                .as_ref()
                .and_then(|s| s.similarity_in(self.embedder.id(), &q_emb))
            {
                Some(sim) => sim.clamp(0.0, 1.0),
                None => {
                    let l_emb = self.embedder.embed(&layer.match_text).await;
                    cosine_similarity(&q_emb, &l_emb).clamp(0.0, 1.0)
                }
            };
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
                residency: ResidencyHint::Local {
                    role: TierRole::Cold,
                },
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
            signature: None,
        }
    }

    // what this catches: the signature-first contract. A layer with a minted
    // signature ranks by IT (corpus-derived, subspace-max) — its match_text is
    // deliberately misleading here and must not matter; a layer whose signature
    // is from another SPACE falls back to match_text instead of scoring a lying
    // 0.0 (which would demote every signed gene after an embedder upgrade).
    #[tokio::test]
    async fn a_minted_signature_outranks_match_text_and_wrong_space_falls_back() {
        let embedder: Arc<dyn EmbeddingProvider> = Arc::new(LexicalEmbedder::default());
        let corpus: Vec<String> =
            vec!["parse tokens into an ast".into(), "lex the source stream".into()];
        let corpus_ref = crate::forge::recipe::CorpusRef {
            name: "parser-corpus".into(),
            content_hash: "sha256:00".into(),
            size_bytes: 1,
            source_url: None,
        };
        let sig = crate::genome::signature::GeneSignature::mint(
            &corpus,
            corpus_ref,
            &embedder,
            0,
        )
        .await
        .expect("mint");

        // Signed layer: match_text is unrelated noise — the signature must carry it.
        let mut signed = layer("completely unrelated cooking recipes");
        signed.signature = Some(sig.clone());
        // Wrong-space layer: same signature but claiming another embedder — must
        // fall back to its (relevant) match_text, not die at 0.
        let mut wrong_space = layer("parse tokens into an ast");
        wrong_space.signature = Some(crate::genome::signature::GeneSignature {
            embedder: "some-other-space".into(),
            ..sig
        });

        let source = GenomeStoreCandidateSource::new(vec![signed, wrong_space], embedder);
        let got = source
            .fetch(
                &query_for("parse tokens into an ast"),
                &RecallContext::cold_start(crate::identity::PeerId::from_uuid(Uuid::nil())),
            )
            .await;
        assert_eq!(got.len(), 2);
        assert!(
            got[0].semantic_factor > 0.5,
            "signature carries the signed layer despite misleading match_text: {}",
            got[0].semantic_factor
        );
        assert!(
            got[1].semantic_factor > 0.5,
            "wrong-space signature falls back to match_text: {}",
            got[1].semantic_factor
        );
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
            .fetch(
                &query_for("rust code refactoring"),
                &RecallContext::cold_start(crate::identity::PeerId::from_uuid(Uuid::nil())),
            )
            .await;
        assert_eq!(cands.len(), 2, "both layers surface as candidates");

        let sem = |id: ArtifactId| {
            cands
                .iter()
                .find(|c| c.artifact_id == id)
                .unwrap()
                .semantic_factor
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

    // what this catches: the LOCAL LoRA intelligence — projecting the persona's live
    // paging-engine adapters into cosine-rankable candidates, no network. The layer
    // whose DOMAIN matches the task ranks higher; ids are stable + deterministic.
    #[tokio::test]
    async fn from_local_adapters_ranks_the_matching_skill_no_network() {
        use crate::persona::genome_paging::GenomeAdapterInfo;
        let mk = |name: &str, domain: &str| GenomeAdapterInfo {
            name: name.to_string(),
            domain: domain.to_string(),
            size_mb: 50.0,
            priority: 0.5,
            is_loaded: false,
            last_used_ms: 1000,
            trained_model_name: None,
            compaction: None,
        };
        let adapters = vec![
            mk("code-expert", "rust code editing and refactoring"),
            mk("poet", "lyrical poetry and creative verse"),
        ];
        let source = GenomeStoreCandidateSource::from_local_adapters(
            &adapters,
            Arc::new(LexicalEmbedder::new()),
        );
        let cands = source
            .fetch(
                &query_for("rust code refactoring"),
                &RecallContext::cold_start(crate::identity::PeerId::from_uuid(Uuid::nil())),
            )
            .await;
        assert_eq!(cands.len(), 2);
        let sem = |id: ArtifactId| {
            cands
                .iter()
                .find(|c| c.artifact_id == id)
                .unwrap()
                .semantic_factor
        };
        assert!(
            sem(stable_local_id("code-expert")) > sem(stable_local_id("poet")),
            "the code skill matches the code task more closely"
        );
        assert_eq!(
            stable_local_id("code-expert"),
            stable_local_id("code-expert"),
            "the local id is deterministic across calls/boots"
        );
    }

    // what this catches: an empty store yields no candidates (recall then falls
    // back to grid/HF sources), never a panic or a fabricated hit.
    #[tokio::test]
    async fn empty_store_yields_no_candidates() {
        let source = GenomeStoreCandidateSource::new(vec![], Arc::new(LexicalEmbedder::new()));
        let cands = source
            .fetch(
                &query_for("anything"),
                &RecallContext::cold_start(crate::identity::PeerId::from_uuid(Uuid::nil())),
            )
            .await;
        assert!(cands.is_empty());
    }
}
