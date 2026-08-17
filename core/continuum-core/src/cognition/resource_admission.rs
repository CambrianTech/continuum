//! Shared Rust resource admission.
//!
//! This is the small lease gate that every expensive subsystem can use
//! while the substrate governor becomes the process-wide allocator:
//! inference, training, rendering, audio, TTS, STT, classifiers, RAG,
//! and background work. Callers submit typed resource policy; the gate
//! admits or denies before work starts and returns an RAII guard that
//! releases the lease on every exit path.

use crate::cognition::adaptive_throughput::{ResourceClass, TargetSilicon};
use crate::cognition::throughput_lease::{
    ThroughputLease, ThroughputLeaseError, ThroughputLeaseRegistry, ThroughputLeaseRevocationPolicy,
};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard};
use ts_rs::TS;

// ── Soft saturation signal (the read-only companion to the hard gate) ──────────
//
// Deliberative model calls currently outstanding against the shared serving target.
// A lock-free process-global gauge: the resource it measures — ONE shared
// llama-server with `serving_plan::MAX_LANES` decode slots serving the whole fleet
// via continuous batching — IS process-global, so the gauge granularity matches the
// resource exactly (no per-caller Arc threading buys any fidelity).
//
// This is a SOFT signal, not an allocator: it admits and denies NOTHING. The hard
// admission decision is `ResourceAdmissionGate` (above); this gauge exists because
// the live inference path is not yet lease-wired AND a background self-tick — the
// lowest-priority work in the system — needs a zero-cost read of "is every decode
// slot busy right now" to decide whether adding an idle deliberation would only
// deepen the queue that live conversation is already waiting behind. Glass-boxed
// 2026-07-15: one inbound message woke six minds, each ran a full ~54s deliberation,
// and the two lanes serialized them into a 250s tail (#139).
// [[conversational-latency-is-a-misdirection-budget]] [[idle-is-self-directed-free-time]]
static INFLIGHT_MODEL_CALLS: Gauge = Gauge::new();

/// The in-flight model-call gauge: a saturating count of outstanding deliberative model calls.
/// Extracted as a named type (not a bare static) for one reason beyond tidiness — testability.
/// The process has exactly ONE shared instance (`INFLIGHT_MODEL_CALLS`) that production RAII
/// guards bump; a unit test constructs its OWN isolated `Gauge` and drives the identical
/// enter/read/saturate logic against it, so its absolute-count assertions are deterministic
/// under `cargo test`'s parallel execution — no cross-module guard on the shared instance can
/// perturb a gauge nothing else can reach. (This is the #1960 flaky class killed at the root:
/// the previous test read the process-global counter as an absolute value while a sibling test's
/// `InflightModelCall` guard bumped it mid-loop; drain-before-baseline couldn't catch mid-loop
/// interference. Inject the gauge instead of draining around it.)
#[derive(Debug)]
struct Gauge {
    count: AtomicUsize,
}

impl Gauge {
    const fn new() -> Self {
        Self {
            count: AtomicUsize::new(0),
        }
    }

    /// Enter one in-flight call; the returned guard decrements the SAME gauge on drop.
    fn enter(&self) -> GaugeGuard<'_> {
        self.count.fetch_add(1, Ordering::AcqRel);
        GaugeGuard(&self.count)
    }

    /// Outstanding calls right now.
    fn inflight(&self) -> usize {
        self.count.load(Ordering::Acquire)
    }

    /// True when `outstanding >= max_lanes` — one more call would queue behind the fleet.
    fn saturated(&self, max_lanes: usize) -> bool {
        self.inflight() >= max_lanes
    }
}

/// RAII decrement for one gauge entry. Borrows the gauge it incremented so drop decrements
/// exactly that gauge — the shared instance for a production guard, a test's local instance for
/// a test's. This borrow is what makes the gauge injectable without a second counter.
#[derive(Debug)]
struct GaugeGuard<'a>(&'a AtomicUsize);

impl Drop for GaugeGuard<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

