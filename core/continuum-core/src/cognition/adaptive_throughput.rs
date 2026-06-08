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
//!
//! This is a planner, not a scheduler. Callers re-plan when MessageBus (or
//! another wake source) reports that artifact keys became ready. The lease
//! layer will later connect these admitted jobs to FootprintRegistry and
//! PressureBroker ownership; this module intentionally stays pure.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/ResourceClass.ts"
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

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/TargetSilicon.ts"
)]
pub enum TargetSilicon {
    Cpu,
    Gpu,
    UnifiedMemory,
    Network,
    Disk,
    Cloud,
    Background,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/ThroughputLaneBudget.ts"
)]
pub struct ThroughputLaneBudget {
    /// Semantic owner for observability. Admission is keyed by target_silicon
    /// so LocalGeneration, Media, and Render can share one physical GPU budget.
    pub resource_class: ResourceClass,
    pub target_silicon: TargetSilicon,
    pub max_concurrency: usize,
    pub max_cost_units: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/ThroughputJob.ts"
)]
pub struct ThroughputJob {
    pub job_id: String,
    pub artifact_key: String,
    pub resource_class: ResourceClass,
    pub target_silicon: TargetSilicon,
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
    export_to = "../../../protocol/typescript/cognition/AdaptiveThroughputRequest.ts"
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
    export_to = "../../../protocol/typescript/cognition/AdaptiveThroughputPlan.ts"
)]
pub struct AdaptiveThroughputPlan {
    pub admitted: Vec<ThroughputJob>,
    pub deferred_missing_dependencies: Vec<ThroughputJob>,
    /// Jobs whose target_silicon has no declared budget. This is a
    /// configuration error, not normal backpressure: callers should surface it
    /// loudly instead of retrying forever.
    pub dropped_no_budget: Vec<ThroughputJob>,
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

    let mut used_by_lane: BTreeMap<TargetSilicon, (usize, u32)> = BTreeMap::new();
    let mut admitted = Vec::new();
    let mut dropped_no_budget = Vec::new();
    let mut deferred_resource_pressure = Vec::new();

    for job in dependency_ready {
        match admit_decision(&job, &lane_budgets, &used_by_lane) {
            AdmissionDecision::Admit => {
                let used = used_by_lane.entry(job.target_silicon).or_insert((0, 0));
                used.0 += 1;
                used.1 = used.1.saturating_add(job.cost_units);
                admitted.push(job);
            }
            AdmissionDecision::NoBudget => dropped_no_budget.push(job),
            AdmissionDecision::ResourcePressure => deferred_resource_pressure.push(job),
        }
    }

    AdaptiveThroughputPlan {
        admitted,
        deferred_missing_dependencies,
        dropped_no_budget,
        deferred_resource_pressure,
        dropped_stale,
        dropped_superseded,
    }
}

fn normalize_lane_budgets(
    budgets: Vec<ThroughputLaneBudget>,
) -> BTreeMap<TargetSilicon, ThroughputLaneBudget> {
    budgets
        .into_iter()
        .map(|budget| (budget.target_silicon, budget))
        .collect()
}

