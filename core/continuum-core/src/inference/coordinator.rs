//! InferenceCoordinator — composes existing substrate primitives
//! into multi-persona-one-model serving per
//! [[INFERENCE-LANES-REALISTIC.md]].
//!
//! Joel (2026-05-31): "Yeah the inference command doesn't do this.
//! It's smart subsystems and daemons. Commands are dumb and short."
//! "We weren't clever enough with our lanes."
//!
//! ### What this layer does
//!
//! The coordinator owns the LANE LIFECYCLE — admission, lease + memory
//! accounting, handle binding, eviction. The
//! `ai/inference/{open,generate,close}` command surface (handle module)
//! routes through here. The adapter trait + handle store stay
//! unaware; the coordinator wraps both.
//!
//! ### Composition (no reinvention)
//!
//! - `plan_adaptive_throughput` from `cognition::adaptive_throughput`
//!   makes admission decisions keyed by `target_silicon`.
//! - `FootprintRegistry::acquire_lease` / `release_lease` from
//!   `inference::footprint_registry` mirrors the lease into byte
//!   accounting in one call.
//! - `InferenceHandleStore` from `inference::handle_store` owns the
//!   actual adapter session.
//! - `Lane` from `inference::lane` binds (persona, task, lease,
//!   handle).
//!
//! ### Doctrine alignment
//!
//! - [[commands-are-kernel-level-and-compose]] — coordinator is a
//!   plain Rust component (not a ServiceModule). The handle module
//!   delegates to it; callers never reach it directly.
//! - [[observability-is-half-the-architecture]] — Step 2 ships
//!   capture-event SHAPES (LaneCaptureEvent enum, sink trait, Noop
//!   default). The wiring through `InferenceHandleModule` in Step 3
//!   adds capture-aware delivery.
//! - [[host-the-seemingly-impossible]] — the coordinator is what
//!   makes "16 personas on commodity hardware" real. Lane
//!   accounting + admission + eviction compose into the substrate's
//!   defining boast.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use dashmap::DashMap;
use uuid::Uuid;

use crate::ai::adapter::AIProviderAdapter;
use crate::ai::types::ActiveAdapterRequest;
use crate::cognition::adaptive_throughput::{
    plan_adaptive_throughput, AdaptiveThroughputRequest, ResourceClass, TargetSilicon,
    ThroughputJob, ThroughputLaneBudget,
};
use crate::cognition::throughput_lease::ThroughputLease;
use crate::governor::classify_hardware;
use crate::governor::types::TargetSilicon as GovernorSilicon;
use crate::identity::PeerId;
use crate::inference::footprint_registry::{FootprintKey, FootprintRegistry, ResourceType};
use crate::inference::handle_store::{InferenceHandleStore, OpenSessionRequest};
use crate::inference::kv_quant::Residency;
use crate::inference::lane::{Lane, LaneClass};
use crate::inference::recipe_budget::TaskKind;
use crate::inference_capability::hw_probe::probe_hardware_profile;
use crate::paging::lease_revocation::disruption_rank;
use crate::runtime::cell_shapes::HandleRef;

/// Configuration the coordinator needs at construction.
///
/// `lane_budgets` is the substrate's per-silicon budget — feeds
/// the AdaptiveThroughputPlanner. `bytes_per_token` is a
/// model-specific KV cache estimate (typical 7B FP16 is ~64 KB;
/// INT8 KV halves it). `lease_duration_ms` is how long a lane's
/// lease lives before expiring (the coordinator's reclaim sweep
/// purges expired lanes).
#[derive(Debug, Clone)]
pub struct CoordinatorConfig {
    pub lane_budgets: Vec<ThroughputLaneBudget>,
    pub bytes_per_token: u64,
    pub lease_duration_ms: u64,
    /// Silicon the lanes target — drives admission lookup. Lanes can
    /// override per-open; this is the default.
    pub default_target_silicon: TargetSilicon,
}

impl CoordinatorConfig {
    /// The realistic-floor lane budget retargeted to a specific silicon
    /// class — the canonical config builder. Shape from the lanes-realistic
    /// doc (4 concurrent lanes, ~80K cost-units, ~64 KB/token FP16 KV); only
    /// the admission silicon varies. Real per-tier budgets arrive from the
    /// governor's policy file in a later slice — this fixes the SILICON
    /// label so the production default isn't anchored on the CPU/UMA floor.
    pub fn for_silicon(silicon: TargetSilicon) -> Self {
        Self {
            lane_budgets: vec![ThroughputLaneBudget {
                resource_class: ResourceClass::LocalGeneration,
                target_silicon: silicon,
                max_concurrency: 4,
                max_cost_units: 80_000,
            }],
            bytes_per_token: 64 * 1024,
            lease_duration_ms: 30 * 60 * 1000, // 30 minutes
            default_target_silicon: silicon,
        }
    }

    /// The "realistic floor" — UnifiedMemory budget. Kept for tests and for
    /// constrained hosts that explicitly want the floor; NOT the production
    /// default (see [`detected`](Self::detected)).
    pub fn realistic_floor_default() -> Self {
        Self::for_silicon(TargetSilicon::UnifiedMemory)
    }

    /// Hardware-detected config: probe the machine, classify its silicon,
    /// and build a [`for_silicon`](Self::for_silicon) budget for it. On an
    /// RTX 5090 this yields a `Gpu`-targeted coordinator; on Apple Silicon,
    /// `UnifiedMemory`; on a GPU-less host, `Cpu`. This RETIRES the hardcoded
    /// `UnifiedMemory` default — GPU-or-bust means the production default
    /// follows the hardware, not a Mac/CPU floor.
    ///
    /// One-shot startup probe (a few file reads + per-backend FFI, per
    /// `probe_hardware_profile`) — call once at boot, reuse the config.
    pub fn detected() -> Self {
        let class = classify_hardware(&probe_hardware_profile());
        Self::for_silicon(coordinator_silicon_for(class.silicon))
    }
}

/// Map the governor's hardware-detected [`GovernorSilicon`] to the
/// coordinator's [`TargetSilicon`] admission class — the seam between the
/// SubstrateGovernor's hardware classification (`classify_hardware`) and the
/// lane scheduler.
///
/// `AppleM` → `UnifiedMemory` (Apple-silicon shared accelerator memory);
/// every discrete/Metal GPU class (`NvidiaCuda` / `AmdRocm` / `IntelVulkan` /
/// `MacIntelMetal`) → `Gpu`; `None` (no accelerator detected) → `Cpu`. `Cpu`
/// is the HONEST classification, not a silent floor — a GPU-or-bust caller
/// inspects the result and refuses rather than quietly serving CPU.
///
/// `MacIntelMetal` (Intel Mac with a Metal-addressable discrete/integrated
/// GPU — task #52) maps to `Gpu`, NOT `UnifiedMemory`: it is a real GPU, but
/// not Apple-silicon shared memory. Added when the governor gained the
/// variant (#1624); this match is in the `--features metal` build path, so
/// the no-metal Linux CI did not catch the non-exhaustive gap.
pub fn coordinator_silicon_for(detected: GovernorSilicon) -> TargetSilicon {
    match detected {
        GovernorSilicon::AppleM => TargetSilicon::UnifiedMemory,
        GovernorSilicon::NvidiaCuda
        | GovernorSilicon::AmdRocm
        | GovernorSilicon::IntelVulkan
        | GovernorSilicon::MacIntelMetal => TargetSilicon::Gpu,
        GovernorSilicon::None => TargetSilicon::Cpu,
    }
}

