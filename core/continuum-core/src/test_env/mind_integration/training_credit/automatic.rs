#![cfg(test)]
//! The gate #5386 left (card fcd8d1c1): a resident's ordinary submit, which names no staged
//! revision and no generation, reaches her credit through the REAL work/submit and work/review
//! commands. Her act's caller carries the generation that issued the submit.
use super::*;
use crate::modules::work::submission::*;
use crate::runtime::LateBound;
use crate::sdk_codegen::{ActionCommand, Ctx};
use reviewed::ReviewedCreditState as State;

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// A resident on a real local airc board with a claimed card and a retained patch.
struct Resident {
    _home: (HomeGuard, tempfile::TempDir),
    wire: std::path::PathBuf,
    airc: Arc<airc_lib::Airc>,
    room: airc_lib::Room,
    persona: Uuid,
    name: &'static str,
    registry: crate::persona::PersonaAircRuntimeRegistry,
    executor: Arc<CommandExecutor>,
    slot: Arc<LateBound<CommandExecutor>>,
    card: airc_work::WorkCardId,
    claim: airc_work::ClaimId,
    params: WorkSubmitParams,
}

impl Resident {
    async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let home = tempfile::tempdir()?;
        let guard = HomeGuard::set(home.path()).await;
        crate::persona::register_substrate_orm_entities(crate::orm::OrmEntityRegistry::global())?;
        let wire = home.path().join("isolated-airc");
        let airc = Arc::new(airc_lib::Airc::open_with_wire_root_for_test(&wire, &wire).await?);
        let room = airc.join("ordinary-project").await?;
        let persona = airc.peer_id().as_uuid();
        let name = "ordinary-resident";
        let registry = crate::persona::PersonaAircRuntimeRegistry::new();
        registry.register(crate::persona::PersonaAircRuntime::from_attached(
            persona,
            name,
            wire.clone(),
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
        Ok(Self { _home: (guard, home), wire, airc, room, persona, name, registry, executor, slot, card, claim, params })
    }

    /// Her act: the generation that emits the call rides the caller, bound by the act seam.
    fn act(&self, generation: &str) -> Ctx {
        Ctx { caller: Some(CallerIdentity::local_persona(self.airc.peer_id()).issued_by(generation.into())), ..Default::default() }
    }

    fn submit(&self) -> WorkSubmit {
        WorkSubmit { registry: self.registry.clone(), executor_slot: self.slot.clone() }
    }

    fn data(&self) -> Connection<InProcessTransport> {
        conn_as(self.executor.clone(), self.persona)
    }

    /// The publication work/submit builds from these params under `submission_id`.
    fn publication(&self, submission_id: Uuid) -> Result<reviewed::Publication, Box<dyn std::error::Error>> {
        let artifact = self.params.artifact.clone().ok_or("the fixture types its artifact")?;
        Ok(reviewed::Publication {
            submission_id,
            room_id: self.room.channel.as_uuid(),
            card_id: self.card.as_uuid(),
            claim_id: self.claim.as_uuid(),
            instance: "ordinary-project-work".into(),
            base_sha: airc_work::GitObjectId::new("a".repeat(40))?,
            artifact: serde_json::from_value(serde_json::to_value(artifact)?)?,
        })
    }

    /// Her turn, staged, carrying `generation`; returns its row.
    async fn stage(&self, generation: &str) -> Result<StagedCredit, Box<dyn std::error::Error>> {
        let mut turn = TurnCreditCapture::with_executor(
            self.executor.clone(),
            self.persona,
            self.name.into(),
            "Make the change the card asks for, then submit it.".into(),
            CapturedCredit {
                task: crate::cognition::held_out::CardTask::NotATask,
                card_id: self.card.as_uuid(),
                claim: Some(ClaimReceipt { claim_id: self.claim.as_uuid(), owner: self.airc.peer_id(), role: CreditRole::Owner }),
            },
        );
        if !turn.record(&settlement_acts(4), &[served_receipt(generation, "served-model")], None, true).await {
            return Err("her turn staged".into());
        }
        Ok(stored_credit_rows(&self.data(), self.name)
            .await
            .into_iter()
            .find(|row| row.receipts.iter().any(|r| r.submitted_request_id == generation))
            .ok_or("the staged row carries the generation")?)
    }

    /// A review of `submission` on its review card, held by `holder`.
    fn review(&self, submission: Uuid, review_card: airc_work::WorkCardId, claim: airc_work::ClaimId, outcome: ReviewOutcome) -> WorkReviewParams {
        WorkReviewParams {
            room: self.params.room.clone(),
            review_id: Some(Uuid::new_v4()),
            card_id: Some(self.card.as_uuid().to_string()),
            submission_id: Some(submission),
            artifact: self.params.artifact.clone(),
            review_card_id: review_card.as_uuid().to_string(),
            review_claim_id: Some(claim.as_uuid()),
            outcome,
            evidence_text: None,
            evidence: self.params.artifact.clone(),
        }
    }

    fn reviewer(&self) -> WorkReview {
        WorkReview { registry: self.registry.clone(), executor_slot: self.slot.clone() }
    }

    fn review_card(&self, submission: Uuid) -> airc_work::WorkCardId {
        airc_work::submission_review_card_id(self.card, airc_work::SubmissionId::from_uuid(submission))
    }
}

// what this catches: the submit records the issuing generation before publishing and binds
// nothing yet; a review that arrives before her turn is staged holds (ProvenanceHeld, never
// Unbound or passed); once the turn stages, the next review binds exactly the issuing turn's
// row; the binding reads back through a new runtime (not a process restart); a same-id retry from a later act keeps
// the first generation. (Her own review: IndependentReviewRequired; acceptance is the
// second-identity test below.)
#[tokio::test]
pub(super) async fn an_ordinary_submit_binds_the_turn_that_issued_it_through_review() -> TestResult {
    let r = Resident::new().await?;
    let published = r.submit().run(&r.act("issuing-gen"), r.params.clone()).await?;
    assert_eq!(published.bound_staged_revision_id, None, "nothing is staged yet, so nothing binds at submit");
    let recorded: reviewed::PendingCreditBinding = reviewed::read_one(&r.data(), r.name, &published.submission_id.to_string())
        .await?
        .ok_or("work/submit recorded the issuing generation before publishing")?;
    assert_eq!(recorded.selection.issuing_generation, "issuing-gen");

    let review_card = r.review_card(published.submission_id);
    let held = r.airc.claim_work_card(airc_lib::ClaimWorkCard { card_id: review_card, ttl_ms: 60_000 }).await?;
    let plain = Ctx { caller: Some(CallerIdentity::local_persona(r.airc.peer_id())), ..Default::default() };
    let early = r.reviewer().run(&plain, r.review(published.submission_id, review_card, held, ReviewOutcome::Failed)).await?;
    assert_eq!(early.credit.map(|c| c.state), Some(State::ProvenanceHeld), "{:?}", early.credit_error);

    let issuing_row = r.stage("issuing-gen").await?;
    let passed = r.reviewer().run(&plain, r.review(published.submission_id, review_card, held, ReviewOutcome::Passed)).await?;
    assert!(passed.credit_error.is_none(), "{:?}", passed.credit_error);
    let credit = passed.credit.ok_or("the review reached her credit")?;
    assert_eq!(credit.state, State::IndependentReviewRequired);
    assert_eq!(credit.staged_revision_id, Some(issuing_row.id), "bound to the turn that issued the submission");

    let reopened = conn_as(data_runtime(), r.persona);
    let binding: reviewed::WorkCreditBinding = reviewed::read_one(&reopened, r.name, &published.submission_id.to_string())
        .await?
        .ok_or("the binding is durable")?;
    assert_eq!(binding.selection.staged_revision_id, issuing_row.id);

    let mut retry = r.params.clone();
    retry.submission_id = Some(published.submission_id);
    let again = r.submit().run(&r.act("later-gen"), retry).await?;
    assert_eq!(again.submission_id, published.submission_id);
    let kept: reviewed::PendingCreditBinding = reviewed::read_one(&reopened, r.name, &published.submission_id.to_string())
        .await?
        .ok_or("the pending record is retained")?;
    assert_eq!(kept.selection.issuing_generation, "issuing-gen", "a retry never reattributes the work");
    Ok(())
}

// what this catches: an independent reviewer, a second identity on the same board, files a
// signed pass through the real work/review; the resident's credit binds the issuing turn and
// its decision is recorded; the same exact consumption, with the training destination mocked
// (MockTransport, not live training), accepts that row.
#[tokio::test]
pub(super) async fn a_second_identitys_signed_pass_accepts_the_issuing_turn() -> TestResult {
    let r = Resident::new().await?;
    let published = r.submit().run(&r.act("issuing-gen"), r.params.clone()).await?;
    let issuing_row = r.stage("issuing-gen").await?;

    let reviewer_home = r.wire.parent().ok_or("the fixture's home")?.join("reviewer-airc");
    let reviewer = Arc::new(airc_lib::Airc::open_with_wire_root_for_test(&reviewer_home, &r.wire).await?);
    let reviewer_room = reviewer.join("ordinary-project").await?;
    assert_eq!(reviewer_room.channel, r.room.channel, "the same board");
    r.registry.register(crate::persona::PersonaAircRuntime::from_attached(
        reviewer.peer_id().as_uuid(),
        "independent-reviewer",
        reviewer_home,
        reviewer.clone(),
        reviewer_room.channel,
        crate::persona::identity_provider::PersonaIdentitySource::FreshlyMinted,
    ));
    let review_card = r.review_card(published.submission_id);
    let held = reviewer.claim_work_card(airc_lib::ClaimWorkCard { card_id: review_card, ttl_ms: 60_000 }).await?;
    let as_reviewer = Ctx { caller: Some(CallerIdentity::local_persona(reviewer.peer_id())), ..Default::default() };
    let pass = r.review(published.submission_id, review_card, held, ReviewOutcome::Passed);
    let filed = r.reviewer().run(&as_reviewer, pass).await?;
    // This executor serves no training destination, so the dispatch itself cannot complete
    // here; the binding and the independent decision are recorded before it.
    let status = reviewed::credit_status(&r.data(), r.name, published.submission_id).await?;
    assert_eq!(status.staged_revision_id, Some(issuing_row.id), "{:?}", filed.credit_error);
    assert_eq!(status.state, State::AwaitingAcceptance, "an independent pass selects the decision");

    let board = r.airc.work_board_in(&r.room).await?;
    let review_id = board
        .submission_reviews_for_card(r.card)
        .find(|review| review.submission_id.as_uuid() == published.submission_id)
        .map(|review| review.review_id)
        .ok_or("the signed review is on the board")?;
    let destination = continuum_client::mock::MockTransport::new();
    script_settlement_reply(
        &destination,
        &issuing_row,
        Some(json!({"success": true, "acceptance": {"submissionId": issuing_row.id, "replayed": false}})),
    );
    let accepted = reviewed::consume_review_with_boundary(
        &settlement_connection(r.executor.clone(), r.persona, destination, None),
        r.name,
        r.persona,
        r.room.channel.as_uuid(),
        &board,
        review_id,
        &reviewed::SelfReview::NotAdmitted,
        None,
    )
    .await?;
    assert_eq!(accepted.state, State::Accepted, "{accepted:?}");
    assert_eq!(accepted.staged_revision_id, Some(issuing_row.id));
    Ok(())
}

// what this catches: a retry from a persisted, unpublished record. The record is seeded the
// way work/submit's first half writes it, and nothing publishes (this does NOT inject a
// cancellation or a lost response into work/submit's own publication). Her retry under the
// SAME id from a later act is the same publication: it publishes now, keeps the first
// generation, and a review binds the turn that first issued it.
#[tokio::test]
pub(super) async fn a_retry_from_a_persisted_unpublished_record_keeps_the_first_generation() -> TestResult {
    let r = Resident::new().await?;
    let submission = Uuid::new_v4();
    // The first half of work/submit, then nothing: the publication never happened.
    reviewed::record_pending(
        &r.data(),
        r.name,
        r.persona,
        reviewed::PendingSelection { publication: r.publication(submission)?, issuing_generation: "first-gen".into() },
    )
    .await?;
    let board = r.airc.work_board_in(&r.room).await?;
    assert!(
        board.card(r.card).is_some_and(|c| c.submissions.iter().all(|s| s.submission_id.as_uuid() != submission)),
        "nothing published yet"
    );
    let first_row = r.stage("first-gen").await?;

    let mut retry = r.params.clone();
    retry.submission_id = Some(submission);
    let published = r.submit().run(&r.act("later-gen"), retry).await?;
    assert_eq!(published.submission_id, submission, "the retry publishes the same submission");
    let kept: reviewed::PendingCreditBinding = reviewed::read_one(&r.data(), r.name, &submission.to_string())
        .await?
        .ok_or("the record is retained")?;
    assert_eq!(kept.selection.issuing_generation, "first-gen", "the act that made it, not the retry");

    let review_card = r.review_card(submission);
    let held = r.airc.claim_work_card(airc_lib::ClaimWorkCard { card_id: review_card, ttl_ms: 60_000 }).await?;
    let plain = Ctx { caller: Some(CallerIdentity::local_persona(r.airc.peer_id())), ..Default::default() };
    let reviewed = r.reviewer().run(&plain, r.review(submission, review_card, held, ReviewOutcome::Passed)).await?;
    let credit = reviewed.credit.ok_or("the review reached her credit")?;
    assert_eq!(credit.staged_revision_id, Some(first_row.id), "{:?}", reviewed.credit_error);
    Ok(())
}

// what this catches: a pending record that refuses (another publication already holds this id)
// refuses the submit before publication; nothing reaches the board.
#[tokio::test]
pub(super) async fn a_refused_record_publishes_nothing() -> TestResult {
    let r = Resident::new().await?;
    let submission = Uuid::new_v4();
    let mut other = r.publication(submission)?;
    other.instance = "another-instance".into();
    reviewed::record_pending(
        &r.data(),
        r.name,
        r.persona,
        reviewed::PendingSelection { publication: other, issuing_generation: "other-gen".into() },
    )
    .await?;
    let mut attempt = r.params.clone();
    attempt.submission_id = Some(submission);
    assert!(r.submit().run(&r.act("this-gen"), attempt).await.is_err(), "a refused record refuses the submit");
    let board = r.airc.work_board_in(&r.room).await?;
    assert!(
        board.card(r.card).is_some_and(|c| c.submissions.iter().all(|s| s.submission_id.as_uuid() != submission)),
        "nothing published"
    );
    Ok(())
}
