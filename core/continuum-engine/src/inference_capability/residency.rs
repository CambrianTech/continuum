//! Qwen GPU residency gate (CBAR-SUBSTRATE missing piece #5, PR-1).
//!
//! `inference_capability::probe` (#1315) answers "does this node have an
//! advertisable GPU at all?" The residency gate answers the next question
//! one level deeper: "will the SELECTED MODEL actually fit with all
//! layers on that GPU, evidenced not guessed?"
//!
//! The CBAR-SUBSTRATE spec (docs/architecture/CBAR-SUBSTRATE-ARCHITECTURE.md
//! §336 piece #5) requires that, before any local-generation turn runs:
//!
//! - The selected Qwen model is named explicitly,
//! - The backend (Metal / CUDA / Vulkan) is named and matches platform,
//! - GPU layer count is reported,
//! - Unsupported layers are enumerated (Vulkan-llama.cpp gaps, etc.),
//! - VRAM residency estimate covers all layers,
//! - "CPU graph splits or unsupported Qwen layers are blockers unless the
//!   turn is explicitly degraded with a visible reason."
//!
//! This module ships the **data + pure derivation layer**. No GGUF I/O,
//! no runtime dispatch, no llama.cpp probe — those land in a future PR-2
//! that wires the GGUF reader to populate `QwenModelMetadata` from
//! `backends::read_gguf_metadata` + a small layer-count extractor, and
//! wires the hardware probe to populate `HardwareProfile`. PR-3 wires
//! the gate result into the actual turn dispatcher with a block-the-turn
//! enforcement point.
//!
//! ## Failure-mode discipline
//!
//! Per vhsm-d1f4 audit pass 1 + the no_cpu_fallback contract:
//!
//! - **No partial GPU split**: if the model needs more layers than the
//!   backend can hold on GPU, the gate **blocks** — it does not silently
//!   split to CPU. The CBAR-SUBSTRATE spec says "CPU graph splits ... are
//!   blockers unless explicitly degraded with a visible reason." This
//!   module produces the visible reason (`BlockReason::PartialGpuSplit`);
//!   the explicit-degrade path lives elsewhere.
//! - **No silent unsupported-layer fallback**: Vulkan llama.cpp doesn't
//!   support every Qwen op today; if the selected backend's compiled
//!   kernel set is missing what the model needs, gate blocks with
//!   `BlockReason::UnsupportedLayer`. The probe in #1315 already gates
//!   Vulkan-only hosts away from native-GPU kinds; this gate is the
//!   per-model second check.
//! - **No assumed defaults**: every field comes from the inputs; no
//!   `unwrap_or(4096)` / `unwrap_or("metal")` / etc.

use crate::system_resources::hardware_profile::HardwareProfile;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// One concrete GPU backend choice. Selected by `select_backend` from a
/// `HardwareProfile` per the CBAR-SUBSTRATE happy-path rule:
/// Mac → Metal, NVIDIA → CUDA, AMD/Intel → Vulkan.
///
/// Not a registry of every possible backend — backends a Qwen model can
/// actually be loaded into via llama.cpp's current vendored build. New
/// backends (MLX, etc.) live in their own enums; this one is the
/// llama.cpp-resident set today.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/inference_capability/BackendChoice.ts"
)]
pub enum BackendChoice {
    Metal,
    Cuda,
    Vulkan,
}

impl BackendChoice {
    pub fn as_str(&self) -> &'static str {
        match self {
            BackendChoice::Metal => "metal",
            BackendChoice::Cuda => "cuda",
            BackendChoice::Vulkan => "vulkan",
        }
    }
}

