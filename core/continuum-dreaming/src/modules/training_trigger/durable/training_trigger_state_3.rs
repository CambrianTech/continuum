use super::*;

impl TrainingTriggerState {

    pub(super) async fn resolve_in_bucket(
        self: Arc<Self>,
        key: &BucketKey,
        dispatch: Uuid,
        resolution: DispatchResolution,
        reason: String,
        resolved_by: Option<Uuid>,
    ) -> Result<ResolveReport, (&'static str, String)> {
        if !self
            .hydrate_bucket(key)
            .await
            .map_err(|e| ("RecoveryRequired", e))?
        {
            return Err((
                "RecoveryRequired",
                "training bucket recovery is still paging; retry the same resolution".into(),
            ));
        }
        let store = self
            .durable
            .require()
            .map_err(|e| ("PersistenceUnavailable", e))?;
        let active = self
            .active_dispatches
            .get(key)
            .map(|a| Arc::clone(a.value()))
            .filter(|a| entity_id(&a.intent.base).is_ok_and(|id| id == dispatch))
            .ok_or_else(|| {
                (
                    "Invalid",
                    format!("dispatch {dispatch} is not the bucket's active intent: it already finished, or another intent owns the bucket; nothing to settle"),
                )
            })?;
        if !matches!(
            active.intent.phase,
            DispatchPhase::Dispatching | DispatchPhase::RecoveryRequired { .. }
        ) {
            return Err((
                "Invalid",
                format!(
                    "dispatch {dispatch} is {:?}: only an intent recovery cannot confirm (Dispatching, RecoveryRequired) is settled by hand",
                    active.intent.phase
                ),
            ));
        }
        // The ledger is read to its end before any evidence is believed: a registration under
        // this dispatch contradicts `NotDispatched`, and names the job `Dispatched` must match.
        // From the intent's own recovery cursor: the prefix behind it was already scanned for
        // THIS intent by recovery, which recorded any malformed rows there once and moved on.
        let from = active.journal_cursor;
        let registered = tokio::task::spawn_blocking({
            #[cfg(test)]
            let board = self.test_job_board.clone();
            move || -> Result<Option<JobHandle>, String> {
                #[cfg(not(test))]
                let board = TrainingJobBoard::global();
                registration_from(&board, dispatch, from)
            }
        })
        .await
        .map_err(|e| ("PersistenceFailed", format!("training dispatch evidence read: {e}")))?
        .map_err(|e| ("RecoveryRequired", e))?;
        let examples = active.batch.examples.len() as u32;
        let id = entity_id(&active.intent.base).map_err(|e| ("PersistenceFailed", e))?;
        match resolution {
            DispatchResolution::NotDispatched | DispatchResolution::RetryUnconfirmed { .. } => {
                let mut receipt: Option<RetryDecision> = None;
                let note = if let DispatchResolution::RetryUnconfirmed { evidence } = &resolution {
                    if evidence.iter().all(|e| e.trim().is_empty()) {
                        return Err(("Invalid", "a retry of an unconfirmed dispatch names the evidence checked about retained results".into()));
                    }
                    if let Some(handle) = &registered {
                        return Err((
                            "Contradicted",
                            format!(
                                "job {} is registered under dispatch {dispatch}: a known job is never retried under its dispatch (recovery would re-adopt it); resolve it as dispatched with that job id, then genome/training-trigger/return returns what it didn't train",
                                handle.local_id
                            ),
                        ));
                    }
                    let (persona, trait_kind, base_model) = (key.persona_id, key.trait_kind.clone(), key.base_model.clone());
                    tokio::task::spawn_blocking({
                        #[cfg(test)]
                        let board = self.test_job_board.clone();
                        move || -> Result<(), (&'static str, String)> {
                            #[cfg(not(test))]
                            let board = TrainingJobBoard::global();
                            let residency = crate::paths::continuum_home()
                                .map(|home| crate::inference::engine_residency::store_path(&home))
                                .map_err(|e| ("PersistenceUnavailable", format!("no continuum home to read engine residency from, so quiescence can't be shown: {e}")))?;
                            let resident = crate::inference::engine_residency::all(&residency)
                                .map_err(|e| ("PersistenceFailed", format!("engine residency unreadable, so quiescence can't be shown: {e}")))?;
                            producers_quiet(&board, persona, &trait_kind, &base_model, &resident, |engine| engine.liveness())
                                .map_err(|why| ("Contradicted", why))
                        }
                    })
                    .await
                    .map_err(|e| ("PersistenceFailed", format!("training quiescence read: {e}")))??;
                    // The decision's receipt commits in ONE transaction with the intent it
                    // releases (below): a retry is never taken without its record, and a power
                    // loss keeps both or neither (Codex root on #5028). The record documents the
                    // decision; the fence above, not the record, keeps a second producer out.
                    receipt = Some(RetryDecision {
                        base: BaseEntity { id: retry_decision_id(dispatch, active.intent.base.version).to_string(), ..BaseEntity::for_new_record() },
                        dispatch_id: dispatch,
                        persona_id: key.persona_id,
                        trait_kind: key.trait_kind.clone(),
                        base_model: key.base_model.clone(),
                        evidence: evidence.clone(),
                        reason: reason.clone(),
                        resolved_by,
                        decided_at_ms: continuum_base::time::EpochMs::now().millis(), // a clock before 1970 stamps 0: the receipt still names its decision and evidence
                    });
                    "retried by operator decision, outcome unconfirmed"
                } else {
                    if let Some(handle) = registered {
                        return Err((
                            "Contradicted",
                            format!("the ledger registers job {} under dispatch {dispatch}: it WAS dispatched; settle it with that job id, not as not-dispatched", handle.local_id),
                        ));
                    }
                    "resolved by hand as not dispatched"
                };
                let mut intent = active.intent.clone();
                intent.phase = DispatchPhase::Retryable {
                    error: format!("{note}: {reason}"),
                };
                match &receipt {
                    Some(decision) => {
                        let decision_id = entity_id(&decision.base).map_err(|e| ("PersistenceFailed", e))?;
                        let operations = vec![
                            store.retry_decisions.create_op(decision_id, decision).map_err(|e| ("PersistenceFailed", e.to_string()))?,
                            store.dispatches.update_op(id, &intent).map_err(|e| ("PersistenceFailed", e.to_string()))?,
                        ];
                        crate::orm::store::commit(&store.adapter, operations)
                            .await
                            .map_err(|e| ("PersistenceFailed", format!("the retry decision and its intent didn't commit (neither did): {e}")))?;
                    }
                    None => store.dispatches.update(id, &intent).await.map_err(|e| ("PersistenceFailed", e.to_string()))?,
                }
                self.active_dispatches.insert(
                    key.clone(),
                    Arc::new(ActiveDispatch {
                        intent,
                        batch: Arc::clone(&active.batch),
                        journal_cursor: active.journal_cursor,
                    }),
                );
                continuum_base::probe!(
                    class = "training.trigger.dispatch_resolved",
                    persona = %key.persona_id,
                    trait_kind = %key.trait_kind,
                    dispatch = %dispatch,
                    resolution = "retryable",
                    note = note,
                    examples = examples as u64,
                    resolved_by = ?resolved_by,
                    reason = %reason,
                    "an intent recovery could not confirm was made retryable by hand, under its stated note; the next tick re-creates its job from the same batch"
                );
                Ok(ResolveReport {
                    dispatch_id: dispatch,
                    resolution: Resolved::Retryable,
                    examples,
                    job_id: None,
                })
            }
            DispatchResolution::Dispatched { job } => {
                if let Some(handle) = &registered {
                    if handle.local_id != job {
                        return Err((
                            "Contradicted",
                            format!("the ledger registers job {} under dispatch {dispatch}, not {job}", handle.local_id),
                        ));
                    }
                }
                let registration = tokio::task::spawn_blocking({
                    #[cfg(test)]
                    let board = self.test_job_board.clone();
                    move || {
                        #[cfg(not(test))]
                        let board = TrainingJobBoard::global();
                        board.registration(job)
                    }
                })
                .await
                .map_err(|e| ("PersistenceFailed", format!("training job board read: {e}")))?
                .ok_or_else(|| ("NotFound", format!("job {job} has no registration on this node's job ledger")))?;
                if registration.persona_id != key.persona_id
                    || registration.trait_kind != key.trait_kind
                    || registration.base_model != key.base_model
                {
                    return Err((
                        "Contradicted",
                        format!("job {job} belongs to another bucket ({}/{}), not this dispatch's", registration.persona_name, registration.trait_kind),
                    ));
                }
                if registration.trigger_dispatch_id.is_some_and(|d| d != dispatch) {
                    return Err((
                        "Contradicted",
                        format!("job {job} is registered under dispatch {}, not {dispatch}", registration.trigger_dispatch_id.unwrap_or(dispatch)), // unwrap_or: guarded by is_some_and on the line above; the fallback is unreachable
                    ));
                }
                let handle = tokio::task::spawn_blocking({
                    #[cfg(test)]
                    let board = self.test_job_board.clone();
                    move || {
                        #[cfg(not(test))]
                        let board = TrainingJobBoard::global();
                        board.registered_handle(job)
                    }
                })
                .await
                .map_err(|e| ("PersistenceFailed", format!("training job board read: {e}")))?
                .ok_or_else(|| ("NotFound", format!("job {job}'s registration names no provider handle; it cannot be adopted")))?;
                let provider = handle.provider_id.clone();
                self.finish_dispatch(key, &active, handle, provider)
                    .await
                    .map_err(|e| ("PersistenceFailed", e))?;
                continuum_base::probe!(
                    class = "training.trigger.dispatch_resolved",
                    persona = %key.persona_id,
                    trait_kind = %key.trait_kind,
                    dispatch = %dispatch,
                    resolution = "dispatched",
                    job = %job,
                    examples = examples as u64,
                    resolved_by = ?resolved_by,
                    reason = %reason,
                    "an intent recovery could not confirm was settled by hand as the job named; that job takes the normal path"
                );
                Ok(ResolveReport {
                    dispatch_id: dispatch,
                    resolution: Resolved::Dispatched,
                    examples,
                    job_id: Some(job),
                })
            }
        }
    }

