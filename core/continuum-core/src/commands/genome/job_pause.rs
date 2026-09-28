//! `genome/job-pause` and `genome/job-resume`: pause a live in-engine training job at its
//! next window boundary, keeping its optimizer, adapter and memory, and resume it.
//!
//! The pause is a persisted fact keyed by the job
//! ([`crate::genome::fine_tuning::training_hold_store`]); the job's run steers the engine to it
//! every tick. (It is durable intent; a relaunched core does not yet re-attach to a running
//! job, see the store's module doc.) It is the one door: a direct engine
//! `/train/pause` with no hold is steered back by design. A paused run KEEPS its allocation,
//! so a pause reduces compute demand and never relieves memory.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::genome::fine_tuning::engine_lora_adapter::PROVIDER_ID as ENGINE_PROVIDER;
use crate::genome::fine_tuning::training_hold_store as store;
use crate::genome::fine_tuning::{FineTuningRegistry, JobHandle, TrainingStatus};

use super::JobLookupParams;

/// Wire shape for `genome/job-pause`.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/JobPauseParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct JobPauseParams {
    /// The job handle returned by `genome/job-create`.
    #[serde(rename = "jobHandle")]
    pub handle: JobHandle,
    /// Why it is paused; it names the pause in the run's probes.
    pub reason: String,
    /// How long the pause stands unless resumed first, at most one day (86400000).
    #[ts(type = "number")]
    pub ttl_ms: u64,
}

/// Outcome envelope for `genome/job-pause` and `genome/job-resume`. A pause carries its
/// `holdId` and `expiresAtMs`; a resume carries how many pauses it `released`. On
/// `success=false`, `errorKind` is `Unsupported` (not an in-engine job), `NotRunning`, or
/// `Store` (the pause file could not be written).
#[derive(Debug, Clone, Default, Serialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/JobPauseOutcome.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct JobPauseOutcome {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub hold_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub expires_at_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub released: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error_kind: Option<String>,
}

fn refused(kind: &str, error: String) -> JobPauseOutcome {
    JobPauseOutcome { success: false, error: Some(error), error_kind: Some(kind.to_string()), ..Default::default() }
}

fn store_path() -> Result<std::path::PathBuf, JobPauseOutcome> {
    crate::commands::benchmark::continuum_home()
        .map(|home| store::store_path(&home))
        .map_err(|e| refused("Store", e.to_string()))
}

crate::action_command! {
    /// Pause a live in-engine training job at its next window boundary until `genome/job-resume`
    /// or `ttlMs` passes. The job keeps its optimizer, adapter and memory while paused.
    pub struct GenomeJobPause { registry: Arc<FineTuningRegistry> }
    name: "genome/job-pause",
    access: Privileged,
    params: JobPauseParams,
    output: JobPauseOutcome,
    run(this, _ctx, p) => {
        if p.handle.provider_id != ENGINE_PROVIDER {
            return Ok(refused("Unsupported", format!(
                "only in-engine jobs ({ENGINE_PROVIDER}) pause; this one is {:?}", p.handle.provider_id
            )));
        }
        // a pause on a job that is not running would stand over nothing until its TTL
        let live = match this.registry.get(&p.handle.provider_id) {
            Some(adapter) => adapter.poll(&p.handle).await,
            None => return Ok(refused("Unsupported", format!("no adapter registered for {ENGINE_PROVIDER}"))),
        };
        if !matches!(live, Ok(TrainingStatus::Queued | TrainingStatus::WaitingForCapacity { .. } | TrainingStatus::Running { .. })) {
            return Ok(refused("NotRunning", format!("job {} is not queued, waiting or running", p.handle.local_id)));
        }
        let path = match store_path() {
            Ok(path) => path,
            Err(outcome) => return Ok(outcome),
        };
        match store::add(&path, p.handle.local_id, &p.reason, p.ttl_ms, store::now_ms()) {
            Ok(hold) => Ok(JobPauseOutcome {
                success: true,
                hold_id: Some(hold.id.to_string()),
                expires_at_ms: Some(hold.created_ms.saturating_add(hold.ttl_ms)),
                ..Default::default()
            }),
            Err(e) => Ok(refused("Store", e)),
        }
    }
}

crate::action_command! {
    /// Resume an in-engine training job: release every pause on it. The run resumes at the
    /// next tick. A job with no pause resumes nothing and still succeeds (`released: 0`).
    pub struct GenomeJobResume;
    name: "genome/job-resume",
    access: Privileged,
    params: JobLookupParams,
    output: JobPauseOutcome,
    run(_this, _ctx, p) => {
        let path = match store_path() {
            Ok(path) => path,
            Err(outcome) => return Ok(outcome),
        };
        match store::release_job(&path, p.handle.local_id, store::now_ms()) {
            Ok(released) => Ok(JobPauseOutcome { success: true, released: Some(released as u64), ..Default::default() }),
            Err(e) => Ok(refused("Store", e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::genome::test_support::registry_with;
    use crate::sdk_codegen::{ActionCommand, Ctx};
    use uuid::Uuid;

    // what this catches: a pause recorded for a job it can never steer. Only an in-engine job
    // can pause (another provider's run has no window to stop at), and a pause is refused
    // before anything is written, so no hold stands over nothing until its TTL.
    #[tokio::test]
    async fn a_pause_is_refused_for_a_job_that_is_not_in_engine() {
        let cmd = GenomeJobPause { registry: registry_with(&["mlx-local"]) };
        let handle = JobHandle { provider_id: "mlx-local".into(), provider_job_id: "x".into(), local_id: Uuid::from_u128(5) };
        let out = cmd
            .run(&Ctx::default(), JobPauseParams { handle, reason: "test".into(), ttl_ms: 60_000 })
            .await
            .expect("test: runs");
        assert!(!out.success);
        assert_eq!(out.error_kind.as_deref(), Some("Unsupported"));
    }
}
