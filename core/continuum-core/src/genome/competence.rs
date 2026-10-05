//! A competence, and the decision that turns one into a gene: reuse, fork, or mint
//! (`docs/architecture/GENE-REUSE-FORK-MINT.md`, build steps 2 and 3's pure core).
//!
//! Joel, 2026-10-05: never one gene per memory. A memory is an engram; an example is a
//! settled, graded turn; a COMPETENCE is a cluster of examples asking for the same skill;
//! a gene is minted for a competence, and only when recall alone leaves her surprised in
//! it. Everything here is pure over embeddings and numbers, so each branch is a test; the
//! embedding call and the recall call happen outside and hand their results in.
//!
//! The same clustering kernel recall and gene signatures use
//! (`modules::embedding::detect_clusters`), never a parallel space.

use crate::genome::signature::SignatureStore;
use crate::modules::embedding::detect_clusters;

/// Fewer settled examples than this is not a competence; it is memories. Measured
/// against the first signatures in the repository, re-pinned as the ledger grows.
pub const MIN_EXAMPLES: usize = 8;
/// Members of one competence agree at least this much (cosine), the same floor a gene
/// signature's subspaces are clustered with.
pub const MIN_COHESION: f32 = 0.70;
/// Her surprise in a competence must be at least this (contradicted / judged, the verdict
/// surprise of `perception_region::SurpriseTally`) before any gene is considered for it:
/// below it, memories suffice.
pub const SURPRISE_FLOOR: f32 = 0.25;
/// Cosine similarity to an existing gene at or above which the competence IS that gene's:
/// reuse it. Above `D_FORK` but below this: a child of it. Below `D_FORK`: a stranger.
pub const SIM_REUSE: f32 = 0.90;
pub const SIM_FORK: f32 = 0.75;

/// A cluster of settled examples that ask for the same skill.
#[derive(Debug, Clone, PartialEq)]
pub struct Competence {
    /// L2-normalized mean of the members' embeddings: comparable to a gene signature's
    /// centroid by `cosine`.
    pub centroid: Vec<f32>,
    /// Indices into the examples handed in.
    pub members: Vec<usize>,
    /// Average intra-cluster similarity.
    pub cohesion: f32,
    /// The member nearest the rest; the example that names the competence.
    pub representative: usize,
}

/// Competences over `embeddings` (one per settled example, all one embedder and dim).
/// Below [`MIN_EXAMPLES`] members a cluster is dropped: not yet a competence.
pub fn competences(embeddings: &[Vec<f32>]) -> Vec<Competence> {
    let Some(dim) = embeddings.first().map(Vec::len) else {
        return Vec::new();
    };
    detect_clusters(embeddings, MIN_COHESION, MIN_EXAMPLES)
        .into_iter()
        .map(|c| {
            let mut centroid = vec![0.0f32; dim];
            for &i in &c.indices {
                for (m, x) in centroid.iter_mut().zip(&embeddings[i]) {
                    *m += x;
                }
            }
            let k = c.indices.len() as f32;
            for m in centroid.iter_mut() {
                *m /= k;
            }
            normalize(&mut centroid);
            Competence { centroid, members: c.indices, cohesion: c.strength, representative: c.representative }
        })
        .collect()
}

/// The nearest existing gene to a competence: from her signature store today, the mesh
/// and the HF repository as later sources of the same lookup. `gene` is the adapter path
/// the signature store keys by (a gene's identity on this node, as the manifest and the
/// serving daemon spell it); `similarity` is the signature's `similarity_in` (max over
/// its centroid and subspaces).
#[derive(Debug, Clone, PartialEq)]
pub struct NearestGene {
    pub gene: String,
    pub similarity: f32,
    pub resident: bool,
}

/// The nearest gene in a local signature store to `competence`, in the embedder's space;
/// `resident` names the adapter paths currently loaded for her. `None` when the store
/// holds nothing comparable (empty, or another embedder's signatures).
pub fn nearest_in_store(competence: &Competence, store: &SignatureStore, embedder_id: &str, resident: &[String]) -> Option<NearestGene> {
    store
        .by_path
        .iter()
        .filter_map(|(path, sig)| {
            let similarity = sig.similarity_in(embedder_id, &competence.centroid)?;
            Some(NearestGene { gene: path.clone(), similarity, resident: resident.iter().any(|r| r == path) })
        })
        .max_by(|a, b| a.similarity.total_cmp(&b.similarity))
}