    /// Withdraw pending submissions before they train (`genome/training-trigger/retire`).
    /// Only rows still pending are examined: by card, that is a bounded scan of the
    /// pending rows across every bucket (a card's dispatched history is not searched;
    /// a job's examples come back through `return`). Each touched bucket is handled
    /// under its own submit gate: the receipt is written first, the rows flip to
    /// retired, and the bucket cache is rebuilt from the rows, so a dispatch racing
    /// this call sees either the rows or their absence, never a half-flipped bucket.
    pub(crate) async fn retire(
        self: &Arc<Self>,
        selection: RetireSelection,
        reason: String,
        retired_by: Option<Uuid>,
    ) -> Result<RetireReport, (&'static str, String)> {
        self.require_ready()
            .map_err(|e| ("PersistenceUnavailable", e))?;
        let store = self
            .durable
            .require()
            .map_err(|e| ("PersistenceUnavailable", e))?;
        let mut report = RetireReport::default();
        // Which rows, grouped by the bucket that owns them.
        let mut by_key: std::collections::BTreeMap<(Uuid, String, String), Vec<Uuid>> =
            std::collections::BTreeMap::new();
        let card = match &selection {
            RetireSelection::Card(card) => Some(*card),
            RetireSelection::Submissions(_) => None,
        };
        match selection {
            RetireSelection::Submissions(ids) => {
                for id in ids {
                    match store
                        .submissions
                        .find_by_id(id)
                        .await
                        .map_err(|e| ("PersistenceFailed", e.to_string()))?
                    {
                        Some(row) => by_key
                            .entry((row.persona_id, row.trait_kind.clone(), row.base_model.clone()))
                            .or_default()
                            .push(id),
                        None => report.refused.push((
                            id,
                            None,
                            "no submission with this id on this node".into(),
                        )),
                    }
                }
            }
            RetireSelection::Card(card) => {
                for row in store
                    .pending_rows_on_card(card)
                    .await
                    .map_err(|e| ("PersistenceFailed", e))?
                {
                    let id = entity_id(&row.base).map_err(|e| ("PersistenceFailed", e))?;
                    by_key
                        .entry((row.persona_id, row.trait_kind.clone(), row.base_model.clone()))
                        .or_default()
                        .push(id);
                }
            }
        }
        for ((persona_id, trait_kind, base_model), ids) in by_key {
            let key = BucketKey {
                persona_id,
                trait_kind,
                base_model,
            };
            let reason = reason.clone();
            let part = self
                .run_owned(key, move |state, key| async move {
                    state.retire_in_bucket(&key, card, ids, reason, retired_by).await
                })
                .await
                .map_err(|e| ("RecoveryRequired", e))?
                .map_err(|e| ("PersistenceFailed", e))?;
            report.absorb(part);
        }
        Ok(report)
    }

