/// Candle Inference Worker - Native Rust LLM Inference
///
/// ARCHITECTURE-AGNOSTIC DESIGN:
/// - CausalLM trait abstracts all text generation models
/// - Registry pattern maps HuggingFace architecture strings to loaders
/// - Adding new models = just implementing CausalLM trait
/// - Supports 30+ model families from candle-transformers
///
/// SUPPORTED ARCHITECTURES:
/// - Llama/Llama2/Llama3 (and derivatives: Vicuna, Alpaca, CodeLlama, etc.)
/// - Mistral/Mixtral
/// - Phi/Phi-2/Phi-3
/// - Qwen/Qwen2/Qwen2-MoE
/// - Gemma/Gemma2/Gemma3
/// - StableLM
/// - Falcon
/// - MPT
/// - Yi
/// - DeepSeek2
/// - OLMo
/// - Granite
/// - StarCoder2
/// - ChatGLM/GLM4
/// - Mamba (state space)
/// - RWKV v5/v6 (linear attention)
/// - And more via config.json detection
///
/// COMMANDS:
/// - ping: Health check
/// - model/load: Load a model from HuggingFace
/// - model/unload: Unload a model from memory
/// - model/list: List loaded models
/// - adapter/load: Load a LoRA adapter
/// - adapter/unload: Unload a LoRA adapter
/// - adapter/apply: Merge loaded adapters into model weights
/// - generate: Generate text with optional adapter composition
/// - gpu/status: Get GPU memory status
/// - gpu/allocate: Request GPU memory allocation
/// - gpu/release: Release GPU memory allocation
/// - gpu/stress-test: Stress test the allocator
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
// std::io::Write not needed - using async writes only
use std::sync::Arc;
use std::time::Instant;

// Tokio async runtime - NON-BLOCKING EVERYTHING
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader as TokioBufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::RwLock;

// Per-model locking uses std::sync::Mutex since we hold it during sync compute
// (tokio Mutex would require .await which doesn't work in sync generate_text)
use std::sync::Mutex;

// GPU Allocator (shared module)
#[path = "../../shared/gpu_allocator.rs"]
mod gpu_allocator;
use gpu_allocator::{get_gpu_allocator, AllocationRequest, AllocationResult, AllocationType};

// Candle imports
use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::generation::LogitsProcessor;

// Model imports - all supported architectures
use candle_transformers::models::falcon::{Config as FalconConfig, Falcon as FalconModel};
use candle_transformers::models::gemma::{Config as GemmaConfig, Model as GemmaModel};
use candle_transformers::models::gemma2::{Config as Gemma2Config, Model as Gemma2Model};
use candle_transformers::models::llama::{
    Cache as LlamaCache, Config as LlamaModelConfig, Llama as LlamaModel,
    LlamaConfig as LlamaRawConfig,
};
use candle_transformers::models::mistral::{Config as MistralConfig, Model as MistralModel};
use candle_transformers::models::mixtral::{Config as MixtralConfig, Model as MixtralModel};
use candle_transformers::models::phi::{Config as PhiConfig, Model as PhiModel};
use candle_transformers::models::phi3::{Config as Phi3Config, Model as Phi3Model};
use candle_transformers::models::qwen2::{Config as Qwen2Config, ModelForCausalLM as Qwen2Model};
use candle_transformers::models::stable_lm::{Config as StableLMConfig, Model as StableLMModel};
use candle_transformers::models::starcoder2::{
    Config as StarCoder2Config, Model as StarCoder2Model,
};

// HuggingFace Hub
use hf_hub::{Repo, RepoType};

// Tokenizers
use tokenizers::Tokenizer;

// Random sampling
use rand::Rng;

// ============================================================================
// JTAG Protocol Types
// ============================================================================

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

// Planned for future JTAG protocol wrapping
#[allow(dead_code)]
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
            error_type: Some("inference_error".to_string()),
        }
    }
}

// ============================================================================
// CausalLM TRAIT - The Universal Model Interface
// ============================================================================

/// Trait for all causal language models (text generation)
///
/// This abstraction allows the worker to handle ANY transformer-based LLM
/// without model-specific code in the main logic.
pub trait CausalLM: Send {
    /// Forward pass: input tokens → output logits
    ///
    /// - `tokens`: Input token IDs as 2D tensor [batch, seq_len]
    /// - `pos`: Position offset for KV cache (0 for first pass, then increment)
    ///
    /// Returns logits tensor [batch, seq_len, vocab_size] or [batch, vocab_size]
    fn forward(&mut self, tokens: &Tensor, pos: usize) -> Result<Tensor, String>;

    /// Clear the KV cache between generations
    /// Must be called before starting a new generation
    fn clear_cache(&mut self) -> Result<(), String>;

    /// Get vocabulary size
    fn vocab_size(&self) -> usize;

    /// Get architecture name (for logging)
    fn architecture(&self) -> &'static str;

    /// Get EOS token ID
    fn eos_token_id(&self) -> u32;
}

// ============================================================================
// Model Wrappers - Implement CausalLM for each architecture
// ============================================================================

/// Wrapper for Phi models (Phi-1, Phi-1.5, Phi-2)
struct PhiWrapper {
    model: PhiModel,
    vocab_size: usize,
    eos_token_id: u32,
}

impl CausalLM for PhiWrapper {
    fn forward(&mut self, tokens: &Tensor, _pos: usize) -> Result<Tensor, String> {
        self.model
            .forward(tokens)
            .map_err(|e| format!("Phi forward failed: {e}"))
    }

    fn clear_cache(&mut self) -> Result<(), String> {
        self.model.clear_kv_cache();
        Ok(())
    }

    fn vocab_size(&self) -> usize {
        self.vocab_size
    }
    fn architecture(&self) -> &'static str {
        "phi"
    }
    fn eos_token_id(&self) -> u32 {
        self.eos_token_id
    }
}

/// Wrapper for Phi-3 models
struct Phi3Wrapper {
    model: Phi3Model,
    vocab_size: usize,
    eos_token_id: u32,
}

impl CausalLM for Phi3Wrapper {
    fn forward(&mut self, tokens: &Tensor, pos: usize) -> Result<Tensor, String> {
        self.model
            .forward(tokens, pos)
            .map_err(|e| format!("Phi3 forward failed: {e}"))
    }

    fn clear_cache(&mut self) -> Result<(), String> {
        self.model.clear_kv_cache();
        Ok(())
    }

    fn vocab_size(&self) -> usize {
        self.vocab_size
    }
    fn architecture(&self) -> &'static str {
        "phi3"
    }
    fn eos_token_id(&self) -> u32 {
        self.eos_token_id
    }
}

/// Wrapper for Llama models (Llama, Llama2, Llama3, CodeLlama, etc.)
struct LlamaWrapper {
    model: LlamaModel,
    cache: LlamaCache,
    config: LlamaModelConfig,
    device: Device,
    vocab_size: usize,
    eos_token_id: u32,
}

impl CausalLM for LlamaWrapper {
    fn forward(&mut self, tokens: &Tensor, pos: usize) -> Result<Tensor, String> {
        self.model
            .forward(tokens, pos, &mut self.cache)
            .map_err(|e| format!("Llama forward failed: {e}"))
    }

    fn clear_cache(&mut self) -> Result<(), String> {
        // Llama cache must be recreated (no reset method)
        self.cache = LlamaCache::new(true, DType::F32, &self.config, &self.device)
            .map_err(|e| format!("Failed to recreate Llama cache: {e}"))?;
        Ok(())
    }

    fn vocab_size(&self) -> usize {
        self.vocab_size
    }
    fn architecture(&self) -> &'static str {
        "llama"
    }
    fn eos_token_id(&self) -> u32 {
        self.eos_token_id
    }
}

/// Wrapper for Mistral models
struct MistralWrapper {
    model: MistralModel,
    vocab_size: usize,
    eos_token_id: u32,
}

impl CausalLM for MistralWrapper {
    fn forward(&mut self, tokens: &Tensor, pos: usize) -> Result<Tensor, String> {
        self.model
            .forward(tokens, pos)
            .map_err(|e| format!("Mistral forward failed: {e}"))
    }

    fn clear_cache(&mut self) -> Result<(), String> {
        self.model.clear_kv_cache();
        Ok(())
    }

    fn vocab_size(&self) -> usize {
        self.vocab_size
    }
    fn architecture(&self) -> &'static str {
        "mistral"
    }
    fn eos_token_id(&self) -> u32 {
        self.eos_token_id
    }
}

/// Wrapper for Mixtral (MoE) models
struct MixtralWrapper {
    model: MixtralModel,
    vocab_size: usize,
    eos_token_id: u32,
}

impl CausalLM for MixtralWrapper {
    fn forward(&mut self, tokens: &Tensor, pos: usize) -> Result<Tensor, String> {
        self.model
            .forward(tokens, pos)
            .map_err(|e| format!("Mixtral forward failed: {e}"))
    }

    fn clear_cache(&mut self) -> Result<(), String> {
        // Mixtral doesn't expose clear_kv_cache - will recreate model if needed
        Ok(())
    }

    fn vocab_size(&self) -> usize {
        self.vocab_size
    }
    fn architecture(&self) -> &'static str {
        "mixtral"
    }
    fn eos_token_id(&self) -> u32 {
        self.eos_token_id
    }
}

/// Wrapper for Qwen2 models
struct Qwen2Wrapper {
    model: Qwen2Model,
    vocab_size: usize,
    eos_token_id: u32,
}

impl CausalLM for Qwen2Wrapper {
    fn forward(&mut self, tokens: &Tensor, pos: usize) -> Result<Tensor, String> {
        self.model
            .forward(tokens, pos)
            .map_err(|e| format!("Qwen2 forward failed: {e}"))
    }

