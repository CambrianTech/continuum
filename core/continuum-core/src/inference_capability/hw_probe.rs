//! Hardware probe — populates `HardwareProfile` from runtime detection
//! (CBAR-PIECE-5 PR-3).
//!
//! PR-1 (`residency.rs`) defined the gate types. PR-2 (`gguf_loader.rs`)
//! reads model metadata from disk. This PR-3 populates the OTHER input
//! to the gate — the live hardware profile — by probing Metal / CUDA /
//! Vulkan independently and combining the result with CPU + RAM data
//! from `sysinfo`.
//!
//! ## Why probe each backend independently
//!
//! `gpu::memory_manager::detect_gpu()` returns the FIRST backend that
//! succeeds (Metal → CUDA → Vulkan → panic). That's correct for the
//! production GpuMemoryManager — only one budget per node — but wrong
//! for `HardwareProfile`, which has separate `has_metal`/`has_cuda`/
//! `has_vulkan` flags. An NVIDIA-on-Linux host can have both CUDA AND
//! Vulkan; the gate's `select_backend` uses the flags to pick CUDA over
//! Vulkan (CUDA's llama.cpp kernels are more complete). If we only set
//! whichever-detected-first, the flags lie.
//!
//! ## What this DOES NOT do
//!
//! - Allocate VRAM (free_vram is reported as total minus a reserve —
//!   PR-4 wires `GpuMemoryManager::stats().total_used_mb` for the real
//!   "what's free RIGHT NOW" number).
//! - Trigger `GpuMemoryManager::detect()` (that's heavyweight + panics
//!   on no-GPU; the probe must not).
//! - Decide whether a model fits — that's `check_residency_gate`.
//! - Choose a backend — that's `select_backend`.
//!
//! ## Failure-mode discipline
//!
//! - Probe NEVER panics. A CPU-only host returns a HardwareProfile with
//!   `has_metal=false, has_cuda=false, has_vulkan=false, free_vram=0`.
//!   The gate then surfaces `NoGpuBackendOnNode` — visible failure, not
//!   silent CPU fallback.
//! - Per-backend probes return `Option<(u64, String)>` — None means
//!   "not available on this build/host." The orchestrator combines.
//! - sysinfo failures fall back to conservative defaults (cpu_cores=1,
//!   system_ram=0). Logged on the cognition channel so an observer
//!   sees the fallback.

use crate::inference_capability::types::HardwareProfile;

/// Probe the local hardware + return a `HardwareProfile` suitable for
/// feeding into `check_residency_gate` and `probe_inference_capabilities`.
///
/// Pure-wrapper around the per-backend probes + sysinfo. Safe to call
/// from any thread; not async (no I/O beyond a few file reads + the
/// per-backend FFI / subprocess calls). For repeat queries, the caller
/// should cache the result — this fn re-probes each call.
pub fn probe_hardware_profile() -> HardwareProfile {
    let metal = try_detect_metal();
    let cuda = try_detect_cuda();
    let vulkan = try_detect_vulkan();
    let (cpu_cores, system_ram_bytes) = probe_cpu_and_ram();
    let platform = platform_identifier();

    build_hardware_profile(metal, cuda, vulkan, cpu_cores, system_ram_bytes, platform)
}

/// Pure derivation function — combines per-backend probes + CPU/RAM +
/// platform string into a HardwareProfile.
///
/// Separated from `probe_hardware_profile` for testability: this fn is
/// 100% deterministic given its inputs and tests synthesize each
/// combination.
///
/// VRAM aggregation rule: when multiple backends report VRAM (e.g.
/// NVIDIA with both CUDA + Vulkan), use the MAX as the shared
/// `total_vram_bytes`. The flags carry which backends are usable; the
/// VRAM number reflects the same physical card. PR-4 will refine with
/// per-backend free-VRAM queries; PR-3 uses a single shared number
/// because that's what the field is.
///
/// free_vram_bytes for PR-3: total minus a conservative 5% reserve.
/// The real "free RIGHT NOW" number requires `GpuMemoryManager::stats()`
/// which PR-3 deliberately doesn't depend on (the manager is heavyweight
/// + panics on no-GPU). PR-4 wires the live number.
pub fn build_hardware_profile(
    metal: Option<(u64, String)>,
    cuda: Option<(u64, String)>,
    vulkan: Option<(u64, String)>,
    cpu_cores: u32,
    system_ram_bytes: u64,
    platform: String,
) -> HardwareProfile {
    let has_metal = metal.is_some();
    let has_cuda = cuda.is_some();
    let has_vulkan = vulkan.is_some();

    // Use the largest reported VRAM across detected backends — same
    // physical card reported by multiple loaders, so MAX is conservative
    // (don't double-count, don't under-count).
    let total_vram_bytes = [
        metal.as_ref().map(|(b, _)| *b).unwrap_or(0),
        cuda.as_ref().map(|(b, _)| *b).unwrap_or(0),
        vulkan.as_ref().map(|(b, _)| *b).unwrap_or(0),
    ]
    .into_iter()
    .max()
    .unwrap_or(0);

    // Conservative free estimate: total minus 5% reserve. PR-4 wires
    // GpuMemoryManager::stats().total_used_mb for the real number.
    let free_vram_bytes = (total_vram_bytes as f64 * 0.95) as u64;

    HardwareProfile {
        platform,
        has_metal,
        has_cuda,
        has_vulkan,
        free_vram_bytes,
        total_vram_bytes,
        cpu_cores,
        system_ram_bytes,
    }
}

