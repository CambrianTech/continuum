use super::*;

#[derive(Debug)]
pub(crate) enum DispatchFailure {
    Retryable(String),
    Uncertain(String),
    /// Certain, and created nothing: no room for the smallest window that trains one of her
    /// examples, by measurement. The intent keeps its batch, parked until the wait wakes.
    Parked(crate::genome::fine_tuning::types::CapacityWait),
}

pub(crate) enum DispatchResult {
    Empty,
    /// The fill was held without a job (joined a job, awaited a trial, or adopted an
    /// existing gene for trial); its examples stay in the bucket for the next fill.
    Held { examples: usize, took: Took },
    Dispatched {
        examples: usize,
        handle: JobHandle,
        provider: String,
    },
    Failed {
        kind: &'static str,
        error: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
#[serde(rename_all = "camelCase")]
#[entity(collection = "training_trigger_submissions")]
#[entity(index(name = "idx_training_submission_bucket", fields = ["personaId", "traitKind", "baseModel", "isPending", "sequence"]))]
#[entity(index(name = "idx_training_submission_pending", fields = ["isPending", "sequence"]))]
pub(in crate::modules::training_trigger) struct Submission {
    #[serde(flatten)]
    pub(super) base: BaseEntity,
    /// Destination-local accepted order, restored from the indexed maximum.
    #[entity(indexed)]
    pub(super) sequence: u64,
    pub(super) persona_id: Uuid,
    pub(super) trait_kind: String,
    pub(super) base_model: String,
    #[entity(json)]
    pub(super) batch: PendingBatch,
    pub(super) is_pending: bool,
    pub(super) dispatch_id: Option<Uuid>,
    /// Withdrawn before any dispatch by `genome/training-trigger/retire`: the
    /// [`Retirement`] receipt that names why. Such a row is never pending again:
    /// the producer's retry of the same submission id replays as accepted and
    /// appends nothing (see [`TrainingTriggerState::accept`]). Absent on every
    /// row written before the verb existed.
    #[serde(default)]
    pub(super) retirement_id: Option<Uuid>,
}

/// The journaled withdrawal of bucket-pending submissions: what was pulled back
/// before it could train, and why. One row per `retire` call; the rows it covers
/// point back at it through `Submission::retirement_id`. Written BEFORE the rows
/// flip, so a crash between the two leaves a receipt that names rows a rerun
/// finishes (a rerun of the same selection reports them `already_retired`).
#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
#[serde(rename_all = "camelCase")]
#[entity(collection = "training_trigger_retirements")]
#[entity(index(name = "idx_training_retirement_card", fields = ["cardId"]))]
pub(in crate::modules::training_trigger) struct Retirement {
    #[serde(flatten)]
    pub(super) base: BaseEntity,
    /// The card whose settled turns were withdrawn, when the selection was by card.
    #[entity(indexed)]
    pub(super) card_id: Option<Uuid>,
    /// The bucket the rows left.
    pub(super) persona_id: Uuid,
    pub(super) trait_kind: String,
    pub(super) base_model: String,
    /// Every submission this receipt withdrew, in the order they were flipped.
    pub(super) submission_ids: Vec<Uuid>,
    /// Examples those submissions carried: what the next job will NOT train on.
    pub(super) examples: u32,
    pub(super) reason: String,
    /// The caller's peer id, when the command carried one.
    pub(super) retired_by: Option<Uuid>,
    pub(super) retired_at_ms: u64,
}

/// An operator's decision to RETRY a training dispatch whose outcome recovery could not
/// confirm, with the evidence they checked about retained results: a receipt, like
/// [`Retirement`], committed in ONE transaction with the intent it releases. It asserts nothing
/// about whether a job existed (Codex root on Sahar's 2b4e7272); it records who decided, why,
/// and on what evidence.
#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
#[serde(rename_all = "camelCase")]
#[entity(collection = "training_trigger_retry_decisions")]
#[entity(index(name = "idx_training_retry_dispatch", fields = ["dispatchId"]))]
pub(in crate::modules::training_trigger) struct RetryDecision {
    #[serde(flatten)]
    pub(super) base: BaseEntity,
    #[entity(indexed)]
    pub(super) dispatch_id: Uuid,
    pub(super) persona_id: Uuid,
    pub(super) trait_kind: String,
    pub(super) base_model: String,
    pub(super) evidence: Vec<String>,
    pub(super) reason: String,
    /// The caller's peer id, when the command carried one.
    pub(super) resolved_by: Option<Uuid>,
    pub(super) decided_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "state", rename_all = "snake_case")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/training_trigger/DispatchPhase.ts"
)]
pub enum DispatchPhase {
    Prepared,
    Dispatching,
    Retryable { error: String },
    RecoveryRequired { error: String },
    Dispatched { handle: JobHandle, provider: String },
    /// The fill was held without a job (a job of hers already training this competence,
    /// a trial judging it, or an existing gene adopted for trial): nothing was created,
    /// the batch went back into the bucket, and this intent is finished.
    Held { took: Took },
    /// No job: the smallest window that trains one of her examples is measured not to fit the
    /// governed training memory (card de9e1342). The intent stays active and keeps owning its
    /// submissions (a crash or retry neither duplicates nor drops a row); the recovery tick
    /// re-plans it only when `wait.wakes(..)`: capacity no longer rules it out, or the engine
    /// changed. Not failed, not trained, not left for a human.
    Parked { wait: crate::genome::fine_tuning::types::CapacityWait },
}