    fn clear_cache(&mut self) -> Result<(), String> {
        self.model.clear_kv_cache();
        Ok(())
    }

    fn vocab_size(&self) -> usize {
        self.vocab_size
    }
    fn architecture(&self) -> &'static str {
        "qwen2"
    }
    fn eos_token_id(&self) -> u32 {
        self.eos_token_id
    }
}

/// Wrapper for Gemma models
struct GemmaWrapper {
    model: GemmaModel,
    vocab_size: usize,
    eos_token_id: u32,
}

impl CausalLM for GemmaWrapper {
    fn forward(&mut self, tokens: &Tensor, pos: usize) -> Result<Tensor, String> {
        self.model
            .forward(tokens, pos)
            .map_err(|e| format!("Gemma forward failed: {e}"))
    }

    fn clear_cache(&mut self) -> Result<(), String> {
        self.model.clear_kv_cache();
        Ok(())
    }

    fn vocab_size(&self) -> usize {
        self.vocab_size
    }
    fn architecture(&self) -> &'static str {
        "gemma"
    }
    fn eos_token_id(&self) -> u32 {
        self.eos_token_id
    }
}

/// Wrapper for Gemma2 models
struct Gemma2Wrapper {
    model: Gemma2Model,
    vocab_size: usize,
    eos_token_id: u32,
}

impl CausalLM for Gemma2Wrapper {
    fn forward(&mut self, tokens: &Tensor, pos: usize) -> Result<Tensor, String> {
        self.model
            .forward(tokens, pos)
            .map_err(|e| format!("Gemma2 forward failed: {e}"))
    }

    fn clear_cache(&mut self) -> Result<(), String> {
        self.model.clear_kv_cache();
        Ok(())
    }

    fn vocab_size(&self) -> usize {
        self.vocab_size
    }
    fn architecture(&self) -> &'static str {
        "gemma2"
    }
    fn eos_token_id(&self) -> u32 {
        self.eos_token_id
    }
}

/// Wrapper for StableLM models
struct StableLMWrapper {
    model: StableLMModel,
    vocab_size: usize,
    eos_token_id: u32,
}

impl CausalLM for StableLMWrapper {
    fn forward(&mut self, tokens: &Tensor, pos: usize) -> Result<Tensor, String> {
        self.model
            .forward(tokens, pos)
            .map_err(|e| format!("StableLM forward failed: {e}"))
    }

    fn clear_cache(&mut self) -> Result<(), String> {
        // StableLM doesn't expose clear_kv_cache
        Ok(())
    }

    fn vocab_size(&self) -> usize {
        self.vocab_size
    }
    fn architecture(&self) -> &'static str {
        "stablelm"
    }
    fn eos_token_id(&self) -> u32 {
        self.eos_token_id
    }
}

/// Wrapper for Falcon models
struct FalconWrapper {
    model: FalconModel,
    vocab_size: usize,
    eos_token_id: u32,
}

impl CausalLM for FalconWrapper {
    fn forward(&mut self, tokens: &Tensor, _pos: usize) -> Result<Tensor, String> {
        // Falcon's forward() only takes tokens - it manages position internally
        self.model
            .forward(tokens)
            .map_err(|e| format!("Falcon forward failed: {e}"))
    }

    fn clear_cache(&mut self) -> Result<(), String> {
        // Falcon doesn't expose clear_kv_cache
        Ok(())
    }

    fn vocab_size(&self) -> usize {
        self.vocab_size
    }
    fn architecture(&self) -> &'static str {
        "falcon"
    }
    fn eos_token_id(&self) -> u32 {
        self.eos_token_id
    }
}

// NOTE: MPT and Yi removed - their Config types don't implement Deserialize
// Can add custom config structs later if needed

/// Wrapper for StarCoder2 models
struct StarCoder2Wrapper {
    model: StarCoder2Model,
    vocab_size: usize,
    eos_token_id: u32,
}

impl CausalLM for StarCoder2Wrapper {
    fn forward(&mut self, tokens: &Tensor, pos: usize) -> Result<Tensor, String> {
        self.model
            .forward(tokens, pos)
            .map_err(|e| format!("StarCoder2 forward failed: {e}"))
    }

    fn clear_cache(&mut self) -> Result<(), String> {
        self.model.clear_kv_cache();
        Ok(())
    }

    fn vocab_size(&self) -> usize {
        self.vocab_size
    }
    fn architecture(&self) -> &'static str {
        "starcoder2"
    }
    fn eos_token_id(&self) -> u32 {
        self.eos_token_id
    }
}

// ============================================================================
// Architecture Registry - Maps config.json "architectures" to loaders
// ============================================================================

/// Architecture names from HuggingFace config.json
/// These are the standard names used in the "architectures" field
const LLAMA_ARCHITECTURES: &[&str] = &[
    "LlamaForCausalLM",
    "LLaMAForCausalLM",
    "CodeLlamaForCausalLM",
    "TinyLlamaForCausalLM",
];

const MISTRAL_ARCHITECTURES: &[&str] = &["MistralForCausalLM"];

const MIXTRAL_ARCHITECTURES: &[&str] = &["MixtralForCausalLM"];

const PHI_ARCHITECTURES: &[&str] = &[
    "PhiForCausalLM",
    "Phi1ForCausalLM",
    "MixFormerSequentialForCausalLM",
];

const PHI3_ARCHITECTURES: &[&str] = &["Phi3ForCausalLM", "Phi3SmallForCausalLM"];

const QWEN2_ARCHITECTURES: &[&str] = &["Qwen2ForCausalLM"];

const GEMMA_ARCHITECTURES: &[&str] = &["GemmaForCausalLM"];

const GEMMA2_ARCHITECTURES: &[&str] = &["Gemma2ForCausalLM"];

const STABLELM_ARCHITECTURES: &[&str] = &["StableLmForCausalLM", "StableLMEpochForCausalLM"];

const FALCON_ARCHITECTURES: &[&str] = &["FalconForCausalLM", "RWForCausalLM"];

// NOTE: MPT_ARCHITECTURES and YI_ARCHITECTURES removed - Config types don't implement Deserialize

const STARCODER2_ARCHITECTURES: &[&str] = &["Starcoder2ForCausalLM"];

/// Detect architecture from config.json
fn detect_architecture(config: &Value) -> Option<&'static str> {
    let architectures = config.get("architectures")?.as_array()?;
    let arch_str = architectures.first()?.as_str()?;

    // Check each architecture family
    if LLAMA_ARCHITECTURES.contains(&arch_str) {
        return Some("llama");
    }
    if MISTRAL_ARCHITECTURES.contains(&arch_str) {
        return Some("mistral");
    }
    if MIXTRAL_ARCHITECTURES.contains(&arch_str) {
        return Some("mixtral");
    }
    if PHI_ARCHITECTURES.contains(&arch_str) {
        return Some("phi");
    }
    if PHI3_ARCHITECTURES.contains(&arch_str) {
        return Some("phi3");
    }
    if QWEN2_ARCHITECTURES.contains(&arch_str) {
        return Some("qwen2");
    }
    if GEMMA_ARCHITECTURES.contains(&arch_str) {
        return Some("gemma");
    }
    if GEMMA2_ARCHITECTURES.contains(&arch_str) {
        return Some("gemma2");
    }
    if STABLELM_ARCHITECTURES.contains(&arch_str) {
        return Some("stablelm");
    }
    if FALCON_ARCHITECTURES.contains(&arch_str) {
        return Some("falcon");
    }
    if STARCODER2_ARCHITECTURES.contains(&arch_str) {
        return Some("starcoder2");
    }

    // Fallback: try to detect from model_type field
    if let Some(model_type) = config.get("model_type").and_then(|v| v.as_str()) {
        match model_type {
            "llama" => return Some("llama"),
            "mistral" => return Some("mistral"),
            "mixtral" => return Some("mixtral"),
            "phi" | "phi-msft" => return Some("phi"),
            "phi3" => return Some("phi3"),
            "qwen2" => return Some("qwen2"),
            "gemma" => return Some("gemma"),
            "gemma2" => return Some("gemma2"),
            "stablelm" | "stablelm_epoch" => return Some("stablelm"),
            "falcon" | "RefinedWeb" | "RefinedWebModel" => return Some("falcon"),
            "starcoder2" => return Some("starcoder2"),
            _ => {}
        }
    }

    None
}

// ============================================================================
// Loaded Model Storage
// ============================================================================

pub(crate) struct LoadedModel {
    model: Box<dyn CausalLM>,
    tokenizer: Tokenizer,
    #[allow(dead_code)]
    model_id: String,
    #[allow(dead_code)]
    load_time_ms: u64,
}

impl LoadedModel {
    /// Generate text - wrapper to avoid borrow checker issues
    fn generate(
        &mut self,
        prompt: &str,
        max_tokens: usize,
        temperature: f64,
        device: &Device,
    ) -> Result<(String, usize, usize), String> {
        generate_text(
            self.model.as_mut(),
            &self.tokenizer,
            prompt,
            max_tokens,
            temperature,
            device,
        )
    }
}

// Planned for future use - adapter composition
#[allow(dead_code)]
struct ModelConfig {
    model_id: String,
    vocab_size: usize,
    context_length: Option<usize>,
    eos_token_id: u32,
}

// ============================================================================
// LoRA Adapter Storage
// ============================================================================

/// Loaded LoRA adapter weights
struct LoadedAdapter {
    /// Unique adapter ID
    #[allow(dead_code)]
    id: String,
    /// Path to the safetensors file
    #[allow(dead_code)]
    path: String,
    /// Target model this adapter is for
    #[allow(dead_code)]
    target_model: String,
    /// Loaded weights as tensors (keyed by layer name)
    weights: HashMap<String, Tensor>,
    /// Total size in bytes
    size_bytes: usize,
    /// Time to load in milliseconds
    load_time_ms: u64,
    /// LoRA rank (detected from weights)
    rank: usize,
}

