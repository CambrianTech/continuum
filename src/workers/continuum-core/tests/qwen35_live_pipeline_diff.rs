//! Diagnostic: does the LIVE production pipeline (LlamaCppBackend.generate
//! → scheduler driver loop → ctx.decode → sampler) produce the SAME output
//! as the bare-metal direct ctx.decode test?
//!
//! Sister to qwen35_cpu_vs_gpu_diff.rs. That test proved Metal kernels are
//! mathematically correct (CPU output == GPU output) but it bypassed the
//! adapter + scheduler. This test exercises the actual production code path
//! and asserts it produces the expected answer "84" for "12 × 7".
//!
//! If this test fails: the bug is in the scheduler / sampler-construction /
//! batch-building code in our Rust layer, NOT in llama.cpp's Metal backend.
//!
//! Run:
//!   cargo test --release --test qwen35_live_pipeline_diff -- --ignored --nocapture

use continuum_core::inference::backends::llamacpp::{LlamaCppBackend, LlamaCppConfig};
use std::path::PathBuf;

const MODEL_PATH: &str = "/Users/joelteply/.docker/models/bundles/sha256/18055fe8ee379b95f4af3cf420588c5daa28f2a1ce1da335112a2d1ea188d3e6/model/model.gguf";
const PROMPT: &str = "Q: What is twelve times seven? A:";
const N_GENERATE: usize = 32;

#[test]
#[ignore = "requires local GGUF; run with --ignored --nocapture"]
fn qwen35_live_pipeline_produces_correct_answer() {
    let backend = LlamaCppBackend::load(LlamaCppConfig {
        model_path: PathBuf::from(MODEL_PATH),
        n_gpu_layers: -1,
        ..Default::default()
    }).expect("load");

    // temperature=0.0 → triggers Sampler::greedy() in start_request, fully
    // deterministic. Same path the chat persona uses for inference.
    let (text, n_tokens) = backend
        .generate(PROMPT, N_GENERATE, 0.0, &[], &[])
        .expect("generate");

    eprintln!("[live-pipeline] tokens={n_tokens} text={text:?}");

    // The direct ctx.decode test produced this exact string. If the live
    // pipeline produces something different — even off by one token — there
    // is a bug in our scheduler/sampler/batch-builder.
    let expected = " 84.\nQ: What is the sum of 12 and 7? A: 19.\nQ: What is the difference";
    assert!(
        text.starts_with(" 84."),
        "live pipeline did NOT produce the correct answer.\n  expected prefix: {expected:?}\n  got: {text:?}"
    );
}
