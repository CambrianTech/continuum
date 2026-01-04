/// Candle Inference Worker - Native Rust LLM Inference
///
/// ARCHITECTURE:
/// - Trait-driven design for pluggable model providers and adapters
/// - Uses shared JTAG protocol (JTAGRequest/JTAGResponse)
/// - Integrated LoggerClient for proper logging
/// - Metal acceleration on Apple Silicon
///
/// TRAITS:
/// - ModelProvider: Load/unload models from different sources
/// - AdapterManager: Load/unload/compose LoRA adapters
/// - TextGenerator: Generate text from a model + adapters
///
/// COMMANDS:
/// - ping: Health check
/// - model/load: Load a model from HuggingFace
/// - model/unload: Unload a model from memory
/// - models/list: List loaded models
/// - adapter/load: Load a LoRA adapter
/// - adapter/unload: Unload a LoRA adapter
/// - generate: Generate text with optional adapter composition

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

// Candle imports
use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::generation::LogitsProcessor;
use candle_transformers::models::phi::{Config as PhiConfig, Model as PhiModel};
use candle_transformers::models::llama::{Llama as LlamaModel, Cache as LlamaCache, LlamaConfig as LlamaRawConfig, Config as LlamaModelConfig};
use candle_transformers::models::mistral::{Config as MistralConfig, Model as MistralModel};
use candle_transformers::models::qwen2::{Config as Qwen2Config, ModelForCausalLM as Qwen2Model};

// HuggingFace Hub
use hf_hub::{api::sync::Api, Repo, RepoType};

// Tokenizers
use tokenizers::Tokenizer;

// Random sampling
use rand::{Rng, SeedableRng};

// TypeScript type generation
use ts_rs::TS;

// ============================================================================
// Shared JTAG Protocol (imported from workers/shared/)
// ============================================================================

/// JTAG Request - Universal packet format for all JTAG communication
/// Mirrors: workers/shared/jtag_protocol.rs and shared/ipc/JTAGProtocol.ts
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JTAGRequest<T> {
    pub id: String,
    #[serde(rename = "type")]
    pub r#type: String,
    pub timestamp: String,
    pub payload: T,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

/// JTAG Response - Universal response format
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JTAGResponse<T> {
    pub id: String,
    #[serde(rename = "type")]
    pub r#type: String,
    pub timestamp: String,
    pub payload: T,
    pub request_id: String,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_type: Option<String>,
}

impl<T> JTAGResponse<T> {
    fn success(request_id: String, r#type: String, payload: T) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            r#type,
            timestamp: chrono::Utc::now().to_rfc3339(),
            payload,
            request_id,
            success: true,
            error: None,
            error_type: None,
        }
    }

    fn error(request_id: String, r#type: String, payload: T, error: String) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            r#type,
            timestamp: chrono::Utc::now().to_rfc3339(),
            payload,
            request_id,
            success: false,
            error: Some(error),
            error_type: Some("internal".to_string()),
        }
    }
}

// ============================================================================
// Logger Client (fire-and-forget logging to LoggerWorker)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteLogPayload {
    pub category: String,
    pub level: LogLevel,
    pub component: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Value>,
}

/// Fire-and-forget logger client
pub struct LoggerClient {
    stream: Option<UnixStream>,
    component: String,
    category: String,
}

impl LoggerClient {
    fn connect(socket_path: &str, component: &str) -> Self {
        let stream = UnixStream::connect(socket_path).ok();
        if stream.is_none() {
            eprintln!("⚠️  LoggerClient: Failed to connect to {}", socket_path);
            eprintln!("   Logs will be written to stderr instead");
        }
        Self {
            stream,
            component: component.to_string(),
            category: "inference".to_string(),
        }
    }

    fn info(&mut self, message: &str) {
        self.log_internal(LogLevel::Info, message);
    }

    fn warn(&mut self, message: &str) {
        self.log_internal(LogLevel::Warn, message);
    }

    fn error(&mut self, message: &str) {
        self.log_internal(LogLevel::Error, message);
    }

    fn log_internal(&mut self, level: LogLevel, message: &str) {
        // Fallback to stderr if no connection
        if self.stream.is_none() {
            let level_str = match level {
                LogLevel::Debug => "DEBUG",
                LogLevel::Info => "INFO",
                LogLevel::Warn => "WARN",
                LogLevel::Error => "ERROR",
            };
            eprintln!("[{}] {}: {}", level_str, self.component, message);
            return;
        }

        let request: JTAGRequest<WriteLogPayload> = JTAGRequest {
            id: uuid::Uuid::new_v4().to_string(),
            r#type: "write-log".to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            payload: WriteLogPayload {
                category: self.category.clone(),
                level,
                component: self.component.clone(),
                message: message.to_string(),
                args: None,
            },
            user_id: Some("inference-worker".to_string()),
            session_id: None,
        };

        if let Some(ref mut stream) = self.stream {
            if let Ok(json) = serde_json::to_string(&request) {
                let _ = writeln!(stream, "{}", json);
                let _ = stream.flush();
            }
        }
    }
}

// ============================================================================
// TRAITS - The Core Abstraction Layer
// ============================================================================

/// Result type for model operations
pub type ModelResult<T> = Result<T, String>;

/// Trait for model providers (HuggingFace, local files, etc.)
pub trait ModelProvider: Send + Sync {
    /// Load a model by ID
    fn load(&mut self, model_id: &str, revision: Option<&str>) -> ModelResult<ModelInfo>;

    /// Unload a model by ID
    fn unload(&mut self, model_id: &str) -> ModelResult<()>;

    /// Check if a model is loaded
    fn is_loaded(&self, model_id: &str) -> bool;

    /// List loaded models
    fn list(&self) -> Vec<ModelInfo>;

    /// Get model by ID
    fn get(&self, model_id: &str) -> Option<&LoadedModel>;
}

/// Trait for LoRA adapter management
pub trait AdapterManager: Send + Sync {
    /// Load an adapter from a file path
    fn load_adapter(
        &mut self,
        model_id: &str,
        adapter_path: &str,
        adapter_name: &str,
    ) -> ModelResult<AdapterInfo>;

    /// Unload an adapter
    fn unload_adapter(&mut self, model_id: &str, adapter_name: &str) -> ModelResult<()>;

    /// List adapters for a model
    fn list_adapters(&self, model_id: &str) -> Vec<AdapterInfo>;

    /// Apply loaded adapters by merging weights into model
    fn apply_adapters(&mut self, model_id: &str) -> ModelResult<ApplyResult>;
}

/// Result of applying adapters
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "generated/")]
#[serde(rename_all = "camelCase")]
pub struct ApplyResult {
    pub model_id: String,
    pub adapters_applied: Vec<String>,
    pub layers_merged: usize,
    pub apply_time_ms: u64,
}

/// Trait for text generation
pub trait TextGenerator: Send + Sync {
    /// Generate text from a prompt
    fn generate(&mut self, request: GenerateRequest) -> ModelResult<GenerateResponse>;
}

// ============================================================================
// Data Types
// ============================================================================

/// Model information returned from load/list operations
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "generated/")]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub model_id: String,
    pub status: String,
    pub load_time_ms: Option<u64>,
    pub device: String,
    pub loaded_at_seconds_ago: Option<u64>,
}

/// LoRA adapter information
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "generated/")]
#[serde(rename_all = "camelCase")]
pub struct AdapterInfo {
    pub name: String,
    pub model_id: String,
    pub path: String,
    pub status: String,
}

/// Supported model architectures
pub enum ModelArchitecture {
    Phi(PhiModel),
    Llama(LlamaModel, LlamaCache, LlamaModelConfig),  // Store config to recreate cache
    Mistral(MistralModel),
    Qwen2(Qwen2Model),
}

// Manual Debug impl since models don't derive Debug
impl std::fmt::Debug for ModelArchitecture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModelArchitecture::Phi(_) => write!(f, "Phi"),
            ModelArchitecture::Llama(_, _, _) => write!(f, "Llama"),
            ModelArchitecture::Mistral(_) => write!(f, "Mistral"),
            ModelArchitecture::Qwen2(_) => write!(f, "Qwen2"),
        }
    }
}

pub struct LoadedModel {
    pub model_id: String,
    pub architecture: ModelArchitecture,
    pub tokenizer: Tokenizer,
    pub config: ModelConfig,
    pub device: Device,
    pub loaded_at: Instant,
    /// Path to the base weights file (for reloading with merged LoRA)
    pub weights_path: Option<std::path::PathBuf>,
    /// Path to config file (for reloading)
    pub config_path: Option<std::path::PathBuf>,
    /// Whether LoRA adapters have been merged into weights
    pub adapters_merged: bool,
}

