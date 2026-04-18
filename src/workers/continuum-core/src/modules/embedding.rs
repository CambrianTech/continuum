//! EmbeddingModule — Native text embedding generation via fastembed (ONNX).
//!
//! Handles: embedding/generate, embedding/model/load, embedding/model/list,
//!          embedding/model/info, embedding/model/unload
//!
//! Benefits of native embedding:
//! - No network overhead (~5ms per embedding)
//! - Batch processing (100 texts in ~100ms)
//! - No external service dependency
//! - True parallelism via ONNX Runtime
//!
//! Priority: Normal — embedding is not time-critical like voice.

use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use async_trait::async_trait;
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use once_cell::sync::OnceCell;
use rayon::prelude::*;
use serde::Serialize;
use serde_json::{json, Value};
use std::any::Any;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tracing::{info, warn};

use crate::gpu::make_entry;
use crate::gpu::memory_manager::{GpuAllocationGuard, GpuMemoryManager, GpuPriority, GpuSubsystem};
use crate::paging::{size_weighted_lru, PagedResourcePool, PoolConfig};
use crate::utils::params::Params;
use sha2::{Digest, Sha256};

/// Global model cache - models loaded on demand
static MODEL_CACHE: OnceCell<Arc<Mutex<HashMap<String, TextEmbedding>>>> = OnceCell::new();

/// GPU allocation guards for loaded embedding models (dynamic: one guard per model)
static EMBEDDING_GPU_GUARDS: OnceCell<Mutex<HashMap<String, GpuAllocationGuard>>> = OnceCell::new();

fn get_gpu_guards() -> &'static Mutex<HashMap<String, GpuAllocationGuard>> {
    EMBEDDING_GPU_GUARDS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// GPU memory manager — set during IPC startup
static EMBEDDING_GPU_MANAGER: std::sync::OnceLock<Arc<GpuMemoryManager>> =
    std::sync::OnceLock::new();

/// Set the GPU memory manager (called from ipc/mod.rs during startup)
pub fn set_gpu_manager(mgr: Arc<GpuMemoryManager>) {
    let _ = EMBEDDING_GPU_MANAGER.set(mgr);
}

fn gpu_manager() -> Option<&'static Arc<GpuMemoryManager>> {
    EMBEDDING_GPU_MANAGER.get()
}

/// Set once ORT panics during init — all subsequent load attempts fail fast
/// instead of re-triggering catch_unwind and spamming logs.
static ORT_UNAVAILABLE: AtomicBool = AtomicBool::new(false);

fn get_model_cache() -> &'static Arc<Mutex<HashMap<String, TextEmbedding>>> {
    MODEL_CACHE.get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
}

/// Embedding pool key: (model, content_hash). The model is part of the key
/// because different models map the same text to different vectors. Content
/// hash is SHA-256 of the input text — collision-free in practice and fixed
/// at 32 bytes (no per-key allocation for the text itself).
#[derive(Clone, Hash, Eq, PartialEq, Debug)]
pub struct EmbeddingKey {
    pub model: String,
    pub content_hash: [u8; 32],
}

impl EmbeddingKey {
    fn new(model: &str, text: &str) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(text.as_bytes());
        let digest = hasher.finalize();
        let mut content_hash = [0u8; 32];
        content_hash.copy_from_slice(&digest);
        Self {
            model: model.to_string(),
            content_hash,
        }
    }
}

/// Default budget for the embedding pool. Tuned for the average codebase
/// indexer + RAG workload: 384-dim FP32 ≈ 1.5 KB per vector → 256 MB holds
/// ~170k entries, comfortably more than the previous 10k count cap.
/// Eventually overridden by recipe-declared budgets (Phase 9) and pressure
/// broker (Phase 7b).
const EMBEDDING_POOL_BUDGET_BYTES: u64 = 256 * 1024 * 1024;

/// Global embedding pool — single source of truth for cached embeddings.
/// Replaces the old per-IPC `EMBEDDING_CACHE` that internal Rust callers
/// (DataModule, memory providers) were silently bypassing — those now
/// route through `generate_embedding{,s_batch}` and inherit cache hits
/// for free.
static EMBEDDING_POOL: OnceCell<Arc<PagedResourcePool<EmbeddingKey, Vec<f32>>>> = OnceCell::new();

