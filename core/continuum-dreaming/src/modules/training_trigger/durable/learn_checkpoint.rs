//! The durable cursor of in-place learning (card 4114a2c6): one row per bucket naming the newest
//! checkpoint generation the engine confirmed, and the journal positions that generation holds.
//!
//! An update the engine committed lives only in its memory until a generation holding it is
//! published; so the cursor moves only with a confirmed generation (Codex on 608651e77). The
//! submissions' `learned` counts and `pending` flags are this row's projection, applied after it is
//! written and re-applied on start; both are monotonic, so a crash between the two is repaired by
//! applying again. Progress since the newest generation is volatile: after a restart the bucket's
//! shadow is restored from the generation and the examples past its positions are learned again.

use super::*;

/// Where one submission stands in a generation: its first `learned` examples are in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Position {
    pub(crate) submission: Uuid,
    pub(crate) learned: u32,
}

/// A bucket's newest confirmed checkpoint generation.
#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
#[serde(rename_all = "camelCase")]
#[entity(collection = "training_trigger_learn_checkpoints")]
pub(crate) struct LearnCheckpoint {
    #[serde(flatten)]
    pub(super) base: BaseEntity,
    pub(crate) persona_id: Uuid,
    pub(crate) trait_kind: String,
    pub(crate) base_model: String,
    /// The bare name it was saved under; the generation is `<name>.checkpoint` in the engine's
    /// train dir.
    pub(crate) name: String,
    pub(crate) binding: String,
    /// Optimizer updates the generation holds, as the engine counted them.
    pub(crate) steps: i64,
    #[entity(json)]
    pub(crate) provenance: serde_json::Value,
    #[entity(json)]
    pub(crate) positions: Vec<Position>,
}

/// The one row id of `key`'s checkpoint: a bucket has exactly one, replaced by each newer generation.
fn checkpoint_id(key: &BucketKey) -> Uuid {
    Uuid::new_v5(&key.persona_id, format!("learn-checkpoint:{}:{}", key.trait_kind, key.base_model).as_bytes())
}

impl DurableStore {
    /// `key`'s newest confirmed generation, if it has one.
    pub(in crate::modules::training_trigger) async fn checkpoint_of(&self, key: &BucketKey) -> Result<Option<LearnCheckpoint>, String> {
        self.learn_checkpoints.find_by_id(checkpoint_id(key)).await.map_err(|e| e.to_string())
    }

    /// Record generation `name` (confirmed by the engine) as `key`'s newest, holding `positions`,
    /// then project the positions onto the submissions. Positions merge with the previous
    /// generation's: a submission it does not name keeps where it stood.
    pub(in crate::modules::training_trigger) async fn commit_checkpoint(
        &self,
        key: &BucketKey,
        name: String,
        binding: String,
        steps: i64,
        provenance: serde_json::Value,
        positions: &[Position],
    ) -> Result<(), String> {
        let id = checkpoint_id(key);
        let mut merged = self.checkpoint_of(key).await?.map(|row| row.positions).unwrap_or_default(); // a bucket's first generation starts from no positions
        for position in positions {
            match merged.iter_mut().find(|held| held.submission == position.submission) {
                Some(held) => held.learned = held.learned.max(position.learned),
                None => merged.push(*position),
            }
        }
        let mut base = BaseEntity::for_new_record();
        base.id = id.to_string();
        let row = LearnCheckpoint {
            base,
            persona_id: key.persona_id,
            trait_kind: key.trait_kind.clone(),
            base_model: key.base_model.clone(),
            name,
            binding,
            steps,
            provenance,
            positions: merged,
        };
        match self.learn_checkpoints.find_by_id(id).await.map_err(|e| e.to_string())? {
            Some(_) => self.learn_checkpoints.update(id, &row).await.map_err(|e| e.to_string())?,
            None => self.learn_checkpoints.save(id, &row).await.map_err(|e| e.to_string())?,
        }
        self.project(&row).await
    }

    /// Apply `checkpoint`'s positions to its submissions (monotonic, so applying again is harmless).
    pub(in crate::modules::training_trigger) async fn project(&self, checkpoint: &LearnCheckpoint) -> Result<(), String> {
        for position in &checkpoint.positions {
            let Some(row) = self.submissions.find_by_id(position.submission).await.map_err(|e| e.to_string())? else {
                continue; // a retired or purged submission: nothing left to project onto
            };
            if row.learned < position.learned {
                self.record_learned(position.submission, position.learned).await?;
            }
        }
        Ok(())
    }
}
