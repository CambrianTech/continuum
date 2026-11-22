//! Re-keying a bucket written under a base spelling that is not the catalog id
//! (`genome/training-trigger/rekey`). Submit keys every new row by the catalog id
//! (`base_identity::canonical_base`, #6222); rows accepted before it keep the spelling they
//! arrived with, a second bucket for one base that no learner on that row ever matches (the
//! 5090, 2026-11-22: Kimi's reviewed correction b38e0149, 8 examples under the GGUF stem
//! `Qwen3.8-27B-Q4_K_M`, beside 26,407 under `ggml-org/Qwen3.8-27B-GGUF`).
//!
//! The preview reads and changes nothing. An apply names its exact source bucket and moves every
//! row the trainer keeps for it (submissions, dispatch intents, retirements, policy corrections)
//! to the catalog id in ONE atomic batch, ids unchanged. It runs as an owned operation holding
//! both buckets' gates (in key order) and then every engine's learning lock (in incarnation order):
//! the same gate-before-engine order every pass takes. Every trainer writer of a bucket's rows
//! holds that bucket's gate, so none interleaves; the generic `data/*` verbs take no trainer gate
//! and are outside this guarantee. A source with a learn checkpoint, or a learner seated on it,
//! is refused: its generation and volatile progress are keyed by the source and cannot move by a
//! column write.

use super::*;
use crate::orm::types::{BatchOperation, BatchOperationType};

/// The four collections a bucket's rows live in, keyed by `(personaId, traitKind, baseModel)`.
fn collections() -> [&'static str; 4] {
    [Submission::COLLECTION, DispatchIntent::COLLECTION, Retirement::COLLECTION, policy::PolicyCorrection::COLLECTION]
}

/// How many rows a bucket holds in each collection.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct RekeyCounts {
    pub(crate) submissions: usize,
    pub(crate) dispatches: usize,
    pub(crate) retirements: usize,
    pub(crate) policy_corrections: usize,
}

impl RekeyCounts {
    fn of(counts: [usize; 4]) -> Self {
        let [submissions, dispatches, retirements, policy_corrections] = counts;
        Self { submissions, dispatches, retirements, policy_corrections }
    }

    pub(crate) fn rows(&self) -> usize {
        self.submissions + self.dispatches + self.retirements + self.policy_corrections
    }
}

/// One bucket keyed by a spelling that is not a catalog id, as the preview finds it.
#[derive(Debug, Clone)]
pub(crate) struct RekeyCandidate {
    pub(crate) from: BucketKey,
    /// The catalog id its base resolves to, or why it names none.
    pub(crate) to: Result<String, crate::model_registry::base_identity::BaseRefused>,
    pub(crate) counts: RekeyCounts,
    pub(crate) checkpointed: CheckpointedSides,
}

/// Which side of a re-key holds a learn checkpoint. A source that does is refused on apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/training_trigger/CheckpointedSides.ts")]
pub enum CheckpointedSides {
    Neither,
    Source,
    Destination,
    Both,
}

impl CheckpointedSides {
    fn of(source: bool, destination: bool) -> Self {
        match (source, destination) {
            (false, false) => Self::Neither,
            (true, false) => Self::Source,
            (false, true) => Self::Destination,
            (true, true) => Self::Both,
        }
    }
}

/// What an apply moved.
#[derive(Debug, Clone)]
pub(crate) struct Rekeyed {
    pub(crate) from: BucketKey,
    pub(crate) to: BucketKey,
    pub(crate) moved: RekeyCounts,
    /// What the destination holds now.
    pub(crate) destination: RekeyCounts,
}

/// Which buckets a preview considers.
#[derive(Debug, Clone, Default)]
pub(crate) struct RekeyFilter {
    pub(crate) persona_id: Option<Uuid>,
    pub(crate) trait_kind: Option<String>,
    pub(crate) from_base: Option<String>,
}

