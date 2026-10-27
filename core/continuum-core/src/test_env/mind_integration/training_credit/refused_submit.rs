#![cfg(test)]
//! A reviewed credit whose training submit was refused (card 99b2773a).
use super::*;

// what this catches (card 99b2773a, Kimi's 28087ebb on the 5090, 2026-10-27): a decided credit
// the trigger refused was answered once and kept nowhere, so it read awaiting_acceptance with no
// error and waited for a review event or a restart. The refusal is kept and named, her staging
// sees it waiting, and the next pass the trigger accepts clears it.
#[tokio::test]
pub(super) async fn a_refused_reviewed_credit_is_kept_named_and_accepted_on_the_next_pass() {
    use crate::persona::training_producer::reviewed::ReviewedCreditState as State;
    let (fx, _capture) = exact_review().await;
    let board = fx.project(vec![(airc_work::WorkEvent::WorkSubmissionReviewed(fx.pass.clone()), fx.reviewer)]);
    let consume = |submit| {
        let conn = settlement_connection(fx.executor.clone(), fx.persona, submit, None);
        let board = board.clone();
        let (name, persona, room, review) = (fx.name, fx.persona, fx.room, fx.pass.review_id);
        async move {
            reviewed::consume_review_with_boundary(&conn, name, persona, room, &board, review, &reviewed::SelfReview::NotAdmitted, None)
                .await
                .expect("the review is consumed")
        }
    };

    let refusing = continuum_client::mock::MockTransport::new();
    script_settlement_reply(
        &refusing,
        &fx.selected,
        Some(json!({"success": false, "errorKind": "RecoveryRequired", "error": "the trigger is still recovering its buckets"})),
    );
    assert_eq!(consume(refusing).await.state, State::AwaitingAcceptance);
    let status = reviewed::credit_status(&fx.data, fx.name, fx.selection.submission_id).await.expect("status");
    assert_eq!(status.state, State::AwaitingAcceptance);
    assert_eq!(
        status.destination.and_then(|d| d.error_kind).as_deref(),
        Some("RecoveryRequired"),
        "the waiting credit names why it waits"
    );
    assert!(reviewed::refused_credit_waiting(&fx.data, fx.name).await.expect("read"), "her staging sees it waiting");

    let accepting = continuum_client::mock::MockTransport::new();
    script_settlement_reply(
        &accepting,
        &fx.selected,
        Some(json!({"success": true, "outcome": "BatchAppended", "currentCount": 1, "threshold": 16,
            "acceptance": {"submissionId": fx.selected.id, "replayed": false}})),
    );
    assert_eq!(consume(accepting).await.state, State::Accepted);
    assert!(!reviewed::refused_credit_waiting(&fx.data, fx.name).await.expect("read"), "acceptance clears the refusal");
}
