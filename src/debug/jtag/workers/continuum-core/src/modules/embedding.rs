//! EmbeddingModule — Native text embedding generation via fastembed (ONNX).
//!
//! Handles: embedding/generate, embedding/model/load, embedding/model/list,
//!          embedding/model/info, embedding/model/unload
//!
//! Benefits over Ollama HTTP:
//! - No network overhead (~5ms vs ~80ms per embedding)
//! - Batch processing (100 texts in ~100ms vs ~8s)
//! - No external service dependency
//! - True parallelism via ONNX Runtime
//!
//! Priority: Normal — embedding is not time-critical like voice.

use crate::runtime::{ServiceModule, ModuleConfig, ModulePriority, CommandResult, ModuleContext};
use async_trait::async_trait;
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use once_cell::sync::OnceCell;
use rayon::prelude::*;
use serde::Serialize;
use serde_json::{json, Value};
use std::any::Any;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tracing::{info, warn};

/// Global model cache - models loaded on demand
static MODEL_CACHE: OnceCell<Arc<Mutex<HashMap<String, TextEmbedding>>>> = OnceCell::new();

fn get_model_cache() -> &'static Arc<Mutex<HashMap<String, TextEmbedding>>> {
    MODEL_CACHE.get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
}

/// Global embedding result cache - avoids recomputing same text embeddings
/// Key: (model_name, text_hash) -> embedding vector
/// TTL: Entries older than 5 minutes are evicted on access
static EMBEDDING_CACHE: OnceCell<Arc<Mutex<EmbeddingResultCache>>> = OnceCell::new();

struct CachedEmbedding {
    embedding: Vec<f32>,
    created_at: Instant,
}

struct EmbeddingResultCache {
    entries: HashMap<(String, u64), CachedEmbedding>,
    ttl: std::time::Duration,
    max_entries: usize,
    hits: u64,
    misses: u64,
}

impl EmbeddingResultCache {
    fn new() -> Self {
        Self {
            entries: HashMap::new(),
            ttl: std::time::Duration::from_secs(300), // 5 minutes
            max_entries: 10_000,
            hits: 0,
            misses: 0,
        }
    }

    fn get(&mut self, model: &str, text_hash: u64) -> Option<Vec<f32>> {
        let key = (model.to_string(), text_hash);
        if let Some(entry) = self.entries.get(&key) {
            if entry.created_at.elapsed() < self.ttl {
                self.hits += 1;
                return Some(entry.embedding.clone());
            }
            // Expired - remove it
            self.entries.remove(&key);
        }
        self.misses += 1;
        None
    }

    fn insert(&mut self, model: &str, text_hash: u64, embedding: Vec<f32>) {
        // Evict oldest if at capacity
        if self.entries.len() >= self.max_entries {
            // Find oldest entry
            if let Some(oldest_key) = self.entries
                .iter()
                .min_by_key(|(_, v)| v.created_at)
                .map(|(k, _)| k.clone())
            {
                self.entries.remove(&oldest_key);
            }
        }

        self.entries.insert(
            (model.to_string(), text_hash),
            CachedEmbedding {
                embedding,
                created_at: Instant::now(),
            },
        );
    }

    fn stats(&self) -> (u64, u64, usize) {
        (self.hits, self.misses, self.entries.len())
    }
}

fn get_embedding_cache() -> &'static Arc<Mutex<EmbeddingResultCache>> {
    EMBEDDING_CACHE.get_or_init(|| Arc::new(Mutex::new(EmbeddingResultCache::new())))
}

/// Fast hash for text (djb2 algorithm)
fn hash_text(text: &str) -> u64 {
    let mut hash: u64 = 5381;
    for byte in text.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(byte as u64);
    }
    hash
}

/// Get cache directory for fastembed models
fn get_cache_dir() -> PathBuf {
    if let Ok(path) = std::env::var("FASTEMBED_CACHE_PATH") {
        PathBuf::from(path)
    } else {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        PathBuf::from(home).join(".continuum/models/fastembed")
    }
}

