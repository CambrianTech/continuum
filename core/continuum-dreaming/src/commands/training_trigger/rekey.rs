//! `genome/training-trigger/rekey`: move a bucket keyed by a base spelling that is not the
//! catalog id to the catalog id. Submit resolves every new base (#6222); rows accepted before it
//! keep the spelling they arrived with, a bucket no learner on that row ever matches (the 5090,
//! 2026-11-22: Kimi's reviewed correction b38e0149 under the GGUF stem). A preview by default;
//! an apply names its exact source bucket. The owner is `TrainingTriggerState::rekey_apply`.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::modules::training_trigger::{BucketKey, CheckpointedSides, RekeyCandidate, RekeyCounts, RekeyFilter, RekeyRefused, Rekeyed, TrainingTriggerState};
use crate::sdk_codegen::CommandError;

#[derive(Debug, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/training_trigger/RekeyParams.ts")]
pub struct RekeyParams {
    #[serde(default)]
    #[ts(optional, type = "string")]
    pub persona_id: Option<Uuid>,
    #[serde(default)]
    #[ts(optional)]
    pub trait_kind: Option<String>,
    /// The base spelling the bucket is keyed by now.
    #[serde(default)]
    #[ts(optional)]
    pub from_base: Option<String>,
    /// Move the one bucket named by personaId, traitKind and fromBase. Without it, a preview.
    #[serde(default)]
    pub apply: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/training_trigger/RekeyRows.ts")]
pub struct RekeyRows {
    #[ts(type = "number")]
    pub submissions: usize,
    #[ts(type = "number")]
    pub dispatches: usize,
    #[ts(type = "number")]
    pub retirements: usize,
    #[ts(type = "number")]
    pub policy_corrections: usize,
}

impl From<RekeyCounts> for RekeyRows {
    fn from(counts: RekeyCounts) -> Self {
        Self { submissions: counts.submissions, dispatches: counts.dispatches, retirements: counts.retirements, policy_corrections: counts.policy_corrections }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/training_trigger/RekeyBucket.ts")]
pub struct RekeyBucket {
    #[ts(type = "string")]
    pub persona_id: Uuid,
    pub trait_kind: String,
    pub from_base: String,
    /// The catalog id it moves to; absent when its base names no single row (`unresolved` says why).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub to_base: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub unresolved: Option<String>,
    pub rows: RekeyRows,
    /// A source with a learn checkpoint is refused on apply.
    pub checkpointed: CheckpointedSides,
}

impl From<RekeyCandidate> for RekeyBucket {
    fn from(candidate: RekeyCandidate) -> Self {
        let (to_base, unresolved) = match candidate.to {
            Ok(base) => (Some(base), None),
            Err(why) => (None, Some(why.to_string())),
        };
        Self {
            persona_id: candidate.from.persona_id,
            trait_kind: candidate.from.trait_kind,
            from_base: candidate.from.base_model,
            to_base,
            unresolved,
            rows: candidate.counts.into(),
            checkpointed: candidate.checkpointed,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/training_trigger/RekeyMove.ts")]
pub struct RekeyMove {
    #[ts(type = "string")]
    pub persona_id: Uuid,
    pub trait_kind: String,
    pub from_base: String,
    pub to_base: String,
    pub moved: RekeyRows,
    /// What the destination holds now.
    pub destination: RekeyRows,
}

impl From<Rekeyed> for RekeyMove {
    fn from(done: Rekeyed) -> Self {
        Self {
            persona_id: done.from.persona_id,
            trait_kind: done.from.trait_kind,
            from_base: done.from.base_model,
            to_base: done.to.base_model,
            moved: done.moved.into(),
            destination: done.destination.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/training_trigger/RekeyOutcome.ts")]
pub struct RekeyOutcome {
    pub success: bool,
    /// The preview: every bucket keyed by a non-catalog base under the filter.
    pub candidates: Vec<RekeyBucket>,
    /// The apply's move.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub moved: Option<RekeyMove>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<String>,
}

impl From<RekeyRefused> for RekeyOutcome {
    fn from(refused: RekeyRefused) -> Self {
        Self { success: false, candidates: Vec::new(), moved: None, error_kind: Some(refused.kind().into()), error: Some(refused.to_string()) }
    }
}

crate::action_command! {
    /// Move a training bucket keyed by a base spelling that is not the catalog id (a GGUF stem,
    /// an HF repo) to the catalog id, so the learner serving that row learns it. Without `apply`,
    /// a preview of every such bucket (filter by personaId, traitKind, fromBase): where each would
    /// move and the rows it holds. With `apply`, personaId, traitKind and fromBase name the one
    /// source; its submissions, dispatch intents, retirements and policy corrections move in one
    /// atomic write, ids unchanged, under both buckets' gates, refused while a seat holds the source. A
    /// source with a learn checkpoint, a learner seated on it, or a policy that differs from the
    /// destination's is refused by name.
    pub struct TrainingTriggerRekey {
        state: Arc<TrainingTriggerState>,
    }
    name: "genome/training-trigger/rekey",
    access: Privileged,
    params: RekeyParams,
    output: RekeyOutcome,
    run(this, _ctx, p) => {
        rekey(&this.state, p).await
    }
}

pub(crate) async fn rekey(state: &Arc<TrainingTriggerState>, p: RekeyParams) -> Result<RekeyOutcome, CommandError> {
    if !p.apply {
        let filter = RekeyFilter { persona_id: p.persona_id, trait_kind: p.trait_kind, from_base: p.from_base };
        return Ok(match state.rekey_preview(filter).await {
            Ok(found) => RekeyOutcome { success: true, candidates: found.into_iter().map(Into::into).collect(), moved: None, error_kind: None, error: None },
            Err(refused) => refused.into(),
        });
    }
    let (Some(persona_id), Some(trait_kind), Some(base_model)) = (p.persona_id, p.trait_kind, p.from_base) else {
        return Err(CommandError::Invalid("an apply names its exact source: personaId, traitKind and fromBase (run the preview to read them)".into()));
    };
    Ok(match state.rekey_apply(&BucketKey { persona_id, trait_kind, base_model }).await {
        Ok(done) => RekeyOutcome { success: true, candidates: Vec::new(), moved: Some(done.into()), error_kind: None, error: None },
        Err(refused) => refused.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::training_trigger::test_support::{build_runtime_trigger_only, submit_params};

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Wire {
        success: bool,
        candidates: Vec<RekeyBucket>,
        error_kind: Option<String>,
    }

    // what this catches: an apply that guesses its source (every stale bucket at once, or the
    // first one found) instead of the one the operator read in the preview, and a preview the wire
    // cannot carry. An apply missing any part of the source is refused before anything is read; a
    // preview over a journal with no stale bucket succeeds with none.
    #[tokio::test]
    async fn an_apply_names_its_exact_source_and_a_clean_preview_lists_nothing() {
        let (trigger, executor, _dir) = build_runtime_trigger_only().await;
        let persona = Uuid::new_v4();
        executor.execute_json("genome/training-trigger/submit", submit_params(persona, "code", vec![crate::commands::training_trigger::test_support::ex("p", "c")])).await.expect("test: submit");
        let preview: Wire = serde_json::from_value(executor.execute_json("genome/training-trigger/rekey", serde_json::json!({ "personaId": persona })).await.expect("test: preview")).expect("test: typed");
        assert!(preview.success && preview.candidates.is_empty() && preview.error_kind.is_none(), "submit keyed it by the catalog id: nothing to move");
        let vague = rekey(&trigger.state, RekeyParams { persona_id: Some(persona), trait_kind: None, from_base: Some("x".into()), apply: true }).await;
        assert!(matches!(vague, Err(CommandError::Invalid(_))), "{vague:?}");
    }
}
