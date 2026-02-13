//! Quantized Model Loading and Generation
//!
//! Supports GGUF format quantized models (Q4_K_M, Q8_0, etc.) for faster inference.
//! Quantized models are ~3x smaller and 2-3x faster than BF16.
//!
//! LoRA Strategy for Quantized Models:
//! 1. Keep LoRA adapters in FP16/BF16 (small ~100MB each)
//! 2. Apply LoRA at runtime during forward pass (QLoRA style)
//! 3. Dequantize -> Apply LoRA -> Re-quantize per layer (mixed precision)

use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::time::Instant;

use candle_core::quantized::gguf_file;
use candle_core::{Device, Tensor};
use candle_transformers::generation::LogitsProcessor;
use candle_transformers::models::quantized_llama::ModelWeights;
use hf_hub::{api::sync::Api, Repo, RepoType};
use rand::Rng;
use tokenizers::Tokenizer;

use super::model::select_best_device;
use crate::runtime;

/// Quantized model state
pub struct QuantizedModelState {
    pub model: ModelWeights,
    pub tokenizer: Tokenizer,
    pub device: Device,
    pub eos_token_ids: Vec<u32>,
    pub model_id: String,
    pub quantization_type: String, // e.g., "Q4_K_M", "Q8_0"
    /// Path to GGUF file for model reloading
    model_path: PathBuf,
}

impl QuantizedModelState {
    /// Reload model from disk to clear KV cache
    ///
    /// CRITICAL: ModelWeights has internal per-layer kv_cache that accumulates across generations.
    /// Unlike the non-quantized Llama model which has an external Cache that can be recreated,
    /// quantized ModelWeights has no public API to clear its internal cache.
    ///
    /// The only way to get a fresh model with empty cache is to reload from disk.
    /// The GGUF file should be in OS page cache, making this ~2-3 seconds (acceptable for chat).
    pub fn reload_model(&mut self) -> Result<(), String> {
        let log = runtime::logger("candle");
        log.debug("Reloading quantized model to clear KV cache");
        let start = Instant::now();

        // Re-read GGUF file (should be in OS page cache)
        let mut file = File::open(&self.model_path)
            .map_err(|e| format!("Failed to open GGUF for reload: {e}"))?;
        let content = gguf_file::Content::read(&mut file)
            .map_err(|e| format!("Failed to read GGUF content: {e}"))?;

        // Create fresh model with empty KV cache
        let mut reader = BufReader::new(File::open(&self.model_path)
            .map_err(|e| format!("Failed to open GGUF for weights: {e}"))?);
        self.model = ModelWeights::from_gguf(content, &mut reader, &self.device)
            .map_err(|e| format!("Failed to reload model weights: {e}"))?;

        log.info(&format!("Model reloaded in {:.2}s", start.elapsed().as_secs_f32()));
        Ok(())
    }
}

/// Download GGUF model from HuggingFace
pub fn download_gguf_model(
    repo_id: &str,
    filename: &str,
) -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
    runtime::logger("candle").info(&format!("Downloading GGUF model: {}/{}", repo_id, filename));
    let start = Instant::now();

    let api = Api::new()?;
    let repo = api.repo(Repo::new(repo_id.to_string(), RepoType::Model));
    let path = repo.get(filename)?;

    runtime::logger("candle").info(&format!(
        "GGUF downloaded in {:.2}s: {:?}",
        start.elapsed().as_secs_f32(),
        path
    ));
    Ok(path)
}