fn is_stale(job: &ThroughputJob, now_ms: u64) -> bool {
    job.stale_after_ms > 0 && now_ms.saturating_sub(job.created_at_ms) > job.stale_after_ms
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

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum AdmissionDecision {
    Admit,
    NoBudget,
    ResourcePressure,
}

fn admit_decision(
    job: &ThroughputJob,
    budgets: &BTreeMap<TargetSilicon, ThroughputLaneBudget>,
    used_by_lane: &BTreeMap<TargetSilicon, (usize, u32)>,
) -> AdmissionDecision {
    let Some(budget) = budgets.get(&job.target_silicon) else {
        return AdmissionDecision::NoBudget;
    };
    let used = used_by_lane
        .get(&job.target_silicon)
        .copied()
        .unwrap_or((0, 0));
    if used.0 < budget.max_concurrency
        && used.1.saturating_add(job.cost_units) <= budget.max_cost_units
    {
        AdmissionDecision::Admit
    } else {
        AdmissionDecision::ResourcePressure
    }
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

    fn budget(
        resource_class: ResourceClass,
        target_silicon: TargetSilicon,
        max_concurrency: usize,
    ) -> ThroughputLaneBudget {
        ThroughputLaneBudget {
            resource_class,
            target_silicon,
            max_concurrency,
            max_cost_units: 1_000,
        }
    }

    fn job(
        id: &str,
        artifact: &str,
        resource_class: ResourceClass,
        target_silicon: TargetSilicon,
        priority: u32,
    ) -> ThroughputJob {
        ThroughputJob {
            job_id: id.to_string(),
            artifact_key: artifact.to_string(),
            resource_class,
            target_silicon,
            priority,
            cost_units: 1,
            dependency_keys: Vec::new(),
            created_at_ms: 100,
            stale_after_ms: 0,
        }
    }

    #[test]
    fn independent_ready_work_is_not_blocked_by_missing_dependencies() {
        let mut blocked = job(
            "blocked",
            "blocked-output",
            ResourceClass::LocalGeneration,
            TargetSilicon::Gpu,
            100,
        );
        blocked.dependency_keys = vec!["missing-rag".to_string()];

        let plan = plan_adaptive_throughput(AdaptiveThroughputRequest {
            ready_artifact_keys: vec!["room-snapshot".to_string()],
            lane_budgets: vec![
                budget(ResourceClass::LocalGeneration, TargetSilicon::Gpu, 1),
                budget(ResourceClass::Cpu, TargetSilicon::Cpu, 4),
            ],
            jobs: vec![
                blocked,
                job(
                    "cpu-ready",
                    "analysis",
                    ResourceClass::Cpu,
                    TargetSilicon::Cpu,
                    50,
                ),
                job(
                    "local-ready",
                    "reply",
                    ResourceClass::LocalGeneration,
                    TargetSilicon::Gpu,
                    40,
                ),
            ],
            now_ms: 150,
        });

        let admitted: Vec<&str> = plan
            .admitted
            .iter()
            .map(|job| job.job_id.as_str())
            .collect();
        assert_eq!(admitted, vec!["cpu-ready", "local-ready"]);
        assert_eq!(plan.deferred_missing_dependencies.len(), 1);
        assert_eq!(plan.deferred_missing_dependencies[0].job_id, "blocked");
    }

    #[test]
    fn same_artifact_jobs_coalesce_to_latest_highest_priority_work() {
        let old = job(
            "old",
            "turn-rag",
            ResourceClass::Cpu,
            TargetSilicon::Cpu,
            10,
        );
        let mut new = job(
            "new",
            "turn-rag",
            ResourceClass::Cpu,
            TargetSilicon::Cpu,
            10,
        );
        new.created_at_ms = 200;

        let plan = plan_adaptive_throughput(AdaptiveThroughputRequest {
            ready_artifact_keys: Vec::new(),
            lane_budgets: vec![budget(ResourceClass::Cpu, TargetSilicon::Cpu, 4)],
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
            lane_budgets: vec![
                budget(ResourceClass::LocalGeneration, TargetSilicon::Gpu, 1),
                budget(ResourceClass::Embedding, TargetSilicon::Cpu, 2),
            ],
            jobs: vec![
                job(
                    "local-a",
                    "reply-a",
                    ResourceClass::LocalGeneration,
                    TargetSilicon::Gpu,
                    100,
                ),
                job(
                    "local-b",
                    "reply-b",
                    ResourceClass::LocalGeneration,
                    TargetSilicon::Gpu,
                    90,
                ),
                job(
                    "embed-a",
                    "embedding-a",
                    ResourceClass::Embedding,
                    TargetSilicon::Cpu,
                    10,
                ),
                job(
                    "embed-b",
                    "embedding-b",
                    ResourceClass::Embedding,
                    TargetSilicon::Cpu,
                    9,
                ),
            ],
            now_ms: 150,
        });

        let admitted: Vec<&str> = plan
            .admitted
            .iter()
            .map(|job| job.job_id.as_str())
            .collect();
        assert_eq!(admitted, vec!["local-a", "embed-a", "embed-b"]);
        assert_eq!(plan.deferred_resource_pressure.len(), 1);
        assert_eq!(plan.deferred_resource_pressure[0].job_id, "local-b");
    }

    #[test]
    fn stale_work_is_dropped_before_it_consumes_lane_budget() {
        let mut stale = job(
            "stale",
            "old-frame",
            ResourceClass::Gpu,
            TargetSilicon::Gpu,
            100,
        );
        stale.created_at_ms = 0;
        stale.stale_after_ms = 50;

        let plan = plan_adaptive_throughput(AdaptiveThroughputRequest {
            ready_artifact_keys: Vec::new(),
            lane_budgets: vec![budget(ResourceClass::Gpu, TargetSilicon::Gpu, 1)],
            jobs: vec![
                stale,
                job(
                    "fresh",
                    "new-frame",
                    ResourceClass::Gpu,
                    TargetSilicon::Gpu,
                    10,
                ),
            ],
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
            TargetSilicon::Gpu,
            90,
        );
        inference.dependency_keys = vec!["room:general:canonical".to_string()];

        let mut media = job(
            "webrtc",
            "frame:42:decoded",
            ResourceClass::Media,
            TargetSilicon::Gpu,
            80,
        );
        media.dependency_keys = vec!["packet:42".to_string()];

        let mut render = job(
            "bevy",
            "texture:42",
            ResourceClass::Render,
            TargetSilicon::Gpu,
            70,
        );
        render.dependency_keys = vec!["frame:42:decoded".to_string()];

        let plan = plan_adaptive_throughput(AdaptiveThroughputRequest {
            ready_artifact_keys: vec![
                "room:general:canonical".to_string(),
                "packet:42".to_string(),
            ],
            lane_budgets: vec![
                budget(ResourceClass::Data, TargetSilicon::Cpu, 4),
                budget(ResourceClass::LocalGeneration, TargetSilicon::Gpu, 2),
            ],
            jobs: vec![
                job(
                    "orm",
                    "room:general:canonical",
                    ResourceClass::Data,
                    TargetSilicon::Cpu,
                    100,
                ),
                inference,
                media,
                render,
            ],
            now_ms: 150,
        });

        let admitted: Vec<&str> = plan
            .admitted
            .iter()
            .map(|job| job.job_id.as_str())
            .collect();
        assert_eq!(admitted, vec!["orm", "infer", "webrtc"]);
        assert_eq!(plan.deferred_missing_dependencies.len(), 1);
        assert_eq!(plan.deferred_missing_dependencies[0].job_id, "bevy");
    }

    #[test]
    fn replanning_moves_dependency_ready_work_into_admitted() {
        let mut render = job(
            "bevy",
            "texture:42",
            ResourceClass::Render,
            TargetSilicon::Gpu,
            70,
        );
        render.dependency_keys = vec!["frame:42:decoded".to_string()];

        let first_plan = plan_adaptive_throughput(AdaptiveThroughputRequest {
            ready_artifact_keys: Vec::new(),
            lane_budgets: vec![budget(ResourceClass::Render, TargetSilicon::Gpu, 1)],
            jobs: vec![render.clone()],
            now_ms: 150,
        });

        assert_eq!(first_plan.admitted.len(), 0);
        assert_eq!(first_plan.deferred_missing_dependencies.len(), 1);

        let second_plan = plan_adaptive_throughput(AdaptiveThroughputRequest {
            ready_artifact_keys: vec!["frame:42:decoded".to_string()],
            lane_budgets: vec![budget(ResourceClass::Render, TargetSilicon::Gpu, 1)],
            jobs: vec![render],
            now_ms: 151,
        });

        assert_eq!(second_plan.deferred_missing_dependencies.len(), 0);
        assert_eq!(second_plan.admitted.len(), 1);
        assert_eq!(second_plan.admitted[0].job_id, "bevy");
    }

    #[test]
    fn gpu_bound_work_shares_one_physical_budget_across_semantic_classes() {
        let plan = plan_adaptive_throughput(AdaptiveThroughputRequest {
            ready_artifact_keys: Vec::new(),
            lane_budgets: vec![budget(ResourceClass::Gpu, TargetSilicon::Gpu, 2)],
            jobs: vec![
                job(
                    "local-a",
                    "reply-a",
                    ResourceClass::LocalGeneration,
                    TargetSilicon::Gpu,
                    100,
                ),
                job(
                    "local-b",
                    "reply-b",
                    ResourceClass::LocalGeneration,
                    TargetSilicon::Gpu,
                    99,
                ),
                job(
                    "media",
                    "frame:42",
                    ResourceClass::Media,
                    TargetSilicon::Gpu,
                    98,
                ),
                job(
                    "render",
                    "texture:42",
                    ResourceClass::Render,
                    TargetSilicon::Gpu,
                    97,
                ),
            ],
            now_ms: 150,
        });

        let admitted: Vec<&str> = plan
            .admitted
            .iter()
            .map(|job| job.job_id.as_str())
            .collect();
        let deferred: Vec<&str> = plan
            .deferred_resource_pressure
            .iter()
            .map(|job| job.job_id.as_str())
            .collect();
        assert_eq!(admitted, vec!["local-a", "local-b"]);
        assert_eq!(deferred, vec!["media", "render"]);
    }

    #[test]
    fn missing_physical_budget_is_loud_not_indefinite_backpressure() {
        let plan = plan_adaptive_throughput(AdaptiveThroughputRequest {
            ready_artifact_keys: Vec::new(),
            lane_budgets: vec![budget(ResourceClass::Cpu, TargetSilicon::Cpu, 4)],
            jobs: vec![
                job(
                    "cpu",
                    "analysis",
                    ResourceClass::Cpu,
                    TargetSilicon::Cpu,
                    100,
                ),
                job(
                    "local",
                    "reply",
                    ResourceClass::LocalGeneration,
                    TargetSilicon::Gpu,
                    90,
                ),
            ],
            now_ms: 150,
        });

        assert_eq!(plan.admitted.len(), 1);
        assert_eq!(plan.admitted[0].job_id, "cpu");
        assert_eq!(plan.deferred_resource_pressure.len(), 0);
        assert_eq!(plan.dropped_no_budget.len(), 1);
        assert_eq!(plan.dropped_no_budget[0].job_id, "local");
    }
}
