#![cfg(test)]

// what this catches (2026-09-16, the 5090): the affordability term charged the
// serve's on-device working set to HOST RAM regardless of topology. On a discrete
// GPU the 27B's weights + a 144k KV window live in VRAM, yet `physical − peak −
// floor` went to 0 and the cache to its 256 MiB hard floor while 49 GB of host RAM
// sat free — every turn a full re-prefill. Same footprint, same demand, same box:
// unified pays the working set, discrete does not, and the discrete answer is the
// demand-sized one.
/// The grant the cache may be promised out of a ceiling: the ceiling less the
/// cache's grant headroom (#4216 — a cache fills to its grant).
pub(super) fn grantable(ceiling: u64) -> u64 {
    (ceiling as f64 * (1.0 - crate::inference::planning::serving_plan::CACHE_GRANT_HEADROOM)) as u64
}

/// The grant out of a known `available` reading: the smaller of "above the OS
/// floor" and "above the Eco exit band", less the co-consumer share.
pub(super) fn grantable_from_available(physical: u64, available: u64) -> u64 {
    grantable(
        available
            .saturating_sub(host_os_floor_bytes(physical))
            .min(available.saturating_sub(crate::capacity::power_mode::ECO_EXIT_BYTES)),
    )
}
use super::*;
use crate::inference::planning::serving_plan::footprint::apply_kv_quantization;

pub(super) const GB: u64 = 1_000_000_000;

use crate::inference::llama_server::LlamaServerError;
use std::sync::atomic::AtomicUsize;

/// Fake serving control: counts serve() calls and reports nothing live
/// (Unreachable), so reconcile always decides to (re)serve. A daemon-level
/// stub for the reconcile WIRING; the reconcile DECISION itself is tested in
/// `inference::llama_server` against its own fake.
pub(super) struct FakeServer {
    serves: Arc<AtomicUsize>,
    idles: Arc<AtomicUsize>,
    ok: bool,
    /// Drives [`LlamaServerControl::decode_smoke_ok`] so a test can wedge the lane's
    /// COMPUTE path (probe → false) independently of its control plane. Defaults true
    /// (a healthy lane decodes). See [`FakeServer::healthy`].
    smoke_ok: Arc<AtomicBool>,
    /// The stderr watcher's report channel, as the real `LlamaServerProcess` exposes it.
    /// A test raises this to stand in for a slot printing impossible progress.
    wedge: crate::inference::wedge::WedgeFlag,
    /// Drives [`LlamaServerControl::slots_activity_fingerprint`] (L11). 0 = the
    /// endpoint is unreadable (`None`); any other value is the fingerprint. A test
    /// bumps it to stand in for the serve loop advancing between smoke misses.
    slots_fp: Arc<AtomicU64>,
    paging_endpoint: Option<Arc<crate::inference::slots::EndpointSlots>>,
    verified_reads: Arc<AtomicUsize>,
    probe_effect: Option<Arc<dyn Fn() + Send + Sync>>,
    active: Option<String>,
    serve_started: Option<Arc<tokio::sync::Notify>>,
    probe_started: Option<Arc<tokio::sync::Notify>>,
    /// `/slots` reads served ([`LlamaServerControl::slots_body`]): the prefill knee's.
    slots_reads: Arc<AtomicUsize>,
    slots_reply: Option<Arc<dyn Fn() -> serde_json::Value + Send + Sync>>,
}

impl FakeServer {
    /// A healthy fake: serve() outcome = `ok`, decode heartbeat passes.
    pub(super) fn healthy(serves: Arc<AtomicUsize>, ok: bool) -> Self {
        Self {
            serves,
            ok,
            idles: Arc::new(AtomicUsize::new(0)),
            smoke_ok: Arc::new(AtomicBool::new(true)),
            wedge: Default::default(),
            slots_fp: Default::default(),
            paging_endpoint: None,
            verified_reads: Arc::new(AtomicUsize::new(0)),
            probe_effect: None,
            active: None,
            serve_started: None,
            probe_started: None,
            slots_reads: Arc::new(AtomicUsize::new(0)),
            slots_reply: None,
        }
    }
}

