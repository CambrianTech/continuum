//! `genome/training-trigger/retire`: withdraw bucket-pending submissions before they
//! train, journaled. The one supported way to pull back rows that settled under a
//! label the room later overturned (card 04a5e867): on 2026-10-10 a card's pre-fix
//! turns settled as passing after a Failed-then-Passed review and 43 examples sat
//! in `code/owner` ready to teach the rejected behaviour. `#4900` governs FUTURE
//! settlements; this verb is for rows already in a bucket. A row already carried
//! into a job is refused by name: that job's examples come back through `return`.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::modules::training_trigger::{RetireReport, RetireSelection, TrainingTriggerState};
use crate::sdk_codegen::CommandError;

#[derive(Debug, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/training_trigger/RetireParams.ts")]
pub struct RetireParams {
    /// Withdraw every pending submission whose examples were settled from this card.
    #[serde(default)]
    #[ts(optional, type = "string")]
    pub card_id: Option<Uuid>,
    /// Withdraw exactly these submissions (the ids `submit` receipted).
    #[serde(default)]
    #[ts(type = "Array<string>")]
    pub submission_ids: Vec<Uuid>,
    /// Why, in the operator's words; journaled on the receipt.
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/training_trigger/RetiredSubmission.ts")]
pub struct RetiredSubmission {
    #[ts(type = "string")]
    pub submission_id: Uuid,
    pub examples: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/training_trigger/RefusedSubmission.ts")]
pub struct RefusedSubmission {
    #[ts(type = "string")]
    pub submission_id: Uuid,
    /// The dispatch that already carried it, when that is the reason.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string")]
    pub dispatch_id: Option<Uuid>,
    pub why: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/training_trigger/RetireOutcome.ts")]
pub struct RetireOutcome {
    /// Something was withdrawn, or the selection was already withdrawn. False only on
    /// a refusal of the whole call (`error_kind` says which).
    pub success: bool,
    /// One journaled receipt per bucket touched.
    #[ts(type = "Array<string>")]
    pub retirement_ids: Vec<Uuid>,
    pub retired: Vec<RetiredSubmission>,
    /// Withdrawn by an earlier call: nothing changed for these.
    #[ts(type = "Array<string>")]
    pub already_retired: Vec<Uuid>,
    /// Not pending, so not withdrawn: dispatched into a job (use `return` on that job),
    /// or unknown to this node.
    pub refused: Vec<RefusedSubmission>,
    /// Examples withdrawn by this call: what the next job will NOT train on.
    pub examples_retired: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<String>,
}

impl RetireOutcome {
    fn refused(kind: &str, error: String) -> Self {
        Self {
            success: false,
            retirement_ids: Vec::new(),
            retired: Vec::new(),
            already_retired: Vec::new(),
            refused: Vec::new(),
            examples_retired: 0,
            error_kind: Some(kind.into()),
            error: Some(error),
        }
    }
}

impl From<RetireReport> for RetireOutcome {
    fn from(report: RetireReport) -> Self {
        Self {
            success: true,
            retirement_ids: report.retirement_ids,
            retired: report
                .retired
                .into_iter()
                .map(|(submission_id, examples)| RetiredSubmission { submission_id, examples })
                .collect(),
            already_retired: report.already_retired,
            refused: report
                .refused
                .into_iter()
                .map(|(submission_id, dispatch_id, why)| RefusedSubmission { submission_id, dispatch_id, why })
                .collect(),
            examples_retired: report.examples_retired,
            error_kind: None,
            error: None,
        }
    }
}

crate::action_command! {
    /// Withdraw bucket-pending training submissions, by card or by id, with a journaled
    /// reason. Pending rows flip to retired and leave the bucket; a retired submission id
    /// replayed by the producer's retry stays retired. Rows already dispatched into a job
    /// are refused by name (that job's examples return through `return`). Only rows still
    /// pending are examined: a card's dispatched history is not scanned.
    pub struct TrainingTriggerRetire {
        state: Arc<TrainingTriggerState>,
    }
    name: "genome/training-trigger/retire",
    access: Privileged,
    params: RetireParams,
    output: RetireOutcome,
    run(this, ctx, p) => {
        let retired_by = ctx.caller.as_ref().map(|caller| caller.peer_id.as_uuid());
        retire(&this.state, p, retired_by).await
    }
}

pub(crate) async fn retire(
    state: &Arc<TrainingTriggerState>,
    p: RetireParams,
    retired_by: Option<Uuid>,
) -> Result<RetireOutcome, CommandError> {
    let reason = p.reason.trim();
    if reason.is_empty() {
        return Err(CommandError::Invalid(
            "a retirement carries its reason: say why these rows must not train".into(),
        ));
    }
    let selection = match (p.card_id, p.submission_ids.is_empty()) {
        (Some(card), true) => RetireSelection::Card(card),
        (None, false) => RetireSelection::Submissions(p.submission_ids),
        (Some(_), false) => {
            return Err(CommandError::Invalid(
                "retire by card OR by submission ids, not both: a card names its own rows".into(),
            ))
        }
        (None, true) => {
            return Err(CommandError::Invalid(
                "nothing selected: give cardId or submissionIds".into(),
            ))
        }
    };
    match state.retire(selection, reason.to_string(), retired_by).await {
        Ok(report) => Ok(report.into()),
        Err((kind, error)) => Ok(RetireOutcome::refused(kind, error)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::training_trigger::test_support::{
        build_runtime_trigger_only, build_runtime_with_trigger_and_genome, ex, submit_params,
    };
    use serde_json::json;

    fn on_card(card: Uuid, prompt: &str) -> crate::genome::fine_tuning::types::TrainingExample {
        let mut example = ex(prompt, "c");
        example.metadata = Some(json!({ "cardId": card }));
        example
    }

    async fn submit(
        executor: &crate::runtime::CommandExecutor,
        persona: Uuid,
        examples: Vec<crate::genome::fine_tuning::types::TrainingExample>,
    ) -> Uuid {
        let json = executor
            .execute_json(
                "genome/training-trigger/submit",
                submit_params(persona, "code/owner", examples, Some(100)),
            )
            .await
            .expect("test: submit");
        assert_eq!(json["success"], true, "got: {json}");
        json["acceptance"]["submissionId"]
            .as_str()
            .and_then(|s| s.parse().ok())
            .expect("test: submission id")
    }

    // what this catches (card 04a5e867, the 5090 on 2026-10-10): a card's turns settled
    // as passing under a review the room overturned; the verb must withdraw exactly that
    // card's pending rows, leave the other card's row in the bucket, journal the receipt,
    // refuse to withdraw the same rows twice as if they were new, and keep a retired
    // submission out of the bucket when the producer's retry replays its id.
    #[tokio::test]
    async fn retire_by_card_withdraws_that_cards_pending_rows_and_nothing_else() {
        let (trigger, executor, _dir) = build_runtime_trigger_only().await;
        let persona = Uuid::new_v4();
        let overturned = Uuid::new_v4();
        let other = Uuid::new_v4();
        let pre_1 = submit(&executor, persona, vec![on_card(overturned, "pre-1a"), on_card(overturned, "pre-1b")]).await;
        let pre_2 = submit(&executor, persona, vec![on_card(overturned, "pre-2")]).await;
        let kept = submit(&executor, persona, vec![on_card(other, "other-1")]).await;
        assert_eq!(trigger.state.bucket_example_count(persona, "code/owner", "synthetic"), Some(4));

        let json = executor
            .execute_json(
                "genome/training-trigger/retire",
                json!({ "cardId": overturned, "reason": "review 5769a6c3 failed these turns; the passing review bound to the later fix" }),
            )
            .await
            .expect("test: retire");
        assert_eq!(json["success"], true, "got: {json}");
        assert_eq!(json["examplesRetired"], 3, "got: {json}");
        assert_eq!(json["retirementIds"].as_array().map(Vec::len), Some(1), "one bucket, one receipt: {json}");
        let mut retired: Vec<Uuid> = json["retired"]
            .as_array()
            .expect("test: retired")
            .iter()
            .map(|r| r["submissionId"].as_str().and_then(|s| s.parse().ok()).expect("test: id"))
            .collect();
        retired.sort();
        let mut expected = vec![pre_1, pre_2];
        expected.sort();
        assert_eq!(retired, expected);
        assert_eq!(json["refused"].as_array().map(Vec::len), Some(0), "got: {json}");
        assert_eq!(
            trigger.state.bucket_example_count(persona, "code/owner", "synthetic"),
            Some(1),
            "the other card's row stays pending"
        );

        // A second call for the same card changes nothing and says so.
        let again = executor
            .execute_json(
                "genome/training-trigger/retire",
                json!({ "cardId": overturned, "reason": "rerun" }),
            )
            .await
            .expect("test: rerun");
        assert_eq!(again["success"], true, "got: {again}");
        assert_eq!(again["examplesRetired"], 0, "got: {again}");
        assert_eq!(again["retired"].as_array().map(Vec::len), Some(0), "got: {again}");
        assert_eq!(again["retirementIds"].as_array().map(Vec::len), Some(0), "no receipt for a no-op: {again}");

        // The producer retries a staged row with the SAME submission id: the retired id
        // replays as accepted (her staged credit settles as transferred) and appends nothing.
        let mut replay = submit_params(persona, "code/owner", vec![on_card(overturned, "pre-2")], Some(100));
        replay["submissionId"] = json!(pre_2);
        let replayed = executor
            .execute_json("genome/training-trigger/submit", replay)
            .await
            .expect("test: replay");
        assert_eq!(replayed["success"], true, "got: {replayed}");
        assert_eq!(replayed["acceptance"]["replayed"], true, "got: {replayed}");
        assert_eq!(trigger.state.bucket_example_count(persona, "code/owner", "synthetic"), Some(1));
        let _ = kept;
    }

    // what this catches: a submission already carried into a job is not a pending row;
    // retire must refuse it BY NAME with its dispatch, never flip it, so the operator
    // reaches for `return` on the job instead of believing the examples were withdrawn.
    #[tokio::test]
    async fn a_dispatched_submission_is_refused_with_its_dispatch_and_an_unknown_one_by_name() {
        let (trigger, executor, _dir) = build_runtime_with_trigger_and_genome().await;
        let persona = Uuid::new_v4();
        let card = Uuid::new_v4();
        let examples = (0..5).map(|i| on_card(card, &format!("p-{i}"))).collect();
        let json = executor
            .execute_json(
                "genome/training-trigger/submit",
                submit_params(persona, "test-trait", examples, Some(100)),
            )
            .await
            .expect("test: submit");
        let dispatched: Uuid = json["acceptance"]["submissionId"].as_str().and_then(|s| s.parse().ok()).expect("test: id");
        let flushed = executor
            .execute_json(
                "genome/training-trigger/flush",
                json!({ "personaId": persona, "traitKind": "test-trait", "baseModel": "synthetic" }),
            )
            .await
            .expect("test: flush");
        assert_eq!(flushed["outcome"], "JobDispatched", "got: {flushed}");
        assert_eq!(trigger.state.bucket_example_count(persona, "test-trait", "synthetic"), None);

        let unknown = Uuid::new_v4();
        let json = executor
            .execute_json(
                "genome/training-trigger/retire",
                json!({ "submissionIds": [dispatched, unknown], "reason": "too late" }),
            )
            .await
            .expect("test: retire");
        assert_eq!(json["success"], true, "got: {json}");
        assert_eq!(json["examplesRetired"], 0, "got: {json}");
        let refused = json["refused"].as_array().expect("test: refused");
        assert_eq!(refused.len(), 2, "got: {json}");
        let by_id = |id: Uuid| refused.iter().find(|r| r["submissionId"] == json!(id)).expect("test: named");
        assert!(by_id(dispatched)["dispatchId"].is_string(), "the dispatch is named: {json}");
        assert!(by_id(unknown)["dispatchId"].is_null(), "an unknown id has no dispatch: {json}");
        assert_eq!(json["retirementIds"].as_array().map(Vec::len), Some(0), "nothing withdrawn, no receipt: {json}");
    }

    // what this catches: a retirement without a reason, or with an ambiguous selection,
    // is refused before anything is read; the receipt's reason is the point of the verb.
    #[tokio::test]
    async fn an_empty_reason_or_an_ambiguous_selection_is_refused_up_front() {
        let (trigger, _executor, _dir) = build_runtime_trigger_only().await;
        let no_reason = retire(
            &trigger.state,
            RetireParams { card_id: Some(Uuid::new_v4()), submission_ids: vec![], reason: "  ".into() },
            None,
        )
        .await;
        assert!(matches!(no_reason, Err(CommandError::Invalid(_))), "{no_reason:?}");
        let both = retire(
            &trigger.state,
            RetireParams { card_id: Some(Uuid::new_v4()), submission_ids: vec![Uuid::new_v4()], reason: "x".into() },
            None,
        )
        .await;
        assert!(matches!(both, Err(CommandError::Invalid(_))), "{both:?}");
        let neither = retire(
            &trigger.state,
            RetireParams { card_id: None, submission_ids: vec![], reason: "x".into() },
            None,
        )
        .await;
        assert!(matches!(neither, Err(CommandError::Invalid(_))), "{neither:?}");
    }
}
