//! The training attempt ledger: the evidence owner for every `genome/job-create` that names an
//! attempt (card 55d4b9e3, design agreed with Codex 2026-10-24).
//!
//! IntelMac 10-24: eight training buckets sat in RecoveryRequired because nothing could say
//! whether a dispatch interrupted mid-create had started a job. The board learns of a job only
//! after `create_job` returns, so "no job on the board" proved nothing. This ledger is written
//! BEFORE the trainer is called, by the same owner that admits the create, so:
//! - a create for an attempt already reconciled as not created is refused before anything is
//!   reserved or spawned (the fence a delayed request meets);
//! - a repeated create for an attempt returns the job it already made, never a second one;
//! - reconciliation can prove "not created" only by closing the attempt here, atomically with
//!   admission, and a row it cannot read is Unknown, never absent.

use std::sync::Arc;

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::lease_artifact::RecordedArtifact;
use crate::genome::fine_tuning::JobHandle;
use crate::orm::{BaseEntity, OrmStore};
use crate::runtime::LateBound;

/// A new row's base, whose id IS the attempt id: the key the ledger is read by.
fn record_base(attempt: Uuid) -> BaseEntity {
    BaseEntity {
        id: attempt.to_string(),
        ..BaseEntity::for_new_record()
    }
}

/// One attempt's durable record, keyed by the attempt id the trigger minted.
#[derive(Debug, Clone, Serialize, Deserialize, crate::orm::Entity)]
#[serde(rename_all = "camelCase")]
#[entity(collection = "training_attempts")]
pub(crate) struct TrainingAttempt {
    #[serde(flatten)]
    base: BaseEntity,
    dispatch_id: Option<Uuid>,
    /// The trainer the coordinator selected; empty for a reconciliation that closed an attempt
    /// no create ever admitted.
    provider: String,
    /// Where the job runs: `local`, or a grid node once training leases out (card 6f05b89d).
    destination: String,
    /// The core process that admitted it, so evidence from another incarnation is never read
    /// as this one's.
    owner_generation: Uuid,
    /// The grid peer whose attempt this is, when this node trains it on lease (card
    /// cc34db0d); `None` for an attempt this node's own trigger made.
    #[serde(default)]
    origin: Option<Uuid>,
    /// The leased payload's digest, computed HERE from the canonical request: a repeat
    /// with the same attempt and another payload is refused.
    #[serde(default)]
    request_digest: Option<String>,
    #[entity(json)]
    state: AttemptState,
    /// The base a leased job trains on, so its artifact is reported with that base's
    /// fingerprint (card 3fd98325).
    #[serde(default)]
    base_model: Option<String>,
    /// A leased job's artifact, described once when it was first seen completed (card
    /// 3fd98325): every later report and every byte served answers from this record.
    #[serde(default)]
    #[entity(json)]
    returned: Option<RecordedArtifact>,
}

/// Where an attempt stands. Only `Created` names a job; `Admitted` alone proves nothing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub(crate) enum AttemptState {
    /// Written before `create_job` ran: a crash from here on leaves the outcome unknown.
    Admitted,
    /// The trainer returned this handle.
    Created { handle: JobHandle },
    /// The trainer refused with a certain, created-nothing error kind.
    Failed { kind: String },
    /// Reconciliation closed an attempt no create had admitted; any later create is refused.
    Closed,
}

/// What admission decided for one create.
#[derive(Debug, PartialEq)]
pub(crate) enum Admission {
    /// Recorded as admitted: call the trainer, then report what it returned.
    Fresh,
    /// This attempt already made a job: return it, never spawn a second one.
    AlreadyCreated(JobHandle),
    /// Closed by reconciliation, or already refused for certain: nothing may start.
    Fenced,
    /// Admitted and never settled (a create in flight, or one that crashed): unknown.
    Unsettled,
}