// Manual Debug impl
impl std::fmt::Debug for LoadedModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoadedModel")
            .field("model_id", &self.model_id)
            .field("architecture", &self.architecture)
            .field("device", &self.device)
            .finish()
    }
}

/// Model configuration for generation
#[derive(Debug, Clone)]
pub struct ModelConfig {
    pub vocab_size: usize,
    pub eos_token_id: u32,
    pub bos_token_id: Option<u32>,
}

/// LoRA adapter with actual weight tensors for inference
///
/// LoRA formula: y = Wx + (alpha/rank) * (x @ A.T) @ B.T
/// Weight merging: W' = W + scaling * (B @ A)
///
/// safetensors format stores:
/// - base_layer.lora_A.weight: [rank, input_dim]
/// - base_layer.lora_B.weight: [output_dim, rank]
#[derive(Debug, Clone)]
pub struct LoadedAdapter {
    pub name: String,
    pub path: String,
    /// LoRA rank (dimension of A's output / B's input)
    pub rank: usize,
    /// Scaling factor: alpha / rank
    pub scaling: f32,
    /// Layer-specific LoRA weights: layer_name -> (lora_A, lora_B)
    /// lora_A: [rank, input_dim], lora_B: [output_dim, rank]
    pub weights: HashMap<String, (Tensor, Tensor)>,
}

impl LoadedAdapter {
    /// Load a LoRA adapter from a safetensors file
    pub fn load(
        name: &str,
        path: &str,
        device: &Device,
    ) -> Result<Self, String> {
        use safetensors::SafeTensors;

        // Read the safetensors file
        let data = std::fs::read(path)
            .map_err(|e| format!("Failed to read adapter file: {}", e))?;

        let tensors = SafeTensors::deserialize(&data)
            .map_err(|e| format!("Failed to parse safetensors: {}", e))?;

        // Try to load adapter_config.json for alpha value
        let config_path = std::path::Path::new(path)
            .parent()
            .map(|p| p.join("adapter_config.json"));

        let alpha: f32 = config_path
            .and_then(|p| std::fs::read_to_string(&p).ok())
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            .and_then(|v| v.get("lora_alpha").and_then(|a| a.as_f64()))
            .map(|a| a as f32)
            .unwrap_or(16.0); // Default alpha

        let mut weights: HashMap<String, (Tensor, Tensor)> = HashMap::new();
        let mut detected_rank: Option<usize> = None;

        // Group tensors by layer name
        // Example names: "model.layers.0.self_attn.q_proj.lora_A.weight"
        let tensor_names: Vec<_> = tensors.names().into_iter().collect();

        for name_a in &tensor_names {
            if name_a.contains("lora_A") && name_a.ends_with(".weight") {
                // Find corresponding lora_B
                let name_b = name_a.replace("lora_A", "lora_B");

                // Use explicit string comparison to avoid type mismatch
                if tensor_names.iter().any(|n| *n == name_b.as_str()) {
                    // Extract layer name (everything before .lora_A)
                    let layer_name = name_a
                        .rsplit_once(".lora_A")
                        .map(|(prefix, _)| prefix.to_string())
                        .unwrap_or_else(|| name_a.to_string());

                    // Load tensors
                    let tensor_a = tensors.tensor(name_a)
                        .map_err(|e| format!("Failed to get lora_A tensor: {}", e))?;
                    let tensor_b = tensors.tensor(&name_b)
                        .map_err(|e| format!("Failed to get lora_B tensor: {}", e))?;

                    // Convert to f32 tensors
                    let lora_a = Self::safetensor_to_candle(tensor_a, device)?;
                    let lora_b = Self::safetensor_to_candle(tensor_b, device)?;

                    // Detect rank from lora_A shape [rank, input_dim]
                    let a_shape = lora_a.dims();
                    if a_shape.len() >= 1 {
                        let rank = a_shape[0];
                        if detected_rank.is_none() {
                            detected_rank = Some(rank);
                        }
                    }

                    weights.insert(layer_name, (lora_a, lora_b));
                }
            }
        }

        let rank = detected_rank.unwrap_or(8);
        let scaling = alpha / rank as f32;

        println!("📊 Adapter {}: {} layers, rank={}, alpha={}, scaling={:.4}",
            name, weights.len(), rank, alpha, scaling);

        Ok(Self {
            name: name.to_string(),
            path: path.to_string(),
            rank,
            scaling,
            weights,
        })
    }

    /// Convert safetensors TensorView to Candle Tensor
    fn safetensor_to_candle(
        view: safetensors::tensor::TensorView<'_>,
        device: &Device,
    ) -> Result<Tensor, String> {
        let shape: Vec<usize> = view.shape().to_vec();
        let dtype = view.dtype();
        let data = view.data();

        // Handle potentially unaligned data from safetensors by copying to aligned buffer
        match dtype {
            safetensors::Dtype::F32 => {
                // Copy bytes to properly aligned Vec<f32>
                let num_floats = data.len() / 4;
                let mut floats = vec![0f32; num_floats];
                for (i, chunk) in data.chunks_exact(4).enumerate() {
                    floats[i] = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                }
                Tensor::from_slice(&floats, shape.as_slice(), device)
                    .map_err(|e| format!("Failed to create f32 tensor: {}", e))
            }
            safetensors::Dtype::F16 => {
                // Copy bytes and convert f16 -> f32
                let num_halfs = data.len() / 2;
                let mut floats = Vec::with_capacity(num_halfs);
                for chunk in data.chunks_exact(2) {
                    let bits = u16::from_le_bytes([chunk[0], chunk[1]]);
                    floats.push(half::f16::from_bits(bits).to_f32());
                }
                Tensor::from_slice(&floats, shape.as_slice(), device)
                    .map_err(|e| format!("Failed to create f16->f32 tensor: {}", e))
            }
            safetensors::Dtype::BF16 => {
                // Copy bytes and convert bf16 -> f32
                let num_halfs = data.len() / 2;
                let mut floats = Vec::with_capacity(num_halfs);
                for chunk in data.chunks_exact(2) {
                    let bits = u16::from_le_bytes([chunk[0], chunk[1]]);
                    floats.push(half::bf16::from_bits(bits).to_f32());
                }
                Tensor::from_slice(&floats, shape.as_slice(), device)
                    .map_err(|e| format!("Failed to create bf16->f32 tensor: {}", e))
            }
            _ => Err(format!("Unsupported dtype: {:?}", dtype)),
        }
    }

    /// Check if this adapter has weights for a given layer
    pub fn has_layer(&self, layer_name: &str) -> bool {
        self.weights.contains_key(layer_name)
    }

    /// Get the merged weight delta for a layer: scaling * (B @ A)
    /// This can be added to the base weight: W' = W + delta
    pub fn get_weight_delta(&self, layer_name: &str) -> Option<Result<Tensor, String>> {
        self.weights.get(layer_name).map(|(lora_a, lora_b)| {
            // LoRA formula: delta = scaling * (B @ A)
            // lora_A: [rank, input_dim]
            // lora_B: [output_dim, rank]
            // B @ A = [output_dim, rank] @ [rank, input_dim] = [output_dim, input_dim]
            lora_b.matmul(lora_a)
                .map_err(|e| format!("LoRA matmul failed: {}", e))
                .and_then(|delta| {
                    (delta * self.scaling as f64)
                        .map_err(|e| format!("LoRA scaling failed: {}", e))
                })
        })
    }
}

/// Request for text generation
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(export, export_to = "generated/")]
#[serde(rename_all = "camelCase")]
pub struct GenerateRequest {
    pub model_id: String,
    pub prompt: String,
    pub max_tokens: Option<usize>,
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    pub adapters: Option<Vec<String>>,
}

/// Response from text generation
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "generated/")]
#[serde(rename_all = "camelCase")]
pub struct GenerateResponse {
    pub model_id: String,
    pub text: String,
    pub prompt_tokens: usize,
    pub generated_tokens: usize,
    pub generation_time_ms: u64,
    pub tokens_per_second: f64,
    pub adapters_used: Vec<String>,
}

// ============================================================================
// HuggingFace Model Provider Implementation
// ============================================================================

pub struct HuggingFaceProvider {
    models: HashMap<String, LoadedModel>,
    adapters: HashMap<String, Vec<LoadedAdapter>>, // model_id -> adapters
    device: Device,
    start_time: Instant,
    logger: Arc<Mutex<LoggerClient>>,
}

/// Loaded model result with paths for LoRA merging
pub struct LoadedModelResult {
    pub architecture: ModelArchitecture,
    pub config: ModelConfig,
    pub weights_path: std::path::PathBuf,
    pub config_path: std::path::PathBuf,
}