#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
#[serde(rename_all = "camelCase")]
#[entity(collection = "training_trigger_dispatches")]
#[entity(index(name = "idx_training_dispatch_bucket", fields = ["personaId", "traitKind", "baseModel", "isActive"]))]
#[entity(index(name = "idx_training_dispatch_active", fields = ["isActive", "id"]))]
pub(in crate::modules::training_trigger) struct DispatchIntent {
    #[serde(flatten)]
    pub(super) base: BaseEntity,
    pub(super) persona_id: Uuid,
    pub(super) trait_kind: String,
    pub(super) base_model: String,
    pub(super) submission_ids: Vec<Uuid>,
    #[entity(json)]
    pub(super) phase: DispatchPhase,
    pub(super) is_active: bool,
    /// The attempt this intent is in (card 55d4b9e3): minted durably when it enters
    /// `Dispatching`, BEFORE job-create runs, so recovery can ask the selection owner about
    /// exactly this attempt instead of inferring from a board that only learns of a job after
    /// `create_job` returns. `None` = written before attempts existed: its outcome is unknown,
    /// and a stranded one is surfaced as unknown, never auto-retried.
    pub(super) attempt: Option<Uuid>,
}

pub(in crate::modules::training_trigger) struct ActiveDispatch {
    pub(super) intent: DispatchIntent,
    pub(in crate::modules::training_trigger) batch: Arc<PendingBatch>,
    pub(super) journal_cursor: u64,
}

pub(in crate::modules::training_trigger) struct DurableStore {
    pub(super) submissions: OrmStore<Submission>,
    pub(super) dispatches: OrmStore<DispatchIntent>,
    pub(super) retirements: OrmStore<Retirement>,
    pub(super) retry_decisions: OrmStore<RetryDecision>,
    pub(super) policy_corrections: OrmStore<policy::PolicyCorrection>,
    pub(super) adapter: Arc<dyn StorageAdapter>,
}

impl DurableStore {
    pub(super) async fn new(adapter: Arc<dyn StorageAdapter>) -> Result<Self, String> {
        Ok(Self {
            submissions: OrmStore::new(adapter.clone())
                .await
                .map_err(|e| e.to_string())?,
            dispatches: OrmStore::new(adapter.clone())
                .await
                .map_err(|e| e.to_string())?,
            retirements: OrmStore::new(adapter.clone())
                .await
                .map_err(|e| e.to_string())?,
            retry_decisions: OrmStore::new(adapter.clone())
                .await
                .map_err(|e| e.to_string())?,
            policy_corrections: OrmStore::new(adapter.clone()).await.map_err(|e| e.to_string())?,
            adapter,
        })
    }

    pub(super) async fn next_sequence(&self) -> Result<u64, String> {
        let result = self
            .adapter
            .query(
                QueryBuilder::new(Submission::COLLECTION)
                    .sort_desc("sequence")
                    .limit(1)
                    .select(vec!["sequence".into()])
                    .build(),
            )
            .await;
        if !result.success {
            return Err(result
                .error
                .unwrap_or_else(|| "training sequence query refused".into())); // The query explicitly failed; missing diagnostics still return an error.
        }
        let rows = result.data.ok_or("training sequence query omitted rows")?;
        let last = rows
            .first()
            .map(|row| {
                row.data
                    .get("sequence")
                    .and_then(|s| s.as_u64())
                    .ok_or("training submission has invalid sequence")
            })
            .transpose()?
            .unwrap_or(0); // A successful empty table starts at sequence one; malformed stored sequences already return Err above.
        last.checked_add(1)
            .ok_or_else(|| "training submission sequence exhausted".into())
    }