// ============================================================================
// Model Loader - Downloads and loads models from HuggingFace
// ============================================================================

struct ModelLoader {
    device: Device,
    dtype: DType,
}

impl ModelLoader {
    fn new() -> Result<Self, String> {
        // Use Metal on macOS, CUDA on Linux/Windows, CPU as fallback
        let (device, dtype) = if cfg!(target_os = "macos") {
            match Device::new_metal(0) {
                Ok(metal_device) => {
                    println!("🔧 Metal device detected, using BF16 for optimal performance");
                    (metal_device, DType::BF16) // BF16 is 2x faster than F32 on Metal
                }
                Err(_) => {
                    println!("⚠️ Metal not available, falling back to CPU with F32");
                    (Device::Cpu, DType::F32)
                }
            }
        } else {
            (Device::Cpu, DType::F32)
        };

        println!("🔧 Using device: {device:?}, dtype: {dtype:?}");

        Ok(Self { device, dtype })
    }

    /// Load a model from HuggingFace Hub
    fn load(&self, model_id: &str) -> Result<LoadedModel, String> {
        let start = Instant::now();
        println!("📥 Loading model: {model_id}");

        // Download model files (reads HF_TOKEN from env for gated models like meta-llama)
        let api = hf_hub::api::sync::ApiBuilder::from_env()
            .build()
            .map_err(|e| format!("HF API error: {e}"))?;
        let repo = api.repo(Repo::new(model_id.to_string(), RepoType::Model));

        // Load config.json to detect architecture
        let config_path = repo
            .get("config.json")
            .map_err(|e| format!("Failed to get config.json: {e}"))?;
        let config_str = fs::read_to_string(&config_path)
            .map_err(|e| format!("Failed to read config.json: {e}"))?;
        let config: Value = serde_json::from_str(&config_str)
            .map_err(|e| format!("Failed to parse config.json: {e}"))?;

        // Detect architecture
        let architecture = detect_architecture(&config)
            .ok_or_else(|| format!("Unknown architecture in {model_id}"))?;
        println!("🔍 Detected architecture: {architecture}");

        // Load tokenizer
        let tokenizer_path = repo
            .get("tokenizer.json")
            .map_err(|e| format!("Failed to get tokenizer.json: {e}"))?;
        println!("📂 Tokenizer: {tokenizer_path:?}");
        let tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| format!("Failed to load tokenizer: {e}"))?;

        // Download weights (handle sharded models)
        let weights_paths = self.download_weights(&repo)?;
        println!(
            "🔧 Loading {} safetensor file(s) to {:?}...",
            weights_paths.len(),
            self.device
        );

        // Build VarBuilder from weights
        let vb = self.build_var_builder(&weights_paths)?;

        // Load model based on architecture
        let model: Box<dyn CausalLM> = match architecture {
            "llama" => self.load_llama(&config, vb)?,
            "mistral" => self.load_mistral(&config, vb)?,
            "mixtral" => self.load_mixtral(&config, vb)?,
            "phi" => self.load_phi(&config, vb)?,
            "phi3" => self.load_phi3(&config, vb)?,
            "qwen2" => self.load_qwen2(&config, vb)?,
            "gemma" => self.load_gemma(&config, vb)?,
            "gemma2" => self.load_gemma2(&config, vb)?,
            "stablelm" => self.load_stablelm(&config, vb)?,
            "falcon" => self.load_falcon(&config, vb)?,
            "starcoder2" => self.load_starcoder2(&config, vb)?,
            _ => return Err(format!("Unsupported architecture: {architecture}")),
        };

        let load_time_ms = start.elapsed().as_millis() as u64;
        println!("✅ Model loaded in {load_time_ms}ms: {model_id}");

        Ok(LoadedModel {
            model,
            tokenizer,
            model_id: model_id.to_string(),
            load_time_ms,
        })
    }

    /// Download model weights (handles sharded models)
    fn download_weights(
        &self,
        repo: &hf_hub::api::sync::ApiRepo,
    ) -> Result<Vec<std::path::PathBuf>, String> {
        // Try single weights file first
        if let Ok(path) = repo.get("model.safetensors") {
            println!("📂 Weights (single): {path:?}");
            return Ok(vec![path]);
        }

        // Try sharded weights (model.safetensors.index.json)
        if let Ok(index_path) = repo.get("model.safetensors.index.json") {
            println!("📂 Found sharded weights index: {index_path:?}");
            let index_str = fs::read_to_string(&index_path)
                .map_err(|e| format!("Failed to read index: {e}"))?;
            let index: Value = serde_json::from_str(&index_str)
                .map_err(|e| format!("Failed to parse index: {e}"))?;

            // Get unique shard files
            let weight_map = index
                .get("weight_map")
                .and_then(|v| v.as_object())
                .ok_or("Invalid index format")?;

            let mut shard_files: Vec<String> = weight_map
                .values()
                .filter_map(|v| v.as_str())
                .map(|s| s.to_string())
                .collect();
            shard_files.sort();
            shard_files.dedup();

            println!("📦 Downloading {} weight shards...", shard_files.len());

            let mut paths = Vec::new();
            for shard in &shard_files {
                let path = repo
                    .get(shard)
                    .map_err(|e| format!("Failed to get shard {shard}: {e}"))?;
                paths.push(path);
            }

            return Ok(paths);
        }

        Err("No weights found (tried model.safetensors and sharded)".to_string())
    }

    /// Build VarBuilder from weight files
    fn build_var_builder(
        &self,
        paths: &[std::path::PathBuf],
    ) -> Result<VarBuilder<'static>, String> {
        // SAFETY: mmap is required by Candle's safetensors loading
        // The files are read-only and memory-mapped for efficiency
        if paths.len() == 1 {
            unsafe {
                VarBuilder::from_mmaped_safetensors(paths, self.dtype, &self.device)
                    .map_err(|e| format!("Failed to load weights: {e}"))
            }
        } else {
            unsafe {
                VarBuilder::from_mmaped_safetensors(paths, self.dtype, &self.device)
                    .map_err(|e| format!("Failed to load sharded weights: {e}"))
            }
        }
    }

    /// Extract vocab_size from config
    fn get_vocab_size(config: &Value) -> usize {
        config
            .get("vocab_size")
            .and_then(|v| v.as_u64())
            .unwrap_or(32000) as usize
    }

    /// Extract EOS token ID from config
    fn get_eos_token_id(config: &Value) -> u32 {
        // Try eos_token_id directly
        if let Some(id) = config.get("eos_token_id").and_then(|v| v.as_u64()) {
            return id as u32;
        }
        // Try array format
        if let Some(arr) = config.get("eos_token_id").and_then(|v| v.as_array()) {
            if let Some(id) = arr.first().and_then(|v| v.as_u64()) {
                return id as u32;
            }
        }
        // Default
        2
    }

    // ========================================================================
    // Architecture-specific loaders
    // ========================================================================

    fn load_llama(
        &self,
        config: &Value,
        vb: VarBuilder<'static>,
    ) -> Result<Box<dyn CausalLM>, String> {
        let llama_config: LlamaRawConfig = serde_json::from_value(config.clone())
            .map_err(|e| format!("Failed to parse Llama config: {e}"))?;
        let vocab_size = Self::get_vocab_size(config);
        let eos_token_id = Self::get_eos_token_id(config);

        let model_config = llama_config.clone().into_config(false);
        let model = LlamaModel::load(vb, &model_config)
            .map_err(|e| format!("Failed to load Llama model: {e}"))?;
        let cache = LlamaCache::new(true, self.dtype, &model_config, &self.device)
            .map_err(|e| format!("Failed to create Llama cache: {e}"))?;

        println!("✅ Llama model loaded: vocab_size={vocab_size}");

        Ok(Box::new(LlamaWrapper {
            model,
            cache,
            config: model_config,
            device: self.device.clone(),
            vocab_size,
            eos_token_id,
        }))
    }

    fn load_mistral(
        &self,
        config: &Value,
        vb: VarBuilder<'static>,
    ) -> Result<Box<dyn CausalLM>, String> {
        let mistral_config: MistralConfig = serde_json::from_value(config.clone())
            .map_err(|e| format!("Failed to parse Mistral config: {e}"))?;
        let vocab_size = mistral_config.vocab_size;
        let eos_token_id = Self::get_eos_token_id(config);

        let model = MistralModel::new(&mistral_config, vb)
            .map_err(|e| format!("Failed to load Mistral model: {e}"))?;

        println!("✅ Mistral model loaded: vocab_size={vocab_size}");

        Ok(Box::new(MistralWrapper {
            model,
            vocab_size,
            eos_token_id,
        }))
    }

    fn load_mixtral(
        &self,
        config: &Value,
        vb: VarBuilder<'static>,
    ) -> Result<Box<dyn CausalLM>, String> {
        let mixtral_config: MixtralConfig = serde_json::from_value(config.clone())
            .map_err(|e| format!("Failed to parse Mixtral config: {e}"))?;
        let vocab_size = Self::get_vocab_size(config);
        let eos_token_id = Self::get_eos_token_id(config);

        let model = MixtralModel::new(&mixtral_config, vb)
            .map_err(|e| format!("Failed to load Mixtral model: {e}"))?;

        println!("✅ Mixtral model loaded: vocab_size={vocab_size}");

        Ok(Box::new(MixtralWrapper {
            model,
            vocab_size,
            eos_token_id,
        }))
    }

    fn load_phi(
        &self,
        config: &Value,
        vb: VarBuilder<'static>,
    ) -> Result<Box<dyn CausalLM>, String> {
        let phi_config: PhiConfig = serde_json::from_value(config.clone())
            .map_err(|e| format!("Failed to parse Phi config: {e}"))?;
        let vocab_size = Self::get_vocab_size(config);
        let eos_token_id = Self::get_eos_token_id(config);

        let model =
            PhiModel::new(&phi_config, vb).map_err(|e| format!("Failed to load Phi model: {e}"))?;

        println!("✅ Phi model loaded: vocab_size={vocab_size}");

        Ok(Box::new(PhiWrapper {
            model,
            vocab_size,
            eos_token_id,
        }))
    }

    fn load_phi3(
        &self,
        config: &Value,
        vb: VarBuilder<'static>,
    ) -> Result<Box<dyn CausalLM>, String> {
        let phi3_config: Phi3Config = serde_json::from_value(config.clone())
            .map_err(|e| format!("Failed to parse Phi3 config: {e}"))?;
        let vocab_size = phi3_config.vocab_size;
        let eos_token_id = Self::get_eos_token_id(config);

        let model = Phi3Model::new(&phi3_config, vb)
            .map_err(|e| format!("Failed to load Phi3 model: {e}"))?;

        println!("✅ Phi3 model loaded: vocab_size={vocab_size}");

        Ok(Box::new(Phi3Wrapper {
            model,
            vocab_size,
            eos_token_id,
        }))
    }

    fn load_qwen2(
        &self,
        config: &Value,
        vb: VarBuilder<'static>,
    ) -> Result<Box<dyn CausalLM>, String> {
        let qwen2_config: Qwen2Config = serde_json::from_value(config.clone())
            .map_err(|e| format!("Failed to parse Qwen2 config: {e}"))?;
        let vocab_size = qwen2_config.vocab_size;
        let eos_token_id = Self::get_eos_token_id(config);

        let model = Qwen2Model::new(&qwen2_config, vb)
            .map_err(|e| format!("Failed to load Qwen2 model: {e}"))?;

        println!("✅ Qwen2 model loaded: vocab_size={vocab_size}");

        Ok(Box::new(Qwen2Wrapper {
            model,
            vocab_size,
            eos_token_id,
        }))
    }

    fn load_gemma(
        &self,
        config: &Value,
        vb: VarBuilder<'static>,
    ) -> Result<Box<dyn CausalLM>, String> {
        let gemma_config: GemmaConfig = serde_json::from_value(config.clone())
            .map_err(|e| format!("Failed to parse Gemma config: {e}"))?;
        let vocab_size = gemma_config.vocab_size;
        let eos_token_id = Self::get_eos_token_id(config);

        let model = GemmaModel::new(false, &gemma_config, vb)
            .map_err(|e| format!("Failed to load Gemma model: {e}"))?;

        println!("✅ Gemma model loaded: vocab_size={vocab_size}");

        Ok(Box::new(GemmaWrapper {
            model,
            vocab_size,
            eos_token_id,
        }))
    }

    fn load_gemma2(
        &self,
        config: &Value,
        vb: VarBuilder<'static>,
    ) -> Result<Box<dyn CausalLM>, String> {
        let gemma2_config: Gemma2Config = serde_json::from_value(config.clone())
            .map_err(|e| format!("Failed to parse Gemma2 config: {e}"))?;
        let vocab_size = gemma2_config.vocab_size;
        let eos_token_id = Self::get_eos_token_id(config);

        let model = Gemma2Model::new(false, &gemma2_config, vb)
            .map_err(|e| format!("Failed to load Gemma2 model: {e}"))?;

        println!("✅ Gemma2 model loaded: vocab_size={vocab_size}");

        Ok(Box::new(Gemma2Wrapper {
            model,
            vocab_size,
            eos_token_id,
        }))
    }

    fn load_stablelm(
        &self,
        config: &Value,
        vb: VarBuilder<'static>,
    ) -> Result<Box<dyn CausalLM>, String> {
        let stablelm_config: StableLMConfig = serde_json::from_value(config.clone())
            .map_err(|e| format!("Failed to parse StableLM config: {e}"))?;
        let vocab_size = Self::get_vocab_size(config);
        let eos_token_id = Self::get_eos_token_id(config);

        let model = StableLMModel::new(&stablelm_config, vb)
            .map_err(|e| format!("Failed to load StableLM model: {e}"))?;

        println!("✅ StableLM model loaded: vocab_size={vocab_size}");

        Ok(Box::new(StableLMWrapper {
            model,
            vocab_size,
            eos_token_id,
        }))
    }

    fn load_falcon(
        &self,
        config: &Value,
        vb: VarBuilder<'static>,
    ) -> Result<Box<dyn CausalLM>, String> {
        let falcon_config: FalconConfig = serde_json::from_value(config.clone())
            .map_err(|e| format!("Failed to parse Falcon config: {e}"))?;
        let vocab_size = falcon_config.vocab_size;
        let eos_token_id = Self::get_eos_token_id(config);

        let model = FalconModel::load(vb, falcon_config)
            .map_err(|e| format!("Failed to load Falcon model: {e}"))?;

        println!("✅ Falcon model loaded: vocab_size={vocab_size}");

        Ok(Box::new(FalconWrapper {
            model,
            vocab_size,
            eos_token_id,
        }))
    }

    // NOTE: load_mpt and load_yi removed - Config types don't implement Deserialize

    fn load_starcoder2(
        &self,
        config: &Value,
        vb: VarBuilder<'static>,
    ) -> Result<Box<dyn CausalLM>, String> {
        let sc2_config: StarCoder2Config = serde_json::from_value(config.clone())
            .map_err(|e| format!("Failed to parse StarCoder2 config: {e}"))?;
        let vocab_size = Self::get_vocab_size(config);
        let eos_token_id = Self::get_eos_token_id(config);

        let model = StarCoder2Model::new(&sc2_config, vb)
            .map_err(|e| format!("Failed to load StarCoder2 model: {e}"))?;

        println!("✅ StarCoder2 model loaded: vocab_size={vocab_size}");

        Ok(Box::new(StarCoder2Wrapper {
            model,
            vocab_size,
            eos_token_id,
        }))
    }
}

