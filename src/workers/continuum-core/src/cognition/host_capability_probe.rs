//! Host-capability probe — detect the [`HostCapability`] this machine
//! advertises to the model resolver.
//!
//! The resolver consumes [`HostCapability`] but doesn't construct it.
//! Production code paths that build a [`crate::cognition::ModelRequirement`]
//! need a real probe to populate the fields; tests construct
//! [`HostCapability`] directly. This module is the production probe.
//!
//! Pure module by design: takes the platform's already-existing
//! [`crate::gpu::monitor::GpuMonitor`] (constructed elsewhere with the
//! right `cfg` flags) and a [`sysinfo::System`] reference. Returns a
//! [`HostCapability`] or a typed [`ProbeError`].
//!
//! No silent CPU fallback. Per Joel's NO COMPROMISE bar (memory:
//! `project_continuum_alpha_product_bar_sensory_personas.md`): if the
//! GPU device-name pattern doesn't match a known hardware tier, the
//! probe ERRORS with [`ProbeError::UnknownGpuDevice`] naming the device.
//! Operator sees the loud-fail and adds the new tier to
//! [`HwCapabilityTier`] explicitly. There is no `Other(String)` /
//! wildcard escape.
//!
//! The CPU-only branch is intentionally absent: `gpu::memory_manager`
//! enforces "no GPU = panic at boot" per the #964 GPU-fallback rule, so
//! by the time the probe runs there's always a `GpuMonitor` of platform
//! `metal` / `cuda` / `vulkan`. Tests can pass `platform = "mock"` to
//! bypass.

use crate::cognition::adaptive_throughput::TargetSilicon;
use crate::cognition::model_resolver::{HostCapability, HwCapabilityTier};
use crate::gpu::monitor::GpuMonitor;
use serde::{Deserialize, Serialize};
use sysinfo::System;
use ts_rs::TS;

/// Why a [`detect_host_capability`] call failed. Loud-fail so the operator
/// sees exactly what the probe couldn't classify and can fix the tier
/// table.
#[derive(Debug, Clone, Serialize, Deserialize, TS, thiserror::Error)]
#[serde(rename_all = "camelCase", tag = "kind")]
#[ts(
    export,
    export_to = "../../../shared/generated/cognition/HostProbeError.ts"
)]
pub enum ProbeError {
    /// GPU was detected but its device-name doesn't match any known
    /// [`HwCapabilityTier`] variant. Names the device + platform so the
    /// operator can add a tier and resubmit. NOT a fallback to CpuOnly —
    /// silent fallback hides exactly the bugs the resolver exists to
    /// catch.
    #[error(
        "unknown GPU device on platform `{platform}`: `{device_name}`. \
         no silent fallback — add a HwCapabilityTier variant for this \
         hardware (or alias it to an existing one) in cognition::model_resolver."
    )]
    UnknownGpuDevice {
        platform: String,
        device_name: String,
    },
    /// The GPU monitor reports an unsupported platform string. The trait
    /// documents the supported set; an unknown platform means a new GPU
    /// adapter was added without updating this probe.
    #[error("unsupported GPU platform `{platform}` — extend host_capability_probe to handle it")]
    UnsupportedPlatform { platform: String },
}

/// Detect [`HostCapability`] from a live GPU monitor + system info
/// snapshot. Pure: caller owns both inputs.
///
/// Mapping rules:
/// - `platform == "metal"` → [`TargetSilicon::UnifiedMemory`]; tier from
///   CPU brand string + total memory (Apple M-series buckets).
/// - `platform == "cuda"` → [`TargetSilicon::Gpu`]; tier from device-name
///   pattern (RTX/A100/H100/V100/B100/T4/etc.).
/// - `platform == "vulkan"` → [`TargetSilicon::Gpu`];
///   [`HwCapabilityTier::VulkanAmd`].
/// - `platform == "mock"` → returns [`HwCapabilityTier::M1Uma16Gb`] /
///   [`TargetSilicon::UnifiedMemory`] (test fixture).
/// - any other → [`ProbeError::UnsupportedPlatform`].
///
/// `available_memory_mb` is the share of system memory inference is
/// willing to claim. Today's heuristic: half of total system RAM,
/// rounded down. Tunable later via a `share_fraction` parameter when a
/// caller needs different policy.
pub fn detect_host_capability(
    gpu_monitor: &dyn GpuMonitor,
    system_info: &System,
) -> Result<HostCapability, ProbeError> {
    let platform = gpu_monitor.platform();
    let device_name = gpu_monitor.device_name();

    let total_mem_bytes = system_info.total_memory();
    let total_mem_mb = (total_mem_bytes / 1_048_576) as u32;
    let available_memory_mb = total_mem_mb / 2;

    let (hw_capability_tier, primary_target_silicon) = match platform {
        "metal" => {
            let cpu_brand = first_cpu_brand(system_info);
            (
                apple_silicon_tier(&cpu_brand, total_mem_mb),
                TargetSilicon::UnifiedMemory,
            )
        }
        "cuda" => (nvidia_sm_tier(device_name, platform)?, TargetSilicon::Gpu),
        "vulkan" => (HwCapabilityTier::VulkanAmd, TargetSilicon::Gpu),
        "mock" => (HwCapabilityTier::M1Uma16Gb, TargetSilicon::UnifiedMemory),
        other => {
            return Err(ProbeError::UnsupportedPlatform {
                platform: other.to_string(),
            })
        }
    };

    Ok(HostCapability {
        hw_capability_tier,
        available_memory_mb,
        primary_target_silicon,
    })
}

