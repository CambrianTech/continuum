#![cfg(test)]
//! Her examples are the calls she lived (card ad107e18; Codex's audit e=3d13e8fd). A turn
//! whose capture is incomplete stays staged and named, never trains as its flat prompt and
//! completion; a capture that lands later completes it from the row's OWN receipt cursors,
//! under her persona, and it transfers once. A missing record and a transferred row both
//! hold no lived copy; the accepted transfer is what tells them apart.
use super::*;

fn never_submit() -> continuum_client::mock::MockTransport {
    let never = continuum_client::mock::MockTransport::new();
    never.respond_to("genome/training-trigger/submit", |_| {
        panic!("a row without its lived calls, or one already transferred, must not dispatch")
    });
    never
}

// what this catches: the ordinary settle path training an incomplete capture flat, a late
// capture never completing its row, or a completed row transferring twice across a shed
// copy and a restart.
#[tokio::test]
pub(super) async fn an_incomplete_capture_waits_and_a_late_one_transfers_once() {
    let home = tempfile::tempdir().unwrap(); // JUSTIFIED: test fixture
    let _home = HomeGuard::set(home.path()).await;
    crate::persona::register_substrate_orm_entities(crate::orm::OrmEntityRegistry::global())
        .unwrap(); // JUSTIFIED: test fixture
    let executor = data_runtime();
    let persona = Uuid::new_v4();
    let name = "lived-settlement";
    let data = conn_as(executor.clone(), persona);

    // Two calls, one of whose captures never landed: no complete record, ever.
    let (_, partial) = stage_settlement_fixture(
        executor.clone(),
        persona,
        name,
        Uuid::new_v4(),
        vec![served_receipt("partial-seen", "model-a"), uncaptured("partial-lost", "model-a")],
    )
    .await;
    assert!(partial.lived.is_none(), "an incomplete capture stages no lived record");

    // One call whose capture was unreadable while she staged, and lands after.
    let late = captured(persona, served_receipt("late", "model-a"));
    let (_, delayed) = captures_unreadable(stage_settlement_fixture(
        executor.clone(),
        persona,
        name,
        Uuid::new_v4(),
        vec![late.clone()],
    ))
    .await;
    assert!(delayed.lived.is_none(), "unreadable at staging, so staged without it");

    let refused = settlement_connection(executor.clone(), persona, never_submit(), None);
    let substrate = substrate_conn(&executor);
    assert!(!settle_staged_row(&refused, &substrate, persona, name, &partial, true).await.unwrap()); // JUSTIFIED: test asserts the Ok arm
    assert!(
        stored_credit_rows(&data, name).await.iter().any(|r| r.id == partial.id),
        "the waiting row stays staged"
    );

    // Exact recovery: the call submitted is the one captured for this receipt, whole: its
    // request, her reasoning, her tool calls and her text, read back from its own cursor.
    let written = lived_call(&late);
    let (cursor, id) = (late.capture.clone(), delayed.id);
    let submit = continuum_client::mock::MockTransport::new();
    submit.respond_to("genome/training-trigger/submit", move |params| {
        let examples = params["examples"].as_array().cloned().unwrap_or_default(); // unwrap_or_default: a missing list fails the length check
        assert_eq!(examples.len(), 1, "one call, one example");
        let lived = &examples[0]["lived"];
        assert_eq!(lived["capture"], json!(cursor), "read back from this receipt's own cursor");
        assert_eq!(lived["request"], serde_json::to_value(&written.request).unwrap(), "her exact request"); // JUSTIFIED: test fixture
        assert_eq!(lived["response"], serde_json::to_value(&written.response).unwrap(), "her exact response: reasoning, tool calls, text"); // JUSTIFIED: test fixture
        assert_eq!(examples[0]["completion"], json!(written.response.text));
        Ok(json!({"success":true,"acceptance":{"submissionId":id,"replayed":false}}))
    });
    let conn = settlement_connection(executor.clone(), persona, submit, None);
    assert!(
        settle_staged_row(&conn, &substrate, persona, name, &delayed, true).await.unwrap(), // JUSTIFIED: test asserts the Ok arm
        "the late capture completes its row from the row's own cursor, and it transfers"
    );

    // Restart: a new runtime over the same store meets the transferred row with its copy shed.
    let reopened = data_runtime();
    let stored = stored_credit_rows(&conn_as(reopened.clone(), persona), name).await;
    let transferred = stored.iter().find(|r| r.id == delayed.id).expect("the transferred row keeps its receipts"); // JUSTIFIED: test fixture
    let waiting = stored.iter().find(|r| r.id == partial.id).expect("the waiting row is still staged"); // JUSTIFIED: test fixture
    assert!(transferred.lived.is_none() && waiting.lived.is_none(), "neither holds a lived copy");
    let reopened_data = conn_as(reopened.clone(), persona);
    assert!(reviewed::transfer_accepted(&reopened_data, name, transferred.id).await.unwrap()); // JUSTIFIED: test asserts the Ok arm
    assert!(!reviewed::transfer_accepted(&reopened_data, name, waiting.id).await.unwrap()); // JUSTIFIED: test asserts the Ok arm
    let replay = settlement_connection(reopened.clone(), persona, never_submit(), None);
    let reopened_substrate = substrate_conn(&reopened);
    assert!(!settle_staged_row(&replay, &reopened_substrate, persona, name, transferred, true).await.unwrap()); // JUSTIFIED: test asserts the Ok arm
    assert!(!settle_staged_row(&replay, &reopened_substrate, persona, name, waiting, true).await.unwrap()); // JUSTIFIED: test asserts the Ok arm
}

