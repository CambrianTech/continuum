//! Durable acceptance belongs to the trigger, not to each curriculum producer.
//!
//! Each submission is inserted once. A dispatch intent names its submissions
//! BEFORE any row is assigned or any provider is called. Interrupted assignments
//! can therefore be completed from the intent; no multi-row transaction is
//! assumed. An interrupted provider call is never repeated on absence of proof.

use super::{BucketKey, PendingBatch, TrainingTriggerState};
use std::sync::{atomic::Ordering, Arc};

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::commands::genome::job_create::Took;
#[cfg(not(test))]
use crate::genome::fine_tuning::TrainingJobBoard;
use crate::genome::fine_tuning::{job_board::DispatchLookup, JobHandle};
use crate::orm::{
    adapter::StorageAdapter,
    entity::{BaseEntity, OrmEntity},
    query::{QueryBuilder, QueryOperator},
    Entity, OrmStore,
};

/// Present only after this destination has durably accepted the immutable batch.
/// This does not attest that training ran, or deduplicate a different destination.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/training_trigger/AcceptanceReceipt.ts"
)]
pub struct AcceptanceReceipt {
    #[ts(type = "string")]
    pub submission_id: Uuid,
    pub replayed: bool,
}

#[derive(Debug)]
pub(crate) enum DispatchFailure {
    Retryable(String),
    Uncertain(String),
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
pub(super) struct Submission {
    #[serde(flatten)]
    base: BaseEntity,
    /// Destination-local accepted order, restored from the indexed maximum.
    #[entity(indexed)]
    sequence: u64,
    persona_id: Uuid,
    trait_kind: String,
    base_model: String,
    #[entity(json)]
    batch: PendingBatch,
    is_pending: bool,
    dispatch_id: Option<Uuid>,
    /// Withdrawn before any dispatch by `genome/training-trigger/retire`: the
    /// [`Retirement`] receipt that names why. Such a row is never pending again:
    /// the producer's retry of the same submission id replays as accepted and
    /// appends nothing (see [`TrainingTriggerState::accept`]). Absent on every
    /// row written before the verb existed.
    #[serde(default)]
    retirement_id: Option<Uuid>,
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
pub(super) struct Retirement {
    #[serde(flatten)]
    base: BaseEntity,
    /// The card whose settled turns were withdrawn, when the selection was by card.
    #[entity(indexed)]
    card_id: Option<Uuid>,
    /// The bucket the rows left.
    persona_id: Uuid,
    trait_kind: String,
    base_model: String,
    /// Every submission this receipt withdrew, in the order they were flipped.
    submission_ids: Vec<Uuid>,
    /// Examples those submissions carried: what the next job will NOT train on.
    examples: u32,
    reason: String,
    /// The caller's peer id, when the command carried one.
    retired_by: Option<Uuid>,
    retired_at_ms: u64,
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
}

#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
#[serde(rename_all = "camelCase")]
#[entity(collection = "training_trigger_dispatches")]
#[entity(index(name = "idx_training_dispatch_bucket", fields = ["personaId", "traitKind", "baseModel", "isActive"]))]
#[entity(index(name = "idx_training_dispatch_active", fields = ["isActive", "id"]))]
pub(super) struct DispatchIntent {
    #[serde(flatten)]
    base: BaseEntity,
    persona_id: Uuid,
    trait_kind: String,
    base_model: String,
    submission_ids: Vec<Uuid>,
    #[entity(json)]
    phase: DispatchPhase,
    is_active: bool,
}

pub(super) struct ActiveDispatch {
    intent: DispatchIntent,
    pub(super) batch: Arc<PendingBatch>,
    journal_cursor: u64,
}

pub(super) struct DurableStore {
    submissions: OrmStore<Submission>,
    dispatches: OrmStore<DispatchIntent>,
    retirements: OrmStore<Retirement>,
    adapter: Arc<dyn StorageAdapter>,
}

impl DurableStore {
    async fn new(adapter: Arc<dyn StorageAdapter>) -> Result<Self, String> {
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
            adapter,
        })
    }