    pub(super) async fn pending_page(
        &self,
        key: &BucketKey,
        after: u64,
        limit: usize,
    ) -> Result<Vec<Submission>, String> {
        let result = self
            .adapter
            .query(
                key.query(Submission::COLLECTION)
                    .filter_eq("isPending", true)
                    .filter("sequence", QueryOperator::Gt(after.into()))
                    .sort_asc("sequence")
                    .limit(limit)
                    .build(),
            )
            .await;
        if !result.success {
            return Err(result
                .error
                .unwrap_or_else(|| "training pending query refused".into())); // The query explicitly failed; this supplies its missing diagnostic, never rows.
        }
        result
            .data
            .ok_or("training pending query omitted rows")?
            .into_iter()
            .map(|row| serde_json::from_value(row.data).map_err(|error| error.to_string())) // Decode persisted ORM rows into the typed submission owner; no transcript re-encoding.
            .collect()
    }

    /// Every pending submission, in any bucket, whose examples carry this card: the
    /// producer stamps `metadata.cardId` on each example it settles from a card. Pages
    /// the pending index (bounded by what is waiting to train, never by history).
    pub(super) async fn pending_rows_on_card(&self, card: Uuid) -> Result<Vec<Submission>, String> {
        const PAGE: usize = 256;
        let card = card.to_string();
        let mut after = 0u64;
        let mut found = Vec::new();
        loop {
            let result = self
                .adapter
                .query(
                    QueryBuilder::new(Submission::COLLECTION)
                        .filter_eq("isPending", true)
                        .filter("sequence", QueryOperator::Gt(after.into()))
                        .sort_asc("sequence")
                        .limit(PAGE)
                        .build(),
                )
                .await;
            if !result.success {
                return Err(result
                    .error
                    .unwrap_or_else(|| "training pending query refused".into())); // The query explicitly failed; this supplies its missing diagnostic, never rows.
            }
            let rows = result.data.ok_or("training pending query omitted rows")?;
            let page = rows.len();
            for row in rows {
                let row: Submission = serde_json::from_value(row.data).map_err(|error| error.to_string())?; // Decode persisted ORM rows into the typed submission owner; no transcript re-encoding.
                after = after.max(row.sequence);
                let on_card = row.batch.examples.iter().any(|example| {
                    example
                        .metadata
                        .as_ref()
                        .and_then(|m| m.get("cardId"))
                        .and_then(|c| c.as_str())
                        .is_some_and(|c| c == card)
                });
                if on_card {
                    found.push(row);
                }
            }
            if page < PAGE {
                return Ok(found);
            }
        }
    }

    pub(super) async fn active_intent(&self, key: &BucketKey) -> Result<Option<DispatchIntent>, String> {
        let result = self
            .adapter
            .query(
                key.query(DispatchIntent::COLLECTION)
                    .filter_eq("isActive", true)
                    .limit(2)
                    .build(),
            )
            .await;
        if !result.success {
            return Err(result
                .error
                .unwrap_or_else(|| "training intent query refused".into())); // Preserve the failed query when its adapter omitted a diagnostic.
        }
        let mut rows = result.data.ok_or("training intent query omitted rows")?;
        if rows.len() > 1 {
            return Err("multiple active training dispatches own the same bucket".into());
        }
        rows.pop()
            .map(|row| serde_json::from_value(row.data).map_err(|error| error.to_string())) // Decode the existing ORM persisted-row format into its typed dispatch intent.
            .transpose()
    }