/// Why admission could not decide. The two are not alike: a record that cannot be READ may
/// name a job already started, so it is uncertain; a NEW record that cannot be written, after
/// the read proved there was none, leaves nothing started (Codex review of c2c955174).
#[derive(Debug, PartialEq)]
pub(crate) enum AdmitError {
    /// The ledger, or this attempt's existing record, could not be read: uncertain.
    Unreadable(String),
    /// No record existed and the admission could not be written: the trainer is not
    /// called, so nothing started under this attempt (certain).
    Unrecorded(String),
}

/// What reconciliation can prove about an attempt.
#[derive(Debug, PartialEq)]
pub(crate) enum Reconciled {
    Created(JobHandle),
    /// Closed here with no admission, or refused for certain: no job exists or can start.
    NotCreated,
    /// The evidence does not decide it; the reason says why.
    Unknown(String),
}

/// What a recorded state means to a new create for the same attempt: the one mapping
/// `admit`, `peek` and `admit_lease` share.
fn admission_of(state: AttemptState) -> Admission {
    match state {
        AttemptState::Created { handle } => Admission::AlreadyCreated(handle),
        AttemptState::Closed | AttemptState::Failed { .. } => Admission::Fenced,
        AttemptState::Admitted => Admission::Unsettled,
    }
}

/// What an existing leased record means to a create with this payload: closed or refused for
/// certain is fenced whatever the payload; a live record from another payload is refused.
fn lease_admission(row: TrainingAttempt, request_digest: &str) -> Result<Admission, LeaseRefusal> {
    match row.state {
        AttemptState::Closed | AttemptState::Failed { .. } => Ok(Admission::Fenced),
        _ if row.request_digest.as_deref() != Some(request_digest) => Err(LeaseRefusal::PayloadChanged),
        state => Ok(admission_of(state)),
    }
}

/// Fixed namespace for [`lease_key`]; changing it would orphan every leased record.
const LEASE_NAMESPACE: Uuid = Uuid::from_u128(0x6f05_b89d_cc34_4db0_8d1e_a5e0_1ea5_e001);

/// A leased attempt's key on the peer training it (card cc34db0d): derived from the
/// AUTHENTICATED origin and its attempt id, so one origin can never reach, create under,
/// or read another origin's record by naming the same attempt id.
pub(crate) fn lease_key(origin: Uuid, attempt: Uuid) -> Uuid {
    let mut name = [0u8; 32];
    name[..16].copy_from_slice(origin.as_bytes());
    name[16..].copy_from_slice(attempt.as_bytes());
    Uuid::new_v5(&LEASE_NAMESPACE, &name)
}

/// Why a lease could not be admitted.
#[derive(Debug, PartialEq)]
pub(crate) enum LeaseRefusal {
    /// The ledger could not decide (see [`AdmitError`]).
    Ledger(AdmitError),
    /// This origin's attempt was admitted before with another payload: refused, so one
    /// attempt can never stand for two trainings.
    PayloadChanged,
}

/// A row's return facts: the recorded artifact, if any, and the base the job trained on.
#[derive(Debug, Clone)]
pub(crate) struct ReturnRecord {
    pub(crate) returned: Option<RecordedArtifact>,
    pub(crate) base_model: Option<String>,
}

/// The ledger. One per process, shared by `genome/job-create` and reconciliation; the
/// per-attempt locks make admission and closing one atomic step for a given attempt.
pub(crate) struct AttemptLedger {
    store: LateBound<OrmStore<TrainingAttempt>>,
    locks: DashMap<Uuid, Arc<tokio::sync::Mutex<()>>>,
    /// One `genome/job-create` at a time per attempt, held for the whole command. Separate
    /// from `locks` (held only around a record read or write) so reconciliation is never
    /// blocked behind a long create.
    runs: DashMap<Uuid, Arc<tokio::sync::Mutex<()>>>,
}

impl AttemptLedger {
    pub(crate) fn new() -> Self {
        Self {
            store: LateBound::new("genome training attempts"),
            locks: DashMap::new(),
            runs: DashMap::new(),
        }
    }

