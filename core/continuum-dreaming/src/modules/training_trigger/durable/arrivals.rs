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
                    lora: row.batch.lora,
                    schedule: row.batch.schedule,
                    examples: row.batch.examples,
                })
            })
            .collect()
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