    /// Assignment follows the durable intent. Partial completion is harmless:
    /// replay checks each ID, and never steals one from another dispatch.
    pub(super) async fn assign(&self, intent: &DispatchIntent) -> Result<(), String> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct LinkageState {
            #[serde(flatten)]
            key: BucketKey,
            is_pending: bool,
            dispatch_id: Option<Uuid>,
        }
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Linkage {
            is_pending: bool,
            dispatch_id: Uuid,
        }
        let dispatch_id = entity_id(&intent.base)?;
        for id in &intent.submission_ids {
            let query = QueryBuilder::new(Submission::COLLECTION)
                .filter_eq("id", id.to_string())
                .select(vec![
                    "personaId".into(),
                    "traitKind".into(),
                    "baseModel".into(),
                    "isPending".into(),
                    "dispatchId".into(),
                ])
                .limit(1)
                .build();
            let result = self.adapter.query(query).await;
            if !result.success {
                let error = result
                    .error
                    .unwrap_or_else(|| "training linkage query refused".into()); // Failed query; retain an error when adapter detail is absent.
                return Err(error);
            }
            let mut rows = result.data.ok_or("training linkage query omitted rows")?;
            let data = rows
                .pop()
                .ok_or_else(|| format!("dispatch {dispatch_id} lost submission {id}"))?
                .data;
            let row: LinkageState = serde_json::from_value(data) // Decode persisted linkage columns; no example payload is fetched.
                .map_err(|e| e.to_string())?;
            if row.key != intent.key() || row.dispatch_id.is_some_and(|old| old != dispatch_id) {
                return Err(format!(
                    "dispatch {dispatch_id} does not own submission {id}"
                ));
            }
            if row.is_pending || row.dispatch_id.is_none() {
                self.submissions
                    .update_fields(
                        *id,
                        &Linkage {
                            is_pending: false,
                            dispatch_id,
                        },
                    )
                    .await
                    .map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }
}

impl Submission {
    pub(super) fn key(&self) -> BucketKey {
        BucketKey {
            persona_id: self.persona_id,
            trait_kind: self.trait_kind.clone(),
            base_model: self.base_model.clone(),
        }
    }
}

impl DispatchIntent {
    pub(super) fn key(&self) -> BucketKey {
        BucketKey {
            persona_id: self.persona_id,
            trait_kind: self.trait_kind.clone(),
            base_model: self.base_model.clone(),
        }
    }
}

impl BucketKey {
    pub(super) fn query(&self, collection: &str) -> QueryBuilder {
        QueryBuilder::new(collection)
            .filter_eq("personaId", self.persona_id.to_string())
            .filter_eq("traitKind", self.trait_kind.clone())
            .filter_eq("baseModel", self.base_model.clone())
    }
}

/// Incomplete per-key recovery stays owned across bounded pages. Commands never
/// accept new policy or dispatch until this key has a complete coherent cache.
#[derive(Default)]
pub(in crate::modules::training_trigger) struct BucketHydration {
    pub(super) initialized: bool,
    pub(super) intent: Option<DispatchIntent>,
    pub(super) cached_active: Option<Arc<ActiveDispatch>>,
    pub(super) active_batch: Option<PendingBatch>,
    pub(super) active_index: usize,
    pub(super) pending: Option<PendingBatch>,
    pub(super) pending_after: u64,
}

pub(super) fn entity_id(base: &BaseEntity) -> Result<Uuid, String> {
    Uuid::parse_str(&base.id).map_err(|e| format!("invalid training entity identity: {e}"))
}

impl PendingBatch {
    /// The immutable request a dispatch sends, also used to recover a native job
    /// which ended before its adapter wrote any files.
    pub(crate) fn training_request(&self, persona_id: Uuid, trait_kind: &str, base_model: &str) -> crate::genome::fine_tuning::types::TrainingJobRequest {
        crate::genome::fine_tuning::types::TrainingJobRequest {
            persona_id,
            persona_name: self.persona_name.clone(),
            base_model: base_model.to_string(),
            trait_kind: trait_kind.to_string(),
            resume_from: None,
            parent: None,
            dataset: crate::genome::fine_tuning::types::TrainingDataset {
                examples: self.examples.clone(), source: self.source, validation_split: self.validation_split,
            },
            eval_set: self.eval_set.clone(),
            lora: self.lora.clone(),
            schedule: self.schedule.clone(),
            local_artifact_dir: self.local_artifact_dir.clone(),
        }
    }
    /// THE fields a bucket pins at first arrival and every later batch must agree on.
    pub(crate) fn policy(&self) -> super::super::BucketPolicy {
        super::super::BucketPolicy {
            source: self.source.clone(),
            lora: self.lora.clone(),
            schedule: self.schedule.clone(),
            validation_split: self.validation_split,
            local_artifact_dir: self.local_artifact_dir.clone(),
            preferred_provider: self.preferred_provider.clone(),
            eval_set: self.eval_set.clone(),
        }
    }

    pub(crate) fn same_policy(&self, other: &Self) -> bool {
        self.policy() == other.policy()
    }

