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
//! trial) belongs to the origin, which pulls the artifact back (S3). A completed job is
//! reported by its artifact's recorded manifest ([`LeaseArtifact`]), never this node's path.

use std::sync::Arc;

use base64::Engine as _;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ts_rs::TS;

use super::attempt_ledger::{lease_key, AdmitError, Admission, AttemptLedger, AttemptState, LeaseRefusal, ReturnRecord};
use super::job_attempt_reconcile::JobAttemptReconcileOutcome;
use super::lease_artifact::{describe, DescribeError, LeaseArtifact};
use crate::genome::fine_tuning::{coordinator::FineTuningCoordinator, FineTuningRegistry, JobHandle, TrainingJobRequest, TrainingStatus};
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
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
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

/// Where a leased job stands, asked by the peer that leased it (card ec1d8b76): `status` from
/// its trainer, `completed` with its artifact's manifest, `unreturnable`, `notCreated` (closed,
/// so nothing can start under it), or `unknown`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/genome/LeasePollOutcome.ts")]
#[serde(tag = "outcome", rename_all = "camelCase")]
pub enum LeasePollOutcome {
    /// Not finished: queued, waiting, running, failed or cancelled.
    Status { status: TrainingStatus },
    /// Finished: the artifact by its recorded files and digests, never this node's path
    /// (card 3fd98325). Every later poll answers from the same record.
    Completed { artifact: LeaseArtifact },
    /// Finished with no local gene to return (a provider-hosted or skeleton artifact): final.
    Unreturnable { reason: String },
    NotCreated,
    Unknown { reason: String },
}

/// What cancelling a leased job did: `cancelled` (its trainer took the cancel), `notCreated`
/// (closed, so nothing will start), or `unknown` (a trainer may still be running it).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/genome/LeaseCancelOutcome.ts")]
#[serde(tag = "outcome", rename_all = "camelCase")]
pub enum LeaseCancelOutcome {
    Cancelled,
    NotCreated,
    Unknown { reason: String },
}

/// The most bytes one `genome/lease/artifact` answer carries. The narrowest airc route caps a
/// whole serialized frame at 16 KiB (WebRTC data channel, airc 0763991ac
/// `webrtc_datachannel/adapter.rs:15`, read by Codex 2026-10-25). Base64 makes 8 KiB about
/// 10.9 KiB, under 12 KiB with its JSON, which leaves 4 KiB for the airc envelope: the largest
/// envelope measured on this mesh was 706 bytes (32 #continuum-dev events, 2026-10-25T13:46Z,
/// airc 0.1.0). `a_full_chunk_fits_the_narrowest_route` pins the encoded size.
pub const MAX_ARTIFACT_CHUNK: u32 = 8 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[ts(export, export_to = "../../../protocol/typescript/genome/LeaseArtifactParams.ts")]
#[serde(rename_all = "camelCase")]
pub struct LeaseArtifactParams {
    /// The origin's attempt id; it is read under the caller's own lease key.
    #[ts(type = "string")]
    pub attempt: uuid::Uuid,
    /// The file's name in the artifact's recorded manifest, never a path.
    pub file: String,
    #[ts(type = "number")]
    pub offset: u64,
    /// Bytes asked for, at most [`MAX_ARTIFACT_CHUNK`].
    pub len: u32,
}

/// One range of a leased job's recorded artifact (card 92cd2251, S3-2): `bytes` (base64, fewer
/// than asked only at the file's end), `refused` (no recorded artifact for the caller's attempt,
/// no such file, a range past its end or over the bound, or the file no longer has its recorded
/// size: the artifact as reported is not here), or `unknown` (it may be readable later).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/genome/LeaseArtifactChunk.ts")]
#[serde(tag = "outcome", rename_all = "camelCase")]
pub enum LeaseArtifactChunk {
    Bytes { data: String },
    Refused { reason: String },
    Unknown { reason: String },
}

