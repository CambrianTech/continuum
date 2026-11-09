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
    /// The generation's directory, as the engine published it: the candidate gene is its
    /// `adapter.gguf`.
    pub(crate) path: String,
    /// Her name, for the candidate's provenance.
    pub(crate) persona_name: String,
    /// The competence's signature (a fork's parent on it), stamped on each candidate.
    #[entity(json)]
    pub(crate) signature: Option<crate::genome::signature::GeneSignature>,
    /// The newest candidate's trial, and how many examples the generations held when it was taken:
    /// durable, so a candidate interrupted after its generation is taken again, and a restart never
    /// opens a second trial beside a settling one.
    pub(crate) candidate_trial: Option<Uuid>,
    pub(crate) candidate_at: u32,
}

impl LearnCheckpoint {
    /// How many examples the generation holds, over every submission it consumed.
    pub(crate) fn learned(&self) -> u32 {
        self.positions.iter().map(|position| position.learned).sum()
    }
}

/// What a confirmed generation is, as the learner that published it describes it.
pub(crate) struct Generation {
    pub(crate) name: String,
    pub(crate) path: String,
    pub(crate) binding: String,
    pub(crate) steps: i64,
    pub(crate) provenance: serde_json::Value,
    pub(crate) persona_name: String,
    pub(crate) signature: Option<crate::genome::signature::GeneSignature>,
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
    pub(in crate::modules::training_trigger) async fn commit_checkpoint(&self, key: &BucketKey, generation: Generation, positions: &[Position]) -> Result<(), String> {
        let id = checkpoint_id(key);
        let previous = self.checkpoint_of(key).await?;
        let (candidate_trial, candidate_at) = previous.as_ref().map_or((None, 0), |row| (row.candidate_trial, row.candidate_at)); // a bucket's first generation has had no candidate
        let mut merged = previous.map(|row| row.positions).unwrap_or_default(); // a bucket's first generation starts from no positions
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
            name: generation.name,
            binding: generation.binding,
            steps: generation.steps,
            provenance: generation.provenance,
            positions: merged,
            path: generation.path,
            persona_name: generation.persona_name,
            signature: generation.signature,
            candidate_trial,
            candidate_at,
        };
        match self.learn_checkpoints.find_by_id(id).await.map_err(|e| e.to_string())? {
            Some(_) => self.learn_checkpoints.update(id, &row).await.map_err(|e| e.to_string())?,
            None => self.learn_checkpoints.save(id, &row).await.map_err(|e| e.to_string())?,
        }
        self.project(&row).await
    }

    /// Every bucket's newest generation: the candidates the snapshot sweep may take.
    pub(in crate::modules::training_trigger) async fn checkpoints(&self) -> Result<Vec<LearnCheckpoint>, String> {
        Ok(self.learn_checkpoints.find_all().await.map_err(|e| e.to_string())?.into_iter().map(|(_, row)| row).collect())
    }

    /// Record that `checkpoint`'s generation was adopted as candidate `trial`.
    pub(in crate::modules::training_trigger) async fn record_candidate(&self, checkpoint: &LearnCheckpoint, trial: Uuid) -> Result<(), String> {
        let id = entity_id(&checkpoint.base)?;
        let mut row = checkpoint.clone();
        row.candidate_trial = Some(trial);
        row.candidate_at = checkpoint.learned();
        self.learn_checkpoints.update(id, &row).await.map_err(|e| e.to_string())
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

#[cfg(test)]
mod tests {
    use super::*;

    fn generation(name: &str) -> Generation {
        Generation { name: name.into(), path: format!("/train/{name}.checkpoint"), binding: "her:code:qwen".into(), steps: 8, provenance: serde_json::json!({}), persona_name: "Kimi".into(), signature: None }
    }

    // The candidate sweep reads before adoption awaits; a learning pass can publish
    // the next generation before that old candidate's adoption receipt returns.
    #[tokio::test]
    async fn an_older_candidate_receipt_cannot_rewind_the_checkpoint() {
        let (adapter, _dir) = crate::orm::store::fresh_adapter().await;
        let store = DurableStore::new(adapter).await.expect("test: store");
        let key = BucketKey { persona_id: Uuid::new_v4(), trait_kind: "code".into(), base_model: "qwen".into() };
        let submission = Uuid::new_v4();
        store.commit_checkpoint(&key, generation("g1"), &[Position { submission, learned: 1 }]).await.expect("test: g1");
        let old = store.checkpoint_of(&key).await.expect("test: read").expect("test: g1 exists");
        store.commit_checkpoint(&key, generation("g2"), &[Position { submission, learned: 2 }]).await.expect("test: g2");
        let trial = Uuid::new_v4();
        store.record_candidate(&old, trial).await.expect("test: adoption receipt");
        let current = store.checkpoint_of(&key).await.expect("test: read").expect("test: g2 exists");
        assert_eq!(current.name, "g2", "an old candidate receipt must not overwrite the surviving optimizer generation");
        assert_eq!(current.positions, vec![Position { submission, learned: 2 }]);
        assert_eq!(current.candidate_trial, Some(trial));
        assert_eq!(current.candidate_at, 1, "only the adopted generation's coverage is credited");
    }

    // what this catches: a candidate interrupted after its generation (a failed provenance write, a
    // refused adoption, a stopped process) never taken again, or taken twice (Codex on 5f26871bc).
    // The generation is due on the durable row, across a reopen, until its candidate is recorded;
    // then the next is due only after a competence's worth more.
    #[tokio::test]
    async fn a_due_candidate_survives_a_reopen_until_it_is_recorded() {
        let (adapter, _dir) = crate::orm::store::fresh_adapter().await;
        let key = BucketKey { persona_id: Uuid::new_v4(), trait_kind: "code".into(), base_model: "qwen".into() };
        let competence = crate::genome::competence::MIN_EXAMPLES as u32;
        let positions = [Position { submission: Uuid::new_v4(), learned: competence }];
        DurableStore::new(adapter.clone()).await.expect("test: store").commit_checkpoint(&key, generation("g1"), &positions).await.expect("test: committed");
        let reopened = DurableStore::new(adapter.clone()).await.expect("test: reopened"); // the process stopped before the candidate
        let rows = reopened.checkpoints().await.expect("test: rows");
        assert_eq!(rows.len(), 1);
        assert!(super::super::super::in_place::snapshot_due(&rows[0]), "still due after the reopen");
        reopened.record_candidate(&rows[0], Uuid::new_v4()).await.expect("test: recorded");
        let rows = reopened.checkpoints().await.expect("test: rows");
        assert!(!super::super::super::in_place::snapshot_due(&rows[0]), "taken: not due again until more is learned");
        let more = [Position { submission: Uuid::new_v4(), learned: competence }];
        reopened.commit_checkpoint(&key, generation("g2"), &more).await.expect("test: committed");
        let rows = reopened.checkpoints().await.expect("test: rows");
        assert_eq!((rows[0].candidate_at, rows[0].learned()), (competence, 2 * competence), "the candidate's place carries to the next generation");
        assert!(super::super::super::in_place::snapshot_due(&rows[0]));
    }
}
