//! `genome/lease/train` and `genome/lease/status`: a grid peer trains a LoRA job another node
//! already decided on (card cc34db0d, S2a of 6f05b89d).
//!
//! The origin owns every training decision: competence, reuse, surprise and trials all run in
//! its own `genome/job-create`. A lease is execute-only: it trains exactly the payload it is
//! given, on a trainer on THIS host ([`FineTuningCoordinator::select_here`]), so a lease is
//! never passed on to a third node or a cloud. It is admitted in the same attempt ledger
//! `genome/job-create` writes before its trainer runs, keyed by the AUTHENTICATED origin and
//! its attempt ([`lease_key`]), so:
//! - a repeat with the same attempt and payload returns the job it already made;
//! - the same attempt with another payload is refused (the digest is computed here);
//! - a crash after the trainer started and before its handle was recorded reads Unknown;
//! - a status query for an attempt never received closes it first, so a create arriving
//!   late is refused, and "not created" stays true.
//!
//! The job is not registered on this node's training board: adoption (the sentinel, the gene
//! trial) belongs to the origin, which pulls the artifact back (S3).

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ts_rs::TS;

use super::attempt_ledger::{lease_key, AdmitError, Admission, AttemptLedger, AttemptState, LeaseRefusal};
use super::job_attempt_reconcile::JobAttemptReconcileOutcome;
use crate::genome::fine_tuning::{coordinator::FineTuningCoordinator, JobHandle, TrainingJobRequest};
use crate::routing::{CallerIdentity, CallerSource};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[ts(export, export_to = "../../../protocol/typescript/genome/LeaseTrainParams.ts")]
#[serde(rename_all = "camelCase")]
pub struct LeaseTrainParams {
    /// The origin's attempt id, minted by its training trigger.
    #[ts(type = "string")]
    pub attempt: uuid::Uuid,
    /// The origin's dispatch id, kept for its reconciliation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string")]
    pub dispatch: Option<uuid::Uuid>,
    /// Exactly what to train: the origin already decided to.
    pub request: TrainingJobRequest,
}