    /// Serialize every `genome/job-create` for `attempt` (Codex review of 30b89a759): held from
    /// before the attempt is read until the command returns, so a second request for the same
    /// attempt can never create and register its job while the first is inside its reuse/join
    /// decision (which would see that job and "join" it, handing the batch back). The second
    /// waits, then reads the attempt and returns the job the first made.
    pub(crate) async fn run(&self, attempt: Uuid) -> tokio::sync::OwnedMutexGuard<()> {
        let run = Arc::clone(self.runs.entry(attempt).or_default().value());
        run.lock_owned().await
    }

    /// How many holders the attempt's run lock has: the ledger's own, plus one per request
    /// holding or waiting on it. A test reads this to prove a second request is parked on it.
    #[cfg(test)]
    pub(crate) fn run_holders(&self, attempt: Uuid) -> usize {
        self.runs.get(&attempt).map_or(0, |run| Arc::strong_count(run.value()))
    }

    pub(crate) fn install(&self, store: Arc<OrmStore<TrainingAttempt>>) {
        self.store.install(store);
    }

    fn lock(&self, attempt: Uuid) -> Arc<tokio::sync::Mutex<()>> {
        Arc::clone(self.locks.entry(attempt).or_default().value())
    }

    /// Admit a create for `attempt`. An error means nothing was recorded, so the caller must
    /// not call the trainer: a create the ledger cannot witness is never started.
    pub(crate) async fn admit(
        &self,
        attempt: Uuid,
        dispatch_id: Option<Uuid>,
        provider: &str,
        destination: &str,
    ) -> Result<Admission, AdmitError> {
        let lock = self.lock(attempt);
        let _held = lock.lock().await;
        let store = self.store.require().map_err(AdmitError::Unreadable)?;
        let existing = store
            .find_by_id(attempt)
            .await
            .map_err(|e| AdmitError::Unreadable(e.to_string()))?;
        match existing {
            Some(row) => Ok(admission_of(row.state)),
            None => {
                let row = TrainingAttempt {
                    base: record_base(attempt),
                    dispatch_id,
                    provider: provider.to_string(),
                    destination: destination.to_string(),
                    owner_generation: crate::capacity::gossip::this_process_origin(),
                    origin: None,
                    request_digest: None,
                    base_model: None,
                    returned: None,
                    state: AttemptState::Admitted,
                };
                store
                    .save(attempt, &row)
                    .await
                    .map_err(|e| AdmitError::Unrecorded(e.to_string()))?;
                Ok(Admission::Fresh)
            }
        }
    }

    /// Admit a leased create (card cc34db0d) under `key` ([`lease_key`]), recording its origin,
    /// the payload digest this node computed and the base it trains on. A closed or settled
    /// record answers as for any attempt; a live record from another payload is refused.
    pub(crate) async fn admit_lease(
        &self,
        key: Uuid,
        origin: Uuid,
        request_digest: &str,
        provider: &str,
        base_model: &str,
    ) -> Result<Admission, LeaseRefusal> {
        let lock = self.lock(key);
        let _held = lock.lock().await;
        let store = self.store.require().map_err(|e| LeaseRefusal::Ledger(AdmitError::Unreadable(e)))?;
        let existing = store
            .find_by_id(key)
            .await
            .map_err(|e| LeaseRefusal::Ledger(AdmitError::Unreadable(e.to_string())))?;
        match existing {
            Some(row) => lease_admission(row, request_digest),
            None => {
                let row = TrainingAttempt {
                    base: record_base(key),
                    dispatch_id: None,
                    provider: provider.to_string(),
                    destination: "local".to_string(),
                    owner_generation: crate::capacity::gossip::this_process_origin(),
                    origin: Some(origin),
                    request_digest: Some(request_digest.to_string()),
                    base_model: Some(base_model.to_string()),
                    returned: None,
                    state: AttemptState::Admitted,
                };
                store
                    .save(key, &row)
                    .await
                    .map_err(|e| LeaseRefusal::Ledger(AdmitError::Unrecorded(e.to_string())))?;
                Ok(Admission::Fresh)
            }
        }
    }

