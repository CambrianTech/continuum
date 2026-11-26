//! `genome/training-trigger/transfer`: carry a bucket's examples not learned yet to the base she
//! runs now. A lesson is her lived text, learnable on any base; her generations stay on the base
//! they were learned on. A preview by default; an apply names its exact source and target. The
//! owner is `TrainingTriggerState::transfer_apply` (card 70c24ee8).

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::modules::training_trigger::{BucketKey, Carried, TrainingTriggerState, TransferRefused};

#[derive(Debug, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/training_trigger/TransferParams.ts")]
pub struct TransferParams {
    #[ts(type = "string")]
    pub persona_id: Uuid,
    pub trait_kind: String,
    /// The base the bucket is keyed by now.
    pub from_base: String,
    /// The base she runs now (any spelling the catalog resolves).
    pub to_base: String,
    /// Carry them. Without it, a preview of what would be carried.
    #[serde(default)]
    pub apply: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/training_trigger/TransferOutcome.ts")]
pub struct TransferOutcome {
    pub success: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub to_base: Option<String>,
    /// Pending source rows with examples not learned yet.
    #[ts(type = "number")]
    pub rows: usize,
    /// Their examples not learned yet: what the target learns.
    #[ts(type = "number")]
    pub examples: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<String>,
}

impl TransferOutcome {
    fn done(to: &BucketKey, carried: Carried) -> Self {
        Self { success: true, to_base: Some(to.base_model.clone()), rows: carried.rows, examples: carried.examples, error_kind: None, error: None }
    }
}

impl From<TransferRefused> for TransferOutcome {
    fn from(refused: TransferRefused) -> Self {
        Self { success: false, to_base: None, rows: 0, examples: 0, error_kind: Some(refused.kind().into()), error: Some(refused.to_string()) }
    }
}

crate::action_command! {
    /// Carry a training bucket's examples not learned yet to the base she runs now, so the learner
    /// serving that base learns them. The source keeps what it learned, its examples and its base;
    /// each of its pending rows leaves pending, naming the row on the new base that carries its
    /// rest (an id that is the same on a repeat). One atomic write under both buckets' gates,
    /// refused while a seat holds either, or when the two buckets pin different training
    /// policies. Without `apply`, a preview of what would be carried.
    pub struct TrainingTriggerTransfer {
        state: Arc<TrainingTriggerState>,
    }
    name: "genome/training-trigger/transfer",
    access: Privileged,
    params: TransferParams,
    output: TransferOutcome,
    run(this, _ctx, p) => {
        Ok(transfer(&this.state, p).await)
    }
}

async fn transfer(state: &Arc<TrainingTriggerState>, p: TransferParams) -> TransferOutcome {
    let from = BucketKey { persona_id: p.persona_id, trait_kind: p.trait_kind, base_model: p.from_base };
    if p.apply {
        return match state.transfer_apply(&from, &p.to_base).await {
            Ok(done) => TransferOutcome::done(&done.to, done.carried),
            Err(refused) => refused.into(),
        };
    }
    match state.transfer_preview(&from, &p.to_base).await {
        Ok((to, carried)) => TransferOutcome::done(&to, carried),
        Err(refused) => refused.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::training_trigger::test_support::{build_runtime_trigger_only, ex, submit_params};

    // what this catches: a preview that writes, or a command the wire cannot reach. A preview over a
    // bucket counts its outstanding examples and leaves them where they are; the apply then carries
    // exactly those.
    #[tokio::test]
    async fn a_preview_counts_and_moves_nothing_and_the_apply_carries_what_it_counted() {
        let (trigger, executor, _dir) = build_runtime_trigger_only().await;
        let persona = Uuid::new_v4();
        let mut params = submit_params(persona, "code", vec![ex("a", "c"), ex("b", "c")]);
        params["baseModel"] = serde_json::json!("test");
        executor.execute_json("genome/training-trigger/submit", params).await.expect("test: submit");
        let ask = |apply: bool| serde_json::json!({ "personaId": persona, "traitKind": "code", "fromBase": "test", "toBase": "qwen", "apply": apply });
        let preview: TransferOutcome = serde_json::from_value(executor.execute_json("genome/training-trigger/transfer", ask(false)).await.expect("test: preview")).expect("test: typed");
        assert!(preview.success && preview.examples == 2, "{preview:?}");
        assert_eq!(trigger.state.bucket_example_count(persona, "code", "test").await, Some(2), "the preview moved nothing");
        let applied: TransferOutcome = serde_json::from_value(executor.execute_json("genome/training-trigger/transfer", ask(true)).await.expect("test: apply")).expect("test: typed");
        assert!(applied.success && applied.examples == preview.examples, "{applied:?}");
        assert_eq!(trigger.state.bucket_example_count(persona, "code", "qwen").await, Some(2));
    }
}
