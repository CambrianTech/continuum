//! Shared control fixture; regression cases live with their concerns.
#![cfg(test)]

use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Fake control: scripted probe result + a counter of serve() calls, so the
/// pure reconcile decision is tested without a live process.
struct FakeControl {
    probe: Result<Option<String>, &'static str>,
    verified_launch: Option<OwnedServingTarget>,
    /// The genome set the fake reports as currently loaded (sorted paths).
    active_adapters: Vec<String>,
    serve_ok: bool,
    serves: AtomicUsize,
    /// Whether the fake's decode smoke-probe succeeds. `true` = a healthy lane
    /// (adoptable); `false` = a compute-wedged orphan (must be rejected).
    decode_ok: bool,
    /// `Some(_)` = the fake's `/slots` fingerprint ADVANCES on every look (a busy
    /// lane); `None` = no slots account (the default: no exoneration, as before).
    slots_busy: Option<AtomicUsize>,
    /// The KV decision the fake's running engine was launched with; `None` = unknown
    /// (the default, so every pre-existing case adopts on the other axes alone).
    launched_kv: Option<crate::inference::lane_process::LaunchKv>,
    /// Whether the fake "owns" the running child (we spawned it). `false` =
    /// an adopted orphan (the conservative default that exercises the
    /// smoke-probe gate).
    owns: bool,
    /// The slot count the fake's `/props` reports (`total_slots`). Defaults to
    /// 0 = "the server named no slot count, nothing to compare", which is the
    /// same lanes-OK verdict these tests had before the lane operand was probed
    /// — so every pre-existing case still exercises the WINDOW axis alone.
    served_lanes: u32,
    /// The per-slot window the fake's `/props` reports. Defaults to the tests'
    /// target window so the window-grow relaunch check (which only fires when
    /// target > served + tolerance) is a no-op for the model/adapter/decode
    /// tests; a grow-relaunch test sets it BELOW the target explicitly.
    served_window: u32,
    /// What an in-place genome change comes to. Defaults to `Unsupported`, the
    /// engine every pre-S1 test modelled: a genome change relaunches.
    in_place: GenomeInPlace,
    /// How many times the reconcile asked this lane to record its observed placement.
    placements_recorded: AtomicUsize,
}

impl FakeControl {
    fn probe(probe: Result<Option<String>, &'static str>) -> Self {
        Self {
            probe,
            verified_launch: None,
            active_adapters: Vec::new(),
            serve_ok: true,
            serves: AtomicUsize::new(0),
            decode_ok: true,
            slots_busy: None,
            launched_kv: None,
            owns: false,
            served_lanes: 0,
            served_window: 32768,
            in_place: GenomeInPlace::Unsupported,
            placements_recorded: AtomicUsize::new(0),
        }
    }
    fn with_in_place(mut self, outcome: GenomeInPlace) -> Self {
        self.in_place = outcome;
        self
    }
    /// Model a lane whose live per-slot window is SMALLER than the plan target —
    /// the starved boot-floor case the window-grow relaunch must catch.
    fn with_served_window(mut self, n: u32) -> Self {
        self.served_window = n;
        self
    }
    /// Model a lane whose live `/props total_slots` is what it is — the LANE
    /// axis of the same grow-vs-adopt decision. Used to pin that a past form of
    /// ourself already serving enough slots is ADOPTED, not reaped.
    fn with_served_lanes(mut self, n: u32) -> Self {
        self.served_lanes = n;
        self
    }
    fn serve_fails(mut self) -> Self {
        self.serve_ok = false;
        self
    }
    /// Report a genome set as already loaded (the live server's catalog).
    fn with_active_adapters(mut self, adapters: Vec<String>) -> Self {
        self.active_adapters = adapters;
        self
    }
    /// Model the compute-wedged orphan: answers `/v1/models` but every decode
    /// 500s.
    fn decode_wedged(mut self) -> Self {
        self.decode_ok = false;
        self
    }
    /// Model a lane whose `/slots` account advances on every look — a busy lane
    /// (mid-prefill for a client this core never knew, or under a build's load).
    fn slots_busy(mut self) -> Self {
        self.slots_busy = Some(AtomicUsize::new(0));
        self
    }
    /// Model an engine launched with this KV decision (read off its argv in production).
    fn launched_with(mut self, cache_type: Option<&str>, flash_attn: bool) -> Self {
        self.launched_kv = Some(crate::inference::lane_process::LaunchKv { cache_type: cache_type.map(str::to_string), flash_attn, cache_ram_mib: None });
        self
    }
    /// Model an engine launched on THIS KV decision with a `--cache-ram` grant of `mib`.
    fn launched_with_cache(mut self, mib: u32) -> Self {
        let planned = crate::cognition::kv_cache_plan::resolve();
        self.launched_kv = Some(crate::inference::lane_process::LaunchKv {
            cache_type: planned.launcher_cache_type().map(str::to_string),
            flash_attn: planned.flash_attn,
            cache_ram_mib: Some(mib),
        });
        self
    }
    /// Model a child we spawned ourselves (trusted without a per-tick probe).
    fn owned(mut self) -> Self {
        self.owns = true;
        self
    }
}