/// Inputs for `open_lane`. `adapter` is `Arc<dyn AIProviderAdapter>`
/// which doesn't implement Debug — that's why this struct doesn't
/// derive Debug. Field-level inspection in tests goes through
/// individual accessors / Lane inspection rather than printing the
/// whole request.
#[derive(Clone)]
pub struct OpenLaneRequest {
    pub persona: PeerId,
    pub task: TaskKind,
    /// The adapter the session runs against. The coordinator
    /// doesn't touch the registry — caller passes the chosen
    /// adapter explicitly so wiring (which decides Heuristic vs
    /// LlamaCpp vs cloud) stays at the module layer.
    pub adapter: Arc<dyn AIProviderAdapter>,
    pub model: Option<String>,
    pub system_prompt: Option<String>,
    pub active_adapters: Option<Vec<ActiveAdapterRequest>>,
    /// Override the class derived from `task`. Used by the
    /// daemons when persona context (currently speaking in voice
    /// chat, etc.) implies a different class than the task's
    /// default.
    pub class_override: Option<LaneClass>,
    /// Wall-clock the admission + lease use. Caller supplies so
    /// the coordinator stays pure-of-clock (testable +
    /// deterministic replay).
    pub now_ms: u64,
}

/// Coordinator errors. Typed so callers branch by variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoordinatorError {
    AdmissionDenied {
        reason: AdmissionDenyReason,
        task: TaskKind,
        persona: PeerId,
    },
    LeaseAcquireFailed(String),
    HandleNotFound {
        handle_id: Uuid,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdmissionDenyReason {
    /// No budget declared for the lane's target_silicon — a
    /// configuration error per AdaptiveThroughputPlan semantics.
    NoBudget,
    /// Lane budget is exhausted — backpressure case; caller
    /// retries later (or re-targets via grid offload per #108).
    ResourcePressure,
    /// The admission planner dropped the job as stale before it
    /// even got admission consideration. Coordinator never sets
    /// the stale-after flag explicitly today but lift this so
    /// the variant exists for future use.
    Stale,
}

impl std::fmt::Display for CoordinatorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CoordinatorError::AdmissionDenied {
                reason,
                task,
                persona,
            } => write!(
                f,
                "coordinator: admission denied (reason: {reason:?}, task: {task:?}, persona: {})",
                persona.as_uuid()
            ),
            CoordinatorError::LeaseAcquireFailed(msg) => {
                write!(f, "coordinator: lease acquire failed: {msg}")
            }
            CoordinatorError::HandleNotFound { handle_id } => {
                write!(f, "coordinator: handle not found: {handle_id}")
            }
        }
    }
}

impl std::error::Error for CoordinatorError {}

/// Snapshot of a single lane's state — observability surface for
/// inspection commands per [[observability-is-half-the-architecture]].
#[derive(Debug, Clone)]
pub struct LaneInspection {
    pub persona: PeerId,
    pub task: TaskKind,
    pub class: LaneClass,
    pub handle_id: Uuid,
    pub seed_kv_tokens: u32,
    pub max_kv_tokens: u32,
    pub bytes_accounted: u64,
    pub lease_id: String,
    pub lease_acquired_at_ms: u64,
    pub lease_expires_at_ms: u64,
    pub is_pinned: bool,
}

/// Capture event emitted by the coordinator for each load-bearing
/// lane lifecycle decision, per [[observability-is-half-the-architecture]].
/// The Noop sink reduces these to no-ops in production; mechanic-shop
/// observers swap in the InMemory or future JSONL sink.
#[derive(Debug, Clone)]
pub enum LaneCaptureEvent {
    /// Open succeeded — admission passed, lease acquired, handle minted.
    LaneOpened {
        captured_at_ms: u64,
        persona: PeerId,
        task: TaskKind,
        class: LaneClass,
        handle_id: Uuid,
        lease_id: String,
        cost_units: u32,
        bytes_accounted: u64,
        target_silicon: TargetSilicon,
    },
    /// Open failed admission — admission planner denied.
    LaneAdmissionDenied {
        captured_at_ms: u64,
        persona: PeerId,
        task: TaskKind,
        reason: AdmissionDenyReason,
        cost_units_requested: u32,
        target_silicon: TargetSilicon,
    },
    /// Close — lane released, footprint freed, handle closed.
    LaneClosed {
        captured_at_ms: u64,
        persona: PeerId,
        task: TaskKind,
        handle_id: Uuid,
        lease_id: String,
        was_present: bool,
    },
    /// Pressure-driven eviction. Differs from LaneClosed in that
    /// the caller didn't choose it — the substrate did, under
    /// memory pressure. Reason classifies why this particular
    /// lane was picked.
    LaneEvicted {
        captured_at_ms: u64,
        persona: PeerId,
        task: TaskKind,
        class: LaneClass,
        handle_id: Uuid,
        lease_id: String,
        bytes_freed: u64,
        reason: EvictionReason,
    },
}

/// Why a particular lane was evicted in a pressure-driven walk.
/// Reported on `LaneCaptureEvent::LaneEvicted` so observers can
/// distinguish lease-expiry cleanup from genuine pressure response.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvictionReason {
    /// Lane's lease expired (regardless of class). First priority in
    /// any eviction walk — expired leases are free bytes.
    LeaseExpired,
    /// Class is `Hard` (Background, Sentinel) — first to go under
    /// non-expired pressure.
    PressureHard,
    /// Class is `Graceful` (Interactive) — second under pressure.
    /// Realtime (Pinned) is never targeted by pressure; expired
    /// realtime leases fall into `LeaseExpired`.
    PressureGraceful,
}

/// Outcome of `evict_under_pressure`.
#[derive(Debug, Clone)]
pub struct EvictionResult {
    pub evicted: Vec<EvictedLane>,
    pub bytes_freed: u64,
    /// `target - bytes_freed`. Zero when the target was met. Positive
    /// when the walk ran out of evictable lanes before reaching the
    /// target (typically because too many pinned lanes are active).
    pub bytes_short: u64,
}

#[derive(Debug, Clone)]
pub struct EvictedLane {
    pub handle_id: Uuid,
    pub persona: PeerId,
    pub task: TaskKind,
    pub class: LaneClass,
    pub bytes_freed: u64,
    pub reason: EvictionReason,
}

/// Sink trait for coordinator capture events. `record` is `&self`
/// so the coordinator's hot path stays lock-free; impls maintain
/// their own interior mutability. The Noop impl is the production
/// default + costs nothing.
pub trait LaneCaptureSink: Send + Sync {
    fn record(&self, event: LaneCaptureEvent);
}

/// Zero-cost default. Drops every event.
pub struct NoopLaneCaptureSink;

impl LaneCaptureSink for NoopLaneCaptureSink {
    fn record(&self, _event: LaneCaptureEvent) {}
}

/// In-memory ring of recent events for tests + introspection.
/// Bounded so a long-running observer doesn't leak memory. Drops
/// oldest events when at capacity.
pub struct InMemoryLaneCaptureSink {
    events: parking_lot::Mutex<std::collections::VecDeque<LaneCaptureEvent>>,
    capacity: usize,
}

impl InMemoryLaneCaptureSink {
    pub fn new(capacity: usize) -> Self {
        Self {
            events: parking_lot::Mutex::new(std::collections::VecDeque::with_capacity(capacity)),
            capacity,
        }
    }
    pub fn drain(&self) -> Vec<LaneCaptureEvent> {
        let mut g = self.events.lock();
        g.drain(..).collect()
    }
    pub fn snapshot(&self) -> Vec<LaneCaptureEvent> {
        self.events.lock().iter().cloned().collect()
    }
    pub fn len(&self) -> usize {
        self.events.lock().len()
    }
    pub fn is_empty(&self) -> bool {
        self.events.lock().is_empty()
    }
}

impl LaneCaptureSink for InMemoryLaneCaptureSink {
    fn record(&self, event: LaneCaptureEvent) {
        let mut g = self.events.lock();
        if g.len() == self.capacity {
            g.pop_front();
        }
        g.push_back(event);
    }
}

