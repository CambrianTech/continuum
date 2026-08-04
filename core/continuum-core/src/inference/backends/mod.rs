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

pub mod compact_llama_safetensors;
pub mod llama_gguf;
pub mod llama_safetensors;
pub mod llamacpp;
pub mod llamacpp_scheduler;
pub mod qwen2_safetensors;

// MLX adapter: macOS + `mlx` feature only. Gated here so non-Mac / feature-off
// builds don't see the module at all. Phase A scaffold — see continuum#897
// and docs/inference/MLX-BACKEND.md.
#[cfg(all(feature = "mlx", target_os = "macos"))]
pub mod mlx_adapter;

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use candle_core::quantized::gguf_file;
use candle_core::{Device, Tensor};
use candle_transformers::generation::LogitsProcessor;
use tokenizers::Tokenizer;

use crate::gpu::memory_manager::{GpuMemoryManager, GpuPriority, GpuSubsystem};
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

/// GPU sync interval during generation.
/// Higher = fewer CPU-GPU round trips = faster throughput.
/// The sampler pulls logits to CPU every token anyway, so this sync
/// is mainly to prevent Metal command buffer overflow on long sequences.
const GPU_SYNC_INTERVAL: usize = 64;

/// Check for NaN only on first N generated tokens.
// context-budget-exempt: how many decoded tokens the NaN sanity probe inspects — a health check, not a budget
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

    /// Token IDs that should NEVER appear in generated output.
    /// Control/special tokens (e.g., Qwen2's `<|endoftext|>`, `<|im_start|>`) that have
    /// inflated logits in quantized models. These are set to probability 0 during sampling.
    /// Default: empty (no suppression). Override per architecture.
    fn suppress_token_ids(&self) -> &[u32] {
        &[]
    }

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
    /// Returns logits from the final token position.
    /// All backends use full-batch prefill via Metal SDPA with is_causal=true.
    fn prefill(&mut self, tokens: &[u32]) -> Result<Tensor, String>;

    /// Clear KV cache for a fresh generation.
    fn clear_cache(&mut self) -> Result<(), String>;

    // ── Tokenization ──

    /// Tokenize text to token IDs (no special tokens — caller handles template).
    fn tokenize(&self, text: &str) -> Result<Vec<u32>, String>;

    /// Decode token IDs back to text.
    fn decode(&self, tokens: &[u32]) -> Result<String, String>;

    // ── Memory ──

    /// Estimated VRAM consumed by this model's weights (bytes).
    /// Used by GpuMemoryManager to track real allocations.
    /// Default: 0 (unknown). Backends should override with file-size-based estimate.
    fn estimated_vram_bytes(&self) -> u64 {
        0
    }

    // ── Optional: LoRA Support ──

    /// Whether this backend supports LoRA adapter merging.
    fn supports_lora(&self) -> bool {
        false
    }

    /// Rebuild model with stacked LoRA adapters merged into weights.
    /// `gpu_manager` enables transient spike tracking during the rebuild
    /// (memory temporarily doubles while old and new weights coexist).
    fn rebuild_with_lora(
        &mut self,
        _adapters: &[GenomeAdapter],
        _gpu_manager: Option<&Arc<GpuMemoryManager>>,
    ) -> Result<(), String> {
        Err("LoRA not supported by this backend".to_string())
    }

    /// Reload base model without any LoRA adapters.
    fn reload_base(&mut self) -> Result<(), String> {
        self.clear_cache()
    }
}

// ─── Unified Text Generation ─────────────────────────────────────────────────

/// Sampling configuration for text generation.
/// All fields are required — no silent defaults.
#[derive(Debug, Clone)]
pub struct SamplingConfig {
    /// Temperature for softmax sampling. 0.0 = greedy (argmax).
    pub temperature: f64,
    /// Repetition penalty applied on logits (llama.cpp style). 1.0 = disabled.
    pub repeat_penalty: f32,
    /// Top-k sampling: keep only the k highest-probability tokens. 0 = disabled.
    pub top_k: usize,
    /// Top-p (nucleus) sampling: keep smallest set of tokens with cumulative prob >= p. 1.0 = disabled.
    pub top_p: f64,
    /// GBNF grammar (e.g. JSON shape). When Some, scheduler attaches it
    /// to the sampler chain BEFORE temp/dist so output is constrained to
    /// match the grammar. None = unconstrained. Set by adapters when the
    /// caller's request_format demands a structured shape (JsonObject).
    pub grammar: Option<String>,
}