impl HuggingFaceProvider {
    pub fn new(logger: Arc<Mutex<LoggerClient>>) -> ModelResult<Self> {
        let device = Self::select_device(&logger)?;

        Ok(Self {
            models: HashMap::new(),
            adapters: HashMap::new(),
            device,
            start_time: Instant::now(),
            logger,
        })
    }

    fn select_device(logger: &Arc<Mutex<LoggerClient>>) -> ModelResult<Device> {
        // Try Metal first (Apple Silicon)
        #[cfg(feature = "metal")]
        {
            match Device::new_metal(0) {
                Ok(device) => {
                    if let Ok(mut log) = logger.lock() {
                        log.info("Metal acceleration enabled");
                    }
                    println!("✅ Metal acceleration enabled");
                    return Ok(device);
                }
                Err(e) => {
                    if let Ok(mut log) = logger.lock() {
                        log.warn(&format!("Metal not available: {}", e));
                    }
                    println!("⚠️  Metal not available: {}", e);
                }
            }
        }

        // Try CUDA
        #[cfg(feature = "cuda")]
        {
            match Device::new_cuda(0) {
                Ok(device) => {
                    if let Ok(mut log) = logger.lock() {
                        log.info("CUDA acceleration enabled");
                    }
                    println!("✅ CUDA acceleration enabled");
                    return Ok(device);
                }
                Err(e) => {
                    if let Ok(mut log) = logger.lock() {
                        log.warn(&format!("CUDA not available: {}", e));
                    }
                    println!("⚠️  CUDA not available: {}", e);
                }
            }
        }

        // Fall back to CPU
        if let Ok(mut log) = logger.lock() {
            log.info("Using CPU (no GPU acceleration)");
        }
        println!("ℹ️  Using CPU (no GPU acceleration)");
        Ok(Device::Cpu)
    }

    pub fn uptime_seconds(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }

    pub fn device_info(&self) -> String {
        format!("{:?}", self.device)
    }

    /// Download model weights, handling both single-file and sharded formats
    ///
    /// Returns a list of weight file paths (1 for single file, N for sharded)
    fn download_weights(&self, repo: &hf_hub::api::sync::ApiRepo) -> ModelResult<Vec<std::path::PathBuf>> {
        // Try single-file formats first
        if let Ok(path) = repo.get("model.safetensors") {
            println!("📂 Weights (single): {:?}", path);
            return Ok(vec![path]);
        }

        // Try sharded safetensors - check for index file
        if let Ok(index_path) = repo.get("model.safetensors.index.json") {
            println!("📂 Found sharded weights index: {:?}", index_path);

            // Parse the index to get weight file names
            let index_str = fs::read_to_string(&index_path)
                .map_err(|e| format!("Failed to read weights index: {}", e))?;
            let index: serde_json::Value = serde_json::from_str(&index_str)
                .map_err(|e| format!("Failed to parse weights index: {}", e))?;

            // Get unique shard filenames from weight_map
            let weight_map = index.get("weight_map")
                .and_then(|m| m.as_object())
                .ok_or_else(|| "No weight_map in index".to_string())?;

            let mut shard_names: Vec<String> = weight_map.values()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();
            shard_names.sort();
            shard_names.dedup();

            println!("📦 Downloading {} weight shards...", shard_names.len());

            let mut shard_paths: Vec<std::path::PathBuf> = Vec::new();
            for shard_name in &shard_names {
                let path = repo.get(shard_name)
                    .map_err(|e| format!("Failed to download shard {}: {}", shard_name, e))?;
                println!("  📂 {}", shard_name);
                shard_paths.push(path);
            }

            return Ok(shard_paths);
        }

        // Try pytorch format as last resort
        if let Ok(path) = repo.get("pytorch_model.bin") {
            println!("📂 Weights (pytorch): {:?}", path);
            return Ok(vec![path]);
        }

        Err("No model weights found (tried model.safetensors, sharded, pytorch_model.bin)".to_string())
    }

    /// Create VarBuilder from weight paths (handles single or multiple files)
    fn load_weights_to_varbuilder(&self, paths: &[std::path::PathBuf]) -> ModelResult<VarBuilder<'static>> {
        if paths.is_empty() {
            return Err("No weight paths provided".to_string());
        }

        let first_path = &paths[0];

