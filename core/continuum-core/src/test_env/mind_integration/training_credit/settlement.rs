#![cfg(test)]
use super::*;

    // what this catches: 41f4e3ef — one row's served model cannot label its
    // siblings, and neither a refusal nor unknown attribution transfers ownership.
    #[tokio::test]
    pub(super) async fn settlement_retains_unaccepted_rows_and_uses_each_rows_actual_model() {
        let home = tempfile::tempdir().unwrap();
        let _home = HomeGuard::set(home.path()).await;
        crate::persona::register_substrate_orm_entities(crate::orm::OrmEntityRegistry::global())
            .unwrap();
        let executor = data_runtime();
        let persona = Uuid::new_v4();
        let card = Uuid::new_v4();
        let other_card = Uuid::new_v4();
        let name = "settlement-models";
        let (_, a) = stage_settlement_fixture(
            executor.clone(),
            persona,
            name,
            card,
            vec![served_receipt("a", "model-a")],
        )
        .await;
        let (_, b) = stage_settlement_fixture(
            executor.clone(),
            persona,
            name,
            other_card,
            vec![served_receipt("b", "model-b")],
        )
        .await;
        let (_, mixed) = stage_settlement_fixture(
            executor.clone(),
            persona,
            name,
            card,
            vec![
                served_receipt("mixed-a", "model-a"),
                served_receipt("mixed-b", "model-b"),
            ],
        )
        .await;
        let (_, faulted) = stage_settlement_fixture(
            executor.clone(),
            persona,
            name,
            card,
            vec![receipt("all-faulted")],
        )
        .await;
        assert!(mixed.served.is_none() && faulted.served.is_none());
        let data = conn_as(executor.clone(), persona);
        let submit = continuum_client::mock::MockTransport::new();
        for reply in [
            Some(
                json!({"success":false,"errorKind":"PersistenceFailed","error":"disk unavailable"}),
            ),
            Some(json!({"success":true,"outcome":"BatchAppended"})),
            Some(
                json!({"success":true,"acceptance":{"submissionId":Uuid::new_v4(),"replayed":false}}),
            ),
            None,
        ] {
            script_settlement_reply(&submit, &a, reply);
        }
        let conn = settlement_connection(executor.clone(), persona, submit.clone(), None);
        for _ in 0..4 {
            assert!(!matches!(
                settle_staged_row(&conn, &substrate_conn(&executor), persona, name, &a, true).await,
                Ok(true)
            ));
            assert_eq!(stored_credit_rows(&data, name).await.len(), 4,
                "refusal, missing/mismatched receipt, or unknown acknowledgement preserves staged ownership");
        }
        // This response must remain queued until A is eligible. A deferred row
        // reaching submit either mismatches A's identity or consumes its receipt.
        script_settlement_reply(
            &submit,
            &a,
            Some(json!({
                "success":true,"outcome":"AlreadyAccepted",
                "acceptance":{"submissionId":a.id,"replayed":true}
            })),
        );
        assert!(!settle_staged_row(&conn, &substrate_conn(&executor), persona, name, &mixed, true)
            .await
            .unwrap());
        assert!(!settle_staged_row(&conn, &substrate_conn(&executor), persona, name, &faulted, true)
            .await
            .unwrap());
        assert!(!settle_staged_row(&conn, &substrate_conn(&executor), persona, name, &a, false)
            .await
            .unwrap());
        let mut claimless = a.clone();
        claimless.claim_id = None;
        claimless.owner = None;
        assert!(!settle_staged_row(&conn, &substrate_conn(&executor), persona, name, &claimless, true)
            .await
            .unwrap());
        let mut foreign = a.clone();
        foreign.owner = Some(Uuid::new_v4());
        assert!(!settle_staged_row(&conn, &substrate_conn(&executor), persona, name, &foreign, true)
            .await
            .unwrap());
        assert_eq!(stored_credit_rows(&data, name).await.len(), 4);
        // Regression for c744: the last card retires its round BEFORE async
        // grading. A different round using the same instance must not receive it.
        let round = Uuid::new_v4();
        let other_round = Uuid::new_v4();
        let instance = format!("credit-{}", card);
        for (id, target) in [(round, card), (other_round, other_card)] {
            crate::cognition::bench_round::open_round(id, "coder-write-eval");
            crate::cognition::bench_round::add_card(id, target);
            crate::cognition::bench_round::seal_round(id);
        }
        crate::cognition::bench_round::record_card_instance(card, &instance);
        crate::cognition::bench_round::record_card_instance(other_card, &instance);
        crate::cognition::bench_round::settle_card_direct(card, "closed");
        assert_eq!(crate::cognition::bench_round::cards_for_instance(&instance), vec![other_card]);
        let verdict = crate::cognition::swe_bench::SweVerdict {
            instance_id: instance.clone(), resolved: true,
            ..Default::default()
        };
        let rows = stored_credit_rows(&data, name).await;
        let evidence = graded::LegacyGradeEvidence {
            room: Uuid::new_v4(), card, owner: persona, claim: a.claim_id.unwrap(),
            review_card: Uuid::new_v4(), reviewer: Uuid::new_v4(), review_claim: Uuid::new_v4(),
            review_post: Uuid::new_v4(), review_sha256: "review-digest".into(),
            benchmark: "coder-write-eval".into(), instance: instance.clone(),
            verdict_sha256: "verdict-digest".into(), artifact_sha256: "artifact-digest".into(),
        };
        let graded_at_ms = rows.iter().map(|row| row.staged_at_ms).max().unwrap();
        let binding = graded::BenchmarkCreditAttribution {
            id: card, rows: graded::select_rows(&evidence, graded_at_ms, &rows).unwrap(),
            attested_by: evidence.reviewer, evidence, graded_at_ms,
        };
        assert_eq!(binding.rows.iter().map(|row| row.id).collect::<Vec<_>>(), vec![a.id],
            "other cards and other claims on this card cannot inherit a legacy grade");
        graded::persist(&data, name, &binding).await.unwrap();
        graded::persist(&data, name, &binding).await.unwrap();
        let retained = reviewed::read_one::<_, graded::BenchmarkCreditAttribution>(
            &data, name, &card.to_string()).await.unwrap().unwrap();
        assert_eq!(retained, binding);
        let mut conflicting = binding.clone();
        conflicting.evidence.claim = Uuid::new_v4();
        assert!(graded::persist(&data, name, &conflicting).await.is_err());
        let mut changed = rows.clone();
        changed.iter_mut().find(|row| row.id == a.id).unwrap().completion.push_str("changed");
        assert!(graded::bound_rows(&binding, changed).is_err(), "no transfer after payload drift");
        let mut changed = rows.clone();
        changed.iter_mut().find(|row| row.id == a.id).unwrap().coursework_set = Some("changed".into());
        assert!(graded::bound_rows_retained(&data, name, &binding, changed).await.is_err(),
            "eligibility metadata is part of the exact attributed row");
        assert_eq!(graded::bound_rows(&binding, rows.clone()).unwrap().len(), 1);
        crate::cognition::swe_bench::record_card_verdict_with(&verdict, card, |target, passed| {
            let (rows, conn, executor, instance) = (&rows, &conn, &executor, &instance);
            async move {
            assert_eq!(target, card);
            assert!(crate::cognition::swe_bench::read_verdict(&instance).unwrap().resolved,
                "the durable grade precedes any transfer");
            let mut transferred = 0;
            for row in rows.iter().filter(|row| row.card_id == target) {
                transferred += usize::from(settle_staged_row(
                    &conn, &substrate_conn(&executor), persona, name, row, passed,
                ).await.unwrap());
            }
            assert_eq!(transferred, 1);
            }
        }).await.unwrap().expect("durable exact-card verdict");
        assert!(reviewed::transfer_accepted(&data, name, a.id).await.unwrap());
        assert_eq!(graded::bound_rows_retained(&data, name, &binding, stored_credit_rows(&data, name).await).await.unwrap().len(), 1,
            "settlement bookkeeping does not invalidate immutable recovery evidence");
        assert!(!reviewed::transfer_accepted(&data, name, b.id).await.unwrap(),
            "same-instance work on another card is not credited");
        for refused in [
            crate::cognition::swe_bench::SweVerdict { resolved: false, ..verdict.clone() },
            crate::cognition::swe_bench::SweVerdict {
                error: Some("grader unavailable".into()), ..verdict.clone()
            },
        ] {
            assert!(crate::cognition::swe_bench::record_card_verdict_with(
                &refused, card, |_, _| async { panic!("unrecorded verdict settled credit") },
            ).await.unwrap().is_none());
        }
        script_settlement_reply(
            &submit,
            &b,
            Some(json!({
                "success":false,"errorKind":"DispatchFailed","error":"provider unavailable",
                "acceptance":{"submissionId":b.id,"replayed":false}
            })),
        );
        assert!(settle_staged_row(&conn, &substrate_conn(&executor), persona, name, &b, true)
            .await
            .unwrap());
        let remaining = stored_credit_rows(&data, name).await;
        assert_eq!(
            remaining.len(),
            4,
            "acceptance preserves inspectable source evidence"
        );
        assert!(reviewed::transfer_accepted(&data, name, a.id)
            .await
            .unwrap());
        assert!(reviewed::transfer_accepted(&data, name, b.id)
            .await
            .unwrap());
        submit.respond_to("genome/training-trigger/submit", |_| {
            panic!("acknowledged transfer was dispatched again")
        });
        crate::cognition::swe_bench::record_card_verdict_with(&verdict, card, |target, passed| {
            let (rows, conn, executor) = (&rows, &conn, &executor);
            async move {
            for row in rows.iter().filter(|row| row.card_id == target) {
                assert!(!settle_staged_row(
                    &conn, &substrate_conn(&executor), persona, name, row, passed,
                ).await.unwrap(), "retry reuses the existing acceptance");
            }
            }
        }).await.unwrap().expect("repeated durable grade can retry transfer");
        assert!(!settle_staged_row(&conn, &substrate_conn(&executor), persona, name, &b, true)
            .await
            .unwrap());
        assert!(remaining.iter().any(|row| row.id == mixed.id));
        assert!(remaining.iter().any(|row| row.id == faulted.id));
        crate::cognition::bench_round::settle_card_direct(other_card, "closed");
    }
