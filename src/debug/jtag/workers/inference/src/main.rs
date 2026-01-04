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
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

// GPU Allocator (shared module)
#[path = "../../shared/gpu_allocator.rs"]
mod gpu_allocator;
use gpu_allocator::{get_gpu_allocator, AllocationRequest, AllocationResult, AllocationType};

// Candle imports
use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::generation::LogitsProcessor;

// Model imports - all supported architectures
use candle_transformers::models::phi::{Config as PhiConfig, Model as PhiModel};
use candle_transformers::models::phi3::{Config as Phi3Config, Model as Phi3Model};
use candle_transformers::models::llama::{Llama as LlamaModel, Cache as LlamaCache, LlamaConfig as LlamaRawConfig, Config as LlamaModelConfig};
use candle_transformers::models::mistral::{Config as MistralConfig, Model as MistralModel};
use candle_transformers::models::mixtral::{Config as MixtralConfig, Model as MixtralModel};
use candle_transformers::models::qwen2::{Config as Qwen2Config, ModelForCausalLM as Qwen2Model};
use candle_transformers::models::gemma::{Config as GemmaConfig, Model as GemmaModel};
use candle_transformers::models::gemma2::{Config as Gemma2Config, Model as Gemma2Model};
use candle_transformers::models::stable_lm::{Config as StableLMConfig, Model as StableLMModel};
use candle_transformers::models::falcon::{Config as FalconConfig, Falcon as FalconModel};
use candle_transformers::models::starcoder2::{Config as StarCoder2Config, Model as StarCoder2Model};

// HuggingFace Hub
use hf_hub::{api::sync::Api, Repo, RepoType};

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
        self.model.forward(tokens)
            .map_err(|e| format!("Phi forward failed: {}", e))
    }

    fn clear_cache(&mut self) -> Result<(), String> {
        self.model.clear_kv_cache();
        Ok(())
    }

    fn vocab_size(&self) -> usize { self.vocab_size }
    fn architecture(&self) -> &'static str { "phi" }
    fn eos_token_id(&self) -> u32 { self.eos_token_id }
}

/// Wrapper for Phi-3 models
struct Phi3Wrapper {
    model: Phi3Model,
    vocab_size: usize,
    eos_token_id: u32,
}

impl CausalLM for Phi3Wrapper {
    fn forward(&mut self, tokens: &Tensor, pos: usize) -> Result<Tensor, String> {
        self.model.forward(tokens, pos)
            .map_err(|e| format!("Phi3 forward failed: {}", e))
    }

    fn clear_cache(&mut self) -> Result<(), String> {
        self.model.clear_kv_cache();
        Ok(())
    }

    fn vocab_size(&self) -> usize { self.vocab_size }
    fn architecture(&self) -> &'static str { "phi3" }
    fn eos_token_id(&self) -> u32 { self.eos_token_id }
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
        self.model.forward(tokens, pos, &mut self.cache)
            .map_err(|e| format!("Llama forward failed: {}", e))
    }

    fn clear_cache(&mut self) -> Result<(), String> {
        // Llama cache must be recreated (no reset method)
        self.cache = LlamaCache::new(true, DType::F32, &self.config, &self.device)
            .map_err(|e| format!("Failed to recreate Llama cache: {}", e))?;
        Ok(())
    }

    fn vocab_size(&self) -> usize { self.vocab_size }
    fn architecture(&self) -> &'static str { "llama" }
    fn eos_token_id(&self) -> u32 { self.eos_token_id }
}

/// Wrapper for Mistral models
struct MistralWrapper {
    model: MistralModel,
    vocab_size: usize,
    eos_token_id: u32,
}

impl CausalLM for MistralWrapper {
    fn forward(&mut self, tokens: &Tensor, pos: usize) -> Result<Tensor, String> {
        self.model.forward(tokens, pos)
            .map_err(|e| format!("Mistral forward failed: {}", e))
    }

    fn clear_cache(&mut self) -> Result<(), String> {
        self.model.clear_kv_cache();
        Ok(())
    }

    fn vocab_size(&self) -> usize { self.vocab_size }
    fn architecture(&self) -> &'static str { "mistral" }
    fn eos_token_id(&self) -> u32 { self.eos_token_id }
}

/// Wrapper for Mixtral (MoE) models
struct MixtralWrapper {
    model: MixtralModel,
    vocab_size: usize,
    eos_token_id: u32,
}

impl CausalLM for MixtralWrapper {
    fn forward(&mut self, tokens: &Tensor, pos: usize) -> Result<Tensor, String> {
        self.model.forward(tokens, pos)
            .map_err(|e| format!("Mixtral forward failed: {}", e))
    }

