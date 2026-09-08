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
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
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
    pub(crate) fn same_policy(&self, other: &Self) -> bool {
        self.source == other.source
            && self.lora == other.lora
            && self.schedule == other.schedule
            && self.validation_split == other.validation_split
            && self.local_artifact_dir == other.local_artifact_dir
            && self.preferred_provider == other.preferred_provider
            && self.eval_set == other.eval_set
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
        current.examples.append(&mut incoming.examples);
    } else {
        incoming.submission_ids.push(id);
        *current = Some(incoming);
    }
    Ok(())
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
        self.active_dispatches.contains_key(key)
            || self
                .buckets
                .get(key)
                .is_some_and(|batch| batch.examples.len() >= batch.min_examples as usize)
    }

    fn contains_submission(&self, key: &BucketKey, id: Uuid) -> bool {
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
            Ok((handle, provider)) => self.finish_dispatch(key, &active, handle, provider).await,
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
                            !state
                                .active_dispatches
                                .get(&key)
                                .is_some_and(|active| active.intent.base.id == id.to_string())
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
}
