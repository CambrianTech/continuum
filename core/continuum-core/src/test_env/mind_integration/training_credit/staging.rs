#![cfg(test)]
use super::*;
use crate::runtime::LateBound;

// what this catches: Astra/S6 on 799b8fe9 — `stage_credit` DISCARDED the results of
// data/ensure-schema and data/batch. Those commands report adapter failure INSIDE the
// payload as StorageResult{success:false}, and InProcessTransport forwards it
// unchanged, so `.await?` caught only transport breakage. A child constraint
// violation rolled the batch back and the caller still returned Ok(submission_id) and
// emitted `training.credit.staged` for rows that do not exist. THE TEST PASSED
// BECAUSE THE CHECK COULD NOT FAIL (card 36e8fdd6).
//
// The rollback is provoked the way production would hit it: TWO receipts sharing one
// submittedRequestId, violating the unique index on (stagedCreditId,
// submittedRequestId) that persona::register_substrate_orm_entities exists to keep.
#[tokio::test]
pub(super) async fn a_rolled_back_batch_is_never_reported_as_staged_credit() {
    let home = tempfile::tempdir().expect("tempdir");
    let _guard = HomeGuard::set(home.path()).await;
    crate::persona::register_substrate_orm_entities(crate::orm::OrmEntityRegistry::global())
        .expect("register staging entities");

    let persona = Uuid::new_v4();
    // Built directly rather than through a WorkCard fixture: what `stage_credit`
    // consumes is the CREDIT, and a claimless card is the honest minimum — a card
    // with no accepted claim still stages, it simply can never be stamped.
    let credit = CapturedCredit {
        task: crate::cognition::held_out::CardTask::NotATask,
        card_id: Uuid::new_v4(),
        claim: None,
    };
    let executor = data_runtime();

    // POSITIVE CONTROL FIRST. Without it, a `stage_credit` that refused everything
    // would satisfy the rollback assertion below for entirely the wrong reason.
    let ok = stage_credit(
        &conn_as(executor.clone(), persona),
        "anwen",
        &credit,
        vec![receipt("req-a"), receipt("req-b")],
        "prompt".to_string(),
        "completion".to_string(),
        None,
        Uuid::new_v4(),
        &[],
    )
    .await;
    let submission = ok.expect("two DISTINCT receipts stage cleanly");

    // THE REGRESSION: one submittedRequestId used twice violates the unique index,
    // the batch rolls back, and `stage_credit` must REPORT that rather than hand back
    // a submission id for rows the database threw away.
    let rolled_back = stage_credit(
        &conn_as(executor, persona),
        "anwen",
        &credit,
        vec![receipt("req-dup"), receipt("req-dup")],
        "prompt".to_string(),
        "completion".to_string(),
        None,
        Uuid::new_v4(),
        &[],
    )
    .await;
    let err =
        rolled_back.expect_err("a rolled-back batch must NOT be reported as staged credit");
    assert_ne!(
        format!("{err}"),
        String::new(),
        "the failure must name itself, not surface as an empty error"
    );

    // And the two runs are distinguishable: the control produced a real id.
    assert!(
        !submission.is_nil(),
        "the clean stage returned a real submission id"
    );
}