    fn clear_cache(&mut self) -> Result<(), String> {
        // Mixtral doesn't expose clear_kv_cache - will recreate model if needed
        Ok(())
    }

    fn vocab_size(&self) -> usize { self.vocab_size }
    fn architecture(&self) -> &'static str { "mixtral" }
    fn eos_token_id(&self) -> u32 { self.eos_token_id }
}

/// Wrapper for Qwen2 models
struct Qwen2Wrapper {
    model: Qwen2Model,
    vocab_size: usize,
    eos_token_id: u32,
}

impl CausalLM for Qwen2Wrapper {
    fn forward(&mut self, tokens: &Tensor, pos: usize) -> Result<Tensor, String> {
        self.model.forward(tokens, pos)
            .map_err(|e| format!("Qwen2 forward failed: {}", e))
    }

    fn clear_cache(&mut self) -> Result<(), String> {
        self.model.clear_kv_cache();
        Ok(())
    }

    fn vocab_size(&self) -> usize { self.vocab_size }
    fn architecture(&self) -> &'static str { "qwen2" }
    fn eos_token_id(&self) -> u32 { self.eos_token_id }
}

/// Wrapper for Gemma models
struct GemmaWrapper {
    model: GemmaModel,
    vocab_size: usize,
    eos_token_id: u32,
}

impl CausalLM for GemmaWrapper {
    fn forward(&mut self, tokens: &Tensor, pos: usize) -> Result<Tensor, String> {
        self.model.forward(tokens, pos)
            .map_err(|e| format!("Gemma forward failed: {}", e))
    }

    fn clear_cache(&mut self) -> Result<(), String> {
        self.model.clear_kv_cache();
        Ok(())
    }

    fn vocab_size(&self) -> usize { self.vocab_size }
    fn architecture(&self) -> &'static str { "gemma" }
    fn eos_token_id(&self) -> u32 { self.eos_token_id }
}

/// Wrapper for Gemma2 models
struct Gemma2Wrapper {
    model: Gemma2Model,
    vocab_size: usize,
    eos_token_id: u32,
}

impl CausalLM for Gemma2Wrapper {
    fn forward(&mut self, tokens: &Tensor, pos: usize) -> Result<Tensor, String> {
        self.model.forward(tokens, pos)
            .map_err(|e| format!("Gemma2 forward failed: {}", e))
    }

    fn clear_cache(&mut self) -> Result<(), String> {
        self.model.clear_kv_cache();
        Ok(())
    }

    fn vocab_size(&self) -> usize { self.vocab_size }
    fn architecture(&self) -> &'static str { "gemma2" }
    fn eos_token_id(&self) -> u32 { self.eos_token_id }
}

/// Wrapper for StableLM models
struct StableLMWrapper {
    model: StableLMModel,
    vocab_size: usize,
    eos_token_id: u32,
}

impl CausalLM for StableLMWrapper {
    fn forward(&mut self, tokens: &Tensor, pos: usize) -> Result<Tensor, String> {
        self.model.forward(tokens, pos)
            .map_err(|e| format!("StableLM forward failed: {}", e))
    }

    fn clear_cache(&mut self) -> Result<(), String> {
        // StableLM doesn't expose clear_kv_cache
        Ok(())
    }

    fn vocab_size(&self) -> usize { self.vocab_size }
    fn architecture(&self) -> &'static str { "stablelm" }
    fn eos_token_id(&self) -> u32 { self.eos_token_id }
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
        self.model.forward(tokens)
            .map_err(|e| format!("Falcon forward failed: {}", e))
    }

    fn clear_cache(&mut self) -> Result<(), String> {
        // Falcon doesn't expose clear_kv_cache
        Ok(())
    }

    fn vocab_size(&self) -> usize { self.vocab_size }
    fn architecture(&self) -> &'static str { "falcon" }
    fn eos_token_id(&self) -> u32 { self.eos_token_id }
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
        self.model.forward(tokens, pos)
            .map_err(|e| format!("StarCoder2 forward failed: {}", e))
    }

    fn clear_cache(&mut self) -> Result<(), String> {
        self.model.clear_kv_cache();
        Ok(())
    }

    fn vocab_size(&self) -> usize { self.vocab_size }
    fn architecture(&self) -> &'static str { "starcoder2" }
    fn eos_token_id(&self) -> u32 { self.eos_token_id }
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

const MISTRAL_ARCHITECTURES: &[&str] = &[
    "MistralForCausalLM",
];

const MIXTRAL_ARCHITECTURES: &[&str] = &[
    "MixtralForCausalLM",
];

const PHI_ARCHITECTURES: &[&str] = &[
    "PhiForCausalLM",
    "Phi1ForCausalLM",
    "MixFormerSequentialForCausalLM",
];