/// The coordinator. Holds the lane map + the registries it composes.
pub struct InferenceCoordinator {
    footprint: Arc<FootprintRegistry>,
    handle_store: Arc<InferenceHandleStore>,
    config: CoordinatorConfig,
    lanes: DashMap<Uuid, Lane>,
    /// Monotonic counter for lease IDs — paired with a UUID
    /// suffix so lease IDs are unique even across coordinator
    /// instances. Atomic so open_lane is lock-free on the hot
    /// path.
    lease_counter: AtomicU64,
    /// Capture sink for lane lifecycle events. Default = Noop
    /// (zero overhead). Swap via `with_capture_sink` at construction.
    capture_sink: Arc<dyn LaneCaptureSink>,
}

impl InferenceCoordinator {
    pub fn new(
        footprint: Arc<FootprintRegistry>,
        handle_store: Arc<InferenceHandleStore>,
        config: CoordinatorConfig,
    ) -> Self {
        Self {
            footprint,
            handle_store,
            config,
            lanes: DashMap::new(),
            lease_counter: AtomicU64::new(0),
            capture_sink: Arc::new(NoopLaneCaptureSink),
        }
    }

    /// Construct with a non-Noop capture sink. Mechanic-shop /
    /// observers / tests pass their own sink here.
    pub fn with_capture_sink(mut self, sink: Arc<dyn LaneCaptureSink>) -> Self {
        self.capture_sink = sink;
        self
    }

    /// Open a lane: admission → lease + footprint acquire → handle
    /// store open → bind lane.
    ///
    /// Failure at any step leaves the coordinator in a consistent
    /// state (no partial lane / lease leak). Adapter Arc is dropped
    /// on failure paths; the handle store entry is closed on lease
    /// errors after the handle was already opened — that doesn't
    /// happen in the current code path because we open the handle
    /// LAST, but the invariant should hold even after Step 4.
    pub fn open_lane(&self, req: OpenLaneRequest) -> Result<HandleRef, CoordinatorError> {
        let class = req
            .class_override
            .unwrap_or_else(|| LaneClass::default_for_task(req.task));
        let seed_tokens = req.task.default_seed_tokens();
        // Cost units = tokens (1:1). Simple + maps directly to the
        // admission planner's per-lane max_cost_units.
        let cost_units = seed_tokens;
        let bytes = (seed_tokens as u64).saturating_mul(self.config.bytes_per_token);

        // ── Step A: admission ────────────────────────────────────
        let job_id = format!(
            "{}:{}:{}",
            req.persona.as_uuid(),
            task_kind_str(req.task),
            req.now_ms
        );
        let job = ThroughputJob {
            job_id: job_id.clone(),
            artifact_key: job_id.clone(),
            resource_class: ResourceClass::LocalGeneration,
            target_silicon: self.config.default_target_silicon,
            priority: 0,
            cost_units,
            dependency_keys: Vec::new(),
            created_at_ms: req.now_ms,
            stale_after_ms: 0,
        };
        // Pull existing leases' cost into the planner so admission
        // sees current load — sum cost_units already consumed at
        // this target_silicon.
        let existing_cost: u32 = self
            .lanes
            .iter()
            .filter(|entry| entry.value().lease().target_silicon == job.target_silicon)
            .map(|entry| entry.value().lease().cost_units)
            .sum();
        // Inject a placeholder job representing existing load so the
        // planner sees the full picture. (Existing leases aren't
        // tracked as jobs by the pure planner; we synthesize.)
        let occupancy_job = ThroughputJob {
            job_id: "__coordinator-occupancy__".to_string(),
            artifact_key: "__coordinator-occupancy__".to_string(),
            resource_class: ResourceClass::LocalGeneration,
            target_silicon: job.target_silicon,
            priority: u32::MAX, // wins ordering so it gets admitted first
            cost_units: existing_cost,
            dependency_keys: Vec::new(),
            created_at_ms: 0,
            stale_after_ms: 0,
        };
        let admission_req = AdaptiveThroughputRequest {
            ready_artifact_keys: Vec::new(),
            lane_budgets: self.config.lane_budgets.clone(),
            jobs: if existing_cost > 0 {
                vec![occupancy_job, job.clone()]
            } else {
                vec![job.clone()]
            },
            now_ms: req.now_ms,
        };
        let plan = plan_adaptive_throughput(admission_req);
        if !plan.admitted.iter().any(|j| j.job_id == job_id) {
            // The new job wasn't admitted. Classify why.
            let reason = if plan.dropped_no_budget.iter().any(|j| j.job_id == job_id) {
                AdmissionDenyReason::NoBudget
            } else if plan
                .deferred_resource_pressure
                .iter()
                .any(|j| j.job_id == job_id)
            {
                AdmissionDenyReason::ResourcePressure
            } else if plan.dropped_stale.iter().any(|j| j.job_id == job_id) {
                AdmissionDenyReason::Stale
            } else {
                AdmissionDenyReason::ResourcePressure
            };
            self.capture_sink
                .record(LaneCaptureEvent::LaneAdmissionDenied {
                    captured_at_ms: req.now_ms,
                    persona: req.persona,
                    task: req.task,
                    reason,
                    cost_units_requested: cost_units,
                    target_silicon: job.target_silicon,
                });
            return Err(CoordinatorError::AdmissionDenied {
                reason,
                task: req.task,
                persona: req.persona,
            });
        }

        // ── Step B: lease + footprint ────────────────────────────
        let lease_seq = self.lease_counter.fetch_add(1, Ordering::Relaxed);
        let lease_id = format!("lane-lease-{lease_seq}-{}", Uuid::new_v4());
        let lease = ThroughputLease {
            lease_id: lease_id.clone(),
            artifact_key: job.artifact_key.clone(),
            resource_class: ResourceClass::LocalGeneration,
            target_silicon: job.target_silicon,
            holder_id: req.persona.as_uuid().to_string(),
            cost_units,
            acquired_at_ms: req.now_ms,
            expires_at_ms: req.now_ms.saturating_add(self.config.lease_duration_ms),
            revocation_policy: class.revocation_policy(),
        };
        let key = FootprintKey::for_persona(
            req.persona.as_uuid(),
            ResourceType::KvCache,
            Residency::Active,
        );
        self.footprint
            .acquire_lease(lease.clone(), key, bytes, req.now_ms)
            .map_err(|e| CoordinatorError::LeaseAcquireFailed(format!("{e:?}")))?;

        // ── Step C: open handle ──────────────────────────────────
        let handle = self.handle_store.open(
            req.adapter,
            OpenSessionRequest {
                model: req.model,
                system_prompt: req.system_prompt,
                active_adapters: req.active_adapters,
                persona_id: Some(req.persona.as_uuid()),
            },
        );

        // ── Step D: bind lane ────────────────────────────────────
        let lane = Lane::new(req.persona, req.task, lease, handle.id.as_uuid(), class);
        let lease_id_for_event = lane.lease_id().to_string();
        let target_silicon_for_event = lane.lease().target_silicon;
        self.lanes.insert(handle.id.as_uuid(), lane);

        self.capture_sink.record(LaneCaptureEvent::LaneOpened {
            captured_at_ms: req.now_ms,
            persona: req.persona,
            task: req.task,
            class,
            handle_id: handle.id.as_uuid(),
            lease_id: lease_id_for_event,
            cost_units,
            bytes_accounted: bytes,
            target_silicon: target_silicon_for_event,
        });
        Ok(handle)
    }

    /// Close a lane: release footprint+lease + remove lane + close
    /// handle. Idempotent — closing an already-closed handle is OK
    /// (returns Ok(false)).
    pub fn close_lane(&self, handle: &HandleRef) -> Result<bool, CoordinatorError> {
        let Some((_, lane)) = self.lanes.remove(&handle.id.as_uuid()) else {
            self.capture_sink.record(LaneCaptureEvent::LaneClosed {
                captured_at_ms: now_ms_for_capture(),
                persona: PeerId::from_uuid(Uuid::nil()),
                task: TaskKind::Chat,
                handle_id: handle.id.as_uuid(),
                lease_id: String::new(),
                was_present: false,
            });
            return Ok(false);
        };
        let lease_id = lane.lease_id().to_string();
        let persona = lane.persona();
        let task = lane.task();
        let _ = self.footprint.release_lease(&lease_id);
        let _ = self.handle_store.close(handle);
        self.capture_sink.record(LaneCaptureEvent::LaneClosed {
            captured_at_ms: now_ms_for_capture(),
            persona,
            task,
            handle_id: handle.id.as_uuid(),
            lease_id,
            was_present: true,
        });
        Ok(true)
    }

