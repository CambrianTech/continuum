#![cfg(test)]
use super::*;

    /// what this catches: a staged example that is not the call she lived. The read
    /// must return the exact request she was served and the response she gave, with
    /// her reasoning and tool calls; a turn with any call that cannot be read back must
    /// say so rather than stage as complete; and each settled example must carry its
    /// call and the card stamp (card ad107e18).
    #[test]
    pub(super) fn a_staged_turn_is_read_back_as_the_calls_she_lived_and_a_hole_is_named() {
        use crate::ai::types::{ChatMessage, TextGenerationRequest, TextGenerationResponse};
        use crate::cognition::prompt_capture::{CaptureLease, JsonlPromptCaptureSink, PromptCall};
        let dir = tempfile::tempdir().expect("capture dir");
        let persona = Uuid::new_v4();
        let request = TextGenerationRequest {
            system_prompt: Some("her identity".into()),
            messages: vec![
                ChatMessage::text("user", "room: the build is red on lib"),
                ChatMessage::text("user", "card: fix the failing test"),
            ],
            ..Default::default()
        };
        let response = TextGenerationResponse {
            text: "reading the failing test first".into(),
            finish_reason: crate::ai::FinishReason::ToolUse,
            model: "served-model".into(),
            provider: "fixture-provider".into(),
            usage: crate::ai::UsageMetrics::default(),
            response_time_ms: 0,
            request_id: "req-lived".into(),
            content: None,
            tool_calls: Some(vec![crate::ai::ToolCall {
                id: "t1".into(),
                name: "code/read".into(),
                input: json!({"path": "src/lib.rs"}),
            }]),
            reasoning: Some("the assertion names a path; read it before guessing".into()),
            routing: None,
            error: None,
            timing: None,
        };
        let sink = JsonlPromptCaptureSink::open(dir.path(), persona).expect("sink");
        let call = PromptCall {
            request_id: "req-lived".into(),
            persona_id: persona,
            room_id: Uuid::new_v4(),
            cycle_id: Some(1),
            context_window: Some(32_768),
            cause: "synthetic",
            cause_root: None,
            replay_of: None,
        };
        let mut lease = CaptureLease::start(Arc::new(sink), &call, &request);
        let submitted = lease.cursor().expect("durable submission").to_owned();
        // the receipt carries the COMPLETED entry's cursor: the submission holds no response
        let cursor = lease.finish(Some(&response), None).expect("completed entry");
        assert_ne!(cursor, submitted);
        let (_, only_request) =
            read_lived_calls(Some(dir.path()), persona, vec![Some(submitted)], Vec::new());
        assert!(only_request.is_some_and(|r| r.contains("no terminal response")));

        let (calls, missing) =
            read_lived_calls(Some(dir.path()), persona, vec![Some(cursor.clone())], Vec::new());
        assert_eq!(missing, None);
        assert_eq!(calls.len(), 1);
        let lived = &calls[0];
        assert_eq!(lived.capture, cursor, "the example links back to its trace");
        assert_eq!(
            serde_json::to_value(&lived.request.messages).unwrap(),
            serde_json::to_value(&request.messages).unwrap(),
            "the example is the request she was served, not a re-derivation"
        );
        assert_eq!(lived.response.reasoning, response.reasoning, "her thinking stays");
        assert_eq!(lived.response.tool_calls.as_ref().map(Vec::len), Some(1));

        // A second pass reuses what the turn already read and reads nothing again.
        let (again, missing) =
            read_lived_calls(None, persona, vec![Some(cursor.clone())], calls.clone());
        assert_eq!((again.len(), missing), (1, None));

        // A hole anywhere in the turn is named, never staged as complete.
        let (_, hole) = read_lived_calls(Some(dir.path()), persona, vec![Some(cursor), None], Vec::new());
        assert!(hole.is_some_and(|reason| reason.contains("no capture cursor")));

        let stamp = OutcomeStamp {
            card_id: Uuid::new_v4(),
            role: CreditRole::Owner,
            outcome: true,
        };
        let plan = SubmitPlan {
            trait_kind: "code".into(),
            prompt: "stimulus".into(),
            completion: "acted chain".into(),
            quality: 1.0,
            stamp: Some(stamp),
        };
        let examples = lived_examples(&plan, &calls);
        assert_eq!(examples.len(), 1);
        assert_eq!(examples[0].completion, "reading the failing test first");
        assert_eq!(examples[0].prompt, "card: fix the failing test");
        assert!(examples[0].lived.is_some());
        assert_eq!(
            examples[0].metadata.as_ref().unwrap()["cardId"],
            json!(stamp.card_id)
        );
    }

    pub(super) fn served_receipt(id: &str, model: &str) -> crate::cognition::provenance::GenerationReceipt {
        crate::cognition::provenance::GenerationReceipt {
            submitted_request_id: id.into(),
            outcome: crate::cognition::provenance::GenerationOutcome::Served {
                model: model.into(),
                provider: "fixture-provider".into(),
                provider_request_id: None,
            },
            capture: None,
            genes: Vec::new(),
        }
    }

    pub(super) async fn stage_settlement_fixture(
        executor: Arc<CommandExecutor>,
        persona: Uuid,
        name: &str,
        card: Uuid,
        receipts: Vec<crate::cognition::provenance::GenerationReceipt>,
    ) -> (TurnCreditCapture, StagedCredit) {
        let mut capture = TurnCreditCapture::with_executor(
            executor.clone(),
            persona,
            name.into(),
            "Inspect the Rust source and report the verified regression behavior.".into(),
            CapturedCredit {
                task: crate::cognition::held_out::CardTask::NotATask,
                card_id: card,
                claim: Some(ClaimReceipt {
                    claim_id: Uuid::new_v4(),
                    owner: airc_core::PeerId::from_uuid(persona),
                    role: CreditRole::Owner,
                }),
            },
        );
        assert!(
            capture
                .record(&settlement_acts(4), &receipts, None, false)
                .await
        );
        let row = stored_credit_rows(&conn_as(executor, persona), name)
            .await
            .into_iter()
            .find(|row| row.receipts == receipts)
            .unwrap();
        (capture, row)
    }

    pub(super) fn script_settlement_reply(
        submit: &continuum_client::mock::MockTransport,
        row: &StagedCredit,
        reply: Option<serde_json::Value>,
    ) {
        let id = row.id;
        let model = row.served.as_ref().unwrap().model.clone();
        let completion = row.completion.clone();
        submit.respond_to("genome/training-trigger/submit", move |params| {
            assert_eq!(
                params["submissionId"],
                json!(id),
                "retry carries this immutable snapshot's identity"
            );
            assert_eq!(
                params["baseModel"],
                json!(model),
                "each row supplies its actual serving model"
            );
            assert_eq!(params["examples"][0]["completion"], json!(completion));
            match &reply {
                Some(value) => Ok(value.clone()),
                None => Err(ClientError::Transport(
                    "acceptance acknowledgement was lost".into(),
                )),
            }
        });
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
        let short = |id: Uuid| id.simple().to_string().chars().take(8).collect::<String>();
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
            "card_id": short(card.as_uuid()),
            "claim_id": short(claim.as_uuid()),
            "instance": "generic-project-work",
            "base_sha": "a".repeat(40),
            "artifact": { "hash": patch_hash, "size_bytes": patch.len(), "mime": "text/x-diff" },
            "staged_revision_id": selected.id.to_string(),
        }))
        .expect("a citizen's call carries the handles her board printed"); // expect: the fixture's own json, asserted decodable
        assert_eq!(
            params.card_id,
            short(card.as_uuid()),
            "the handle survives deserialization — it used to be refused here"
        );
        let submit = WorkSubmit {
            registry: registry.clone(),
            executor_slot: slot.clone(),
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
                "card_id": short(card.as_uuid()),
                "submission_id": short(
                    params.submission_id.expect("the fixture names its submission"), // expect: set in the json above
                ),
                "include_staged_evidence": true,
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
        let evidence = inspected.staged_evidence.as_ref().unwrap();
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
        let public_evidence = serde_json::to_string(evidence).unwrap();
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

    /// The exact-review scenario: her staged row bound to one submission on one card, a
    /// reviewer, and the room's work events up to the review. Shared by every exact-path
    /// test, so each states only its own case (CLAUDE.md: one fixture per concern).
    pub(super) struct ExactReview {
        pub(super) _home: (HomeGuard, tempfile::TempDir),
        pub(super) executor: Arc<CommandExecutor>,
        pub(super) persona: Uuid,
        pub(super) name: &'static str,
        pub(super) owner: airc_core::PeerId,
        pub(super) reviewer: airc_core::PeerId,
        pub(super) card_id: airc_work::WorkCardId,
        pub(super) room: Uuid,
        pub(super) receipts: Vec<crate::cognition::provenance::GenerationReceipt>,
        pub(super) selected: StagedCredit,
        pub(super) selection: reviewed::SubmissionSelection,
        pub(super) data: Connection<InProcessTransport>,
        pub(super) repo: airc_work::RepoId,
        pub(super) pass: airc_work::WorkSubmissionReview,
        pub(super) prefix: Vec<(airc_work::WorkEvent, airc_core::PeerId)>,
    }

    impl ExactReview {
        pub(super) fn create(&self, card_id: airc_work::WorkCardId, reviews: Option<airc_work::WorkCardId>) -> airc_work::WorkEvent {
            airc_work::WorkEvent::CardCreated(airc_work::CardCreated {
                card_id,
                repo: self.repo.clone(),
                title: "ordinary work with reviewed deliverable".into(),
                body: None,
                priority: airc_work::Priority::P1,
                lane_id: None,
                created_by: self.owner,
                created_at_ms: 1,
                reviews,
                origin: None,
            })
        }

        pub(super) fn claim(card_id: airc_work::WorkCardId, claim_id: airc_work::ClaimId, holder: airc_core::PeerId) -> airc_work::WorkEvent {
            airc_work::WorkEvent::CardClaimed(airc_work::WorkCardClaimed {
                selected_at_ms: None,
                origin: airc_work::ClaimOrigin::Unknown,
                card_id,
                claim_id,
                owner: holder,
                ttl_ms: 1000,
                claimed_at_ms: 2,
            })
        }

        /// The room's board after the scenario's events plus `tail`, through the existing
        /// codec and replay boundary, including the authoritative transcript peer.
        pub(super) fn project(&self, tail: Vec<(airc_work::WorkEvent, airc_core::PeerId)>) -> airc_work::WorkBoardProjection {
            airc_work::project_transcript_work_events(
                self.prefix.iter().cloned().chain(tail).enumerate().map(|(n, (event, peer_id))| {
                    let (headers, body) = airc_work::encode_work_event(&event).unwrap();
                    airc_core::TranscriptEvent {
                        event_id: airc_core::EventId::new(),
                        room_id: airc_core::RoomId::from_uuid(self.room),
                        peer_id,
                        client_id: airc_core::ClientId::new(),
                        kind: airc_core::TranscriptKind::System,
                        occurred_at_ms: event.occurred_at_ms(),
                        lamport: n as u64 + 1,
                        target: airc_core::MentionTarget::All,
                        headers,
                        body: Some(body),
                        attachment: None,
                        receipt: None,
                        metadata: serde_json::Value::Null,
                    }
                }),
            )
            .unwrap()
        }
    }

    pub(super) async fn exact_review() -> (ExactReview, TurnCreditCapture) {
        use airc_work::{WorkEvent, WorkReviewOutcome, WorkSubmissionReview};
        let home = tempfile::tempdir().unwrap();
        let guard = HomeGuard::set(home.path()).await;
        crate::persona::register_substrate_orm_entities(crate::orm::OrmEntityRegistry::global())
            .unwrap();
        let executor = data_runtime();
        let persona = Uuid::new_v4();
        let owner = airc_core::PeerId::from_uuid(persona);
        let reviewer = airc_core::PeerId::new();
        let card_id = airc_work::WorkCardId::new();
        let room = Uuid::new_v4();
        let name = "reviewed-project-work";
        let receipts = vec![served_receipt("reviewed-request", "actual-model")];
        let (capture, selected) =
            stage_settlement_fixture(executor.clone(), persona, name, card_id.as_uuid(), receipts.clone()).await;
        let selection = reviewed::SubmissionSelection {
            submission_id: Uuid::new_v4(),
            room_id: room,
            card_id: card_id.as_uuid(),
            claim_id: selected.claim_id.unwrap(),
            staged_revision_id: selected.id,
            instance: "ordinary-project-work".into(),
            base_sha: airc_work::GitObjectId::new("a".repeat(40)).unwrap(),
            artifact: serde_json::from_value(json!({"hash":"b".repeat(64),"size_bytes":12})).unwrap(),
        };
        let data = conn_as(executor.clone(), persona);
        reviewed::bind_submission(&data, name, persona, selection.clone()).await.unwrap();
        let submitted = airc_work::WorkSubmission {
            submission_id: airc_work::SubmissionId::from_uuid(selection.submission_id),
            card_id,
            claim_id: airc_work::ClaimId::from_uuid(selection.claim_id),
            instance: selection.instance.clone(),
            base_sha: selection.base_sha.clone(),
            artifact: selection.artifact.clone(),
            publisher: owner,
            submitted_at_ms: 3,
        };
        let review_card = airc_work::WorkCardId::new();
        let review_claim = airc_work::ClaimId::new();
        let pass = WorkSubmissionReview {
            review_id: airc_work::WorkReviewId::new(),
            card_id,
            submission_id: submitted.submission_id,
            artifact: submitted.artifact.clone(),
            review_card_id: review_card,
            review_claim_id: review_claim,
            reviewer,
            outcome: WorkReviewOutcome::Passed,
            evidence: submitted.artifact.clone(),
            evidence_text: None,
            reviewed_at_ms: 4,
        };
        let mut fx = ExactReview {
            _home: (guard, home),
            executor,
            persona,
            name,
            owner,
            reviewer,
            card_id,
            room,
            receipts,
            selected,
            selection,
            data,
            repo: airc_work::RepoId::new("acme/ordinary-project").unwrap(),
            pass,
            prefix: Vec::new(),
        };
        fx.prefix = vec![
            (fx.create(card_id, None), owner),
            (ExactReview::claim(card_id, submitted.claim_id, owner), owner),
            (WorkEvent::WorkSubmitted(submitted), owner),
            (fx.create(review_card, Some(card_id)), reviewer),
            (ExactReview::claim(review_card, review_claim, reviewer), reviewer),
        ];
        (fx, capture)
    }
