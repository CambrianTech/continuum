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

impl DurableStore {
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
            .map(|row| {
                let row: Submission = serde_json::from_value(row.data).map_err(|error| error.to_string())?; // decode the persisted ORM row into its typed owner
                Ok(Arrival {
                    id: entity_id(&row.base)?,
                    sequence: row.sequence,
                    key: BucketKey { persona_id: row.persona_id, trait_kind: row.trait_kind, base_model: row.base_model },
                    learned: row.learned,
                    persona_name: row.batch.persona_name,
                    lora: row.batch.lora,
                    schedule: row.batch.schedule,
                    examples: row.batch.examples,
                })
            })
            .collect()
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
        row.is_pending = learned < total;
        self.submissions.update(id, &row).await.map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn submission(sequence: u64, examples: usize) -> (Uuid, Submission) {
        submission_for(sequence, examples, "qwen")
    }

    fn submission_for(sequence: u64, examples: usize, base_model: &str) -> (Uuid, Submission) {
        let base = BaseEntity::for_new_record();
        let id = entity_id(&base).expect("test: a fresh entity id");
        let example = |n: usize| TrainingExample { prompt: format!("p{n}"), completion: "c".into(), metadata: None, lived: None };
        let batch = PendingBatch {
            submission_ids: vec![id],
            submission_examples: vec![examples as u32],
            submission_returned: vec![false],
            returned: false,
            persona_name: "Kimi".into(),
            source: crate::genome::fine_tuning::TrainingSource::OperatorCurated,
            examples: (0..examples).map(example).collect(),
            lora: None,
            schedule: None,
            local_artifact_dir: None,
            preferred_provider: None,
            min_examples: 1,
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
        };
        (id, row)
    }

    // what this catches: the learned cursor moving backwards, past its examples, or leaving a row
    // pending once every example is learned (or pending-free before then). Arrivals read it back.
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

    // what this catches: a pass that reads one page of the pending journal, where 256 rows for
    // another base hide every eligible row behind them forever (Codex on d177df17f).
    #[tokio::test]
    async fn an_eligible_row_behind_a_full_page_of_other_bases_is_found() {
        let (adapter, _dir) = crate::orm::store::fresh_adapter().await;
        let store = DurableStore::new(adapter).await.expect("test: store");
        for sequence in 1..=257 {
            let (id, row) = submission_for(sequence, 1, "some-other-base-model");
            store.submissions.save(id, &row).await.expect("test: saved");
        }
        let (eligible, row) = submission_for(258, 1, "qwen");
        store.submissions.save(eligible, &row).await.expect("test: saved");
        let buckets = super::super::super::in_place::eligible_buckets(&store, "qwen").await.expect("test: gathered");
        assert_eq!(buckets.len(), 1);
        assert_eq!(buckets[0].1.iter().map(|row| row.id).collect::<Vec<_>>(), vec![eligible], "found past the first page");
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
        store.commit_checkpoint(&key, "generation-1.gguf".into(), "binding".into(), 1,
            serde_json::json!({"submission": id}), &[Position { submission: id, learned: 1 }])
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
