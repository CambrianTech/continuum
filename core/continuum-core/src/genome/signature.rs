//! `GeneSignature` — a gene's identity in embedding space, COMPUTED at mint,
//! never authored ([[gene-routing-is-distance-not-keywords]]).
//!
//! The routing question "which gene helps this task?" is nearest-neighbor in
//! the SAME embedding space engram recall uses — gene recall and memory recall
//! are one problem, so this module reuses the one embedding lane
//! ([`crate::cognition::embedding`]) and the one clustering kernel
//! ([`crate::modules::embedding::detect_clusters`]), never a parallel space.
//!
//! # Where signatures come from and where they live
//!
//! The training corpus is in hand exactly once: at `genome/job-create`, before
//! the dataset is consumed by the trainer. The chain used to BREAK there — by
//! the time the L3 sentinel adopted a gene, the corpus that produced it was no
//! longer referenceable (audited 2026-08-22: `WatchedJob` carried `eval_set`
//! but no corpus pointer). So: **mint at job-create**, carry on the
//! [`WatchedJob`](crate::genome::fine_tuning::WatchedJob), and the sentinel
//! **stamps** the adopted gene's signature into the [`SignatureStore`] sidecar
//! beside the adapter manifest — a sidecar, not a `TrainedAdapter` field,
//! because the manifest struct derives `Eq` (a `Vec<f32>` would forfeit it)
//! and because old manifests must keep loading byte-identically.
//!
//! # The shape
//!
//! - `centroid` — L2-normalized mean of the corpus embeddings: the gene's
//!   center of mass.
//! - `subspaces` — cluster centroids over the same embeddings: a gene is near
//!   SEVERAL domains (an FP gene is near Scheme AND near parsers), and
//!   similarity takes the max over centroid+subspaces so tangential reach
//!   survives averaging.
//! - `embedder` + `dim` — the embedding-SPACE identity. Similarity across
//!   mismatched spaces is meaningless, so [`GeneSignature::similarity_in`]
//!   returns `None` on mismatch (honest absence, never a lying 0.0) and the
//!   caller falls back to its non-signature path.
//! - `corpus` — the [`CorpusRef`] (name + `sha256:` hash + size): the
//!   falsifiable mint provenance a gene card publishes.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::cognition::embedding::{cosine_similarity, EmbeddingProvider};
use crate::forge::recipe::CorpusRef;

/// Cluster admission floor for subspace detection. The recall space is
/// anisotropic (unrelated pairs sit ~0.25–0.30, see `recall_faculty`'s war
/// story), so this must clear the unrelated band by a wide margin — 0.55 keeps
/// only genuinely themed neighborhoods.
const SUBSPACE_MIN_SIMILARITY: f32 = 0.55;

/// A subspace must be a THEME, not an outlier pair.
const SUBSPACE_MIN_CLUSTER_SIZE: usize = 3;

/// Routing needs coarse domains, not a topic model — and the signature rides
/// wire/cards, so its size is bounded. Largest clusters win the seats.
const MAX_SUBSPACES: usize = 4;

/// A gene's computed identity in embedding space. See the module docs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneSignature {
    /// Embedding-space identity — the provider's `id()` at mint time.
    pub embedder: String,
    /// Vector dimensionality of that space.
    pub dim: usize,
    /// L2-normalized mean of the corpus embeddings.
    pub centroid: Vec<f32>,
    /// Cluster centroids (each L2-normalized), largest clusters first,
    /// at most [`MAX_SUBSPACES`]. May be empty (a corpus with one theme).
    pub subspaces: Vec<Vec<f32>>,
    /// Falsifiable mint provenance: which corpus, hashed, how big.
    pub corpus: CorpusRef,
    /// Unix-ms mint time (receipt-age axis for later fitness decay).
    pub minted_at_ms: u64,
}

