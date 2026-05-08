//! Adaptive throughput planning primitives.
//!
//! This is the small, pure contract behind the "Adaptive Throughput
//! Substrate" architecture. It does not execute jobs, touch IPC, load
//! models, or inspect ORM state. It answers one question:
//!
//! Given ready artifacts, resource lane budgets, and a batch of proposed
//! jobs, which jobs should run now, which should defer, and which stale
//! duplicates should be dropped?
//!
//! Every expensive subsystem should eventually map into this shape: chat,
//! RAG, memory, embeddings, vision, live video, game observers, local
//! generation, LoRA paging, MoE expert routing, airc bridging, and
//! grid-distributed work.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[ts(
    export,
    export_to = "../../../shared/generated/cognition/ResourceClass.ts"
)]
pub enum ResourceClass {
    Cpu,
    Data,
    Gpu,
    Embedding,
    LocalGeneration,
    CloudProvider,
    Io,
    Media,
    Render,
    Memory,
    Background,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../shared/generated/cognition/ThroughputLaneBudget.ts"
)]
pub struct ThroughputLaneBudget {
    pub resource_class: ResourceClass,
    pub max_concurrency: usize,
    pub max_cost_units: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../shared/generated/cognition/ThroughputJob.ts"
)]
pub struct ThroughputJob {
    pub job_id: String,
    pub artifact_key: String,
    pub resource_class: ResourceClass,
    pub priority: u32,
    pub cost_units: u32,
    #[serde(default)]
    pub dependency_keys: Vec<String>,
    #[serde(default)]
    #[ts(type = "number")]
    pub created_at_ms: u64,
    /// Zero means never stale.
    #[serde(default)]
    #[ts(type = "number")]
    pub stale_after_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../shared/generated/cognition/AdaptiveThroughputRequest.ts"
)]
pub struct AdaptiveThroughputRequest {
    #[serde(default)]
    pub ready_artifact_keys: Vec<String>,
    pub lane_budgets: Vec<ThroughputLaneBudget>,
    pub jobs: Vec<ThroughputJob>,
    #[serde(default)]
    #[ts(type = "number")]
    pub now_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../shared/generated/cognition/AdaptiveThroughputPlan.ts"
)]
pub struct AdaptiveThroughputPlan {
    pub admitted: Vec<ThroughputJob>,
    pub deferred_missing_dependencies: Vec<ThroughputJob>,
    pub deferred_resource_pressure: Vec<ThroughputJob>,
    pub dropped_stale: Vec<ThroughputJob>,
    pub dropped_superseded: Vec<ThroughputJob>,
}

pub fn plan_adaptive_throughput(req: AdaptiveThroughputRequest) -> AdaptiveThroughputPlan {
    let ready_artifacts: BTreeSet<String> = req.ready_artifact_keys.into_iter().collect();
    let lane_budgets = normalize_lane_budgets(req.lane_budgets);
    let mut usable_jobs = Vec::new();
    let mut dropped_stale = Vec::new();

    for job in req.jobs {
        if is_stale(&job, req.now_ms) {
            dropped_stale.push(job);
        } else {
            usable_jobs.push(job);
        }
    }

    let (coalesced_jobs, dropped_superseded) = coalesce_by_identity(usable_jobs);

    let mut dependency_ready = Vec::new();
    let mut deferred_missing_dependencies = Vec::new();
    for job in coalesced_jobs {
        if dependencies_ready(&job, &ready_artifacts) {
            dependency_ready.push(job);
        } else {
            deferred_missing_dependencies.push(job);
        }
    }

    dependency_ready.sort_by(compare_jobs);

    let mut used_by_lane: BTreeMap<ResourceClass, (usize, u32)> = BTreeMap::new();
    let mut admitted = Vec::new();
    let mut deferred_resource_pressure = Vec::new();

    for job in dependency_ready {
        if can_admit(&job, &lane_budgets, &used_by_lane) {
            let used = used_by_lane.entry(job.resource_class).or_insert((0, 0));
            used.0 += 1;
            used.1 = used.1.saturating_add(job.cost_units);
            admitted.push(job);
        } else {
            deferred_resource_pressure.push(job);
        }
    }

    AdaptiveThroughputPlan {
        admitted,
        deferred_missing_dependencies,
        deferred_resource_pressure,
        dropped_stale,
        dropped_superseded,
    }
}

