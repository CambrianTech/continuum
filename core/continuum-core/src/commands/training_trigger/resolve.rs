//! `genome/training-trigger/resolve`: settle a dispatch intent that recovery cannot,
//! from evidence an operator names, journaled. Recovery confirms a Dispatching or
//! RecoveryRequired intent only by a job registered under it; with no such job it
//! refuses forever ("no confirmed result; it was not repeated"), which is right, and
//! which held 113 of Kimi's examples on the 5090 (card d24e3f25, 2026-10-10: dispatch
//! 0512a303 never registered a job before the core was replaced). This verb is the
//! operator's half of that contract, by a NAMED claim: `neverDispatched` (no job was
//! created) or `interruptedUntrained` (a job may have existed, but it ended and nothing it
//! trained survives) make the intent retryable so the next tick re-creates the job from the
//! same batch; `dispatched` adopts a job the operator names. Each re-checks the ledger first
//! and refuses evidence that contradicts it, and each is journaled under its own claim.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::modules::training_trigger::{DispatchResolution, ResolveReport, RetryClaim, TrainingTriggerState};
use crate::sdk_codegen::CommandError;

#[derive(Debug, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/training_trigger/ResolveParams.ts")]
pub struct ResolveParams {
    /// The dispatch intent to settle (the id recovery names in its refusal).
    #[ts(type = "string")]
    pub dispatch_id: Uuid,
    /// What settles it, and what the operator asserts by it.
    pub resolution: ResolveAs,
    /// With `dispatched`: the job the intent DID create (its registration carries no
    /// dispatch, or this one); the intent finishes as dispatched and the job takes the
    /// normal path. Refused with the other resolutions.
    #[serde(default)]
    #[ts(optional, type = "string")]
    pub job_id: Option<Uuid>,
    /// Why, in the operator's words; journaled on the intent.
    pub reason: String,
}

/// How an unconfirmed training dispatch is settled, and what the operator asserts by it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/training_trigger/ResolveAs.ts")]
pub enum ResolveAs {
    // No job was ever created under it; refused if the ledger registers one. Retryable.
    NeverDispatched,
    // A job may have existed, but it ended and nothing it trained survives; refused for a
    // registered job that has not ended or that completed training. Retryable.
    InterruptedUntrained,
    // It made the job named by `jobId`, which is adopted.
    Dispatched,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/training_trigger/ResolveOutcome.ts")]
pub struct ResolveOutcome {
    pub success: bool,
    #[ts(type = "string")]
    pub dispatch_id: Uuid,
    /// `retryable` (the next tick re-creates the job) or `dispatched` (the named job
    /// was adopted). Absent on refusal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub resolution: Option<String>,
    /// Examples the intent carries, back on their way to a job.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub examples: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string")]
    pub job_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<String>,
}

impl ResolveOutcome {
    fn refused(dispatch_id: Uuid, kind: &str, error: String) -> Self {
        Self {
            success: false,
            dispatch_id,
            resolution: None,
            examples: None,
            job_id: None,
            error_kind: Some(kind.into()),
            error: Some(error),
        }
    }
}

impl From<ResolveReport> for ResolveOutcome {
    fn from(report: ResolveReport) -> Self {
        Self {
            success: true,
            dispatch_id: report.dispatch_id,
            resolution: Some(report.resolution.into()),
            examples: Some(report.examples),
            job_id: report.job_id,
            error_kind: None,
            error: None,
        }
    }
}

crate::action_command! {
    /// Settle a dispatch intent recovery cannot confirm, from named evidence: `resolution`
    /// `neverDispatched` (no job was created) or `interruptedUntrained` (it ended untrained)
    /// make it retryable, re-created from the same batch next tick; `dispatched` with `jobId`
    /// adopts that job. Each re-checks the ledger and refuses evidence it contradicts; the
    /// claim and the reason are journaled.
    pub struct TrainingTriggerResolve {
        state: Arc<TrainingTriggerState>,
    }
    name: "genome/training-trigger/resolve",
    access: Privileged,
    params: ResolveParams,
    output: ResolveOutcome,
    run(this, ctx, p) => {
        let resolved_by = ctx.caller.as_ref().map(|caller| caller.peer_id.as_uuid());
        resolve(&this.state, p, resolved_by).await
    }
}

pub(crate) async fn resolve(
    state: &Arc<TrainingTriggerState>,
    p: ResolveParams,
    resolved_by: Option<Uuid>,
) -> Result<ResolveOutcome, CommandError> {
    let reason = p.reason.trim();
    if reason.is_empty() {
        return Err(CommandError::Invalid(
            "a resolution carries its reason: say what evidence settles this dispatch".into(),
        ));
    }
    let resolution = match (p.resolution, p.job_id) {
        (ResolveAs::Dispatched, Some(job)) => DispatchResolution::Dispatched { job },
        (ResolveAs::Dispatched, None) => {
            return Err(CommandError::Invalid("dispatched names the job it made: give jobId".into()))
        }
        (_, Some(_)) => {
            return Err(CommandError::Invalid(
                "jobId goes only with dispatched: a retry claims no job of its own".into(),
            ))
        }
        (ResolveAs::NeverDispatched, None) => DispatchResolution::Retry(RetryClaim::NeverDispatched),
        (ResolveAs::InterruptedUntrained, None) => DispatchResolution::Retry(RetryClaim::InterruptedUntrained),
    };
    match state
        .resolve_dispatch(p.dispatch_id, resolution, reason.to_string(), resolved_by)
        .await
    {
        Ok(report) => Ok(report.into()),
        Err((kind, error)) => Ok(ResolveOutcome::refused(p.dispatch_id, kind, error)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a resolution without evidence or with contradictory evidence is
    // refused before anything is read; the intent's phase is never touched on a bad ask.
    #[tokio::test]
    async fn an_empty_reason_or_an_ambiguous_resolution_is_refused_up_front() {
        let (trigger, _executor, _dir) =
            crate::commands::training_trigger::test_support::build_runtime_trigger_only().await;
        for (resolution, job_id, reason) in [
            (ResolveAs::NeverDispatched, None, "  "),
            (ResolveAs::NeverDispatched, Some(Uuid::new_v4()), "x"),
            (ResolveAs::InterruptedUntrained, Some(Uuid::new_v4()), "x"),
            (ResolveAs::Dispatched, None, "x"),
        ] {
            let got = resolve(
                &trigger.state,
                ResolveParams { dispatch_id: Uuid::new_v4(), resolution, job_id, reason: reason.into() },
                None,
            )
            .await;
            assert!(matches!(got, Err(CommandError::Invalid(_))), "{got:?}");
        }
    }

    // what this catches: a dispatch nobody has heard of is a refusal with a name, not a
    // silent success and not a panic.
    #[tokio::test]
    async fn an_unknown_dispatch_is_refused_by_name() {
        let (trigger, _executor, _dir) =
            crate::commands::training_trigger::test_support::build_runtime_trigger_only().await;
        let id = Uuid::new_v4();
        let got = resolve(
            &trigger.state,
            ResolveParams { dispatch_id: id, resolution: ResolveAs::NeverDispatched, job_id: None, reason: "never made a job".into() },
            None,
        )
        .await
        .expect("test: a refusal is an outcome");
        assert_eq!(got.success, false, "{got:?}");
        assert_eq!(got.error_kind.as_deref(), Some("NotFound"), "{got:?}");
        assert_eq!(got.dispatch_id, id);
    }
}