/// L2-normalize in place; a zero vector stays zero (and can never win a max).
fn normalize(v: &mut [f32]) {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > f32::EPSILON {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}

impl GeneSignature {
    /// Mint a signature from the training corpus. Embeds every text through
    /// the ONE embedding lane (the content-addressed cache absorbs repeats),
    /// takes the normalized mean as centroid, and cluster centroids as
    /// subspaces. An empty corpus fails loud — a signature minted from
    /// nothing would route by noise.
    pub async fn mint(
        corpus_texts: &[String],
        corpus: CorpusRef,
        embedder: &Arc<dyn EmbeddingProvider>,
        now_ms: u64,
    ) -> Result<Self, String> {
        if corpus_texts.is_empty() {
            return Err("gene signature mint refused: empty corpus (routing by noise)".into());
        }
        let dim = embedder.dim();
        let mut embeddings: Vec<Vec<f32>> = Vec::with_capacity(corpus_texts.len());
        for text in corpus_texts {
            let e = embedder.embed(text).await;
            if e.len() != dim {
                return Err(format!(
                    "embedder '{}' returned {} dims (declared {dim}) — refusing a corrupt signature",
                    embedder.id(),
                    e.len()
                ));
            }
            embeddings.push(e);
        }
        // Centroid: mean of (already-normalized) embeddings, re-normalized.
        let mut centroid = vec![0.0f32; dim];
        for e in &embeddings {
            for (c, x) in centroid.iter_mut().zip(e) {
                *c += x;
            }
        }
        let n = embeddings.len() as f32;
        for c in centroid.iter_mut() {
            *c /= n;
        }
        normalize(&mut centroid);

        // Subspaces: the shared clustering kernel, largest themes first.
        let mut clusters = crate::modules::embedding::detect_clusters(
            &embeddings,
            SUBSPACE_MIN_SIMILARITY,
            SUBSPACE_MIN_CLUSTER_SIZE,
        );
        clusters.sort_by_key(|c| std::cmp::Reverse(c.indices.len()));
        let subspaces = clusters
            .into_iter()
            .take(MAX_SUBSPACES)
            .map(|c| {
                let mut mean = vec![0.0f32; dim];
                for &i in &c.indices {
                    for (m, x) in mean.iter_mut().zip(&embeddings[i]) {
                        *m += x;
                    }
                }
                let k = c.indices.len() as f32;
                for m in mean.iter_mut() {
                    *m /= k;
                }
                normalize(&mut mean);
                mean
            })
            .collect();

        Ok(Self {
            embedder: embedder.id().to_string(),
            dim,
            centroid,
            subspaces,
            corpus,
            minted_at_ms: now_ms,
        })
    }

    /// Similarity of a query embedding to this gene, in the named space.
    /// `None` when the spaces don't match (different embedder or dim) —
    /// honest absence; the caller falls back to its non-signature path
    /// rather than comparing apples to oranges as 0.0.
    pub fn similarity_in(&self, embedder_id: &str, query: &[f32]) -> Option<f32> {
        if embedder_id != self.embedder || query.len() != self.dim {
            return None;
        }
        let mut best = cosine_similarity(query, &self.centroid);
        for s in &self.subspaces {
            best = best.max(cosine_similarity(query, s));
        }
        Some(best)
    }
}

/// The sidecar store: gene path → signature, persisted beside the adapter
/// manifest (so a `CONTINUUM_ADAPTER_MANIFEST` override relocates both).
/// Keyed by the gene's on-disk PATH string — the same identity the manifest
/// dedups on. One writer (the L3 sentinel at adoption); readers are the
/// resolver's candidate sources.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SignatureStore {
    /// gene path (as recorded on its `TrainedAdapter`) → signature.
    pub by_path: BTreeMap<String, GeneSignature>,
}

/// Where the sidecar lives: next to the manifest, same override semantics.
pub fn signature_store_path() -> Result<PathBuf, String> {
    Ok(crate::forge::adapter_manifest::manifest_path()?.with_file_name("signatures.json"))
}

impl SignatureStore {
    /// Load the store at `path`. Missing file = empty store (the legitimate
    /// pre-first-adoption state); present-but-unparsable fails loud — a
    /// half-read store would silently unroute every signed gene.
    pub fn load_at(path: &std::path::Path) -> Result<Self, String> {
        match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text)
                .map_err(|e| format!("signature store {} unparsable: {e}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(format!("signature store {} unreadable: {e}", path.display())),
        }
    }

