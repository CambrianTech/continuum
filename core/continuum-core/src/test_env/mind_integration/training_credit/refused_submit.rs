#![cfg(test)]
//! A reviewed credit whose training submit was refused (card 99b2773a).
use super::*;
use crate::persona::training_producer::reviewed::ReviewedCreditState as State;
use crate::runtime::{BrainRegion, CadenceHint, RegionContext};

/// A credit-acceptance region over `fx`'s store, whose trigger answers with `reply` (`None`:
/// the trigger must not be called at all).
fn region_answering(fx: &ExactReview, reply: Option<serde_json::Value>) -> CreditAcceptanceRegion<SettlementTransport> {
    let (executor, persona, name, selected) = (fx.executor.clone(), fx.persona, fx.name, fx.selected.clone());
    CreditAcceptanceRegion::new(Arc::new(move |asked| {
        assert_eq!(asked, persona, "the region settles only the persona it is ticked for");
        let trigger = continuum_client::mock::MockTransport::new();
        match &reply {
            Some(reply) => script_settlement_reply(&trigger, &selected, Some(reply.clone())),
            None => {
                trigger.respond_to("genome/training-trigger/submit", |_| panic!("an accepted credit is never submitted again"));
            }
        }
        Some((name.to_string(), settlement_connection(executor.clone(), persona, trigger, None)))
    }))
}

/// Tick `region` for her until her credit's status satisfies `done`, at most `passes` times,
/// `between` apart: a settle runs off the tick (card a4e8756d), so its outcome is observed on a
/// later look, never assumed right after the pass that started it.
async fn tick_until<T: continuum_client::Transport + Send + Sync + 'static>(
    region: &CreditAcceptanceRegion<T>,
    fx: &ExactReview,
    passes: usize,
    between: std::time::Duration,
    done: impl Fn(&reviewed::ReviewedCredit) -> bool,
) -> bool {
    let scoped = RegionContext { tick_number: 1, persona_scope: Some(fx.persona) };
    for _ in 0..passes {
        region.tick(&scoped).await;
        tokio::time::sleep(between).await;
        if reviewed::credit_status(&fx.data, fx.name, fx.selection.submission_id).await.is_ok_and(|status| done(&status)) {
            return true;
        }
    }
    false
}

// what this catches (card 99b2773a, Kimi's 28087ebb on the 5090, 2026-10-27, Codex on #5531):
// a decided credit the trigger refused waited for a review event, a registration or her next
// turn, and an idle citizen has none; a credit decided before refusals were kept had no row to
// find it by. The region settles it from the decision alone, under the same submission id, with
// no review, registration or turn; and once accepted it is never submitted again.
#[tokio::test]
pub(super) async fn the_region_settles_a_refused_decided_credit_with_no_turn_review_or_registration() {
    let (fx, _capture) = exact_review().await;
    let board = fx.project(vec![(airc_work::WorkEvent::WorkSubmissionReviewed(fx.pass.clone()), fx.reviewer)]);
    let refusing = continuum_client::mock::MockTransport::new();
    script_settlement_reply(
        &refusing,
        &fx.selected,
        Some(json!({"success": false, "errorKind": "RecoveryRequired", "error": "the trigger is still recovering its buckets"})),
    );
    let conn = settlement_connection(fx.executor.clone(), fx.persona, refusing, None);
    let decided = reviewed::consume_review_with_boundary(&conn, fx.name, fx.persona, fx.room, &board, fx.pass.review_id, &reviewed::SelfReview::NotAdmitted, None)
        .await
        .expect("the review is consumed");
    assert_eq!(decided.state, State::AwaitingAcceptance);
    let status = reviewed::credit_status(&fx.data, fx.name, fx.selection.submission_id).await.expect("status");
    assert_eq!(status.destination.and_then(|d| d.error_kind).as_deref(), Some("RecoveryRequired"), "the waiting credit names why it waits");

    // A decision kept before refusals were: no refusal row, only the decision to find it by.
    let delete = crate::orm::types::BatchOperation {
        operation_type: crate::orm::types::BatchOperationType::Delete,
        collection: reviewed::CreditSubmitRefusal::COLLECTION.into(),
        id: Some(fx.selection.submission_id.to_string()),
        data: None,
    };
    let forget = json!({"operations": [delete], "dbPath": format!("@persona:{}", fx.name)});
    fx.data.commands().execute_value("data/batch", forget).await.expect("delete the refusal row");

    let scoped = RegionContext { tick_number: 1, persona_scope: Some(fx.persona) };
    let still_refusing = region_answering(&fx, Some(json!({"success": false, "errorKind": "BucketHeldFull", "error": "held at its bound"})));
    assert_eq!(still_refusing.tick(&scoped).await.cadence_hint, Some(CadenceHint::Slower), "an owed credit backs the region off");
    let refused = |status: &reviewed::ReviewedCredit| status.destination.as_ref().and_then(|d| d.error_kind.as_deref()) == Some("BucketHeldFull");
    assert!(tick_until(&still_refusing, &fx, 40, std::time::Duration::from_millis(50), refused).await, "the refusal is recorded once its settle has run");
    let status = reviewed::credit_status(&fx.data, fx.name, fx.selection.submission_id).await.expect("status");
    assert_eq!(status.state, State::AwaitingAcceptance);

    let accepting = region_answering(
        &fx,
        Some(json!({"success": true, "outcome": "BatchAppended", "currentCount": 1, "threshold": 16,
            "acceptance": {"submissionId": fx.selected.id, "replayed": false}})),
    );
    let accepted = tick_until(&accepting, &fx, 40, std::time::Duration::from_millis(50), |status| status.state == State::Accepted).await;
    assert!(accepted, "settled with no review event, registration or turn");

    let never_again = region_answering(&fx, None);
    assert_eq!(never_again.tick(&scoped).await.cadence_hint, Some(CadenceHint::Sleep), "nothing owed: the region sleeps");
}

