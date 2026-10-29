use super::*;

impl TrainingTriggerState {

    pub(super) async fn dispatch_pending_inner(&self, key: &BucketKey) -> Result<DispatchResult, String> {
        self.require_ready()?;
        if !self.hydrate_bucket(key).await? {
            return Ok(DispatchResult::Failed {
                kind: "RecoveryRequired",
                error: "training bucket recovery is still paging".into(),
            });
        }
        let store = self.durable.require()?;
        let active = if let Some(active) = self.active_dispatches.get(key) {
            Arc::clone(active.value())
        } else {
            let Some((_, batch)) = self.buckets.remove(key) else {
                self.hydrated.remove(key);
                return Ok(DispatchResult::Empty);
            };
            // A RUN THE LEARNING LOOP CAN WAIT FOR (training_rate): the oldest submissions whose
            // measured cost fits the run budget; the rest stay in the bucket, pending, for the next
            // run (their durable rows are not in this intent, so a restart re-hydrates them as
            // pending too). With no measured rate, the bucket's own threshold calibrates.
            let batch = match budget_split(batch, &key.base_model) {
                (taken, Some(rest)) => {
                    continuum_base::probe!(
                        class = "training.trigger.batch_budgeted",
                        persona = %key.persona_id,
                        trait_kind = %key.trait_kind,
                        taken = taken.examples.len() as u64,
                        pending = rest.examples.len() as u64,
                        budget_s = crate::genome::fine_tuning::training_rate::TRAINING_RUN_BUDGET.as_secs(),
                        "this run trains the oldest examples that fit the run budget at the measured rate; the rest wait for the next run"
                    );
                    self.buckets.insert(key.clone(), rest);
                    taken
                }
                (whole, None) => whole,
            };
            let intent = DispatchIntent {
                base: BaseEntity::for_new_record(),
                persona_id: key.persona_id,
                trait_kind: key.trait_kind.clone(),
                base_model: key.base_model.clone(),
                submission_ids: batch.submission_ids.clone(),
                phase: DispatchPhase::Prepared,
                is_active: true,
                attempt: None,
            };
            let active = Arc::new(ActiveDispatch {
                intent,
                batch: Arc::new(batch),
                journal_cursor: 0,
            });
            self.active_dispatches.insert(key.clone(), active.clone());
            active
        };
        // Publish intent ownership in memory before its first await. Cancellation
        // cannot leave this owner's examples absent or mint a second intent.
        let id = entity_id(&active.intent.base)?;
        if matches!(active.intent.phase, DispatchPhase::Prepared)
            && store
                .dispatches
                .find_by_id(id)
                .await
                .map_err(|e| e.to_string())?
                .is_none()
        {
            store
                .dispatches
                .save(id, &active.intent)
                .await
                .map_err(|e| e.to_string())?;
        }
        if matches!(active.intent.phase, DispatchPhase::Prepared) {
            store.assign(&active.intent).await?;
        }
        match &active.intent.phase {
            DispatchPhase::Dispatched { handle, provider } => {
                return self
                    .finish_dispatch(key, &active, handle.clone(), provider.clone())
                    .await;
            }
            // Finished at a restart: the batch was returned to the bucket when it was held.
            DispatchPhase::Held { .. } => {
                self.active_dispatches.remove(key);
                return Ok(DispatchResult::Empty);
            }
            DispatchPhase::Dispatching | DispatchPhase::RecoveryRequired { .. } => {
                let cursor = active.journal_cursor;
                #[cfg(test)]
                let board = self.test_job_board.clone();
                let lookup = tokio::task::spawn_blocking(move || {
                    #[cfg(not(test))]
                    let board = TrainingJobBoard::global();
                    board.lookup_trigger_dispatch(id, cursor)
                })
                .await
                .map_err(|e| format!("training dispatch evidence read: {e}"))??;
                match lookup {
                    DispatchLookup::Observed(handle) => {
                        let provider = handle.provider_id.clone();
                        return self.finish_dispatch(key, &active, handle, provider).await;
                    }
                    DispatchLookup::Incomplete { next_offset } => {
                        self.active_dispatches.insert(
                            key.clone(),
                            Arc::new(ActiveDispatch {
                                intent: active.intent.clone(),
                                batch: active.batch.clone(),
                                journal_cursor: next_offset,
                            }),
                        );
                    }
                    DispatchLookup::NotObserved => {
                        // Not on the board proves nothing: the board learns of a job only after
                        // create_job returns. An intent that names its attempt asks the attempt
                        // ledger, which closes the attempt before answering "not created"
                        // (card 55d4b9e3). A legacy intent (no attempt) stays unknown.
                        if let Some(attempt) = active.intent.attempt {
                            use super::super::attempt_evidence::AttemptEvidence;
                            match self.reconcile_attempt(attempt).await {
                                AttemptEvidence::Created(handle) => {
                                    // A created attempt is settled only after the board holds
                                    // its job, so the board missing it means the journal lost it.
                                    // Finishing would leave a real job unwatched: unknown instead.
                                    return Ok(DispatchResult::Failed {
                                        kind: "RecoveryRequired",
                                        error: format!(
                                            "dispatch {id}: attempt {attempt} created job {} but the board has no registration for it; not finished unwatched",
                                            handle.local_id
                                        ),
                                    });
                                }
                                AttemptEvidence::NotCreated => {
                                    let error = format!("dispatch {id}: attempt {attempt} reconciled as not created and closed; retried under a new attempt");
                                    let mut intent = active.intent.clone();
                                    intent.phase = DispatchPhase::Retryable { error: error.clone() };
                                    let active = Arc::new(ActiveDispatch {
                                        intent,
                                        batch: active.batch.clone(),
                                        journal_cursor: 0,
                                    });
                                    self.active_dispatches.insert(key.clone(), active.clone());
                                    store.dispatches.update(id, &active.intent).await.map_err(|e| e.to_string())?;
                                    return Ok(DispatchResult::Failed { kind: "DispatchFailed", error });
                                }
                                AttemptEvidence::Unknown(reason) => {
                                    return Ok(DispatchResult::Failed {
                                        kind: "RecoveryRequired",
                                        error: format!("dispatch {id}: {reason}; it was not repeated"),
                                    });
                                }
                            }
                        }
                    }
                    DispatchLookup::Corrupt {
                        observed,
                        malformed,
                        next_offset,
                    } => {
                        if let Some(handle) = observed {
                            let provider = handle.provider_id.clone();
                            return self.finish_dispatch(key, &active, handle, provider).await;
                        }
                        self.active_dispatches.insert(
                            key.clone(),
                            Arc::new(ActiveDispatch {
                                intent: active.intent.clone(),
                                batch: active.batch.clone(),
                                journal_cursor: next_offset,
                            }),
                        );
                        return Ok(DispatchResult::Failed {
                            kind: "RecoveryRequired",
                            error: format!("dispatch {id}: {malformed} unreadable journal rows; evidence remains uncertain, not repeated"),
                        });
                    }
                }
                return Ok(DispatchResult::Failed {
                    kind: "RecoveryRequired",
                    error: format!("dispatch {id} has no confirmed result; it was not repeated"),
                });
            }
            DispatchPhase::Parked { wait } => {
                let free = crate::genome::fine_tuning::engine_lora_adapter::governed_training_free(
                    "genome-train:wake",
                    crate::inference::llama_server::serving_pool_kind(), // a parked run waits for the resident lane's pool
                );
                let engine = crate::inference::engine_slots::live_lane_build();
                if !wait.wakes(free, engine.as_deref()) {
                    return Ok(DispatchResult::Empty);
                }
                continuum_base::probe!(
                    class = "training.dispatch.capacity_woke",
                    dispatch = %id,
                    window = u64::from(wait.window),
                    need_bytes = wait.need_bytes,
                    governed_free = free,
                    engine_changed = engine.as_deref() != wait.engine_stamp.as_deref(),
                    "a capacity-parked dispatch may re-plan: its need no longer rules it out, or the engine changed"
                );
            }
            DispatchPhase::Prepared | DispatchPhase::Retryable { .. } => {}
        }
        let mut intent = active.intent.clone();
        intent.phase = DispatchPhase::Dispatching;
        // A new attempt for every entry into Dispatching, written in the same durable update
        // below, before job-create is called (card 55d4b9e3).
        intent.attempt = Some(Uuid::new_v4());
        let active = Arc::new(ActiveDispatch {
            intent,
            batch: active.batch.clone(),
            journal_cursor: 0,
        });
        self.active_dispatches.insert(key.clone(), active.clone());
        if let Err(error) = store.dispatches.update(id, &active.intent).await {
            // This invocation has not called the provider. Keep the explicit safe
            // retry in memory; a crash with an uncertain write is conservative at boot.
            let mut intent = active.intent.clone();
            intent.phase = DispatchPhase::Retryable {
                error: error.to_string(),
            };
            self.active_dispatches.insert(
                key.clone(),
                Arc::new(ActiveDispatch {
                    intent,
                    batch: active.batch.clone(),
                    journal_cursor: 0,
                }),
            );
            return Err(error.to_string());
        }
        match self
            .dispatch_job_create(
                key.persona_id,
                &key.trait_kind,
                &key.base_model,
                &active.batch,
                id,
                active.intent.attempt,
            )
            .await
        {
            Ok(super::super::Created::Job(handle, provider)) => self.finish_dispatch(key, &active, handle, provider).await,
            Ok(super::super::Created::Held(took)) => self.finish_held(key, &active, took).await,
            Err(failure) => {
                let (phase, kind, error) = match failure {
                    DispatchFailure::Retryable(error) => (
                        DispatchPhase::Retryable {
                            error: error.clone(),
                        },
                        "DispatchFailed",
                        error,
                    ),
                    DispatchFailure::Uncertain(error) => (
                        DispatchPhase::RecoveryRequired {
                            error: error.clone(),
                        },
                        "RecoveryRequired",
                        error,
                    ),
                    DispatchFailure::Parked(wait) => {
                        let error = format!(
                            "parked for capacity: window {} needs {} bytes ({:?}), {} governed free",
                            wait.window, wait.need_bytes, wait.need_source, wait.governed_free
                        );
                        (DispatchPhase::Parked { wait }, "CapacityParked", error)
                    }
                };
                let mut intent = active.intent.clone();
                intent.phase = phase;
                let active = Arc::new(ActiveDispatch {
                    intent,
                    batch: active.batch.clone(),
                    journal_cursor: 0,
                });
                self.active_dispatches.insert(key.clone(), active.clone());
                #[cfg(test)]
                if matches!(active.intent.phase, DispatchPhase::Retryable { .. }) {
                    self.pause_operation(OperationPoint::BeforeRetryableCommit)
                        .await;
                }
                store
                    .dispatches
                    .update(id, &active.intent)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(DispatchResult::Failed { kind, error })
            }
        }
    }

