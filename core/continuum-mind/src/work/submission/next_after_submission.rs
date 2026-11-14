use super::*;

#[async_trait]
impl ActionCommand for WorkSubmit {
    const NAME: &'static str = "work/submit";
    const NATIVE: bool = true;
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Submit own work for review, derived from your checkout. Publication is not success.";
    type Params = WorkSubmitParams;
    type Output = WorkSubmitResult;

    async fn run(&self, ctx: &Ctx, p: WorkSubmitParams) -> Result<WorkSubmitResult, CommandError> {
        let runtime = persona_runtime(&self.registry, ctx, "work/submit")?;
        let airc = runtime.airc();
        let room = room_of_card(airc, p.room.as_deref(), &p.card_id, "work/submit").await?;
        // Validate before allocating a durable binding. The SDK validates again
        // against its own latest projection immediately before publication.
        let board = airc
            .work_board_in(&room)
            .await
            .map_err(|e| CommandError::Internal(e.to_string()))?;
        // THE BOARD IS READ FIRST so the handle resolves against THIS room's cards and
        // nothing wider. A full UUID passes straight through and never consults the set.
        let snapshot = board.snapshot();
        let card_uuid = match resolve_shown(
            &p.card_id,
            &snapshot.cards.iter().map(|c| c.card_id.as_uuid()).collect::<Vec<_>>(),
            "card",
        ) {
            Ok(id) => id,
            Err(e) => {
                let hint = card_elsewhere(airc, &p.card_id, room.channel.as_uuid()).await;
                return Err(with_hint(e, hint));
            }
        };
        let card_id = WorkCardId::from_uuid(card_uuid);
        let Some(card) = board.card(card_id) else {
            let absent = CommandError::NotFound(format!(
                "card {} is absent from the board of room {}: you asked for '{}'",
                card_uuid,
                room_label(&room.name, room.channel.as_uuid()),
                p.room.as_deref().unwrap_or("none, the card's own") // unwrap_or: no room named is a fact to state, never an error
            ));
            let hint = card_elsewhere(airc, &p.card_id, room.channel.as_uuid()).await;
            return Err(with_hint(absent, hint));
        };
        // THE PRECONDITION, at the mutation boundary, against the card as the board holds it
        // now (`work_preconditions::submit`, the one owner the surface reads too): her own card,
        // not ended, under her claim, the claim she names (from a receipt) the card's claim now.
        // A supplied claim id never bypasses ownership before local work is read (#4309);
        // publication below still validates the lease against the latest board.
        let now_ms = EpochMs::now().millis();
        let request = work_preconditions::Request {
            at_ms: now_ms,
            claim: match p.claim_id.as_deref() {
                // Scoped to THIS card's own claim, so a handle from a stale receipt is refused
                // by name rather than silently accepted.
                Some(c) => work_preconditions::ClaimArg::Named(ClaimId::from_uuid(resolve_shown(
                    c,
                    &card.claim_id.map(|id| id.as_uuid()).into_iter().collect::<Vec<_>>(),
                    "claim",
                )?)),
                None => work_preconditions::ClaimArg::Hers,
            },
            seen_revision: None,
        };
        let claim_id = work_preconditions::decide(work_preconditions::submit(card, airc.peer_id(), &request), card, airc.peer_id(), now_ms)
            .map_err(super::super::refused)?;
        // Her act IS her presence: a lease that lapsed between wakes is renewed by the
        // submit itself, never a reason to refuse the owner. Responsibility is durable,
        // the lease is only presence (2026-09-28); the board kept her as owner through
        // the expiry, so this is a renewal, not a claim. A renewal the wire refuses is
        // reported and the submit proceeds on her ownership (the precondition passed).
        renew_owner_lease(&airc, card, claim_id).await;
        // A complete, real artifact she wrote herself is honoured as-is; anything less
        // — an omitted part, or the manual's zero hash read back as a value — is read
        // off her checkout, and a placeholder says so in the ledger.
        // A card its oracle grades lands by its grade; every other card lands through its PR.
        let bench = crate::recipe::benchmark_catalog::graded_at_submit(&card.title);
        let typed_in_full = matches!(&p.artifact, Some(a) if !is_placeholder_hash(&a.hash))
            && p.base_sha.is_some()
            && p.head.is_some()
            && p.instance.is_some();
        if matches!(&p.artifact, Some(a) if is_placeholder_hash(&a.hash)) {
            continuum_base::probe!(
                class = "work.submit.placeholder_artifact",
                card = %card_uuid,
                "artifact.hash was a placeholder (zeros / a manual blank) — deriving the real one from her checkout"
            );
        }
        // Her checkout: what a derived submission reads, and what its pull request is pushed from.
        // A typed submission uses a checkout already here and never stages one.
        // A checkout off its creation anchor is staged again: airc re-allocates an anchorless one or
        // refuses naming what it holds, and arrival puts an anchored one back on its anchor (cards
        // 7f824214, f76005e8); never submitted from as found.
        let found = match crate::persona::card_checkout::checkout_path_for(&runtime.persona_id(), card) {
            Some(path) if crate::persona::card_checkout::off_creation_anchor(&path).await => None,
            found => found,
        };
        let checkout = match (typed_in_full, found) {
            (_, Some(path)) => Some(path),
            (true, None) => None,
            (false, None) => Some(stage_for_owner_on_demand(&runtime, card).await?),
        };
        let (instance, base_sha_text, head_text, artifact_ref) = if typed_in_full {
            (
                p.instance.clone().unwrap_or_default(), // unwrap_or: guarded by typed_in_full
                p.base_sha.clone().unwrap_or_default(), // unwrap_or: guarded by typed_in_full
                p.head.clone().unwrap_or_default(), // unwrap_or: guarded by typed_in_full
                p.artifact.clone().unwrap_or(WorkArtifactReference { hash: String::new(), size_bytes: 0, mime: None }), // unwrap_or: guarded by typed_in_full
            )
        } else {
            // Claim-time staging owns this binding. Ambient turn focus can be home,
            // another card, or absent after restart; none changes this card's checkout.
            // A checkout missing here (a restart, a node the card moved to) used to
            // bounce her to a re-claim, and work/claim refuses a card in Review, so the
            // owner of a Review card could never submit again (Kimi, 2026-10-04: 7
            // submit refusals, 3 claim refusals, 29 of 42 board calls failed). The
            // owner's submit stages the same way her claim does; staging is the
            // substrate's job whenever she needs her hands, never a verb she must find.
            let checkout = checkout.as_deref().ok_or_else(|| CommandError::Internal("a derived submission resolved no checkout".into()))?;
            let listed = p.submodules.as_deref().unwrap_or_default(); // unwrap_or_default: none listed is an empty list
            let d = derive_submission(self.benchmark.as_ref(), checkout, card_uuid, p.instance.clone(), p.base_sha.clone(), Some(&card.repo), listed).await?;
            (d.instance, d.base_sha, d.head, d.artifact)
        };
        continuum_base::probe!(
            class = "work.submit.shaped",
            card = %card_uuid,
            instance = %instance,
            base = %base_sha_text,
            head = %head_text,
            size_bytes = artifact_ref.size_bytes,
            derived = !typed_in_full,
            "the submission as it goes to the board"
        );
        let artifact = artifact_ref.into_artifact()?;
        if typed_in_full {
            // A typed artifact is honoured only if its bytes are retained: a reference with
            // nothing behind it published a submission no reviewer could read (card e1babb61).
            crate::media::artifact::read_text_blob(&artifact).map_err(|why| {
                CommandError::Invalid(format!(
                    "the artifact you typed has no retained bytes ({why}); submit again without `artifact`: the patch is derived from your checkout and kept"
                ))
            })?;
        }
        let base_sha = airc_work::GitObjectId::new(base_sha_text)
            .map_err(|e| CommandError::Invalid(format!("base_sha: {e}")))?;
        let head = airc_work::GitObjectId::new(head_text)
            .map_err(|e| CommandError::Invalid(format!("head: {e}")))?;
        // THE SAME PATCH, SUBMITTED AGAIN, is refused by name (Kimi, 2026-09-28: her
        // handoff said "do not re-make it", she resubmitted, and two identical
        // submissions sat on the card with credit ambiguous between them). Only when she
        // named no submission id: an explicit id is a deliberate call.
        if p.submission_id.is_none() {
            let artifact_hash = artifact.hash.to_string();
            // Not keyed on the claim (Cormac on #4580): a claim renewed or re-taken across a
            // rebuild mints a new id, which is exactly how her duplicate happened.
            // Keyed on what makes it the same work: who, which patch, applied to which base,
            // for which instance (Codex on #4580: an identical diff on a new base is a
            // legitimate rebase, never a duplicate).
            let earlier = card.submissions.iter().map(|s| SubmissionKey {
                id: s.submission_id.as_uuid(),
                publisher: s.publisher.as_uuid(),
                artifact_hash: s.artifact.hash.to_string(),
                base_sha: s.base_sha.to_string(),
                instance: s.instance.clone(),
            });
            let this = SubmissionKey {
                id: Uuid::nil(),
                publisher: airc.peer_id().as_uuid(),
                artifact_hash,
                base_sha: base_sha.to_string(),
                instance: instance.clone(),
            };
            // Answered at SUBMISSION IDENTITY, as success: the board already holds this exact
            // submission, so she is told which one and what became of it, and her loop ends with
            // information (Benchy, the M5 2026-11-04: 5 refused submits). A changed patch, or the
            // same patch on a new base, is a new submission and publishes below.
            if let Some(existing) = same_submission(earlier, &this).and_then(|id| card.submissions.iter().find(|s| s.submission_id.as_uuid() == id)) {
                let state = review_state(&board, existing, &crate::persona::work_board::review::room_policy(airc, &room).await.counted());
                // The same patch again is how she retries a pull request that did not go up: it is
                // ensured here too, at the submitted head.
                let landing = super::card_pull_request::land_submission(airc, &room, card, super::card_pull_request::Submitted {
                    checkout: checkout.as_deref(), head: &existing.head, submission: existing.submission_id.as_uuid(), patch_hash: existing.artifact.hash.to_string(),
                }, bench, ctx).await;
                let linked = landing.as_ref().and_then(|l| l.as_ref().ok()).map(|e| e.pull_request.clone()).or_else(|| card.pull_request.clone());
                let mut result = already_submitted(existing, room.channel.as_uuid(), state, bench, linked.as_ref());
                result.next = super::card_pull_request::with_pull_request(result.next, landing.as_ref());
                return Ok(result);
            }
        }
        // A retry names her submission as she was shown it (Benchy, M5 2026-11-04 17:51:56Z:
        // the short id died in serde as "expected length 32"); a full id she mints passes.
        let submission_id = match &p.submission_id {
            Some(shown) => submission_named(shown, Some(card)).map_err(shown_refused)?.as_uuid(),
            None => Uuid::new_v4(),
        };
        let candidate = airc_work::WorkSubmission {
            submission_id: airc_work::SubmissionId::from_uuid(submission_id),
            card_id,
            claim_id,
            instance,
            base_sha,
            head: airc_work::SubmissionHead::At(head.clone()),
            artifact,
            publisher: airc.peer_id(),
            submitted_at_ms: continuum_base::time::EpochMs::now().millis(),
        };
        candidate
            .validate_for_card(card)
            .map_err(|e| CommandError::Invalid(format!("submission refused: {e}")))?;
        // At most one credit selection, bound once: the binding below matches this value,
        // so there is no third "neither" case for it to meet.
        let selection = match (p.generation_request_id.as_deref(), p.staged_revision_id) {
            (Some(_), Some(_)) => {
                return Err(CommandError::Invalid(
                    "select staged_revision_id or generation_request_id, not both".into(),
                ));
            }
            (Some(generation), None) => Some(CreditSelection::Generation(generation)),
            (None, Some(revision)) => Some(CreditSelection::Revision(revision)),
            (None, None) => None,
        };
        let mut bound_staged_revision_id = None;
        if let Some(selection) = selection {
            if self.registry.get(runtime.persona_id()).is_none()
                || runtime.persona_id() != airc.peer_id().as_uuid()
            {
                return Err(CommandError::Invalid(
                    "selected credit must belong to the calling resident persona".into(),
                ));
            }
            let conn = credit_connection(self.executor_slot.cloned(), runtime.persona_id())?;
            let binding = match selection {
                CreditSelection::Generation(generation) => reviewed::bind_submission_generation(
                    &conn,
                    runtime.agent_name(),
                    runtime.persona_id(),
                    room.channel.as_uuid(),
                    &candidate,
                    generation,
                )
                .await,
                CreditSelection::Revision(revision) => reviewed::bind_submission(
                    &conn,
                    runtime.agent_name(),
                    runtime.persona_id(),
                    SubmissionSelection {
                        submission_id,
                        room_id: room.channel.as_uuid(),
                        card_id: card_uuid,
                        claim_id: claim_id.as_uuid(),
                        staged_revision_id: revision,
                        instance: candidate.instance.clone(),
                        base_sha: candidate.base_sha.clone(),
                        artifact: candidate.artifact.clone(),
                    },
                )
                .await,
            }
            .map_err(|e| match e {
                reviewed::CreditBindingError::Storage(source) => {
                    CommandError::Internal(source.to_string())
                }
                other => CommandError::Invalid(other.to_string()),
            })?;
            bound_staged_revision_id = Some(binding.selection.staged_revision_id);
        } else if let Some(generation) = ctx
            .caller
            .as_ref()
            .filter(|c| c.peer_id.as_uuid() == runtime.persona_id())
            .and_then(|c| c.issuing_generation.clone())
            // A retry of a submission the board already holds records nothing new.
            .filter(|_| !card.submissions.iter().any(|s| s.submission_id.as_uuid() == submission_id))
        {
            // She named no selection: the generation whose tool call this is, bound by her act
            // seam and never by her arguments, is recorded before publication and binds when a
            // review arrives (card fe8554d2). Only her own act's caller carries it. A record that
            // fails refuses, like an explicit binding: publish only after provenance is durable.
            let conn = credit_connection(self.executor_slot.cloned(), runtime.persona_id())?;
            reviewed::record_pending(
                &conn,
                runtime.agent_name(),
                runtime.persona_id(),
                reviewed::PendingSelection {
                    publication: reviewed::Publication::of(room.channel.as_uuid(), &candidate),
                    issuing_generation: generation,
                },
            )
            .await
            .map_err(|e| match e {
                reviewed::CreditBindingError::Storage(source) => CommandError::Internal(format!(
                    "credit provenance was not recorded; retry the same submission_id: {source}"
                )),
                other => CommandError::Invalid(other.to_string()),
            })?;
        }
        let grade_title = card.title.clone();
        let grade_patch = candidate.artifact.clone();
        let published_head = head;
        let published = airc.submit_work_in(&room, airc_lib::SubmitWork {
            submission_id: candidate.submission_id, card_id, claim_id: candidate.claim_id,
            instance: candidate.instance, base_sha: candidate.base_sha, head: published_head.clone(), artifact: candidate.artifact,
        }).await.map_err(|e| CommandError::Internal(format!(
            "submission {submission_id} publication was not acknowledged; retry with submission_id={submission_id} and the same selection: {e}")))?;
        crate::persona::work_board::board_index::wrote(airc, &room);
        // Her work reaches the merger only through a pull request linked to the card: ensured
        // here, at the head just published, so the reviewed head and the PR head are one.
        let landing = super::card_pull_request::land_submission(airc, &room, card, super::card_pull_request::Submitted {
            checkout: checkout.as_deref(), head: &airc_work::SubmissionHead::At(published_head), submission: published.submission_id.as_uuid(), patch_hash: published.artifact.hash.to_string(),
        }, bench, ctx).await;
        let linked = landing.as_ref().and_then(|l| l.as_ref().ok()).map(|e| e.pull_request.clone()).or_else(|| card.pull_request.clone());
        let handoff = if bench {
            Ok(None)
        } else {
            super::super::open_room_review_card(airc, &room, card, card_id, Some(published.submission_id)).await.map(Some)
        };
        // A hand-in on a bench card is graded now, here, where the artifact lives; a
        // card that is not a SWE bench card returns at once.
        let benchmark = Arc::clone(&self.benchmark);
        let grade_airc = Arc::clone(airc);
        let grade_room = room.channel;
        let grade_instance = published.instance.clone();
        tokio::spawn(async move {
            benchmark.grade_hand_in(grade_airc, grade_room, grade_title, grade_instance, grade_patch).await;
        });
        // Guidance reflects this command's board snapshot. A concurrently
        // arriving verdict appears on the next work/get or work/submission read.
        // A submission published just now carries no verdict yet: nothing recorded, nothing passed.
        let state = ReviewState::Awaiting { passes: 0, required: 1 };
        Ok(WorkSubmitResult {
            outcome: SubmitOutcome::Published,
            submission_id: published.submission_id.as_uuid(),
            card_id: published.card_id.as_uuid(),
            room_id: room.channel.as_uuid(),
            publisher: published.publisher.as_uuid(),
            claim_id: published.claim_id.as_uuid(),
            base_sha: published.base_sha.to_string(),
            head: published.head.commit().map(ToString::to_string),
            instance: published.instance.clone(),
            artifact: (&published.artifact).into(),
            submitted_at_ms: published.submitted_at_ms,
            bound_staged_revision_id,
            next: match handoff {
                Ok(review) => super::card_pull_request::with_pull_request(
                    next_after_submission(Some(published.submission_id.as_uuid()), state, bench, review, linked.as_ref()).unwrap_or_default(), // unwrap_or_default: Some(..) in, so always Some
                    landing.as_ref(),
                ),
                Err(error) => format!("Submission {} is retained, but review handoff failed: {error}. Retry work/submit with this same submission_id; do not create a new candidate to retry handoff.", published.submission_id),
            },
        })
    }
}

