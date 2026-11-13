//! A correction is separate from the immutable submission it qualifies. Both raw
//! and corrected replay identities refer to the same acceptance, never another batch.
use super::super::BucketPolicy;
use super::*;
use sha2::{Digest, Sha256};

/// Which return a stored correction qualified: kept so the corrections already on disk read back
/// (the return verb that wrote them is gone with the batch path).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ReturnPolicyEdge {
    pub(super) dispatch_id: Uuid,
    pub(super) job_id: Uuid,
    /// None is a whole return; Some names the exact ascending parked indices.
    pub(super) parked: Option<Vec<usize>>,
    pub(super) chunk_index: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
#[serde(rename_all = "camelCase")]
#[entity(collection = "training_trigger_policy_corrections")]
pub(super) struct PolicyCorrection {
    #[serde(flatten)]
    base: BaseEntity,
    persona_id: Uuid,
    trait_kind: String,
    base_model: String,
    batch_sha256: String,
    #[entity(json)]
    policy: BucketPolicy,
    #[entity(json)]
    edges: Vec<ReturnPolicyEdge>,
    #[entity(json)]
    source_digests: Vec<(Uuid, String)>,
    actor: Uuid,
    reason: String,
}

fn digest(batch: &PendingBatch) -> Result<String, String> {
    // Exact immutable batch content boundary, including lived examples and policy.
    serde_json::to_vec(batch) // Immutable batch content-digest boundary, including lived content.
        .map(|bytes| format!("{:x}", Sha256::digest(bytes)))
        .map_err(|e| e.to_string())
}

fn apply_policy(batch: &mut PendingBatch, policy: &BucketPolicy) {
    batch.source = policy.source.clone();
    batch.lora = policy.lora.clone();
    batch.schedule = policy.schedule.clone();
    batch.validation_split = policy.validation_split;
    batch.local_artifact_dir = policy.local_artifact_dir.clone();
    batch.preferred_provider = policy.preferred_provider.clone();
    batch.eval_set = policy.eval_set.clone();
}

impl DurableStore {
    pub(super) async fn effective_batch(&self, row: &Submission) -> Result<PendingBatch, String> {
        let id = entity_id(&row.base)?;
        let mut batch = row.batch.clone();
        if let Some(receipt) = self
            .policy_corrections
            .find_by_id(id)
            .await
            .map_err(|e| e.to_string())?
        {
            if receipt.persona_id != row.persona_id
                || receipt.trait_kind != row.trait_kind
                || receipt.base_model != row.base_model
                || receipt.batch_sha256 != digest(&batch)?
            {
                return Err(format!(
                    "policy correction does not match immutable submission {id}"
                ));
            }
            apply_policy(&mut batch, &receipt.policy);
        }
        Ok(batch)
    }

    pub(super) async fn replay_matches(
        &self,
        row: &Submission,
        incoming: &PendingBatch,
    ) -> Result<bool, String> {
        let id = entity_id(&row.base)?;
        if self
            .policy_corrections
            .find_by_id(id)
            .await
            .map_err(|e| e.to_string())?
            .is_none()
        {
            return Ok(row.batch.same_submission(incoming));
        }
        // Unlike generic legacy replay, this audited alternative binds all lived content too.
        Ok(digest(&row.batch)? == digest(incoming)?
            || digest(&self.effective_batch(row).await?)? == digest(incoming)?)
    }
}
