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
    #[entity(json)]
    state: AttemptState,
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

/// The ledger. One per process, shared by `genome/job-create` and reconciliation; the
/// per-attempt locks make admission and closing one atomic step for a given attempt.
pub(crate) struct AttemptLedger {
    store: LateBound<OrmStore<TrainingAttempt>>,
    locks: DashMap<Uuid, Arc<tokio::sync::Mutex<()>>>,
}

impl AttemptLedger {
    pub(crate) fn new() -> Self {
        Self {
            store: LateBound::new("genome training attempts"),
            locks: DashMap::new(),
        }
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
            Some(row) => Ok(match row.state {
                AttemptState::Created { handle } => Admission::AlreadyCreated(handle),
                AttemptState::Closed | AttemptState::Failed { .. } => Admission::Fenced,
                AttemptState::Admitted => Admission::Unsettled,
            }),
            None => {
                let row = TrainingAttempt {
                    base: record_base(attempt),
                    dispatch_id,
                    provider: provider.to_string(),
                    destination: destination.to_string(),
                    owner_generation: crate::capacity::gossip::this_process_origin(),
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
}