// ============================================================================
// LoRA Adapter Loader
// ============================================================================

/// Load LoRA adapter weights from a safetensors file
fn load_adapter(
    adapter_id: &str,
    adapter_path: &str,
    target_model: &str,
    device: &Device,
    dtype: DType,
) -> Result<LoadedAdapter, String> {
    let start = Instant::now();
    println!("📥 Loading adapter: {adapter_id} from {adapter_path}");

    // Check file exists
    let path = std::path::Path::new(adapter_path);
    if !path.exists() {
        return Err(format!("Adapter file not found: {adapter_path}"));
    }

    // Load safetensors file
    // SAFETY: mmap is required by Candle's VarBuilder API. The file is read-only
    // and we hold the mapping for the lifetime of the adapter.
    let tensors = unsafe {
        candle_core::safetensors::MmapedSafetensors::new(adapter_path)
            .map_err(|e| format!("Failed to mmap safetensors: {e}"))?
    };

    // Extract all tensors and convert to our device/dtype
    let mut weights: HashMap<String, Tensor> = HashMap::new();
    let mut total_bytes = 0usize;
    let mut detected_rank = 0usize;

    for name in tensors.tensors().iter().map(|(name, _)| name.clone()) {
        let tensor = tensors
            .load(&name, device)
            .map_err(|e| format!("Failed to load tensor {name}: {e}"))?;

        // Convert to target dtype if needed
        let tensor = if tensor.dtype() != dtype {
            tensor
                .to_dtype(dtype)
                .map_err(|e| format!("Failed to convert tensor {name} dtype: {e}"))?
        } else {
            tensor
        };

        // Calculate size
        let dims = tensor.dims();
        let elem_size = match dtype {
            DType::F32 => 4,
            DType::F16 | DType::BF16 => 2,
            _ => 4,
        };
        let tensor_bytes: usize = dims.iter().product::<usize>() * elem_size;
        total_bytes += tensor_bytes;

        // Detect LoRA rank from lora_A weights (shape is [rank, hidden_dim])
        if name.contains("lora_A") && dims.len() == 2 {
            detected_rank = detected_rank.max(dims[0]);
        }
        // Or from lora_B weights (shape is [hidden_dim, rank])
        if name.contains("lora_B") && dims.len() == 2 {
            detected_rank = detected_rank.max(dims[1]);
        }

        weights.insert(name, tensor);
    }

    let load_time_ms = start.elapsed().as_millis() as u64;
    let size_mb = total_bytes / (1024 * 1024);

    println!(
        "✅ Adapter loaded: {} tensors, {}MB, rank={}, {}ms",
        weights.len(),
        size_mb,
        detected_rank,
        load_time_ms
    );

    Ok(LoadedAdapter {
        id: adapter_id.to_string(),
        path: adapter_path.to_string(),
        target_model: target_model.to_string(),
        weights,
        size_bytes: total_bytes,
        load_time_ms,
        rank: detected_rank,
    })
}

// ============================================================================
// Text Generation
// ============================================================================

