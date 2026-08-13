//! EmbeddingModule — the pure vector-math kernels (similarity / clustering).
//!
//! The `embedding/*` commands themselves (similarity, similarity-matrix, top-k,
//! cluster) are now typed stateless `ActionCommand`s under `commands/embedding/*`
//! that call the kernels in this file. This module no longer carries legacy
//! `handle_command` arms — its `handle_command` exists only to fail loud if a
//! typed registration is ever missing. What it still owns is the SIMD/Rayon math
//! (`cosine_similarity`, `pairwise_similarity_matrix`, `top_k_similar`,
//! `detect_clusters`, the `Cluster` wire type) plus `build_adapter_embedder` — the
//! single source of truth every embedding caller scores against.
//!
//! NOTE (task #40): embedding *generation* is async + adapter-routed
//! (unsloth / llama-server `/v1/embeddings` via the `AIProviderAdapter`, see
//! [`crate::cognition::embedding`]). The old in-process fastembed/ONNX model
//! cache, `generate_embedding{,s_batch}`, the `embedding/generate` +
//! `embedding/model/*` handlers, the embedding pool, and the GPU guards were
//! removed — embedding never loads an ONNX model in-process anymore. This module
//! now owns only the pure, Rayon-parallel relevance math (no model, no IPC hop,
//! no GPU allocation), which callers run over vectors the adapter already
//! produced.
//!
//! Priority: Normal — similarity math is CPU-bound but not time-critical.

use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use async_trait::async_trait;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::any::Any;

use std::sync::Arc;
use wide::f32x8;

// ─── Adapter-Routed Embedder Construction ─────────────────────────────────────

/// Build the adapter-routed embedder used by background/IPC embedding work
/// (Hippocampus boot, `vector/backfill`) — the SAME async, content-addressed,
/// neural-or-lexical embedder the live recall path uses
/// ([`crate::cognition::embedding::resolve_recall_embedder`]).
///
/// Reads the served model from Contract A (the serving daemon's snapshot) and
/// builds the gateway adapter against it. llama-server serves embeddings from the
/// SAME process it serves chat from (launched with `--embeddings`), so the embed
/// model IS the served model — there is no separate embed gateway to probe. This
/// is the ONE place the gateway-adapter build is expressed for the memory side,
/// so callers don't each re-derive base-url / model-selection.
///
/// Fail-loud (no silent ONNX fallback, task #40): no ready served model within the
/// readiness bound, or a ready snapshot that names no model, is propagated with its
/// cause named — the caller decides whether that is fatal (it is for
/// `vector/backfill`). The readiness wait is bounded by Contract A's single
/// [`DEFAULT_SERVING_WAIT`](crate::inference::llama_server::DEFAULT_SERVING_WAIT);
/// reading the snapshot never hangs (a `watch` borrow, not an HTTP probe), so there
/// is no per-module timeout to maintain — the old 5s guard existed only because an
/// unsloth gateway could hold `/v1/models` open, a hazard the snapshot removes.
pub async fn build_adapter_embedder(
) -> Result<Arc<dyn crate::cognition::embedding::EmbeddingProvider>, String> {
    // `initialize` is an `AIProviderAdapter` trait method — bring the trait into
    // scope so it resolves on the concrete adapter.
    use crate::ai::adapter::AIProviderAdapter as _;

    // Read the served model from the daemon's published snapshot instead of each
    // embedder re-probing `/v1/models` ("subscribers READ the snapshot, they do
    // NOT each HTTP-probe" — Contract A). Wait bounded for the daemon's first
    // ready reconcile (covers a cold GGUF load); no ready model → fail loud.
    let snap = crate::inference::llama_server::await_ready_serving(
        crate::inference::llama_server::DEFAULT_SERVING_WAIT,
    )
    .await
    .ok_or_else(|| {
        "serving daemon brought up NO ready model within the readiness bound — \
         cannot select an embed model (no local ONNX fallback)"
            .to_string()
    })?;
    let model = snap.active_model.ok_or_else(|| {
        "serving snapshot is ready but names no active model — cannot embed \
         (no local ONNX fallback)"
            .to_string()
    })?;
    let mut adapter = crate::ai::openai_adapter::OpenAICompatibleAdapter::from_registry(
        crate::inference::llama_server::PROVIDER_ID,
    )
    .with_runtime_base_url(snap.base_url)
    .with_default_model(model);
    adapter
        .initialize()
        .await
        .map_err(|e| format!("embed adapter initialize failed: {e}"))?;
    Ok(crate::cognition::embedding::resolve_recall_embedder(Arc::new(adapter)).await)
}

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