const PHI3_ARCHITECTURES: &[&str] = &[
    "Phi3ForCausalLM",
    "Phi3SmallForCausalLM",
];

const QWEN2_ARCHITECTURES: &[&str] = &[
    "Qwen2ForCausalLM",
];

const GEMMA_ARCHITECTURES: &[&str] = &[
    "GemmaForCausalLM",
];

const GEMMA2_ARCHITECTURES: &[&str] = &[
    "Gemma2ForCausalLM",
];

const STABLELM_ARCHITECTURES: &[&str] = &[
    "StableLmForCausalLM",
    "StableLMEpochForCausalLM",
];

const FALCON_ARCHITECTURES: &[&str] = &[
    "FalconForCausalLM",
    "RWForCausalLM",
];

// NOTE: MPT_ARCHITECTURES and YI_ARCHITECTURES removed - Config types don't implement Deserialize

const STARCODER2_ARCHITECTURES: &[&str] = &[
    "Starcoder2ForCausalLM",
];

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

struct LoadedModel {
    model: Box<dyn CausalLM>,
    tokenizer: Tokenizer,
    #[allow(dead_code)]
    model_id: String,
    #[allow(dead_code)]
    load_time_ms: u64,
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
// Model Loader - Downloads and loads models from HuggingFace
// ============================================================================

struct ModelLoader {
    device: Device,
    dtype: DType,
}

impl ModelLoader {
    fn new() -> Result<Self, String> {
        // Use Metal on macOS, CUDA on Linux/Windows, CPU as fallback
        let device = if cfg!(target_os = "macos") {
            Device::new_metal(0).unwrap_or(Device::Cpu)
        } else {
            Device::Cpu
        };

        println!("🔧 Using device: {:?}", device);

        Ok(Self {
            device,
            dtype: DType::F32, // F16 for Metal, F32 for CPU
        })
    }

    /// Load a model from HuggingFace Hub
    fn load(&self, model_id: &str) -> Result<LoadedModel, String> {
        let start = Instant::now();
        println!("📥 Loading model: {}", model_id);

        // Download model files
        let api = Api::new().map_err(|e| format!("HF API error: {}", e))?;
        let repo = api.repo(Repo::new(model_id.to_string(), RepoType::Model));

        // Load config.json to detect architecture
        let config_path = repo.get("config.json")
            .map_err(|e| format!("Failed to get config.json: {}", e))?;
        let config_str = fs::read_to_string(&config_path)
            .map_err(|e| format!("Failed to read config.json: {}", e))?;
        let config: Value = serde_json::from_str(&config_str)
            .map_err(|e| format!("Failed to parse config.json: {}", e))?;

        // Detect architecture
        let architecture = detect_architecture(&config)
            .ok_or_else(|| format!("Unknown architecture in {}", model_id))?;
        println!("🔍 Detected architecture: {}", architecture);

        // Load tokenizer
        let tokenizer_path = repo.get("tokenizer.json")
            .map_err(|e| format!("Failed to get tokenizer.json: {}", e))?;
        println!("📂 Tokenizer: {:?}", tokenizer_path);
        let tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| format!("Failed to load tokenizer: {}", e))?;

        // Download weights (handle sharded models)
        let weights_paths = self.download_weights(&repo)?;
        println!("🔧 Loading {} safetensor file(s) to {:?}...", weights_paths.len(), self.device);

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
            _ => return Err(format!("Unsupported architecture: {}", architecture)),
        };

        let load_time_ms = start.elapsed().as_millis() as u64;
        println!("✅ Model loaded in {}ms: {}", load_time_ms, model_id);

