use super::*;

impl TrainingTriggerState {
    /// Admit before spawning: contending callers retain their own wait/payload,
    /// not a second queue of background tasks. Once admitted, the finite owner
    /// operation holds the lease through every storage/provider await. SQLite's
    /// blocking writer cannot outlive that lease when the caller disconnects.
    pub(crate) async fn run_owned<T, F, Fut>(
        self: &Arc<Self>,
        key: BucketKey,
        operation: F,
    ) -> Result<T, String>
    where
        T: Send + 'static,
        F: FnOnce(Arc<Self>, BucketKey) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = T> + Send + 'static,
    {
        let lease = self.submit_gates.acquire(&key).await;
        let mut operations = self.operations.lock().await;
        self.require_ready()?;
        while let Some(joined) = operations.try_join_next() {
            if let Err(error) = joined {
                tracing::error!(%error, "training owner operation failed; durable recovery retains its work");
            }
        }
        let (send, receive) = tokio::sync::oneshot::channel();
        let state = self.clone();
        operations.spawn(async move {
            let _lease = lease;
            let result = operation(state, key).await;
            let _ = send.send(result); // a disconnected caller does not cancel ownership
        });
        drop(operations);
        receive.await.map_err(|_| "training owner operation ended without a result; inspect durable status before retry".into())
    }

    pub(crate) async fn drain_operations(&self) -> Result<(), String> {
        self.stopping.store(true, Ordering::Release);
        self.ready.store(false, Ordering::Release);
        // Keep the set in the owner if the runtime's shutdown deadline expires.
        // Dropping the shutdown wait does not abort still-running operations.
        let mut operations = self.operations.lock().await;
        let mut failure = None;
        while let Some(joined) = operations.join_next().await {
            if let Err(error) = joined {
                failure = Some(error.to_string());
            }
        }
        failure.map_or(Ok(()), Err)
    }

    pub(crate) fn pending_views(
        &self,
    ) -> Vec<crate::commands::training_trigger::status::PendingBucketView> {
        use crate::commands::training_trigger::status::PendingBucketView;
        let view =
            |key: &BucketKey, batch: &PendingBatch, dispatch_id, dispatch| PendingBucketView {
                persona_id: key.persona_id,
                persona_name: batch.persona_name.clone(),
                trait_kind: key.trait_kind.clone(),
                base_model: key.base_model.clone(),
                examples_pending: batch.examples.len() as u32,
                min_examples: batch.min_examples,
                dispatch_id,
                dispatch,
            };
        let mut rows: Vec<_> = self
            .buckets
            .iter()
            .map(|row| view(row.key(), row.value(), None, None))
            .collect();
        rows.extend(self.active_dispatches.iter().map(|row| {
            view(
                row.key(),
                &row.batch,
                Uuid::parse_str(&row.intent.base.id).ok(),
                Some(row.intent.phase.clone()),
            )
        }));
        rows
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

    pub(crate) fn recovery_progress(&self) -> (bool, usize) {
        (
            self.initial_scan.load(Ordering::Acquire) == 3,
            self.hydrating.len(),
        )
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
        self.initialize_storage(data.get_adapter("main").await?)
            .await
    }

    /// Caller holds the admitted bucket lease. One page per operation; even a
    /// large pending bucket cannot monopolize boot or restart from page zero.
    pub(super) async fn hydrate_bucket(&self, key: &BucketKey) -> Result<bool, String> {
        if self.hydrated.contains(key) {
            return Ok(true);
        }
        let mut progress = self
            .hydrating
            .remove(key)
            .map(|(_, progress)| progress)
            .unwrap_or_default();
        let result = self.hydrate_page(key, &mut progress).await;
        if matches!(result, Ok(true)) {
            if let Some(active) = progress.cached_active {
                self.active_dispatches.insert(key.clone(), active);
            } else if let Some(mut intent) = progress.intent {
                let batch = progress
                    .active_batch
                    .ok_or("active dispatch has no examples")?;
                if matches!(intent.phase, DispatchPhase::Dispatching) {
                    intent.phase = DispatchPhase::RecoveryRequired {
                        error: "core stopped during dispatch; awaiting exact JobBoard evidence"
                            .into(),
                    };
                }
                self.active_dispatches.insert(
                    key.clone(),
                    Arc::new(ActiveDispatch {
                        intent,
                        batch: Arc::new(batch),
                        journal_cursor: 0,
                    }),
                );
            } else {
                self.active_dispatches.remove(key);
            }
            if let Some(batch) = progress.pending {
                self.buckets.insert(key.clone(), batch);
            } else {
                self.buckets.remove(key);
            }
            if self.buckets.contains_key(key) || self.active_dispatches.contains_key(key) {
                self.hydrated.insert(key.clone());
            } else {
                self.hydrated.remove(key);
            }
        } else {
            self.hydrating.insert(key.clone(), progress);
        }
        result
    }

    pub(super) async fn hydrate_page(
        &self,
        key: &BucketKey,
        progress: &mut BucketHydration,
    ) -> Result<bool, String> {
        let store = self.durable.require()?;
        if !progress.initialized {
            progress.cached_active = self.active_dispatches.get(key).map(|active| active.clone());
            if progress.cached_active.is_none() {
                progress.intent = store.active_intent(key).await?;
            }
            progress.initialized = true;
        }
        let mut remaining = 64;
        if let Some(intent) = &progress.intent {
            let end = (progress.active_index + 64).min(intent.submission_ids.len());
            for index in progress.active_index..end {
                let id = intent.submission_ids[index];
                let row = store
                    .submissions
                    .find_by_id(id)
                    .await
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| format!("dispatch lost submission {id}"))?;
                if row.key() != *key {
                    return Err(format!("dispatch has a foreign submission {id}"));
                }
                append_batch(&mut progress.active_batch, id, store.effective_batch(&row).await?)?;
                progress.active_index = index + 1;
                remaining -= 1;
            }
            if end < intent.submission_ids.len() {
                return Ok(false);
            }
        }
        if remaining == 0 {
            return Ok(false);
        }
        let rows = store
            .pending_page(key, progress.pending_after, remaining)
            .await?;
        let complete = rows.len() < remaining;
        for row in rows {
            let id = entity_id(&row.base)?;
            let sequence = row.sequence;
            // A Prepared intent already owns these IDs, even if assignment was
            // interrupted. Never append them a second time as pending successors.
            let intent = progress
                .intent
                .as_ref()
                .or_else(|| progress.cached_active.as_ref().map(|active| &active.intent));
            if !intent.is_some_and(|intent| intent.submission_ids.contains(&id)) {
                append_batch(&mut progress.pending, id, store.effective_batch(&row).await?)?;
            }
            progress.pending_after = sequence;
        }
        Ok(complete)
    }

