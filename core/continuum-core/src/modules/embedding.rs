//! EmbeddingModule — pure vector math service (similarity / clustering).
//!
//! Handles: embedding/similarity, embedding/similarity-matrix, embedding/top-k,
//!          embedding/cluster
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
use serde::Serialize;
use serde_json::{json, Value};
use std::any::Any;
use std::time::Instant;
use tracing::info;

use crate::utils::params::Params;
use std::sync::Arc;

// ─── Adapter-Routed Embedder Construction ─────────────────────────────────────

/// Build the adapter-routed embedder used by background/IPC embedding work
/// (Hippocampus boot, `vector/backfill`) — the SAME async, content-addressed,
/// neural-or-lexical embedder the live recall path uses
/// ([`crate::cognition::embedding::resolve_recall_embedder`]).
///
/// Connects to the unsloth gateway and resolves the embed model (neural when it
/// serves, lexical otherwise). This is the ONE place the gateway-adapter build is
/// expressed for the memory side, so callers don't each re-derive base-url /
/// model-selection / probe logic.
///
/// Fail-loud (no silent ONNX fallback, task #40): if the gateway is unreachable
/// or serves no model the error is propagated with its cause named — the caller
/// decides whether that is fatal (it is for `vector/backfill`).
pub async fn build_adapter_embedder(
) -> Result<Arc<dyn crate::cognition::embedding::EmbeddingProvider>, String> {
    // Hard bound on the whole gateway handshake. The gateway can be unreachable
    // OR responsive-but-hung (e.g. an unsloth gateway that holds `/v1/models`
    // open). Either way a caller must NEVER block indefinitely — fail loud with
    // a named cause so the caller (e.g. `vector/backfill`) can decide.
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        build_adapter_embedder_inner(),
    )
    .await
    .map_err(|_| {
        "unsloth embed gateway did not respond within 5s while resolving an embed model \
         (gateway unreachable or hung) — no local ONNX fallback"
            .to_string()
    })?
}

async fn build_adapter_embedder_inner(
) -> Result<Arc<dyn crate::cognition::embedding::EmbeddingProvider>, String> {
    let base = crate::inference::unsloth_control::unsloth_base_url();
    let served = crate::inference::unsloth_control::UnslothHttp::from_config()
        .list_models()
        .await
        .map_err(|e| format!("unsloth gateway unreachable while selecting embed model ({base}): {e}"))?;
    let model = served.into_iter().next().ok_or_else(|| {
        format!("unsloth gateway @ {base} serves NO model — load one before embedding (no local ONNX fallback)")
    })?;
    // `initialize` is an `AIProviderAdapter` trait method — bring the trait into
    // scope so it resolves on the concrete adapter.
    use crate::ai::adapter::AIProviderAdapter as _;
    let mut adapter = crate::ai::openai_adapter::OpenAICompatibleAdapter::from_registry("unsloth")
        .with_runtime_base_url(base)
        .with_default_model(model);
    adapter
        .initialize()
        .await
        .map_err(|e| format!("unsloth embed adapter initialize failed: {e}"))?;
    Ok(crate::cognition::embedding::resolve_recall_embedder(Arc::new(adapter)).await)
}

// ─── Similarity Functions ───────────────────────────────────────────────────

/// Cosine similarity between two embedding vectors.
/// Returns value in [-1, 1] where 1 = identical, 0 = orthogonal, -1 = opposite.
/// SIMD-optimized in release mode via rustc auto-vectorization.
#[inline]
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let mut dot = 0.0f32;
    let mut norm_a = 0.0f32;
    let mut norm_b = 0.0f32;

    for i in 0..a.len() {
        dot += a[i] * b[i];
        norm_a += a[i] * a[i];
        norm_b += b[i] * b[i];
    }

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
    targets
        .par_iter()
        .map(|target| cosine_similarity(query, target))
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
    let similarities: Vec<(usize, f32)> = targets
        .par_iter()
        .enumerate()
        .map(|(i, target)| (i, cosine_similarity(query, target)))
        .filter(|(_, sim)| *sim >= threshold)
        .collect();

    // Sort by similarity descending and take top k
    let mut sorted = similarities;
    sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    sorted.truncate(k);
    sorted
}

// ─── Clustering Functions ───────────────────────────────────────────────────

