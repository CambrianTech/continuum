//! ORT GPU Execution Provider configuration — single source of truth.
//!
//! ## Why this exists
//!
//! Per Joel's architectural rule (2026-05-01): "lack of GPU integration is
//! forbidden, GPU acceleration in all cases." Continuum runs on GPU
//! everywhere — Metal native, Metal via Docker (DMR), CUDA via Docker GPU
//! runner, Vulkan. CPU-fallback paths are categorically excluded.
//!
//! ORT (the `ort` crate wrapping ONNX Runtime) ships an implicit CPU
//! Execution Provider as the final fallback when none of the GPU EPs in
//! the user-supplied list can handle a node. That implicit fallback is
//! exactly what this rule forbids — it's the silent-degradation vector
//! that produced #964 (800-900% MLAS CPU spike during chat-induced
//! embedding calls on Mac M5 Pro).
//!
//! ## What this provides
//!
//! `build_ort_gpu_execution_providers()` — returns the GPU EP list that
//! every ORT consumer in this crate should use. Hard-fails with an
//! actionable error when no GPU EP is configured for the current
//! platform / cargo feature combination, so callers cannot accidentally
//! pass an empty list to ORT (which would let the implicit CPU EP take
//! over silently).
//!
//! ## Pre-fix bugs this surface fixes (#964)
//!
//! Before this helper, three call sites ALL had the same broken cfg
//! gate: `#[cfg(all(feature = "coreml", target_os = "macos"))]`. There
//! is no `coreml` feature in continuum-core's Cargo.toml — the actual
//! feature is `metal` which propagates to `ort/coreml`. So the cfg
//! attribute was always false, the CoreML EP was never added, and ORT's
//! implicit CPU EP took every op. Three production sites:
//!
//!   - memory/embedding.rs       (fastembed)
//!   - live/audio/tts/piper.rs   (TTS)
//!   - live/audio/stt/moonshine.rs (STT)
//!
//! All three: dead GPU branch → silent CPU usage → 800-900% CPU spike.
//!
//! Centralizing here means ANY future ORT consumer in continuum-core
//! gets the right cfg gating + the hard-fail enforcement automatically,
//! and there is ONE place to add ROCm / OpenVINO / DirectML / etc. when
//! those EPs become viable.
//!
//! ## Cargo feature matrix
//!
//!   --features metal    → CoreML EP (Mac, Apple Silicon GPU)
//!   --features cuda     → CUDA EP (Linux+Nvidia, WSL+Nvidia, Windows+Nvidia)
//!
//! Coverage gaps tracked separately:
//!   - Linux+AMD (ROCm EP) — needs ort/rocm feature wiring
//!   - Linux+Intel (Vulkan/OpenVINO EP) — needs ort/openvino feature
//!   - Windows-native (DirectML EP) — needs ort/directml feature
//!
//! These gaps mean we still hard-fail on those platforms today rather
//! than silently routing to CPU — which is correct per the rule. Builds
//! that fail here are a signal to add the missing EP wiring, not to
//! relax the no-CPU-fallback constraint.

use ort::execution_providers::ExecutionProviderDispatch;

/// Build the GPU Execution Provider list for an ORT session on this
/// platform / build configuration.
///
/// Returns:
///   `Ok(Vec<...>)` — non-empty list of GPU EPs ORT should try in order
///   `Err(String)` — no GPU EP configured for this platform/feature combo;
///                   actionable message naming the cargo feature flags
///                   the caller's build needs
///
/// Callers MUST propagate the error rather than passing an empty list to
/// ORT — that would let ORT's implicit CPU EP take every node, the exact
/// silent-fallback shape this helper exists to prevent (see #964).
pub fn build_ort_gpu_execution_providers() -> Result<Vec<ExecutionProviderDispatch>, String> {
    let mut providers: Vec<ExecutionProviderDispatch> = Vec::new();

    #[cfg(all(feature = "metal", target_os = "macos"))]
    {
        use ort::execution_providers::CoreMLExecutionProvider;
        providers.push(CoreMLExecutionProvider::default().build());
    }

    #[cfg(all(feature = "cuda", not(target_os = "macos")))]
    {
        use ort::execution_providers::CUDAExecutionProvider;
        providers.push(CUDAExecutionProvider::default().build());
    }

    // ROCm — Linux + AMD GPU. Builds when --features rocm + ROCm runtime
    // libs are installed. Carl on Linux+AMD picks this path.
    #[cfg(all(feature = "rocm", target_os = "linux"))]
    {
        use ort::execution_providers::ROCmExecutionProvider;
        providers.push(ROCmExecutionProvider::default().build());
    }

    // DirectML — Windows native. Works with any DX12-compatible GPU
    // (Nvidia / AMD / Intel). Carl on Windows-native picks this path.
    #[cfg(all(feature = "directml", target_os = "windows"))]
    {
        use ort::execution_providers::DirectMLExecutionProvider;
        providers.push(DirectMLExecutionProvider::default().build());
    }

    // OpenVINO — Intel CPU/GPU/VPU. Linux + Windows. NOT a CPU fallback
    // (OpenVINO targets Intel's accelerators specifically). Carl on
    // Intel-Arc Linux or Windows picks this path.
    #[cfg(feature = "openvino")]
    {
        use ort::execution_providers::OpenVINOExecutionProvider;
        providers.push(OpenVINOExecutionProvider::default().build());
    }

    if providers.is_empty() {
        return Err(format!(
            "No GPU Execution Provider configured for ORT on this build. \
             Per architecture, CPU fallback is forbidden — ORT consumers \
             (embedding, TTS, STT, vision) must run on GPU. \
             Build with the appropriate cargo feature: \
             '--features metal' (Mac, Apple Silicon GPU via CoreML EP), \
             '--features cuda' (Linux+Nvidia, WSL+Nvidia, Windows+Nvidia), \
             '--features rocm' (Linux+AMD), \
             '--features directml' (Windows-native, any DX12 GPU), \
             '--features openvino' (Linux+Intel / Windows+Intel). \
             Detected: target_os={}, features=(metal={}, cuda={}, rocm={}, directml={}, openvino={}).",
            std::env::consts::OS,
            cfg!(feature = "metal"),
            cfg!(feature = "cuda"),
            cfg!(feature = "rocm"),
            cfg!(feature = "directml"),
            cfg!(feature = "openvino"),
        ));
    }

    Ok(providers)
}
