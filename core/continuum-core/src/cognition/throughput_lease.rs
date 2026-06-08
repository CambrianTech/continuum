//! Throughput leases.
//!
//! A lease is the ownership primitive that sits between the pure
//! adaptive-throughput planner and real resource managers such as
//! FootprintRegistry, PagedResourcePool, and PressureBroker. The planner
//! decides which jobs may run; leases record who owns the admitted resource
//! budget, for how long, and whether pressure is allowed to revoke it.
//!
//! This module is intentionally pure and in-memory. The next integration
//! layer can mirror acquire/release into FootprintRegistry and teach
//! PressureBroker to prefer expired or revocable leases before touching
//! pinned work.

use super::{ResourceClass, TargetSilicon};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/ThroughputLeaseRevocationPolicy.ts"
)]
pub enum ThroughputLeaseRevocationPolicy {
    /// Pressure may revoke this lease after notifying the holder.
    Graceful,
    /// Pressure may revoke immediately. Suitable for stale frames.
    Hard,
    /// Do not revoke while active. Page-out/eviction must defer.
    Pinned,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/ThroughputLease.ts"
)]
pub struct ThroughputLease {
    pub lease_id: String,
    pub artifact_key: String,
    pub resource_class: ResourceClass,
    pub target_silicon: TargetSilicon,
    pub holder_id: String,
    pub cost_units: u32,
    #[ts(type = "number")]
    pub acquired_at_ms: u64,
    #[ts(type = "number")]
    pub expires_at_ms: u64,
    pub revocation_policy: ThroughputLeaseRevocationPolicy,
}

impl ThroughputLease {
    pub fn is_expired(&self, now_ms: u64) -> bool {
        now_ms >= self.expires_at_ms
    }

    pub fn is_reclaimable(&self, now_ms: u64) -> bool {
        self.is_expired(now_ms) || self.revocation_policy != ThroughputLeaseRevocationPolicy::Pinned
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/ThroughputLeaseSnapshot.ts"
)]
pub struct ThroughputLeaseSnapshot {
    pub active: Vec<ThroughputLease>,
    pub expired: Vec<ThroughputLease>,
    pub cost_by_target_silicon: BTreeMap<TargetSilicon, u32>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ThroughputLeaseError {
    DuplicateLease { lease_id: String },
    MissingLease { lease_id: String },
    ExpiredLease { lease_id: String },
}

#[derive(Debug, Default)]
pub struct ThroughputLeaseRegistry {
    leases: BTreeMap<String, ThroughputLease>,
}

impl ThroughputLeaseRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn acquire(
        &mut self,
        lease: ThroughputLease,
        now_ms: u64,
    ) -> Result<(), ThroughputLeaseError> {
        if lease.is_expired(now_ms) {
            return Err(ThroughputLeaseError::ExpiredLease {
                lease_id: lease.lease_id,
            });
        }
        if self.leases.contains_key(&lease.lease_id) {
            return Err(ThroughputLeaseError::DuplicateLease {
                lease_id: lease.lease_id,
            });
        }
        self.leases.insert(lease.lease_id.clone(), lease);
        Ok(())
    }

    pub fn renew(
        &mut self,
        lease_id: &str,
        expires_at_ms: u64,
        now_ms: u64,
    ) -> Result<(), ThroughputLeaseError> {
        let Some(lease) = self.leases.get_mut(lease_id) else {
            return Err(ThroughputLeaseError::MissingLease {
                lease_id: lease_id.to_string(),
            });
        };
        if lease.is_expired(now_ms) {
            return Err(ThroughputLeaseError::ExpiredLease {
                lease_id: lease_id.to_string(),
            });
        }
        lease.expires_at_ms = expires_at_ms;
        Ok(())
    }

    pub fn release(&mut self, lease_id: &str) -> Result<ThroughputLease, ThroughputLeaseError> {
        self.leases
            .remove(lease_id)
            .ok_or_else(|| ThroughputLeaseError::MissingLease {
                lease_id: lease_id.to_string(),
            })
    }

    pub fn expire(&mut self, now_ms: u64) -> Vec<ThroughputLease> {
        let expired_ids: Vec<String> = self
            .leases
            .iter()
            .filter(|(_, lease)| lease.is_expired(now_ms))
            .map(|(lease_id, _)| lease_id.clone())
            .collect();

        expired_ids
            .into_iter()
            .filter_map(|lease_id| self.leases.remove(&lease_id))
            .collect()
    }

    pub fn snapshot(&self, now_ms: u64) -> ThroughputLeaseSnapshot {
        let mut active = Vec::new();
        let mut expired = Vec::new();
        let mut cost_by_target_silicon = BTreeMap::new();

        for lease in self.leases.values() {
            if lease.is_expired(now_ms) {
                expired.push(lease.clone());
            } else {
                *cost_by_target_silicon
                    .entry(lease.target_silicon)
                    .or_insert(0u32) += lease.cost_units;
                active.push(lease.clone());
            }
        }

        ThroughputLeaseSnapshot {
            active,
            expired,
            cost_by_target_silicon,
        }
    }

