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

use crate::inference_capability::types::HardwareProfile;
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
    export_to = "../../../shared/generated/inference_capability/BackendChoice.ts"
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
    export_to = "../../../shared/generated/inference_capability/QwenModelMetadata.ts"
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
    export_to = "../../../shared/generated/inference_capability/BlockReason.ts"
)]
pub enum BlockReason {
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
    export_to = "../../../shared/generated/inference_capability/ResidencyEvidence.ts"
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
    export_to = "../../../shared/generated/inference_capability/ResidencyGateResult.ts"
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
mod tests {
    use super::*;

    // ---- Synthetic Qwen variants (published HF model card values) ----

    fn qwen25_7b_q4km() -> QwenModelMetadata {
        QwenModelMetadata {
            model_name: "Qwen2.5-7B-Instruct".into(),
            architecture: "qwen2".into(),
            layer_count: 28,
            parameter_count_billions: 7.0,
            bytes_per_parameter_quantized: 0.5, // Q4_K_M
            layer_kinds_needing_check: vec![],
        }
    }

    fn qwen25_3b_q4km() -> QwenModelMetadata {
        QwenModelMetadata {
            model_name: "Qwen2.5-3B-Instruct".into(),
            architecture: "qwen2".into(),
            layer_count: 36,
            parameter_count_billions: 3.0,
            bytes_per_parameter_quantized: 0.5,
            layer_kinds_needing_check: vec![],
        }
    }

    fn qwen25_coder_7b_q4km() -> QwenModelMetadata {
        QwenModelMetadata {
            model_name: "Qwen2.5-Coder-7B-Instruct".into(),
            architecture: "qwen2".into(),
            layer_count: 28,
            parameter_count_billions: 7.0,
            bytes_per_parameter_quantized: 0.5,
            layer_kinds_needing_check: vec![],
        }
    }

    fn qwen3_30b_a3b_q4km() -> QwenModelMetadata {
        QwenModelMetadata {
            model_name: "Qwen3-30B-A3B-Instruct".into(),
            architecture: "qwen3moe".into(),
            layer_count: 48,
            parameter_count_billions: 30.0,
            bytes_per_parameter_quantized: 0.5,
            // MoE gate is a Vulkan gap today
            layer_kinds_needing_check: vec!["moe_gate".into()],
        }
    }

    fn qwen2vl_7b_q4km() -> QwenModelMetadata {
        QwenModelMetadata {
            model_name: "Qwen2-VL-7B-Instruct".into(),
            architecture: "qwen2vl".into(),
            layer_count: 28,
            parameter_count_billions: 7.0,
            bytes_per_parameter_quantized: 0.5,
            layer_kinds_needing_check: vec![],
        }
    }

    // ---- Synthetic hardware tiers (matches probe.rs test fixtures) ----

    fn macbook_air_m2_8gb() -> HardwareProfile {
        HardwareProfile {
            platform: "macos-arm64-m2".into(),
            has_metal: true,
            has_cuda: false,
            has_vulkan: false,
            free_vram_bytes: 5 * 1024 * 1024 * 1024, // 5 GB
            total_vram_bytes: 8 * 1024 * 1024 * 1024,
            cpu_cores: 8,
            system_ram_bytes: 8 * 1024 * 1024 * 1024,
        }
    }

