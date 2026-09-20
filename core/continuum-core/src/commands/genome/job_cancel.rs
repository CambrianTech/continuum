//! `genome/job-cancel` — look the adapter up by `handle.providerId`, cancel.

use std::sync::Arc;

use serde::Serialize;
use ts_rs::TS;

use crate::genome::fine_tuning::FineTuningRegistry;

use super::{fine_tuning_error_kind, JobLookupParams};

/// Outcome envelope for `genome/job-cancel`. `success=true` carries nothing else;
/// `success=false` carries `error` + an `errorKind` slug (`UnknownHandle` when no
/// adapter owns the handle's provider, else the adapter's error kind).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/JobCancelOutcome.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct JobCancelOutcome {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error_kind: Option<String>,
}

crate::action_command! {
    /// Cancel a fine-tuning job by its handle. The adapter is looked back up from
    /// `handle.providerId` and asked to cancel. An unknown provider yields
    /// `success=false` with `errorKind="UnknownHandle"`; an adapter error yields its
    /// own `errorKind` slug. Routes purely by the handle — no module-side cached
    /// selection.
    pub struct GenomeJobCancel { registry: Arc<FineTuningRegistry> }
    name: "genome/job-cancel",
    access: Privileged,
    params: JobLookupParams,
    output: JobCancelOutcome,
    run(this, _ctx, p) => {
        let adapter = match this.registry.get(&p.handle.provider_id) {
            Some(a) => a,
            None => {
                return Ok(JobCancelOutcome {
                    success: false,
                    error: Some(format!(
                        "no adapter registered for provider {:?}",
                        p.handle.provider_id
                    )),
                    error_kind: Some("UnknownHandle".to_string()),
                });
            }
        };

        match adapter.cancel(&p.handle).await {
            Ok(()) => Ok(JobCancelOutcome {
                success: true,
                error: None,
                error_kind: None,
            }),
            Err(e) => Ok(JobCancelOutcome {
                success: false,
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

    // what this catches: name/access wiring — cancelling a training job is on the
    // Privileged surface, not AiSafe.
    #[test]
    fn name_and_access_wired() {
        assert_eq!(GenomeJobCancel::NAME, "genome/job-cancel");
        assert!(matches!(
            GenomeJobCancel::ACCESS,
            crate::sdk_codegen::AccessLevel::Privileged
        ));
    }

    // what this catches: cancel routes by handle.providerId to the matching adapter;
    // the stub returns Ok(()) and the outcome is bare success=true (no extra fields).
    #[tokio::test]
    async fn cancel_routes_by_handle_provider_id() {
        let cmd = GenomeJobCancel {
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
        assert!(out.error.is_none());
    }

    // what this catches: an unknown provider yields the stable UnknownHandle slug
    // rather than a transport Err — the outcome-as-data contract for cancel.
    #[tokio::test]
    async fn unknown_provider_returns_unknown_handle_slug() {
        let cmd = GenomeJobCancel {
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
}
