//! Substrate Governor typed surface — Lane H PR-1 (substrate-governor:
//! governor-types) per GENOME-FOUNDRY-SENTINEL #1327 Part 11.
//!
//! The governor is the DVFS layer for the AI substrate. The ONE Rust
//! subsystem that makes "same code on MacBook Air and RTX 5090" real:
//! detect hardware at boot, write the policy file, expose a read-only
//! `current_policy()` to every other subsystem, adjust at runtime under
//! pressure, and reverse cleanly when pressure releases. Every other
//! subsystem in this design — tier stores, recall, composer, speculator,
//! foundry, sentinel, sharing protocol — reads the governor and never
//! writes back. The governor IS the single source of truth for sizing.
//!
//! ## PR-1 scope (this file)
//!
//! Pure typed surface. No impl, no TOML loader, no cascade state
//! machine, no probe wiring. Later slices ship policy parsing,
//! selection, cascade, and pressure-signal subscriber wiring.
//!
//! This matches the rate_proposals / generate_recipe / PIECE-5 PR-1
//! cadence — typed surface first, impl second, integration third.
//!
//! ## Hardware bridge
//!
//! `classify_hardware(profile: HardwareProfile) -> HardwareClass` is
//! the pure function that maps my just-shipped `hw_probe` (PIECE-5
//! PR-3 #1335) output to the typed governor input. It's the seam
//! between the probe layer (boolean flags + numeric VRAM/RAM) and the
//! governor layer (typed enum classification). PR-2 of substrate-
//! governor wires the actual TOML policy file selection off the
//! resulting `HardwareClass`.

use crate::inference_capability::types::HardwareProfile;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

// ─── Hardware classification ─────────────────────────────────────────

/// Which GPU / inference silicon class this node has. Fallbacks are
/// typed + named — no silent "guess where we are" per the no_silent_fallback
/// rule the rest of the substrate honors.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/governor/TargetSilicon.ts"
)]
pub enum TargetSilicon {
    /// Apple Silicon (M1/M2/M3/M4/M5 + descendants). UMA — system_ram
    /// and "vram" are the same physical pool.
    AppleM,
    /// Mac Intel chassis with a discrete GPU (typically AMD Polaris /
    /// Vega / Navi) exposed through Apple's Metal driver, not CUDA or
    /// Vulkan. NOT UMA — Intel CPU + discrete VRAM. Examples:
    /// MacBookPro15,1 with Radeon Pro 555X; iMac (Intel, 2019) with
    /// Radeon Pro 580X; Mac Pro (2019) with W6800X Duo. The hardware
    /// reports `has_metal: true` exactly like Apple Silicon does, but
    /// the tier policy is the discrete-GPU shape (5 tier roles
    /// including Warm), NOT the UMA-class 4-tier shape. Caller
    /// distinguishes via the platform architecture string in
    /// HardwareProfile (`macos-arm64-*` = AppleM, `macos-x86_64-*` =
    /// MacIntelMetal). Without this variant, classify_silicon falls
    /// to `AppleM` for any host with `has_metal: true` and the entire
    /// downstream governor + tier_configs pipeline picks the wrong
    /// shape (task #52).
    MacIntelMetal,
    /// NVIDIA CUDA. Discrete VRAM separate from system RAM.
    NvidiaCuda,
    /// AMD ROCm. Discrete VRAM separate from system RAM. Less mature
    /// than CUDA for our workloads but supported.
    AmdRocm,
    /// Intel Arc / discrete GPU via Vulkan. Fallback path for non-
    /// CUDA/non-ROCm discrete cards.
    IntelVulkan,
    /// No GPU detected. The governor refuses to launch a CPU-only
    /// policy — `None` here surfaces a `NoGpuBackendOnNode`-shape
    /// failure upstream (the inference layer's gate already enforces
    /// this; the governor inherits the contract).
    None,
}

/// Where the node is getting power. Affects power/perf trade-offs in
/// the governor's policy. On a laptop on battery, the governor
/// throttles speculation + lowers consolidation cadence; on plugged-in
/// the same hardware runs at full aggressiveness.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/governor/PowerSource.ts"
)]
pub enum PowerSource {
    Battery,
    Plugged,
}

