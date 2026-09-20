//! Public types for the model resolver.
//!
//! Extracted from `model_resolver.rs` (continuum#1208) so the resolver
//! function and its tests live in `mod.rs` while the type contracts —
//! HwCapabilityTier, residency policy, request/result, error variants —
//! sit in their own readable file. All types re-exported at the parent
//! path; external callers see no API change.

use crate::cognition::adaptive_throughput::TargetSilicon;
use crate::model_registry::types::{Arch, Capability};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use ts_rs::TS;

/// Finer-grained hardware tier than [`TargetSilicon`]. Selects which model
/// VARIANT a host can run, not which physical-budget POOL admission uses.
///
/// Example: `M1Uma8Gb` and `M3UmaProMax` both have
/// `target_silicon == TargetSilicon::UnifiedMemory`, but only the latter
/// can hold a 4B-parameter model alongside a 7B vision model.
///
/// Lane B's lease layer + adaptive_throughput's budgets care about the
/// pool (TargetSilicon). Lane C's resolver cares about the variant
/// (HwCapabilityTier).
///
/// **Closed enum by design.** New hardware classes (RTX 6090 → `Sm130`,
/// M4, future Apple silicon) require an enum-edit + ts-rs regen + an
/// explicit decision on which existing variant — if any — they alias to.
/// There is intentionally no `Other(String)` or wildcard fallback variant:
/// "unknown hardware" silently routing to a default tier hides
/// capacity-mismatch bugs the resolver exists to catch. See Joel's rule
/// on no fallbacks (`docs/architecture/...`). Adding a tier means the
/// caller's hardware probe must produce it AND every match-on-tier site
/// gets a compile error reminding the author to handle it.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/HwCapabilityTier.ts"
)]
pub enum HwCapabilityTier {
    /// No GPU, no NPU. Inference happens on CPU only.
    CpuOnly,
    /// Apple M1, 8GB unified memory. MBA-tier baseline.
    M1Uma8Gb,
    /// Apple M1/M2, 16GB unified memory.
    M1Uma16Gb,
    /// Apple M2/M3 Pro/Max, 32GB+ unified memory.
    M2UmaProMax,
    /// Apple M3 Pro/Max/Ultra, 32GB+ unified memory.
    M3UmaProMax,
    /// Apple M4 Pro/Max/Ultra, 32GB+ unified memory. Adds
    /// Metal 3 tensor-API + AMX matmul accelerators (HW gen 2024).
    /// Throughput ~30% better than M3 on Qwen-7B Q4_K_M.
    M4UmaProMax,
    /// Apple M5 Pro/Max/Ultra, 24-48 GB+ unified memory. Latest
    /// Apple Silicon (2026). Higher memory-bandwidth + improved
    /// Metal driver; Qwen-2.5-14B Q4_K_M comfortably at 24 GB,
    /// 27B at 48 GB. Joel's daily-driver target per
    /// [`docs/planning/INTEL-MAC-PERSONA-STRATEGY.md`].
    M5UmaProMax,
    /// Mac Intel + discrete Metal GPU (AMD Radeon Pro on 2018-2019
    /// MacBookPro15,*). Distinct from Apple Silicon: Metal API works but
    /// the GPU is a discrete card with its own small VRAM budget (e.g.
    /// 4GB on Radeon Pro 560X), no unified memory, Metal 2 only (no
    /// Metal 3 / tensor API). llama.cpp's Metal shaders assume Apple
    /// Silicon's unified-memory addressing and produce garbled tokens
    /// on this path (continuum 2026-05-30 evidence: 0.8 tok/s + nil
    /// tensor buffers on MacBookPro15,1 / Radeon Pro 560X). Standard
    /// personas on this tier must downsize to the smallest GGUF that
    /// fits CPU-only inference until our CambrianTech/llama.cpp fork
    /// patches the Metal-AMD shader path. TargetSilicon for this tier
    /// is `Gpu` (discrete VRAM, not unified) — but in PRACTICE the
    /// resolver should be conservative and prefer CPU lanes until the
    /// fork patch lands.
    MacIntelMetalDiscrete,
    /// nVidia compute capability 6.x (Pascal — GTX 10xx series:
    /// 1080 Ti, 1080, 1070 Ti, etc.; Tesla P100). Two generations
    /// behind Ampere; no tensor cores. Standard transformer
    /// inference works via llama.cpp's CUDA backend; smaller VRAM
    /// budgets (11 GB on 1080 Ti) constrain model size to Qwen-7B
    /// class at Q4_K_M. Joel's "older desktop still in use" daily
    /// target per the strategy doc.
    Sm60,
    /// nVidia compute capability 7.0 (V100).
    Sm70,
    /// nVidia compute capability 7.5 (T4 datacenter, RTX 20xx, GTX 16xx).
    /// Common on cloud GPU inference instances.
    Sm75,
    /// nVidia compute capability 8.0 (A100).
    Sm80,
    /// nVidia compute capability 8.6 (RTX 30xx, A40).
    Sm86,
    /// nVidia compute capability 8.9 (RTX 40xx).
    Sm89,
    /// nVidia compute capability 9.0 (H100).
    Sm90,
    /// nVidia compute capability 10.0 (Blackwell datacenter B100/B200,
    /// HBM3e). Distinct from `Sm120` — Blackwell-consumer (RTX 50xx) and
    /// Blackwell-datacenter take different driver paths.
    Sm100,
    /// nVidia compute capability 12.0 (RTX 50xx Blackwell-consumer).
    Sm120,
    /// AMD GPU via Vulkan backend.
    VulkanAmd,
    /// Remote inference — host capability irrelevant.
    Cloud,
}