    /// The existing record of a leased attempt, judged against this payload, without
    /// recording anything: read BEFORE a trainer is chosen, so a repeat answers from its own
    /// record (its job, a refusal, or Unknown) whatever this node's trainers are doing now.
    /// `None` = no record; `admit_lease` still decides under the lock.
    pub(crate) async fn lease_record(&self, key: Uuid, request_digest: &str) -> Result<Option<Admission>, LeaseRefusal> {
        let lock = self.lock(key);
        let _held = lock.lock().await;
        let store = self.store.require().map_err(|e| LeaseRefusal::Ledger(AdmitError::Unreadable(e)))?;
        let existing = store
            .find_by_id(key)
            .await
            .map_err(|e| LeaseRefusal::Ledger(AdmitError::Unreadable(e.to_string())))?;
        existing.map(|row| lease_admission(row, request_digest)).transpose()
    }

    /// What admission WOULD decide for `attempt`, without recording anything: lets
    /// `genome/job-create` return an attempt's existing job, or refuse a fenced one, before it
    /// runs the reuse/join decisions (which would otherwise "join" the attempt's own job and
    /// hand its batch back). `None` = no record; `admit` still decides under the lock.
    pub(crate) async fn peek(&self, attempt: Uuid) -> Result<Option<Admission>, AdmitError> {
        let lock = self.lock(attempt);
        let _held = lock.lock().await;
        let store = self.store.require().map_err(AdmitError::Unreadable)?;
        let existing = store
            .find_by_id(attempt)
            .await
            .map_err(|e| AdmitError::Unreadable(e.to_string()))?;
        Ok(existing.map(|row| admission_of(row.state)))
    }