        if first_path.to_string_lossy().ends_with(".safetensors") {
            // SAFETY: File paths are valid and VarBuilder owns the mmap for its lifetime
            println!("🔧 Loading {} safetensor file(s) to {:?}...", paths.len(), self.device);
            unsafe {
                VarBuilder::from_mmaped_safetensors(paths, DType::F32, &self.device)
                    .map_err(|e| format!("Failed to load safetensors: {}", e))
            }
        } else if first_path.to_string_lossy().ends_with(".bin") {
            if paths.len() > 1 {
                return Err("Sharded pytorch weights not supported".to_string());
            }
            println!("🔧 Loading pytorch weights to {:?}...", self.device);
            VarBuilder::from_pth(first_path.clone(), DType::F32, &self.device)
                .map_err(|e| format!("Failed to load pytorch weights: {}", e))
        } else {
            Err(format!("Unknown weight format: {:?}", first_path))
        }
    }

    /// Load Phi model architecture (supports sharded weights)
    fn load_phi(&self, repo: &hf_hub::api::sync::ApiRepo, tokenizer: &Tokenizer, _model_id: &str) -> ModelResult<LoadedModelResult> {
        // Download weights (handles both single and sharded)
        let weight_paths = self.download_weights(repo)?;
        let weights_path = weight_paths[0].clone(); // Store first for LoRA (TODO: full shard support)

        // Download config
        let config_path = repo
            .get("config.json")
            .map_err(|e| format!("Failed to download config: {}", e))?;
        println!("📂 Config: {:?}", config_path);

        // Load config
        let config_str = fs::read_to_string(&config_path)
            .map_err(|e| format!("Failed to read config: {}", e))?;
        let phi_config: PhiConfig = serde_json::from_str(&config_str)
            .map_err(|e| format!("Failed to parse config: {}", e))?;

        // Get vocab size from tokenizer (config fields are private)
        let vocab_size = tokenizer.get_vocab_size(true);
        // Get EOS token from tokenizer, fallback to common defaults
        let eos_token_id = tokenizer.token_to_id("<|endoftext|>")
            .or_else(|| tokenizer.token_to_id("</s>"))
            .or_else(|| tokenizer.token_to_id("<eos>"))
            .unwrap_or(50256);

        let vb = self.load_weights_to_varbuilder(&weight_paths)?;

        // Create Phi model
        let model = PhiModel::new(&phi_config, vb)
            .map_err(|e| format!("Failed to create Phi model: {}", e))?;
        println!("✅ Phi model loaded: vocab_size={}", vocab_size);

        let config = ModelConfig {
            vocab_size,
            eos_token_id,
            bos_token_id: None,
        };

        Ok(LoadedModelResult {
            architecture: ModelArchitecture::Phi(model),
            config,
            weights_path,
            config_path,
        })
    }

    /// Load Llama model architecture (Llama 2, Llama 3, supports sharded weights)
    fn load_llama(&self, repo: &hf_hub::api::sync::ApiRepo, tokenizer: &Tokenizer, _model_id: &str) -> ModelResult<LoadedModelResult> {
        // Download weights (handles both single and sharded)
        let weight_paths = self.download_weights(repo)?;
        let weights_path = weight_paths[0].clone(); // Store first for LoRA (TODO: full shard support)

        let config_path = repo
            .get("config.json")
            .map_err(|e| format!("Failed to download config: {}", e))?;
        println!("📂 Config: {:?}", config_path);

        // Load and parse config
        let config_str = fs::read_to_string(&config_path)
            .map_err(|e| format!("Failed to read config: {}", e))?;
        let llama_raw_config: LlamaRawConfig = serde_json::from_str(&config_str)
            .map_err(|e| format!("Failed to parse Llama config: {}", e))?;
        let llama_config = llama_raw_config.into_config(false); // no flash attention

        let vocab_size = tokenizer.get_vocab_size(true);
        let eos_token_id = match &llama_config.eos_token_id {
            Some(candle_transformers::models::llama::LlamaEosToks::Single(id)) => *id,
            Some(candle_transformers::models::llama::LlamaEosToks::Multiple(ids)) => ids.first().copied().unwrap_or(2),
            None => tokenizer.token_to_id("</s>").unwrap_or(2),
        };

        let vb = self.load_weights_to_varbuilder(&weight_paths)?;

        // Create Llama model from weights
        let model = LlamaModel::load(vb, &llama_config)
            .map_err(|e| format!("Failed to load Llama model: {}", e))?;

        // Create cache for KV state
        let cache = LlamaCache::new(true, DType::F32, &llama_config, &self.device)
            .map_err(|e| format!("Failed to create Llama cache: {}", e))?;

        println!("✅ Llama model loaded: vocab_size={}", vocab_size);

        Ok(LoadedModelResult {
            architecture: ModelArchitecture::Llama(model, cache, llama_config.clone()),
            config: ModelConfig { vocab_size, eos_token_id, bos_token_id: llama_config.bos_token_id },
            weights_path,
            config_path,
        })
    }

    /// Load Mistral model architecture (supports sharded weights)
    fn load_mistral(&self, repo: &hf_hub::api::sync::ApiRepo, tokenizer: &Tokenizer, _model_id: &str) -> ModelResult<LoadedModelResult> {
        // Download weights (handles both single and sharded)
        let weight_paths = self.download_weights(repo)?;
        let weights_path = weight_paths[0].clone(); // Store first for LoRA (TODO: full shard support)

        let config_path = repo
            .get("config.json")
            .map_err(|e| format!("Failed to download config: {}", e))?;
        println!("📂 Config: {:?}", config_path);

        let config_str = fs::read_to_string(&config_path)
            .map_err(|e| format!("Failed to read config: {}", e))?;
        let mistral_config: MistralConfig = serde_json::from_str(&config_str)
            .map_err(|e| format!("Failed to parse Mistral config: {}", e))?;

        let vocab_size = tokenizer.get_vocab_size(true);
        let eos_token_id = tokenizer.token_to_id("</s>").unwrap_or(2);

        let vb = self.load_weights_to_varbuilder(&weight_paths)?;

        let model = MistralModel::new(&mistral_config, vb)
            .map_err(|e| format!("Failed to create Mistral model: {}", e))?;
        println!("✅ Mistral model loaded: vocab_size={}", vocab_size);

        Ok(LoadedModelResult {
            architecture: ModelArchitecture::Mistral(model),
            config: ModelConfig { vocab_size, eos_token_id, bos_token_id: None },
            weights_path,
            config_path,
        })
    }

    /// Load Qwen2 model architecture (supports sharded weights)
    fn load_qwen2(&self, repo: &hf_hub::api::sync::ApiRepo, tokenizer: &Tokenizer, _model_id: &str) -> ModelResult<LoadedModelResult> {
        // Download weights (handles both single and sharded)
        let weight_paths = self.download_weights(repo)?;
        let weights_path = weight_paths[0].clone(); // Store first for LoRA (TODO: full shard support)

        let config_path = repo
            .get("config.json")
            .map_err(|e| format!("Failed to download config: {}", e))?;
        println!("📂 Config: {:?}", config_path);

        let config_str = fs::read_to_string(&config_path)
            .map_err(|e| format!("Failed to read config: {}", e))?;
        let qwen2_config: Qwen2Config = serde_json::from_str(&config_str)
            .map_err(|e| format!("Failed to parse Qwen2 config: {}", e))?;

        let vocab_size = tokenizer.get_vocab_size(true);
        let eos_token_id = tokenizer.token_to_id("<|endoftext|>")
            .or_else(|| tokenizer.token_to_id("<|im_end|>"))
            .unwrap_or(151643);

        let vb = self.load_weights_to_varbuilder(&weight_paths)?;

        let model = Qwen2Model::new(&qwen2_config, vb)
            .map_err(|e| format!("Failed to create Qwen2 model: {}", e))?;
        println!("✅ Qwen2 model loaded: vocab_size={}", vocab_size);

        Ok(LoadedModelResult {
            architecture: ModelArchitecture::Qwen2(model),
            config: ModelConfig { vocab_size, eos_token_id, bos_token_id: None },
            weights_path,
            config_path,
        })
    }

    /// Detect model architecture from model_id or config.json
    fn detect_architecture(&self, model_id: &str, config_path: &std::path::Path) -> &'static str {
        let model_id_lower = model_id.to_lowercase();

        // Check model_id first
        if model_id_lower.contains("llama") {
            return "llama";
        }
        if model_id_lower.contains("mistral") {
            return "mistral";
        }
        if model_id_lower.contains("qwen") {
            return "qwen2";
        }
        if model_id_lower.contains("phi") {
            return "phi";
        }

        // Try to detect from config.json architectures field
        if let Ok(config_str) = fs::read_to_string(config_path) {
            if let Ok(config) = serde_json::from_str::<serde_json::Value>(&config_str) {
                if let Some(archs) = config.get("architectures").and_then(|a| a.as_array()) {
                    for arch in archs {
                        if let Some(arch_str) = arch.as_str() {
                            let arch_lower = arch_str.to_lowercase();
                            if arch_lower.contains("llama") {
                                return "llama";
                            }
                            if arch_lower.contains("mistral") {
                                return "mistral";
                            }
                            if arch_lower.contains("qwen") {
                                return "qwen2";
                            }
                            if arch_lower.contains("phi") {
                                return "phi";
                            }
                        }
                    }
                }
                // Also check model_type field
                if let Some(model_type) = config.get("model_type").and_then(|m| m.as_str()) {
                    let mt_lower = model_type.to_lowercase();
                    if mt_lower.contains("llama") {
                        return "llama";
                    }
                    if mt_lower.contains("mistral") {
                        return "mistral";
                    }
                    if mt_lower.contains("qwen") {
                        return "qwen2";
                    }
                    if mt_lower.contains("phi") {
                        return "phi";
                    }
                }
            }
        }

        // Default to phi for unknown models
        "phi"
    }

    /// Load safetensors file into a HashMap of tensors
    fn load_safetensors_to_map(&self, path: &std::path::Path) -> ModelResult<HashMap<String, Tensor>> {
        use safetensors::SafeTensors;

        let data = fs::read(path)
            .map_err(|e| format!("Failed to read safetensors: {}", e))?;
        let tensors = SafeTensors::deserialize(&data)
            .map_err(|e| format!("Failed to parse safetensors: {}", e))?;

        let mut result: HashMap<String, Tensor> = HashMap::new();

        for name in tensors.names() {
            let view = tensors.tensor(name)
                .map_err(|e| format!("Failed to get tensor {}: {}", name, e))?;

            let shape: Vec<usize> = view.shape().to_vec();
            let data = view.data();

            // Handle F32 (most common for base models)
            let tensor = match view.dtype() {
                safetensors::Dtype::F32 => {
                    let num_floats = data.len() / 4;
                    let mut floats = vec![0f32; num_floats];
                    for (i, chunk) in data.chunks_exact(4).enumerate() {
                        floats[i] = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                    }
                    Tensor::from_slice(&floats, shape.as_slice(), &self.device)
                        .map_err(|e| format!("Failed to create tensor: {}", e))?
                }
                safetensors::Dtype::F16 => {
                    let num_halfs = data.len() / 2;
                    let mut floats = Vec::with_capacity(num_halfs);
                    for chunk in data.chunks_exact(2) {
                        let bits = u16::from_le_bytes([chunk[0], chunk[1]]);
                        floats.push(half::f16::from_bits(bits).to_f32());
                    }
                    Tensor::from_slice(&floats, shape.as_slice(), &self.device)
                        .map_err(|e| format!("Failed to create tensor: {}", e))?
                }
                safetensors::Dtype::BF16 => {
                    let num_halfs = data.len() / 2;
                    let mut floats = Vec::with_capacity(num_halfs);
                    for chunk in data.chunks_exact(2) {
                        let bits = u16::from_le_bytes([chunk[0], chunk[1]]);
                        floats.push(half::bf16::from_bits(bits).to_f32());
                    }
                    Tensor::from_slice(&floats, shape.as_slice(), &self.device)
                        .map_err(|e| format!("Failed to create tensor: {}", e))?
                }
                _ => {
                    println!("⚠️  Skipping tensor {} with unsupported dtype {:?}", name, view.dtype());
                    continue;
                }
            };

            result.insert(name.to_string(), tensor);
        }

        Ok(result)
    }

    /// Apply LoRA adapters to base weights and rebuild model
    fn apply_adapters_to_model(
        &self,
        weights_path: &std::path::Path,
        config_path: &std::path::Path,
        adapters: &[LoadedAdapter],
        tokenizer: &Tokenizer,
    ) -> ModelResult<(ModelArchitecture, ModelConfig)> {
        println!("🔄 Merging {} adapter(s) into model weights...", adapters.len());
        let start = Instant::now();

        // 1. Load base weights into HashMap
        let mut base_weights = self.load_safetensors_to_map(weights_path)?;
        println!("  📦 Loaded {} base tensors", base_weights.len());

        // 2. Apply each adapter's deltas
        let mut merged_count = 0;
        for adapter in adapters {
            for (layer_name, (_lora_a, _lora_b)) in &adapter.weights {
                // The layer_name from adapter is like "model.layers.0.self_attn.q_proj"
                // The base weight name is like "model.layers.0.self_attn.q_proj.weight"
                let base_name = format!("{}.weight", layer_name);

                if let Some(base_weight) = base_weights.get(&base_name) {
                    // Compute delta: scaling * (B @ A)
                    if let Some(delta_result) = adapter.get_weight_delta(layer_name) {
                        match delta_result {
                            Ok(delta) => {
                                // Merge: W' = W + delta
                                let merged = base_weight.add(&delta)
                                    .map_err(|e| format!("Failed to merge weights for {}: {}", layer_name, e))?;
                                base_weights.insert(base_name.clone(), merged);
                                merged_count += 1;
                            }
                            Err(e) => {
                                println!("⚠️  Failed to compute delta for {}: {}", layer_name, e);
                            }
                        }
                    }
                } else {
                    // Try without .weight suffix (some layers store differently)
                    println!("⚠️  No base weight found for: {} (tried {})", layer_name, base_name);
                }
            }
        }
        println!("  ✅ Merged {} layer weights", merged_count);

        // 3. Rebuild model with merged weights
        let config_str = fs::read_to_string(config_path)
            .map_err(|e| format!("Failed to read config: {}", e))?;
        let phi_config: PhiConfig = serde_json::from_str(&config_str)
            .map_err(|e| format!("Failed to parse config: {}", e))?;

        // Create VarBuilder from the merged weights HashMap
        let vb = VarBuilder::from_tensors(base_weights, DType::F32, &self.device);

        let model = PhiModel::new(&phi_config, vb)
            .map_err(|e| format!("Failed to create merged Phi model: {}", e))?;

        let vocab_size = tokenizer.get_vocab_size(true);
        let eos_token_id = tokenizer.token_to_id("<|endoftext|>")
            .or_else(|| tokenizer.token_to_id("</s>"))
            .or_else(|| tokenizer.token_to_id("<eos>"))
            .unwrap_or(50256);

        println!("  ✅ Model rebuilt with merged weights in {}ms", start.elapsed().as_millis());

        Ok((
            ModelArchitecture::Phi(model),
            ModelConfig { vocab_size, eos_token_id, bos_token_id: None },
        ))
    }
}

