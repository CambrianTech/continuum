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
static INFLIGHT_MODEL_CALLS: AtomicUsize = AtomicUsize::new(0);

/// RAII marker: increments the shared in-flight gauge on entry and decrements on
/// EVERY exit path (Ok, Err, panic-unwind). The deliberation faculty wraps its single
/// generate call in this so the gauge reflects exactly the model-call window
/// (lane-queue + prefill + decode) and nothing downstream.
#[derive(Debug)]
pub struct InflightModelCall(());

impl InflightModelCall {
    pub fn enter() -> Self {
        INFLIGHT_MODEL_CALLS.fetch_add(1, Ordering::AcqRel);
        Self(())
    }
}

impl Drop for InflightModelCall {
    fn drop(&mut self) {
        INFLIGHT_MODEL_CALLS.fetch_sub(1, Ordering::AcqRel);
    }
}

/// Deliberative model calls outstanding against the shared serving target right now.
pub fn inflight_model_calls() -> usize {
    INFLIGHT_MODEL_CALLS.load(Ordering::Acquire)
}

/// True when every shared decode slot is busy, so one more call would QUEUE behind
/// the fleet. Threshold is the serving concurrency (`serving_plan::MAX_LANES`) — the
/// same constant the planner sizes lanes by, NOT a new magic number. A self-tick
/// yields on this; message/directed turns are never gated on it (a human/peer waits).
pub fn shared_model_saturated() -> bool {
    inflight_model_calls() >= crate::cognition::serving_plan::MAX_LANES as usize
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

/// How many ambient turns may run at once. 1 = strongly prioritize directed work:
/// under a burst, the addressed persona plus at most one ambient contribution run;
/// the rest yield. Under light/staggered load ambient turns are naturally serial, so
/// this never throttles a quiet room.
const AMBIENT_TURN_CONCURRENCY: usize = 1;

static AMBIENT_TURN_PERMITS: std::sync::OnceLock<std::sync::Arc<tokio::sync::Semaphore>> =
    std::sync::OnceLock::new();

fn ambient_permits() -> &'static std::sync::Arc<tokio::sync::Semaphore> {
    AMBIENT_TURN_PERMITS
        .get_or_init(|| std::sync::Arc::new(tokio::sync::Semaphore::new(AMBIENT_TURN_CONCURRENCY)))
}

/// Try to claim an ambient-turn slot. `Some(permit)` → run the ambient turn (hold the
/// permit for the turn's lifetime; it releases on drop). `None` → all ambient slots
/// are busy; the caller yields this ambient turn. Non-blocking (never waits).
pub fn try_hold_ambient_turn() -> Option<tokio::sync::OwnedSemaphorePermit> {
    ambient_permits().clone().try_acquire_owned().ok()
}

// ── Serving-lane reservation for directed turns (#139) ──────────────────────────
//
// The ambient PERMIT above bounds how many ambient TURNS run at once (1). But a single
// ambient `drive_to_settle` makes many model calls over minutes, and an idle self-tick
// is not ambient-permit-gated at all — so together they can occupy BOTH physical decode
// lanes (`llama --parallel MAX_LANES`), and an addressed (directed) question then queues
// INSIDE the serving process behind them. Glass-boxed 2026-07-15: a directed turn sat
// 8+ minutes behind one 197s idle self-tick + one 213s ambient turn on the two lanes;
// its latency was lane-QUEUE, not decode (a free-lane turn is ~30-60s).
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

/// Physical decode lanes on the shared serving target — the same ceiling the planner
/// sizes by and the saturation gauge reads. Floored at 1.
fn serving_lane_count() -> usize {
    (crate::cognition::serving_plan::MAX_LANES as usize).max(1)
}

/// Lanes a non-directed (ambient / idle) model call may occupy: all lanes minus one
/// reserved for directed work, floored at 1 so a single-lane machine — where there is
/// nothing to reserve — still lets idle work run rather than starving it forever.
fn nondirected_lane_budget() -> usize {
    serving_lane_count().saturating_sub(1).max(1)
}

static SERVING_LANES: std::sync::OnceLock<std::sync::Arc<tokio::sync::Semaphore>> =
    std::sync::OnceLock::new();
static NONDIRECTED_LANES: std::sync::OnceLock<std::sync::Arc<tokio::sync::Semaphore>> =
    std::sync::OnceLock::new();

fn serving_lanes() -> &'static std::sync::Arc<tokio::sync::Semaphore> {
    SERVING_LANES
        .get_or_init(|| std::sync::Arc::new(tokio::sync::Semaphore::new(serving_lane_count())))
}

fn nondirected_lanes() -> &'static std::sync::Arc<tokio::sync::Semaphore> {
    NONDIRECTED_LANES
        .get_or_init(|| std::sync::Arc::new(tokio::sync::Semaphore::new(nondirected_lane_budget())))
}

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