/// Coarse thermal class. Drives the cascade's aggressiveness — a
/// ThinAndLight chassis throttles at lower thermals than a Workstation.
/// Probed from silicon + chassis hints at boot.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/governor/ThermalClass.ts"
)]
pub enum ThermalClass {
    /// Laptop, fan-limited. MacBook Air, Surface Pro, ultrabooks.
    ThinAndLight,
    /// Workstation desktop / Mac Studio / tower. Substantial cooling.
    Workstation,
    /// Rack server / colocated hardware. Best cooling.
    Server,
    /// Phone, tablet, Vision Pro. Aggressive thermal throttling expected.
    Mobile,
}

/// Live thermal pressure signal. Drives cascade-step entry/exit.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "kebab-case")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/governor/ThermalSeverity.ts"
)]
pub enum ThermalSeverity {
    Cool,
    Warm,
    Hot,
    Critical,
}

/// Hardware classification produced at boot + on hardware-change
/// events. The governor selects a policy file off this fingerprint.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/governor/HardwareClass.ts"
)]
pub struct HardwareClass {
    pub silicon: TargetSilicon,
    /// Human-readable model name ("M2", "RTX 5090", "Radeon RX 7900 XTX").
    /// From sysinfo / nvidia-smi / metal::Device::name.
    pub silicon_model: String,
    /// VRAM in MB. 0 for unified-memory targets (Apple Silicon) where
    /// the governor uses a fraction of `system_ram_mb` for inference.
    #[ts(type = "number")]
    pub vram_mb: u64,
    /// System RAM in MB. Always populated.
    #[ts(type = "number")]
    pub system_ram_mb: u64,
    pub power_source: PowerSource,
    pub thermal_class: ThermalClass,
    /// Battery charge, 0-100. `None` if no battery (desktop, server).
    #[ts(type = "number | null")]
    pub battery_pct: Option<u8>,
    /// Thermal headroom 0-100 (100 = cold, 0 = at-limit). `None` if
    /// the platform doesn't expose it.
    #[ts(type = "number | null")]
    pub thermal_headroom_pct: Option<u8>,
}

// ─── Governor policy ─────────────────────────────────────────────────

/// Tier sizes the governor budgets per HardwareClass. Loaded from TOML
/// in PR-3. PR-1 ships the type so other modules can reference it.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/governor/TierSizes.ts"
)]
pub struct TierSizes {
    #[ts(type = "number")]
    pub l1_lora_layers: u32,
    #[ts(type = "number")]
    pub l1_kv_tokens: u32,
    #[ts(type = "number")]
    pub l2_lora_layers: u32,
    #[ts(type = "number")]
    pub l3_lora_layers: u32,
    #[ts(type = "number")]
    pub l3_engrams: u32,
}

/// Multipliers applied to cadence schedules per resource class. realtime
/// stays at 1.0; delayed and background stretch under pressure.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/governor/CadenceMultipliers.ts"
)]
pub struct CadenceMultipliers {
    pub realtime: f32,
    pub delayed: f32,
    pub background: f32,
}

/// Per-subsystem concurrency caps. Governor reduces under pressure;
/// modules read at task-dispatch time.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/governor/ConcurrencyCaps.ts"
)]
pub struct ConcurrencyCaps {
    #[ts(type = "number")]
    pub personas_concurrent: u32,
    #[ts(type = "number")]
    pub inference_lanes: u32,
    #[ts(type = "number")]
    pub foundry_lanes: u32,
    #[ts(type = "number")]
    pub sentinel_lanes: u32,
}

/// Speculation aggressiveness. Drops under pressure (cascade step 1).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "kebab-case")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/governor/SpeculationLevel.ts"
)]
pub enum SpeculationLevel {
    Off,
    Conservative,
    Balanced,
    Aggressive,
}

/// When consolidation (artifact refinement, engram crystallization) runs.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/governor/ConsolidationSchedule.ts"
)]
pub enum ConsolidationSchedule {
    Always,
    Idle,
    IdlePluggedIn,
    Manual,
}

/// Federation pull cadence — how often a node pulls peer artifacts.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/governor/FederationCadence.ts"
)]
pub struct FederationCadence {
    #[ts(type = "number")]
    pub pull_cadence_seconds: u32,
}

/// Scoring weights for `DemandAlignedRecall` (Lane H PR-3). Sum should
/// be ~1.0 by convention; the governor's policy file enforces this.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/governor/RecallScoreWeights.ts"
)]
pub struct RecallScoreWeights {
    pub semantic: f32,
    pub outcome_history: f32,
    pub recency: f32,
    pub tier_proximity: f32,
    pub provenance_trust: f32,
}

