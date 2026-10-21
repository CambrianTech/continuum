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
        let links: crate::modules::data::DataListResult = serde_json::from_value(links).unwrap();
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
                receipts.clone(),
            )
            .await;
            let submit = continuum_client::mock::MockTransport::new();
            script_settlement_reply(&submit, &original, refused);
            let conn = settlement_connection(executor.clone(), persona, submit, None);
            assert!(!matches!(
                settle_staged_row(&conn, &substrate_conn(&executor), persona, name, &original, true).await,
                Ok(true)
            ));
            let mut later_receipts = receipts;
            later_receipts.push(served_receipt("later-generation", "actual-model"));
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
            stage_settlement_fixture(executor.clone(), persona, name, card, receipts.clone()).await;
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
        let mut later_receipts = receipts;
        later_receipts
            .extend((4..8).map(|n| served_receipt(&format!("generation-{n}"), "model-a")));
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