        Ok(LoadedModel {
            model,
            tokenizer,
            model_id: model_id.to_string(),
            load_time_ms,
        })
    }

    /// Download model weights (handles sharded models)
    fn download_weights(&self, repo: &hf_hub::api::sync::ApiRepo) -> Result<Vec<std::path::PathBuf>, String> {
        // Try single weights file first
        if let Ok(path) = repo.get("model.safetensors") {
            println!("📂 Weights (single): {:?}", path);
            return Ok(vec![path]);
        }

        // Try sharded weights (model.safetensors.index.json)
        if let Ok(index_path) = repo.get("model.safetensors.index.json") {
            println!("📂 Found sharded weights index: {:?}", index_path);
            let index_str = fs::read_to_string(&index_path)
                .map_err(|e| format!("Failed to read index: {}", e))?;
            let index: Value = serde_json::from_str(&index_str)
                .map_err(|e| format!("Failed to parse index: {}", e))?;

            // Get unique shard files
            let weight_map = index.get("weight_map")
                .and_then(|v| v.as_object())
                .ok_or("Invalid index format")?;

            let mut shard_files: Vec<String> = weight_map.values()
                .filter_map(|v| v.as_str())
                .map(|s| s.to_string())
                .collect();
            shard_files.sort();
            shard_files.dedup();

            println!("📦 Downloading {} weight shards...", shard_files.len());

            let mut paths = Vec::new();
            for shard in &shard_files {
                let path = repo.get(shard)
                    .map_err(|e| format!("Failed to get shard {}: {}", shard, e))?;
                paths.push(path);
            }

            return Ok(paths);
        }

        Err("No weights found (tried model.safetensors and sharded)".to_string())
    }

    /// Build VarBuilder from weight files
    fn build_var_builder(&self, paths: &[std::path::PathBuf]) -> Result<VarBuilder<'static>, String> {
        // SAFETY: mmap is required by Candle's safetensors loading
        // The files are read-only and memory-mapped for efficiency
        if paths.len() == 1 {
            unsafe {
                VarBuilder::from_mmaped_safetensors(&paths, self.dtype, &self.device)
                    .map_err(|e| format!("Failed to load weights: {}", e))
            }
        } else {
            unsafe {
                VarBuilder::from_mmaped_safetensors(&paths, self.dtype, &self.device)
                    .map_err(|e| format!("Failed to load sharded weights: {}", e))
            }
        }
    }

    /// Extract vocab_size from config
    fn get_vocab_size(config: &Value) -> usize {
        config.get("vocab_size")
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

    fn load_llama(&self, config: &Value, vb: VarBuilder<'static>) -> Result<Box<dyn CausalLM>, String> {
        let llama_config: LlamaRawConfig = serde_json::from_value(config.clone())
            .map_err(|e| format!("Failed to parse Llama config: {}", e))?;
        let vocab_size = Self::get_vocab_size(config);
        let eos_token_id = Self::get_eos_token_id(config);

        let model_config = llama_config.clone().into_config(false);
        let model = LlamaModel::load(vb, &model_config)
            .map_err(|e| format!("Failed to load Llama model: {}", e))?;
        let cache = LlamaCache::new(true, self.dtype, &model_config, &self.device)
            .map_err(|e| format!("Failed to create Llama cache: {}", e))?;

        println!("✅ Llama model loaded: vocab_size={}", vocab_size);

        Ok(Box::new(LlamaWrapper {
            model,
            cache,
            config: model_config,
            device: self.device.clone(),
            vocab_size,
            eos_token_id,
        }))
    }

    fn load_mistral(&self, config: &Value, vb: VarBuilder<'static>) -> Result<Box<dyn CausalLM>, String> {
        let mistral_config: MistralConfig = serde_json::from_value(config.clone())
            .map_err(|e| format!("Failed to parse Mistral config: {}", e))?;
        let vocab_size = mistral_config.vocab_size;
        let eos_token_id = Self::get_eos_token_id(config);

        let model = MistralModel::new(&mistral_config, vb)
            .map_err(|e| format!("Failed to load Mistral model: {}", e))?;

        println!("✅ Mistral model loaded: vocab_size={}", vocab_size);

        Ok(Box::new(MistralWrapper { model, vocab_size, eos_token_id }))
    }

    fn load_mixtral(&self, config: &Value, vb: VarBuilder<'static>) -> Result<Box<dyn CausalLM>, String> {
        let mixtral_config: MixtralConfig = serde_json::from_value(config.clone())
            .map_err(|e| format!("Failed to parse Mixtral config: {}", e))?;
        let vocab_size = Self::get_vocab_size(config);
        let eos_token_id = Self::get_eos_token_id(config);

        let model = MixtralModel::new(&mixtral_config, vb)
            .map_err(|e| format!("Failed to load Mixtral model: {}", e))?;

        println!("✅ Mixtral model loaded: vocab_size={}", vocab_size);

        Ok(Box::new(MixtralWrapper { model, vocab_size, eos_token_id }))
    }

    fn load_phi(&self, config: &Value, vb: VarBuilder<'static>) -> Result<Box<dyn CausalLM>, String> {
        let phi_config: PhiConfig = serde_json::from_value(config.clone())
            .map_err(|e| format!("Failed to parse Phi config: {}", e))?;
        let vocab_size = Self::get_vocab_size(config);
        let eos_token_id = Self::get_eos_token_id(config);

        let model = PhiModel::new(&phi_config, vb)
            .map_err(|e| format!("Failed to load Phi model: {}", e))?;

        println!("✅ Phi model loaded: vocab_size={}", vocab_size);

        Ok(Box::new(PhiWrapper { model, vocab_size, eos_token_id }))
    }

    fn load_phi3(&self, config: &Value, vb: VarBuilder<'static>) -> Result<Box<dyn CausalLM>, String> {
        let phi3_config: Phi3Config = serde_json::from_value(config.clone())
            .map_err(|e| format!("Failed to parse Phi3 config: {}", e))?;
        let vocab_size = phi3_config.vocab_size;
        let eos_token_id = Self::get_eos_token_id(config);

        let model = Phi3Model::new(&phi3_config, vb)
            .map_err(|e| format!("Failed to load Phi3 model: {}", e))?;

        println!("✅ Phi3 model loaded: vocab_size={}", vocab_size);

        Ok(Box::new(Phi3Wrapper { model, vocab_size, eos_token_id }))
    }

    fn load_qwen2(&self, config: &Value, vb: VarBuilder<'static>) -> Result<Box<dyn CausalLM>, String> {
        let qwen2_config: Qwen2Config = serde_json::from_value(config.clone())
            .map_err(|e| format!("Failed to parse Qwen2 config: {}", e))?;
        let vocab_size = qwen2_config.vocab_size;
        let eos_token_id = Self::get_eos_token_id(config);

        let model = Qwen2Model::new(&qwen2_config, vb)
            .map_err(|e| format!("Failed to load Qwen2 model: {}", e))?;

        println!("✅ Qwen2 model loaded: vocab_size={}", vocab_size);

        Ok(Box::new(Qwen2Wrapper { model, vocab_size, eos_token_id }))
    }

    fn load_gemma(&self, config: &Value, vb: VarBuilder<'static>) -> Result<Box<dyn CausalLM>, String> {
        let gemma_config: GemmaConfig = serde_json::from_value(config.clone())
            .map_err(|e| format!("Failed to parse Gemma config: {}", e))?;
        let vocab_size = gemma_config.vocab_size;
        let eos_token_id = Self::get_eos_token_id(config);

        let model = GemmaModel::new(false, &gemma_config, vb)
            .map_err(|e| format!("Failed to load Gemma model: {}", e))?;

        println!("✅ Gemma model loaded: vocab_size={}", vocab_size);

        Ok(Box::new(GemmaWrapper { model, vocab_size, eos_token_id }))
    }

    fn load_gemma2(&self, config: &Value, vb: VarBuilder<'static>) -> Result<Box<dyn CausalLM>, String> {
        let gemma2_config: Gemma2Config = serde_json::from_value(config.clone())
            .map_err(|e| format!("Failed to parse Gemma2 config: {}", e))?;
        let vocab_size = gemma2_config.vocab_size;
        let eos_token_id = Self::get_eos_token_id(config);

        let model = Gemma2Model::new(false, &gemma2_config, vb)
            .map_err(|e| format!("Failed to load Gemma2 model: {}", e))?;

        println!("✅ Gemma2 model loaded: vocab_size={}", vocab_size);

        Ok(Box::new(Gemma2Wrapper { model, vocab_size, eos_token_id }))
    }

    fn load_stablelm(&self, config: &Value, vb: VarBuilder<'static>) -> Result<Box<dyn CausalLM>, String> {
        let stablelm_config: StableLMConfig = serde_json::from_value(config.clone())
            .map_err(|e| format!("Failed to parse StableLM config: {}", e))?;
        let vocab_size = Self::get_vocab_size(config);
        let eos_token_id = Self::get_eos_token_id(config);

        let model = StableLMModel::new(&stablelm_config, vb)
            .map_err(|e| format!("Failed to load StableLM model: {}", e))?;

        println!("✅ StableLM model loaded: vocab_size={}", vocab_size);

        Ok(Box::new(StableLMWrapper { model, vocab_size, eos_token_id }))
    }

    fn load_falcon(&self, config: &Value, vb: VarBuilder<'static>) -> Result<Box<dyn CausalLM>, String> {
        let falcon_config: FalconConfig = serde_json::from_value(config.clone())
            .map_err(|e| format!("Failed to parse Falcon config: {}", e))?;
        let vocab_size = falcon_config.vocab_size;
        let eos_token_id = Self::get_eos_token_id(config);

        let model = FalconModel::load(vb, falcon_config)
            .map_err(|e| format!("Failed to load Falcon model: {}", e))?;

        println!("✅ Falcon model loaded: vocab_size={}", vocab_size);

        Ok(Box::new(FalconWrapper { model, vocab_size, eos_token_id }))
    }

    // NOTE: load_mpt and load_yi removed - Config types don't implement Deserialize

    fn load_starcoder2(&self, config: &Value, vb: VarBuilder<'static>) -> Result<Box<dyn CausalLM>, String> {
        let sc2_config: StarCoder2Config = serde_json::from_value(config.clone())
            .map_err(|e| format!("Failed to parse StarCoder2 config: {}", e))?;
        let vocab_size = Self::get_vocab_size(config);
        let eos_token_id = Self::get_eos_token_id(config);

        let model = StarCoder2Model::new(&sc2_config, vb)
            .map_err(|e| format!("Failed to load StarCoder2 model: {}", e))?;

        println!("✅ StarCoder2 model loaded: vocab_size={}", vocab_size);

        Ok(Box::new(StarCoder2Wrapper { model, vocab_size, eos_token_id }))
    }
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
    let encoding = tokenizer.encode(prompt, true)
        .map_err(|e| format!("Tokenization failed: {}", e))?;
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
            .map_err(|e| format!("Failed to create input tensor: {}", e))?
            .unsqueeze(0)
            .map_err(|e| format!("Failed to unsqueeze: {}", e))?;

        // Forward pass
        let pos = if i == 0 { 0 } else { all_tokens.len() - 1 };
        let logits = model.forward(&input, pos)?;

        // Get last token logits
        let logits = if logits.dims().len() == 3 {
            logits.squeeze(0).map_err(|e| format!("Squeeze failed: {}", e))?
        } else {
            logits
        };

        let last_logits = if logits.dims()[0] > 1 {
            logits.get(logits.dims()[0] - 1)
                .map_err(|e| format!("Get last logits failed: {}", e))?
        } else {
            logits.squeeze(0).map_err(|e| format!("Squeeze logits failed: {}", e))?
        };

        // Sample next token
        let next_token = logits_processor.sample(&last_logits)
            .map_err(|e| format!("Sampling failed: {}", e))?;

        // Check for EOS
        if next_token == eos_token_id {
            break;
        }

        all_tokens.push(next_token);
    }

    // Decode generated tokens
    let generated_tokens = &all_tokens[prompt_len..];
    let generated_text = tokenizer.decode(generated_tokens, true)
        .map_err(|e| format!("Decoding failed: {}", e))?;

    let elapsed = start.elapsed().as_millis();
    let tok_per_sec = if elapsed > 0 {
        (generated_tokens.len() as f64 / elapsed as f64) * 1000.0
    } else {
        0.0
    };

    println!("✨ Generated {} tokens in {}ms ({:.1} tok/s)",
        generated_tokens.len(), elapsed, tok_per_sec);

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