/// Load a quantized GGUF model
pub fn load_quantized_model(
    model_path: &PathBuf,
    tokenizer_repo: &str,
) -> Result<QuantizedModelState, Box<dyn std::error::Error + Send + Sync>> {
    let log = runtime::logger("candle");
    log.info(&format!("Loading quantized model from {:?}", model_path));
    let start = Instant::now();

    let device = select_best_device();
    log.info(&format!("  Device: {:?}", device));

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

    log.info(&format!("  Quantization: {}", quant_type));

    // Load model weights
    let mut reader = BufReader::new(File::open(model_path)?);
    let model = ModelWeights::from_gguf(content, &mut reader, &device)?;

    log.info("  Model loaded");

    // Load tokenizer - try multiple sources in case some are gated
    log.info(&format!("  Loading tokenizer from {}", tokenizer_repo));
    let api = Api::new()?;

    // Try the specified repo first, then fallbacks for gated models
    let tokenizer_sources = vec![
        tokenizer_repo.to_string(),
        // Fallback: unsloth mirrors (not gated) - 3B variant for 3B models
        "unsloth/Llama-3.2-3B-Instruct".to_string(),
        // Fallback: unsloth 8B if loading 8B model
        "unsloth/Meta-Llama-3.1-8B-Instruct".to_string(),
    ];

    let mut tokenizer: Option<Tokenizer> = None;
    let mut last_error = String::new();

    for source in &tokenizer_sources {
        log.info(&format!("  Trying tokenizer from: {}", source));
        let repo = api.repo(Repo::new(source.clone(), RepoType::Model));
        match repo.get("tokenizer.json") {
            Ok(path) => {
                log.info(&format!("  Found tokenizer.json at {:?}", path));
                match Tokenizer::from_file(&path) {
                    Ok(t) => {
                        log.info(&format!("  Tokenizer loaded from {}", source));
                        tokenizer = Some(t);
                        break;
                    }
                    Err(e) => {
                        last_error = format!("Failed to parse tokenizer from {}: {}", source, e);
                        log.warn(&last_error);
                    }
                }
            }
            Err(e) => {
                last_error = format!("Failed to download tokenizer from {}: {}", source, e);
                log.warn(&last_error);
            }
        }
    }

    let tokenizer = tokenizer.ok_or_else(|| format!("Could not load tokenizer from any source. Last error: {}", last_error))?;

    // Llama 3.2 EOS tokens
    let eos_token_ids = vec![128009u32];

    let duration = start.elapsed();
    log.info(&format!(
        "Quantized model loaded in {:.2}s",
        duration.as_secs_f32()
    ));

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
        model_path: model_path.clone(),
    })
}

/// GPU sync frequency - sync every N tokens instead of every token
/// Higher = faster but more memory pressure, Lower = slower but safer
const GPU_SYNC_INTERVAL: usize = 16;

/// Only check for NaN on first N tokens (NaN usually appears early from bad prompts)
const NAN_CHECK_TOKENS: usize = 3;

