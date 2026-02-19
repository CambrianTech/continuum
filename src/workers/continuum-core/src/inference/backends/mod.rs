//! Model Backends — Unified Interface for ALL Local Inference
//!
//! Every local model (GGUF quantized, safetensors BF16/FP32) implements the
//! `ModelBackend` trait. The model file is the single source of truth for
//! capabilities: context_length, EOS tokens, architecture.
//!
//! Adding a new model format/architecture:
//!   1. Create `backends/<arch>_<format>.rs` implementing `ModelBackend`
//!   2. Add `pub mod <name>;` below
//!   3. Add factory function or match arm in load functions
//!
//! The trait abstracts: forward pass, prefill strategy, context length,
//! EOS tokens, tokenization, cache management, and LoRA support.
//! One `generate()` function works with ANY backend.

pub mod llama_gguf;
pub mod llama_safetensors;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use candle_core::{Device, Tensor};
use candle_core::quantized::gguf_file;
use candle_transformers::generation::LogitsProcessor;
use rand::Rng;
use tokenizers::Tokenizer;

use crate::inference::lora::LoRAWeights;
use crate::runtime;

// ─── Model Format ────────────────────────────────────────────────────────────

/// Model serialization format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelFormat {
    /// GGUF quantized (Q4_K_M, Q8_0, etc.)
    Gguf,
    /// Safetensors (BF16, FP16, FP32)
    Safetensors,
}

// ─── LoRA Adapter ────────────────────────────────────────────────────────────

/// Adapter entry for genome stacking.
/// Moved here so the trait can reference it without circular deps.
pub struct GenomeAdapter {
    pub adapter_id: String,
    pub weights: HashMap<String, LoRAWeights>,
    pub scale: f64,
}

// ─── ModelBackend Trait ──────────────────────────────────────────────────────

/// GPU sync interval during token-by-token prefill and generation.
const GPU_SYNC_INTERVAL: usize = 16;

/// Check for NaN only on first N generated tokens.
const NAN_CHECK_TOKENS: usize = 3;

/// Unified trait for ALL local model backends.
///
/// Every local model — regardless of format (GGUF, safetensors) or
/// architecture (Llama, Qwen, Phi) — implements this trait. The model
/// file is the single source of truth for all capabilities.
///
/// CandleAdapter holds `Box<dyn ModelBackend>` and calls `generate()`.
/// No switch statements, no format-specific code in the adapter.
pub trait ModelBackend: Send + Sync {
    // ── Identity & Capabilities (from model metadata) ──

    /// Architecture name from model metadata (e.g., "llama", "qwen2", "phi3")
    fn architecture(&self) -> &str;

    /// Context length from model metadata — the model's true maximum.
    /// GGUF: `llama.context_length`. Safetensors: `config.max_position_embeddings`.
    fn context_length(&self) -> usize;

    /// EOS token IDs for this model, read from model metadata.
    fn eos_token_ids(&self) -> &[u32];

    /// Model identifier (HuggingFace repo ID or filename).
    fn model_id(&self) -> &str;

    /// Serialization format of this model.
    fn format(&self) -> ModelFormat;

    /// Compute device this model is loaded on.
    fn device(&self) -> &Device;

    // ── Inference ──

    /// Forward pass: process input tensor at given position, return logits.
    fn forward(&mut self, input: &Tensor, index_pos: usize) -> Result<Tensor, candle_core::Error>;

    /// Prefill: process prompt tokens to build KV cache before generation.
    ///
    /// Returns logits from the final token position. Each backend chooses
    /// its own strategy:
    /// - GGUF: token-by-token (seq_len=1 each, Metal SDPA safe)
    /// - Safetensors BF16: full-batch (proper causal masking, GPU-efficient)
    fn prefill(&mut self, tokens: &[u32]) -> Result<Tensor, String>;

    /// Clear KV cache for a fresh generation.
    fn clear_cache(&mut self) -> Result<(), String>;

    // ── Tokenization ──