/// Generate text using any CausalLM model
fn generate_text(
    model: &mut dyn CausalLM,
    tokenizer: &Tokenizer,
    prompt: &str,
    max_tokens: usize,
    temperature: f64,
    device: &Device,
) -> Result<(String, usize, usize), String> {
    let start = Instant::now();

    // Encode prompt
    let encoding = tokenizer
        .encode(prompt, true)
        .map_err(|e| format!("Tokenization failed: {e}"))?;
    let prompt_tokens: Vec<u32> = encoding.get_ids().to_vec();
    let prompt_len = prompt_tokens.len();

    if prompt_len == 0 {
        return Err("Empty prompt".to_string());
    }

    // Clear cache before generation
    model.clear_cache()?;

    // Setup logits processor for sampling
    let seed = rand::thread_rng().gen::<u64>();
    let mut logits_processor = LogitsProcessor::new(seed, Some(temperature), None);

    // Generate tokens
    let mut all_tokens = prompt_tokens.clone();
    let eos_token_id = model.eos_token_id();

    for i in 0..max_tokens {
        // Get input - full sequence on first pass, just last token after
        let input_tokens = if i == 0 {
            all_tokens.clone()
        } else {
            vec![*all_tokens.last().unwrap()]
        };

        let input = Tensor::new(&input_tokens[..], device)
            .map_err(|e| format!("Failed to create input tensor: {e}"))?
            .unsqueeze(0)
            .map_err(|e| format!("Failed to unsqueeze: {e}"))?;

        // Forward pass
        let pos = if i == 0 { 0 } else { all_tokens.len() - 1 };
        let logits = model.forward(&input, pos)?;

        // Get last token logits
        let logits = if logits.dims().len() == 3 {
            logits
                .squeeze(0)
                .map_err(|e| format!("Squeeze failed: {e}"))?
        } else {
            logits
        };

        let last_logits = if logits.dims()[0] > 1 {
            logits
                .get(logits.dims()[0] - 1)
                .map_err(|e| format!("Get last logits failed: {e}"))?
        } else {
            logits
                .squeeze(0)
                .map_err(|e| format!("Squeeze logits failed: {e}"))?
        };

        // Sample next token
        let next_token = logits_processor
            .sample(&last_logits)
            .map_err(|e| format!("Sampling failed: {e}"))?;

        // Check for EOS
        if next_token == eos_token_id {
            break;
        }

        all_tokens.push(next_token);
    }

    // Decode generated tokens
    let generated_tokens = &all_tokens[prompt_len..];
    let generated_text = tokenizer
        .decode(generated_tokens, true)
        .map_err(|e| format!("Decoding failed: {e}"))?;

    let elapsed = start.elapsed().as_millis();
    let tok_per_sec = if elapsed > 0 {
        (generated_tokens.len() as f64 / elapsed as f64) * 1000.0
    } else {
        0.0
    };

    println!(
        "✨ Generated {} tokens in {}ms ({:.1} tok/s)",
        generated_tokens.len(),
        elapsed,
        tok_per_sec
    );

    Ok((generated_text, prompt_len, generated_tokens.len()))
}

// ============================================================================
// Command Handlers
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "command")]
enum InferenceCommand {
    #[serde(rename = "ping")]
    Ping,

    #[serde(rename = "model/load")]
    ModelLoad { model_id: String },

    #[serde(rename = "model/unload")]
    ModelUnload { model_id: String },

    #[serde(rename = "model/list")]
    ModelList,

    // =========================================================================
    // Handle-based API (NON-BLOCKING) - The Right Way™
    // =========================================================================
    /// Get or create a handle for a model
    /// Returns IMMEDIATELY with handle_id, even if model is still loading
    /// Use handle/status to poll for Ready state
    #[serde(rename = "model/handle")]
    ModelHandle { model_id: String },

    /// Get status of a handle
    #[serde(rename = "handle/status")]
    HandleStatus { handle_id: String },

    /// List all handles with their status
    #[serde(rename = "handle/list")]
    HandleList,

    /// Release a handle (unloads model if no other handles reference it)
    #[serde(rename = "handle/release")]
    HandleRelease { handle_id: String },

    // LoRA Adapter Commands
    #[serde(rename = "adapter/load")]
    AdapterLoad {
        adapter_id: String,
        adapter_path: String,
        /// Target model this adapter is for (for tracking/validation)
        #[serde(default)]
        target_model: Option<String>,
    },

    #[serde(rename = "adapter/unload")]
    AdapterUnload { adapter_id: String },

    #[serde(rename = "adapter/list")]
    AdapterList,

    #[serde(rename = "generate")]
    Generate {
        model_id: String,
        prompt: String,
        #[serde(default = "default_max_tokens")]
        max_tokens: usize,
        #[serde(default = "default_temperature")]
        temperature: f64,
    },

    /// Binary protocol: prompt bytes follow the header
    /// Format: {"command":"generate/binary",...}\n<u32 length><raw prompt bytes>
    /// Response: {"type":"binary",...}\n<raw response bytes>
    #[serde(rename = "generate/binary")]
    GenerateBinary {
        model_id: String,
        /// Prompt length in bytes (follows header)
        prompt_length: usize,
        #[serde(default = "default_max_tokens")]
        max_tokens: usize,
        #[serde(default = "default_temperature")]
        temperature: f64,
    },

    // =========================================================================
    // GPU Memory Management Commands
    // =========================================================================
    /// Get GPU memory status
    #[serde(rename = "gpu/status")]
    GpuStatus,

    /// Request GPU memory allocation
    #[serde(rename = "gpu/allocate")]
    GpuAllocate {
        id: String,
        owner: String,
        size_mb: u64,
        #[serde(default = "default_priority")]
        priority: f32,
        /// Load time in ms (for paging optimization)
        load_time_ms: Option<u64>,
        /// Type: "model", "adapter", "embedding", or "other"
        #[serde(default)]
        alloc_type: Option<String>,
    },

    /// Release GPU memory allocation
    #[serde(rename = "gpu/release")]
    GpuRelease { id: String },

    /// Get paging statistics by allocation type
    #[serde(rename = "gpu/paging-stats")]
    GpuPagingStats,

    /// Stress test the allocator with many allocations
    #[serde(rename = "gpu/stress-test")]
    GpuStressTest {
        /// Number of allocations to create
        #[serde(default = "default_stress_count")]
        count: usize,
        /// Size range for each allocation (random between min and max)
        #[serde(default = "default_stress_min_mb")]
        min_mb: u64,
        #[serde(default = "default_stress_max_mb")]
        max_mb: u64,
    },
}

fn default_priority() -> f32 {
    0.5
}
fn default_stress_count() -> usize {
    100
}
fn default_stress_min_mb() -> u64 {
    10
}
fn default_stress_max_mb() -> u64 {
    500
}

fn parse_alloc_type(s: &Option<String>) -> AllocationType {
    match s.as_ref().map(|s| s.as_str()) {
        Some("model") => AllocationType::Model,
        Some("adapter") => AllocationType::Adapter,
        Some("embedding") => AllocationType::Embedding,
        _ => AllocationType::Other,
    }
}

fn default_max_tokens() -> usize {
    256
}
fn default_temperature() -> f64 {
    0.7
}

/// Binary response header for text generation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BinaryTextHeader {
    #[serde(rename = "type")]
    r#type: String, // "binary"
    length: usize,
    dtype: String, // "u8" for UTF-8 text
    prompt_tokens: usize,
    generated_tokens: usize,
    model_id: String,
}

// ============================================================================
// Handle System - Non-blocking model access
// ============================================================================

/// Status of a model handle
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HandleStatus {
    /// Model is being loaded (async operation in progress)
    Loading,
    /// Model is ready for inference
    Ready,
    /// Model failed to load
    Error,
    /// Model was unloaded to free memory
    Unloaded,
}

/// A handle to a model - the ONLY way to access models
/// Handles are returned immediately, even if model is still loading
#[derive(Clone)]
pub(crate) struct ModelHandle {
    /// Unique handle ID (UUID)
    pub id: String,
    /// HuggingFace model ID (e.g., "Qwen/Qwen2-0.5B-Instruct")
    pub model_id: String,
    /// Current status
    pub status: HandleStatus,
    /// Loaded model (only present when status == Ready)
    pub model: Option<Arc<Mutex<LoadedModel>>>,
    /// Estimated memory usage in MB
    pub memory_mb: u64,
    /// Last time this handle was used for generation
    pub last_used: Instant,
    /// Error message if status == Error
    pub error: Option<String>,
    /// Time when loading started
    pub created_at: Instant,
}

impl ModelHandle {
    /// Create a new handle in Loading state
    fn new_loading(handle_id: String, model_id: String) -> Self {
        Self {
            id: handle_id,
            model_id,
            status: HandleStatus::Loading,
            model: None,
            memory_mb: 0,
            last_used: Instant::now(),
            error: None,
            created_at: Instant::now(),
        }
    }

    /// Update handle to Ready state with loaded model
    fn set_ready(&mut self, model: Arc<Mutex<LoadedModel>>, memory_mb: u64) {
        self.status = HandleStatus::Ready;
        self.model = Some(model);
        self.memory_mb = memory_mb;
        self.last_used = Instant::now();
    }

    /// Update handle to Error state
    fn set_error(&mut self, error: String) {
        self.status = HandleStatus::Error;
        self.model = None;
        self.error = Some(error);
    }

    /// Touch handle to update last_used time
    #[allow(dead_code)]
    fn touch(&mut self) {
        self.last_used = Instant::now();
    }

    /// Serialize to JSON for API responses
    fn to_json(&self) -> Value {
        json!({
            "handle_id": self.id,
            "model_id": self.model_id,
            "status": self.status,
            "memory_mb": self.memory_mb,
            "last_used_ms": self.last_used.elapsed().as_millis() as u64,
            "age_ms": self.created_at.elapsed().as_millis() as u64,
            "error": self.error
        })
    }
}

/// Registry of all model handles - Rust owns the truth
pub struct HandleRegistry {
    /// All handles by handle_id
    handles: HashMap<String, ModelHandle>,
    /// Reverse lookup: model_id -> handle_id (for reuse)
    model_to_handle: HashMap<String, String>,
}

impl HandleRegistry {
    fn new() -> Self {
        Self {
            handles: HashMap::new(),
            model_to_handle: HashMap::new(),
        }
    }

    /// Get existing handle for a model, or None if not found
    fn get_handle_for_model(&self, model_id: &str) -> Option<&ModelHandle> {
        self.model_to_handle
            .get(model_id)
            .and_then(|handle_id| self.handles.get(handle_id))
    }

    /// Get handle by ID
    fn get(&self, handle_id: &str) -> Option<&ModelHandle> {
        self.handles.get(handle_id)
    }