/// The full policy the governor publishes. Every other subsystem reads
/// this; no one writes back. Rewritten on cascade steps + hardware
/// changes via `arc_swap`.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/governor/GovernorPolicy.ts"
)]
pub struct GovernorPolicy {
    /// Monotonic; increments on every rewrite. Subscribers compare to
    /// detect "did the policy change since I last looked."
    #[ts(type = "number")]
    pub policy_version: u64,
    /// What HardwareClass produced this policy.
    pub hardware_class: HardwareClass,
    pub tier_sizes: TierSizes,
    pub cadence_multipliers: CadenceMultipliers,
    pub concurrency_caps: ConcurrencyCaps,
    pub speculation_aggressiveness: SpeculationLevel,
    pub consolidation_schedule: ConsolidationSchedule,
    pub federation_pull_cadence: FederationCadence,
    pub recall_score_weights: RecallScoreWeights,
    /// 0 = normal; 1..5 = under pressure (see cascade in PR-3).
    #[ts(type = "number")]
    pub cascade_step: u8,
    /// Unix-ms timestamp the policy was committed.
    #[ts(type = "number")]
    pub committed_at_ms: u64,
}

// ─── Pressure signals + snapshot ─────────────────────────────────────

/// Typed pressure signals the cascade reacts to. PressureBroker
/// (CBAR-SUBSTRATE Lane E) emits these; governor consumes.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase", tag = "kind")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/governor/PressureSignal.ts"
)]
pub enum PressureSignal {
    Thermal {
        severity: ThermalSeverity,
    },
    BatteryLow {
        #[ts(type = "number")]
        remaining_pct: u8,
    },
    SystemMemHigh {
        #[ts(type = "number")]
        used_pct: u8,
    },
    VRAMHigh {
        #[ts(type = "number")]
        used_pct: u8,
    },
    UserActive {
        foreground: bool,
    },
    InferenceQueueDepth {
        #[ts(type = "number")]
        depth: u32,
    },
    SpeculationMissRate {
        rate: f32,
    },
}

/// Telemetry snapshot — current policy + cascade-step counter +
/// recent cascade history (PR-3 wires the history; PR-1 ships the
/// shape).
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/governor/GovernorSnapshot.ts"
)]
pub struct GovernorSnapshot {
    pub current_policy: GovernorPolicy,
    /// Number of cascade-step transitions since boot. Diagnostic — high
    /// counts = oscillation, low counts = stable.
    #[ts(type = "number")]
    pub cascade_transition_count: u64,
    /// Last N pressure signals received. PR-3 implements; PR-1 ships
    /// the slot. Empty in PR-1.
    pub recent_signals: Vec<PressureSignal>,
}

// ─── Hardware classification bridge ──────────────────────────────────

/// Pure-function bridge from my `hw_probe` PIECE-5 PR-3 #1335 surface
/// (`HardwareProfile`: boolean flags + numeric VRAM/RAM) to the
/// governor's typed `HardwareClass`.
///
/// The classification is conservative — when in doubt, picks the
/// more-throttled side of the policy spectrum:
///
/// - `power_source` defaults to `Plugged` when undetermined (matches
///   the spec's "favor performance when we can't tell").
/// - `thermal_class` defaults to `Workstation` unless an explicit
///   ThinAndLight hint is present in the platform string (cheap
///   substring match for "macbook-air" / similar). PR-2 wires a
///   proper IORegistry / DMI probe.
/// - `battery_pct` + `thermal_headroom_pct` default to `None` —
///   they require platform-specific syscalls that PR-2 wires.
///
/// All defaults are documented (no silent guess); see also the
/// hardware-detection §"All fallbacks are typed and logged" in
/// GENOME-FOUNDRY-SENTINEL.md Part 11.
pub fn classify_hardware(profile: &HardwareProfile) -> HardwareClass {
    let silicon = classify_silicon(profile);
    let thermal_class = classify_thermal_class(&profile.platform);
    let system_ram_mb = profile.system_ram_bytes / (1024 * 1024);
    // For UMA (Apple Silicon), vram_mb is 0 per spec — the governor
    // computes the inference budget as a fraction of system_ram_mb.
    // For discrete GPUs, vram_mb is the actual VRAM.
    let vram_mb = if silicon == TargetSilicon::AppleM {
        0
    } else {
        profile.total_vram_bytes / (1024 * 1024)
    };

    HardwareClass {
        silicon,
        silicon_model: derive_silicon_model(profile),
        vram_mb,
        system_ram_mb,
        // Plugged is the "favor performance when we can't tell"
        // default per spec. PR-2 wires real probe.
        power_source: PowerSource::Plugged,
        thermal_class,
        battery_pct: None,
        thermal_headroom_pct: None,
    }
}