crate::register_command!(WorkSubmit);

pub struct WorkReview {
    pub registry: PersonaAircRuntimeRegistry,
    pub executor_slot: Arc<LateBound<CommandExecutor>>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/work/ReviewOutcome.ts"
)]
pub enum ReviewOutcome {
    Passed,
    Failed,
    Unknown,
}

impl From<airc_work::WorkReviewOutcome> for ReviewOutcome {
    fn from(value: airc_work::WorkReviewOutcome) -> Self {
        match value {
            airc_work::WorkReviewOutcome::Passed => Self::Passed,
            airc_work::WorkReviewOutcome::Failed => Self::Failed,
            airc_work::WorkReviewOutcome::Unknown => Self::Unknown,
        }
    }
}

impl From<ReviewOutcome> for airc_work::WorkReviewOutcome {
    fn from(value: ReviewOutcome) -> Self {
        match value {
            ReviewOutcome::Passed => Self::Passed,
            ReviewOutcome::Failed => Self::Failed,
            ReviewOutcome::Unknown => Self::Unknown,
        }
    }
}

// Deserialize only: the `Shown` ids are read from her call, never re-rendered.
#[derive(Debug, Clone, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/work/WorkReviewParams.ts"
)]
pub struct WorkReviewParams {
    /// Optional; the card's board decides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub room: Option<String>,
    // The REVIEW card she holds. The parent card, its latest submission and artifact,
    // and her claim on the review card are read off the board from it when the fields
    // below are omitted — a reviewer never had a way to know a submission id or an
    // artifact hash by hand (5204f4b5 sent all-zero ids, 2026-09-16).
    /// Your linked review card (board handle or full UUID), not the task you authored.
    #[ts(type = "string")]
    pub review_card_id: String,
    /// Your verdict.
    pub outcome: ReviewOutcome,
    /// What you ran and saw; the review's evidence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub evidence_text: Option<String>,
    /// Minted when omitted.
    #[serde(default)]
    #[ts(optional, type = "string")]
    #[schemars(with = "Option<String>")]
    pub review_id: Option<Shown<airc_work::WorkReviewId>>,
    /// Parent card (handle or UUID); defaults to review parent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string")]
    pub card_id: Option<String>,
    /// `latest` (default) or a submission id.
    #[serde(default)]
    #[ts(optional, type = "\"latest\" | string")]
    pub submission_id: SubmissionHandle,
    /// Artifact; defaults to board.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub artifact: Option<WorkArtifactReference>,
    /// Review claim; defaults to board.
    #[serde(default)]
    #[ts(optional, type = "string")]
    #[schemars(with = "Option<String>")]
    pub review_claim_id: Option<Shown<ClaimId>>,
    /// Typed evidence instead of evidence_text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub evidence: Option<WorkArtifactReference>,
    /// Required to pass: the commit you ran the checks at; must be the submission's head.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub verified_head: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/work/WorkReviewResult.ts"
)]
pub struct WorkReviewResult {
    #[ts(type = "string")]
    pub review_id: Uuid,
    #[ts(type = "string")]
    pub submission_id: Uuid,
    #[ts(type = "string")]
    pub reviewer: Uuid,
    pub outcome: ReviewOutcome,
    #[ts(type = "number")]
    pub reviewed_at_ms: u64,
    #[ts(type = "string")]
    pub review_card_id: Uuid,
    #[ts(type = "string")]
    pub review_claim_id: Uuid,
    pub artifact: WorkArtifactReference,
    pub evidence: WorkArtifactReference,
    pub evidence_text: Option<String>,
    /// A review can publish even when its local learning transfer is deferred.
    pub credit: Option<reviewed::ReviewedCredit>,
    pub credit_error: Option<String>,
}

