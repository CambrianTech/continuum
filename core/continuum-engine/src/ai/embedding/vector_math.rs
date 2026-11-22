//! The vector math every embedding kind shares once it is a flat `&[f32]`: the SIMD
//! kernels, query batches, top-k and connected-component clusters (computed without a pairwise matrix).
//! Pure CPU math with no service state, so it lives with the embedding owner; the
//! embedding commands and the genome's signature/competence clustering all call it here.

use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use wide::f32x8;

use super::cosine_similarity;

// ─── Similarity Functions ───────────────────────────────────────────────────

// ─── SIMD Kernels (the ONE place the float math is vectorized) ────────────────
//
// Every embedding kind — Qwen3-Embedding, bge, vision, whatever the adapter
// produces — converges to a flat `&[f32]`. The model of origin is irrelevant
// once it's a vector, so there is exactly ONE similarity kernel, optimized once
// and shared by all kinds (the compression principle). Lanes of 8 via
// `wide::f32x8` (NEON on aarch64, AVX on x86) handle any dimension; a scalar tail
// mops up the < 8 remainder. `wide` GUARANTEES the SIMD path regardless of
// opt-level — unlike auto-vectorization, which silently reverts to scalar in
// debug and is fragile to refactors.

/// Dot product + the TARGET's squared L2 norm only. The batch caller hoists the
/// query norm (computed once, reused across all targets — per the reuse
/// doctrine), so this does 2 FMAs per lane instead of 3. Caller guarantees
/// `query.len() == target.len()`.
#[inline]
fn simd_dot_and_target_norm(query: &[f32], target: &[f32]) -> (f32, f32) {
    let mut dot = f32x8::ZERO;
    let mut nt = f32x8::ZERO;
    let (cq, rq) = query.as_chunks::<8>();
    let (ct, rt) = target.as_chunks::<8>();
    for (qq, qt) in cq.iter().zip(ct.iter()) {
        let vq = f32x8::new(*qq);
        let vt = f32x8::new(*qt);
        dot = vq.mul_add(vt, dot);
        nt = vt.mul_add(vt, nt);
    }
    let mut d = dot.reduce_add();
    let mut st = nt.reduce_add();
    for (x, y) in rq.iter().zip(rt.iter()) {
        d += x * y;
        st += y * y;
    }
    (d, st)
}

/// SIMD L2 norm of a single vector.
#[inline]
fn simd_l2_norm(v: &[f32]) -> f32 {
    let mut acc = f32x8::ZERO;
    let (c, rem) = v.as_chunks::<8>();
    for q in c {
        let x = f32x8::new(*q);
        acc = x.mul_add(x, acc);
    }
    let mut s = acc.reduce_add();
    for x in rem {
        s += x * x;
    }
    s.sqrt()
}

/// Compute similarity of one query vector against multiple target vectors.
/// Returns Vec<f32> of similarities (one per target), parallelized with Rayon.
/// Use case: semantic search - find most similar items to a query.
pub fn query_similarity_batch(query: &[f32], targets: &[Vec<f32>]) -> Vec<f32> {
    if query.is_empty() {
        return vec![0.0; targets.len()];
    }
    // Hoist the query norm: compute ONCE, reuse across every target. The old path
    // recomputed it inside every cosine_similarity call (N redundant passes over
    // the query). Reuse, per doctrine.
    let q_norm = simd_l2_norm(query);
    if q_norm == 0.0 {
        return vec![0.0; targets.len()];
    }
    targets
        .par_iter()
        .map(|target| {
            if target.len() != query.len() {
                return 0.0;
            }
            let (dot, t_norm_sq) = simd_dot_and_target_norm(query, target);
            let denom = q_norm * t_norm_sq.sqrt();
            if denom == 0.0 {
                0.0
            } else {
                dot / denom
            }
        })
        .collect()
}

