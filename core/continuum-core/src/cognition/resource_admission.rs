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
use std::sync::{Mutex, MutexGuard};
use ts_rs::TS;

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