/// Classify silicon from hw_probe's three booleans. Apple Silicon wins
/// over CUDA on a Mac (native path). CUDA wins over Vulkan when both
/// present (CUDA kernels more complete than Vulkan in our llama.cpp
/// build). ROCm detection is left for PR-2 (requires rocm-smi probe).
fn classify_silicon(profile: &HardwareProfile) -> TargetSilicon {
    if profile.has_metal {
        // Bug fix for task #52: `has_metal: true` is reported by BOTH
        // Apple Silicon (UMA) AND Mac Intel hosts with a discrete GPU
        // exposed through Apple's Metal driver (Joel's MacBookPro15,1 +
        // Radeon Pro 555X is the canonical example). Pre-fix, both
        // shipped as `AppleM`, so `classify_hardware` then forced
        // `vram_mb = 0`, and `tier_configs_for` returned the UMA
        // 4-tier shape (no Warm) on a host that's actually
        // discrete-GPU and needs the 5-tier shape including Warm.
        //
        // The differentiator is the platform string's architecture
        // hint. Apple Silicon platforms include `arm64` / `aarch64`
        // (e.g. `macos-arm64-air`, `macos-arm64-m5pro`). Mac Intel
        // includes `x86_64` (`macos-x86_64-pro`,
        // `macos-x86_64-macbookpro15`). The hw_probe upstream sets
        // these reliably per task #47.
        //
        // Default-to-AppleM-when-uncertain is intentional: the
        // dominant Metal-shipping target IS Apple Silicon today,
        // and on a future Apple-on-Apple host we'd rather lean UMA
        // than treat it as discrete. The fallback only fires when
        // the platform string is silent on architecture — at which
        // point the upstream hw_probe has a separate bug to surface.
        let platform_lower = profile.platform.to_lowercase();
        let is_mac_intel = platform_lower.contains("x86_64")
            || platform_lower.contains("x86-64")
            || platform_lower.contains("intel");
        if is_mac_intel {
            TargetSilicon::MacIntelMetal
        } else {
            TargetSilicon::AppleM
        }
    } else if profile.has_cuda {
        TargetSilicon::NvidiaCuda
    } else if profile.has_vulkan {
        TargetSilicon::IntelVulkan
    } else {
        TargetSilicon::None
    }
}

/// Coarse thermal-class derivation from platform string. PR-2 wires a
/// real probe (IORegistry on macOS, DMI on Linux). PR-1 uses substring
/// hints — wrong sometimes, never silent (typed + tested + commented).
fn classify_thermal_class(platform: &str) -> ThermalClass {
    let p = platform.to_lowercase();
    if p.contains("ios") || p.contains("vision-pro") || p.contains("mobile") {
        ThermalClass::Mobile
    } else if p.contains("air") || p.contains("ultrabook") || p.contains("surface") {
        ThermalClass::ThinAndLight
    } else if p.contains("server") || p.contains("colocated") {
        ThermalClass::Server
    } else {
        // Default to Workstation — fan-rich desktops, Mac Studios, Mac
        // Pros, gaming/training rigs. The most common runtime target.
        ThermalClass::Workstation
    }
}

