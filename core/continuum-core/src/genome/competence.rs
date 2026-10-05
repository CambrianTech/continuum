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

use std::path::{Path, PathBuf};

use uuid::Uuid;

use crate::genome::signature::SignatureStore;
use crate::modules::embedding::detect_clusters;

/// A gene's identity, as the two places a gene can live spell it: an adapter PATH on
/// this node (what the manifest, the serving daemon and the signature store key by),
/// or a REPO on the hub (what `genome/pull` takes). An enum, never a string that is
/// sometimes a path and sometimes a repo: the variant says which, and the stores keep
/// their own key types.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/genome/GeneRef.ts")]
#[serde(tag = "at", rename_all = "snake_case")]
pub enum GeneRef {
    Local {
        #[ts(type = "string")]
        path: PathBuf,
    },
    Hub { repo: String },
}

impl std::fmt::Display for GeneRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GeneRef::Local { path } => write!(f, "{}", path.display()),
            GeneRef::Hub { repo } => f.write_str(repo),
        }
    }
}

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
/// and the HF repository as later sources of the same lookup; `similarity` is the
/// signature's `similarity_in` (max over its centroid and subspaces).
#[derive(Debug, Clone, PartialEq)]
pub struct NearestGene {
    pub gene: GeneRef,
    pub similarity: f32,
    pub resident: bool,
}