impl From<&airc_work::WorkSubmissionReview> for WorkReviewResult {
    fn from(r: &airc_work::WorkSubmissionReview) -> Self {
        Self {
            review_id: r.review_id.as_uuid(), submission_id: r.submission_id.as_uuid(),
            reviewer: r.reviewer.as_uuid(), outcome: r.outcome.into(),
            reviewed_at_ms: r.reviewed_at_ms, review_card_id: r.review_card_id.as_uuid(),
            review_claim_id: r.review_claim_id.as_uuid(), artifact: (&r.artifact).into(),
            evidence: (&r.evidence).into(), evidence_text: r.evidence_text.clone(),
            credit: None, credit_error: None,
        }
    }
}

#[async_trait]
impl ActionCommand for WorkReview {
    const NAME: &'static str = "work/review";
    // In hand only on a review card she holds (`work_preconditions::in_hand`): never offered on
    // her own implementation card, where it can only be refused.
    const DISCLOSURE: continuum_base::tool_disclosure::Disclosure = continuum_base::tool_disclosure::Disclosure::OnDemand;
    const NATIVE: bool = true;
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str = "Verdict under a reviewer claim, not training completion. Own work: work/submit.";
    type Params = WorkReviewParams;
    type Output = WorkReviewResult;

    async fn run(&self, ctx: &Ctx, p: WorkReviewParams) -> Result<Self::Output, CommandError> {
        let runtime = persona_runtime(&self.registry, ctx, "work/review")?;
        let airc = runtime.airc();
        let room = room_of_card(airc, p.room.as_deref(), &p.review_card_id, "work/review").await?;
        // Everything a reviewer cannot know by hand is read off the board from the
        // review card she holds: the parent, her claim, the parent's latest submission
        // and its artifact. Typed values are honoured; placeholders were already
        // refused at the executor seam.
        let board = airc
            .work_board_in(&room)
            .await
            .map_err(|e| CommandError::Internal(e.to_string()))?;
        // Her handles resolve against THIS room's cards, the board she was shown, the way
        // work/submit's do: the board shows short ids, and a Uuid field refused them in serde
        // before any resolver ran (Kimi's verdict on #4825, 2026-10-06: "expected length 32
        // ... found 8").
        let snapshot = board.snapshot();
        let shown: Vec<Uuid> = snapshot.cards.iter().map(|c| c.card_id.as_uuid()).collect();
        let review_uuid = match resolve_shown(&p.review_card_id, &shown, "card") {
            Ok(id) => id,
            Err(e) => {
                let hint = card_elsewhere(airc, &p.review_card_id, room.channel.as_uuid()).await;
                return Err(with_hint(e, hint));
            }
        };
        // Absent here: name the room that does hold it, the way work/submit does. Kimi
        // reviewed a peer's card at 07:19Z with room 'academy' while the review card sat
        // on another board, and the refusal said only where it was not.
        let Some(review_card) = board.card(WorkCardId::from_uuid(review_uuid)) else {
            let absent = CommandError::NotFound(format!(
                "the review card you named is absent from the board of room {} — you asked for '{}'",
                room_label(&room.name, room.channel.as_uuid()),
                p.room.as_deref().unwrap_or("none, the card's own") // unwrap_or: no room named is a fact to state, never an error
            ));
            let hint = card_elsewhere(airc, &p.review_card_id, room.channel.as_uuid()).await;
            return Err(with_hint(absent, hint));
        };
        // THE PRECONDITION, at the mutation boundary (`work_preconditions::review`, the one
        // owner the surface reads too): the card reviews something (the parent she names, when
        // she names one), and she holds it unless she states the review claim herself.
        let named = match p.card_id.as_deref() {
            Some(c) => Some(WorkCardId::from_uuid(resolve_shown(c, &shown, "card")?)),
            None => None,
        };
        // Her claim on THIS review card, named by what she was shown: a handle from a stale
        // receipt is refused by name. The holder rule itself is the precondition's.
        let stated_claim = p
            .review_claim_id
            .as_ref()
            .map(|c| c.resolve(review_card.claim_id))
            .transpose()
            .map_err(shown_refused)?;
        let now_ms = EpochMs::now().millis();
        let (review_claim_id, card_id) = work_preconditions::decide(
            work_preconditions::review(review_card, airc.peer_id(), named, stated_claim),
            review_card,
            airc.peer_id(),
            now_ms,
        )
        .map_err(super::super::refused)?;
        // The parent's submissions are the ones a review can name, through the one handle
        // resolver `work/submission` reads by too (Benchy's short id, M5 2026-11-04 17:51:56Z).
        let submission = match p.submission_id.read(&board, review_card) {
            Ok(SubmissionRead::Found { submission, .. }) => submission,
            Ok(SubmissionRead::NoneYet { on }) => {
                return Err(submission_refused(SubmissionRefusal::NoneYet { card: on.card_id }, &p.submission_id, review_card.card_id, on, airc.peer_id(), now_ms))
            }
            Err(unread) => {
                return Err(submission_refused(SubmissionRefusal::Unread(unread), &p.submission_id, review_card.card_id, review_card, airc.peer_id(), now_ms))
            }
        };
        // THE AUTHOR'S MIND NEVER JUDGES HER OWN WORK unless the room declares `self: true`
        // (cards fa4aaaaa, 0174211f): compared as minds through the identity owner, never as raw
        // ids. A room declaring no policy admitted it, and Aiko passed her own submissions twice
        // on the M5, 2026-11-14 (reviews 13f23b90, 54f644ac). Refused up front, by type.
        if same_mind(&self.registry, submission.publisher, airc.peer_id()) {
            let policy = crate::persona::work_board::review::room_policy(airc, &room).await;
            work_preconditions::decide(
                author_may_review(policy.declared(), &self.registry, submission, airc.peer_id())
                    .map_err(work_preconditions::Unmet::AuthorCannotReview),
                review_card,
                airc.peer_id(),
                now_ms,
            )
            .map_err(super::super::refused)?;
        }
        // A PASS IS COMPLETED VERIFICATION of the submitted head (card 0174211f): she names the
        // commit she ran, and it is the submission's head. Aiko's 07:04Z pass tested a file copied
        // from 905cadb while the PR head failed CI.
        let verified = match p.outcome {
            ReviewOutcome::Passed => Some(
                verified_head(submission, p.verified_head.as_deref())
                    .map_err(|unverified| CommandError::Invalid(unverified.to_string()))?
                    .clone(),
            ),
            ReviewOutcome::Failed | ReviewOutcome::Unknown => None,
        };
        let artifact = match p.artifact.clone() {
            Some(a) => a.into_artifact()?,
            None => submission.artifact.clone(),
        };
        // The verified head leads her evidence words, rendered from the typed value, so every reader
        // of the signed record sees which commit the pass covers.
        let evidence_text = match (&verified, p.evidence_text.as_deref()) {
            (Some(head), Some(text)) if !text.trim().is_empty() => Some(format!("verified at {head}: {text}")),
            (_, text) => text.map(str::to_string),
        };
        let evidence = match (p.evidence.clone(), evidence_text.as_deref()) {
            (Some(e), _) => e.into_artifact()?,
            (None, Some(text)) if !text.trim().is_empty() => artifact_of_text(text).into_artifact()?,
            _ => {
                return Err(CommandError::Invalid(
                    "a review carries evidence: pass evidence_text (what you ran and saw) — a verdict without evidence is not a verdict".into(),
                ))
            }
        };
        // A retry names her review as she was shown it; a full id she mints passes.
        let review_id = match &p.review_id {
            Some(shown) => shown
                .resolve(board.submission_reviews_for(submission.submission_id).map(|r| r.review_id))
                .map_err(shown_refused)?
                .as_uuid(),
            None => Uuid::new_v4(),
        };
        continuum_base::probe!(
            class = "work.review.shaped",
            review_card = %short8(review_uuid),
            card = %card_id,
            submission = %submission.submission_id,
            derived = p.submission_id == SubmissionHandle::Latest || p.artifact.is_none() || p.review_claim_id.is_none() || p.card_id.is_none(),
            "the review as it goes to the board"
        );
        let review = airc
            .review_work_submission_in(
                &room,
                airc_lib::ReviewWorkSubmission {
                    review_id: airc_work::WorkReviewId::from_uuid(review_id),
                    card_id,
                    submission_id: submission.submission_id,
                    artifact,
                    review_card_id: WorkCardId::from_uuid(review_uuid),
                    review_claim_id,
                    outcome: p.outcome.into(),
                    evidence,
                    evidence_text,
                },
            )
            .await
            .map_err(|e| {
                CommandError::Invalid(format!(
                    "review not acknowledged; retry the same review_id and judgement: {e}"
                ))
            })?;
        // Re-read accepted signed projection even on the local command path.
        // Publication alone is not permission to bypass replay validation.
        let credit = async {
            let board = airc
                .work_board_in(&room)
                .await
                .map_err(|e| CommandError::Internal(e.to_string()))?;
            let executor = self
                .executor_slot
                .cloned()
                .ok_or_else(|| CommandError::Internal("work executor is not ready".into()))?;
            reviewed::consume_as_publisher(
                &self.registry,
                executor,
                room.channel.as_uuid(),
                &board,
                review.review_id,
            )
            .await
        }
        .await;
        let (credit, credit_error) = match credit {
            Ok(value) => (Some(value), None),
            Err(error) => (None, Some(error.to_string())),
        };
        Ok(WorkReviewResult { credit, credit_error, ..WorkReviewResult::from(&review) })
    }
}

