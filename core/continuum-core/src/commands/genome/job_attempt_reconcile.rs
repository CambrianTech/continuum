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
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
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

impl From<Reconciled> for JobAttemptReconcileOutcome {
    fn from(reconciled: Reconciled) -> Self {
        match reconciled {
            Reconciled::Created(handle) => Self::Created { handle },
            Reconciled::NotCreated => Self::NotCreated,
            Reconciled::Unknown(reason) => Self::Unknown { reason },
        }
    }
}

crate::action_command! {
    /// Prove what a training attempt did: `created` (with its job), `notCreated` (closed,
    /// so nothing can start under it), or `unknown` with the reason. Reads the attempt
    /// ledger `genome/job-create` writes before the trainer runs.
    pub struct GenomeJobAttemptReconcile {
        attempts: Arc<AttemptLedger>,
        transport: Arc<dyn super::grid_peer_adapter::LeaseTransport>,
    }
    name: "genome/job-attempt-reconcile",
    access: Privileged,
    params: JobAttemptReconcileParams,
    output: JobAttemptReconcileOutcome,
    run(this, _ctx, p) => {
        let local = this.attempts.reconcile(p.attempt).await;
        // A lease's outcome lives on its peer (card e53bb821): an attempt this ledger cannot
        // decide, recorded to run on a peer, is asked of that peer, never guessed here.
        if let Reconciled::Unknown(_) = &local {
            if let Ok(Some(stored)) = this.attempts.destination_of(p.attempt).await {
                if let Some(crate::genome::fine_tuning::Destination::Peer(peer)) =
                    crate::genome::fine_tuning::Destination::from_ledger(&stored)
                {
                    return Ok(peer_reconcile(this.transport.as_ref(), &this.attempts, peer, p.attempt).await);
                }
            }
        }
        Ok(local.into())
    }
}

/// What a leased attempt did, asked of its peer under this node's own lease key there. A peer
/// that made the job settles it Created here; one that never received it (and has now closed
/// it, so a late create is fenced) settles it refused here too; anything else stays Unknown.
async fn peer_reconcile(
    transport: &dyn super::grid_peer_adapter::LeaseTransport,
    attempts: &AttemptLedger,
    peer: uuid::Uuid,
    attempt: uuid::Uuid,
) -> JobAttemptReconcileOutcome {
    use super::attempt_ledger::AttemptState;
    let asked = match serde_json::to_value(super::lease::LeaseStatusParams { attempt }) { // lease wire boundary: the status ask crosses the grid as JSON
        Ok(params) => transport.call(peer, "genome/lease/status", params).await,
        Err(e) => Err(e.to_string()),
    };
    let answer = asked.and_then(|value| {
        serde_json::from_value::<JobAttemptReconcileOutcome>(value).map_err(|e| e.to_string()) // lease wire boundary: the peer's reply arrives as JSON over airc
    });
    match answer {
        Ok(JobAttemptReconcileOutcome::Created { .. }) => {
            let handle = super::grid_peer_adapter::lease_handle(peer, attempt);
            if let Err(error) = attempts.settle(attempt, AttemptState::Created { handle: handle.clone() }).await {
                continuum_base::probe!(class = "genome.lease.reconcile_unsettled", attempt = %attempt, error = %error,
                    "the peer made the leased job but this ledger could not record it");
            }
            JobAttemptReconcileOutcome::Created { handle }
        }
        Ok(JobAttemptReconcileOutcome::NotCreated) => {
            if let Err(error) = attempts.settle(attempt, AttemptState::Failed { kind: "peer_not_created".into() }).await {
                continuum_base::probe!(class = "genome.lease.reconcile_unsettled", attempt = %attempt, error = %error,
                    "the peer never made the leased job but this ledger could not record it");
            }
            JobAttemptReconcileOutcome::NotCreated
        }
        Ok(JobAttemptReconcileOutcome::Unknown { reason }) => JobAttemptReconcileOutcome::Unknown { reason: format!("peer {peer}: {reason}") },
        Err(error) => JobAttemptReconcileOutcome::Unknown { reason: format!("peer {peer} could not be asked: {error}") },
    }
}

