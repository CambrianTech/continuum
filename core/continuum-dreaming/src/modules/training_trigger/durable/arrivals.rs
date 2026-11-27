//! The journal as in-place learning reads it (card 4114a2c6): accepted submissions in accepted
//! order, and how far each was learned. The row's own `learned` count is the cursor; rows commit
//! out of sequence order (`training_trigger_state_2`), so the reader scans pending rows, never a
//! sequence watermark.

use super::*;
use crate::genome::fine_tuning::types::{LoRAHyperparams, ScheduleParams, TrainingExample};

/// One accepted submission still pending, as the learner needs it.
#[derive(Debug, Clone)]
pub(crate) struct Arrival {
    pub(crate) id: Uuid,
    pub(crate) sequence: u64,
    pub(crate) key: BucketKey,
    pub(crate) persona_name: String,
    pub(crate) examples: Vec<TrainingExample>,
    /// Examples already learned, in order.
    pub(crate) learned: u32,
    pub(crate) lora: Option<LoRAHyperparams>,
    pub(crate) schedule: Option<ScheduleParams>,
}

/// One pending submission as counting reads it: no batch decoded (card d1e5675c).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Counted {
    pub(crate) id: Uuid,
    pub(crate) sequence: u64,
    /// Examples already learned, in order.
    pub(crate) learned: u32,
    /// Examples the row holds.
    pub(crate) examples: u64,
}

/// Why a counted read of the journal failed: typed, so a caller matches it.
#[derive(Debug, thiserror::Error)]
pub(crate) enum JournalRead {
    #[error("the training journal: {0}")]
    Store(#[from] crate::orm::store::OrmStoreError),
    #[error("submission {0} is not in the journal")]
    Missing(Uuid),
    #[error("the training journal is not installed yet")]
    NotInstalled,
}

/// The counting columns of a submission row, read without its payload (a projected query).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CountColumns {
    sequence: u64,
    #[serde(default)] // the row's own default: absent on rows written before in-place learning, none learned
    learned: u32,
    #[serde(default)] // absent on rows written before the column: counted once from the batch, never guessed
    example_count: Option<u64>,
}

impl CountColumns {
    /// The id is selected for the row it pairs with (`OrmStore::find_projected`), never decoded here.
    fn names() -> Vec<String> {
        ["id", "sequence", "learned", "exampleCount"].map(String::from).to_vec()
    }
}

/// The bucket columns of a submission row, read without its payload (a projected query).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct BucketColumns {
    pub(super) sequence: u64,
    pub(super) persona_id: Uuid,
    pub(super) trait_kind: String,
    pub(super) base_model: String,
}

impl BucketColumns {
    /// The columns a projected query selects to read [`BucketColumns`].
    pub(super) fn names() -> Vec<String> {
        ["personaId", "traitKind", "baseModel", "sequence"].map(String::from).to_vec()
    }

    pub(super) fn key(self) -> BucketKey {
        BucketKey { persona_id: self.persona_id, trait_kind: self.trait_kind, base_model: self.base_model }
    }
}

/// A pending submission row as the learner reads it.
fn arrival(row: Submission) -> Result<Arrival, String> {
    Ok(arrival_at(entity_id(&row.base)?, row))
}

/// Submission `row`, whose id is `id`, as the learner reads it.
fn arrival_at(id: Uuid, row: Submission) -> Arrival {
    Arrival {
        id,
        sequence: row.sequence,
        key: BucketKey { persona_id: row.persona_id, trait_kind: row.trait_kind, base_model: row.base_model },
        learned: row.learned,
        persona_name: row.batch.persona_name,
        lora: row.batch.lora,
        schedule: row.batch.schedule,
        examples: row.batch.examples,
    }
}

