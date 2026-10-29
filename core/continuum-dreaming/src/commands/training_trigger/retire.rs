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
        // who decided, through the one resolver for who is acting (see resolve)
        let retired_by = crate::persona::operator_peer::acting_now(ctx, "genome/training-trigger/retire")?.peer;
        retire(&this.state, p, Some(retired_by)).await
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

    /// Retire as a signed local caller: a decision is attributed to whoever is acting
    /// (`operator_peer::acting_now`), and this test runtime has no operator self-peer online.
    async fn retire_as_operator(executor: &crate::runtime::CommandExecutor, params: serde_json::Value) -> Result<serde_json::Value, String> {
        let operator = crate::routing::CallerIdentity::local(crate::identity::PeerId::new());
        executor.execute_with_caller("genome/training-trigger/retire", params, Some(operator)).await?.to_json_value()
    }

    // what this catches (Codex root on Sahar's 2b4e7272, whose RetryDecision recorded
    // resolvedBy NULL): a decision recorded with no decider. A retire nobody can be named for
    // (no signed caller, and no operator self-peer online to act as) is refused and withdraws
    // nothing; it is never journaled unattributed.
    #[tokio::test]
    async fn a_retire_nobody_can_be_named_for_is_refused_and_withdraws_nothing() {
        let (trigger, executor, _dir) = build_runtime_trigger_only().await;
        let persona = Uuid::new_v4();
        let card = Uuid::new_v4();
        submit(&executor, persona, vec![on_card(card, "only")]).await;
        let refused = executor
            .execute_json("genome/training-trigger/retire", json!({ "cardId": card, "reason": "unattributed" }))
            .await;
        // a refused command surfaces as an error or as an unsuccessful result; either way it names why
        let (failed, text) = match &refused {
            Ok(json) => (json["success"] == false, json.to_string()),
            Err(e) => (true, e.clone()),
        };
        assert!(failed && text.contains("operator self-peer"), "an unnameable decider is refused by name: {refused:?}");
        assert_eq!(trigger.state.bucket_example_count(persona, "code/owner", "synthetic"), Some(1), "and nothing is withdrawn");
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
        // card 4545eafd: a withdrawn example's demand clears; another card's stays
        use crate::genome::fine_tuning::training_demand::TrainingDemand;
        let demand = TrainingDemand::for_persona(persona);
        let (withdrawn, staying) = (TrainingDemand::key_of(&on_card(overturned, "pre-1a")), TrainingDemand::key_of(&on_card(other, "other-1")));
        demand.record(&[(withdrawn.clone(), 64_256), (staying.clone(), 51_712)], &[], None).expect("test: demand recorded");

        let json = retire_as_operator(
            &executor,
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
        assert!(!demand.records_all(std::slice::from_ref(&withdrawn)).expect("test: readable"), "a withdrawn example reserves nothing");
        assert!(demand.records_all(std::slice::from_ref(&staying)).expect("test: readable"), "a pending one keeps its demand");

        // A second call for the same card changes nothing and says so.
        let again = retire_as_operator(
            &executor,
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

    // what this catches (card 4545eafd; Codex on #5572): a retirement that commits over a demand
    // clear that never happened. A corrupt demand store read leniently is "empty", the clear
    // is a no-op, and the rows flip with the withdrawn examples' reservation left behind. A
    // store that cannot answer fails the retirement: the rows stay pending, the file keeps its
    // bytes, and once the store is repaired the same retirement finishes.
    #[tokio::test]
    async fn a_demand_store_that_cannot_answer_fails_the_retirement_and_flips_nothing() {
        let (trigger, executor, _dir) = build_runtime_trigger_only().await;
        let persona = Uuid::new_v4();
        let card = Uuid::new_v4();
        submit(&executor, persona, vec![on_card(card, "w-1"), on_card(card, "w-2")]).await;
        assert_eq!(trigger.state.bucket_example_count(persona, "code/owner", "synthetic"), Some(2));
        use crate::genome::fine_tuning::training_demand::TrainingDemand;
        let root = crate::genome::fine_tuning::job_dir::genome_root().expect("test: a home");
        let file = TrainingDemand::store_dir(&root).join(format!("{persona}.json"));
        std::fs::create_dir_all(file.parent().expect("test: parent")).expect("test: dir");
        std::fs::write(&file, b"{ torn").expect("test: corrupt store");

        let failed = retire_as_operator(&executor, json!({ "cardId": card, "reason": "superseded" })).await;
        let refused = match failed {
            Ok(json) => json["success"] != true,
            Err(_) => true,
        };
        assert!(refused, "a store that cannot answer fails the retirement");
        assert_eq!(trigger.state.bucket_example_count(persona, "code/owner", "synthetic"), Some(2), "the rows stay pending");
        assert_eq!(std::fs::read(&file).expect("test: read"), b"{ torn", "the store keeps its bytes");

        std::fs::remove_file(&file).expect("test: repair the store");
        let finished = retire_as_operator(&executor, json!({ "cardId": card, "reason": "superseded" })).await.expect("test: rerun");
        assert_eq!(finished["success"], true, "the same retirement finishes once the store answers: {finished}");
        // every row retired: the bucket is gone, not an empty one (Codex read CI: None)
        assert!(
            matches!(trigger.state.bucket_example_count(persona, "code/owner", "synthetic"), None | Some(0)),
            "nothing of hers stays pending"
        );
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
        let json = retire_as_operator(
            &executor,
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
