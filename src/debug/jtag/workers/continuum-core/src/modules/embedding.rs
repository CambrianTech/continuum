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

        // Load model if needed
        get_or_load_model(model_name)?;

        // Get model from cache
        let cache = get_model_cache();
        let models = cache.lock().map_err(|e| format!("Lock error: {e}"))?;
        let embedding_model = models
            .get(model_name)
            .ok_or_else(|| format!("Model not loaded: {model_name}"))?;

        // Generate embeddings
        let text_refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
        let embeddings = embedding_model
            .embed(text_refs, None)
            .map_err(|e| format!("Embedding generation failed: {e}"))?;

        let duration_ms = start.elapsed().as_millis() as u64;
        let dimensions = embeddings.first().map(|e| e.len()).unwrap_or(0);
        let batch_size = embeddings.len();

        info!(
            "Generated {} embeddings ({}d) in {}ms",
            batch_size, dimensions, duration_ms
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
            "embedding/model/load" => self.handle_model_load(&params),
            "embedding/model/list" => self.handle_model_list(),
            "embedding/model/info" => self.handle_model_info(&params),
            "embedding/model/unload" => self.handle_model_unload(&params),
            _ => Err(format!("Unknown embedding command: {command}")),
        }
    }

    fn as_any(&self) -> &dyn Any { self }
}