// what this catches: 6c36c24d — the actual public verbs bind before signed
// publication, preserve caller/room, and report review versus credit honestly.
#[tokio::test]
pub(super) async fn work_submission_commands_bind_before_publication_and_preserve_identity() {
    use crate::modules::work::submission::*;
    use crate::sdk_codegen::{ActionCommand, Ctx};
    let home = tempfile::tempdir().unwrap();
    let _home = HomeGuard::set(home.path()).await;
    crate::persona::register_substrate_orm_entities(crate::orm::OrmEntityRegistry::global())
        .unwrap();
    let airc_home = home.path().join("isolated-airc");
    let airc = Arc::new(
        airc_lib::Airc::open_with_wire_root_for_test(&airc_home, &airc_home)
            .await
            .unwrap(),
    );
    let room = airc.join("ordinary-project").await.unwrap();
    let persona = airc.peer_id().as_uuid();
    let name = "public-submission-owner";
    let registry = crate::persona::PersonaAircRuntimeRegistry::new();
    registry.register(crate::persona::PersonaAircRuntime::from_attached(
        persona,
        name,
        airc_home,
        airc.clone(),
        room.channel,
        crate::persona::identity_provider::PersonaIdentitySource::FreshlyMinted,
    ));
    let executor = data_runtime();
    let slot = Arc::new(LateBound::new("work submission fixture"));
    slot.install(executor.clone());
    let ctx = Ctx {
        caller: Some(CallerIdentity::local_persona(airc.peer_id())),
        ..Default::default()
    };
    let repo = airc_work::RepoId::new("acme/ordinary-project").unwrap();
    let card = airc
        .create_work_card(airc_lib::CreateWorkCard::new(
            repo.clone(),
            "verified ordinary work",
            airc_work::Priority::P1,
        ))
        .await
        .unwrap();
    let claim = airc
        .claim_work_card(airc_lib::ClaimWorkCard {
            card_id: card,
            ttl_ms: 60_000,
        })
        .await
        .unwrap();
    let mut capture = TurnCreditCapture::with_executor(
        executor.clone(),
        persona,
        name.into(),
        "Inspect and repair the actual implementation, with evidence.".into(),
        CapturedCredit {
            task: crate::cognition::held_out::CardTask::NotATask,
            card_id: card.as_uuid(),
            claim: Some(ClaimReceipt {
                claim_id: claim.as_uuid(),
                owner: airc.peer_id(),
                role: CreditRole::Owner,
            }),
        },
    );
    assert!(
        capture
            .record(
                &settlement_acts(4),
                &[served_receipt("public-command-request", "served-model")],
                None,
                true
            )
            .await
    );
    let selected = stored_credit_rows(&conn_as(executor, persona), name)
        .await
        .pop()
        .unwrap();
    // THROUGH SERDE, CARRYING THE SHORT HANDLES THE BOARD PRINTS (Astra's review of
    // #4357). The defect these verbs carried was a DESERIALIZATION refusal — a
    // `Uuid`-typed `card_id` rejected `d61513e4` before any handler ran — so a test
    // that builds the struct in Rust cannot see it at all. This fixture now arrives
    // the way a citizen's call actually does: as JSON, in the 8-char form she was
    // shown, and the assertions below prove the whole path end to end.
    // A typed artifact must have its bytes retained (card e1babb61), so the fixture
    // retains a real patch in this test's store and types ITS hash and size.
    let patch = "diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1 +1 @@\n-a\n+b\n";
    let patch_hash = {
        use airc_blobs::ContentAddressedStore;
        crate::media::artifact::store().expect("test: store").put(patch.as_bytes()).expect("test: retain the patch").to_hex()
    };
    let params: WorkSubmitParams = serde_json::from_value(serde_json::json!({
        "room": room.channel.as_uuid().to_string(),
        "submission_id": Uuid::new_v4().to_string(),
        "card_id": airc_core::shown_id::shown_form(card.as_uuid()),
        "claim_id": airc_core::shown_id::shown_form(claim.as_uuid()),
        "instance": "generic-project-work",
        "base_sha": "a".repeat(40),
        "artifact": { "hash": patch_hash, "size_bytes": patch.len(), "mime": "text/x-diff" },
        "staged_revision_id": selected.id.to_string(),
    }))
    .expect("a citizen's call carries the handles her board printed"); // expect: the fixture's own json, asserted decodable
    assert_eq!(
        params.card_id,
        airc_core::shown_id::shown_form(card.as_uuid()),
        "the handle survives deserialization — it used to be refused here"
    );
    let submit = WorkSubmit {
        registry: registry.clone(),
        executor_slot: slot.clone(),
        benchmark: std::sync::Arc::new(crate::modules::work_seams::CoreBenchmarkSubmission),
    };
    let published = submit.run(&ctx, params.clone()).await.unwrap();
    assert_eq!(published.publisher, persona);
    assert_eq!(published.bound_staged_revision_id, Some(selected.id));
    let mut overlap = params.clone();
    overlap.submission_id = Some(Uuid::new_v4());
    assert!(submit.run(&ctx, overlap.clone()).await.is_err());
    assert!(
        !airc
            .work_board_in(&room)
            .await
            .unwrap()
            .card(card)
            .unwrap()
            .submissions
            .iter()
            .any(|s| Some(s.submission_id.as_uuid()) == overlap.submission_id),
        "failed binding cannot publish its artifact"
    );
    let review_card = airc_work::submission_review_card_id(
        card,
        airc_work::SubmissionId::from_uuid(published.submission_id),
    );
    let handoff_board = airc.work_board_in(&room).await.unwrap();
    assert!(handoff_board.review_request(review_card).is_some());
    assert_eq!(handoff_board.card(review_card).unwrap().reviews, Some(card));
    let review_claim = airc
        .claim_work_card(airc_lib::ClaimWorkCard {
            card_id: review_card,
            ttl_ms: 60_000,
        })
        .await
        .unwrap();
    let focus = airc.join("unrelated-focus").await.unwrap();
    let replay = submit.run(&ctx, params.clone()).await.unwrap();
    assert_eq!(replay.submitted_at_ms, published.submitted_at_ms);
    let failed = WorkReview {
        registry: registry.clone(),
        executor_slot: slot.clone(),
    }
    .run(
        &ctx,
        WorkReviewParams {
            room: params.room.clone(),
            review_id: Some(Uuid::new_v4()),
            card_id: Some(card.as_uuid().to_string()),
            submission_id: Some(published.submission_id),
            artifact: params.artifact.clone(),
            review_card_id: review_card.as_uuid().to_string(),
            review_claim_id: Some(review_claim.as_uuid()),
            outcome: ReviewOutcome::Failed,
            evidence_text: None,
            evidence: params.artifact.clone(),
        },
    )
    .await
    .unwrap();
    assert!(matches!(failed.outcome, ReviewOutcome::Failed));
    let retry_after_review = submit.run(&ctx, params.clone()).await.unwrap();
    assert_eq!(retry_after_review.submission_id, published.submission_id);
    assert!(retry_after_review.next.contains("Revise the work"));
    let board_after_review = airc.work_board_in(&room).await.unwrap();
    assert_eq!(board_after_review.card(card).unwrap().claim_id, handoff_board.card(card).unwrap().claim_id);
    assert_eq!(board_after_review.card(review_card).unwrap().claim_id, Some(review_claim));
    let reviewed = WorkReview {
        registry: registry.clone(),
        executor_slot: slot.clone(),
    }
    .run(
        &ctx,
        WorkReviewParams {
            room: params.room.clone(),
            review_id: Some(Uuid::new_v4()),
            card_id: Some(card.as_uuid().to_string()),
            submission_id: Some(
                params
                    .submission_id
                    .expect("the fixture names its submission"),
            ),
            artifact: Some(
                params
                    .artifact
                    .clone()
                    .expect("the fixture names its artifact"),
            ),
            review_card_id: review_card.as_uuid().to_string(),
            review_claim_id: Some(review_claim.as_uuid()),
            outcome: ReviewOutcome::Passed,
            evidence_text: None,
            evidence: Some(
                params
                    .artifact
                    .clone()
                    .expect("the fixture names its artifact"),
            ),
        },
    )
    .await
    .unwrap();
    assert!(
        reviewed.credit_error.is_none(),
        "{:?}",
        reviewed.credit_error
    );
    assert_eq!(
        reviewed.credit.unwrap().state,
        reviewed::ReviewedCreditState::IndependentReviewRequired
    );
    let later_revision = Uuid::new_v4();
    let another_claim = CapturedCredit {
        task: crate::cognition::held_out::CardTask::NotATask,
        card_id: card.as_uuid(),
        claim: Some(ClaimReceipt {
            claim_id: Uuid::new_v4(),
            owner: airc.peer_id(),
            role: CreditRole::Owner,
        }),
    };
    stage_credit(
        &capture.conn,
        name,
        &another_claim,
        vec![served_receipt("another-claim-request", "served-model")],
        "private later prompt".into(),
        "private later completion".into(),
        None,
        later_revision,
        &[],
    )
    .await
    .unwrap();
    let inspected = WorkSubmission {
        registry,
        executor_slot: slot,
    }
    .run(
        &ctx,
        // The readback takes handles too, and its submission handle resolves against
        // THIS CARD's submissions — the card-scoped half of the same change. The
        // evidence opt-in rides the same decode, so one fixture proves both.
        serde_json::from_value(serde_json::json!({
            "room": params.room,
            "card_id": airc_core::shown_id::shown_form(card.as_uuid()),
            "submission_id": airc_core::shown_id::shown_form(
                params.submission_id.expect("the fixture names its submission"), // expect: set in the json above
            ),
            "include_staged_evidence": true,
            "evidence_rows": "all",
            "evidence_limit": 2,
        }))
        .expect("the readback's handles decode"), // expect: the fixture's own json, asserted decodable
    )
    .await
    .unwrap();
    assert_eq!(inspected.submission.publisher, persona);
    assert_eq!(inspected.reviews.len(), 2);
    assert_eq!(inspected.reviews[0].reviewer, persona);
    // Regression: Kimi's accepted review exposed only "unbound", with no
    // way to inspect the source-owned revision without reading private text.
    let page = inspected.staged_evidence.as_ref().unwrap();
    assert_eq!(page.summary.total_rows, 2);
    assert_eq!(page.summary.matching_claim, 1);
    assert_eq!(page.omitted_rows, 0);
    let evidence = &page.rows;
    assert_eq!(evidence.len(), 2);
    let original = evidence
        .iter()
        .find(|row| row.revision_id == selected.id)
        .unwrap();
    assert!(original.matches_submission_claim);
    assert!(original.predates_submission);
    assert_eq!(original.generation_count, selected.receipts.len());
    assert_eq!(original.generation_request_ids, vec!["public-command-request"]);
    let later = evidence
        .iter()
        .find(|row| row.revision_id == later_revision)
        .unwrap();
    assert!(
        !later.matches_submission_claim,
        "newest is not automatically the selected experience"
    );
    assert!(inspected.staged_evidence_error.is_none());
    let public_evidence = serde_json::to_string(page).unwrap();
    assert!(!public_evidence.contains(&selected.prompt));
    assert!(!public_evidence.contains(&selected.completion));
    assert!(!public_evidence.contains("private later"));
    assert_eq!(
        inspected.credit.unwrap().state,
        reviewed::ReviewedCreditState::AwaitingReview
    );
    let safe = serde_json::to_value(&inspected.submission).unwrap();
    assert!(!safe.to_string().contains(&selected.prompt));
    assert_eq!(
        airc.current_room().await.unwrap().channel,
        focus.channel,
        "all explicit work verbs preserve unrelated focus"
    );
}

