use super::*;

#[async_trait]
impl ActionCommand for WorkSubmission {
    const NAME: &'static str = "work/submission";
    // In hand when her held card puts it there (a review card, or her card once it carries a
    // submission: `work_preconditions::in_hand`), one `commands/help` away otherwise.
    const DISCLOSURE: continuum_base::tool_disclosure::Disclosure = continuum_base::tool_disclosure::Disclosure::OnDemand;
    const NATIVE: bool = true;
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str = "Read submission_id. Task: work/get. Publish: work/submit.";
    type Params = WorkSubmissionParams;
    type Output = WorkSubmissionResult;

    async fn run(&self, ctx: &Ctx, p: WorkSubmissionParams) -> Result<Self::Output, CommandError> {
        let runtime = persona_runtime(&self.registry, ctx, "work/submission")?;
        let evidence_request = EvidenceRequest::from_params(p.evidence_rows, p.evidence_offset, p.evidence_limit);
        let room = room_of_card(runtime.airc(), p.room.as_deref(), &p.card_id, "work/submission").await?;
        let board = runtime
            .airc()
            .work_board_in(&room)
            .await
            .map_err(|e| CommandError::Internal(e.to_string()))?;
        // ROOM-SCOPED for the card, then CARD-SCOPED for the submission — each handle is
        // resolved against the narrowest population that can contain it, so an ambiguous
        // prefix names the candidates it actually collided with.
        let snapshot = board.snapshot();
        let card_uuid = resolve_shown(
            &p.card_id,
            &snapshot.cards.iter().map(|c| c.card_id.as_uuid()).collect::<Vec<_>>(),
            "card",
        )?;
        let card = board.card(WorkCardId::from_uuid(card_uuid)).ok_or_else(|| {
            CommandError::NotFound(format!(
                "card {card_uuid} is absent from the board of room {} — to read a card use \
                 work/get, to publish your own work use work/submit",
                room_label(&room.name, room.channel.as_uuid()),
            ))
        })?;
        // The one handle resolver: `latest` (the default) is the card's newest, a review card
        // reads what it reviews, and a card with none is refused with the call that makes one.
        let actor = runtime.airc().peer_id();
        let now_ms = EpochMs::now().millis();
        let (submitted, card) = match p.submission_id.read(&board, card) {
            Ok(SubmissionRead::Found { submission, on }) => (submission, on),
            Ok(SubmissionRead::NoneYet { on }) => {
                return Err(submission_refused(SubmissionRefusal::NoneYet { card: on.card_id }, &p.submission_id, card.card_id, on, actor, now_ms))
            }
            Err(unread) => return Err(submission_refused(SubmissionRefusal::Unread(unread), &p.submission_id, card.card_id, card, actor, now_ms)),
        };
        let submission_uuid = submitted.submission_id.as_uuid();
        let card_uuid = card.card_id.as_uuid();
        let mut staged_evidence = None;
        let mut staged_evidence_error = None;
        let credit = async {
            let Some(owner) = self.registry.get(submitted.publisher.as_uuid()) else {
                if p.include_staged_evidence {
                    staged_evidence_error = Some("publisher is not resident on this node".into());
                }
                return Ok(reviewed::ReviewedCredit::pending(
                    reviewed::ReviewedCreditState::PublisherNotResident,
                ));
            };
            if owner.airc().peer_id() != submitted.publisher {
                return Err(CommandError::Invalid(
                    "registered publisher identity differs".into(),
                ));
            }
            let conn = credit_connection(self.executor_slot.cloned(), owner.persona_id())?;
            if p.include_staged_evidence {
                match reviewed::staged_evidence(&conn, owner.agent_name(), submitted).await {
                    Ok(rows) => match StagedEvidencePage::answer(rows, evidence_request) {
                        Ok(page) => staged_evidence = Some(page),
                        Err(error) => staged_evidence_error = Some(error.to_string()),
                    },
                    Err(error) => staged_evidence_error = Some(error.to_string()),
                }
            }
            reviewed::credit_status(&conn, owner.agent_name(), submission_uuid)
                .await
                .map_err(|e| CommandError::Internal(e.to_string()))
        }
        .await;
        let (credit, credit_error) = match credit {
            Ok(value) => (Some(value), None),
            Err(e) => (None, Some(e.to_string())),
        };
        let outcomes: Vec<_> = board
            .submission_reviews_for(submitted.submission_id)
            .filter(|r| r.card_id == submitted.card_id)
            .map(|r| r.outcome)
            .collect();
        let reviews = board
            .submission_reviews_for(submitted.submission_id)
            .filter(|r| r.card_id == submitted.card_id)
            .map(WorkReviewResult::from)
            .collect();
        let (patch, patch_error) = match crate::media::artifact::read_text_blob(&submitted.artifact) {
            Ok(text) => (Some(text), None),
            Err(why) => (None, Some(why)),
        };
        Ok(WorkSubmissionResult {
            submission: WorkSubmitResult {
                outcome: SubmitOutcome::Published,
                submission_id: submission_uuid,
                card_id: card_uuid,
                room_id: room.channel.as_uuid(),
                publisher: submitted.publisher.as_uuid(),
                claim_id: submitted.claim_id.as_uuid(),
                base_sha: submitted.base_sha.to_string(),
                instance: submitted.instance.clone(),
                artifact: (&submitted.artifact).into(),
                submitted_at_ms: submitted.submitted_at_ms,
                bound_staged_revision_id: credit.as_ref().and_then(|c| c.staged_revision_id),
                next: next_after_submission(Some(submission_uuid), &outcomes, crate::recipe::benchmark_catalog::graded_at_submit(&card.title), submission_review_handoff(&board, card.card_id, Some(submitted.submission_id)), card.pull_request.as_ref()).unwrap_or_default(), // unwrap_or_default: Some(..) in, so always Some
            },
            claim_id: submitted.claim_id.as_uuid(),
            base_sha: submitted.base_sha.to_string(),
            instance: submitted.instance.clone(),
            reviews,
            credit,
            credit_error,
            staged_evidence,
            staged_evidence_error,
            patch,
            patch_error,
        })
    }
}

crate::register_command!(WorkSubmission);