#[async_trait]
impl LlamaServerControl for FakeServer {
    fn paging_recovery_required(&self) -> bool {
        self.paging_endpoint
            .as_ref()
            .is_some_and(|endpoint| endpoint.paging_recovery_required())
    }

    fn wedge_flag(&self) -> Option<crate::inference::wedge::WedgeFlag> {
        Some(self.wedge.clone())
    }

    fn owned_serving_target(&self) -> Option<crate::inference::llama_server::OwnedServingTarget> {
        self.verified_reads.fetch_add(1, Ordering::SeqCst);
        None
    }
    async fn active_model(&self) -> Result<Option<String>, LlamaServerError> {
        if let Some(effect) = &self.probe_effect {
            effect();
        }
        if let Some(started) = &self.probe_started {
            started.notify_one();
            std::future::pending::<()>().await;
        }
        match &self.active {
            Some(active) => Ok(Some(active.clone())),
            None => Err(LlamaServerError::Unreachable("test: nothing up".into())),
        }
    }
    async fn active_adapters(&self) -> Result<Vec<String>, LlamaServerError> {
        Ok(Vec::new())
    }
    async fn idle(&self) -> Result<(), LlamaServerError> {
        self.idles.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    async fn serve(&self, _target: &ServingTarget) -> Result<(), LlamaServerError> {
        self.serves.fetch_add(1, Ordering::SeqCst);
        if let Some(started) = &self.serve_started {
            started.notify_one();
            std::future::pending::<()>().await;
        }
        if self.ok {
            Ok(())
        } else {
            Err(LlamaServerError::Spawn("test boom".into()))
        }
    }
    async fn served_context_window(&self) -> Result<u32, LlamaServerError> {
        // After a successful (test) serve the daemon reads the real per-slot
        // window; a fixed non-zero value stands in for the live `/props` read
        // so the reconcile publishes a READY snapshot carrying a real window.
        Ok(11008)
    }
    async fn served_lanes(&self) -> Result<u32, LlamaServerError> {
        // 0 = "this fake's /props names no slot count", i.e. nothing to compare
        // on the lane axis. These daemon tests assert the window/readiness path,
        // so the lane operand stays inert exactly as it was before it was probed.
        Ok(0)
    }
    async fn decode_smoke_ok(&self) -> bool {
        // Driven by `smoke_ok` so a test can wedge the COMPUTE path (the #175
        // liveness-heartbeat tests) independently of the control plane; defaults
        // true (a healthy fake decodes).
        self.smoke_ok.load(Ordering::Relaxed)
    }
    async fn slots_body(&self) -> Option<serde_json::Value> {
        self.slots_reads.fetch_add(1, Ordering::SeqCst);
        Some(
            self.slots_reply
                .as_ref()
                .map(|reply| reply())
                .unwrap_or_else(|| serde_json::json!([])),
        )
    }

    async fn slots_activity_fingerprint(&self) -> Option<u64> {
        match self.slots_fp.load(Ordering::Relaxed) {
            0 => None,
            v => Some(v),
        }
    }
}

pub(super) fn fake_model(id: &str) -> Model {
    use crate::model_registry::types::{Arch, MultiPartyChatStrategy};
    Model {
        weights_bytes: None,
        mmproj_bytes: None,
        id: id.to_string(),
        name: None,
        provider: "llamacpp-local".to_string(),
        arch: Arch::Qwen2,
        context_window: 262_144,
        max_output_tokens: 4096,
        tokens_per_second: 0.0,
        // A fixture row that can SPEAK: the base-model candidate set now requires
        // TextGeneration or Chat (can_serve_minds, card af635057), and every real
        // catalog row carries them; an empty set here would model an embedding row.
        capabilities: [
            crate::model_registry::types::Capability::Chat,
            crate::model_registry::types::Capability::TextGeneration,
        ]
        .into_iter()
        .collect(),
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
    }
}

/// A live catalog seeded from the real registry — the shared universe the
/// daemon plans off. Tests that drive `publish_plan` directly pass candidates
/// explicitly and never touch it; the reconcile-wiring tests just need it to
/// exist so the daemon can be constructed.
pub(super) fn test_catalog() -> Arc<ModelCatalog> {
    let reg = crate::model_registry::catalog::registry().expect("Rust catalog must validate");
    Arc::new(ModelCatalog::from_registry(&reg))
}

pub(super) fn test_resource_daemon() -> Arc<ResourceDaemon> {
    use crate::resources::{DaemonConfig, MockCapacitySource};
    ResourceDaemon::start(
        vec![Arc::new(MockCapacitySource::new(
            ResourceKind::Vram,
            53 * GB,
        ))],
        vec![],
        DaemonConfig::default(),
    )
}

// Upper-layer transaction owns invocation of the engine's shared real-child scenario.
#[cfg(windows)]
#[tokio::test]
async fn cancelled_exclusive_teacher_restores_real_owned_child() {
    const CHILD_ENV: &str = "CONTINUUM_TEACHER_RESTORE_CHILD";
    if std::env::var_os(CHILD_ENV).is_some() {
        use std::io::Read;
        let mut byte = [0];
        let _ = std::io::stdin().read(&mut byte);
        return;
    }
    let executable = std::env::current_exe();
    assert!(executable.is_ok(), "fixture executable must be available");
    let Ok(executable) = executable else {
        return;
    };
    let mut command = tokio::process::Command::new(executable);
    command
        .args([
            "--exact",
            "modules::serving_daemon::tests::cancelled_exclusive_teacher_restores_real_owned_child",
        ])
        .env(CHILD_ENV, "1")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .creation_flags(0x08000000);
    crate::inference::llama_server::test_fixtures::exercise_prepared_restore(
        command,
        |server, generation, root, http, saves| async move {
            exercise_cancelled_exclusive_teacher(server, generation, &root, http, saves).await;
        },
    )
    .await;
}

// Called by the existing real-child fixture, which supplies original validated
// inputs, a real child, and the actual HTTP checkpoint acknowledgement.
#[cfg(windows)]
pub(crate) async fn exercise_cancelled_exclusive_teacher(
    server: Arc<dyn LlamaServerControl>,
    generation: crate::inference::slots::EngineGeneration,
    root: &str,
    checkpoint_http: JoinHandle<()>,
    saves: Arc<AtomicUsize>,
) {
    use crate::cognition::eval::TeacherBatchRequest;
    let original = server.owned_serving_target().unwrap();
    let mut teacher = original.target.model.clone();
    teacher.id = "exclusive-teacher-fixture".into();
    teacher.provider = crate::inference::llama_server::PROVIDER_ID.into();
    let mut configured = daemon_with(server.clone());
    configured.set_model_resolver(Arc::new(move |_| Some(teacher.clone())));
    let daemon = Arc::new(configured);
    let mut prior = ServingSnapshot::empty();
    prior.active_model = Some(original.target.model.id.clone());
    prior.adapters = original.target.adapter_paths();
    prior.base_url = format!("{root}/v1");
    prior.ready = true;
    prior.served_context_window = original.observed_context_window;
    prior.lanes = original.observed_lanes;
    prior.host_prompt_cache_mib = original.launched_host_prompt_cache_mib;
    daemon.serving_tx.send_replace(prior.clone());
    daemon.acknowledge_verified_target(&prior, daemon.intent.snapshot().revision);
    assert!(daemon.verified_serving_target().is_some());
    let available = daemon
        .resource_daemon
        .board()
        .kinds
        .iter()
        .find(|kind| kind.kind == ResourceKind::Vram)
        .unwrap()
        .available_bytes;
    let pressure = daemon
        .resource_daemon
        .acquire_guarded(&crate::resources::LeaseRequest {
            consumer_id: "exclusive-fixture-pressure".into(),
            kind: ResourceKind::Vram,
            bytes: available,
            ttl_ms: u64::MAX,
            reclaim_policy: crate::resources::ReclaimPolicy::Pinned,
        })
        .unwrap();
    let owner = daemon.clone();
    let mut caller = tokio::spawn(async move {
        owner
            .run_teacher_batch(TeacherBatchRequest {
                tasks: vec![],
                teacher_model: "exclusive-teacher-fixture".into(),
                temperature: 0.0,
                max_fix_iters: Some(0),
            })
            .await
    });
    tokio::time::timeout(Duration::from_secs(15), async {
        let mut tick = tokio::time::interval(Duration::from_millis(10));
        while !crate::inference::llama_server::test_fixtures::generation_has_exited(&generation) {
            if caller.is_finished() {
                let outcome = (&mut caller).await.map(|result| result.map(|_| ()));
                panic!("checkpoint/retirement must be reached through the owner; early result: {outcome:?}");
            }
            tick.tick().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        saves.load(Ordering::SeqCst),
        1,
        "save confirmed before observed original exit"
    );
    assert!(daemon.reconciling.load(Ordering::Acquire));
    caller.abort();
    assert!(caller
        .await
        .err()
        .expect("cancelled command")
        .is_cancelled());
    checkpoint_http.abort();
    let _ = checkpoint_http.await;
    // Only now make capacity available. Caller cancellation cannot surrender
    // the cleanup obligation or turn actual exit into a resource grant.
    drop(pressure);
    tokio::time::timeout(Duration::from_secs(20), async {
        let mut tick = tokio::time::interval(Duration::from_millis(10));
        while server.owned_engine().is_none() {
            tick.tick().await;
            daemon.poll_teacher_batch();
        }
    })
    .await
    .unwrap();
    assert!(server.owned_engine().unwrap() != original.identity);
    assert!(
        daemon
            .resource_daemon
            .board()
            .leases
            .iter()
            .any(|lease| lease.consumer_id == SERVING_CONSUMER_ID && lease.bytes > 0),
        "shared restore spawn must own an actual serving grant"
    );
    assert!(
        !daemon.serving_tx.borrow().ready,
        "non-model child cannot fabricate restore success"
    );
    daemon.intent.set_pin(None, true);
    tokio::time::timeout(Duration::from_secs(20), async {
        let mut tick = tokio::time::interval(Duration::from_millis(10));
        while daemon.reconciling.load(Ordering::Acquire) {
            tick.tick().await;
            daemon.poll_teacher_batch();
        }
    })
    .await
    .unwrap();
    assert!(
        server.owned_engine().is_none(),
        "failed restore child must exit before releasing operation"
    );
    assert!(!daemon
        .resource_daemon
        .board()
        .leases
        .iter()
        .any(|lease| lease.consumer_id == SERVING_CONSUMER_ID));
    assert!(
        !daemon.serving_tx.borrow().ready,
        "new intent is left to normal reconcile"
    );
}

pub(super) fn daemon_with(server: Arc<dyn LlamaServerControl>) -> ServingDaemonModule {
    daemon_on_card(server, 53 * GB)
}

/// The same daemon on a card of a chosen size — the host floor is derived from
/// PHYSICAL VRAM, so a test about "a genuinely small box" must give it one.
pub(super) fn daemon_on_card(
    server: Arc<dyn LlamaServerControl>,
    vram_bytes: u64,
) -> ServingDaemonModule {
    let gpu = Arc::new(GpuMemoryManager::simulated("test card", vram_bytes));
    let system = Arc::new(SystemResourceMonitor::new());
    let mut daemon = ServingDaemonModule::with_control(
        gpu,
        system,
        test_resource_daemon(),
        server,
        test_catalog(),
        test_pin_store(),
    );
    // Every harness daemon OWNS its demand pool: a private working set and an empty
    // leased-in ring. Both are process globals in production and shared by every test
    // in the binary — one test's samples became another test's requirement (CI,
    // 2026-09-20, three runs).
    daemon.working_set = crate::cognition::working_set::WorkingSetRegistry::new();
    daemon.set_leased_in_sent_source(Arc::new(Vec::new));
    // Resolve any planned id to a fake Model so reconcile can build a
    // ServingTarget without a populated global registry.
    daemon.set_model_resolver(Arc::new(|id: &str| Some(fake_model(id))));
    // NO real decode observed — the fresh-boot state every heartbeat test below assumes.
    // Left to the default this reads a process-global that ANY other test in this binary
    // can stamp (`llama_server`'s `note_real_decode` coverage does), which silently turns
    // the trust short-circuit on and reds these tests by scheduling order alone.
    daemon.set_decode_age_source(Arc::new(|| None));
    // NO inherited lane — the fresh-machine state every plan test below assumes. Left to the
    // default this reads the OPERATOR's real pidfile + lane registry, so a developer with a
    // 27B serving would silently hand every planning test an incumbent it never declared.
    // Same class of leak as the decode-age global above; caught by measurement in
    // `serving_consumer` before it could ever be caught by a red.
    daemon.set_inherited_lane(Arc::new(|| None));
    daemon
}

/// The #438 boot geometry, in the shape taken from the live ledger (2026-08-19): a 27B whose
/// weights are ~19 GB is still resident from the previous generation, so the successor's
/// first sample of free VRAM reads ~6 GB. Only something small fits that; the 27B does not.
pub(super) fn boot_squeeze() -> (HostBudget, Vec<ModelFootprint>) {
    (
        HostBudget {
            usable_bytes: 6 * GB,
            perf_cores: 6,
        },
        vec![
            footprint_from_parts("qwen3-27b", 19 * GB, 8192, true, None).unwrap(),
            footprint_from_parts("coder-4b", 3 * GB, 8192, true, None).unwrap(),
        ],
    )
}

pub(super) fn inherited_27b() -> LaneRecord {
    LaneRecord {
        pid: 11341,
        port: 58057,
        role: crate::inference::lane_registry::LaneRole::Live,
        model: "qwen3-27b".into(),
        context_window: 25_075,
        lanes: 4,
        page_dir: None,
        engine_bin: None,
        weights: None,
        started_s: 0,
        placement: None,
    }
}

/// A lane whose live per-slot window sits at `live_pct` of what the plan
/// actually affords, ready to have `reconcile_to_plan` driven at it repeatedly.
/// Returns the daemon and the REAL planned window.
///
/// Two things this fixture exists to get right, both of which a hand-written
/// pair of numbers gets wrong:
///
/// 1. A plan is capped by MEASURED demand ([`ServingDemand`]). With nothing
///    recorded it falls back to [`BOOTSTRAP_WORKING_SET`] — 16,384, which is
///    exactly the number the live lane was stranded at, so a fixture that skips
///    this reproduces a plan == live and no gain to sustain at all.
/// 2. The planner is free to serve less than the model's max context (host fit,
///    lane split). Asserting against a window we merely ASKED for tests the
///    fixture; deriving the live window from the plan we GOT tests the guard.
pub(super) fn lane_under_plan(
    serves: Arc<AtomicUsize>,
    plan_model_ctx: u32,
    live_pct: u32,
) -> (ServingDaemonModule, u32) {
    let daemon = daemon_with(Arc::new(FakeServer::healthy(serves, true)));
    // A measured peak is what lets a plan exceed the cold-start prior.
    daemon
        .working_set()
        .record_in_memory(uuid::Uuid::new_v4(), plan_model_ctx, 0);
    let budget = HostBudget {
        usable_bytes: 45 * GB,
        perf_cores: 6,
    };
    let candidates =
        vec![footprint_from_parts("coder-14b", 9 * GB, plan_model_ctx, true, None).unwrap()];
    daemon.publish_plan(budget, &candidates, &candidates);
    let (plan_window, plan_lanes) = {
        let plan = daemon.plan_tx.borrow();
        let plan = plan.plan.as_ref().expect("plan published");
        (plan.served_context_window, plan.lanes)
    };
    let _ = daemon.serving_tx.send_replace(ServingSnapshot {
        ready_verified_at_ms: None,
        active_model: Some("coder-14b".into()),
        ready: true,
        base_url: serving_v1_url(),
        adapters: Vec::new(),
        served_context_window: plan_window * live_pct / 100,
        // Live lanes match the plan's so these fixtures isolate the WINDOW
        // dimension of re-home evidence; the lanes dimension has its own test
        // (rehome_evidence_counts_lanes_not_just_per_slot_window).
        lanes: plan_lanes,
        host_prompt_cache_mib: 0,
        degraded_reason: None,
        vision_ready: false,
        vision_base_url: None,
        vision_model: None,
        loading_model: None,
        held_for: None,
    });
    (daemon, plan_window)
}

/// Believe-ready snapshot fixture for the liveness-heartbeat tests.
pub(super) fn ready_snapshot() -> ServingSnapshot {
    ServingSnapshot {
        // test fixture: no live readiness was ever CONFIRMED here.
        ready_verified_at_ms: None,
        active_model: Some("coder-14b".into()),
        ready: true,
        base_url: serving_v1_url(),
        adapters: Vec::new(),
        served_context_window: 11008,
        lanes: 4,
        host_prompt_cache_mib: 0,
        degraded_reason: None,
        vision_ready: false,
        vision_base_url: None,
        vision_model: None,
        loading_model: None,
        held_for: None,
    }
}

/// This file's PRODUCTION half — everything before `#[cfg(test)]`.
///
/// A source assertion must never be able to satisfy itself. `include_str!` pulls in
/// the whole file, tests included, so `src.contains("install_lane_admission(")`
/// matched the assertion's OWN string literal and stayed green with the production
/// call deleted — a check that cannot fail, which is the exact class this repo keeps
/// getting bitten by ([[success-signal-that-cannot-report-failure]]). Caught here by
/// a sibling assertion tripping over the same self-reference.
// After the responsibility split, include every production owner and no test source.
pub(super) fn production_source() -> &'static str {
    static SOURCE: OnceLock<String> = OnceLock::new();
    SOURCE
        .get_or_init(|| {
            [
                include_str!("../serving_daemon.rs"),
                include_str!("policy.rs"),
                include_str!("health.rs"),
                include_str!("publication.rs"),
                include_str!("placement.rs"),
                include_str!("moe_cache.rs"),
                include_str!("intent.rs"),
                include_str!("lifecycle.rs"),
                include_str!("demand.rs"),
                include_str!("budget.rs"),
                include_str!("device_budget.rs"),
                include_str!("consumer.rs"),
                include_str!("prompt_cache.rs"),
                include_str!("catalog.rs"),
                include_str!("launch_window.rs"),
                include_str!("tick.rs"),
                include_str!("planning.rs"),
                include_str!("reconcile.rs"),
                include_str!("launch.rs"),
            ]
            .join("\n")
        })
        .as_str()
}

/// A stored pin must never be able to take the node dark. Nested per the one-mod rule.
mod a_bad_pin_cannot_brick_the_node;

/// The gate that replaced #56's unbuilt `ResourceGovernor`. Nested per the one-mod rule.
mod engine_admission;

mod budget;
mod catalog;
mod demand;
mod health;
mod prompt_cache;
mod publication;
use publication::test_pin_store;
mod reconciliation;
mod rehome;
mod teacher;