/// Why a re-key read or moved nothing.
#[derive(Debug, Clone, thiserror::Error)]
pub(crate) enum RekeyRefused {
    #[error("the trainer's store is unavailable: {0}")]
    Unavailable(String),
    #[error("the trainer's store failed: {0}")]
    Store(String),
    #[error("bucket base '{0}' is already a catalog id: nothing to re-key")]
    AlreadyCanonical(String),
    #[error("no rows are keyed by this bucket: nothing to re-key")]
    NoSuchBucket,
    #[error(transparent)]
    Base(#[from] crate::model_registry::base_identity::BaseRefused),
    #[error("the source bucket has a learn checkpoint: its generation is keyed by the source and does not move with its rows")]
    SourceCheckpointed,
    #[error("a learner of the source bucket is seated on engine {0:?}: re-key once she pages out")]
    SourceSeated(crate::inference::engine_residency::EngineIncarnation),
    #[error("the source and destination buckets pin different training policies: merging them would train one bucket under two")]
    PolicyDiffers,
    #[error("the batch committed but the counts do not balance: source left {left:?}, destination {destination:?}")]
    Unbalanced { left: RekeyCounts, destination: RekeyCounts },
}

impl RekeyRefused {
    /// The wire's `errorKind`.
    pub(crate) fn kind(&self) -> &'static str {
        match self {
            Self::Unavailable(_) => "PersistenceUnavailable",
            Self::Store(_) => "PersistenceFailed",
            Self::AlreadyCanonical(_) => "AlreadyCanonical",
            Self::NoSuchBucket => "NoSuchBucket",
            Self::Base(_) => "BaseRefused",
            Self::SourceCheckpointed => "SourceCheckpointed",
            Self::SourceSeated(_) => "SourceSeated",
            Self::PolicyDiffers => "InconsistentBucket",
            Self::Unbalanced { .. } => "Unbalanced",
        }
    }
}

/// The trainer's bucket membership as a pass read it before selecting its rows
/// ([`TrainingTriggerState::membership`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Membership(u64);

/// The bucket columns of a row, read without its payload.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BucketColumns {
    sequence: u64,
    persona_id: Uuid,
    trait_kind: String,
    base_model: String,
}

impl DurableStore {
    async fn counts_of(&self, key: &BucketKey) -> Result<RekeyCounts, RekeyRefused> {
        let mut counts = [0usize; 4];
        for (slot, collection) in counts.iter_mut().zip(collections()) {
            let result = self.adapter.count(key.query(collection).build()).await;
            *slot = match (result.success, result.data) {
                (true, Some(count)) => count,
                _ => return Err(RekeyRefused::Store(result.error.unwrap_or_else(|| format!("{collection} count refused")))), // the adapter refused without detail; still a refusal
            };
        }
        Ok(RekeyCounts::of(counts))
    }

    /// The ids of every row `key` holds in `collection`, read without their payloads.
    async fn ids_of(&self, key: &BucketKey, collection: &str) -> Result<Vec<String>, RekeyRefused> {
        let result = self.adapter.query(key.query(collection).select(vec!["baseModel".into()]).build()).await;
        match (result.success, result.data) {
            (true, Some(rows)) => Ok(rows.into_iter().map(|row| row.id).collect()),
            _ => Err(RekeyRefused::Store(result.error.unwrap_or_else(|| format!("{collection} query refused")))), // the adapter refused without detail; still a refusal
        }
    }

    /// Every bucket with a submission keyed by a base that is not one of `catalog_ids`, under `filter`.
    async fn stale_buckets(&self, catalog_ids: &[String], filter: &RekeyFilter) -> Result<std::collections::BTreeSet<BucketKey>, RekeyRefused> {
        const PAGE: usize = 1024;
        let mut found = std::collections::BTreeSet::new();
        let mut after = 0u64;
        loop {
            // one filter per field: a named base is matched exactly, otherwise every non-catalog base
            let base = match &filter.from_base {
                Some(base) => QueryOperator::Eq(base.clone().into()),
                None => QueryOperator::NotIn(catalog_ids.iter().map(|id| id.as_str().into()).collect()),
            };
            let mut query = QueryBuilder::new(Submission::COLLECTION).filter("baseModel", base).filter("sequence", QueryOperator::Gt(after.into())); // bounded: paged PAGE rows at a time along the sequence cursor (limit below)
            if let Some(persona) = filter.persona_id {
                query = query.filter_eq("personaId", persona.to_string());
            }
            if let Some(trait_kind) = &filter.trait_kind {
                query = query.filter_eq("traitKind", trait_kind.as_str());
            }
            let fields = ["personaId", "traitKind", "baseModel", "sequence"].map(String::from).to_vec();
            let result = self.adapter.query(query.sort_asc("sequence").limit(PAGE).select(fields).build()).await;
            let rows = match (result.success, result.data) {
                (true, Some(rows)) => rows,
                _ => return Err(RekeyRefused::Store(result.error.unwrap_or_else(|| "stale bucket query refused".into()))), // the adapter refused without detail; still a refusal
            };
            let full = rows.len() == PAGE;
            for row in rows {
                let columns: BucketColumns = row.decode().map_err(|e| RekeyRefused::Store(e.to_string()))?;
                after = after.max(columns.sequence);
                found.insert(BucketKey { persona_id: columns.persona_id, trait_kind: columns.trait_kind, base_model: columns.base_model });
            }
            if !full {
                return Ok(found);
            }
        }
    }