/// Where the resolved model is allowed to physically run. Enforces the
/// alpha sensory bar's "no silent CPU fallback" rule (PR #1072,
/// `docs/architecture/SENSORY-PERSONA-ALPHA-CONTRACT.md`, memory:
/// `project_continuum_alpha_product_bar_sensory_personas.md`).
///
/// Standard personas use [`Self::GpuOrUnifiedMemoryOnly`]; the resolver
/// REJECTS any candidate whose [`TargetSilicon`] would land on CPU, Cloud
/// (when local was preferred), Network, Disk, or Background. Tests and
/// non-alpha-path callers use [`Self::AnySilicon`] — and must justify it
/// in code review.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/SiliconResidencyRequirement.ts"
)]
pub enum SiliconResidencyRequirement {
    /// Standard alpha bar: model MUST run on GPU or UnifiedMemory. Any
    /// other silicon (Cpu, Cloud, Network, Disk, Background) triggers
    /// [`ResolutionError::SiliconResidencyViolated`] with the rejected
    /// model id and the silicon the resolver would have produced.
    GpuOrUnifiedMemoryOnly,
    /// Caller accepts any silicon. Used by tests and adapter/compat paths
    /// that explicitly opt out of the bar. Standard personas MUST NOT use
    /// this — they go through [`ModelRequirement::standard_persona`].
    AnySilicon,
}

impl SiliconResidencyRequirement {
    /// True when `silicon` is in the allowed set for this requirement.
    pub fn allows(self, silicon: TargetSilicon) -> bool {
        match self {
            Self::GpuOrUnifiedMemoryOnly => {
                matches!(silicon, TargetSilicon::Gpu | TargetSilicon::UnifiedMemory)
            }
            Self::AnySilicon => true,
        }
    }
}

/// How aggressively to prefer local vs cloud providers.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/LocalOrCloudPolicy.ts"
)]
pub enum LocalOrCloudPolicy {
    /// Match local providers only. Cloud models are filtered out.
    LocalOnly,
    /// Match cloud providers only. Local models are filtered out.
    CloudOnly,
    /// Both eligible; rank local higher in the result.
    PreferLocal,
    /// Both eligible; rank cloud higher in the result.
    PreferCloud,
    /// Both eligible; no ranking preference.
    Any,
}

