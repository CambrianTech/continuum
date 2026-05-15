//! Regression test for the no-CPU-fallback alpha contract (#1262 → #1275).
//!
//! Continuum's documented contract per `project_continuum_alpha_product_bar_sensory_personas.md`
//! and `docs/architecture/SENSORY-PERSONA-ALPHA-CONTRACT.md` is **NO silent CPU fallback**:
//! standard personas use `SiliconResidencyRequirement::GpuOrUnifiedMemoryOnly` and the model
//! resolver is supposed to refuse rather than fall through to CPU.
//!
//! The contract is enforced at runtime by `inference::model::select_best_device` (panics if
//! no GPU device is available) and by `inference::ort_providers` (CPU-fallback comment block
//! at line ~119). This test asserts those invariants by inspection of the source files —
//! a future PR that removes the loud-fail panic, weakens the message, or adds a silent
//! CPU branch will fail this test.
//!
//! This is a **forbidden-strings ratchet** following the established pattern from lane F
//! PR-2 (#1129 — TS persona forbidden-strings) applied to the Rust inference layer.
//!
//! Audit context:
//!   https://github.com/CambrianTech/continuum/issues/1262#issuecomment-4461757997

const SELECT_BEST_DEVICE_SOURCE: &str =
    include_str!("../src/inference/model.rs");

const ORT_PROVIDERS_SOURCE: &str =
    include_str!("../src/inference/ort_providers.rs");

const LLAMACPP_ADAPTER_SOURCE: &str =
    include_str!("../src/inference/llamacpp_adapter.rs");

#[test]
fn select_best_device_panics_loudly_on_no_gpu() {
    // The function MUST contain an explicit panic with a message that tells
    // the user why we won't fall through to CPU. If a future PR removes the
    // panic, weakens the message, or replaces it with a silent fallback
    // (e.g. `Device::Cpu` return), this test fails and the no-CPU-fallback
    // alpha contract is preserved.

    assert!(
        SELECT_BEST_DEVICE_SOURCE.contains("panic!(\"No GPU device available for inference. CPU fallback is disabled.\")"),
        "select_best_device must loud-fail with the documented message. \
         If you changed it, update both this test and the alpha contract docs \
         (docs/architecture/SENSORY-PERSONA-ALPHA-CONTRACT.md). \
         A silent fallthrough to Device::Cpu was the bug #1262 was filed for."
    );

    // Belt-and-suspenders: verify the function explicitly returns Device early
    // for both Cuda and Metal cases (the only legitimate non-panic exits).
    assert!(
        SELECT_BEST_DEVICE_SOURCE.contains("Device::new_cuda(0)"),
        "select_best_device must try CUDA before panicking"
    );
    assert!(
        SELECT_BEST_DEVICE_SOURCE.contains("Device::new_metal(0)"),
        "select_best_device must try Metal before panicking"
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