/// Her holds as the board reports them, set by the test between acts; `unreadable` makes
/// the board refuse the read, as a daemon outage would.
struct Holds(std::sync::Mutex<Vec<airc_work::WorkCard>>, std::sync::atomic::AtomicBool);

impl Holds {
    fn new() -> Arc<Self> {
        Arc::new(Self(std::sync::Mutex::new(Vec::new()), std::sync::atomic::AtomicBool::new(false)))
    }
    fn set(&self, held: Vec<airc_work::WorkCard>) {
        *self.0.lock().expect("holds lock") = held;
    }
    fn unreadable(&self, refuse: bool) {
        self.1.store(refuse, std::sync::atomic::Ordering::SeqCst);
    }
}

#[async_trait::async_trait]
impl crate::persona::active_work_source::AircWorkReader for Holds {
    async fn active_claims(&self) -> Result<Vec<airc_work::WorkCard>, airc_lib::AircError> {
        if self.1.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(airc_lib::AircError::Transport("the board is unreachable in this fixture".into()));
        }
        Ok(self.0.lock().expect("holds lock").clone())
    }
}

fn opener_for(executor: Arc<CommandExecutor>, persona: Uuid, name: &'static str) -> OpenCapture {
    Box::new(move |credit: &CapturedCredit| {
        Some(TurnCreditCapture::with_executor(executor.clone(), persona, name.into(), "her own time".into(), credit.clone()))
    })
}