/// What the resolver knows about THIS machine. Caller populates from a
/// hardware-detection probe at boot (see future `device_probe` module).
/// The resolver consumes this as a snapshot — re-invoke when probe values
/// change.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/HostCapability.ts"
)]
pub struct HostCapability {
    pub hw_capability_tier: HwCapabilityTier,
    /// Memory available for inference workloads in megabytes. For unified-
    /// memory hosts this is the share inference is willing to claim, not
    /// total system RAM.
    pub available_memory_mb: u32,
    /// Which physical-budget pool inference workloads on this host should
    /// admit against. Mac M-series → `UnifiedMemory`; nVidia → `Gpu`;
    /// CPU-only → `Cpu`.
    pub primary_target_silicon: TargetSilicon,
}

/// Capability-shaped query for the resolver. Callers describe what the
/// model needs to DO (generate text, see images, etc.) — not which model
/// to use. Per Joel's axiom: code knows ARCHETYPES, models are data.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/ModelRequirement.ts"
)]
pub struct ModelRequirement {
    /// Capabilities every candidate must advertise. Empty set matches any
    /// model (rare — usually callers want at least `Chat`). Standard-persona
    /// callers should use [`Self::standard_persona`] which bundles the
    /// sensory capability set required by the alpha bar.
    pub required_capabilities: BTreeSet<Capability>,
    /// Architectural family preference. Empty = any architecture qualifies.
    /// When non-empty, candidates outside the preference are filtered out
    /// rather than down-ranked — caller wants this family or none.
    #[serde(default)]
    pub arch_preference: Vec<Arch>,
    /// Minimum context window in tokens. `0` = any.
    #[serde(default)]
    pub context_window_min: u32,
    /// Local-vs-cloud preference. See [`LocalOrCloudPolicy`].
    pub provider_policy: LocalOrCloudPolicy,
    /// Host capability snapshot. See [`HostCapability`].
    pub host: HostCapability,
    /// Where the resolved model must physically run. Standard personas
    /// require [`SiliconResidencyRequirement::GpuOrUnifiedMemoryOnly`]; the
    /// resolver REJECTS any model whose silicon would violate this. No
    /// silent CPU fallback. No silent Cloud fallback under preference for
    /// local. See [`SiliconResidencyRequirement`].
    pub silicon_residency: SiliconResidencyRequirement,
}

impl ModelRequirement {
    /// The alpha sensory bar — NO COMPROMISE. Bundles the multimodal
    /// capability set (Chat + Vision + AudioInput + AudioOutput) and the
    /// GPU/UnifiedMemory residency requirement. Local providers are
    /// preferred; cloud is acceptable only if no local model satisfies the
    /// bar (operator can opt for [`LocalOrCloudPolicy::LocalOnly`]
    /// explicitly via [`Self::standard_persona_local_only`]).
    ///
    /// PR #1072 (sensory persona alpha contract):
    /// `docs/architecture/SENSORY-PERSONA-ALPHA-CONTRACT.md`. Memory:
    /// `project_continuum_alpha_product_bar_sensory_personas.md`.
    /// Joel 2026-05-11: "every standard persona has sensory I/O and
    /// WebRTC presence; text-only is a compatibility mode, not the
    /// product. — never forget this. NO COMPROMISE."
    pub fn standard_persona(host: HostCapability) -> Self {
        Self {
            required_capabilities: [
                Capability::Chat,
                Capability::Vision,
                Capability::AudioInput,
                Capability::AudioOutput,
            ]
            .into_iter()
            .collect(),
            arch_preference: vec![],
            context_window_min: 0,
            provider_policy: LocalOrCloudPolicy::PreferLocal,
            host,
            silicon_residency: SiliconResidencyRequirement::GpuOrUnifiedMemoryOnly,
        }
    }