crate::register_command!(WorkReview);

/// The one credit a submission explicitly selects: the generation that produced it, or a
/// staged revision. Parsed from the two optional params, which may not both be set.
enum CreditSelection<'p> {
    Generation(&'p str),
    Revision(Uuid),
}

pub struct WorkSubmission {
    pub registry: PersonaAircRuntimeRegistry,
    pub executor_slot: Arc<LateBound<CommandExecutor>>,
}

// Serialize: a refusal names `work/submission` as her next call with these params.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/work/WorkSubmissionParams.ts"
)]
pub struct WorkSubmissionParams {
    /// Optional; the card's board decides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub room: Option<String>,
    /// Card (a review card reads its parent) — handle or UUID.
    #[ts(type = "string")]
    pub card_id: String,
    /// `latest` (default) or a submission id.
    #[serde(default)]
    #[ts(optional, type = "\"latest\" | string")]
    pub submission_id: SubmissionHandle,
    /// Inspect only: no binding/training. Answers a summary; rows only with evidence_limit.
    #[serde(default)]
    pub include_staged_evidence: bool,
    /// Rows to page (default: not matching the claim).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub evidence_rows: Option<StagedEvidenceRows>,
    /// First row of the page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub evidence_offset: Option<usize>,
    /// Rows to return.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub evidence_limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/work/WorkSubmissionResult.ts"
)]
pub struct WorkSubmissionResult {
    pub submission: WorkSubmitResult,
    #[ts(type = "string")]
    pub claim_id: Uuid,
    pub base_sha: String,
    pub instance: String,
    pub reviews: Vec<WorkReviewResult>,
    pub credit: Option<reviewed::ReviewedCredit>,
    pub credit_error: Option<String>,
    pub staged_evidence: Option<StagedEvidencePage>,
    pub staged_evidence_error: Option<String>,
    // The submitted patch itself, read from the retained blob and verified against the
    // artifact's hash and size: what the reviewer reviews (card e1babb61). Absent with
    // `patch_error` naming why when this node holds no such bytes.
    pub patch: Option<String>,
    pub patch_error: Option<String>,
}
