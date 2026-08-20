//! [`FineTuningCoordinator`] — provider selection for fine-tuning
//! jobs. Same shape and intent as `InferenceCoordinator` (#109) does
//! for inference: the persona never names a provider; the substrate
//! picks based on capabilities + observed pressure + cost + trust.
//!
//! ## Selection algorithm
//!
//! 1. **Filter** by hard capability matches:
//!    - the adapter's `supported_base_model_prefixes` must contain
//!      a prefix that matches the request's `base_model`, OR the
//!      adapter declares wildcard (empty `Vec`)
//!    - if the request needs LoRA-style training, the adapter must
//!      advertise `supports_lora`
//!    - if the dataset has `validation_split > 0`, the adapter must
//!      advertise `supports_validation`
//! 2. **Honor caller preference** when supplied:
//!    - if `preferred_provider == Some(id)` and that adapter is in
//!      the filtered set, it wins
//!    - if it's NOT in the filtered set, the coordinator returns an
//!      explicit [`CoordinatorError::PreferredUnavailable`]
//!      ([[no-fallbacks-ever]]: the caller wanted X; honoring Y
//!      silently is a fallback)
//! 3. **Otherwise prefer locality**:
//!    - adapters with `produces_local_artifact=true` (local Candle,
//!      future cross-grid airc-routed local-on-peer) win over cloud
//!    - rationale: local training keeps training data on-substrate
//!      (privacy + trust), avoids cloud-provider API rate limits,
//!      and the matrix-dojo doctrine compounds best when layers
//!      stay in-grid.
//! 4. **Final tie-break: alphabetical by provider_id**. Stable, no
//!    hidden ranking. Future work (cost/pressure/reputation
//!    signals) plugs in here as additional comparators BEFORE the
//!    alphabetical fallback.
//!
//! ## Why coordinator-as-data-type, not coordinator-as-trait
//!
//! Selection logic is universal across all `FineTuningAdapter`
//! impls — there's no "OpenAI-specific selector" vs "Candle-specific
//! selector." A concrete struct (not a trait) makes the logic
//! testable in isolation against any combination of adapter
//! capabilities without standing up real HTTP / GPU contexts.

use std::sync::Arc;

use super::adapter::{ArcFineTuningAdapter, FineTuningCapabilities};
use super::registry::FineTuningRegistry;
use super::types::TrainingJobRequest;
use crate::inference_capability::{probe_hardware_profile, HardwareProfile};

/// Why the coordinator couldn't pick an adapter for a given request.
/// Typed so the calling ServiceModule can map each variant to the
/// right user-facing error without parsing strings.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CoordinatorError {
    /// No registered adapter advertises capability for the request's
    /// base model + training shape. Common case: operator hasn't
    /// configured any provider credentials, OR the requested base
    /// isn't supported by any registered cloud trainer AND local
    /// Candle isn't online.
    #[error(
        "no fine-tuning adapter advertises capability for base_model={base_model:?} \
         (registered providers: {registered:?}; supported base prefixes by provider: {supported_prefixes:?})"
    )]
    NoCapableAdapter {
        base_model: String,
        registered: Vec<String>,
        /// Per-provider list of `supported_base_model_prefixes`. Lets
        /// the caller see *exactly* which prefix string would have
        /// routed to which adapter — surfaces wire literals like
        /// `SYNTHETIC_BASE_PREFIX` that callers otherwise have to
        /// discover by reading Rust source. Per Reviewer 2 BLOCK B3:
        /// load-bearing prefix strings need a discoverable contract.
        supported_prefixes: Vec<(String, Vec<String>)>,
    },

    /// The caller specified `preferred_provider` but that adapter
    /// either isn't registered OR doesn't advertise capability for
    /// this request. Per [[no-fallbacks-ever]]: the caller wanted X;
    /// silently picking Y is a fallback. Caller gets a typed
    /// rejection.
    #[error(
        "preferred provider {preferred:?} unavailable for this request \
         (registered: {registered:?}, capable for this base: {capable:?})"
    )]
    PreferredUnavailable {
        preferred: String,
        registered: Vec<String>,
        capable: Vec<String>,
    },
}