fn receipts(ids: &[&str]) -> Vec<crate::ai::provenance::GenerationReceipt> {
    ids.iter().map(|id| receipt(id)).collect()
}

/// Each staged row's generations, under the given claim, as a sorted set.
fn rows_under(rows: &[StagedCredit], claim: Option<airc_work::ClaimId>) -> Vec<Vec<String>> {
    rows.iter()
        .filter(|r| r.claim_id == claim.map(|c| c.as_uuid()))
        .map(|r| r.receipts.iter().map(|g| g.submitted_request_id.clone()).collect())
        .collect::<std::collections::BTreeSet<Vec<String>>>()
        .into_iter()
        .collect()
}

fn act(name: &str) -> (String, Vec<crate::ai::types::ToolCall>) {
    (
        format!("Do {name} for the card and check the result against the regression it fixes."),
        vec![crate::ai::types::ToolCall { id: format!("call-{name}"), name: name.into(), input: json!({}) }],
    )
}

// what this catches (card 12ddd78f, Kimi's c46cc5c6 on the 5090, 2026-10-27): a turn's credit
// read once at its start. Her initiative turn began holding nothing, then claimed, committed
// and submitted; nothing was staged, so the submit's issuing generation bound nowhere and its
// PASS review held forever. And a turn that began on one claim and took another staged every
// generation under the first. Here: credit opens at the claiming act, never covers what came
// before it, a new claim on the same card is a new segment, and the submission's issuing
// generation resolves to exactly the row of the claim it was made under.
#[tokio::test]
pub(super) async fn a_claim_taken_during_a_turn_credits_from_that_act_on() {
    let home = tempfile::tempdir().expect("tempdir");
    let _guard = HomeGuard::set(home.path()).await;
    crate::persona::register_substrate_orm_entities(crate::orm::OrmEntityRegistry::global())
        .expect("register staging entities");
    let executor = data_runtime();
    let persona = Uuid::new_v4();
    let owner = airc_core::PeerId::from_uuid(persona);
    let name = "claims-mid-turn";
    let card = claimed_card_fixture(Some(owner), Some(airc_work::ClaimId::new()));
    let mut reclaimed = card.clone();
    reclaimed.claim_id = Some(airc_work::ClaimId::new());
    let holds = Holds::new();
    let opener = |executor: Arc<CommandExecutor>| opener_for(executor, persona, name);
    use crate::cognition::act_observe::TurnCredit;

    // Turn 1 begins holding nothing: a read, then the claim, then the submit.
    let mut turn = ClaimFollowingCapture::new(None, Some(Arc::clone(&holds)), opener(executor.clone()));
    let mut acts = vec![act("code/read")];
    assert!(!turn.record(&acts, &receipts(&["before-claim"]), None, false).await, "nothing held: nothing staged");
    holds.set(vec![card.clone()]);
    acts.push(act("work/claim"));
    turn.record(&acts, &receipts(&["before-claim", "claimed"]), None, false).await;
    acts.push(act("work/submit"));
    turn.record(&acts, &receipts(&["before-claim", "claimed", "issuing"]), None, false).await;
    assert!(turn.record(&acts, &receipts(&["before-claim", "claimed", "issuing"]), Some("submitted"), true).await);
    assert_eq!(turn.into_credit().and_then(|c| c.claim).map(|c| c.claim_id), card.claim_id.map(|c| c.as_uuid()));

    let rows = stored_credit_rows(&conn_as(executor.clone(), persona), name).await;
    assert_eq!(rows.len(), 1, "one row, under the claim taken during the turn");
    let row = &rows[0];
    assert_eq!(row.claim_id, card.claim_id.map(|c| c.as_uuid()));
    assert_eq!(
        row.receipts.iter().map(|r| r.submitted_request_id.as_str()).collect::<Vec<_>>(),
        ["claimed", "issuing"],
        "the generation before the claim is never credited to it"
    );

    // Turn 2 begins on that claim; she reopens and reclaims the same card mid-turn.
    let first = CapturedCredit::from_selected_card(&card);
    let mut turn = ClaimFollowingCapture::new(Some(&first), Some(Arc::clone(&holds)), opener(executor.clone()));
    let mut acts = vec![act("code/read")];
    turn.record(&acts, &receipts(&["old-claim"]), None, false).await;
    holds.set(vec![reclaimed.clone()]);
    acts.push(act("work/claim"));
    turn.record(&acts, &receipts(&["old-claim", "reclaimed"]), None, false).await;
    acts.push(act("work/submit"));
    assert!(turn.record(&acts, &receipts(&["old-claim", "reclaimed", "reissuing"]), None, true).await);

    let rows = stored_credit_rows(&conn_as(executor.clone(), persona), name).await;
    let under = |claim| rows_under(&rows, claim);
    assert_eq!(under(card.claim_id), [vec!["claimed".to_string(), "issuing".into()], vec!["old-claim".into()]], "turn 1's row and turn 2's pre-reclaim act, each under the first claim");
    assert_eq!(under(reclaimed.claim_id), [vec!["reclaimed".to_string(), "reissuing".into()]]);

    // The reclaimed submission's issuing generation resolves to exactly its claim's row.
    let room = Uuid::new_v4();
    let submitted = airc_work::WorkSubmission {
        submission_id: airc_work::SubmissionId::from_uuid(Uuid::new_v4()),
        card_id: card.card_id,
        claim_id: reclaimed.claim_id.expect("reclaimed fixture has a claim"),
        instance: "ordinary-project-work".into(),
        base_sha: airc_work::GitObjectId::new("a".repeat(40)).expect("sha fixture"),
        artifact: serde_json::from_value(json!({"hash":"b".repeat(64),"size_bytes":12})).expect("artifact fixture"),
        publisher: owner,
        submitted_at_ms: 3,
    };
    let data = conn_as(executor, persona);
    let pending = reviewed::PendingSelection {
        publication: reviewed::Publication::of(room, &submitted),
        issuing_generation: "reissuing".into(),
    };
    if let Err(e) = reviewed::record_pending(&data, name, persona, pending).await {
        panic!("the pending binding records: {e}");
    }
    let bound = match reviewed::resolve_pending(&data, name, persona, room, &submitted).await {
        Ok(Some(Ok(binding))) => binding,
        other => panic!("the issuing generation binds its claim's row, got {other:?}"),
    };
    let reclaimed_row = rows.iter().find(|r| r.claim_id == reclaimed.claim_id.map(|c| c.as_uuid())).expect("reclaimed row");
    assert_eq!(bound.selection.staged_revision_id, reclaimed_row.id);
}