    /// The gated half of [`Self::retire`] for one bucket: eligibility is judged on the
    /// rows as persisted NOW (a dispatch may have taken them since the selection),
    /// the receipt names exactly the rows that flip, and the cache is rebuilt from
    /// the rows afterwards. A crash after the receipt and before the last flip
    /// leaves rows a rerun of the same selection finishes under a second receipt,
    /// reporting the first's rows as `already_retired`.
    pub(super) async fn retire_in_bucket(
        self: Arc<Self>,
        key: &BucketKey,
        card: Option<Uuid>,
        ids: Vec<Uuid>,
        reason: String,
        retired_by: Option<Uuid>,
    ) -> Result<RetireReport, String> {
        if !self.hydrate_bucket(key).await? {
            return Err("training bucket recovery is still paging; retry the same retirement".into());
        }
        let store = self.durable.require()?;
        let mut report = RetireReport::default();
        let mut eligible = Vec::new();
        for id in ids {
            match store.submissions.find_by_id(id).await.map_err(|e| e.to_string())? {
                None => report
                    .refused
                    .push((id, None, "no submission with this id on this node".into())),
                Some(row) if row.key() != *key => report.refused.push((
                    id,
                    None,
                    "the submission belongs to another bucket".into(),
                )),
                Some(row) if row.retirement_id.is_some() => report.already_retired.push(id),
                Some(row) if !row.is_pending => report.refused.push((
                    id,
                    row.dispatch_id,
                    "already carried into a job: its examples come back through genome/training-trigger/return on that job".into(),
                )),
                Some(row) => eligible.push((id, row)),
            }
        }
        if eligible.is_empty() {
            return Ok(report);
        }
        let retirement_id = Uuid::new_v4();
        let examples: u32 = eligible
            .iter()
            .map(|(_, row)| row.batch.examples.len() as u32)
            .sum();
        let mut base = BaseEntity::for_new_record();
        base.id = retirement_id.to_string();
        let receipt = Retirement {
            base,
            card_id: card,
            persona_id: key.persona_id,
            trait_kind: key.trait_kind.clone(),
            base_model: key.base_model.clone(),
            submission_ids: eligible.iter().map(|(id, _)| *id).collect(),
            examples,
            reason: reason.clone(),
            retired_by,
            retired_at_ms: continuum_base::time::EpochMs::now().millis(), // a clock before 1970 stamps 0: the receipt still names its rows and reason
        };
        store
            .retirements
            .save(retirement_id, &receipt)
            .await
            .map_err(|e| e.to_string())?;
        // Withdrawn examples owe serving nothing: their demand (a parked example returned to her
        // bucket keeps it) is cleared before the rows flip, or it would reserve a window for
        // examples that will never train (card 4545eafd; FOCUS-CONTRACT: no stale demand). A
        // failure here leaves the rows pending under this receipt; rerunning finishes it.
        let withdrawn: Vec<String> = eligible
            .iter()
            .flat_map(|(_, row)| row.batch.examples.iter().map(crate::genome::fine_tuning::training_demand::TrainingDemand::key_of))
            .collect();
        crate::genome::fine_tuning::training_demand::TrainingDemand::for_persona(key.persona_id)
            .clear(&withdrawn)
            .map_err(|e| format!("retirement {retirement_id}: the withdrawn examples' demand could not be cleared ({e}); rerun the same retirement to finish"))?;
        for (id, mut row) in eligible {
            let count = row.batch.examples.len() as u32;
            row.is_pending = false;
            row.retirement_id = Some(retirement_id);
            store
                .submissions
                .update(id, &row)
                .await
                .map_err(|e| format!("submission {id} could not be retired after receipt {retirement_id} was written: {e}; rerun the same retirement to finish"))?;
            report.retired.push((id, count));
            report.examples_retired += count;
        }
        report.retirement_ids.push(retirement_id);
        // The cache is rebuilt from the rows, never edited in place.
        self.restore_pending_bucket(key).await?;
        continuum_base::probe!(
            class = "training.trigger.retired",
            persona = %key.persona_id,
            trait_kind = %key.trait_kind,
            card = ?card,
            retirement = %retirement_id,
            submissions = report.retired.len(),
            examples = report.examples_retired,
            reason = %reason,
            "pending submissions withdrawn before training; the next job will not see them"
        );
        Ok(report)
    }

}

