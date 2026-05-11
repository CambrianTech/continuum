//! Model resolver — capability-shaped model selection.
//!
//! Pure contract for "given a ModelRequirement, which concrete model_id
//! satisfies it on this host?" Does not load models, initialize backends,
//! or call providers. Does not invent fallbacks: a requirement that cannot
//! be satisfied returns a typed [`ResolutionError`], not a best-guess model.
//!
//! Per Joel's rule (`fallbacks are illegal`): callers handle the error
//! explicitly. There is no fall-through to a base model — that turns silent
//! capability mismatches into runtime failures downstream.
//!
//! The resolver is the lookup half of the Adaptive Throughput Substrate.
//! `adaptive_throughput` plans LANES; this module picks WHICH MODEL fills
//! a given lane's request. The two share [`TargetSilicon`] as the join
//! key — `ResolvedModel.target_silicon` flows into
//! `ThroughputJob.target_silicon` when the resolver's output is admitted.
//!
//! Symmetrical to `adaptive_throughput.rs`: pure planner, callers re-invoke
//! when host capabilities change (e.g., another model evicted, GPU
//! pressure shifted).
//!
//! Source-of-truth ordering for model data: this module reads Models from
//! the typed registry (`crate::model_registry`). It does NOT itself read
//! `models.toml` or `models.json` — the registry already loaded both.