    /// Whether `from` and `to` agree on the policy their oldest pending rows pin (an empty side agrees).
    async fn policies_agree(&self, from: &BucketKey, to: &BucketKey) -> Result<bool, RekeyRefused> {
        let mut pinned = Vec::with_capacity(2);
        for key in [from, to] {
            let first = self.pending_page(key, 0, 1).await.map_err(RekeyRefused::Store)?;
            if let Some(row) = first.first() {
                pinned.push(self.effective_batch(row).await.map_err(RekeyRefused::Store)?);
            }
        }
        Ok(match pinned.as_slice() {
            [a, b] => a.same_policy(b),
            _ => true,
        })
    }
}

impl TrainingTriggerState {
    /// Bucket membership now: a pass reads it BEFORE selecting its rows and checks it under its
    /// bucket's gate before seating or stepping; a move committed in between ends that pass, and
    /// the next pass selects the rows where they are now (Codex on #6226).
    pub(crate) fn membership(&self) -> Membership {
        Membership(self.membership.load(std::sync::atomic::Ordering::Acquire))
    }

    /// Whether rows moved between buckets since `seen`.
    pub(crate) fn membership_moved_since(&self, seen: Membership) -> bool {
        self.membership() != seen
    }

    fn store_for_rekey(&self) -> Result<&DurableStore, RekeyRefused> {
        self.require_ready().map_err(RekeyRefused::Unavailable)?;
        self.durable.require().map(Arc::as_ref).map_err(RekeyRefused::Unavailable)
    }

    /// Every bucket keyed by a base that is not a catalog id, what it would move to, and what it
    /// holds. Reads only.
    pub(crate) async fn rekey_preview(&self, filter: RekeyFilter) -> Result<Vec<RekeyCandidate>, RekeyRefused> {
        let store = self.store_for_rekey()?;
        let catalog = self.models_now();
        let catalog_ids: Vec<String> = catalog.models.keys().cloned().collect();
        let mut candidates = Vec::new();
        for from in store.stale_buckets(&catalog_ids, &filter).await? {
            let to = crate::model_registry::base_identity::canonical_base(&catalog, &from.base_model);
            if to.as_ref().is_ok_and(|base| *base == from.base_model) {
                continue; // a named base that is already a catalog id: nothing to move
            }
            let destination = match &to {
                Ok(base) => store.checkpoint_of(&BucketKey { base_model: base.clone(), ..from.clone() }).await.map_err(RekeyRefused::Store)?.is_some(),
                Err(_) => false,
            };
            candidates.push(RekeyCandidate {
                counts: store.counts_of(&from).await?,
                checkpointed: CheckpointedSides::of(store.checkpoint_of(&from).await.map_err(RekeyRefused::Store)?.is_some(), destination),
                from,
                to,
            });
        }
        Ok(candidates)
    }

    /// Move every row of bucket `from` to its base's catalog id, atomically, as an owned operation
    /// holding both buckets' gates and every engine (module doc). A replay after the move finds no
    /// rows and is refused `NoSuchBucket`, having changed nothing.
    pub(crate) async fn rekey_apply(self: &Arc<Self>, from: &BucketKey) -> Result<Rekeyed, RekeyRefused> {
        self.store_for_rekey()?;
        let from = from.clone(); // owned by the operation it runs as
        let base = crate::model_registry::base_identity::canonical_base(&self.models_now(), &from.base_model)?;
        if base == from.base_model {
            return Err(RekeyRefused::AlreadyCanonical(base));
        }
        let to = BucketKey { base_model: base, ..from.clone() };
        // gates in key order, as any two-bucket holder takes them
        let (first, second) = if from < to { (from.clone(), to.clone()) } else { (to.clone(), from.clone()) };
        let first_lease = self.submit_gates.acquire(&first).await;
        let second_lease = self.submit_gates.acquire(&second).await;
        self.run_leased(first_lease, first, move |state, _| async move {
            let _second = second_lease;
            state.rekey_holding_gates(from, to).await
        })
        .await
        .map_err(RekeyRefused::Unavailable)?
    }

