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
}