    /// The fill was held without a job (joined a job of hers, awaited a trial, or adopted
    /// an existing gene for trial): the batch returns to the bucket whole (every
    /// submission id, every example), the intent ends as `Held` and is persisted, and the
    /// bucket waits (`ready_to_dispatch` is false while that job or trial is pending) for
    /// the fill that decides against what came of it.
    pub(super) async fn finish_held(&self, key: &BucketKey, active: &ActiveDispatch, took: Took) -> Result<DispatchResult, String> {
        let examples = active.batch.examples.len();
        let mut intent = active.intent.clone();
        intent.phase = DispatchPhase::Held { took: took.clone() };
        intent.is_active = false;
        self.durable
            .require()?
            .dispatches
            .update(entity_id(&intent.base)?, &intent)
            .await
            .map_err(|e| e.to_string())?;
        self.active_dispatches.remove(key);
        let returned = (*active.batch).clone();
        self.buckets.insert(key.clone(), returned);
        continuum_base::probe!(
            class = "training.trigger.held",
            persona = %key.persona_id,
            trait_kind = %key.trait_kind,
            took = ?took,
            examples = examples as u64,
            "the fill was held without a job (a job, a trial, or an existing gene pending) — its examples wait in the bucket"
        );
        Ok(DispatchResult::Held { examples, took })
    }