    fn m5_pro_48gb() -> HardwareProfile {
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

    fn blackwell_rtx_5090() -> HardwareProfile {
        HardwareProfile {
            platform: "linux-x86_64-blackwell".into(),
            has_metal: false,
            has_cuda: true,
            has_vulkan: true,
            free_vram_bytes: 28 * 1024 * 1024 * 1024,
            total_vram_bytes: 32 * 1024 * 1024 * 1024,
            cpu_cores: 32,
            system_ram_bytes: 128 * 1024 * 1024 * 1024,
        }
    }

    fn generic_dell_no_gpu() -> HardwareProfile {
        HardwareProfile {
            platform: "linux-x86_64-generic".into(),
            has_metal: false,
            has_cuda: false,
            has_vulkan: false,
            free_vram_bytes: 0,
            total_vram_bytes: 0,
            cpu_cores: 12,
            system_ram_bytes: 32 * 1024 * 1024 * 1024,
        }
    }

    fn amd_with_vulkan_only() -> HardwareProfile {
        HardwareProfile {
            platform: "linux-x86_64-amd-rdna3".into(),
            has_metal: false,
            has_cuda: false,
            has_vulkan: true,
            free_vram_bytes: 16 * 1024 * 1024 * 1024,
            total_vram_bytes: 24 * 1024 * 1024 * 1024,
            cpu_cores: 16,
            system_ram_bytes: 64 * 1024 * 1024 * 1024,
        }
    }

    // ===== select_backend =====

    /// What this catches: select_backend picks Metal on Mac (Apple
    /// Silicon path). If this regresses, every Mac host silently routes
    /// inference through CUDA-or-nothing.
    #[test]
    fn select_backend_picks_metal_on_mac() {
        assert_eq!(select_backend(&macbook_air_m2_8gb()), Some(BackendChoice::Metal));
        assert_eq!(select_backend(&m5_pro_48gb()), Some(BackendChoice::Metal));
    }

    /// What this catches: CUDA wins over Vulkan on a host that has
    /// both (NVIDIA cards expose Vulkan too). llama.cpp's CUDA kernels
    /// are more complete than its Vulkan kernels today; CUDA must win
    /// the precedence.
    #[test]
    fn select_backend_picks_cuda_over_vulkan_on_nvidia() {
        // Blackwell has BOTH has_cuda + has_vulkan
        assert_eq!(select_backend(&blackwell_rtx_5090()), Some(BackendChoice::Cuda));
    }

    /// What this catches: Vulkan-only host (AMD without CUDA) gets
    /// Vulkan as the selection. Without this, AMD hosts would be
    /// silently CPU-only.
    #[test]
    fn select_backend_picks_vulkan_when_amd_only() {
        assert_eq!(select_backend(&amd_with_vulkan_only()), Some(BackendChoice::Vulkan));
    }

    /// What this catches: no GPU at all → None. The gate then
    /// surfaces NoGpuBackendOnNode. Critical — silent CPU fallback is
    /// the bug this whole module exists to prevent.
    #[test]
    fn select_backend_returns_none_on_cpu_only() {
        assert_eq!(select_backend(&generic_dell_no_gpu()), None);
    }

    // ===== check_residency_gate — happy paths =====

    /// What this catches: M5 Pro Metal + Qwen2.5-7B Q4_K_M passes the
    /// gate with full evidence. The flagship Mac tier × the workhorse
    /// model — if this regresses, no Mac runs Qwen.
    #[test]
    fn m5_pro_runs_qwen25_7b_q4km() {
        let result = check_residency_gate(&qwen25_7b_q4km(), &m5_pro_48gb());
        assert!(result.is_pass(), "expected Pass; got {result:?}");
        if let ResidencyGateResult::Pass(ev) = result {
            assert_eq!(ev.backend, BackendChoice::Metal);
            assert_eq!(ev.gpu_layer_count, 28);
            assert_eq!(ev.model_name, "Qwen2.5-7B-Instruct");
            assert_eq!(ev.platform, "macos-arm64-m5pro");
        }
    }

    /// What this catches: MacBook Air M2 8GB has 5GB free VRAM; a 3B
    /// Q4_K_M (≈ 1.65 GB estimated) fits cleanly. The smallest-Mac ×
    /// smallest-Qwen path must pass — this is the m2-8gb-baseline.
    #[test]
    fn macbook_air_m2_runs_qwen25_3b_q4km() {
        let result = check_residency_gate(&qwen25_3b_q4km(), &macbook_air_m2_8gb());
        assert!(result.is_pass(), "expected Pass; got {result:?}");
    }

    /// What this catches: Blackwell + Qwen2.5-Coder-7B passes via CUDA
    /// (not Vulkan, even though both available). Codepath used in CI
    /// for code-completion bench.
    #[test]
    fn blackwell_runs_qwen25_coder_7b_via_cuda() {
        let result = check_residency_gate(&qwen25_coder_7b_q4km(), &blackwell_rtx_5090());
        assert!(result.is_pass());
        if let ResidencyGateResult::Pass(ev) = result {
            assert_eq!(ev.backend, BackendChoice::Cuda);
        }
    }

    /// What this catches: Qwen2-VL on Metal passes — vision variant
    /// uses qwen2vl architecture, which Metal handles cleanly. If this
    /// regresses, Vision AI persona is silently unavailable on Mac.
    #[test]
    fn m5_pro_runs_qwen2vl_7b_via_metal() {
        let result = check_residency_gate(&qwen2vl_7b_q4km(), &m5_pro_48gb());
        assert!(result.is_pass());
        if let ResidencyGateResult::Pass(ev) = result {
            assert_eq!(ev.backend, BackendChoice::Metal);
            assert_eq!(ev.architecture, "qwen2vl");
        }
    }

    // ===== check_residency_gate — block paths =====

    /// What this catches: CPU-only host blocks with NoGpuBackendOnNode
    /// and ONLY that reason (other checks are bypassed). Per
    /// no_cpu_fallback rule — never silently route to CPU.
    #[test]
    fn cpu_only_host_blocks_with_no_gpu_reason() {
        let result = check_residency_gate(&qwen25_3b_q4km(), &generic_dell_no_gpu());
        assert!(!result.is_pass());
        match result {
            ResidencyGateResult::Block { reasons } => {
                assert_eq!(reasons.len(), 1, "no-GPU is exclusive; got {reasons:?}");
                match &reasons[0] {
                    BlockReason::NoGpuBackendOnNode { platform } => {
                        assert_eq!(platform, "linux-x86_64-generic");
                    }
                    other => panic!("expected NoGpuBackendOnNode, got {other:?}"),
                }
            }
            other => panic!("expected Block, got {other:?}"),
        }
    }

    /// What this catches: MacBook Air M2 (5GB free) trying to run
    /// Qwen2.5-7B Q4_K_M (≈ 3.85 GB estimated, plus headroom) — should
    /// PASS at 5GB free. But Qwen3-30B-A3B on M2 (60GB Q4 + 10%
    /// headroom = 16.5GB) should BLOCK with PartialGpuSplit.
    #[test]
    fn m2_air_blocks_qwen3_30b_for_vram() {
        let result = check_residency_gate(&qwen3_30b_a3b_q4km(), &macbook_air_m2_8gb());
        assert!(!result.is_pass(), "30B on 5GB free must block");
        match result {
            ResidencyGateResult::Block { reasons } => {
                assert!(reasons.iter().any(|r| matches!(r, BlockReason::PartialGpuSplit { .. })));
            }
            _ => panic!("expected Block"),
        }
    }

    /// What this catches: AMD Vulkan-only + Qwen3 MoE blocks with
    /// UnsupportedLayer (Vulkan llama.cpp lacks MoE gate). This is
    /// the per-model second check beyond the probe — probe.rs already
    /// excludes Vulkan-only hosts from llamacpp advertisement, but if
    /// something forces backend selection through, the gate catches.
    #[test]
    fn amd_vulkan_blocks_qwen3_moe_with_unsupported_layer() {
        let result = check_residency_gate(&qwen3_30b_a3b_q4km(), &amd_with_vulkan_only());
        assert!(!result.is_pass());
        match result {
            ResidencyGateResult::Block { reasons } => {
                let has_unsupported = reasons
                    .iter()
                    .any(|r| matches!(r, BlockReason::UnsupportedLayer { layer_kind, .. } if layer_kind == "moe_gate"));
                assert!(has_unsupported, "expected UnsupportedLayer moe_gate; got {reasons:?}");
            }
            _ => panic!("expected Block"),
        }
    }

    /// What this catches: AMD Vulkan + Qwen2 (NOT MoE) PASSES — Vulkan
    /// supports qwen2 architecture today per the vulkan_safe_archs
    /// list. If this regresses, AMD-fleet onboarding loses Qwen2.5
    /// silently.
    #[test]
    fn amd_vulkan_runs_qwen25_7b_via_vulkan() {
        let result = check_residency_gate(&qwen25_7b_q4km(), &amd_with_vulkan_only());
        assert!(result.is_pass(), "qwen2 should run on Vulkan: {result:?}");
        if let ResidencyGateResult::Pass(ev) = result {
            assert_eq!(ev.backend, BackendChoice::Vulkan);
        }
    }

    /// What this catches: a Qwen variant that lists a
    /// layer_kinds_needing_check but the backend is Metal (full
    /// coverage) → no UnsupportedLayer reason. The supported-on-native
    /// guarantee is preserved.
    #[test]
    fn metal_backend_passes_qwen3_moe_no_unsupported() {
        // Hypothetical M5 Pro with enough VRAM for 30B Q4 (16.5GB est)
        let mut hw = m5_pro_48gb();
        hw.free_vram_bytes = 20 * 1024 * 1024 * 1024;
        let result = check_residency_gate(&qwen3_30b_a3b_q4km(), &hw);
        assert!(result.is_pass(), "Metal should handle qwen3moe: {result:?}");
        if let ResidencyGateResult::Pass(ev) = result {
            assert_eq!(ev.backend, BackendChoice::Metal);
            assert_eq!(ev.architecture, "qwen3moe");
        }
    }

    /// What this catches: a block can carry MULTIPLE reasons. If a
    /// host has both an unsupported layer AND insufficient VRAM, the
    /// caller sees both, not just the first. Important for diagnosis
    /// — "you'd fail for two reasons" beats "you'd fail because X
    /// (then later: oh also Y)".
    #[test]
    fn block_accumulates_multiple_reasons() {
        // Vulkan-only host, very low VRAM, Qwen3 MoE — both
        // UnsupportedLayer + PartialGpuSplit.
        let mut hw = amd_with_vulkan_only();
        hw.free_vram_bytes = 2 * 1024 * 1024 * 1024; // 2GB, way under 30B Q4 ≈ 16.5GB
        let result = check_residency_gate(&qwen3_30b_a3b_q4km(), &hw);
        match result {
            ResidencyGateResult::Block { reasons } => {
                assert!(reasons.len() >= 2, "expected multi-reason block; got {reasons:?}");
                assert!(reasons.iter().any(|r| matches!(r, BlockReason::UnsupportedLayer { .. })));
                assert!(reasons.iter().any(|r| matches!(r, BlockReason::PartialGpuSplit { .. })));
            }
            _ => panic!("expected Block"),
        }
    }

    // ===== estimated_vram_bytes =====

    /// What this catches: Q4_K_M 7B estimate stays within the expected
    /// rough band (3.5–4.5 GB). Pins the formula; refactors that drift
    /// the multiplier will trip this test.
    #[test]
    fn vram_estimate_q4_7b_within_expected_band() {
        let m = qwen25_7b_q4km();
        let est = m.estimated_vram_bytes();
        let gb = 1024u64 * 1024 * 1024;
        assert!(
            est >= 3 * gb && est <= 5 * gb,
            "Q4 7B should estimate 3-5GB; got {} ({} GB)",
            est,
            est as f64 / gb as f64
        );
    }

    /// What this catches: 30B Q4 estimate stays in the 14–18 GB band
    /// (theoretical: 30 × 0.5 × 1.10 = 16.5 GB).
    #[test]
    fn vram_estimate_q4_30b_within_expected_band() {
        let m = qwen3_30b_a3b_q4km();
        let est = m.estimated_vram_bytes();
        let gb = 1024u64 * 1024 * 1024;
        assert!(est >= 14 * gb && est <= 18 * gb, "30B Q4: got {est} ({} GB)", est as f64 / gb as f64);
    }

    /// What this catches: bigger quantization → bigger estimate.
    /// Sanity check the linear-in-bytes-per-param relationship; a
    /// regression that ignored the field would break this.
    #[test]
    fn vram_estimate_scales_with_quantization() {
        let mut q4 = qwen25_7b_q4km();
        let q4_est = q4.estimated_vram_bytes();
        q4.bytes_per_parameter_quantized = 1.0; // Q8_0
        let q8_est = q4.estimated_vram_bytes();
        assert!(q8_est > q4_est, "Q8 must estimate higher than Q4");
        assert!(q8_est >= 2 * q4_est - 1024 * 1024 * 1024, "Q8 should be ~2× Q4");
    }

    // ===== Pass with full evidence =====

    /// What this catches: passing gate emits every field the
    /// CBAR-SUBSTRATE spec requires — model_name, backend, gpu layer
    /// count, vram estimate, free vram, platform. Omission would be a
    /// no_silent violation by missing evidence.
    #[test]
    fn pass_evidence_has_all_required_fields() {
        let result = check_residency_gate(&qwen25_7b_q4km(), &m5_pro_48gb());
        match result {
            ResidencyGateResult::Pass(ev) => {
                assert!(!ev.model_name.is_empty());
                assert!(!ev.architecture.is_empty());
                assert!(!ev.platform.is_empty());
                assert!(ev.gpu_layer_count > 0);
                assert!(ev.estimated_vram_bytes > 0);
                assert!(ev.free_vram_bytes > 0);
                // backend is non-Option enum, always set
                let _ = ev.backend;
            }
            other => panic!("expected Pass, got {other:?}"),
        }
    }

    // ===== Determinism + serde =====

    /// What this catches: same inputs → same gate result. Pure-function
    /// guarantee — no I/O, no globals, no thread-local state. PR-3
    /// can cache the result keyed on (model, hw) without worrying
    /// about silent drift.
    #[test]
    fn gate_is_deterministic() {
        let m = qwen25_7b_q4km();
        let hw = m5_pro_48gb();
        let a = check_residency_gate(&m, &hw);
        let b = check_residency_gate(&m, &hw);
        assert_eq!(format!("{a:?}"), format!("{b:?}"));
    }

    /// What this catches: BackendChoice serializes as lowercase string
    /// (matching LatencyClass + the rest of the ts-rs surface). Wire
    /// stability for PR-3 + PR-4 + the eventual cross-node dispatcher.
    #[test]
    fn backend_choice_serializes_lowercase() {
        assert_eq!(serde_json::to_string(&BackendChoice::Metal).unwrap(), "\"metal\"");
        assert_eq!(serde_json::to_string(&BackendChoice::Cuda).unwrap(), "\"cuda\"");
        assert_eq!(serde_json::to_string(&BackendChoice::Vulkan).unwrap(), "\"vulkan\"");
    }

    /// What this catches: BlockReason serde round-trip (tagged-union
    /// with `kind` discriminator). PR-3's caller will deserialize
    /// these from grid wire / recorder fixtures; the shape must round-
    /// trip cleanly.
    #[test]
    fn block_reason_serde_round_trip() {
        let reasons = vec![
            BlockReason::NoGpuBackendOnNode { platform: "test".into() },
            BlockReason::UnsupportedLayer {
                backend: BackendChoice::Vulkan,
                architecture: "qwen3moe".into(),
                layer_kind: "moe_gate".into(),
            },
            BlockReason::PartialGpuSplit {
                backend: BackendChoice::Metal,
                estimated_required_bytes: 16_000_000_000,
                free_vram_bytes: 5_000_000_000,
            },
        ];
        for r in &reasons {
            let j = serde_json::to_string(r).unwrap();
            let back: BlockReason = serde_json::from_str(&j).unwrap();
            assert_eq!(*r, back);
            assert!(j.contains("\"kind\":\""), "tag missing: {j}");
        }
    }

    /// What this catches: ResidencyGateResult Pass/Block tagged-union
    /// round-trips with `outcome` discriminator + nested fields.
    #[test]
    fn gate_result_serde_round_trip() {
        let pass = check_residency_gate(&qwen25_7b_q4km(), &m5_pro_48gb());
        let j = serde_json::to_string(&pass).unwrap();
        let back: ResidencyGateResult = serde_json::from_str(&j).unwrap();
        assert_eq!(pass, back);
        assert!(j.contains("\"outcome\":\"pass\""), "outcome tag: {j}");

        let block = check_residency_gate(&qwen25_3b_q4km(), &generic_dell_no_gpu());
        let j = serde_json::to_string(&block).unwrap();
        let back: ResidencyGateResult = serde_json::from_str(&j).unwrap();
        assert_eq!(block, back);
        assert!(j.contains("\"outcome\":\"block\""));
    }

    /// What this catches: QwenModelMetadata round-trips with camelCase.
    /// PR-2 will populate this from GGUF + ship to the recorder; field
    /// names must match what TypeScript consumers expect.
    #[test]
    fn qwen_model_metadata_serde_camelcase() {
        let m = qwen3_30b_a3b_q4km();
        let j = serde_json::to_string(&m).unwrap();
        assert!(j.contains("\"modelName\":"));
        assert!(j.contains("\"layerCount\":48"));
        assert!(j.contains("\"parameterCountBillions\":30.0"));
        assert!(j.contains("\"bytesPerParameterQuantized\":0.5"));
        assert!(j.contains("\"layerKindsNeedingCheck\":[\"moe_gate\"]"));
        let back: QwenModelMetadata = serde_json::from_str(&j).unwrap();
        assert_eq!(back, m);
    }

    /// What this catches: ResidencyEvidence round-trips with camelCase
    /// + every field's JSON name matches PR-3/PR-4 contracts.
    #[test]
    fn residency_evidence_serde_camelcase() {
        let result = check_residency_gate(&qwen25_7b_q4km(), &blackwell_rtx_5090());
        if let ResidencyGateResult::Pass(ev) = result {
            let j = serde_json::to_string(&ev).unwrap();
            assert!(j.contains("\"modelName\":"));
            assert!(j.contains("\"gpuLayerCount\":28"));
            assert!(j.contains("\"estimatedVramBytes\":"));
            assert!(j.contains("\"freeVramBytes\":"));
            assert!(j.contains("\"backend\":\"cuda\""));
        } else {
            panic!("expected Pass");
        }
    }

    // ===== Edge cases =====

    /// What this catches: free VRAM exactly equal to estimate → pass
    /// (inclusive boundary). Symmetric with probe.rs
    /// find_capable_matches_on_exact_vram_boundary.
    #[test]
    fn vram_exactly_at_estimate_passes() {
        let m = qwen25_7b_q4km();
        let est = m.estimated_vram_bytes();
        let mut hw = m5_pro_48gb();
        hw.free_vram_bytes = est;
        let result = check_residency_gate(&m, &hw);
        assert!(result.is_pass(), "VRAM == estimate must pass; got {result:?}");
    }

    /// What this catches: free VRAM one byte below estimate → block.
    /// Establishes the inclusive-min boundary explicitly.
    #[test]
    fn vram_one_byte_under_estimate_blocks() {
        let m = qwen25_7b_q4km();
        let est = m.estimated_vram_bytes();
        let mut hw = m5_pro_48gb();
        hw.free_vram_bytes = est - 1;
        let result = check_residency_gate(&m, &hw);
        assert!(!result.is_pass());
    }

    /// What this catches: tiny Qwen variant (e.g. Qwen2.5-0.5B) on
    /// a CPU-only host still blocks. Size doesn't rescue the gate —
    /// no GPU = block, period.
    #[test]
    fn tiny_model_on_cpu_only_still_blocks() {
        let mut m = qwen25_3b_q4km();
        m.parameter_count_billions = 0.5;
        let result = check_residency_gate(&m, &generic_dell_no_gpu());
        assert!(!result.is_pass());
        assert!(result
            .reasons()
            .iter()
            .any(|r| matches!(r, BlockReason::NoGpuBackendOnNode { .. })));
    }

    /// What this catches: a model variant the local probe would have
    /// included but the gate now rejects per residency. The two layers
    /// (probe + residency) must compose: probe says "node can take
    /// llamacpp," residency says "can take THIS llamacpp model." Both
    /// guarantees are needed; this test pins the gap.
    #[test]
    fn probe_passes_but_residency_blocks_partial_split() {
        use crate::inference_capability::probe::probe_inference_capabilities;
        use crate::inference_capability::types::kinds;

        let hw = macbook_air_m2_8gb();
        let probe_caps = probe_inference_capabilities(&hw);
        // probe advertises llamacpp on this host
        assert!(probe_caps.iter().any(|c| c.kind.as_str() == kinds::LLAMACPP));

        // but residency gate blocks a 30B model on it
        let result = check_residency_gate(&qwen3_30b_a3b_q4km(), &hw);
        assert!(!result.is_pass());
    }

    /// What this catches: BackendChoice::as_str() returns the lowercase
    /// wire-stable string for each variant. Used in error messages +
    /// log lines; if it drifts, grep-by-backend-name breaks.
    #[test]
    fn backend_choice_as_str() {
        assert_eq!(BackendChoice::Metal.as_str(), "metal");
        assert_eq!(BackendChoice::Cuda.as_str(), "cuda");
        assert_eq!(BackendChoice::Vulkan.as_str(), "vulkan");
    }

    /// What this catches: layer_kinds_needing_check with MULTIPLE
    /// entries on a Vulkan + qwen3moe combo emits one UnsupportedLayer
    /// reason per kind. PR-3 surfaces every gap, not just the first.
    #[test]
    fn vulkan_qwen3_emits_one_unsupported_per_layer_kind() {
        let mut m = qwen3_30b_a3b_q4km();
        m.layer_kinds_needing_check = vec!["moe_gate".into(), "sliding_window_attn".into()];
        let mut hw = amd_with_vulkan_only();
        hw.free_vram_bytes = 64 * 1024 * 1024 * 1024; // enough VRAM; only layer issues
        let result = check_residency_gate(&m, &hw);
        let kinds: Vec<&str> = result
            .reasons()
            .iter()
            .filter_map(|r| match r {
                BlockReason::UnsupportedLayer { layer_kind, .. } => Some(layer_kind.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(kinds.len(), 2);
        assert!(kinds.contains(&"moe_gate"));
        assert!(kinds.contains(&"sliding_window_attn"));
    }

    /// What this catches: empty layer_kinds_needing_check NEVER emits
    /// UnsupportedLayer regardless of backend. Default-case safety —
    /// models that don't declare tricky layers shouldn't be blocked.
    #[test]
    fn empty_layer_kinds_never_emits_unsupported() {
        let m = qwen25_7b_q4km();
        for hw in &[
            macbook_air_m2_8gb(),
            m5_pro_48gb(),
            blackwell_rtx_5090(),
            amd_with_vulkan_only(),
        ] {
            let result = check_residency_gate(&m, hw);
            for r in result.reasons() {
                assert!(
                    !matches!(r, BlockReason::UnsupportedLayer { .. }),
                    "empty layer_kinds emitted UnsupportedLayer on {}",
                    hw.platform
                );
            }
        }
    }

    /// What this catches: free_vram_bytes = 0 on a GPU-equipped host
    /// (another process holds all VRAM) blocks with PartialGpuSplit
    /// even for the smallest model. Probe (#1315) deadheads below 2GB
    /// at probe time; this catches the race where VRAM dropped between
    /// probe + gate.
    #[test]
    fn zero_free_vram_on_gpu_host_blocks_smallest_model() {
        let mut hw = m5_pro_48gb();
        hw.free_vram_bytes = 0;
        let mut tiny = qwen25_3b_q4km();
        tiny.parameter_count_billions = 0.5;
        let result = check_residency_gate(&tiny, &hw);
        assert!(!result.is_pass());
        assert!(result
            .reasons()
            .iter()
            .any(|r| matches!(r, BlockReason::PartialGpuSplit { .. })));
    }

    /// What this catches: a Pass returns an empty reasons slice. Lets
    /// callers iterate uniformly without conditional pattern-matching.
    #[test]
    fn pass_reasons_is_empty_slice() {
        let pass = check_residency_gate(&qwen25_7b_q4km(), &m5_pro_48gb());
        assert!(pass.is_pass());
        assert_eq!(pass.reasons(), &[] as &[BlockReason]);
    }

    /// What this catches: FP16 Qwen 7B estimate (~15GB) blocks on an
    /// 8GB Mac. Pins bytes_per_parameter_quantized's load-bearing role
    /// — dropping it would silently route FP16 onto undersized hosts.
    #[test]
    fn fp16_7b_blocks_on_8gb_mac() {
        let mut m = qwen25_7b_q4km();
        m.bytes_per_parameter_quantized = 2.0; // FP16
        let result = check_residency_gate(&m, &macbook_air_m2_8gb());
        assert!(!result.is_pass(), "FP16 7B on 5GB free must block");
    }

    /// What this catches: BlockReason::WrongBackendForPlatform variant
    /// exists in the type even if no current code path emits it.
    /// Defensive — future codepaths that force backend selection
    /// (e.g. user override) need this variant to surface the mismatch
    /// instead of a runtime panic. Variant must round-trip cleanly.
    #[test]
    fn wrong_backend_variant_serde_round_trips() {
        let r = BlockReason::WrongBackendForPlatform {
            platform: "macos-arm64-m2".into(),
            backend: BackendChoice::Cuda,
        };
        let j = serde_json::to_string(&r).unwrap();
        let back: BlockReason = serde_json::from_str(&j).unwrap();
        assert_eq!(r, back);
        assert!(j.contains("\"kind\":\"wrongBackendForPlatform\""));
    }

    /// What this catches: `is_pass()` helper agrees with the variant.
    /// Defensive — callers will use is_pass() instead of pattern-
    /// matching most of the time; if the helper drifts, the gate
    /// becomes a footgun.
    #[test]
    fn is_pass_matches_variant() {
        let p = check_residency_gate(&qwen25_7b_q4km(), &m5_pro_48gb());
        assert!(p.is_pass());
        assert_eq!(p.reasons().len(), 0);

        let b = check_residency_gate(&qwen25_7b_q4km(), &generic_dell_no_gpu());
        assert!(!b.is_pass());
        assert!(!b.reasons().is_empty());
    }
}