/// RAII marker: increments the shared in-flight gauge on entry and decrements on EVERY exit
/// path (Ok, Err, panic-unwind). The deliberation faculty wraps its single generate call in this
/// so the gauge reflects exactly the model-call window (lane-queue + prefill + decode) and
/// nothing downstream. Wraps a guard on the process-global `INFLIGHT_MODEL_CALLS`, so it carries
/// NO lifetime and stays storable as a plain field (e.g. `ServingLanePermit._inflight`).
#[derive(Debug)]
pub struct InflightModelCall(GaugeGuard<'static>);

impl InflightModelCall {
    pub fn enter() -> Self {
        Self(INFLIGHT_MODEL_CALLS.enter())
    }
}

/// Deliberative model calls outstanding against the shared serving target right now.
pub fn inflight_model_calls() -> usize {
    INFLIGHT_MODEL_CALLS.inflight()
}

/// True when every shared decode slot is busy, so one more call would QUEUE behind
/// the fleet. Threshold is the LIVE served lane count (`LANES.lane_count()` — the real
/// `--parallel` slots serving reports, `set_served_lane_count`), falling back to the
/// `MAX_LANES` backstop only before the first plan lands. This must track the live count,
/// not the `MAX_LANES` ceiling: once the ceiling can exceed the served count (#266 raised it
/// 2 → 8), keying saturation off the ceiling would let a 2-slot host admit 8 in-flight and
/// queue 6 at the backend before a self-tick ever yields. A self-tick yields on this;
/// message/directed turns are never gated on it (a human/peer waits).
pub fn shared_model_saturated() -> bool {
    INFLIGHT_MODEL_CALLS.saturated(LANES.lane_count())
}

// ── Ambient-turn admission permit (#171) ───────────────────────────────────────
//
// The inflight GAUGE above catches STAGGERED load, but not the acute fan-out: a room
// burst wakes N peers simultaneously and they all read the gauge as 0 (none has
// generated yet) and stampede. A PERMIT fixes the timing: an ambient (non-directed)
// turn must ACQUIRE one of a small fixed number of ambient slots or it yields — held
// across the turn, so concurrency is bounded regardless of when everyone woke.
// Directed turns bypass this entirely (they were addressed; they run now). A yielded
// ambient turn defers to a later beat with free capacity — the durable transcript is
// unchanged, so nothing is lost. [[conversational-latency-is-a-misdirection-budget]]

// How many ambient turns may run at once: [`LaneAdmission::nondirected_budget`] — the
// SAME live (lanes − 1, floored at 1) budget the per-call lane reservation below uses.
// One machine, one answer to "how much non-directed concurrency is there", derived from
// live capacity instead of declared twice.
//
// This was `const AMBIENT_TURN_CONCURRENCY: usize = 1` until 2026-08-17, and the constant
// was the roster's starvation CEILING. Two things made it invisible:
//   1. `service_loop.rs`'s self-tick gate documents this permit as "sized to the LIVE
//      served lane count (LaneAdmission ← set_served_lane_count)". It never was — it was
//      a bare 1, and the comment described the design that was intended.
//   2. A hard 1 is indistinguishable from a quiet room at n≈1 citizen, which is how it
//      was reasoned about ("under light load ambient turns are naturally serial").
// Measured on this box 2026-08-17: 4 served lanes, 20+ hosted citizens, so 3 non-directed
// lanes sat permanently idle while every citizen but one yielded on a pool of 1.
//
// Why lowering the bound is still safe — the directed-turn guarantee never came from THIS
// permit. It comes from the per-call reservation (`acquire_serving_lane`), which caps
// non-directed calls at lanes−1 so a directed call always finds a lane. That is the layer
// that owns lane priority, and it is untouched. This permit's own job (#171) is anti-
// STAMPEDE: bound the fan-out when N peers wake on the same beat and all read inflight=0.
// A bound of lanes−1 does that job exactly as well as a bound of 1 — it is still fixed,
// still acquired non-blockingly, still held across the whole turn — while no longer
// throttling below the hardware. On a 1- or 2-lane box the budget floors at 1, so weak
// machines get byte-identical behaviour to before.

/// Try to claim an ambient-turn slot. `Some(permit)` → run the ambient turn (hold the
/// permit for the turn's lifetime; it releases on drop). `None` → all ambient slots
/// are busy; the caller yields this ambient turn. Non-blocking (never waits).
pub fn try_hold_ambient_turn() -> Option<tokio::sync::OwnedSemaphorePermit> {
    LANES.try_hold_ambient_turn()
}

// ── Serving-lane reservation for directed turns (#139) ──────────────────────────
//
// The ambient PERMIT above bounds how many ambient TURNS run at once (1). But ONE
// permitted turn is not one model call: a single `drive_to_settle` makes many calls over
// minutes (act → observe → act), so the permit-holder alone can occupy several physical
// decode lanes (`llama --parallel`), and an addressed (directed) question then queues
// INSIDE the serving process behind it. Glass-boxed 2026-07-15: a directed turn sat
// 8+ minutes behind one 197s idle self-tick + one 213s ambient turn on the two lanes;
// its latency was lane-QUEUE, not decode (a free-lane turn is ~30-60s).
//
// (That glass-box predates the permit reaching the self-tick. Both non-directed paths are
// permit-gated today — `service_loop.rs` acquires at the self-tick gate AND at the
// message-ambient gate, and they share the one ambient pool. The reservation below is
// still required, because the thing it bounds is CALLS-per-turn, which no turn-level
// permit can see. Corrected 2026-08-17: this paragraph had claimed "an idle self-tick is
// not ambient-permit-gated at all", contradicting the self-tick gate's own comment block
// in the same tree — a stale premise sitting under a live design argument.)
//
// Neither the gauge nor the turn-level permit can fix this — the reservation must live
// at the LANE the model call actually consumes. So every model call acquires a lane
// here, priced by priority:
//   - directed → acquires from the FULL pool (MAX_LANES). Waits only if MAX_LANES other
//                directed calls already hold every lane (all lanes serving live work).
//   - non-directed (ambient + idle self-tick) → additionally bounded to (MAX_LANES-1),
//                so at least ONE lane is always reserved for a directed call.
// No deadlock: directed never touches the non-directed cap, and non-directed holds at
// most MAX_LANES-1 physical lanes, so a directed acquire always finds a free lane unless
// every lane already serves directed. On a single-lane machine there is nothing to
// reserve, so the non-directed budget floors at 1 (no starvation, no false guarantee).
// [[conversational-latency-is-a-misdirection-budget]] [[never-thrash-sticky-hysteresis-on-every-lane]]

/// The LIVE `--parallel` slot count of the running serving target, published by the
/// serving daemon on every reconcile (`set_served_lane_count`). 0 until serving is up.
/// This is the ground truth for how many physical decode lanes exist RIGHT NOW —
/// distinct from `serving_plan::MAX_LANES`, which is only the safety ceiling. Before the
/// compute-buffer fit term (#139), lanes were always == MAX_LANES so the constant was
/// exact; now the plan serves DEMAND (e.g. 4 lanes on this box while MAX_LANES = 6), so
/// sizing the admission semaphores by the constant would over-admit by the difference
/// and weaken the directed-turn reservation. Sizing by THIS keeps the reservation exact.
/// Every piece of admission state that used to be six separate process-global statics
/// (`SERVED_LANE_COUNT`, `SERVING_LANES`, `NONDIRECTED_LANES`, their two installed
/// counters, and the resize lock) plus the ambient-turn permits — owned by ONE type.
///
/// Why (2026-08-06): scattered mutable statics made the tests order-dependent, so they took
/// a `TEST_SERIAL` lock whose own comment admitted it was insufficient — "TEST_SERIAL only
/// serializes THIS module, but a cross-module guard could bump it mid-loop". This file had
/// already learned the lesson once and applied it to the in-flight Gauge, stating the
/// principle outright: *the race is gone at the SOURCE, not merely serialized*. That cure
/// reached one of three cases. This applies it to the other two, and to the sibling
/// [`crate::cognition::prefill_throttle`] the same day.
///
/// The semaphores stay LAZY (`OnceLock` per field, not eager construction) and that is
/// load-bearing, not incidental: they must capture the lane count at FIRST USE, once
/// serving is up. Sizing them eagerly at struct construction would bake in the
/// `MAX_LANES` boot ceiling, and a later `set_served_lane_count` to a SMALLER real count
/// only ever GROWS — so an eager version would sit permanently over-admitted. On a weak
/// box that is not a slow tick; it is admitting more concurrent decodes than the machine
/// can hold.
pub struct LaneAdmission {
    /// LIVE `--parallel` count; 0 until serving publishes one.
    served: AtomicUsize,
    serving: std::sync::OnceLock<std::sync::Arc<tokio::sync::Semaphore>>,
    nondirected: std::sync::OnceLock<std::sync::Arc<tokio::sync::Semaphore>>,
    serving_installed: AtomicUsize,
    nondirected_installed: AtomicUsize,
    /// Serializes the rare live semaphore reconcile so two concurrent re-plans can't
    /// double-count a grow.
    resize_lock: Mutex<()>,
    ambient: std::sync::OnceLock<std::sync::Arc<tokio::sync::Semaphore>>,
    ambient_installed: AtomicUsize,
}

/// The process-global admission gate — ONE INSTANCE of [`LaneAdmission`], not a separate
/// implementation of it. Tests build their own with [`LaneAdmission::new`].
static LANES: LaneAdmission = LaneAdmission::new();

/// Physical decode lanes on the shared serving target — the LIVE served count when
/// serving has come up, else the planner's `MAX_LANES` ceiling as the boot-time fallback.
/// Floored at 1. Read live by the saturation gauge; captured at lazy-init by the lane
/// semaphores (serving is always up before the first persona model call, so the init
/// captures the real count, not the ceiling).
/// Publish the running serving target's live lane count (the plan's `--parallel`). The
/// serving daemon calls this on every reconcile. Sizes the admission semaphores exactly:
/// the lazy-init captures it at boot, and a LATER increase (roster grows, more personas
/// demand lanes) GROWS the live semaphores via `add_permits` — instant and safe. A
/// DECREASE is not forced onto live semaphores (evicting held permits would block); it
/// takes effect on the next process start. Over-admitting by a lane until then only means
/// a call queues at llama, never an OOM — the fit math already guards residency.
pub fn set_served_lane_count(lanes: usize) {
    LANES.set_served_lane_count(lanes);
}

/// Grow a live semaphore to `target` total permits (no-op if already ≥). Never shrinks —
/// see [`set_served_lane_count`]. `installed` tracks the total permits ever added so the
/// delta is computed correctly (a Semaphore only exposes AVAILABLE, not total).
fn grow_semaphore_to(sem: &tokio::sync::Semaphore, installed: &AtomicUsize, target: usize) {
    let cur = installed.load(Ordering::Acquire);
    if target > cur {
        sem.add_permits(target - cur);
        installed.store(target, Ordering::Release);
    }
}

/// Total permits installed into each lane semaphore — the grow-delta bookkeeping for
/// [`grow_semaphore_to`], set at lazy-init and bumped on each live grow.

/// The lane count a sibling gate should boot with before any plan publishes — the same
/// live-count-else-ceiling read the lane semaphores lazy-init from. Used by the prefill
/// throttle (#56) so both gates start from the ONE number.
pub fn boot_lane_count() -> usize {
    LANES.lane_count()
}

/// Lanes a non-directed (ambient / idle) model call may occupy: all lanes minus one
/// reserved for directed work, floored at 1 so a single-lane machine — where there is
/// nothing to reserve — still lets idle work run rather than starving it forever.
/// RAII permit for one serving-lane model call. Holds a physical-lane permit, the
/// (optional) non-directed sub-cap permit, and the in-flight gauge marker — ALL released
/// on drop. Acquire it around the single generate call so the reservation window matches
/// exactly the lane-consuming window (queue + prefill + decode) and nothing downstream.
#[derive(Debug)]
pub struct ServingLanePermit {
    _lane: tokio::sync::OwnedSemaphorePermit,
    _nondirected: Option<tokio::sync::OwnedSemaphorePermit>,
    _inflight: InflightModelCall,
}

impl LaneAdmission {
    /// A fresh, INDEPENDENT admission gate. `const` so the process-global can be a `static`;
    /// tests call it to get their own and therefore run order-independently and in parallel.
    pub const fn new() -> Self {
        Self {
            served: AtomicUsize::new(0),
            serving: std::sync::OnceLock::new(),
            nondirected: std::sync::OnceLock::new(),
            serving_installed: AtomicUsize::new(0),
            nondirected_installed: AtomicUsize::new(0),
            resize_lock: Mutex::new(()),
            ambient: std::sync::OnceLock::new(),
            ambient_installed: AtomicUsize::new(0),
        }
    }

    /// Physical decode lanes: the LIVE served count once serving is up, else the planner's
    /// ceiling as the boot fallback. Floored at 1.
    fn lane_count(&self) -> usize {
        match self.served.load(Ordering::Acquire) {
            0 => (crate::cognition::serving_plan::MAX_LANES as usize).max(1),
            n => n,
        }
    }

    /// Lanes a non-directed (ambient / idle) call may occupy: all but one, floored at 1 so a
    /// single-lane machine still lets idle work run rather than starving it forever.
    fn nondirected_budget(&self) -> usize {
        self.lane_count().saturating_sub(1).max(1)
    }

    /// See [`set_served_lane_count`].
    pub fn set_served_lane_count(&self, lanes: usize) {
        let lanes = lanes.max(1);
        self.served.store(lanes, Ordering::Release);
        let _guard = self
            .resize_lock
            .lock()
            .expect("lane-resize lock never poisoned");
        if let Some(sem) = self.serving.get() {
            grow_semaphore_to(sem, &self.serving_installed, lanes);
        }
        if let Some(sem) = self.nondirected.get() {
            grow_semaphore_to(
                sem,
                &self.nondirected_installed,
                lanes.saturating_sub(1).max(1),
            );
        }
        // The ambient-turn pool rides the SAME budget, so it grows on the same signal.
        // Missing this is how a pool installed at the boot floor would stay there for the
        // process's life while serving grew underneath it.
        if let Some(sem) = self.ambient.get() {
            grow_semaphore_to(sem, &self.ambient_installed, lanes.saturating_sub(1).max(1));
        }
    }

    fn serving_lanes(&self) -> &std::sync::Arc<tokio::sync::Semaphore> {
        self.serving.get_or_init(|| {
            let n = self.lane_count();
            self.serving_installed.store(n, Ordering::Release);
            std::sync::Arc::new(tokio::sync::Semaphore::new(n))
        })
    }

    fn nondirected_lanes(&self) -> &std::sync::Arc<tokio::sync::Semaphore> {
        self.nondirected.get_or_init(|| {
            let n = self.nondirected_budget();
            self.nondirected_installed.store(n, Ordering::Release);
            std::sync::Arc::new(tokio::sync::Semaphore::new(n))
        })
    }

    /// See [`try_hold_ambient_turn`]. Lazy like its siblings, and for the same load-bearing
    /// reason: it must capture the budget at FIRST USE, once serving has published a real
    /// lane count — never at construction, when only the boot ceiling is known.
    pub fn try_hold_ambient_turn(&self) -> Option<tokio::sync::OwnedSemaphorePermit> {
        self.ambient
            .get_or_init(|| {
                let n = self.nondirected_budget();
                self.ambient_installed.store(n, Ordering::Release);
                std::sync::Arc::new(tokio::sync::Semaphore::new(n))
            })
            .clone()
            .try_acquire_owned()
            .ok()
    }

    /// See [`acquire_serving_lane`] — the reservation policy lives HERE so the global and a
    /// test instance can never drift into two different policies.
    pub async fn acquire_serving_lane(&self, directed: bool) -> ServingLanePermit {
        // Non-directed reserves within the (lanes-1) budget FIRST, so the physical-lane
        // acquire below can never let non-directed work starve a directed caller.
        let nondirected = if directed {
            None
        } else {
            let sem = self.nondirected_lanes().clone();
            let permit = match sem.clone().try_acquire_owned() {
                Ok(p) => p,
                Err(_) => {
                    tracing::info!(
                        probe_class = "serving.lane.nondirected_waiting",
                        "non-directed model call waiting — a lane is reserved for directed turns (#139)"
                    );
                    sem.acquire_owned()
                        .await
                        .expect("non-directed-lane semaphore is never closed")
                }
            };
            Some(permit)
        };
        let lane = self
            .serving_lanes()
            .clone()
            .acquire_owned()
            .await
            .expect("serving-lane semaphore is never closed");
        ServingLanePermit {
            _lane: lane,
            _nondirected: nondirected,
            _inflight: InflightModelCall::enter(),
        }
    }
}

impl Default for LaneAdmission {
    fn default() -> Self {
        Self::new()
    }
}

/// Acquire a serving lane for a model call, priced by priority (#139). `directed`
/// callers take from the full lane pool; non-directed callers first claim the
/// (MAX_LANES-1) non-directed budget, guaranteeing a directed caller always finds a free
/// physical lane. Awaits only under genuine contention; the returned permit releases
/// every lane on drop.
pub async fn acquire_serving_lane(directed: bool) -> ServingLanePermit {
    LANES.acquire_serving_lane(directed).await
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/ResourceAdmissionPolicy.ts"
)]
pub struct ResourceAdmissionPolicy {
    pub resource_class: ResourceClass,
    pub target_silicon: TargetSilicon,
    pub max_concurrency: usize,
    pub max_cost_units: u32,
    pub cost_units: u32,
    #[ts(type = "number")]
    pub lease_ttl_ms: u64,
    pub revocation_policy: ThroughputLeaseRevocationPolicy,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResourceAdmissionRequest {
    pub lease_id: String,
    pub artifact_key: String,
    pub holder_id: String,
    pub policy: ResourceAdmissionPolicy,
    pub now_ms: u64,
}

#[derive(Debug, Clone, Eq, PartialEq, thiserror::Error)]
pub enum ResourceAdmissionError {
    #[error("invalid resource admission policy: {reason}")]
    InvalidPolicy { reason: String },
    #[error("resource admission denied: {reason}")]
    Denied { reason: String },
    #[error("resource lease error: {reason}")]
    Lease { reason: String },
}

#[derive(Debug, Default)]
pub struct ResourceAdmissionGate {
    registry: Mutex<ThroughputLeaseRegistry>,
}

impl ResourceAdmissionGate {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn acquire(
        &'static self,
        request: ResourceAdmissionRequest,
    ) -> Result<ResourceAdmissionGuard, ResourceAdmissionError> {
        validate_policy(&request.policy)?;

        let lease = ThroughputLease {
            lease_id: request.lease_id.clone(),
            artifact_key: request.artifact_key,
            resource_class: request.policy.resource_class,
            target_silicon: request.policy.target_silicon,
            holder_id: request.holder_id,
            cost_units: request.policy.cost_units,
            acquired_at_ms: request.now_ms,
            expires_at_ms: request.now_ms.saturating_add(request.policy.lease_ttl_ms),
            revocation_policy: request.policy.revocation_policy,
        };

        let mut registry = self.lock_registry();
        registry.expire(request.now_ms);
        let snapshot = registry.snapshot(request.now_ms);
        let active_count = snapshot
            .active
            .iter()
            .filter(|lease| lease.target_silicon == request.policy.target_silicon)
            .count();
        let active_cost = snapshot
            .cost_by_target_silicon
            .get(&request.policy.target_silicon)
            .copied()
            .unwrap_or(0);

        if active_count >= request.policy.max_concurrency {
            return Err(ResourceAdmissionError::Denied {
                reason: format!(
                    "resource_class={:?} target_silicon={:?} active_count={} max_concurrency={}",
                    request.policy.resource_class,
                    request.policy.target_silicon,
                    active_count,
                    request.policy.max_concurrency
                ),
            });
        }
        if active_cost.saturating_add(request.policy.cost_units) > request.policy.max_cost_units {
            return Err(ResourceAdmissionError::Denied {
                reason: format!(
                    "resource_class={:?} target_silicon={:?} active_cost={} requested_cost={} max_cost_units={}",
                    request.policy.resource_class,
                    request.policy.target_silicon,
                    active_cost,
                    request.policy.cost_units,
                    request.policy.max_cost_units
                ),
            });
        }

        registry
            .acquire(lease, request.now_ms)
            .map_err(|err| ResourceAdmissionError::Lease {
                reason: format_lease_error(err),
            })?;

        Ok(ResourceAdmissionGuard {
            gate: self,
            lease_id: Some(request.lease_id),
        })
    }