/// Find top-k most similar targets to a query.
/// Returns indices and similarities sorted by similarity descending.
pub fn top_k_similar(
    query: &[f32],
    targets: &[Vec<f32>],
    k: usize,
    threshold: f32,
) -> Vec<(usize, f32)> {
    if query.is_empty() {
        return vec![];
    }
    // Hoist the query norm out of the per-target loop (computed once, reused).
    let q_norm = simd_l2_norm(query);
    if q_norm == 0.0 {
        return vec![];
    }
    let mut sorted: Vec<(usize, f32)> = targets
        .par_iter()
        .enumerate()
        .map(|(i, target)| {
            if target.len() != query.len() {
                return (i, 0.0);
            }
            let (dot, t_norm_sq) = simd_dot_and_target_norm(query, target);
            let denom = q_norm * t_norm_sq.sqrt();
            let sim = if denom == 0.0 { 0.0 } else { dot / denom };
            (i, sim)
        })
        .filter(|(_, sim)| *sim >= threshold)
        .collect();

    // Sort by similarity descending and take top k
    sorted.sort_by(|a, b| b.1.total_cmp(&a.1));
    sorted.truncate(k);
    sorted
}

// ─── Clustering Functions ───────────────────────────────────────────────────

/// Cluster result from connected components clustering.
#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/embedding/Cluster.ts"
)]
pub struct Cluster {
    /// Indices of items in this cluster
    pub indices: Vec<usize>,
    /// Average intra-cluster similarity (cluster cohesion)
    pub strength: f32,
    /// Index of the most representative item (highest avg similarity to others)
    pub representative: usize,
}

/// Detect clusters using connected components algorithm.
/// Two items are connected if their similarity >= min_similarity.
/// Returns clusters sorted by strength (descending).
pub fn detect_clusters(
    embeddings: &[Vec<f32>],
    min_similarity: f32,
    min_cluster_size: usize,
) -> Vec<Cluster> {
    let n = embeddings.len();
    if n < min_cluster_size {
        return vec![];
    }

    // Similarities are computed where they are needed and never stored: a full pairwise matrix (and its
    // pair list) of a 26,415-example corpus is ~7 GB, and nine of them at once exhausted the 5090's host
    // memory inside the trainer (2026-11-22, symbolized dump; card 8ad3f6f3). Same similarity, same
    // components, same strength and representative; memory O(n).
    let sim = |i: usize, j: usize| -> f32 {
        if i == j {
            1.0
        } else {
            cosine_similarity(&embeddings[i], &embeddings[j])
        }
    };

    // Connected components via BFS over a shrinking unvisited list: each similarity is computed once
    // between a popped node and what is still unclaimed (about n^2/2 in all, as the matrix had).
    let mut unvisited: Vec<usize> = (0..n).collect();
    let mut clusters = Vec::new();

    while let Some(start) = unvisited.pop() {
        let mut component = Vec::new();
        let mut queue = vec![start];
        while let Some(node) = queue.pop() {
            component.push(node);
            // claim the neighbors above threshold among the unclaimed, scanned in parallel
            let near: Vec<bool> = unvisited.par_iter().map(|&other| sim(node, other) >= min_similarity).collect();
            let mut rest = Vec::with_capacity(unvisited.len());
            for (other, is_near) in unvisited.drain(..).zip(near) {
                if is_near { queue.push(other) } else { rest.push(other) }
            }
            unvisited = rest;
        }

        // Only keep clusters meeting minimum size
        if component.len() >= min_cluster_size {
            // One pass over the component's pairs: each similarity adds to both members' sums (the
            // representative's average) and to the total (the strength), per-thread sums merged after.
            let k = component.len();
            let (total_sim, sums) = (0..k)
                .into_par_iter()
                .fold(
                    || (0.0f32, vec![0.0f32; k]),
                    |(mut total, mut sums), i| {
                        for j in i + 1..k {
                            let s = sim(component[i], component[j]);
                            total += s;
                            sums[i] += s;
                            sums[j] += s;
                        }
                        (total, sums)
                    },
                )
                .reduce(
                    || (0.0f32, vec![0.0f32; k]),
                    |(ta, mut sa), (tb, sb)| {
                        for (x, y) in sa.iter_mut().zip(sb) {
                            *x += y;
                        }
                        (ta + tb, sa)
                    },
                );
            let pairs = k * (k - 1) / 2;
            let strength = if pairs > 0 { total_sim / pairs as f32 } else { 1.0 };
            // the highest average; a tie goes to the smallest index, whatever order the BFS met them in
            let representative = sums
                .iter()
                .zip(&component)
                .max_by(|a, b| a.0.total_cmp(b.0).then(b.1.cmp(a.1)))
                .map(|(_, &item)| item)
                .unwrap_or(component[0]); // unwrap_or: a component holds at least its BFS start, so max_by is never empty; component[0] is that start

            clusters.push(Cluster {
                indices: component,
                strength,
                representative,
            });
        }
    }

    // Sort by strength descending
    clusters.sort_by(|a, b| b.strength.total_cmp(&a.strength));
    clusters
}

