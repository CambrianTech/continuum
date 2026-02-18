//! Quantized Model Loading
//!
//! Handles downloading and loading GGUF quantized models.
//! Returns `Box<dyn ModelBackend>` — the unified interface.
//!
//! The backend reads architecture, context_length, and EOS tokens
//! from GGUF metadata. No hardcoded values.

use std::path::PathBuf;
use std::time::Instant;

use hf_hub::{api::sync::Api, Repo, RepoType};
use tokenizers::Tokenizer;

use super::backends::{self, ModelBackend};
use super::model::select_best_device;
use crate::runtime;

/// Download GGUF model from HuggingFace.
pub fn download_gguf_model(
    repo_id: &str,
    filename: &str,
) -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
    let log = runtime::logger("candle");
    log.info(&format!("Downloading GGUF model: {}/{}", repo_id, filename));
    let start = Instant::now();

    let api = Api::new()?;
    let repo = api.repo(Repo::new(repo_id.to_string(), RepoType::Model));
    let path = repo.get(filename)?;

    log.info(&format!(
        "GGUF downloaded in {:.2}s: {:?}",
        start.elapsed().as_secs_f32(),
        path
    ));
    Ok(path)
}

/// Load a quantized GGUF model as a ModelBackend.
///
/// Architecture and context length are read from GGUF metadata.
/// The correct backend (Llama, Qwen2, etc.) is instantiated automatically.
pub fn load_quantized_model(
    model_path: &PathBuf,
    tokenizer_repo: &str,
    model_id: &str,
) -> Result<Box<dyn ModelBackend>, Box<dyn std::error::Error + Send + Sync>> {
    let log = runtime::logger("candle");
    log.info(&format!("Loading quantized model from {:?}", model_path));
    let start = Instant::now();

    let device = select_best_device();
    log.info(&format!("  Device: {:?}", device));

    // Load tokenizer
    log.info(&format!("  Loading tokenizer from {}", tokenizer_repo));
    let api = Api::new()?;

    let tokenizer_sources = vec![
        tokenizer_repo.to_string(),
        "unsloth/Llama-3.2-3B-Instruct".to_string(),
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

    let tokenizer = tokenizer.ok_or_else(|| {
        format!("Could not load tokenizer from any source. Last error: {}", last_error)
    })?;

    // Load backend (reads architecture + context_length from GGUF metadata)
    let backend = backends::load_gguf_backend(model_path, tokenizer, model_id, &device)
        .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.into() })?;

    let duration = start.elapsed();
    log.info(&format!(
        "Quantized model loaded in {:.2}s (arch={}, ctx={}, format={:?})",
        duration.as_secs_f32(),
        backend.architecture(),
        backend.context_length(),
        backend.format()
    ));

    Ok(backend)
}

/// Load default quantized model (Q8_0 Llama 3.2 3B).
pub fn load_default_quantized() -> Result<Box<dyn ModelBackend>, Box<dyn std::error::Error + Send + Sync>> {
    let gguf_path = download_gguf_model(
        "hugging-quants/Llama-3.2-3B-Instruct-Q8_0-GGUF",
        "llama-3.2-3b-instruct-q8_0.gguf",
    )?;

    load_quantized_model(
        &gguf_path,
        "unsloth/Llama-3.2-3B-Instruct",
        "unsloth/Llama-3.2-3B-Instruct",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::backends;

    #[test]
    #[ignore] // Requires model download
    fn test_context_length_from_model() {
        let backend = load_default_quantized().expect("Failed to load quantized model");

        let ctx = backend.context_length();
        println!("Model reports context_length = {}", ctx);
        assert!(ctx >= 8192, "Should be at least 8192, got {}", ctx);
        assert_ne!(ctx, 4096, "Should NOT be hardcoded 4096");
    }

    #[test]
    #[ignore] // Requires model download
    fn test_generate_simple() {
        let mut backend = load_default_quantized().expect("Failed to load");

        let prompt = "<|begin_of_text|><|start_header_id|>user<|end_header_id|>\n\nSay hello.<|eot_id|><|start_header_id|>assistant<|end_header_id|>\n\n";
        let (output, tokens) = backends::generate(&mut *backend, prompt, 30, 0.3)
            .expect("Generation failed");

        println!("Generated {} tokens: {}", tokens, output);
        assert!(!output.contains('\u{FFFD}'), "Output contains garbage");
        assert!(tokens > 0, "Should generate at least one token");
    }

    #[test]
    #[ignore] // Requires model download
    fn test_prompt_exceeding_context_rejected() {
        let mut backend = load_default_quantized().expect("Failed to load");

        let ctx = backend.context_length();
        let filler = "word ".repeat(ctx * 2);
        let prompt = format!(
            "<|begin_of_text|><|start_header_id|>user<|end_header_id|>\n\n{}<|eot_id|><|start_header_id|>assistant<|end_header_id|>\n\n",
            filler
        );

        let result = backends::generate(&mut *backend, &prompt, 10, 0.3);
        assert!(result.is_err(), "Should reject oversized prompt");
    }
}
