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

use super::adapter::{ArcFineTuningAdapter, FineTuningCapabilities, TrainingPlace};
use super::registry::FineTuningRegistry;
use super::types::TrainingJobRequest;
use crate::inference_capability::probe_hardware_profile;
use crate::system_resources::hardware_profile::HardwareProfile;

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
         (registered providers: {registered:?}; supported base prefixes by provider: {supported_prefixes:?}; \
         matching this request but not serving this base now: {not_serving_now:?})"
    )]
    NoCapableAdapter {
        base_model: String,
        registered: Vec<String>,
        /// Trainers whose capabilities match this request but whose live state said no at
        /// this selection (`serves_base`, read once per selection): `grid-peer` with no fresh peer advertising the base, an in-engine
        /// trainer with no lane serving it. An empty prefix list means "any base, gated live",
        /// so without this the refusal read as if nothing could ever train the base.
        not_serving_now: Vec<String>,
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

/// A trainer that runs on THIS host: its resident weights, an engine lane, its own copy or
/// its CPU. A grid-peer or cloud trainer only passes the job on.
fn on_this_host(caps: &FineTuningCapabilities) -> bool {
    !matches!(caps.place, TrainingPlace::GridPeer | TrainingPlace::Cloud)
}

impl FineTuningCoordinator {
    /// Production constructor — probes the real host hardware once.
    pub fn new(registry: Arc<FineTuningRegistry>) -> Self {
        Self {
            registry,
            host: probe_hardware_profile(),
        }
    }

    /// Pick an adapter for this request. Returns the adapter +
    /// the provider id that was chosen (so the caller can log which
    /// provider got picked without re-deriving from the adapter).
    pub fn select(
        &self,
        request: &TrainingJobRequest,
        preferred_provider: Option<&str>,
    ) -> Result<(String, ArcFineTuningAdapter), CoordinatorError> {
        self.select_where(request, preferred_provider, |_| true)
    }

    /// [`select`](Self::select) among the trainers on THIS host only: a grid-peer or cloud
    /// trainer is never chosen. A peer serving a leased job uses it (card cc34db0d), so a
    /// lease is trained here or refused, and never passed on to a third node or a cloud.
    pub fn select_here(
        &self,
        request: &TrainingJobRequest,
    ) -> Result<(String, ArcFineTuningAdapter), CoordinatorError> {
        self.select_where(request, None, on_this_host)
    }

    fn select_where(
        &self,
        request: &TrainingJobRequest,
        preferred_provider: Option<&str>,
        admits: impl Fn(&FineTuningCapabilities) -> bool,
    ) -> Result<(String, ArcFineTuningAdapter), CoordinatorError> {
        let all_ids = self.registry.list();

        // Compute the capable set once. A trainer bound to live state (the in-engine one, a
        // grid peer) is asked ONCE whether it serves THIS base now, so it is never chosen for a
        // job it would refuse, and a refusal names it from that same answer.
        let mut capable: Vec<(String, ArcFineTuningAdapter, FineTuningCapabilities)> = Vec::new();
        let mut not_serving_now: Vec<String> = Vec::new();
        for (id, adapter) in all_ids.iter().filter_map(|id| self.registry.get(id).map(|a| (id.clone(), a))) {
            let caps = adapter.capabilities();
            if !(admits(&caps) && self.caps_match(&caps, request)) {
                continue;
            }
            if adapter.serves_base(&request.base_model) {
                capable.push((id, adapter, caps));
            } else {
                not_serving_now.push(id);
            }
        }

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
                    not_serving_now,
                })
            }
        }
    }

    /// The bases a grid peer may lease to this node (card 6f05b89d): `base` when a trainer
    /// on THIS host, its resident weights or its own copy, would accept a LoRA job for it.
    /// A grid-peer or cloud trainer never counts, so a node never advertises training it
    /// would only pass on. The selection gate is the one `select` uses, never a second one.
    pub fn trains_base_here(&self, base: &str) -> bool {
        self.registry.list().iter().filter_map(|id| self.registry.get(id)).any(|adapter| {
            let caps = adapter.capabilities();
            on_this_host(&caps)
                && caps.supports_lora
                && self.runs_base_here(&caps, base)
                && adapter.serves_base(base)
        })
    }

    /// The host can run this trainer, and it accepts this base: the hardware gate first (an
    /// Apple `mlx-local` trainer on a Linux box, or a CUDA trainer on a Mac, is filtered out
    /// before anything else, on the host's probed device flags), then the base prefix (an
    /// empty list means the adapter validates the base on create_job, so any base matches).
    fn runs_base_here(&self, caps: &FineTuningCapabilities, base: &str) -> bool {
        caps.requires.satisfied_by(&self.host)
            && (caps.supported_base_model_prefixes.is_empty()
                || caps.supported_base_model_prefixes.iter().any(|p| base.starts_with(p)))
    }

    /// `true` iff the adapter's static capabilities cover this
    /// request's hard requirements AND the host can actually run it.
    /// Soft preferences (cost, pressure) are applied later in `rank`;
    /// this is the binary "can it even run on THIS host?" gate.
    fn caps_match(&self, caps: &FineTuningCapabilities, request: &TrainingJobRequest) -> bool {
        if !self.runs_base_here(caps, &request.base_model) {
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

    /// THE STATE WALK (ONE-RESIDENT-MODEL §S3): rank by the trainer's declared
    /// [`TrainingPlace`] (the resident weights here, a grid peer's lane, a separate copy
    /// here, the CPU here, a cloud service), then the alphabetical tie-break in `select`.
    /// Before 2026-10-05 the engine trainer declared `requires: Any` and so ranked BELOW
    /// `cuda-local` on the very machine where the resident engine should win: Kimi's first
    /// gene (job bcb7316f) went to the separate Python trainer and parked forever waiting
    /// for 30 GB the serving lane holds. A place is declared, never inferred from hardware.
    fn rank(&self, caps: &FineTuningCapabilities) -> (TrainingPlace, bool) {
        // Within one place a trainer built for THIS host's accelerator goes first (on Apple
        // Silicon the MLX copy before the generic one), until the separate copies are gone.
        let native = caps.requires.is_specific_accelerator() && caps.requires.satisfied_by(&self.host);
        (caps.place, !native)
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
    use crate::system_resources::hardware_profile::HardwareProfile;
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

    /// Configurable stub: capabilities are set at construction time
    /// so each test can isolate one selection rule.
    /// The second field is its live answer to `serves_base`.
    struct StubWithCaps(FineTuningCapabilities, bool);

    #[async_trait]
    impl FineTuningAdapter for StubWithCaps {
        fn capabilities(&self) -> FineTuningCapabilities {
            self.0.clone()
        }
        fn serves_base(&self, _base_model: &str) -> bool {
            self.1
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
            place: if produces_local_artifact { TrainingPlace::SeparateCopyHere } else { TrainingPlace::Cloud },
        }
    }

    fn base_request(base_model: &str) -> TrainingJobRequest {
        TrainingJobRequest {
            persona_id: Uuid::nil(),
            persona_name: "test".into(),
            base_model: base_model.into(),
            trait_kind: "test-trait".into(),
            resume_from: None,
            parent: None,
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
            reg.register(Arc::new(StubWithCaps(c, true)));
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
    // what this catches (card a6703c24; IntelMac, x86_64 with a Radeon over Metal): an Intel
    // Mac's GPU reports Metal, and the MLX trainer, which needs Apple Silicon, passed the hardware
    // gate and outranked the CPU trainer, so every dispatch wedged on a trainer that cannot run
    // there. On Apple Silicon it still runs.
    #[test]
    fn the_mlx_trainer_needs_apple_silicon_not_just_metal() {
        let intel_mac = HardwareProfile { platform: "macos-x86_64-macbookpro15".into(), ..host(true, false, false) };
        let apple = HardwareProfile { platform: "macos-arm64-m5pro".into(), ..host(true, false, false) };
        assert!(!TrainerHardware::AppleSilicon.satisfied_by(&intel_mac), "an Intel Mac has Metal but is not Apple Silicon");
        assert!(TrainerHardware::AppleSilicon.satisfied_by(&apple));
        assert!(TrainerHardware::Any.satisfied_by(&intel_mac), "the CPU trainer still runs there");
    }

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
            top_layers: None,
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

    // what this catches (card 6f05b89d): a node advertising training it would only pass on.
    // A cloud trainer or a grid-peer trainer must not make a node look trainable to the grid
    // (a peer leasing to it would bounce the job onward or to a provider), and a trainer
    // without LoRA, or for another base, does not count either.
    #[test]
    fn only_a_trainer_on_this_host_makes_a_base_trainable_here() {
        let mut grid = caps("grid-peer", true, true, true, &[]);
        grid.place = TrainingPlace::GridPeer;
        let remote_only = registry_with(vec![
            ("openai", caps("openai", true, true, false, &["gpt-"])),
            ("grid-peer", grid),
        ]);
        let coord = FineTuningCoordinator::new(remote_only);
        assert!(!coord.trains_base_here("gpt-4o-mini"), "a cloud trainer is not training here");
        assert!(!coord.trains_base_here("qwen3.5-4b"), "a grid-peer trainer is not training here");

        let local = registry_with(vec![
            ("local", caps("local", true, true, true, &["qwen"])),
            ("no-lora", caps("no-lora", false, true, true, &[])),
        ]);
        let coord = FineTuningCoordinator::new(local);
        assert!(coord.trains_base_here("qwen3.5-4b"));
        assert!(!coord.trains_base_here("llama-3-8b"), "another base, and the wildcard trainer has no LoRA");
    }

    // what this catches (card cc34db0d): a leased job passed on. A peer serving a lease
    // selects among its own trainers only, so a grid-peer trainer (which would forward it to a
    // third node) or a cloud trainer is never chosen, even when it would rank first or is the
    // only one capable; with a trainer on this host, that one is chosen.
    #[test]
    fn a_lease_is_trained_on_this_host_or_refused() {
        let mut grid = caps("grid-peer", true, true, true, &[]);
        grid.place = TrainingPlace::GridPeer;
        let req = base_request("qwen3.5-4b");
        let only_remote = FineTuningCoordinator::new(registry_with(vec![("grid-peer", grid.clone())]));
        assert_eq!(only_remote.select(&req, None).map(|(id, _)| id).ok().as_deref(), Some("grid-peer"));
        assert!(
            matches!(only_remote.select_here(&req), Err(CoordinatorError::NoCapableAdapter { .. })),
            "a lease never goes on to another node"
        );
        let both = FineTuningCoordinator::new(registry_with(vec![
            ("grid-peer", grid),
            ("local", caps("local", true, true, true, &["qwen"])),
        ]));
        assert_eq!(both.select_here(&req).map(|(id, _)| id).ok().as_deref(), Some("local"));
    }

    // what this catches (card 7c296a5d): a refusal that hides why. enrico's 0.5b jobs refused
    // listing only base prefixes, where grid-peer's empty list means "any base, gated live",
    // so the reason (no fresh peer advertises the base) was invisible. A trainer that matches
    // but is not serving the base now is named; one that does not match at all is not.
    #[test]
    fn a_refusal_names_the_trainers_that_match_but_are_not_serving_the_base_now() {
        let mut grid = caps("grid-peer", true, true, true, &[]);
        grid.place = TrainingPlace::GridPeer;
        let reg = Arc::new(FineTuningRegistry::new());
        reg.register(Arc::new(StubWithCaps(grid, false)));
        reg.register(Arc::new(StubWithCaps(caps("local-candle", true, true, true, &["synthetic"]), true)));
        let err = FineTuningCoordinator::new(reg).select(&base_request("qwen2.5-coder-0.5b"), None).err().expect("must reject");
        let CoordinatorError::NoCapableAdapter { not_serving_now, .. } = &err else { panic!("{err:?}") };
        assert_eq!(not_serving_now, &vec!["grid-peer".to_string()]);
        assert!(err.to_string().contains("not serving this base now: [\"grid-peer\"]"), "{err}");
    }
}
