//! Residency-gate enforcement helper (CBAR-PIECE-5 PR-4).
//!
//! Composes the three pure layers shipped in PR-1/PR-2/PR-3 into ONE
//! function callers can invoke before launching a local-generation
//! turn:
//!
//!   `enforce_residency(model_path) -> Result<ResidencyEvidence, Box<ResidencyBlock>>`
//!
//! Pass → caller gets typed evidence to record + proceeds with the turn.
//! Block → caller refuses the turn rather than silently letting llama.cpp
//! split layers to CPU.
//!
//! Wiring (not in this PR — left for callers to integrate at the
//! adapter-construction or per-turn point that fits their concurrency
//! model best):
//!
//! ```ignore
//! // In LlamaCppAdapter::try_new_from, after resolving model_path:
//! use crate::inference_capability::enforcement::{enforce_residency, ResidencyBlock};
//!
//! let evidence = enforce_residency(&model_path).map_err(|block: ResidencyBlock| {
//!     // Extend NoLocalModelLoadable or wrap with a new enum variant
//!     // surfacing the typed BlockReason list to the caller.
//!     NoLocalModelLoadable::residency_blocked(block)
//! })?;
//! // proceed with adapter construction, store evidence for telemetry
//! ```
//!
//! ## Why a helper, not wired directly
//!
//! - The injection point is a hot path (run_render → adapter → generate).
//!   The helper is pure-composition + can be tested independently of any
//!   adapter or dispatcher. Wiring into a specific call-site involves
//!   choices about caching, per-turn-vs-per-load, and error-type
//!   extensions that deserve their own PR + review.
//! - PR-4 (this) ships the typed composition. PR-5 ships the wiring.
//!   This isolates the riskier change.

use crate::inference_capability::gguf_loader::read_qwen_model_metadata;
use crate::inference_capability::hw_probe::probe_hardware_profile;
use crate::inference_capability::residency::{
    check_residency_gate, BlockReason, QwenModelMetadata, ResidencyEvidence, ResidencyGateResult,
};
use crate::inference_capability::types::HardwareProfile;
use std::path::Path;

/// Typed error for the enforcement path. Carries the BlockReasons
/// emitted by the gate PLUS the model + hardware context that produced
/// them, so callers can render full diagnostics ("could not run Qwen3
/// MoE on AMD Vulkan because moe_gate unsupported, free vram 16GB <
/// estimated 17GB").
///
/// Not derived `ts-rs` because the use-site is Rust-internal error
/// propagation — the wire-shape lives in `ResidencyGateResult`.
#[derive(Debug, Clone, PartialEq)]
pub struct ResidencyBlock {
    pub reasons: Vec<BlockReason>,
    pub attempted_model: QwenModelMetadata,
    pub attempted_hardware: HardwareProfile,
}

impl std::fmt::Display for ResidencyBlock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Qwen residency gate REFUSED turn for model '{}' (arch={}, {}B params, ~{:.1}GB est) \
             on {} (metal={}, cuda={}, vulkan={}, {} GB free VRAM). Reasons:",
            self.attempted_model.model_name,
            self.attempted_model.architecture,
            self.attempted_model.parameter_count_billions,
            self.attempted_model.estimated_vram_bytes() as f64 / 1.0e9,
            self.attempted_hardware.platform,
            self.attempted_hardware.has_metal,
            self.attempted_hardware.has_cuda,
            self.attempted_hardware.has_vulkan,
            self.attempted_hardware.free_vram_bytes as f64 / 1.0e9,
        )?;
        for r in &self.reasons {
            write!(f, " {r:?};")?;
        }
        Ok(())
    }
}

impl std::error::Error for ResidencyBlock {}