/// What to do about a competence. Every variant is a receipt: the probe carries it with
/// the numbers that decided it.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(tag = "branch", rename_all = "snake_case")]
pub enum Decision {
    /// Memories suffice, or the cluster is too small to be a competence.
    Nothing { why: NothingBecause },
    /// Page the nearest gene in and trial it on her cards; no training.
    Reuse { gene: String, similarity: f32 },
    /// The nearest gene is already resident and she is still surprised: it is not
    /// enough; train a child of it on her examples (warm start, lineage parent).
    Fork { parent: String, similarity: f32 },
    /// Nothing near: train a new gene on her examples from the base.
    Mint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NothingBecause {
    TooFewExamples,
    SurpriseLow,
}

/// The decision, in the order the design states. `surprise` is `S` for this competence
/// (`None` = no judged expectation yet, which is not "low": it is unknown, and unknown
/// never mints).
pub fn decide(competence: &Competence, surprise: Option<f32>, nearest: Option<&NearestGene>) -> Decision {
    if competence.members.len() < MIN_EXAMPLES {
        return Decision::Nothing { why: NothingBecause::TooFewExamples };
    }
    match surprise {
        Some(s) if s >= SURPRISE_FLOOR => {}
        _ => return Decision::Nothing { why: NothingBecause::SurpriseLow },
    }
    match nearest {
        // Near enough to BE this competence's gene, and not yet in her: reuse.
        Some(n) if n.similarity >= SIM_REUSE && !n.resident => Decision::Reuse { gene: n.gene.clone(), similarity: n.similarity },
        // Near, but either already resident (and she is still surprised) or a cousin:
        // a child of it, with lineage.
        Some(n) if n.similarity >= SIM_FORK => Decision::Fork { parent: n.gene.clone(), similarity: n.similarity },
        _ => Decision::Mint,
    }
}

/// Cosine between a competence and a gene signature's centroid (both normalized).
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

fn normalize(v: &mut [f32]) {
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if n > 0.0 {
        for x in v.iter_mut() {
            *x /= n;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit(dir: usize, dim: usize, wobble: f32, seed: usize) -> Vec<f32> {
        let mut v = vec![0.0f32; dim];
        v[dir] = 1.0;
        v[(dir + 1 + seed) % dim] = wobble;
        normalize(&mut v);
        v
    }

    // what this catches (Joel: never one gene per memory): seven examples of one skill are
    // memories, eight are a competence; two distinct skills are two competences; and a
    // competence's centroid sits with its members, not between skills.
    #[test]
    fn a_competence_needs_min_examples_and_skills_do_not_merge() {
        let dim = 8;
        let mut ex: Vec<Vec<f32>> = (0..7).map(|i| unit(0, dim, 0.1, i)).collect();
        assert!(competences(&ex).is_empty(), "seven examples are memories");
        ex.push(unit(0, dim, 0.1, 7));
        let one = competences(&ex);
        assert_eq!(one.len(), 1, "eight are a competence");
        assert_eq!(one[0].members.len(), 8);
        for i in 0..8 {
            ex.push(unit(4, dim, 0.1, i));
        }
        let two = competences(&ex);
        assert_eq!(two.len(), 2, "two skills, two competences");
        for c in &two {
            let own = ex[c.representative].clone();
            assert!(cosine(&c.centroid, &own) > 0.95, "the centroid sits with its members");
        }
    }

    // what this catches: the order of the decision. Too few examples or low surprise →
    // nothing (unknown surprise never mints); a near gene she lacks → reuse; a near gene
    // she already has, still surprised → fork; a cousin → fork with lineage; a stranger →
    // mint. Each branch carries the numbers that decided it.
    #[test]
    fn the_decision_reuses_before_forking_and_forks_before_minting() {
        let c = Competence { centroid: vec![1.0, 0.0], members: (0..MIN_EXAMPLES).collect(), cohesion: 0.9, representative: 0 };
        let small = Competence { members: vec![0, 1], ..c.clone() };
        let g = "/genes/rust-tests.gguf".to_string();
        let near = |similarity, resident| NearestGene { gene: g.clone(), similarity, resident };
        assert_eq!(decide(&small, Some(0.9), None), Decision::Nothing { why: NothingBecause::TooFewExamples });
        assert_eq!(decide(&c, None, None), Decision::Nothing { why: NothingBecause::SurpriseLow }, "unknown never mints");
        assert_eq!(decide(&c, Some(0.1), None), Decision::Nothing { why: NothingBecause::SurpriseLow });
        assert_eq!(decide(&c, Some(0.5), Some(&near(0.95, false))), Decision::Reuse { gene: g.clone(), similarity: 0.95 });
        assert_eq!(decide(&c, Some(0.5), Some(&near(0.95, true))), Decision::Fork { parent: g.clone(), similarity: 0.95 }, "resident and still surprised: a child");
        assert_eq!(decide(&c, Some(0.5), Some(&near(0.80, false))), Decision::Fork { parent: g.clone(), similarity: 0.80 }, "a cousin: a child with lineage");
        assert_eq!(decide(&c, Some(0.5), Some(&near(0.40, false))), Decision::Mint);
        assert_eq!(decide(&c, Some(0.5), None), Decision::Mint);
        assert!(SIM_FORK < SIM_REUSE, "the thresholds order the branches");
    }

    // what this catches: the nearest gene is read from the signature store in the SAME
    // space (another embedder's signature is not comparable and is skipped, never
    // mis-scored), the max over the store wins, and residency is read from the loaded set.
    #[test]
    fn the_nearest_gene_comes_from_the_store_in_the_same_space() {
        use crate::genome::signature::GeneSignature;
        use crate::forge::recipe::CorpusRef;
        let sig = |embedder: &str, centroid: Vec<f32>| GeneSignature {
            embedder: embedder.into(),
            dim: centroid.len(),
            centroid,
            subspaces: vec![],
            corpus: CorpusRef { name: "t".into(), content_hash: "sha256:0".into(), size_bytes: 0, source_url: None },
            minted_at_ms: 0,
        };
        let mut store = SignatureStore::default();
        store.by_path.insert("/g/far.gguf".into(), sig("e1", vec![0.0, 1.0]));
        store.by_path.insert("/g/near.gguf".into(), sig("e1", vec![0.96, 0.28]));
        store.by_path.insert("/g/other-space.gguf".into(), sig("e2", vec![1.0, 0.0]));
        let c = Competence { centroid: vec![1.0, 0.0], members: (0..MIN_EXAMPLES).collect(), cohesion: 0.9, representative: 0 };
        let n = nearest_in_store(&c, &store, "e1", &["/g/near.gguf".to_string()]).expect("a nearest gene");
        assert_eq!(n.gene, "/g/near.gguf");
        assert!(n.similarity > 0.9 && n.resident, "{n:?}");
        assert!(nearest_in_store(&c, &store, "e3", &[]).is_none(), "nothing comparable in another space");
    }
}