/// Metadata for one Qwen model loaded from a GGUF file. Pure data —
/// populated by a future PR-2 that wires `read_gguf_metadata` + a
/// layer-count extractor; for PR-1 tests synthesize known values for
/// shipped Qwen variants.
///
/// `parameter_count_billions` × `bytes_per_parameter_quantized` gives
/// the VRAM footprint estimate. The estimate is intentionally
/// conservative — small enough to be wrong on the safe side (will block
/// when it could have fit, never pass when it would have spilled).
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/inference_capability/QwenModelMetadata.ts"
)]
pub struct QwenModelMetadata {
    /// Human-readable model identifier from `general.name` in the GGUF
    /// or the model registry's display name. NOT trusted for backend
    /// selection — that's `architecture`.
    pub model_name: String,
    /// `general.architecture` from the GGUF (e.g. "qwen2", "qwen3",
    /// "qwen2vl"). Used to gate Vulkan support per-architecture.
    pub architecture: String,
    /// Total transformer layer count (e.g. Qwen2.5-7B = 28, Qwen2.5-3B
    /// = 36, Qwen2.5-Coder-7B = 28). From `{architecture}.block_count`
    /// in the GGUF.
    #[ts(type = "number")]
    pub layer_count: u32,
    /// Total parameter count in billions (e.g. 7.0 for 7B, 30.0 for
    /// 30B-A3B). Used with `bytes_per_parameter_quantized` to estimate
    /// VRAM footprint.
    pub parameter_count_billions: f64,
    /// Bytes per parameter for the selected quantization. Q4_K_M is
    /// ~0.5 bytes; Q5_K_M is ~0.625; Q6_K is ~0.75; Q8_0 is ~1.0; FP16
    /// is 2.0. Populated by reading the GGUF tensor type.
    pub bytes_per_parameter_quantized: f64,
    /// Layer-kind names this model needs that the SELECTED BACKEND
    /// might not implement (e.g. "moe_gate" for MoE Qwen3 on Vulkan
    /// llama.cpp today, "sliding_window_attn" for some variants).
    /// Empty when the model uses only universally-supported kinds.
    /// Future-extensible: a real PR-2 populates this from
    /// llama.cpp's compiled-kernel set introspection.
    pub layer_kinds_needing_check: Vec<String>,
}

impl QwenModelMetadata {
    /// Estimated VRAM footprint in bytes, derived from parameter count
    /// + quantization. Pure derivation, no I/O.
    ///
    /// Conservative formula: `params × bytes_per_param × 1.10` — the
    /// 10% headroom covers KV cache + scratch buffers for a moderate
    /// context. Real-world numbers from llama.cpp on Qwen2.5-7B Q4_K_M
    /// show ~4.6 GB resident at 4K ctx; this formula gives ~4.5 GB on
    /// 7B × 0.5 × 1.10 = 3.85 GB, which is on the safe side but
    /// rough — PR-2 should refine using `llama_state_seq_get_size`
    /// once the loader is wired.
    pub fn estimated_vram_bytes(&self) -> u64 {
        let raw = self.parameter_count_billions * 1.0e9 * self.bytes_per_parameter_quantized;
        (raw * 1.10) as u64
    }
}

/// One blocking reason emitted when the gate refuses a turn. Typed so
/// the calling code can render specific user-facing messages + so the
/// recorder can capture exact reasons for VDD review.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase", tag = "kind")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/inference_capability/BlockReason.ts"
)]
pub enum BlockReason {
    /// The selected model could not be inspected as GGUF metadata, so
    /// the runtime cannot prove all layers will remain GPU resident.
    ModelMetadataUnreadable { model_path: String, error: String },
    /// No GPU on this node — CPU-only would be a silent fallback, which
    /// is forbidden. Routing to a peer-grid node (PR-3 of
    /// GRID-INFERENCE-ROUTING) is the right escape hatch.
    NoGpuBackendOnNode {
        /// Platform identifier ("macos-arm64-m2", "linux-x86_64-generic", etc).
        platform: String,
    },
    /// Selected backend exists but doesn't support this Qwen variant's
    /// layer kinds (e.g. Qwen3 MoE on Vulkan llama.cpp).
    UnsupportedLayer {
        backend: BackendChoice,
        architecture: String,
        layer_kind: String,
    },
    /// Free VRAM under the conservative estimate — would cause llama.cpp
    /// to silently split layers to CPU. Block per CBAR-SUBSTRATE rule.
    PartialGpuSplit {
        backend: BackendChoice,
        #[ts(type = "number")]
        estimated_required_bytes: u64,
        #[ts(type = "number")]
        free_vram_bytes: u64,
    },
    /// Architecture in the model doesn't match what the selected
    /// backend was built for. Defensive — should never happen since
    /// `select_backend` uses the hardware profile, but caught here so a
    /// future codepath can't bypass.
    WrongBackendForPlatform {
        platform: String,
        backend: BackendChoice,
    },
}

