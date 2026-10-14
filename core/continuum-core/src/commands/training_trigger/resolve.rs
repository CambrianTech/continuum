//! `genome/training-trigger/resolve`: settle a dispatch intent that recovery cannot,
//! from evidence an operator names, journaled. Recovery confirms a Dispatching or
//! RecoveryRequired intent only by a job registered under it; with no such job it
//! refuses forever ("no confirmed result; it was not repeated"), which is right, and
//! which held 113 of Kimi's examples on the 5090 (card d24e3f25, 2026-10-10: dispatch
//! 0512a303 never registered a job before the core was replaced). This verb is the
//! operator's half of that contract: `not dispatched` makes the intent retryable so the
//! next tick re-creates the job from the same batch; `retry unconfirmed` does the same for an
//! intent whose outcome can't be confirmed either way, as an AUDITED decision with the evidence
//! checked about retained results journaled beside it, asserting nothing about whether a job
//! existed (Codex root on Sahar's 2b4e7272); `dispatched` adopts a job the operator names. All
//! re-check the ledger first and refuse evidence that contradicts it.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::modules::training_trigger::{DispatchResolution, ResolveReport, TrainingTriggerState};
use crate::sdk_codegen::CommandError;

#[derive(Debug, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/training_trigger/ResolveParams.ts")]
pub struct ResolveParams {
    /// The dispatch intent to settle (the id recovery names in its refusal).
    #[ts(type = "string")]
    pub dispatch_id: Uuid,
    /// The intent never created a job: it becomes retryable and the next tick
    /// re-creates the job from the same batch. Refused if the ledger shows a job
    /// registered under this dispatch.
    #[serde(default)]
    pub not_dispatched: bool,
    /// The intent's outcome is UNCONFIRMED (no job registered under it, no evidence either way
    /// that one ran) and the operator retries it anyway: it becomes retryable like
    /// `notDispatched`, journaled as an operator decision with `evidence`, never as "no job was
    /// created". Refused when a job IS registered under it: resolve that as dispatched with
    /// `jobId`, then `genome/training-trigger/return` returns what it didn't train.
    #[serde(default)]
    pub retry_unconfirmed: bool,
    /// With `retryUnconfirmed`: what was checked about retained results, one line per check
    /// and its finding (adapters, artifacts, overlap with any job that ran). Kept on the
    /// decision's receipt, committed together with the intent it releases.
    #[serde(default)]
    pub evidence: Vec<String>,
    /// The intent DID create this job (its registration carries no dispatch, or this
    /// one): the intent finishes as dispatched and the job takes the normal path.
    #[serde(default)]
    #[ts(optional, type = "string")]
    pub job_id: Option<Uuid>,
    /// Why, in the operator's words; journaled on the intent.
    pub reason: String,
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
    /// Settle a dispatch intent recovery cannot confirm, from named evidence: `notDispatched`
    /// (no job was ever registered under it; it becomes retryable and the next tick re-creates
    /// the job from the same batch), `retryUnconfirmed` with `evidence` (its outcome can't be
    /// confirmed; retried as an audited operator decision, asserting no job either way), or
    /// `jobId` (that job is adopted as the intent's). Each re-checks the ledger and refuses
    /// evidence it contradicts. The reason is journaled.
    pub struct TrainingTriggerResolve {
        state: Arc<TrainingTriggerState>,
    }
    name: "genome/training-trigger/resolve",
    access: Privileged,
    params: ResolveParams,
    output: ResolveOutcome,
    run(this, ctx, p) => {
        // who decided, through the one resolver for who is acting: a CLI decision names the
        // operator (or the agent self-peer), never NULL; one it can't name is refused (Codex
        // root on Sahar's 2b4e7272, whose receipt recorded NULL)
        let resolved_by = crate::persona::operator_peer::acting_now(ctx, "genome/training-trigger/resolve")?.peer;
        resolve(&this.state, p, Some(resolved_by)).await
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
    let resolution = match (p.not_dispatched, p.retry_unconfirmed, p.job_id) {
        (true, false, None) => DispatchResolution::NotDispatched,
        (false, true, None) => DispatchResolution::RetryUnconfirmed { evidence: p.evidence },
        (false, false, Some(job)) => DispatchResolution::Dispatched { job },
        (false, false, None) => {
            return Err(CommandError::Invalid(
                "nothing settled: give notDispatched, retryUnconfirmed (with evidence), or the jobId the dispatch made".into(),
            ))
        }
        _ => {
            return Err(CommandError::Invalid(
                "one disposition per resolution: notDispatched, retryUnconfirmed, or jobId".into(),
            ))
        }
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
        for (not_dispatched, retry_unconfirmed, job_id, reason) in [
            (true, false, None, "  "),
            (true, false, Some(Uuid::new_v4()), "x"),
            (true, true, None, "x"),
            (false, true, Some(Uuid::new_v4()), "x"),
            (false, false, None, "x"),
        ] {
            let got = resolve(
                &trigger.state,
                ResolveParams { dispatch_id: Uuid::new_v4(), not_dispatched, retry_unconfirmed, evidence: Vec::new(), job_id, reason: reason.into() },
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
            ResolveParams { dispatch_id: id, not_dispatched: true, retry_unconfirmed: false, evidence: Vec::new(), job_id: None, reason: "never made a job".into() },
            None,
        )
        .await
        .expect("test: a refusal is an outcome");
        assert_eq!(got.success, false, "{got:?}");
        assert_eq!(got.error_kind.as_deref(), Some("NotFound"), "{got:?}");
        assert_eq!(got.dispatch_id, id);
    }
}
