#![cfg(test)]
use super::*;

    // what this catches: Kimi's inspected revision was pruned before submission;
    // stable generation selection must survive replacement without rebinding retries.
    #[tokio::test]
    pub(super) async fn generation_binding_survives_inspection_replacement_and_retries() {
        use reviewed::{CreditBindingError, SubmissionSelection};
        let home = tempfile::tempdir().unwrap();
        let _home = HomeGuard::set(home.path()).await;
        crate::persona::register_substrate_orm_entities(crate::orm::OrmEntityRegistry::global())
            .unwrap();
        let executor = data_runtime();
        let persona = Uuid::new_v4();
        let card = Uuid::new_v4();
        let room = Uuid::new_v4();
        let name = "generation-work-binding";
        let mut receipts = vec![served_receipt("correction", "model-a")];
        let (mut capture, inspected) =
            stage_settlement_fixture(executor.clone(), persona, name, card, receipts.clone()).await;
        let data = conn_as(executor, persona);
        receipts.push(served_receipt("inspection", "model-a"));
        assert!(
            capture
                .record(
                    &settlement_acts(8),
                    &receipts,
                    Some("Inspected evidence."),
                    true
                )
                .await
        );
        let selected = stored_credit_rows(&data, name).await.pop().unwrap();
        assert_ne!(selected.id, inspected.id);
        let submitted = airc_work::WorkSubmission {
            submission_id: airc_work::SubmissionId::from_uuid(Uuid::new_v4()),
            card_id: airc_work::WorkCardId::from_uuid(card),
            claim_id: airc_work::ClaimId::from_uuid(selected.claim_id.unwrap()),
            instance: "ordinary-project-card".into(),
            base_sha: airc_work::GitObjectId::new("a".repeat(40)).unwrap(),
            artifact: serde_json::from_value(
                json!({"hash":"b".repeat(64),"size_bytes":5,"mime":"text/x-diff"}),
            )
            .unwrap(),
            publisher: airc_core::PeerId::from_uuid(persona),
            submitted_at_ms: 3,
        };
        let old = SubmissionSelection {
            submission_id: submitted.submission_id.as_uuid(),
            room_id: room,
            card_id: card,
            claim_id: submitted.claim_id.as_uuid(),
            staged_revision_id: inspected.id,
            instance: submitted.instance.clone(),
            base_sha: submitted.base_sha.clone(),
            artifact: submitted.artifact.clone(),
        };
        assert!(matches!(
            reviewed::bind_submission(&data, name, persona, old).await,
            Err(CreditBindingError::MissingRevision(_))
        ));
        assert!(matches!(
            reviewed::bind_submission_generation(&data, name, persona, room, &submitted, "missing")
                .await,
            Err(CreditBindingError::AmbiguousGeneration(_))
        ));
        // A missing correlation child must not fall back to scanning every
        // captured prompt on the card. Restore it to exercise the normal path.
        let links = data
            .commands()
            .execute_value(
                "data/list",
                json!({
                    "collection": StagedCreditGeneration::COLLECTION,
                    "dbPath": format!("@persona:{name}"),
                    "filter": {"submittedRequestId": "correction"}
                }),
            )
            .await
            .unwrap();
        let links: crate::orm::list_result::DataListResult = serde_json::from_value(links).unwrap();
        assert_eq!(links.total, 1);
        let link: crate::orm::types::DataRecord =
            serde_json::from_value(links.items[0].clone()).unwrap();
        let child: StagedCreditGeneration = serde_json::from_value(link.data).unwrap();
        let handle = format!("@persona:{name}");
        let removed = data.commands().execute_value("data/batch", json!({
            "dbPath": handle,
            "operations": [{"operationType": "delete", "collection": StagedCreditGeneration::COLLECTION, "id": child.id.to_string()}]
        })).await.unwrap();
        storage_ok(&removed, "data/batch", StagedCreditGeneration::COLLECTION).unwrap();
        assert!(matches!(
            reviewed::bind_submission_generation(
                &data,
                name,
                persona,
                room,
                &submitted,
                "correction"
            )
            .await,
            Err(CreditBindingError::AmbiguousGeneration(_))
        ));
        let restored = data.commands().execute_value("data/batch", json!({
            "dbPath": handle,
            "operations": [{"operationType": "create", "collection": StagedCreditGeneration::COLLECTION, "id": child.id.to_string(), "data": child}]
        })).await.unwrap();
        storage_ok(&restored, "data/batch", StagedCreditGeneration::COLLECTION).unwrap();
        let bound = reviewed::bind_submission_generation(
            &data,
            name,
            persona,
            room,
            &submitted,
            "correction",
        )
        .await
        .unwrap();
        assert_eq!(bound.selection.staged_revision_id, selected.id);
        receipts.push(served_receipt("later", "model-a"));
        assert!(
            capture
                .record(&settlement_acts(12), &receipts, Some("Later work."), true)
                .await
        );
        let reopened = conn_as(data_runtime(), persona);
        let retry = reviewed::bind_submission_generation(
            &reopened,
            name,
            persona,
            room,
            &submitted,
            "correction",
        )
        .await
        .unwrap();
        assert_eq!(retry.selection, bound.selection);
        assert!(matches!(
            reviewed::bind_submission_generation(
                &reopened, name, persona, room, &submitted, "later"
            )
            .await,
            Err(CreditBindingError::WrongSelection)
        ));
        let mut changed = submitted.clone();
        changed.artifact.size_bytes += 1;
        assert!(matches!(
            reviewed::bind_submission_generation(
                &reopened,
                name,
                persona,
                room,
                &changed,
                "correction"
            )
            .await,
            Err(CreditBindingError::ConflictingSubmission(_))
        ));
        changed = submitted.clone();
        changed.submission_id = airc_work::SubmissionId::from_uuid(Uuid::new_v4());
        assert!(reviewed::bind_submission_generation(
            &reopened,
            name,
            persona,
            room,
            &changed,
            "correction"
        )
        .await
        .is_err());
    }

    // what this catches (card 44aee459, the 5090 on 2026-10-10): a transferred row's lived
    // copy is shed through data/update, which the gate refuses at her identity (Trusted)
    // and admits for the substrate (Owner). Under the REAL gate: through her connection the
    // copy stays (26 refusals in one pass on the 5090), through the substrate's it is gone.
    #[tokio::test]
    pub(super) async fn the_lived_copy_is_shed_by_the_substrate_not_by_her_identity() {
        use crate::ai::types::{TextGenerationRequest, TextGenerationResponse};
        // Her store resolves through the process home (`@persona:<name>` under $HOME). Without
        // its own pinned home this test raced the home-mutating fixtures: ensure-schema and the
        // batch resolved to two different databases ("no such table: staged_credit", card
        // d4909786, on #5239 and #5244), and it wrote into the REAL ~/.continuum/personas/kimi.
        let home = tempfile::tempdir().expect("tempdir");
        let _home = HomeGuard::set(home.path()).await;
        let executor = data_runtime();
        let persona = Uuid::new_v4();
        let name = "kimi";
        let hers = conn_as(executor.clone(), persona);
        let lived = crate::genome::fine_tuning::LivedCall {
            capture: Uuid::new_v4().to_string(),
            request: TextGenerationRequest::default(),
            response: TextGenerationResponse {
                text: "read it before guessing".into(),
                finish_reason: crate::ai::FinishReason::Stop,
                model: "m".into(),
                provider: "p".into(),
                usage: crate::ai::UsageMetrics::default(),
                response_time_ms: 0,
                request_id: "r".into(),
                content: None,
                tool_calls: None,
                reasoning: None,
                routing: None,
                error: None,
                timing: None,
            },
        };
        let credit = CapturedCredit {
            task: crate::cognition::held_out::CardTask::NotATask,
            card_id: Uuid::new_v4(),
            claim: Some(ClaimReceipt {
                claim_id: Uuid::new_v4(),
                owner: airc_core::PeerId::from_uuid(persona),
                role: CreditRole::Owner,
            }),
        };
        // Her act, under her identity: staging is hers and is admitted.
        let id = stage_credit(
            &hers,
            name,
            &credit,
            vec![served_receipt("lived-generation", "actual-model")],
            "p".into(),
            "c".into(),
            Some(vec![lived]),
            Uuid::new_v4(),
            &[],
        )
        .await
        .expect("test: her staging is admitted");
        let row = stored_credit_rows(&hers, name).await.into_iter().find(|r| r.id == id).expect("test: staged");
        assert!(row.lived.as_deref().is_some_and(|calls| !calls.is_empty()), "staged with its lived copy");
        clear_lived(&hers, name, &row).await;
        let still = stored_credit_rows(&hers, name).await;
        assert!(
            still.iter().any(|r| r.id == id && r.lived.is_some()),
            "her own identity cannot shed the copy: data/update is refused at Trusted"
        );
        clear_lived(&substrate_conn(&executor), name, &row).await;
        let shed = stored_credit_rows(&hers, name).await;
        assert!(
            shed.iter().any(|r| r.id == id && r.lived.is_none()),
            "the substrate sheds it"
        );
    }

    // what this catches: 6c36c24d — legacy refusal/uncertain acknowledgement
    // cannot leave generation reservations whose replaced payload is lost.
    #[tokio::test]
    pub(super) async fn refused_legacy_transfer_reopens_its_exact_payload_after_periodic_replacement() {
        let home = tempfile::tempdir().unwrap();
        let _home = HomeGuard::set(home.path()).await;
        crate::persona::register_substrate_orm_entities(crate::orm::OrmEntityRegistry::global())
            .unwrap();
        for (name, refused) in [
            (
                "legacy-refused",
                Some(
                    json!({"success":false,"errorKind":"PersistenceFailed","error":"unavailable"}),
                ),
            ),
            ("legacy-uncertain", None),
        ] {
            let executor = data_runtime();
            let persona = Uuid::new_v4();
            let receipts = vec![served_receipt("original-generation", "actual-model")];
            let (mut capture, original) = stage_settlement_fixture(
                executor.clone(),
                persona,
                name,
                Uuid::new_v4(),
                receipts,
            )
            .await;
            let submit = continuum_client::mock::MockTransport::new();
            script_settlement_reply(&submit, &original, refused);
            let conn = settlement_connection(executor.clone(), persona, submit, None);
            assert!(!matches!(
                settle_staged_row(&conn, &substrate_conn(&executor), persona, name, &original, true).await,
                Ok(true)
            ));
            let mut later_receipts = original.receipts.clone();
            later_receipts.push(captured(persona, served_receipt("later-generation", "actual-model")));
            assert!(
                capture
                    .record(
                        &settlement_acts(8),
                        &later_receipts,
                        Some("Later progress is preserved separately."),
                        true
                    )
                    .await
            );
            let reopened_executor = data_runtime();
            let reopened = conn_as(reopened_executor.clone(), persona);
            assert!(reviewed::read_one::<_, StagedCredit>(
                &reopened,
                name,
                &original.id.to_string()
            )
            .await
            .unwrap()
            .is_none());
            reviewed::ensure_storage(&reopened, name).await.unwrap();
            let intent = reviewed::read_one::<_, reviewed::CreditTransferIntent>(
                &reopened,
                name,
                &original.id.to_string(),
            )
            .await
            .unwrap()
            .unwrap();
            assert_eq!(intent.work_submission_id, None);
            assert_eq!(intent.snapshot.id, original.id);
            assert_eq!(intent.snapshot.prompt, original.prompt);
            assert_eq!(intent.snapshot.completion, original.completion);
            assert_eq!(intent.snapshot.receipts, original.receipts);
            assert_eq!(intent.snapshot.served, original.served);
            let newer = stored_credit_rows(&reopened, name).await.pop().unwrap();
            assert_ne!(newer.id, original.id);

            let submit = continuum_client::mock::MockTransport::new();
            script_settlement_reply(
                &submit,
                &original,
                Some(json!({
                    "success":true,"outcome":"AlreadyAccepted",
                    "acceptance":{"submissionId":original.id,"replayed":true}
                })),
            );
            submit.respond_to("genome/training-trigger/submit", |_| {
                panic!("overlap reached destination")
            });
            let conn = settlement_connection(reopened_executor.clone(), persona, submit, None);
            assert!(
                settle_staged_row(&conn, &substrate_conn(&reopened_executor), persona, name, &intent.snapshot, true)
                    .await
                    .unwrap()
            );
            assert!(!settle_staged_row(&conn, &substrate_conn(&reopened_executor), persona, name, &newer, true)
                .await
                .unwrap());
            assert_eq!(stored_credit_rows(&reopened, name).await[0].id, newer.id);
            assert!(
                reviewed::read_one::<_, reviewed::CreditTransferIntent>(
                    &reopened,
                    name,
                    &original.id.to_string()
                )
                .await
                .unwrap()
                .is_some(),
                "accepted evidence remains inspectable"
            );
        }
    }

    // what this catches: 41f4e3ef / 6c36c24d — an older verdict drain must never
    // delete newer progress, nor permit its cumulative successor to train twice.
    #[tokio::test]
    pub(super) async fn settlement_of_an_older_snapshot_cannot_delete_newer_progress() {
        let home = tempfile::tempdir().unwrap();
        let _home = HomeGuard::set(home.path()).await;
        crate::persona::register_substrate_orm_entities(crate::orm::OrmEntityRegistry::global())
            .unwrap();
        let executor = data_runtime();
        let persona = Uuid::new_v4();
        let card = Uuid::new_v4();
        let name = "overlapping-settlement";
        let receipts: Vec<_> = (0..4)
            .map(|n| served_receipt(&format!("generation-{n}"), "model-a"))
            .collect();
        let (mut capture, older) =
            stage_settlement_fixture(executor.clone(), persona, name, card, receipts).await;
        let submit = continuum_client::mock::MockTransport::new();
        script_settlement_reply(
            &submit,
            &older,
            Some(json!({
                "success":true, "outcome":"BatchAppended",
                "acceptance":{"submissionId":older.id,"replayed":false}
            })),
        );
        let entered = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        let conn = settlement_connection(
            executor.clone(),
            persona,
            submit,
            Some((entered.clone(), release.clone())),
        );
        let substrate = substrate_conn(&executor);
        let mut drain = Box::pin(settle_staged_row(&conn, &substrate, persona, name, &older, true));
        tokio::select! {
            _ = entered.notified() => {},
            result = &mut drain => panic!("drain settled before its paused receipt: {result:?}"),
            _ = tokio::time::sleep(std::time::Duration::from_secs(10)) => panic!("drain never submitted"),
        }
        let mut later_receipts = older.receipts.clone();
        later_receipts
            .extend((4..8).map(|n| captured(persona, served_receipt(&format!("generation-{n}"), "model-a"))));
        assert!(
            capture
                .record(
                    &settlement_acts(8),
                    &later_receipts,
                    Some("The final source review is complete."),
                    true
                )
                .await
        );
        let data = conn_as(executor.clone(), persona);
        let before = stored_credit_rows(&data, name).await;
        assert_eq!(before.len(), 1);
        let newer = before.into_iter().next().unwrap();
        assert_ne!(
            newer.id, older.id,
            "changed content is a new immutable snapshot"
        );
        release.notify_one();
        assert!(drain.await.unwrap());
        let remaining = stored_credit_rows(&data, name).await;
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, newer.id);
        assert_eq!(remaining[0].receipts, later_receipts);
        let children = data
            .commands()
            .execute_value(
                "data/list",
                json!({
                    "collection":StagedCreditGeneration::COLLECTION,
                    "dbPath":format!("@persona:{name}")
                }),
            )
            .await
            .unwrap();
        assert_eq!(children["items"].as_array().unwrap().len(), 8);

        let submit = continuum_client::mock::MockTransport::new();
        submit.respond_to("genome/training-trigger/submit", |_| {
            panic!("overlapping cumulative revision reached destination submission")
        });
        let conn = settlement_connection(executor.clone(), persona, submit, None);
        assert!(
            !settle_staged_row(&conn, &substrate_conn(&executor), persona, name, &newer, true)
                .await
                .unwrap(),
            "a cumulative successor cannot inherit a verdict and train the same generations again"
        );
        assert_eq!(stored_credit_rows(&data, name).await[0].id, newer.id);
    }

    // what this catches (card fe8554d2, Codex on #5386): a submission with no explicit
    // selection binds the turn that ISSUED it, never "her newest revision". Turn 1 issues the
    // submission; a newer unrelated turn on the same claim stages after it; the review binds
    // turn 1. A generation not yet staged at publish holds, then binds once staged. A row
    // carrying another pending submission's generation holds (staging is periodic, so failed S
    // and corrected M can share one), in either order, including when M is recorded after S's
    // binding was made, and when S was bound explicitly. A record whose publication's answer was
    // lost still counts (it may have landed). A board submission that is not the recorded one holds;
    // the same id retried from a later act keeps the first generation; a different one is refused.
    // Storage-level: record/resolve are called directly, not through WorkSubmit or a signed
    // board review.
    #[tokio::test]
    pub(super) async fn a_pending_binding_resolves_to_the_issuing_turn_or_holds(
    ) -> Result<(), Box<dyn std::error::Error>> {
        use reviewed::{CreditBindingError, Held, PendingSelection};
        let home = tempfile::tempdir()?;
        let _home = HomeGuard::set(home.path()).await;
        crate::persona::register_substrate_orm_entities(crate::orm::OrmEntityRegistry::global())?;
        let executor = data_runtime();
        let (persona, card, room) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        let name = "pending-work-binding";
        let (_turn1, issuing) =
            stage_settlement_fixture(executor.clone(), persona, name, card, vec![served_receipt("work-a", "model-a")])
                .await;
        let claim = issuing.claim_id.ok_or("fixture row carries its claim")?;
        let turn = |prompt: &str| {
            TurnCreditCapture::with_executor(
                executor.clone(),
                persona,
                name.into(),
                prompt.into(),
                CapturedCredit {
                    task: crate::cognition::held_out::CardTask::NotATask,
                    card_id: card,
                    claim: Some(ClaimReceipt {
                        claim_id: claim,
                        owner: airc_core::PeerId::from_uuid(persona),
                        role: CreditRole::Owner,
                    }),
                },
            )
        };
        let data = conn_as(executor.clone(), persona);
        let submission = |byte: char| -> Result<airc_work::WorkSubmission, Box<dyn std::error::Error>> {
            Ok(airc_work::WorkSubmission {
                submission_id: airc_work::SubmissionId::from_uuid(Uuid::new_v4()),
                card_id: airc_work::WorkCardId::from_uuid(card),
                claim_id: airc_work::ClaimId::from_uuid(claim),
                instance: "ordinary-project-card".into(),
                base_sha: airc_work::GitObjectId::new("a".repeat(40))?,
                artifact: serde_json::from_value(
                    json!({"hash": byte.to_string().repeat(64), "size_bytes": 5, "mime": "text/x-diff"}),
                )?,
                publisher: airc_core::PeerId::from_uuid(persona),
                submitted_at_ms: 3,
            })
        };
        let pending = |s: &airc_work::WorkSubmission, generation: &str| PendingSelection {
            publication: reviewed::Publication::of(room, s),
            issuing_generation: generation.into(),
        };


        // Turn 1 issued submission A; a newer unrelated turn stages after it.
        let a = submission('b')?;
        reviewed::record_pending(&data, name, persona, pending(&a, "work-a")).await?;
        let mut newer = turn("Inspect an unrelated file.");
        assert!(newer.record(&settlement_acts(4), &[served_receipt("inspect-b", "model-a")], None, true).await);
        let bound = reviewed::resolve_pending(&data, name, persona, room, &a).await?;
        assert!(
            matches!(&bound, Some(Ok(b)) if b.selection.staged_revision_id == issuing.id),
            "the issuing turn binds, not the newer one: {bound:?}"
        );

        // Not yet staged at publish: held, then bound once her turn stages it.
        let late = submission('c')?;
        reviewed::record_pending(&data, name, persona, pending(&late, "late")).await?;
        assert!(matches!(
            reviewed::resolve_pending(&data, name, persona, room, &late).await?,
            Some(Err(Held::Unresolved(_)))
        ));
        let mut later = turn("Finish the change.");
        assert!(later.record(&settlement_acts(4), &[served_receipt("late", "model-a")], None, true).await);
        assert!(matches!(reviewed::resolve_pending(&data, name, persona, room, &late).await?, Some(Ok(_))));

        // Failed S and corrected M share one staged row: either order holds.
        let (s, m) = (submission('d')?, submission('e')?);
        reviewed::record_pending(&data, name, persona, pending(&s, "s-gen")).await?;
        reviewed::record_pending(&data, name, persona, pending(&m, "m-gen")).await?;
        let mut shared = turn("Submit, take the failure, correct, resubmit.");
        let both = [served_receipt("s-gen", "model-a"), served_receipt("m-gen", "model-a")];
        assert!(shared.record(&settlement_acts(4), &both, None, true).await);
        assert_eq!(
            reviewed::resolve_pending(&data, name, persona, room, &m).await?.and_then(Result::err),
            Some(Held::SharedRevision(s.submission_id.as_uuid()))
        );
        assert_eq!(
            reviewed::resolve_pending(&data, name, persona, room, &s).await?.and_then(Result::err),
            Some(Held::SharedRevision(m.submission_id.as_uuid()))
        );

        // Temporal: S binds while only S is pending, and its row already carries what becomes
        // M's issuing generation. M is recorded afterwards; consuming S's binding re-checks its
        // immutable snapshot and sees M.
        let (s2, m2) = (submission('1')?, submission('2')?);
        let mut bucket = turn("One staged bucket, two submissions.");
        let both = [served_receipt("s2-gen", "model-a"), served_receipt("m2-gen", "model-a")];
        assert!(bucket.record(&settlement_acts(4), &both, None, true).await);
        reviewed::record_pending(&data, name, persona, pending(&s2, "s2-gen")).await?;
        let Some(Ok(s2_bound)) = reviewed::resolve_pending(&data, name, persona, room, &s2).await? else {
            return Err("S binds while it is the only pending submission".into());
        };
        reviewed::record_pending(&data, name, persona, pending(&m2, "m2-gen")).await?;
        let intent: reviewed::CreditTransferIntent = reviewed::read_one(&data, name, &s2_bound.transfer_intent_id.to_string())
            .await?
            .ok_or("the binding retains its transfer intent")?;
        assert_eq!(
            reviewed::issued_elsewhere(&data, name, &intent.snapshot, s2.submission_id.as_uuid()).await?,
            Some(m2.submission_id.as_uuid()),
            "consumption sees a submission recorded after the binding"
        );

        // Explicit sharing: S binds its revision explicitly (no pending row), the turn restages
        // with M's generation added, and M's row carries S's reserved generation: M holds.
        let (s3, m3) = (submission('3')?, submission('4')?);
        let mut explicit = turn("Explicit S, automatic M.");
        assert!(explicit.record(&settlement_acts(4), &[served_receipt("s3-gen", "model-a")], None, true).await);
        reviewed::bind_submission_generation(&data, name, persona, room, &s3, "s3-gen").await?;
        let grown = [served_receipt("s3-gen", "model-a"), served_receipt("m3-gen", "model-a")];
        assert!(explicit.record(&settlement_acts(8), &grown, None, true).await);
        reviewed::record_pending(&data, name, persona, pending(&m3, "m3-gen")).await?;
        assert_eq!(
            reviewed::resolve_pending(&data, name, persona, room, &m3).await?.and_then(Result::err),
            Some(Held::SharedRevision(s3.submission_id.as_uuid())),
            "S's explicit reservation is seen without a pending row"
        );

        // An orphan: its record landed, its publication's answer was lost. It may have landed, so
        // it still counts: a retry under a NEW id that shares its row holds. Its own same-id
        // retry, from a later act, is the same publication and is accepted.
        let (orphan, retry) = (submission('5')?, submission('6')?);
        reviewed::record_pending(&data, name, persona, pending(&orphan, "orphan-gen")).await?;
        reviewed::record_pending(&data, name, persona, pending(&retry, "retry-gen")).await?;
        let mut restaged = turn("Publication answer lost; retry under a new id.");
        let both = [served_receipt("orphan-gen", "model-a"), served_receipt("retry-gen", "model-a")];
        assert!(restaged.record(&settlement_acts(4), &both, None, true).await);
        assert_eq!(
            reviewed::resolve_pending(&data, name, persona, room, &retry).await?.and_then(Result::err),
            Some(Held::SharedRevision(orphan.submission_id.as_uuid())),
            "an uncertain publication is never ruled out by absence"
        );
        reviewed::record_pending(&data, name, persona, pending(&orphan, "later-act")).await?;

        // A crash between record and publish: the same record again is a no-op, a different
        // one is refused, and a board submission that differs from the record holds.
        let retried = submission('f')?;
        reviewed::record_pending(&data, name, persona, pending(&retried, "work-a")).await?;
        // The same id retried from a later act (a later generation) is the same publication:
        // accepted, and the first generation, the act that made it, is kept.
        reviewed::record_pending(&data, name, persona, pending(&retried, "later-act")).await?;
        let kept: reviewed::PendingCreditBinding =
            reviewed::read_one(&data, name, &retried.submission_id.as_uuid().to_string())
                .await?
                .ok_or("the pending row is retained")?;
        assert_eq!(kept.selection.issuing_generation, "work-a");
        let mut other = pending(&retried, "work-a");
        other.publication.instance = "another-instance".into();
        assert!(matches!(
            reviewed::record_pending(&data, name, persona, other).await,
            Err(CreditBindingError::ConflictingSubmission(_))
        ));
        let mut on_board = retried.clone();
        on_board.artifact.size_bytes += 1;
        assert_eq!(
            reviewed::resolve_pending(&data, name, persona, room, &on_board).await?.and_then(Result::err),
            Some(Held::PublicationMismatch)
        );
        Ok(())
    }