    /// Tokenize text to token IDs (no special tokens — caller handles template).
    fn tokenize(&self, text: &str) -> Result<Vec<u32>, String>;

    /// Decode token IDs back to text.
    fn decode(&self, tokens: &[u32]) -> Result<String, String>;

    // ── Optional: LoRA Support ──

    /// Whether this backend supports LoRA adapter merging.
    fn supports_lora(&self) -> bool { false }

    /// Rebuild model with stacked LoRA adapters merged into weights.
    fn rebuild_with_lora(&mut self, _adapters: &[GenomeAdapter]) -> Result<(), String> {
        Err("LoRA not supported by this backend".to_string())
    }

    /// Reload base model without any LoRA adapters.
    fn reload_base(&mut self) -> Result<(), String> {
        self.clear_cache()
    }
}

// ─── Unified Text Generation ─────────────────────────────────────────────────

/// Generate text from a prompt using ANY ModelBackend.
///
/// One function for all local models. Handles:
/// - Context length validation
/// - Prefill via backend strategy (token-by-token or full-batch)
/// - Token generation with sampling
/// - NaN detection and prompt replay on failure
/// - GPU sync management
pub fn generate(
    backend: &mut dyn ModelBackend,
    prompt: &str,
    max_tokens: usize,
    temperature: f64,
) -> Result<(String, usize), String> {
    let log = runtime::logger("candle");
    let start = Instant::now();

    // Tokenize
    let prompt_tokens = backend.tokenize(prompt)?;
    let prompt_len = prompt_tokens.len();

    if prompt_len == 0 {
        return Err("Empty prompt".to_string());
    }

    // Validate against model context length — hard error if prompt too large.
    // If this fires, the RAG builder upstream has a bug (wrong context window).
    let ctx_len = backend.context_length();
    if prompt_len + max_tokens > ctx_len {
        return Err(format!(
            "Prompt ({} tokens) + max_tokens ({}) = {} exceeds context length ({}). \
             RAG builder must respect the model's context window.",
            prompt_len, max_tokens, prompt_len + max_tokens, ctx_len
        ));
    }

    log.debug(&format!(
        "generate: {} prompt tokens, max_tokens={}, context={}, arch={}, format={:?}",
        prompt_len, max_tokens, ctx_len, backend.architecture(), backend.format()
    ));

    // Clear KV cache
    backend.clear_cache()?;

    // ── Phase 1: Prefill ──
    let prefill_logits = backend.prefill(&prompt_tokens)?;
    let prefill_logits = extract_last_logits(&prefill_logits)?;
    let (prefill_logits, had_nan) = sanitize_logits_with_flag(&prefill_logits, backend.device())?;
    if had_nan {
        log.error("NaN/Inf on prefill — prompt may be malformed or too long");
        save_prompt_replay(prompt, &prompt_tokens, "NaN on prefill");
        return Err("Model produced NaN on prefill — prompt may be malformed or too long".to_string());
    }

    // Setup sampler
    let seed = rand::thread_rng().gen::<u64>();
    let mut logits_processor = LogitsProcessor::new(seed, Some(temperature), None);

    let mut all_tokens = prompt_tokens;

    // Sample first token from prefill logits
    let first_token = logits_processor
        .sample(&prefill_logits)
        .map_err(|e| format!("First token sampling failed: {e}"))?;

    if backend.eos_token_ids().contains(&first_token) {
        return Ok((String::new(), 0));
    }
    all_tokens.push(first_token);

    // ── Phase 2: Generate ──
    let mut nan_count = 0;

    for i in 1..max_tokens {
        let token = *all_tokens.last().ok_or("Empty token sequence")?;
        let input = Tensor::new(&[token], backend.device())
            .map_err(|e| format!("Tensor creation failed: {e}"))?
            .unsqueeze(0)
            .map_err(|e| format!("Unsqueeze failed: {e}"))?;

        let pos = all_tokens.len() - 1;

        // Context length guard
        if pos >= ctx_len {
            log.warn(&format!("Reached context limit ({}) at token {}", ctx_len, i));
            break;
        }

        let logits = backend
            .forward(&input, pos)
            .map_err(|e| format!("Forward failed at token {i}: {e}"))?;

        // GPU sync periodically
        if (i + 1) % GPU_SYNC_INTERVAL == 0 {
            backend
                .device()
                .synchronize()
                .map_err(|e| format!("GPU sync failed: {e}"))?;
        }

        let logits = extract_last_logits(&logits)?;

        // NaN check on early tokens only
        let logits = if i < NAN_CHECK_TOKENS {
            let (sanitized, had_nan) = sanitize_logits_with_flag(&logits, backend.device())?;
            if had_nan {
                nan_count += 1;
                if nan_count > 2 {
                    log.error(&format!("Multiple NaN in first {} tokens — aborting", NAN_CHECK_TOKENS));
                    save_prompt_replay(prompt, &all_tokens[..prompt_len], "Multiple NaN in early tokens");
                    break;
                }
            }
            sanitized
        } else {
            logits
        };

        // Sample next token
        let next_token = match logits_processor.sample(&logits) {
            Ok(token) => {
                nan_count = 0;
                token
            }
            Err(e) => {
                nan_count += 1;
                if nan_count > 5 {
                    log.warn(&format!("Aborting after {} consecutive NaN errors", nan_count));
                    save_prompt_replay(prompt, &all_tokens[..prompt_len], &format!("{} consecutive NaN", nan_count));
                    break;
                }
                log.warn(&format!("Sampling failed at token {}, retrying: {}", i, e));
                let (sanitized, _) = sanitize_logits_with_flag(&logits, backend.device())?;
                logits_processor
                    .sample(&sanitized)
                    .map_err(|e| format!("Sampling failed even after sanitization: {e}"))?
            }
        };

        if backend.eos_token_ids().contains(&next_token) {
            break;
        }
        all_tokens.push(next_token);
    }

    // Final GPU sync
    backend
        .device()
        .synchronize()
        .map_err(|e| format!("Final GPU sync failed: {e}"))?;

    // Decode
    let generated_tokens = &all_tokens[prompt_len..];
    let output_text = backend.decode(generated_tokens)?;

    let duration = start.elapsed();
    log.info(&format!(
        "Generated {} tokens in {:?} (arch={}, format={:?}, prefill={})",
        generated_tokens.len(),
        duration,
        backend.architecture(),
        backend.format(),
        prompt_len
    ));

    Ok((output_text, generated_tokens.len()))
}

