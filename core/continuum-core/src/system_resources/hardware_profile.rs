//! The machine's hardware, as one fact: backends present, VRAM, cores, RAM.
//!
//! A ring-0 fact (the machine), read by the governor and the engine alike. It lived in
//! `inference_capability` (ring 1), which held the governor in the core's dependency cycle;
//! the probe that fills it stays with the engine (`inference_capability::hw_probe`).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Hardware profile a node's supervisor probes at boot + on hardware-change
/// events. Carried in `probe_inference_capabilities` to derive the
/// capability list. Pure data — the runtime probe writes this; tests
/// synthesize it for the four hardware tiers vhsm-d1f4 named.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/inference_capability/HardwareProfile.ts"
)]
pub struct HardwareProfile {
    /// Human-readable platform identifier ("macos-arm64", "linux-x86_64-cuda",
    /// "macos-arm64-m5pro", "linux-x86_64-blackwell"). Free-form; the
    /// supervisor probe sets this from sysinfo + GPU vendor strings.
    pub platform: String,
    /// Metal device available (any Apple Silicon).
    pub has_metal: bool,
    /// CUDA device available (NVIDIA).
    pub has_cuda: bool,
    /// Vulkan device available (AMD or non-CUDA NVIDIA on Linux/Windows).
    pub has_vulkan: bool,
    /// Free VRAM in bytes. 0 when no discrete/unified GPU memory. Sourced
    /// from the GPU memory manager's live probe (`GpuMemoryManager::stats`).
    #[ts(type = "number")]
    pub free_vram_bytes: u64,
    /// Total VRAM in bytes (for capacity scoring). 0 when not applicable.
    #[ts(type = "number")]
    pub total_vram_bytes: u64,
    /// CPU core count. Set even on GPU-equipped nodes; PR-3 uses it as a
    /// tiebreaker when GPU capacity is similar.
    #[ts(type = "number")]
    pub cpu_cores: u32,
    /// System RAM in bytes (the resource pool the broker meters for
    /// non-GPU work — embeddings, vision pre/postproc, TTS spectrogram).
    #[ts(type = "number")]
    pub system_ram_bytes: u64,
}

impl HardwareProfile {
    /// True when this node has ANY GPU backend (Metal / CUDA / Vulkan).
    ///
    /// This is the eligibility predicate for the cross-node GPU-first
    /// guardrail (Joel's "GPU-first ALWAYS", promoted from per-node to
    /// ACROSS-node; no_cpu_fallback contract, offload path): a CPU-only
    /// node (all three false) must NEVER be an eligible offload target for
    /// a heavy inference job — the substrate refuses rather than silently
    /// degrade a remote peer to CPU. Mirrors the per-node residency-gate
    /// GPU refusal at the cross-node boundary.
    pub fn has_gpu(&self) -> bool {
        self.has_metal || self.has_cuda || self.has_vulkan
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What this catches: HardwareProfile round-trips with camelCase wire
    /// names. PR-2's grid serialization depends on field-name stability.
    #[test]
    fn hardware_profile_serde_camelcase() {
        let h = HardwareProfile {
            platform: "macos-arm64-m5pro".into(),
            has_metal: true,
            has_cuda: false,
            has_vulkan: false,
            free_vram_bytes: 32_000_000_000,
            total_vram_bytes: 48_000_000_000,
            cpu_cores: 16,
            system_ram_bytes: 64_000_000_000,
        };
        let j = serde_json::to_string(&h).expect("a HardwareProfile always (de)serializes");
        assert!(j.contains("\"hasMetal\":true"));
        assert!(j.contains("\"freeVramBytes\":32000000000"));
        assert!(j.contains("\"systemRamBytes\":64000000000"));
        let back: HardwareProfile = serde_json::from_str(&j).expect("a HardwareProfile always (de)serializes");
        assert_eq!(back, h);
    }

    /// What this catches: HardwareProfile::has_gpu() is true for ANY GPU
    /// backend (Metal OR CUDA OR Vulkan) and false only when all three are
    /// absent. This predicate is the cross-node GPU-first guardrail's
    /// eligibility test (no_cpu_fallback offload path) — a regression that
    /// checked only has_metal would mis-classify CUDA/Vulkan GPU peers as
    /// ineligible, or a flipped condition would let CPU-only peers through.
    #[test]
    fn has_gpu_true_for_any_backend_false_for_cpu_only() {
        let cpu_only = HardwareProfile {
            platform: "cpu-box".into(),
            has_metal: false,
            has_cuda: false,
            has_vulkan: false,
            free_vram_bytes: 0,
            total_vram_bytes: 0,
            cpu_cores: 8,
            system_ram_bytes: 16_000_000_000,
        };
        assert!(!cpu_only.has_gpu(), "all-false = CPU-only = no GPU");
        assert!(HardwareProfile {
            has_metal: true,
            ..cpu_only.clone()
        }
        .has_gpu());
        assert!(HardwareProfile {
            has_cuda: true,
            ..cpu_only.clone()
        }
        .has_gpu());
        assert!(HardwareProfile {
            has_vulkan: true,
            ..cpu_only.clone()
        }
        .has_gpu());
    }
}