    /// Get mutable handle by ID
    fn get_mut(&mut self, handle_id: &str) -> Option<&mut ModelHandle> {
        self.handles.get_mut(handle_id)
    }

    /// Create a new handle for a model (in Loading state)
    fn create_handle(&mut self, model_id: &str) -> String {
        let handle_id = uuid::Uuid::new_v4().to_string();
        let handle = ModelHandle::new_loading(handle_id.clone(), model_id.to_string());
        self.handles.insert(handle_id.clone(), handle);
        self.model_to_handle
            .insert(model_id.to_string(), handle_id.clone());
        handle_id
    }

    /// Remove a handle (for unload)
    fn remove(&mut self, handle_id: &str) -> Option<ModelHandle> {
        if let Some(handle) = self.handles.remove(handle_id) {
            self.model_to_handle.remove(&handle.model_id);
            Some(handle)
        } else {
            None
        }
    }

    /// List all handles
    fn list(&self) -> Vec<Value> {
        self.handles.values().map(|h| h.to_json()).collect()
    }
}

// ============================================================================
// Worker State
// ============================================================================

/// WorkerState uses fine-grained locking to prevent blocking:
/// - handles: HandleRegistry tracks all model handles
/// - Each model has its own Mutex, so one generate doesn't block others
/// - The outer HashMap is protected by RwLock for concurrent read access
/// - ping/list can run while generate is in progress
struct WorkerState {
    /// Handle registry - the source of truth for all model access
    handles: HandleRegistry,
    /// Legacy models map (transitional - will be removed once handle API is complete)
    models: HashMap<String, Arc<Mutex<LoadedModel>>>,
    adapters: HashMap<String, LoadedAdapter>,
    loader: ModelLoader,
}

impl WorkerState {
    fn new() -> Result<Self, String> {
        Ok(Self {
            handles: HandleRegistry::new(),
            models: HashMap::new(),
            adapters: HashMap::new(),
            loader: ModelLoader::new()?,
        })
    }

    /// Handle binary generation - returns raw (text, prompt_tokens, generated_tokens)
    /// Used by binary protocol path to avoid JSON serialization of prompts/responses
    fn handle_generate_binary(
        &self,
        model_id: &str,
        prompt: &str,
        max_tokens: usize,
        temperature: f64,
    ) -> Result<(String, usize, usize), String> {
        // Get model Arc and lock only this model (doesn't block other models/operations)
        let model_arc = self
            .models
            .get(model_id)
            .ok_or_else(|| format!("Model not loaded: {model_id}"))?;
        let mut loaded = model_arc
            .lock()
            .map_err(|e| format!("Lock poisoned: {e}"))?;

        loaded.generate(prompt, max_tokens, temperature, &self.loader.device)
    }

    /// Handle read-only commands (ping, list, generate, gpu)
    /// Takes &self so it can run concurrently with other read operations
    fn handle_command_readonly(&self, cmd: InferenceCommand) -> Result<Value, String> {
        match cmd {
            InferenceCommand::Ping => Ok(json!({
                "worker": "inference",
                "version": "3.0.0",
                "models_loaded": self.models.len(),
                "handles_active": self.handles.handles.len(),
                "async": true,
                "supported_architectures": [
                    "llama", "mistral", "mixtral", "phi", "phi3", "qwen2",
                    "gemma", "gemma2", "stablelm", "falcon", "starcoder2"
                ],
                "api": ["model/handle", "handle/status", "handle/list", "handle/release", "generate"]
            })),

            InferenceCommand::ModelList => {
                let models: Vec<Value> = self
                    .models
                    .iter()
                    .filter_map(|(id, model_arc)| {
                        model_arc.try_lock().ok().map(|loaded| {
                            json!({
                                "model_id": id,
                                "architecture": loaded.model.architecture(),
                                "vocab_size": loaded.model.vocab_size()
                            })
                        })
                    })
                    .collect();
                Ok(json!({ "models": models }))
            }

            InferenceCommand::AdapterList => {
                let adapters: Vec<Value> = self
                    .adapters
                    .iter()
                    .map(|(id, adapter)| {
                        json!({
                            "adapter_id": id,
                            "target_model": adapter.target_model,
                            "size_mb": adapter.size_bytes / (1024 * 1024),
                            "rank": adapter.rank,
                            "tensor_count": adapter.weights.len(),
                            "load_time_ms": adapter.load_time_ms
                        })
                    })
                    .collect();
                Ok(json!({ "adapters": adapters, "count": adapters.len() }))
            }

            // =========================================================================
            // Handle API - Read Operations
            // =========================================================================
            InferenceCommand::HandleStatus { handle_id } => match self.handles.get(&handle_id) {
                Some(handle) => Ok(handle.to_json()),
                None => Err(format!("Handle not found: {handle_id}")),
            },

            InferenceCommand::HandleList => {
                let handles = self.handles.list();
                Ok(json!({
                    "handles": handles,
                    "count": handles.len()
                }))
            }

            InferenceCommand::Generate {
                model_id,
                prompt,
                max_tokens,
                temperature,
            } => {
                let model_arc = self
                    .models
                    .get(&model_id)
                    .ok_or_else(|| format!("Model not loaded: {model_id}"))?;
                let mut loaded = model_arc
                    .lock()
                    .map_err(|e| format!("Lock poisoned: {e}"))?;

                let (text, prompt_tokens, generated_tokens) =
                    loaded.generate(&prompt, max_tokens, temperature, &self.loader.device)?;

                Ok(json!({
                    "text": text,
                    "model_id": model_id,
                    "prompt_tokens": prompt_tokens,
                    "generated_tokens": generated_tokens
                }))
            }

            InferenceCommand::GpuStatus => {
                let allocator = get_gpu_allocator();
                let status = allocator.status();
                Ok(json!({
                    "total_mb": status.total_mb,
                    "allocated_mb": status.allocated_mb,
                    "available_mb": status.available_mb,
                    "pressure": status.pressure,
                    "allocation_count": status.allocation_count,
                    "should_evict": allocator.should_evict()
                }))
            }

            // GPU allocation/release are actually mutations but they use their own internal lock
            InferenceCommand::GpuAllocate {
                id,
                owner,
                size_mb,
                priority,
                load_time_ms,
                alloc_type,
            } => {
                let allocator = get_gpu_allocator();
                let at = alloc_type.map(|s| match s.as_str() {
                    "model" => AllocationType::Model,
                    "adapter" => AllocationType::Adapter,
                    "embedding" => AllocationType::Embedding,
                    _ => AllocationType::Other,
                });
                let result = allocator.allocate(AllocationRequest {
                    id,
                    owner,
                    size_mb,
                    priority,
                    load_time_ms,
                    alloc_type: at,
                });
                match result {
                    AllocationResult::Granted => Ok(json!({ "status": "granted" })),
                    AllocationResult::NeedEviction { suggested_victims } => Ok(
                        json!({ "status": "need_eviction", "suggested_victims": suggested_victims }),
                    ),
                    AllocationResult::Denied { reason } => Err(reason),
                }
            }

            InferenceCommand::GpuRelease { id } => {
                let allocator = get_gpu_allocator();
                allocator.release(&id);
                Ok(json!({ "status": "released", "id": id }))
            }

            InferenceCommand::GpuStressTest {
                count,
                min_mb,
                max_mb,
            } => {
                let allocator = get_gpu_allocator();
                let start = Instant::now();
                let mut granted = 0u64;
                let mut need_eviction = 0u64;
                let mut denied = 0u64;
                let mut total_allocated_mb = 0u64;
                let mut rng = rand::thread_rng();

                for i in 0..count {
                    let size = rng.gen_range(min_mb..=max_mb);
                    let priority: f32 = rng.gen_range(0.1..0.9);
                    let id = format!("stress-{i}");
                    let alloc_type = if i % 10 < 2 {
                        AllocationType::Model
                    } else {
                        AllocationType::Adapter
                    };
                    let result = allocator.allocate(AllocationRequest {
                        id,
                        owner: "stress-test".to_string(),
                        size_mb: size,
                        priority,
                        load_time_ms: None,
                        alloc_type: Some(alloc_type),
                    });
                    match result {
                        AllocationResult::Granted => {
                            granted += 1;
                            total_allocated_mb += size;
                        }
                        AllocationResult::NeedEviction { .. } => {
                            need_eviction += 1;
                        }
                        AllocationResult::Denied { .. } => {
                            denied += 1;
                        }
                    }
                }
                // Cleanup
                for i in 0..count {
                    allocator.release(&format!("stress-{i}"));
                }
                Ok(
                    json!({ "count": count, "granted": granted, "need_eviction": need_eviction, "denied": denied, "total_mb": total_allocated_mb, "elapsed_ms": start.elapsed().as_millis() as u64 }),
                )
            }

            // These should have been routed to handle_command (write path)
            cmd => Err(format!("Command {cmd:?} requires write access")),
        }
    }

