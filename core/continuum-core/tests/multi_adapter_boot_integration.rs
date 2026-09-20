//! Multi-adapter boot integration test — mirrors the runtime's
//! `register_adapters` walk over the Rust catalog (catalog.rs)'s llamacpp-local rows
//! and proves the cumulative Metal residency + first-decode pressure
//! doesn't wedge the GPU.
//!
//! # Why this test exists
//!
//! 2026-04-22: Joel hit a hard Mac brick (mouse-frozen, hard reset).
//! Root cause: TWO `LlamaCppBackend` instances loaded into Metal at
//! boot (qwen2-vl-7b ~5GB GGUF + qwen2-audio-7b ~5GB GGUF), each
//! eagerly via `LlamaCppAdapter::initialize()`. Cumulative GPU
//! residency + the first decode's command-buffer allocation tipped
//! Metal over the cliff:
//!
//!   ```
//!   ggml_metal_synchronize: error: command buffer 0 failed with status 5
//!   error: Insufficient Memory
//!     (00000008:kIOGPUCommandBufferCallbackErrorOutOfMemory)
//!   ggml_metal_graph_compute: backend is in error state from a previous
//!     command buffer failure - recreate the backend to recover
//!   llama_decode: failed to decode, ret = -3
//!   ```
//!
//! Once a backend hits that error state it stays dead until process
//! restart — every persona using local inference returns `-3` for the
//! rest of the boot. Chat is unusable.
//!
//! The existing `vision_integration.rs` only registers ONE adapter
//! (qwen2-vl), so it never exercised the multi-row scenario. Result:
//! Joel's bug had no test that would have caught it. This file fixes
//! that — it walks every llamacpp-local row in the Rust catalog (catalog.rs) whose
//! files exist on disk and instantiates each adapter the way the
//! runtime does in `modules::ai_provider::register_adapters`.
//!
//! # The contract this test enforces
//!
//! After every llamacpp-local adapter has been registered AND
//! initialized, EVERY adapter must accept a tiny smoke decode without
//! returning `-3`. If two mtmd-capable rows can't coexist on the host
//! GPU, this test fails — same as production. Adding a new local
//! model row to the Rust catalog (catalog.rs) should run this test as the gate, not
//! "ship it and watch chat brick at runtime."
//!
//! # Run
//!
//! ```bash
//! # Default: skipped (need real models on disk + ~10s + a lot of GPU)
//! cargo test --release --features metal,accelerate \
//!   --test multi_adapter_boot_integration -- --ignored --nocapture
//! ```
//!
//! Skips cleanly with a printed reason when no llamacpp-local rows
//! have files on disk (CI hosts won't have these 5–10GB GGUFs).

use continuum_core::ai::adapter::AIProviderAdapter;
use continuum_core::ai::types::TextGenerationRequest;
use continuum_core::inference::{LlamaCppAdapter, LLAMACPP_PROVIDER_ID};
use continuum_core::model_registry;

