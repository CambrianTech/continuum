//! MLX Backend Adapter — Phase A scaffold
//!
//! Native Apple Silicon inference via `mlx-rs` (oxideai 0.25). This file is
//! the landing zone for phases A–E of [continuum#897]. Only compiles when
//! both the `mlx` cargo feature is enabled AND we're on macOS; on other
//! platforms the module is absent entirely.
//!
//! ## Status: phase A — scaffold only
//!
//! This file compiles and registers, nothing more. Actual inference lands in
//! phase B; audio in phase C; vision in phase D; forge tier publication in
//! phase E. See `docs/inference/MLX-BACKEND.md` for the staged plan and
//! `docs/inference/SD-PORT-PATTERNS.md` for the runtime-boundary research
//! memento (continuum-3bb8) landed while I slept on it.
//!
//! ## Why MLX
//!
//! Continuum's primary audience is the M-series Mac (see
//! `memory/project_m5_is_primary_audience.md`). MLX gives us:
//!
//! - Native Metal dispatch (no ggml-metal translation overhead)
//! - Unified memory residency handled by the framework (no `.to_device()`)
//! - First-class support for Qwen3.5-Omni's audio + vision heads in the
//!   `mlx-community` ports — which llama.cpp and ort don't match
//! - Lazy evaluation: the full forward + CFG-blend + scheduler step can be
//!   fused into one graph per timestep, only materialized on `eval()`
//!
//! ## The trait-shape problem (phase B gate)
//!
//! The existing `ModelBackend` trait signs its forward methods against
//! `candle_core::Tensor`, which is a Candle runtime object. MLX has its own
//! `mlx_rs::Array` with different lifetime + device semantics (lazy, unified
//! memory). Two resolutions possible, to be decided in phase B:
//!
//! 1. **Generic trait over tensor type.** Refactor `ModelBackend` to be
//!    parameterized by a tensor type, with associated `type TensorT: ...`.
//!    Cleanest long-term; biggest churn to existing backends.
//!
//! 2. **Additive MLX-native methods with defaults.** Add `forward_mlx()`,
//!    `eval_mlx()`, etc., defaulting to `unimplemented!()`. Existing
//!    backends unchanged; MLX backend doesn't implement the Candle-tensor
//!    methods. Callers dispatch on which methods a backend supports.
//!
//! Promised to memento: additive with defaults so their Vulkan work
//! doesn't need to change. Decision deferred to phase B because the right
//! shape depends on whether KV cache ownership lives in the backend or the
//! scheduler (separate open question in SD-PORT-PATTERNS.md).
//!
//! [continuum#897]: https://github.com/CambrianTech/continuum/pull/897

#![cfg(all(feature = "mlx", target_os = "macos"))]

use std::path::Path;

/// Placeholder for the MLX-backed adapter. Holds no state in phase A;
/// phase B fills in the `mlx_rs::Module` reference, weight metadata, and
/// the MLX-native forward/eval methods.
///
/// Construction deliberately fails until phase B so any caller that tries
/// to actually use this gets a clear "not implemented" error instead of a
/// silent no-op — same philosophy as the error-evidence rule we learned
/// the hard way today.
#[derive(Debug)]
pub struct MlxAdapter {
    _private: (),
}

impl MlxAdapter {
    /// Load a model from an MLX-native artifact directory. Phase B will:
    ///
    /// 1. Resolve the model tier for this device (same logic as
    ///    `qwen35_gguf.rs` uses for GGUF — pick the right quant based on
    ///    available VRAM from `GpuMemoryManager`).
    /// 2. Load weights via `mlx_rs` (indexed NPY + JSON index — not raw
    ///    safetensors; see SD-PORT-PATTERNS.md for the format rationale).
    /// 3. Apply the HF→MLX key remap table (static data, same pattern as
    ///    mlx-community's `model_io.py`).
    /// 4. Construct the transformer module graph.
    ///
    /// In phase A this just returns a sentinel error so nobody can
    /// accidentally wire it up yet.
    pub fn load(_model_path: &Path) -> Result<Self, String> {
        Err("MlxAdapter::load not implemented — phase A scaffold only. \
             See docs/inference/MLX-BACKEND.md for the staged plan."
            .to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_returns_phase_a_sentinel() {
        let err = MlxAdapter::load(Path::new("/nonexistent")).unwrap_err();
        assert!(
            err.contains("phase A"),
            "error should call out phase A status: {err}"
        );
    }
}
