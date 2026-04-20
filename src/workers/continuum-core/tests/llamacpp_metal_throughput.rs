//! Smoke test for the bundled llama.cpp's Metal acceleration on M-series Macs.
//!
//! Bypasses Docker Model Runner (DMR) entirely and loads qwen3.5-4b directly
//! through the in-process LlamaCppBackend wrapper. Measures throughput and
//! prints whether the Metal tensor API path is active.
//!
//! Why this test exists: 2026-04-19 found that DMR's container Metal toolchain
//! fails to compile the f16 tensor API source on M5 Pro (MTLGPUFamilyMetal4),
//! causing `has tensor = false` and a degraded fallback that runs SLOWER than
//! pre-M5 hardware (M5 at 22 tok/s vs M1 at 27 tok/s for the same qwen2.5-7B).
//!
//! Hypothesis: our bundled llama.cpp built on the host Metal toolchain DOES
//! compile the tensor API source correctly. If this test produces ≥50 tok/s on
//! M5 for qwen3.5-4b (Q4_K_M), the bypass-DMR path is the answer for Mac local
//! inference AND we have concrete repro evidence for an upstream llama.cpp
//! issue ("DMR build is degraded vs host build on identical hardware").
//!
//! Run manually:
//!   cargo test --package continuum-core --test llamacpp_metal_throughput \
//!     --release -- --ignored --nocapture
//!
//! Marked #[ignore] because it requires the qwen3.5-4b GGUF file at the DMR
//! path, takes 10-30s, and isn't part of the regular CI test loop.

use continuum_core::inference::backends::llamacpp::{LlamaCppBackend, LlamaCppConfig};
use std::path::PathBuf;
use std::time::Instant;

/// SHA256-keyed path to the qwen3.5-4b-code-forged GGUF, as DMR pulls it.
/// Hardcoded because this test reproduces a specific DMR-vs-host comparison;
/// the path corresponds to `huggingface.co/continuum-ai/qwen3.5-4b-code-forged-gguf`.
const QWEN35_4B_GGUF_PATH: &str = "/Users/joelteply/.docker/models/bundles/sha256/18055fe8ee379b95f4af3cf420588c5daa28f2a1ce1da335112a2d1ea188d3e6/model/model.gguf";

#[test]
#[ignore = "requires local GGUF + 10-30s; run manually with --ignored --nocapture"]
fn qwen35_4b_metal_throughput_via_bundled_llamacpp() {
    let model_path = PathBuf::from(QWEN35_4B_GGUF_PATH);
    if !model_path.exists() {
        panic!(
            "qwen3.5-4b GGUF not found at {:?} — pull via `docker model pull \
             huggingface.co/continuum-ai/qwen3.5-4b-code-forged-gguf` first",
            model_path
        );
    }

    let load_start = Instant::now();
    let config = LlamaCppConfig {
        model_path,
        n_gpu_layers: -1, // Offload all layers to GPU (Metal on Mac)
        ..Default::default()
    };
    let backend = LlamaCppBackend::load(config).expect("failed to load llama.cpp backend");
    let load_ms = load_start.elapsed().as_millis();
    eprintln!(
        "[smoke] backend loaded in {}ms (model_id={})",
        load_ms,
        backend.model_id()
    );

    // Warm-up call so the first-call compile/cache cost doesn't pollute measurement.
    eprintln!("[smoke] warm-up generation (10 tokens)...");
    let warm_start = Instant::now();
    let warm_result = backend
        .generate("Reply OK.", 10, 0.7, &[], &[])
        .expect("warm-up generate failed");
    eprintln!(
        "[smoke] warm-up: {} tokens in {}ms ({:.1} tok/s) — text={:?}",
        warm_result.1,
        warm_start.elapsed().as_millis(),
        warm_result.1 as f64 / warm_start.elapsed().as_secs_f64(),
        warm_result.0
    );

    // Real measurement: 100 tokens, longer output, isolated decode rate.
    eprintln!("[smoke] measurement generation (100 tokens)...");
    let gen_start = Instant::now();
    let (text, tokens) = backend
        .generate("Count from 1 to 50, separated by commas.", 100, 0.7, &[], &[])
        .expect("measurement generate failed");
    let elapsed_secs = gen_start.elapsed().as_secs_f64();
    let tokens_per_sec = tokens as f64 / elapsed_secs;

    eprintln!("");
    eprintln!("=== llamacpp metal throughput on this host ===");
    eprintln!("  tokens generated: {tokens}");
    eprintln!("  elapsed: {:.2}s", elapsed_secs);
    eprintln!("  THROUGHPUT: {:.1} tok/s", tokens_per_sec);
    eprintln!("");
    eprintln!("  reference (DMR's degraded path on M5 Pro): ~22 tok/s");
    eprintln!("  expected for fully-accelerated Metal on M5 Pro: ≥50 tok/s");
    eprintln!("  text head: {:?}", &text[..text.len().min(120)]);
    eprintln!("===============================================");
    eprintln!("");

    // Don't assert a hard floor — this test is observational. The output above
    // is the diagnostic. Manual review interprets whether the bypass-DMR
    // approach is justified by the throughput delta vs DMR's measured 22 tok/s.
    assert!(
        tokens > 0,
        "no tokens generated — backend may have failed to load model on Metal"
    );
}