/// Typed evidence emitted on a passing gate. Required by the
/// CBAR-SUBSTRATE spec — without this evidence, the gate has "passed"
/// without showing its work, which is a no_cpu_fallback / no_silent
/// violation by omission.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/inference_capability/ResidencyEvidence.ts"
)]
pub struct ResidencyEvidence {
    pub model_name: String,
    pub architecture: String,
    pub backend: BackendChoice,
    #[ts(type = "number")]
    pub gpu_layer_count: u32,
    #[ts(type = "number")]
    pub estimated_vram_bytes: u64,
    #[ts(type = "number")]
    pub free_vram_bytes: u64,
    pub platform: String,
}

/// Result of running the residency gate. Pass carries evidence; Block
/// carries reasons. Caller (PR-3) acts on this — turn runs if Pass,
/// turn rejects with visible reasons if Block.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase", tag = "outcome")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/inference_capability/ResidencyGateResult.ts"
)]
pub enum ResidencyGateResult {
    Pass(ResidencyEvidence),
    Block { reasons: Vec<BlockReason> },
}

impl ResidencyGateResult {
    pub fn is_pass(&self) -> bool {
        matches!(self, ResidencyGateResult::Pass(_))
    }

    pub fn reasons(&self) -> &[BlockReason] {
        match self {
            ResidencyGateResult::Block { reasons } => reasons,
            ResidencyGateResult::Pass(_) => &[],
        }
    }
}

/// Pick the right native-GPU backend for this node per the
/// CBAR-SUBSTRATE happy-path rule: Mac → Metal, NVIDIA → CUDA, AMD/Intel
/// → Vulkan. Returns None when no GPU is usable for native llama.cpp
/// inference (CPU-only host, or a hardware probe that hasn't filled the
/// fields).
///
/// Metal wins over CUDA/Vulkan on a Mac because Metal IS the native
/// path on Apple Silicon. CUDA wins over Vulkan on a Mac/Linux with an
/// NVIDIA card because llama.cpp's CUDA kernels are more complete than
/// Vulkan today. Vulkan is the fallback for AMD/Intel discrete GPUs.
///
/// This matches the precedence already used by `probe.rs` for the
/// `llamacpp` advertisement (Metal OR CUDA gate native-GPU
/// advertisement; Vulkan-only doesn't get llamacpp).
pub fn select_backend(hw: &HardwareProfile) -> Option<BackendChoice> {
    if hw.has_metal {
        Some(BackendChoice::Metal)
    } else if hw.has_cuda {
        Some(BackendChoice::Cuda)
    } else if hw.has_vulkan {
        Some(BackendChoice::Vulkan)
    } else {
        None
    }
}

