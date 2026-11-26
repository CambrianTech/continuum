use super::*;

impl TrainingTriggerState {
    /// Admit before spawning: contending callers retain their own wait/payload,
    /// not a second queue of background tasks. Once admitted, the finite owner
    /// operation holds the lease through every storage/provider await. SQLite's
    /// blocking writer cannot outlive that lease when the caller disconnects.
    pub(crate) async fn run_owned<T, F, Fut>(
        self: &Arc<Self>,
        key: BucketKey,
        kind: super::super::OwnedKind,
        operation: F,
    ) -> Result<T, String>
    where
        T: Send + 'static,
        F: FnOnce(Arc<Self>, BucketKey) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = T> + Send + 'static,
    {
        let lease = self.submit_gates.acquire(&key).await;
        self.run_leased(lease, key, kind, operation).await
    }

    /// [`Self::run_owned`] for a caller that already holds the bucket's lease (taken by
    /// `try_acquire` while it held serving resources it would not wait holding).
    pub(crate) async fn run_leased<T, F, Fut>(
        self: &Arc<Self>,
        lease: crate::runtime::Lease<BucketKey>,
        key: BucketKey,
        kind: super::super::OwnedKind,
        operation: F,
    ) -> Result<T, String>
    where
        T: Send + 'static,
        F: FnOnce(Arc<Self>, BucketKey) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = T> + Send + 'static,
    {
        self.require_ready()?; // a bucket's operation needs recovered state; shutdown is rechecked under the lock
        let state = self.clone();
        let persona = Some(key.persona_id);
        Ok(self
            .run_operation(kind, persona, async move {
                let _lease = lease;
                operation(state, key).await
            })
            .await?)
    }

    /// Run `operation` as an owned operation: admitted only while the trigger is ready, held in
    /// `operations` so shutdown's join waits for it, named for its kind. What every owned operation
    /// runs through, a bucket's or the trainer's recovery pass, which holds no bucket (Codex on #6240:
    /// a pass outside `operations` let shutdown return while its storage read went on).
    pub(crate) async fn run_operation<T, Fut>(self: &Arc<Self>, kind: super::super::OwnedKind, persona: Option<Uuid>, operation: Fut) -> Result<T, OperationRefused>
    where
        T: Send + 'static,
        Fut: std::future::Future<Output = T> + Send + 'static,
    {
        let (send, receive) = tokio::sync::oneshot::channel();
        {
            let mut operations = self.operations.lock();
            // under the lock shutdown drains: admitted before it, or refused. Readiness is the caller's:
            // the recovery pass runs before it to retry an interrupted schema setup.
            if self.stopping.load(Ordering::Acquire) {
                return Err(OperationRefused::ShuttingDown);
            }
            while let Some(joined) = operations.try_join_next() {
                if let Err(error) = joined {
                    tracing::error!(%error, "training owner operation failed; durable recovery retains its work");
                }
            }
            operations.spawn(self.running.named(kind, persona, async move {
                let result = operation.await;
                let _ = send.send(result); // a disconnected caller does not cancel ownership
            }));
        }
        receive.await.map_err(|_| OperationRefused::Lost)
    }

    /// Admit no new owned operation, and count the ones still running.
    pub(crate) async fn stop_admitting(&self) -> usize {
        self.stopping.store(true, Ordering::Release);
        self.ready.store(false, Ordering::Release);
        let mut operations = self.operations.lock();
        while let Some(joined) = operations.try_join_next() {
            if let Err(error) = joined {
                tracing::error!(%error, "training owner operation failed; durable recovery retains its work");
            }
        }
        continuum_base::probe!(class = "training.shutdown.operations", at = "stop_admitting", in_flight = operations.len() as u64,
            running = ?self.running.now(), "the trainer admits nothing new; these operations still run");
        operations.len()
    }

    pub(crate) async fn drain_operations(&self) -> Result<(), String> {
        self.stopping.store(true, Ordering::Release);
        self.ready.store(false, Ordering::Release);
        // Keep the set in the owner if the runtime's shutdown deadline expires.
        // Dropping the shutdown wait does not abort still-running operations.
        // A running step can register a fence before it finishes. Release the set
        // between polls so the fence can join the same shutdown.
        let failure = super::super::shutdown_step("shutdown.join_operations", async {
            // read inside the step: a stall acquiring the set's lock is then named too
            let in_flight = self.operations.lock().len();
            continuum_base::probe!(class = "training.shutdown.operations", at = "join", in_flight = in_flight as u64,
                running = ?self.running.now(), "the trainer's shutdown waits for these operations still in flight");
            let mut failure = None;
            while let Some(joined) = std::future::poll_fn(|cx| self.operations.lock().poll_join_next(cx)).await {
                if let Err(error) = joined {
                    failure = Some(error.to_string());
                }
            }
            failure
        })
        .await;
        // nothing learns now: leave no seat held
        let released = super::super::shutdown_step("shutdown.let_go_seats", async { self.let_go_seats() }).await;
        continuum_base::probe!(class = "training.on_engine.stopped", released = released as u64, "the trigger stopped: every seat on the node engine was let go");
        failure.map_or(Ok(()), Err)
    }

