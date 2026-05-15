//! Regression test for the no-CPU-fallback alpha contract (#1262 → #1275 → #1280).
//!
//! Continuum's documented contract per `project_continuum_alpha_product_bar_sensory_personas.md`
//! and `docs/architecture/SENSORY-PERSONA-ALPHA-CONTRACT.md` is **NO silent CPU fallback**:
//! standard personas use `SiliconResidencyRequirement::GpuOrUnifiedMemoryOnly` and the model
//! resolver is supposed to refuse rather than fall through to CPU.
//!
//! Pre-#1280 this contract was enforced (in part) by an explicit `panic!` inside
//! `inference::model::select_best_device`. That function lived in the dead Candle
//! chain (CandleAdapter → ContinuumModel → select_best_device), unreachable from
//! `AIProviderModule::register_adapters`. #1280 deleted the chain and moved the
//! contract assertion to its actually-load-bearing site:
//!
//!   `LlamaCppConfig::default()` sets `n_gpu_layers: -1` (= "all layers on GPU").
//!   When no GPU is available, llama.cpp's own model loader hard-fails — this is
//!   the runtime mechanism that prevents CPU fallback on the production hot path.
//!
//! This test asserts the `n_gpu_layers: -1` invariant by source inspection plus the
//! ort_providers + LlamaCppAdapter assertions that survived #1280 unchanged.
//!
//! Pattern: forbidden-strings ratchet (same shape as lane F PR-2 #1129 — TS persona
//! forbidden-strings ratchet) applied to the Rust inference layer.
//!
//! Audit context:
//!   https://github.com/CambrianTech/continuum/issues/1262#issuecomment-4461757997
//!   https://github.com/CambrianTech/continuum/issues/1280#issuecomment-4462181316

const LLAMACPP_BACKEND_SOURCE: &str =
    include_str!("../src/inference/backends/llamacpp.rs");

const ORT_PROVIDERS_SOURCE: &str =
    include_str!("../src/inference/ort_providers.rs");

const LLAMACPP_ADAPTER_SOURCE: &str =
    include_str!("../src/inference/llamacpp_adapter.rs");

#[test]
fn llamacpp_default_config_requires_full_gpu_offload() {
    // The production load path is `LlamaCppConfig::default()` →
    // `LlamaCppBackend::load(config)` → llama.cpp `Model::load_from_file`.
    // `n_gpu_layers: -1` means "put ALL layers on the GPU" — when no GPU
    // is available, llama.cpp's loader returns an error rather than
    // silently running on CPU.
    //
    // If a future PR changes the default to a positive integer (partial
    // offload) or to 0 (CPU-only), the no-CPU-fallback alpha contract is
    // broken on the production hot path. This assertion stops that from
    // shipping.

    assert!(
        LLAMACPP_BACKEND_SOURCE.contains("n_gpu_layers: -1"),
        "LlamaCppConfig::default() must set n_gpu_layers: -1 (all layers on GPU) so llama.cpp \
         loud-fails on no-GPU hosts rather than silently running on CPU. If you changed it, \
         update both this test and docs/architecture/SENSORY-PERSONA-ALPHA-CONTRACT.md. \
         A partial-offload or CPU-only default was the bug #1262 was filed for."
    );
}

#[test]
fn ort_providers_documents_no_cpu_fallback_contract() {
    // ort_providers.rs carries the same contract for the ORT consumer
    // (embedding / TTS / STT / vision via ONNX Runtime). The doc string
    // must remain present so the architectural rule is discoverable from
    // source alone.

    assert!(
        ORT_PROVIDERS_SOURCE.contains("CPU fallback is forbidden"),
        "ort_providers.rs must document 'CPU fallback is forbidden' for the ORT consumer. \
         If you removed the comment, the no-CPU-fallback rule is no longer self-documenting \
         from source — surface the rule in another way before removing the comment."
    );
}

#[test]
fn llamacpp_adapter_uses_loud_fail_for_no_local_model() {
    // The production adapter must use the typed `NoLocalModelLoadable` error
    // (shipped in #1093 / lane A PR-2) rather than a silent fallthrough when
    // no local GGUF is on disk.

    assert!(
        LLAMACPP_ADAPTER_SOURCE.contains("NoLocalModelLoadable"),
        "LlamaCppAdapter must use the typed NoLocalModelLoadable error for missing-model cases. \
         If you replaced it with a silent skip / Result::Ok-with-None / log-and-continue, \
         the no-fallback alpha contract is violated and the user gets 1 tok/sec CPU instead \
         of a clear 'install missing artifact' error."
    );
}