impl DurableStore {
    /// Every bucket with a pending submission, read from the bucket columns alone: no example
    /// or batch is decoded, so a pass that only needs to know which buckets wait holds keys,
    /// never the journal (Codex on the 5090 replay, 2026-11-25). Paged along the sequence.
    pub(in crate::modules::training_trigger) async fn pending_keys(&self) -> Result<std::collections::BTreeSet<BucketKey>, String> {
        const PAGE: usize = 1024;
        let mut keys = std::collections::BTreeSet::new();
        let mut after = 0u64;
        loop {
            let query = QueryBuilder::new(Submission::COLLECTION)
                .filter_eq("isPending", true)
                .filter("sequence", QueryOperator::Gt(after.into()))
                .sort_asc("sequence")
                .limit(PAGE)
                .select(BucketColumns::names());
            let result = self.adapter.query(query.build()).await;
            if !result.success {
                return Err(result.error.unwrap_or_else(|| "pending bucket query refused".into())); // the query failed; this supplies its missing diagnostic, never keys
            }
            let rows = result.data.ok_or("pending bucket query omitted rows")?;
            let full = rows.len() == PAGE;
            for row in rows {
                let columns: BucketColumns = row.decode().map_err(|error| error.to_string())?; // the projected row's typed columns
                after = after.max(columns.sequence);
                keys.insert(columns.key());
            }
            if !full {
                return Ok(keys);
            }
        }
    }

    /// Bucket `key`'s pending submissions, oldest first: read through the bucket's own index,
    /// never the whole journal.
    pub(in crate::modules::training_trigger) async fn arrivals_in(&self, key: &BucketKey) -> Result<Vec<Arrival>, String> {
        const PAGE: usize = 256;
        let mut rows = Vec::new();
        let mut after = 0u64;
        loop {
            let page = self.pending_page(key, after, PAGE).await?;
            let full = page.len() == PAGE;
            for row in page {
                after = after.max(row.sequence);
                rows.push(arrival(row)?);
            }
            if !full {
                return Ok(rows);
            }
        }
    }

    /// Pending submissions with `sequence > after`, oldest first, at most `limit`.
    pub(in crate::modules::training_trigger) async fn arrivals(&self, after: u64, limit: usize) -> Result<Vec<Arrival>, String> {
        let result = self
            .adapter
            .query(
                QueryBuilder::new(Submission::COLLECTION)
                    .filter_eq("isPending", true)
                    .filter("sequence", QueryOperator::Gt(after.into()))
                    .sort_asc("sequence")
                    .limit(limit)
                    .build(),
            )
            .await;
        if !result.success {
            return Err(result.error.unwrap_or_else(|| "training arrivals query refused".into())); // the query failed; this supplies its missing diagnostic, never rows
        }
        result
            .data
            .ok_or("training arrivals query omitted rows")?
            .into_iter()
            .map(|row| arrival(row.decode().map_err(|error| error.to_string())?)) // decode the persisted ORM row into its typed owner
            .collect()
    }

    /// How many examples bucket `key` holds pending and not yet learned, counted from its rows' columns.
    /// Bucket `key`'s pending rows, oldest first, as counting reads them: ids, cursors and example
    /// counts from a projected query, never a batch (card d1e5675c). A row written before the count
    /// column has its batch read once here and the count written back, so it is never read again.
    pub(in crate::modules::training_trigger) async fn pending_counts(&self, key: &BucketKey) -> Result<Vec<Counted>, JournalRead> {
        const PAGE: usize = 1024;
        let mut counted = Vec::new();
        let mut after = 0u64;
        loop {
            let query = key
                .query(Submission::COLLECTION)
                .filter_eq("isPending", true)
                .filter("sequence", QueryOperator::Gt(after.into()))
                .sort_asc("sequence")
                .limit(PAGE)
                .select(CountColumns::names());
            let rows: Vec<(Uuid, CountColumns)> = self.submissions.find_projected(query.build()).await?;
            let full = rows.len() == PAGE;
            for (id, columns) in rows {
                after = after.max(columns.sequence);
                let examples = match columns.example_count {
                    Some(examples) => examples,
                    None => self.count_examples_of(id).await?,
                };
                counted.push(Counted { id, sequence: columns.sequence, learned: columns.learned, examples });
            }
            if !full {
                return Ok(counted);
            }
        }
    }