    /// Every bucket holding examples not yet learned, read from the journal's pending rows.
    pub(crate) async fn pending_views(&self) -> Result<Vec<crate::commands::training_trigger::status::PendingBucketView>, String> {
        use crate::commands::training_trigger::status::PendingBucketView;
        let store = self.durable.require()?;
        let mut views: Vec<PendingBucketView> = Vec::new();
        let mut after = 0u64;
        loop {
            let page = store.arrivals(after, 256).await?;
            let full = page.len() == 256;
            for row in &page {
                let unlearned = (row.examples.len() as u32).saturating_sub(row.learned);
                match views.iter_mut().find(|v| v.persona_id == row.key.persona_id && v.trait_kind == row.key.trait_kind && v.base_model == row.key.base_model) {
                    Some(view) => view.examples_pending += unlearned,
                    None => views.push(PendingBucketView {
                        persona_id: row.key.persona_id,
                        persona_name: row.persona_name.clone(),
                        trait_kind: row.key.trait_kind.clone(),
                        base_model: row.key.base_model.clone(),
                        examples_pending: unlearned,
                    }),
                }
                after = after.max(row.sequence);
            }
            if !full {
                return Ok(views);
            }
        }
    }

    pub(crate) fn require_ready(&self) -> Result<(), String> {
        if self.stopping.load(Ordering::Acquire) {
            return Err("training-trigger is shutting down".into());
        }
        if !self.ready.load(Ordering::Acquire) {
            return Err("training-trigger durable state has not recovered".into());
        }
        Ok(())
    }


    pub(crate) async fn initialize_storage(
        &self,
        adapter: Arc<dyn StorageAdapter>,
    ) -> Result<(), String> {
        let _ = self.initial_adapter.set(adapter.clone());
        let _lease = self.initialization.lock().await;
        if self.ready.load(Ordering::Acquire) {
            return Ok(());
        }
        if self.stopping.load(Ordering::Acquire) {
            return Err("training-trigger is shutting down".into());
        }
        #[cfg(test)]
        self.pause_operation(OperationPoint::BeforeSchemaSetup)
            .await;
        let store = Arc::new(DurableStore::new(adapter).await?);
        let reclaimed = store.reclaim_dispatched().await?;
        if reclaimed > 0 {
            continuum_base::probe!(class = "training.trigger.dispatches_reclaimed", submissions = reclaimed as u64,
                "the batch path's open dispatches went back to pending; in-place learning takes them");
        }
        self.next_sequence
            .store(store.next_sequence().await?, Ordering::Relaxed);
        // Only schema + indexed metadata at boot. Payload hydration belongs to
        // bounded owner operations, never the runtime's finite init deadline.
        self.durable.install(store);
        if self.stopping.load(Ordering::Acquire) {
            return Err("training-trigger is shutting down".into());
        }
        self.ready.store(true, Ordering::Release);
        Ok(())
    }

    pub(crate) async fn ensure_storage(&self) -> Result<(), String> {
        if self.stopping.load(Ordering::Acquire) {
            return Err("training-trigger is shutting down".into());
        }
        if self.ready.load(Ordering::Acquire) {
            return Ok(());
        }
        if let Some(adapter) = self.initial_adapter.get() {
            return self.initialize_storage(adapter.clone()).await;
        }
        let data = self.data.require()?;
        self.initialize_storage(data.adapter("main").await?)
            .await
    }