    fn release(&self, lease_id: &str) -> Result<ThroughputLease, ThroughputLeaseError> {
        self.lock_registry().release(lease_id)
    }

    fn lock_registry(&self) -> MutexGuard<'_, ThroughputLeaseRegistry> {
        self.registry
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[cfg(test)]
    pub fn reset_for_test(&self) {
        *self.lock_registry() = ThroughputLeaseRegistry::new();
    }

    #[cfg(test)]
    pub fn active_count_for_test(&self, now_ms: u64) -> usize {
        self.lock_registry().snapshot(now_ms).active.len()
    }
}

#[derive(Debug)]
pub struct ResourceAdmissionGuard {
    gate: &'static ResourceAdmissionGate,
    lease_id: Option<String>,
}

impl ResourceAdmissionGuard {
    #[cfg(test)]
    pub fn release(mut self) -> Result<ThroughputLease, ThroughputLeaseError> {
        let lease_id = self
            .lease_id
            .take()
            .expect("resource admission guard must contain a lease id before release");
        self.gate.release(&lease_id)
    }
}

impl Drop for ResourceAdmissionGuard {
    fn drop(&mut self) {
        let Some(lease_id) = self.lease_id.take() else {
            return;
        };
        let _ = self.gate.release(&lease_id);
    }
}

fn validate_policy(policy: &ResourceAdmissionPolicy) -> Result<(), ResourceAdmissionError> {
    if policy.max_concurrency == 0 {
        return Err(invalid_policy("max_concurrency must be greater than zero"));
    }
    if policy.cost_units == 0 {
        return Err(invalid_policy("cost_units must be greater than zero"));
    }
    if policy.max_cost_units == 0 {
        return Err(invalid_policy("max_cost_units must be greater than zero"));
    }
    if policy.cost_units > policy.max_cost_units {
        return Err(invalid_policy(format!(
            "cost_units={} exceeds max_cost_units={}",
            policy.cost_units, policy.max_cost_units
        )));
    }
    if policy.lease_ttl_ms == 0 {
        return Err(invalid_policy("lease_ttl_ms must be greater than zero"));
    }
    Ok(())
}

fn invalid_policy(reason: impl Into<String>) -> ResourceAdmissionError {
    ResourceAdmissionError::InvalidPolicy {
        reason: reason.into(),
    }
}

fn format_lease_error(err: ThroughputLeaseError) -> String {
    match err {
        ThroughputLeaseError::DuplicateLease { lease_id } => {
            format!("duplicate lease_id={lease_id}")
        }
        ThroughputLeaseError::MissingLease { lease_id } => {
            format!("missing lease_id={lease_id}")
        }
        ThroughputLeaseError::ExpiredLease { lease_id } => {
            format!("expired lease_id={lease_id}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // No serialization lock lives here any more, and that is the point. It used to exist
    // because the ambient-permit and serving-lane tests mutated PROCESS-GLOBAL admission
    // statics — and its own comment admitted the lock was not enough: "TEST_SERIAL only
    // serializes THIS module, but a cross-module guard could bump it mid-loop" (#1960/#191).
    // The in-flight Gauge test was rescued first by driving a LOCAL instance; these two now
    // do the same with `LaneAdmission::new()`. The race is gone at the SOURCE, not merely
    // serialized, and the tests run in parallel.

    // what this catches (#139 idle admission): the in-flight gauge counts each
    // model-call entry, releases on drop, and reads SATURATED exactly at the serving
    // concurrency (serving_plan::MAX_LANES) — the signal a self-tick yields on so an
    // idle deliberation never deepens the queue live conversation waits behind.
    //
    // Drives a LOCAL Gauge instance — the SAME type production's process-global
    // INFLIGHT_MODEL_CALLS is, but one nothing else in the process can reach — so the
    // absolute-count assertions are deterministic under cargo's parallel execution. The
    // prior version read the process-global counter and a sibling test's InflightModelCall
    // guard could bump it mid-loop (the #1960 flaky class, canary red 2026-07-25); scoping
    // the gauge kills that race at the root, so no TEST_SERIAL / drain-wait is needed here.
    #[test]
    fn inflight_gauge_counts_releases_and_saturates_at_serving_concurrency() {
        let max = crate::cognition::serving_plan::MAX_LANES as usize;
        let gauge = Gauge::new();

        assert_eq!(gauge.inflight(), 0, "a fresh gauge starts empty");
        assert!(!gauge.saturated(max), "an empty gauge is not saturated");

        let mut guards = Vec::new();
        for expected in 1..=max {
            guards.push(gauge.enter());
            assert_eq!(gauge.inflight(), expected);
        }
        // Every decode slot busy → one more call would queue behind the fleet.
        assert!(
            gauge.saturated(max),
            "MAX_LANES outstanding must read saturated"
        );

        guards.pop(); // free a slot
        assert_eq!(gauge.inflight(), max - 1);
        assert!(
            !gauge.saturated(max),
            "freeing a slot clears saturation — an idle self-tick may run again"
        );

        drop(guards);
        assert_eq!(gauge.inflight(), 0, "all guards released → baseline");
    }

    // what this catches (#171 fan-out): the ambient-turn permit bounds how many
    // NON-directed turns deliberate at once, no matter how the room burst wakes them.
    // The first cut gated on the in-flight gauge and did NOT fire live: a simultaneous
    // burst wakes every peer, they all read inflight=0 (none had generated yet) and
    // stampede past the check together. A permit has no such window — the slot is
    // claimed at the decision point and held for the turn, so the (N-1) peers that woke
    // in the same instant find it taken and yield. This is the only test that touches
    // the process-global ambient semaphore, so it starts with all slots free.
    #[test]
    fn ambient_permit_bounds_concurrency_and_releases_on_drop() {
        // OUR OWN gate — no process-global, so no lock and no order dependence.
        let gate = LaneAdmission::new();
        // PREMISE CHANGE 2026-08-17: the pool used to be a bare `AMBIENT_TURN_CONCURRENCY
        // = 1`; it is now the live `nondirected_budget()` (lanes − 1, floored at 1), the
        // same budget the per-call lane reservation uses. Pin a real multi-lane machine so
        // the burst has something to bound — at the old hardcoded 1 this test could not
        // distinguish "correctly bounded" from "throttled below the hardware", which is
        // exactly how the starvation ceiling stayed invisible.
        gate.set_served_lane_count(4);
        let budget = gate.nondirected_budget();
        assert_eq!(budget, 3, "4 served lanes → 3 non-directed, 1 reserved directed");
        // A simultaneous-wake burst: several ambient turns try to claim a slot at once.
        // Exactly `budget` win; the rest get None and must yield.
        let mut held: Vec<tokio::sync::OwnedSemaphorePermit> = Vec::new();
        for _ in 0..budget {
            held.push(
                gate.try_hold_ambient_turn()
                    .expect("a free slot is grantable"),
            );
        }
        // The next simultaneous ambient waker finds every slot taken → yields.
        assert!(
            gate.try_hold_ambient_turn().is_none(),
            "over-capacity ambient turn must yield (the stampede the gauge let through)"
        );

        // The addressed persona never calls this — directed work is unthrottled. Model
        // that by simply NOT touching the permit here; the held slots stay full and the
        // assertion above already proved a concurrent ambient turn can't sneak a slot.

        // A held ambient turn finishes → its permit drops → capacity frees for the next
        // beat, so a yielded room re-perceives and contributes when there's headroom.
        held.pop();
        let reclaimed = gate
            .try_hold_ambient_turn()
            .expect("dropping a finished turn frees its slot for the next");
        drop(reclaimed);
        drop(held); // release the rest (nothing else can observe this gate anyway)
    }

    // what this catches: the WEAK-BOX floor of the same change. Deriving the ambient pool
    // from lanes−1 must not regress a 1- or 2-lane machine below the behaviour it had
    // under the old hardcoded 1 — a single-lane host has nothing to reserve, so its
    // budget floors at 1 and it stays byte-identical. This is the half of the change that
    // could silently hurt the smallest supported hardware, so it gets its own row.
    #[test]
    fn a_one_lane_box_keeps_exactly_one_ambient_slot() {
        for lanes in [1usize, 2] {
            let gate = LaneAdmission::new();
            gate.set_served_lane_count(lanes);
            assert_eq!(
                gate.nondirected_budget(),
                1,
                "{lanes}-lane box floors the non-directed budget at 1"
            );
            let held = gate
                .try_hold_ambient_turn()
                .expect("the one slot is grantable");
            assert!(
                gate.try_hold_ambient_turn().is_none(),
                "a {lanes}-lane box admits exactly ONE ambient turn — unchanged from the \
                 pre-2026-08-17 hardcoded bound"
            );
            drop(held);
        }
    }

    // what this catches: an ambient pool installed at the BOOT floor and then never grown.
    // The pool is lazy on purpose (capture the real lane count at first use), but that
    // makes "first use happened before serving published its count" a live possibility —
    // and without the grow-on-resize wiring the whole roster would stay pinned at the boot
    // budget for the life of the process while 3 lanes sat idle. Regression for the
    // starvation ceiling this change removes.
    #[test]
    fn the_ambient_pool_grows_when_serving_publishes_more_lanes() {
        let gate = LaneAdmission::new();
        gate.set_served_lane_count(1); // cold boot: one lane
        let first = gate.try_hold_ambient_turn().expect("the one slot");
        assert!(gate.try_hold_ambient_turn().is_none(), "1 lane → 1 ambient slot");

        gate.set_served_lane_count(4); // serving warms up and reports its real width
        let second = gate
            .try_hold_ambient_turn()
            .expect("growing to 4 lanes must open a second ambient slot");
        let third = gate
            .try_hold_ambient_turn()
            .expect("…and a third (budget = lanes - 1)");
        assert!(
            gate.try_hold_ambient_turn().is_none(),
            "still bounded at lanes-1 — growth must not remove the directed reservation"
        );
        drop((first, second, third));
    }

    // what this catches (#139 lane starvation): a directed (addressed) turn must never
    // queue behind non-directed model calls. Non-directed callers are capped at
    // (MAX_LANES-1) lanes, so a directed caller always finds a reserved lane — this is
    // the fix for the glass-boxed 8-minute directed-turn wait behind an idle self-tick
    // + one long ambient turn on the two decode lanes. This is the only test that
    // touches the process-global serving-lane semaphores, so it starts all-free.
    #[tokio::test]
    async fn directed_turn_always_finds_a_reserved_lane() {
        let gate = LaneAdmission::new();
        use std::time::Duration;
        let budget = gate.nondirected_budget();

        // Fill the ENTIRE non-directed budget (all lanes idle/ambient work may hold).
        let mut nondirected = Vec::new();
        for _ in 0..budget {
            nondirected.push(gate.acquire_serving_lane(false).await);
        }

        // On a machine with a lane to reserve (MAX_LANES >= 2), a directed call still
        // acquires immediately — it is not blocked by the saturated non-directed budget.
        if gate.lane_count() > 1 {
            let directed =
                tokio::time::timeout(Duration::from_millis(250), gate.acquire_serving_lane(true))
                    .await;
            assert!(
                directed.is_ok(),
                "a directed turn must get a reserved lane, never queue behind non-directed work"
            );

            // And a FURTHER non-directed call must now WAIT (its budget is full) — it
            // times out rather than stealing the lane the directed turn is using.
            let extra_nondirected =
                tokio::time::timeout(Duration::from_millis(150), gate.acquire_serving_lane(false))
                    .await;
            assert!(
                extra_nondirected.is_err(),
                "non-directed work over its (MAX_LANES-1) budget must wait, not preempt"
            );
            drop(directed);
        }

        drop(nondirected); // release (nothing else can observe this gate anyway)
    }

    // what this catches: the grow-delta bookkeeping behind live lane resizing (#139
    // follow-up). A Semaphore only exposes AVAILABLE permits, so growing to a target
    // must track the TOTAL ever installed and add only the difference — and a repeat
    // to the same/smaller target must be a no-op, never a double-add. Uses a LOCAL
    // semaphore so it never touches the process-global lane statics other tests share.
    #[test]
    fn grow_semaphore_to_adds_only_the_delta_and_never_shrinks() {
        let sem = tokio::sync::Semaphore::new(2);
        let installed = AtomicUsize::new(2);

        // Grow 2 → 4: adds exactly 2.
        grow_semaphore_to(&sem, &installed, 4);
        assert_eq!(sem.available_permits(), 4);
        assert_eq!(installed.load(Ordering::Acquire), 4);

        // Re-assert the same target: no-op (the double-count bug this guards).
        grow_semaphore_to(&sem, &installed, 4);
        assert_eq!(sem.available_permits(), 4, "same target must not add again");

        // A SMALLER target never shrinks a live semaphore (evicting held permits would
        // block; over-admit is safe — the fit math guards residency).
        grow_semaphore_to(&sem, &installed, 3);
        assert_eq!(sem.available_permits(), 4, "never shrinks live");
        assert_eq!(installed.load(Ordering::Acquire), 4);
    }

    // what this catches: serving_lane_count reads the LIVE published count when serving is
    // up, and only falls back to the MAX_LANES ceiling when nothing has published yet —
    // the exact fix so the admission reservation sizes by what's SERVED (e.g. 4), not the
    // ceiling (6).
    //
    // Drives its OWN gate. The prior version swapped the process-global atomic and restored
    // it at the end — brittle in two ways at once: a sibling test reading the count in the
    // window between swap and restore sees a value this test invented, and a panic anywhere
    // in between leaves the global corrupted for every test that follows. Owning the state
    // removes both, with neither a lock nor a restore.
    #[test]
    fn serving_lane_count_prefers_the_live_published_count_over_the_ceiling() {
        let gate = LaneAdmission::new();
        assert_eq!(
            gate.lane_count(),
            (crate::cognition::serving_plan::MAX_LANES as usize).max(1),
            "unset → MAX_LANES ceiling fallback"
        );
        gate.set_served_lane_count(4);
        assert_eq!(
            gate.lane_count(),
            4,
            "published live count wins over the ceiling"
        );
    }
}
