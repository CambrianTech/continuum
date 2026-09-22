//! `genome/job-status` -- live adapter status or explicitly attributed terminal history.

use super::fine_tuning_error_kind;
use crate::genome::fine_tuning::job_board::JournalLookup;
use crate::genome::fine_tuning::{
    FineTuningError, FineTuningRegistry, JobHandle, TrainingJobBoard, TrainingStatus,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/JobStatusParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct JobStatusParams {
    /// Handle returned by job-create; distinct from the kernel resource handle.
    pub job_handle: JobHandle,
    /// Continue an incomplete historical scan at the returned byte offset.
    #[serde(default)]
    #[ts(optional)]
    pub history_offset: Option<u64>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/JobStatusSource.ts"
)]
#[serde(rename_all = "snake_case")]
pub enum JobStatusSource {
    Adapter,
    Journal,
}

/// A journal result is historical evidence, not a resumed provider job.
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
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub source: Option<JobStatusSource>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub next_history_offset: Option<u64>,
}

impl JobStatusOutcome {
    fn observed(status: TrainingStatus, source: JobStatusSource) -> Self {
        Self {
            success: true,
            status: Some(status),
            source: Some(source),
            error: None,
            error_kind: None,
            next_history_offset: None,
        }
    }
    fn refused(kind: &str, error: String) -> Self {
        Self {
            success: false,
            status: None,
            source: None,
            error: Some(error),
            error_kind: Some(kind.into()),
            next_history_offset: None,
        }
    }
}

crate::action_command! {
    /// Read jobHandle status. Only an unknown handle/provider consults terminal
    /// history; transient provider failures stay failures. HistoryIncomplete returns
    /// nextHistoryOffset for another bounded read. Journal status does not resume work.
    pub struct GenomeJobStatus {
        registry: Arc<FineTuningRegistry>,
        #[cfg(test)] test_job_board: Arc<TrainingJobBoard>,
    }
    name: "genome/job-status",
    access: Privileged,
    params: JobStatusParams,
    output: JobStatusOutcome,
    run(this, _ctx, p) => {
        if let Some(adapter) = this.registry.get(&p.job_handle.provider_id) {
            match adapter.poll(&p.job_handle).await {
                Ok(status) => return Ok(JobStatusOutcome::observed(status, JobStatusSource::Adapter)),
                Err(FineTuningError::UnknownHandle(_)) => {},
                Err(e) => return Ok(JobStatusOutcome::refused(fine_tuning_error_kind(&e), e.to_string())),
            }
        }
        let history = TrainingJobBoard::terminal_history(
            p.job_handle, p.history_offset.unwrap_or(0), // JUSTIFIED unwrap_or: absent continuation starts at the journal's first byte.
            #[cfg(test)] this.test_job_board.clone(),
        ).await;
        Ok(match history {
            Ok(JournalLookup::Observed(status)) => JobStatusOutcome::observed(status, JobStatusSource::Journal),
            Ok(JournalLookup::NotObserved) => JobStatusOutcome::refused("UnknownHandle", "no typed terminal receipt for this full handle".into()),
            Ok(JournalLookup::Incomplete { next_offset }) => {
                let mut result = JobStatusOutcome::refused("HistoryIncomplete", "history scan incomplete; continue with nextHistoryOffset".into());
                result.source = Some(JobStatusSource::Journal);
                result.next_history_offset = Some(next_offset);
                result
            },
            Err(error) => JobStatusOutcome::refused("HistoryUnavailable", error),
        })
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
        let mut cmd = GenomeJobStatus {
            registry: registry_with(&["openai"]),
            test_job_board: Arc::new(TrainingJobBoard::default()),
        };
        let out = cmd
            .run(
                &Ctx::default(),
                JobStatusParams {
                    history_offset: None,
                    job_handle: JobHandle {
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
        // The same public request after a restart can recover an exact terminal
        // receipt even when its provider is no longer registered.
        let directory = tempfile::tempdir().unwrap();
        let ledger = directory.path().join("jobs.jsonl");
        let handle = JobHandle {
            provider_id: "no-such-adapter".into(),
            provider_job_id: "x".into(),
            local_id: Uuid::nil(),
        };
        std::fs::write(
            &ledger,
            format!(
                "{}\n",
                serde_json::json!({
                    "event": "terminal", "local_id": handle.local_id, "handle": handle,
                    "status": {"state": "failed", "error": "capacity refused before weights"}
                })
            ),
        )
        .unwrap();
        cmd.test_job_board = Arc::new(TrainingJobBoard::with_ledger(Some(ledger)));
        let request = crate::runtime::CommandRequest::<JobStatusParams>::from_value(
            serde_json::json!({"jobHandle": handle}),
        )
        .unwrap();
        let out = cmd.run(&Ctx::default(), request.params).await.unwrap();
        assert!(out.success);
        assert!(matches!(out.source, Some(JobStatusSource::Journal)));
        assert!(matches!(out.status, Some(TrainingStatus::Failed { error })
            if error == "capacity refused before weights"));
    }

    // what this catches: a known provider routes to its adapter and returns the polled
    // status (the stub completes), proving lookup-by-providerId reaches the adapter.
    #[tokio::test]
    async fn known_provider_returns_status() {
        let cmd = GenomeJobStatus {
            registry: registry_with(&["openai"]),
            test_job_board: Arc::new(TrainingJobBoard::default()),
        };
        // Regression: the common envelope owns `handle`; a provider JobHandle
        // must survive that real boundary under its distinct `jobHandle` field.
        let request = crate::runtime::CommandRequest::<JobStatusParams>::from_value(
            serde_json::json!({"jobHandle": {
                "providerId": "openai", "providerJobId": "openai-job-1",
                "localId": Uuid::nil()
            }}),
        )
        .unwrap();
        assert!(request.handle.is_none());
        let out = cmd.run(&Ctx::default(), request.params).await.unwrap();
        assert!(out.success);
        assert!(matches!(out.status, Some(TrainingStatus::Completed { .. })));
    }
}