    pub(super) fn append_pending(&self, key: BucketKey, id: Uuid, batch: PendingBatch) -> Result<(), String> {
        let mut current = self.buckets.remove(&key).map(|(_, batch)| batch);
        let result = append_batch(&mut current, id, batch);
        if let Some(current) = current {
            self.buckets.insert(key, current);
        }
        result
    }

    /// Reconstruct only the exact dispatched input of this canonical job. Engine jobs
    /// can end before writing request.json; an older submission's directory is not an
    /// alias for the newer job. Dispatch order and every submission's policy survive.
    pub(crate) async fn dispatched_request(
        &self,
        key: &BucketKey,
        dispatch: Uuid,
        job: Uuid,
    ) -> Result<crate::genome::fine_tuning::types::TrainingJobRequest, String> {
        self.require_ready()?;
        let store = self.durable.require()?;
        let intent = store.dispatches.find_by_id(dispatch).await.map_err(|e| e.to_string())?
            .ok_or_else(|| format!("job {job} has no durable dispatch {dispatch}"))?;
        if intent.key() != *key || !matches!(&intent.phase, DispatchPhase::Dispatched { handle, .. } if handle.local_id == job) {
            return Err(format!("dispatch {dispatch} does not prove ownership of job {job}"));
        }
        let mut batch = None;
        for id in &intent.submission_ids {
            let row = store.submissions.find_by_id(*id).await.map_err(|e| e.to_string())?
                .ok_or_else(|| format!("dispatch {dispatch} lost submission {id}"))?;
            if row.key() != *key || row.is_pending || row.dispatch_id != Some(dispatch) {
                return Err(format!("submission {id} does not belong to dispatch {dispatch}"));
            }
            append_batch(&mut batch, *id, store.effective_batch(&row).await?)?;
        }
        let batch = batch.ok_or_else(|| format!("dispatch {dispatch} has no submitted examples"))?;
        Ok(batch.training_request(key.persona_id, &key.trait_kind, &key.base_model))
    }