    /// Settle an admitted attempt with what the trainer returned. A failed write leaves it
    /// `Admitted`, which reconciles as Unknown: conservative, never a false "not created".
    pub(crate) async fn settle(&self, attempt: Uuid, state: AttemptState) -> Result<(), String> {
        let lock = self.lock(attempt);
        let _held = lock.lock().await;
        let store = self.store.require()?;
        let mut row = store
            .find_by_id(attempt)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("attempt {attempt} was never admitted"))?;
        row.state = state;
        store.update(attempt, &row).await.map_err(|e| e.to_string())
    }

    /// Where `attempt` was recorded to run (`Destination::to_ledger`: `local` or `peer:<uuid>`),
    /// or `None` with no record. Read by reconciliation, so a lease's outcome is asked of its
    /// peer instead of guessed here (card e53bb821).
    pub(crate) async fn destination_of(&self, attempt: Uuid) -> Result<Option<String>, String> {
        let lock = self.lock(attempt);
        let _held = lock.lock().await;
        let store = self.store.require()?;
        Ok(store.find_by_id(attempt).await.map_err(|e| e.to_string())?.map(|row| row.destination))
    }

    /// The artifact recorded for a created attempt, or this one recorded now if none was: the
    /// first description wins, so a report never changes under the origin (card 3fd98325).
    pub(crate) async fn record_return(&self, attempt: Uuid, described: RecordedArtifact) -> Result<RecordedArtifact, String> {
        let lock = self.lock(attempt);
        let _held = lock.lock().await;
        let store = self.store.require()?;
        let mut row = store
            .find_by_id(attempt)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("attempt {attempt} was never admitted"))?;
        if !matches!(row.state, AttemptState::Created { .. }) {
            return Err(format!("attempt {attempt} created no job: it has no artifact"));
        }
        if let Some(recorded) = row.returned {
            return Ok(recorded);
        }
        row.returned = Some(described.clone());
        store.update(attempt, &row).await.map_err(|e| e.to_string())?;
        Ok(described)
    }

    /// What `attempt`'s row holds for its return: the artifact already recorded, and the base
    /// it trained on. `None` with no row.
    pub(crate) async fn return_record(&self, attempt: Uuid) -> Result<Option<ReturnRecord>, String> {
        let lock = self.lock(attempt);
        let _held = lock.lock().await;
        let store = self.store.require()?;
        Ok(store
            .find_by_id(attempt)
            .await
            .map_err(|e| e.to_string())?
            .map(|row| ReturnRecord { returned: row.returned, base_model: row.base_model }))
    }

    /// Prove what `attempt` did. With no row, the attempt is CLOSED here first, under the same
    /// lock admission takes, so a delayed create can no longer start it: only then is "no
    /// admission" proof of NotCreated. An unreadable ledger is Unknown, never absent.
    pub(crate) async fn reconcile(&self, attempt: Uuid) -> Reconciled {
        let lock = self.lock(attempt);
        let _held = lock.lock().await;
        let store = match self.store.require() {
            Ok(store) => store,
            Err(e) => return Reconciled::Unknown(format!("attempt ledger unavailable: {e}")),
        };
        match store.find_by_id(attempt).await {
            Err(e) => Reconciled::Unknown(format!("attempt ledger unreadable: {e}")),
            Ok(Some(row)) => match row.state {
                AttemptState::Created { handle } => Reconciled::Created(handle),
                AttemptState::Closed | AttemptState::Failed { .. } => Reconciled::NotCreated,
                AttemptState::Admitted => Reconciled::Unknown(format!(
                    "attempt {attempt} was admitted to {} and never settled: a create may have started",
                    row.provider
                )),
            },
            Ok(None) => {
                let row = TrainingAttempt {
                    base: record_base(attempt),
                    dispatch_id: None,
                    provider: String::new(),
                    destination: String::new(),
                    owner_generation: crate::capacity::gossip::this_process_origin(),
                    origin: None,
                    request_digest: None,
                    base_model: None,
                    returned: None,
                    state: AttemptState::Closed,
                };
                match store.save(attempt, &row).await {
                    Ok(()) => Reconciled::NotCreated,
                    Err(e) => Reconciled::Unknown(format!("could not close attempt {attempt}: {e}")),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn ledger() -> (AttemptLedger, tempfile::TempDir) {
        let (adapter, dir) = crate::orm::store::fresh_adapter().await;
        let ledger = AttemptLedger::new();
        ledger.install(Arc::new(OrmStore::new(adapter).await.expect("attempt store")));
        (ledger, dir)
    }

    fn handle() -> JobHandle {
        let id = Uuid::new_v4();
        JobHandle {
            provider_id: "engine-local".into(),
            provider_job_id: id.to_string(),
            local_id: id,
        }
    }

    // what this catches (Codex on 55d4b9e3): a delayed create racing a negative reconciliation.
    // Reconciliation closes the attempt first, so the late create is refused before anything
    // starts: "not created" stays true.
    #[tokio::test]
    async fn a_create_arriving_after_a_negative_reconciliation_is_fenced() {
        let (ledger, _dir) = ledger().await;
        let attempt = Uuid::new_v4();
        assert_eq!(ledger.reconcile(attempt).await, Reconciled::NotCreated);
        assert_eq!(
            ledger.admit(attempt, None, "engine-local", "local").await,
            Ok(Admission::Fenced)
        );
        assert_eq!(ledger.reconcile(attempt).await, Reconciled::NotCreated);
    }

    // what this catches: a crash after the admission record and before the trainer answered.
    // Admission alone proves nothing, so the outcome is Unknown, never NotCreated.
    #[tokio::test]
    async fn an_admitted_attempt_that_never_settled_is_unknown() {
        let (ledger, _dir) = ledger().await;
        let attempt = Uuid::new_v4();
        assert_eq!(
            ledger.admit(attempt, Some(Uuid::new_v4()), "engine-local", "local").await,
            Ok(Admission::Fresh)
        );
        assert!(matches!(ledger.reconcile(attempt).await, Reconciled::Unknown(_)));
        assert_eq!(
            ledger.admit(attempt, None, "engine-local", "local").await,
            Ok(Admission::Unsettled),
            "a duplicate create while unsettled starts nothing"
        );
    }

    // what this catches: a duplicate create(attempt) returns the job it already made, never a
    // second one; a certain refusal fences the attempt for good.
    #[tokio::test]
    async fn a_duplicate_create_returns_the_same_job_and_a_refusal_fences() {
        let (ledger, _dir) = ledger().await;
        let made = Uuid::new_v4();
        assert_eq!(ledger.admit(made, None, "engine-local", "local").await, Ok(Admission::Fresh));
        let job = handle();
        ledger.settle(made, AttemptState::Created { handle: job.clone() }).await.unwrap();
        assert_eq!(
            ledger.admit(made, None, "engine-local", "local").await,
            Ok(Admission::AlreadyCreated(job.clone()))
        );
        assert_eq!(ledger.reconcile(made).await, Reconciled::Created(job));

        let refused = Uuid::new_v4();
        assert_eq!(ledger.admit(refused, None, "mlx-local", "local").await, Ok(Admission::Fresh));
        ledger
            .settle(refused, AttemptState::Failed { kind: "TrainerUnavailable".into() })
            .await
            .unwrap();
        assert_eq!(ledger.reconcile(refused).await, Reconciled::NotCreated);
        assert_eq!(
            ledger.admit(refused, None, "mlx-local", "local").await,
            Ok(Admission::Fenced)
        );
    }

    // what this catches: an unreadable ledger is never read as "no row". Reconciliation says
    // Unknown, and admission refuses, so no create runs unwitnessed.
    #[tokio::test]
    async fn an_unreadable_ledger_is_unknown_and_admits_nothing() {
        let ledger = AttemptLedger::new(); // never installed: the store cannot be read
        let attempt = Uuid::new_v4();
        assert!(matches!(ledger.reconcile(attempt).await, Reconciled::Unknown(_)));
        assert!(matches!(
            ledger.admit(attempt, None, "engine-local", "local").await,
            Err(AdmitError::Unreadable(_))
        ));
    }

    // what this catches (Codex review of c2c955174): an EXISTING record that cannot be read
    // may name a job already started, so admission is uncertain (Unreadable), never the
    // certain Unrecorded, and reconciliation says Unknown, never "not created".
    #[tokio::test]
    async fn an_existing_record_that_cannot_be_read_is_uncertain() {
        let (adapter, _dir) = crate::orm::store::fresh_adapter().await;
        let typed: Arc<OrmStore<TrainingAttempt>> =
            Arc::new(OrmStore::new(adapter).await.expect("attempt store"));
        let ledger = AttemptLedger::new();
        ledger.install(Arc::clone(&typed));
        let attempt = Uuid::new_v4();
        assert_eq!(ledger.admit(attempt, None, "engine-local", "local").await, Ok(Admission::Fresh));
        ledger.settle(attempt, AttemptState::Created { handle: handle() }).await.unwrap();
        // The created record's state is overwritten with one this build cannot decode.
        typed
            .update_fields(attempt, &serde_json::json!({ "state": { "state": "from_a_future_build" } }))
            .await
            .unwrap();
        assert!(matches!(
            ledger.admit(attempt, None, "engine-local", "local").await,
            Err(AdmitError::Unreadable(_))
        ));
        assert!(matches!(ledger.reconcile(attempt).await, Reconciled::Unknown(_)));
    }

    // what this catches: admission and closing are one atomic step per attempt. Racing a
    // create against a reconciliation never yields both "admitted" and "not created".
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn a_concurrent_create_and_reconcile_never_both_win() {
        let (ledger, _dir) = ledger().await;
        let ledger = Arc::new(ledger);
        for _ in 0..32 {
            let attempt = Uuid::new_v4();
            let (a, r) = (Arc::clone(&ledger), Arc::clone(&ledger));
            let (admitted, reconciled) = tokio::join!(
                tokio::spawn(async move { a.admit(attempt, None, "engine-local", "local").await }),
                tokio::spawn(async move { r.reconcile(attempt).await }),
            );
            match (admitted.unwrap(), reconciled.unwrap()) {
                (Ok(Admission::Fresh), Reconciled::Unknown(_)) => {}
                (Ok(Admission::Fenced), Reconciled::NotCreated) => {}
                other => panic!("create and reconcile both decided: {other:?}"),
            }
        }
    }

    // what this catches: the ledger is durable across a restart. A new owner over the same
    // database returns the job a created attempt made and keeps a closed attempt closed.
    #[tokio::test]
    async fn the_ledger_survives_a_restart() {
        let (adapter, _dir) = crate::orm::store::fresh_adapter().await;
        let open = |adapter: Arc<dyn crate::orm::StorageAdapter>| async move {
            let ledger = AttemptLedger::new();
            ledger.install(Arc::new(OrmStore::new(adapter).await.expect("attempt store")));
            ledger
        };
        let (made, closed, job) = (Uuid::new_v4(), Uuid::new_v4(), handle());
        {
            let before = open(adapter.clone()).await;
            assert_eq!(before.admit(made, None, "engine-local", "local").await, Ok(Admission::Fresh));
            before.settle(made, AttemptState::Created { handle: job.clone() }).await.unwrap();
            assert_eq!(before.reconcile(closed).await, Reconciled::NotCreated);
        }
        let after = open(adapter).await;
        assert_eq!(
            after.admit(made, None, "engine-local", "local").await,
            Ok(Admission::AlreadyCreated(job.clone()))
        );
        assert_eq!(after.reconcile(made).await, Reconciled::Created(job));
        assert_eq!(after.admit(closed, None, "engine-local", "local").await, Ok(Admission::Fenced));
    }

    // what this catches (card cc34db0d, Codex's gates for a leased attempt):
    // - the same attempt with another payload is refused, never a second training;
    // - a concurrent or crashed repeat (admitted, never settled) is never admitted fresh, so a
    //   crash between the trainer starting and its handle being recorded reads unknown;
    // - a status query for an attempt never received closes it first, so a late create is
    //   fenced and "not created" stays true;
    // - a forged origin derives another key: its query closes nothing of the real origin's.
    #[tokio::test]
    async fn a_leased_attempt_is_keyed_by_its_origin_and_never_trained_twice() {
        let (ledger, _dir) = ledger().await;
        let (origin, attempt) = (Uuid::new_v4(), Uuid::new_v4());
        let key = lease_key(origin, attempt);
        assert_eq!(ledger.admit_lease(key, origin, "digest-a", "engine-lane", "qwen").await, Ok(Admission::Fresh));
        assert_eq!(
            ledger.admit_lease(key, origin, "digest-b", "engine-lane", "qwen").await,
            Err(LeaseRefusal::PayloadChanged),
            "another payload under the same attempt"
        );
        assert_eq!(
            ledger.admit_lease(key, origin, "digest-a", "engine-lane", "qwen").await,
            Ok(Admission::Unsettled),
            "a repeat while the first is in flight, or after a crash, is never fresh"
        );
        assert!(matches!(ledger.reconcile(key).await, Reconciled::Unknown(_)), "admitted, never settled: unknown");

        let forger = Uuid::new_v4();
        assert_ne!(lease_key(forger, attempt), key);
        assert_eq!(ledger.reconcile(lease_key(forger, attempt)).await, Reconciled::NotCreated);
        assert!(matches!(ledger.reconcile(key).await, Reconciled::Unknown(_)), "the real record is untouched");

        let never_sent = Uuid::new_v4();
        let late_key = lease_key(origin, never_sent);
        assert_eq!(ledger.reconcile(late_key).await, Reconciled::NotCreated, "asked before it arrived");
        assert_eq!(ledger.admit_lease(late_key, origin, "digest-c", "engine-lane", "qwen").await, Ok(Admission::Fenced), "the late create starts nothing");

        ledger.settle(key, AttemptState::Created { handle: handle() }).await.expect("settled");
        assert!(matches!(
            ledger.admit_lease(key, origin, "digest-a", "engine-lane", "qwen").await,
            Ok(Admission::AlreadyCreated(_))
        ));
    }
}