fn normalize_lane_budgets(
    budgets: Vec<ThroughputLaneBudget>,
) -> BTreeMap<ResourceClass, ThroughputLaneBudget> {
    budgets
        .into_iter()
        .map(|budget| (budget.resource_class, budget))
        .collect()
}

fn is_stale(job: &ThroughputJob, now_ms: u64) -> bool {
    job.stale_after_ms > 0
        && now_ms.saturating_sub(job.created_at_ms) > job.stale_after_ms
}

fn coalesce_by_identity(jobs: Vec<ThroughputJob>) -> (Vec<ThroughputJob>, Vec<ThroughputJob>) {
    let mut winners: BTreeMap<(ResourceClass, String), ThroughputJob> = BTreeMap::new();
    let mut dropped = Vec::new();

    for job in jobs {
        let key = (job.resource_class, job.artifact_key.clone());
        if let Some(existing) = winners.get(&key) {
            if compare_jobs(&job, existing).is_lt() {
                dropped.push(existing.clone());
                winners.insert(key, job);
            } else {
                dropped.push(job);
            }
        } else {
            winners.insert(key, job);
        }
    }

    (winners.into_values().collect(), dropped)
}

fn dependencies_ready(job: &ThroughputJob, ready_artifacts: &BTreeSet<String>) -> bool {
    job.dependency_keys
        .iter()
        .all(|key| ready_artifacts.contains(key))
}

fn can_admit(
    job: &ThroughputJob,
    budgets: &BTreeMap<ResourceClass, ThroughputLaneBudget>,
    used_by_lane: &BTreeMap<ResourceClass, (usize, u32)>,
) -> bool {
    let Some(budget) = budgets.get(&job.resource_class) else {
        return false;
    };
    let used = used_by_lane.get(&job.resource_class).copied().unwrap_or((0, 0));
    used.0 < budget.max_concurrency
        && used.1.saturating_add(job.cost_units) <= budget.max_cost_units
}

