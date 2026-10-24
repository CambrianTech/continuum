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
        // The claim is HERS on THIS card, read off the board — a typed id she would
        // otherwise have to remember from a claim receipt three turns ago.
        let claim_id = match p.claim_id.as_deref() {
            // Scoped to THIS card's own claim — the narrowest set there is, so a handle
            // from a stale receipt is refused by name rather than silently accepted.
            Some(c) => ClaimId::from_uuid(resolve_shown(
                c,
                &card.claim_id.map(|id| id.as_uuid()).into_iter().collect::<Vec<_>>(),
                "claim",
            )?),
            None => match (card.owner, card.claim_id) {
                (Some(owner), Some(claim)) if owner == airc.peer_id() => claim,
                (Some(owner), _) => {
                    return Err(CommandError::Invalid(format!(
                        "card {} is held by {owner}, not by you — only the holder submits",
                        card_uuid
                    )))
                }
                _ => {
                    return Err(CommandError::Invalid(format!(
                        "card {} is not claimed — claim it (work/claim) before submitting",
                        card_uuid
                    )))
                }
            },
        };
        // A supplied claim id must not bypass ownership before we read local work
        // (#4309). Publication below still validates the current lease against the
        // latest board. The decision is pure so both refusals are pinned by a test.
        holder_guard(card.owner, airc.peer_id(), card.claim_id, claim_id, card_uuid)?;
        // Her act IS her presence: a lease that lapsed between wakes is renewed by the
        // submit itself, never a reason to refuse the owner. Responsibility is durable,
        // the lease is only presence (2026-09-28); the board kept her as owner through
        // the expiry, so this is a renewal, not a claim. A renewal the wire refuses is
        // reported and the submit proceeds on her ownership (holder_guard passed).
        renew_owner_lease(&airc, card, claim_id).await;
        // A complete, real artifact she wrote herself is honoured as-is; anything less
        // — an omitted part, or the manual's zero hash read back as a value — is read
        // off her checkout, and a placeholder says so in the ledger.
        let typed_in_full = matches!(&p.artifact, Some(a) if !is_placeholder_hash(&a.hash))
            && p.base_sha.is_some()
            && p.instance.is_some();
        if matches!(&p.artifact, Some(a) if is_placeholder_hash(&a.hash)) {
            continuum_base::probe!(
                class = "work.submit.placeholder_artifact",
                card = %card_uuid,
                "artifact.hash was a placeholder (zeros / a manual blank) — deriving the real one from her checkout"
            );
        }
        let (instance, base_sha_text, artifact_ref) = if typed_in_full {
            (
                p.instance.clone().unwrap_or_default(), // unwrap_or: guarded by typed_in_full
                p.base_sha.clone().unwrap_or_default(), // unwrap_or: guarded by typed_in_full
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
            let checkout = match crate::persona::card_checkout::checkout_path_for(&runtime.persona_id(), card) {
                Some(path) => path,
                None => stage_for_owner_on_demand(&runtime, card).await?,
            };
            let listed = p.submodules.as_deref().unwrap_or_default(); // unwrap_or_default: none listed is an empty list
            let d = derive_submission(self.benchmark.as_ref(), &checkout, card_uuid, p.instance.clone(), p.base_sha.clone(), Some(&card.repo), listed).await?;
            (d.instance, d.base_sha, d.artifact)
        };
        continuum_base::probe!(
            class = "work.submit.shaped",
            card = %card_uuid,
            instance = %instance,
            base = %&base_sha_text[..base_sha_text.len().min(9)],
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
                    "the artifact you typed has no retained bytes ({why}); omit `artifact` and work/submit derives the patch from your checkout and keeps it"
                ))
            })?;
        }
        let base_sha = airc_work::GitObjectId::new(base_sha_text)
            .map_err(|e| CommandError::Invalid(format!("base_sha: {e}")))?;
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
            if let Some(existing) = same_submission(earlier, &this) {
                return Err(CommandError::Invalid(format!(
                    "card {} already has your submission {} of this exact patch, \
                     so nothing was submitted. Read it with work/submission (submission_id {}); \
                     submit again only after the patch changes.",
                    airc_core::shown_id::shown_form(card_uuid),
                    airc_core::shown_id::shown_form(existing),
                    existing
                )));
            }
        }
        let submission_id = p.submission_id.unwrap_or_else(Uuid::new_v4); // unwrap_or: minted here when she named none — the id is ours to give
        let candidate = airc_work::WorkSubmission {
            submission_id: airc_work::SubmissionId::from_uuid(submission_id),
            card_id,
            claim_id,
            instance,
            base_sha,
            artifact,
            publisher: airc.peer_id(),
            submitted_at_ms: continuum_base::time::EpochMs::now().millis(),
        };
        candidate
            .validate_for_card(card)
            .map_err(|e| CommandError::Invalid(format!("submission refused: {e}")))?;
        if p.staged_revision_id.is_some() && p.generation_request_id.is_some() {
            return Err(CommandError::Invalid(
                "select staged_revision_id or generation_request_id, not both".into(),
            ));
        }
        let mut bound_staged_revision_id = None;
        if p.staged_revision_id.is_some() || p.generation_request_id.is_some() {
            if self.registry.get(runtime.persona_id()).is_none()
                || runtime.persona_id() != airc.peer_id().as_uuid()
            {
                return Err(CommandError::Invalid(
                    "selected credit must belong to the calling resident persona".into(),
                ));
            }
            let conn = credit_connection(self.executor_slot.cloned(), runtime.persona_id())?;
            let binding = if let Some(generation) = p.generation_request_id.as_deref() {
                reviewed::bind_submission_generation(
                    &conn,
                    runtime.agent_name(),
                    runtime.persona_id(),
                    room.channel.as_uuid(),
                    &candidate,
                    generation,
                )
                .await
            } else if let Some(revision) = p.staged_revision_id {
                reviewed::bind_submission(
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
                .await
            } else {
                unreachable!("credit selection checked above")
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
        let bench = crate::recipe::benchmark_catalog::graded_at_submit(&grade_title);
        let grade_patch = candidate.artifact.clone();
        let published = airc.submit_work_in(&room, airc_lib::SubmitWork {
            submission_id: candidate.submission_id, card_id, claim_id: candidate.claim_id,
            instance: candidate.instance, base_sha: candidate.base_sha, artifact: candidate.artifact,
        }).await.map_err(|e| CommandError::Internal(format!(
            "submission {submission_id} publication was not acknowledged; retry with submission_id={submission_id} and the same selection: {e}")))?;
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
        let outcomes: Vec<_> = board.submission_reviews_for(published.submission_id).filter(|review| review.card_id == card_id).map(|review| review.outcome).collect();
        Ok(WorkSubmitResult {
            submission_id: published.submission_id.as_uuid(),
            card_id: published.card_id.as_uuid(),
            room_id: room.channel.as_uuid(),
            publisher: published.publisher.as_uuid(),
            claim_id: published.claim_id.as_uuid(),
            base_sha: published.base_sha.to_string(),
            instance: published.instance.clone(),
            artifact: (&published.artifact).into(),
            submitted_at_ms: published.submitted_at_ms,
            bound_staged_revision_id,
            next: match handoff {
                Ok(review) => next_after_submission(Some(published.submission_id.as_uuid()), &outcomes, bench, review).unwrap_or_default(), // unwrap_or_default: Some(..) in, so always Some
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

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string")]
    pub review_id: Option<Uuid>,
    /// Parent card (handle or UUID); defaults to review parent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string")]
    pub card_id: Option<String>,
    /// Submission; defaults to latest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string")]
    pub submission_id: Option<Uuid>,
    /// Artifact; defaults to board.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub artifact: Option<WorkArtifactReference>,
    /// Review claim; defaults to board.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string")]
    pub review_claim_id: Option<Uuid>,
    /// Typed evidence instead of evidence_text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub evidence: Option<WorkArtifactReference>,
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
        let card_id = match p.card_id.as_deref() {
            Some(c) => WorkCardId::from_uuid(resolve_shown(c, &shown, "card")?),
            None => review_card.reviews.ok_or_else(|| {
                CommandError::Invalid(
                    "the card you named is not a review card (it reviews nothing) — pass card_id if you are reviewing out of band"
                        .to_string(),
                )
            })?,
        };
        let review_claim_id = match p.review_claim_id {
            Some(c) => ClaimId::from_uuid(c),
            None => match (review_card.owner, review_card.claim_id) {
                (Some(owner), Some(claim)) if owner == airc.peer_id() => claim,
                (Some(_), _) => {
                    return Err(CommandError::Invalid(
                        "the review card you named is held by another peer, not by you — only its holder reviews".to_string(),
                    ))
                }
                _ => {
                    return Err(CommandError::Invalid(
                        "the review card you named is not claimed — claim it (work/claim) before reviewing".to_string(),
                    ))
                }
            },
        };
        if review_card.reviews != Some(card_id) {
            return Err(CommandError::Invalid("review card does not review the named parent".into()));
        }
        let submission = board.candidate_for_review(review_card.card_id, p.submission_id.map(airc_work::SubmissionId::from_uuid))
            .map_err(|e| CommandError::Invalid(format!("review candidate: {e}")))?;
        // SELF-REVIEW IS THE ROOM'S CALL (card fa4aaaaa). The author reviewing her own
        // submission is admitted only where the room's policy says `self`; a room that
        // declares no policy keeps today's behaviour. Refused up front, by name, rather
        // than counted as nothing after the fact.
        if submission.publisher == airc.peer_id() {
            let policy = crate::persona::work_board::review::room_policy(airc, &room).await;
            own_review_publishable(&policy, &card_id)?;
        }
        let artifact = match p.artifact.clone() {
            Some(a) => a.into_artifact()?,
            None => submission.artifact.clone(),
        };
        let evidence = match (p.evidence.clone(), p.evidence_text.as_deref()) {
            (Some(e), _) => e.into_artifact()?,
            (None, Some(text)) if !text.trim().is_empty() => artifact_of_text(text).into_artifact()?,
            _ => {
                return Err(CommandError::Invalid(
                    "a review carries evidence: pass evidence_text (what you ran and saw) — a verdict without evidence is not a verdict".into(),
                ))
            }
        };
        let review_id = p.review_id.unwrap_or_else(Uuid::new_v4); // unwrap_or: minted here when she named none — the id is ours to give
        continuum_base::probe!(
            class = "work.review.shaped",
            review_card = %airc_core::shown_id::shown_form(review_uuid),
            card = %card_id,
            submission = %submission.submission_id,
            derived = p.submission_id.is_none() || p.artifact.is_none() || p.review_claim_id.is_none() || p.card_id.is_none(),
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
                    evidence_text: p.evidence_text.clone(),
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

/// May the author PUBLISH a review of her own submission, given the ONE policy read the
/// command took? A declared `self: false` refuses; a policy that could not be read refuses
/// (it was admitted on a read failure); a room that declares none keeps today's behaviour.
/// Whether that review then earns credit is the exact path's `SelfReview`, a different question.
pub(super) fn own_review_publishable(
    policy: &crate::persona::work_board::review::PolicyRead,
    card_id: &WorkCardId,
) -> Result<(), CommandError> {
    use crate::persona::work_board::review::PolicyRead;
    match policy {
        PolicyRead::Absent => Ok(()),
        PolicyRead::Declared(policy) if policy.self_review => Ok(()),
        PolicyRead::Declared(_) => Err(CommandError::Invalid(format!(
            "this room's review policy does not admit the author's own review (review.self is false): \
             a member other than you reviews card {card_id}. A room can allow it: \
             activity/spawn --params '{{\"review\":{{\"self\":true}}}}'."
        ))),
        PolicyRead::Unavailable(why) => Err(CommandError::Invalid(format!(
            "this room's review policy could not be read ({why}), so your own review of card {card_id} \
             cannot be admitted now: retry, or have a member other than you review it."
        ))),
    }
}

pub struct WorkSubmission {
    pub registry: PersonaAircRuntimeRegistry,
    pub executor_slot: Arc<LateBound<CommandExecutor>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/work/WorkSubmissionParams.ts"
)]
pub struct WorkSubmissionParams {
    /// Optional; the card's board decides.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub room: Option<String>,
    /// Parent card — board handle or full UUID.
    #[ts(type = "string")]
    pub card_id: String,
    /// The submission to read — handle or full UUID.
    #[ts(type = "string")]
    pub submission_id: String,
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