/// Provider selector. Stateless beyond the `Arc<FineTuningRegistry>`
/// it consults and the host's [`HardwareProfile`], probed ONCE at
/// construction (struct-carrier: read the rich host capabilities once,
/// hold them, route against them — never re-probe per selection).
/// Construct once at boot, share across all dispatch.
pub struct FineTuningCoordinator {
    registry: Arc<FineTuningRegistry>,
    /// Host accelerator supply, probed once. The coordinator never
    /// routes a job to a trainer whose `requires` accelerator isn't
    /// present here, and ranks a host-native accelerator trainer above
    /// the accelerator-agnostic ones.
    host: HardwareProfile,
}

impl FineTuningCoordinator {
    /// Production constructor — probes the real host hardware once.
    pub fn new(registry: Arc<FineTuningRegistry>) -> Self {
        Self {
            registry,
            host: probe_hardware_profile(),
        }
    }

    /// Construct with an explicit host profile. Lets tests route as if
    /// on an NVIDIA box (or any host) without that hardware present, and
    /// lets a grid scheduler hand in a leased peer's advertised profile
    /// (the cross-grid routing rail).
    pub fn with_host(registry: Arc<FineTuningRegistry>, host: HardwareProfile) -> Self {
        Self { registry, host }
    }

    /// Pick an adapter for this request. Returns the adapter +
    /// the provider id that was chosen (so the caller can log which
    /// provider got picked without re-deriving from the adapter).
    pub fn select(
        &self,
        request: &TrainingJobRequest,
        preferred_provider: Option<&str>,
    ) -> Result<(String, ArcFineTuningAdapter), CoordinatorError> {
        let all_ids = self.registry.list();

        // Compute the capable set once.
        let capable: Vec<(String, ArcFineTuningAdapter, FineTuningCapabilities)> = all_ids
            .iter()
            .filter_map(|id| self.registry.get(id).map(|a| (id.clone(), a)))
            .filter_map(|(id, adapter)| {
                let caps = adapter.capabilities();
                if self.caps_match(&caps, request) {
                    Some((id, adapter, caps))
                } else {
                    None
                }
            })
            .collect();

        // Honor caller preference if it's in the capable set.
        if let Some(pref) = preferred_provider {
            if let Some((id, adapter, _caps)) =
                capable.iter().find(|(id, _, _)| id == pref).cloned()
            {
                return Ok((id, adapter));
            }
            return Err(CoordinatorError::PreferredUnavailable {
                preferred: pref.to_string(),
                registered: all_ids,
                capable: capable.into_iter().map(|(id, _, _)| id).collect(),
            });
        }

        // No preference. Apply the locality + alphabetical
        // comparators.
        let mut ranked = capable;
        ranked.sort_by(|a, b| self.rank(&a.2).cmp(&self.rank(&b.2)).then(a.0.cmp(&b.0)));

        match ranked.into_iter().next() {
            Some((id, adapter, _)) => Ok((id, adapter)),
            None => {
                // Build the supported_prefixes diagnostic — same
                // adapter walk as the capability scan above, but
                // surfaced into the error so the caller learns
                // exactly which prefix string would have routed.
                let supported_prefixes: Vec<(String, Vec<String>)> = all_ids
                    .iter()
                    .filter_map(|id| {
                        self.registry
                            .get(id)
                            .map(|a| (id.clone(), a.capabilities().supported_base_model_prefixes))
                    })
                    .collect();
                Err(CoordinatorError::NoCapableAdapter {
                    base_model: request.base_model.clone(),
                    registered: all_ids,
                    supported_prefixes,
                })
            }
        }
    }

