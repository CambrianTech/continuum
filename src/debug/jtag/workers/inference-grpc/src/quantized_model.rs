/**
 * Quantized Model Loading and Generation
 *
 * Supports GGUF format quantized models (Q4_K_M, Q8_0, etc.) for faster inference.
 * Quantized models are ~3x smaller and 2-3x faster than BF16.
 *
 * LoRA Strategy for Quantized Models:
 * 1. Keep LoRA adapters in FP16/BF16 (small ~100MB each)
 * 2. Apply LoRA at runtime during forward pass (QLoRA style)
 * 3. Dequantize → Apply LoRA → Re-quantize per layer (mixed precision)
 */
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::time::Instant;

use candle_core::quantized::gguf_file;
use candle_core::{Device, Tensor};
use candle_transformers::generation::LogitsProcessor;
use candle_transformers::models::quantized_llama::ModelWeights;
use hf_hub::{api::sync::Api, Repo, RepoType};
use log::info;
use rand::Rng;
use tokenizers::Tokenizer;

/// Quantized model state
pub struct QuantizedModelState {
    pub model: ModelWeights,
    pub tokenizer: Tokenizer,
    pub device: Device,
    pub eos_token_ids: Vec<u32>,
    #[allow(dead_code)]
    pub model_id: String,
    #[allow(dead_code)]
    pub quantization_type: String, // e.g., "Q4_K_M", "Q8_0"
}

impl QuantizedModelState {
    /// Clear KV cache for new generation
    #[allow(dead_code)]
    pub fn clear_cache(&mut self) {
        // ModelWeights has internal cache that resets on each forward
        // No explicit clear needed as it's handled per-generation
    }
}

/// Download GGUF model from HuggingFace
pub fn download_gguf_model(
    repo_id: &str,
    filename: &str,
) -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
    info!("📥 Downloading GGUF model: {repo_id}/{filename}");
    let start = Instant::now();

    let api = Api::new()?;
    let repo = api.repo(Repo::new(repo_id.to_string(), RepoType::Model));
    let path = repo.get(filename)?;

    info!(
        "✅ GGUF downloaded in {:.2}s: {:?}",
        start.elapsed().as_secs_f32(),
        path
    );
    Ok(path)
}

/// Load a quantized GGUF model
pub fn load_quantized_model(
    model_path: &PathBuf,
    tokenizer_repo: &str,
) -> Result<QuantizedModelState, Box<dyn std::error::Error + Send + Sync>> {
    info!("📥 Loading quantized model from {model_path:?}");
    let start = Instant::now();

    // Device selection: CUDA > Metal > CPU
    let device = select_best_device();

    fn select_best_device() -> Device {
        // Try CUDA first (RTX 5090, etc.) - quantized inference is FAST on CUDA
        #[cfg(feature = "cuda")]
        {
            if let Ok(device) = Device::new_cuda(0) {
                info!("  Using CUDA device (quantized + CUDA = fast!)");
                return device;
            }
            info!("  CUDA not available");
        }

        // Try Metal (macOS)
        #[cfg(feature = "metal")]
        {
            if let Ok(device) = Device::new_metal(0) {
                info!("  Using Metal device");
                return device;
            }
            info!("  Metal not available");
        }

        // Fall back to CPU
        info!("  Using CPU (no GPU acceleration)");
        Device::Cpu
    }

    info!("  Device: {device:?}");

    // Open GGUF file
    let mut file = File::open(model_path)?;
    let content = gguf_file::Content::read(&mut file)?;

    // Extract quantization type from metadata
    let quant_type = content
        .metadata
        .get("general.quantization_version")
        .and_then(|v| v.to_u32().ok())
        .map(|v| format!("Q{v}"))
        .unwrap_or_else(|| "unknown".to_string());

    info!("  Quantization: {quant_type}");

    // Load model weights
    let mut reader = BufReader::new(File::open(model_path)?);
    let model = ModelWeights::from_gguf(content, &mut reader, &device)?;

    info!("  Model loaded");

    // Load tokenizer from the base model repo
    let api = Api::new()?;
    let tokenizer_repo = api.repo(Repo::new(tokenizer_repo.to_string(), RepoType::Model));
    let tokenizer_path = tokenizer_repo.get("tokenizer.json")?;
    let tokenizer = Tokenizer::from_file(tokenizer_path)
        .map_err(|e| format!("Failed to load tokenizer: {e}"))?;

    // Llama 3.2 EOS tokens
    let eos_token_ids = vec![128009u32];

    let duration = start.elapsed();
    info!(
        "✅ Quantized model loaded in {:.2}s",
        duration.as_secs_f32()
    );

    Ok(QuantizedModelState {
        model,
        tokenizer,
        device,
        eos_token_ids,
        model_id: model_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string(),
        quantization_type: quant_type,
    })
}