// what this catches (Codex on #5531 and #5547): a scan that moved past a whole page before
// settling any of it, so a decision whose settle always outlasted the governor's tick starved every
// decision after it; a malformed row that stopped the scan where it stood; and a hung settle with
// nothing waiting behind it never retried. Progress is kept per decision: the malformed row is
// named and passed, the tick never waits on the settle, the hung settle is cut once past its
// quantum (the governor's tick bound), and the scan settles that same submission on a later pass.
#[tokio::test]
pub(super) async fn a_bad_or_hanging_decision_never_starves_the_rest() {
    let (fx, _capture) = exact_review().await;
    let board = fx.project(vec![(airc_work::WorkEvent::WorkSubmissionReviewed(fx.pass.clone()), fx.reviewer)]);
    let refusing = continuum_client::mock::MockTransport::new();
    script_settlement_reply(&refusing, &fx.selected, Some(json!({"success": false, "errorKind": "RecoveryRequired"})));
    let conn = settlement_connection(fx.executor.clone(), fx.persona, refusing, None);
    reviewed::consume_review_with_boundary(&conn, fx.name, fx.persona, fx.room, &board, fx.pass.review_id, &reviewed::SelfReview::NotAdmitted, None)
        .await
        .expect("the review is consumed");

    // A decision row ahead of hers in id order whose data is not a decision.
    let malformed = crate::orm::types::BatchOperation {
        operation_type: crate::orm::types::BatchOperationType::Create,
        collection: reviewed::CreditReviewDecision::COLLECTION.into(),
        id: Some("00000000-0000-0000-0000-000000000001".into()),
        data: Some(json!({"id": "00000000-0000-0000-0000-000000000001", "bindingId": fx.selection.submission_id, "review": "not a review"})),
    };
    let insert = json!({"operations": [malformed], "dbPath": format!("@persona:{}", fx.name)});
    let inserted = fx.data.commands().execute_value("data/batch", insert).await.expect("insert the malformed row");
    assert_ne!(inserted["success"], json!(false), "the malformed row is stored: {inserted}");

    // Her settle hangs on the region's first pass (a submit that never returns), then the
    // trigger accepts.
    let passes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let (executor, persona, name, selected) = (fx.executor.clone(), fx.persona, fx.name, fx.selected.clone());
    let counted = Arc::clone(&passes);
    let region = CreditAcceptanceRegion::new(Arc::new(move |_| {
        let trigger = continuum_client::mock::MockTransport::new();
        script_settlement_reply(&trigger, &selected, Some(json!({"success": true, "outcome": "BatchAppended", "currentCount": 1,
            "threshold": 16, "acceptance": {"submissionId": selected.id, "replayed": false}})));
        let hang = (counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0)
            .then(|| (Arc::new(tokio::sync::Notify::new()), Arc::new(tokio::sync::Notify::new())));
        Some((name.to_string(), settlement_connection(executor.clone(), persona, trigger, hang)))
    }));
    let scoped = RegionContext { tick_number: 1, persona_scope: Some(fx.persona) };
    let started = std::time::Instant::now();
    assert_eq!(region.tick(&scoped).await.cadence_hint, Some(CadenceHint::Slower), "the malformed row and the cut settle both still wait");
    assert!(started.elapsed() < std::time::Duration::from_secs(5), "the tick never waits on the hanging settle");
    let status = reviewed::credit_status(&fx.data, fx.name, fx.selection.submission_id).await.expect("status");
    assert_eq!(status.state, State::AwaitingAcceptance);

    // past its quantum (5 s) the hung settle is cut, and the retry settles her: 60 looks, 250 ms apart
    let accepted = tick_until(&region, &fx, 60, std::time::Duration::from_millis(250), |status| status.state == State::Accepted).await;
    assert!(accepted, "the scan came round to her decision past the malformed row and the hang");
}