    /// Bridge the old return identity without accepting its examples twice on upgrade.
    /// A legacy row has no return-purpose tag: payload equality and a `returned` journal
    /// row cannot distinguish a real return from the old erroneous AlreadyAccepted.
    /// Only a dispatch receipt proving that row produced THIS job permits a new return.
    /// Caller holds the bucket gate; this is shared by manual and boot-orphan returns.
    pub(crate) async fn verify_return_ownership(
        &self,
        key: &BucketKey,
        job: Uuid,
        returned: Uuid,
    ) -> Result<(), (&'static str, String)> {
        let store = self.durable.require().map_err(|e| ("PersistenceUnavailable", e))?;
        if store.submissions.find_by_id(returned).await
            .map_err(|e| ("PersistenceFailed", e.to_string()))?.is_some() {
            return Ok(()); // accept still validates the immutable payload of this replay.
        }
        let Some(legacy) = store.submissions.find_by_id(job).await
            .map_err(|e| ("PersistenceFailed", e.to_string()))? else {
            return Ok(()); // No old acceptance exists to duplicate.
        };
        if legacy.key() == *key && !legacy.is_pending {
            if let Some(dispatch) = legacy.dispatch_id {
                if let Some(intent) = store.dispatches.find_by_id(dispatch).await
                    .map_err(|e| ("PersistenceFailed", e.to_string()))? {
                    if intent.key() == *key && intent.submission_ids.contains(&job)
                        && matches!(&intent.phase, DispatchPhase::Dispatched { handle, .. } if handle.local_id == job) {
                        return Ok(());
                    }
                }
            }
        }
        Err(("RecoveryRequired", format!(
            "legacy submission {job} may already own this returned batch; no examples appended. Inspect its training_trigger_submissions row and linked training_trigger_dispatches receipt to establish the original job ownership; payload equality or a returned journal entry alone is not proof"
        )))
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
        if !self
            .hydrate_bucket(key)
            .await
            .map_err(|error| ("RecoveryRequired", error))?
        {
            return Err((
                "RecoveryRequired",
                "training bucket recovery is still paging; no new submission accepted".into(),
            ));
        }
        let store = self
            .durable
            .require()
            .map_err(|e| ("PersistenceUnavailable", e))?;
        let id = submission_id.unwrap_or_else(Uuid::new_v4);
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
            if existing.is_pending && !self.contains_submission(key, id) {
                self.restore_pending_bucket(key)
                    .await
                    .map_err(|e| ("RecoveryRequired", e))?;
            }
            if !self.buckets.contains_key(key) && !self.active_dispatches.contains_key(key) {
                self.hydrated.remove(key);
            }
            return Ok(AcceptanceReceipt {
                submission_id: id,
                replayed: true,
            });
        }
        if self
            .buckets
            .get(key)
            .is_some_and(|pending| !pending.same_policy(&batch))
        {
            return Err((
                "InconsistentBucket",
                "pending bucket has different source or training policy".into(),
            ));
        }
        // A pending successor must also agree with the still-active batch; a
        // failed/uncertain dispatch must not silently repin its bucket's policy.
        if self
            .active_dispatches
            .get(key)
            .is_some_and(|active| !active.batch.same_policy(&batch))
        {
            return Err((
                "InconsistentBucket",
                "active dispatch has different training policy".into(),
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
        };
        if let Err(error) = store.submissions.save(id, &row).await {
            if !self.buckets.contains_key(key) && !self.active_dispatches.contains_key(key) {
                self.hydrated.remove(key);
            }
            return Err(("PersistenceFailed", error.to_string()));
        }
        #[cfg(test)]
        self.pause_operation(OperationPoint::AcceptanceCommitted)
            .await;
        // Persist first, then move the already-owned examples into the cache.
        self.append_pending(key.clone(), id, row.batch)
            .map_err(|e| ("RecoveryRequired", e))?;
        self.hydrated.insert(key.clone());
        Ok(AcceptanceReceipt {
            submission_id: id,
            replayed: false,
        })
    }

    pub(crate) fn ready_to_dispatch(&self, key: &BucketKey) -> bool {
        // A JOB OF HERS ALREADY TRAINING THIS COMPETENCE, or a gene of hers ON TRIAL for
        // it: the bucket keeps filling and waits; the fill after the job lands or the
        // trial decides judges against what came of it (Fork, Reuse, or a retired gene
        // never offered again), never a second mint beside it (ten Mints for one
        // competence on the 5090, 2026-10-05) and never a fork of a gene still being
        // judged. An active dispatch of this bucket still resumes.
        if !self.active_dispatches.contains_key(key) && self.held_for(key).is_some() {
            return false;
        }
        self.active_dispatches.contains_key(key)
            || self
                .buckets
                .get(key)
                .is_some_and(|batch| batch.examples.len() >= batch.min_examples as usize)
    }

    /// The training job for this bucket's `(persona, trait, base)` on the job board, if
    /// one is in flight: the job a held fill's examples wait for.
    pub(crate) fn job_in_flight_for(&self, key: &BucketKey) -> Option<Uuid> {
        #[cfg(not(test))]
        let jobs = crate::genome::fine_tuning::TrainingJobBoard::global().pending();
        #[cfg(test)]
        let jobs = self.test_job_board.pending();
        jobs.iter()
            .find(|j| j.persona_id == key.persona_id && j.trait_kind == key.trait_kind && j.base_model == key.base_model)
            .map(|j| j.handle.local_id)
    }

    /// What holds this bucket without a dispatch of its own: a job of hers in flight for
    /// the key, else a gene of hers on trial for it. `None` = nothing pending; the bucket
    /// dispatches when full. ONE computation serves the gate and the submit receipt.
    pub(crate) fn held_for(&self, key: &BucketKey) -> Option<Took> {
        if let Some(job) = self.job_in_flight_for(key) {
            return Some(Took::Joined { job });
        }
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

    pub(crate) fn contains_submission(&self, key: &BucketKey, id: Uuid) -> bool {
        self.buckets
            .get(key)
            .is_some_and(|b| b.submission_ids.contains(&id))
            || self
                .active_dispatches
                .get(key)
                .is_some_and(|a| a.intent.submission_ids.contains(&id))
    }

    /// Caller holds the existing per-bucket gate through all intent transitions.
    pub(crate) async fn dispatch_pending(&self, key: &BucketKey) -> DispatchResult {
        match self.dispatch_pending_inner(key).await {
            Ok(result) => result,
            Err(error) => DispatchResult::Failed {
                kind: "PersistenceFailed",
                error,
            },
        }
    }
}