    pub fn reclaimable(&self, now_ms: u64) -> Vec<ThroughputLease> {
        self.leases
            .values()
            .filter(|lease| lease.is_reclaimable(now_ms))
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lease(
        lease_id: &str,
        target_silicon: TargetSilicon,
        cost_units: u32,
        expires_at_ms: u64,
        revocation_policy: ThroughputLeaseRevocationPolicy,
    ) -> ThroughputLease {
        ThroughputLease {
            lease_id: lease_id.to_string(),
            artifact_key: format!("artifact:{lease_id}"),
            resource_class: ResourceClass::LocalGeneration,
            target_silicon,
            holder_id: "persona:helper".to_string(),
            cost_units,
            acquired_at_ms: 100,
            expires_at_ms,
            revocation_policy,
        }
    }

    #[test]
    fn acquire_snapshot_and_release_tracks_target_silicon_cost() {
        let mut registry = ThroughputLeaseRegistry::new();
        registry
            .acquire(
                lease(
                    "gpu-a",
                    TargetSilicon::Gpu,
                    4,
                    1_000,
                    ThroughputLeaseRevocationPolicy::Graceful,
                ),
                100,
            )
            .unwrap();
        registry
            .acquire(
                lease(
                    "gpu-b",
                    TargetSilicon::Gpu,
                    6,
                    1_000,
                    ThroughputLeaseRevocationPolicy::Hard,
                ),
                100,
            )
            .unwrap();
        registry
            .acquire(
                lease(
                    "cpu",
                    TargetSilicon::Cpu,
                    2,
                    1_000,
                    ThroughputLeaseRevocationPolicy::Graceful,
                ),
                100,
            )
            .unwrap();

        let snapshot = registry.snapshot(200);
        assert_eq!(snapshot.active.len(), 3);
        assert_eq!(
            snapshot.cost_by_target_silicon.get(&TargetSilicon::Gpu),
            Some(&10)
        );
        assert_eq!(
            snapshot.cost_by_target_silicon.get(&TargetSilicon::Cpu),
            Some(&2)
        );

        let released = registry.release("gpu-a").unwrap();
        assert_eq!(released.lease_id, "gpu-a");
        assert_eq!(
            registry
                .snapshot(200)
                .cost_by_target_silicon
                .get(&TargetSilicon::Gpu),
            Some(&6)
        );
    }

    #[test]
    fn duplicate_and_missing_leases_fail_loudly() {
        let mut registry = ThroughputLeaseRegistry::new();
        let gpu = lease(
            "gpu",
            TargetSilicon::Gpu,
            1,
            1_000,
            ThroughputLeaseRevocationPolicy::Graceful,
        );
        registry.acquire(gpu.clone(), 100).unwrap();

        assert_eq!(
            registry.acquire(gpu, 100),
            Err(ThroughputLeaseError::DuplicateLease {
                lease_id: "gpu".to_string()
            })
        );
        assert_eq!(
            registry.release("missing"),
            Err(ThroughputLeaseError::MissingLease {
                lease_id: "missing".to_string()
            })
        );
    }

    #[test]
    fn expired_leases_are_not_counted_as_active_and_can_be_reaped() {
        let mut registry = ThroughputLeaseRegistry::new();
        registry
            .acquire(
                lease(
                    "old-frame",
                    TargetSilicon::Gpu,
                    1,
                    150,
                    ThroughputLeaseRevocationPolicy::Hard,
                ),
                100,
            )
            .unwrap();
        registry
            .acquire(
                lease(
                    "fresh-frame",
                    TargetSilicon::Gpu,
                    2,
                    1_000,
                    ThroughputLeaseRevocationPolicy::Hard,
                ),
                100,
            )
            .unwrap();

        let snapshot = registry.snapshot(200);
        assert_eq!(snapshot.active.len(), 1);
        assert_eq!(snapshot.expired.len(), 1);
        assert_eq!(
            snapshot.cost_by_target_silicon.get(&TargetSilicon::Gpu),
            Some(&2)
        );

        let expired = registry.expire(200);
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].lease_id, "old-frame");
        assert_eq!(registry.snapshot(200).expired.len(), 0);
    }

    #[test]
    fn pinned_active_leases_are_not_reclaimable_until_expired() {
        let mut registry = ThroughputLeaseRegistry::new();
        registry
            .acquire(
                lease(
                    "pinned",
                    TargetSilicon::Gpu,
                    8,
                    1_000,
                    ThroughputLeaseRevocationPolicy::Pinned,
                ),
                100,
            )
            .unwrap();
        registry
            .acquire(
                lease(
                    "revocable",
                    TargetSilicon::Gpu,
                    1,
                    1_000,
                    ThroughputLeaseRevocationPolicy::Graceful,
                ),
                100,
            )
            .unwrap();

        let reclaimable_now: Vec<String> = registry
            .reclaimable(200)
            .into_iter()
            .map(|lease| lease.lease_id)
            .collect();
        assert_eq!(reclaimable_now, vec!["revocable"]);

        let reclaimable_later: Vec<String> = registry
            .reclaimable(1_001)
            .into_iter()
            .map(|lease| lease.lease_id)
            .collect();
        assert_eq!(reclaimable_later, vec!["pinned", "revocable"]);
    }

    #[test]
    fn renew_extends_only_active_leases() {
        let mut registry = ThroughputLeaseRegistry::new();
        registry
            .acquire(
                lease(
                    "gpu",
                    TargetSilicon::Gpu,
                    1,
                    200,
                    ThroughputLeaseRevocationPolicy::Graceful,
                ),
                100,
            )
            .unwrap();

        registry.renew("gpu", 1_000, 150).unwrap();
        assert_eq!(registry.snapshot(500).active.len(), 1);

        assert_eq!(
            registry.renew("gpu", 2_000, 1_001),
            Err(ThroughputLeaseError::ExpiredLease {
                lease_id: "gpu".to_string()
            })
        );
    }
}