/// Map string model name to fastembed EmbeddingModel enum
fn parse_model_name(name: &str) -> Result<EmbeddingModel, String> {
    match name.to_lowercase().as_str() {
        "allminilml6v2" | "all-minilm-l6-v2" | "default" => Ok(EmbeddingModel::AllMiniLML6V2),
        "allminilml6v2q" | "all-minilm-l6-v2-q" => Ok(EmbeddingModel::AllMiniLML6V2Q),
        "bgesmallenv15" | "bge-small-en-v1.5" => Ok(EmbeddingModel::BGESmallENV15),
        "bgebaseenv15" | "bge-base-en-v1.5" => Ok(EmbeddingModel::BGEBaseENV15),
        "bgelargeenv15" | "bge-large-en-v1.5" => Ok(EmbeddingModel::BGELargeENV15),
        "nomicembedtextv1" | "nomic-embed-text-v1" => Ok(EmbeddingModel::NomicEmbedTextV1),
        "nomicembedtextv15" | "nomic-embed-text-v1.5" => Ok(EmbeddingModel::NomicEmbedTextV15),
        _ => Err(format!(
            "Unknown model: {name}. Use 'embedding/model/list' to see available models."
        )),
    }
}

/// Get or load a model by name
fn get_or_load_model(model_name: &str) -> Result<(), String> {
    let cache = get_model_cache();
    let mut models = cache.lock().map_err(|e| format!("Lock error: {e}"))?;

    if !models.contains_key(model_name) {
        info!("Loading embedding model: {model_name}");
        let start = Instant::now();

        let model_enum = parse_model_name(model_name)?;
        let cache_dir = get_cache_dir();

        // Ensure cache directory exists
        std::fs::create_dir_all(&cache_dir)
            .map_err(|e| format!("Failed to create cache dir: {e}"))?;

        let model = TextEmbedding::try_new(
            InitOptions::new(model_enum)
                .with_cache_dir(cache_dir)
                .with_show_download_progress(true),
        )
        .map_err(|e| format!("Failed to load model: {e}"))?;

        let elapsed = start.elapsed();
        info!("Model loaded in {:.2}s: {}", elapsed.as_secs_f64(), model_name);

        models.insert(model_name.to_string(), model);
    }

    Ok(())
}

/// Public function for cross-module embedding generation
/// Used by DataModule for backfillVectors
pub fn generate_embedding(text: &str, model_name: &str) -> Result<Vec<f32>, String> {
    // Load model if needed
    get_or_load_model(model_name)?;

    // Get model from cache
    let cache = get_model_cache();
    let models = cache.lock().map_err(|e| format!("Lock error: {e}"))?;
    let embedding_model = models
        .get(model_name)
        .ok_or_else(|| format!("Model not loaded: {model_name}"))?;

    // Generate embedding for single text
    let embeddings = embedding_model
        .embed(vec![text], None)
        .map_err(|e| format!("Embedding generation failed: {e}"))?;

    embeddings
        .into_iter()
        .next()
        .ok_or_else(|| "No embedding returned".to_string())
}