/// The job registered under `dispatch`, read from `from` to the ledger's end (the whole
/// remainder, not one recovery page). A registration anywhere in it wins, even on a page
/// with malformed rows; a malformed row with no registration refuses, because that row
/// could have been it. Rows before `from` are the prefix recovery already scanned for this
/// intent (it records malformed rows once and advances the intent's cursor past them):
/// on the 5090, 2026-10-10, thirteen cargo-test fixture rows from September sat there and
/// refused every resolve that rescanned from 0.
pub(super) fn registration_from(board: &crate::genome::fine_tuning::TrainingJobBoard, dispatch: Uuid, from: u64) -> Result<Option<JobHandle>, String> {
    let mut cursor = from;
    loop {
        match board.lookup_trigger_dispatch(dispatch, cursor)? {
            DispatchLookup::Observed(handle) => return Ok(Some(handle)),
            DispatchLookup::Incomplete { next_offset } => cursor = next_offset,
            DispatchLookup::NotObserved => return Ok(None),
            DispatchLookup::Corrupt { observed: Some(handle), .. } => return Ok(Some(handle)),
            DispatchLookup::Corrupt { observed: None, malformed, .. } => {
                return Err(format!(
                    "{malformed} unreadable journal rows after this intent's recovery cursor ({cursor}): one of them could be its registration; repair the ledger before settling by hand"
                ))
            }
        }
    }
}

