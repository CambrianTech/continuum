//! Pure probe: HardwareProfile → Vec<InferenceCapability>.
//!
//! Given a node's hardware profile, decides what inference backends are
//! viable on this node and reports free VRAM + zero current leases (the
//! supervisor's lease counter feeds the live update separately).
//!
//! This is the *derivation* layer — no global state, no IO, no syscalls.
//! Tests pass synthetic profiles for the four hardware tiers vhsm-d1f4
//! named (MacBook Air, M5 Pro, Blackwell, generic Dell with no GPU) and
//! assert the right capabilities surface.
//!
//! At runtime the supervisor calls `probe_hardware_profile()` (from a
//! later PR-2 wiring; not in this PR) to fill the `HardwareProfile` from
//! `sysinfo` + GpuMemoryManager + Metal/CUDA probes, then calls
//! `probe_inference_capabilities()` here to derive the capability list.

use crate::inference_capability::types::{
    kinds, HardwareProfile, InferenceCapability, InferenceKind, LatencyClass,
};

/// Minimum free VRAM (bytes) below which the node should NOT advertise a
/// GPU-resident inference backend. A 7B Q4_K_M model needs ~4GB; smaller
/// embedding/vision models need ~1GB. We pick 2GB as a conservative floor:
/// anything less and we'd be telling the router we can take a job when in
/// practice the load would fail. Better to deadhead the node than to fail
/// mid-inference.
const MIN_GPU_INFERENCE_VRAM_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// Derive the list of inference capabilities this node can take.
///
/// Pure function — no IO, no globals. Identical input → identical output.
/// The supervisor calls this at boot + on hardware-change events; the
/// result feeds PR-2's GridCapabilityAnnouncer.
///
/// Decisions encoded here:
/// - **llamacpp**: GPU-required (Metal or CUDA). No CPU advertisement —
///   per CLAUDE.md off-main-thread rule + the no-CPU-fallback audit
///   (vhsm-d1f4 2026-05-16). A CUDA host on Linux advertises llamacpp;
///   a Metal host on macOS advertises llamacpp; a CPU-only host doesn't.
/// - **candle**: same GPU-required policy as llamacpp.
/// - **ort-vision / ort-tts / ort-stt / ort-embedding**: GPU-required via
///   the ORT GPU execution providers (centralized in
///   `crate::inference::ort_providers`). The host needs some GPU to
///   advertise these; the specific kind (Vulkan, CUDA, Metal-via-CoreML)
///   is resolved at lease time by the EP selector.
///
/// Vulkan is treated as "has a GPU usable for ORT but not for the
/// llama.cpp/candle native paths today" — those are gated on Metal or
/// CUDA specifically. As llama.cpp/candle gain Vulkan backends, lift
/// the kind gate (no code change needed elsewhere — registry of kinds
/// is dynamic).
pub fn probe_inference_capabilities(
    hw: &HardwareProfile,
) -> Vec<InferenceCapability> {
    let mut caps: Vec<InferenceCapability> = Vec::new();

    let has_native_gpu = hw.has_metal || hw.has_cuda;
    let has_enough_vram = hw.free_vram_bytes >= MIN_GPU_INFERENCE_VRAM_BYTES;
    let has_ort_gpu = hw.has_metal || hw.has_cuda || hw.has_vulkan;

    // llamacpp + candle: native GPU (Metal or CUDA) with adequate VRAM.
    if has_native_gpu && has_enough_vram {
        caps.push(InferenceCapability {
            kind: InferenceKind::from(kinds::LLAMACPP),
            free_vram_bytes: hw.free_vram_bytes,
            current_lease_count: 0,
            latency_class: LatencyClass::Local,
        });
        caps.push(InferenceCapability {
            kind: InferenceKind::from(kinds::CANDLE),
            free_vram_bytes: hw.free_vram_bytes,
            current_lease_count: 0,
            latency_class: LatencyClass::Local,
        });
    }

    // ORT-backed kinds: vision / tts / stt / embedding. Any GPU EP works.
    if has_ort_gpu && has_enough_vram {
        for kind_name in &[
            kinds::ORT_VISION,
            kinds::ORT_TTS,
            kinds::ORT_STT,
            kinds::ORT_EMBEDDING,
        ] {
            caps.push(InferenceCapability {
                kind: InferenceKind::from(*kind_name),
                free_vram_bytes: hw.free_vram_bytes,
                current_lease_count: 0,
                latency_class: LatencyClass::Local,
            });
        }
    }

    caps
}