/// Dot product + both squared L2 norms in a single pass. Caller guarantees
/// `a.len() == b.len()`.
#[inline]
fn simd_dot_and_norms(a: &[f32], b: &[f32]) -> (f32, f32, f32) {
    let mut dot = f32x8::ZERO;
    let mut na = f32x8::ZERO;
    let mut nb = f32x8::ZERO;
    let mut ca = a.chunks_exact(8);
    let mut cb = b.chunks_exact(8);
    for (qa, qb) in ca.by_ref().zip(cb.by_ref()) {
        let va = f32x8::new(qa.try_into().unwrap());
        let vb = f32x8::new(qb.try_into().unwrap());
        dot = va.mul_add(vb, dot);
        na = va.mul_add(va, na);
        nb = vb.mul_add(vb, nb);
    }
    let mut d = dot.reduce_add();
    let mut sa = na.reduce_add();
    let mut sb = nb.reduce_add();
    for (x, y) in ca.remainder().iter().zip(cb.remainder().iter()) {
        d += x * y;
        sa += x * x;
        sb += y * y;
    }
    (d, sa, sb)
}

/// Dot product + the TARGET's squared L2 norm only. The batch caller hoists the
/// query norm (computed once, reused across all targets — per the reuse
/// doctrine), so this does 2 FMAs per lane instead of 3. Caller guarantees
/// `query.len() == target.len()`.
#[inline]
fn simd_dot_and_target_norm(query: &[f32], target: &[f32]) -> (f32, f32) {
    let mut dot = f32x8::ZERO;
    let mut nt = f32x8::ZERO;
    let mut cq = query.chunks_exact(8);
    let mut ct = target.chunks_exact(8);
    for (qq, qt) in cq.by_ref().zip(ct.by_ref()) {
        let vq = f32x8::new(qq.try_into().unwrap());
        let vt = f32x8::new(qt.try_into().unwrap());
        dot = vq.mul_add(vt, dot);
        nt = vt.mul_add(vt, nt);
    }
    let mut d = dot.reduce_add();
    let mut st = nt.reduce_add();
    for (x, y) in cq.remainder().iter().zip(ct.remainder().iter()) {
        d += x * y;
        st += y * y;
    }
    (d, st)
}

/// SIMD L2 norm of a single vector.
#[inline]
fn simd_l2_norm(v: &[f32]) -> f32 {
    let mut acc = f32x8::ZERO;
    let mut c = v.chunks_exact(8);
    for q in c.by_ref() {
        let x = f32x8::new(q.try_into().unwrap());
        acc = x.mul_add(x, acc);
    }
    let mut s = acc.reduce_add();
    for x in c.remainder() {
        s += x * x;
    }
    s.sqrt()
}

/// Cosine similarity between two embedding vectors.
/// Returns value in [-1, 1] where 1 = identical, 0 = orthogonal, -1 = opposite.
/// SIMD-vectorized via `wide::f32x8` (see kernel note above).
#[inline]
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let (dot, norm_a, norm_b) = simd_dot_and_norms(a, b);
    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom == 0.0 {
        0.0
    } else {
        dot / denom
    }
}

