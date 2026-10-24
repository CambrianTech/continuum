#![cfg(test)]
use super::*;
use crate::runtime::LateBound;

// what this catches (card fcd8d1c1, the gate #5386 left): a resident's ordinary submit names no
// staged revision and no generation; her act's caller carries the generation that issued it.
// Through the REAL work/submit and work/review commands: the submit records that generation
// before publishing and binds nothing yet; a review that arrives before her turn is staged
// holds (ProvenanceHeld, never Unbound or passed); once the turn stages, the next review binds
// exactly the issuing turn's row; the binding survives a reopened store; and a same-id retry
// from a later act keeps the first generation. Training acceptance itself is the mocked
// destination tests' (reviewed_credit.rs); this proves the binding a review reaches.
#[tokio::test]
pub(super) async fn an_ordinary_submit_binds_the_turn_that_issued_it_through_review(
) -> Result<(), Box<dyn std::error::Error>> {
    use crate::modules::work::submission::*;
    use crate::sdk_codegen::{ActionCommand, Ctx};
    use reviewed::ReviewedCreditState as State;
    let home = tempfile::tempdir()?;
    let _home = HomeGuard::set(home.path()).await;
    crate::persona::register_substrate_orm_entities(crate::orm::OrmEntityRegistry::global())?;
    let airc_home = home.path().join("isolated-airc");
    let airc = Arc::new(airc_lib::Airc::open_with_wire_root_for_test(&airc_home, &airc_home).await?);
    let room = airc.join("ordinary-project").await?;
    let persona = airc.peer_id().as_uuid();
    let name = "ordinary-resident";
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
    let slot = Arc::new(LateBound::new("ordinary submit fixture"));
    slot.install(executor.clone());
    let card = airc
        .create_work_card(airc_lib::CreateWorkCard::new(
            airc_work::RepoId::new("acme/ordinary-project")?,
            "ordinary work, submitted the ordinary way",
            airc_work::Priority::P1,
        ))
        .await?;
    let claim = airc.claim_work_card(airc_lib::ClaimWorkCard { card_id: card, ttl_ms: 60_000 }).await?;
    // Her act: the generation that emits the submit call rides the caller, bound by the act seam.
    let her_act = |generation: &str| Ctx {
        caller: Some(CallerIdentity::local_persona(airc.peer_id()).issued_by(generation.into())),
        ..Default::default()
    };
    let patch = "diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1 +1 @@\n-a\n+b\n";
    let patch_hash = {
        use airc_blobs::ContentAddressedStore;
        crate::media::artifact::store()?.put(patch.as_bytes())?.to_hex()
    };
    // No submission_id, staged_revision_id or generation_request_id: what a resident sends.
    let params: WorkSubmitParams = serde_json::from_value(json!({
        "room": room.channel.as_uuid().to_string(),
        "card_id": card.as_uuid().to_string(),
        "claim_id": claim.as_uuid().to_string(),
        "instance": "ordinary-project-work",
        "base_sha": "a".repeat(40),
        "artifact": { "hash": patch_hash, "size_bytes": patch.len(), "mime": "text/x-diff" },
    }))?;
    let submit = WorkSubmit { registry: registry.clone(), executor_slot: slot.clone() };
    let published = submit.run(&her_act("issuing-gen"), params.clone()).await?;
    assert_eq!(published.bound_staged_revision_id, None, "nothing is staged yet, so nothing binds at submit");
    let data = conn_as(executor.clone(), persona);
    let recorded: reviewed::PendingCreditBinding = reviewed::read_one(&data, name, &published.submission_id.to_string())
        .await?
        .ok_or("work/submit recorded the issuing generation before publishing")?;
    assert_eq!(recorded.selection.issuing_generation, "issuing-gen");

    let review_card = airc_work::submission_review_card_id(card, airc_work::SubmissionId::from_uuid(published.submission_id));
    let review_claim = airc.claim_work_card(airc_lib::ClaimWorkCard { card_id: review_card, ttl_ms: 60_000 }).await?;
    let review = |outcome: ReviewOutcome| WorkReviewParams {
        room: params.room.clone(),
        review_id: Some(Uuid::new_v4()),
        card_id: Some(card.as_uuid().to_string()),
        submission_id: Some(published.submission_id),
        artifact: params.artifact.clone(),
        review_card_id: review_card.as_uuid().to_string(),
        review_claim_id: Some(review_claim.as_uuid()),
        outcome,
        evidence_text: None,
        evidence: params.artifact.clone(),
    };
    let reviewer = WorkReview { registry: registry.clone(), executor_slot: slot.clone() };
    let plain = Ctx { caller: Some(CallerIdentity::local_persona(airc.peer_id())), ..Default::default() };

    // A review before her turn is staged: held, never Unbound and never passed.
    let early = reviewer.run(&plain, review(ReviewOutcome::Failed)).await?;
    assert_eq!(early.credit.map(|c| c.state), Some(State::ProvenanceHeld), "{:?}", early.credit_error);

    // Her turn stages, carrying the issuing generation; the next review binds exactly that row.
    let mut turn = TurnCreditCapture::with_executor(
        executor.clone(),
        persona,
        name.into(),
        "Make the change the card asks for, then submit it.".into(),
        CapturedCredit {
            task: crate::cognition::held_out::CardTask::NotATask,
            card_id: card.as_uuid(),
            claim: Some(ClaimReceipt { claim_id: claim.as_uuid(), owner: airc.peer_id(), role: CreditRole::Owner }),
        },
    );
    assert!(turn.record(&settlement_acts(4), &[served_receipt("issuing-gen", "served-model")], None, true).await);
    let issuing_row = stored_credit_rows(&data, name)
        .await
        .into_iter()
        .find(|row| row.receipts.iter().any(|r| r.submitted_request_id == "issuing-gen"))
        .ok_or("her turn staged the issuing generation")?;
    let passed = reviewer.run(&plain, review(ReviewOutcome::Passed)).await?;
    assert!(passed.credit_error.is_none(), "{:?}", passed.credit_error);
    let credit = passed.credit.ok_or("the review reached her credit")?;
    // Her own review: the room declares no self-review policy, so an independent one is
    // required; the binding to the issuing row is made either way.
    assert_eq!(credit.state, State::IndependentReviewRequired);
    assert_eq!(credit.staged_revision_id, Some(issuing_row.id), "bound to the turn that issued the submission");

    // Retained across a reopened store.
    let reopened = conn_as(data_runtime(), persona);
    let binding: reviewed::WorkCreditBinding = reviewed::read_one(&reopened, name, &published.submission_id.to_string())
        .await?
        .ok_or("the binding is durable")?;
    assert_eq!(binding.selection.staged_revision_id, issuing_row.id);

    // A same-id retry from a later act is the same publication: it keeps the first generation.
    let mut retry = params.clone();
    retry.submission_id = Some(published.submission_id);
    let again = submit.run(&her_act("later-gen"), retry).await?;
    assert_eq!(again.submission_id, published.submission_id);
    let kept: reviewed::PendingCreditBinding = reviewed::read_one(&reopened, name, &published.submission_id.to_string())
        .await?
        .ok_or("the pending record is retained")?;
    assert_eq!(kept.selection.issuing_generation, "issuing-gen", "a retry never reattributes the work");
    Ok(())
}