/// Acquire a serving lane for a model call, priced by priority (#139). `directed`
/// callers take from the full lane pool; non-directed callers first claim the
/// (MAX_LANES-1) non-directed budget, guaranteeing a directed caller always finds a free
/// physical lane. Awaits only under genuine contention; the returned permit releases
/// every lane on drop.
pub async fn acquire_serving_lane(directed: bool) -> ServingLanePermit {
    // Non-directed reserves within the (MAX_LANES-1) budget FIRST, so the physical-lane
    // acquire below can never let non-directed work starve a directed caller.
    let nondirected = if directed {
        None
    } else {
        let sem = nondirected_lanes().clone();
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
    let lane = serving_lanes()
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

    // what this catches (#139 idle admission): the in-flight gauge counts each
    // model-call entry, releases on drop, and reads SATURATED exactly at the serving
    // concurrency (serving_plan::MAX_LANES) — the signal a self-tick yields on so an
    // idle deliberation never deepens the queue live conversation waits behind. This
    // is the only test that touches the process-global gauge, so it starts at zero.
    #[test]
    fn inflight_gauge_counts_releases_and_saturates_at_serving_concurrency() {
        let max = crate::cognition::serving_plan::MAX_LANES as usize;
        assert_eq!(inflight_model_calls(), 0, "gauge starts clean");
        assert!(!shared_model_saturated(), "idle model is not saturated");

        let mut guards: Vec<InflightModelCall> = Vec::new();
        for expected in 1..=max {
            guards.push(InflightModelCall::enter());
            assert_eq!(inflight_model_calls(), expected);
        }
        // Every decode slot busy → one more call would queue behind the fleet.
        assert!(
            shared_model_saturated(),
            "MAX_LANES outstanding must read saturated"
        );

        guards.pop(); // free a slot
        assert_eq!(inflight_model_calls(), max - 1);
        assert!(
            !shared_model_saturated(),
            "freeing a slot clears saturation — an idle self-tick may run again"
        );

        drop(guards);
        assert_eq!(inflight_model_calls(), 0, "all guards released → baseline");
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
        // A simultaneous-wake burst: several ambient turns try to claim a slot at once.
        // Exactly AMBIENT_TURN_CONCURRENCY win; the rest get None and must yield.
        let mut held: Vec<tokio::sync::OwnedSemaphorePermit> = Vec::new();
        for _ in 0..AMBIENT_TURN_CONCURRENCY {
            held.push(try_hold_ambient_turn().expect("a free slot is grantable"));
        }
        // The next simultaneous ambient waker finds every slot taken → yields.
        assert!(
            try_hold_ambient_turn().is_none(),
            "over-capacity ambient turn must yield (the stampede the gauge let through)"
        );

        // The addressed persona never calls this — directed work is unthrottled. Model
        // that by simply NOT touching the permit here; the held slots stay full and the
        // assertion above already proved a concurrent ambient turn can't sneak a slot.

        // A held ambient turn finishes → its permit drops → capacity frees for the next
        // beat, so a yielded room re-perceives and contributes when there's headroom.
        held.pop();
        let reclaimed =
            try_hold_ambient_turn().expect("dropping a finished turn frees its slot for the next");
        drop(reclaimed);
        drop(held); // release the rest → back to all-free for any later test
    }

    // what this catches (#139 lane starvation): a directed (addressed) turn must never
    // queue behind non-directed model calls. Non-directed callers are capped at
    // (MAX_LANES-1) lanes, so a directed caller always finds a reserved lane — this is
    // the fix for the glass-boxed 8-minute directed-turn wait behind an idle self-tick
    // + one long ambient turn on the two decode lanes. This is the only test that
    // touches the process-global serving-lane semaphores, so it starts all-free.
    #[tokio::test]
    async fn directed_turn_always_finds_a_reserved_lane() {
        use std::time::Duration;
        let budget = nondirected_lane_budget();

        // Fill the ENTIRE non-directed budget (all lanes idle/ambient work may hold).
        let mut nondirected = Vec::new();
        for _ in 0..budget {
            nondirected.push(acquire_serving_lane(false).await);
        }

        // On a machine with a lane to reserve (MAX_LANES >= 2), a directed call still
        // acquires immediately — it is not blocked by the saturated non-directed budget.
        if serving_lane_count() > 1 {
            let directed = tokio::time::timeout(
                Duration::from_millis(250),
                acquire_serving_lane(true),
            )
            .await;
            assert!(
                directed.is_ok(),
                "a directed turn must get a reserved lane, never queue behind non-directed work"
            );

            // And a FURTHER non-directed call must now WAIT (its budget is full) — it
            // times out rather than stealing the lane the directed turn is using.
            let extra_nondirected = tokio::time::timeout(
                Duration::from_millis(150),
                acquire_serving_lane(false),
            )
            .await;
            assert!(
                extra_nondirected.is_err(),
                "non-directed work over its (MAX_LANES-1) budget must wait, not preempt"
            );
            drop(directed);
        }

        drop(nondirected); // release → all lanes free for any later test
    }
}