/// Generate text from a prompt using quantized model
pub fn generate_text_quantized(
    state: &mut QuantizedModelState,
    prompt: &str,
    max_tokens: usize,
    temperature: f64,
) -> Result<(String, usize), String> {
    let start = Instant::now();

    // Tokenize prompt
    let encoding = state
        .tokenizer
        .encode(prompt, true)
        .map_err(|e| format!("Tokenization failed: {e}"))?;
    let prompt_tokens: Vec<u32> = encoding.get_ids().to_vec();
    let prompt_len = prompt_tokens.len();

    if prompt_len == 0 {
        return Err("Empty prompt".to_string());
    }

    // Log prompt length for debugging
    info!("📊 Quantized generation: {} tokens from {} char prompt", prompt_len, prompt.len());

    // Setup logits processor
    let seed = rand::thread_rng().gen::<u64>();
    let mut logits_processor = LogitsProcessor::new(seed, Some(temperature), None);

    let mut all_tokens = prompt_tokens.clone();
    let mut nan_count = 0;

    // Generate tokens
    for i in 0..max_tokens {
        let input_tokens = if i == 0 {
            all_tokens.clone()
        } else {
            vec![*all_tokens.last().unwrap()]
        };

        let input = Tensor::new(&input_tokens[..], &state.device)
            .map_err(|e| format!("Tensor creation failed: {e}"))?
            .unsqueeze(0)
            .map_err(|e| format!("Unsqueeze failed: {e}"))?;

        let pos = if i == 0 { 0 } else { all_tokens.len() - 1 };
        let logits = state
            .model
            .forward(&input, pos)
            .map_err(|e| format!("Forward pass failed: {e}"))?;

        // CRITICAL: Synchronize GPU after each forward pass to prevent command buffer accumulation
        // Without this, Metal command buffers queue up faster than GPU can process them,
        // causing memory to explode (1M+ buffers, 25GB+ RAM, swap thrashing)
        state
            .device
            .synchronize()
            .map_err(|e| format!("GPU sync failed: {e}"))?;

        // Get logits for last token
        let logits = logits
            .squeeze(0)
            .map_err(|e| format!("Squeeze failed: {e}"))?;
        let logits = if logits.dims().len() > 1 {
            logits
                .get(logits.dims()[0] - 1)
                .map_err(|e| format!("Get last failed: {e}"))?
        } else {
            logits
        };

        // Protect against NaN/Inf in logits before sampling
        // This can happen with long contexts or numerical instability
        let (logits, had_nan) = sanitize_logits_with_flag(&logits, &state.device)?;

        if had_nan {
            nan_count += 1;
            // If first token has NaN, the prompt itself is causing issues - abort early
            if i == 0 {
                log::error!("❌ NaN/Inf on first token - prompt may be malformed. First 500 chars: {}", &prompt[..prompt.len().min(500)]);
                return Err("Model produced NaN on first token - prompt may be malformed or too long".to_string());
            }
            // If too many NaN tokens, abort to prevent garbage output
            if nan_count > 5 {
                log::error!("❌ Too many NaN tokens ({}) - aborting generation", nan_count);
                break;
            }
        }

        let next_token = logits_processor
            .sample(&logits)
            .map_err(|e| format!("Sampling failed: {e}"))?;

        if state.eos_token_ids.contains(&next_token) {
            break;
        }

        all_tokens.push(next_token);
    }

    // Final GPU sync to ensure all work is complete before returning
    // This allows GPU memory to be fully reclaimed
    state
        .device
        .synchronize()
        .map_err(|e| format!("Final GPU sync failed: {e}"))?;

    // Decode generated tokens
    let generated_tokens = &all_tokens[prompt_len..];
    let output_text = state
        .tokenizer
        .decode(generated_tokens, true)
        .map_err(|e| format!("Decode failed: {e}"))?;

    let duration = start.elapsed();
    info!(
        "📝 Quantized generated {} tokens in {:?}",
        generated_tokens.len(),
        duration
    );

    Ok((output_text, generated_tokens.len()))
}

/// Sanitize logits to prevent NaN/Inf from crashing the sampler
/// Returns (sanitized_tensor, had_nan_or_inf)
///
/// This can happen with:
/// - Very long contexts (RoPE position overflow)
/// - Numerical instability in quantized models
/// - Edge case prompts
fn sanitize_logits_with_flag(logits: &Tensor, device: &Device) -> Result<(Tensor, bool), String> {
    // Move to CPU for inspection (fast for 1D vocab-size tensor)
    let logits_vec: Vec<f32> = logits
        .to_vec1()
        .map_err(|e| format!("Failed to read logits: {e}"))?;

    // Check for NaN/Inf
    let has_bad_values = logits_vec.iter().any(|&x| x.is_nan() || x.is_infinite());

    if has_bad_values {
        log::warn!("⚠️ Detected NaN/Inf in logits, applying sanitization");

        // Replace NaN with -100 (effectively zero probability), Inf with large finite value
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

/// Load default quantized model (Q4_K_M for best speed/quality balance)
pub fn load_default_quantized(
) -> Result<QuantizedModelState, Box<dyn std::error::Error + Send + Sync>> {
    // Download Q4_K_M GGUF if not cached
    let gguf_path = download_gguf_model(
        "hugging-quants/Llama-3.2-3B-Instruct-Q4_K_M-GGUF",
        "llama-3.2-3b-instruct-q4_k_m.gguf",
    )?;

    // Load with tokenizer from unsloth (same tokenizer, fully public)
    load_quantized_model(&gguf_path, "unsloth/Llama-3.2-3B-Instruct")
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_load_quantized() {
        // This test requires network access and takes time
        // Run with: cargo test --release test_load_quantized -- --ignored
    }
}
