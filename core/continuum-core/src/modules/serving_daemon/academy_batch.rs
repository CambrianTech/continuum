//! Explicit teacher batches owned by the existing serving lifecycle operation.
//! Only a corpus leaves this module; the private borrower never enters a pool.

use super::*;
use crate::cognition::eval::{EvalTask, PrivateTeacherLane};
use crate::commands::genome::teach::RemediationCorpus;
use crate::inference::llama_server::{EngineRetirementStatus, OwnedRestoreSession};
use crate::sdk_codegen::CommandError;
use futures::FutureExt;
use std::panic::AssertUnwindSafe;
use tokio::sync::{oneshot, Mutex};

pub(crate) struct TeacherBatchRequest {
    pub tasks: Vec<EvalTask>,
    pub teacher_model: String,
    pub temperature: f32,
    pub max_fix_iters: u32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Untouched,
    Checkpoint,
    Retired,
    Borrowing,
    Restoring,
    Complete,
}

struct BatchState {
    _operation: ServingOperation,
    request: TeacherBatchRequest,
    capture: Option<VerifiedServingCapture>,
    prior: ServingSnapshot,
    restore_bytes: Option<u64>,
    session: Option<OwnedRestoreSession>,
    borrower: Option<PrivateTeacherLane>,
    phase: Phase,
    cancellation: watch::Receiver<bool>,
    response: Option<oneshot::Sender<Result<RemediationCorpus, CommandError>>>,
    outcome: Option<Result<RemediationCorpus, CommandError>>,
    shutdown: Arc<AtomicBool>,
}

pub(super) struct BatchSlot {
    state: Arc<Mutex<BatchState>>,
    task: JoinHandle<()>,
    owner: std::sync::Weak<ServingDaemonModule>,
    cancellation: watch::Sender<bool>,
    shutdown: Arc<AtomicBool>,
}

struct CancelOnDrop(watch::Sender<bool>);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        let _ = self.0.send(true);
    }
}

fn failure(message: impl Into<String>) -> CommandError {
    CommandError::Internal(message.into())
}

impl ServingDaemonModule {
    pub(crate) async fn run_teacher_batch(
        self: &Arc<Self>,
        request: TeacherBatchRequest,
    ) -> Result<RemediationCorpus, CommandError> {
        let capture = self.verified_serving_target();
        let operation = ServingOperation::acquire(self.reconciling.clone())
            .ok_or_else(|| failure("serving lifecycle is busy; teacher batch deferred"))?;
        let (response, receiver) = oneshot::channel();
        let (cancellation, cancel_rx) = watch::channel(false);
        let cancel_on_drop = CancelOnDrop(cancellation.clone());
        let shutdown = Arc::new(AtomicBool::new(false));
        let restore_bytes = capture.as_ref().and_then(|captured| {
            let launch = &captured.launch;
            footprint_for(&launch.target.model)
                .map(|footprint| {
                    attribute_serving(
                        footprint.peak_resident_bytes(
                            launch.observed_context_window,
                            launch.observed_lanes,
                        ),
                        launch.observed_lanes,
                        launch.launched_host_prompt_cache_mib,
                    )
                })
                .filter(|bytes| *bytes > 0)
        });
        let state = Arc::new(Mutex::new(BatchState {
            _operation: operation,
            request,
            capture,
            prior: self.serving_tx.borrow().clone(),
            restore_bytes,
            session: None,
            borrower: None,
            phase: Phase::Untouched,
            cancellation: cancel_rx,
            response: Some(response),
            outcome: None,
            shutdown: shutdown.clone(),
        }));
        {
            let mut slot = self.academy_batch.lock();
            if slot.is_some() {
                return Err(failure("previous teacher cleanup remains owned"));
            }
            let task = Self::spawn_teacher_worker(self.clone(), state.clone(), true);
            *slot = Some(BatchSlot {
                state,
                task,
                owner: Arc::downgrade(self),
                cancellation,
                shutdown,
            });
        }
        let result = receiver
            .await
            .map_err(|_| failure("teacher owner ended without a terminal result"))?;
        drop(cancel_on_drop);
        result
    }