// what this catches (Codex on #5523): her holds unreadable right after the act that claimed.
// Staging on under the claim she had would credit the new claim's work to the old one for
// good. Unread, nothing is staged; the read that answers puts the boundary at the claiming
// act; still unread at the end, only what came before it is staged and the rest is no one's.
#[tokio::test]
pub(super) async fn an_unread_claim_stages_nothing_until_a_read_says_whose_it_was() {
    use crate::cognition::act_observe::TurnCredit;
    let home = tempfile::tempdir().expect("tempdir");
    let _guard = HomeGuard::set(home.path()).await;
    crate::persona::register_substrate_orm_entities(crate::orm::OrmEntityRegistry::global())
        .expect("register staging entities");
    let executor = data_runtime();
    let persona = Uuid::new_v4();
    let owner = airc_core::PeerId::from_uuid(persona);
    let name = "unread-claim";
    let card = claimed_card_fixture(Some(owner), Some(airc_work::ClaimId::new()));
    let mut reclaimed = card.clone();
    reclaimed.claim_id = Some(airc_work::ClaimId::new());
    let first = CapturedCredit::from_selected_card(&card);
    let holds = Holds::new();
    holds.set(vec![card.clone()]);

    // The claim's read fails, the next act's read fails too, the third answers.
    let mut turn = ClaimFollowingCapture::new(Some(&first), Some(Arc::clone(&holds)), opener_for(executor.clone(), persona, name));
    let mut acts = vec![act("code/read")];
    turn.record(&acts, &receipts(&["before"]), None, false).await;
    holds.set(vec![reclaimed.clone()]);
    holds.unreadable(true);
    acts.push(act("work/claim"));
    assert!(!turn.record(&acts, &receipts(&["before", "claiming"]), None, false).await);
    acts.push(act("code/read"));
    assert!(!turn.record(&acts, &receipts(&["before", "claiming", "unread"]), None, false).await, "nothing staged while unread");
    holds.unreadable(false);
    acts.push(act("code/edit"));
    turn.record(&acts, &receipts(&["before", "claiming", "unread", "answered"]), None, false).await;
    assert!(turn.record(&acts, &receipts(&["before", "claiming", "unread", "answered"]), None, true).await);
    let rows = stored_credit_rows(&conn_as(executor.clone(), persona), name).await;
    assert_eq!(rows_under(&rows, card.claim_id), [vec!["before".to_string()]], "the old claim keeps only what came before the claiming act");
    assert_eq!(
        rows_under(&rows, reclaimed.claim_id),
        [vec!["claiming".to_string(), "unread".into(), "answered".into()]],
        "the new claim from the claiming act on, once a read said so"
    );

    // Unread after one claim, then the next act is itself a claim whose read answers: the
    // newest claim begins at its own act, and the unread claim's generation is no one's.
    let mut newest = reclaimed.clone();
    newest.claim_id = Some(airc_work::ClaimId::new());
    let mut skipped = reclaimed.clone();
    skipped.claim_id = Some(airc_work::ClaimId::new());
    holds.set(vec![reclaimed.clone()]);
    let on = CapturedCredit::from_selected_card(&reclaimed);
    let mut turn = ClaimFollowingCapture::new(Some(&on), Some(Arc::clone(&holds)), opener_for(executor.clone(), persona, name));
    let mut acts = vec![act("code/read")];
    turn.record(&acts, &receipts(&["third-before"]), None, false).await;
    holds.set(vec![skipped.clone()]);
    holds.unreadable(true);
    acts.push(act("work/claim"));
    turn.record(&acts, &receipts(&["third-before", "unread-claim"]), None, false).await;
    holds.set(vec![newest.clone()]);
    holds.unreadable(false);
    acts.push(act("work/claim"));
    turn.record(&acts, &receipts(&["third-before", "unread-claim", "answered-claim"]), None, false).await;
    assert!(turn.record(&acts, &receipts(&["third-before", "unread-claim", "answered-claim"]), None, true).await);
    let rows = stored_credit_rows(&conn_as(executor.clone(), persona), name).await;
    assert!(rows_under(&rows, reclaimed.claim_id).contains(&vec!["third-before".to_string()]));
    assert_eq!(rows_under(&rows, newest.claim_id), [vec!["answered-claim".to_string()]], "the answered claim begins at its own act");
    assert!(rows.iter().all(|r| r.receipts.iter().all(|g| g.submitted_request_id != "unread-claim")), "the unread claim's generation is no one's");

    // Claim B unread, then release B and the read answers A again: unchanged across two board
    // acts proves nothing, so A stops at the unread claim and resumes after the answering act.
    let mut b = reclaimed.clone();
    b.claim_id = Some(airc_work::ClaimId::new());
    holds.set(vec![reclaimed.clone()]);
    let a = CapturedCredit::from_selected_card(&reclaimed);
    let mut turn = ClaimFollowingCapture::new(Some(&a), Some(Arc::clone(&holds)), opener_for(executor.clone(), persona, name));
    let mut acts = vec![act("code/read")];
    turn.record(&acts, &receipts(&["a-before"]), None, false).await;
    holds.set(vec![b.clone()]);
    holds.unreadable(true);
    acts.push(act("work/claim"));
    turn.record(&acts, &receipts(&["a-before", "b-claim"]), None, false).await;
    holds.set(vec![reclaimed.clone()]);
    holds.unreadable(false);
    acts.push(act("work/release"));
    turn.record(&acts, &receipts(&["a-before", "b-claim", "b-release"]), None, false).await;
    acts.push(act("code/read"));
    turn.record(&acts, &receipts(&["a-before", "b-claim", "b-release", "a-after"]), None, false).await;
    assert!(turn.record(&acts, &receipts(&["a-before", "b-claim", "b-release", "a-after"]), None, true).await);
    let rows = stored_credit_rows(&conn_as(executor.clone(), persona), name).await;
    let under_a = rows_under(&rows, reclaimed.claim_id);
    assert!(under_a.contains(&vec!["a-before".to_string()]) && under_a.contains(&vec!["a-after".to_string()]));
    assert!(
        rows.iter().all(|r| r.receipts.iter().all(|g| g.submitted_request_id != "b-claim" && g.submitted_request_id != "b-release")),
        "the uncertain stretch is credited to no claim, not restored to A"
    );

    // Still unread when the turn ends: what came before is staged, the rest credited to no one.
    let mut again = reclaimed.clone();
    again.claim_id = Some(airc_work::ClaimId::new());
    holds.set(vec![reclaimed.clone()]);
    let start = CapturedCredit::from_selected_card(&reclaimed);
    let mut turn = ClaimFollowingCapture::new(Some(&start), Some(Arc::clone(&holds)), opener_for(executor.clone(), persona, name));
    let mut acts = vec![act("code/read")];
    turn.record(&acts, &receipts(&["second-before"]), None, false).await;
    holds.set(vec![again.clone()]);
    holds.unreadable(true);
    acts.push(act("work/claim"));
    turn.record(&acts, &receipts(&["second-before", "never-read"]), None, false).await;
    assert!(turn.record(&acts, &receipts(&["second-before", "never-read"]), None, true).await);
    let rows = stored_credit_rows(&conn_as(executor, persona), name).await;
    assert!(rows_under(&rows, reclaimed.claim_id).contains(&vec!["second-before".to_string()]));
    assert!(rows_under(&rows, again.claim_id).is_empty(), "an unread claim is never guessed");
    assert!(
        rows.iter().all(|r| r.receipts.iter().all(|g| g.submitted_request_id != "never-read")),
        "the generation after the unread claim is credited to no claim"
    );
}
