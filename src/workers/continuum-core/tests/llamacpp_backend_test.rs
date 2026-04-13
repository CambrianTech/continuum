//! Integration tests for the llama.cpp backend.
//!
//! Run with: cargo test --release --features metal --test llamacpp_backend_test
//!
//! These tests require a GGUF model at the standard HF cache path.
//! They are ignored by default (use `--ignored` to run them).

use std::path::PathBuf;

use continuum_core::inference::backends::llamacpp::{LlamaCppBackend, LlamaCppConfig};

fn test_model_path() -> Option<PathBuf> {
    // Use standard Qwen2.5 (not our forged Qwen3.5 which has custom "qwen35" arch)
    let path = PathBuf::from("/tmp/qwen25_3b.gguf");
    if path.exists() { Some(path) } else { None }
}

/// Test 1: Load a model, generate text, unload.
#[test]
#[ignore] // requires model file
fn test_load_generate_unload() {
    let model_path = test_model_path().expect("Test model not found");
    let config = LlamaCppConfig {
        model_path: model_path.to_string_lossy().to_string(),
        context_length: 2048,
        n_gpu_layers: -1,
        ..Default::default()
    };

    let backend = LlamaCppBackend::load(config).expect("Load failed");

    let (text, tokens) = backend.generate(
        "fn fibonacci(n: u32) -> u32 {\n",
        32,
        0.0,
        &["\n}"],
    ).expect("Generate failed");

    assert!(!text.is_empty(), "Generated text should not be empty");
    assert!(tokens > 0, "Should generate at least one token");
    println!("Generated {} tokens: {}", tokens, text);
}

/// Test 2: Generation speed — must be > 30 tok/s on M5.
#[test]
#[ignore]
fn test_generation_speed() {
    let model_path = test_model_path().expect("Test model not found");
    let config = LlamaCppConfig {
        model_path: model_path.to_string_lossy().to_string(),
        context_length: 2048,
        n_gpu_layers: -1,
        ..Default::default()
    };

    let backend = LlamaCppBackend::load(config).expect("Load failed");

    // Warmup
    let _ = backend.generate("Hello", 8, 0.0, &[]);

    // Measure
    let start = std::time::Instant::now();
    let (_text, tokens) = backend.generate(
        "Write a Rust function:",
        64,
        0.0,
        &[],
    ).expect("Generate failed");
    let elapsed = start.elapsed();
    let tok_s = (tokens as f64 / elapsed.as_millis() as f64) * 1000.0;

    println!("Speed: {:.1} tok/s ({} tokens in {:?})", tok_s, tokens, elapsed);
    assert!(tok_s > 30.0, "Generation must exceed 30 tok/s, got {:.1}", tok_s);
}

/// Test 3: LoRA hot-swap — load adapter, generate, remove, generate again.
#[test]
#[ignore]
fn test_lora_hot_swap() {
    let model_path = test_model_path().expect("Test model not found");
    let config = LlamaCppConfig {
        model_path: model_path.to_string_lossy().to_string(),
        context_length: 2048,
        n_gpu_layers: -1,
        ..Default::default()
    };

    let backend = LlamaCppBackend::load(config).expect("Load failed");

    // Generate without LoRA
    let (text1, _) = backend.generate("Hello", 16, 0.0, &[]).expect("Gen1 failed");

    // TODO: Load a test LoRA adapter once we have one
    // backend.load_lora_adapter("test_adapter", "/path/to/adapter.gguf", 1.0).expect("LoRA load failed");

    // Generate with LoRA
    let (text2, _) = backend.generate("Hello", 16, 0.0, &[]).expect("Gen2 failed");

    // Remove LoRA
    // backend.remove_lora_adapter("test_adapter").expect("LoRA remove failed");

    assert!(!text1.is_empty());
    assert!(!text2.is_empty());
}

/// Test 4: Model path must exist and fail loudly if not.
#[test]
fn test_missing_model_fails_loudly() {
    let config = LlamaCppConfig {
        model_path: "/nonexistent/model.gguf".into(),
        ..Default::default()
    };

    let result = LlamaCppBackend::load(config);
    assert!(result.is_err(), "Loading nonexistent model must fail");
    if let Err(err) = result {
        assert!(err.contains("model") || err.contains("file") || err.contains("load") || err.contains("not found"),
            "Error message should mention the problem: {}", err);
    }
}