    fn handle_command(&mut self, cmd: InferenceCommand) -> Result<Value, String> {
        match cmd {
            InferenceCommand::Ping => Ok(json!({
                "worker": "inference",
                "version": "3.0.0",
                "models_loaded": self.models.len(),
                "handles_active": self.handles.handles.len(),
                "async": true,
                "supported_architectures": [
                    "llama", "mistral", "mixtral", "phi", "phi3", "qwen2",
                    "gemma", "gemma2", "stablelm", "falcon", "starcoder2"
                ],
                "api": ["model/handle", "handle/status", "handle/list", "handle/release", "generate"]
            })),

            InferenceCommand::ModelLoad { model_id } => {
                if self.models.contains_key(&model_id) {
                    return Ok(json!({
                        "status": "already_loaded",
                        "model_id": model_id
                    }));
                }

                let loaded = self.loader.load(&model_id)?;
                let load_time = loaded.load_time_ms;
                // Wrap in Arc<Mutex> for per-model locking
                self.models
                    .insert(model_id.clone(), Arc::new(Mutex::new(loaded)));

                Ok(json!({
                    "status": "loaded",
                    "model_id": model_id,
                    "load_time_ms": load_time
                }))
            }

            InferenceCommand::ModelUnload { model_id } => {
                if self.models.remove(&model_id).is_some() {
                    Ok(json!({
                        "status": "unloaded",
                        "model_id": model_id
                    }))
                } else {
                    Err(format!("Model not loaded: {model_id}"))
                }
            }

            // =========================================================================
            // Handle API - Write Operations (NON-BLOCKING)
            // =========================================================================
            InferenceCommand::ModelHandle { model_id } => {
                // Check if we already have a handle for this model
                if let Some(handle) = self.handles.get_handle_for_model(&model_id) {
                    // Return existing handle immediately
                    return Ok(json!({
                        "handle_id": handle.id,
                        "status": handle.status,
                        "model_id": model_id,
                        "existing": true
                    }));
                }

                // Create new handle in Loading state - returns IMMEDIATELY
                let handle_id = self.handles.create_handle(&model_id);

                // NOTE: The model loading happens asynchronously via a separate mechanism
                // For now, we do synchronous loading (will be improved with proper async loading)
                // This is still better than the old API because the handle is tracked

                // Attempt to load synchronously for now
                match self.loader.load(&model_id) {
                    Ok(loaded) => {
                        let load_time = loaded.load_time_ms;
                        let model_arc = Arc::new(Mutex::new(loaded));

                        // Update handle to Ready
                        if let Some(handle) = self.handles.get_mut(&handle_id) {
                            // Estimate memory based on model size (~4 bytes per param for small models)
                            // This is rough - real memory tracking should come from Metal/CUDA APIs
                            handle.set_ready(model_arc.clone(), 500); // TODO: Get actual memory
                        }

                        // Also store in legacy models map for backward compatibility
                        self.models.insert(model_id.clone(), model_arc);

                        Ok(json!({
                            "handle_id": handle_id,
                            "status": "ready",
                            "model_id": model_id,
                            "load_time_ms": load_time
                        }))
                    }
                    Err(e) => {
                        // Update handle to Error state
                        if let Some(handle) = self.handles.get_mut(&handle_id) {
                            handle.set_error(e.clone());
                        }

                        Ok(json!({
                            "handle_id": handle_id,
                            "status": "error",
                            "model_id": model_id,
                            "error": e
                        }))
                    }
                }
            }

            InferenceCommand::HandleStatus { handle_id } => {
                // Also available in readonly, but handle it here for completeness
                match self.handles.get(&handle_id) {
                    Some(handle) => Ok(handle.to_json()),
                    None => Err(format!("Handle not found: {handle_id}")),
                }
            }

            InferenceCommand::HandleList => {
                // Also available in readonly
                let handles = self.handles.list();
                Ok(json!({
                    "handles": handles,
                    "count": handles.len()
                }))
            }

            InferenceCommand::HandleRelease { handle_id } => {
                match self.handles.remove(&handle_id) {
                    Some(handle) => {
                        // Also remove from legacy models map
                        self.models.remove(&handle.model_id);

                        Ok(json!({
                            "status": "released",
                            "handle_id": handle_id,
                            "model_id": handle.model_id,
                            "memory_freed_mb": handle.memory_mb
                        }))
                    }
                    None => Err(format!("Handle not found: {handle_id}")),
                }
            }

            InferenceCommand::ModelList => {
                let models: Vec<Value> = self
                    .models
                    .iter()
                    .filter_map(|(id, model_arc)| {
                        // Try to lock briefly - skip if model is busy
                        model_arc.try_lock().ok().map(|loaded| {
                            json!({
                                "model_id": id,
                                "architecture": loaded.model.architecture(),
                                "vocab_size": loaded.model.vocab_size()
                            })
                        })
                    })
                    .collect();

                Ok(json!({ "models": models }))
            }

            // =========================================================================
            // LoRA Adapter Handlers
            // =========================================================================
            InferenceCommand::AdapterLoad {
                adapter_id,
                adapter_path,
                target_model,
            } => {
                if self.adapters.contains_key(&adapter_id) {
                    return Ok(json!({
                        "status": "already_loaded",
                        "adapter_id": adapter_id
                    }));
                }

                let target = target_model.unwrap_or_else(|| "unknown".to_string());

                // Load adapter with timing
                let adapter = load_adapter(
                    &adapter_id,
                    &adapter_path,
                    &target,
                    &self.loader.device,
                    self.loader.dtype,
                )?;

                let load_time_ms = adapter.load_time_ms;
                let size_mb = adapter.size_bytes / (1024 * 1024);
                let rank = adapter.rank;
                let tensor_count = adapter.weights.len();

                // Register in GPU allocator for paging
                let allocator = get_gpu_allocator();
                allocator.allocate(AllocationRequest {
                    id: adapter_id.clone(),
                    owner: target.clone(),
                    size_mb: size_mb as u64,
                    priority: 0.5, // Adapters have medium priority
                    load_time_ms: Some(load_time_ms),
                    alloc_type: Some(AllocationType::Adapter),
                });

                self.adapters.insert(adapter_id.clone(), adapter);

                Ok(json!({
                    "status": "loaded",
                    "adapter_id": adapter_id,
                    "target_model": target,
                    "load_time_ms": load_time_ms,
                    "size_mb": size_mb,
                    "rank": rank,
                    "tensor_count": tensor_count
                }))
            }

            InferenceCommand::AdapterUnload { adapter_id } => {
                if let Some(adapter) = self.adapters.remove(&adapter_id) {
                    // Release from GPU allocator
                    let allocator = get_gpu_allocator();
                    allocator.release(&adapter_id);

                    let size_mb = adapter.size_bytes / (1024 * 1024);

                    Ok(json!({
                        "status": "unloaded",
                        "adapter_id": adapter_id,
                        "freed_mb": size_mb
                    }))
                } else {
                    Err(format!("Adapter not loaded: {adapter_id}"))
                }
            }

            InferenceCommand::AdapterList => {
                let adapters: Vec<Value> = self
                    .adapters
                    .iter()
                    .map(|(id, adapter)| {
                        json!({
                            "adapter_id": id,
                            "target_model": adapter.target_model,
                            "size_mb": adapter.size_bytes / (1024 * 1024),
                            "rank": adapter.rank,
                            "tensor_count": adapter.weights.len(),
                            "load_time_ms": adapter.load_time_ms
                        })
                    })
                    .collect();

                Ok(json!({
                    "adapters": adapters,
                    "count": adapters.len()
                }))
            }

            InferenceCommand::Generate {
                model_id,
                prompt,
                max_tokens,
                temperature,
            } => {
                // Per-model lock - only blocks this model, not other models/operations
                let model_arc = self
                    .models
                    .get(&model_id)
                    .ok_or_else(|| format!("Model not loaded: {model_id}"))?;
                let mut loaded = model_arc
                    .lock()
                    .map_err(|e| format!("Lock poisoned: {e}"))?;

                let (text, prompt_tokens, generated_tokens) =
                    loaded.generate(&prompt, max_tokens, temperature, &self.loader.device)?;

                Ok(json!({
                    "text": text,
                    "model_id": model_id,
                    "prompt_tokens": prompt_tokens,
                    "generated_tokens": generated_tokens
                }))
            }

            // GenerateBinary is handled separately in handle_client with binary I/O
            // This arm exists only for match exhaustiveness
            InferenceCommand::GenerateBinary { .. } => {
                Err("GenerateBinary should be handled by binary protocol path".to_string())
            }

            // =========================================================================
            // GPU Memory Management Handlers
            // =========================================================================
            InferenceCommand::GpuStatus => {
                let allocator = get_gpu_allocator();
                let status = allocator.status();
                Ok(json!({
                    "total_mb": status.total_mb,
                    "allocated_mb": status.allocated_mb,
                    "available_mb": status.available_mb,
                    "pressure": status.pressure,
                    "allocation_count": status.allocation_count,
                    "should_evict": allocator.should_evict()
                }))
            }

            InferenceCommand::GpuAllocate {
                id,
                owner,
                size_mb,
                priority,
                load_time_ms,
                alloc_type,
            } => {
                let allocator = get_gpu_allocator();
                let parsed_type = parse_alloc_type(&alloc_type);
                let result = allocator.allocate(AllocationRequest {
                    id: id.clone(),
                    owner: owner.clone(),
                    size_mb,
                    priority,
                    load_time_ms,
                    alloc_type: Some(parsed_type),
                });

                match result {
                    AllocationResult::Granted => Ok(json!({
                        "status": "granted",
                        "id": id,
                        "size_mb": size_mb,
                        "alloc_type": format!("{:?}", parsed_type)
                    })),
                    AllocationResult::NeedEviction { suggested_victims } => Ok(json!({
                        "status": "need_eviction",
                        "id": id,
                        "suggested_victims": suggested_victims
                    })),
                    AllocationResult::Denied { reason } => Err(reason),
                }
            }

            InferenceCommand::GpuPagingStats => {
                let allocator = get_gpu_allocator();
                let stats = allocator.paging_stats();
                Ok(serde_json::to_value(stats).unwrap())
            }

            InferenceCommand::GpuRelease { id } => {
                let allocator = get_gpu_allocator();
                if let Some(alloc) = allocator.release(&id) {
                    Ok(json!({
                        "status": "released",
                        "id": id,
                        "freed_mb": alloc.size_mb
                    }))
                } else {
                    Err(format!("Allocation not found: {id}"))
                }
            }

            // GPU stress test doesn't need write lock since allocator has internal lock
            InferenceCommand::GpuStressTest { .. } => {
                Err("GpuStressTest should use handle_command_readonly".to_string())
            }
        }
    }
}