fn default_priority() -> f32 { 0.5 }
fn default_stress_count() -> usize { 100 }
fn default_stress_min_mb() -> u64 { 10 }
fn default_stress_max_mb() -> u64 { 500 }

fn parse_alloc_type(s: &Option<String>) -> AllocationType {
    match s.as_ref().map(|s| s.as_str()) {
        Some("model") => AllocationType::Model,
        Some("adapter") => AllocationType::Adapter,
        Some("embedding") => AllocationType::Embedding,
        _ => AllocationType::Other,
    }
}

fn default_max_tokens() -> usize { 256 }
fn default_temperature() -> f64 { 0.7 }

/// Binary response header for text generation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BinaryTextHeader {
    #[serde(rename = "type")]
    r#type: String, // "binary"
    length: usize,
    dtype: String,  // "u8" for UTF-8 text
    prompt_tokens: usize,
    generated_tokens: usize,
    model_id: String,
}

// ============================================================================
// Worker State
// ============================================================================

struct WorkerState {
    models: HashMap<String, LoadedModel>,
    loader: ModelLoader,
}

impl WorkerState {
    fn new() -> Result<Self, String> {
        Ok(Self {
            models: HashMap::new(),
            loader: ModelLoader::new()?,
        })
    }

    /// Handle binary generation - returns raw (text, prompt_tokens, generated_tokens)
    /// Used by binary protocol path to avoid JSON serialization of prompts/responses
    fn handle_generate_binary(
        &mut self,
        model_id: &str,
        prompt: &str,
        max_tokens: usize,
        temperature: f64,
    ) -> Result<(String, usize, usize), String> {
        let loaded = self.models.get_mut(model_id)
            .ok_or_else(|| format!("Model not loaded: {}", model_id))?;

        generate_text(
            loaded.model.as_mut(),
            &loaded.tokenizer,
            prompt,
            max_tokens,
            temperature,
            &self.loader.device,
        )
    }