/// Generate text from a prompt using quantized model
pub fn generate_text_quantized(
    state: &mut QuantizedModelState,
    prompt: &str,
    max_tokens: usize,
    temperature: f64,
) -> Result<(String, usize), String> {
    let log = runtime::logger("candle");
    let start = Instant::now();

    // CRITICAL: Reload model to clear KV cache from previous generations
    // ModelWeights has internal per-layer kv_cache that accumulates and corrupts output
    // if not cleared between generations. See reload_model() doc comment.
    state.reload_model()?;

    // DON'T add special tokens - build_prompt_from_messages already includes them
    // Using add_special_tokens=true would cause double BOS tokens and corrupt output
    let encoding = state
        .tokenizer
        .encode(prompt, false)
        .map_err(|e| format!("Tokenization failed: {e}"))?;
    let prompt_tokens: Vec<u32> = encoding.get_ids().to_vec();
    let prompt_len = prompt_tokens.len();

    if prompt_len == 0 {
        return Err("Empty prompt".to_string());
    }

    log.debug(&format!("Quantized generation: {} tokens from {} char prompt", prompt_len, prompt.len()));

    // INCIDENT CAPTURE: Log prompt hash and first/last chars for reproducibility
    // When NaN occurs, we can find this prompt in logs and recreate in tests
    let prompt_hash = {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        prompt.hash(&mut hasher);
        hasher.finish()
    };
    log.debug(&format!(
        "INCIDENT_CAPTURE: prompt_hash={:016x} tokens={} first_100={}",
        prompt_hash,
        prompt_len,
        &prompt.chars().take(100).collect::<String>().replace('\n', "\\n")
    ));

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

        // Batch GPU syncs - only sync every N tokens to prevent command buffer explosion
        if i == 0 || (i + 1) % GPU_SYNC_INTERVAL == 0 {
            state
                .device
                .synchronize()
                .map_err(|e| format!("GPU sync failed: {e}"))?;
        }

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

        // Only check NaN on first few tokens proactively - NaN from bad prompts appears immediately
        // For later tokens, we catch sampling errors and sanitize on-demand
        let logits = if i < NAN_CHECK_TOKENS {
            let (sanitized, had_nan) = sanitize_logits_with_flag(&logits, &state.device)?;
            if had_nan {
                nan_count += 1;
                if i == 0 {
                    log.error("NaN/Inf on first token - prompt may be malformed");
                    return Err("Model produced NaN on first token - prompt may be malformed or too long".to_string());
                }
                if nan_count > 2 {
                    log.error(&format!("Multiple NaN tokens in first {} - aborting", NAN_CHECK_TOKENS));
                    break;
                }
            }
            sanitized
        } else {
            logits
        };

        // Try to sample - if it fails (likely NaN), sanitize and retry
        let next_token = match logits_processor.sample(&logits) {
            Ok(token) => {
                nan_count = 0; // Reset on success
                token
            }
            Err(e) => {
                nan_count += 1;
                // If we get more than 5 consecutive NaN errors, abort early with what we have
                // This prevents generating pages of garbage
                if nan_count > 5 {
                    log.warn(&format!("Aborting generation after {} consecutive NaN errors at token {}", nan_count, i));
                    break;
                }
                // Sampling failed - likely NaN/Inf in logits. Sanitize and retry.
                log.warn(&format!("Sampling failed at token {}, sanitizing and retrying: {}", i, e));
                let (sanitized, _) = sanitize_logits_with_flag(&logits, &state.device)?;
                logits_processor
                    .sample(&sanitized)
                    .map_err(|e| format!("Sampling failed even after sanitization at token {}: {}", i, e))?
            }
        };

        if state.eos_token_ids.contains(&next_token) {
            break;
        }

        all_tokens.push(next_token);
    }

    // Final GPU sync
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
    log.info(&format!(
        "Quantized generated {} tokens in {:?}",
        generated_tokens.len(),
        duration
    ));

    Ok((output_text, generated_tokens.len()))
}

/// Sanitize logits to prevent NaN/Inf from crashing the sampler
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