    pub(super) async fn finish_dispatch(
        &self,
        key: &BucketKey,
        active: &ActiveDispatch,
        handle: JobHandle,
        provider: String,
    ) -> Result<DispatchResult, String> {
        let mut intent = active.intent.clone();
        intent.phase = DispatchPhase::Dispatched {
            handle: handle.clone(),
            provider: provider.clone(),
        };
        intent.is_active = false;
        // Retain observed evidence in memory even if its final persistence fails.
        // On restart, Dispatching instead requires the exact JobBoard receipt.
        self.active_dispatches.insert(
            key.clone(),
            Arc::new(ActiveDispatch {
                intent: intent.clone(),
                batch: active.batch.clone(),
                journal_cursor: 0,
            }),
        );
        self.durable
            .require()?
            .dispatches
            .update(entity_id(&intent.base)?, &intent)
            .await
            .map_err(|e| e.to_string())?;
        self.active_dispatches.remove(key);
        if !self.buckets.contains_key(key) {
            self.hydrated.remove(key);
        }
        Ok(DispatchResult::Dispatched {
            examples: active.batch.examples.len(),
            handle,
            provider,
        })
    }

    /// Existing module cadence retries setup, discovers indexed metadata pages,
    /// and advances only bounded bucket recovery operations. No boot payload scan.
    pub(crate) async fn recover_tick(self: &Arc<Self>) -> Result<(), String> {
        let _recovery = self.recovery.lock().await;
        self.ensure_storage().await?;
        let mut candidates = self.recovery_heads().await?;
        #[cfg(test)]
        self.pause_operation(OperationPoint::AfterMetadataScan)
            .await;
        // Also revisit a cached intent whose final persistence returned an error.
        // Rotate a bounded slice; order never depends on DashMap iteration order.
        let mut cached: Vec<_> = self
            .active_dispatches
            .iter()
            .map(|row| row.key().clone())
            .chain(self.hydrating.iter().map(|row| row.key().clone()))
            .collect();
        cached.sort();
        cached.dedup();
        let after = self
            .cached_scan_after
            .lock()
            .map_err(|error| error.to_string())?
            .clone();
        let start = after
            .as_ref()
            .map_or(0, |after| cached.partition_point(|key| key <= after));
        let count = cached.len().min(64);
        for key in cached.iter().cycle().skip(start).take(count) {
            candidates.entry(key.clone()).or_default();
        }
        if count > 0 {
            *self
                .cached_scan_after
                .lock()
                .map_err(|error| error.to_string())? =
                Some(cached[(start + count - 1) % cached.len()].clone());
        }
        for (key, heads) in candidates {
            let report_key = key.clone();
            let result = self
                .run_owned(key, move |state, key| async move {
                    let missing = heads.into_iter().any(|(id, intent)| {
                        if intent {
                            state
                                .active_dispatches
                                .get(&key)
                                .is_none_or(|active| active.intent.base.id != id.to_string())
                        } else {
                            !state.contains_submission(&key, id)
                        }
                    });
                    if missing {
                        state.hydrated.remove(&key);
                    }
                    match state.hydrate_bucket(&key).await {
                        Ok(true) if state.ready_to_recover(&key) => {
                            state.dispatch_pending(&key).await
                        }
                        Ok(_) => DispatchResult::Empty,
                        Err(error) => DispatchResult::Failed {
                            kind: "RecoveryRequired",
                            error,
                        },
                    }
                })
                .await?;
            match result {
                DispatchResult::Failed { kind, error } => {
                    let said = format!("{kind}: {error}");
                    if refusal_is_news(&self.reported_recovery, &report_key, said) {
                        continuum_base::probe!(class = "training.recovery.pending", persona = %report_key.persona_id,
                            trait_kind = %report_key.trait_kind, kind, error = %error,
                            "accepted training remains owned by the durable trigger");
                    }
                }
                _ => {
                    self.reported_recovery.remove(&report_key);
                }
            }
        }
        Ok(())
    }