    /// Stamp (insert or replace) one gene's signature and persist atomically
    /// (tmp + rename — a crash never leaves a torn store).
    pub fn stamp_at(
        path: &std::path::Path,
        gene_path: &str,
        signature: GeneSignature,
    ) -> Result<(), String> {
        let mut store = Self::load_at(path)?;
        store.by_path.insert(gene_path.to_string(), signature);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
        }
        let tmp = path.with_extension("json.tmp");
        let text = serde_json::to_string_pretty(&store).map_err(|e| format!("serialize: {e}"))?;
        std::fs::write(&tmp, text).map_err(|e| format!("write {}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, path).map_err(|e| format!("rename {}: {e}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cognition::embedding::LexicalEmbedder;

    fn corpus_ref() -> CorpusRef {
        CorpusRef {
            name: "test-corpus".into(),
            content_hash: "sha256:0000000000000000000000000000000000000000000000000000000000000000"
                .into(),
            size_bytes: 42,
            source_url: None,
        }
    }

    // what this catches: the mint contract — a signature is COMPUTED, normalized,
    // space-stamped, and refuses an empty corpus. LexicalEmbedder is deterministic,
    // so the same corpus always mints the same signature (the property that makes a
    // published signature falsifiable: anyone can re-mint and compare).
    #[tokio::test]
    async fn mint_is_deterministic_normalized_and_refuses_an_empty_corpus() {
        let embedder: Arc<dyn EmbeddingProvider> = Arc::new(LexicalEmbedder::default());
        let texts: Vec<String> = ["fold the list", "map the vector", "recursion base case"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let a = GeneSignature::mint(&texts, corpus_ref(), &embedder, 1_000)
            .await
            .expect("mint");
        let b = GeneSignature::mint(&texts, corpus_ref(), &embedder, 1_000)
            .await
            .expect("mint");
        assert_eq!(a.centroid, b.centroid, "same corpus, same signature — falsifiable");
        assert_eq!(a.embedder, embedder.id());
        assert_eq!(a.dim, embedder.dim());
        let norm: f32 = a.centroid.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-4, "centroid is unit-length, got {norm}");

        let err = GeneSignature::mint(&[], corpus_ref(), &embedder, 1_000)
            .await
            .expect_err("empty corpus must refuse");
        assert!(err.contains("empty corpus"), "{err}");
    }

    // what this catches: cross-SPACE comparison silently passing. A similarity
    // between vectors from different embedders (or dims) is meaningless; returning
    // 0.0 would quietly demote every signed gene instead of falling back — the
    // honest answer is None, and same-space queries answer with a real cosine.
    #[tokio::test]
    async fn similarity_answers_in_its_own_space_and_refuses_others() {
        let embedder: Arc<dyn EmbeddingProvider> = Arc::new(LexicalEmbedder::default());
        let texts: Vec<String> = vec!["parse the tokens".into(), "lex the source".into()];
        let sig = GeneSignature::mint(&texts, corpus_ref(), &embedder, 0)
            .await
            .expect("mint");

        let near = embedder.embed("parse the tokens").await;
        let sim = sig
            .similarity_in(embedder.id(), &near)
            .expect("same space answers");
        assert!(sim > 0.0, "a corpus member scores positive, got {sim}");

        assert!(sig.similarity_in("some-other-embedder", &near).is_none());
        assert!(sig.similarity_in(embedder.id(), &near[..near.len() - 1]).is_none());
    }

    // what this catches: the sidecar's load/stamp contract — missing file is the
    // legitimate empty state, a stamp round-trips, and re-stamping the same path
    // REPLACES (a retrained gene's new signature must win, never duplicate).
    #[tokio::test]
    async fn the_sidecar_store_round_trips_and_restamp_replaces() {
        let embedder: Arc<dyn EmbeddingProvider> = Arc::new(LexicalEmbedder::default());
        let sig = GeneSignature::mint(&["alpha".to_string()], corpus_ref(), &embedder, 7)
            .await
            .expect("mint");
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("signatures.json");

        assert!(SignatureStore::load_at(&path).expect("missing = empty").by_path.is_empty());

        SignatureStore::stamp_at(&path, "/genes/fp.gguf", sig.clone()).expect("stamp");
        let loaded = SignatureStore::load_at(&path).expect("load");
        assert_eq!(loaded.by_path.len(), 1);
        assert_eq!(loaded.by_path["/genes/fp.gguf"].minted_at_ms, 7);

        let mut newer = sig;
        newer.minted_at_ms = 9;
        SignatureStore::stamp_at(&path, "/genes/fp.gguf", newer).expect("restamp");
        let reloaded = SignatureStore::load_at(&path).expect("reload");
        assert_eq!(reloaded.by_path.len(), 1, "replace, never duplicate");
        assert_eq!(reloaded.by_path["/genes/fp.gguf"].minted_at_ms, 9);
    }
}