    /// Count submission `id`'s examples from its batch and write the count to its row: once per row
    /// written before the count column.
    async fn count_examples_of(&self, id: Uuid) -> Result<u64, JournalRead> {
        let mut row = self.submissions.find_by_id(id).await?.ok_or(JournalRead::Missing(id))?;
        let examples = row.batch.examples.len() as u64; // as: a usize fits a u64 on every target this builds for
        row.example_count = Some(examples);
        self.submissions.update(id, &row).await?;
        continuum_base::probe!(class = "training.journal.count_backfilled", submission = %id, examples, "a row written before the count column was counted once and its count written");
        Ok(examples)
    }

    /// Submission `id` as the learner reads it, one row decoded: its next lesson and her policy are
    /// read from it, never from her whole history.
    pub(in crate::modules::training_trigger) async fn arrival_of(&self, id: Uuid) -> Result<Arrival, JournalRead> {
        let row = self.submissions.find_by_id(id).await?.ok_or(JournalRead::Missing(id))?;
        Ok(arrival_at(id, row))
    }

    /// The examples submission `id` carries, pending or not: what a generation consumed from it.
    pub(in crate::modules::training_trigger) async fn submission_examples(&self, id: Uuid) -> Result<Vec<TrainingExample>, String> {
        let row = self.submissions.find_by_id(id).await.map_err(|e| e.to_string())?.ok_or_else(|| format!("submission {id} is not in the journal"))?;
        Ok(row.batch.examples)
    }