    fn spawn_teacher_worker(
        owner: Arc<Self>,
        state: Arc<Mutex<BatchState>>,
        execute: bool,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            // The slot owns this state independently of the future. Panic drops an
            // async guard, never the permit, captured recipe or borrower process.
            let mut batch = state.lock().await;
            if execute {
                batch.outcome = Some(
                    match AssertUnwindSafe(owner.execute_teacher_batch(&mut batch))
                        .catch_unwind()
                        .await
                    {
                        Ok(result) => result,
                        Err(_) => Err(failure(
                            "teacher batch panicked; owned cleanup remains required",
                        )),
                    },
                );
            }
            let cleanup = match AssertUnwindSafe(owner.cleanup_teacher_batch(&mut batch))
                .catch_unwind()
                .await
            {
                Ok(result) => result,
                Err(_) => Err(failure("cleanup panicked")),
            };
            match cleanup {
                Ok(()) => {
                    batch.phase = Phase::Complete;
                    let outcome = batch
                        .outcome
                        .take()
                        .unwrap_or_else(|| Err(failure("teacher batch interrupted")));
                    if let Some(response) = batch.response.take() {
                        let _ = response.send(outcome);
                    }
                }
                Err(error) => {
                    let detail = error.to_string();
                    if batch.session.is_some() {
                        let mut snapshot = owner.serving_tx.borrow().clone();
                        snapshot.ready = false;
                        snapshot.ready_verified_at_ms = None;
                        snapshot.degraded_reason =
                            Some(format!("teacher cleanup pending: {detail}"));
                        Self::emit_serving(owner.bus.get(), &snapshot);
                        let _ = owner.serving_tx.send_replace(snapshot);
                    }
                    if let Some(response) = batch.response.take() {
                        let _ = response.send(Err(failure(format!(
                            "teacher cleanup remains owned: {detail}"
                        ))));
                    }
                }
            }
        })
    }

    /// Reuse the daemon tick; no second reaper or scheduler. Never join while
    /// holding a lock the worker needs. Finished-task observation includes panic.
    pub(super) fn poll_teacher_batch(&self) {
        let mut slot = self.academy_batch.lock();
        let Some(active) = slot.as_mut() else { return };
        if !active.task.is_finished() {
            return;
        }
        if let Some(Err(error)) = (&mut active.task).now_or_never() {
            if let Ok(mut state) = active.state.try_lock() {
                state.outcome = Some(Err(failure(format!("teacher owner task ended: {error}"))));
            }
        }
        let complete = active
            .state
            .try_lock()
            .is_ok_and(|s| s.phase == Phase::Complete);
        if complete {
            slot.take();
            return;
        }
        if let Some(owner) = active.owner.upgrade() {
            active.task = Self::spawn_teacher_worker(owner, active.state.clone(), false);
        }
    }

    pub(super) fn interrupt_teacher_batch(&self) -> Result<u32, String> {
        let slot = self.academy_batch.lock();
        if let Some(active) = slot.as_ref() {
            active.shutdown.store(true, Ordering::Release);
            let _ = active.cancellation.send(true);
            return Err("teacher window interrupted by shutdown; child cleanup remains owned, cold restoration is not acknowledged".into());
        }
        Ok(0)
    }

    fn publish_teacher_unavailable(&self, reason: &str) {
        let snapshot = ServingSnapshot::degraded(reason.to_string());
        let _ = self.verified_target.send_replace(None);
        Self::emit_serving(self.bus.get(), &snapshot);
        let _ = self.serving_tx.send_replace(snapshot);
    }

    async fn execute_teacher_batch(
        &self,
        batch: &mut BatchState,
    ) -> Result<RemediationCorpus, CommandError> {
        if *batch.cancellation.borrow() {
            return Err(failure("teacher batch cancelled before admission"));
        }
        let base = (self.model_resolver)(&batch.request.teacher_model).ok_or_else(|| {
            failure(format!(
                "teacher '{}' is not registered",
                batch.request.teacher_model
            ))
        })?;
        let serving = self.serving_tx.borrow().clone();
        let mut cancellation = batch.cancellation.clone();
        let shared = tokio::select! {
            result = crate::cognition::eval::share_teacher_lane(&base, &serving) => result?,
            _ = cancellation.changed() => return Err(failure("teacher cancelled during sharing preflight")),
        };
        if let Some(shared) = shared {
            // Cancellation stops new attempts, never drops an in-flight shared
            // generation and pretends that the live engine stopped decoding.
            return crate::commands::genome::teach::synthesize_remediation_with_adapter(
                &batch.request.tasks,
                &batch.request.teacher_model,
                batch.request.temperature,
                batch.request.max_fix_iters,
                &shared.adapter,
                Some(&batch.cancellation),
            )
            .await;
        }
        // An independently admitted teacher does not require incumbent retirement.
        batch.borrower = tokio::select! {
            result = PrivateTeacherLane::prepare(&base, &self.resource_daemon) => result?,
            _ = cancellation.changed() => return Err(failure("teacher cancelled during admission")),
        };
        if batch.borrower.is_none() {
            let capture = batch
                .capture
                .as_ref()
                .ok_or_else(|| failure("incumbent has no current owned restoration receipt"))?;
            if batch.restore_bytes.is_none() {
                return Err(failure("incumbent restoration footprint is unknown"));
            }
            if self.intent.snapshot().revision != capture.intent_revision {
                return Err(failure("serving intent changed before teacher admission"));
            }
            batch.session = Some(
                self.server
                    .begin_owned_restore(&capture.launch)
                    .await
                    .map_err(|e| failure(e.to_string()))?,
            );
            batch.phase = Phase::Checkpoint;
            let admitted = AtomicBool::new(false);
            let current = || {
                if self.intent.snapshot().revision != capture.intent_revision
                    || *batch.cancellation.borrow()
                {
                    return false;
                }
                if !admitted.swap(true, Ordering::AcqRel) {
                    Self::publish_lifecycle_admission(
                        &self.serving_tx,
                        &self.verified_target,
                        self.bus.get(),
                        None,
                    );
                }
                true
            };
            let session = batch
                .session
                .as_ref()
                .ok_or_else(|| failure("missing original restore session"))?;
            let receipt = self
                .server
                .checkpoint_owned_restore(session, &current)
                .await
                .map_err(|e| failure(e.to_string()))?;
            batch.phase = Phase::Retired;
            self.wait_teacher_exit(&receipt).await?;
            self.publish_teacher_unavailable("incumbent exited for explicit teacher batch");
            let mut board = self.resource_daemon.subscribe();
            let admission = async {
                loop {
                    if *batch.cancellation.borrow() {
                        return Err(failure("teacher batch cancelled during admission"));
                    }
                    if self.intent.snapshot().revision != capture.intent_revision {
                        return Err(failure("serving intent changed during admission"));
                    }
                    if let Some(lane) =
                        PrivateTeacherLane::prepare(&base, &self.resource_daemon).await?
                    {
                        return Ok(lane);
                    }
                    board
                        .changed()
                        .await
                        .map_err(|_| failure("resource authority ended"))?;
                }
            };
            batch.borrower = Some(tokio::select! {
                result = tokio::time::timeout(Duration::from_secs(30), admission) => result
                    .map_err(|_| failure("teacher capacity did not become available"))??,
                _ = cancellation.changed() => return Err(failure("teacher cancelled while awaiting capacity")),
            });
        }
        batch.phase = Phase::Borrowing;
        let borrower = batch
            .borrower
            .as_mut()
            .ok_or_else(|| failure("private teacher missing"))?;
        let mut cancellation = batch.cancellation.clone();
        tokio::select! {
            result = borrower.start() => result?,
            _ = cancellation.changed() => return Err(failure("teacher cancelled during startup")),
        }
        tokio::select! {
            result = borrower.synthesize(&batch.request.tasks, batch.request.temperature, batch.request.max_fix_iters) => result,
            _ = cancellation.changed() => Err(failure("teacher cancelled during generation; child exit required")),
        }
    }

    async fn wait_teacher_exit(
        &self,
        receipt: &crate::inference::llama_server::EngineRetirementReceipt,
    ) -> Result<(), CommandError> {
        let wait = async {
            let mut tick = tokio::time::interval(Duration::from_millis(100));
            loop {
                tick.tick().await;
                match self
                    .server
                    .observe_engine_retirement(receipt)
                    .map_err(|e| failure(e.to_string()))?
                {
                    EngineRetirementStatus::Exited => return Ok(()),
                    EngineRetirementStatus::Pending => {}
                }
            }
        };
        tokio::time::timeout(Duration::from_secs(10), wait)
            .await
            .map_err(|_| failure("actual child exit remains unconfirmed"))?
    }

    async fn cleanup_teacher_batch(&self, batch: &mut BatchState) -> Result<(), CommandError> {
        if let Some(borrower) = batch.borrower.as_mut() {
            let finish = async {
                let mut tick = tokio::time::interval(Duration::from_millis(100));
                loop {
                    tick.tick().await;
                    if borrower.finish().await? == EngineRetirementStatus::Exited {
                        return Ok::<_, CommandError>(());
                    }
                }
            };
            tokio::time::timeout(Duration::from_secs(10), finish)
                .await
                .map_err(|_| failure("private teacher exit remains unconfirmed"))??;
            batch.borrower.take();
        }
        let Some(session) = batch.session.as_ref() else {
            return Ok(());
        };
        let capture = batch
            .capture
            .as_ref()
            .ok_or_else(|| failure("restore capture missing"))?;
        let current = || self.intent.snapshot().revision == capture.intent_revision;
        if batch.phase == Phase::Checkpoint
            && self.server.owned_engine().as_ref() == Some(&capture.launch.identity)
        {
            if !current() || self.server.paging_recovery_required() {
                // No borrower started. Existing reconcile owns superseded intent
                // or quarantined-page recovery, with no false readiness claim.
                return Ok(());
            }
            if !session.was_suspended() {
                return Ok(());
            }
            self.server
                .resume_owned_checkpoint(session, &current)
                .await
                .map_err(|e| failure(e.to_string()))?;
            self.publish_teacher_restored(&batch.prior, capture.intent_revision);
            return Ok(());
        }
        let expected = session.attempt_identity();
        if self.server.owned_engine().as_ref() == Some(&expected) {
            self.server
                .retire_owned_engine(&expected)
                .await
                .map_err(|e| failure(e.to_string()))?;
        }
        self.wait_teacher_exit(&session.attempt_retirement())
            .await?;
        if batch.shutdown.load(Ordering::Acquire) {
            batch.outcome = Some(Err(failure(
                "teacher window interrupted by shutdown; original launch was not restored",
            )));
            self.publish_teacher_unavailable(
                "teacher window interrupted by shutdown; normal boot recovery required",
            );
            return Ok(());
        }
        if !current() {
            return Ok(());
        }
        batch.phase = Phase::Restoring;
        self.publish_teacher_unavailable("restoring original owned launch after teacher batch");
        let bytes = batch
            .restore_bytes
            .ok_or_else(|| failure("original serving footprint missing"))?;
        let mut board = self.resource_daemon.subscribe();
        let grant = async {
            // Require a subsequent authority publication after borrower exit;
            // never subtract captured residency or treat Exited as a grant.
            board
                .changed()
                .await
                .map_err(|_| failure("resource authority ended"))?;
            loop {
                if !current() {
                    return Err(failure("serving intent changed before restoration"));
                }
                let request = crate::resources::LeaseRequest {
                    consumer_id: SERVING_CONSUMER_ID.into(),
                    kind: serving_pool_kind(),
                    bytes,
                    ttl_ms: u64::MAX,
                    reclaim_policy: crate::resources::ReclaimPolicy::Pinned,
                };
                match self.resource_daemon.acquire_guarded(&request) {
                    Ok(grant) => return Ok(grant),
                    Err(crate::resources::LeaseError::InsufficientCapacity { .. }) => {
                        board
                            .changed()
                            .await
                            .map_err(|_| failure("resource authority ended"))?;
                    }
                    Err(error) => {
                        return Err(failure(format!("restore admission failed: {error:?}")))
                    }
                }
            }
        };
        let grant = tokio::time::timeout(Duration::from_secs(30), grant)
            .await
            .map_err(|_| failure("restore capacity remains unavailable"))??;
        self.server
            .restore_owned_session(session, Some(grant), &current)
            .await
            .map_err(|e| failure(e.to_string()))?;
        self.publish_teacher_restored(&batch.prior, capture.intent_revision);
        Ok(())
    }

    fn publish_teacher_restored(&self, prior: &ServingSnapshot, revision: u64) {
        let mut restored = prior.clone();
        restored.ready_verified_at_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .map(|d| d.as_millis() as u64);
        Self::emit_serving(self.bus.get(), &restored);
        let _ = self.serving_tx.send_replace(restored.clone());
        self.acknowledge_verified_target(&restored, revision);
    }
}
