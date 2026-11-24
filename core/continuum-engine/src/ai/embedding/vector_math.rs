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
/// PURE: one pass over the `k` members' pairs, summing `sim(i, j)` into the total (a cluster's
/// strength) and into both members' sums (the representative's average). In f64: an f32 total stops
/// gaining past 2^24 (Codex on #6229, the 5090: 12,000 identical vectors scored 0.466, not 1). Per-thread
/// sums are merged, so the floats agree with a sequential sum to rounding, not bit for bit; memory O(k).
fn pair_sums(k: usize, sim: impl Fn(usize, usize) -> f32 + Sync) -> (f64, Vec<f64>) {
    (0..k)
        .into_par_iter()
        .fold(
            || (0.0f64, vec![0.0f64; k]),
            |(mut total, mut sums), i| {
                for j in i + 1..k {
                    let s = f64::from(sim(i, j));
                    total += s;
                    sums[i] += s;
                    sums[j] += s;
                }
                (total, sums)
            },
        )
        .reduce(
            || (0.0f64, vec![0.0f64; k]),
            |(ta, mut sa), (tb, sb)| {
                for (x, y) in sa.iter_mut().zip(sb) {
                    *x += y;
                }
                (ta + tb, sa)
            },
        )
}

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

    // Components are found in the same order as before (each at its smallest index, starts ascending),
    // so the strength sort's ties keep their old order; a member is claimed when it is first reached,
    // never pushed twice (the matrix version's stack could hold ~n^2/2 duplicates in one giant
    // component), so memory stays O(n). `remaining` holds the unclaimed indices, ascending.
    let mut remaining: std::collections::VecDeque<usize> = (0..n).collect();
    let mut clusters = Vec::new();

    while let Some(start) = remaining.pop_front() {
        let mut component = Vec::new();
        let mut queue = vec![start];
        while let Some(node) = queue.pop() {
            component.push(node);
            let near: Vec<bool> = remaining.par_iter().map(|&other| sim(node, other) >= min_similarity).collect();
            let mut keep = std::collections::VecDeque::with_capacity(remaining.len());
            for (other, is_near) in remaining.drain(..).zip(near) {
                if is_near { queue.push(other) } else { keep.push_back(other) }
            }
            remaining = keep;
        }

        // Only keep clusters meeting minimum size
        if component.len() >= min_cluster_size {
            // the total is the strength, each member's sum its average (pair_sums, in f64)
            let k = component.len();
            let (total_sim, sums) = pair_sums(k, |i, j| sim(component[i], component[j]));
            let pairs = k * (k - 1) / 2;
            let strength = if pairs > 0 { (total_sim / pairs as f64) as f32 } else { 1.0 };
            // the highest average; a tie goes to the smallest index (the matrix version took whichever
            // its traversal met last, an order this one no longer reproduces)
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

    // what this catches (Codex on #6229, the 5090: 12,000 identical vectors scored 0.466, not 1): a
    // cluster's pair total accumulated in f32, which stops gaining past 2^24, so a large cluster's
    // strength (and its representative's average) comes out wrong and themes mis-rank. 6,000 members
    // give ~18M pairs, past 2^24, with every similarity 1: the total must be exactly the pair count.
    #[test]
    fn a_large_clusters_pair_sums_stay_exact_past_f32s_range() {
        let k = 6_000usize;
        let (total, sums) = pair_sums(k, |_, _| 1.0);
        let pairs = (k * (k - 1) / 2) as f64;
        assert!(pairs > f64::from(1u32 << 24), "test: the cluster is past f32's exact range");
        assert_eq!(total, pairs, "every pair counted exactly");
        assert_eq!((total / pairs) as f32, 1.0, "so identical members score a strength of exactly 1");
        assert!(sums.iter().all(|&s| s == (k - 1) as f64), "and each member's sum is its k-1 pairs");
    }

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

    // what this catches (card 8ad3f6f3, the 5090's host OOM; Codex on #6229): the matrix-free clusters
    // differing from what the stored pairwise matrix produced. The oracle is the old code, verbatim; the
    // corpus holds a chain (single linkage through a middle member) and five disjoint, equal-size,
    // equal-strength pairs (the strength sort's ties, and each pair's representative tie). Every cluster,
    // in the old order, has the same members and the same strength to rounding; the representative
    // follows the stated tie rule (smallest index), the one intended change.
    #[test]
    fn matrix_free_clusters_match_the_old_matrix_code() {
        fn oracle(embeddings: &[Vec<f32>], min_similarity: f32, min_cluster_size: usize) -> Vec<Cluster> {
            let n = embeddings.len();
            let pairs: Vec<(usize, usize)> = (0..n).flat_map(|i| (i + 1..n).map(move |j| (i, j))).collect();
            let similarities: Vec<f32> = pairs.iter().map(|&(i, j)| cosine_similarity(&embeddings[i], &embeddings[j])).collect();
            let get_sim = |i: usize, j: usize| -> f32 {
                if i == j {
                    return 1.0;
                }
                let (a, b) = if i < j { (i, j) } else { (j, i) };
                similarities[a * n - (a * (a + 1)) / 2 + (b - a - 1)]
            };
            let mut visited = vec![false; n];
            let mut clusters = Vec::new();
            for start in 0..n {
                if visited[start] {
                    continue;
                }
                let mut component = Vec::new();
                let mut queue = vec![start];
                while let Some(node) = queue.pop() {
                    if visited[node] {
                        continue;
                    }
                    visited[node] = true;
                    component.push(node);
                    for (neighbor, &is_visited) in visited.iter().enumerate() {
                        if !is_visited && get_sim(node, neighbor) >= min_similarity {
                            queue.push(neighbor);
                        }
                    }
                }
                if component.len() >= min_cluster_size {
                    let mut total_sim = 0.0f32;
                    let mut count = 0;
                    for (i, &a) in component.iter().enumerate() {
                        for &b in component.iter().skip(i + 1) {
                            total_sim += get_sim(a, b);
                            count += 1;
                        }
                    }
                    let strength = if count > 0 { total_sim / count as f32 } else { 1.0 };
                    let representative = component
                        .iter()
                        .map(|&item| {
                            // the old divisor was (len - 1).max(1); every kept component here holds at least two
                            let avg: f32 = component.iter().filter(|&&other| other != item).map(|&other| get_sim(item, other)).sum::<f32>()
                                / (component.len() - 1) as f32;
                            (item, avg)
                        })
                        .max_by(|a, b| a.1.total_cmp(&b.1))
                        .map(|(item, _)| item)
                        .expect("test: a component holds its start");
                    clusters.push(Cluster { indices: component, strength, representative });
                }
            }
            clusters.sort_by(|a, b| b.strength.total_cmp(&a.strength));
            clusters
        }
        let dim = 16;
        let axis = |k: usize, tilt: f32| {
            let mut v = vec![0.0f32; dim];
            v[k] = 1.0;
            v[(k + 1) % dim] = tilt;
            v
        };
        let mut embs = vec![axis(0, 0.0), axis(0, 0.33), axis(0, 0.7)]; // a chain: 0-1 and 1-2 join, 0-2 does not
        for k in [3, 5, 7, 9, 11] {
            embs.push(axis(k, 0.0)); // five pairs, each the same angle apart: equal size, equal strength
            embs.push(axis(k, 0.2));
        }
        embs.push(axis(14, 0.0)); // alone
        let (floor, min) = (0.94, 2);
        let found = detect_clusters(&embs, floor, min);
        let expected = oracle(&embs, floor, min);
        assert_eq!(found.len(), expected.len(), "the same number of clusters");
        for (got, want) in found.iter().zip(&expected) {
            let (mut a, mut b) = (got.indices.clone(), want.indices.clone());
            a.sort_unstable();
            b.sort_unstable();
            assert_eq!(a, b, "the same members, cluster by cluster in the old order (ties included)");
            assert!((got.strength - want.strength).abs() <= 1e-6, "the same strength to rounding: {} vs {}", got.strength, want.strength);
            // the representative: the highest average, a tie to the smallest index (an intended change:
            // the old code took whichever member its traversal met last)
            let avg = |m: usize| b.iter().filter(|&&o| o != m).map(|&o| cosine_similarity(&embs[m], &embs[o])).sum::<f32>();
            let best = b.iter().copied().max_by(|&x, &y| avg(x).total_cmp(&avg(y)).then(y.cmp(&x)));
            assert_eq!(Some(got.representative), best, "the representative by the stated rule");
        }
        assert!(expected.iter().any(|c| c.indices.len() == 3), "the fixture holds its chain");
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