/// Compose probe + loader + gate into a single before-turn enforcement
/// call. Pure-composition over the three layers; the only I/O is
/// inherited from `read_qwen_model_metadata` (GGUF file read) +
/// `probe_hardware_profile` (per-backend FFI / subprocess + sysinfo).
///
/// Pass → `Ok(ResidencyEvidence)`: caller records the evidence in
/// trace + proceeds with the turn.
///
/// Block → `Err(ResidencyBlock)`: caller refuses the turn with full
/// diagnostic context. Per the CBAR-SUBSTRATE spec, the turn does NOT
/// silently degrade — caller renders the block reason to the user (or
/// routes to a peer-grid node via GRID-INFERENCE-ROUTING PR-3, once
/// that lands).
pub fn enforce_residency(model_path: &Path) -> Result<ResidencyEvidence, Box<ResidencyBlock>> {
    let model = read_qwen_model_metadata(model_path).map_err(|gguf_err| {
        // GGUF read failed BEFORE gate could run — synthesize a
        // ResidencyBlock with a probe of the current hardware so the
        // caller still gets typed context. The BlockReason for this
        // case is a degenerate `NoGpuBackendOnNode` if no GPU, or
        // `WrongBackendForPlatform` as a placeholder otherwise. The
        // GGUF error message is preserved in the model's model_name
        // field for visibility.
        //
        // This path triggers when the GGUF file is missing required
        // fields (per backends::read_gguf_metadata's no-fallback
        // posture) or the file isn't a GGUF at all.
        let hw = probe_hardware_profile();
        let placeholder_model = QwenModelMetadata {
            model_name: format!("GGUF_READ_FAILED({}): {gguf_err}", model_path.display()),
            architecture: "unknown".into(),
            layer_count: 0,
            parameter_count_billions: 0.0,
            bytes_per_parameter_quantized: 0.0,
            layer_kinds_needing_check: vec![],
        };
        let mut reasons = vec![BlockReason::ModelMetadataUnreadable {
            model_path: model_path.display().to_string(),
            error: gguf_err.to_string(),
        }];
        if !hw.has_metal && !hw.has_cuda && !hw.has_vulkan {
            reasons.push(BlockReason::NoGpuBackendOnNode {
                platform: hw.platform.clone(),
            });
        }
        Box::new(ResidencyBlock {
            reasons,
            attempted_model: placeholder_model,
            attempted_hardware: hw,
        })
    })?;

    let hw = probe_hardware_profile();

    match check_residency_gate(&model, &hw) {
        ResidencyGateResult::Pass(evidence) => Ok(evidence),
        ResidencyGateResult::Block { reasons } => Err(Box::new(ResidencyBlock {
            reasons,
            attempted_model: model,
            attempted_hardware: hw,
        })),
    }
}