    /// Pressure-driven eviction walk. Releases lanes until
    /// `target_bytes` of accounted KV cache is freed, OR the walk
    /// exhausts non-pinned evictable lanes.
    ///
    /// Order:
    /// 1. Expired leases first, oldest first (any class — expired
    ///    realtime leases fall here, NOT under PressureHard).
    /// 2. `Hard` revocation policy lanes (Background, Sentinel),
    ///    oldest first by lease acquisition time.
    /// 3. `Graceful` revocation policy lanes (Interactive),
    ///    oldest first.
    /// 4. `Pinned` lanes (Realtime) are NEVER targeted by pressure.
    ///    Expired realtime leases get hit by step 1, not by
    ///    pressure-class targeting.
    ///
    /// Returns `EvictionResult` with the evicted lanes + bytes freed
    /// + bytes_short (target - freed, zero when target met). Emits
    /// `LaneCaptureEvent::LaneEvicted` for each lane per
    /// [[observability-is-half-the-architecture]].
    ///
    /// **Critical: pinned lanes are NEVER evicted by pressure.**
    /// The prior-attempt failure mode was hot-path adapter swap on
    /// active conversations. The `Pinned` revocation policy is the
    /// substrate's contract that the realtime lane stays warm until
    /// the conversation ends. Operator's escape valve: lease
    /// expiry — when a pinned lease expires (lease.expires_at_ms <
    /// now_ms), step 1 collects it like any other expired lease.
    ///
    /// The tier ordering above is NOT re-encoded here — it comes from the
    /// single revocation-ladder definition,
    /// [`disruption_rank`](crate::paging::lease_revocation::disruption_rank),
    /// shared with `select_leases_to_revoke`. This method is one
    /// *selection strategy* over that ladder (oldest-first, ungated,
    /// lane-aware); the broker-style strategy is another.
    pub fn evict_under_pressure(&self, target_bytes: u64, now_ms: u64) -> EvictionResult {
        // Snapshot lane references so we don't hold DashMap entries
        // while we mutate. We need (handle_id, lease_acquired_at_ms,
        // class, revocation rank, bytes_to_free).
        //
        // `rank` comes from the SINGLE revocation-ladder definition
        // (`disruption_rank`) — NOT a parallel inline class→tier match.
        // `disruption_rank` reads `lease.revocation_policy`, which
        // `open_lane` sets from `LaneClass::revocation_policy()`, so the
        // ranks are identical to the former inline tier() (expired=0,
        // Hard=1, Graceful=2) while keeping the ladder defined once. An
        // active `Pinned` lane returns `None` here and is filtered out
        // entirely (the substrate contract: pressure never evicts it).
        struct EvictCandidate {
            handle_id: Uuid,
            acquired_at_ms: u64,
            rank: u8,
            bytes: u64,
        }
        let bytes_per_token = self.config.bytes_per_token;
        let mut candidates: Vec<EvictCandidate> = self
            .lanes
            .iter()
            .filter_map(|entry| {
                let lane = entry.value();
                // None = active Pinned (Realtime, unexpired) → never a
                // pressure-eviction candidate. Expired leases of any
                // policy rank 0 (their holder is gone).
                let rank = disruption_rank(lane.lease(), now_ms)?;
                Some(EvictCandidate {
                    handle_id: lane.handle_id(),
                    acquired_at_ms: lane.lease().acquired_at_ms,
                    rank,
                    bytes: (lane.seed_kv_tokens() as u64).saturating_mul(bytes_per_token),
                })
            })
            .collect();

        // Least-disruptive first (rank ascending: expired → Hard →
        // Graceful), and within a rank oldest-first by acquisition time —
        // the coordinator's fairness choice (drain the longest-running
        // lane of a class before younger ones).
        candidates.sort_by(|a, b| {
            a.rank
                .cmp(&b.rank)
                .then(a.acquired_at_ms.cmp(&b.acquired_at_ms))
        });

        let mut evicted = Vec::new();
        let mut bytes_freed: u64 = 0;
        for cand in candidates {
            if bytes_freed >= target_bytes {
                break;
            }
            // Reason derives from the shared rank: 0 = expired lease
            // reclaim, 1 = Hard (Background/Sentinel) under pressure,
            // 2 = Graceful (Interactive) under pressure.
            let reason = match cand.rank {
                0 => EvictionReason::LeaseExpired,
                1 => EvictionReason::PressureHard,
                _ => EvictionReason::PressureGraceful,
            };

            // Snapshot the lane's persona + task + lease_id BEFORE
            // we remove it from the map, then close + emit.
            let Some((_, lane)) = self.lanes.remove(&cand.handle_id) else {
                continue;
            };
            let persona = lane.persona();
            let task = lane.task();
            let class = lane.class();
            let lease_id = lane.lease_id().to_string();
            let bytes_freed_for_lane = cand.bytes;

            let _ = self.footprint.release_lease(&lease_id);
            // Best-effort handle store close. The session-side close
            // can't fail unrecoverably; we don't propagate.
            let handle_ref = HandleRef {
                owner: crate::inference::handle_store::HANDLE_OWNER.to_string(),
                id: cand.handle_id.into(),
                type_tag: crate::inference::handle_store::HANDLE_TYPE_TAG.to_string(),
                created_at_ms: lane.lease().acquired_at_ms,
            };
            let _ = self.handle_store.close(&handle_ref);

            bytes_freed = bytes_freed.saturating_add(bytes_freed_for_lane);
            self.capture_sink.record(LaneCaptureEvent::LaneEvicted {
                captured_at_ms: now_ms,
                persona,
                task,
                class,
                handle_id: cand.handle_id,
                lease_id,
                bytes_freed: bytes_freed_for_lane,
                reason,
            });
            evicted.push(EvictedLane {
                handle_id: cand.handle_id,
                persona,
                task,
                class,
                bytes_freed: bytes_freed_for_lane,
                reason,
            });
        }
        let bytes_short = target_bytes.saturating_sub(bytes_freed);
        EvictionResult {
            evicted,
            bytes_freed,
            bytes_short,
        }
    }

    /// Get a snapshot of one lane's state. Used by the handle
    /// module's inspect command per
    /// [[observability-is-half-the-architecture]].
    pub fn inspect(&self, handle: &HandleRef) -> Option<LaneInspection> {
        self.lanes.get(&handle.id.as_uuid()).map(|entry| {
            let lane = entry.value();
            let bytes = (lane.seed_kv_tokens() as u64).saturating_mul(self.config.bytes_per_token);
            LaneInspection {
                persona: lane.persona(),
                task: lane.task(),
                class: lane.class(),
                handle_id: lane.handle_id(),
                seed_kv_tokens: lane.seed_kv_tokens(),
                max_kv_tokens: lane.max_kv_tokens(),
                bytes_accounted: bytes,
                lease_id: lane.lease_id().to_string(),
                lease_acquired_at_ms: lane.lease().acquired_at_ms,
                lease_expires_at_ms: lane.lease().expires_at_ms,
                is_pinned: lane.is_pinned(),
            }
        })
    }

    /// Snapshot of one lane (clone) — used by tests + the handle
    /// module for delegation.
    pub fn lane_for_handle(&self, handle: &HandleRef) -> Option<Lane> {
        self.lanes
            .get(&handle.id.as_uuid())
            .map(|e| e.value().clone())
    }

    pub fn lane_count(&self) -> usize {
        self.lanes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lanes.is_empty()
    }

