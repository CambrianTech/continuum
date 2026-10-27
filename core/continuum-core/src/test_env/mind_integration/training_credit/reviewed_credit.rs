#![cfg(test)]
use super::*;

    // what this catches (Codex root, #5017 review): where the room's policy declares
    // `self: true`, her own review of her own submission transfers HER exact bound row,
    // under her review's id, once; an exact replay does not dispatch again.
    #[tokio::test]
    pub(super) async fn a_permitted_self_review_transfers_her_exact_row_once() {
        use airc_work::{WorkEvent, WorkSubmissionReview};
        use reviewed::ReviewedCreditState as State;
        let (fx, _capture) = exact_review().await;
        let (executor, persona, name, owner, card_id, room) = (fx.executor.clone(), fx.persona, fx.name, fx.owner, fx.card_id, fx.room);
        let (selected, pass) = (fx.selected.clone(), fx.pass.clone());
        let create = |card, reviews| fx.create(card, reviews);
        let claim = ExactReview::claim;
        let project = |tail| fx.project(tail);
        let self_card = airc_work::WorkCardId::new();
        let self_claim = airc_work::ClaimId::new();
        let own = WorkSubmissionReview {
            review_card_id: self_card,
            review_claim_id: self_claim,
            reviewer: owner,
            ..pass.clone()
        };
        let board = project(vec![
            (create(self_card, Some(card_id)), owner),
            (claim(self_card, self_claim, owner), owner),
            (WorkEvent::WorkSubmissionReviewed(own.clone()), owner),
        ]);
        let submit = continuum_client::mock::MockTransport::new();
        script_settlement_reply(
            &submit,
            &selected,
            Some(json!({"success":true,"acceptance":{"submissionId":selected.id,"replayed":false}})),
        );
        let conn = settlement_connection(executor.clone(), persona, submit, None);
        let result = reviewed::consume_review_with_boundary(
            &conn,
            name,
            persona,
            room,
            &board,
            own.review_id,
            &reviewed::SelfReview::Admitted,
            None,
        )
        .await
        .unwrap();
        assert_eq!(result.state, State::Accepted, "an admitted self-review settles her row");
        assert_eq!(result.decision_review_id, Some(own.review_id.as_uuid()));
        let never_submit = continuum_client::mock::MockTransport::new();
        never_submit.respond_to("genome/training-trigger/submit", |_| {
            panic!("an accepted binding must not dispatch again")
        });
        let replay = settlement_connection(executor.clone(), persona, never_submit, None);
        assert_eq!(
            reviewed::consume_review_with_boundary(
                &replay,
                name,
                persona,
                room,
                &board,
                own.review_id,
                &reviewed::SelfReview::Admitted,
                None,
            )
            .await
            .unwrap()
            .state,
            State::Accepted
        );
    }

    // what this catches: 6c36c24d — only an accepted independent exact-artifact
    // review may transfer a bound snapshot; lost ACK/replacement/reopen cannot
    // relabel the evidence or submit it under a fresh destination identity.
    #[tokio::test]
    pub(super) async fn reviewed_credit_replays_exact_authority_and_recovers_after_replacement() {
        use airc_work::{WorkEvent, WorkReviewOutcome, WorkSubmissionReview};
        use reviewed::ReviewedCreditState as State;
        use crate::cognition::dream_consolidation::test_support::{
            drain, episodic, region_over_with_store, seeded_admission,
        };
        use crate::runtime::brain_region::{BrainRegion, RegionContext};
        let (fx, mut capture) = exact_review().await;
        let (executor, persona, name, owner, reviewer, card_id, room) =
            (fx.executor.clone(), fx.persona, fx.name, fx.owner, fx.reviewer, fx.card_id, fx.room);
        let (receipts, selected, selection, pass) = (fx.receipts.clone(), fx.selected.clone(), fx.selection.clone(), fx.pass.clone());
        let data = &fx.data;
        let admission = seeded_admission(
            &(0..3)
                .map(|n| {
                    episodic(
                        Uuid::new_v4(),
                        &format!("ordinary reviewed work {n}"),
                        &["reviewed-project"],
                    )
                })
                .collect::<Vec<_>>(),
        );
        let store =
            reviewed::boundaries::ReviewBoundaryStore::new(executor.clone(), name.into(), persona);
        let dream = region_over_with_store(persona, admission.clone(), Some(Arc::new(store.clone())));
        let create = |card, reviews| fx.create(card, reviews);
        let claim = ExactReview::claim;
        let project = |tail| fx.project(tail);
        let never_submit = continuum_client::mock::MockTransport::new();
        never_submit.respond_to("genome/training-trigger/submit", |_| {
            panic!("unaccepted/failed/unknown review must not submit")
        });
        let refused_conn = settlement_connection(executor.clone(), persona, never_submit, None);
        for outcome in [WorkReviewOutcome::Failed, WorkReviewOutcome::Unknown] {
            let review = WorkSubmissionReview {
                outcome,
                ..pass.clone()
            };
            let board = project(vec![(WorkEvent::WorkSubmissionReviewed(review), reviewer)]);
            let result = reviewed::consume_review_with_boundary(
                &refused_conn,
                name,
                persona,
                room,
                &board,
                pass.review_id,
                &reviewed::SelfReview::NotAdmitted,
                Some(&dream),
            )
            .await
            .unwrap();
            assert_eq!(
                result.state,
                if outcome == WorkReviewOutcome::Failed {
                    State::FailedReview
                } else {
                    State::UnknownReview
                }
            );
        }
        let self_card = airc_work::WorkCardId::new();
        let self_claim = airc_work::ClaimId::new();
        let self_review = WorkSubmissionReview {
            review_card_id: self_card,
            review_claim_id: self_claim,
            reviewer: owner,
            ..pass.clone()
        };
        let self_board = project(vec![
            (create(self_card, Some(card_id)), owner),
            (claim(self_card, self_claim, owner), owner),
            (WorkEvent::WorkSubmissionReviewed(self_review), owner),
        ]);
        assert_eq!(
            reviewed::consume_review_with_boundary(
                &refused_conn,
                name,
                persona,
                room,
                &self_board,
                pass.review_id,
                &reviewed::SelfReview::NotAdmitted,
                Some(&dream)
            )
            .await
            .unwrap()
            .state,
            State::IndependentReviewRequired,
            "signed self-review remains visible without positive learning credit"
        );
        // what this catches (Codex root, #5017 review): an unreadable room policy read as
        // admission. It settles nothing and records nothing, so a later replay decides it.
        assert_eq!(
            reviewed::consume_review_with_boundary(
                &refused_conn,
                name,
                persona,
                room,
                &self_board,
                pass.review_id,
                &reviewed::SelfReview::Unknown("wall unreachable".into()),
                Some(&dream)
            )
            .await
            .unwrap()
            .state,
            State::PolicyUnavailable,
            "an unreadable policy neither admits nor refuses her own review"
        );
        let spoofed = project(vec![(
            WorkEvent::WorkSubmissionReviewed(pass.clone()),
            owner,
        )]);
        assert!(reviewed::consume_review_with_boundary(
            &refused_conn,
            name,
            persona,
            room,
            &spoofed,
            pass.review_id,
            &reviewed::SelfReview::NotAdmitted,
            Some(&dream)
        )
        .await
        .is_err());
        let board = project(vec![(
            WorkEvent::WorkSubmissionReviewed(pass.clone()),
            reviewer,
        )]);
        let unrelated_transport = continuum_client::mock::MockTransport::new();
        unrelated_transport.respond_to("data/ensure-schema", |_| {
            panic!("unrelated observer must not open credit storage")
        });
        unrelated_transport.respond_to("genome/training-trigger/submit", |_| {
            panic!("unrelated observer must not transfer publisher credit")
        });
        let unrelated = Connection::new(unrelated_transport);
        assert!(reviewed::consume_observed_review_with_boundary(
            &unrelated,
            "unrelated-resident",
            reviewer.as_uuid(),
            room,
            &board,
            pass.review_id,
            &reviewed::SelfReview::NotAdmitted,
            Some(&dream)
        )
        .await
        .unwrap()
        .is_none());
        assert!(reviewed::consume_review_with_boundary(
            &refused_conn,
            name,
            persona,
            Uuid::new_v4(),
            &board,
            pass.review_id,
            &reviewed::SelfReview::NotAdmitted,
            Some(&dream)
        )
        .await
        .is_err());
        assert!(
            !dream.review_boundary_pending(persona, selection.submission_id),
            "failed, unknown, self, unrelated and wrong-room reviews cannot queue a boundary"
        );
        let submit = continuum_client::mock::MockTransport::new();
        let selected_id = selected.id;
        let review_id = pass.review_id.as_uuid();
        submit.respond_to("genome/training-trigger/submit", move |params| {
            assert_eq!(params["submissionId"], json!(selected_id));
            assert_eq!(params["baseModel"], "actual-model");
            assert_eq!(
                params["examples"][0]["metadata"]["reviewedSubmission"]["review"]["review_id"],
                json!(review_id)
            );
            // what this catches (card 04a5e867): the exact path's examples carry the card
            // beside the review, so `genome/training-trigger/retire --card` finds them.
            assert_eq!(
                params["examples"][0]["metadata"]["cardId"],
                json!(card_id.as_uuid())
            );
            Err(ClientError::Transport("destination ACK lost".into()))
        });
        let conn = settlement_connection(executor.clone(), persona, submit, None);
        assert!(reviewed::consume_observed_review_with_boundary(
            &conn,
            name,
            persona,
            room,
            &board,
            pass.review_id,
            &reviewed::SelfReview::NotAdmitted,
            Some(&dream)
        )
        .await
        .is_err());
        let pending = reviewed::credit_status(data, name, selection.submission_id)
            .await
            .unwrap();
        assert_eq!(pending.state, State::AwaitingAcceptance);
        assert!(
            !dream.review_boundary_pending(persona, selection.submission_id),
            "lost destination ACK does not acknowledge a consolidation boundary"
        );
        assert_eq!(pending.decision_review_id, Some(review_id));
        let mut later = receipts;
        later.push(captured(persona, served_receipt("later-request", "actual-model")));
        assert!(
            capture
                .record(&settlement_acts(8), &later, None, true)
                .await
        );
        assert!(
            reviewed::read_one::<_, StagedCredit>(data, name, &selected.id.to_string())
                .await
                .unwrap()
                .is_none()
        );
        let competing = WorkSubmissionReview {
            review_id: airc_work::WorkReviewId::new(),
            ..pass.clone()
        };
        let board = project(vec![
            (WorkEvent::WorkSubmissionReviewed(pass.clone()), reviewer),
            (
                WorkEvent::WorkSubmissionReviewed(competing.clone()),
                reviewer,
            ),
        ]);
        assert_eq!(
            reviewed::consume_review_with_boundary(
                &refused_conn,
                name,
                persona,
                room,
                &board,
                competing.review_id,
                &reviewed::SelfReview::NotAdmitted,
                Some(&dream)
            )
            .await
            .unwrap()
            .state,
            State::AnotherReviewSelected
        );
        let submit = continuum_client::mock::MockTransport::new();
        script_settlement_reply(
            &submit,
            &selected,
            Some(json!({"success":false,"errorKind":"DispatchFailed",
            "acceptance":{"submissionId":selected.id,"replayed":true}})),
        );
        let reopened = settlement_connection(data_runtime(), persona, submit, None);
        let result = reviewed::consume_observed_review_with_boundary(
            &reopened,
            name,
            persona,
            room,
            &board,
            pass.review_id,
            &reviewed::SelfReview::NotAdmitted,
            Some(&dream),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(
            result.state,
            State::Accepted,
            "durable acceptance is independent of dispatch success"
        );
        assert_eq!(
            result
                .destination
                .unwrap()
                .acceptance
                .unwrap()
                .submission_id,
            selected.id
        );
        assert_eq!(
            reviewed::consume_review_with_boundary(
                &refused_conn,
                name,
                persona,
                room,
                &board,
                pass.review_id,
                &reviewed::SelfReview::NotAdmitted,
                Some(&dream)
            )
            .await
            .unwrap()
            .state,
            State::Accepted,
            "accepted replay does not dispatch again"
        );
        assert!(dream.review_boundary_pending(persona, selection.submission_id));
        assert_eq!(
            reviewed::consume_review_with_boundary(
                &refused_conn,
                name,
                persona,
                room,
                &board,
                competing.review_id,
                &reviewed::SelfReview::NotAdmitted,
                Some(&dream)
            )
            .await
            .unwrap()
            .state,
            State::Accepted,
            "another review can inspect the accepted submission without owning its boundary"
        );
        // The original owner dies after durable acceptance, before its pending
        // notification is consumed. No AIRC replay or second submission follows.
        drop(dream);
        let reopened_store =
            reviewed::boundaries::ReviewBoundaryStore::new(data_runtime(), name.into(), persona);
        let page = reopened_store.page(None, 1).await.unwrap();
        assert_eq!(page.pending, vec![selection.submission_id]);
        assert_eq!(page.next, Some(selection.submission_id.to_string()));
        assert!(reopened_store
            .page(page.next, 1)
            .await
            .unwrap()
            .pending
            .is_empty());
        // A malformed earlier acceptance cannot permanently hide later valid IDs.
        // Keep it in the same real ORM store; recovery never regrades or deletes it.
        let mut malformed = reviewed::read_one::<_, reviewed::CreditReviewAcceptance>(
            data,
            name,
            &selection.submission_id.to_string(),
        )
        .await
        .unwrap()
        .unwrap();
        malformed.id = Uuid::nil();
        data.commands().execute_value("data/batch", json!({
            "dbPath": format!("@persona:{name}"),
            "operations": [{"operationType": "create", "collection": reviewed::CreditReviewAcceptance::COLLECTION,
                "id": malformed.id.to_string(), "data": malformed}],
        })).await.unwrap();
        let skipped = reopened_store.page(None, 1).await.unwrap();
        assert!(skipped.pending.is_empty());
        assert_eq!(skipped.next, Some(Uuid::nil().to_string()));
        assert_eq!(
            reopened_store.page(skipped.next, 1).await.unwrap().pending,
            vec![selection.submission_id]
        );
        let wrong_owner = reviewed::boundaries::ReviewBoundaryStore::new(
            data_runtime(),
            name.into(),
            Uuid::new_v4(),
        );
        assert!(wrong_owner
            .finish(&[selection.submission_id], 0)
            .await
            .is_err());
        let dream =
            region_over_with_store(persona, admission.clone(), Some(Arc::new(reopened_store.clone())));
        assert!(!dream.review_boundary_pending(persona, selection.submission_id));
        dream.tick(&RegionContext::for_persona(0, persona)).await;
        drain(&dream).await;
        assert!(!dream.review_boundary_pending(persona, selection.submission_id));
        assert_eq!(
            admission
                .recall_recent(16)
                .iter()
                .filter(|e| e.kind == crate::cognition::engram::EngramKind::Semantic)
                .count(),
            1
        );
        let completed = reviewed::read_one::<_, reviewed::boundaries::ProcessedReviewBoundary>(
            data,
            name,
            &selection.submission_id.to_string(),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(completed.persona_id, persona);
        assert_eq!(completed.review_id, pass.review_id.as_uuid());
        assert_eq!(completed.eligible_clusters, 1);
        assert!(reopened_store
            .page(Some(Uuid::nil().to_string()), 1)
            .await
            .unwrap()
            .pending
            .is_empty());
        let restarted = region_over_with_store(persona, admission, Some(Arc::new(reopened_store)));
        reviewed::consume_review_with_boundary(
            &refused_conn,
            name,
            persona,
            room,
            &board,
            competing.review_id,
            &reviewed::SelfReview::NotAdmitted,
            Some(&restarted),
        )
        .await
        .unwrap();
        assert!(
            !restarted.review_boundary_pending(persona, selection.submission_id),
            "another signed review cannot replay the selected acceptance into a fresh owner"
        );
        reviewed::consume_review_with_boundary(
            &refused_conn,
            name,
            persona,
            room,
            &board,
            pass.review_id,
            &reviewed::SelfReview::NotAdmitted,
            Some(&restarted),
        )
        .await
        .unwrap();
        assert!(
            !restarted.review_boundary_pending(persona, selection.submission_id),
            "durable completion suppresses exact replay after another restart"
        );
        assert_eq!(
            stored_credit_rows(data, name).await.len(),
            1,
            "newer staging remains intact"
        );
    }

    // what this catches: 6c36c24d — publication pins a selected immutable copy,
    // not the periodically replaced row; overlapping bindings roll back whole.
    #[tokio::test]
    pub(super) async fn submission_binding_survives_replacement_and_reserves_generations_atomically() {
        use reviewed::{CreditBindingError, CreditGenerationReservation, CreditTransferIntent, SubmissionSelection, WorkCreditBinding};
        let home = tempfile::tempdir().unwrap();
        let _home = HomeGuard::set(home.path()).await;
        crate::persona::register_substrate_orm_entities(crate::orm::OrmEntityRegistry::global())
            .unwrap();
        let executor = data_runtime();
        let persona = Uuid::new_v4();
        let card = Uuid::new_v4();
        let name = "immutable-work-binding";
        let receipts = vec![served_receipt("generation-a", "model-a")];
        let (mut capture, selected) =
            stage_settlement_fixture(executor.clone(), persona, name, card, receipts).await;
        let receipts = selected.receipts.clone();
        let data = conn_as(executor.clone(), persona);
        let selection = SubmissionSelection {
            submission_id: Uuid::new_v4(),
            room_id: Uuid::new_v4(),
            card_id: card,
            claim_id: selected.claim_id.unwrap(),
            staged_revision_id: selected.id,
            instance: "ordinary-project-card".into(),
            base_sha: airc_work::GitObjectId::new("a".repeat(40)).unwrap(),
            artifact: serde_json::from_value(json!({
                "hash":"b".repeat(64),"size_bytes":5,"mime":"text/x-diff"
            }))
            .unwrap(),
        };
        let binding = reviewed::bind_submission(&data, name, persona, selection.clone())
            .await
            .unwrap();
        assert_eq!(binding.transfer_intent_id, selected.id);
        let mut later = receipts;
        later.push(captured(persona, served_receipt("generation-b", "model-a")));
        assert!(
            capture
                .record(
                    &settlement_acts(8),
                    &later,
                    Some("The later revision includes an additional inspection."),
                    true
                )
                .await
        );
        let current = stored_credit_rows(&data, name).await.pop().unwrap();
        assert_ne!(current.id, selected.id);
        assert!(
            reviewed::read_one::<_, StagedCredit>(&data, name, &selected.id.to_string())
                .await
                .unwrap()
                .is_none()
        );

        // New caller/executor uses the durable binding, including after a lost
        // publication acknowledgement. It does not require the old working row.
        let reopened = conn_as(data_runtime(), persona);
        let replay = reviewed::bind_submission(&reopened, name, persona, selection.clone())
            .await
            .unwrap();
        let intent = reviewed::read_one::<_, CreditTransferIntent>(
            &reopened,
            name,
            &replay.transfer_intent_id.to_string(),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(intent.snapshot.completion, selected.completion);
        assert_eq!(intent.snapshot.receipts, selected.receipts);
        let mut changed = selection.clone();
        changed.artifact.size_bytes += 1;
        assert!(matches!(
            reviewed::bind_submission(&data, name, persona, changed).await,
            Err(CreditBindingError::ConflictingSubmission(_))
        ));

        let mut overlapping = selection.clone();
        overlapping.submission_id = Uuid::new_v4();
        overlapping.staged_revision_id = current.id;
        assert!(
            reviewed::bind_submission(&data, name, persona, overlapping.clone())
                .await
                .is_err()
        );
        assert!(
            reviewed::read_one::<_, WorkCreditBinding>(
                &data,
                name,
                &overlapping.submission_id.to_string()
            )
            .await
            .unwrap()
            .is_none(),
            "failed child reservation rolls back the new binding"
        );
        assert!(
            reviewed::read_one::<_, CreditGenerationReservation>(&data, name, "generation-b")
                .await
                .unwrap()
                .is_none(),
            "rollback leaves no partial reservation of the new generation"
        );
        assert!(
            reviewed::read_one::<_, CreditTransferIntent>(&data, name, &current.id.to_string())
                .await
                .unwrap()
                .is_none(),
            "failed binding also rolls back its preserved snapshot"
        );
        assert!(
            matches!(
                reviewed::reserve_transfer(&data, name, &selected, None).await,
                Err(CreditBindingError::Overlap)
            ),
            "the legacy grader cannot train a bound snapshot"
        );
        reviewed::reserve_transfer(&data, name, &intent.snapshot, Some(selection.submission_id))
            .await
            .unwrap();
        assert_eq!(stored_credit_rows(&data, name).await[0].id, current.id);
    }

    // what this catches (card fe8554d2, Codex on #5386): a submission bound by the generation
    // that issued it, through the real board projection and review consumption. A row another
    // pending submission also issued from holds and never dispatches. A review of a submission
    // that was never published (the record landed, the publication failed) accepts nothing.
    #[tokio::test]
    pub(super) async fn a_review_of_a_pending_submission_holds_a_shared_or_unpublished_row(
    ) -> Result<(), Box<dyn std::error::Error>> {
        use airc_work::WorkEvent;
        use reviewed::ReviewedCreditState as State;
        let (mut fx, _capture) = exact_review_bound_by(Some("reviewed-request")).await;
        let (executor, persona, name, room) = (fx.executor.clone(), fx.persona, fx.name, fx.room);
        let pass = fx.pass.clone();
        let twin_id = Uuid::new_v4();
        let twin_published = airc_work::WorkSubmission {
            submission_id: airc_work::SubmissionId::from_uuid(twin_id),
            card_id: fx.card_id,
            claim_id: airc_work::ClaimId::from_uuid(fx.selection.claim_id),
            instance: fx.selection.instance.clone(),
            base_sha: fx.selection.base_sha.clone(),
            artifact: fx.selection.artifact.clone(),
            publisher: fx.owner,
            submitted_at_ms: 3,
        };
        // The twin is published too; a pending row shares a row whether or not it is.
        let board = fx.project(vec![
            (WorkEvent::WorkSubmitted(twin_published.clone()), fx.owner),
            (WorkEvent::WorkSubmissionReviewed(pass.clone()), fx.reviewer),
        ]);
        let twin = reviewed::PendingSelection {
            publication: reviewed::Publication::of(room, &twin_published),
            issuing_generation: "reviewed-request".into(),
        };
        reviewed::record_pending(&fx.data, name, persona, twin).await?;
        let never_submit = continuum_client::mock::MockTransport::new();
        never_submit.respond_to("genome/training-trigger/submit", |_| panic!("held provenance must not dispatch"));
        let held = reviewed::consume_review_with_boundary(
            &settlement_connection(executor.clone(), persona, never_submit, None),
            name, persona, room, &board, pass.review_id, &reviewed::SelfReview::NotAdmitted, None,
        )
        .await?;
        assert_eq!(held.state, State::ProvenanceHeld, "{held:?}");

        fx.prefix.retain(|(event, _)| !matches!(event, WorkEvent::WorkSubmitted(_)));
        let unpublished = fx.project(vec![(WorkEvent::WorkSubmissionReviewed(pass.clone()), fx.reviewer)]);
        let never = continuum_client::mock::MockTransport::new();
        never.respond_to("genome/training-trigger/submit", |_| panic!("an unpublished submission must not dispatch"));
        assert!(reviewed::consume_review_with_boundary(
            &settlement_connection(executor, persona, never, None),
            name, persona, room, &unpublished, pass.review_id, &reviewed::SelfReview::NotAdmitted, None,
        )
        .await
        .is_err());
        Ok(())
    }

    // what this catches (card fe8554d2): with no other submission on the row, a signed pass
    // resolves the pending binding to the issuing turn's row and accepts it.
    #[tokio::test]
    pub(super) async fn a_review_of_a_pending_submission_accepts_the_issuing_turns_row(
    ) -> Result<(), Box<dyn std::error::Error>> {
        use airc_work::WorkEvent;
        use reviewed::ReviewedCreditState as State;
        let (fx, _capture) = exact_review_bound_by(Some("reviewed-request")).await;
        let board = fx.project(vec![(WorkEvent::WorkSubmissionReviewed(fx.pass.clone()), fx.reviewer)]);
        let submit = continuum_client::mock::MockTransport::new();
        script_settlement_reply(
            &submit,
            &fx.selected,
            Some(json!({"success":true,"acceptance":{"submissionId":fx.selected.id,"replayed":false}})),
        );
        let accepted = reviewed::consume_review_with_boundary(
            &settlement_connection(fx.executor.clone(), fx.persona, submit, None),
            fx.name, fx.persona, fx.room, &board, fx.pass.review_id, &reviewed::SelfReview::NotAdmitted, None,
        )
        .await?;
        assert_eq!(accepted.state, State::Accepted, "{accepted:?}");
        assert_eq!(accepted.staged_revision_id, Some(fx.selected.id), "the issuing turn's row");
        Ok(())
    }