/// First CPU's brand string from sysinfo, or empty string when no CPUs
/// were enumerated (only happens before `system.refresh_cpu_*()` ran).
/// Apple Silicon brands look like `Apple M3 Pro`, `Apple M2 Max`, etc.
fn first_cpu_brand(system_info: &System) -> String {
    system_info
        .cpus()
        .first()
        .map(|c| c.brand().to_string())
        .unwrap_or_default()
}

/// Map an Apple Silicon CPU brand + total system memory to an
/// [`HwCapabilityTier`]. The tier represents what model variants this
/// machine can run, not just the chip generation — so memory is part of
/// the bucket.
///
/// Buckets:
/// - M3+ chip → `M3UmaProMax` (assumes Pro/Max/Ultra config; base M3 with
///   <16GB still maps here because the M3 generation gates which adapter
///   sets we'd page in).
/// - M2 chip with ≥24GB memory → `M2UmaProMax`
/// - any Apple Silicon with ≥14GB memory → `M1Uma16Gb`
/// - else → `M1Uma8Gb` (M1 MBA baseline)
///
/// The thresholds are deliberately under the marketing "16GB / 32GB"
/// numbers because sysinfo reports physical-memory minus reserved
/// firmware/OS regions — a "16GB" Mac reports ~15.5GiB ≈ 15800MB.
fn apple_silicon_tier(cpu_brand: &str, total_mem_mb: u32) -> HwCapabilityTier {
    if cpu_brand.contains("M3") || cpu_brand.contains("M4") || cpu_brand.contains("M5") {
        HwCapabilityTier::M3UmaProMax
    } else if cpu_brand.contains("M2") && total_mem_mb >= 24_000 {
        HwCapabilityTier::M2UmaProMax
    } else if total_mem_mb >= 14_000 {
        HwCapabilityTier::M1Uma16Gb
    } else {
        HwCapabilityTier::M1Uma8Gb
    }
}