#[cfg(test)]
mod tests {
    //! Pure vector-math tests. No model, no pool — just the relevance math the
    //! adapter-produced vectors are scored with.
    use super::*;

    #[test]
    fn cosine_identical_is_one() {
        let v = vec![0.5, 0.25, 0.75, 0.1];
        assert!((cosine_similarity(&v, &v) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_orthogonal_is_zero() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        assert!(cosine_similarity(&a, &b).abs() < 1e-6);
    }

    #[test]
    fn cosine_dimension_mismatch_is_zero() {
        // what this catches: mismatched dims must not panic / index OOB.
        assert_eq!(cosine_similarity(&[1.0, 2.0], &[1.0, 2.0, 3.0]), 0.0);
    }

    #[test]
    fn simd_matches_scalar_reference_across_dims() {
        // what this catches: the wide::f32x8 path (chunks of 8 + scalar
        // remainder) must agree with a naive scalar cosine for ANY dimension —
        // lengths < 8, exact multiples of 8, and non-multiples. Regression guard
        // for the SIMD rewrite of cosine_similarity. Epsilon (not exact) because
        // SIMD reorders the float summation vs a left-to-right scalar loop.
        fn scalar_cosine(a: &[f32], b: &[f32]) -> f32 {
            let mut dot = 0.0f32;
            let mut na = 0.0f32;
            let mut nb = 0.0f32;
            for i in 0..a.len() {
                dot += a[i] * b[i];
                na += a[i] * a[i];
                nb += b[i] * b[i];
            }
            let denom = na.sqrt() * nb.sqrt();
            if denom == 0.0 {
                0.0
            } else {
                dot / denom
            }
        }
        for &dim in &[1usize, 3, 7, 8, 9, 15, 16, 17, 768, 769] {
            // deterministic pseudo-vectors (no rng dep)
            let a: Vec<f32> = (0..dim).map(|i| ((i % 7) as f32) * 0.13 - 0.4).collect();
            let b: Vec<f32> = (0..dim).map(|i| ((i % 5) as f32) * 0.21 + 0.05).collect();
            let simd = cosine_similarity(&a, &b);
            let scalar = scalar_cosine(&a, &b);
            assert!(
                (simd - scalar).abs() < 1e-5,
                "dim {dim}: simd {simd} vs scalar {scalar}"
            );
        }
    }

    #[test]
    fn batch_hoisted_norm_matches_per_call_cosine() {
        // what this catches: query_similarity_batch / top_k_similar hoist the
        // query norm out of the per-target loop (compute-once reuse); each result
        // must still equal cosine_similarity(query, target) computed per target.
        let query = vec![0.2f32, -0.5, 0.9, 0.1, 0.33, -0.7, 0.05, 0.8, 0.42];
        let targets = vec![
            vec![0.1f32, 0.4, -0.2, 0.7, 0.0, 0.6, -0.3, 0.2, 0.9],
            vec![-0.5f32, 0.5, 0.5, -0.5, 0.5, 0.5, 0.5, -0.5, 0.1],
        ];
        let batch = query_similarity_batch(&query, &targets);
        for (i, t) in targets.iter().enumerate() {
            let direct = cosine_similarity(&query, t);
            assert!(
                (batch[i] - direct).abs() < 1e-5,
                "target {i}: batch {} vs direct {}",
                batch[i],
                direct
            );
        }
    }

    #[test]
    fn top_k_respects_threshold_and_order() {
        // what this catches: threshold filtering + descending sort + truncation.
        let query = vec![1.0, 0.0];
        let targets = vec![
            vec![1.0, 0.0], // sim 1.0
            vec![0.0, 1.0], // sim 0.0
            vec![0.7, 0.7], // sim ~0.707
        ];
        let got = top_k_similar(&query, &targets, 10, 0.5);
        assert_eq!(got.len(), 2, "only sims >= 0.5 survive the threshold");
        assert_eq!(got[0].0, 0, "highest similarity (index 0) ranks first");
        assert_eq!(got[1].0, 2);
    }

    // what this catches (card 8ad3f6f3, the 5090's host OOM): the matrix-free clusters differing from
    // what the stored pairwise matrix produced. A chain (single-linkage joins through a middle member),
    // a tight pair, a singleton and an under-size pair: every component, strength and representative
    // equals the matrix oracle's, exactly (the same cosine, the same order of sums).
    #[test]
    fn matrix_free_clusters_equal_the_pairwise_matrix_oracle() {
        let embs: Vec<Vec<f32>> = vec![
            vec![1.0, 0.0, 0.0, 0.0],
            vec![0.95, 0.31, 0.0, 0.0], // joins 0
            vec![0.81, 0.59, 0.0, 0.0], // joins 1, not 0: a chain
            vec![0.0, 0.0, 1.0, 0.0],
            vec![0.0, 0.05, 0.99, 0.0], // a tight pair with 3
            vec![0.0, 0.0, 0.0, 1.0],   // alone
        ];
        let n = embs.len();
        let oracle_sim = |i: usize, j: usize| if i == j { 1.0 } else { cosine_similarity(&embs[i], &embs[j]) };
        let (floor, min) = (0.94, 2);
        let mut visited = vec![false; n];
        let mut oracle = Vec::new();
        for start in 0..n {
            if visited[start] {
                continue;
            }
            let (mut component, mut queue) = (Vec::new(), vec![start]);
            while let Some(node) = queue.pop() {
                if visited[node] {
                    continue;
                }
                visited[node] = true;
                component.push(node);
                for (j, &seen) in visited.iter().enumerate() {
                    if !seen && oracle_sim(node, j) >= floor {
                        queue.push(j);
                    }
                }
            }
            if component.len() >= min {
                oracle.push(component);
            }
        }
        let found = detect_clusters(&embs, floor, min);
        assert_eq!(found.len(), oracle.len(), "the same components");
        for cluster in &found {
            let expected = oracle.iter().find(|c| c.contains(&cluster.representative)).expect("test: its component exists");
            let (mut a, mut b) = (cluster.indices.clone(), expected.clone());
            a.sort_unstable();
            b.sort_unstable();
            assert_eq!(a, b, "the same members");
            // the oracle's strength (mean pairwise similarity) and representative (nearest the rest)
            let pairs: Vec<f32> = b.iter().enumerate().flat_map(|(i, &x)| b[i + 1..].iter().map(move |&y| (x, y))).map(|(x, y)| oracle_sim(x, y)).collect();
            let strength = pairs.iter().sum::<f32>() / pairs.len() as f32;
            assert!((cluster.strength - strength).abs() < 1e-6, "the same strength: {} vs {strength}", cluster.strength);
            let nearest = b.iter().copied().max_by(|&x, &y| {
                let avg = |m: usize| b.iter().filter(|&&o| o != m).map(|&o| oracle_sim(m, o)).sum::<f32>();
                avg(x).total_cmp(&avg(y)).then(y.cmp(&x)) // a tie goes to the smallest index
            });
            assert_eq!(Some(cluster.representative), nearest, "the same representative");
        }
        let chain = found.iter().find(|c| c.indices.contains(&0)).expect("test: the chain");
        assert_eq!(chain.indices.len(), 3, "single linkage joins 0 and 2 through 1");
        assert_eq!(chain.representative, 1, "the middle member is nearest the rest");
    }

    #[test]
    fn detect_clusters_groups_similar_vectors() {
        // what this catches: connected-components clustering finds the dense group.
        let embs = vec![
            vec![1.0, 0.0, 0.0],
            vec![0.99, 0.01, 0.0],
            vec![0.0, 0.0, 1.0],
        ];
        let clusters = detect_clusters(&embs, 0.9, 2);
        assert_eq!(clusters.len(), 1, "the two near-identical vectors cluster");
        assert_eq!(clusters[0].indices.len(), 2);
    }
}
