use crate::resources::{
    ResourceClass, TargetSilicon, ThroughputLease, ThroughputLeaseError, ThroughputLeaseRegistry,
    ThroughputLeaseRevocationPolicy,
};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceBrokerConfig {
    pub lane_budgets: Vec<ResourceLaneBudget>,
}

impl ResourceBrokerConfig {
    pub fn local_default() -> Self {
        let logical_cpus = std::thread::available_parallelism()
            .map(|n| n.get())
            .expect("host must report available parallelism for resource defaults");
        let gpu_slots = match std::env::var("CONTINUUM_GPU_CONCURRENCY") {
            Ok(raw) => {
                let parsed = raw.parse::<usize>().unwrap_or_else(|e| {
                    panic!("CONTINUUM_GPU_CONCURRENCY must be a positive integer: {e}")
                });
                assert!(
                    parsed > 0,
                    "CONTINUUM_GPU_CONCURRENCY must be greater than zero"
                );
                parsed
            }
            Err(std::env::VarError::NotPresent) => logical_cpus.clamp(4, 8),
            Err(std::env::VarError::NotUnicode(_)) => {
                panic!("CONTINUUM_GPU_CONCURRENCY must be valid UTF-8")
            }
        };
        let scaled_cost = |slots: usize| (slots as u32).saturating_mul(100);

        Self {
            lane_budgets: vec![
                ResourceLaneBudget {
                    resource_class: ResourceClass::Cpu,
                    target_silicon: TargetSilicon::Cpu,
                    max_concurrency: logical_cpus,
                    max_cost_units: scaled_cost(logical_cpus),
                },
                ResourceLaneBudget {
                    resource_class: ResourceClass::Gpu,
                    target_silicon: TargetSilicon::Gpu,
                    max_concurrency: gpu_slots,
                    max_cost_units: scaled_cost(gpu_slots),
                },
                ResourceLaneBudget {
                    resource_class: ResourceClass::Memory,
                    target_silicon: TargetSilicon::UnifiedMemory,
                    max_concurrency: logical_cpus,
                    max_cost_units: scaled_cost(logical_cpus),
                },
                ResourceLaneBudget {
                    resource_class: ResourceClass::Io,
                    target_silicon: TargetSilicon::Disk,
                    max_concurrency: logical_cpus,
                    max_cost_units: scaled_cost(logical_cpus),
                },
                ResourceLaneBudget {
                    resource_class: ResourceClass::CloudProvider,
                    target_silicon: TargetSilicon::Network,
                    max_concurrency: logical_cpus,
                    max_cost_units: scaled_cost(logical_cpus),
                },
                ResourceLaneBudget {
                    resource_class: ResourceClass::Background,
                    target_silicon: TargetSilicon::Background,
                    max_concurrency: logical_cpus,
                    max_cost_units: scaled_cost(logical_cpus),
                },
            ],
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResourceLaneBudget {
    pub resource_class: ResourceClass,
    pub target_silicon: TargetSilicon,
    pub max_concurrency: usize,
    pub max_cost_units: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResourceDemand {
    pub demand_id: String,
    pub holder_id: String,
    pub artifact_key: String,
    pub resource_class: ResourceClass,
    pub target_silicon: TargetSilicon,
    pub priority: u32,
    pub cost_units: u32,
    #[serde(default)]
    pub dependency_keys: Vec<String>,
    #[serde(default)]
    pub created_at_ms: u64,
    #[serde(default)]
    pub stale_after_ms: u64,
    pub ttl_ms: u64,
    pub revocation_policy: ThroughputLeaseRevocationPolicy,
}

impl ResourceDemand {
    pub fn persona_generation(
        persona_id: impl Into<String>,
        event_id: impl Into<String>,
        priority: u32,
        cost_units: u32,
        ttl_ms: u64,
    ) -> Self {
        let persona_id = persona_id.into();
        let event_id = event_id.into();
        Self {
            demand_id: format!("persona:{persona_id}:generate:{event_id}"),
            holder_id: format!("persona:{persona_id}"),
            artifact_key: format!("persona:{persona_id}:event:{event_id}:reply"),
            resource_class: ResourceClass::LocalGeneration,
            target_silicon: TargetSilicon::Gpu,
            priority,
            cost_units,
            dependency_keys: Vec::new(),
            created_at_ms: 0,
            stale_after_ms: 0,
            ttl_ms,
            revocation_policy: ThroughputLeaseRevocationPolicy::Pinned,
        }
    }

    fn is_stale(&self, now_ms: u64) -> bool {
        self.stale_after_ms > 0 && now_ms.saturating_sub(self.created_at_ms) > self.stale_after_ms
    }

    fn lease_id(&self) -> String {
        format!(
            "{}:{}:{}",
            self.holder_id, self.artifact_key, self.created_at_ms
        )
    }

    fn into_lease(self, now_ms: u64) -> ThroughputLease {
        ThroughputLease {
            lease_id: self.lease_id(),
            artifact_key: self.artifact_key,
            resource_class: self.resource_class,
            target_silicon: self.target_silicon,
            holder_id: self.holder_id,
            cost_units: self.cost_units,
            acquired_at_ms: now_ms,
            expires_at_ms: now_ms.saturating_add(self.ttl_ms),
            revocation_policy: self.revocation_policy,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ResourceRefusalReason {
    MissingDependency,
    NoBudget,
    ResourcePressure,
    Stale,
    Superseded,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResourceAdmissionReport {
    pub admitted: Vec<ThroughputLease>,
    pub refused: Vec<(ResourceDemand, ResourceRefusalReason)>,
    pub expired: Vec<ThroughputLease>,
}

#[derive(Debug)]
pub struct ResourceBroker {
    budgets: BTreeMap<TargetSilicon, ResourceLaneBudget>,
    leases: ThroughputLeaseRegistry,
}

impl ResourceBroker {
    pub fn new(config: ResourceBrokerConfig) -> Self {
        let budgets = config
            .lane_budgets
            .into_iter()
            .map(|budget| (budget.target_silicon, budget))
            .collect();
        Self {
            budgets,
            leases: ThroughputLeaseRegistry::new(),
        }
    }

    pub fn local_default() -> Self {
        Self::new(ResourceBrokerConfig::local_default())
    }

    pub fn lane_budgets(&self) -> Vec<ResourceLaneBudget> {
        self.budgets.values().copied().collect()
    }

    pub fn active_leases(&self, now_ms: u64) -> crate::resources::ThroughputLeaseSnapshot {
        self.leases.snapshot(now_ms)
    }

    pub fn reclaimable(&self, now_ms: u64) -> Vec<ThroughputLease> {
        self.leases.reclaimable(now_ms)
    }

    pub fn release(&mut self, lease_id: &str) -> Result<ThroughputLease, ThroughputLeaseError> {
        self.leases.release(lease_id)
    }

    pub fn admit(
        &mut self,
        demands: Vec<ResourceDemand>,
        ready_artifact_keys: Vec<String>,
        now_ms: u64,
    ) -> ResourceAdmissionReport {
        let expired = self.leases.expire(now_ms);
        let ready: BTreeSet<String> = ready_artifact_keys.into_iter().collect();
        let mut refused = Vec::new();
        let mut usable = Vec::new();

        for demand in demands {
            if demand.is_stale(now_ms) {
                refused.push((demand, ResourceRefusalReason::Stale));
            } else {
                usable.push(demand);
            }
        }

        let (mut candidates, superseded) = coalesce(usable);
        refused.extend(
            superseded
                .into_iter()
                .map(|demand| (demand, ResourceRefusalReason::Superseded)),
        );
        candidates.sort_by(compare_demands);

        let mut used = self.used_capacity(now_ms);
        let mut admitted = Vec::new();

        for demand in candidates {
            if !dependencies_ready(&demand, &ready) {
                refused.push((demand, ResourceRefusalReason::MissingDependency));
                continue;
            }

            let Some(budget) = self.budgets.get(&demand.target_silicon) else {
                refused.push((demand, ResourceRefusalReason::NoBudget));
                continue;
            };

            let lane = used.entry(demand.target_silicon).or_insert((0usize, 0u32));
            let can_fit = lane.0 < budget.max_concurrency
                && lane.1.saturating_add(demand.cost_units) <= budget.max_cost_units;

            if !can_fit {
                refused.push((demand, ResourceRefusalReason::ResourcePressure));
                continue;
            }

            lane.0 += 1;
            lane.1 = lane.1.saturating_add(demand.cost_units);
            let lease = demand.into_lease(now_ms);
            self.leases
                .acquire(lease.clone(), now_ms)
                .expect("lease id should be unique after demand coalescing");
            admitted.push(lease);
        }

        ResourceAdmissionReport {
            admitted,
            refused,
            expired,
        }
    }

    fn used_capacity(&self, now_ms: u64) -> BTreeMap<TargetSilicon, (usize, u32)> {
        let mut used = BTreeMap::new();
        for lease in self.leases.snapshot(now_ms).active {
            let lane = used.entry(lease.target_silicon).or_insert((0usize, 0u32));
            lane.0 += 1;
            lane.1 = lane.1.saturating_add(lease.cost_units);
        }
        used
    }
}

fn dependencies_ready(demand: &ResourceDemand, ready: &BTreeSet<String>) -> bool {
    demand.dependency_keys.iter().all(|key| ready.contains(key))
}

fn coalesce(demands: Vec<ResourceDemand>) -> (Vec<ResourceDemand>, Vec<ResourceDemand>) {
    let mut winners: BTreeMap<(ResourceClass, String, String), ResourceDemand> = BTreeMap::new();
    let mut dropped = Vec::new();

    for demand in demands {
        let key = (
            demand.resource_class,
            demand.holder_id.clone(),
            demand.artifact_key.clone(),
        );
        if let Some(existing) = winners.get(&key) {
            if compare_demands(&demand, existing).is_lt() {
                dropped.push(existing.clone());
                winners.insert(key, demand);
            } else {
                dropped.push(demand);
            }
        } else {
            winners.insert(key, demand);
        }
    }

    (winners.into_values().collect(), dropped)
}

fn compare_demands(left: &ResourceDemand, right: &ResourceDemand) -> Ordering {
    right
        .priority
        .cmp(&left.priority)
        .then_with(|| right.created_at_ms.cmp(&left.created_at_ms))
        .then_with(|| left.demand_id.cmp(&right.demand_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn broker(gpu_slots: usize) -> ResourceBroker {
        ResourceBroker::new(ResourceBrokerConfig {
            lane_budgets: vec![
                ResourceLaneBudget {
                    resource_class: ResourceClass::LocalGeneration,
                    target_silicon: TargetSilicon::Gpu,
                    max_concurrency: gpu_slots,
                    max_cost_units: 100,
                },
                ResourceLaneBudget {
                    resource_class: ResourceClass::Cpu,
                    target_silicon: TargetSilicon::Cpu,
                    max_concurrency: 4,
                    max_cost_units: 100,
                },
            ],
        })
    }

    #[test]
    fn independent_personas_on_same_event_are_not_coalesced() {
        let mut broker = broker(4);
        let event_id = "chat:general:42";

        let report = broker.admit(
            vec![
                ResourceDemand::persona_generation("helper", event_id, 80, 10, 1_000),
                ResourceDemand::persona_generation("planner", event_id, 79, 10, 1_000),
                ResourceDemand::persona_generation("critic", event_id, 78, 10, 1_000),
            ],
            Vec::new(),
            100,
        );

        let holders: Vec<&str> = report
            .admitted
            .iter()
            .map(|lease| lease.holder_id.as_str())
            .collect();
        assert_eq!(
            holders,
            vec!["persona:helper", "persona:planner", "persona:critic"]
        );
        assert!(report.refused.is_empty());
    }

    #[test]
    fn active_leases_reserve_capacity_across_batches() {
        let mut broker = broker(2);
        let first = broker.admit(
            vec![ResourceDemand::persona_generation(
                "helper", "event-a", 90, 10, 1_000,
            )],
            Vec::new(),
            100,
        );
        assert_eq!(first.admitted.len(), 1);

        let second = broker.admit(
            vec![
                ResourceDemand::persona_generation("planner", "event-a", 89, 10, 1_000),
                ResourceDemand::persona_generation("critic", "event-a", 88, 10, 1_000),
            ],
            Vec::new(),
            101,
        );

        assert_eq!(second.admitted.len(), 1);
        assert_eq!(second.admitted[0].holder_id, "persona:planner");
        assert_eq!(second.refused.len(), 1);
        assert_eq!(second.refused[0].0.holder_id, "persona:critic");
        assert_eq!(second.refused[0].1, ResourceRefusalReason::ResourcePressure);
    }

    #[test]
    fn same_holder_same_artifact_coalesces_without_cross_persona_suppression() {
        let mut broker = broker(4);
        let mut old = ResourceDemand::persona_generation("helper", "event-a", 10, 10, 1_000);
        old.created_at_ms = 100;
        let mut new = old.clone();
        new.demand_id = "newer".to_string();
        new.priority = 20;
        new.created_at_ms = 200;
        let other_persona = ResourceDemand::persona_generation("planner", "event-a", 10, 10, 1_000);

        let report = broker.admit(vec![old, new, other_persona], Vec::new(), 250);

        let holders: Vec<&str> = report
            .admitted
            .iter()
            .map(|lease| lease.holder_id.as_str())
            .collect();
        assert_eq!(holders, vec!["persona:helper", "persona:planner"]);
        assert_eq!(report.refused.len(), 1);
        assert_eq!(report.refused[0].1, ResourceRefusalReason::Superseded);
    }

    #[test]
    fn pinned_leases_are_not_reclaimable_until_expired() {
        let mut broker = ResourceBroker::new(ResourceBrokerConfig {
            lane_budgets: vec![ResourceLaneBudget {
                resource_class: ResourceClass::Memory,
                target_silicon: TargetSilicon::UnifiedMemory,
                max_concurrency: 2,
                max_cost_units: 100,
            }],
        });
        let report = broker.admit(
            vec![ResourceDemand {
                demand_id: "genome-page".to_string(),
                holder_id: "persona:helper".to_string(),
                artifact_key: "lora:rust-expert".to_string(),
                resource_class: ResourceClass::Memory,
                target_silicon: TargetSilicon::UnifiedMemory,
                priority: 100,
                cost_units: 1,
                dependency_keys: Vec::new(),
                created_at_ms: 100,
                stale_after_ms: 0,
                ttl_ms: 1_000,
                revocation_policy: ThroughputLeaseRevocationPolicy::Pinned,
            }],
            Vec::new(),
            100,
        );

        assert_eq!(report.admitted.len(), 1);
        assert!(broker.reclaimable(500).is_empty());
        assert_eq!(broker.reclaimable(1_101).len(), 1);
    }
}
