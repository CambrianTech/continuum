//! End-to-end native vision integration test against real Qwen2-VL-7B.
//!
//! Why this test exists: the README's thesis row reads "Text in, text
//! out → Full embodiment — see, hear, speak, attend meetings, build
//! together, play together." In January 2026 the system had AIs natively
//! seeing users in video chat (describing their shirts). The 2026-04-20
//! Rust-cognition cutover removed the live TS multimodal path; the Rust
//! receiver was text-only AND `llamacpp_adapter` filter_map'd Parts down
//! to Text only. Restoring native local vision is priority-1 per Joel
//! 2026-04-21.
//!
//! Validation chain (this test is the bottom rung):
//!
//!   1. brew's `llama-mtmd-cli` against the same vendored llama.cpp
//!      sources confirmed Qwen2-VL-7B Q4_K_M + mmproj-f16 produces
//!      correct image descriptions on M5 Metal in ~1s.
//!   2. We added libmtmd build flags + bindgen + safe `MtmdContext`
//!      wrapper to the llama crate (commit d32b8840a).
//!   3. `LlamaCppBackend::generate_with_image` orchestrates load +
//!      eval_image + sampler loop, bypassing the scheduler for now.
//!   4. THIS test proves the full Rust path produces the same correct
//!      output the brew binary did. If THIS passes, the Rust pipeline
//!      is restored to behavioral parity for the single-shot multimodal
//!      case.
//!
//! Marked `#[ignore]` because it requires the qwen2-vl-7b GGUF + mmproj
//! on disk (~6 GB) and pays a ~5–10s load cost. Run with:
//!
//!     cargo test --package continuum-core --test llamacpp_vision_integration \
//!       --release -- --ignored --nocapture

use continuum_core::inference::backends::llamacpp::{LlamaCppBackend, LlamaCppConfig};
use continuum_core::inference::backends::SamplingConfig;
use std::env;
use std::path::PathBuf;
use std::time::Instant;

fn qwen2_vl_paths() -> (PathBuf, PathBuf) {
    let model = env::var("QWEN2_VL_7B_GGUF")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env::var("HOME").unwrap_or_else(|_| "/Users/joelteply".to_string()))
                .join("models/qwen2-vl-7b/Qwen2-VL-7B-Instruct-Q4_K_M.gguf")
        });
    let mmproj = env::var("QWEN2_VL_7B_MMPROJ")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env::var("HOME").unwrap_or_else(|_| "/Users/joelteply".to_string()))
                .join("models/qwen2-vl-7b/mmproj-Qwen2-VL-7B-Instruct-f16.gguf")
        });
    (model, mmproj)
}

/// Real test image, loaded from `/tmp/cat.jpg` if present (smoke-test
/// path used during development; `curl -sL <unsplash-url>` to populate),
/// or from a `TEST_VISION_IMAGE` env var override. We REQUIRE a real
/// JPEG/PNG file because hand-rolled tiny test images don't carry
/// enough signal for the model to describe them — the smoke run that
/// confirmed Qwen2-VL works (`brew llama-mtmd-cli`) used a real photo.
///
/// Returns `None` if no image is available; the test then skips with a
/// clear message instead of failing on garbage input.
fn load_test_image() -> Option<Vec<u8>> {
    let path = env::var("TEST_VISION_IMAGE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp/cat.jpg"));
    if !path.exists() {
        return None;
    }
    std::fs::read(&path).ok()
}