/// Map an NVIDIA device name to a CUDA compute-capability tier. The
/// trait doesn't expose the raw `compute_cap` (CUDA-only field), so we
/// pattern-match on device-name substrings the GPU SKUs reliably carry.
///
/// **Closed mapping by design** — see [`HwCapabilityTier`] doc. New SKUs
/// require an enum variant + a branch here. Returns
/// [`ProbeError::UnknownGpuDevice`] when the name doesn't match —
/// operator adds the variant rather than getting silent CpuOnly.
fn nvidia_sm_tier(device_name: &str, platform: &str) -> Result<HwCapabilityTier, ProbeError> {
    let upper = device_name.to_uppercase();
    // Order matters: more-specific patterns before less-specific. RTX 50
    // includes the substring "RTX 5" so RTX 50 must be checked before any
    // RTX 5x sibling pattern.
    if upper.contains("RTX 50") || upper.contains("RTX 5090") || upper.contains("RTX 5080") {
        Ok(HwCapabilityTier::Sm120)
    } else if upper.contains("B100") || upper.contains("B200") {
        Ok(HwCapabilityTier::Sm100)
    } else if upper.contains("H100") || upper.contains("H200") {
        Ok(HwCapabilityTier::Sm90)
    } else if upper.contains("RTX 40") {
        Ok(HwCapabilityTier::Sm89)
    } else if upper.contains("A100") {
        // Must precede the "A10" branch — substring overlap would
        // misclassify A100 as Sm86 otherwise.
        Ok(HwCapabilityTier::Sm80)
    } else if upper.contains("RTX 30") || upper.contains("A40") || upper.contains("A10") {
        Ok(HwCapabilityTier::Sm86)
    } else if upper.contains("T4") || upper.contains("RTX 20") || upper.contains("GTX 16") {
        Ok(HwCapabilityTier::Sm75)
    } else if upper.contains("V100") {
        Ok(HwCapabilityTier::Sm70)
    } else {
        Err(ProbeError::UnknownGpuDevice {
            platform: platform.to_string(),
            device_name: device_name.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu::monitor::MockMonitor;

    fn fresh_system() -> System {
        let mut s = System::new();
        s.refresh_memory();
        s.refresh_cpu_all();
        s
    }

    #[test]
    fn mock_platform_returns_test_fixture() {
        let monitor = MockMonitor::new(16_000_000_000);
        let sys = fresh_system();
        let cap = detect_host_capability(&monitor, &sys).unwrap();
        assert_eq!(cap.hw_capability_tier, HwCapabilityTier::M1Uma16Gb);
        assert_eq!(cap.primary_target_silicon, TargetSilicon::UnifiedMemory);
        assert!(
            cap.available_memory_mb > 0,
            "available memory should be derived from sysinfo"
        );
    }

    #[test]
    fn unsupported_platform_errors_loudly() {
        struct OddballMonitor;
        impl GpuMonitor for OddballMonitor {
            fn platform(&self) -> &'static str {
                "trapped-in-an-fpga"
            }
            fn device_name(&self) -> &str {
                "Some Custom FPGA Card"
            }
            fn total_bytes(&self) -> u64 {
                1
            }
            fn free_bytes(&self) -> u64 {
                1
            }
            fn process_bytes(&self) -> u64 {
                0
            }
            fn utilization(&self) -> f32 {
                0.0
            }
            fn temperature_c(&self) -> Option<f32> {
                None
            }
            fn power_watts(&self) -> Option<f32> {
                None
            }
            fn pressure_rx(&self) -> tokio::sync::watch::Receiver<f32> {
                let (_tx, rx) = tokio::sync::watch::channel(0.0);
                rx
            }
        }
        let sys = fresh_system();
        let err = detect_host_capability(&OddballMonitor, &sys).unwrap_err();
        match err {
            ProbeError::UnsupportedPlatform { platform } => {
                assert_eq!(platform, "trapped-in-an-fpga");
            }
            other => panic!("expected UnsupportedPlatform; got {other:?}"),
        }
    }

    #[test]
    fn nvidia_pattern_match_resolves_known_skus() {
        // Each pair: device-name substring as the GPU monitor would
        // report it, expected HwCapabilityTier. Uses the platform="cuda"
        // branch via nvidia_sm_tier directly.
        let cases = &[
            ("NVIDIA GeForce RTX 5090", HwCapabilityTier::Sm120),
            ("NVIDIA GeForce RTX 4090", HwCapabilityTier::Sm89),
            ("NVIDIA GeForce RTX 3080", HwCapabilityTier::Sm86),
            ("NVIDIA H100 PCIe", HwCapabilityTier::Sm90),
            ("NVIDIA A100-SXM4-80GB", HwCapabilityTier::Sm80),
            ("Tesla T4", HwCapabilityTier::Sm75),
            ("NVIDIA GeForce RTX 2080 Ti", HwCapabilityTier::Sm75),
            ("NVIDIA Tesla V100-SXM2-16GB", HwCapabilityTier::Sm70),
            ("NVIDIA B100 80GB", HwCapabilityTier::Sm100),
        ];
        for (name, expected) in cases {
            assert_eq!(
                nvidia_sm_tier(name, "cuda").unwrap(),
                *expected,
                "device name `{name}` should map to {expected:?}",
            );
        }
    }

    #[test]
    fn nvidia_unknown_sku_errors_no_silent_fallback() {
        let err = nvidia_sm_tier("NVIDIA Voodoo 5 6000", "cuda").unwrap_err();
        match err {
            ProbeError::UnknownGpuDevice {
                platform,
                device_name,
            } => {
                assert_eq!(platform, "cuda");
                assert_eq!(device_name, "NVIDIA Voodoo 5 6000");
            }
            other => panic!("expected UnknownGpuDevice; got {other:?}"),
        }
    }

    #[test]
    fn apple_silicon_tier_mapping() {
        assert_eq!(
            apple_silicon_tier("Apple M1", 8_000),
            HwCapabilityTier::M1Uma8Gb
        );
        assert_eq!(
            apple_silicon_tier("Apple M1", 15_500),
            HwCapabilityTier::M1Uma16Gb
        );
        assert_eq!(
            apple_silicon_tier("Apple M2 Max", 32_000),
            HwCapabilityTier::M2UmaProMax
        );
        assert_eq!(
            apple_silicon_tier("Apple M2", 8_000),
            HwCapabilityTier::M1Uma8Gb,
            "M2 with low memory falls into the 8Gb tier; chip generation \
             alone doesn't bump tier without enough memory"
        );
        assert_eq!(
            apple_silicon_tier("Apple M3 Pro", 18_000),
            HwCapabilityTier::M3UmaProMax
        );
        assert_eq!(
            apple_silicon_tier("Apple M4 Max", 64_000),
            HwCapabilityTier::M3UmaProMax,
            "M4 currently aliases to M3UmaProMax until a dedicated tier ships"
        );
    }
}