/// Batch embedding generation for efficiency
pub fn generate_embeddings_batch(texts: &[&str], model_name: &str) -> Result<Vec<Vec<f32>>, String> {
    if texts.is_empty() {
        return Ok(vec![]);
    }

    // Load model if needed
    get_or_load_model(model_name)?;

    // Get model from cache
    let cache = get_model_cache();
    let models = cache.lock().map_err(|e| format!("Lock error: {e}"))?;
    let embedding_model = models
        .get(model_name)
        .ok_or_else(|| format!("Model not loaded: {model_name}"))?;

    // Generate embeddings
    embedding_model
        .embed(texts.to_vec(), None)
        .map_err(|e| format!("Embedding generation failed: {e}"))
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
    if denom == 0.0 { 0.0 } else { dot / denom }
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
        .flat_map(|i| (i+1..n).map(move |j| (i, j)))
        .collect();

    // Compute similarities in parallel with Rayon
    pairs.par_iter()
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
    targets.par_iter()
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
    let similarities: Vec<(usize, f32)> = targets.par_iter()
        .enumerate()
        .map(|(i, target)| (i, cosine_similarity(query, target)))
        .filter(|(_, sim)| *sim >= threshold)
        .collect();

    // Sort by similarity descending and take top k
    let mut sorted = similarities;
    sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
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
            for neighbor in 0..n {
                if !visited[neighbor] && get_sim(node, neighbor) >= min_similarity {
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
            let strength = if count > 0 { total_sim / count as f32 } else { 1.0 };

            // Find representative (highest avg similarity to others in cluster)
            let representative = component.iter()
                .map(|&item| {
                    let avg: f32 = component.iter()
                        .filter(|&&other| other != item)
                        .map(|&other| get_sim(item, other))
                        .sum::<f32>() / (component.len() - 1).max(1) as f32;
                    (item, avg)
                })
                .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
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
    clusters.sort_by(|a, b| b.strength.partial_cmp(&a.strength).unwrap());
    clusters
}

#[derive(Serialize)]
struct ModelInfo {
    name: String,
    dimensions: usize,
    description: String,
    size_mb: usize,
    loaded: bool,
}

fn get_model_info_list() -> Vec<ModelInfo> {
    let cache = get_model_cache();
    let loaded_models: Vec<String> = cache
        .lock()
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default();

    vec![
        ModelInfo {
            name: "AllMiniLML6V2".to_string(),
            dimensions: 384,
            description: "Fast, good quality, default".to_string(),
            size_mb: 90,
            loaded: loaded_models.contains(&"AllMiniLML6V2".to_string()),
        },
        ModelInfo {
            name: "AllMiniLML6V2Q".to_string(),
            dimensions: 384,
            description: "Quantized, fastest, smallest".to_string(),
            size_mb: 25,
            loaded: loaded_models.contains(&"AllMiniLML6V2Q".to_string()),
        },
        ModelInfo {
            name: "BGESmallENV15".to_string(),
            dimensions: 384,
            description: "Better quality than MiniLM".to_string(),
            size_mb: 130,
            loaded: loaded_models.contains(&"BGESmallENV15".to_string()),
        },
        ModelInfo {
            name: "BGEBaseENV15".to_string(),
            dimensions: 768,
            description: "High quality, larger embeddings".to_string(),
            size_mb: 440,
            loaded: loaded_models.contains(&"BGEBaseENV15".to_string()),
        },
        ModelInfo {
            name: "NomicEmbedTextV15".to_string(),
            dimensions: 768,
            description: "Nomic model, same as Ollama nomic-embed-text".to_string(),
            size_mb: 550,
            loaded: loaded_models.contains(&"NomicEmbedTextV15".to_string()),
        },
    ]
}

pub struct EmbeddingModule;

impl EmbeddingModule {
    pub fn new() -> Self {
        Self
    }

    /// Pre-load the default model on startup
    pub fn preload_default_model() {
        info!("Pre-loading default embedding model (AllMiniLML6V2)...");
        match get_or_load_model("AllMiniLML6V2") {
            Ok(()) => info!("Default embedding model ready"),
            Err(e) => warn!("Failed to pre-load default model: {e}"),
        }
    }

    fn handle_generate(&self, params: &Value) -> Result<CommandResult, String> {
        let texts: Vec<String> = params.get("texts")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .ok_or("Missing or invalid 'texts' array")?;

        let model_name = params.get("model")
            .and_then(|v| v.as_str())
            .unwrap_or("AllMiniLML6V2");

        if texts.is_empty() {
            return Err("No texts provided".to_string());
        }

        let start = Instant::now();
        let batch_size = texts.len();

        // Check embedding cache for each text
        let embed_cache = get_embedding_cache();
        let mut embeddings: Vec<Vec<f32>> = Vec::with_capacity(batch_size);
        let mut texts_to_generate: Vec<(usize, String)> = Vec::new(); // (index, text)

        {
            let mut cache = embed_cache.lock().map_err(|e| format!("Cache lock error: {e}"))?;
            for (i, text) in texts.iter().enumerate() {
                let text_hash = hash_text(text);
                if let Some(cached) = cache.get(model_name, text_hash) {
                    embeddings.push(cached);
                } else {
                    embeddings.push(vec![]); // Placeholder
                    texts_to_generate.push((i, text.clone()));
                }
            }
        }

        let cache_hits = batch_size - texts_to_generate.len();

        // Generate embeddings only for texts not in cache
        if !texts_to_generate.is_empty() {
            // Load model if needed
            get_or_load_model(model_name)?;

            // Get model from cache
            let model_cache = get_model_cache();
            let models = model_cache.lock().map_err(|e| format!("Lock error: {e}"))?;
            let embedding_model = models
                .get(model_name)
                .ok_or_else(|| format!("Model not loaded: {model_name}"))?;

            // Generate embeddings for uncached texts
            let text_refs: Vec<&str> = texts_to_generate.iter().map(|(_, t)| t.as_str()).collect();
            let new_embeddings = embedding_model
                .embed(text_refs, None)
                .map_err(|e| format!("Embedding generation failed: {e}"))?;

            // Store in cache and update result vector
            let mut cache = embed_cache.lock().map_err(|e| format!("Cache lock error: {e}"))?;
            for ((idx, text), emb) in texts_to_generate.iter().zip(new_embeddings.into_iter()) {
                let text_hash = hash_text(text);
                cache.insert(model_name, text_hash, emb.clone());
                embeddings[*idx] = emb;
            }
        }

        let duration_ms = start.elapsed().as_millis() as u64;
        let dimensions = embeddings.first().map(|e| e.len()).unwrap_or(0);

        info!(
            "Generated {} embeddings ({}d) in {}ms (cache: {}/{} hits)",
            batch_size, dimensions, duration_ms, cache_hits, batch_size
        );

        // Convert to binary: flatten f32 vectors to bytes
        let total_floats = batch_size * dimensions;
        let mut flat: Vec<f32> = Vec::with_capacity(total_floats);
        for emb in &embeddings {
            flat.extend_from_slice(emb);
        }

        // Reinterpret as bytes - zero copy
        let bytes: Vec<u8> = flat.iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();

        Ok(CommandResult::Binary {
            metadata: json!({
                "type": "binary",
                "length": bytes.len(),
                "dtype": "f32",
                "shape": [dimensions],
                "batchSize": batch_size,
                "durationMs": duration_ms,
                "model": model_name
            }),
            data: bytes,
        })
    }

    fn handle_model_load(&self, params: &Value) -> Result<CommandResult, String> {
        let model = params.get("model")
            .and_then(|v| v.as_str())
            .ok_or("Missing 'model' parameter")?;

        let start = Instant::now();
        get_or_load_model(model)?;
        let duration_ms = start.elapsed().as_millis() as u64;

        Ok(CommandResult::Json(json!({
            "model": model,
            "loaded": true,
            "durationMs": duration_ms
        })))
    }

    fn handle_model_list(&self) -> Result<CommandResult, String> {
        let models = get_model_info_list();
        Ok(CommandResult::Json(json!({
            "models": models,
            "count": models.len(),
            "cacheDir": get_cache_dir().to_string_lossy()
        })))
    }

    fn handle_model_info(&self, params: &Value) -> Result<CommandResult, String> {
        let model = params.get("model")
            .and_then(|v| v.as_str())
            .ok_or("Missing 'model' parameter")?;

        let models = get_model_info_list();
        match models.into_iter().find(|m| m.name == model) {
            Some(info) => Ok(CommandResult::Json(
                serde_json::to_value(info).unwrap_or(json!({}))
            )),
            None => Err(format!("Unknown model: {model}")),
        }
    }

    fn handle_model_unload(&self, params: &Value) -> Result<CommandResult, String> {
        let model = params.get("model")
            .and_then(|v| v.as_str())
            .ok_or("Missing 'model' parameter")?;

        let cache = get_model_cache();
        let mut models = cache.lock().map_err(|e| format!("Lock error: {e}"))?;

        if models.remove(model).is_some() {
            info!("Unloaded embedding model: {model}");
            Ok(CommandResult::Json(json!({
                "model": model,
                "unloaded": true
            })))
        } else {
            Err(format!("Model not loaded: {model}"))
        }
    }

    /// Handle embedding/similarity - compute cosine similarity between two embeddings
    fn handle_similarity(&self, params: &Value) -> Result<CommandResult, String> {
        let a: Vec<f32> = params.get("a")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .ok_or("Missing or invalid 'a' vector")?;

        let b: Vec<f32> = params.get("b")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .ok_or("Missing or invalid 'b' vector")?;

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
        let embeddings: Vec<Vec<f32>> = params.get("embeddings")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .ok_or("Missing or invalid 'embeddings' array")?;

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
                    i, dim, emb.len()
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
        let bytes: Vec<u8> = similarities.iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();

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
        let query: Vec<f32> = params.get("query")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .ok_or("Missing or invalid 'query' vector")?;

        let targets: Vec<Vec<f32>> = params.get("targets")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .ok_or("Missing or invalid 'targets' array")?;

        let k = params.get("k")
            .and_then(|v| v.as_u64())
            .unwrap_or(10) as usize;

        let threshold = params.get("threshold")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as f32;

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
                    i, dim, target.len()
                ));
            }
        }

        let start = Instant::now();
        let results = top_k_similar(&query, &targets, k, threshold);
        let duration_ms = start.elapsed().as_millis() as u64;

        info!(
            "Found {} top-k matches from {} targets ({}d) in {}ms",
            results.len(), targets.len(), dim, duration_ms
        );

        // Return as array of {index, similarity} objects
        let result_objects: Vec<Value> = results.iter()
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

    /// Handle embedding/cache/stats - get cache hit/miss statistics
    fn handle_cache_stats(&self) -> Result<CommandResult, String> {
        let embed_cache = get_embedding_cache();
        let cache = embed_cache.lock().map_err(|e| format!("Cache lock error: {e}"))?;
        let (hits, misses, size) = cache.stats();
        let hit_rate = if hits + misses > 0 {
            (hits as f64) / ((hits + misses) as f64) * 100.0
        } else {
            0.0
        };

        Ok(CommandResult::Json(json!({
            "hits": hits,
            "misses": misses,
            "size": size,
            "maxSize": 10_000,
            "hitRatePercent": format!("{:.1}", hit_rate),
            "ttlSeconds": 300
        })))
    }

    /// Handle embedding/cache/clear - clear the embedding cache
    fn handle_cache_clear(&self) -> Result<CommandResult, String> {
        let embed_cache = get_embedding_cache();
        let mut cache = embed_cache.lock().map_err(|e| format!("Cache lock error: {e}"))?;
        let cleared = cache.entries.len();
        cache.entries.clear();
        cache.hits = 0;
        cache.misses = 0;

        info!("Cleared {} cached embeddings", cleared);

        Ok(CommandResult::Json(json!({
            "cleared": cleared,
            "success": true
        })))
    }

    /// Handle embedding/cluster - detect clusters via connected components
    ///
    /// Takes embeddings and clustering parameters, returns cluster assignments.
    /// Full clustering algorithm in Rust (similarity matrix + connected components).
    fn handle_cluster(&self, params: &Value) -> Result<CommandResult, String> {
        let embeddings: Vec<Vec<f32>> = params.get("embeddings")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .ok_or("Missing or invalid 'embeddings' array")?;

        let min_similarity = params.get("minSimilarity")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.7) as f32;

        let min_cluster_size = params.get("minClusterSize")
            .and_then(|v| v.as_u64())
            .unwrap_or(2) as usize;

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
                    i, dim, emb.len()
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
            max_concurrency: 0,
        }
    }

    async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
        // Pre-load default model in background
        tokio::task::spawn_blocking(|| {
            Self::preload_default_model();
        });
        Ok(())
    }

    async fn handle_command(
        &self,
        command: &str,
        params: Value,
    ) -> Result<CommandResult, String> {
        match command {
            "embedding/generate" => self.handle_generate(&params),
            "embedding/similarity" => self.handle_similarity(&params),
            "embedding/similarity-matrix" => self.handle_similarity_matrix(&params),
            "embedding/top-k" => self.handle_top_k(&params),
            "embedding/cluster" => self.handle_cluster(&params),
            "embedding/cache/stats" => self.handle_cache_stats(),
            "embedding/cache/clear" => self.handle_cache_clear(),
            "embedding/model/load" => self.handle_model_load(&params),
            "embedding/model/list" => self.handle_model_list(),
            "embedding/model/info" => self.handle_model_info(&params),
            "embedding/model/unload" => self.handle_model_unload(&params),
            _ => Err(format!("Unknown embedding command: {command}")),
        }
    }

    fn as_any(&self) -> &dyn Any { self }
}