// ─── GGUF Metadata ───────────────────────────────────────────────────────────

/// GGUF metadata extracted before backend construction.
pub struct GgufMetadata {
    pub architecture: String,
    pub context_length: usize,
    pub model_name: Option<String>,
}

/// Read common metadata from a GGUF file without loading weights.
pub fn read_gguf_metadata(path: &Path) -> Result<GgufMetadata, String> {
    let mut file = std::fs::File::open(path)
        .map_err(|e| format!("Failed to open GGUF: {e}"))?;
    let content = gguf_file::Content::read(&mut file)
        .map_err(|e| format!("Failed to read GGUF: {e}"))?;

    let architecture = content
        .metadata
        .get("general.architecture")
        .and_then(|v| v.to_string().ok())
        .cloned()
        .unwrap_or_else(|| "llama".to_string());

    let context_length = content
        .metadata
        .get(&format!("{architecture}.context_length"))
        .and_then(|v| v.to_u32().ok())
        .map(|v| v as usize)
        .unwrap_or(4096);

    let model_name = content
        .metadata
        .get("general.name")
        .and_then(|v| v.to_string().ok())
        .cloned();

    Ok(GgufMetadata { architecture, context_length, model_name })
}

/// Load a GGUF model as a ModelBackend.
///
/// Reads `general.architecture` from metadata and instantiates the correct backend.
/// The tokenizer is loaded separately and passed in.
pub fn load_gguf_backend(
    model_path: &Path,
    tokenizer: Tokenizer,
    model_id: &str,
    device: &Device,
) -> Result<Box<dyn ModelBackend>, String> {
    let log = runtime::logger("candle");

    let mut file = std::fs::File::open(model_path)
        .map_err(|e| format!("Failed to open GGUF: {e}"))?;
    let content = gguf_file::Content::read(&mut file)
        .map_err(|e| format!("Failed to read GGUF: {e}"))?;

    let architecture = content
        .metadata
        .get("general.architecture")
        .and_then(|v| v.to_string().ok())
        .cloned()
        .unwrap_or_else(|| "llama".to_string());

    log.info(&format!("GGUF architecture: {architecture}"));

    let mut reader = std::io::BufReader::new(
        std::fs::File::open(model_path)
            .map_err(|e| format!("Failed to reopen GGUF: {e}"))?,
    );

    match architecture.as_str() {
        "llama" => {
            let backend = llama_gguf::LlamaGgufBackend::from_gguf(
                content, &mut reader, tokenizer, model_id, model_path, device,
            )?;
            log.info(&format!(
                "Loaded Llama GGUF backend: context_length={}",
                backend.context_length()
            ));
            Ok(Box::new(backend))
        }
        // Future architectures:
        // "qwen2" => { llama_gguf or qwen2_gguf::... }
        // "phi3" => { phi3_gguf::... }
        other => Err(format!(
            "Unsupported GGUF architecture: '{other}'. \
             Supported: llama. \
             Add a new backend in inference/backends/ to support this architecture."
        )),
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Extract logits for the last token position from model output.
fn extract_last_logits(logits: &Tensor) -> Result<Tensor, String> {
    let logits = logits
        .squeeze(0)
        .map_err(|e| format!("Squeeze failed: {e}"))?;
    if logits.dims().len() > 1 {
        logits
            .get(logits.dims()[0] - 1)
            .map_err(|e| format!("Get last failed: {e}"))
    } else {
        Ok(logits)
    }
}

/// Sanitize logits to prevent NaN/Inf from crashing the sampler.
fn sanitize_logits_with_flag(
    logits: &Tensor,
    device: &Device,
) -> Result<(Tensor, bool), String> {
    let logits_vec: Vec<f32> = logits
        .to_vec1()
        .map_err(|e| format!("Failed to read logits: {e}"))?;

    let has_bad_values = logits_vec.iter().any(|&x| x.is_nan() || x.is_infinite());

    if has_bad_values {
        runtime::logger("candle").warn("Detected NaN/Inf in logits, applying sanitization");

        let sanitized: Vec<f32> = logits_vec
            .iter()
            .map(|&x| {
                if x.is_nan() {
                    -100.0
                } else if x.is_infinite() {
                    if x > 0.0 { 100.0 } else { -100.0 }
                } else {
                    x
                }
            })
            .collect();

        let tensor = Tensor::from_vec(sanitized, logits.dims(), device)
            .map_err(|e| format!("Failed to create sanitized tensor: {e}"))?;
        Ok((tensor, true))
    } else {
        Ok((logits.clone(), false))
    }
}

/// Save a failed prompt to disk for replay in tests.
fn save_prompt_replay(prompt: &str, tokens: &[u32], error: &str) {
    let log = runtime::logger("candle");
    let replay_dir = PathBuf::from(".continuum/jtag/logs/prompt-replays");
    if std::fs::create_dir_all(&replay_dir).is_err() {
        log.warn("Failed to create prompt-replays directory");
        return;
    }

    let filename = format!("{}.json", chrono::Utc::now().format("%Y%m%d_%H%M%S_%3f"));
    let data = serde_json::json!({
        "prompt": prompt,
        "token_count": tokens.len(),
        "error": error,
        "timestamp": chrono::Utc::now().to_rfc3339(),
    });

    match std::fs::write(replay_dir.join(&filename), data.to_string()) {
        Ok(()) => log.info(&format!("Saved prompt replay: {}", filename)),
        Err(e) => log.warn(&format!("Failed to save prompt replay: {}", e)),
    }
}