/// The leased attempt's job, read under the caller's OWN lease key: an attempt never received
/// is closed first (so a late create is fenced), and a forged origin derives another key.
async fn leased_job(attempts: &AttemptLedger, caller: Option<&CallerIdentity>, attempt: uuid::Uuid) -> Result<Option<(uuid::Uuid, JobHandle)>, String> {
    let key = lease_key(lease_origin(caller)?, attempt);
    match attempts.reconcile(key).await {
        super::attempt_ledger::Reconciled::Created(handle) => Ok(Some((key, handle))),
        super::attempt_ledger::Reconciled::NotCreated => Ok(None),
        super::attempt_ledger::Reconciled::Unknown(reason) => Err(reason),
    }
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

/// The answer an attempt's record already gives, or `None` when it was just admitted fresh
/// and its trainer should start. An admitted, never-settled record may have started one: it
/// is Unknown, never refused and never started again.
fn recorded_answer(admission: Admission, attempt: uuid::Uuid) -> Option<LeaseTrainOutcome> {
    match admission {
        Admission::Fresh => None,
        Admission::AlreadyCreated(handle) => Some(LeaseTrainOutcome::Created { handle }),
        Admission::Fenced => Some(refused("attempt_fenced", format!("attempt {attempt} was closed or refused here; nothing started"))),
        Admission::Unsettled => Some(LeaseTrainOutcome::Unknown { reason: format!("attempt {attempt} was admitted here and never settled: a trainer may have started") }),
    }
}

/// A completed job's artifact, described and recorded once: the first description recorded
/// is what every later poll reports and what serving the bytes is checked against. A file
/// unreadable now, or a record that could not be written, is Unknown (it may pass); an
/// artifact with no local gene is final.
async fn returned_artifact(
    attempts: &AttemptLedger,
    key: uuid::Uuid,
    base_model: Option<&str>,
    artifact: &crate::genome::fine_tuning::TrainingArtifact,
) -> LeasePollOutcome {
    // A lease admitted before its base was recorded cannot name the base it fits: final.
    let Some(base_model) = base_model else {
        return LeasePollOutcome::Unreturnable { reason: "the lease record names no base model to fingerprint".into() };
    };
    let described = match describe(artifact, base_model).await {
        Ok(described) => described,
        Err(DescribeError::Unreturnable(reason)) => return LeasePollOutcome::Unreturnable { reason },
        Err(DescribeError::Unread(reason)) => return LeasePollOutcome::Unknown { reason },
    };
    match attempts.record_return(key, described).await {
        Ok(recorded) => LeasePollOutcome::Completed { artifact: recorded.artifact },
        Err(reason) => LeasePollOutcome::Unknown { reason: format!("the artifact could not be recorded: {reason}") },
    }
}

/// A lease the ledger would not admit: a changed payload starts nothing, an unreadable record
/// may hold a started trainer (Unknown), an unwritable new record started nothing.
fn refusal_answer(refusal: LeaseRefusal, attempt: uuid::Uuid) -> LeaseTrainOutcome {
    match refusal {
        LeaseRefusal::PayloadChanged => refused("payload_changed", format!("attempt {attempt} was leased here with another payload")),
        LeaseRefusal::Ledger(AdmitError::Unreadable(e)) => LeaseTrainOutcome::Unknown { reason: format!("attempt record unreadable: {e}") },
        LeaseRefusal::Ledger(AdmitError::Unrecorded(e)) => refused("attempt_unrecorded", format!("could not be admitted, nothing started: {e}")),
    }
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
        // Binding first: an attempt this node already holds answers from its own record,
        // checked against this payload, before any trainer is chosen (Codex on #5409).
        match this.attempts.lease_record(key, &digest).await {
            Ok(None) => {}
            Ok(Some(existing)) => {
                if let Some(answer) = recorded_answer(existing, p.attempt) {
                    return Ok(answer);
                }
            }
            Err(refusal) => return Ok(refusal_answer(refusal, p.attempt)),
        }
        let (provider, adapter) = match this.coordinator.select_here(&p.request) {
            Ok(selected) => selected,
            Err(e) => return Ok(refused("no_trainer_here", e.to_string())),
        };
        match this.attempts.admit_lease(key, origin, &digest, &provider, &p.request.base_model).await {
            Ok(admission) => {
                if let Some(answer) = recorded_answer(admission, p.attempt) {
                    return Ok(answer);
                }
            }
            Err(refusal) => return Ok(refusal_answer(refusal, p.attempt)),
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

crate::action_command! {
    /// Where a leased job stands, asked by the peer that leased it: its trainer's status, its
    /// artifact's recorded manifest once completed, `unreturnable`, `notCreated`, or `unknown`.
    /// Read under the caller's own lease key only.
    pub struct GenomeLeasePoll {
        attempts: Arc<AttemptLedger>,
        registry: Arc<FineTuningRegistry>,
    }
    name: "genome/lease/poll",
    access: Privileged,
    params: LeaseStatusParams,
    output: LeasePollOutcome,
    run(this, ctx, p) => {
        let (key, handle) = match leased_job(&this.attempts, ctx.caller.as_ref(), p.attempt).await {
            Ok(Some(job)) => job,
            Ok(None) => return Ok(LeasePollOutcome::NotCreated),
            Err(reason) => return Ok(LeasePollOutcome::Unknown { reason }),
        };
        // A recorded artifact is the answer, whatever the trainer remembers now.
        let base_model = match this.attempts.return_record(key).await {
            Ok(Some(ReturnRecord { returned: Some(recorded), .. })) => return Ok(LeasePollOutcome::Completed { artifact: recorded.artifact }),
            Ok(Some(ReturnRecord { returned: None, base_model })) => base_model,
            Ok(None) => None,
            Err(reason) => return Ok(LeasePollOutcome::Unknown { reason }),
        };
        let Some(adapter) = this.registry.get(&handle.provider_id) else {
            return Ok(LeasePollOutcome::Unknown { reason: format!("trainer {} is not registered here", handle.provider_id) });
        };
        Ok(match adapter.poll(&handle).await {
            Ok(TrainingStatus::Completed { artifact }) => returned_artifact(&this.attempts, key, base_model.as_deref(), &artifact).await,
            Ok(status) => LeasePollOutcome::Status { status },
            Err(e) => LeasePollOutcome::Unknown { reason: e.to_string() },
        })
    }
}

crate::action_command! {
    /// Cancel a leased job, asked by the peer that leased it: `cancelled`, `notCreated`
    /// (closed, so nothing will start), or `unknown`. Only the caller's own lease key.
    pub struct GenomeLeaseCancel { attempts: Arc<AttemptLedger>, registry: Arc<FineTuningRegistry> }
    name: "genome/lease/cancel",
    access: Privileged,
    params: LeaseStatusParams,
    output: LeaseCancelOutcome,
    run(this, ctx, p) => {
        let (_, handle) = match leased_job(&this.attempts, ctx.caller.as_ref(), p.attempt).await {
            Ok(Some(job)) => job,
            Ok(None) => return Ok(LeaseCancelOutcome::NotCreated),
            Err(reason) => return Ok(LeaseCancelOutcome::Unknown { reason }),
        };
        let Some(adapter) = this.registry.get(&handle.provider_id) else {
            return Ok(LeaseCancelOutcome::Unknown { reason: format!("trainer {} is not registered here", handle.provider_id) });
        };
        Ok(match adapter.cancel(&handle).await {
            Ok(()) => LeaseCancelOutcome::Cancelled,
            Err(e) => LeaseCancelOutcome::Unknown { reason: e.to_string() },
        })
    }
}

crate::action_command! {
    /// A byte range of a leased job's recorded artifact, asked by the peer that leased it:
    /// resolved through the caller's own lease key and the manifest recorded when the job was
    /// first reported completed, by file NAME (never a path), and refused when the file no
    /// longer has its recorded size. At most [`MAX_ARTIFACT_CHUNK`] bytes an answer.
    pub struct GenomeLeaseArtifact { attempts: Arc<AttemptLedger> }
    name: "genome/lease/artifact",
    access: Privileged,
    params: LeaseArtifactParams,
    output: LeaseArtifactChunk,
    run(this, ctx, p) => {
        let refused = |reason: String| Ok(LeaseArtifactChunk::Refused { reason });
        let origin = match lease_origin(ctx.caller.as_ref()) {
            Ok(origin) => origin,
            Err(reason) => return refused(reason),
        };
        if p.len == 0 || p.len > MAX_ARTIFACT_CHUNK {
            return refused(format!("a range asks 1 to {MAX_ARTIFACT_CHUNK} bytes, not {}", p.len));
        }
        let recorded = match this.attempts.return_record(lease_key(origin, p.attempt)).await {
            Ok(Some(ReturnRecord { returned: Some(recorded), .. })) => recorded,
            Ok(_) => return refused(format!("attempt {} has no recorded artifact here", p.attempt)),
            Err(reason) => return Ok(LeaseArtifactChunk::Unknown { reason }),
        };
        let Some(file) = recorded.artifact.files.iter().find(|f| f.name == p.file).cloned() else {
            return refused(format!("the artifact has no file {}", p.file));
        };
        if p.offset > file.bytes {
            return refused(format!("offset {} is past {}'s {} bytes", p.offset, file.name, file.bytes));
        }
        let len = u64::from(p.len).min(file.bytes - p.offset) as usize; // at most MAX_ARTIFACT_CHUNK (8 KiB)
        let (path, offset) = (recorded.root.join(&file.name), p.offset);
        let read = tokio::task::spawn_blocking(move || -> Result<Result<Vec<u8>, String>, std::io::Error> {
            let handle = std::fs::File::open(&path)?;
            let size = handle.metadata()?.len();
            if size != file.bytes {
                return Ok(Err(format!("{} is {size} bytes, not the {} reported: it changed since", file.name, file.bytes)));
            }
            let mut buf = vec![0u8; len];
            continuum_host::positioned::read_exact_at(&handle, &mut buf, offset)?;
            Ok(Ok(buf))
        })
        .await;
        Ok(match read {
            Ok(Ok(Ok(bytes))) => LeaseArtifactChunk::Bytes {
                data: base64::engine::general_purpose::STANDARD.encode(bytes), // lease wire boundary: artifact bytes cross the grid as JSON text
            },
            Ok(Ok(Err(changed))) => LeaseArtifactChunk::Refused { reason: changed },
            Ok(Err(e)) => LeaseArtifactChunk::Unknown { reason: e.to_string() },
            Err(e) => LeaseArtifactChunk::Unknown { reason: format!("read task failed: {e}") },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bound a full chunk's encoded answer must stay under: the narrowest route's 16 KiB frame
    /// minus the 4 KiB envelope reserve ([`MAX_ARTIFACT_CHUNK`]'s doc). Only this test reads it.
    const ARTIFACT_CHUNK_FRAME_BUDGET: usize = 16 * 1024 - 4 * 1024;

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

    /// A lease through the command itself, from `origin`, on a node with no trainer that may
    /// take it (the stub trainers are cloud-placed, which `select_here` never chooses).
    async fn lease_without_a_trainer(
        attempts: Arc<AttemptLedger>,
        origin: uuid::Uuid,
        attempt: uuid::Uuid,
        request: TrainingJobRequest,
    ) -> LeaseTrainOutcome {
        use crate::sdk_codegen::{ActionCommand, Ctx};
        let command = GenomeLeaseTrain {
            coordinator: Arc::new(FineTuningCoordinator::new(super::super::test_support::registry_with(&["cloud-only"]))),
            attempts,
        };
        let mut ctx = Ctx::default();
        ctx.caller = Some(CallerIdentity::airc(crate::identity::PeerId::from_uuid(origin)));
        command
            .run(&ctx, LeaseTrainParams { attempt, dispatch: None, request })
            .await
            .expect("a lease answers with an outcome") // JUSTIFIED: test asserts the answer
    }

    fn lora_request(persona: &str) -> TrainingJobRequest {
        let mut request = super::super::test_support::request_for("qwen3.5-4b");
        request.persona_name = persona.into();
        request.lora = Some(crate::genome::fine_tuning::LoRAHyperparams { rank: 8, alpha: 16, dropout: 0.0, target_modules: vec![], top_layers: None });
        request
    }

    async fn ledger() -> (Arc<AttemptLedger>, tempfile::TempDir) {
        let (adapter, dir) = crate::orm::store::fresh_adapter().await;
        let ledger = AttemptLedger::new();
        ledger.install(Arc::new(crate::orm::OrmStore::new(adapter).await.expect("attempt store"))); // JUSTIFIED: test fixture
        (Arc::new(ledger), dir)
    }

    // what this catches (Codex on #5409, binding first): a repeat answered from whatever this
    // node's trainers do NOW instead of from its own record. With no trainer available, an
    // attempt already created answers with its job, the same attempt with another payload is
    // refused, an admitted never-settled one is Unknown (never refused: a trainer may have
    // started), an unreadable ledger is Unknown, and only a new attempt is "no trainer here".
    #[tokio::test]
    async fn a_repeat_answers_from_its_record_before_any_trainer_is_chosen() {
        let (attempts, _dir) = ledger().await;
        let origin = uuid::Uuid::new_v4();
        let request = lora_request("kimi");
        let digest = request_digest(&request).expect("digest"); // JUSTIFIED: test fixture

        let created = uuid::Uuid::new_v4();
        let job = JobHandle { provider_id: "engine-lane".into(), provider_job_id: "job-1".into(), local_id: uuid::Uuid::new_v4() };
        attempts.admit_lease(lease_key(origin, created), origin, &digest, "engine-lane", "qwen3.5-4b").await.expect("admitted"); // JUSTIFIED: test fixture
        attempts.settle(lease_key(origin, created), AttemptState::Created { handle: job.clone() }).await.expect("settled"); // JUSTIFIED: test fixture
        assert!(matches!(
            lease_without_a_trainer(attempts.clone(), origin, created, request.clone()).await,
            LeaseTrainOutcome::Created { handle } if handle == job
        ));
        assert!(matches!(
            lease_without_a_trainer(attempts.clone(), origin, created, lora_request("sahar")).await,
            LeaseTrainOutcome::Refused { error_kind, .. } if error_kind == "payload_changed"
        ));

        let admitted = uuid::Uuid::new_v4();
        attempts.admit_lease(lease_key(origin, admitted), origin, &digest, "engine-lane", "qwen3.5-4b").await.expect("admitted"); // JUSTIFIED: test fixture
        assert!(matches!(
            lease_without_a_trainer(attempts.clone(), origin, admitted, request.clone()).await,
            LeaseTrainOutcome::Unknown { .. }
        ));

        assert!(matches!(
            lease_without_a_trainer(Arc::new(AttemptLedger::new()), origin, uuid::Uuid::new_v4(), request.clone()).await,
            LeaseTrainOutcome::Unknown { .. }
        ), "an unreadable ledger may hold a started trainer");

        assert!(matches!(
            lease_without_a_trainer(attempts, origin, uuid::Uuid::new_v4(), request).await,
            LeaseTrainOutcome::Refused { error_kind, .. } if error_kind == "no_trainer_here"
        ));
    }

    fn lease_ctx(origin: uuid::Uuid) -> crate::sdk_codegen::Ctx {
        let mut ctx = crate::sdk_codegen::Ctx::default();
        ctx.caller = Some(CallerIdentity::airc(crate::identity::PeerId::from_uuid(origin)));
        ctx
    }

    // what this catches (card ec1d8b76): a leased job polled or cancelled by anyone but the
    // peer that leased it, or an attempt never received answered as if it might exist. Under
    // the origin's own key a created job reports its trainer's status and takes the cancel; an
    // attempt never received is notCreated (and closed, so a late create is fenced); a forged
    // origin reaches nothing of the real one's.
    #[tokio::test]
    async fn only_the_leasing_peer_polls_or_cancels_its_job() {
        use crate::sdk_codegen::ActionCommand;
        let (attempts, _dir) = ledger().await;
        let registry = super::super::test_support::registry_with(&["engine-lane"]);
        let poll = GenomeLeasePoll { attempts: attempts.clone(), registry: registry.clone() };
        let cancel = GenomeLeaseCancel { attempts: attempts.clone(), registry };
        let (origin, attempt) = (uuid::Uuid::new_v4(), uuid::Uuid::new_v4());
        let job = JobHandle { provider_id: "engine-lane".into(), provider_job_id: "job-1".into(), local_id: uuid::Uuid::new_v4() };
        attempts.admit_lease(lease_key(origin, attempt), origin, "digest", "engine-lane", "qwen3.5-4b").await.expect("admitted"); // JUSTIFIED: test fixture
        attempts.settle(lease_key(origin, attempt), AttemptState::Created { handle: job }).await.expect("settled"); // JUSTIFIED: test fixture
        let ask = || LeaseStatusParams { attempt };

        // The stub trainer's job finished provider-hosted: nothing to return, final.
        assert!(matches!(poll.run(&lease_ctx(origin), ask()).await, Ok(LeasePollOutcome::Unreturnable { .. })));
        assert!(matches!(cancel.run(&lease_ctx(origin), ask()).await, Ok(LeaseCancelOutcome::Cancelled)));

        let forger = uuid::Uuid::new_v4();
        assert!(matches!(poll.run(&lease_ctx(forger), ask()).await, Ok(LeasePollOutcome::NotCreated)));
        assert!(matches!(cancel.run(&lease_ctx(forger), ask()).await, Ok(LeaseCancelOutcome::NotCreated)));
        assert!(matches!(poll.run(&lease_ctx(origin), ask()).await, Ok(LeasePollOutcome::Unreturnable { .. })), "the forger touched nothing");

        let never = LeaseStatusParams { attempt: uuid::Uuid::new_v4() };
        assert!(matches!(poll.run(&lease_ctx(origin), never).await, Ok(LeasePollOutcome::NotCreated)));
        assert!(matches!(
            poll.run(&crate::sdk_codegen::Ctx::default(), ask()).await,
            Ok(LeasePollOutcome::Unknown { .. })
        ), "a local caller has no lease to ask about");
    }

    // what this catches (card 3fd98325): a completed lease reported by the trainer's path, by
    // a digest that is not the bytes, or by a report that changes after it was first given.
    // The first poll describes and records the artifact (its files and its base's fingerprint);
    // a later poll answers from that record even after the file on disk changed, so serving
    // the bytes can refuse what no longer matches.
    #[tokio::test]
    async fn a_completed_lease_is_reported_by_its_recorded_manifest() {
        use crate::genome::fine_tuning::{ArtifactFormat, RecordingFineTuningAdapter, RECORDING_PROVIDER_ID};
        use crate::sdk_codegen::ActionCommand;
        let (attempts, _dir) = ledger().await;
        let files = tempfile::tempdir().expect("dir"); // JUSTIFIED: test fixture
        let (adapter_file, base) = (files.path().join("adapter.gguf"), files.path().join("base.gguf"));
        std::fs::write(&adapter_file, b"lora").expect("write"); // JUSTIFIED: test fixture
        super::super::test_support::write_base_gguf(&base, 0.0);
        let registry = Arc::new(FineTuningRegistry::new());
        let stamp = crate::inference::weights_stamp::WeightsStamp::of(&base).expect("stamp"); // JUSTIFIED: test fixture
        registry.register(Arc::new(
            RecordingFineTuningAdapter::new().with_local_artifact(adapter_file.clone(), ArtifactFormat::GgufLora).trained_on(stamp),
        ));
        let poll = GenomeLeasePoll { attempts: attempts.clone(), registry };
        let (origin, attempt) = (uuid::Uuid::new_v4(), uuid::Uuid::new_v4());
        let job = JobHandle { provider_id: RECORDING_PROVIDER_ID.into(), provider_job_id: "job-1".into(), local_id: uuid::Uuid::new_v4() };
        attempts.admit_lease(lease_key(origin, attempt), origin, "digest", RECORDING_PROVIDER_ID, "qwen").await.expect("admitted"); // JUSTIFIED: test fixture
        attempts.settle(lease_key(origin, attempt), AttemptState::Created { handle: job }).await.expect("settled"); // JUSTIFIED: test fixture

        let Ok(LeasePollOutcome::Completed { artifact }) = poll.run(&lease_ctx(origin), LeaseStatusParams { attempt }).await else {
            panic!("a completed job with local weights is reported by its manifest"); // JUSTIFIED: test asserts the arm
        };
        assert_eq!(artifact.files.len(), 1);
        assert_eq!(artifact.files[0].sha256, format!("{:x}", Sha256::digest(b"lora")));
        assert_eq!(artifact.base.model_id, "qwen");
        assert_eq!(artifact.base.fingerprint, crate::inference_capability::gguf_fingerprint::lora_base_fingerprint(&mut std::fs::File::open(&base).expect("open")).expect("digest")); // JUSTIFIED: test asserts the base
        assert_eq!(artifact.base.source_sha256, format!("{:x}", Sha256::digest(std::fs::read(&base).expect("read")))); // JUSTIFIED: test asserts the base

        std::fs::write(&adapter_file, b"replaced").expect("write"); // JUSTIFIED: test fixture
        assert!(
            matches!(poll.run(&lease_ctx(origin), LeaseStatusParams { attempt }).await, Ok(LeasePollOutcome::Completed { artifact: again }) if again.files == artifact.files && again.base == artifact.base),
            "the first record is the report"
        );
    }

    // what this catches (card 92cd2251): an artifact range served to anyone but the leasing peer,
    // read by a caller-named path, past the bound or the file, from a file that changed since it
    // was reported, or encoded too large for the narrowest airc route. A full chunk's encoded
    // answer fits ARTIFACT_CHUNK_FRAME_BUDGET; the tail answers short; everything else refuses.
    #[tokio::test]
    async fn an_artifact_range_is_served_only_from_the_record_and_fits_the_narrowest_route() {
        use crate::genome::fine_tuning::{ArtifactFormat, RecordingFineTuningAdapter, RECORDING_PROVIDER_ID};
        use crate::sdk_codegen::ActionCommand;
        let (attempts, _dir) = ledger().await;
        let files = tempfile::tempdir().expect("dir"); // JUSTIFIED: test fixture
        let (adapter_file, base) = (files.path().join("adapter.gguf"), files.path().join("base.gguf"));
        let bytes: Vec<u8> = (0..20_000u32).map(|i| (i % 251) as u8).collect();
        std::fs::write(&adapter_file, &bytes).expect("write"); // JUSTIFIED: test fixture
        super::super::test_support::write_base_gguf(&base, 0.0);
        let stamp = crate::inference::weights_stamp::WeightsStamp::of(&base).expect("stamp"); // JUSTIFIED: test fixture
        let registry = Arc::new(FineTuningRegistry::new());
        registry.register(Arc::new(RecordingFineTuningAdapter::new().with_local_artifact(adapter_file.clone(), ArtifactFormat::GgufLora).trained_on(stamp)));
        let (origin, attempt) = (uuid::Uuid::new_v4(), uuid::Uuid::new_v4());
        let job = JobHandle { provider_id: RECORDING_PROVIDER_ID.into(), provider_job_id: "job-1".into(), local_id: uuid::Uuid::new_v4() };
        attempts.admit_lease(lease_key(origin, attempt), origin, "digest", RECORDING_PROVIDER_ID, "qwen").await.expect("admitted"); // JUSTIFIED: test fixture
        attempts.settle(lease_key(origin, attempt), AttemptState::Created { handle: job }).await.expect("settled"); // JUSTIFIED: test fixture
        let range = GenomeLeaseArtifact { attempts: attempts.clone() };
        let ask = |file: &str, offset: u64, len: u32| LeaseArtifactParams { attempt, file: file.into(), offset, len };
        assert!(
            matches!(range.run(&lease_ctx(origin), ask("adapter.gguf", 0, 16)).await, Ok(LeaseArtifactChunk::Refused { .. })),
            "nothing is served before the artifact is reported"
        );
        let poll = GenomeLeasePoll { attempts: attempts.clone(), registry };
        assert!(matches!(poll.run(&lease_ctx(origin), LeaseStatusParams { attempt }).await, Ok(LeasePollOutcome::Completed { .. })));

        let full = range.run(&lease_ctx(origin), ask("adapter.gguf", 0, MAX_ARTIFACT_CHUNK)).await.expect("answered"); // JUSTIFIED: test asserts the answer
        let encoded = serde_json::to_string(&full).expect("encode"); // JUSTIFIED: test measures the wire size
        assert!(encoded.len() <= ARTIFACT_CHUNK_FRAME_BUDGET, "a full chunk encodes to {} bytes, over {}", encoded.len(), ARTIFACT_CHUNK_FRAME_BUDGET);
        let LeaseArtifactChunk::Bytes { data } = full else { panic!("a full chunk is bytes") }; // JUSTIFIED: test asserts the arm
        let decoded = base64::engine::general_purpose::STANDARD.decode(data).expect("base64"); // JUSTIFIED: test fixture
        assert_eq!(decoded, bytes[..MAX_ARTIFACT_CHUNK as usize]);
        let tail = range.run(&lease_ctx(origin), ask("adapter.gguf", 16_384, MAX_ARTIFACT_CHUNK)).await;
        assert!(matches!(tail, Ok(LeaseArtifactChunk::Bytes { data }) if base64::engine::general_purpose::STANDARD.decode(&data).map(|d| d.len()) == Ok(20_000 - 16_384)));

        for (file, offset, len) in [("adapter.gguf", 0, 0), ("adapter.gguf", 0, MAX_ARTIFACT_CHUNK + 1), ("adapter.gguf", 20_001, 16), ("../base.gguf", 0, 16), ("base.gguf", 0, 16)] {
            assert!(matches!(range.run(&lease_ctx(origin), ask(file, offset, len)).await, Ok(LeaseArtifactChunk::Refused { .. })), "{file} {offset} {len}");
        }
        assert!(matches!(range.run(&lease_ctx(uuid::Uuid::new_v4()), ask("adapter.gguf", 0, 16)).await, Ok(LeaseArtifactChunk::Refused { .. })), "a forged origin reads nothing");
        assert!(matches!(range.run(&crate::sdk_codegen::Ctx::default(), ask("adapter.gguf", 0, 16)).await, Ok(LeaseArtifactChunk::Refused { .. })), "a local caller has no lease");

        std::fs::write(&adapter_file, b"replaced").expect("write"); // JUSTIFIED: test fixture
        assert!(matches!(range.run(&lease_ctx(origin), ask("adapter.gguf", 0, 16)).await, Ok(LeaseArtifactChunk::Refused { reason }) if reason.contains("changed")));
    }
}