    async fn next_sequence(&self) -> Result<u64, String> {
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

    async fn pending_page(
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
    async fn pending_rows_on_card(&self, card: Uuid) -> Result<Vec<Submission>, String> {
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

    async fn active_intent(&self, key: &BucketKey) -> Result<Option<DispatchIntent>, String> {
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
    async fn assign(&self, intent: &DispatchIntent) -> Result<(), String> {
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
    fn key(&self) -> BucketKey {
        BucketKey {
            persona_id: self.persona_id,
            trait_kind: self.trait_kind.clone(),
            base_model: self.base_model.clone(),
        }
    }
}
impl DispatchIntent {
    fn key(&self) -> BucketKey {
        BucketKey {
            persona_id: self.persona_id,
            trait_kind: self.trait_kind.clone(),
            base_model: self.base_model.clone(),
        }
    }
}
impl BucketKey {
    fn query(&self, collection: &str) -> QueryBuilder {
        QueryBuilder::new(collection)
            .filter_eq("personaId", self.persona_id.to_string())
            .filter_eq("traitKind", self.trait_kind.clone())
            .filter_eq("baseModel", self.base_model.clone())
    }
}

/// Incomplete per-key recovery stays owned across bounded pages. Commands never
/// accept new policy or dispatch until this key has a complete coherent cache.
#[derive(Default)]
pub(super) struct BucketHydration {
    initialized: bool,
    intent: Option<DispatchIntent>,
    cached_active: Option<Arc<ActiveDispatch>>,
    active_batch: Option<PendingBatch>,
    active_index: usize,
    pending: Option<PendingBatch>,
    pending_after: u64,
}

fn entity_id(base: &BaseEntity) -> Result<Uuid, String> {
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
    pub(crate) fn policy(&self) -> super::BucketPolicy {
        super::BucketPolicy {
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

    fn same_submission(&self, other: &Self) -> bool {
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
fn budget_split(batch: PendingBatch, base_model: &str) -> (PendingBatch, Option<PendingBatch>) {
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
    rest.examples = taken.examples.split_off(split_at);
    (taken, Some(rest))
}

fn append_batch(
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
        current.examples.append(&mut incoming.examples);
    } else {
        incoming.submission_ids = vec![id];
        incoming.submission_examples = vec![incoming.examples.len() as u32];
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
fn unsurprised(persona: Uuid) -> Option<f32> {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0); // pre-epoch clock: every tally reads outside its window = not measured
    crate::persona::perception_feed::awareness_of(persona, now_ms)
        .and_then(|a| a.verdict_surprise())
        .map(|v| v.s)
        .filter(|s| *s < crate::genome::competence::SURPRISE_FLOOR)
}

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
    async fn hydrate_bucket(&self, key: &BucketKey) -> Result<bool, String> {
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

    async fn hydrate_page(
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
                append_batch(&mut progress.active_batch, id, row.batch)?;
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
                append_batch(&mut progress.pending, id, row.batch)?;
            }
            progress.pending_after = sequence;
        }
        Ok(complete)
    }

    fn append_pending(&self, key: BucketKey, id: Uuid, batch: PendingBatch) -> Result<(), String> {
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
            append_batch(&mut batch, *id, row.batch)?;
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
            if existing.key() != *key || !existing.batch.same_submission(&batch) {
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
    fn trial_open_for(&self, key: &BucketKey) -> Result<Option<Uuid>, ()> {
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
                crate::probe!(
                    class = "training.trigger.trial_file_unreadable",
                    persona = %key.persona_id,
                    trait_kind = %key.trait_kind,
                    error = %error,
                    "her trial file could not be read: the bucket holds until it can, nothing dispatches beside a trial nobody can see"
                );
                return Err(());
            }
        };
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0); // pre-epoch clock: every landed gene reads as settling, the conservative side
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

    async fn dispatch_pending_inner(&self, key: &BucketKey) -> Result<DispatchResult, String> {
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
                    crate::probe!(
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
                    DispatchLookup::NotObserved => {}
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
            DispatchPhase::Prepared | DispatchPhase::Retryable { .. } => {}
        }
        let mut intent = active.intent.clone();
        intent.phase = DispatchPhase::Dispatching;
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
            )
            .await
        {
            Ok(super::Created::Job(handle, provider)) => self.finish_dispatch(key, &active, handle, provider).await,
            Ok(super::Created::Held(took)) => self.finish_held(key, &active, took).await,
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
    async fn finish_held(&self, key: &BucketKey, active: &ActiveDispatch, took: Took) -> Result<DispatchResult, String> {
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
        crate::probe!(
            class = "training.trigger.held",
            persona = %key.persona_id,
            trait_kind = %key.trait_kind,
            took = ?took,
            examples = examples as u64,
            "the fill was held without a job (a job, a trial, or an existing gene pending) — its examples wait in the bucket"
        );
        Ok(DispatchResult::Held { examples, took })
    }

    async fn finish_dispatch(
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
                        Ok(true) if state.ready_to_dispatch(&key) => {
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
            if let DispatchResult::Failed { kind, error } = result {
                crate::probe!(class = "training.recovery.pending", persona = %report_key.persona_id,
                    trait_kind = %report_key.trait_kind, kind, error = %error,
                    "accepted training remains owned by the durable trigger");
            }
        }
        Ok(())
    }

    /// Two indexed metadata pages, never historical examples. Cursors wrap after
    /// an exhausted page, so an earlier allocated sequence that committed later
    /// remains discoverable on the next pass.
    async fn recovery_heads(
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
    async fn restore_pending_bucket(&self, key: &BucketKey) -> Result<(), String> {
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

    async fn resolve_in_bucket(
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
            DispatchResolution::NotDispatched => {
                if let Some(handle) = registered {
                    return Err((
                        "Contradicted",
                        format!("the ledger registers job {} under dispatch {dispatch}: it WAS dispatched; settle it with that job id, not as not-dispatched", handle.local_id),
                    ));
                }
                let mut intent = active.intent.clone();
                intent.phase = DispatchPhase::Retryable {
                    error: format!("resolved by hand as not dispatched: {reason}"),
                };
                store
                    .dispatches
                    .update(id, &intent)
                    .await
                    .map_err(|e| ("PersistenceFailed", e.to_string()))?;
                self.active_dispatches.insert(
                    key.clone(),
                    Arc::new(ActiveDispatch {
                        intent,
                        batch: Arc::clone(&active.batch),
                        journal_cursor: active.journal_cursor,
                    }),
                );
                crate::probe!(
                    class = "training.trigger.dispatch_resolved",
                    persona = %key.persona_id,
                    trait_kind = %key.trait_kind,
                    dispatch = %dispatch,
                    resolution = "retryable",
                    examples = examples as u64,
                    resolved_by = ?resolved_by,
                    reason = %reason,
                    "an intent recovery could not confirm was settled by hand as never dispatched; the next tick re-creates its job from the same batch"
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
                crate::probe!(
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
    async fn retire_in_bucket(
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
            retired_at_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0), // a clock before 1970 stamps 0: the receipt still names its rows and reason
        };
        store
            .retirements
            .save(retirement_id, &receipt)
            .await
            .map_err(|e| e.to_string())?;
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
        crate::probe!(
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
fn registration_from(board: &crate::genome::fine_tuning::TrainingJobBoard, dispatch: Uuid, from: u64) -> Result<Option<JobHandle>, String> {
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

/// What `genome/training-trigger/resolve` settles an unconfirmed intent as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DispatchResolution {
    /// No job was ever registered under it: retry from the same batch.
    NotDispatched,
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
    fn absorb(&mut self, other: RetireReport) {
        self.retirement_ids.extend(other.retirement_ids);
        self.retired.extend(other.retired);
        self.already_retired.extend(other.already_retired);
        self.refused.extend(other.refused);
        self.examples_retired += other.examples_retired;
    }
}

#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq)]
enum OperationPoint {
    BeforeSchemaSetup,
    AfterMetadataScan,
    AcceptanceCommitted,
    BeforeRetryableCommit,
}

/// Pause the actual owner at a persistence boundary; no replacement storage or
/// dispatch implementation participates in cancellation regressions.
#[cfg(test)]
pub(super) struct OperationPause {
    point: OperationPoint,
    entered: tokio::sync::Notify,
    resume: tokio::sync::Notify,
}

#[cfg(test)]
impl TrainingTriggerState {
    async fn pause_operation(&self, point: OperationPoint) {
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

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (the 5090, 2026-10-10): a bucket that filled all day dispatched as one
    // ~100-hour run. With no measured rate a run takes the OLDEST whole submissions up to the
    // bucket's threshold (the drain: none starves) and leaves the rest pending, in order; a bucket
    // whose per-submission counts do not cover its examples is never split.
    #[test]
    fn a_dispatch_takes_whole_oldest_submissions_and_leaves_the_rest_pending() {
        let ex = |p: &str| crate::genome::fine_tuning::TrainingExample { prompt: p.into(), completion: "c".into(), metadata: None, lived: None };
        let ids: Vec<Uuid> = (0..4).map(|_| Uuid::new_v4()).collect();
        let batch = PendingBatch {
            submission_ids: ids.clone(),
            submission_examples: vec![2, 1, 3, 1],
            persona_name: "Kimi".into(),
            source: crate::genome::fine_tuning::TrainingSource::OperatorCurated,
            examples: ["a1", "a2", "b1", "c1", "c2", "c3", "d1"].iter().map(|p| ex(p)).collect(),
            lora: None,
            schedule: None,
            local_artifact_dir: None,
            preferred_provider: None,
            min_examples: 3,
            validation_split: 0.0,
            eval_set: None,
        };
        let (taken, rest) = budget_split(batch.clone(), "no-rate-for-this-base");
        let rest = rest.expect("over the threshold: split");
        assert_eq!(taken.submission_ids, ids[..2].to_vec(), "the oldest whole submissions, to the threshold");
        assert_eq!(taken.examples.iter().map(|e| e.prompt.as_str()).collect::<Vec<_>>(), ["a1", "a2", "b1"], "with exactly their examples");
        assert_eq!(rest.submission_ids, ids[2..].to_vec(), "the newer ones stay pending, in order");
        assert_eq!(rest.examples.iter().map(|e| e.prompt.as_str()).collect::<Vec<_>>(), ["c1", "c2", "c3", "d1"]);
        let mut unknown = batch;
        unknown.submission_examples.pop();
        let (whole, none) = budget_split(unknown, "no-rate-for-this-base");
        assert!(none.is_none() && whole.examples.len() == 7, "counts that do not cover the examples: never split");
    }
    use crate::commands::training_trigger::test_support::{build_runtime, ex, submit_params};
    use crate::runtime::ServiceModule;

    fn pause_at(state: &TrainingTriggerState, point: OperationPoint) -> Arc<OperationPause> {
        let pause = Arc::new(OperationPause {
            point,
            entered: tokio::sync::Notify::new(),
            resume: tokio::sync::Notify::new(),
        });
        *state.operation_pause.lock().unwrap() = Some(pause.clone());
        pause
    }

    // Regression for #4858: changing return identity must neither duplicate a legacy
    // successful return nor replay the original submission as if it were a return.
    // One ORM setup covers legacy ambiguity and proven equal/different-payload collisions.
    #[tokio::test]
    async fn return_identity_upgrade_requires_original_dispatch_proof() {
        use crate::commands::training_trigger::submit::{return_request, returned_id};
        let (adapter, _dir) = crate::orm::store::fresh_adapter().await;
        let (old, executor) = build_runtime(adapter.clone(), false).await;
        let artifacts = tempfile::tempdir().unwrap();
        let board = crate::genome::fine_tuning::TrainingJobBoard::with_ledger(Some(artifacts.path().join("jobs.jsonl")));
        let mut cases = Vec::new();
        // 0: old return still pending; 1/2: original submission, equal/different
        // returned payload; 3: current job has two differently named submissions and
        // no request directory; 4: old return already dispatched into a NEW job.
        for scenario in 0..5 {
            let job = Uuid::new_v4();
            let submission = if scenario == 3 { Uuid::new_v4() } else { job };
            let persona = Uuid::new_v4();
            let original = vec![ex("original", "answer")];
            let mut params = submit_params(persona, "code", original.clone(), Some(super::super::DEFAULT_MIN_EXAMPLES));
            params["submissionId"] = serde_json::json!(submission);
            let result = executor.execute_json("genome/training-trigger/submit", params).await.unwrap();
            assert_eq!(result["success"], true, "{result}");
            let key = BucketKey { persona_id: persona, trait_kind: "code".into(), base_model: "synthetic".into() };
            let store = old.state.durable.require().unwrap();
            if scenario != 0 {
                let dispatched_job = if scenario == 4 { Uuid::new_v4() } else { job };
                let mut submission_ids = vec![submission];
                if scenario == 3 {
                    let extra = Uuid::new_v4();
                    let mut params = submit_params(persona, "code", vec![ex("extra", "answer")], Some(super::super::DEFAULT_MIN_EXAMPLES));
                    params["submissionId"] = serde_json::json!(extra);
                    assert_eq!(executor.execute_json("genome/training-trigger/submit", params).await.unwrap()["success"], true);
                    submission_ids.push(extra);
                }
                // Persist the same dispatch ownership that dispatch_pending/finish_dispatch
                // writes. Merely accepting a batch under the job UUID is NOT this proof.
                let intent = DispatchIntent {
                    base: BaseEntity::for_new_record(),
                    persona_id: persona,
                    trait_kind: key.trait_kind.clone(),
                    base_model: key.base_model.clone(),
                    submission_ids,
                    phase: DispatchPhase::Dispatched {
                        handle: JobHandle { provider_id: "fixture".into(), provider_job_id: dispatched_job.to_string(), local_id: dispatched_job },
                        provider: "fixture".into(),
                    },
                    is_active: false,
                };
                let dispatch = entity_id(&intent.base).unwrap();
                store.dispatches.save(dispatch, &intent).await.unwrap();
                store.assign(&intent).await.unwrap();
                assert!(old.state.dispatched_request(&key, dispatch, Uuid::new_v4()).await.is_err(), "a submission's dispatch is not a different job's input");
                if scenario == 3 {
                    board.register(crate::genome::fine_tuning::job_board::WatchedJob {
                        trigger_dispatch_id: Some(dispatch),
                        handle: JobHandle { provider_id: "fixture".into(), provider_job_id: job.to_string(), local_id: job },
                        persona_id: persona, persona_name: "test-p".into(), base_model: "synthetic".into(), trait_kind: "code".into(),
                        eval_set: None, signature: None, decision: None,
                    });
                    board.claim(job, &crate::genome::fine_tuning::types::TrainingStatus::Failed { error: "refused before training".into() });
                }
            }
            let examples = match scenario {
                2 => vec![ex("returned-1", "a"), ex("returned-2", "b")],
                3 => vec![ex("original", "answer"), ex("extra", "answer")],
                _ => original,
            };
            let request: crate::genome::fine_tuning::types::TrainingJobRequest = serde_json::from_value(serde_json::json!({
                "personaId": persona, "personaName": "test-p", "baseModel": "synthetic", "traitKind": "code",
                "dataset": { "examples": examples, "source": "operator_curated", "validationSplit": 0.0 }
            })).unwrap();
            cases.push((scenario, job, key, request));
        }
        old.shutdown().await.unwrap();
        drop(executor);
        drop(old);
        let (next, _executor) = build_runtime(adapter, false).await;
        for (scenario, job, key, request) in cases {
            for attempt in 0..2 {
                let result = if scenario == 3 {
                    // The canonical job has no request.json, and its two submissions
                    // have different UUIDs. No historical directory may stand in for it.
                    crate::commands::training_trigger::return_::return_job(&next.state, &board, artifacts.path(), job).await.unwrap()
                } else {
                    return_request(&next.state, request.clone(), job).await.unwrap()
                };
                if scenario == 0 || scenario == 4 {
                    assert_eq!(result.error_kind.as_deref(), Some("RecoveryRequired"), "{result:?}");
                    assert!(next.state.durable.require().unwrap().submissions.find_by_id(returned_id(job)).await.unwrap().is_none(), "no duplicate legacy return acceptance");
                } else {
                    assert!(result.success, "{result:?}");
                    assert_eq!(next.state.bucket_example_count(key.persona_id, &key.trait_kind, &key.base_model), Some(request.dataset.examples.len()));
                    {
                        let pending = next.state.buckets.get(&key).unwrap();
                        assert_eq!(serde_json::to_value(&pending.examples).unwrap(), serde_json::to_value(&request.dataset.examples).unwrap(), "return preserves the dispatched examples and their order");
                    }
                    let acceptance = result.acceptance.unwrap();
                    assert_eq!(acceptance.submission_id, returned_id(job));
                    assert_eq!(acceptance.replayed, attempt != 0);
                }
            }
            if scenario == 0 || scenario == 4 {
                let legacy = next.state.durable.require().unwrap().submissions.find_by_id(job).await.unwrap().unwrap();
                assert_eq!(legacy.is_pending, scenario == 0);
                assert_eq!(legacy.batch.examples.len(), 1, "old successful acceptance remains intact");
            }
        }
        next.shutdown().await.unwrap();
    }

    // What this catches (278afa6c): an init timeout must not permanently disable
    // learning. Cancel the real owner's setup wait; its existing tick retries
    // against the retained actual SQLite adapter, without a global HOME/config edit.
    #[tokio::test]
    async fn owner_tick_retries_interrupted_schema_setup() {
        let (adapter, _dir) = crate::orm::store::fresh_adapter().await;
        let trigger = Arc::new(super::super::TrainingTriggerModule::new());
        let pause = pause_at(&trigger.state, OperationPoint::BeforeSchemaSetup);
        let state = trigger.state.clone();
        let setup = tokio::spawn(async move { state.initialize_storage(adapter).await });
        tokio::time::timeout(std::time::Duration::from_secs(5), pause.entered.notified())
            .await
            .unwrap();
        setup.abort();
        assert!(setup.await.unwrap_err().is_cancelled());
        assert!(trigger.state.require_ready().is_err());
        trigger.tick().await.unwrap();
        trigger.state.require_ready().unwrap();
        assert!(trigger
            .state
            .durable
            .require()
            .unwrap()
            .submissions
            .find_all()
            .await
            .unwrap()
            .is_empty());
        trigger.shutdown().await.unwrap();
        // A late initialization publication cannot override authoritative stop.
        trigger.state.ready.store(true, Ordering::Release);
        assert!(trigger.state.require_ready().is_err());
        assert!(trigger.tick().await.is_err());
        let admission: Result<(), String> = trigger
            .state
            .run_owned(
                BucketKey {
                    persona_id: Uuid::new_v4(),
                    trait_kind: "code".into(),
                    base_model: "synthetic".into(),
                },
                |_, _| async { panic!("shutdown admitted a new operation") },
            )
            .await;
        assert!(admission.is_err());
    }

    // What this catches (278afa6c): boot never loads an entire backlog. Admission
    // advances one real page but cannot install new policy before prior pages are
    // coherent; the module tick resumes retained progress rather than page zero.
    #[tokio::test]
    async fn large_bucket_recovers_in_pages_before_accepting_new_policy() {
        let (adapter, _dir) = crate::orm::store::fresh_adapter().await;
        let (old, executor) = build_runtime(adapter.clone(), false).await;
        let persona = Uuid::new_v4();
        let key = BucketKey {
            persona_id: persona,
            trait_kind: "code".into(),
            base_model: "synthetic".into(),
        };
        let mut first = None;
        for index in 0..66 {
            let mut request = submit_params(
                persona,
                "code",
                vec![ex(&index.to_string(), "c")],
                Some(1000),
            );
            request["submissionId"] = serde_json::json!(Uuid::new_v4());
            let receipt = executor
                .execute_json("genome/training-trigger/submit", request.clone())
                .await
                .unwrap();
            assert_eq!(receipt["success"], true, "{receipt}");
            if first.is_none() {
                first = Some(request);
            }
        }
        old.shutdown().await.unwrap();
        let (next, executor) = build_runtime(adapter, false).await;
        assert!(
            next.state.buckets.is_empty(),
            "schema setup must not hydrate pending payloads"
        );
        let mut changed = submit_params(persona, "code", vec![ex("new", "c")], Some(1000));
        changed["preferredProvider"] = serde_json::json!("different");
        let deferred = executor
            .execute_json("genome/training-trigger/submit", changed.clone())
            .await
            .unwrap();
        assert_eq!(deferred["errorKind"], "RecoveryRequired", "{deferred}");
        assert!(deferred.get("acceptance").is_none());
        assert_eq!(
            next.state
                .hydrating
                .get(&key)
                .unwrap()
                .pending
                .as_ref()
                .unwrap()
                .examples
                .len(),
            64
        );
        assert!(!next.state.hydrated.contains(&key));
        next.tick().await.unwrap();
        assert!(!next.state.hydrating.contains_key(&key));
        let bucket = next.state.buckets.get(&key).unwrap();
        assert_eq!(bucket.examples.len(), 66);
        assert_eq!(bucket.examples[0].prompt, "0");
        assert_eq!(bucket.examples[65].prompt, "65");
        drop(bucket);
        let refused = executor
            .execute_json("genome/training-trigger/submit", changed)
            .await
            .unwrap();
        assert_eq!(refused["errorKind"], "InconsistentBucket");
        let replay = executor
            .execute_json("genome/training-trigger/submit", first.unwrap())
            .await
            .unwrap();
        assert_eq!(replay["acceptance"]["replayed"], true);
        assert_eq!(
            next.state
                .bucket_example_count(persona, "code", "synthetic"),
            Some(66)
        );
        // Changed-bucket replays must not retain hydrated markers for empty keys.
        let original = next.state.buckets.get(&key).unwrap().submission_ids[0];
        for trait_kind in ["foreign-a", "foreign-b"] {
            let mut foreign = submit_params(persona, trait_kind, vec![ex("0", "c")], Some(1000));
            foreign["submissionId"] = serde_json::json!(original);
            let refused = executor
                .execute_json("genome/training-trigger/submit", foreign)
                .await
                .unwrap();
            assert_eq!(refused["errorKind"], "SubmissionConflict");
        }
        assert_eq!(next.state.hydrated.len(), 1);
    }

    // What this catches (278afa6c): real SQLite INSERT commits, then the caller
    // cancels before cache publication. The admitted owner must keep its lease,
    // preserve A's policy/order, and prevent B from installing conflicting policy.
    #[tokio::test]
    async fn cancelled_acceptance_keeps_policy_and_order_until_commit_is_published() {
        use std::time::Duration;
        let (adapter, _dir) = crate::orm::store::fresh_adapter().await;
        let (trigger, executor) = build_runtime(adapter.clone(), false).await;
        let persona = Uuid::new_v4();
        let id = Uuid::new_v4();
        let mut request = submit_params(persona, "code", vec![ex("first", "a")], Some(100));
        request["submissionId"] = serde_json::json!(id);
        let pause = pause_at(&trigger.state, OperationPoint::AcceptanceCommitted);
        let caller_executor = executor.clone();
        let caller_request = request.clone();
        let caller = tokio::spawn(async move {
            caller_executor
                .execute_json("genome/training-trigger/submit", caller_request)
                .await
        });
        tokio::time::timeout(Duration::from_secs(5), pause.entered.notified())
            .await
            .unwrap();
        assert!(trigger
            .state
            .durable
            .require()
            .unwrap()
            .submissions
            .find_by_id(id)
            .await
            .unwrap()
            .is_some());
        caller.abort();
        assert!(caller.await.unwrap_err().is_cancelled());
        let mut changed = submit_params(persona, "code", vec![ex("second", "b")], Some(100));
        changed["preferredProvider"] = serde_json::json!("different-policy");
        let waiting = executor.execute_json("genome/training-trigger/submit", changed);
        tokio::pin!(waiting);
        assert!(
            tokio::time::timeout(Duration::from_millis(20), &mut waiting)
                .await
                .is_err(),
            "cancelled caller must not release the owner's lease"
        );
        pause.resume.notify_one();
        let refused = waiting.await.unwrap();
        assert_eq!(refused["errorKind"], "InconsistentBucket", "{refused}");
        assert!(refused.get("acceptance").is_none());
        let replay = executor
            .execute_json("genome/training-trigger/submit", request)
            .await
            .unwrap();
        assert_eq!(replay["acceptance"]["replayed"], true);
        let next = executor
            .execute_json(
                "genome/training-trigger/submit",
                submit_params(persona, "code", vec![ex("second", "b")], Some(100)),
            )
            .await
            .unwrap();
        assert_eq!(next["currentCount"], 2);
        trigger.shutdown().await.unwrap();
        let (restarted, _) = build_runtime(adapter, false).await;
        restarted.tick().await.unwrap();
        let key = BucketKey {
            persona_id: persona,
            trait_kind: "code".into(),
            base_model: "synthetic".into(),
        };
        let bucket = restarted.state.buckets.get(&key).unwrap();
        assert_eq!(
            bucket
                .examples
                .iter()
                .map(|example| example.prompt.as_str())
                .collect::<Vec<_>>(),
            ["first", "second"]
        );
    }

    // What this catches (278afa6c): a cancelled caller cannot let a later attempt
    // overtake an old Retryable write. Pause the actual transition before its real
    // SQLite update; the retry waits for that same owner to finish, then keeps IDs.
    #[tokio::test]
    async fn cancelled_retryable_transition_cannot_be_overtaken() {
        use std::time::Duration;
        let (adapter, _dir) = crate::orm::store::fresh_adapter().await;
        let (trigger, executor) = build_runtime(adapter.clone(), true).await;
        let persona = Uuid::new_v4();
        let key = BucketKey {
            persona_id: persona,
            trait_kind: "code".into(),
            base_model: "synthetic".into(),
        };
        let mut request = submit_params(persona, "code", vec![ex("p", "c")], Some(1));
        request["submissionId"] = serde_json::json!(Uuid::new_v4());
        request["preferredProvider"] = serde_json::json!("unavailable-provider");
        let pause = pause_at(&trigger.state, OperationPoint::BeforeRetryableCommit);
        let caller_executor = executor.clone();
        let caller_request = request.clone();
        let caller = tokio::spawn(async move {
            caller_executor
                .execute_json("genome/training-trigger/submit", caller_request)
                .await
        });
        tokio::time::timeout(Duration::from_secs(5), pause.entered.notified())
            .await
            .unwrap();
        let dispatch_id = entity_id(
            &trigger
                .state
                .active_dispatches
                .get(&key)
                .unwrap()
                .intent
                .base,
        )
        .unwrap();
        let intent = trigger
            .state
            .durable
            .require()
            .unwrap()
            .dispatches
            .find_by_id(dispatch_id)
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(intent.phase, DispatchPhase::Dispatching));
        caller.abort();
        assert!(caller.await.unwrap_err().is_cancelled());
        let waiting = executor.execute_json("genome/training-trigger/submit", request);
        tokio::pin!(waiting);
        assert!(
            tokio::time::timeout(Duration::from_millis(20), &mut waiting)
                .await
                .is_err()
        );
        pause.resume.notify_one();
        let replay = waiting.await.unwrap();
        assert_eq!(replay["acceptance"]["replayed"], true, "{replay}");
        assert_eq!(replay["errorKind"], "DispatchFailed");
        assert_eq!(
            entity_id(
                &trigger
                    .state
                    .active_dispatches
                    .get(&key)
                    .unwrap()
                    .intent
                    .base
            )
            .unwrap(),
            dispatch_id
        );
        trigger.shutdown().await.unwrap();
        let (restarted, _) = build_runtime(adapter, true).await;
        restarted.tick().await.unwrap();
        let active = restarted.state.active_dispatches.get(&key).unwrap();
        assert_eq!(entity_id(&active.intent.base).unwrap(), dispatch_id);
        assert!(matches!(
            active.intent.phase,
            DispatchPhase::Retryable { .. }
        ));
        assert_eq!(active.batch.examples.len(), 1);
    }

    // What this catches (278afa6c): a persisted intent is the recovery owner
    // even when its multi-row assignments only partly reached disk. The actual
    // flush path must dispatch the original examples once, with correlated proof.
    #[tokio::test]
    async fn partial_intent_assignment_recovers_then_dispatches_once() {
        let (adapter, _dir) = crate::orm::store::fresh_adapter().await;
        let (old, executor) = build_runtime(adapter.clone(), true).await;
        let persona = Uuid::new_v4();
        let key = BucketKey {
            persona_id: persona,
            trait_kind: "test-trait".into(),
            base_model: "synthetic".into(),
        };
        let mut requests = Vec::new();
        for (id, count) in [(Uuid::new_v4(), 2), (Uuid::new_v4(), 3)] {
            let mut request = submit_params(
                persona,
                "test-trait",
                (0..count)
                    .map(|i| ex(&format!("{id}-{i}"), "completion"))
                    .collect(),
                Some(100),
            );
            request["submissionId"] = serde_json::json!(id);
            let receipt = executor
                .execute_json("genome/training-trigger/submit", request.clone())
                .await
                .unwrap();
            assert_eq!(receipt["success"], true, "{receipt}");
            requests.push(request);
        }
        let batch = old.state.buckets.remove(&key).unwrap().1;
        let intent = DispatchIntent {
            base: BaseEntity::for_new_record(),
            persona_id: key.persona_id,
            trait_kind: key.trait_kind.clone(),
            base_model: key.base_model.clone(),
            submission_ids: batch.submission_ids.clone(),
            phase: DispatchPhase::Prepared,
            is_active: true,
        };
        let dispatch_id = entity_id(&intent.base).unwrap();
        let store = old.state.durable.require().unwrap();
        store.dispatches.save(dispatch_id, &intent).await.unwrap();
        let first = intent.submission_ids[0];
        let mut row = store.submissions.find_by_id(first).await.unwrap().unwrap();
        row.is_pending = false;
        row.dispatch_id = Some(dispatch_id);
        store.submissions.update(first, &row).await.unwrap();
        drop(executor);
        let (next, executor) = build_runtime(adapter, true).await;
        assert!(
            next.state.buckets.is_empty(),
            "boot must not hydrate payloads"
        );
        assert!(
            next.state.active_dispatches.is_empty(),
            "intent recovery is owned by command/tick pages"
        );
        // Hold a real tick's metadata snapshot while flush settles that intent.
        // The stale head must neither replay it nor retain an empty cache marker.
        let pause = pause_at(&next.state, OperationPoint::AfterMetadataScan);
        let ticking = next.clone();
        let tick = tokio::spawn(async move { ticking.tick().await });
        tokio::time::timeout(std::time::Duration::from_secs(5), pause.entered.notified())
            .await
            .unwrap();
        let params = serde_json::json!({"personaId": persona, "traitKind": "test-trait", "baseModel": "synthetic"});
        let dispatched = executor
            .execute_json("genome/training-trigger/flush", params.clone())
            .await
            .unwrap();
        assert_eq!(dispatched["outcome"], "JobDispatched", "{dispatched}");
        let handle_id = dispatched["jobHandle"]["localId"].as_str().unwrap();
        assert_eq!(dispatched["examplesUsed"], 5);
        let evidence = next
            .state
            .test_job_board
            .lookup_trigger_dispatch(dispatch_id, 0)
            .unwrap();
        assert!(
            matches!(evidence, DispatchLookup::Observed(ref handle) if handle.local_id.to_string() == handle_id)
        );
        pause.resume.notify_one();
        tick.await.unwrap().unwrap();
        assert!(next.state.hydrated.is_empty());
        let again = executor
            .execute_json("genome/training-trigger/flush", params)
            .await
            .unwrap();
        assert_eq!(again["outcome"], "NothingToFlush");
        let replay = executor
            .execute_json("genome/training-trigger/submit", requests.remove(0))
            .await
            .unwrap();
        assert_eq!(replay["outcome"], "AlreadyAccepted");
        assert_eq!(replay["acceptance"]["replayed"], true);
        assert_eq!(next.state.pending_bucket_count(), 0);
    }

    // What this catches (278afa6c): cancellation after a successful insert must
    // not require the producer to retry. The existing owner's tick discovers
    // committed rows absent from RAM without duplicating already-cached peers.
    #[tokio::test]
    async fn owner_tick_restores_committed_but_uncached_acceptance() {
        let (adapter, _dir) = crate::orm::store::fresh_adapter().await;
        let (trigger, executor) = build_runtime(adapter, false).await;
        let persona = Uuid::new_v4();
        let key = BucketKey {
            persona_id: persona,
            trait_kind: "code".into(),
            base_model: "synthetic".into(),
        };
        let mut request = submit_params(persona, "code", vec![ex("earlier", "c")], Some(100));
        request["submissionId"] = serde_json::json!(Uuid::new_v4());
        let receipt = executor
            .execute_json("genome/training-trigger/submit", request.clone())
            .await
            .unwrap();
        assert_eq!(receipt["success"], true);
        executor
            .execute_json(
                "genome/training-trigger/submit",
                submit_params(persona, "code", vec![ex("later", "c")], Some(100)),
            )
            .await
            .unwrap();
        {
            let mut bucket = trigger.state.buckets.get_mut(&key).unwrap();
            bucket.submission_ids.remove(0);
            bucket.examples.remove(0); // real durable row absent from the live cache
        }
        trigger.tick().await.unwrap();
        trigger.tick().await.unwrap();
        {
            let bucket = trigger.state.buckets.get(&key).unwrap();
            assert_eq!(
                bucket
                    .examples
                    .iter()
                    .map(|example| example.prompt.as_str())
                    .collect::<Vec<_>>(),
                ["earlier", "later"]
            );
        }
        let replay = executor
            .execute_json("genome/training-trigger/submit", request)
            .await
            .unwrap();
        assert_eq!(replay["acceptance"]["replayed"], true);
        assert_eq!(
            trigger
                .state
                .bucket_example_count(persona, "code", "synthetic"),
            Some(2)
        );
    }

    // What this catches (278afa6c): an interrupted provider call is NOT pending
    // work to repeat. Recreating the real owner keeps it explicitly uncertain,
    // even though a fully capable provider is now available.
    #[tokio::test]
    async fn interrupted_dispatch_is_recovery_required_not_redispatched() {
        let (adapter, _dir) = crate::orm::store::fresh_adapter().await;
        let (old, executor) = build_runtime(adapter.clone(), false).await;
        let persona = Uuid::new_v4();
        let key = BucketKey {
            persona_id: persona,
            trait_kind: "code".into(),
            base_model: "synthetic".into(),
        };
        let receipt = executor
            .execute_json(
                "genome/training-trigger/submit",
                submit_params(persona, "code", vec![ex("p", "c")], Some(100)),
            )
            .await
            .unwrap();
        assert_eq!(receipt["success"], true);
        let batch = old.state.buckets.remove(&key).unwrap().1;
        let intent = DispatchIntent {
            base: BaseEntity::for_new_record(),
            persona_id: key.persona_id,
            trait_kind: key.trait_kind.clone(),
            base_model: key.base_model.clone(),
            submission_ids: batch.submission_ids,
            phase: DispatchPhase::Dispatching,
            is_active: true,
        };
        let id = entity_id(&intent.base).unwrap();
        old.state
            .durable
            .require()
            .unwrap()
            .dispatches
            .save(id, &intent)
            .await
            .unwrap();
        old.state
            .durable
            .require()
            .unwrap()
            .assign(&intent)
            .await
            .unwrap();
        let (next, executor) = build_runtime(adapter, true).await;
        next.tick().await.unwrap();
        let flushed = executor
            .execute_json(
                "genome/training-trigger/flush",
                serde_json::json!({
                    "personaId": persona, "traitKind": "code", "baseModel": "synthetic",
                }),
            )
            .await
            .unwrap();
        assert_eq!(flushed["success"], false);
        assert!(flushed.get("jobHandle").is_none());
        let active = next.state.active_dispatches.get(&key).unwrap();
        assert_eq!(entity_id(&active.intent.base).unwrap(), id);
        assert!(matches!(
            active.intent.phase,
            DispatchPhase::RecoveryRequired { .. }
        ));
        assert_eq!(active.batch.examples.len(), 1);
    }

    // what this catches (the 5090, 2026-10-10: thirteen cargo-test fixture rows from
    // September in the production jobs ledger refused every resolve): a malformed row BEHIND
    // the intent's recovery cursor is the prefix recovery already scanned and no longer refuses;
    // the same row read from 0, or one AFTER the cursor, still refuses; a registration after it
    // is still found.
    #[test]
    fn resolve_reads_the_ledger_from_the_intents_recovery_cursor() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("jobs-ledger.jsonl");
        let junk = "{this is not a ledger row\n";
        std::fs::write(&path, junk).unwrap();
        let board = crate::genome::fine_tuning::TrainingJobBoard::with_ledger(Some(path.clone()));
        let dispatch = Uuid::new_v4();
        let past = junk.len() as u64;
        assert!(registration_from(&board, dispatch, 0).is_err(), "from 0 the fixture row could be anything");
        assert!(registration_from(&board, dispatch, past).unwrap().is_none(), "behind the cursor: already judged");
        let job = Uuid::new_v4();
        let row = serde_json::json!({
            "event": "registered", "local_id": job.to_string(), "trigger_dispatch_id": dispatch.to_string(),
            "provider_id": "engine-local", "provider_job_id": job.to_string(),
            "persona_id": Uuid::new_v4().to_string(), "persona_name": "p", "base_model": "m", "trait_kind": "code",
        });
        let mut text = std::fs::read_to_string(&path).unwrap();
        text.push_str(&format!("{row}\n{junk}"));
        std::fs::write(&path, text).unwrap();
        let found = registration_from(&board, dispatch, past).unwrap().expect("the registration after the cursor is found");
        assert_eq!(found.local_id, job);
        assert!(registration_from(&board, Uuid::new_v4(), past).is_err(), "a malformed row after the cursor still refuses");
    }

    /// The fixture every resolve test starts from: an intent persisted mid-dispatch by one
    /// core, found RecoveryRequired by the next, with no job registered under it.
    async fn recovery_required_intent() -> (
        Arc<crate::modules::training_trigger::TrainingTriggerModule>,
        Arc<crate::runtime::CommandExecutor>,
        BucketKey,
        Uuid,
        tempfile::TempDir,
    ) {
        let (adapter, dir) = crate::orm::store::fresh_adapter().await;
        let (old, executor) = build_runtime(adapter.clone(), false).await;
        let persona = Uuid::new_v4();
        let key = BucketKey { persona_id: persona, trait_kind: "code".into(), base_model: "synthetic".into() };
        let receipt = executor
            .execute_json(
                "genome/training-trigger/submit",
                // five: the local trainer needs one full batch (as flush_dispatches_partial_bucket)
                submit_params(persona, "code", (0..5).map(|i| ex(&format!("p-{i}"), "c")).collect(), Some(100)),
            )
            .await
            .unwrap();
        assert_eq!(receipt["success"], true);
        let batch = old.state.buckets.remove(&key).unwrap().1;
        let intent = DispatchIntent {
            base: BaseEntity::for_new_record(),
            persona_id: key.persona_id,
            trait_kind: key.trait_kind.clone(),
            base_model: key.base_model.clone(),
            submission_ids: batch.submission_ids,
            phase: DispatchPhase::Dispatching,
            is_active: true,
        };
        let id = entity_id(&intent.base).unwrap();
        let store = old.state.durable.require().unwrap();
        store.dispatches.save(id, &intent).await.unwrap();
        store.assign(&intent).await.unwrap();
        let (next, executor) = build_runtime(adapter, true).await;
        next.tick().await.unwrap();
        let active = next.state.active_dispatches.get(&key).unwrap();
        assert!(matches!(active.intent.phase, DispatchPhase::RecoveryRequired { .. }));
        drop(active);
        (next, executor, key, id, dir)
    }

    // what this catches (card d24e3f25, the 5090 on 2026-10-10: 113 examples held on
    // dispatch 0512a303, which never registered a job): recovery refuses forever, correctly;
    // an operator settling it as not-dispatched makes it retryable and the SAME batch
    // dispatches on the next pass, through the normal path, with nothing re-created by guess.
    #[tokio::test]
    async fn a_recovery_required_intent_settled_as_not_dispatched_retries_from_its_own_batch() {
        let (next, executor, key, id, _dir) = recovery_required_intent().await;
        let report = next
            .state
            .resolve_dispatch(id, DispatchResolution::NotDispatched, "the ledger has no line for it".into(), None)
            .await
            .expect("settled");
        assert_eq!(report.resolution, Resolved::Retryable);
        assert_eq!(report.examples, 5);
        assert!(matches!(
            next.state.active_dispatches.get(&key).unwrap().intent.phase,
            DispatchPhase::Retryable { ref error } if error.contains("not dispatched")
        ));
        let flushed = executor
            .execute_json(
                "genome/training-trigger/flush",
                serde_json::json!({ "personaId": key.persona_id, "traitKind": "code", "baseModel": "synthetic" }),
            )
            .await
            .unwrap();
        assert_eq!(flushed["success"], true, "{flushed}");
        assert_eq!(flushed["outcome"], "JobDispatched", "{flushed}");
        assert_eq!(flushed["examplesUsed"], 5, "the same batch, not a copy: {flushed}");
        let settled_twice = next
            .state
            .resolve_dispatch(id, DispatchResolution::NotDispatched, "again".into(), None)
            .await;
        assert!(matches!(settled_twice, Err(("Invalid", _))), "a finished intent is not settled again: {settled_twice:?}");
    }

    // what this catches: named evidence never beats the ledger. A job registered under the
    // dispatch contradicts not-dispatched; the dispatched arm adopts exactly that job and
    // refuses any other; a job from another bucket is refused by name.
    #[tokio::test]
    async fn a_registered_job_contradicts_not_dispatched_and_is_what_the_dispatched_arm_adopts() {
        use crate::genome::fine_tuning::job_board::WatchedJob;
        let (next, _executor, key, id, _dir) = recovery_required_intent().await;
        let job = Uuid::new_v4();
        let watched = |local: Uuid, dispatch: Option<Uuid>, trait_kind: &str| WatchedJob {
            trigger_dispatch_id: dispatch,
            handle: JobHandle { provider_id: "engine-local".into(), provider_job_id: local.to_string(), local_id: local },
            persona_id: key.persona_id,
            persona_name: "test-p".into(),
            base_model: "synthetic".into(),
            trait_kind: trait_kind.into(),
            eval_set: None,
            signature: None,
            decision: None,
        };
        next.state.test_job_board.register(watched(job, Some(id), "code"));
        let other = Uuid::new_v4();
        next.state.test_job_board.register(watched(other, None, "chat"));
        let contradicted = next
            .state
            .resolve_dispatch(id, DispatchResolution::NotDispatched, "wrong".into(), None)
            .await;
        assert!(matches!(contradicted, Err(("Contradicted", _))), "{contradicted:?}");
        let wrong_job = next
            .state
            .resolve_dispatch(id, DispatchResolution::Dispatched { job: other }, "wrong job".into(), None)
            .await;
        assert!(matches!(wrong_job, Err(("Contradicted", _))), "{wrong_job:?}");
        let adopted = next
            .state
            .resolve_dispatch(id, DispatchResolution::Dispatched { job }, "the ledger names it".into(), None)
            .await
            .expect("adopted");
        assert_eq!(adopted.resolution, Resolved::Dispatched);
        assert_eq!(adopted.job_id, Some(job));
        let store = next.state.durable.require().unwrap();
        let intent = store.dispatches.find_by_id(id).await.unwrap().unwrap();
        assert!(matches!(intent.phase, DispatchPhase::Dispatched { ref handle, .. } if handle.local_id == job));
        assert!(!intent.is_active);
        assert!(next.state.active_dispatches.get(&key).is_none_or(|a| entity_id(&a.intent.base).ok() != Some(id)));
    }
}