impl ModelProvider for HuggingFaceProvider {
    fn load(&mut self, model_id: &str, revision: Option<&str>) -> ModelResult<ModelInfo> {
        if self.models.contains_key(model_id) {
            return Ok(ModelInfo {
                model_id: model_id.to_string(),
                status: "already_loaded".to_string(),
                load_time_ms: None,
                device: self.device_info(),
                loaded_at_seconds_ago: self
                    .models
                    .get(model_id)
                    .map(|m| m.loaded_at.elapsed().as_secs()),
            });
        }

        if let Ok(mut log) = self.logger.lock() {
            log.info(&format!("Loading model: {}", model_id));
        }
        println!("📥 Loading model: {}", model_id);
        let load_start = Instant::now();

        // Download from HuggingFace Hub
        let api = Api::new().map_err(|e| format!("Failed to create HF API: {}", e))?;
        let repo = api.repo(Repo::with_revision(
            model_id.to_string(),
            RepoType::Model,
            revision.unwrap_or("main").to_string(),
        ));

        // Load tokenizer
        let tokenizer_path = repo
            .get("tokenizer.json")
            .map_err(|e| format!("Failed to download tokenizer: {}", e))?;
        println!("📂 Tokenizer: {:?}", tokenizer_path);
        let tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| format!("Failed to load tokenizer: {}", e))?;

        // Detect and load model architecture
        let config_path = repo
            .get("config.json")
            .map_err(|e| format!("Failed to download config for detection: {}", e))?;

        let arch = self.detect_architecture(model_id, &config_path);
        println!("🔍 Detected architecture: {}", arch);

        let model_result = match arch {
            "llama" => self.load_llama(&repo, &tokenizer, model_id)?,
            "mistral" => self.load_mistral(&repo, &tokenizer, model_id)?,
            "qwen2" => self.load_qwen2(&repo, &tokenizer, model_id)?,
            _ => self.load_phi(&repo, &tokenizer, model_id)?,  // Default to Phi
        };

        let load_time_ms = load_start.elapsed().as_millis() as u64;

        self.models.insert(
            model_id.to_string(),
            LoadedModel {
                model_id: model_id.to_string(),
                architecture: model_result.architecture,
                tokenizer,
                config: model_result.config,
                device: self.device.clone(),
                loaded_at: Instant::now(),
                weights_path: Some(model_result.weights_path),
                config_path: Some(model_result.config_path),
                adapters_merged: false,
            },
        );

        if let Ok(mut log) = self.logger.lock() {
            log.info(&format!("Model loaded in {}ms: {}", load_time_ms, model_id));
        }
        println!("✅ Model loaded in {}ms: {}", load_time_ms, model_id);

        Ok(ModelInfo {
            model_id: model_id.to_string(),
            status: "loaded".to_string(),
            load_time_ms: Some(load_time_ms),
            device: self.device_info(),
            loaded_at_seconds_ago: Some(0),
        })
    }

    fn unload(&mut self, model_id: &str) -> ModelResult<()> {
        if self.models.remove(model_id).is_some() {
            self.adapters.remove(model_id);
            if let Ok(mut log) = self.logger.lock() {
                log.info(&format!("Unloaded model: {}", model_id));
            }
            println!("🗑️  Unloaded model: {}", model_id);
            Ok(())
        } else {
            Err(format!("Model not found: {}", model_id))
        }
    }

    fn is_loaded(&self, model_id: &str) -> bool {
        self.models.contains_key(model_id)
    }

    fn list(&self) -> Vec<ModelInfo> {
        self.models
            .iter()
            .map(|(id, m)| ModelInfo {
                model_id: id.clone(),
                status: "loaded".to_string(),
                load_time_ms: None,
                device: self.device_info(),
                loaded_at_seconds_ago: Some(m.loaded_at.elapsed().as_secs()),
            })
            .collect()
    }

    fn get(&self, model_id: &str) -> Option<&LoadedModel> {
        self.models.get(model_id)
    }
}

impl AdapterManager for HuggingFaceProvider {
    fn load_adapter(
        &mut self,
        model_id: &str,
        adapter_path: &str,
        adapter_name: &str,
    ) -> ModelResult<AdapterInfo> {
        if !self.models.contains_key(model_id) {
            return Err(format!("Model not loaded: {}", model_id));
        }

        if !std::path::Path::new(adapter_path).exists() {
            return Err(format!("Adapter file not found: {}", adapter_path));
        }

        if let Ok(mut log) = self.logger.lock() {
            log.info(&format!(
                "Loading adapter: {} for model: {}",
                adapter_name, model_id
            ));
        }
        println!(
            "📥 Loading adapter: {} for model: {}",
            adapter_name, model_id
        );

        // Load actual LoRA weights from safetensors file
        let adapter = LoadedAdapter::load(adapter_name, adapter_path, &self.device)?;

        let layer_count = adapter.weights.len();
        let rank = adapter.rank;

        self.adapters
            .entry(model_id.to_string())
            .or_insert_with(Vec::new)
            .push(adapter);

        println!("✅ Adapter loaded: {} ({} layers, rank={})", adapter_name, layer_count, rank);

        Ok(AdapterInfo {
            name: adapter_name.to_string(),
            model_id: model_id.to_string(),
            path: adapter_path.to_string(),
            status: "loaded".to_string(),
        })
    }

    fn unload_adapter(&mut self, model_id: &str, adapter_name: &str) -> ModelResult<()> {
        if let Some(adapters) = self.adapters.get_mut(model_id) {
            let initial_len = adapters.len();
            adapters.retain(|a| a.name != adapter_name);

            if adapters.len() < initial_len {
                if let Ok(mut log) = self.logger.lock() {
                    log.info(&format!(
                        "Unloaded adapter: {} from model: {}",
                        adapter_name, model_id
                    ));
                }
                println!(
                    "🗑️  Unloaded adapter: {} from model: {}",
                    adapter_name, model_id
                );
                return Ok(());
            }
        }

        Err(format!(
            "Adapter not found: {} for model: {}",
            adapter_name, model_id
        ))
    }