use crate::cognition::adaptive_throughput::TargetSilicon;
use crate::model_registry::types::{Arch, Capability, Model, Provider, ProviderKind};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
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
    export_to = "../../../shared/generated/cognition/HwCapabilityTier.ts"
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
    export_to = "../../../shared/generated/cognition/SiliconResidencyRequirement.ts"
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
    export_to = "../../../shared/generated/cognition/LocalOrCloudPolicy.ts"
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
    export_to = "../../../shared/generated/cognition/HostCapability.ts"
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
    export_to = "../../../shared/generated/cognition/ModelRequirement.ts"
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
    export_to = "../../../shared/generated/cognition/ResolvedModel.ts"
)]
pub struct ResolvedModel {
    pub model_id: String,
    pub provider_id: String,
    /// Expected memory footprint in megabytes if the registry knows it.
    /// `None` for cloud models (always-fits) and for local models whose
    /// row in `models.toml` doesn't yet declare a memory estimate. A
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

/// Why a [`resolve_model`] call failed. Each variant names the SPECIFIC
/// filter that eliminated all candidates so the caller's error message
/// can be actionable.
///
/// No `Fallback` variant. Per Joel's rule: missing-model is an error, not
/// a soft retry on a default. Callers that want graceful degradation must
/// EXPLICITLY relax their requirement and re-invoke.
#[derive(Debug, Clone, Serialize, Deserialize, TS, thiserror::Error)]
#[serde(rename_all = "camelCase", tag = "kind")]
#[ts(
    export,
    export_to = "../../../shared/generated/cognition/ResolutionError.ts"
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

fn derive_target_silicon(
    model: &Model,
    provider_kinds: &HashMap<&str, ProviderKind>,
    host: &HostCapability,
) -> TargetSilicon {
    let kind = provider_kinds
        .get(model.provider.as_str())
        .copied()
        .unwrap_or_default(); // ProviderKind::Cloud — unknown provider treated as cloud
    match kind {
        ProviderKind::Local => host.primary_target_silicon,
        ProviderKind::Cloud => TargetSilicon::Cloud,
    }
}

/// Resolve a [`ModelRequirement`] against a model catalog + provider table.
/// Pure: caller supplies iterators of [`Model`] and [`Provider`] (typically
/// `registry.models()` and `registry.providers()`).
///
/// Filter order (each step records the unmet predicate when it eliminates
/// the last candidate, so the error names the specific cause):
/// 1. `required_capabilities` — every cap must be advertised. When the
///    requirement included the multimodal sensory bundle (Vision +
///    AudioInput) and no model satisfies, errors with
///    [`ResolutionError::NoMultimodalBase`] (forge gap, not config bug).
/// 2. `arch_preference` — when non-empty, must match
/// 3. `context_window_min` — model's window ≥ requirement
/// 4. `provider_policy` — Local/Cloud filter, keyed on the provider's
///    [`ProviderKind`] (no hardcoded provider-id list — providers declare
///    their own residency in `providers.toml`)
/// 5. `silicon_residency` — after the best candidate is ranked and its
///    target silicon derived, reject if the silicon violates the caller's
///    residency requirement. Enforces the alpha bar's no-silent-CPU
///    rule. Errors with [`ResolutionError::SiliconResidencyViolated`].
///
/// Returns the first survivor under the policy's ranking. `PreferLocal`
/// puts local providers first; `PreferCloud` puts cloud providers first;
/// other policies preserve registry order.
pub fn resolve_model<'a, M, P>(
    requirement: &ModelRequirement,
    models: M,
    providers: P,
) -> Result<ResolvedModel, ResolutionError>
where
    M: IntoIterator<Item = &'a Model>,
    P: IntoIterator<Item = &'a Provider>,
{
    let provider_kinds: HashMap<&str, ProviderKind> = providers
        .into_iter()
        .map(|p| (p.id.as_str(), p.kind))
        .collect();
    let is_local = |provider_id: &str| {
        provider_kinds.get(provider_id).copied().unwrap_or_default() == ProviderKind::Local
    };

    let registry: Vec<&Model> = models.into_iter().collect();
    let registry_count = registry.len();
    let mut unmet: Vec<String> = Vec::new();

    // Sensory-bundle queries get routed to NoMultimodalBase when ANY filter
    // empties candidates — capability filter, provider-policy filter,
    // anything. The operator-actionable failure is "no LOCAL multimodal
    // base for this tier," NOT a generic "tighten your filter" message.
    let is_sensory_query = requirement
        .required_capabilities
        .contains(&Capability::Vision)
        && requirement
            .required_capabilities
            .contains(&Capability::AudioInput);
    let no_multimodal_base_err = || ResolutionError::NoMultimodalBase {
        registry_count,
        required_sensory_capabilities: requirement
            .required_capabilities
            .iter()
            .map(|c| format!("{c:?}"))
            .collect(),
    };

    // Filter 1: required capabilities.
    let mut candidates: Vec<&Model> = registry
        .iter()
        .copied()
        .filter(|m| requirement.required_capabilities.iter().all(|c| m.has(*c)))
        .collect();
    if candidates.is_empty() && !requirement.required_capabilities.is_empty() {
        if is_sensory_query {
            return Err(no_multimodal_base_err());
        }
        unmet.push(format!(
            "required_capabilities={:?}",
            requirement.required_capabilities
        ));
        return Err(ResolutionError::NoModelMatchesRequirement {
            registry_count,
            candidates_after_filter: 0,
            unmet_filters: unmet,
        });
    }

    // Filter 2: arch preference.
    if !requirement.arch_preference.is_empty() {
        let after_arch: Vec<&Model> = candidates
            .iter()
            .copied()
            .filter(|m| requirement.arch_preference.contains(&m.arch))
            .collect();
        if after_arch.is_empty() {
            if is_sensory_query {
                return Err(no_multimodal_base_err());
            }
            unmet.push(format!(
                "arch_preference={:?} (no survivor matched)",
                requirement.arch_preference
            ));
            return Err(ResolutionError::NoModelMatchesRequirement {
                registry_count,
                candidates_after_filter: 0,
                unmet_filters: unmet,
            });
        }
        candidates = after_arch;
    }

    // Filter 3: context window minimum.
    if requirement.context_window_min > 0 {
        let before = candidates.len();
        candidates.retain(|m| m.context_window >= requirement.context_window_min);
        if candidates.is_empty() {
            if is_sensory_query {
                return Err(no_multimodal_base_err());
            }
            unmet.push(format!(
                "context_window_min={} (eliminated {} candidates)",
                requirement.context_window_min, before
            ));
            return Err(ResolutionError::NoModelMatchesRequirement {
                registry_count,
                candidates_after_filter: 0,
                unmet_filters: unmet,
            });
        }
    }

    // Filter 4: provider policy.
    let before_provider = candidates.len();
    candidates.retain(|m| match requirement.provider_policy {
        LocalOrCloudPolicy::LocalOnly => is_local(&m.provider),
        LocalOrCloudPolicy::CloudOnly => !is_local(&m.provider),
        LocalOrCloudPolicy::PreferLocal
        | LocalOrCloudPolicy::PreferCloud
        | LocalOrCloudPolicy::Any => true,
    });
    if candidates.is_empty() {
        if is_sensory_query {
            return Err(no_multimodal_base_err());
        }
        unmet.push(format!(
            "provider_policy={:?} (eliminated {} candidates)",
            requirement.provider_policy, before_provider
        ));
        return Err(ResolutionError::NoModelMatchesRequirement {
            registry_count,
            candidates_after_filter: 0,
            unmet_filters: unmet,
        });
    }

    // Rank: PreferLocal/PreferCloud reorder; other policies preserve order.
    match requirement.provider_policy {
        LocalOrCloudPolicy::PreferLocal => {
            candidates.sort_by_key(|m| u8::from(!is_local(&m.provider)));
        }
        LocalOrCloudPolicy::PreferCloud => {
            candidates.sort_by_key(|m| u8::from(is_local(&m.provider)));
        }
        _ => {}
    }

    let best = candidates.first().expect("non-empty after filters");
    let target_silicon = derive_target_silicon(best, &provider_kinds, &requirement.host);

    // Silicon-residency gate. No silent CPU fallback. No silent Cloud
    // fallback under GpuOrUnifiedMemoryOnly. The check happens AFTER all
    // other filters because we need the resolved model to name in the
    // error — operator wants to know "qwen2-vl-7b would have run on Cpu
    // here" not just "no model matched."
    if !requirement.silicon_residency.allows(target_silicon) {
        return Err(ResolutionError::SiliconResidencyViolated {
            rejected_model_id: best.id.clone(),
            actual_silicon: target_silicon,
        });
    }

    let reason = format!(
        "matched {} required capability(ies) on arch={:?}, context={}, provider={}, policy={:?}",
        requirement.required_capabilities.len(),
        best.arch,
        best.context_window,
        best.provider,
        requirement.provider_policy,
    );

    Ok(ResolvedModel {
        model_id: best.id.clone(),
        provider_id: best.provider.clone(),
        // expected_memory_mb stays None until the Model schema gains an
        // `estimated_memory_mb` field. Not blocking for v1; the
        // LocalOnly/CloudOnly filter already prevents the worst class of
        // mis-routing (running a 7B model on the cloud lane).
        expected_memory_mb: None,
        target_silicon,
        hw_capability_tier: requirement.host.hw_capability_tier,
        reason,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_registry::types::{AuthKind, MultiPartyChatStrategy};

    fn make_model(
        id: &str,
        provider: &str,
        arch: Arch,
        context_window: u32,
        caps: &[Capability],
    ) -> Model {
        Model {
            id: id.into(),
            name: None,
            provider: provider.into(),
            arch,
            context_window,
            max_output_tokens: 4096,
            tokens_per_second: 50.0,
            capabilities: caps.iter().copied().collect(),
            cost_input_per_1k: 0.0,
            cost_output_per_1k: 0.0,
            gguf_hint: None,
            gguf_local_path: None,
            mmproj_local_path: None,
            chat_template: None,
            multi_party_strategy: MultiPartyChatStrategy::default(),
            stop_sequences: vec![],
        }
    }

    fn make_provider(id: &str, kind: ProviderKind) -> Provider {
        Provider {
            id: id.into(),
            name: None,
            base_url: "http://test".into(),
            api_key_env: None,
            default_model: None,
            auth: AuthKind::None,
            model_prefixes: vec![],
            kind,
        }
    }

    fn providers() -> Vec<Provider> {
        vec![
            make_provider("anthropic", ProviderKind::Cloud),
            make_provider("openai", ProviderKind::Cloud),
            make_provider("llamacpp-local", ProviderKind::Local),
        ]
    }

    fn host_m1_8gb() -> HostCapability {
        HostCapability {
            hw_capability_tier: HwCapabilityTier::M1Uma8Gb,
            available_memory_mb: 6144,
            primary_target_silicon: TargetSilicon::UnifiedMemory,
        }
    }

    fn host_rtx5090() -> HostCapability {
        HostCapability {
            hw_capability_tier: HwCapabilityTier::Sm120,
            available_memory_mb: 32768,
            primary_target_silicon: TargetSilicon::Gpu,
        }
    }

    fn host_cpu_only() -> HostCapability {
        HostCapability {
            hw_capability_tier: HwCapabilityTier::CpuOnly,
            available_memory_mb: 8192,
            primary_target_silicon: TargetSilicon::Cpu,
        }
    }

    fn registry() -> Vec<Model> {
        vec![
            make_model(
                "claude-sonnet-4-5-20250929",
                "anthropic",
                Arch::Claude,
                200_000,
                &[
                    Capability::TextGeneration,
                    Capability::Chat,
                    Capability::ToolUse,
                    Capability::Vision,
                    Capability::Streaming,
                ],
            ),
            make_model(
                "gpt-4o",
                "openai",
                Arch::Gpt,
                128_000,
                &[
                    Capability::TextGeneration,
                    Capability::Chat,
                    Capability::Vision,
                    Capability::AudioInput,
                    Capability::AudioOutput,
                ],
            ),
            make_model(
                "continuum-ai/qwen3.5-4b-code-forged-GGUF",
                "llamacpp-local",
                Arch::Qwen35,
                262_144,
                &[
                    Capability::TextGeneration,
                    Capability::Chat,
                    Capability::ToolUse,
                ],
            ),
            make_model(
                "qwen2-vl-7b-instruct",
                "llamacpp-local",
                Arch::Qwen2,
                32_768,
                &[
                    Capability::TextGeneration,
                    Capability::Chat,
                    Capability::Vision,
                ],
            ),
            make_model(
                "qwen2-0.5b-gating",
                "llamacpp-local",
                Arch::Qwen2,
                8_192,
                &[Capability::TextGeneration, Capability::Chat],
            ),
        ]
    }

    fn req_chat_local(host: HostCapability) -> ModelRequirement {
        ModelRequirement {
            required_capabilities: [Capability::Chat].iter().copied().collect(),
            arch_preference: vec![],
            context_window_min: 0,
            provider_policy: LocalOrCloudPolicy::LocalOnly,
            host,
            silicon_residency: SiliconResidencyRequirement::AnySilicon,
        }
    }

    fn req_vision_local(host: HostCapability) -> ModelRequirement {
        ModelRequirement {
            required_capabilities: [Capability::Chat, Capability::Vision]
                .iter()
                .copied()
                .collect(),
            arch_preference: vec![],
            context_window_min: 0,
            provider_policy: LocalOrCloudPolicy::LocalOnly,
            host,
            silicon_residency: SiliconResidencyRequirement::AnySilicon,
        }
    }

    #[test]
    fn local_chat_resolves_to_qwen35_on_m1() {
        let r = registry();
        let resolved =
            resolve_model(&req_chat_local(host_m1_8gb()), r.iter(), providers().iter()).unwrap();
        assert_eq!(resolved.provider_id, "llamacpp-local");
        assert_eq!(
            resolved.model_id,
            "continuum-ai/qwen3.5-4b-code-forged-GGUF"
        );
        assert_eq!(resolved.target_silicon, TargetSilicon::UnifiedMemory);
        assert_eq!(resolved.hw_capability_tier, HwCapabilityTier::M1Uma8Gb);
    }

    #[test]
    fn vision_request_resolves_to_qwen2_vl() {
        let r = registry();
        let resolved = resolve_model(
            &req_vision_local(host_rtx5090()),
            r.iter(),
            providers().iter(),
        )
        .unwrap();
        assert_eq!(resolved.model_id, "qwen2-vl-7b-instruct");
        assert_eq!(resolved.provider_id, "llamacpp-local");
        assert_eq!(resolved.target_silicon, TargetSilicon::Gpu);
        assert_eq!(resolved.hw_capability_tier, HwCapabilityTier::Sm120);
    }

    #[test]
    fn cloud_only_skips_local_models() {
        let r = registry();
        let mut req = req_chat_local(host_rtx5090());
        req.provider_policy = LocalOrCloudPolicy::CloudOnly;
        let resolved = resolve_model(&req, r.iter(), providers().iter()).unwrap();
        assert!(
            ["anthropic", "openai"].contains(&resolved.provider_id.as_str()),
            "expected cloud provider, got {}",
            resolved.provider_id,
        );
        assert_eq!(resolved.target_silicon, TargetSilicon::Cloud);
    }

    #[test]
    fn missing_capability_errors_no_fallback() {
        let r = registry();
        let req = ModelRequirement {
            required_capabilities: [Capability::ImageGeneration].iter().copied().collect(),
            arch_preference: vec![],
            context_window_min: 0,
            provider_policy: LocalOrCloudPolicy::Any,
            host: host_rtx5090(),
            silicon_residency: SiliconResidencyRequirement::AnySilicon,
        };
        let err = resolve_model(&req, r.iter(), providers().iter()).unwrap_err();
        match err {
            ResolutionError::NoModelMatchesRequirement {
                registry_count,
                candidates_after_filter,
                unmet_filters,
            } => {
                assert_eq!(registry_count, r.len());
                assert_eq!(candidates_after_filter, 0);
                assert!(
                    unmet_filters.iter().any(|f| f.contains("ImageGeneration")),
                    "unmet filters should name ImageGeneration: {unmet_filters:?}"
                );
            }
            other => panic!("expected NoModelMatchesRequirement; got {other:?}"),
        }
    }

    #[test]
    fn vision_with_local_only_on_cpu_host_still_finds_local_vision_model() {
        // Even on a CPU-only host, the resolver should return the local
        // vision model — admission/feasibility is the substrate's job
        // (adaptive_throughput will refuse the lane if the host can't
        // run it). The resolver answers "what fits the requirement,"
        // not "what will succeed at inference time."
        let r = registry();
        let resolved = resolve_model(
            &req_vision_local(host_cpu_only()),
            r.iter(),
            providers().iter(),
        )
        .unwrap();
        assert_eq!(resolved.model_id, "qwen2-vl-7b-instruct");
        assert_eq!(resolved.target_silicon, TargetSilicon::Cpu);
        assert_eq!(resolved.hw_capability_tier, HwCapabilityTier::CpuOnly);
    }

    #[test]
    fn context_window_min_filters_small_models() {
        let r = registry();
        let req = ModelRequirement {
            required_capabilities: [Capability::Chat].iter().copied().collect(),
            arch_preference: vec![],
            context_window_min: 100_000,
            provider_policy: LocalOrCloudPolicy::LocalOnly,
            host: host_rtx5090(),
            silicon_residency: SiliconResidencyRequirement::AnySilicon,
        };
        let resolved = resolve_model(&req, r.iter(), providers().iter()).unwrap();
        // Only qwen3.5-4b (262144 ctx) survives among local with ≥100k window.
        assert_eq!(
            resolved.model_id,
            "continuum-ai/qwen3.5-4b-code-forged-GGUF"
        );
    }

    #[test]
    fn arch_preference_filters_to_qwen35_only() {
        let r = registry();
        let req = ModelRequirement {
            required_capabilities: [Capability::Chat].iter().copied().collect(),
            arch_preference: vec![Arch::Qwen35],
            context_window_min: 0,
            provider_policy: LocalOrCloudPolicy::Any,
            host: host_rtx5090(),
            silicon_residency: SiliconResidencyRequirement::AnySilicon,
        };
        let resolved = resolve_model(&req, r.iter(), providers().iter()).unwrap();
        assert_eq!(
            resolved.model_id,
            "continuum-ai/qwen3.5-4b-code-forged-GGUF"
        );
    }

    #[test]
    fn prefer_local_ranks_local_first() {
        let r = registry();
        let req = ModelRequirement {
            required_capabilities: [Capability::Chat, Capability::Vision]
                .iter()
                .copied()
                .collect(),
            arch_preference: vec![],
            context_window_min: 0,
            provider_policy: LocalOrCloudPolicy::PreferLocal,
            host: host_rtx5090(),
            silicon_residency: SiliconResidencyRequirement::AnySilicon,
        };
        let resolved = resolve_model(&req, r.iter(), providers().iter()).unwrap();
        assert_eq!(resolved.provider_id, "llamacpp-local");
        assert_eq!(resolved.model_id, "qwen2-vl-7b-instruct");
    }

    #[test]
    fn prefer_cloud_ranks_cloud_first() {
        let r = registry();
        let req = ModelRequirement {
            required_capabilities: [Capability::Chat, Capability::Vision]
                .iter()
                .copied()
                .collect(),
            arch_preference: vec![],
            context_window_min: 0,
            provider_policy: LocalOrCloudPolicy::PreferCloud,
            host: host_rtx5090(),
            silicon_residency: SiliconResidencyRequirement::AnySilicon,
        };
        let resolved = resolve_model(&req, r.iter(), providers().iter()).unwrap();
        assert!(
            ["anthropic", "openai"].contains(&resolved.provider_id.as_str()),
            "expected cloud first, got {}",
            resolved.provider_id,
        );
    }

    #[test]
    fn provider_kind_drives_local_classification_not_id() {
        // Confirms the LOCAL_PROVIDER_IDS hardcoding is gone — Provider's
        // kind field is what decides Local vs Cloud. Construct a custom
        // provider whose id has nothing to do with the old hardcoded set.
        let models = vec![make_model(
            "custom-local-model",
            "custom-local-provider",
            Arch::Llama,
            8192,
            &[Capability::Chat],
        )];
        let providers = vec![make_provider("custom-local-provider", ProviderKind::Local)];
        let req = req_chat_local(host_m1_8gb());
        let resolved = resolve_model(&req, models.iter(), providers.iter()).unwrap();
        assert_eq!(resolved.model_id, "custom-local-model");
        assert_eq!(resolved.target_silicon, TargetSilicon::UnifiedMemory);
    }

    #[test]
    fn unknown_provider_defaults_to_cloud_for_safety() {
        // If a model references a provider id that isn't in the providers
        // table at all, the resolver treats it as Cloud (default kind).
        // This is loud: a LocalOnly query will reject the model rather
        // than silently routing unknown-residency work to local hardware.
        let models = vec![make_model(
            "orphan-model",
            "orphan-provider",
            Arch::Llama,
            8192,
            &[Capability::Chat],
        )];
        let providers: Vec<Provider> = vec![];
        let req = req_chat_local(host_m1_8gb());
        let err = resolve_model(&req, models.iter(), providers.iter()).unwrap_err();
        assert!(
            matches!(err, ResolutionError::NoModelMatchesRequirement { .. }),
            "LocalOnly with unknown provider must error, not silently treat as local"
        );
    }

    #[test]
    fn five_persona_resolution_smoke() {
        // Lane C contract test: 5 personas with different needs all
        // resolve to the correct concrete model + missing path errors.
        let r = registry();

        // Persona 1: Helper AI — local chat.
        let helper =
            resolve_model(&req_chat_local(host_m1_8gb()), r.iter(), providers().iter()).unwrap();
        assert_eq!(helper.provider_id, "llamacpp-local");

        // Persona 2: Vision AI — local vision.
        let vision = resolve_model(
            &req_vision_local(host_m1_8gb()),
            r.iter(),
            providers().iter(),
        )
        .unwrap();
        assert_eq!(vision.model_id, "qwen2-vl-7b-instruct");

        // Persona 3: Cloud-only persona — wants vision via cloud.
        let mut cloud_vision_req = req_vision_local(host_m1_8gb());
        cloud_vision_req.provider_policy = LocalOrCloudPolicy::CloudOnly;
        let cloud_vision = resolve_model(&cloud_vision_req, r.iter(), providers().iter()).unwrap();
        assert!(
            ["anthropic", "openai"].contains(&cloud_vision.provider_id.as_str()),
            "expected cloud, got {}",
            cloud_vision.provider_id,
        );

        // Persona 4: Audio-input persona on cloud only (no local audio model
        // in registry — should resolve to gpt-4o which has audio-input).
        let mut audio_req = req_chat_local(host_rtx5090());
        audio_req.required_capabilities = [Capability::Chat, Capability::AudioInput]
            .iter()
            .copied()
            .collect();
        audio_req.provider_policy = LocalOrCloudPolicy::Any;
        let audio = resolve_model(&audio_req, r.iter(), providers().iter()).unwrap();
        assert_eq!(audio.model_id, "gpt-4o");

        // Persona 5: Code persona requiring tool-use — qwen3.5 OR claude.
        let mut code_req = req_chat_local(host_rtx5090());
        code_req.required_capabilities = [Capability::Chat, Capability::ToolUse]
            .iter()
            .copied()
            .collect();
        code_req.provider_policy = LocalOrCloudPolicy::PreferLocal;
        let code = resolve_model(&code_req, r.iter(), providers().iter()).unwrap();
        assert_eq!(code.provider_id, "llamacpp-local");
        assert_eq!(code.model_id, "continuum-ai/qwen3.5-4b-code-forged-GGUF");

        // Missing-model error path: persona requires ImageGeneration which
        // none of the registered models advertise. Must error, not fall
        // back.
        let img_req = ModelRequirement {
            required_capabilities: [Capability::ImageGeneration].iter().copied().collect(),
            arch_preference: vec![],
            context_window_min: 0,
            provider_policy: LocalOrCloudPolicy::Any,
            host: host_rtx5090(),
            silicon_residency: SiliconResidencyRequirement::AnySilicon,
        };
        assert!(
            matches!(
                resolve_model(&img_req, r.iter(), providers().iter()),
                Err(ResolutionError::NoModelMatchesRequirement { .. })
            ),
            "missing capability must error, not fall back"
        );
    }

    // ─── Standard-persona sensory bar (PR #1072) ────────────────────────
    //
    // These tests pin the alpha contract: every standard persona resolution
    // must satisfy the multimodal capability bundle AND land on GPU /
    // UnifiedMemory silicon. NO COMPROMISE.

    #[test]
    fn standard_persona_constructor_bundles_the_alpha_bar() {
        let req = ModelRequirement::standard_persona(host_m1_8gb());
        assert!(req.required_capabilities.contains(&Capability::Chat));
        assert!(req.required_capabilities.contains(&Capability::Vision));
        assert!(req.required_capabilities.contains(&Capability::AudioInput));
        assert!(req.required_capabilities.contains(&Capability::AudioOutput));
        assert_eq!(req.silicon_residency, SiliconResidencyRequirement::GpuOrUnifiedMemoryOnly);
        assert_eq!(req.provider_policy, LocalOrCloudPolicy::PreferLocal);
    }

    #[test]
    fn standard_persona_local_only_constructor_locks_provider_policy() {
        let req = ModelRequirement::standard_persona_local_only(host_m1_8gb());
        assert_eq!(req.provider_policy, LocalOrCloudPolicy::LocalOnly);
        // Bar fields still bundled.
        assert!(req.required_capabilities.contains(&Capability::Vision));
        assert_eq!(req.silicon_residency, SiliconResidencyRequirement::GpuOrUnifiedMemoryOnly);
    }

    #[test]
    fn current_registry_state_fails_alpha_bar_naming_the_forge_gap() {
        // The current test registry mirrors today's models.toml: qwen3.5-4b
        // has Chat+ToolUse but no Vision/Audio. qwen2-vl-7b has Chat+Vision
        // but no Audio. gpt-4o has the full sensory bundle but is CLOUD.
        // No LOCAL multimodal base = the forge gap PR #1072 names. This
        // test will start passing differently when the registry adds a true
        // multimodal local base — at that point update it to assert success.
        let r = registry();
        let p = providers();
        let req = ModelRequirement::standard_persona_local_only(host_m1_8gb());
        let err = resolve_model(&req, r.iter(), p.iter()).unwrap_err();
        match err {
            ResolutionError::NoMultimodalBase {
                registry_count,
                required_sensory_capabilities,
            } => {
                assert_eq!(registry_count, r.len());
                assert!(
                    required_sensory_capabilities.iter().any(|c| c == "Vision"),
                    "error must name Vision capability: {required_sensory_capabilities:?}"
                );
                assert!(
                    required_sensory_capabilities.iter().any(|c| c == "AudioInput"),
                    "error must name AudioInput capability: {required_sensory_capabilities:?}"
                );
            }
            other => panic!(
                "expected NoMultimodalBase (forge gap); got {other:?}. \
                 If this fired NoModelMatchesRequirement instead, the filter-1 \
                 distinguish-the-sensory-bundle logic regressed."
            ),
        }
    }

    #[test]
    fn standard_persona_resolves_when_multimodal_local_base_exists() {
        // Synthetic registry: add a true multimodal local base to prove
        // the resolver SELECTS it under StandardPersona. This is what the
        // forge pipeline (Position 3) eventually delivers.
        let mut r = registry();
        r.push(make_model(
            "synthetic-qwen3.5-multimodal-7b",
            "llamacpp-local",
            Arch::Qwen35,
            32_768,
            &[
                Capability::Chat,
                Capability::Vision,
                Capability::AudioInput,
                Capability::AudioOutput,
            ],
        ));
        let p = providers();
        let req = ModelRequirement::standard_persona_local_only(host_m1_8gb());
        let resolved = resolve_model(&req, r.iter(), p.iter()).unwrap();
        assert_eq!(resolved.model_id, "synthetic-qwen3.5-multimodal-7b");
        assert_eq!(resolved.target_silicon, TargetSilicon::UnifiedMemory);
        assert_eq!(resolved.hw_capability_tier, HwCapabilityTier::M1Uma8Gb);
    }

    #[test]
    fn standard_persona_rejects_cpu_silicon_no_silent_fallback() {
        // CPU-only host with a multimodal local model present: capabilities
        // match, provider matches (local), but silicon would be Cpu —
        // SiliconResidencyViolated must fire. No silent CPU fallback.
        let mut r = registry();
        r.push(make_model(
            "synthetic-multimodal-cpu-rejected",
            "llamacpp-local",
            Arch::Qwen35,
            32_768,
            &[
                Capability::Chat,
                Capability::Vision,
                Capability::AudioInput,
                Capability::AudioOutput,
            ],
        ));
        let p = providers();
        let req = ModelRequirement::standard_persona_local_only(host_cpu_only());
        let err = resolve_model(&req, r.iter(), p.iter()).unwrap_err();
        match err {
            ResolutionError::SiliconResidencyViolated {
                rejected_model_id,
                actual_silicon,
            } => {
                assert_eq!(rejected_model_id, "synthetic-multimodal-cpu-rejected");
                assert_eq!(actual_silicon, TargetSilicon::Cpu);
            }
            other => panic!(
                "expected SiliconResidencyViolated on CPU host; got {other:?}. \
                 the silicon-residency gate is supposed to refuse CPU even when \
                 capabilities match."
            ),
        }
    }

    #[test]
    fn standard_persona_rejects_cloud_silicon_under_gpu_residency_with_prefer_local_fallback() {
        // PreferLocal + no local multimodal base: today the resolver would
        // rank cloud second and pick gpt-4o (which has the sensory bundle).
        // Under StandardPersona's GpuOrUnifiedMemoryOnly bar, that cloud
        // model resolves to TargetSilicon::Cloud which violates the
        // residency requirement. Loud-fail: SiliconResidencyViolated names
        // the cloud model that WOULD have been picked. Operator's choices:
        // (a) ship a local multimodal base, (b) explicitly opt for
        // CloudOnly + AnySilicon (not via StandardPersona).
        //
        // NOTE: today the registry has gpt-4o as the only model with all 4
        // sensory caps. With PreferLocal, no local match, gpt-4o wins
        // ranking — and then silicon-residency rejects it.
        let r = registry();
        let p = providers();
        let req = ModelRequirement::standard_persona(host_m1_8gb());
        let err = resolve_model(&req, r.iter(), p.iter()).unwrap_err();
        match err {
            ResolutionError::SiliconResidencyViolated {
                rejected_model_id,
                actual_silicon,
            } => {
                assert_eq!(rejected_model_id, "gpt-4o");
                assert_eq!(actual_silicon, TargetSilicon::Cloud);
            }
            other => panic!(
                "expected SiliconResidencyViolated naming gpt-4o on Cloud silicon; got {other:?}"
            ),
        }
    }
}