/// Walk the Rust catalog (catalog.rs)'s llamacpp-local rows, register one adapter per
/// model that has its files on disk, then smoke-decode each. Asserts
/// no Metal OOM occurs across the cumulative load + first decode of
/// every backend. This is the test that would have failed the moment
/// `qwen2-audio-7b-instruct` was added to the Rust catalog (catalog.rs) next to
/// `qwen2-vl-7b-instruct` — same coexistence behavior the runtime
/// exhibits, just isolated and asserted.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "loads real GGUFs (~5–10GB Metal); run via --ignored --nocapture"]
async fn llamacpp_local_models_coexist_without_metal_oom() {
    model_registry::init_global().expect("the Rust catalog (catalog.rs) loads");
    let registry = model_registry::global();
    let local_rows: Vec<_> = registry
        .models_for_provider(LLAMACPP_PROVIDER_ID)
        .into_iter()
        .filter(|m| {
            m.gguf_local_path
                .as_ref()
                .map(|p| p.exists())
                .unwrap_or(false)
        })
        .collect();

    if local_rows.is_empty() {
        eprintln!(
            "[multi-adapter] skipping — no llamacpp-local rows with GGUFs on disk. \
             Pull at least two local models to exercise coexistence."
        );
        return;
    }

    eprintln!(
        "[multi-adapter] found {} llamacpp-local row(s) with GGUFs on disk:",
        local_rows.len()
    );
    for m in &local_rows {
        let mtmd = if m
            .mmproj_local_path
            .as_ref()
            .map(|p| p.exists())
            .unwrap_or(false)
        {
            "mtmd-capable"
        } else {
            "text-only"
        };
        eprintln!("[multi-adapter]   - {} ({mtmd})", m.id);
    }

    // Register every adapter — same shape `register_adapters` uses in
    // production. Sequential init (matches runtime's `initialize_all`)
    // is the contract: if two backends can't even sequentially load
    // and each smoke-decode without bricking, they cannot coexist
    // safely in production either.
    let mut adapters: Vec<Box<dyn AIProviderAdapter>> = Vec::with_capacity(local_rows.len());
    for model_meta in &local_rows {
        let gguf = model_meta.gguf_local_path.as_ref().unwrap().clone();
        let adapter =
            LlamaCppAdapter::with_model_id(gguf, model_meta.id.clone()).with_context_length(32768);
        let mut boxed: Box<dyn AIProviderAdapter> = Box::new(adapter);
        let init_start = std::time::Instant::now();
        boxed
            .initialize()
            .await
            .unwrap_or_else(|e| panic!("adapter init for '{}' failed: {e}", model_meta.id));
        eprintln!(
            "[multi-adapter] initialized '{}' in {:.2}s",
            model_meta.id,
            init_start.elapsed().as_secs_f64()
        );
        adapters.push(boxed);
    }

    // Smoke-decode each adapter. This is where Metal OOM surfaces —
    // base-model load alone may stay under the cliff; the first decode
    // dispatch (command buffer alloc + KV scratch) is what historically
    // wedged. The decode is intentionally tiny — 4 tokens — so the
    // test's purpose is "did Metal allocator survive the first ask",
    // not "is the model coherent".
    for (i, adapter) in adapters.iter().enumerate() {
        let model_id = local_rows[i].id.clone();
        let req = TextGenerationRequest {
            messages: vec![continuum_core::ai::types::ChatMessage {
                role: "user".to_string(),
                content: continuum_core::ai::types::MessageContent::Text("hi".to_string()),
                name: None,
            }],
            model: Some(model_id.clone()),
            provider: Some("local".to_string()),
            temperature: Some(0.0),
            max_tokens: Some(4),
            purpose: Some("multi-adapter-smoke".to_string()),
            ..Default::default()
        };
        let decode_start = std::time::Instant::now();
        let result = adapter.generate_text(req).await;
        match result {
            Ok(_) => eprintln!(
                "[multi-adapter] smoke-decode '{}' OK ({:.2}s)",
                model_id,
                decode_start.elapsed().as_secs_f64()
            ),
            Err(e) => {
                // The specific failure mode this test exists to catch:
                // any decode error mentioning -3 / command buffer / Metal
                // OOM means the cumulative backend load wedged the GPU.
                let lower = e.to_lowercase();
                let is_metal_brick = lower.contains("returned -3")
                    || lower.contains("command buffer")
                    || lower.contains("kiogpu")
                    || lower.contains("error state");
                panic!(
                    "smoke-decode for '{}' FAILED — {} — {}",
                    model_id,
                    if is_metal_brick {
                        "this is the Metal multi-backend brick. Adding this model \
                         to the Rust catalog (catalog.rs) + the others below it overflowed Metal at \
                         boot. Either disable one mtmd row OR ship the substrate \
                         work (mmproj init mutex + backend recovery on OOM) before \
                         re-enabling."
                    } else {
                        "non-Metal failure (still a regression — investigate)"
                    },
                    e
                );
            }
        }
    }

    eprintln!(
        "[multi-adapter] ✅ {} llamacpp-local backend(s) coexist safely on this GPU",
        local_rows.len()
    );
}
