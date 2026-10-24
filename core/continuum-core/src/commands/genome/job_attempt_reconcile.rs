//! `genome/job-attempt-reconcile`: what a training attempt provably did (card 55d4b9e3).
//!
//! The training trigger asks this when an interrupted dispatch has no job on the board. The
//! attempt ledger owns the evidence because it is written before the trainer runs; with no row
//! it CLOSES the attempt, so a delayed create can no longer start it, before answering.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::attempt_ledger::{AttemptLedger, Reconciled};
use crate::genome::fine_tuning::JobHandle;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/JobAttemptReconcileParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct JobAttemptReconcileParams {
    /// The attempt the trigger minted before calling `genome/job-create`.
    #[ts(type = "string")]
    pub attempt: uuid::Uuid,
}

/// The proven outcome. `notCreated` means the attempt is now closed: nothing exists and
/// nothing can start under it. `unknown` names why the evidence does not decide.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/JobAttemptReconcileOutcome.ts"
)]
#[serde(tag = "outcome", rename_all = "camelCase")]
pub enum JobAttemptReconcileOutcome {
    Created {
        #[serde(rename = "jobHandle")]
        handle: JobHandle,
    },
    NotCreated,
    Unknown {
        reason: String,
    },
}

crate::action_command! {
    /// Prove what a training attempt did: `created` (with its job), `notCreated` (closed,
    /// so nothing can start under it), or `unknown` with the reason. Reads the attempt
    /// ledger `genome/job-create` writes before the trainer runs.
    pub struct GenomeJobAttemptReconcile { attempts: Arc<AttemptLedger> }
    name: "genome/job-attempt-reconcile",
    access: Privileged,
    params: JobAttemptReconcileParams,
    output: JobAttemptReconcileOutcome,
    run(this, _ctx, p) => {
        Ok(match this.attempts.reconcile(p.attempt).await {
            Reconciled::Created(handle) => JobAttemptReconcileOutcome::Created { handle },
            Reconciled::NotCreated => JobAttemptReconcileOutcome::NotCreated,
            Reconciled::Unknown(reason) => JobAttemptReconcileOutcome::Unknown { reason },
        })
    }
}