    /// [`Self::rekey_apply`], both gates held: hold every engine, re-read, and move.
    async fn rekey_holding_gates(self: Arc<Self>, from: BucketKey, to: BucketKey) -> Result<Rekeyed, RekeyRefused> {
        let engines = self.in_place.hold_every_engine().await;
        if let Some(engine) = engines.seating(&from) {
            return Err(RekeyRefused::SourceSeated(engine));
        }
        let store = self.durable.require().map_err(RekeyRefused::Unavailable)?;
        let moved = store.counts_of(&from).await?;
        if moved.rows() == 0 {
            return Err(RekeyRefused::NoSuchBucket);
        }
        if store.checkpoint_of(&from).await.map_err(RekeyRefused::Store)?.is_some() {
            return Err(RekeyRefused::SourceCheckpointed);
        }
        if !store.policies_agree(&from, &to).await? {
            return Err(RekeyRefused::PolicyDiffers);
        }
        let before = store.counts_of(&to).await?;
        let mut operations = Vec::with_capacity(moved.rows());
        for collection in collections() {
            for id in store.ids_of(&from, collection).await? {
                operations.push(BatchOperation {
                    operation_type: BatchOperationType::Update,
                    collection: collection.to_string(),
                    id: Some(id),
                    data: Some(serde_json::json!({ "baseModel": to.base_model })),
                });
            }
        }
        let result = store.adapter.batch(operations).await;
        if !result.success {
            return Err(RekeyRefused::Store(result.error.unwrap_or_else(|| "the re-key batch was refused".into()))); // the adapter refused without detail; still a refusal
        }
        // still under both gates: every pass that selected rows before this move stops at its gate
        self.membership.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        let left = store.counts_of(&from).await?;
        let destination = store.counts_of(&to).await?;
        let expected = RekeyCounts {
            submissions: before.submissions + moved.submissions,
            dispatches: before.dispatches + moved.dispatches,
            retirements: before.retirements + moved.retirements,
            policy_corrections: before.policy_corrections + moved.policy_corrections,
        };
        if left.rows() != 0 || destination != expected {
            return Err(RekeyRefused::Unbalanced { left, destination });
        }
        drop(engines);
        continuum_base::probe!(
            class = "training.trigger.rekeyed",
            persona = %from.persona_id,
            trait_kind = %from.trait_kind,
            from = %from.base_model,
            to = %to.base_model,
            submissions = moved.submissions,
            dispatches = moved.dispatches,
            retirements = moved.retirements,
            policy_corrections = moved.policy_corrections,
            "a bucket keyed by a non-catalog base spelling moved to its catalog id, ids unchanged"
        );
        Ok(Rekeyed { from, to, moved, destination })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::training_trigger::test_support::{build_runtime, ex, submit_params, FIXTURE_HF};

    const CANONICAL: &str = "fixture-base";

    fn key(persona: Uuid, base: &str) -> BucketKey {
        BucketKey { persona_id: persona, trait_kind: "code".into(), base_model: base.into() }
    }

    fn batch(prompts: &[&str], provider: Option<&str>) -> PendingBatch {
        PendingBatch {
            returned: false,
            persona_name: "test-p".into(),
            source: serde_json::from_value(serde_json::json!("operator_curated")).expect("test: source"),
            examples: prompts.iter().map(|p| ex(p, "c")).collect(),
            lora: None,
            schedule: None,
            local_artifact_dir: None,
            preferred_provider: provider.map(str::to_owned),
            validation_split: 0.1,
            eval_set: None,
        }
    }

    /// A row accepted under `key` as given: how rows arrived before submit resolved every base.
    async fn seed(state: &Arc<TrainingTriggerState>, persona: Uuid, base: &str, batch: PendingBatch) -> Uuid {
        let id = Uuid::new_v4();
        state
            .run_owned(key(persona, base), move |state, key| async move { state.accept(&key, Some(id), batch).await })
            .await
            .expect("test: admitted")
            .expect("test: accepted");
        id
    }

    async fn runtime() -> (Arc<super::super::super::TrainingTriggerModule>, Arc<crate::runtime::CommandExecutor>, tempfile::TempDir) {
        let (adapter, dir) = crate::orm::store::fresh_adapter().await;
        let (trigger, executor) = build_runtime(adapter).await;
        (trigger, executor, dir)
    }

    // what this catches (the 5090, 2026-11-22: b38e0149's 8 examples keyed by a GGUF stem beside
    // 26,407 under the catalog id): a preview that misses a stale bucket, lists a canonical one, or
    // writes anything. It names the stale bucket, where it would move and what it holds, and both
    // buckets read the same afterwards.
    #[tokio::test]
    async fn a_preview_names_the_stale_bucket_and_changes_nothing() {
        let (trigger, executor, _dir) = runtime().await;
        let persona = Uuid::new_v4();
        let stale = key(persona, FIXTURE_HF);
        seed(&trigger.state, persona, FIXTURE_HF, batch(&["s-1", "s-2"], None)).await;
        let mut canonical = submit_params(persona, "code", vec![ex("c-1", "c")]);
        canonical["baseModel"] = serde_json::json!(CANONICAL);
        executor.execute_json("genome/training-trigger/submit", canonical).await.expect("test: canonical submit");

        let found = trigger.state.rekey_preview(RekeyFilter { persona_id: Some(persona), ..RekeyFilter::default() }).await.expect("test: preview");
        assert_eq!(found.len(), 1, "only the stale bucket: {found:?}");
        assert_eq!(found[0].from, stale);
        assert_eq!(found[0].to.as_deref(), Ok(CANONICAL));
        assert_eq!(found[0].counts, RekeyCounts { submissions: 1, ..RekeyCounts::default() });
        assert_eq!(found[0].checkpointed, CheckpointedSides::Neither);
        assert_eq!(trigger.state.bucket_example_count(persona, "code", FIXTURE_HF).await, Some(2), "the preview moved nothing");
        assert_eq!(trigger.state.bucket_example_count(persona, "code", CANONICAL).await, Some(1));
    }

    // what this catches: an apply that loses a row, mints new ids (a producer's retry would then
    // land a second copy), leaves a retirement behind under the old key, or moves twice. Every row
    // moves with its id; the source is empty; a replay moves nothing and says why.
    #[tokio::test]
    async fn an_apply_moves_every_row_with_its_id_and_a_replay_moves_nothing() {
        let (trigger, _executor, _dir) = runtime().await;
        let persona = Uuid::new_v4();
        let stale = key(persona, FIXTURE_HF);
        let kept = seed(&trigger.state, persona, FIXTURE_HF, batch(&["s-1", "s-2"], None)).await;
        let withdrawn = seed(&trigger.state, persona, FIXTURE_HF, batch(&["s-3"], None)).await;
        trigger.state.retire(RetireSelection::Submissions(vec![withdrawn]), "superseded".into(), None).await.expect("test: retired");

        let moved = trigger.state.rekey_apply(&stale).await.expect("test: applied");
        assert_eq!(moved.to, key(persona, CANONICAL));
        assert_eq!(moved.moved, RekeyCounts { submissions: 2, retirements: 1, ..RekeyCounts::default() });
        assert_eq!(moved.destination, moved.moved, "the destination was empty, so it holds exactly what moved");
        let store = trigger.state.durable.require().expect("test: store");
        let row = store.submissions.find_by_id(kept).await.expect("test: read").expect("test: the row keeps its id");
        assert_eq!(row.base_model, CANONICAL);
        assert_eq!(store.counts_of(&stale).await.expect("test: counts").rows(), 0, "nothing is left under the stale key");
        assert_eq!(trigger.state.bucket_example_count(persona, "code", CANONICAL).await, Some(2), "her pending examples are in the catalog bucket");

        let replay = trigger.state.rekey_apply(&stale).await;
        assert!(matches!(replay, Err(RekeyRefused::NoSuchBucket)), "{replay:?}");
    }

    // what this catches: a source whose learning is already published (its generation is keyed by
    // the source) moved by a column write, orphaning the generation; or two policies merged into one
    // bucket. Each is refused and nothing moves.
    #[tokio::test]
    async fn a_checkpointed_source_or_a_differing_policy_is_refused_and_nothing_moves() {
        let (trigger, executor, _dir) = runtime().await;
        let persona = Uuid::new_v4();
        let stale = key(persona, FIXTURE_HF);
        let id = seed(&trigger.state, persona, FIXTURE_HF, batch(&["s-1"], Some("elsewhere"))).await;
        let mut canonical = submit_params(persona, "code", vec![ex("c-1", "c")]);
        canonical["baseModel"] = serde_json::json!(CANONICAL);
        executor.execute_json("genome/training-trigger/submit", canonical).await.expect("test: canonical submit");
        let differs = trigger.state.rekey_apply(&stale).await;
        assert!(matches!(differs, Err(RekeyRefused::PolicyDiffers)), "{differs:?}");

        let generation = Generation { name: "g1".into(), path: "/nowhere".into(), binding: "b".into(), steps: 1, provenance: serde_json::json!({}), persona_name: "test-p".into(), signature: None };
        trigger.state.durable.require().expect("test: store").commit_checkpoint(&stale, generation, &[Position { submission: id, learned: 0 }]).await.expect("test: checkpoint");
        let checkpointed = trigger.state.rekey_apply(&stale).await;
        assert!(matches!(checkpointed, Err(RekeyRefused::SourceCheckpointed)), "{checkpointed:?}");
        assert_eq!(trigger.state.bucket_example_count(persona, "code", FIXTURE_HF).await, Some(1), "the source keeps its row");
    }

    // what this catches (Codex on #6226 @849c32d0e): a learning pass that selected its rows, then
    // waited for her act holding nothing, while a re-key moved those rows; it would take the old
    // bucket's gate and seat and learn the moved examples under the old key. The pass reads the
    // membership before its rows and checks it under its gate: a move in between ends it with nothing
    // seated or learned, and the rows stay where the re-key put them.
    #[tokio::test]
    async fn a_pass_that_selected_its_rows_before_a_move_seats_and_learns_nothing() {
        let (trigger, _executor, _dir) = runtime().await;
        let persona = Uuid::new_v4();
        let stale = key(persona, FIXTURE_HF);
        seed(&trigger.state, persona, FIXTURE_HF, batch(&["s-1"], None)).await;
        let seen = trigger.state.membership(); // the pass reads it first, as learn_in_place does
        let store = trigger.state.durable.require().expect("test: store");
        let rows: Vec<Arrival> = store.arrivals(0, 16).await.expect("test: arrivals").into_iter().filter(|row| row.key == stale).collect();
        assert_eq!(rows.len(), 1, "the pass selected her stale row");
        trigger.state.rekey_apply(&stale).await.expect("test: the move commits while the pass waits");
        let engine = crate::inference::engine_residency::EngineIncarnation::of(std::process::id(), 9).expect("test: this process's start time");
        let pass = trigger.state.learn_bucket(stale, rows, engine, seen).await.expect("test: the pass ends cleanly");
        assert_eq!(pass, super::super::super::in_place::BucketPass::default(), "nothing seated, learned, deferred or checkpointed");
        assert_eq!(trigger.state.bucket_example_count(persona, "code", CANONICAL).await, Some(1), "her row is where the move put it, unlearned");
    }

    // what this catches: an apply that reads and rewrites a bucket while a trainer operation holds
    // its gate (a submit appending, a pass recording learned), losing that operation's write. The
    // apply takes the destination's gate (first in key order) and waits on the source's; it moves
    // only once the holder lets go, and counts what the holder wrote.
    #[tokio::test]
    async fn an_apply_waits_for_the_operation_holding_the_source_gate() {
        let (trigger, _executor, _dir) = runtime().await;
        let persona = Uuid::new_v4();
        let (stale, canonical) = (key(persona, FIXTURE_HF), key(persona, CANONICAL));
        assert!(canonical < stale, "the fixture's destination is first in key order");
        seed(&trigger.state, persona, FIXTURE_HF, batch(&["s-1"], None)).await;
        let holder = trigger.state.submit_gates.acquire(&stale).await;
        let apply = trigger.state.rekey_apply(&stale);
        tokio::pin!(apply);
        assert!(futures::poll!(&mut apply).is_pending(), "the source's gate is held: the apply waits");
        assert!(trigger.state.submit_gates.try_acquire(&canonical).is_none(), "waiting, it holds the destination's gate");
        trigger.state.accept(&stale, None, batch(&["s-2"], None)).await.expect("test: the holder's own write");
        drop(holder);
        let moved = apply.await.expect("test: applied");
        assert_eq!(moved.moved.submissions, 2, "the holder's write moved with the rest");
        assert!(trigger.state.submit_gates.try_acquire(&canonical).is_some(), "and both gates are free again");
    }
}