    pub(super) fn same_submission(&self, other: &Self) -> bool {
        self.same_policy(other)
            && self.persona_name == other.persona_name
            && self.min_examples == other.min_examples
            && self.examples.len() == other.examples.len()
            && self.examples.iter().zip(&other.examples).all(|(a, b)| {
                a.prompt == b.prompt && a.completion == b.completion && a.metadata == b.metadata
            })
    }
}

/// Split a bucket for one run: `(taken, rest)`, `rest` `None` when the whole batch is the run.
/// Takes whole submissions, OLDEST first: the drain, so under steady inflow every submission is
/// eventually trained (Cormac on #4926). A bucket whose per-submission counts do not cover its
/// examples (a shape this core did not assemble) is never split: it trains whole.
pub(super) fn budget_split(batch: PendingBatch, base_model: &str) -> (PendingBatch, Option<PendingBatch>) {
    use crate::genome::fine_tuning::training_rate::{default_epochs, example_chars, oldest_within_budget, TrainingRates, TRAINING_RUN_BUDGET};
    let counts = &batch.submission_examples;
    if counts.len() != batch.submission_ids.len() || counts.iter().map(|&c| c as usize).sum::<usize>() != batch.examples.len() {
        return (batch, None);
    }
    let epochs = batch.schedule.as_ref().map_or_else(default_epochs, |s| s.epochs);
    let take = match TrainingRates::in_home().and_then(|r| r.secs_per_char_epoch(base_model)) {
        Some(rate) => {
            let mut offset = 0;
            let chars: Vec<u64> = counts
                .iter()
                .map(|&c| {
                    let end = offset + c as usize;
                    let sum = batch.examples[offset..end].iter().map(example_chars).sum();
                    offset = end;
                    sum
                })
                .collect();
            oldest_within_budget(&chars, rate, epochs, TRAINING_RUN_BUDGET)
        }
        // no measured rate: a calibration run of the bucket's own threshold, oldest first
        None => {
            let mut examples = 0usize;
            let take = counts.iter().take_while(|&&c| {
                let under = examples < batch.min_examples as usize;
                examples += c as usize;
                under
            }).count();
            (take < counts.len()).then_some(take)
        }
    };
    let Some(take) = take.filter(|&t| t > 0 && t < counts.len()) else {
        return (batch, None);
    };
    let split_at: usize = counts[..take].iter().map(|&c| c as usize).sum();
    let mut taken = batch;
    let mut rest = taken.clone();
    rest.submission_ids = taken.submission_ids.split_off(take);
    rest.submission_examples = taken.submission_examples.split_off(take);
    rest.submission_returned = taken.submission_returned.split_off(take.min(taken.submission_returned.len()));
    taken.returned = taken.submission_returned.contains(&true);
    rest.returned = rest.submission_returned.contains(&true);
    rest.examples = taken.examples.split_off(split_at);
    (taken, Some(rest))
}

pub(super) fn append_batch(
    current: &mut Option<PendingBatch>,
    id: Uuid,
    mut incoming: PendingBatch,
) -> Result<(), String> {
    if let Some(current) = current {
        if !current.same_policy(&incoming) {
            return Err("durable submissions have inconsistent bucket policy".into());
        }
        current.min_examples = current.min_examples.min(incoming.min_examples);
        current.submission_ids.push(id);
        current.submission_examples.push(incoming.examples.len() as u32);
        current.submission_returned.push(incoming.returned);
        current.returned |= incoming.returned;
        current.examples.append(&mut incoming.examples);
    } else {
        incoming.submission_ids = vec![id];
        incoming.submission_examples = vec![incoming.examples.len() as u32];
        incoming.submission_returned = vec![incoming.returned];
        *current = Some(incoming);
    }
    Ok(())
}

/// Her measured verdict surprise when it is BELOW the floor: the room confirms her
/// expectations, memories suffice, and the bucket holds (`Took::Unsurprised`). `None`
/// when she is surprised enough, or not yet judged at all (unknown never halts
/// training), or no mind of hers is resident on this core. One in-memory read of her
/// strip; the same number job-create decides on, read here so a fill that would only
/// come back unsurprised is never dispatched.
pub(super) fn unsurprised(persona: Uuid) -> Option<f32> {
    let now_ms = continuum_base::time::EpochMs::now().millis(); // pre-epoch clock: every tally reads outside its window = not measured
    crate::persona::perception_feed::awareness_of(persona, now_ms)
        .and_then(|a| a.verdict_surprise())
        .map(|v| v.s)
        .filter(|s| *s < crate::genome::competence::SURPRISE_FLOOR)
}