/// Derive a human-readable silicon model from the platform string.
/// PR-2 wires per-platform probes (Metal device name, nvidia-smi
/// --query-gpu=name); PR-1 uses platform string as a placeholder.
fn derive_silicon_model(profile: &HardwareProfile) -> String {
    profile.platform.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mac_m2_air() -> HardwareProfile {
        HardwareProfile {
            platform: "macos-arm64-air".into(),
            has_metal: true,
            has_cuda: false,
            has_vulkan: false,
            free_vram_bytes: 5 * 1024 * 1024 * 1024,
            total_vram_bytes: 8 * 1024 * 1024 * 1024,
            cpu_cores: 8,
            system_ram_bytes: 16 * 1024 * 1024 * 1024,
        }
    }

    fn m5_pro_workstation() -> HardwareProfile {
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

    /// Mac Intel chassis (MacBookPro15,1 = the canonical task #52
    /// regression target) with an AMD discrete GPU exposed via Metal.
    /// The hardware reports `has_metal: true` exactly like Apple
    /// Silicon, but the platform string carries `x86_64`. Joel's
    /// actual hardware ships this profile.
    fn mac_intel_macbookpro15() -> HardwareProfile {
        HardwareProfile {
            platform: "macos-x86_64-macbookpro15".into(),
            has_metal: true,
            has_cuda: false,
            has_vulkan: false,
            // Radeon Pro 555X is the discrete GPU; 4 GiB GDDR5.
            free_vram_bytes: 3 * 1024 * 1024 * 1024,
            total_vram_bytes: 4 * 1024 * 1024 * 1024,
            cpu_cores: 6,
            system_ram_bytes: 16 * 1024 * 1024 * 1024,
        }
    }

    fn blackwell_5090() -> HardwareProfile {
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

    fn amd_vulkan_workstation() -> HardwareProfile {
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

    fn cpu_only_server() -> HardwareProfile {
        HardwareProfile {
            platform: "linux-x86_64-server".into(),
            has_metal: false,
            has_cuda: false,
            has_vulkan: false,
            free_vram_bytes: 0,
            total_vram_bytes: 0,
            cpu_cores: 32,
            system_ram_bytes: 128 * 1024 * 1024 * 1024,
        }
    }

    fn vision_pro() -> HardwareProfile {
        HardwareProfile {
            platform: "ios-arm64-vision-pro".into(),
            has_metal: true,
            has_cuda: false,
            has_vulkan: false,
            free_vram_bytes: 6 * 1024 * 1024 * 1024,
            total_vram_bytes: 8 * 1024 * 1024 * 1024,
            cpu_cores: 8,
            system_ram_bytes: 16 * 1024 * 1024 * 1024,
        }
    }

    // ===== classify_silicon =====

    /// What this catches: Apple Silicon wins the silicon classification
    /// on Mac. This is THE most common runtime; if it regresses, every
    /// Mac runs through the wrong policy.
    #[test]
    fn mac_classifies_as_apple_m() {
        assert_eq!(
            classify_hardware(&mac_m2_air()).silicon,
            TargetSilicon::AppleM
        );
        assert_eq!(
            classify_hardware(&m5_pro_workstation()).silicon,
            TargetSilicon::AppleM
        );
    }

    /// Task #52 regression: Mac Intel chassis with `has_metal: true`
    /// must classify as `MacIntelMetal`, NOT `AppleM`. Pre-fix, the
    /// `if profile.has_metal { TargetSilicon::AppleM }` branch swallowed
    /// every metal-capable host into the UMA bucket — including Joel's
    /// MacBookPro15,1 with Radeon Pro 555X — and forced `vram_mb = 0`,
    /// then `tier_configs_for` emitted the UMA 4-tier shape on a
    /// discrete-GPU host. Misclassification cascaded through the
    /// substrate's governor + paging policy.
    #[test]
    fn mac_intel_classifies_as_mac_intel_metal_not_apple_m() {
        let cls = classify_hardware(&mac_intel_macbookpro15());
        assert_eq!(
            cls.silicon,
            TargetSilicon::MacIntelMetal,
            "Mac Intel with discrete GPU via Metal must be \
             MacIntelMetal, NOT AppleM (task #52 regression). \
             classify_silicon got: {:?}",
            cls.silicon
        );
    }

    /// Same regression cross-checked via the downstream side-effect:
    /// `classify_hardware` zeros `vram_mb` on AppleM (UMA spec) but
    /// must preserve real VRAM on MacIntelMetal. The misclassification
    /// pre-fix manifested HERE — Joel's machine reported `vram_mb = 0`
    /// despite having 4 GiB of GDDR5 on the Radeon, and the governor's
    /// inference budget then drew from system_ram instead of VRAM.
    #[test]
    fn mac_intel_preserves_real_vram_mb() {
        let cls = classify_hardware(&mac_intel_macbookpro15());
        let expected_mb = mac_intel_macbookpro15().total_vram_bytes / (1024 * 1024);
        assert_eq!(
            cls.vram_mb, expected_mb,
            "Mac Intel with discrete GPU must report real VRAM, not 0 \
             (the AppleM UMA branch must not swallow discrete hosts). \
             Got: vram_mb={}, expected_mb={}",
            cls.vram_mb, expected_mb
        );
    }

    /// What this catches: NVIDIA + Vulkan (typical Blackwell setup)
    /// classifies as NvidiaCuda — CUDA wins over Vulkan when both
    /// present (CUDA kernels more complete in our llama.cpp build).
    #[test]
    fn nvidia_with_vulkan_classifies_as_cuda() {
        assert_eq!(
            classify_hardware(&blackwell_5090()).silicon,
            TargetSilicon::NvidiaCuda
        );
    }

    /// What this catches: AMD/Intel Vulkan-only host classifies as
    /// IntelVulkan. Without ROCm detection (PR-2), AMD also falls
    /// here — documented limitation.
    #[test]
    fn vulkan_only_classifies_as_intel_vulkan() {
        assert_eq!(
            classify_hardware(&amd_vulkan_workstation()).silicon,
            TargetSilicon::IntelVulkan
        );
    }

    /// What this catches: CPU-only host classifies as None. Governor
    /// must surface "no GPU" rather than silently launch a CPU policy
    /// — same no_silent_fallback rule as the inference gate.
    #[test]
    fn cpu_only_classifies_as_none() {
        assert_eq!(
            classify_hardware(&cpu_only_server()).silicon,
            TargetSilicon::None
        );
    }

    // ===== UMA VRAM handling =====

    /// What this catches: UMA targets report `vram_mb = 0` per spec.
    /// The governor's policy file selects "use system_ram fraction" when
    /// it sees 0. If this regresses (we report real VRAM for UMA), the
    /// policy double-counts memory.
    #[test]
    fn apple_silicon_vram_reported_as_zero_uma_convention() {
        let cls = classify_hardware(&mac_m2_air());
        assert_eq!(cls.vram_mb, 0, "UMA must report vram_mb=0 per spec");
        assert!(cls.system_ram_mb > 0, "system_ram_mb must be populated");
    }

    /// What this catches: discrete GPU reports actual VRAM. Without
    /// this, the governor can't size tier_sizes correctly on Blackwell
    /// (32GB → tier sizes need to match).
    #[test]
    fn nvidia_vram_reflects_total_vram() {
        let cls = classify_hardware(&blackwell_5090());
        let expected_mb = 32 * 1024; // 32GB
        assert_eq!(cls.vram_mb, expected_mb);
    }

    // ===== thermal_class =====

    /// What this catches: "air" in platform string → ThinAndLight.
    /// MacBook Air is the canonical low-thermal-budget target; the
    /// policy file should throttle speculation + cap personas.
    #[test]
    fn air_platform_classifies_as_thin_and_light() {
        assert_eq!(
            classify_hardware(&mac_m2_air()).thermal_class,
            ThermalClass::ThinAndLight
        );
    }

    /// What this catches: M5 Pro (no "air" in name) classifies as
    /// Workstation. Mac Studios / desktops get the full policy.
    #[test]
    fn m5_pro_classifies_as_workstation() {
        assert_eq!(
            classify_hardware(&m5_pro_workstation()).thermal_class,
            ThermalClass::Workstation
        );
    }

    /// What this catches: iOS / Vision Pro classifies as Mobile — the
    /// most aggressive thermal throttling target.
    #[test]
    fn ios_classifies_as_mobile() {
        assert_eq!(
            classify_hardware(&vision_pro()).thermal_class,
            ThermalClass::Mobile
        );
    }

    /// What this catches: "server" in platform → Server thermal class.
    /// Best cooling, least throttling.
    #[test]
    fn server_platform_classifies_as_server() {
        assert_eq!(
            classify_hardware(&cpu_only_server()).thermal_class,
            ThermalClass::Server
        );
    }

    /// What this catches: unknown platform defaults to Workstation
    /// (most common runtime target). Documented in code comment.
    #[test]
    fn unknown_platform_defaults_to_workstation() {
        let mut hw = blackwell_5090();
        hw.platform = "some-future-platform".into();
        assert_eq!(
            classify_hardware(&hw).thermal_class,
            ThermalClass::Workstation
        );
    }

    // ===== defaults =====

    /// What this catches: power_source defaults to Plugged (favor
    /// performance when undetermined). PR-2 wires real probe.
    #[test]
    fn power_source_defaults_to_plugged() {
        assert_eq!(
            classify_hardware(&mac_m2_air()).power_source,
            PowerSource::Plugged
        );
    }

    /// What this catches: battery_pct + thermal_headroom_pct are None
    /// in PR-1 (no probe yet). When PR-2 wires the probe, this test
    /// will need updating — by design, surfaces the missing-data state
    /// in code review.
    #[test]
    fn battery_and_thermal_headroom_are_none_in_pr1() {
        let cls = classify_hardware(&mac_m2_air());
        assert_eq!(cls.battery_pct, None);
        assert_eq!(cls.thermal_headroom_pct, None);
    }

    // ===== full HardwareClass shape =====

    /// What this catches: every required field on HardwareClass is
    /// populated by classify_hardware. Sanity check on the full
    /// classification.
    #[test]
    fn classify_populates_every_field() {
        let cls = classify_hardware(&blackwell_5090());
        assert_eq!(cls.silicon, TargetSilicon::NvidiaCuda);
        assert!(!cls.silicon_model.is_empty());
        assert!(cls.vram_mb > 0);
        assert!(cls.system_ram_mb > 0);
        assert_eq!(cls.power_source, PowerSource::Plugged);
        assert_eq!(cls.thermal_class, ThermalClass::Workstation);
    }

    // ===== serde + ts-rs =====

    /// What this catches: TargetSilicon serializes kebab-case for the
    /// TS wire. Wire stability — every consumer parses these strings.
    #[test]
    fn target_silicon_serializes_kebab_case() {
        assert_eq!(
            serde_json::to_string(&TargetSilicon::AppleM).unwrap(),
            "\"apple-m\""
        );
        assert_eq!(
            serde_json::to_string(&TargetSilicon::NvidiaCuda).unwrap(),
            "\"nvidia-cuda\""
        );
        assert_eq!(
            serde_json::to_string(&TargetSilicon::AmdRocm).unwrap(),
            "\"amd-rocm\""
        );
        assert_eq!(
            serde_json::to_string(&TargetSilicon::IntelVulkan).unwrap(),
            "\"intel-vulkan\""
        );
        assert_eq!(
            serde_json::to_string(&TargetSilicon::None).unwrap(),
            "\"none\""
        );
    }

    /// What this catches: HardwareClass round-trips with camelCase.
    /// TS consumers (continuum status, telemetry dashboard) depend on
    /// these names.
    #[test]
    fn hardware_class_serde_camelcase() {
        let cls = classify_hardware(&blackwell_5090());
        let j = serde_json::to_string(&cls).unwrap();
        assert!(j.contains("\"siliconModel\""));
        assert!(j.contains("\"vramMb\""));
        assert!(j.contains("\"systemRamMb\""));
        assert!(j.contains("\"powerSource\""));
        assert!(j.contains("\"thermalClass\""));
        let back: HardwareClass = serde_json::from_str(&j).unwrap();
        assert_eq!(back, cls);
    }

    /// What this catches: GovernorPolicy round-trips with every field
    /// populated. The policy is the canonical published shape; if it
    /// breaks, every subscriber breaks.
    #[test]
    fn governor_policy_serde_round_trip() {
        let policy = GovernorPolicy {
            policy_version: 7,
            hardware_class: classify_hardware(&m5_pro_workstation()),
            tier_sizes: TierSizes {
                l1_lora_layers: 4,
                l1_kv_tokens: 4096,
                l2_lora_layers: 8,
                l3_lora_layers: 24,
                l3_engrams: 4096,
            },
            cadence_multipliers: CadenceMultipliers {
                realtime: 1.0,
                delayed: 1.0,
                background: 1.5,
            },
            concurrency_caps: ConcurrencyCaps {
                personas_concurrent: 4,
                inference_lanes: 2,
                foundry_lanes: 1,
                sentinel_lanes: 1,
            },
            speculation_aggressiveness: SpeculationLevel::Balanced,
            consolidation_schedule: ConsolidationSchedule::Idle,
            federation_pull_cadence: FederationCadence {
                pull_cadence_seconds: 300,
            },
            recall_score_weights: RecallScoreWeights {
                semantic: 0.4,
                outcome_history: 0.3,
                recency: 0.1,
                tier_proximity: 0.1,
                provenance_trust: 0.1,
            },
            cascade_step: 0,
            committed_at_ms: 1_715_625_600_000,
        };
        let j = serde_json::to_string(&policy).unwrap();
        let back: GovernorPolicy = serde_json::from_str(&j).unwrap();
        assert_eq!(back, policy);
        assert!(j.contains("\"policyVersion\":7"));
        assert!(j.contains("\"cascadeStep\":0"));
        assert!(j.contains("\"speculationAggressiveness\":\"balanced\""));
    }

    /// What this catches: PressureSignal tagged-union round-trips via
    /// the `kind` discriminator. PressureBroker emits these via
    /// MessageBus; governor deserializes from peer wire.
    #[test]
    fn pressure_signal_tagged_union_round_trips() {
        let signals = vec![
            PressureSignal::Thermal {
                severity: ThermalSeverity::Hot,
            },
            PressureSignal::BatteryLow { remaining_pct: 15 },
            PressureSignal::SystemMemHigh { used_pct: 90 },
            PressureSignal::VRAMHigh { used_pct: 85 },
            PressureSignal::UserActive { foreground: true },
            PressureSignal::InferenceQueueDepth { depth: 12 },
            PressureSignal::SpeculationMissRate { rate: 0.7 },
        ];
        for sig in &signals {
            let j = serde_json::to_string(sig).unwrap();
            let back: PressureSignal = serde_json::from_str(&j).unwrap();
            assert_eq!(*sig, back);
            assert!(j.contains("\"kind\":\""), "tag missing: {j}");
        }
    }

    /// What this catches: ThermalSeverity orders Cool < Warm < Hot <
    /// Critical. Cascade thresholds compare directly; if ordering
    /// regresses, "Hot" might compare-less-than "Warm" and the cascade
    /// triggers in the wrong direction.
    #[test]
    fn thermal_severity_ordered() {
        assert!(ThermalSeverity::Cool < ThermalSeverity::Warm);
        assert!(ThermalSeverity::Warm < ThermalSeverity::Hot);
        assert!(ThermalSeverity::Hot < ThermalSeverity::Critical);
    }

    /// What this catches: SpeculationLevel orders Off < Conservative <
    /// Balanced < Aggressive. Cascade drops it down; ordering matters.
    #[test]
    fn speculation_level_ordered() {
        assert!(SpeculationLevel::Off < SpeculationLevel::Conservative);
        assert!(SpeculationLevel::Conservative < SpeculationLevel::Balanced);
        assert!(SpeculationLevel::Balanced < SpeculationLevel::Aggressive);
    }

    /// What this catches: GovernorSnapshot includes the full current
    /// policy. Telemetry consumers (continuum status, dashboards)
    /// expect to deserialize the entire policy from the snapshot.
    #[test]
    fn governor_snapshot_includes_full_policy() {
        let policy = GovernorPolicy {
            policy_version: 1,
            hardware_class: classify_hardware(&mac_m2_air()),
            tier_sizes: TierSizes {
                l1_lora_layers: 2,
                l1_kv_tokens: 2048,
                l2_lora_layers: 4,
                l3_lora_layers: 12,
                l3_engrams: 1024,
            },
            cadence_multipliers: CadenceMultipliers {
                realtime: 1.0,
                delayed: 1.5,
                background: 2.0,
            },
            concurrency_caps: ConcurrencyCaps {
                personas_concurrent: 2,
                inference_lanes: 1,
                foundry_lanes: 0,
                sentinel_lanes: 1,
            },
            speculation_aggressiveness: SpeculationLevel::Conservative,
            consolidation_schedule: ConsolidationSchedule::IdlePluggedIn,
            federation_pull_cadence: FederationCadence {
                pull_cadence_seconds: 600,
            },
            recall_score_weights: RecallScoreWeights {
                semantic: 0.4,
                outcome_history: 0.3,
                recency: 0.1,
                tier_proximity: 0.1,
                provenance_trust: 0.1,
            },
            cascade_step: 0,
            committed_at_ms: 1_715_625_600_000,
        };
        let snapshot = GovernorSnapshot {
            current_policy: policy.clone(),
            cascade_transition_count: 0,
            recent_signals: vec![],
        };
        assert_eq!(snapshot.current_policy, policy);
        let j = serde_json::to_string(&snapshot).unwrap();
        assert!(j.contains("\"currentPolicy\""));
        assert!(j.contains("\"cascadeTransitionCount\""));
        assert!(j.contains("\"recentSignals\""));
    }
}