impl SamplingConfig {
    /// Config for code generation: greedy, moderate repeat penalty.
    pub fn code() -> Self {
        Self {
            temperature: 0.0,
            repeat_penalty: 1.1,
            top_k: 0,
            top_p: 1.0,
            grammar: None,
        }
    }
    /// Config for chat: slight creativity, standard repeat penalty.
    pub fn chat() -> Self {
        Self {
            temperature: 0.6,
            repeat_penalty: 1.1,
            top_k: 40,
            top_p: 0.95,
            grammar: None,
        }
    }
}

/// Built-in JSON grammar (GBNF) — produces a valid JSON object. Used when
/// callers request `response_format: JsonObject`. Keep this aligned with the
/// vendored llama.cpp `grammars/json.gbnf`.
pub const JSON_GRAMMAR: &str = r#"
root   ::= object
value  ::= object | array | string | number | ("true" | "false" | "null") ws

object ::=
  "{" ws (
            string ":" ws value
    ("," ws string ":" ws value)*
  )? "}" ws

array  ::=
  "[" ws (
            value
    ("," ws value)*
  )? "]" ws

string ::=
  "\"" (
    [^"\\\x7F\x00-\x1F] |
    "\\" (["\\bfnrt] | "u" [0-9a-fA-F]{4})
  )* "\"" ws

number ::= ("-"? ([0-9] | [1-9] [0-9]{0,15})) ("." [0-9]+)? ([eE] [-+]? [0-9] [1-9]{0,15})? ws
ws ::= | " " | "\n" [ \t]{0,20}
"#;

/// Generate text from a prompt using ANY ModelBackend.
///
/// One function for all local models. Handles:
/// - Context length validation
/// - Prefill via full-batch Metal SDPA
/// - Token generation with sampling
/// - NaN detection and prompt replay on failure
/// - GPU sync management
pub fn generate(
    backend: &mut dyn ModelBackend,
    prompt: &str,
    max_tokens: usize,
    sampling: &SamplingConfig,
) -> Result<(String, usize), String> {
    let log = runtime::logger("candle");
    let start = Instant::now();
    let rss_before = crate::system_resources::process_rss_mb();
    log.debug(&format!("generate start: RSS={}MB", rss_before));

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
            prompt_len,
            max_tokens,
            prompt_len + max_tokens,
            ctx_len
        ));
    }

    log.debug(&format!(
        "generate: {} prompt tokens, max_tokens={}, context={}, arch={}, format={:?}",
        prompt_len,
        max_tokens,
        ctx_len,
        backend.architecture(),
        backend.format()
    ));

    // Clear KV cache
    backend.clear_cache()?;

    // ── Phase 1: Prefill ──
    let prefill_start = Instant::now();
    let prefill_logits = backend.prefill(&prompt_tokens)?;
    backend
        .device()
        .synchronize()
        .map_err(|e| format!("Prefill sync: {e}"))?;
    let prefill_ms = prefill_start.elapsed().as_millis();
    log.info(&format!(
        "Prefill: {} tokens in {}ms ({:.1}ms/tok)",
        prompt_len,
        prefill_ms,
        prefill_ms as f64 / prompt_len as f64
    ));

    let prefill_logits = extract_last_logits(&prefill_logits)?;
    let (prefill_logits, had_nan) = sanitize_logits_with_flag(&prefill_logits, backend.device())?;
    if had_nan {
        log.error("NaN/Inf on prefill — prompt may be malformed or too long");
        save_prompt_replay(prompt, &prompt_tokens, "NaN on prefill");
        return Err(
            "Model produced NaN on prefill — prompt may be malformed or too long".to_string(),
        );
    }

    // Setup sampler from config — no hardcoded defaults.
    let use_greedy = sampling.temperature <= 0.0;
    let seed = 299792458u64; // deterministic seed
    let top_p = if sampling.top_p < 1.0 {
        Some(sampling.top_p)
    } else {
        None
    };
    let mut logits_processor = if use_greedy {
        // Greedy: we use our own argmax, but LogitsProcessor still needed as fallback
        LogitsProcessor::new(seed, Some(0.01), top_p)
    } else {
        LogitsProcessor::new(seed, Some(sampling.temperature), top_p)
    };

    log.info(&format!("Sampling: {:?}", sampling));

    // Debug: log token-level diagnostics if CANDLE_DEBUG_TOKENS is set
    let debug_tokens = std::env::var("CANDLE_DEBUG_TOKENS").is_ok();

    // Print top-10 logits from prefill for comparison with PyTorch
    if debug_tokens {
        let prefill_vec: Vec<f32> = prefill_logits
            .flatten_all()
            .and_then(|t| t.to_vec1())
            .unwrap_or_default();
        let mut indexed: Vec<(usize, f32)> = prefill_vec
            .iter()
            .enumerate()
            .map(|(i, &v)| (i, v))
            .collect();
        indexed.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        eprintln!("Top 10 logits after prefill (Candle GGUF):");
        for (rank, &(tid, val)) in indexed.iter().take(10).enumerate() {
            let decoded = backend.decode(&[tid as u32]).unwrap_or_else(|_| "?".into());
            eprintln!(
                "  {}. token={:>6} logit={:>8.3}  {:?}",
                rank + 1,
                tid,
                val,
                crate::utils::str_truncate::truncate_at_char_boundary(&decoded, 20)
            );
        }
        for &eos_id in backend.eos_token_ids() {
            if let Some(&val) = prefill_vec.get(eos_id as usize) {
                eprintln!("  EOS[{}] logit={:.3}", eos_id, val);
            }
        }
        // Print suppressed token logits for comparison with llama.cpp
        for &sid in backend.suppress_token_ids() {
            if let Some(&val) = prefill_vec.get(sid as usize) {
                let name = backend
                    .decode(&[sid])
                    .unwrap_or_else(|_| format!("?{}", sid));
                eprintln!("  suppress[{}] {:?} logit={:.3}", sid, name, val);
            }
        }
    }

    let mut all_tokens = prompt_tokens;

    let _eos_ids = backend.eos_token_ids().to_vec();

    // Tokens to suppress during generation (architecture-specific control tokens).
    let suppress_ids: Vec<usize> = backend
        .suppress_token_ids()
        .iter()
        .map(|&t| t as usize)
        .collect();

    // Sample first token from prefill logits
    let mut prefill_vec: Vec<f32> = prefill_logits
        .to_vec1()
        .map_err(|e| format!("Prefill logits to vec: {e}"))?;
    apply_logit_processing(&mut prefill_vec, &suppress_ids, &[], sampling);
    let first_token = if use_greedy {
        argmax_f32(&prefill_vec) as u32
    } else {
        let t = Tensor::from_slice(&prefill_vec, prefill_vec.len(), backend.device())
            .map_err(|e| format!("Prefill logits to tensor: {e}"))?;
        logits_processor
            .sample(&t)
            .map_err(|e| format!("First token sampling failed: {e}"))?
    };

    if backend.eos_token_ids().contains(&first_token) {
        return Ok((String::new(), 0));
    }
    all_tokens.push(first_token);

    // ── Phase 2: Generate ──
    let gen_start = Instant::now();
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
            log.warn(&format!(
                "Reached context limit ({}) at token {}",
                ctx_len, i
            ));
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
                    log.error(&format!(
                        "Multiple NaN in first {} tokens — aborting",
                        NAN_CHECK_TOKENS
                    ));
                    save_prompt_replay(
                        prompt,
                        &all_tokens[..prompt_len],
                        "Multiple NaN in early tokens",
                    );
                    break;
                }
            }
            sanitized
        } else {
            logits
        };

        // Apply suppress + repetition penalty + top-k on logits, then sample.
        // For greedy: operate entirely on Vec<f32> (no GPU round-trip).
        // For non-greedy: rebuild Tensor for LogitsProcessor.
        let mut logits_vec: Vec<f32> = logits
            .to_vec1()
            .map_err(|e| format!("Logits to vec: {e}"))?;
        apply_logit_processing(
            &mut logits_vec,
            &suppress_ids,
            &all_tokens[prompt_len..],
            sampling,
        );

        let next_token = sample_token(
            &logits_vec,
            use_greedy,
            &mut logits_processor,
            &logits,
            backend.device(),
            &mut nan_count,
            i,
            prompt,
            &all_tokens[..prompt_len],
            &log,
        )?;
        let next_token = match next_token {
            Some(t) => t,
            None => break, // nan_count exceeded
        };

        if debug_tokens {
            // Log: token ID, decoded text, logit stats, EOS logit rank
            let decoded = backend.decode(&[next_token]).unwrap_or_else(|_| "?".into());
            let logits_vec: Vec<f32> = logits
                .flatten_all()
                .and_then(|t| t.to_vec1())
                .unwrap_or_default();
            let max_logit = logits_vec.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
            let min_logit = logits_vec.iter().cloned().fold(f32::INFINITY, f32::min);

            // Check where EOS tokens rank in logits
            let mut eos_info = String::new();
            for &eos_id in backend.eos_token_ids() {
                if let Some(&eos_logit) = logits_vec.get(eos_id as usize) {
                    let rank = logits_vec.iter().filter(|&&v| v > eos_logit).count();
                    eos_info.push_str(&format!(" eos[{}]={:.2}(rank {})", eos_id, eos_logit, rank));
                }
            }

            eprintln!(
                "  tok[{:>3}] id={:<6} {:>20} logits=[{:.1}..{:.1}]{}",
                i,
                next_token,
                format!("{:?}", crate::utils::str_truncate::truncate_at_char_boundary(&decoded, 20)),
                min_logit,
                max_logit,
                eos_info
            );
        }

        if backend.eos_token_ids().contains(&next_token) {
            if debug_tokens {
                eprintln!("  → EOS hit: token {} at iteration {}", next_token, i);
            }
            break;
        }
        all_tokens.push(next_token);
        if debug_tokens && i <= 3 {
            eprintln!(
                "  → generated token {} at pos {}, total tokens {}",
                next_token,
                pos,
                all_tokens.len()
            );
        }
    }

    // Final GPU sync + KV cache cleanup to prevent memory accumulation
    // across sequential generations (e.g. 98-challenge benchmarks).
    backend
        .device()
        .synchronize()
        .map_err(|e| format!("Final GPU sync failed: {e}"))?;

    // Decode BEFORE clearing cache (cache not needed for decode)
    let generated_tokens = &all_tokens[prompt_len..];
    let output_text = backend.decode(generated_tokens)?;

    // Clear KV cache immediately after generation to free GPU memory.
    // Without this, Metal buffer pools accumulate across sequential runs.
    backend.clear_cache()?;

    // Release unused Metal buffers from the allocation pool.
    // clear_cache() drops KV tensors (Arc count → 1 = only pool holds ref).
    // release_unused_buffers() removes those from the pool, freeing the MTLBuffers.
    // Without this, the pool grows indefinitely across sequential inferences.
    // See: https://github.com/huggingface/candle/issues/2271
    #[cfg(feature = "metal")]
    if backend.device().is_metal() {
        if let Ok(metal) = backend.device().as_metal_device() {
            metal
                .release_unused_buffers()
                .map_err(|e| format!("Metal pool cleanup: {e}"))?;
        }
    }

    let gen_ms = gen_start.elapsed().as_millis();
    let gen_count = generated_tokens.len();
    let gen_tok_s = if gen_ms > 0 {
        (gen_count as f64 / gen_ms as f64) * 1000.0
    } else {
        0.0
    };
    log.info(&format!(
        "Generation: {} tokens in {}ms ({:.1} tok/s)",
        gen_count, gen_ms, gen_tok_s
    ));

    let rss_after = crate::system_resources::process_rss_mb();
    let duration = start.elapsed();
    log.info(&format!(
        "Total: {} tokens in {:?} (arch={}, format={:?}, prefill={}tok/{}ms, gen={:.1}tok/s, RSS={}→{}MB Δ{}MB)",
        gen_count,
        duration,
        backend.architecture(),
        backend.format(),
        prompt_len, prefill_ms, gen_tok_s,
        rss_before,
        rss_after,
        rss_after as i64 - rss_before as i64
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
    let mut file = std::fs::File::open(path).map_err(|e| format!("Failed to open GGUF: {e}"))?;
    let content =
        gguf_file::Content::read(&mut file).map_err(|e| format!("Failed to read GGUF: {e}"))?;

    // general.architecture is REQUIRED — silently falling back to "llama" would
    // route a qwen/mistral/phi/etc. model through the wrong backend and produce
    // garbage output or outright crash. Rule-2 violation (fallbacks are illegal)
    // fixed 2026-04-23. If a GGUF is missing this metadata, that's a broken file,
    // not a thing to paper over. Read via the ONE shared canonical-key reader.
    let architecture = crate::inference_capability::gguf_keys::architecture(&content).ok_or_else(
        || {
            format!(
                "GGUF {} is missing required metadata key 'general.architecture' — cannot \
             determine backend. Silent fallback to 'llama' has been removed; fix the \
             GGUF file or re-export it with proper metadata.",
                path.display()
            )
        },
    )?;

    // context_length via the shared reader: architecture-specific key first,
    // then the historical `llama.context_length` fallback — the ONE place that
    // policy is defined. If neither exists, that's a broken GGUF, not a thing
    // to guess 4096 for.
    let context_length =
        crate::inference_capability::gguf_keys::context_length(&content, &architecture)
            .map(|v| v as usize)
            .ok_or_else(|| {
                format!(
                    "GGUF {} (architecture={architecture}) is missing context_length metadata \
             (tried '{architecture}.context_length' and 'llama.context_length'). Silent \
             fallback to 4096 has been removed; fix the GGUF file.",
                    path.display()
                )
            })?;

    let model_name = crate::inference_capability::gguf_keys::general_name(&content);

    Ok(GgufMetadata {
        architecture,
        context_length,
        model_name,
    })
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

    let mut file =
        std::fs::File::open(model_path).map_err(|e| format!("Failed to open GGUF: {e}"))?;
    let content =
        gguf_file::Content::read(&mut file).map_err(|e| format!("Failed to read GGUF: {e}"))?;

    // Same fallback prohibition as parse_gguf_metadata above — broken GGUF
    // metadata must surface as an error, not be guessed into the llama backend.
    let architecture = content
        .metadata
        .get("general.architecture")
        .and_then(|v| v.to_string().ok())
        .cloned()
        .ok_or_else(|| {
            format!(
                "GGUF {} is missing required 'general.architecture' metadata — cannot \
             determine backend. Fix the GGUF file or re-export it with proper metadata.",
                model_path.display()
            )
        })?;

    log.info(&format!("GGUF architecture: {architecture}"));

    let mut reader = std::io::BufReader::new(
        std::fs::File::open(model_path).map_err(|e| format!("Failed to reopen GGUF: {e}"))?,
    );

    match architecture.as_str() {
        "llama" => {
            let backend = llama_gguf::LlamaGgufBackend::from_gguf(
                content,
                &mut reader,
                tokenizer,
                model_id,
                model_path,
                device,
            )?;
            log.info(&format!(
                "Loaded Llama GGUF backend: context_length={}",
                backend.context_length()
            ));
            Ok(Box::new(backend))
        }
        // Qwen2 uses the same GGUF format as Llama (same tensor layout in GGUF).
        // The architecture metadata is "qwen2" but the model structure is compatible.
        "qwen2" => {
            let backend = llama_gguf::LlamaGgufBackend::from_gguf(
                content,
                &mut reader,
                tokenizer,
                model_id,
                model_path,
                device,
            )?;
            log.info(&format!(
                "Loaded Qwen2 via Llama GGUF backend: context_length={}",
                backend.context_length()
            ));
            Ok(Box::new(backend))
        }
        // Qwen3.5 — hybrid DeltaNet + Attention architecture.
        // The Candle implementation (Qwen35GgufBackend + vendored
        // quantized_qwen35) was deleted in #1273 — it was vestigial
        // post-llama.cpp migration; production routes Qwen3.5 through
        // LlamaCppAdapter, not through this Candle-side load path.
        "qwen3" | "qwen35" => Err(
            "Qwen3.5 GGUF routing through the Candle backend was removed in #1273. \
             Use LlamaCppAdapter (the production hot path) — it owns Qwen3.5 inference \
             via the bundled llama.cpp library. The Candle path was unreachable from \
             AIProviderModule::register_adapters and only kept the vendored DeltaNet \
             + Attention recurrence loop alive as dead code."
                .to_string(),
        ),
        // Future architectures:
        // "phi3" => { phi3_gguf::... }
        other => Err(format!(
            "Unsupported GGUF architecture: '{other}'. \
             Supported: llama, qwen2 (via Llama backend). \
             Qwen3.5 routes through LlamaCppAdapter, not this loader. \
             Add a new backend in inference/backends/ to support this architecture."
        )),
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Argmax over a float slice — returns index of the largest value.
fn argmax_f32(data: &[f32]) -> usize {
    data.iter()
        .enumerate()
        .fold((0usize, f32::NEG_INFINITY), |(bi, bv), (i, &v)| {
            if v > bv {
                (i, v)
            } else {
                (bi, bv)
            }
        })
        .0
}

/// Apply token suppression, repetition penalty, and top-k filtering on a logits vector.
fn apply_logit_processing(
    logits: &mut Vec<f32>,
    suppress_ids: &[usize],
    generated_tokens: &[u32],
    sampling: &SamplingConfig,
) {
    // Suppress control tokens
    for &tid in suppress_ids {
        if tid < logits.len() {
            logits[tid] = f32::NEG_INFINITY;
        }
    }
    // Repetition penalty (llama.cpp style: divide positive, multiply negative)
    if sampling.repeat_penalty != 1.0 {
        for &token_id in generated_tokens {
            let idx = token_id as usize;
            if idx < logits.len() {
                if logits[idx] > 0.0 {
                    logits[idx] /= sampling.repeat_penalty;
                } else {
                    logits[idx] *= sampling.repeat_penalty;
                }
            }
        }
    }
    // Top-k: keep only the k highest logits, set rest to -inf.
    // Uses select_nth_unstable (O(n) average) instead of full sort (O(n log n)).
    if sampling.top_k > 0 && sampling.top_k < logits.len() {
        let mut scratch = logits.clone();
        scratch.select_nth_unstable_by(sampling.top_k, |a, b| {
            b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal)
        });
        let threshold = scratch[sampling.top_k];
        for v in logits.iter_mut() {
            if *v < threshold {
                *v = f32::NEG_INFINITY;
            }
        }
    }
}

/// Sample a token from processed logits. Returns None if nan_count exceeded (caller should break).
/// For greedy: scans the Vec directly (no GPU round-trip).
/// For non-greedy: rebuilds Tensor for LogitsProcessor.
#[allow(clippy::too_many_arguments)]
fn sample_token(
    logits_vec: &[f32],
    use_greedy: bool,
    logits_processor: &mut LogitsProcessor,
    _logits_tensor: &Tensor, // original tensor for device reference
    device: &Device,
    nan_count: &mut u32,
    token_idx: usize,
    prompt: &str,
    prompt_tokens: &[u32],
    log: &std::sync::Arc<crate::runtime::ModuleLogger>,
) -> Result<Option<u32>, String> {
    if use_greedy {
        let token = argmax_f32(logits_vec) as u32;
        *nan_count = 0;
        Ok(Some(token))
    } else {
        let logits = Tensor::from_slice(logits_vec, logits_vec.len(), device)
            .map_err(|e| format!("Logits to tensor: {e}"))?;
        match logits_processor.sample(&logits) {
            Ok(token) => {
                *nan_count = 0;
                Ok(Some(token))
            }
            Err(e) => {
                *nan_count += 1;
                if *nan_count > 5 {
                    log.warn(&format!(
                        "Aborting after {} consecutive NaN errors",
                        nan_count
                    ));
                    save_prompt_replay(
                        prompt,
                        prompt_tokens,
                        &format!("{} consecutive NaN", nan_count),
                    );
                    return Ok(None);
                }
                log.warn(&format!(
                    "Sampling failed at token {}, retrying: {}",
                    token_idx, e
                ));
                let (sanitized, _) = sanitize_logits_with_flag(&logits, device)?;
                let token = logits_processor
                    .sample(&sanitized)
                    .map_err(|e| format!("Sampling failed even after sanitization: {e}"))?;
                Ok(Some(token))
            }
        }
    }
}

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
fn sanitize_logits_with_flag(logits: &Tensor, device: &Device) -> Result<(Tensor, bool), String> {
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
                    if x > 0.0 {
                        100.0
                    } else {
                        -100.0
                    }
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
    let home = dirs::home_dir().expect("Failed to resolve home directory");
    let replay_dir = home
        .join(".continuum")
        .join("jtag")
        .join("logs")
        .join("prompt-replays");
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