    /// Coordinator config (read-only view). Used by the
    /// CoordinatorResourcePool wrapper to compute capacity bytes
    /// for PressureBroker integration.
    pub fn config(&self) -> &CoordinatorConfig {
        &self.config
    }

    /// Total bytes currently accounted across all active lanes —
    /// sum of `seed_kv_tokens × bytes_per_token` per lane. Mirrors
    /// what `FootprintRegistry::total_bytes()` reports for the
    /// KvCache resource type, but limited to this coordinator's
    /// lanes (other adapters / other coordinators on the same
    /// process have their own footprint slots).
    pub fn lanes_usage_bytes(&self) -> u64 {
        let bytes_per_token = self.config.bytes_per_token;
        self.lanes
            .iter()
            .map(|entry| (entry.value().seed_kv_tokens() as u64).saturating_mul(bytes_per_token))
            .sum()
    }

    /// Total capacity in bytes the coordinator's lane budgets can
    /// theoretically host — sum of `lane_budget.max_cost_units ×
    /// bytes_per_token` across configured budgets. Used by the
    /// PressureBroker wrapper.
    pub fn capacity_bytes(&self) -> u64 {
        let bytes_per_token = self.config.bytes_per_token;
        self.config
            .lane_budgets
            .iter()
            .map(|b| (b.max_cost_units as u64).saturating_mul(bytes_per_token))
            .sum()
    }

    /// One entry per active lane, in the shape PressureBroker /
    /// dashboards expect (per `paging::pool::ResourcePoolEntry`).
    pub fn lanes_snapshot(&self) -> Vec<crate::paging::pool::ResourcePoolEntry> {
        let bytes_per_token = self.config.bytes_per_token;
        self.lanes
            .iter()
            .map(|entry| {
                let lane = entry.value();
                let size_bytes = (lane.seed_kv_tokens() as u64).saturating_mul(bytes_per_token);
                crate::paging::pool::ResourcePoolEntry {
                    key: lane.handle_id().to_string(),
                    size_bytes,
                    pinned_count: if lane.is_pinned() { 1 } else { 0 },
                    loaded_at: lane.lease().acquired_at_ms,
                    last_access_at: lane.lease().acquired_at_ms,
                    access_count: 0,
                }
            })
            .collect()
    }

    /// Borrow the inner handle store. The handle module uses this
    /// to dispatch generate calls without going through the
    /// coordinator (generation isn't a coordinator concern in Step
    /// 2; Step 4 wires the batched-decode path).
    pub fn handle_store(&self) -> Arc<InferenceHandleStore> {
        self.handle_store.clone()
    }
}