/// The id of the decision to retry `dispatch` from its intent at `version`: one decision per
/// state of the intent, so a resolve repeated against the same state can't record twice.
pub(super) fn retry_decision_id(dispatch: Uuid, version: u32) -> Uuid {
    Uuid::new_v5(&dispatch, format!("retry-unconfirmed:{version}").as_bytes())
}

/// QUIESCENCE of a bucket's producers: the fence a retry of an unconfirmed dispatch passes
/// before its receipt is committed (Codex root on #5028). A missing ledger row doesn't show that nothing is
/// still producing. Two write-ahead records can. This core's board holds a job in memory even
/// when its ledger append failed. And engine work is bound to its engine process (pid + start
/// time) BEFORE its /train is posted (engine_residency), so a record on this base whose engine
/// is Alive or Unknown fences WHATEVER the board says: a terminal ledger row is what this core
/// concluded about the job, not proof that the engine stopped stepping it. Only a VERIFIED-dead
/// engine releases its record; an Unknown one holds until it is retired by its owner.
pub(super) fn producers_quiet(
    board: &crate::genome::fine_tuning::TrainingJobBoard,
    persona: Uuid,
    trait_kind: &str,
    base_model: &str,
    resident: &[crate::inference::engine_residency::ResidentWork],
    liveness: impl Fn(&crate::inference::engine_residency::EngineIncarnation) -> crate::inference::engine_residency::Liveness,
) -> Result<(), String> {
    if let Some(job) = board
        .snapshot()
        .into_iter()
        .find(|j| j.persona_id == persona && j.trait_kind == trait_kind && j.base_model == base_model && !board.is_terminal(j.handle.local_id))
    {
        return Err(format!("job {} of this bucket is still in flight on this core: a producer may be running; retry once it has ended", job.handle.local_id));
    }
    for work in resident.iter().filter(|w| w.base_model == base_model) {
        let state = liveness(&work.engine);
        if state != crate::inference::engine_residency::Liveness::Dead {
            return Err(format!(
                "engine work {} on base {base_model} is bound to an engine that is {state:?} (pid {}): a producer may still be running, whatever the board concluded; retry once that engine is verified gone",
                work.job, work.engine.pid
            ));
        }
    }
    Ok(())
}