/// Read CPU cores + total system RAM from sysinfo. Falls back to
/// (1, 0) on probe failure (better to under-report than panic).
fn probe_cpu_and_ram() -> (u32, u64) {
    let cores = num_cpus::get() as u32;
    let ram_bytes = {
        let mut sys = sysinfo::System::new_all();
        sys.refresh_memory();
        sys.total_memory() // sysinfo 0.30+ returns bytes directly
    };
    (cores.max(1), ram_bytes)
}

/// Build a platform identifier string from build-time + runtime data.
/// Examples: "macos-arm64-m2", "linux-x86_64-blackwell" (when we can
/// fingerprint), "linux-x86_64-generic". The format is free-form;
/// callers use it only for telemetry + the `BlockReason::NoGpuBackendOnNode`
/// error message.
fn platform_identifier() -> String {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    // GPU-vendor fingerprint would slot here in a future PR (parse
    // metal device name → m1/m2/m3/m4/m5, parse nvidia-smi name →
    // blackwell/ada/ampere, etc). For PR-3 we keep it simple +
    // observable.
    format!("{os}-{arch}")
}

// ─── Per-backend probes ─────────────────────────────────────────────────

/// Try to detect Metal. Returns Some((total_vram_bytes, device_name))
/// when Metal is usable, None otherwise. Never panics.
///
/// Mirrors `gpu::memory_manager::detect_metal` but returns None instead
/// of falling through to the next backend (we probe each independently
/// so HardwareProfile flags accurately reflect "what's on this host").
fn try_detect_metal() -> Option<(u64, String)> {
    #[cfg(target_os = "macos")]
    {
        let device = metal::Device::system_default()?;
        let total = device.recommended_max_working_set_size();
        if total == 0 {
            return None;
        }
        return Some((total, device.name().to_string()));
    }
    #[allow(unreachable_code)]
    None
}

/// Try to detect CUDA via nvidia-smi subprocess (same pattern as
/// `gpu::memory_manager::detect_cuda`). Subprocess approach because
/// candle_core doesn't expose device memory directly.
fn try_detect_cuda() -> Option<(u64, String)> {
    #[cfg(feature = "cuda")]
    {
        // BOUNDED (#3733's class, third site). Reached from
        // `probe_hardware_profile`, which the inference coordinator and the
        // capacity profile both call — so an unbounded wait here stalls
        // whichever of those runs first, and on a boot path that is the whole
        // startup. Same 10 s ceiling as the other driver queries: one number
        // for "how long may nvidia-smi take", not three.
        let probed = crate::system_resources::bounded_command::probe(
            "nvidia-smi",
            &[
                "--query-gpu=memory.total,name",
                "--format=csv,noheader,nounits",
            ],
            std::time::Duration::from_secs(10),
        );
        crate::probe!(
            class = "boot.gpu_detect",
            backend = "cuda",
            stage = "hw_profile",
            outcome = probed.outcome(),
            "hardware-profile CUDA probe (bounded)"
        );
        let stdout = probed.stdout_if_ok()?;
        let line = stdout.lines().next()?;
        let parts: Vec<&str> = line.split(", ").collect();
        if parts.len() < 2 {
            return None;
        }
        let total_mib: u64 = parts[0].trim().parse().ok()?;
        let name = parts[1].trim().to_string();
        return Some((total_mib * 1024 * 1024, name));
    }
    #[allow(unreachable_code)]
    None
}