    /// Two indexed metadata pages, never historical examples. Cursors wrap after
    /// an exhausted page, so an earlier allocated sequence that committed later
    /// remains discoverable on the next pass.
    pub(super) async fn recovery_heads(
        &self,
    ) -> Result<std::collections::BTreeMap<BucketKey, Vec<(Uuid, bool)>>, String> {
        #[derive(Deserialize)]
        struct Head {
            #[serde(flatten)]
            key: BucketKey,
            sequence: Option<u64>,
        }
        let store = self.durable.require()?;
        let after = self.pending_scan_sequence.load(Ordering::Relaxed);
        let active_after = self
            .active_scan_after
            .lock()
            .map_err(|error| error.to_string())?
            .clone();
        let mut intents = QueryBuilder::new(DispatchIntent::COLLECTION)
            .filter_eq("isActive", true)
            .sort_asc("id")
            .limit(64)
            .select(vec![
                "personaId".into(),
                "traitKind".into(),
                "baseModel".into(),
            ]);
        if let Some(after) = active_after {
            intents = intents.filter("id", QueryOperator::Gt(after.into()));
        }
        let pending = QueryBuilder::new(Submission::COLLECTION)
            .filter_eq("isPending", true)
            .filter("sequence", QueryOperator::Gt(after.into()))
            .sort_asc("sequence")
            .limit(64)
            .select(vec![
                "personaId".into(),
                "traitKind".into(),
                "baseModel".into(),
                "sequence".into(),
            ]);
        let mut keys = std::collections::BTreeMap::<BucketKey, Vec<(Uuid, bool)>>::new();
        for (query, is_intent) in [(intents, true), (pending, false)] {
            let result = store.adapter.query(query.build()).await;
            if !result.success {
                return Err(result
                    .error
                    .unwrap_or_else(|| "training recovery metadata query refused".into()));
            }
            let rows = result
                .data
                .ok_or("training recovery metadata omitted rows")?;
            let full = rows.len() == 64;
            let mut last_id = None;
            let mut last_sequence = 0;
            for row in rows {
                let id = Uuid::parse_str(&row.id).map_err(|error| error.to_string())?;
                let head: Head =
                    serde_json::from_value(row.data).map_err(|error| error.to_string())?;
                if !is_intent {
                    last_sequence = head
                        .sequence
                        .ok_or("training pending metadata omitted sequence")?;
                }
                last_id = Some(row.id);
                keys.entry(head.key).or_default().push((id, is_intent));
            }
            if !full {
                self.initial_scan
                    .fetch_or(if is_intent { 1 } else { 2 }, Ordering::Release);
            }
            if is_intent {
                *self
                    .active_scan_after
                    .lock()
                    .map_err(|error| error.to_string())? = if full { last_id } else { None };
            } else {
                self.pending_scan_sequence
                    .store(if full { last_sequence } else { 0 }, Ordering::Relaxed);
            }
        }
        Ok(keys)
    }