    fn list_adapters(&self, model_id: &str) -> Vec<AdapterInfo> {
        self.adapters
            .get(model_id)
            .map(|adapters| {
                adapters
                    .iter()
                    .map(|a| AdapterInfo {
                        name: a.name.clone(),
                        model_id: model_id.to_string(),
                        path: a.path.clone(),
                        status: "loaded".to_string(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn apply_adapters(&mut self, model_id: &str) -> ModelResult<ApplyResult> {
        let start = Instant::now();

        // Get loaded model and its paths
        let loaded_model = self.models.get(model_id)
            .ok_or_else(|| format!("Model not loaded: {}", model_id))?;

        let weights_path = loaded_model.weights_path.clone()
            .ok_or_else(|| "No weights path stored for model".to_string())?;
        let config_path = loaded_model.config_path.clone()
            .ok_or_else(|| "No config path stored for model".to_string())?;

        if loaded_model.adapters_merged {
            return Err("Adapters already merged. Reload model to apply different adapters.".to_string());
        }

        // Get adapters for this model
        let adapters = self.adapters.get(model_id)
            .cloned()
            .unwrap_or_default();

        if adapters.is_empty() {
            return Err(format!("No adapters loaded for model: {}", model_id));
        }

        let adapter_names: Vec<String> = adapters.iter().map(|a| a.name.clone()).collect();
        let total_layers: usize = adapters.iter().map(|a| a.weights.len()).sum();

        // Get tokenizer reference (we need to clone it since we'll mutate the model)
        let tokenizer = self.models.get(model_id).unwrap().tokenizer.clone();

        // Merge weights and rebuild model
        let (new_arch, new_config) = self.apply_adapters_to_model(
            &weights_path,
            &config_path,
            &adapters,
            &tokenizer,
        )?;

        // Update the loaded model with merged architecture
        let loaded_model = self.models.get_mut(model_id).unwrap();
        loaded_model.architecture = new_arch;
        loaded_model.config = new_config;
        loaded_model.adapters_merged = true;
        loaded_model.loaded_at = Instant::now();

        let apply_time_ms = start.elapsed().as_millis() as u64;

        println!("✅ Applied {} adapter(s) to {}: {} layers merged in {}ms",
            adapter_names.len(), model_id, total_layers, apply_time_ms);

        Ok(ApplyResult {
            model_id: model_id.to_string(),
            adapters_applied: adapter_names,
            layers_merged: total_layers,
            apply_time_ms,
        })
    }
}

// ============================================================================
// Generation Helpers for Each Architecture (standalone functions to avoid borrow issues)
// ============================================================================

/// Generate with Phi model (internal KV cache)
fn generate_with_phi(
    model: &mut PhiModel,
    prompt_tokens: &[u32],
    max_tokens: usize,
    logits_processor: &mut LogitsProcessor,
    device: &Device,
    eos_token_id: u32,
) -> ModelResult<Vec<u32>> {
    let prompt_len = prompt_tokens.len();
    let total_capacity = prompt_len + max_tokens;
    let mut all_tokens: Box<Vec<u32>> = Box::new(Vec::with_capacity(total_capacity));
    all_tokens.extend_from_slice(prompt_tokens);
    let mut gen_tokens: Box<Vec<u32>> = Box::new(Vec::with_capacity(max_tokens));
    let mut single_token_buf = [0u32; 1];

    for i in 0..max_tokens {
        let input = if i == 0 {
            Tensor::new(all_tokens.as_slice(), device)
                .map_err(|e| format!("Failed to create prompt tensor: {}", e))?
        } else {
            single_token_buf[0] = *all_tokens.last().unwrap_or(&0);
            Tensor::new(&single_token_buf[..], device)
                .map_err(|e| format!("Failed to create token tensor: {}", e))?
        };

        let input = input.unsqueeze(0)
            .map_err(|e| format!("Failed to unsqueeze: {}", e))?;

        let logits = model
            .forward(&input)
            .map_err(|e| format!("Phi forward pass failed: {}", e))?;

        if i == 0 {
            println!("🔍 Phi logits shape: {:?}", logits.shape());
        }

        let next_token = sample_from_logits(&logits, logits_processor)?;

        if next_token == eos_token_id {
            break;
        }

        all_tokens.push(next_token);
        gen_tokens.push(next_token);
    }

    Ok(*gen_tokens)
}

/// Generate with Llama model (external cache)
fn generate_with_llama(
    model: &LlamaModel,
    cache: &mut LlamaCache,
    prompt_tokens: &[u32],
    max_tokens: usize,
    logits_processor: &mut LogitsProcessor,
    device: &Device,
    eos_token_id: u32,
) -> ModelResult<Vec<u32>> {
    let prompt_len = prompt_tokens.len();
    let total_capacity = prompt_len + max_tokens;
    let mut all_tokens: Box<Vec<u32>> = Box::new(Vec::with_capacity(total_capacity));
    all_tokens.extend_from_slice(prompt_tokens);
    let mut gen_tokens: Box<Vec<u32>> = Box::new(Vec::with_capacity(max_tokens));
    let mut index_pos = 0usize;

    for i in 0..max_tokens {
        let (input_tokens, current_len) = if i == 0 {
            (all_tokens.as_slice(), prompt_len)
        } else {
            let last = all_tokens.last().copied().unwrap_or(0);
            (&[last][..], 1)
        };

        let input = Tensor::new(input_tokens, device)
            .map_err(|e| format!("Failed to create tensor: {}", e))?
            .unsqueeze(0)
            .map_err(|e| format!("Failed to unsqueeze: {}", e))?;

        // Llama forward: (input, index_pos, cache)
        let logits = model
            .forward(&input, index_pos, cache)
            .map_err(|e| format!("Llama forward pass failed: {}", e))?;

        if i == 0 {
            println!("🔍 Llama logits shape: {:?}", logits.shape());
        }

        index_pos += current_len;

        let next_token = sample_from_logits(&logits, logits_processor)?;

        if next_token == eos_token_id {
            break;
        }

        all_tokens.push(next_token);
        gen_tokens.push(next_token);
    }

    Ok(*gen_tokens)
}

/// Generate with Mistral model (internal KV cache, seqlen offset)
fn generate_with_mistral(
    model: &mut MistralModel,
    prompt_tokens: &[u32],
    max_tokens: usize,
    logits_processor: &mut LogitsProcessor,
    device: &Device,
    eos_token_id: u32,
) -> ModelResult<Vec<u32>> {
    let prompt_len = prompt_tokens.len();
    let total_capacity = prompt_len + max_tokens;
    let mut all_tokens: Box<Vec<u32>> = Box::new(Vec::with_capacity(total_capacity));
    all_tokens.extend_from_slice(prompt_tokens);
    let mut gen_tokens: Box<Vec<u32>> = Box::new(Vec::with_capacity(max_tokens));
    let mut seqlen_offset = 0usize;

    for i in 0..max_tokens {
        let (input_tokens, current_len) = if i == 0 {
            (all_tokens.as_slice(), prompt_len)
        } else {
            let last = all_tokens.last().copied().unwrap_or(0);
            (&[last][..], 1)
        };

        let input = Tensor::new(input_tokens, device)
            .map_err(|e| format!("Failed to create tensor: {}", e))?
            .unsqueeze(0)
            .map_err(|e| format!("Failed to unsqueeze: {}", e))?;

        // Mistral forward: (input, seqlen_offset)
        let logits = model
            .forward(&input, seqlen_offset)
            .map_err(|e| format!("Mistral forward pass failed: {}", e))?;

        if i == 0 {
            println!("🔍 Mistral logits shape: {:?}", logits.shape());
        }

        seqlen_offset += current_len;

        let next_token = sample_from_logits(&logits, logits_processor)?;

        if next_token == eos_token_id {
            break;
        }

        all_tokens.push(next_token);
        gen_tokens.push(next_token);
    }

    Ok(*gen_tokens)
}

/// Generate with Qwen2 model (internal KV cache, seqlen offset)
fn generate_with_qwen2(
    model: &mut Qwen2Model,
    prompt_tokens: &[u32],
    max_tokens: usize,
    logits_processor: &mut LogitsProcessor,
    device: &Device,
    eos_token_id: u32,
) -> ModelResult<Vec<u32>> {
    let prompt_len = prompt_tokens.len();
    let total_capacity = prompt_len + max_tokens;
    let mut all_tokens: Box<Vec<u32>> = Box::new(Vec::with_capacity(total_capacity));
    all_tokens.extend_from_slice(prompt_tokens);
    let mut gen_tokens: Box<Vec<u32>> = Box::new(Vec::with_capacity(max_tokens));
    let mut seqlen_offset = 0usize;

    for i in 0..max_tokens {
        let (input_tokens, current_len) = if i == 0 {
            (all_tokens.as_slice(), prompt_len)
        } else {
            let last = all_tokens.last().copied().unwrap_or(0);
            (&[last][..], 1)
        };

        let input = Tensor::new(input_tokens, device)
            .map_err(|e| format!("Failed to create tensor: {}", e))?
            .unsqueeze(0)
            .map_err(|e| format!("Failed to unsqueeze: {}", e))?;

        // Qwen2 forward: (input, seqlen_offset)
        let logits = model
            .forward(&input, seqlen_offset)
            .map_err(|e| format!("Qwen2 forward pass failed: {}", e))?;

        if i == 0 {
            println!("🔍 Qwen2 logits shape: {:?}", logits.shape());
        }

        seqlen_offset += current_len;

        let next_token = sample_from_logits(&logits, logits_processor)?;

        if next_token == eos_token_id {
            break;
        }

        all_tokens.push(next_token);
        gen_tokens.push(next_token);
    }

    Ok(*gen_tokens)
}

/// Sample next token from logits (shared across all architectures)
fn sample_from_logits(
    logits: &Tensor,
    logits_processor: &mut LogitsProcessor,
) -> ModelResult<u32> {
    let dims = logits.dims();
    let last_logits = match dims.len() {
        2 => {
            // Shape [batch, vocab] - squeeze batch dim
            logits.squeeze(0)
                .map_err(|e| format!("Failed to squeeze 2D: {}", e))?
        }
        3 => {
            // Shape [batch, seq, vocab] - get last token
            let seq_len = dims[1];
            if seq_len == 0 {
                return Err("Empty sequence in logits".to_string());
            }
            logits
                .squeeze(0)  // [seq, vocab]
                .map_err(|e| format!("Failed to squeeze 3D: {}", e))?
                .get(seq_len - 1)  // [vocab]
                .map_err(|e| format!("Failed to get last token: {}", e))?
        }
        _ => return Err(format!("Unexpected logits dims: {:?}", dims)),
    };

    logits_processor
        .sample(&last_logits)
        .map_err(|e| format!("Sampling failed: {}", e))
}

impl TextGenerator for HuggingFaceProvider {
    fn generate(&mut self, request: GenerateRequest) -> ModelResult<GenerateResponse> {
        let loaded_model = self
            .models
            .get_mut(&request.model_id)
            .ok_or_else(|| format!("Model not loaded: {}", request.model_id))?;

        let gen_start = Instant::now();

        // Tokenize the prompt
        let encoding = loaded_model
            .tokenizer
            .encode(request.prompt.as_str(), true)
            .map_err(|e| format!("Tokenization failed: {}", e))?;

        let prompt_tokens: Vec<u32> = encoding.get_ids().to_vec();
        let prompt_len = prompt_tokens.len();

        let adapters_used = request.adapters.clone().unwrap_or_default();

        // NOTE: LoRA adapters are loaded with actual weights (see LoadedAdapter::load),
        // but weight application during inference is not yet implemented.
        //
        // STATUS: LoadedAdapter now stores the actual A/B weight tensors and can compute
        // weight deltas via get_weight_delta(). However, Candle's PhiModel holds weights
        // immutably after construction.
        //
        // TO IMPLEMENT LoRA APPLICATION (choose one):
        // 1. Weight merging: Rebuild model with W' = W + scaling*(B@A) for each target layer
        //    - Slow but works with current architecture
        //    - Cache merged model when adapter set changes
        // 2. Custom forward: Implement LoRA-aware forward pass that applies delta at runtime
        //    - Faster for adapter switching but requires model architecture changes
        // 3. candle-lora: Wait for/contribute to candle-lora crate for native support
        //
        // For now, log adapters but generation uses base model weights only.
        if !adapters_used.is_empty() {
            println!("⚠️  Adapters requested but not yet applied: {:?}", adapters_used);
            println!("   LoRA weights ARE loaded, but weight merging not implemented yet.");
            println!("   See TODO in generate() for implementation options.");
        }

        if let Ok(mut log) = self.logger.lock() {
            log.info(&format!(
                "Generating: {} prompt tokens, adapters: {:?}{}",
                prompt_len,
                adapters_used,
                if adapters_used.is_empty() { "" } else { " (not applied)" }
            ));
        }

        let max_tokens = request.max_tokens.unwrap_or(256);
        let temperature = request.temperature.unwrap_or(0.7);
        let top_p = request.top_p.unwrap_or(0.9);

        // Create logits processor for sampling
        let mut logits_processor = LogitsProcessor::new(
            rand::rngs::StdRng::seed_from_u64(42).gen(), // Random seed
            Some(temperature),
            Some(top_p),
        );

        // Extract values we need before borrowing model mutably
        let device = loaded_model.device.clone();
        let eos_token_id = loaded_model.config.eos_token_id;

        // Generate tokens autoregressively
        let generated_tokens = match &mut loaded_model.architecture {
            ModelArchitecture::Phi(model) => {
                // Clear KV cache before starting generation
                model.clear_kv_cache();
                generate_with_phi(model, &prompt_tokens, max_tokens, &mut logits_processor, &device, eos_token_id)?
            }
            ModelArchitecture::Llama(model, cache, config) => {
                // Recreate cache to clear KV state between generations
                // (LlamaCache has no reset method, so we replace it entirely)
                *cache = LlamaCache::new(true, DType::F32, config, &device)
                    .map_err(|e| format!("Failed to recreate Llama cache: {}", e))?;
                generate_with_llama(model, cache, &prompt_tokens, max_tokens, &mut logits_processor, &device, eos_token_id)?
            }
            ModelArchitecture::Mistral(model) => {
                // Clear Mistral internal cache
                model.clear_kv_cache();
                generate_with_mistral(model, &prompt_tokens, max_tokens, &mut logits_processor, &device, eos_token_id)?
            }
            ModelArchitecture::Qwen2(model) => {
                // Clear Qwen2 internal cache
                model.clear_kv_cache();
                generate_with_qwen2(model, &prompt_tokens, max_tokens, &mut logits_processor, &device, eos_token_id)?
            }
        };

        let gen_time_ms = gen_start.elapsed().as_millis() as u64;
        let num_generated = generated_tokens.len();
        let tokens_per_second = if gen_time_ms > 0 {
            (num_generated as f64 / gen_time_ms as f64) * 1000.0
        } else {
            0.0
        };

        // Decode generated tokens
        let generated_text = loaded_model
            .tokenizer
            .decode(&generated_tokens, true)
            .map_err(|e| format!("Decoding failed: {}", e))?;

        if let Ok(mut log) = self.logger.lock() {
            log.info(&format!(
                "Generated {} tokens in {}ms ({:.1} tok/s)",
                num_generated, gen_time_ms, tokens_per_second
            ));
        }
        println!(
            "✨ Generated {} tokens in {}ms ({:.1} tok/s)",
            num_generated, gen_time_ms, tokens_per_second
        );

        Ok(GenerateResponse {
            model_id: request.model_id,
            text: generated_text,
            prompt_tokens: prompt_len,
            generated_tokens: num_generated,
            generation_time_ms: gen_time_ms,
            tokens_per_second,
            adapters_used,
        })
    }
}

// ============================================================================
// Request/Response Types (Command Payloads)
// ============================================================================

#[derive(Debug, Deserialize)]
#[serde(tag = "command")]
enum InferenceCommand {
    #[serde(rename = "ping")]
    Ping,

    #[serde(rename = "model/load")]
    ModelLoad {
        model_id: String,
        revision: Option<String>,
    },

    #[serde(rename = "model/unload")]
    ModelUnload { model_id: String },

    #[serde(rename = "models/list")]
    ModelsList,

    #[serde(rename = "adapter/load")]
    AdapterLoad {
        model_id: String,
        adapter_path: String,
        adapter_name: String,
    },

    #[serde(rename = "adapter/unload")]
    AdapterUnload {
        model_id: String,
        adapter_name: String,
    },

    #[serde(rename = "adapters/list")]
    AdaptersList { model_id: String },

    /// Apply loaded adapters by merging weights into model (rebuilds model)
    #[serde(rename = "adapter/apply")]
    AdapterApply { model_id: String },

    #[serde(rename = "generate")]
    Generate {
        model_id: String,
        prompt: String,
        max_tokens: Option<usize>,
        temperature: Option<f64>,
        top_p: Option<f64>,
        adapters: Option<Vec<String>>,
    },
}

// ============================================================================
// Inference Worker (Main Coordinator)
// ============================================================================

struct InferenceWorker<P: ModelProvider + AdapterManager + TextGenerator> {
    provider: Mutex<P>,
    logger: Arc<Mutex<LoggerClient>>,
}

impl<P: ModelProvider + AdapterManager + TextGenerator> InferenceWorker<P> {
    fn new(provider: P, logger: Arc<Mutex<LoggerClient>>) -> Self {
        Self {
            provider: Mutex::new(provider),
            logger,
        }
    }

    fn handle_request(&self, request_json: &str) -> String {
        // Try to parse as JTAG protocol first, then fallback to direct command
        let (request_id, command) = match self.parse_request(request_json) {
            Ok((id, cmd)) => (id, cmd),
            Err(e) => {
                return self.error_response("unknown", "parse_error", &e);
            }
        };

        let response_type = self.command_type(&command);

        match self.execute_command(command) {
            Ok(data) => {
                let response = JTAGResponse::success(request_id, response_type, data);
                serde_json::to_string(&response).unwrap_or_else(|_| {
                    r#"{"success":false,"error":"Failed to serialize response"}"#.to_string()
                })
            }
            Err(e) => {
                if let Ok(mut log) = self.logger.lock() {
                    log.error(&format!("Command error: {}", e));
                }
                let response =
                    JTAGResponse::error(request_id, response_type, json!(null), e);
                serde_json::to_string(&response).unwrap_or_else(|_| {
                    r#"{"success":false,"error":"Failed to serialize error response"}"#.to_string()
                })
            }
        }
    }

    fn parse_request(&self, json_str: &str) -> Result<(String, InferenceCommand), String> {
        // Try JTAG protocol format first (has "payload" field)
        if let Ok(jtag_req) = serde_json::from_str::<JTAGRequest<InferenceCommand>>(json_str) {
            return Ok((jtag_req.id, jtag_req.payload));
        }

        // Fallback to direct command format (for backward compatibility)
        match serde_json::from_str::<InferenceCommand>(json_str) {
            Ok(cmd) => Ok((uuid::Uuid::new_v4().to_string(), cmd)),
            Err(e) => Err(format!("Invalid request: {}", e)),
        }
    }

    fn command_type(&self, command: &InferenceCommand) -> String {
        match command {
            InferenceCommand::Ping => "ping",
            InferenceCommand::ModelLoad { .. } => "model/load",
            InferenceCommand::ModelUnload { .. } => "model/unload",
            InferenceCommand::ModelsList => "models/list",
            InferenceCommand::AdapterLoad { .. } => "adapter/load",
            InferenceCommand::AdapterUnload { .. } => "adapter/unload",
            InferenceCommand::AdaptersList { .. } => "adapters/list",
            InferenceCommand::AdapterApply { .. } => "adapter/apply",
            InferenceCommand::Generate { .. } => "generate",
        }
        .to_string()
    }

    fn execute_command(&self, command: InferenceCommand) -> ModelResult<Value> {
        match command {
            InferenceCommand::Ping => {
                // Simple health check - no provider access needed
                Ok(json!({
                    "status": "pong",
                    "worker": "inference-worker",
                    "version": "0.1.0"
                }))
            }

            InferenceCommand::ModelLoad { model_id, revision } => {
                let mut provider = self.provider.lock().map_err(|e| e.to_string())?;
                let info = provider.load(&model_id, revision.as_deref())?;
                Ok(serde_json::to_value(info).unwrap_or(json!({})))
            }

            InferenceCommand::ModelUnload { model_id } => {
                let mut provider = self.provider.lock().map_err(|e| e.to_string())?;
                provider.unload(&model_id)?;
                Ok(json!({
                    "model_id": model_id,
                    "status": "unloaded"
                }))
            }

            InferenceCommand::ModelsList => {
                let provider = self.provider.lock().map_err(|e| e.to_string())?;
                let models = provider.list();
                Ok(json!({
                    "models": models,
                    "count": models.len()
                }))
            }

            InferenceCommand::AdapterLoad {
                model_id,
                adapter_path,
                adapter_name,
            } => {
                let mut provider = self.provider.lock().map_err(|e| e.to_string())?;
                let info = provider.load_adapter(&model_id, &adapter_path, &adapter_name)?;
                Ok(serde_json::to_value(info).unwrap_or(json!({})))
            }

            InferenceCommand::AdapterUnload {
                model_id,
                adapter_name,
            } => {
                let mut provider = self.provider.lock().map_err(|e| e.to_string())?;
                provider.unload_adapter(&model_id, &adapter_name)?;
                Ok(json!({
                    "model_id": model_id,
                    "adapter_name": adapter_name,
                    "status": "unloaded"
                }))
            }

            InferenceCommand::AdaptersList { model_id } => {
                let provider = self.provider.lock().map_err(|e| e.to_string())?;
                let adapters = provider.list_adapters(&model_id);

                // Include detailed info about each adapter
                let adapter_details: Vec<Value> = adapters
                    .iter()
                    .map(|a| json!({
                        "name": a.name,
                        "path": a.path,
                        "status": a.status
                    }))
                    .collect();

                Ok(json!({
                    "model_id": model_id,
                    "adapters": adapter_details,
                    "count": adapters.len()
                }))
            }

            InferenceCommand::AdapterApply { model_id } => {
                let mut provider = self.provider.lock().map_err(|e| e.to_string())?;
                let result = provider.apply_adapters(&model_id)?;
                Ok(serde_json::to_value(result).unwrap_or(json!({})))
            }

            InferenceCommand::Generate {
                model_id,
                prompt,
                max_tokens,
                temperature,
                top_p,
                adapters,
            } => {
                let mut provider = self.provider.lock().map_err(|e| e.to_string())?;
                let response = provider.generate(GenerateRequest {
                    model_id,
                    prompt,
                    max_tokens,
                    temperature,
                    top_p,
                    adapters,
                })?;
                Ok(serde_json::to_value(response).unwrap_or(json!({})))
            }
        }
    }

    fn error_response(&self, request_id: &str, r#type: &str, error: &str) -> String {
        let response = JTAGResponse::error(
            request_id.to_string(),
            r#type.to_string(),
            json!(null),
            error.to_string(),
        );
        serde_json::to_string(&response)
            .unwrap_or_else(|_| format!(r#"{{"success":false,"error":"{}"}}"#, error))
    }
}

// ============================================================================
// Connection Handler
// ============================================================================

fn handle_connection<P: ModelProvider + AdapterManager + TextGenerator>(
    stream: UnixStream,
    worker: Arc<InferenceWorker<P>>,
) -> std::io::Result<()> {
    let mut reader = BufReader::new(&stream);
    let mut writer = stream.try_clone()?;

    loop {
        let mut line = String::new();
        let bytes = reader.read_line(&mut line)?;
        if bytes == 0 {
            break;
        }

        let response_json = worker.handle_request(line.trim());
        writeln!(writer, "{}", response_json)?;
        writer.flush()?;
    }

    Ok(())
}

// ============================================================================
// Main Entry Point
// ============================================================================

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <socket-path>", args[0]);
        eprintln!("Example: {} /tmp/jtag-inference.sock", args[0]);
        std::process::exit(1);
    }

    let socket_path = &args[1];

    // Remove socket if exists
    if fs::metadata(socket_path).is_ok() {
        fs::remove_file(socket_path)?;
    }

    println!("🦀 Candle Inference Worker starting...");
    println!("📡 Socket: {}", socket_path);

    // Initialize logger (connects to LoggerWorker)
    let logger = Arc::new(Mutex::new(LoggerClient::connect(
        "/tmp/jtag-logger-worker.sock",
        "InferenceWorker",
    )));

    if let Ok(mut log) = logger.lock() {
        log.info("Inference Worker initializing...");
    }

    // Initialize HuggingFace provider
    let provider = match HuggingFaceProvider::new(logger.clone()) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("❌ Failed to initialize provider: {}", e);
            if let Ok(mut log) = logger.lock() {
                log.error(&format!("Failed to initialize: {}", e));
            }
            std::process::exit(1);
        }
    };

    println!("🔧 Using device: {}", provider.device_info());

    // Create worker
    let worker = Arc::new(InferenceWorker::new(provider, logger.clone()));

    println!("✅ Inference Worker ready\n");

    if let Ok(mut log) = logger.lock() {
        log.info("Inference Worker ready");
    }

    // Bind socket
    let listener = UnixListener::bind(socket_path)?;
    println!("✅ Listening for connections\n");

    // Accept connections
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let worker_clone = worker.clone();
                thread::spawn(move || {
                    if let Err(e) = handle_connection(stream, worker_clone) {
                        eprintln!("Connection error: {}", e);
                    }
                });
            }
            Err(e) => eprintln!("Accept error: {}", e),
        }
    }

    Ok(())
}