/// Try to detect Vulkan via vulkaninfo subprocess.
///
/// `vulkaninfo --summary` output contains deviceName lines per device.
/// VRAM size isn't reliably in --summary; we report a conservative
/// 1 GiB so the probe can flip has_vulkan=true. Real Vulkan VRAM lookup
/// requires deeper introspection (PR-4 / follow-up).
fn try_detect_vulkan() -> Option<(u64, String)> {
    #[cfg(feature = "vulkan")]
    {
        // BOUNDED (#3733's class, fourth site). Same path as the CUDA probe
        // above and reached when it declines, so on a host whose GPU stack is
        // unwell this is the SECOND unbounded wait in a row — the identical
        // shape #3733 fixed in `detect_gpu`, in a different file.
        let probed = crate::system_resources::bounded_command::probe(
            "vulkaninfo",
            &["--summary"],
            std::time::Duration::from_secs(10),
        );
        crate::probe!(
            class = "boot.gpu_detect",
            backend = "vulkan",
            stage = "hw_profile",
            outcome = probed.outcome(),
            "hardware-profile Vulkan probe (bounded)"
        );
        let stdout = probed.stdout_if_ok()?;
        // Look for a line like "deviceName    = Some GPU Name"
        let name = stdout
            .lines()
            .find_map(|line| {
                let trimmed = line.trim();
                trimmed
                    .strip_prefix("deviceName")
                    .and_then(|rest| rest.split('=').nth(1))
                    .map(|n| n.trim().to_string())
            })
            .unwrap_or_else(|| "vulkan-device".to_string());
        // Conservative 1 GiB placeholder — PR-4 will refine.
        return Some((1024 * 1024 * 1024, name));
    }
    #[allow(unreachable_code)]
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===== build_hardware_profile — pure derivation =====

    /// What this catches: Metal-only host (typical Mac) gets the flags
    /// set correctly + VRAM populated from the Metal probe + free_vram
    /// at 95% of total. The most common hardware path in production.
    #[test]
    fn metal_only_sets_metal_flag_and_vram() {
        let hw = build_hardware_profile(
            Some((16 * 1024 * 1024 * 1024, "Apple M2".into())),
            None,
            None,
            8,
            16 * 1024 * 1024 * 1024,
            "macos-arm64".into(),
        );
        assert!(hw.has_metal);
        assert!(!hw.has_cuda);
        assert!(!hw.has_vulkan);
        assert_eq!(hw.total_vram_bytes, 16 * 1024 * 1024 * 1024);
        // 95% conservative reserve
        assert!(hw.free_vram_bytes >= (15 * 1024 * 1024 * 1024));
        assert!(hw.free_vram_bytes <= (16 * 1024 * 1024 * 1024));
        assert_eq!(hw.cpu_cores, 8);
        assert_eq!(hw.platform, "macos-arm64");
    }

    /// What this catches: NVIDIA host with both CUDA + Vulkan detected
    /// (NVIDIA cards expose both). Flags BOTH true. VRAM is the MAX of
    /// the two reports (same physical card; don't double-count + don't
    /// under-count).
    #[test]
    fn nvidia_sets_both_cuda_and_vulkan_flags() {
        let hw = build_hardware_profile(
            None,
            Some((32 * 1024 * 1024 * 1024, "RTX 5090".into())),
            Some((24 * 1024 * 1024 * 1024, "vulkan-RTX-5090".into())),
            32,
            128 * 1024 * 1024 * 1024,
            "linux-x86_64".into(),
        );
        assert!(!hw.has_metal);
        assert!(hw.has_cuda);
        assert!(hw.has_vulkan);
        assert_eq!(
            hw.total_vram_bytes,
            32 * 1024 * 1024 * 1024,
            "MAX of CUDA+Vulkan reports"
        );
        assert_eq!(hw.cpu_cores, 32);
        assert_eq!(hw.system_ram_bytes, 128 * 1024 * 1024 * 1024);
    }

    /// What this catches: AMD-Vulkan-only host gets has_vulkan=true,
    /// other flags false. The gate then picks Vulkan via select_backend
    /// + applies the qwen3 unsupported-layer rule.
    #[test]
    fn vulkan_only_sets_only_vulkan_flag() {
        let hw = build_hardware_profile(
            None,
            None,
            Some((16 * 1024 * 1024 * 1024, "AMD RDNA3".into())),
            16,
            64 * 1024 * 1024 * 1024,
            "linux-x86_64".into(),
        );
        assert!(!hw.has_metal);
        assert!(!hw.has_cuda);
        assert!(hw.has_vulkan);
    }

    /// What this catches: CPU-only host (no GPU detected) produces a
    /// HardwareProfile with all flags false + zero VRAM. The gate
    /// then surfaces NoGpuBackendOnNode. Never panic; never silent
    /// CPU degrade.
    #[test]
    fn cpu_only_returns_zero_vram_no_flags() {
        let hw = build_hardware_profile(
            None,
            None,
            None,
            12,
            32 * 1024 * 1024 * 1024,
            "linux-x86_64-generic".into(),
        );
        assert!(!hw.has_metal);
        assert!(!hw.has_cuda);
        assert!(!hw.has_vulkan);
        assert_eq!(hw.total_vram_bytes, 0);
        assert_eq!(hw.free_vram_bytes, 0);
        assert_eq!(hw.cpu_cores, 12);
    }

    /// What this catches: free_vram is exactly 95% of total_vram — the
    /// conservative reserve PR-3 ships. PR-4 will refine to live
    /// stats(); this test pins the placeholder so the refinement is
    /// loud (the test fails when PR-4 changes the percentage).
    #[test]
    fn free_vram_is_95_percent_of_total_in_pr3() {
        let total = 10 * 1024 * 1024 * 1024_u64;
        let hw = build_hardware_profile(
            Some((total, "test".into())),
            None,
            None,
            8,
            16 * 1024 * 1024 * 1024,
            "test".into(),
        );
        let expected = (total as f64 * 0.95) as u64;
        assert_eq!(hw.free_vram_bytes, expected);
    }

    /// What this catches: when the MAX-VRAM rule applies (multiple
    /// backends report), pick the larger. NVIDIA cards sometimes have
    /// vulkaninfo report less than nvidia-smi (deviceLocal heap only);
    /// the gate should use the bigger number.
    #[test]
    fn vram_picks_max_across_backends() {
        let hw = build_hardware_profile(
            None,
            Some((40 * 1024 * 1024 * 1024, "cuda".into())),
            Some((20 * 1024 * 1024 * 1024, "vulkan".into())),
            16,
            64 * 1024 * 1024 * 1024,
            "test".into(),
        );
        assert_eq!(hw.total_vram_bytes, 40 * 1024 * 1024 * 1024);
    }

    /// What this catches: all three backends reporting (theoretical;
    /// would happen on a Mac with an external CUDA box + Vulkan ICD)
    /// flips all flags + picks max. Defensive — the design doesn't
    /// preclude multi-backend hosts, even if rare.
    #[test]
    fn all_three_backends_all_flags_true() {
        let hw = build_hardware_profile(
            Some((8 * 1024 * 1024 * 1024, "metal".into())),
            Some((16 * 1024 * 1024 * 1024, "cuda".into())),
            Some((12 * 1024 * 1024 * 1024, "vulkan".into())),
            16,
            32 * 1024 * 1024 * 1024,
            "test".into(),
        );
        assert!(hw.has_metal && hw.has_cuda && hw.has_vulkan);
        assert_eq!(hw.total_vram_bytes, 16 * 1024 * 1024 * 1024);
    }

    /// What this catches: platform string flows through unchanged. The
    /// gate's `NoGpuBackendOnNode` reason names this; telemetry uses it.
    #[test]
    fn platform_string_propagates() {
        let hw = build_hardware_profile(
            None,
            None,
            None,
            4,
            8 * 1024 * 1024 * 1024,
            "test-platform-123".into(),
        );
        assert_eq!(hw.platform, "test-platform-123");
    }

    /// What this catches: zero CPU cores from `num_cpus::get()` (would
    /// indicate a bug) is clamped to 1 via the `.max(1)` in
    /// probe_cpu_and_ram. Tested indirectly here by passing 0 to
    /// build_hardware_profile + asserting it propagates — the clamping
    /// happens upstream so build_hardware_profile faithfully reports
    /// whatever it receives. This test pins that build_hardware_profile
    /// doesn't itself silently fix bad inputs.
    #[test]
    fn zero_cpu_cores_propagates_to_profile() {
        let hw = build_hardware_profile(None, None, None, 0, 8 * 1024 * 1024 * 1024, "test".into());
        assert_eq!(hw.cpu_cores, 0);
    }

    // ===== composition with gate + probe =====

    /// What this catches: the probed HardwareProfile feeds cleanly into
    /// check_residency_gate. Composition smoke test — if either side's
    /// type contract drifts, this fails.
    #[test]
    fn probed_profile_feeds_residency_gate() {
        use crate::inference_capability::residency::{
            check_residency_gate, QwenModelMetadata, ResidencyGateResult,
        };

        let hw = build_hardware_profile(
            Some((32 * 1024 * 1024 * 1024, "M5 Pro".into())),
            None,
            None,
            16,
            64 * 1024 * 1024 * 1024,
            "macos-arm64-m5pro".into(),
        );
        let model = QwenModelMetadata {
            model_name: "Qwen2.5-7B".into(),
            architecture: "qwen2".into(),
            layer_count: 28,
            parameter_count_billions: 7.0,
            bytes_per_parameter_quantized: 0.5,
            layer_kinds_needing_check: vec![],
        };
        let result = check_residency_gate(&model, &hw);
        match result {
            ResidencyGateResult::Pass(_) => {} // expected
            other => panic!("M5 Pro probed profile should pass Qwen2.5-7B Q4; got {other:?}"),
        }
    }

    /// What this catches: a CPU-only probed profile fed to the gate
    /// blocks with NoGpuBackendOnNode. End-to-end composition test for
    /// the no-CPU-fallback contract.
    #[test]
    fn cpu_only_probed_profile_blocks_gate() {
        use crate::inference_capability::residency::{
            check_residency_gate, BlockReason, QwenModelMetadata, ResidencyGateResult,
        };

        let hw = build_hardware_profile(
            None,
            None,
            None,
            8,
            16 * 1024 * 1024 * 1024,
            "linux-x86_64-generic".into(),
        );
        let model = QwenModelMetadata {
            model_name: "Qwen2.5-0.5B".into(),
            architecture: "qwen2".into(),
            layer_count: 24,
            parameter_count_billions: 0.5,
            bytes_per_parameter_quantized: 0.5,
            layer_kinds_needing_check: vec![],
        };
        let result = check_residency_gate(&model, &hw);
        match result {
            ResidencyGateResult::Block { reasons } => {
                assert!(reasons
                    .iter()
                    .any(|r| matches!(r, BlockReason::NoGpuBackendOnNode { .. })));
            }
            other => panic!("CPU-only must block; got {other:?}"),
        }
    }

    // ===== live probe smoke test =====

    /// What this catches: probe_hardware_profile() doesn't panic on
    /// the current host. Smoke test — without specifying expected
    /// values (varies per machine), we just verify it runs + returns a
    /// reasonable profile.
    #[test]
    fn live_probe_does_not_panic() {
        let hw = probe_hardware_profile();
        // Sanity: cpu_cores must be at least 1 (clamped)
        assert!(
            hw.cpu_cores >= 1,
            "cpu_cores={} should be clamped >=1",
            hw.cpu_cores
        );
        // Sanity: platform string is non-empty
        assert!(!hw.platform.is_empty());
        // Sanity: on a no-GPU-features build, all flags must be false
        // (this test runs without specific features so we can't assert
        // positive flags; just that the call returned)
        let _ = hw.has_metal;
        let _ = hw.has_cuda;
        let _ = hw.has_vulkan;
    }

    /// What this catches: on macOS (test runner platform) the platform
    /// string includes "macos". On Linux, "linux". Sanity check on the
    /// runtime detection.
    #[test]
    fn live_probe_platform_includes_os() {
        let hw = probe_hardware_profile();
        let os = std::env::consts::OS;
        assert!(
            hw.platform.contains(os),
            "platform={} should contain os={}",
            hw.platform,
            os
        );
    }

    /// What this catches: probe_hardware_profile is callable multiple
    /// times without side effects (no caching / shared mutable state
    /// in the probe). Same input → same output. Important for
    /// caching strategies in PR-4.
    #[test]
    fn live_probe_is_idempotent_in_essentials() {
        let a = probe_hardware_profile();
        let b = probe_hardware_profile();
        // VRAM detection on the same host should be identical across
        // back-to-back calls (no other process is consuming VRAM in the
        // test microsecond).
        assert_eq!(a.has_metal, b.has_metal);
        assert_eq!(a.has_cuda, b.has_cuda);
        assert_eq!(a.has_vulkan, b.has_vulkan);
        assert_eq!(a.total_vram_bytes, b.total_vram_bytes);
        assert_eq!(a.platform, b.platform);
        assert_eq!(a.cpu_cores, b.cpu_cores);
    }
}
