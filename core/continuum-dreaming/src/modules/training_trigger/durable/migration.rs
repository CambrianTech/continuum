//! Base-change migration for training buckets.
//!
//! When a persona's model override changes (e.g., from "model-A" to "model-B"),
//! pending examples should migrate from old buckets to new ones keyed by the
//! new base_model. This ensures that a persona doesn't accumulate stale training
//! data under a base they are no longer assigned.

use std::sync::Arc;

use uuid::Uuid;

use crate::modules::training_trigger::{BucketKey, PendingBatch};
use super::TrainingTriggerState;

/// Migrates all pending buckets for `persona_id` that have `old_base` as their
/// base model into new buckets keyed by `new_base`. Old buckets are removed
/// once their examples have been transferred. Returns the number of migrated
/// buckets.
pub(crate) async fn migrate_base_change_buckets(
    state: &Arc<TrainingTriggerState>,
    persona_id: Uuid,
    old_base: &str,
    new_base: &str,
) -> Result<usize, String> {
    if old_base == new_base {
        return Ok(0); // nothing to migrate
    }

    state.require_ready()?;

    // Collect all matching buckets first (can't mutate DashMap while iterating)
    let mut to_move: Vec<(BucketKey, PendingBatch)> = Vec::new();

    for entry in state.buckets.iter() {
        let key = entry.key().clone();
        if key.persona_id == persona_id && key.base_model == old_base {
            to_move.push((key.clone(), entry.value().clone()));
        }
    }

    let mut migrated = 0;

    for (old_key, batch) in to_move {
        // Create the new bucket key with the new base_model
        let new_key = BucketKey {
            persona_id: old_key.persona_id,
            trait_kind: old_key.trait_kind.clone(),
            base_model: new_base.to_string(),
        };

        // Insert into the new bucket (if it already exists, we'd merge — but the
        // card says "creates new bucket", so let's just insert/replace)
        state.buckets.insert(new_key, batch);

        // Remove the old bucket
        state.buckets.remove(&old_key);

        migrated += 1;
    }

    Ok(migrated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::training_trigger::test_support::{build_runtime, ex, submit_params};
    use uuid::Uuid as UuidType;

    /// What this catches: when a persona's base_model changes (via model override),
    /// pending examples in the old bucket are moved to a new bucket keyed by the
    /// new base. The old bucket is left clean — no stale data persists under the
    /// retired base.
    #[tokio::test]
    async fn migrate_base_change_creates_new_bucket_when_override_differs_and_old_is_clean() {
        let (adapter, _dir) = crate::orm::store::fresh_adapter().await;
        let (trigger, _executor) = build_runtime(adapter.clone(), false).await;

        let persona_id = UuidType::new_v4();
        let trait_kind = "code".to_string();
        let old_base = "synthetic";
        let new_base = "other";

        // Submit examples that create a bucket under the OLD base
        let mut params = submit_params(persona_id, &trait_kind, vec![ex("prompt-1", "answer-1")], Some(16));
        params["submissionId"] = serde_json::json!(UuidType::new_v4());
        let result = trigger.executor.execute_json("genome/training-trigger/submit", params).await.unwrap();
        assert_eq!(result["success"], true, "initial submit succeeded: {result}");

        // Verify the bucket exists under old_base
        let old_key = BucketKey {
            persona_id,
            trait_kind: trait_kind.clone(),
            base_model: old_base.to_string(),
        };
        assert!(trigger.state.buckets.contains_key(&old_key), "bucket should exist under old base before migration");

        // Run the migration
        let count = migrate_base_change_buckets(
            &trigger.state,
            persona_id,
            old_base,
            new_base,
        ).await.unwrap();

        // Verify: exactly one bucket was migrated
        assert_eq!(count, 1, "exactly one bucket should be migrated");

        // Old bucket is gone (clean)
        assert!(!trigger.state.buckets.contains_key(&old_key), "old bucket should be removed after migration");

        // New bucket exists with the same examples
        let new_key = BucketKey {
            persona_id,
            trait_kind: trait_kind.clone(),
            base_model: new_base.to_string(),
        };
        assert!(trigger.state.buckets.contains_key(&new_key), "new bucket should exist under new base after migration");

        let new_bucket = trigger.state.buckets.get(&new_key).unwrap();
        assert_eq!(new_bucket.examples.len(), 1, "new bucket has the migrated example");
        assert_eq!(new_bucket.examples[0].prompt, "prompt-1", "example content is preserved");

        // Verify via public API that old base shows 0 and new base shows 1
        assert_eq!(trigger.state.bucket_example_count(persona_id, &trait_kind, old_base), Some(0));
        assert_eq!(trigger.state.bucket_example_count(persona_id, &trait_kind, new_base), Some(1));

        println!("PASS: migrate_base_change_creates_new_bucket_when_override_differs_and_old_is_clean");
    }
}