/// Her self-review of the fixture's submission, admitted, consumed on `conn`.
async fn consume_own_review(
    fx: &ExactReview,
    conn: &Connection<impl Transport>,
) -> reviewed::ReviewedCreditState {
    use airc_work::{WorkEvent, WorkSubmissionReview};
    let self_card = airc_work::WorkCardId::new();
    let self_claim = airc_work::ClaimId::new();
    let own = WorkSubmissionReview {
        review_card_id: self_card,
        review_claim_id: self_claim,
        reviewer: fx.owner,
        ..fx.pass.clone()
    };
    let board = fx.project(vec![
        (fx.create(self_card, Some(fx.card_id)), fx.owner),
        (ExactReview::claim(self_card, self_claim, fx.owner), fx.owner),
        (WorkEvent::WorkSubmissionReviewed(own.clone()), fx.owner),
    ]);
    reviewed::consume_review_with_boundary(
        conn,
        fx.name,
        fx.persona,
        fx.room,
        &board,
        own.review_id,
        &reviewed::SelfReview::Admitted,
        None,
    )
    .await
    .unwrap() // JUSTIFIED: test fixture
    .state
}

// what this catches: the reviewed path passing a bound snapshot whose capture is incomplete
// as flat text, failing to complete one whose capture landed after she staged, or
// dispatching it again after a restart on either path.
#[tokio::test]
pub(super) async fn a_reviewed_snapshot_without_its_lived_calls_is_ineligible_until_they_land() {
    use reviewed::ReviewedCreditState as State;
    {
        // Its own home: dropped (with its guard) before the next fixture takes one.
        let (partial, _capture) = exact_review_staged(
            None,
            |_| vec![served_receipt("reviewed-seen", "actual-model"), uncaptured("reviewed-lost", "actual-model")],
            false,
        )
        .await;
        assert!(partial.selected.lived.is_none());
        let refused = settlement_connection(partial.executor.clone(), partial.persona, never_submit(), None);
        assert_eq!(consume_own_review(&partial, &refused).await, State::IneligibleEvidence);
    }

    let (late, _capture) = exact_review_staged(
        None,
        |_| vec![served_receipt("reviewed-late", "actual-model")],
        true,
    )
    .await;
    assert!(late.selected.lived.is_none(), "unreadable at staging, so bound without it");
    let submit = continuum_client::mock::MockTransport::new();
    script_settlement_reply(
        &submit,
        &late.selected,
        Some(json!({"success":true,"acceptance":{"submissionId":late.selected.id,"replayed":false}})),
    );
    let conn = settlement_connection(late.executor.clone(), late.persona, submit, None);
    assert_eq!(consume_own_review(&late, &conn).await, State::Accepted, "the landed capture completes the snapshot's params");
    // Restart: a new runtime over the same store. The recovered calls fed only the params, so
    // the stored row still holds no lived copy; the review's acceptance marks it done, and
    // neither path dispatches it again.
    let reopened = data_runtime();
    let reopened_data = conn_as(reopened.clone(), late.persona);
    let stored = stored_credit_rows(&reopened_data, late.name).await;
    let row = stored.iter().find(|r| r.id == late.selected.id).expect("the reviewed row is stored"); // JUSTIFIED: test fixture
    assert!(row.lived.is_none(), "the stored row was never rewritten");
    // The review path's own acceptance, keyed by the submission (not the row's settle record).
    assert_eq!(
        reviewed::credit_status(&reopened_data, late.name, late.selection.submission_id).await.unwrap().state, // JUSTIFIED: test asserts the Ok arm
        State::Accepted
    );
    let replay = settlement_connection(reopened.clone(), late.persona, never_submit(), None);
    assert_eq!(consume_own_review(&late, &replay).await, State::Accepted, "and it transfers once");
    assert!(
        !settle_staged_row(&replay, &substrate_conn(&reopened), late.persona, late.name, row, true).await.unwrap(), // JUSTIFIED: test asserts the Ok arm
        "the ordinary path meets the review's reservation of the same generations"
    );
}