#[cfg(test)]
mod tests {
    use super::*;

    fn macbook_air_m2_8gb() -> HardwareProfile {
        HardwareProfile {
            platform: "macos-arm64-m2".into(),
            has_metal: true,
            has_cuda: false,
            has_vulkan: false,
            // M2 8GB has ~5GB available to the GPU after OS reservation.
            free_vram_bytes: 5 * 1024 * 1024 * 1024,
            total_vram_bytes: 8 * 1024 * 1024 * 1024,
            cpu_cores: 8,
            system_ram_bytes: 8 * 1024 * 1024 * 1024,
        }
    }

    fn macbook_air_m2_below_floor() -> HardwareProfile {
        let mut hw = macbook_air_m2_8gb();
        // Heavy other-workload — only 1GB free; below MIN_GPU_INFERENCE_VRAM_BYTES.
        hw.free_vram_bytes = 1 * 1024 * 1024 * 1024;
        hw
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
            has_vulkan: true, // NVIDIA cards usually expose Vulkan too
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

    fn amd_with_vulkan_no_native_gpu() -> HardwareProfile {
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

    fn kinds_of(caps: &[InferenceCapability]) -> Vec<String> {
        let mut ks: Vec<String> = caps.iter().map(|c| c.kind.as_str().to_string()).collect();
        ks.sort();
        ks
    }

    /// What this catches: MacBook Air with 5GB free VRAM (above the 2GB
    /// floor) advertises llamacpp + candle + all 4 ORT-backed kinds via
    /// Metal. The lowest-end Mac vhsm-d1f4 named in the tier list — if
    /// this fails, the M2 fleet is silently excluded from the grid.
    #[test]
    fn macbook_air_m2_advertises_full_gpu_kit() {
        let caps = probe_inference_capabilities(&macbook_air_m2_8gb());
        assert_eq!(
            kinds_of(&caps),
            vec![
                "candle".to_string(),
                "llamacpp".into(),
                "ort-embedding".into(),
                "ort-stt".into(),
                "ort-tts".into(),
                "ort-vision".into(),
            ],
        );
        assert!(caps
            .iter()
            .all(|c| c.latency_class == LatencyClass::Local));
        assert!(caps.iter().all(|c| c.current_lease_count == 0));
        assert!(caps.iter().all(|c| c.free_vram_bytes == 5 * 1024 * 1024 * 1024));
    }

    /// What this catches: M5 Pro with 32GB free VRAM advertises every kind
    /// at full capacity. The flagship Mac tier vhsm-d1f4 named.
    #[test]
    fn m5_pro_advertises_full_gpu_kit_at_higher_vram() {
        let caps = probe_inference_capabilities(&m5_pro_48gb());
        assert_eq!(caps.len(), 6, "llamacpp+candle+4 ort kinds");
        assert!(caps
            .iter()
            .all(|c| c.free_vram_bytes == 32 * 1024 * 1024 * 1024));
    }

    /// What this catches: Blackwell (CUDA + Vulkan) advertises the same
    /// 6-kind kit. CUDA satisfies has_native_gpu; the kinds list is
    /// platform-agnostic so the router can pick between Mac/Blackwell on
    /// scoring without special-casing the kind set.
    #[test]
    fn blackwell_advertises_full_gpu_kit_via_cuda() {
        let caps = probe_inference_capabilities(&blackwell_rtx_5090());
        assert_eq!(kinds_of(&caps).len(), 6);
        assert!(
            caps.iter().any(|c| c.kind.as_str() == kinds::LLAMACPP),
            "llamacpp via CUDA"
        );
        assert!(
            caps.iter().any(|c| c.kind.as_str() == kinds::CANDLE),
            "candle via CUDA"
        );
    }

    /// What this catches: generic Dell with NO GPU advertises ZERO
    /// capabilities. The no-CPU-fallback contract at the capability layer:
    /// CPU-only nodes don't pretend to be inference nodes. Per
    /// vhsm-d1f4: "the supervisor offers a GPU lease or it doesn't;
    /// modules don't have a CPU branch to fall back into."
    #[test]
    fn generic_dell_no_gpu_advertises_nothing() {
        let caps = probe_inference_capabilities(&generic_dell_no_gpu());
        assert!(
            caps.is_empty(),
            "CPU-only host must not advertise inference; got: {:?}",
            kinds_of(&caps),
        );
    }

    /// What this catches: a host with Vulkan but no Metal/CUDA advertises
    /// the 4 ORT-backed kinds (vision/tts/stt/embedding) but NOT
    /// llamacpp/candle. ORT supports Vulkan via DirectML/etc; the native
    /// llama.cpp/candle paths don't have Vulkan kernels in the version
    /// we ship today. Documented so AMD/RDNA fleet onboarding doesn't
    /// silently lose the LLM workload class — it's a known gap pending
    /// candle-vulkan / llama.cpp-vulkan support.
    #[test]
    fn amd_vulkan_only_advertises_ort_kinds_not_native_gpu() {
        let caps = probe_inference_capabilities(&amd_with_vulkan_no_native_gpu());
        let ks = kinds_of(&caps);
        assert_eq!(ks.len(), 4, "4 ort kinds only");
        assert!(ks.contains(&"ort-vision".to_string()));
        assert!(ks.contains(&"ort-tts".to_string()));
        assert!(ks.contains(&"ort-stt".to_string()));
        assert!(ks.contains(&"ort-embedding".to_string()));
        assert!(
            !ks.contains(&"llamacpp".to_string()),
            "llama.cpp Vulkan not supported in current vendored build",
        );
        assert!(
            !ks.contains(&"candle".to_string()),
            "candle Vulkan not supported in current build",
        );
    }

    /// What this catches: GPU-equipped host with VRAM BELOW the 2GB floor
    /// (e.g. another workload is hogging memory) advertises NOTHING. The
    /// router seeing "0 capabilities" rather than "yes can take a job but
    /// will fail" is the difference between failing fast and failing
    /// mid-inference. Tests the deadhead-don't-fail policy.
    #[test]
    fn gpu_below_vram_floor_advertises_nothing() {
        let caps = probe_inference_capabilities(&macbook_air_m2_below_floor());
        assert!(
            caps.is_empty(),
            "below 2GB free VRAM = deadhead, not advertise; got: {:?}",
            kinds_of(&caps),
        );
    }

    /// What this catches: every capability's `current_lease_count` starts
    /// at 0. The supervisor's lease counter (live, separate from this
    /// pure derivation) updates the running value; this is the
    /// fresh-probe baseline. PR-2's announcer reads this then overlays
    /// live lease state.
    #[test]
    fn fresh_probe_reports_zero_leases() {
        for hw in &[macbook_air_m2_8gb(), m5_pro_48gb(), blackwell_rtx_5090()] {
            let caps = probe_inference_capabilities(hw);
            assert!(!caps.is_empty(), "{} should have caps", hw.platform);
            assert!(
                caps.iter().all(|c| c.current_lease_count == 0),
                "fresh probe must report 0 leases ({})",
                hw.platform,
            );
        }
    }

    /// What this catches: every capability's `latency_class` is `Local`.
    /// The probe is for THIS node; PR-3's router synthesizes other
    /// latency classes (Fast/Mesh/Wan) for remote nodes from grid
    /// transport's live RTT measurements.
    #[test]
    fn local_probe_always_reports_local_latency() {
        for hw in &[macbook_air_m2_8gb(), m5_pro_48gb(), blackwell_rtx_5090()] {
            let caps = probe_inference_capabilities(hw);
            assert!(
                caps.iter().all(|c| c.latency_class == LatencyClass::Local),
                "local probe must always report Local latency_class ({})",
                hw.platform,
            );
        }
    }

    /// What this catches: same hardware profile in, same capabilities out.
    /// Pure-function contract — no globals, no IO, no syscalls. PR-2 can
    /// cache the result across announcements without worrying about
    /// drift between calls with identical input.
    #[test]
    fn probe_is_deterministic_for_same_input() {
        let hw = m5_pro_48gb();
        let a = probe_inference_capabilities(&hw);
        let b = probe_inference_capabilities(&hw);
        assert_eq!(a, b);
    }

    /// What this catches: free_vram_bytes from the hardware profile
    /// flows through to every capability advertised. PR-3's router scores
    /// nodes partly on this field; if it diverged from the profile, the
    /// router would over- or under-commit.
    #[test]
    fn free_vram_propagates_to_every_capability() {
        let mut hw = blackwell_rtx_5090();
        hw.free_vram_bytes = 12_345_678_900;
        let caps = probe_inference_capabilities(&hw);
        assert!(!caps.is_empty());
        assert!(caps.iter().all(|c| c.free_vram_bytes == 12_345_678_900));
    }

    /// What this catches: a Vulkan-equipped host with VRAM BELOW the
    /// 2GB floor advertises ZERO capabilities, even though `has_vulkan`
    /// would otherwise unlock the ORT-backed kinds. The floor applies
    /// to ALL GPU paths, not just Metal/CUDA — symmetric guarantee
    /// across hardware classes.
    #[test]
    fn vulkan_below_floor_vram_advertises_nothing() {
        let mut hw = amd_with_vulkan_no_native_gpu();
        hw.free_vram_bytes = 1024 * 1024 * 1024; // 1GB, below 2GB floor.
        let caps = probe_inference_capabilities(&hw);
        assert!(
            caps.is_empty(),
            "Vulkan below floor must NOT advertise; got: {:?}",
            kinds_of(&caps),
        );
    }

    /// What this catches: a CPU-only host with non-trivial system_ram
    /// still advertises zero capabilities. system_ram is irrelevant to
    /// the no-CPU-fallback contract; only GPU presence + VRAM gate
    /// advertisement. Pins the boundary explicitly so a future "use
    /// system RAM as a fallback" optimization can't sneak past tests.
    #[test]
    fn cpu_only_host_with_huge_ram_still_advertises_nothing() {
        let mut hw = generic_dell_no_gpu();
        hw.system_ram_bytes = 512 * 1024 * 1024 * 1024; // 512GB RAM, no GPU.
        let caps = probe_inference_capabilities(&hw);
        assert!(
            caps.is_empty(),
            "system_ram is not a GPU substitute; got: {:?}",
            kinds_of(&caps),
        );
    }

    /// What this catches: every capability on a Blackwell + Vulkan host
    /// reports the same free_vram_bytes (the hardware profile's value)
    /// across BOTH the native-GPU kinds AND the ORT-GPU kinds. The two
    /// branches in `probe_inference_capabilities` must agree on the
    /// VRAM-source-of-truth — if they ever diverge (e.g. one reads
    /// total instead of free), PR-3's router gets inconsistent scoring.
    #[test]
    fn both_native_and_ort_branches_report_same_free_vram() {
        let hw = blackwell_rtx_5090();
        let caps = probe_inference_capabilities(&hw);
        let unique_vram: std::collections::HashSet<u64> =
            caps.iter().map(|c| c.free_vram_bytes).collect();
        assert_eq!(
            unique_vram.len(),
            1,
            "all caps must report same free VRAM; got: {unique_vram:?}",
        );
        assert_eq!(unique_vram.into_iter().next().unwrap(), hw.free_vram_bytes);
    }

    /// What this catches: capability ordering is deterministic
    /// (llamacpp, candle, ort-* in declared order). PR-2's announcer can
    /// hash-compare announcements without sorting first; PR-3's router
    /// produces stable scoring outputs given stable inputs.
    #[test]
    fn capability_ordering_is_deterministic() {
        let caps = probe_inference_capabilities(&m5_pro_48gb());
        let kinds: Vec<&str> = caps.iter().map(|c| c.kind.as_str()).collect();
        assert_eq!(
            kinds,
            vec!["llamacpp", "candle", "ort-vision", "ort-tts", "ort-stt", "ort-embedding"],
            "ordering shifted — PR-2/PR-3 may have implicit assumptions; pin it explicitly",
        );
    }
}