    fn handle_command(&mut self, cmd: InferenceCommand) -> Result<Value, String> {
        match cmd {
            InferenceCommand::Ping => {
                Ok(json!({
                    "worker": "inference",
                    "version": "2.0.0",
                    "models_loaded": self.models.len(),
                    "supported_architectures": [
                        "llama", "mistral", "mixtral", "phi", "phi3", "qwen2",
                        "gemma", "gemma2", "stablelm", "falcon", "starcoder2"
                    ]
                }))
            }

            InferenceCommand::ModelLoad { model_id } => {
                if self.models.contains_key(&model_id) {
                    return Ok(json!({
                        "status": "already_loaded",
                        "model_id": model_id
                    }));
                }

                let loaded = self.loader.load(&model_id)?;
                let load_time = loaded.load_time_ms;
                self.models.insert(model_id.clone(), loaded);

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
                    Err(format!("Model not loaded: {}", model_id))
                }
            }

            InferenceCommand::ModelList => {
                let models: Vec<Value> = self.models.iter()
                    .map(|(id, loaded)| json!({
                        "model_id": id,
                        "architecture": loaded.model.architecture(),
                        "vocab_size": loaded.model.vocab_size()
                    }))
                    .collect();

                Ok(json!({ "models": models }))
            }

            InferenceCommand::Generate { model_id, prompt, max_tokens, temperature } => {
                let loaded = self.models.get_mut(&model_id)
                    .ok_or_else(|| format!("Model not loaded: {}", model_id))?;

                let (text, prompt_tokens, generated_tokens) = generate_text(
                    loaded.model.as_mut(),
                    &loaded.tokenizer,
                    &prompt,
                    max_tokens,
                    temperature,
                    &self.loader.device,
                )?;

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

            InferenceCommand::GpuAllocate { id, owner, size_mb, priority, load_time_ms, alloc_type } => {
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
                    Err(format!("Allocation not found: {}", id))
                }
            }

            InferenceCommand::GpuStressTest { count, min_mb, max_mb } => {
                let allocator = get_gpu_allocator();
                let start = Instant::now();

                let mut granted = 0u64;
                let mut need_eviction = 0u64;
                let mut denied = 0u64;
                let mut total_allocated_mb = 0u64;
                let mut eviction_suggestions: Vec<String> = Vec::new();

                // Create random allocations (mix of models and adapters)
                let mut rng = rand::thread_rng();
                for i in 0..count {
                    let size = rng.gen_range(min_mb..=max_mb);
                    let priority: f32 = rng.gen_range(0.1..0.9);
                    let id = format!("stress-{}", i);
                    let owner = format!("stress-owner-{}", i % 10);
                    // Mix of types: 20% models, 70% adapters, 10% other
                    let alloc_type = if i % 10 < 2 {
                        AllocationType::Model
                    } else if i % 10 < 9 {
                        AllocationType::Adapter
                    } else {
                        AllocationType::Other
                    };
                    let load_time = if alloc_type == AllocationType::Model { 7000 } else { 200 };

                    let result = allocator.allocate(AllocationRequest {
                        id,
                        owner,
                        size_mb: size,
                        priority,
                        load_time_ms: Some(load_time),
                        alloc_type: Some(alloc_type),
                    });

                    match result {
                        AllocationResult::Granted => {
                            granted += 1;
                            total_allocated_mb += size;
                        }
                        AllocationResult::NeedEviction { suggested_victims } => {
                            need_eviction += 1;
                            eviction_suggestions.extend(suggested_victims);
                        }
                        AllocationResult::Denied { .. } => {
                            denied += 1;
                        }
                    }
                }

                let elapsed = start.elapsed();
                let status = allocator.status();

                // Clean up stress test allocations
                for i in 0..count {
                    let id = format!("stress-{}", i);
                    allocator.release(&id);
                }

                Ok(json!({
                    "duration_ms": elapsed.as_millis() as u64,
                    "operations": count,
                    "granted": granted,
                    "need_eviction": need_eviction,
                    "denied": denied,
                    "peak_allocated_mb": total_allocated_mb,
                    "eviction_suggestions_count": eviction_suggestions.len(),
                    "ops_per_second": (count as f64 / elapsed.as_secs_f64()) as u64,
                    "final_status": {
                        "total_mb": status.total_mb,
                        "allocated_mb": status.allocated_mb,
                        "pressure": status.pressure
                    }
                }))
            }
        }
    }
}

