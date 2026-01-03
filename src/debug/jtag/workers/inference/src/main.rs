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

// HuggingFace Hub
use hf_hub::{api::sync::Api, Repo, RepoType};

// Tokenizers
use tokenizers::Tokenizer;

// Random sampling
use rand::{Rng, SeedableRng};

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
}

/// Trait for text generation
pub trait TextGenerator: Send + Sync {
    /// Generate text from a prompt
    fn generate(&mut self, request: GenerateRequest) -> ModelResult<GenerateResponse>;
}

// ============================================================================
// Data Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub model_id: String,
    pub status: String,
    pub load_time_ms: Option<u64>,
    pub device: String,
    pub loaded_at_seconds_ago: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    // Future: Llama, Mistral, etc.
}

// Manual Debug impl since PhiModel doesn't derive Debug
impl std::fmt::Debug for ModelArchitecture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModelArchitecture::Phi(_) => write!(f, "Phi"),
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

#[derive(Debug, Clone)]
pub struct LoadedAdapter {
    pub name: String,
    pub path: String,
    // TODO: Add actual LoRA weight tensors when implementing real inference
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateRequest {
    pub model_id: String,
    pub prompt: String,
    pub max_tokens: Option<usize>,
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    pub adapters: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
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

    /// Load Phi model architecture
    fn load_phi(&self, repo: &hf_hub::api::sync::ApiRepo, tokenizer: &Tokenizer, _model_id: &str) -> ModelResult<(ModelArchitecture, ModelConfig)> {
        // Download model weights
        let weights_path = repo
            .get("model.safetensors")
            .or_else(|_| repo.get("pytorch_model.bin"))
            .map_err(|e| format!("Failed to download model weights: {}", e))?;
        println!("📂 Weights: {:?}", weights_path);

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

        // Load model weights
        // Note: Memory-mapping (unsafe) is the canonical way to load large models efficiently:
        // - Only loads pages that are accessed (lazy loading)
        // - Uses OS page cache efficiently
        // - Doesn't duplicate memory (maps file directly to address space)
        // The safe alternative (from_buffered_safetensors) loads entire file to RAM first.
        println!("🔧 Loading weights to {:?}...", self.device);
        let vb = if weights_path.to_string_lossy().ends_with(".safetensors") {
            // SAFETY: File path is valid and we own the VarBuilder for its lifetime
            unsafe {
                VarBuilder::from_mmaped_safetensors(&[weights_path], DType::F32, &self.device)
                    .map_err(|e| format!("Failed to load safetensors: {}", e))?
            }
        } else {
            VarBuilder::from_pth(weights_path, DType::F32, &self.device)
                .map_err(|e| format!("Failed to load pytorch weights: {}", e))?
        };

        // Create Phi model
        let model = PhiModel::new(&phi_config, vb)
            .map_err(|e| format!("Failed to create Phi model: {}", e))?;
        println!("✅ Phi model loaded: vocab_size={}", vocab_size);

        let config = ModelConfig {
            vocab_size,
            eos_token_id,
            bos_token_id: None,
        };

        Ok((ModelArchitecture::Phi(model), config))
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

        // Determine model architecture and load weights
        let (architecture, config) = if model_id.to_lowercase().contains("phi") {
            self.load_phi(&repo, &tokenizer, &model_id)?
        } else {
            // Default to Phi architecture for unknown models
            // TODO: Add detection for Llama, Mistral, etc.
            println!("⚠️  Unknown architecture, trying Phi format for: {}", model_id);
            self.load_phi(&repo, &tokenizer, &model_id)?
        };

        let load_time_ms = load_start.elapsed().as_millis() as u64;

        self.models.insert(
            model_id.to_string(),
            LoadedModel {
                model_id: model_id.to_string(),
                architecture,
                tokenizer,
                config,
                device: self.device.clone(),
                loaded_at: Instant::now(),
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

        let adapter = LoadedAdapter {
            name: adapter_name.to_string(),
            path: adapter_path.to_string(),
        };

        self.adapters
            .entry(model_id.to_string())
            .or_insert_with(Vec::new)
            .push(adapter);

        println!("✅ Adapter loaded: {}", adapter_name);

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

        if let Ok(mut log) = self.logger.lock() {
            log.info(&format!(
                "Generating: {} prompt tokens, adapters: {:?}",
                prompt_len, adapters_used
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

                // Preallocate buffers - avoid reallocation during generation
                let total_capacity = prompt_len + max_tokens;
                let mut all_tokens: Box<Vec<u32>> = Box::new(Vec::with_capacity(total_capacity));
                all_tokens.extend_from_slice(&prompt_tokens);

                let mut gen_tokens: Box<Vec<u32>> = Box::new(Vec::with_capacity(max_tokens));

                // Reusable single-token buffer for subsequent forward passes
                let mut single_token_buf = [0u32; 1];

                for i in 0..max_tokens {
                    // On first iteration, pass full prompt via slice reference
                    // On subsequent iterations, pass single token (cache has the rest)
                    let input = if i == 0 {
                        Tensor::new(all_tokens.as_slice(), &device)
                            .map_err(|e| format!("Failed to create prompt tensor: {}", e))?
                    } else {
                        // Use fixed buffer - no allocation
                        single_token_buf[0] = *all_tokens.last().unwrap_or(&0);
                        Tensor::new(&single_token_buf[..], &device)
                            .map_err(|e| format!("Failed to create token tensor: {}", e))?
                    };

                    let input = input.unsqueeze(0)
                        .map_err(|e| format!("Failed to unsqueeze: {}", e))?;

                    // Forward pass
                    let logits = model
                        .forward(&input)
                        .map_err(|e| format!("Forward pass failed: {}", e))?;

                    // Debug: print logits shape on first iteration only
                    if i == 0 {
                        println!("🔍 Logits shape: {:?}", logits.shape());
                    }

                    // Extract last token logits - handle both 2D and 3D shapes
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

                    // Sample next token
                    let next_token = logits_processor
                        .sample(&last_logits)
                        .map_err(|e| format!("Sampling failed: {}", e))?;

                    // Check for EOS
                    if next_token == eos_token_id {
                        break;
                    }

                    all_tokens.push(next_token);
                    gen_tokens.push(next_token);
                }

                // Move out of Box to return owned Vec
                *gen_tokens
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