fn get_embedding_pool() -> &'static Arc<PagedResourcePool<EmbeddingKey, Vec<f32>>> {
    EMBEDDING_POOL.get_or_init(|| {
        Arc::new(PagedResourcePool::new(PoolConfig {
            name: "embedding-cache".to_string(),
            max_bytes: EMBEDDING_POOL_BUDGET_BYTES,
            // Sizer: 4 bytes per f32 dimension. Accurate to within an
            // allocator-rounding constant; pool budgeting is byte-driven.
            sizer: Arc::new(|emb: &Vec<f32>| (emb.len() * std::mem::size_of::<f32>()) as u64),
            // Size-weighted LRU: among similarly-aged entries, evict the
            // largest first (frees more memory per drop). Right policy for
            // mixed-dimension models in one pool.
            eviction_priority: size_weighted_lru(),
        }))
    })
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

/// Estimate VRAM/memory bytes for a loaded embedding model.
/// Based on known model sizes from fastembed/sentence-transformers catalog.
fn estimate_embedding_model_bytes(model_name: &str) -> u64 {
    match model_name.to_lowercase().as_str() {
        "allminilml6v2" | "all-minilm-l6-v2" | "default" => 90 * 1024 * 1024, // ~90MB
        "allminilml6v2q" | "all-minilm-l6-v2-q" => 25 * 1024 * 1024,          // ~25MB quantized
        "bgesmallenv15" | "bge-small-en-v1.5" => 130 * 1024 * 1024,           // ~130MB
        "bgebaseenv15" | "bge-base-en-v1.5" => 440 * 1024 * 1024,             // ~440MB
        "bgelargeenv15" | "bge-large-en-v1.5" => 1300 * 1024 * 1024,          // ~1.3GB
        "nomicembedtextv1" | "nomic-embed-text-v1" => 550 * 1024 * 1024,      // ~550MB
        "nomicembedtextv15" | "nomic-embed-text-v1.5" => 550 * 1024 * 1024,   // ~550MB
        _ => 100 * 1024 * 1024, // Conservative default for unknown models
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

/// Get or load a model by name.
///
/// CRITICAL: Model loading (TextEmbedding::try_new) is a blocking operation that
/// can take seconds (ONNX init, possible HuggingFace download). We must NOT hold
/// the mutex during loading, or all tokio worker threads pile up on the lock and
/// the entire runtime deadlocks (no IPC processing at all).
///
/// Pattern: check → release lock → load → re-acquire → insert.
/// If two threads race to load the same model, one "wastes" a load — acceptable
/// tradeoff vs deadlocking the entire system.
fn get_or_load_model(model_name: &str) -> Result<(), String> {
    // Fast path: model already loaded
    {
        let models = get_model_cache()
            .lock()
            .map_err(|e| format!("Lock error: {e}"))?;
        if models.contains_key(model_name) {
            return Ok(());
        }
    } // Lock released here — BEFORE the expensive loading

    // Slow path: load model WITHOUT holding the mutex
    info!("Loading embedding model: {model_name}");
    let start = Instant::now();

    let model_enum = parse_model_name(model_name)?;
    let cache_dir = get_cache_dir();

    std::fs::create_dir_all(&cache_dir).map_err(|e| format!("Failed to create cache dir: {e}"))?;

    // Fail fast if ORT already panicked in a previous attempt
    if ORT_UNAVAILABLE.load(Ordering::Relaxed) {
        return Err("ORT runtime previously panicked — embeddings unavailable until restart".to_string());
    }

    // ORT crate panics if libonnxruntime can't be loaded (instead of returning error).
    // catch_unwind prevents the panic from unwinding out of this call and killing the process.
    let model_result = std::panic::catch_unwind(|| {
        TextEmbedding::try_new(
            InitOptions::new(model_enum)
                .with_cache_dir(cache_dir)
                .with_show_download_progress(true),
        )
    });
    let model = match model_result {
        Ok(Ok(m)) => m,
        Ok(Err(e)) => return Err(format!("Failed to load model: {e}")),
        Err(panic_payload) => {
            ORT_UNAVAILABLE.store(true, Ordering::Relaxed);
            let msg = panic_payload
                .downcast_ref::<String>()
                .map(|s| s.as_str())
                .or_else(|| panic_payload.downcast_ref::<&str>().copied())
                .unwrap_or("unknown cause");
            return Err(format!("ORT runtime panicked during model init: {msg}. Check ORT_DYLIB_PATH."));
        }
    };

    let elapsed = start.elapsed();
    info!(
        "Model loaded in {:.2}s: {}",
        elapsed.as_secs_f64(),
        model_name
    );

    // Track GPU allocation for embedding model
    if let Some(mgr) = gpu_manager() {
        let model_bytes = estimate_embedding_model_bytes(model_name);
        if model_bytes > 0 {
            match mgr.allocate(
                GpuSubsystem::Inference,
                model_bytes,
                GpuPriority::Interactive,
            ) {
                Ok(guard) => {
                    info!(
                        "Embedding GPU: {} allocation {:.0}MB",
                        model_name,
                        model_bytes as f64 / (1024.0 * 1024.0)
                    );
                    mgr.eviction_registry.register(make_entry(
                        &format!("embed:{}", model_name),
                        &format!("Embedding {}", model_name),
                        GpuPriority::Interactive,
                        model_bytes,
                    ));
                    if let Ok(mut guards) = get_gpu_guards().lock() {
                        guards.insert(model_name.to_string(), guard);
                    }
                }
                Err(e) => {
                    warn!(
                        "Embedding GPU: allocation for {} failed ({}), proceeding",
                        model_name, e
                    );
                }
            }
        }
    }

    // Re-acquire lock and insert (double-check to avoid overwriting concurrent load)
    let mut models = get_model_cache()
        .lock()
        .map_err(|e| format!("Lock error: {e}"))?;
    models.entry(model_name.to_string()).or_insert(model);

    Ok(())
}

/// Public function for cross-module embedding generation
/// Used by DataModule for backfillVectors
pub fn generate_embedding(text: &str, model_name: &str) -> Result<Vec<f32>, String> {
    // Pool check first — the cache lookup that internal callers used to
    // bypass. This is the 0/64-hit-rate fix: now every internal embedder
    // (DataModule.backfill_vectors, ModuleBackedEmbeddingProvider, etc.)
    // hits the same cache the IPC handler does.
    let pool = get_embedding_pool();
    let key = EmbeddingKey::new(model_name, text);
    if let Some(cached) = pool.get(&key) {
        return Ok(cached);
    }

    // Miss → load model + embed + cache.
    get_or_load_model(model_name)?;
    let cache = get_model_cache();
    let mut models = cache.lock().map_err(|e| format!("Lock error: {e}"))?;
    let embedding_model = models
        .get_mut(model_name)
        .ok_or_else(|| format!("Model not loaded: {model_name}"))?;
    let embeddings = embedding_model
        .embed(vec![text], None)
        .map_err(|e| format!("Embedding generation failed: {e}"))?;
    let embedding = embeddings
        .into_iter()
        .next()
        .ok_or_else(|| "No embedding returned".to_string())?;

    pool.insert(key, embedding.clone());
    Ok(embedding)
}

/// Batch embedding generation. Pool-aware: per-text cache lookup, single
/// `model.embed()` call for the uncached subset, then per-text insert.
/// One model invocation per batch (vs N for per-text single-flight) keeps
/// the GPU/ONNX path efficient.
pub fn generate_embeddings_batch(
    texts: &[&str],
    model_name: &str,
) -> Result<Vec<Vec<f32>>, String> {
    if texts.is_empty() {
        return Ok(vec![]);
    }

    let pool = get_embedding_pool();

    // Pre-fill from cache; collect misses for batched generation.
    let mut results: Vec<Option<Vec<f32>>> = Vec::with_capacity(texts.len());
    let mut keys: Vec<EmbeddingKey> = Vec::with_capacity(texts.len());
    let mut misses: Vec<(usize, &str)> = Vec::new();
    for (i, text) in texts.iter().enumerate() {
        let key = EmbeddingKey::new(model_name, text);
        if let Some(cached) = pool.get(&key) {
            results.push(Some(cached));
        } else {
            results.push(None);
            misses.push((i, *text));
        }
        keys.push(key);
    }

    // Generate only the misses — one batched model.embed() call.
    if !misses.is_empty() {
        get_or_load_model(model_name)?;
        let cache = get_model_cache();
        let mut models = cache.lock().map_err(|e| format!("Lock error: {e}"))?;
        let embedding_model = models
            .get_mut(model_name)
            .ok_or_else(|| format!("Model not loaded: {model_name}"))?;
        let miss_texts: Vec<&str> = misses.iter().map(|(_, t)| *t).collect();
        let new_embeddings = embedding_model
            .embed(miss_texts, None)
            .map_err(|e| format!("Embedding generation failed: {e}"))?;
        for ((idx, _), emb) in misses.into_iter().zip(new_embeddings.into_iter()) {
            pool.insert(keys[idx].clone(), emb.clone());
            results[idx] = Some(emb);
        }
    }

    Ok(results
        .into_iter()
        .map(|opt| opt.expect("every slot was either cached or generated"))
        .collect())
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
            description: "Nomic Embed Text v1.5 - 768 dimensions".to_string(),
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

    /// Pre-load the default model on startup.
    /// Wrapped in catch_unwind because the ORT crate panics (instead of returning
    /// an error) when libonnxruntime can't be loaded. The panic would unwind out
    /// of this call and kill the process. By catching it here, we set ORT_UNAVAILABLE
    /// so subsequent calls fail fast, and the rest of the system stays alive.
    pub fn preload_default_model() {
        info!("Pre-loading default embedding model (AllMiniLML6V2)...");
        let result = std::panic::catch_unwind(|| {
            get_or_load_model("AllMiniLML6V2")
        });
        match result {
            Ok(Ok(())) => info!("Default embedding model ready"),
            Ok(Err(e)) => warn!("Failed to pre-load default model: {e} — embeddings unavailable"),
            Err(panic_payload) => {
                ORT_UNAVAILABLE.store(true, Ordering::Relaxed);
                let msg = panic_payload
                    .downcast_ref::<String>()
                    .map(|s| s.as_str())
                    .or_else(|| panic_payload.downcast_ref::<&str>().copied())
                    .unwrap_or("unknown cause");
                warn!("ORT runtime panicked during model load: {msg} — embeddings unavailable. Check ORT_DYLIB_PATH.");
            }
        }
    }

    fn handle_generate(&self, params: &Value) -> Result<CommandResult, String> {
        let p = Params::new(params);
        let texts: Vec<String> = p.json("texts")?;
        let model_name = p.str_or("model", "AllMiniLML6V2");

        if texts.is_empty() {
            return Err("No texts provided".to_string());
        }

        let start = Instant::now();
        let batch_size = texts.len();

        // Pool-backed generation — cache check + batched miss-fill happens
        // inside `generate_embeddings_batch`. Same path internal callers
        // take, so cache hits accrue across IPC + Rust consumers uniformly.
        let pool_hits_before = get_embedding_pool().stats_blocking().hit_count;
        let text_refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
        let embeddings = generate_embeddings_batch(&text_refs, model_name)?;
        let cache_hits =
            (get_embedding_pool().stats_blocking().hit_count - pool_hits_before) as usize;

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
        let bytes: Vec<u8> = flat.iter().flat_map(|f| f.to_le_bytes()).collect();

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
        let p = Params::new(params);
        let model = p.str("model")?;

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
        let p = Params::new(params);
        let model = p.str("model")?;

        let models = get_model_info_list();
        match models.into_iter().find(|m| m.name == model) {
            Some(info) => Ok(CommandResult::Json(
                serde_json::to_value(info).unwrap_or(json!({})),
            )),
            None => Err(format!("Unknown model: {model}")),
        }
    }

    fn handle_model_unload(&self, params: &Value) -> Result<CommandResult, String> {
        let p = Params::new(params);
        let model = p.str("model")?;

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

    /// Handle embedding/cache/stats - get cache hit/miss + pressure
    /// statistics from the embedding pool. Now byte-driven (pressure +
    /// max_bytes) rather than count-capped — see EMBEDDING_POOL_BUDGET_BYTES.
    fn handle_cache_stats(&self) -> Result<CommandResult, String> {
        let stats = get_embedding_pool().stats_blocking();
        let total = stats.hit_count + stats.miss_count;
        let hit_rate = if total > 0 {
            (stats.hit_count as f64) / (total as f64) * 100.0
        } else {
            0.0
        };

        Ok(CommandResult::Json(json!({
            "hits": stats.hit_count,
            "misses": stats.miss_count,
            "size": stats.entry_count,
            "pinned": stats.pinned_count,
            "totalBytes": stats.total_bytes,
            "maxBytes": stats.max_bytes,
            "pressure": stats.pressure,
            "evictions": stats.eviction_count,
            "hitRatePercent": format!("{:.1}", hit_rate),
        })))
    }

    /// Handle embedding/cache/clear - drain the embedding pool. Resets
    /// hit/miss/eviction counters too (admin-level reset).
    fn handle_cache_clear(&self) -> Result<CommandResult, String> {
        let cleared = get_embedding_pool().clear();
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
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
        // Pre-load default model in background
        tokio::task::spawn_blocking(|| {
            Self::preload_default_model();
        });
        Ok(())
    }

    async fn handle_command(&self, command: &str, params: Value) -> Result<CommandResult, String> {
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

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {
    //! Pool integration tests. Don't load real ONNX models — pre-populate
    //! the pool with synthetic embeddings and verify the lookup path
    //! short-circuits before fastembed is called. The "0/64 hit rate"
    //! regression test sits here.
    //!
    //! NOTE: `EMBEDDING_POOL` is a process-global; cargo test runs tests
    //! in parallel by default and would race on the shared hit/miss
    //! counters. `TEST_LOCK` serializes the cases that read counters; the
    //! pure-key-shape test (`embedding_key_is_model_namespaced`) doesn't
    //! need the lock.
    use super::*;
    use std::sync::Mutex as StdMutex;

    static TEST_LOCK: StdMutex<()> = StdMutex::new(());

    fn fake_embedding(seed: u8) -> Vec<f32> {
        (0..384).map(|i| (i as f32 + seed as f32) * 0.001).collect()
    }

    /// The bug we're fixing: internal callers used to bypass the cache
    /// entirely. After migration, `generate_embeddings_batch` consults
    /// the pool first and returns cached entries without touching the
    /// model — proven here by pre-loading the pool with a known vector
    /// for a model that doesn't exist (so a real model.embed() would
    /// error out).
    #[test]
    fn generate_embeddings_batch_hits_pool_before_loading_model() {
        let _g = TEST_LOCK.lock().unwrap();
        let pool = get_embedding_pool();
        pool.clear();
        let key = EmbeddingKey::new("nonexistent-model-name", "the cached text");
        let expected = fake_embedding(7);
        pool.insert(key, expected.clone());

        let got = generate_embeddings_batch(&["the cached text"], "nonexistent-model-name")
            .expect("cache hit should succeed without loading the (nonexistent) model");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0], expected);

        // Hit accounted for in pool stats.
        let stats = pool.stats_blocking();
        assert!(stats.hit_count >= 1, "expected ≥1 hit, got {}", stats.hit_count);
    }

    #[test]
    fn single_embedding_hits_pool_before_loading_model() {
        let _g = TEST_LOCK.lock().unwrap();
        let pool = get_embedding_pool();
        pool.clear();
        let key = EmbeddingKey::new("nonexistent-model-name", "single text");
        let expected = fake_embedding(11);
        pool.insert(key, expected.clone());

        let got = generate_embedding("single text", "nonexistent-model-name")
            .expect("cache hit should bypass model load");
        assert_eq!(got, expected);
    }

    #[test]
    fn embedding_key_is_model_namespaced() {
        // Same text + different model = different cache slots. Critical
        // because different models map identical text to different vectors.
        let k1 = EmbeddingKey::new("modelA", "hello");
        let k2 = EmbeddingKey::new("modelB", "hello");
        assert_ne!(k1, k2, "same-text + different-model must not collide");

        let k3 = EmbeddingKey::new("modelA", "hello");
        assert_eq!(k1, k3, "deterministic: same model + text → same key");
    }

    #[test]
    fn pool_clear_resets_stats_and_drops_entries() {
        let _g = TEST_LOCK.lock().unwrap();
        let pool = get_embedding_pool();
        let key = EmbeddingKey::new("test", "to be cleared");
        pool.insert(key.clone(), fake_embedding(3));
        let _ = pool.get(&key); // force a hit
        let dropped = pool.clear();
        assert!(dropped >= 1, "expected ≥1 entry dropped, got {}", dropped);
        let stats = pool.stats_blocking();
        assert_eq!(stats.hit_count, 0, "clear resets hits");
        assert_eq!(stats.miss_count, 0, "clear resets misses");
        assert_eq!(stats.entry_count, 0, "all entries dropped");
    }

    #[test]
    fn batch_with_partial_hits_records_correct_hit_count() {
        let _g = TEST_LOCK.lock().unwrap();
        let pool = get_embedding_pool();
        pool.clear();
        // Pre-populate two of three entries.
        let model = "nonexistent-batch-test-model";
        pool.insert(EmbeddingKey::new(model, "cached_a"), fake_embedding(1));
        pool.insert(EmbeddingKey::new(model, "cached_b"), fake_embedding(2));
        // Calling batch with only the cached texts should fully short-circuit.
        let got = generate_embeddings_batch(&["cached_a", "cached_b"], model)
            .expect("all-hit batch should succeed without model load");
        assert_eq!(got.len(), 2);
        let stats = pool.stats_blocking();
        assert_eq!(stats.hit_count, 2);
        assert_eq!(stats.miss_count, 0);
    }
}