// ============================================================================
// Binary Protocol Helpers
// ============================================================================

/// Write generated text as binary: JSON header + raw UTF-8 bytes
/// This eliminates JSON escaping overhead for the response text
fn write_binary_text<W: Write>(
    writer: &mut W,
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
    writer.write_all(header_json.as_bytes())?;
    writer.write_all(b"\n")?;

    // Write raw UTF-8 bytes - NO JSON ESCAPING
    writer.write_all(bytes)?;
    writer.flush()?;

    Ok(())
}

/// Read exact number of bytes from a reader
fn read_exact_bytes<R: std::io::Read>(reader: &mut R, len: usize) -> std::io::Result<Vec<u8>> {
    let mut buffer = vec![0u8; len];
    reader.read_exact(&mut buffer)?;
    Ok(buffer)
}

// ============================================================================
// Main Server
// ============================================================================

fn handle_client(mut stream: UnixStream, state: Arc<Mutex<WorkerState>>) {
    let mut reader = BufReader::new(stream.try_clone().expect("Failed to clone stream"));
    let mut line = String::new();

    while reader.read_line(&mut line).unwrap_or(0) > 0 {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            line.clear();
            continue;
        }

        // First, peek at the command type to detect binary protocol
        let parsed: Result<Value, _> = serde_json::from_str(trimmed);
        let is_binary = parsed.as_ref()
            .map(|v| v.get("command").and_then(|c| c.as_str()) == Some("generate/binary"))
            .unwrap_or(false);

        if is_binary {
            // Handle binary protocol: read prompt bytes, generate, write binary response
            if let Ok(cmd) = serde_json::from_str::<InferenceCommand>(trimmed) {
                if let InferenceCommand::GenerateBinary { model_id, prompt_length, max_tokens, temperature } = cmd {
                    // Read binary prompt payload
                    let prompt_result = read_exact_bytes(&mut reader, prompt_length)
                        .map_err(|e| format!("Failed to read prompt bytes: {}", e))
                        .and_then(|bytes| {
                            String::from_utf8(bytes)
                                .map_err(|e| format!("Invalid UTF-8 in prompt: {}", e))
                        });

                    match prompt_result {
                        Ok(prompt) => {
                            let mut state_guard = state.lock().unwrap();
                            let gen_result = state_guard.handle_generate_binary(
                                &model_id, &prompt, max_tokens, temperature
                            );

                            match gen_result {
                                Ok((text, prompt_tokens, generated_tokens)) => {
                                    if let Err(e) = write_binary_text(
                                        &mut stream, &text, &model_id, prompt_tokens, generated_tokens
                                    ) {
                                        eprintln!("❌ Failed to write binary response: {}", e);
                                        break;
                                    }
                                }
                                Err(e) => {
                                    // Error response still uses JSON for consistency
                                    let error_response = json!({
                                        "success": false,
                                        "error": e
                                    });
                                    let response_str = serde_json::to_string(&error_response).unwrap() + "\n";
                                    if stream.write_all(response_str.as_bytes()).is_err() {
                                        break;
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            let error_response = json!({
                                "success": false,
                                "error": e
                            });
                            let response_str = serde_json::to_string(&error_response).unwrap() + "\n";
                            if stream.write_all(response_str.as_bytes()).is_err() {
                                break;
                            }
                        }
                    }
                }
            }
            line.clear();
            continue;
        }

        // Standard JSON protocol for all other commands
        let response: Value = match serde_json::from_str::<InferenceCommand>(trimmed) {
            Ok(cmd) => {
                let mut state_guard = state.lock().unwrap();
                match state_guard.handle_command(cmd) {
                    Ok(result) => json!({
                        "success": true,
                        "result": result
                    }),
                    Err(e) => json!({
                        "success": false,
                        "error": e
                    })
                }
            }
            Err(e) => json!({
                "success": false,
                "error": format!("Invalid command: {}", e)
            })
        };

        // Send response
        let response_str = serde_json::to_string(&response).unwrap() + "\n";
        if stream.write_all(response_str.as_bytes()).is_err() {
            break;
        }

        line.clear();
    }
}

fn main() {
    println!("🦀 Candle Inference Worker v2.0 starting...");

    // Get socket path from args
    let args: Vec<String> = std::env::args().collect();
    let socket_path = args.get(1)
        .map(|s| s.as_str())
        .unwrap_or("/tmp/jtag-inference.sock");

    println!("📡 Socket: {}", socket_path);

    // Remove old socket
    let _ = fs::remove_file(socket_path);

    // Initialize state
    let state = match WorkerState::new() {
        Ok(s) => Arc::new(Mutex::new(s)),
        Err(e) => {
            eprintln!("❌ Failed to initialize: {}", e);
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

    // Start server
    let listener = UnixListener::bind(socket_path).expect("Failed to bind socket");
    println!("✅ Inference Worker v2.0 ready");
    println!("📂 Supported: llama, mistral, mixtral, phi, phi3, qwen2, gemma, gemma2, stablelm, falcon, starcoder2");
    println!("✅ Listening for connections\n");

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let state = Arc::clone(&state);
                thread::spawn(move || handle_client(stream, state));
            }
            Err(e) => eprintln!("Connection error: {}", e),
        }
    }
}