/// Pure-composition variant that takes pre-built model + hw — useful
/// for callers that already have these in hand (e.g. cached at
/// adapter-load time) and want to re-check on each turn without
/// re-doing the GGUF read or hardware probe.
///
/// Same semantics as `enforce_residency` minus the I/O.
pub fn enforce_residency_with(
    model: QwenModelMetadata,
    hw: HardwareProfile,
) -> Result<ResidencyEvidence, Box<ResidencyBlock>> {
    match check_residency_gate(&model, &hw) {
        ResidencyGateResult::Pass(evidence) => Ok(evidence),
        ResidencyGateResult::Block { reasons } => Err(Box::new(ResidencyBlock {
            reasons,
            attempted_model: model,
            attempted_hardware: hw,
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inference_capability::residency::BackendChoice;

    fn qwen_7b_test() -> QwenModelMetadata {
        QwenModelMetadata {
            model_name: "Qwen2.5-7B-Test".into(),
            architecture: "qwen2".into(),
            layer_count: 28,
            parameter_count_billions: 7.0,
            bytes_per_parameter_quantized: 0.5,
            layer_kinds_needing_check: vec![],
        }
    }

    fn m5_pro_test() -> HardwareProfile {
        HardwareProfile {
            platform: "macos-arm64-m5pro".into(),
            has_metal: true,
            has_cuda: false,
            has_vulkan: false,
            free_vram_bytes: 32 * 1024 * 1024 * 1024,
            total_vram_bytes: 48 * 1024 * 1024 * 1024,
            cpu_cores: 16,
            system_ram_bytes: 64 * 1024 * 1024 * 1024,
        }
    }

    fn cpu_only_test() -> HardwareProfile {
        HardwareProfile {
            platform: "linux-x86_64-generic".into(),
            has_metal: false,
            has_cuda: false,
            has_vulkan: false,
            free_vram_bytes: 0,
            total_vram_bytes: 0,
            cpu_cores: 8,
            system_ram_bytes: 16 * 1024 * 1024 * 1024,
        }
    }

    // ===== enforce_residency_with — pure composition =====

    /// What this catches: model + hardware that pass the gate produce
    /// Ok(ResidencyEvidence). Smoke test for the happy path.
    #[test]
    fn enforce_with_passes_when_gate_passes() {
        let result = enforce_residency_with(qwen_7b_test(), m5_pro_test());
        assert!(result.is_ok());
        let ev = result.unwrap();
        assert_eq!(ev.model_name, "Qwen2.5-7B-Test");
        assert_eq!(ev.backend, BackendChoice::Metal);
    }

    /// What this catches: CPU-only host produces a ResidencyBlock with
    /// NoGpuBackendOnNode in reasons + full context preserved.
    #[test]
    fn enforce_with_blocks_on_cpu_only() {
        let result = enforce_residency_with(qwen_7b_test(), cpu_only_test());
        assert!(result.is_err());
        let block = result.unwrap_err();
        assert_eq!(block.attempted_model.model_name, "Qwen2.5-7B-Test");
        assert_eq!(block.attempted_hardware.platform, "linux-x86_64-generic");
        assert!(block
            .reasons
            .iter()
            .any(|r| matches!(r, BlockReason::NoGpuBackendOnNode { .. })));
    }

    /// What this catches: ResidencyBlock implements Display with both
    /// model + hardware context + reason list. Important for
    /// log/airc/UI rendering — the operator needs to see WHY in one
    /// line.
    #[test]
    fn residency_block_display_includes_context() {
        let block = enforce_residency_with(qwen_7b_test(), cpu_only_test()).unwrap_err();
        let display = format!("{block}");
        assert!(
            display.contains("Qwen2.5-7B-Test"),
            "model_name missing: {display}"
        );
        assert!(display.contains("linux-x86_64-generic"), "platform missing");
        assert!(display.contains("NoGpuBackendOnNode"), "reason missing");
        assert!(display.contains("REFUSED"), "REFUSED keyword missing");
    }

    /// What this catches: ResidencyBlock implements std::error::Error
    /// so callers can use it in `?` chains + dyn Error contexts.
    #[test]
    fn residency_block_implements_error_trait() {
        let block = enforce_residency_with(qwen_7b_test(), cpu_only_test()).unwrap_err();
        let _: &dyn std::error::Error = &block;
    }

    /// What this catches: ResidencyBlock equality holds (Clone + Eq).
    /// Used in test assertions + caching keys.
    #[test]
    fn residency_block_partial_eq() {
        let a = enforce_residency_with(qwen_7b_test(), cpu_only_test()).unwrap_err();
        let b = enforce_residency_with(qwen_7b_test(), cpu_only_test()).unwrap_err();
        assert_eq!(a, b);
    }

    /// What this catches: a 30B model on a 5GB-free Mac blocks with
    /// PartialGpuSplit + carries model_name (not generic message).
    /// Tests the FULL ResidencyBlock context preservation on the
    /// PartialGpuSplit path.
    #[test]
    fn enforce_with_partial_split_preserves_full_context() {
        let mut hw = m5_pro_test();
        hw.free_vram_bytes = 5 * 1024 * 1024 * 1024;
        let mut model = qwen_7b_test();
        model.parameter_count_billions = 30.0;
        model.model_name = "Qwen3-30B-A3B".into();

        let block = enforce_residency_with(model, hw).unwrap_err();
        assert_eq!(block.attempted_model.model_name, "Qwen3-30B-A3B");
        assert_eq!(block.attempted_model.parameter_count_billions, 30.0);
        assert!(block
            .reasons
            .iter()
            .any(|r| matches!(r, BlockReason::PartialGpuSplit { .. })));
    }

    // ===== enforce_residency — full I/O path =====

    /// What this catches: enforce_residency on a non-existent path
    /// returns ResidencyBlock with the GGUF-read error embedded in
    /// model_name (not a panic, not Ok). The caller sees a typed
    /// error + the actual GGUF problem in the error message.
    #[test]
    fn enforce_returns_block_on_missing_gguf() {
        let result = enforce_residency(Path::new("/nonexistent/missing.gguf"));
        assert!(result.is_err());
        let block = result.unwrap_err();
        // The model_name on this path encodes the GGUF read failure
        assert!(
            block
                .attempted_model
                .model_name
                .contains("GGUF_READ_FAILED"),
            "model_name should encode GGUF failure: {}",
            block.attempted_model.model_name
        );
        assert!(!block.reasons.is_empty());
    }

    /// What this catches: enforce_residency on Cargo.toml (a known
    /// non-GGUF file) returns ResidencyBlock. Symmetric with
    /// nonexistent-path case — non-readable-as-GGUF is treated the same.
    #[test]
    fn enforce_returns_block_on_non_gguf_file() {
        let path = std::env::current_dir()
            .ok()
            .map(|d| d.join("Cargo.toml"))
            .filter(|p| p.exists());
        let Some(path) = path else {
            return;
        };
        let result = enforce_residency(&path);
        assert!(result.is_err());
        let block = result.unwrap_err();
        assert!(block
            .attempted_model
            .model_name
            .contains("GGUF_READ_FAILED"));
    }
}
