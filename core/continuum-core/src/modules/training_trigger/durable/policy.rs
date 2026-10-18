//! A correction is separate from the immutable submission it qualifies. Both raw
//! and corrected replay identities refer to the same acceptance, never another batch.
use super::super::BucketPolicy;
use super::*;
use crate::commands::training_trigger::policy_recover::{
    PolicyRecoverOutcome, PolicyRecoverParams, ReturnPolicyEdge,
};
use crate::commands::training_trigger::submit::ReturnScope;
use sha2::{Digest, Sha256};
use std::collections::HashSet;

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

impl TrainingTriggerState {
    pub(crate) async fn recover_return_policy(
        self: &Arc<Self>,
        p: PolicyRecoverParams,
        actor: Uuid,
    ) -> Result<PolicyRecoverOutcome, String> {
        if p.edges.is_empty() || p.reason.trim().is_empty() {
            return Err("policy recovery requires exact return lineage and a reason".into());
        }
        let row = self
            .durable
            .require()?
            .submissions
            .find_by_id(p.submission_id)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "target submission is absent".to_string())?;
        let key = row.key();
        self.run_owned(key, move |state, key| async move {
            state.correct_policy_in_bucket(&key, p, actor).await
        })
        .await?
    }

    async fn correct_policy_in_bucket(
        self: Arc<Self>,
        key: &BucketKey,
        p: PolicyRecoverParams,
        actor: Uuid,
    ) -> Result<PolicyRecoverOutcome, String> {
        let store = self.durable.require()?;
        let target = store
            .submissions
            .find_by_id(p.submission_id)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "target submission is absent".to_string())?;
        let hash = digest(&target.batch)?;
        if target.key() != *key
            || p.expected_batch_sha256
                .as_ref()
                .is_some_and(|expected| *expected != hash)
            || (p.apply && p.expected_batch_sha256.is_none())
        {
            return Err("target key or immutable batch digest changed".into());
        }
        let result = |applied, replayed| PolicyRecoverOutcome {
            submission_id: p.submission_id,
            batch_sha256: hash.clone(),
            examples: target.batch.examples.len(),
            applied,
            replayed,
        };
        if let Some(prior) = store
            .policy_corrections
            .find_by_id(p.submission_id)
            .await
            .map_err(|e| e.to_string())?
        {
            if prior.batch_sha256 != hash
                || prior.edges != p.edges
                || prior.actor != actor
                || prior.reason != p.reason
            {
                return Err("a different policy correction already exists".into());
            }
            store.effective_batch(&target).await?;
            if p.apply && target.is_pending {
                self.restore_pending_bucket(key).await?;
            }
            return Ok(result(true, true));
        }
        if p.apply && !self.hydrate_bucket(key).await? {
            return Err("bucket hydration is incomplete; no correction recorded".into());
        }
        if !target.is_pending
            || target.retirement_id.is_some()
            || store.active_intent(key).await?.is_some()
            || self.active_dispatches.contains_key(key)
        {
            return Err("only an undispatched pending return can be corrected".into());
        }
        let pending = store.pending_page(key, 0, 2).await?;
        if pending.len() != 1 || entity_id(&pending[0].base)? != p.submission_id {
            return Err("policy correction requires the exact sole pending submission".into());
        }
        let mut current = target.clone();
        let mut seen = HashSet::new();
        let mut sources = Vec::new();
        let mut materialization = false;
        for edge in &p.edges {
            let current_id = entity_id(&current.base)?;
            if !seen.insert(current_id) {
                return Err("return lineage contains a cycle".into());
            }
            let intent = store
                .dispatches
                .find_by_id(edge.dispatch_id)
                .await
                .map_err(|e| e.to_string())?
                .ok_or_else(|| "lineage dispatch is absent".to_string())?;
            if intent.key() != *key
                || intent.submission_ids.len() != 1
                || !matches!(&intent.phase, DispatchPhase::Dispatched { handle, .. } if handle.local_id == edge.job_id)
            {
                return Err("lineage dispatch does not prove one exact source job".into());
            }
            let source_id = intent.submission_ids[0];
            let source = store
                .submissions
                .find_by_id(source_id)
                .await
                .map_err(|e| e.to_string())?
                .ok_or_else(|| "lineage source submission is absent".to_string())?;
            if source.key() != *key
                || source.batch.persona_name != current.batch.persona_name
                || source.is_pending
                || source.dispatch_id != Some(edge.dispatch_id)
                || seen.contains(&source_id)
            {
                return Err("lineage source ownership or cycle mismatch".into());
            }
            if store
                .policy_corrections
                .find_by_id(source_id)
                .await
                .map_err(|e| e.to_string())?
                .is_some()
            {
                return Err("lineage already contains a corrected source; explicit reconciliation is required".into());
            }
            let scope = edge
                .parked
                .as_deref()
                .map_or(ReturnScope::Whole, ReturnScope::Parked);
            if scope.chunk_id(edge.job_id, edge.chunk_index) != current_id {
                return Err("submission is not the claimed derived return identity".into());
            }
            let indices = edge
                .parked
                .clone()
                .unwrap_or_else(|| (0..source.batch.examples.len()).collect());
            if indices.is_empty()
                || indices.windows(2).any(|w| w[0] >= w[1])
                || indices.iter().any(|&i| i >= source.batch.examples.len())
            {
                return Err(
                    "return indices are empty, unordered or outside the original request".into(),
                );
            }
            let size = super::super::DEFAULT_MIN_EXAMPLES as usize;
            let start = edge
                .chunk_index
                .checked_mul(size)
                .ok_or("return chunk overflow")?;
            let selected = indices
                .get(start..indices.len().min(start.saturating_add(size)))
                .ok_or("return chunk is outside original request")?;
            let examples: Vec<_> = selected
                .iter()
                .map(|&i| &source.batch.examples[i])
                .collect();
            if serde_json::to_value(&examples).map_err(|e| e.to_string())? // Exact persisted example-content comparison boundary.
                != serde_json::to_value(&current.batch.examples).map_err(|e| e.to_string())? // Compare the complete returned content, including lived data.
            {
                return Err("return examples differ from the exact original ordered subset".into());
            }
            if source.batch.lora != current.batch.lora
                || source.batch.schedule != current.batch.schedule
            {
                let provider = match &intent.phase {
                    DispatchPhase::Dispatched { handle, provider }
                        if handle.provider_id == *provider =>
                    {
                        provider
                    }
                    _ => return Err("native dispatch provider identity disagrees".into()),
                };
                if materialization
                    || source.batch.lora.is_some()
                    || source.batch.schedule.is_some()
                    || !crate::genome::fine_tuning::materialized_defaults(
                        provider,
                        &current.batch.lora,
                        &current.batch.schedule,
                    )
                {
                    return Err(
                        "lineage is not one omitted-policy native default materialization".into(),
                    );
                }
                materialization = true;
            }
            let mut before = source.batch.policy();
            let mut after = current.batch.policy();
            before.lora = None;
            before.schedule = None;
            after.lora = None;
            after.schedule = None;
            if before != after {
                return Err(
                    "lineage changes policy beyond adapter LoRA/schedule materialization".into(),
                );
            }
            sources.push((source_id, digest(&source.batch)?));
            current = source;
        }
        let policy = current.batch.policy();
        if !materialization
            || policy.lora.is_some()
            || policy.schedule.is_some()
            || policy == target.batch.policy()
        {
            return Err("lineage establishes no policy correction".into());
        }
        if !p.apply {
            return Ok(result(false, false));
        }
        let mut base = BaseEntity::for_new_record();
        base.id = p.submission_id.to_string();
        let receipt = PolicyCorrection {
            base,
            persona_id: key.persona_id,
            trait_kind: key.trait_kind.clone(),
            base_model: key.base_model.clone(),
            batch_sha256: hash.clone(),
            policy,
            edges: p.edges.clone(),
            source_digests: sources,
            actor,
            reason: p.reason.clone(),
        };
        let op = store
            .policy_corrections
            .create_op(p.submission_id, &receipt)
            .map_err(|e| e.to_string())?;
        crate::orm::store::commit(&store.adapter, vec![op])
            .await
            .map_err(|e| e.to_string())?;
        self.restore_pending_bucket(key).await?;
        Ok(result(true, false))
    }
}