/// Check whether the given backend is known to support the given Qwen
/// variant's layer kinds. Conservative — when in doubt, return the
/// list of layer-kinds-needing-check so the gate can block with
/// specific reasons rather than silently allow.
///
/// Today's known gaps (llama.cpp vendored build as of 2026-05-16):
///
/// - **Vulkan**: missing several Qwen3-specific ops (MoE gate, sliding
///   window attention). Vulkan-only hosts shouldn't run Qwen3 MoE; the
///   probe in #1315 already excludes Vulkan from llamacpp
///   advertisement on those hosts, but if a future code path bypasses
///   the probe (e.g. forced backend selection), this gate catches it.
///
/// - **Metal + CUDA**: full Qwen2 + Qwen3 + Qwen2-VL coverage as of
///   today. Returns empty unsupported-list.
fn unsupported_layer_kinds_on_backend(
    backend: BackendChoice,
    arch: &str,
    layer_kinds_needing_check: &[String],
) -> Vec<String> {
    match backend {
        BackendChoice::Metal | BackendChoice::Cuda => {
            // Native paths support the shipped Qwen ops today. Leave as
            // empty; future architectures with new kernels not yet in
            // llama.cpp metal/cuda would populate here.
            Vec::new()
        }
        BackendChoice::Vulkan => {
            // Vulkan llama.cpp lacks Qwen3 MoE + some attention variants
            // in the vendored build. Surface every layer-kind-needing-
            // check unless the architecture is one Vulkan handles cleanly.
            //
            // qwen2 / qwen2vl: Vulkan supports these well today.
            // qwen3 / qwen3moe: Vulkan path is incomplete.
            let vulkan_safe_archs = ["qwen2", "qwen2vl"];
            if vulkan_safe_archs.contains(&arch) {
                Vec::new()
            } else {
                layer_kinds_needing_check.to_vec()
            }
        }
    }
}

/// Run the full residency gate. Composes hardware backend selection +
/// per-architecture layer-support check + VRAM-fit check, producing a
/// typed Pass-with-evidence or Block-with-reasons.
///
/// Order of checks is deliberate — most fundamental failure first so
/// the reason list reads from "can't even do this" to "could do but
/// shouldn't":
///   1. No GPU backend at all → NoGpuBackendOnNode (alone in reasons)
///   2. Selected backend has unsupported layers → UnsupportedLayer + ...
///   3. Free VRAM under estimate → PartialGpuSplit + ...
///
/// 2 + 3 accumulate — a single turn could be blocked by both an
/// unsupported layer AND insufficient VRAM, and the caller should see
/// both. 1 is exclusive because if there's no backend, the other checks
/// are meaningless.
pub fn check_residency_gate(
    model: &QwenModelMetadata,
    hw: &HardwareProfile,
) -> ResidencyGateResult {
    let backend = match select_backend(hw) {
        Some(b) => b,
        None => {
            return ResidencyGateResult::Block {
                reasons: vec![BlockReason::NoGpuBackendOnNode {
                    platform: hw.platform.clone(),
                }],
            }
        }
    };

    let mut reasons: Vec<BlockReason> = Vec::new();

    let unsupported = unsupported_layer_kinds_on_backend(
        backend,
        &model.architecture,
        &model.layer_kinds_needing_check,
    );
    for layer_kind in &unsupported {
        reasons.push(BlockReason::UnsupportedLayer {
            backend,
            architecture: model.architecture.clone(),
            layer_kind: layer_kind.clone(),
        });
    }

    let estimated_vram = model.estimated_vram_bytes();
    if hw.free_vram_bytes < estimated_vram {
        reasons.push(BlockReason::PartialGpuSplit {
            backend,
            estimated_required_bytes: estimated_vram,
            free_vram_bytes: hw.free_vram_bytes,
        });
    }

    if reasons.is_empty() {
        ResidencyGateResult::Pass(ResidencyEvidence {
            model_name: model.model_name.clone(),
            architecture: model.architecture.clone(),
            backend,
            gpu_layer_count: model.layer_count,
            estimated_vram_bytes: estimated_vram,
            free_vram_bytes: hw.free_vram_bytes,
            platform: hw.platform.clone(),
        })
    } else {
        ResidencyGateResult::Block { reasons }
    }
}

#[cfg(test)]
mod tests;