    /// Caller holds the bucket gate. The independent ID gate prevents a changed
    /// request selecting another bucket from racing the same durable primary key.
    pub(crate) async fn accept(
        &self,
        key: &BucketKey,
        submission_id: Option<Uuid>,
        batch: PendingBatch,
    ) -> Result<AcceptanceReceipt, (&'static str, String)> {
        self.require_ready()
            .map_err(|e| ("PersistenceUnavailable", e))?;
        let store = self
            .durable
            .require()
            .map_err(|e| ("PersistenceUnavailable", e))?;
        let id = submission_id.unwrap_or_else(Uuid::new_v4); // a submission with no id is minted one here: an identity, never a quantity
        let _lease = self.acceptance_gates.acquire(&id).await;
        if let Some(existing) = store
            .submissions
            .find_by_id(id)
            .await
            .map_err(|e| ("PersistenceFailed", e.to_string()))?
        {
            if existing.key() != *key || !store.replay_matches(&existing, &batch).await.map_err(|e| ("RecoveryRequired", e))? {
                return Err((
                    "SubmissionConflict",
                    format!("submission {id} already binds different content or policy"),
                ));
            }
            return Ok(AcceptanceReceipt {
                submission_id: id,
                replayed: true,
            });
        }
        // a bucket's policy is its oldest pending submission's (first arrival wins)
        let first = store.pending_page(key, 0, 1).await.map_err(|e| ("PersistenceFailed", e))?;
        let pinned = match first.first() {
            Some(row) => Some(store.effective_batch(row).await.map_err(|e| ("RecoveryRequired", e))?),
            None => None,
        };
        if pinned.is_some_and(|pending| !pending.same_policy(&batch)) {
            return Err((
                "InconsistentBucket",
                "pending bucket has different source or training policy".into(),
            ));
        }
        let mut base = BaseEntity::for_new_record();
        base.id = id.to_string();
        let sequence = self
            .next_sequence
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| {
                (
                    "PersistenceFailed",
                    "training submission sequence exhausted".to_string(),
                )
            })?;
        let row = Submission {
            base,
            sequence,
            persona_id: key.persona_id,
            trait_kind: key.trait_kind.clone(),
            base_model: key.base_model.clone(),
            batch,
            is_pending: true,
            dispatch_id: None,
            retirement_id: None,
            learned: 0,
            carried_to: None,
            carried_from: None,
            carried_offset: 0,
        };
        if let Err(error) = store.submissions.save(id, &row).await {
            return Err(("PersistenceFailed", error.to_string()));
        }
        self.note_arrival(sequence);
        #[cfg(test)]
        self.pause_operation(OperationPoint::AcceptanceCommitted)
            .await;
        Ok(AcceptanceReceipt {
            submission_id: id,
            replayed: false,
        })
    }

    /// Take the trainer's arrivals output and say the current sequence at once: the trainer's first
    /// wake recovers whatever was already waiting.
    pub(crate) fn install_out(&self, mut out: super::super::trainer::TrainerOut) {
        out.arrived(self.next_sequence.load(Ordering::Relaxed));
        *self.out.lock() = Some(out);
    }

    /// Say something on the trainer's bus; nothing is said before a trainer is installed.
    pub(crate) fn say(&self, what: impl FnOnce(&mut super::super::trainer::TrainerOut)) {
        if let Some(out) = self.out.lock().as_mut() {
            what(out);
        }
    }

    /// Say that example `sequence` was accepted, waking the trainer; nothing is said before a
    /// trainer is installed.
    pub(crate) fn note_arrival(&self, sequence: u64) {
        self.say(|out| out.arrived(sequence));
    }

    /// How many examples bucket `key` holds pending and not yet learned.
    pub(crate) async fn unlearned_in(&self, key: &BucketKey) -> Result<u32, String> {
        self.durable.require()?.unlearned_in(key).await
    }

    /// What holds this bucket's learning: a gene of hers on trial for it, or her surprise too low
    /// to learn it. `None` = nothing holds it. ONE computation serves the gate and the receipt.
    pub(crate) fn held_for(&self, key: &BucketKey) -> Option<Took> {
        match self.trial_open_for(key) {
            Ok(Some(trial)) => Some(Took::Awaited { trial }),
            Ok(None) => unsurprised(key.persona_id).map(|s| Took::Unsurprised { s }),
            Err(()) => Some(Took::TrialFileUnreadable),
        }
    }

    /// The gene of hers that just landed for this bucket's `(persona, trait, base)` and is
    /// still settling (its id). The trial file is one small JSON read, bounded by the genes
    /// ever trialled. Past its settling window a gene holds nothing (`is_settling`); it
    /// stays in her head regardless. An UNREADABLE file holds the bucket, loudly: nothing
    /// dispatches beside a gene nobody can see (Cormac on #4794, point 5).
    pub(super) fn trial_open_for(&self, key: &BucketKey) -> Result<Option<Uuid>, ()> {
        #[cfg(not(test))]
        let trials = crate::genome::gene_trial::GeneTrials::default_store();
        #[cfg(test)]
        let trials = Some(crate::genome::gene_trial::GeneTrials::at(self.test_trials.path()));
        let Some(trials) = trials else {
            return Ok(None); // no home directory: no trial file can exist, nothing to hold
        };
        let all = match trials.load() {
            Ok(all) => all,
            Err(error) => {
                continuum_base::probe!(
                    class = "training.trigger.trial_file_unreadable",
                    persona = %key.persona_id,
                    trait_kind = %key.trait_kind,
                    error = %error,
                    "her trial file could not be read: the bucket holds until it can, nothing dispatches beside a trial nobody can see"
                );
                return Err(());
            }
        };
        let now_ms = continuum_base::time::EpochMs::now().millis(); // pre-epoch clock: every landed gene reads as settling, the conservative side
        Ok(all
            .iter()
            .find(|t| t.persona_id == key.persona_id && t.alias == key.trait_kind && t.base_model_id == key.base_model && t.is_settling(now_ms))
            .map(|t| t.id))
    }


}



/// Why an owned operation did not run to its result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum OperationRefused {
    #[error("training-trigger is shutting down")]
    ShuttingDown,
    #[error("training owner operation ended without a result; inspect durable status before retry")]
    Lost,
}

/// The bucket operations' existing text contract, at its one boundary.
impl From<OperationRefused> for String {
    fn from(refused: OperationRefused) -> Self {
        refused.to_string()
    }
}