    /// `true` iff the adapter's static capabilities cover this
    /// request's hard requirements AND the host can actually run it.
    /// Soft preferences (cost, pressure) are applied later in `rank`;
    /// this is the binary "can it even run on THIS host?" gate.
    fn caps_match(&self, caps: &FineTuningCapabilities, request: &TrainingJobRequest) -> bool {
        // Hardware gate FIRST — an Apple `mlx-local` trainer on a Linux
        // box, or a CUDA trainer on a Mac, is filtered out before any
        // other consideration. Deterministic match on the host's probed
        // device flags (no string parsing of `platform`).
        if !caps.requires.satisfied_by(&self.host) {
            return false;
        }
        // Wildcard prefix list = adapter validates the actual base
        // on create_job. Treat as "match any base."
        let base_ok = caps.supported_base_model_prefixes.is_empty()
            || caps
                .supported_base_model_prefixes
                .iter()
                .any(|p| request.base_model.starts_with(p));
        if !base_ok {
            return false;
        }
        if request.lora.is_some() && !caps.supports_lora {
            return false;
        }
        if request.dataset.validation_split > 0.0 && !caps.supports_validation {
            return false;
        }
        true
    }

    /// Rank (lower = preferred). Three tiers:
    ///   0 — host-native accelerator trainer (Apple `mlx-local` on a
    ///       Metal host, a CUDA trainer on an NVIDIA host): the fast,
    ///       owned, on-device path.
    ///   1 — accelerator-agnostic local-artifact trainer (Candle).
    ///   2 — everything else (cloud).
    /// This is what makes "Apple→mlx, NVIDIA→cuda, else→generic/cloud"
    /// automatic: a new accelerator trainer that advertises its
    /// `requires` lands in tier 0 on its native host with zero
    /// coordinator change. Future signals (cost, pressure, reputation)
    /// slot in as additional tiers or fractional adjustments here.
    fn rank(&self, caps: &FineTuningCapabilities) -> u8 {
        if caps.requires.is_specific_accelerator() && caps.requires.satisfied_by(&self.host) {
            0
        } else if caps.produces_local_artifact {
            1
        } else {
            2
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::fine_tuning::adapter::{
        FineTuningAdapter, FineTuningCapabilities, FineTuningError, TrainerHardware,
    };
    use crate::genome::fine_tuning::types::{
        JobHandle, TrainingDataset, TrainingSource, TrainingStatus,
    };
    use crate::inference_capability::HardwareProfile;
    use async_trait::async_trait;
    use uuid::Uuid;

    /// Synthesize a host profile with the given accelerator flags. Only
    /// the device flags drive routing; vram/cores/platform are filler.
    fn host(has_metal: bool, has_cuda: bool, has_vulkan: bool) -> HardwareProfile {
        HardwareProfile {
            platform: "test-host".into(),
            has_metal,
            has_cuda,
            has_vulkan,
            free_vram_bytes: 0,
            total_vram_bytes: 0,
            cpu_cores: 8,
            system_ram_bytes: 16 * 1024 * 1024 * 1024,
        }
    }

    /// Like `caps` but with an explicit hardware requirement — for the
    /// platform-routing tests.
    fn caps_hw(
        provider_id: &str,
        produces_local_artifact: bool,
        requires: TrainerHardware,
    ) -> FineTuningCapabilities {
        FineTuningCapabilities {
            provider_id: provider_id.to_string(),
            supports_lora: true,
            supports_validation: true,
            produces_local_artifact,
            supported_base_model_prefixes: vec![],
            requires,
        }
    }

    /// Configurable stub: capabilities are set at construction time
    /// so each test can isolate one selection rule.
    struct StubWithCaps(FineTuningCapabilities);

    #[async_trait]
    impl FineTuningAdapter for StubWithCaps {
        fn capabilities(&self) -> FineTuningCapabilities {
            self.0.clone()
        }
        async fn create_job(&self, _r: TrainingJobRequest) -> Result<JobHandle, FineTuningError> {
            unimplemented!()
        }
        async fn poll(&self, _h: &JobHandle) -> Result<TrainingStatus, FineTuningError> {
            unimplemented!()
        }
        async fn cancel(&self, _h: &JobHandle) -> Result<(), FineTuningError> {
            unimplemented!()
        }
    }

    fn caps(
        provider_id: &str,
        supports_lora: bool,
        supports_validation: bool,
        produces_local_artifact: bool,
        prefixes: &[&str],
    ) -> FineTuningCapabilities {
        FineTuningCapabilities {
            provider_id: provider_id.to_string(),
            supports_lora,
            supports_validation,
            produces_local_artifact,
            supported_base_model_prefixes: prefixes.iter().map(|s| s.to_string()).collect(),
            // Existing tests exercise base-prefix / locality / lora /
            // validation gates with host-agnostic adapters.
            requires: TrainerHardware::Any,
        }
    }

    fn base_request(base_model: &str) -> TrainingJobRequest {
        TrainingJobRequest {
            persona_id: Uuid::nil(),
            persona_name: "test".into(),
            base_model: base_model.into(),
            trait_kind: "test-trait".into(),
            dataset: TrainingDataset {
                examples: vec![],
                source: TrainingSource::OperatorCurated,
                validation_split: 0.0,
            },
            eval_set: None,
            lora: None,
            schedule: None,
            local_artifact_dir: None,
        }
    }

    fn registry_with(adapters: Vec<(&str, FineTuningCapabilities)>) -> Arc<FineTuningRegistry> {
        let reg = Arc::new(FineTuningRegistry::new());
        for (_id, c) in adapters {
            reg.register(Arc::new(StubWithCaps(c)));
        }
        reg
    }

    // what this catches: empty registry must return NoCapableAdapter,
    // not panic or pick a default. The Stage B genome trigger we
    // deleted lied about success when no adapter existed; this test
    // pins the typed-rejection contract.
    #[test]
    fn empty_registry_returns_no_capable_adapter() {
        let coord = FineTuningCoordinator::new(Arc::new(FineTuningRegistry::new()));
        let err = coord
            .select(&base_request("gpt-4o-mini"), None)
            .err()
            .expect("must reject");
        assert!(matches!(err, CoordinatorError::NoCapableAdapter { .. }));
    }

    // what this catches: provider preference is honored when capable.
    // A future refactor that flips this to "best-rank-wins regardless
    // of preference" would silently route operator-locked traffic
    // (e.g. EU-data-residency Mistral) to a non-preferred provider.
    #[test]
    fn preferred_provider_wins_when_capable() {
        let reg = registry_with(vec![
            ("openai", caps("openai", true, true, false, &["gpt-"])),
            ("mistral", caps("mistral", true, true, false, &["mistral-"])),
        ]);
        let coord = FineTuningCoordinator::new(reg);
        let (id, _) = coord
            .select(&base_request("gpt-4o-mini"), Some("openai"))
            .unwrap();
        assert_eq!(id, "openai");
    }

    // what this catches: preferred but not capable → typed
    // PreferredUnavailable error, NOT silent fallback to a capable
    // adapter. This is the no-fallbacks-ever invariant.
    #[test]
    fn preferred_uncapable_returns_typed_error_not_fallback() {
        let reg = registry_with(vec![
            ("openai", caps("openai", true, true, false, &["gpt-"])),
            ("mistral", caps("mistral", true, true, false, &["mistral-"])),
        ]);
        let coord = FineTuningCoordinator::new(reg);
        let err = coord
            .select(&base_request("gpt-4o-mini"), Some("mistral"))
            .err()
            .expect("must reject");
        match err {
            CoordinatorError::PreferredUnavailable {
                preferred, capable, ..
            } => {
                assert_eq!(preferred, "mistral");
                assert_eq!(capable, vec!["openai"]);
            }
            other => panic!("expected PreferredUnavailable, got {other:?}"),
        }
    }

    // what this catches: locality preference. Given equal
    // capabilities, local-artifact-producing adapter (the substrate-
    // native sibling, the matrix-dojo doctrine's preferred path)
    // beats the cloud sibling. A future refactor that flips this
    // would silently route training data to cloud providers when
    // local capacity is available — a privacy / trust regression.
    #[test]
    fn local_artifact_producer_beats_cloud_when_both_capable() {
        let reg = registry_with(vec![
            ("openai", caps("openai", true, true, false, &[])),
            ("local-candle", caps("local-candle", true, true, true, &[])),
        ]);
        let coord = FineTuningCoordinator::new(reg);
        let (id, _) = coord.select(&base_request("gpt-4o-mini"), None).unwrap();
        assert_eq!(id, "local-candle");
    }

    // what this catches: alphabetical tie-break is the final, stable
    // ordering when locality + base-prefix don't discriminate. A
    // future implicit ordering (HashMap iteration order, registration
    // order) would make selection non-deterministic and flaky tests
    // would lie about coordinator behavior.
    #[test]
    fn tie_break_is_alphabetical_within_locality_tier() {
        let reg = registry_with(vec![
            ("mistral", caps("mistral", true, true, false, &[])),
            ("openai", caps("openai", true, true, false, &[])),
            ("anthropic", caps("anthropic", true, true, false, &[])),
        ]);
        let coord = FineTuningCoordinator::new(reg);
        let (id, _) = coord.select(&base_request("gpt-4o-mini"), None).unwrap();
        assert_eq!(id, "anthropic");
    }

    // what this catches: base_model prefix filter. A request for a
    // Mistral base must NOT route to OpenAI just because OpenAI is
    // registered. The capability advertisement is the gate.
    #[test]
    fn base_model_prefix_gates_capability() {
        let reg = registry_with(vec![
            ("openai", caps("openai", true, true, false, &["gpt-"])),
            ("mistral", caps("mistral", true, true, false, &["mistral-"])),
        ]);
        let coord = FineTuningCoordinator::new(reg);
        let (id, _) = coord
            .select(&base_request("mistral-large-latest"), None)
            .unwrap();
        assert_eq!(id, "mistral");
    }

    // what this catches: empty prefix list = wildcard. The local-
    // Candle adapter doesn't know in advance which base models it
    // can train on (depends on what's in the local model cache); it
    // declares wildcard and validates on create_job. A future
    // refactor that interprets empty as "matches nothing" would
    // silently make local Candle unselectable.
    #[test]
    fn empty_prefix_list_means_wildcard_not_zero_match() {
        let reg = registry_with(vec![(
            "local-candle",
            caps("local-candle", true, true, true, &[]),
        )]);
        let coord = FineTuningCoordinator::new(reg);
        let (id, _) = coord
            .select(&base_request("some-random-base-model"), None)
            .unwrap();
        assert_eq!(id, "local-candle");
    }

    // what this catches: LoRA requirement filter. If the caller
    // supplied LoRAHyperparams but the only registered adapter is
    // a hypothetical full-finetune-only provider, the coordinator
    // returns NoCapableAdapter instead of silently dropping the
    // LoRA config. (No such adapter exists in the registry today;
    // this guards a future addition.)
    #[test]
    fn lora_requirement_filters_non_lora_adapters() {
        use crate::genome::fine_tuning::types::LoRAHyperparams;
        let reg = registry_with(vec![(
            "full-finetune-only",
            caps("full-finetune-only", false, true, false, &[]),
        )]);
        let coord = FineTuningCoordinator::new(reg);
        let mut req = base_request("anything");
        req.lora = Some(LoRAHyperparams {
            rank: 8,
            alpha: 16,
            dropout: 0.0,
            target_modules: vec![],
        });
        let err = coord.select(&req, None).err().expect("must reject");
        assert!(matches!(err, CoordinatorError::NoCapableAdapter { .. }));
    }

    // what this catches: validation_split > 0 requires
    // supports_validation. Same shape as the LoRA filter — the
    // capabilities are gates, not soft preferences.
    #[test]
    fn validation_split_requires_validation_capable_adapter() {
        let reg = registry_with(vec![("no-val", caps("no-val", true, false, false, &[]))]);
        let coord = FineTuningCoordinator::new(reg);
        let mut req = base_request("anything");
        req.dataset.validation_split = 0.1;
        let err = coord.select(&req, None).err().expect("must reject");
        assert!(matches!(err, CoordinatorError::NoCapableAdapter { .. }));
    }

    // what this catches: on an Apple-Silicon (Metal) host, the native
    // Metal trainer (mlx-local) beats the accelerator-agnostic local
    // Candle trainer even though both produce a local artifact. This is
    // the bug the hardware tier fixes: without it, both sat at the same
    // locality tier and alphabetical tie-break picked `local-candle`,
    // so MLX never ran where it's the *right* trainer.
    #[test]
    fn metal_trainer_beats_generic_local_on_metal_host() {
        let reg = registry_with(vec![
            (
                "local-candle",
                caps_hw("local-candle", true, TrainerHardware::Any),
            ),
            (
                "mlx-local",
                caps_hw("mlx-local", true, TrainerHardware::Metal),
            ),
        ]);
        let coord = FineTuningCoordinator::with_host(reg, host(true, false, false));
        let (id, _) = coord
            .select(&base_request("Qwen/Qwen2.5-Coder-3B"), None)
            .unwrap();
        assert_eq!(id, "mlx-local");
    }

    // what this catches: "the same for CUDA or other varieties" made
    // real — a CUDA trainer advertising TrainerHardware::Cuda lands in
    // the native-accelerator tier on an NVIDIA host and beats Candle,
    // with ZERO coordinator change beyond registering the adapter. This
    // test stands in for that future adapter via a Cuda-requiring stub.
    #[test]
    fn cuda_trainer_beats_generic_local_on_cuda_host() {
        let reg = registry_with(vec![
            (
                "local-candle",
                caps_hw("local-candle", true, TrainerHardware::Any),
            ),
            (
                "cuda-trainer",
                caps_hw("cuda-trainer", true, TrainerHardware::Cuda),
            ),
        ]);
        let coord = FineTuningCoordinator::with_host(reg, host(false, true, false));
        let (id, _) = coord
            .select(&base_request("Qwen/Qwen2.5-Coder-3B"), None)
            .unwrap();
        assert_eq!(id, "cuda-trainer");
    }

    // what this catches: an adapter whose required accelerator the host
    // lacks is FILTERED OUT, never selected. A Metal trainer on a
    // CUDA-only host falls back to nothing-of-its-kind; the generic
    // Candle trainer wins instead. Without the hardware gate the
    // coordinator would route a job to mlx-local that create_job would
    // then reject — fail-loud at routing beats fail-late at spawn.
    #[test]
    fn metal_trainer_filtered_out_on_non_metal_host() {
        let reg = registry_with(vec![
            (
                "local-candle",
                caps_hw("local-candle", true, TrainerHardware::Any),
            ),
            (
                "mlx-local",
                caps_hw("mlx-local", true, TrainerHardware::Metal),
            ),
        ]);
        let coord = FineTuningCoordinator::with_host(reg, host(false, true, false));
        let (id, _) = coord
            .select(&base_request("Qwen/Qwen2.5-Coder-3B"), None)
            .unwrap();
        assert_eq!(id, "local-candle");
    }

    // what this catches: when the ONLY registered trainer needs an
    // accelerator the host lacks, the coordinator returns the typed
    // NoCapableAdapter rejection — not a silent no-op, not a panic.
    // This is the no-fallbacks-ever invariant at the hardware gate.
    #[test]
    fn no_capable_adapter_when_host_lacks_required_accelerator() {
        let reg = registry_with(vec![(
            "mlx-local",
            caps_hw("mlx-local", true, TrainerHardware::Metal),
        )]);
        let coord = FineTuningCoordinator::with_host(reg, host(false, true, false));
        let err = coord
            .select(&base_request("Qwen/Qwen2.5-Coder-3B"), None)
            .err()
            .expect("must reject");
        assert!(matches!(err, CoordinatorError::NoCapableAdapter { .. }));
    }
}