/// Cluster result from connected components clustering.
#[derive(Serialize)]
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

    /// Handle embedding/similarity - compute cosine similarity between two embeddings
    fn handle_similarity(&self, params: &Value) -> Result<CommandResult, String> {
        let p = Params::new(params);
        let a: Vec<f32> = p.json("a")?;
        let b: Vec<f32> = p.json("b")?;

        if a.len() != b.len() {
            return Err(format!("Dimension mismatch: {} vs {}", a.len(), b.len()));
        }

        let similarity = cosine_similarity(&a, &b);

        Ok(CommandResult::Json(json!({
            "similarity": similarity,
            "dimensions": a.len()
        })))
    }

    /// Handle embedding/similarity-matrix - compute pairwise similarities in parallel
    ///
    /// Takes an array of embeddings, returns lower-triangular similarity matrix.
    /// For n embeddings, returns n*(n-1)/2 similarity values.
    fn handle_similarity_matrix(&self, params: &Value) -> Result<CommandResult, String> {
        let p = Params::new(params);
        let embeddings: Vec<Vec<f32>> = p.json("embeddings")?;

        let n = embeddings.len();
        if n < 2 {
            return Ok(CommandResult::Json(json!({
                "similarities": [],
                "count": n,
                "pairs": 0
            })));
        }

        // Verify all embeddings have same dimensions
        let dim = embeddings[0].len();
        for (i, emb) in embeddings.iter().enumerate() {
            if emb.len() != dim {
                return Err(format!(
                    "Dimension mismatch at index {}: expected {}, got {}",
                    i,
                    dim,
                    emb.len()
                ));
            }
        }

        let start = Instant::now();
        let similarities = pairwise_similarity_matrix(&embeddings);
        let duration_ms = start.elapsed().as_millis() as u64;

        let num_pairs = similarities.len();
        info!(
            "Computed {} pairwise similarities ({} embeddings, {}d) in {}ms",
            num_pairs, n, dim, duration_ms
        );

        // Return as binary for efficiency (avoid JSON number serialization overhead)
        let bytes: Vec<u8> = similarities.iter().flat_map(|f| f.to_le_bytes()).collect();

        Ok(CommandResult::Binary {
            metadata: json!({
                "type": "binary",
                "length": bytes.len(),
                "dtype": "f32",
                "count": n,
                "pairs": num_pairs,
                "dimensions": dim,
                "durationMs": duration_ms
            }),
            data: bytes,
        })
    }

    /// Handle embedding/top-k - find top-k most similar embeddings to a query
    ///
    /// Takes a query embedding and array of target embeddings, returns indices
    /// and similarities of top-k matches. Parallelized with Rayon.
    fn handle_top_k(&self, params: &Value) -> Result<CommandResult, String> {
        let p = Params::new(params);
        let query: Vec<f32> = p.json("query")?;
        let targets: Vec<Vec<f32>> = p.json("targets")?;
        let k = p.u64_or("k", 10) as usize;
        let threshold = p.f64_or("threshold", 0.0) as f32;

        if targets.is_empty() {
            return Ok(CommandResult::Json(json!({
                "results": [],
                "count": 0
            })));
        }

        // Verify dimensions match
        let dim = query.len();
        for (i, target) in targets.iter().enumerate() {
            if target.len() != dim {
                return Err(format!(
                    "Dimension mismatch at target index {}: expected {}, got {}",
                    i,
                    dim,
                    target.len()
                ));
            }
        }

        let start = Instant::now();
        let results = top_k_similar(&query, &targets, k, threshold);
        let duration_ms = start.elapsed().as_millis() as u64;

        info!(
            "Found {} top-k matches from {} targets ({}d) in {}ms",
            results.len(),
            targets.len(),
            dim,
            duration_ms
        );

        // Return as array of {index, similarity} objects
        let result_objects: Vec<Value> = results
            .iter()
            .map(|(idx, sim)| json!({ "index": idx, "similarity": sim }))
            .collect();

        Ok(CommandResult::Json(json!({
            "results": result_objects,
            "count": results.len(),
            "totalTargets": targets.len(),
            "k": k,
            "threshold": threshold,
            "dimensions": dim,
            "durationMs": duration_ms
        })))
    }

    /// Handle embedding/cluster - detect clusters via connected components
    ///
    /// Takes embeddings and clustering parameters, returns cluster assignments.
    /// Full clustering algorithm in Rust (similarity matrix + connected components).
    fn handle_cluster(&self, params: &Value) -> Result<CommandResult, String> {
        let p = Params::new(params);
        let embeddings: Vec<Vec<f32>> = p.json("embeddings")?;
        let min_similarity = p.f64_or("minSimilarity", 0.7) as f32;
        let min_cluster_size = p.u64_or("minClusterSize", 2) as usize;

        let n = embeddings.len();
        if n < min_cluster_size {
            return Ok(CommandResult::Json(json!({
                "clusters": [],
                "count": n,
                "clusterCount": 0
            })));
        }

        // Verify all embeddings have same dimensions
        let dim = embeddings[0].len();
        for (i, emb) in embeddings.iter().enumerate() {
            if emb.len() != dim {
                return Err(format!(
                    "Dimension mismatch at index {}: expected {}, got {}",
                    i,
                    dim,
                    emb.len()
                ));
            }
        }

        let start = Instant::now();
        let clusters = detect_clusters(&embeddings, min_similarity, min_cluster_size);
        let duration_ms = start.elapsed().as_millis() as u64;

        let cluster_count = clusters.len();
        info!(
            "Detected {} clusters from {} embeddings ({}d) in {}ms",
            cluster_count, n, dim, duration_ms
        );

        Ok(CommandResult::Json(json!({
            "clusters": clusters,
            "count": n,
            "clusterCount": cluster_count,
            "dimensions": dim,
            "minSimilarity": min_similarity,
            "minClusterSize": min_cluster_size,
            "durationMs": duration_ms
        })))
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

    async fn handle_command(&self, command: &str, params: Value) -> Result<CommandResult, String> {
        match command {
            "embedding/similarity" => self.handle_similarity(&params),
            "embedding/similarity-matrix" => self.handle_similarity_matrix(&params),
            "embedding/top-k" => self.handle_top_k(&params),
            "embedding/cluster" => self.handle_cluster(&params),
            _ => Err(format!(
                "Unknown embedding command: {command}. Embedding generation is \
                 adapter-routed (ai/* via /v1/embeddings), not served here; this \
                 module only does vector math (similarity / matrix / top-k / cluster)."
            )),
        }
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
            vec![1.0, 0.0],   // sim 1.0
            vec![0.0, 1.0],   // sim 0.0
            vec![0.7, 0.7],   // sim ~0.707
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