    /// Rebuild a real cache miss through the same bounded hydration owner.
    pub(super) async fn restore_pending_bucket(&self, key: &BucketKey) -> Result<(), String> {
        self.hydrated.remove(key);
        if self.hydrate_bucket(key).await? {
            Ok(())
        } else {
            Err("training bucket recovery is still paging; retry the same submission ID".into())
        }
    }

    /// Settle a dispatch intent recovery cannot confirm (`genome/training-trigger/resolve`).
    /// Recovery confirms Dispatching / RecoveryRequired only by a job registered under the
    /// intent and otherwise refuses forever, which is right: it must never re-create a job
    /// that may exist. This is the operator's half of that contract, under the bucket's own
    /// gate, re-checking the whole ledger first so named evidence can never contradict it.
    /// `NotDispatched` makes the intent retryable (the next tick re-creates the job from the
    /// same batch); `Dispatched { job }` adopts a job the operator names. Journaled on the
    /// intent (the Retryable error text carries the reason) and on a probe.
    pub(crate) async fn resolve_dispatch(
        self: &Arc<Self>,
        dispatch: Uuid,
        resolution: DispatchResolution,
        reason: String,
        resolved_by: Option<Uuid>,
    ) -> Result<ResolveReport, (&'static str, String)> {
        self.require_ready()
            .map_err(|e| ("PersistenceUnavailable", e))?;
        let store = self
            .durable
            .require()
            .map_err(|e| ("PersistenceUnavailable", e))?;
        let intent = store
            .dispatches
            .find_by_id(dispatch)
            .await
            .map_err(|e| ("PersistenceFailed", e.to_string()))?
            .ok_or_else(|| ("NotFound", format!("dispatch {dispatch} is not on this node")))?;
        let key = intent.key();
        self.run_owned(key, move |state, key| async move {
            state.resolve_in_bucket(&key, dispatch, resolution, reason, resolved_by).await
        })
        .await
        .map_err(|e| ("RecoveryRequired", e))?
    }
}

/// Whether a bucket's refusal is news: recorded per bucket, an unchanged refusal is said once
/// and a changed one again. The caller clears a bucket's entry when it recovers, so a later
/// recurrence is news too.
pub(super) fn refusal_is_news<K: std::hash::Hash + Eq + Clone>(
    reported: &dashmap::DashMap<K, String>,
    key: &K,
    said: String,
) -> bool {
    reported.insert(key.clone(), said.clone()).as_ref() != Some(&said)
}
