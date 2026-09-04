//! `genome/training-trigger/status` — read-only snapshot of every pending training
//! bucket, in a deterministic order.
//!
//! Dep-holding: reads the owning module's
//! [`TrainingTriggerState`](crate::modules::training_trigger::TrainingTriggerState)
//! buckets. Pure read — no gate acquisition, no mutation.
//!
//! ## Gating
//!
//! `AiSafe` — observing pending curriculum buckets is a read-only inspection a
//! persona may legitimately do (e.g. the teacher checking whether its synthesis has
//! accumulated enough to fire). It spends no compute and exposes no credentials.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::modules::training_trigger::TrainingTriggerState;

/// `genome/training-trigger/status` input — none; returns all pending buckets.
#[derive(Debug, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/training_trigger/StatusParams.ts"
)]
pub struct StatusParams {}

/// One row in the status snapshot — a single pending bucket's identity + counts.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/training_trigger/PendingBucketView.ts"
)]
pub struct PendingBucketView {
    #[ts(type = "string")]
    pub persona_id: Uuid,
    pub persona_name: String,
    pub trait_kind: String,
    pub base_model: String,
    pub examples_pending: u32,
    pub min_examples: u32,
}

/// `genome/training-trigger/status` output — all pending buckets, sorted by
/// `(persona_id, trait_kind, base_model)` for deterministic operator diffing.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/training_trigger/StatusReport.ts"
)]
pub struct StatusReport {
    pub success: bool,
    pub buckets: Vec<PendingBucketView>,
}

crate::action_command! {
    /// List every pending training bucket — `(persona, traitKind, baseModel)` plus how
    /// many examples are queued vs the bucket's fire threshold — in a deterministic
    /// order. Read-only inspection: spends no compute, dispatches nothing. Use it to
    /// see whether accumulated curriculum is close to firing a training job.
    pub struct TrainingTriggerStatus {
        state: Arc<TrainingTriggerState>,
    }
    name: "genome/training-trigger/status",
    access: AiSafe,
    params: StatusParams,
    output: StatusReport,
    run(this, _ctx, _p) => {
        let state = &this.state;
        let mut buckets: Vec<PendingBucketView> = Vec::with_capacity(state.buckets.len());
        for entry in state.buckets.iter() {
            buckets.push(PendingBucketView {
                persona_id: entry.key().persona_id,
                persona_name: entry.value().persona_name.clone(),
                trait_kind: entry.key().trait_kind.clone(),
                base_model: entry.key().base_model.clone(),
                examples_pending: entry.value().examples.len() as u32,
                min_examples: entry.value().min_examples,
            });
        }
        // Deterministic order — sort by (persona_id, trait_kind, base_model) so
        // operator-tooling tests don't flake on DashMap iteration order.
        buckets.sort_by(|a, b| {
            (a.persona_id, &a.trait_kind, &a.base_model)
                .cmp(&(b.persona_id, &b.trait_kind, &b.base_model))
        });

        Ok(StatusReport { success: true, buckets })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::training_trigger::test_support::{
        build_runtime_with_trigger_and_genome, ex, submit_params,
    };
    use crate::sdk_codegen::{AccessLevel, ActionCommand};
    use uuid::Uuid;

    // what this catches: name/access wiring — status is a read-only inspection, so it
    // lives on the AiSafe surface (a persona may observe its own pending curriculum).
    #[test]
    fn name_and_access_wired() {
        assert_eq!(
            TrainingTriggerStatus::NAME,
            "genome/training-trigger/status"
        );
        assert!(matches!(TrainingTriggerStatus::ACCESS, AccessLevel::AiSafe));
    }

    // what this catches: status lists all pending buckets in a deterministic order.
    // Operator tooling relies on this for visual diffing — non-deterministic ordering
    // would make every snapshot look "different" even when state is identical.
    #[tokio::test]
    async fn status_returns_deterministic_bucket_list() {
        let (_trigger, executor) = build_runtime_with_trigger_and_genome().await;
        let a = Uuid::nil(); // stable for ordering
        let b = Uuid::from_u128(1);

        let _ = executor
            .execute_json(
                "genome/training-trigger/submit",
                submit_params(b, "trait-z", vec![ex("a", "b")], Some(5)),
            )
            .await
            .unwrap();
        let _ = executor
            .execute_json(
                "genome/training-trigger/submit",
                submit_params(a, "trait-a", vec![ex("c", "d")], Some(5)),
            )
            .await
            .unwrap();

        let json = executor
            .execute_json("genome/training-trigger/status", serde_json::json!({}))
            .await
            .unwrap();
        let buckets = json["buckets"].as_array().unwrap();
        assert_eq!(buckets.len(), 2);
        // a (nil uuid) sorts before b.
        assert_eq!(buckets[0]["personaId"], a.to_string());
        assert_eq!(buckets[0]["traitKind"], "trait-a");
        assert_eq!(buckets[1]["personaId"], b.to_string());
        assert_eq!(buckets[1]["traitKind"], "trait-z");
    }
}