fn now_ms_for_capture() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn task_kind_str(t: TaskKind) -> &'static str {
    match t {
        TaskKind::Chat => "chat",
        TaskKind::VoiceChat => "voice_chat",
        TaskKind::VideoChat => "video_chat",
        TaskKind::CodingSmall => "coding_small",
        TaskKind::CodingLarge => "coding_large",
        TaskKind::GameNpcIdle => "game_npc_idle",
        TaskKind::GameNpcEngaged => "game_npc_engaged",
        TaskKind::SentinelEasy => "sentinel_easy",
        TaskKind::SentinelHard => "sentinel_hard",
        TaskKind::AcademyStudent => "academy_student",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::heuristic_adapter::HeuristicInferenceAdapter;

    fn persona(id: u128) -> PeerId {
        PeerId::from_uuid(Uuid::from_u128(id))
    }

    /// what this catches: the governor→coordinator silicon translation. A
    /// discrete GPU (NvidiaCuda/AmdRocm/IntelVulkan) MUST map to `Gpu` (not
    /// the UnifiedMemory floor) — that's the GPU-or-bust fix. AppleM →
    /// UnifiedMemory; `None` → `Cpu` (honest, not a silent GPU pretense).
    #[test]
    fn governor_silicon_maps_discrete_gpu_to_gpu_not_floor() {
        assert_eq!(
            coordinator_silicon_for(GovernorSilicon::NvidiaCuda),
            TargetSilicon::Gpu
        );
        assert_eq!(
            coordinator_silicon_for(GovernorSilicon::AmdRocm),
            TargetSilicon::Gpu
        );
        assert_eq!(
            coordinator_silicon_for(GovernorSilicon::IntelVulkan),
            TargetSilicon::Gpu
        );
        assert_eq!(
            coordinator_silicon_for(GovernorSilicon::AppleM),
            TargetSilicon::UnifiedMemory
        );
        assert_eq!(
            coordinator_silicon_for(GovernorSilicon::None),
            TargetSilicon::Cpu
        );
    }

    /// what this catches: `for_silicon` sets BOTH the lane-budget silicon AND
    /// the default to the SAME class — a mismatch would deny admission (the
    /// planner looks up the budget by the lease's target_silicon). Pins that
    /// a Gpu config targets Gpu end-to-end, not a stray UnifiedMemory.
    #[test]
    fn for_silicon_targets_one_silicon_end_to_end() {
        let cfg = CoordinatorConfig::for_silicon(TargetSilicon::Gpu);
        assert_eq!(cfg.default_target_silicon, TargetSilicon::Gpu);
        assert_eq!(cfg.lane_budgets.len(), 1);
        assert_eq!(cfg.lane_budgets[0].target_silicon, TargetSilicon::Gpu);
        // realistic_floor_default is now just for_silicon(UnifiedMemory).
        let floor = CoordinatorConfig::realistic_floor_default();
        assert_eq!(floor.default_target_silicon, TargetSilicon::UnifiedMemory);
        assert_eq!(
            floor.lane_budgets[0].target_silicon,
            TargetSilicon::UnifiedMemory
        );
    }

    /// what this catches: `detected()` runs to completion without panicking
    /// on the test host (the probe→classify→translate chain), and the result
    /// it produces is self-consistent. The end-to-end consistency invariant
    /// itself is owned by `for_silicon_targets_one_silicon_end_to_end`; this
    /// is the smoke test that the real hardware-probe path doesn't blow up.
    #[test]
    fn detected_config_runs_without_panicking() {
        let cfg = CoordinatorConfig::detected();
        assert_eq!(
            cfg.default_target_silicon,
            cfg.lane_budgets[0].target_silicon
        );
    }

    fn small_budget_config() -> CoordinatorConfig {
        // Tight budgets so tests can exercise admission deny without
        // having to set up production-sized numbers.
        CoordinatorConfig {
            lane_budgets: vec![ThroughputLaneBudget {
                resource_class: ResourceClass::LocalGeneration,
                target_silicon: TargetSilicon::Cpu,
                // Cap at 2 concurrent lanes to exercise concurrency
                // backpressure.
                max_concurrency: 2,
                // Cap at 20K cost_units (≈ 2× Chat lanes worth).
                max_cost_units: 20_000,
            }],
            // Tiny per-token bytes so footprint stays trivial in tests.
            bytes_per_token: 1,
            lease_duration_ms: 60_000,
            default_target_silicon: TargetSilicon::Cpu,
        }
    }

    fn build_coordinator() -> InferenceCoordinator {
        let footprint = Arc::new(FootprintRegistry::new());
        let handle_store = Arc::new(InferenceHandleStore::new());
        InferenceCoordinator::new(footprint, handle_store, small_budget_config())
    }

    fn open_chat(
        c: &InferenceCoordinator,
        persona_id: u128,
        now_ms: u64,
    ) -> Result<HandleRef, CoordinatorError> {
        c.open_lane(OpenLaneRequest {
            persona: persona(persona_id),
            task: TaskKind::Chat,
            adapter: Arc::new(HeuristicInferenceAdapter::new()),
            model: None,
            system_prompt: None,
            active_adapters: None,
            class_override: None,
            now_ms,
        })
    }

    // ── basic open + close ──────────────────────────────────────

    #[test]
    fn open_lane_admits_first_persona_returns_handle() {
        let c = build_coordinator();
        let h = open_chat(&c, 1, 1_000_000).unwrap();
        assert_eq!(c.lane_count(), 1);
        assert_eq!(h.owner, crate::inference::handle_store::HANDLE_OWNER);
    }

    #[test]
    fn lane_is_bound_to_handle_and_carries_persona_task_class() {
        let c = build_coordinator();
        let h = open_chat(&c, 1, 1_000_000).unwrap();
        let lane = c.lane_for_handle(&h).unwrap();
        assert_eq!(lane.persona(), persona(1));
        assert_eq!(lane.task(), TaskKind::Chat);
        assert_eq!(lane.class(), LaneClass::Interactive);
        assert_eq!(lane.handle_id(), h.id.as_uuid());
    }

    #[test]
    fn close_lane_releases_and_decrements_count() {
        let c = build_coordinator();
        let h = open_chat(&c, 1, 1_000_000).unwrap();
        assert_eq!(c.lane_count(), 1);
        assert!(c.close_lane(&h).unwrap());
        assert_eq!(c.lane_count(), 0);
        // Double-close is idempotent.
        assert!(!c.close_lane(&h).unwrap());
    }

    // ── admission ───────────────────────────────────────────────

    #[test]
    fn admission_denies_when_concurrency_exceeded() {
        let c = build_coordinator();
        // budget: max_concurrency=2 → first two admit, third denies
        open_chat(&c, 1, 1_000_000).unwrap();
        open_chat(&c, 2, 1_000_000).unwrap();
        let err = open_chat(&c, 3, 1_000_000).unwrap_err();
        match err {
            CoordinatorError::AdmissionDenied { reason, .. } => {
                assert_eq!(reason, AdmissionDenyReason::ResourcePressure);
            }
            other => panic!("expected AdmissionDenied, got {other:?}"),
        }
        assert_eq!(c.lane_count(), 2);
    }

    #[test]
    fn admission_denies_when_cost_units_exceeded() {
        // Two CodingLarge (128K each) blows past 20K max_cost_units.
        let c = build_coordinator();
        let _ = c
            .open_lane(OpenLaneRequest {
                persona: persona(1),
                task: TaskKind::CodingLarge,
                adapter: Arc::new(HeuristicInferenceAdapter::new()),
                model: None,
                system_prompt: None,
                active_adapters: None,
                class_override: None,
                now_ms: 1_000_000,
            })
            .unwrap_err();
        // Even the FIRST CodingLarge fails because its cost_units
        // (128K) exceeds the lane's max_cost_units (20K).
        assert_eq!(c.lane_count(), 0);
    }

    #[test]
    fn admission_denies_when_no_budget_for_silicon() {
        // Config only has Cpu budget; request UnifiedMemory.
        let mut config = small_budget_config();
        config.default_target_silicon = TargetSilicon::UnifiedMemory;
        // Don't add a UnifiedMemory lane budget — admission will say NoBudget.
        let footprint = Arc::new(FootprintRegistry::new());
        let handle_store = Arc::new(InferenceHandleStore::new());
        let c = InferenceCoordinator::new(footprint, handle_store, config);
        let err = open_chat(&c, 1, 1_000_000).unwrap_err();
        match err {
            CoordinatorError::AdmissionDenied { reason, .. } => {
                assert_eq!(reason, AdmissionDenyReason::NoBudget);
            }
            other => panic!("expected NoBudget, got {other:?}"),
        }
    }

    // ── three-persona realistic floor smoke ─────────────────────

    #[test]
    fn three_personas_concurrent_lanes_on_one_adapter_realistic_floor() {
        // The substrate's defining boast at the realistic floor.
        let mut cfg = small_budget_config();
        cfg.lane_budgets[0].max_concurrency = 3;
        cfg.lane_budgets[0].max_cost_units = 30_000;
        let c = InferenceCoordinator::new(
            Arc::new(FootprintRegistry::new()),
            Arc::new(InferenceHandleStore::new()),
            cfg,
        );
        // One shared heuristic adapter.
        let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
        let mk = |id, task| OpenLaneRequest {
            persona: persona(id),
            task,
            adapter: adapter.clone(),
            model: None,
            system_prompt: None,
            active_adapters: None,
            class_override: None,
            now_ms: 1_000_000,
        };
        let h1 = c.open_lane(mk(1, TaskKind::Chat)).unwrap();
        let h2 = c.open_lane(mk(2, TaskKind::VoiceChat)).unwrap();
        let h3 = c.open_lane(mk(3, TaskKind::GameNpcIdle)).unwrap();
        assert_eq!(c.lane_count(), 3);

        // Distinct lanes per persona × task.
        let l1 = c.lane_for_handle(&h1).unwrap();
        let l2 = c.lane_for_handle(&h2).unwrap();
        let l3 = c.lane_for_handle(&h3).unwrap();
        assert_eq!(l1.task(), TaskKind::Chat);
        assert_eq!(l2.task(), TaskKind::VoiceChat);
        assert_eq!(l3.task(), TaskKind::GameNpcIdle);

        // Class derives correctly per task.
        assert_eq!(l1.class(), LaneClass::Interactive);
        assert_eq!(l2.class(), LaneClass::Realtime); // pinned!
        assert_eq!(l3.class(), LaneClass::Background);

        // KV budgets per recipe table.
        assert_eq!(l1.seed_kv_tokens(), 8 * 1024);
        assert_eq!(l2.seed_kv_tokens(), 8 * 1024);
        assert_eq!(l3.seed_kv_tokens(), 4 * 1024);

        // Pinned status follows class.
        assert!(l2.is_pinned());
        assert!(!l1.is_pinned());
        assert!(!l3.is_pinned());
    }

    // ── observability + inspection ──────────────────────────────

    #[test]
    fn inspect_returns_full_snapshot_for_known_handle() {
        let c = build_coordinator();
        let h = open_chat(&c, 7, 1_500_000).unwrap();
        let inspection = c.inspect(&h).unwrap();
        assert_eq!(inspection.persona, persona(7));
        assert_eq!(inspection.task, TaskKind::Chat);
        assert_eq!(inspection.class, LaneClass::Interactive);
        assert_eq!(inspection.handle_id, h.id.as_uuid());
        assert_eq!(inspection.seed_kv_tokens, 8 * 1024);
        assert_eq!(inspection.max_kv_tokens, 16 * 1024);
        assert_eq!(inspection.bytes_accounted, 8 * 1024); // small config bytes_per_token=1
        assert_eq!(inspection.lease_acquired_at_ms, 1_500_000);
        assert_eq!(inspection.lease_expires_at_ms, 1_500_000 + 60_000);
        assert!(!inspection.is_pinned);
    }

    #[test]
    fn inspect_unknown_handle_returns_none() {
        let c = build_coordinator();
        let phantom = HandleRef::mint(
            crate::inference::handle_store::HANDLE_OWNER,
            crate::inference::handle_store::HANDLE_TYPE_TAG,
        );
        assert!(c.inspect(&phantom).is_none());
    }

    // ── class override ──────────────────────────────────────────

    // ── capture sink ────────────────────────────────────────────

    #[test]
    fn capture_sink_records_lane_opened_event_on_successful_open() {
        let sink = Arc::new(InMemoryLaneCaptureSink::new(64));
        let c = InferenceCoordinator::new(
            Arc::new(FootprintRegistry::new()),
            Arc::new(InferenceHandleStore::new()),
            small_budget_config(),
        )
        .with_capture_sink(sink.clone());
        let h = open_chat(&c, 1, 1_000_000).unwrap();
        let events = sink.snapshot();
        assert_eq!(events.len(), 1);
        match &events[0] {
            LaneCaptureEvent::LaneOpened {
                persona: p,
                task,
                class,
                handle_id,
                cost_units,
                target_silicon,
                ..
            } => {
                assert_eq!(*p, persona(1));
                assert_eq!(*task, TaskKind::Chat);
                assert_eq!(*class, LaneClass::Interactive);
                assert_eq!(*handle_id, h.id.as_uuid());
                assert_eq!(*cost_units, 8 * 1024);
                assert_eq!(*target_silicon, TargetSilicon::Cpu);
            }
            other => panic!("expected LaneOpened, got {other:?}"),
        }
    }

    #[test]
    fn capture_sink_records_admission_denied_with_reason() {
        let sink = Arc::new(InMemoryLaneCaptureSink::new(64));
        let c = InferenceCoordinator::new(
            Arc::new(FootprintRegistry::new()),
            Arc::new(InferenceHandleStore::new()),
            small_budget_config(),
        )
        .with_capture_sink(sink.clone());
        open_chat(&c, 1, 1_000_000).unwrap();
        open_chat(&c, 2, 1_000_000).unwrap();
        // Third one denies.
        let _ = open_chat(&c, 3, 1_000_000).unwrap_err();
        let events = sink.snapshot();
        assert_eq!(events.len(), 3); // 2 opened + 1 denied
        match &events[2] {
            LaneCaptureEvent::LaneAdmissionDenied {
                reason,
                persona: p,
                task,
                ..
            } => {
                assert_eq!(*reason, AdmissionDenyReason::ResourcePressure);
                assert_eq!(*p, persona(3));
                assert_eq!(*task, TaskKind::Chat);
            }
            other => panic!("expected LaneAdmissionDenied, got {other:?}"),
        }
    }

    #[test]
    fn capture_sink_records_lane_closed_with_was_present_flag() {
        let sink = Arc::new(InMemoryLaneCaptureSink::new(64));
        let c = InferenceCoordinator::new(
            Arc::new(FootprintRegistry::new()),
            Arc::new(InferenceHandleStore::new()),
            small_budget_config(),
        )
        .with_capture_sink(sink.clone());
        let h = open_chat(&c, 7, 1_000_000).unwrap();
        sink.drain(); // forget the open event
        c.close_lane(&h).unwrap();
        c.close_lane(&h).unwrap(); // double close
        let events = sink.snapshot();
        assert_eq!(events.len(), 2);
        match &events[0] {
            LaneCaptureEvent::LaneClosed { was_present, .. } => assert!(*was_present),
            other => panic!("expected LaneClosed present, got {other:?}"),
        }
        match &events[1] {
            LaneCaptureEvent::LaneClosed { was_present, .. } => assert!(!*was_present),
            other => panic!("expected LaneClosed absent, got {other:?}"),
        }
    }

    #[test]
    fn noop_sink_drops_events_without_panic_or_alloc() {
        // Same workload as the previous test but with the default
        // Noop sink. Just verify it doesn't panic + the coordinator
        // works identically.
        let c = build_coordinator();
        let h = open_chat(&c, 1, 1_000_000).unwrap();
        assert!(c.close_lane(&h).unwrap());
    }

    #[test]
    fn in_memory_sink_capacity_drops_oldest() {
        let sink = InMemoryLaneCaptureSink::new(2);
        sink.record(LaneCaptureEvent::LaneOpened {
            captured_at_ms: 1,
            persona: persona(1),
            task: TaskKind::Chat,
            class: LaneClass::Interactive,
            handle_id: Uuid::nil(),
            lease_id: "a".to_string(),
            cost_units: 1,
            bytes_accounted: 1,
            target_silicon: TargetSilicon::Cpu,
        });
        sink.record(LaneCaptureEvent::LaneOpened {
            captured_at_ms: 2,
            persona: persona(2),
            task: TaskKind::Chat,
            class: LaneClass::Interactive,
            handle_id: Uuid::nil(),
            lease_id: "b".to_string(),
            cost_units: 1,
            bytes_accounted: 1,
            target_silicon: TargetSilicon::Cpu,
        });
        sink.record(LaneCaptureEvent::LaneOpened {
            captured_at_ms: 3,
            persona: persona(3),
            task: TaskKind::Chat,
            class: LaneClass::Interactive,
            handle_id: Uuid::nil(),
            lease_id: "c".to_string(),
            cost_units: 1,
            bytes_accounted: 1,
            target_silicon: TargetSilicon::Cpu,
        });
        // Capacity 2 → first event evicted.
        let events = sink.snapshot();
        assert_eq!(events.len(), 2);
        match &events[0] {
            LaneCaptureEvent::LaneOpened { lease_id, .. } => assert_eq!(lease_id, "b"),
            other => panic!("expected lease 'b', got {other:?}"),
        }
    }

    // ── pressure-driven eviction (step 5) ───────────────────────

    fn open_with_class(
        c: &InferenceCoordinator,
        persona_id: u128,
        task: TaskKind,
        class: LaneClass,
        now_ms: u64,
    ) -> HandleRef {
        c.open_lane(OpenLaneRequest {
            persona: persona(persona_id),
            task,
            adapter: Arc::new(HeuristicInferenceAdapter::new()),
            model: None,
            system_prompt: None,
            active_adapters: None,
            class_override: Some(class),
            now_ms,
        })
        .unwrap()
    }

    fn open_chat_now(c: &InferenceCoordinator, persona_id: u128, now_ms: u64) -> HandleRef {
        c.open_lane(OpenLaneRequest {
            persona: persona(persona_id),
            task: TaskKind::Chat,
            adapter: Arc::new(HeuristicInferenceAdapter::new()),
            model: None,
            system_prompt: None,
            active_adapters: None,
            class_override: None,
            now_ms,
        })
        .unwrap()
    }

    fn eviction_config() -> CoordinatorConfig {
        // Generous concurrency so we can open many lanes before
        // evicting; modest cost_units so multiple lanes fit. Long
        // lease so a typical now=1.5M wall-clock test stays within
        // the lease window (the expired-lease test uses now past
        // acquisition + lease_duration to force expiry).
        CoordinatorConfig {
            lane_budgets: vec![ThroughputLaneBudget {
                resource_class: ResourceClass::LocalGeneration,
                target_silicon: TargetSilicon::Cpu,
                max_concurrency: 16,
                max_cost_units: 200_000,
            }],
            // 1 byte per token so bytes-freed numbers in tests are
            // just seed_kv_tokens (8K Chat → 8192 bytes etc.).
            bytes_per_token: 1,
            lease_duration_ms: 5_000_000,
            default_target_silicon: TargetSilicon::Cpu,
        }
    }

    fn build_eviction_coordinator() -> InferenceCoordinator {
        InferenceCoordinator::new(
            Arc::new(FootprintRegistry::new()),
            Arc::new(InferenceHandleStore::new()),
            eviction_config(),
        )
    }

    #[test]
    fn evict_under_pressure_does_not_touch_pinned_realtime_lane() {
        let c = build_eviction_coordinator();
        // 1 realtime (pinned), 1 background — evict 100MB of pressure.
        let realtime = open_with_class(&c, 1, TaskKind::VoiceChat, LaneClass::Realtime, 1_000_000);
        let _background = open_with_class(
            &c,
            2,
            TaskKind::CodingSmall,
            LaneClass::Background,
            1_000_000,
        );
        let result = c.evict_under_pressure(100_000_000, 1_500_000);
        assert_eq!(result.evicted.len(), 1);
        assert_eq!(result.evicted[0].class, LaneClass::Background);
        // Realtime lane survives.
        assert!(c.lane_for_handle(&realtime).is_some());
        // Only background's bytes were freed (CodingSmall = 32K tokens).
        assert_eq!(result.bytes_freed, 32 * 1024);
        assert!(result.bytes_short > 0); // didn't reach 100MB target
    }

    #[test]
    fn evict_under_pressure_prefers_hard_then_graceful() {
        let c = build_eviction_coordinator();
        // 1 Interactive (Graceful) + 1 Background (Hard) + 1 Sentinel (Hard).
        let _interactive =
            open_with_class(&c, 1, TaskKind::Chat, LaneClass::Interactive, 1_000_000);
        let _background = open_with_class(
            &c,
            2,
            TaskKind::CodingSmall,
            LaneClass::Background,
            1_000_000,
        );
        let _sentinel = open_with_class(
            &c,
            3,
            TaskKind::SentinelEasy,
            LaneClass::Sentinel,
            1_000_000,
        );
        // Evict just one lane's worth (small budget).
        let result = c.evict_under_pressure(1, 1_500_000);
        assert_eq!(result.evicted.len(), 1);
        // First evicted = Hard (Background or Sentinel — older first within tier).
        assert!(matches!(
            result.evicted[0].reason,
            EvictionReason::PressureHard
        ));
        assert!(matches!(
            result.evicted[0].class,
            LaneClass::Background | LaneClass::Sentinel
        ));
    }

    #[test]
    fn evict_under_pressure_picks_oldest_within_same_tier() {
        let c = build_eviction_coordinator();
        // Two Background lanes, different acquired_at_ms.
        let _old = open_with_class(
            &c,
            1,
            TaskKind::CodingSmall,
            LaneClass::Background,
            1_000_000,
        );
        let _new = open_with_class(
            &c,
            2,
            TaskKind::CodingSmall,
            LaneClass::Background,
            2_000_000,
        );
        let result = c.evict_under_pressure(1, 3_000_000);
        assert_eq!(result.evicted.len(), 1);
        // Older lane (persona 1, acquired at 1M) gets evicted first.
        assert_eq!(result.evicted[0].persona, persona(1));
    }

    #[test]
    fn evict_under_pressure_collects_expired_first_even_pinned() {
        // Need ONE lane expired and one active so pressure-priority
        // would normally pick the active background, but expired
        // priority MUST pick the realtime first.
        let c = build_eviction_coordinator();
        // Realtime opens at 1M with 5M lease → expires at 6M.
        let _realtime = open_with_class(&c, 1, TaskKind::VoiceChat, LaneClass::Realtime, 1_000_000);
        // Background opens at 5M with 5M lease → expires at 10M.
        let _background = open_with_class(
            &c,
            2,
            TaskKind::CodingSmall,
            LaneClass::Background,
            5_000_000,
        );
        // Evict at 7M: realtime expired, background still active.
        let result = c.evict_under_pressure(1, 7_000_000);
        assert_eq!(result.evicted.len(), 1);
        assert_eq!(result.evicted[0].class, LaneClass::Realtime);
        assert_eq!(result.evicted[0].reason, EvictionReason::LeaseExpired);
    }

    #[test]
    fn evict_under_pressure_stops_when_target_met() {
        let c = build_eviction_coordinator();
        // 3 Background lanes, each 32K tokens = 32K bytes (with
        // bytes_per_token=1).
        for i in 1..=3 {
            open_with_class(
                &c,
                i,
                TaskKind::CodingSmall,
                LaneClass::Background,
                1_000_000,
            );
        }
        // Target 33K bytes — enough for 2 lanes but not 3.
        let result = c.evict_under_pressure(33_000, 1_500_000);
        assert_eq!(result.evicted.len(), 2);
        assert_eq!(result.bytes_freed, 64 * 1024);
        assert_eq!(result.bytes_short, 0);
        // Third lane still present.
        assert_eq!(c.lane_count(), 1);
    }

    #[test]
    fn evict_under_pressure_reports_bytes_short_when_all_pinned() {
        let c = build_eviction_coordinator();
        // 3 Realtime lanes — pressure can't touch any.
        for i in 1..=3 {
            open_with_class(&c, i, TaskKind::VoiceChat, LaneClass::Realtime, 1_000_000);
        }
        let target = 1_000_000;
        let result = c.evict_under_pressure(target, 1_500_000);
        assert_eq!(result.evicted.len(), 0);
        assert_eq!(result.bytes_freed, 0);
        assert_eq!(result.bytes_short, target);
        assert_eq!(c.lane_count(), 3); // all still present
    }

    #[test]
    fn evict_under_pressure_emits_lane_evicted_capture_with_reason() {
        let sink = Arc::new(InMemoryLaneCaptureSink::new(64));
        let c = InferenceCoordinator::new(
            Arc::new(FootprintRegistry::new()),
            Arc::new(InferenceHandleStore::new()),
            eviction_config(),
        )
        .with_capture_sink(sink.clone());
        let _ = open_chat_now(&c, 1, 1_000_000); // Interactive (Graceful)
        let _ = open_with_class(
            &c,
            2,
            TaskKind::CodingSmall,
            LaneClass::Background,
            1_000_000,
        ); // Hard
        sink.drain(); // forget the LaneOpened events
        let _result = c.evict_under_pressure(1, 1_500_000);
        let events = sink.snapshot();
        assert_eq!(events.len(), 1);
        match &events[0] {
            LaneCaptureEvent::LaneEvicted {
                reason,
                class,
                bytes_freed,
                ..
            } => {
                assert_eq!(*reason, EvictionReason::PressureHard);
                assert_eq!(*class, LaneClass::Background);
                assert_eq!(*bytes_freed, 32 * 1024);
            }
            other => panic!("expected LaneEvicted, got {other:?}"),
        }
    }

    #[test]
    fn evict_under_pressure_with_zero_target_evicts_nothing() {
        let c = build_eviction_coordinator();
        for i in 1..=3 {
            open_chat_now(&c, i, 1_000_000);
        }
        let result = c.evict_under_pressure(0, 1_500_000);
        assert_eq!(result.evicted.len(), 0);
        assert_eq!(result.bytes_freed, 0);
        assert_eq!(result.bytes_short, 0);
        assert_eq!(c.lane_count(), 3);
    }

    #[test]
    fn evict_under_pressure_on_empty_coordinator_is_noop() {
        let c = build_eviction_coordinator();
        let result = c.evict_under_pressure(1_000_000, 1_500_000);
        assert_eq!(result.evicted.len(), 0);
        assert_eq!(result.bytes_freed, 0);
        assert_eq!(result.bytes_short, 1_000_000);
    }

    #[test]
    fn evict_realistic_floor_scenario_three_personas_one_must_yield() {
        // The substrate's defining boast under pressure:
        // 3 lanes (Realtime/Interactive/Background), pressure says
        // free at least 4K bytes. The Background lane yields; the
        // Realtime + Interactive lanes stay warm. This is the
        // multi-persona-on-commodity-hardware story under load.
        let c = build_eviction_coordinator();
        let realtime = open_with_class(&c, 1, TaskKind::VoiceChat, LaneClass::Realtime, 1_000_000);
        let interactive = open_with_class(&c, 2, TaskKind::Chat, LaneClass::Interactive, 1_000_000);
        let _background = open_with_class(
            &c,
            3,
            TaskKind::GameNpcIdle,
            LaneClass::Background,
            1_000_000,
        );
        let result = c.evict_under_pressure(4 * 1024, 1_500_000);
        assert_eq!(result.evicted.len(), 1);
        assert_eq!(result.evicted[0].class, LaneClass::Background);
        assert_eq!(result.evicted[0].reason, EvictionReason::PressureHard);
        // Realtime + Interactive survive.
        assert!(c.lane_for_handle(&realtime).is_some());
        assert!(c.lane_for_handle(&interactive).is_some());
        assert_eq!(c.lane_count(), 2);
    }

    #[test]
    fn class_override_lets_daemon_promote_chat_to_realtime() {
        // A daemon can promote a Chat lane to Realtime when the
        // persona is currently in a voice-engaged state.
        let c = build_coordinator();
        let req = OpenLaneRequest {
            persona: persona(1),
            task: TaskKind::Chat,
            adapter: Arc::new(HeuristicInferenceAdapter::new()),
            model: None,
            system_prompt: None,
            active_adapters: None,
            class_override: Some(LaneClass::Realtime),
            now_ms: 1_000_000,
        };
        let h = c.open_lane(req).unwrap();
        let lane = c.lane_for_handle(&h).unwrap();
        assert_eq!(lane.class(), LaneClass::Realtime);
        assert!(lane.is_pinned());
    }
}