    /// Strict variant of [`Self::standard_persona`]: local providers ONLY.
    /// Use when the persona must not fall through to cloud. Useful for
    /// air-gapped deployments and the M-series default install path.
    pub fn standard_persona_local_only(host: HostCapability) -> Self {
        let mut req = Self::standard_persona(host);
        req.provider_policy = LocalOrCloudPolicy::LocalOnly;
        req
    }
}

/// Resolver output. Includes the silicon target so the caller can plumb it
/// straight into a [`ThroughputJob`] without re-deriving it from the
/// model + host.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/ResolvedModel.ts"
)]
pub struct ResolvedModel {
    pub model_id: String,
    pub provider_id: String,
    /// Expected memory footprint in megabytes if the registry knows it.
    /// `None` for cloud models (always-fits) and for local models whose
    /// row in the Rust catalog (catalog.rs) doesn't yet declare a memory estimate. A
    /// follow-up adds an `estimated_memory_mb` field to the Model schema;
    /// until then memory-budget filtering is best-effort on local models
    /// (the resolver still rejects cloud models from `LocalOnly` queries).
    #[ts(optional)]
    pub expected_memory_mb: Option<u32>,
    pub target_silicon: TargetSilicon,
    pub hw_capability_tier: HwCapabilityTier,
    /// Human-readable explanation of why this model was chosen. Surfaced
    /// in logs + UI when a persona's resolution changes (e.g., "switched
    /// from gpt-4o to claude-sonnet-4-5 because PreferLocal couldn't
    /// satisfy required Capability::Vision on this host").
    pub reason: String,
}

/// Why a [`super::resolve_model`] call failed. Each variant names the
/// SPECIFIC filter that eliminated all candidates so the caller's error
/// message can be actionable.
///
/// No `Fallback` variant. Per Joel's rule: missing-model is an error, not
/// a soft retry on a default. Callers that want graceful degradation must
/// EXPLICITLY relax their requirement and re-invoke.
#[derive(Debug, Clone, Serialize, Deserialize, TS, thiserror::Error)]
#[serde(rename_all = "camelCase", tag = "kind")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/ResolutionError.ts"
)]
pub enum ResolutionError {
    #[error(
        "no model satisfies requirement: {registry_count} models in registry, \
         {candidates_after_filter} survived filtering. unmet: {unmet_filters:?}"
    )]
    NoModelMatchesRequirement {
        registry_count: usize,
        candidates_after_filter: usize,
        unmet_filters: Vec<String>,
    },
    /// Standard-persona resolution failed because no model in the registry
    /// satisfies the bundled multimodal capability bar (Chat + Vision +
    /// AudioInput + AudioOutput together). This names the FORGE GAP
    /// directly: ship a multimodal base model for this hardware tier. It
    /// is NOT a config bug — relaxing the bar is forbidden per the alpha
    /// product contract (PR #1072,
    /// `project_continuum_alpha_product_bar_sensory_personas.md`).
    #[error(
        "no multimodal base in registry: {registry_count} models, but none satisfy \
         the sensory bar {required_sensory_capabilities:?}. forge a multimodal base \
         for this tier — text-only models are not the product"
    )]
    NoMultimodalBase {
        registry_count: usize,
        required_sensory_capabilities: Vec<String>,
    },
    /// Standard-persona resolution found a model but its physical silicon
    /// (CPU, Cloud, Network, Disk, etc.) violates the caller's silicon
    /// residency requirement. Loud-fail surfaces the model that WOULD have
    /// been picked + the silicon it would have run on, so operators can
    /// decide between (a) fixing the host (e.g., enable GPU), (b) shipping
    /// a smaller model that fits the host's GPU/UnifiedMemory, or (c)
    /// explicitly opting out of the bar via `AnySilicon` (which standard
    /// personas may not do).
    #[error(
        "silicon residency violated: model `{rejected_model_id}` would run on \
         {actual_silicon:?} but requirement allows only GPU / unified-memory. \
         no silent CPU or cloud fallback under the alpha bar."
    )]
    SiliconResidencyViolated {
        rejected_model_id: String,
        actual_silicon: TargetSilicon,
    },
}