/// Compute pairwise cosine similarity matrix in parallel.
/// Returns flattened lower-triangular matrix (excluding diagonal) as Vec<f32>.
/// For n vectors, returns n*(n-1)/2 similarities.
///
/// Layout: [(0,1), (0,2), ..., (0,n-1), (1,2), (1,3), ..., (n-2,n-1)]
///
/// This is O(n²) but parallelized with Rayon for significant speedup.
pub fn pairwise_similarity_matrix(embeddings: &[Vec<f32>]) -> Vec<f32> {
    let n = embeddings.len();
    if n < 2 {
        return vec![];
    }

    // Number of pairs: n choose 2 = n*(n-1)/2
    let num_pairs = n * (n - 1) / 2;

    // Pre-allocate result
    let mut result = vec![0.0f32; num_pairs];

    // Generate all (i,j) pairs where i < j
    let pairs: Vec<(usize, usize)> = (0..n)
        .flat_map(|i| (i + 1..n).map(move |j| (i, j)))
        .collect();

    // Compute similarities in parallel with Rayon
    pairs
        .par_iter()
        .zip(result.par_iter_mut())
        .for_each(|((i, j), sim)| {
            *sim = cosine_similarity(&embeddings[*i], &embeddings[*j]);
        });

    result
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
    sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
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

    // Compute full similarity matrix (needed for cluster strength)
    let similarities = pairwise_similarity_matrix(embeddings);

    // Helper to get similarity from flat array
    let get_sim = |i: usize, j: usize| -> f32 {
        if i == j {
            return 1.0;
        }
        let (a, b) = if i < j { (i, j) } else { (j, i) };
        let idx = a * n - (a * (a + 1)) / 2 + (b - a - 1);
        similarities[idx]
    };

    // Connected components via BFS
    let mut visited = vec![false; n];
    let mut clusters = Vec::new();

    for start in 0..n {
        if visited[start] {
            continue;
        }

        // BFS to find connected component
        let mut component = Vec::new();
        let mut queue = vec![start];

        while let Some(node) = queue.pop() {
            if visited[node] {
                continue;
            }
            visited[node] = true;
            component.push(node);

            // Add neighbors above threshold
            for (neighbor, &is_visited) in visited.iter().enumerate() {
                if !is_visited && get_sim(node, neighbor) >= min_similarity {
                    queue.push(neighbor);
                }
            }
        }

        // Only keep clusters meeting minimum size
        if component.len() >= min_cluster_size {
            // Compute cluster strength (average intra-cluster similarity)
            let mut total_sim = 0.0f32;
            let mut count = 0;
            for (i, &a) in component.iter().enumerate() {
                for &b in component.iter().skip(i + 1) {
                    total_sim += get_sim(a, b);
                    count += 1;
                }
            }
            let strength = if count > 0 {
                total_sim / count as f32
            } else {
                1.0
            };

            // Find representative (highest avg similarity to others in cluster)
            let representative = component
                .iter()
                .map(|&item| {
                    let avg: f32 = component
                        .iter()
                        .filter(|&&other| other != item)
                        .map(|&other| get_sim(item, other))
                        .sum::<f32>()
                        / (component.len() - 1).max(1) as f32;
                    (item, avg)
                })
                .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(item, _)| item)
                .unwrap_or(component[0]);

            clusters.push(Cluster {
                indices: component,
                strength,
                representative,
            });
        }
    }

    // Sort by strength descending
    clusters.sort_by(|a, b| {
        b.strength
            .partial_cmp(&a.strength)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    clusters
}

pub struct EmbeddingModule;

impl EmbeddingModule {
    pub fn new() -> Self {
        Self
    }
}

impl Default for EmbeddingModule {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ServiceModule for EmbeddingModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "embedding",
            priority: ModulePriority::Normal,
            command_prefixes: &["embedding/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            // Pure CPU similarity math (Rayon). Serialize at the runtime level
            // so concurrent matrix/cluster requests don't multiply Rayon
            // threadpools during persona bursts.
            max_concurrency: 1,
            tick_interval: None,
        }
    }

    async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
        Ok(())
    }

    async fn handle_command(&self, command: &str, _params: Value) -> Result<CommandResult, String> {
        // The vector-math commands (similarity / matrix / top-k / cluster) are now
        // typed stateless ActionCommands under `commands/embedding/*` — the executor
        // routes them via the typed path (`route_object`) before it ever reaches a
        // module's legacy handler. If one lands here, the typed registration is
        // missing: fail loud rather than silently re-implement it.
        Err(format!(
            "'{command}' did not resolve on the typed command registry. Embedding \
             vector math is served by the stateless `commands/embedding/*` commands; \
             embedding *generation* is adapter-routed (ai/* via /v1/embeddings). This \
             module no longer carries a legacy handler — this error means the typed \
             command failed to register, not that the command is unknown."
        ))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
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
    fn pairwise_matrix_has_n_choose_2_entries() {
        let embs = vec![vec![1.0, 0.0], vec![0.0, 1.0], vec![1.0, 1.0]];
        let m = pairwise_similarity_matrix(&embs);
        assert_eq!(m.len(), 3, "3 choose 2 = 3 pairs");
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