/// Load default quantized model (Q8_0 for numerical stability with long contexts)
/// Uses Llama 3.2 3B - sweet spot for M1 Mac (fast, fits in memory, good quality)
///
/// Q8_0 vs Q4_K_M tradeoffs:
/// - Q8_0: 3.42 GB, more numerically stable, better for long contexts
/// - Q4_K_M: ~2 GB, smaller but can produce NaN with long prompts (>1000 tokens)
///
/// For LoRA training: Use non-quantized BF16 model (load_model_by_id)
/// For inference: Use quantized for speed (this function)
/// QLoRA approach: Keep model quantized, keep LoRA adapters in FP16/BF16
pub fn load_default_quantized(
) -> Result<QuantizedModelState, Box<dyn std::error::Error + Send + Sync>> {
    // Download Q8_0 GGUF if not cached (3B model - Q8 for stability over Q4)
    let gguf_path = download_gguf_model(
        "hugging-quants/Llama-3.2-3B-Instruct-Q8_0-GGUF",
        "llama-3.2-3b-instruct-q8_0.gguf",
    )?;

    // Load with tokenizer from unsloth (public, same tokenizer)
    load_quantized_model(&gguf_path, "unsloth/Llama-3.2-3B-Instruct")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test that multiple generations produce coherent output (not garbage)
    ///
    /// This test validates the KV cache reload fix. Before the fix, the second
    /// generation would produce garbage because the KV cache was polluted by
    /// the first generation.
    ///
    /// Run with: cargo test --release test_multiple_generations -- --ignored --nocapture
    #[test]
    #[ignore] // Requires model download, takes ~30 seconds
    fn test_multiple_generations() {
        // Load model
        let mut state = load_default_quantized()
            .expect("Failed to load quantized model");

        println!("Model loaded: {}", state.model_id);

        // First generation - short prompt
        let prompt1 = "<|begin_of_text|><|start_header_id|>user<|end_header_id|>\n\nWhat is 2+2?<|eot_id|><|start_header_id|>assistant<|end_header_id|>\n\n";
        let (output1, tokens1) = generate_text_quantized(&mut state, prompt1, 50, 0.3)
            .expect("First generation failed");

        println!("Generation 1: {} tokens", tokens1);
        println!("Output 1: {}", output1);

        // Verify output is coherent (not garbage)
        assert!(!output1.contains('\u{FFFD}'), "Output 1 contains replacement character (garbage)");
        assert!(output1.len() > 0, "Output 1 is empty");

        // Second generation - different prompt
        let prompt2 = "<|begin_of_text|><|start_header_id|>user<|end_header_id|>\n\nWhat color is the sky?<|eot_id|><|start_header_id|>assistant<|end_header_id|>\n\n";
        let (output2, tokens2) = generate_text_quantized(&mut state, prompt2, 50, 0.3)
            .expect("Second generation failed");

        println!("Generation 2: {} tokens", tokens2);
        println!("Output 2: {}", output2);

        // Verify output is coherent (not garbage)
        assert!(!output2.contains('\u{FFFD}'), "Output 2 contains replacement character (garbage)");
        assert!(output2.len() > 0, "Output 2 is empty");

        // Both outputs should be different (answering different questions)
        assert_ne!(output1, output2, "Outputs should be different for different questions");

        println!("✓ Both generations produced coherent output");
    }

    /// Test that a single generation works with simple prompt
    ///
    /// Run with: cargo test --release test_single_generation -- --ignored --nocapture
    #[test]
    #[ignore] // Requires model download
    fn test_single_generation() {
        let mut state = load_default_quantized()
            .expect("Failed to load quantized model");

        // Simple chat prompt with proper Llama 3 format
        let prompt = "<|begin_of_text|><|start_header_id|>user<|end_header_id|>\n\nSay hello.<|eot_id|><|start_header_id|>assistant<|end_header_id|>\n\n";

        let (output, tokens) = generate_text_quantized(&mut state, prompt, 30, 0.3)
            .expect("Generation failed");

        println!("Generated {} tokens: {}", tokens, output);

        // Basic sanity checks
        assert!(!output.contains('\u{FFFD}'), "Output contains garbage replacement characters");
        assert!(tokens > 0, "Should generate at least one token");

        // Output should contain greeting-like content
        let output_lower = output.to_lowercase();
        let has_greeting = output_lower.contains("hello")
            || output_lower.contains("hi")
            || output_lower.contains("hey")
            || output_lower.contains("greet");
        assert!(has_greeting, "Output should contain a greeting: {}", output);
    }

    /// Test to find the NaN threshold for quantized model
    ///
    /// This test sends progressively longer prompts to identify the exact
    /// token count where NaN starts occurring. Used to set safe limits.
    ///
    /// Known from production logs:
    /// - 149 tokens: works fine
    /// - 1451 tokens: NaN detected
    /// - 1622 tokens: NaN abort
    ///
    /// Run with: cargo test --release test_find_nan_threshold -- --ignored --nocapture
    #[test]
    #[ignore] // Requires model download, takes several minutes
    fn test_find_nan_threshold() {
        let mut state = load_default_quantized()
            .expect("Failed to load quantized model");

        println!("Finding NaN threshold for model: {}", state.model_id);
        println!("============================================");

        // Test at different token counts
        // We'll generate prompts of various sizes and see where NaN appears
        let test_sizes: Vec<usize> = vec![
            100,   // Should work
            200,   // Should work
            400,   // Should work
            600,   // Likely works
            800,   // May start having issues
            1000,  // Threshold area based on docs
            1200,  // Above documented threshold
            1400,  // Near observed failure point
        ];

        // Create a repeatable filler that tokenizes consistently
        // "The quick brown fox jumps. " is ~7 tokens
        let filler = "The quick brown fox jumps over the lazy dog. ";

        for target_tokens in test_sizes {
            // Build prompt with approximately target_tokens
            // Header is ~20 tokens, so subtract that
            let content_tokens = target_tokens.saturating_sub(20);
            let repetitions = content_tokens / 10; // ~10 tokens per filler repetition

            let mut content = String::new();
            for _ in 0..repetitions {
                content.push_str(filler);
            }

            let prompt = format!(
                "<|begin_of_text|><|start_header_id|>user<|end_header_id|>\n\n{}<|eot_id|><|start_header_id|>assistant<|end_header_id|>\n\n",
                content
            );

            // Count actual tokens
            let tokens_encoded = state.tokenizer.encode(prompt.as_str(), true)
                .expect("Tokenization failed");
            let actual_tokens = tokens_encoded.len();

            print!("Testing {} tokens (target {})... ", actual_tokens, target_tokens);

            // Reload model to clear KV cache before each test
            state.reload_model().expect("Reload failed");

            match generate_text_quantized(&mut state, &prompt, 20, 0.3) {
                Ok((output, gen_tokens)) => {
                    // Check for garbage output (indicates NaN recovery produced junk)
                    let has_garbage = output.chars().any(|c| {
                        c == '\u{FFFD}' || // replacement char
                        (c as u32 > 0x1F000) || // emoji/symbol range often = garbage
                        output.contains("zeroes") || // Known garbage pattern
                        output.contains("valueOf") // Known garbage pattern
                    });

                    if has_garbage {
                        println!("⚠️  {} tokens generated but GARBAGE detected: {}",
                            gen_tokens, &output.chars().take(50).collect::<String>());
                    } else {
                        println!("✓ {} tokens, output: {}",
                            gen_tokens, &output.chars().take(30).collect::<String>());
                    }
                }
                Err(e) => {
                    println!("✗ FAILED: {}", e);
                    println!("\n==> NaN threshold appears to be around {} input tokens", actual_tokens);
                    break;
                }
            }
        }

        println!("\n============================================");
        println!("Test complete. Use results to set safe input token limits.");
    }

    /// Test that prompts at the safe threshold work reliably
    ///
    /// Run with: cargo test --release test_safe_threshold -- --ignored --nocapture
    #[test]
    #[ignore]
    fn test_safe_threshold() {
        let mut state = load_default_quantized()
            .expect("Failed to load quantized model");

        // Test at safe threshold (800 tokens based on analysis)
        const SAFE_INPUT_TOKENS: usize = 800;

        let filler = "The quick brown fox jumps over the lazy dog. ";
        let repetitions = (SAFE_INPUT_TOKENS - 20) / 10;

        let mut content = String::new();
        for _ in 0..repetitions {
            content.push_str(filler);
        }

        let prompt = format!(
            "<|begin_of_text|><|start_header_id|>user<|end_header_id|>\n\n{}<|eot_id|><|start_header_id|>assistant<|end_header_id|>\n\n",
            content
        );

        let tokens = state.tokenizer.encode(prompt.as_str(), true)
            .expect("Tokenization failed").len();

        println!("Testing safe threshold with {} tokens", tokens);

        // Run 5 times to ensure reliability
        for i in 0..5 {
            state.reload_model().expect("Reload failed");
            let (output, gen_tokens) = generate_text_quantized(&mut state, &prompt, 20, 0.3)
                .expect(&format!("Generation {} failed", i + 1));

            assert!(!output.contains('\u{FFFD}'), "Output {} contains garbage", i + 1);
            assert!(!output.contains("zeroes"), "Output {} contains garbage pattern", i + 1);
            println!("Run {}: {} tokens, OK", i + 1, gen_tokens);
        }

        println!("✓ Safe threshold of {} tokens verified reliable", tokens);
    }
}