fn compare_jobs(left: &ThroughputJob, right: &ThroughputJob) -> std::cmp::Ordering {
    right
        .priority
        .cmp(&left.priority)
        .then_with(|| right.created_at_ms.cmp(&left.created_at_ms))
        .then_with(|| left.job_id.cmp(&right.job_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn budget(resource_class: ResourceClass, max_concurrency: usize) -> ThroughputLaneBudget {
        ThroughputLaneBudget {
            resource_class,
            max_concurrency,
            max_cost_units: 1_000,
        }
    }

    fn job(
        id: &str,
        artifact: &str,
        resource_class: ResourceClass,
        priority: u32,
    ) -> ThroughputJob {
        ThroughputJob {
            job_id: id.to_string(),
            artifact_key: artifact.to_string(),
            resource_class,
            priority,
            cost_units: 1,
            dependency_keys: Vec::new(),
            created_at_ms: 100,
            stale_after_ms: 0,
        }
    }

    #[test]
    fn independent_ready_work_is_not_blocked_by_missing_dependencies() {
        let mut blocked = job("blocked", "blocked-output", ResourceClass::LocalGeneration, 100);
        blocked.dependency_keys = vec!["missing-rag".to_string()];

        let plan = plan_adaptive_throughput(AdaptiveThroughputRequest {
            ready_artifact_keys: vec!["room-snapshot".to_string()],
            lane_budgets: vec![budget(ResourceClass::LocalGeneration, 1), budget(ResourceClass::Cpu, 4)],
            jobs: vec![
                blocked,
                job("cpu-ready", "analysis", ResourceClass::Cpu, 50),
                job("local-ready", "reply", ResourceClass::LocalGeneration, 40),
            ],
            now_ms: 150,
        });

        let admitted: Vec<&str> = plan.admitted.iter().map(|job| job.job_id.as_str()).collect();
        assert_eq!(admitted, vec!["cpu-ready", "local-ready"]);
        assert_eq!(plan.deferred_missing_dependencies.len(), 1);
        assert_eq!(plan.deferred_missing_dependencies[0].job_id, "blocked");
    }

    #[test]
    fn same_artifact_jobs_coalesce_to_latest_highest_priority_work() {
        let old = job("old", "turn-rag", ResourceClass::Cpu, 10);
        let mut new = job("new", "turn-rag", ResourceClass::Cpu, 10);
        new.created_at_ms = 200;

        let plan = plan_adaptive_throughput(AdaptiveThroughputRequest {
            ready_artifact_keys: Vec::new(),
            lane_budgets: vec![budget(ResourceClass::Cpu, 4)],
            jobs: vec![old, new],
            now_ms: 250,
        });

        assert_eq!(plan.admitted.len(), 1);
        assert_eq!(plan.admitted[0].job_id, "new");
        assert_eq!(plan.dropped_superseded.len(), 1);
        assert_eq!(plan.dropped_superseded[0].job_id, "old");
    }

    #[test]
    fn resource_lane_budget_defers_excess_without_blocking_other_lanes() {
        let plan = plan_adaptive_throughput(AdaptiveThroughputRequest {
            ready_artifact_keys: Vec::new(),
            lane_budgets: vec![budget(ResourceClass::LocalGeneration, 1), budget(ResourceClass::Embedding, 2)],
            jobs: vec![
                job("local-a", "reply-a", ResourceClass::LocalGeneration, 100),
                job("local-b", "reply-b", ResourceClass::LocalGeneration, 90),
                job("embed-a", "embedding-a", ResourceClass::Embedding, 10),
                job("embed-b", "embedding-b", ResourceClass::Embedding, 9),
            ],
            now_ms: 150,
        });

        let admitted: Vec<&str> = plan.admitted.iter().map(|job| job.job_id.as_str()).collect();
        assert_eq!(admitted, vec!["local-a", "embed-a", "embed-b"]);
        assert_eq!(plan.deferred_resource_pressure.len(), 1);
        assert_eq!(plan.deferred_resource_pressure[0].job_id, "local-b");
    }

    #[test]
    fn stale_work_is_dropped_before_it_consumes_lane_budget() {
        let mut stale = job("stale", "old-frame", ResourceClass::Gpu, 100);
        stale.created_at_ms = 0;
        stale.stale_after_ms = 50;

        let plan = plan_adaptive_throughput(AdaptiveThroughputRequest {
            ready_artifact_keys: Vec::new(),
            lane_budgets: vec![budget(ResourceClass::Gpu, 1)],
            jobs: vec![stale, job("fresh", "new-frame", ResourceClass::Gpu, 10)],
            now_ms: 100,
        });

        assert_eq!(plan.admitted.len(), 1);
        assert_eq!(plan.admitted[0].job_id, "fresh");
        assert_eq!(plan.dropped_stale.len(), 1);
        assert_eq!(plan.dropped_stale[0].job_id, "stale");
    }

    #[test]
    fn orm_inference_webrtc_and_bevy_paths_share_the_same_substrate() {
        let mut inference = job(
            "infer",
            "turn:1:reply",
            ResourceClass::LocalGeneration,
            90,
        );
        inference.dependency_keys = vec!["room:general:canonical".to_string()];

        let mut media = job("webrtc", "frame:42:decoded", ResourceClass::Media, 80);
        media.dependency_keys = vec!["packet:42".to_string()];

        let mut render = job("bevy", "texture:42", ResourceClass::Render, 70);
        render.dependency_keys = vec!["frame:42:decoded".to_string()];

        let plan = plan_adaptive_throughput(AdaptiveThroughputRequest {
            ready_artifact_keys: vec![
                "room:general:canonical".to_string(),
                "packet:42".to_string(),
            ],
            lane_budgets: vec![
                budget(ResourceClass::Data, 4),
                budget(ResourceClass::LocalGeneration, 1),
                budget(ResourceClass::Media, 2),
                budget(ResourceClass::Render, 1),
            ],
            jobs: vec![
                job("orm", "room:general:canonical", ResourceClass::Data, 100),
                inference,
                media,
                render,
            ],
            now_ms: 150,
        });

        let admitted: Vec<&str> = plan.admitted.iter().map(|job| job.job_id.as_str()).collect();
        assert_eq!(admitted, vec!["orm", "infer", "webrtc"]);
        assert_eq!(plan.deferred_missing_dependencies.len(), 1);
        assert_eq!(plan.deferred_missing_dependencies[0].job_id, "bevy");
    }
}
