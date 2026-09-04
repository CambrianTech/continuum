//! `genome/job-status` — look the adapter up by `handle.providerId`, poll, return
//! the typed [`TrainingStatus`].

use std::sync::Arc;

use serde::Serialize;
use ts_rs::TS;

use crate::genome::fine_tuning::{FineTuningRegistry, TrainingStatus};

use super::{fine_tuning_error_kind, JobLookupParams};

/// Outcome envelope for `genome/job-status`. `success=true` carries the polled
/// `status`; `success=false` carries `error` + an `errorKind` slug (`UnknownHandle`
/// when no adapter owns the handle's provider, else the adapter's error kind).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/JobStatusOutcome.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct JobStatusOutcome {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub status: Option<TrainingStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error_kind: Option<String>,
}

crate::action_command! {
    /// Poll a fine-tuning job's status by its handle. The adapter is looked back up
    /// from `handle.providerId`; on a hit the typed `TrainingStatus` (Queued /
    /// Running / Completed / Failed) comes back. An unknown provider yields
    /// `success=false` with `errorKind="UnknownHandle"`; an adapter error yields its
    /// own `errorKind` slug.
    pub struct GenomeJobStatus { registry: Arc<FineTuningRegistry> }
    name: "genome/job-status",
    access: Privileged,
    params: JobLookupParams,
    output: JobStatusOutcome,
    run(this, _ctx, p) => {
        let adapter = match this.registry.get(&p.handle.provider_id) {
            Some(a) => a,
            None => {
                return Ok(JobStatusOutcome {
                    success: false,
                    status: None,
                    error: Some(format!(
                        "no adapter registered for provider {:?}",
                        p.handle.provider_id
                    )),
                    error_kind: Some("UnknownHandle".to_string()),
                });
            }
        };

        match adapter.poll(&p.handle).await {
            Ok(status) => Ok(JobStatusOutcome {
                success: true,
                status: Some(status),
                error: None,
                error_kind: None,
            }),
            Err(e) => Ok(JobStatusOutcome {
                success: false,
                status: None,
                error: Some(e.to_string()),
                error_kind: Some(fine_tuning_error_kind(&e).to_string()),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::genome::test_support::registry_with;
    use crate::genome::fine_tuning::JobHandle;
    use crate::sdk_codegen::{ActionCommand, Ctx};
    use uuid::Uuid;

    // what this catches: name/access wiring — polling a training job is on the
    // Privileged surface alongside create/cancel, not the AiSafe read surface.
    #[test]
    fn name_and_access_wired() {
        assert_eq!(GenomeJobStatus::NAME, "genome/job-status");
        assert!(matches!(
            GenomeJobStatus::ACCESS,
            crate::sdk_codegen::AccessLevel::Privileged
        ));
    }

    // what this catches: error kind taxonomy on the wire. A caller branches on
    // errorKind to decide retry behavior; an unknown provider must yield the stable
    // "UnknownHandle" slug, not free-form text.
    #[tokio::test]
    async fn unknown_provider_returns_unknown_handle_slug() {
        let cmd = GenomeJobStatus {
            registry: registry_with(&["openai"]),
        };
        let out = cmd
            .run(
                &Ctx::default(),
                JobLookupParams {
                    handle: JobHandle {
                        provider_id: "no-such-adapter".into(),
                        provider_job_id: "x".into(),
                        local_id: Uuid::nil(),
                    },
                },
            )
            .await
            .unwrap();
        assert!(!out.success);
        assert_eq!(out.error_kind.as_deref(), Some("UnknownHandle"));
    }

    // what this catches: a known provider routes to its adapter and returns the polled
    // status (the stub completes), proving lookup-by-providerId reaches the adapter.
    #[tokio::test]
    async fn known_provider_returns_status() {
        let cmd = GenomeJobStatus {
            registry: registry_with(&["openai"]),
        };
        let out = cmd
            .run(
                &Ctx::default(),
                JobLookupParams {
                    handle: JobHandle {
                        provider_id: "openai".into(),
                        provider_job_id: "openai-job-1".into(),
                        local_id: Uuid::nil(),
                    },
                },
            )
            .await
            .unwrap();
        assert!(out.success);
        assert!(matches!(out.status, Some(TrainingStatus::Completed { .. })));
    }
}