#[async_trait]
impl LlamaServerControl for FakeControl {
    fn owned_serving_target(&self) -> Option<OwnedServingTarget> {
        self.verified_launch.clone()
    }
    fn owned_engine(&self) -> Option<OwnedEngineIdentity> {
        self.verified_launch
            .as_ref()
            .map(|launch| launch.identity.clone())
    }
    async fn active_model(&self) -> Result<Option<String>, LlamaServerError> {
        match &self.probe {
            Ok(v) => Ok(v.clone()),
            Err(m) => Err(LlamaServerError::Unreachable(m.to_string())),
        }
    }
    async fn active_adapters(&self) -> Result<Vec<String>, LlamaServerError> {
        Ok(self.active_adapters.clone())
    }
    async fn adopt_genome_in_place(
        &self,
        _target: &ServingTarget,
    ) -> Result<GenomeInPlace, LlamaServerError> {
        Ok(self.in_place)
    }
    async fn serve(&self, _target: &ServingTarget) -> Result<(), LlamaServerError> {
        self.serves.fetch_add(1, Ordering::SeqCst);
        if self.serve_ok {
            Ok(())
        } else {
            Err(LlamaServerError::Spawn("boom".into()))
        }
    }
    async fn served_context_window(&self) -> Result<u32, LlamaServerError> {
        // The per-slot window the fake's /props reports (configurable via
        // `with_served_window`). Non-zero so a ready outcome publishes a ready
        // snapshot; defaults to the target window so the window-grow relaunch
        // check is a no-op unless a test deliberately floors it.
        Ok(self.served_window)
    }
    async fn served_lanes(&self) -> Result<u32, LlamaServerError> {
        // The slot count the fake's /props reports (configurable via
        // `with_served_lanes`). Defaults to 0 = "nothing to compare", so the
        // lane axis is inert unless a test deliberately sets it.
        Ok(self.served_lanes)
    }
    async fn decode_smoke_ok(&self) -> bool {
        self.decode_ok
    }
    async fn slots_activity_fingerprint(&self) -> Option<u64> {
        self.slots_busy
            .as_ref()
            .map(|n| n.fetch_add(1, Ordering::SeqCst) as u64)
    }
    fn launch_kv(&self) -> Option<crate::inference::lane_process::LaunchKv> {
        self.launched_kv.clone()
    }
    fn owns_child(&self) -> bool {
        self.owns
    }
    async fn record_observed_placement(&self) {
        self.placements_recorded.fetch_add(1, Ordering::SeqCst);
    }
}

/// Minimal serving target for the reconcile-logic tests: only `model.id` is
/// load-bearing here (it drives the relaunch decision); the rest is filler so
/// the grouped `Model` constructs. The real launcher path is covered by the
/// live serving daemon, not these pure-decision tests.
fn target(id: &str) -> ServingTarget {
    use crate::model_registry::types::{Arch, MultiPartyChatStrategy};
    ServingTarget {
        host_prompt_cache_mib: crate::inference::lane_args::CACHE_RAM_MIB,
        model: Model {
            weights_bytes: None,
            mmproj_bytes: None,
            id: id.to_string(),
            name: None,
            provider: "llamacpp-local".to_string(),
            arch: Arch::Qwen2,
            context_window: 32768,
            max_output_tokens: 4096,
            tokens_per_second: 0.0,
            capabilities: std::collections::BTreeSet::new(),
            cost_input_per_1k: 0.0,
            cost_output_per_1k: 0.0,
            gguf_hint: None,
            hf_source: None,
            gguf_local_path: None,
            chat_template: None,
            stop_sequences: Vec::new(),
            multi_party_strategy: MultiPartyChatStrategy::ProperChatMlSingleParty,
            mmproj_local_path: None,
            parameter_count: 0,
            sampling: crate::model_registry::types::ModelSampling::default(),
            persona_serving_eligible: true,
            serving: Default::default(), // test/fixture literal: substrate defaults (text-only main lane, unverified kv-shift)
        },
        context_window: 32768,
        lanes: 1,
        adapters: Vec::new(),
        placement: LanePlacement::Gpu,
        expert_placement: None,
        resident_override: None,
        vision_sidecar: false,
    }
}

/// Same as [`target`] but with a genome set loaded — for the relaunch-on-
/// genome-change tests. Paths are the load identity the reconcile compares.
fn target_with_adapters(id: &str, paths: &[&str]) -> ServingTarget {
    let mut t = target(id);
    t.adapters = paths
        .iter()
        .map(|p| AdapterEntry {
            alias: format!("gene-{p}"),
            path: PathBuf::from(p),
        })
        .collect();
    t
}

mod progress;
mod engine_install;
mod genome;
mod retirement;
mod liveness;
mod snapshot;
mod readiness;
mod configuration;
mod reconcile;
mod pages;

use super::configuration::{DEFAULT_HOST, DEFAULT_PORT, chosen_port};

use super::pages::{sweep_stale_page_generations_guarded_in};

use super::process::{tail_or_hang_marker};

use super::progress::{restore_progress, restore_probe_response};

use super::readiness::{ready_budget, ReadyLoadProgress, LoadProgressSample, LoadingHealth};

use super::smoke::{counts_upward};

use super::vision::{main_lane_mmproj_decision};
