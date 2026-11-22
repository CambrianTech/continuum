//! The validator for one-pass themes (card 1d9a3512; Codex on the 5090, 2026-11-22: validate the
//! cutoff, the missing-calibration case and the assignment error on the captured bucket before any
//! claim). It shapes the same unit embeddings both ways, the pairwise clusters the one-pass themes
//! replace and the themes themselves, and reports their times and their disagreement. Test-fixture
//! code: the `signature_shapes` example runs it on a node's real corpus.

use super::{region_of, themes, MAX_SUBSPACES};
use crate::ai::embedding::vector_math::{detect_clusters, unit_cosine};

/// One shape's subspaces, its region edge, how long it took, and how it routes: the share of the
/// corpus's held-out examples at or above its region edge (its own unseen work, which it should
/// reach) and the share of an unrelated contrast corpus at or above it (work it should not reach).
#[derive(Debug, Clone, serde::Serialize)]
pub struct Shaped {
    pub subspaces: usize,
    pub region: Option<f32>,
    pub ms: f64,
    pub held_out_reached: f32,
    pub contrast_reached: f32,
}

/// The share of `queries` whose nearness to (`centroid`, `subspaces`) is at least `region`.
fn reached(centroid: &[f32], subspaces: &[Vec<f32>], region: Option<f32>, queries: &[Vec<f32>]) -> f32 {
    let Some(edge) = region else { return 0.0 };
    if queries.is_empty() {
        return 0.0;
    }
    let hits = queries.iter().filter(|q| super::nearness(centroid, subspaces, q) >= edge).count();
    hits as f32 / queries.len() as f32
}

/// The two shapes of one corpus, side by side.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ShapeComparison {
    pub vectors: usize,
    pub floor: f32,
    pub pairwise: Shaped,
    pub one_pass: Shaped,
    /// For each pairwise subspace (largest first), the cosine to the nearest one-pass theme.
    pub matched: Vec<f32>,
    /// Vectors the two shapes place differently (in a theme by one and not the other, or in themes
    /// that do not match), over the vectors either shape placed.
    pub assignment_disagreement: f32,
}

/// The centroid of `members` of `embeddings`, L2-normalized.
fn mean(embeddings: &[Vec<f32>], members: &[usize], dim: usize) -> Vec<f32> {
    let mut m = vec![0.0f32; dim];
    for &i in members {
        for (a, x) in m.iter_mut().zip(&embeddings[i]) {
            *a += x;
        }
    }
    super::normalize(&mut m);
    m
}

/// The theme a vector belongs to: the nearest of `themes` at or above `floor`.
fn placed(themes: &[Vec<f32>], e: &[f32], floor: f32) -> Option<usize> {
    themes.iter().enumerate().map(|(i, t)| (i, unit_cosine(t, e))).filter(|(_, c)| *c >= floor).max_by(|a, b| a.1.total_cmp(&b.1)).map(|(i, _)| i)
}

/// Both shapes of `embeddings` (each L2-normalized, one dim) at subspace floor `floor`, minted on
/// `embeddings` and routed against `held_out` (more of the same corpus) and `contrast` (unrelated).
pub fn compare(embeddings: &[Vec<f32>], held_out: &[Vec<f32>], contrast: &[Vec<f32>], floor: f32) -> ShapeComparison {
    let dim = embeddings.first().map(Vec::len).expect("a comparison needs a corpus"); // test fixture: the validator compares a non-empty corpus
    let mut centroid = vec![0.0f32; dim];
    for e in embeddings {
        for (c, x) in centroid.iter_mut().zip(e) {
            *c += x;
        }
    }
    super::normalize(&mut centroid);

    let started = std::time::Instant::now();
    let mut clusters = detect_clusters(embeddings, floor, 3); // the pairwise path as it was minted: three members made a theme
    clusters.sort_by_key(|c| std::cmp::Reverse(c.indices.len()));
    clusters.truncate(MAX_SUBSPACES);
    let pairwise: Vec<Vec<f32>> = clusters.iter().map(|c| mean(embeddings, &c.indices, dim)).collect();
    let pairwise_region = region_of(&centroid, &pairwise, embeddings);
    let pairwise_ms = started.elapsed().as_secs_f64() * 1000.0;

    let started = std::time::Instant::now();
    let one_pass = themes::themes(embeddings, floor, MAX_SUBSPACES);
    let one_pass_region = region_of(&centroid, &one_pass, embeddings);
    let one_pass_ms = started.elapsed().as_secs_f64() * 1000.0;

    let matched: Vec<f32> = pairwise.iter().map(|p| one_pass.iter().map(|t| unit_cosine(p, t)).fold(f32::MIN, f32::max)).collect();
    // each pairwise subspace's nearest one-pass theme: the label the two shapes should agree on
    let label: Vec<Option<usize>> = pairwise.iter().map(|p| placed(&one_pass, p, f32::MIN)).collect();
    let mut cluster_of = vec![None; embeddings.len()];
    for (c, cluster) in clusters.iter().enumerate() {
        for &i in &cluster.indices {
            cluster_of[i] = Some(c);
        }
    }
    let (mut either, mut differ) = (0usize, 0usize);
    for (i, e) in embeddings.iter().enumerate() {
        let theirs = cluster_of[i].and_then(|c| label[c]);
        let ours = placed(&one_pass, e, floor);
        if theirs.is_some() || ours.is_some() {
            either += 1;
            differ += usize::from(theirs != ours);
        }
    }
    ShapeComparison {
        vectors: embeddings.len(),
        floor,
        pairwise: Shaped {
            subspaces: pairwise.len(),
            region: pairwise_region,
            ms: pairwise_ms,
            held_out_reached: reached(&centroid, &pairwise, pairwise_region, held_out),
            contrast_reached: reached(&centroid, &pairwise, pairwise_region, contrast),
        },
        one_pass: Shaped {
            subspaces: one_pass.len(),
            region: one_pass_region,
            ms: one_pass_ms,
            held_out_reached: reached(&centroid, &one_pass, one_pass_region, held_out),
            contrast_reached: reached(&centroid, &one_pass, one_pass_region, contrast),
        },
        matched,
        assignment_disagreement: if either == 0 { 0.0 } else { differ as f32 / either as f32 },
    }
}