    /// Record that submission `id` has its first `learned` examples learned; when that is all of
    /// them it leaves pending. Never moves backwards, and never past the examples it holds.
    pub(in crate::modules::training_trigger) async fn record_learned(&self, id: Uuid, learned: u32) -> Result<(), String> {
        let mut row = self.submissions.find_by_id(id).await.map_err(|e| e.to_string())?.ok_or_else(|| format!("submission {id} is not in the journal"))?;
        let total = u32::try_from(row.batch.examples.len()).map_err(|_| format!("submission {id} holds more examples than a count can name"))?;
        if learned > total {
            return Err(format!("submission {id} holds {total} examples, not {learned}"));
        }
        if learned < row.learned {
            return Err(format!("submission {id} already learned {} examples; a cursor never moves back to {learned}", row.learned));
        }
        row.learned = learned;
        row.example_count = Some(u64::from(total)); // counted here too: a row the learner moves is never counted from its batch again
        row.is_pending = learned < total;
        self.submissions.update(id, &row).await.map_err(|e| e.to_string())
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    pub(in crate::modules::training_trigger::durable) fn submission(sequence: u64, examples: usize) -> (Uuid, Submission) {
        submission_for(sequence, examples, "qwen")
    }

    fn submission_for(sequence: u64, examples: usize, base_model: &str) -> (Uuid, Submission) {
        let base = BaseEntity::for_new_record();
        let id = entity_id(&base).expect("test: a fresh entity id");
        let example = |n: usize| TrainingExample { prompt: format!("p{n}"), completion: "c".into(), metadata: None, lived: None };
        let batch = PendingBatch {
            returned: false,
            persona_name: "Kimi".into(),
            source: crate::genome::fine_tuning::TrainingSource::OperatorCurated,
            examples: (0..examples).map(example).collect(),
            lora: None,
            schedule: None,
            local_artifact_dir: None,
            preferred_provider: None,
            validation_split: 0.0,
            eval_set: None,
        };
        let row = Submission {
            base,
            sequence,
            persona_id: Uuid::nil(),
            trait_kind: "code".into(),
            base_model: base_model.into(),
            batch,
            is_pending: true,
            dispatch_id: None,
            retirement_id: None,
            learned: 0,
            example_count: Some(examples as u64), // as: a fixture count fits
            carried_to: None,
            carried_from: None,
            carried_offset: 0,
        };
        (id, row)
    }

    // what this catches: the learned cursor moving backwards, past its examples, or leaving a row
    // pending once every example is learned (or pending-free before then). Arrivals read it back.
    // what this catches (card d1e5675c, the 5090: decoding Kimi's 784 rows took 34.7 s and freeing them
    // 340.4 s): a count that differs from the rows it counts, or a row written before the count column
    // that is guessed (zero) instead of counted, or counted from its batch on every read.
    #[tokio::test]
    async fn pending_counts_match_the_rows_and_a_row_without_its_count_is_counted_once() {
        let (adapter, _dir) = crate::orm::store::fresh_adapter().await;
        let store = DurableStore::new(adapter).await.expect("test: store");
        let (counted_id, counted) = submission(1, 3);
        let key = counted.key();
        store.submissions.save(counted_id, &counted).await.expect("test: saved");
        let (legacy_id, mut legacy) = submission(2, 5);
        legacy.example_count = None; // written before the column
        store.submissions.save(legacy_id, &legacy).await.expect("test: saved");
        store.record_learned(counted_id, 1).await.expect("test: one learned");

        let counts = store.pending_counts(&key).await.expect("test: counted");
        let rows = store.arrivals_in(&key).await.expect("test: its rows");
        let from_rows: Vec<Counted> = rows.iter().map(|row| Counted { id: row.id, sequence: row.sequence, learned: row.learned, examples: row.examples.len() as u64 }).collect();
        assert_eq!(counts, from_rows, "counted from the columns, the same as decoded from the rows");
        assert_eq!(counts.iter().map(|row| row.examples - u64::from(row.learned)).sum::<u64>(), 2 + 5, "unlearned from the counts");
        let written = store.submissions.find_by_id(legacy_id).await.expect("test: read").expect("test: present");
        assert_eq!(written.example_count, Some(5), "the legacy row's count was written back, so it is never counted from its batch again");
    }

    #[tokio::test]
    async fn the_learned_cursor_advances_and_retires_the_row_when_every_example_is_learned() {
        let (adapter, _dir) = crate::orm::store::fresh_adapter().await;
        let store = DurableStore::new(adapter).await.expect("test: store");
        let (id, row) = submission(1, 2);
        store.submissions.save(id, &row).await.expect("test: saved");
        store.record_learned(id, 1).await.expect("test: one learned");
        let arrived = store.arrivals(0, 8).await.expect("test: arrivals");
        assert_eq!((arrived.len(), arrived[0].learned), (1, 1), "still pending, one learned");
        assert!(store.record_learned(id, 0).await.is_err(), "never backwards");
        assert!(store.record_learned(id, 3).await.is_err(), "never past its examples");
        store.record_learned(id, 2).await.expect("test: all learned");
        assert!(store.arrivals(0, 8).await.expect("test: arrivals").is_empty(), "retired once every example is learned");
    }

    // what this catches: a pass that reads one page of the pending keys, where a full page of rows
    // for another base hides every eligible bucket behind them forever (Codex on d177df17f).
    #[tokio::test]
    async fn an_eligible_bucket_behind_a_full_page_of_other_bases_is_found() {
        let (adapter, _dir) = crate::orm::store::fresh_adapter().await;
        let store = DurableStore::new(adapter).await.expect("test: store");
        for sequence in 1..=1025 {
            let (id, row) = submission_for(sequence, 1, "some-other-base-model");
            store.submissions.save(id, &row).await.expect("test: saved");
        }
        let (eligible, row) = submission_for(1026, 1, "qwen");
        let key = BucketKey { persona_id: row.persona_id, trait_kind: row.trait_kind.clone(), base_model: row.base_model.clone() };
        store.submissions.save(eligible, &row).await.expect("test: saved");
        let waiting = super::super::super::in_place::waiting_by_base(&store, "qwen").await.expect("test: gathered");
        assert_eq!(waiting.here, vec![key.clone()], "found past the first page, and only the served base's");
        assert_eq!(waiting.elsewhere.len(), 1, "the other base's bucket is listed as elsewhere, not seated");
        let rows = store.arrivals_in(&key).await.expect("test: its rows");
        assert_eq!(rows.iter().map(|row| row.id).collect::<Vec<_>>(), vec![eligible]);
    }

    // what this catches (Codex on the 5090 replay, 2026-11-25): a pass that holds every pending
    // row with its examples to learn one bucket. A bucket's rows come from its own index, so another
    // bucket's rows are never read for it, and listing the waiting buckets reads keys alone.
    #[tokio::test]
    async fn a_bucket_reads_only_its_own_rows_and_the_waiting_buckets_are_keys() {
        let (adapter, _dir) = crate::orm::store::fresh_adapter().await;
        let store = DurableStore::new(adapter).await.expect("test: store");
        let (mine, row) = submission_for(1, 2, "qwen");
        let key = BucketKey { persona_id: row.persona_id, trait_kind: row.trait_kind.clone(), base_model: row.base_model.clone() };
        store.submissions.save(mine, &row).await.expect("test: saved");
        let (other, mut other_row) = submission_for(2, 3, "qwen");
        other_row.persona_id = Uuid::new_v4(); // another persona's bucket on the same base
        let other_key = BucketKey { persona_id: other_row.persona_id, trait_kind: other_row.trait_kind.clone(), base_model: other_row.base_model.clone() };
        store.submissions.save(other, &other_row).await.expect("test: saved");

        let keys = store.pending_keys().await.expect("test: keys");
        assert_eq!(keys.len(), 2);
        assert!(keys.contains(&key) && keys.contains(&other_key));
        let rows = store.arrivals_in(&key).await.expect("test: its rows");
        assert_eq!(rows.iter().map(|row| row.id).collect::<Vec<_>>(), vec![mine], "only its own rows");
        assert_eq!(rows[0].examples.len(), 2);
    }

    // Independent recovery fixture: the checkpoint row survived but its submission projection
    // did not. A pass may already hold that stale submission when recovery projects the cursor.
    #[tokio::test]
    async fn a_reopened_checkpoint_keeps_a_captured_stale_arrival_past_its_committed_example() {
        let (adapter, _dir) = crate::orm::store::fresh_adapter().await;
        let store = DurableStore::new(adapter.clone()).await.expect("test: store");
        let (id, row) = submission(1, 2);
        let key = BucketKey { persona_id: row.persona_id, trait_kind: row.trait_kind.clone(), base_model: row.base_model.clone() };
        store.submissions.save(id, &row).await.expect("test: accepted");
        let generation = super::super::learn_checkpoint::Generation {
            name: "generation-1.gguf".into(),
            path: "/train/generation-1.gguf.checkpoint".into(),
            binding: "binding".into(),
            steps: 1,
            provenance: serde_json::json!({"submission": id}),
            persona_name: "Kimi".into(),
            signature: None,
        };
        store.commit_checkpoint(&key, generation, &[Position { submission: id, learned: 1 }])
            .await.expect("test: confirmed generation");
        // Materialize the crash boundary: the generation write is durable, the submission still
        // has its pre-projection contents. Reopen the store rather than retaining its owner.
        store.submissions.update(id, &row).await.expect("test: interrupted projection state");
        drop(store);
        let reopened = DurableStore::new(adapter).await.expect("test: reopened");
        let captured = reopened.arrivals(0, 8).await.expect("test: capture before recovery");
        assert_eq!(captured[0].learned, 0);
        let generation = reopened.checkpoint_of(&key).await.expect("test: checkpoint query").expect("test: durable generation");
        assert_eq!((generation.name.as_str(), generation.steps), ("generation-1.gguf", 1));
        reopened.project(&generation).await.expect("test: recovery projection");
        reopened.project(&generation).await.expect("test: duplicate recovery");
        assert_eq!(reopened.arrivals(0, 8).await.expect("test: projected")[0].learned, 1);
        let checkpointed = generation.positions.iter().find(|p| p.submission == id).map(|p| p.learned);
        let next = super::super::super::in_place::start(captured[0].learned, checkpointed, None);
        assert_eq!(next, 1, "the captured stale row must not replay example zero");
        assert_eq!(captured[0].examples[next as usize].prompt, "p1");
    }
}