/// What `genome/training-trigger/resolve` settles an unconfirmed intent as. Owned, not Copy:
/// a retry of an unconfirmed outcome carries its evidence lines into its receipt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DispatchResolution {
    /// No job was ever registered under it: retry from the same batch.
    NotDispatched,
    /// NO job is registered under it, its outcome is UNCONFIRMED, and the operator decides to
    /// retry it anyway on the evidence they name about retained results. Asserts nothing about
    /// whether a job existed (Codex root on Sahar's 2b4e7272); the decision and its evidence are
    /// a [`RetryDecision`] receipt, committed together with the intent. A dispatch WITH a registered job is never
    /// retried under its own id (after a crash, recovery would re-adopt that registration:
    /// Astra on #5028): it is resolved as dispatched to that job, and `training-trigger/return`
    /// returns what it didn't train, through its artifact guards.
    RetryUnconfirmed { evidence: Vec<String> },
    /// This job is the one it made: adopt it.
    Dispatched { job: Uuid },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Resolved {
    Retryable,
    Dispatched,
}

impl From<Resolved> for String {
    fn from(r: Resolved) -> Self {
        match r {
            Resolved::Retryable => "retryable".into(),
            Resolved::Dispatched => "dispatched".into(),
        }
    }
}

#[derive(Debug)]
pub(crate) struct ResolveReport {
    pub(crate) dispatch_id: Uuid,
    pub(crate) resolution: Resolved,
    pub(crate) examples: u32,
    pub(crate) job_id: Option<Uuid>,
}

/// What [`TrainingTriggerState::retire`] withdraws.
#[derive(Debug, Clone)]
pub(crate) enum RetireSelection {
    /// Every pending submission whose examples were settled from this card.
    Card(Uuid),
    /// Exactly these submissions.
    Submissions(Vec<Uuid>),
}

/// The crate-internal result of a retirement; the verb projects it onto the wire.
#[derive(Debug, Default)]
pub(crate) struct RetireReport {
    pub(crate) retirement_ids: Vec<Uuid>,
    /// `(submission, examples it carried)`.
    pub(crate) retired: Vec<(Uuid, u32)>,
    pub(crate) already_retired: Vec<Uuid>,
    /// `(submission, the dispatch that carried it if that is why, why)`.
    pub(crate) refused: Vec<(Uuid, Option<Uuid>, String)>,
    pub(crate) examples_retired: u32,
}

impl RetireReport {
    pub(super) fn absorb(&mut self, other: RetireReport) {
        self.retirement_ids.extend(other.retirement_ids);
        self.retired.extend(other.retired);
        self.already_retired.extend(other.already_retired);
        self.refused.extend(other.refused);
        self.examples_retired += other.examples_retired;
    }
}

#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum OperationPoint {
    BeforeSchemaSetup,
    AfterMetadataScan,
    AcceptanceCommitted,
    BeforeRetryableCommit,
}

/// Pause the actual owner at a persistence boundary; no replacement storage or
/// dispatch implementation participates in cancellation regressions.
#[cfg(test)]
pub(in crate::modules::training_trigger) struct OperationPause {
    pub(super) point: OperationPoint,
    pub(super) entered: tokio::sync::Notify,
    pub(super) resume: tokio::sync::Notify,
}

#[cfg(test)]
impl TrainingTriggerState {
    pub(super) async fn pause_operation(&self, point: OperationPoint) {
        let pause = {
            let mut slot = self.operation_pause.lock().unwrap();
            if slot.as_ref().is_some_and(|pause| pause.point == point) {
                slot.take()
            } else {
                None
            }
        };
        if let Some(pause) = pause {
            pause.entered.notify_one();
            pause.resume.notified().await;
        }
    }
}