/// What this catches: native vision through the LlamaCppBackend's
/// `generate_with_image` failing to produce a coherent description of
/// the input image. If this passes, the chain (mmproj load → bitmap
/// init → tokenize+image-splice → mtmd_helper_eval_chunks → sampler
/// loop) works end-to-end against a real model. If it fails, the
/// printed output (under --nocapture) shows the model's actual
/// response — we look for color or shape vocabulary rather than
/// pinning an exact string because vision-LLM phrasing varies.
///
/// Validated 2026-04-21: brew's llama-mtmd-cli on the same model files
/// + a real cat photo printed "The animal in the image is a cat." in
/// ~1s on M5 Metal. The Rust path uses the SAME vendored llama.cpp +
/// SAME mtmd C API + SAME model files, so the assertion threshold is
/// "Rust produces equivalently-shaped output." A failure here means
/// the Rust wrapper diverged from the C reference path.
#[test]
#[ignore = "requires real Qwen2-VL-7B GGUF + mmproj + 5-10s; run manually with --ignored --nocapture"]
fn qwen2_vl_describes_image_via_rust_pipeline() {
    let (model_path, mmproj_path) = qwen2_vl_paths();
    if !model_path.exists() {
        eprintln!(
            "[vision-int] skipping — Qwen2-VL-7B GGUF not at {}. \
             Set QWEN2_VL_7B_GGUF or download via \
             `hf download bartowski/Qwen2-VL-7B-Instruct-GGUF Qwen2-VL-7B-Instruct-Q4_K_M.gguf --local-dir ~/models/qwen2-vl-7b`",
            model_path.display()
        );
        return;
    }
    if !mmproj_path.exists() {
        eprintln!(
            "[vision-int] skipping — mmproj not at {}. \
             Vision-capable model needs the projector file alongside the main GGUF.",
            mmproj_path.display()
        );
        return;
    }

    let load_start = Instant::now();
    let config = LlamaCppConfig {
        model_path: model_path.clone(),
        mmproj_path: Some(mmproj_path.clone()),
        context_length: None, // = derive from GGUF (32768 for qwen2-vl-7b)
        n_batch: 2048,
        n_gpu_layers: -1,
        n_seq_max: 1,
        ..Default::default()
    };
    let backend =
        LlamaCppBackend::load(config).expect("backend loads with vision-capable Qwen2-VL");
    eprintln!(
        "[vision-int] backend loaded in {}ms",
        load_start.elapsed().as_millis()
    );

    let Some(image) = load_test_image() else {
        eprintln!(
            "[vision-int] skipping — no test image at /tmp/cat.jpg. \
             Fetch one: `curl -sL -A 'Mozilla/5.0' \
             'https://images.unsplash.com/photo-1574158622682-e40e69881006?w=400&q=80' \
             -o /tmp/cat.jpg`, then re-run this test. \
             Or set TEST_VISION_IMAGE=/path/to/your.jpg"
        );
        return;
    };
    eprintln!("[vision-int] image is {} bytes", image.len());

    // Apply the model's embedded chat template via llama::render_chat
    // — same machinery brew's llama-mtmd-cli uses internally
    // (common_chat_apply_template). Hand-rolling the prompt with
    // <|im_start|>... wrappers misses qwen2-vl's template logic around
    // <|vision_start|> placement, which made the model output bbox
    // coordinates instead of natural language during initial testing.
    //
    // The marker (`<__media__>`) goes inside the user content; the
    // template handles the surrounding turn structure. Prompt phrasing
    // matters: open-ended "describe" gets natural language; "what
    // animal" triggers detection-style bbox output (verified empirically
    // 2026-04-21 against this same model).
    let user_content = format!(
        "{}Describe this image in one sentence.",
        llama::MtmdContext::default_marker()
    );
    let messages = vec![llama::ChatMsg {
        role: "user".to_string(),
        content: user_content,
    }];
    let template = backend.model_chat_template();
    let prompt = llama::render_chat(template.as_deref(), &messages, true)
        .expect("render_chat with model's embedded template");
    eprintln!("[vision-int] rendered prompt: {prompt:?}");

    let gen_start = Instant::now();
    // Match brew's llama-mtmd-cli defaults: low temp, no top_p truncation.
    // Higher temp + top_k/top_p (chat() defaults) caused the model to
    // wander into bbox-detection mode with the same prompt; greedy /
    // low-temp keeps it on the description path.
    let mut sampling = SamplingConfig::chat();
    sampling.temperature = 0.0; // greedy
    sampling.top_k = 0;
    sampling.top_p = 1.0;
    sampling.repeat_penalty = 1.0;
    let (text, tokens) = backend
        .generate_with_image(
            &prompt,
            &image,
            120, // max_tokens — keep test cheap
            sampling,
            &["<|im_end|>", "<|endoftext|>"],
        )
        .expect("generate_with_image should produce a description");
    eprintln!(
        "[vision-int] generated {} tokens in {}ms ({:.1} tok/s)",
        tokens,
        gen_start.elapsed().as_millis(),
        tokens as f64 / gen_start.elapsed().as_secs_f64().max(0.001)
    );
    eprintln!("[vision-int] response: {text:?}");

    assert!(tokens > 0, "model produced zero tokens — generation failed");
    let lower = text.to_lowercase();
    // The default test image (/tmp/cat.jpg fetched via the curl line in
    // the skip message) is a cat. Vision-LLM phrasing varies — accept
    // any of these animal-identifier words. Brew's llama-mtmd-cli on
    // this same model + image returned "The animal in the image is a cat."
    let mentions_animal = ["cat", "kitten", "feline"]
        .iter()
        .any(|c| lower.contains(c));
    assert!(
        mentions_animal,
        "response should identify the animal (image is a cat); got: {text:?}"
    );
}
