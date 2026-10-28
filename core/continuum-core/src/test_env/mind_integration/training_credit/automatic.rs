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
        Ok(Self { _home: (guard, home), airc, room, persona, name, registry, executor, slot, card, claim, params })
    }

    /// Her act: the generation that emits the call rides the caller, bound by the act seam.
    fn act(&self, generation: &str) -> Ctx {
        Ctx { caller: Some(CallerIdentity::local_persona(self.airc.peer_id()).issued_by(generation.into())), ..Default::default() }
    }

    fn submit(&self) -> WorkSubmit {
        WorkSubmit { registry: self.registry.clone(), executor_slot: self.slot.clone(), benchmark: std::sync::Arc::new(crate::modules::work_seams::CoreBenchmarkSubmission) }
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
        if !turn.record(&settlement_acts(4), &[captured(self.persona, served_receipt(generation, "served-model"))], None, true).await {
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

// what this catches: an independent reviewer's signed pass reaches acceptance. The submission is
// the one the real work/submit published; the board holding it and a Passed review signed by a
// separate reviewer peer is projected through airc's codec (two test identities share no
// transcript without a daemon, so the real work/review run by a second identity stays on
// fcd8d1c1). Her credit binds the issuing turn, and the exact consumption, with the training
// destination mocked (MockTransport, not live training), accepts that row.
#[tokio::test]
pub(super) async fn an_independent_signed_pass_accepts_the_issuing_turn() -> TestResult {
    use airc_work::{CardCreated, WorkCardClaimed, WorkEvent, WorkReviewOutcome, WorkSubmissionReview};
    let r = Resident::new().await?;
    let published = r.submit().run(&r.act("issuing-gen"), r.params.clone()).await?;
    let issuing_row = r.stage("issuing-gen").await?;
    let submitted = r
        .airc
        .work_board_in(&r.room)
        .await?
        .card(r.card)
        .and_then(|c| c.submissions.iter().find(|s| s.submission_id.as_uuid() == published.submission_id).cloned())
        .ok_or("work/submit published the submission")?;

    let (owner, reviewer) = (r.airc.peer_id(), airc_core::PeerId::new());
    // Ordered around the real submission's wall-clock time: the card and claim before it,
    // within the claim's lease, and the review after it.
    let at = submitted.submitted_at_ms;
    let review_card = r.review_card(published.submission_id);
    let review_claim = airc_work::ClaimId::new();
    let repo = airc_work::RepoId::new("acme/ordinary-project")?;
    let created = |card_id, reviews, by| WorkEvent::CardCreated(CardCreated {
        card_id,
        repo: repo.clone(),
        title: "ordinary work, submitted the ordinary way".into(),
        body: None,
        priority: airc_work::Priority::P1,
        lane_id: None,
        created_by: by,
        created_at_ms: at.saturating_sub(2),
        reviews,
        origin: None,
    });
    let claimed = |card_id, claim_id, holder| WorkEvent::CardClaimed(WorkCardClaimed {
        selected_at_ms: None,
        origin: airc_work::ClaimOrigin::Unknown,
        card_id,
        claim_id,
        owner: holder,
        ttl_ms: 600_000,
        claimed_at_ms: at.saturating_sub(1),
    });
    let pass = WorkSubmissionReview {
        review_id: airc_work::WorkReviewId::new(),
        card_id: r.card,
        submission_id: submitted.submission_id,
        artifact: submitted.artifact.clone(),
        review_card_id: review_card,
        review_claim_id: review_claim,
        reviewer,
        outcome: WorkReviewOutcome::Passed,
        evidence: submitted.artifact.clone(),
        evidence_text: None,
        reviewed_at_ms: at + 1,
    };
    let board = project_events(
        r.room.channel.as_uuid(),
        vec![
            (created(r.card, None, owner), owner),
            (claimed(r.card, submitted.claim_id, owner), owner),
            (WorkEvent::WorkSubmitted(submitted), owner),
            (created(review_card, Some(r.card), reviewer), reviewer),
            (claimed(review_card, review_claim, reviewer), reviewer),
            (WorkEvent::WorkSubmissionReviewed(pass.clone()), reviewer),
        ],
    );
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
        pass.review_id,
        &reviewed::SelfReview::NotAdmitted,
        None,
    )
    .await?;
    assert_eq!(accepted.state, State::Accepted, "{accepted:?}");
    assert_eq!(accepted.staged_revision_id, Some(issuing_row.id), "the turn that issued the submission");
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

// what this catches (card 6593a869): the artifact is derived from her real checkout, not typed.
// A git checkout at the card's worktree with airc's creation-base ref and her change committed
// on top; the submit omits instance, base and artifact. work/submit derives the base (the
// creation commit), retains the patch, and publishes exactly that patch's SHA-256; the pending
// record names the issuing generation as before.
#[tokio::test]
pub(super) async fn a_checkout_derived_submit_publishes_her_patch_and_records_its_issuer() -> TestResult {
    let r = Resident::new().await?;
    let checkout = airc_lib::work_worktree::worktree_path_for(r.card).ok_or("the card has a worktree path")?;
    std::fs::create_dir_all(&checkout)?;
    let git = |args: &[&str]| -> Result<String, Box<dyn std::error::Error>> {
        let out = std::process::Command::new("git").args(args).current_dir(&checkout).output()?;
        if !out.status.success() {
            return Err(format!("git {args:?}: {}", String::from_utf8_lossy(&out.stderr)).into());
        }
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    };
    let commit = ["-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid", "-c", "commit.gpgsign=false", "commit", "-q", "-m"];
    git(&["init", "-q", "-b", "work"])?;
    std::fs::write(checkout.join("subject.txt"), "a\n")?;
    git(&["add", "subject.txt"])?;
    git(&[&commit[..], &["base"]].concat())?;
    let base = git(&["rev-parse", "HEAD"])?;
    git(&["update-ref", airc_lib::work_worktree::CREATION_BASE_REF, &base])?;
    std::fs::write(checkout.join("subject.txt"), "b\n")?;
    git(&["add", "subject.txt"])?;
    git(&[&commit[..], &["her change"]].concat())?;

    let params: WorkSubmitParams = serde_json::from_value(json!({
        "room": r.room.channel.as_uuid().to_string(),
        "card_id": r.card.as_uuid().to_string(),
        "claim_id": r.claim.as_uuid().to_string(),
    }))?;
    let published = r.submit().run(&r.act("issuing-gen"), params).await?;

    let submitted = r
        .airc
        .work_board_in(&r.room)
        .await?
        .card(r.card)
        .and_then(|c| c.submissions.iter().find(|s| s.submission_id.as_uuid() == published.submission_id).cloned())
        .ok_or("work/submit published the submission")?;
    assert_eq!(submitted.base_sha, airc_work::GitObjectId::new(base.clone())?, "the creation base, read off her checkout");
    let patch = crate::commands::benchmark::workspace_candidate_diff_from(&checkout.to_string_lossy(), Some(&base))?;
    assert!(patch.contains("+b"), "her change is the patch: {patch}");
    let artifact = serde_json::to_value(&submitted.artifact)?;
    let expected = {
        use sha2::Digest;
        format!("{:x}", sha2::Sha256::digest(patch.as_bytes()))
    };
    assert_eq!(artifact["hash"], json!(expected), "the published artifact is exactly her patch");
    assert_eq!(artifact["size_bytes"], json!(patch.len()));
    // Retained, not only hashed: the store hands back exactly her patch's bytes.
    let retained = {
        use airc_blobs::ContentAddressedStore;
        crate::media::artifact::store()?.get(&airc_blobs::ContentHash::from_bytes(patch.as_bytes()))?
    };
    assert_eq!(retained, patch.as_bytes(), "the retained blob is the published patch");
    let recorded: reviewed::PendingCreditBinding = reviewed::read_one(&r.data(), r.name, &published.submission_id.to_string())
        .await?
        .ok_or("the issuing generation is recorded")?;
    assert_eq!(recorded.selection.issuing_generation, "issuing-gen");
    Ok(())
}

/// Her store, with a training trigger that never answers a submit for `hang`, answers one for
/// `slow` only after [`SLOWER_THAN_A_TICK_BUDGET`], and accepts any other at once.
struct HangOn {
    data: InProcessTransport,
    hang: Uuid,
    slow: Option<Uuid>,
}

/// Longer than the 2.5 s per-tick budget the region's settles used to be cut at, so a settle
/// that is still cut by a tick can never complete it.
const SLOWER_THAN_A_TICK_BUDGET: std::time::Duration = std::time::Duration::from_secs(3);

#[async_trait::async_trait]
impl Transport for HangOn {
    async fn execute(&self, command: &str, params: serde_json::Value) -> Result<serde_json::Value, ClientError> {
        if command != "genome/training-trigger/submit" {
            return self.data.execute(command, params).await;
        }
        let submission = params["submissionId"].clone();
        if submission == json!(self.hang) {
            std::future::pending::<()>().await;
        }
        if self.slow.is_some_and(|slow| submission == json!(slow)) {
            tokio::time::sleep(SLOWER_THAN_A_TICK_BUDGET).await;
        }
        Ok(json!({"success": true, "acceptance": {"submissionId": submission, "replayed": false}}))
    }
    async fn subscribe(&self, class: &str) -> Result<continuum_client::event::EventStream, ClientError> {
        self.data.subscribe(class).await
    }
    async fn emit(&self, class: &str, payload: serde_json::Value) -> Result<(), ClientError> {
        self.data.emit(class, payload).await
    }
    async fn provide(&self, command: &str, handler: Arc<dyn continuum_client::ServeHandler>) -> Result<(), ClientError> {
        self.data.provide(command, handler).await
    }
    async fn revoke(&self, command: &str) -> Result<(), ClientError> {
        self.data.revoke(command).await
    }
    async fn close(&self) -> Result<(), ClientError> {
        self.data.close().await
    }
}

// what this catches (Codex on #5531; card a4e8756d, Kimi's 28087ebb on Windows 2026-10-28): an
// earlier decided credit whose settle hangs on EVERY pass holding back a different decided credit
// behind it, and a settle slower than a tick's budget cut on every pass so it never completes.
// Two ordinary submissions on her card, both passed while the trigger refused; the first in id
// order never settles, the second answers only after SLOWER_THAN_A_TICK_BUDGET and is still
// accepted by the region with no review event, registration or turn; the first still waits.
#[tokio::test]
pub(super) async fn a_decision_that_always_hangs_never_holds_back_the_next() -> TestResult {
    use crate::runtime::{BrainRegion, RegionContext};
    use airc_work::{CardCreated, WorkCardClaimed, WorkEvent, WorkReviewOutcome, WorkSubmissionReview};
    let r = Resident::new().await?;
    let first = r.submit().run(&r.act("gen-first"), r.params.clone()).await?;
    let first_row = r.stage("gen-first").await?;
    let patch = "diff --git a/y b/y\n--- a/y\n+++ b/y\n@@ -1 +1 @@\n-c\n+d\n";
    let hash = {
        use airc_blobs::ContentAddressedStore;
        crate::media::artifact::store()?.put(patch.as_bytes())?.to_hex()
    };
    let mut second_params = r.params.clone();
    second_params.artifact = Some(serde_json::from_value(json!({"hash": hash, "size_bytes": patch.len(), "mime": "text/x-diff"}))?);
    let second = r.submit().run(&r.act("gen-second"), second_params).await?;
    let second_row = r.stage("gen-second").await?;

    let board_now = r.airc.work_board_in(&r.room).await?;
    let card = board_now.card(r.card).ok_or("her card is on the board")?;
    let submitted = |id: Uuid| card.submissions.iter().find(|s| s.submission_id.as_uuid() == id).cloned().ok_or("published");
    let (a, b) = (submitted(first.submission_id)?, submitted(second.submission_id)?);
    let (owner, reviewer) = (r.airc.peer_id(), airc_core::PeerId::new());
    let at = a.submitted_at_ms.min(b.submitted_at_ms);
    let repo = airc_work::RepoId::new("acme/ordinary-project")?;
    let created = |card_id, reviews, by| WorkEvent::CardCreated(CardCreated {
        card_id, repo: repo.clone(), title: "ordinary work, submitted the ordinary way".into(), body: None,
        priority: airc_work::Priority::P1, lane_id: None, created_by: by, created_at_ms: at.saturating_sub(2), reviews, origin: None,
    });
    let claimed = |card_id, claim_id, holder| WorkEvent::CardClaimed(WorkCardClaimed {
        selected_at_ms: None, origin: airc_work::ClaimOrigin::Unknown, card_id, claim_id, owner: holder, ttl_ms: 600_000,
        claimed_at_ms: at.saturating_sub(1),
    });
    let mut events = vec![
        (created(r.card, None, owner), owner),
        (claimed(r.card, a.claim_id, owner), owner),
        (WorkEvent::WorkSubmitted(a.clone()), owner),
        (WorkEvent::WorkSubmitted(b.clone()), owner),
    ];
    let mut passes = Vec::new();
    for s in [&a, &b] {
        let (review_card, review_claim) = (r.review_card(s.submission_id.as_uuid()), airc_work::ClaimId::new());
        let pass = WorkSubmissionReview {
            review_id: airc_work::WorkReviewId::new(), card_id: r.card, submission_id: s.submission_id, artifact: s.artifact.clone(),
            review_card_id: review_card, review_claim_id: review_claim, reviewer, outcome: WorkReviewOutcome::Passed,
            evidence: s.artifact.clone(), evidence_text: None, reviewed_at_ms: at + 10,
        };
        events.push((created(review_card, Some(r.card), reviewer), reviewer));
        events.push((claimed(review_card, review_claim, reviewer), reviewer));
        events.push((WorkEvent::WorkSubmissionReviewed(pass.clone()), reviewer));
        passes.push(pass);
    }
    let board = project_events(r.room.channel.as_uuid(), events);
    for (pass, row) in passes.iter().zip([&first_row, &second_row]) {
        let refusing = continuum_client::mock::MockTransport::new();
        script_settlement_reply(&refusing, row, Some(json!({"success": false, "errorKind": "RecoveryRequired"})));
        let decided = reviewed::consume_review_with_boundary(
            &settlement_connection(r.executor.clone(), r.persona, refusing, None),
            r.name, r.persona, r.room.channel.as_uuid(), &board, pass.review_id, &reviewed::SelfReview::NotAdmitted, None,
        )
        .await?;
        assert_eq!(decided.state, State::AwaitingAcceptance, "{decided:?}");
    }

    // The decision first in id order hangs on every pass; the other must still be settled.
    let (stuck, healthy) = if first.submission_id < second.submission_id { (first.submission_id, second.submission_id) } else { (second.submission_id, first.submission_id) };
    let stuck_row = if stuck == first.submission_id { first_row.id } else { second_row.id };
    let healthy_row = if stuck == first.submission_id { second_row.id } else { first_row.id };
    let (executor, persona, name) = (r.executor.clone(), r.persona, r.name);
    let region = CreditAcceptanceRegion::new(Arc::new(move |_| {
        let data = InProcessTransport::new(executor.clone(), Some(CallerIdentity::local_persona(crate::identity::PeerId::from_uuid(persona))));
        Some((name.to_string(), Connection::new(HangOn { data, hang: stuck_row, slow: Some(healthy_row) })))
    }));
    let scoped = RegionContext { tick_number: 1, persona_scope: Some(r.persona) };
    // Settles run off the tick: tick as the governor does and give them time, well past the
    // slow one's answer.
    let mut settled = false;
    for _ in 0..40 {
        region.tick(&scoped).await;
        if reviewed::credit_status(&r.data(), r.name, healthy).await?.state == State::Accepted {
            settled = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    assert!(settled, "the healthy decision behind a permanently hanging one is settled");
    assert_eq!(reviewed::credit_status(&r.data(), r.name, stuck).await?.state, State::AwaitingAcceptance, "the hanging one still waits");
    Ok(())
}