/// The nearest gene in a local signature store to `competence`, in the embedder's space;
/// `resident` names the adapter paths currently loaded for her, `retired` the ones her
/// own work already retired (a retired gene leaves the serving manifest but keeps its
/// signature, and offering it back to her as a reuse would trial it forever). `None`
/// when the store holds nothing comparable (empty, or another embedder's signatures).
pub fn nearest_in_store(
    competence: &Competence,
    store: &SignatureStore,
    embedder_id: &str,
    resident: &[PathBuf],
    retired: &[PathBuf],
) -> Option<NearestGene> {
    store
        .by_path
        .iter()
        .filter(|(path, _)| !retired.iter().any(|r| r.as_path() == Path::new(path)))
        .filter_map(|(path, sig)| {
            let similarity = sig.similarity_in(embedder_id, &competence.centroid)?;
            let resident = resident.iter().any(|r| r.as_path() == Path::new(path));
            Some(NearestGene { gene: GeneRef::Local { path: PathBuf::from(path) }, similarity, resident })
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
    Reuse { gene: GeneRef, similarity: f32 },
    /// The nearest gene is already resident and she is still surprised: it is not
    /// enough; train a child of it on her examples (warm start, lineage parent).
    Fork { parent: GeneRef, similarity: f32 },
    /// Nothing near: train a new gene on her examples from the base.
    Mint,
    /// A gene for this competence is already being born: a job of hers in flight whose
    /// signature is within reuse distance. These examples join it (or wait for it); a
    /// second mint for one competence is the design's falsifier #2 (four Mints for one
    /// card's credit on the 5090, 2026-10-05 13:17Z, before this branch existed).
    Join { job: Uuid, similarity: f32 },
    /// A gene for this competence is already on trial in her work (reused or freshly
    /// trained, now resident and drawn on a share of her cards). Her cards decide it;
    /// these examples wait for the verdict. Without this, the fill after a reuse would
    /// read the gene as resident-and-still-surprised and fork it while it is being judged.
    Await { trial: Uuid, similarity: f32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NothingBecause {
    SurpriseLow,
}

/// Her surprise in a competence, as the decision receives it. `NotYetMeasured` is not
/// "low": the floor applies only to a measured number. Until surprise is folded per
/// competence (step 1 measures it per activity, #4774), a full bucket decides on
/// distance alone, as the trigger did before this, and the probe says `not_measured`.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
#[serde(tag = "surprise", rename_all = "snake_case")]
pub enum Surprise {
    Measured { s: f32 },
    NotYetMeasured,
}

/// Something of hers already underway for a competence, by the signature it carries: a
/// job in flight (the job board's local id) or a trial open in her work (the trial's
/// id). One shape, because the decision treats both the same way: nothing is minted
/// beside a gene that is being born or being judged.
#[derive(Debug, Clone, PartialEq)]
pub struct Pending {
    /// A u128 on the stack: the job's local id, or the trial's id.
    pub id: Uuid,
    pub similarity: f32,
}

/// The nearest pending thing whose signature sits within the competence's space; the
/// caller hands in her jobs (or her open trials) for the same base, each with the
/// signature its gene was minted with.
pub fn nearest_pending<'a>(
    competence: &Competence,
    embedder_id: &str,
    candidates: impl Iterator<Item = (Uuid, &'a crate::genome::signature::GeneSignature)>,
) -> Option<Pending> {
    candidates
        .filter_map(|(id, sig)| {
            let similarity = sig.similarity_in(embedder_id, &competence.centroid)?;
            Some(Pending { id, similarity })
        })
        .max_by(|a, b| a.similarity.total_cmp(&b.similarity))
}

/// The decision, in the order the design states. A job in flight and a trial open are
/// checked before any branch that would train or reuse: a competence already being
/// learned is joined, one already being judged is awaited, never minted twice. The
/// competence handed in IS one: [`competences`] applies [`MIN_EXAMPLES`] when it clusters
/// her curriculum, and a full bucket is one by the room's own threshold; the decision
/// never second-guesses its size (a smaller bucket skipped Join and Await on the way to
/// a mint, 2026-10-05).
pub fn decide(surprise: Surprise, nearest: Option<&NearestGene>) -> Decision {
    decide_with_pending(surprise, nearest, None, None)
}

pub fn decide_with_pending(
    surprise: Surprise,
    nearest: Option<&NearestGene>,
    in_flight: Option<&Pending>,
    on_trial: Option<&Pending>,
) -> Decision {
    if let Some(j) = in_flight.filter(|j| j.similarity >= SIM_REUSE) {
        return Decision::Join { job: j.id, similarity: j.similarity };
    }
    if let Some(t) = on_trial.filter(|t| t.similarity >= SIM_REUSE) {
        return Decision::Await { trial: t.id, similarity: t.similarity };
    }
    if let Surprise::Measured { s } = surprise {
        if s < SURPRISE_FLOOR {
            return Decision::Nothing { why: NothingBecause::SurpriseLow };
        }
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
        let g = GeneRef::Local { path: PathBuf::from("/genes/rust-tests.gguf") };
        let near = |similarity, resident| NearestGene { gene: g.clone(), similarity, resident };
        let s = |x| Surprise::Measured { s: x };
        // The size of a competence is settled by whoever made it (the clustering floor, or
        // the bucket's threshold): the decision takes no competence and never second-guesses it.
        assert_eq!(decide(s(0.9), Some(&near(0.95, false))), Decision::Reuse { gene: g.clone(), similarity: 0.95 });
        assert_eq!(decide(s(0.1), None), Decision::Nothing { why: NothingBecause::SurpriseLow });
        assert_eq!(decide(Surprise::NotYetMeasured, None), Decision::Mint, "not yet measured is not low: distance decides, as before");
        assert_eq!(decide(s(0.5), Some(&near(0.95, false))), Decision::Reuse { gene: g.clone(), similarity: 0.95 });
        assert_eq!(decide(s(0.5), Some(&near(0.95, true))), Decision::Fork { parent: g.clone(), similarity: 0.95 }, "resident and still surprised: a child");
        assert_eq!(decide(s(0.5), Some(&near(0.80, false))), Decision::Fork { parent: g.clone(), similarity: 0.80 }, "a cousin: a child with lineage");
        assert_eq!(decide(s(0.5), Some(&near(0.40, false))), Decision::Mint);
        assert_eq!(decide(s(0.5), None), Decision::Mint);
        assert!(SIM_FORK < SIM_REUSE, "the thresholds order the branches");
        // A job of hers already training this competence: join it, whatever the store says
        // (the four-Mints-for-one-card shape); a distant job in flight changes nothing.
        let flying_id = Uuid::from_u128(0xbcb7316f);
        let flying = Pending { id: flying_id, similarity: 0.97 };
        assert_eq!(decide_with_pending(s(0.5), None, Some(&flying), None), Decision::Join { job: flying_id, similarity: 0.97 });
        assert_eq!(decide_with_pending(s(0.5), Some(&near(0.95, false)), Some(&flying), None), Decision::Join { job: flying_id, similarity: 0.97 }, "join before reuse: the gene being born is hers");
        let far = Pending { id: Uuid::from_u128(0x0f), similarity: 0.3 };
        assert_eq!(decide_with_pending(s(0.5), None, Some(&far), None), Decision::Mint);
        // A gene on trial for this competence: await her verdict. The resident gene the
        // trial is judging would otherwise read as resident-and-still-surprised and fork.
        let trial_id = Uuid::from_u128(0x17dc0a7b);
        let judged = Pending { id: trial_id, similarity: 0.96 };
        assert_eq!(decide_with_pending(s(0.5), Some(&near(0.96, true)), None, Some(&judged)), Decision::Await { trial: trial_id, similarity: 0.96 }, "await before fork: the gene is being judged");
        assert_eq!(decide_with_pending(s(0.5), Some(&near(0.96, true)), Some(&flying), Some(&judged)), Decision::Join { job: flying_id, similarity: 0.97 }, "a job in flight outranks a trial");
        let cousin_trial = Pending { id: trial_id, similarity: 0.8 };
        assert_eq!(decide_with_pending(s(0.5), None, None, Some(&cousin_trial)), Decision::Mint, "a cousin's trial does not settle this competence");
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
            parent: None,
        };
        let mut store = SignatureStore::default();
        store.by_path.insert("/g/far.gguf".into(), sig("e1", vec![0.0, 1.0]));
        store.by_path.insert("/g/near.gguf".into(), sig("e1", vec![0.96, 0.28]));
        store.by_path.insert("/g/other-space.gguf".into(), sig("e2", vec![1.0, 0.0]));
        let c = Competence { centroid: vec![1.0, 0.0], members: (0..MIN_EXAMPLES).collect(), cohesion: 0.9, representative: 0 };
        let near = PathBuf::from("/g/near.gguf");
        let n = nearest_in_store(&c, &store, "e1", std::slice::from_ref(&near), &[]).expect("a nearest gene");
        assert_eq!(n.gene, GeneRef::Local { path: near.clone() });
        assert!(n.similarity > 0.9 && n.resident, "{n:?}");
        assert!(nearest_in_store(&c, &store, "e3", &[], &[]).is_none(), "nothing comparable in another space");
        // A gene her work retired keeps its signature but is never offered back: the
        // next nearest wins (here the far one, below fork distance, so the caller mints).
        let n = nearest_in_store(&c, &store, "e1", &[], std::slice::from_ref(&near)).expect("the far gene");
        assert_eq!(n.gene, GeneRef::Local { path: PathBuf::from("/g/far.gguf") }, "a retired gene is not a candidate");
    }
}