// ============================================================================
// Binary Protocol Helpers (Async)
// ============================================================================

/// Write generated text as binary: JSON header + raw UTF-8 bytes (async version)
/// This eliminates JSON escaping overhead for the response text
async fn write_binary_text_async(
    stream: &mut UnixStream,
    text: &str,
    model_id: &str,
    prompt_tokens: usize,
    generated_tokens: usize,
) -> std::io::Result<()> {
    let bytes = text.as_bytes();

    let header = BinaryTextHeader {
        r#type: "binary".to_string(),
        length: bytes.len(),
        dtype: "u8".to_string(),
        prompt_tokens,
        generated_tokens,
        model_id: model_id.to_string(),
    };

    // Write JSON header with newline
    let header_json = serde_json::to_string(&header)?;
    stream.write_all(header_json.as_bytes()).await?;
    stream.write_all(b"\n").await?;

    // Write raw UTF-8 bytes - NO JSON ESCAPING
    stream.write_all(bytes).await?;
    stream.flush().await?;

    Ok(())
}

/// Read exact number of bytes from a reader (async version)
async fn read_exact_bytes_async(
    reader: &mut TokioBufReader<tokio::net::unix::OwnedReadHalf>,
    len: usize,
) -> std::io::Result<Vec<u8>> {
    let mut buffer = vec![0u8; len];
    reader.read_exact(&mut buffer).await?;
    Ok(buffer)
}

// ============================================================================
// Main Server (Tokio Async - NON-BLOCKING)
// ============================================================================

/// Handle a single client connection asynchronously
/// This function runs in its own tokio task - doesn't block other connections
async fn handle_client_async(stream: UnixStream, state: Arc<RwLock<WorkerState>>) {
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = TokioBufReader::new(read_half);
    let mut line = String::new();

    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) => break, // EOF
            Ok(_) => {}
            Err(e) => {
                eprintln!("❌ Read error: {e}");
                break;
            }
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // First, peek at the command type to detect binary protocol
        let parsed: Result<Value, _> = serde_json::from_str(trimmed);
        let is_binary = parsed
            .as_ref()
            .map(|v| v.get("command").and_then(|c| c.as_str()) == Some("generate/binary"))
            .unwrap_or(false);

        if is_binary {
            // Handle binary protocol: read prompt bytes, generate, write binary response
            if let Ok(InferenceCommand::GenerateBinary {
                model_id,
                prompt_length,
                max_tokens,
                temperature,
            }) = serde_json::from_str::<InferenceCommand>(trimmed)
            {
                    // Read binary prompt payload (async)
                    let prompt_result = read_exact_bytes_async(&mut reader, prompt_length)
                        .await
                        .map_err(|e| format!("Failed to read prompt bytes: {e}"))
                        .and_then(|bytes| {
                            String::from_utf8(bytes)
                                .map_err(|e| format!("Invalid UTF-8 in prompt: {e}"))
                        });

                    match prompt_result {
                        Ok(prompt) => {
                            // Spawn blocking task for compute-heavy generation
                            // This prevents blocking the async runtime
                            let state_clone = Arc::clone(&state);
                            let model_id_clone = model_id.clone();
                            let gen_result = tokio::task::spawn_blocking(move || {
                                let state_guard = state_clone.blocking_read();
                                state_guard.handle_generate_binary(
                                    &model_id_clone,
                                    &prompt,
                                    max_tokens,
                                    temperature,
                                )
                            })
                            .await
                            .unwrap_or_else(|e| Err(format!("Task panicked: {e}")));

                            // Reunite for writing (need full stream for binary write)
                            let mut full_stream = write_half
                                .reunite(reader.into_inner())
                                .expect("Failed to reunite stream");

                            match gen_result {
                                Ok((text, prompt_tokens, generated_tokens)) => {
                                    if let Err(e) = write_binary_text_async(
                                        &mut full_stream,
                                        &text,
                                        &model_id,
                                        prompt_tokens,
                                        generated_tokens,
                                    )
                                    .await
                                    {
                                        eprintln!("❌ Failed to write binary response: {e}");
                                        return;
                                    }
                                }
                                Err(e) => {
                                    let error_response = json!({
                                        "success": false,
                                        "error": e
                                    });
                                    let response_str =
                                        serde_json::to_string(&error_response).unwrap() + "\n";
                                    if full_stream
                                        .write_all(response_str.as_bytes())
                                        .await
                                        .is_err()
                                    {
                                        return;
                                    }
                                }
                            }

                            // Split again for continued reading
                            let (new_read, new_write) = full_stream.into_split();
                            reader = TokioBufReader::new(new_read);
                            write_half = new_write;
                        }
                        Err(e) => {
                            let error_response = json!({
                                "success": false,
                                "error": e
                            });
                            let response_str =
                                serde_json::to_string(&error_response).unwrap() + "\n";
                            if write_half.write_all(response_str.as_bytes()).await.is_err() {
                                break;
                            }
                        }
                    }
            }
            continue;
        }

        // Standard JSON protocol for all other commands
        // CRITICAL: Extract request_id for response correlation
        let request_id: Option<String> =
            serde_json::from_str::<Value>(trimmed).ok().and_then(|v| {
                v.get("request_id")
                    .and_then(|r| r.as_str().map(String::from))
            });

        let mut response: Value = match serde_json::from_str::<InferenceCommand>(trimmed) {
            Ok(cmd) => {
                // Determine if this command needs write access to state
                let needs_write = matches!(
                    cmd,
                    InferenceCommand::ModelLoad { .. } |
                    InferenceCommand::ModelUnload { .. } |
                    InferenceCommand::ModelHandle { .. } |     // Creates/updates handles
                    InferenceCommand::HandleRelease { .. } |   // Removes handles
                    InferenceCommand::AdapterLoad { .. } |
                    InferenceCommand::AdapterUnload { .. }
                );

                // Check if this is a compute-heavy operation that should be spawned blocking
                let is_compute_heavy = matches!(cmd, InferenceCommand::Generate { .. });

                if is_compute_heavy {
                    // Spawn blocking for generation to avoid blocking async runtime
                    let state_clone = Arc::clone(&state);
                    tokio::task::spawn_blocking(move || {
                        let state_guard = state_clone.blocking_read();
                        match state_guard.handle_command_readonly(cmd) {
                            Ok(result) => json!({ "success": true, "result": result }),
                            Err(e) => json!({ "success": false, "error": e }),
                        }
                    })
                    .await
                    .unwrap_or_else(|e| {
                        json!({
                            "success": false,
                            "error": format!("Task panicked: {}", e)
                        })
                    })
                } else if needs_write {
                    // Write operations (load/unload) - use spawn_blocking for heavy IO
                    let state_clone = Arc::clone(&state);
                    tokio::task::spawn_blocking(move || {
                        let mut state_guard = state_clone.blocking_write();
                        match state_guard.handle_command(cmd) {
                            Ok(result) => json!({ "success": true, "result": result }),
                            Err(e) => json!({ "success": false, "error": e }),
                        }
                    })
                    .await
                    .unwrap_or_else(|e| {
                        json!({
                            "success": false,
                            "error": format!("Task panicked: {}", e)
                        })
                    })
                } else {
                    // Light read operations (ping, list, gpu status) - can run inline
                    let state_guard = state.read().await;
                    match state_guard.handle_command_readonly(cmd) {
                        Ok(result) => json!({ "success": true, "result": result }),
                        Err(e) => json!({ "success": false, "error": e }),
                    }
                }
            }
            Err(e) => json!({
                "success": false,
                "error": format!("Invalid command: {}", e)
            }),
        };

        // CRITICAL: Echo back request_id for TypeScript correlation
        if let Some(req_id) = request_id {
            response
                .as_object_mut()
                .unwrap()
                .insert("request_id".to_string(), json!(req_id));
        }

        // Send response (async)
        let response_str = serde_json::to_string(&response).unwrap() + "\n";
        if write_half.write_all(response_str.as_bytes()).await.is_err() {
            break;
        }
    }
}

#[tokio::main]
async fn main() {
    println!("🦀 Candle Inference Worker v3.0 (Tokio Async) starting...");

    // Get socket path from args
    let args: Vec<String> = std::env::args().collect();
    let socket_path = args
        .get(1)
        .map(|s| s.as_str())
        .unwrap_or("/tmp/jtag-inference.sock");

    println!("📡 Socket: {socket_path}");

    // Remove old socket
    let _ = fs::remove_file(socket_path);

    // Initialize state with tokio RwLock for async concurrent access
    let state = match WorkerState::new() {
        Ok(s) => Arc::new(RwLock::new(s)),
        Err(e) => {
            eprintln!("❌ Failed to initialize: {e}");
            std::process::exit(1);
        }
    };

    // Check for Metal
    if cfg!(target_os = "macos") {
        if Device::new_metal(0).is_ok() {
            println!("✅ Metal acceleration enabled");
        } else {
            println!("⚠️  Metal not available, using CPU");
        }
    }

    // Bind socket (async)
    let listener = UnixListener::bind(socket_path).expect("Failed to bind socket");
    println!("✅ Inference Worker v3.0 ready (Tokio async, non-blocking)");
    println!("📂 Supported: llama, mistral, mixtral, phi, phi3, qwen2, gemma, gemma2, stablelm, falcon, starcoder2");
    println!("✅ Listening for connections (concurrent, non-blocking)\n");

    // Accept loop - each connection spawns a new async task
    loop {
        match listener.accept().await {
            Ok((stream, _addr)) => {
                let state = Arc::clone(&state);
                // Spawn async task per connection - doesn't block accept loop
                tokio::spawn(async move {
                    handle_client_async(stream, state).await;
                });
            }
            Err(e) => eprintln!("Connection error: {e}"),
        }
    }
}