/// What a lease did. `created` carries the job; `refused` started nothing; `unknown` may
/// have started a trainer and is never retried as if it had not.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/genome/LeaseTrainOutcome.ts")]
#[serde(tag = "outcome", rename_all = "camelCase")]
pub enum LeaseTrainOutcome {
    Created {
        #[serde(rename = "jobHandle")]
        handle: JobHandle,
    },
    Refused {
        #[serde(rename = "errorKind")]
        error_kind: String,
        error: String,
    },
    Unknown {
        reason: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[ts(export, export_to = "../../../protocol/typescript/genome/LeaseStatusParams.ts")]
#[serde(rename_all = "camelCase")]
pub struct LeaseStatusParams {
    /// The origin's attempt id; it is read under the caller's own lease key.
    #[ts(type = "string")]
    pub attempt: uuid::Uuid,
}

/// The origin of a lease: the authenticated grid peer that sent it, never a parameter. A
/// local caller (a resident, this node's own code) has no lease to give.
fn lease_origin(caller: Option<&CallerIdentity>) -> Result<uuid::Uuid, String> {
    match caller {
        Some(caller) if matches!(caller.source, CallerSource::Airc) => Ok(caller.peer_id.as_uuid()),
        _ => Err("a lease comes from a grid peer over airc: its origin is the authenticated sender".into()),
    }
}

/// The payload's digest, computed here from the canonical request (base, method, dataset,
/// config), never taken from the caller.
fn request_digest(request: &TrainingJobRequest) -> Result<String, String> {
    serde_json::to_vec(request) // lease wire boundary: the digest covers the payload as it crossed the grid
        .map(|bytes| format!("{:x}", Sha256::digest(bytes)))
        .map_err(|e| format!("the request could not be encoded: {e}"))
}

fn refused(error_kind: &str, error: impl Into<String>) -> LeaseTrainOutcome {
    LeaseTrainOutcome::Refused { error_kind: error_kind.into(), error: error.into() }
}

crate::action_command! {
    /// Train a LoRA job a grid peer already decided on, on a trainer on this host (never
    /// passed on). Keyed by the authenticated peer and its attempt: a repeat returns the same
    /// job, another payload under the same attempt is refused. Grid peers only.
    pub struct GenomeLeaseTrain {
        coordinator: Arc<FineTuningCoordinator>,
        attempts: Arc<AttemptLedger>,
    }
    name: "genome/lease/train",
    access: Privileged,
    params: LeaseTrainParams,
    output: LeaseTrainOutcome,
    run(this, ctx, p) => {
        let origin = match lease_origin(ctx.caller.as_ref()) {
            Ok(origin) => origin,
            Err(e) => return Ok(refused("not_a_lease", e)),
        };
        // LoRA only: the advertisement promises nothing else, and nothing else is trained.
        if p.request.lora.is_none() {
            return Ok(refused("not_lora", "a lease trains a LoRA adapter; the request names no LoRA"));
        }
        let digest = match request_digest(&p.request) {
            Ok(digest) => digest,
            Err(e) => return Ok(refused("invalid_request", e)),
        };
        let key = lease_key(origin, p.attempt);
        // One create per leased attempt at a time: a concurrent repeat waits, then reads it.
        let _run = this.attempts.run(key).await;
        let (provider, adapter) = match this.coordinator.select_here(&p.request) {
            Ok(selected) => selected,
            Err(e) => {
                // A repeat of a job already made answers with it even if the trainer is busy now.
                if let Ok(Some(Admission::AlreadyCreated(handle))) = this.attempts.peek(key).await {
                    return Ok(LeaseTrainOutcome::Created { handle });
                }
                return Ok(refused("no_trainer_here", e.to_string()));
            }
        };
        match this.attempts.admit_lease(key, origin, &digest, &provider).await {
            Ok(Admission::Fresh) => {}
            Ok(Admission::AlreadyCreated(handle)) => return Ok(LeaseTrainOutcome::Created { handle }),
            Ok(Admission::Fenced) => return Ok(refused("attempt_fenced", format!("attempt {} was closed or refused here; nothing started", p.attempt))),
            Ok(Admission::Unsettled) => return Ok(LeaseTrainOutcome::Unknown { reason: format!("attempt {} was admitted here and never settled: a trainer may have started", p.attempt) }),
            Err(LeaseRefusal::PayloadChanged) => return Ok(refused("payload_changed", format!("attempt {} was leased here with another payload", p.attempt))),
            Err(LeaseRefusal::Ledger(AdmitError::Unreadable(e))) => return Ok(LeaseTrainOutcome::Unknown { reason: format!("attempt record unreadable: {e}") }),
            Err(LeaseRefusal::Ledger(AdmitError::Unrecorded(e))) => return Ok(refused("attempt_unrecorded", format!("could not be admitted, nothing started: {e}"))),
        }
        continuum_base::probe!(
            class = "genome.lease.admitted",
            origin = %origin,
            attempt = %p.attempt,
            provider = %provider,
            "a grid peer's LoRA job is admitted here; its trainer starts"
        );
        match adapter.create_job(p.request).await {
            Ok(handle) => {
                if let Err(error) = this.attempts.settle(key, AttemptState::Created { handle: handle.clone() }).await {
                    continuum_base::probe!(
                        class = "genome.lease.unsettled",
                        attempt = %p.attempt,
                        error = %error,
                        "the leased job started but the ledger could not record it: it reads unknown"
                    );
                }
                Ok(LeaseTrainOutcome::Created { handle })
            }
            Err(e) if super::job_create::settles_as_not_created(&e) => {
                let kind = super::fine_tuning_error_kind(&e);
                if let Err(error) = this.attempts.settle(key, AttemptState::Failed { kind: kind.to_string() }).await {
                    continuum_base::probe!(
                        class = "genome.lease.unsettled",
                        attempt = %p.attempt,
                        error = %error,
                        "the trainer refused but the ledger could not record it: it reads unknown"
                    );
                }
                Ok(refused(kind, e.to_string()))
            }
            // Anything else may follow a trainer that started: left admitted, read as unknown.
            Err(e) => Ok(LeaseTrainOutcome::Unknown { reason: e.to_string() }),
        }
    }
}

crate::action_command! {
    /// What a leased attempt did here, asked by the peer that leased it: `created` with its
    /// job, `notCreated` (closed, so nothing can start under it), or `unknown` with the
    /// reason. Read under the caller's own lease key, so a peer sees only its own attempts.
    pub struct GenomeLeaseStatus { attempts: Arc<AttemptLedger> }
    name: "genome/lease/status",
    access: Privileged,
    params: LeaseStatusParams,
    output: JobAttemptReconcileOutcome,
    run(this, ctx, p) => {
        let origin = match lease_origin(ctx.caller.as_ref()) {
            Ok(origin) => origin,
            Err(reason) => return Ok(JobAttemptReconcileOutcome::Unknown { reason }),
        };
        Ok(this.attempts.reconcile(lease_key(origin, p.attempt)).await.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (card cc34db0d): a lease taking its origin from anything but the
    // authenticated airc sender. A local caller (a resident, this node's code) is refused, and
    // the origin is the sender's peer id, so a forged id names nobody else's record.
    #[test]
    fn a_lease_origin_is_the_authenticated_grid_sender() {
        let id = uuid::Uuid::new_v4();
        let peer = || crate::identity::PeerId::from_uuid(id);
        assert_eq!(lease_origin(Some(&CallerIdentity::airc(peer()))), Ok(id));
        assert!(lease_origin(Some(&CallerIdentity::local_persona(peer()))).is_err(), "a resident has no lease to give");
        assert!(lease_origin(None).is_err(), "this node's own code has no lease to give");
    }
}
