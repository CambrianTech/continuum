#![cfg(test)]

    use super::*;
    use super::test_support::*;

    #[test]
    pub(super) fn a_pass_after_a_failure_trains_the_correction_not_the_rejected_turns() {
        let row = |at: u64, rejected: Option<u64>| -> StagedCredit {
            let mut r: StagedCredit = serde_json::from_value(json!({
                "id": Uuid::new_v4(), "cardId": Uuid::from_u128(0x6385), "claimId": null, "owner": null,
                "role": null, "receipts": [], "served": null, "prompt": "p", "completion": "c", "stagedAtMs": at,
            }))
            .expect("a minimal staged row");
            r.rejected_at_ms = rejected;
            r
        };
        let before = row(10, Some(50));
        let fix = row(60, None);
        assert_eq!(pass_admits(&[before.clone(), fix.clone()]), PassAdmits::AfterFailure);
        let (rejected, admitted): (Vec<_>, Vec<_>) =
            [before, fix.clone()].into_iter().partition(|r| r.rejected_at_ms.is_some());
        assert_eq!((rejected.len(), admitted.len()), (1, 1));
        assert_eq!(admitted[0].id, fix.id, "only the turn staged after the failure trains");
        assert_eq!(pass_admits(&[row(10, None)]), PassAdmits::All, "never failed: every row is the passing work");

    }

    // A review reaches the exact path only signed and accepted; its settlement is the
    // exact path's (tests with real rows: `a_permitted_self_review_transfers_her_exact_row_once`
    // and `reviewed_credit_replays_exact_authority_and_recovers_after_replacement`).
    pub(super) mod a_review_settles_the_cards_credit {
        use super::*;

        fn media() -> airc_blobs::MediaRef {
            serde_json::from_value(serde_json::json!({"hash": "b".repeat(64), "size_bytes": 12})).unwrap()
        }

        fn review(card: Uuid, outcome: airc_work::WorkReviewOutcome) -> airc_work::WorkEvent {
            airc_work::WorkEvent::WorkSubmissionReviewed(airc_work::WorkSubmissionReview {
                review_id: airc_work::WorkReviewId::new(),
                card_id: airc_work::WorkCardId::from_uuid(card),
                submission_id: airc_work::SubmissionId::new(),
                artifact: media(),
                review_card_id: airc_work::WorkCardId::new(),
                review_claim_id: airc_work::ClaimId::new(),
                reviewer: crate::identity::PeerId::from_uuid(Uuid::new_v4()),
                outcome,
                evidence: media(),
                evidence_text: None,
                reviewed_at_ms: 6_000,
            })
        }

        // what this catches (Astra, review of the credit design): a review-shaped event
        // reaching the exact path with no authority: an author other than the signed
        // sender, or a review the room's projection does not hold. Self-review admission
        // is the exact path's single rule (`SelfReview`), tested with real rows below.
        #[test]
        fn a_review_reaches_the_exact_path_only_signed_and_accepted() {
            use crate::persona::training_producer::reviewed::{check_review, ReviewRefusal};
            let card = Uuid::new_v4();
            let airc_work::WorkEvent::WorkSubmissionReviewed(r) = review(card, airc_work::WorkReviewOutcome::Failed) else {
                unreachable!("review() builds a review") // JUSTIFIED: the fixture's own constructor
            };
            let reviewer = r.reviewer;
            let stranger = crate::identity::PeerId::from_uuid(Uuid::new_v4());
            assert_eq!(check_review(reviewer, &r, Some(&r)), Ok(()));
            assert_eq!(check_review(stranger, &r, Some(&r)), Err(ReviewRefusal::NotSignedByReviewer));
            assert_eq!(check_review(reviewer, &r, None), Err(ReviewRefusal::NotAccepted));
            let mut other = r.clone();
            other.outcome = airc_work::WorkReviewOutcome::Passed;
            assert_eq!(check_review(reviewer, &r, Some(&other)), Err(ReviewRefusal::NotAccepted), "the projection's review must be this exact one");
        }
    }

    /// Card f1771c83: the settle path's ONLY untraced exit.
    ///
    /// `staged_submission_params` returning `None` skipped a staged revision with no
    /// probe anywhere — every other exit on that path names itself. A citizen's
    /// credit could sit unsettled forever and the only evidence was a row that never
    /// moved. These pin that each refusal now NAMES its reason.
    pub(super) mod a_skipped_credit_names_its_reason {
        use super::*;

        fn row(claim: Option<Uuid>, owner: Option<Uuid>, role: Option<CreditRole>) -> StagedCredit {
            StagedCredit {
                id: Uuid::new_v4(),
                card_id: Uuid::new_v4(),
                claim_id: claim,
                owner,
                role,
                receipts: Vec::new(),
                served: None,
                prompt: "p".into(),
                completion: "c".into(),
                lived: None,
                staged_at_ms: 1,
                coursework_set: None,
                task: None,
                rejected_at_ms: None,
            }
        }

        /// what this catches: the reasons collapsing back into one silent `None`.
        /// Each arm must be distinguishable, because "this card failed" and "this
        /// row has no provenance" are a normal discard and a defect respectively,
        /// and a reader who cannot tell them apart learns nothing.
        #[test]
        fn each_refusal_is_distinguishable_from_the_others() {
            let me = Uuid::new_v4();
            let claim = Some(Uuid::new_v4());

            let fail = staged_submission_params(
                me,
                "Kimi",
                &row(claim, Some(me), Some(CreditRole::Owner)),
                false,
            )
            .expect_err("a FAIL verdict submits nothing"); // JUSTIFIED: the test asserts the Err arm
            assert!(fail.contains("FAIL"), "{fail}");

            let no_claim = staged_submission_params(
                me,
                "Kimi",
                &row(None, Some(me), Some(CreditRole::Owner)),
                true,
            )
            .expect_err("no claim"); // JUSTIFIED: asserting the Err arm
            assert!(no_claim.contains("claim_id"), "{no_claim}");

            let foreign = staged_submission_params(
                me,
                "Kimi",
                &row(claim, Some(Uuid::new_v4()), Some(CreditRole::Owner)),
                true,
            )
            .expect_err("another persona's row"); // JUSTIFIED: asserting the Err arm
            assert!(foreign.contains("owner"), "{foreign}");

            // Distinct reasons, not one catch-all string.
            assert_ne!(fail, no_claim);
            assert_ne!(no_claim, foreign);
        }

        /// what this catches: the provenance gate going quiet again. A row with no
        /// served model/provider in its receipts is the case that CANNOT be fixed by
        /// retrying, so it is the one most worth naming out loud.
        #[test]
        fn a_row_with_no_served_provenance_says_so() {
            let me = Uuid::new_v4();
            let r = row(Some(Uuid::new_v4()), Some(me), Some(CreditRole::Owner));
            let why = staged_submission_params(me, "Kimi", &r, true)
                .expect_err("empty receipts carry no provenance"); // JUSTIFIED: asserting the Err arm
            assert!(
                why.contains("provenance"),
                "the unfixable case must name itself: {why}"
            );
        }

        /// what this catches (card ad107e18, Codex's audit): a turn whose capture is
        /// incomplete training as its flat prompt and completion. It must refuse by name and
        /// stay staged; the same row with the calls she lived submits them as its examples.
        #[test]
        fn a_row_without_its_lived_calls_waits_and_never_trains_flat() {
            let me = Uuid::new_v4();
            let mut r = row(Some(Uuid::new_v4()), Some(me), Some(CreditRole::Owner));
            r.receipts = vec![served_receipt("unread-request", "served-model")];
            r.prompt = "The parser drops the last token when the input ends without a newline.".into();
            r.completion = "Fixed: the tokenizer now flushes its pending token at end of input, and a \
                            regression test feeds it a file with no trailing newline."
                .into();
            let why = staged_submission_params(me, "Kimi", &r, true)
                .expect_err("no lived record submits nothing"); // JUSTIFIED: asserting the Err arm
            assert!(why.contains("lived"), "the wait names itself: {why}");
            r.lived = Some(Vec::new());
            assert_eq!(staged_submission_params(me, "Kimi", &r, true).err(), Some(why), "an empty record is no record");
            r.lived = Some(r.receipts.iter().map(crate::persona::training_producer::test_support::lived_call).collect());
            let params = staged_submission_params(me, "Kimi", &r, true).expect("the lived row submits"); // JUSTIFIED: the test asserts the Ok arm
            let examples: Vec<TrainingExample> =
                serde_json::from_value(params["examples"].clone()).expect("the wire examples are training examples"); // JUSTIFIED: test fixture
            assert!(!examples.is_empty() && examples.iter().all(|e| e.lived.is_some()), "every example is a call she lived");
        }

        /// what this catches (goal 8, card 67af4fca): the task captured with the card must
        /// reach the examples a settled row becomes. Dropped on the way, every example reads
        /// as unknown to `cognition::held_out::trained`, and no gene is ever judged on a
        /// held-out set known to be clean.
        #[test]
        fn a_settled_row_carries_its_task_onto_its_examples() {
            let me = Uuid::new_v4();
            let mut r = row(Some(Uuid::new_v4()), Some(me), Some(CreditRole::Owner));
            r.receipts = vec![served_receipt("settled-request", "served-model")];
            r.lived = Some(r.receipts.iter().map(crate::persona::training_producer::test_support::lived_call).collect());
            r.prompt = "The parser drops the last token when the input ends without a newline.".into();
            r.completion = "Fixed: the tokenizer now flushes its pending token at end of input, and a \
                            regression test feeds it a file with no trailing newline."
                .into();
            let task = crate::cognition::held_out::CardTask::of_card_title("[bench coursework-0123456789ab] t2: fix");
            r.task = Some(task);
            let params = staged_submission_params(me, "Kimi", &r, true).expect("a passing, claimed, served row submits"); // JUSTIFIED: the test asserts the Ok arm
            let examples: Vec<TrainingExample> =
                serde_json::from_value(params["examples"].clone()).expect("the wire examples are training examples"); // JUSTIFIED: test fixture
            let trained = crate::cognition::held_out::trained(&examples);
            assert_eq!(trained.unknown(), 0, "every example names its task");
            assert_eq!(trained.tasks().len(), 1);
        }
    }

    // what this catches (card 657e74de): the speech-discipline plan is exactly {burst →
    // PASS} in its OWN bucket at the curated floor — never the chat domain, never a
    // stamp, never a completion other than the silence token the parser reads.
    #[test]
    pub(super) fn a_gated_echo_plans_as_burst_to_pass_in_its_own_bucket() {
        let plan = plan_speech_discipline("[wake] You are Paige, awake on the continuum grid.");
        assert_eq!(plan.trait_kind, SPEECH_DISCIPLINE_TRAIT);
        assert_eq!(
            plan.bucket_key(),
            SPEECH_DISCIPLINE_TRAIT,
            "unstamped: the bare bucket"
        );
        assert_eq!(plan.completion, SILENCE_COMPLETION);
        assert!(plan.prompt.starts_with("[wake]"));
        assert!(
            plan.quality >= MIN_TRAINING_QUALITY,
            "curated floor clears the chat gate"
        );
        assert!(plan.stamp.is_none());
        // and the token is the one the Speak seam decides silence on
        assert_eq!(
            crate::cognition::deliberation_parse::decision_from_response(
                SILENCE_COMPLETION,
                None,
                &[]
            ),
            crate::cognition::workspace::Decision::pass()
        );
        // the bucket is measurable: its gym exists, so the sentinel can adopt or refuse
        assert!(crate::cognition::gym::gym_for_trait(SPEECH_DISCIPLINE_TRAIT).is_some());
    }

    // what this catches: card 3eaabbc6 — a successful transport can carry a
    // refused submit; absent/malformed receipts must never look like acceptance.
    #[tokio::test]
    pub(super) async fn training_submission_preserves_refusal_and_requires_a_receipt() {
        use continuum_client::mock::MockTransport;
        let transport = MockTransport::new();
        for value in [
            json!({"success": true, "outcome": "BatchAppended", "currentCount": 1, "threshold": 16}),
            json!({"success": false, "errorKind": "InconsistentBucket", "error": "different policy"}),
            json!({"success": false, "errorKind": "DispatchFailed", "error": "provider unavailable"}),
            json!({"outcome": "BatchAppended"}),
        ] {
            transport.respond_to("genome/training-trigger/submit", move |_| Ok(value.clone()));
        }
        let conn = Connection::new(transport);
        let accepted = submit_training(&conn, json!({}))
            .await
            .expect("accepted receipt");
        assert!(accepted.success);
        assert_eq!(accepted.current_count, Some(1));
        for kind in ["InconsistentBucket", "DispatchFailed"] {
            let refused = submit_training(&conn, json!({}))
                .await
                .expect("refusal is data");
            assert!(!refused.success);
            assert_eq!(refused.error_kind.as_deref(), Some(kind));
            assert!(refused.error.is_some());
        }
        assert!(
            submit_training(&conn, json!({})).await.is_err(),
            "missing success is malformed"
        );
        assert!(
            submit_training(&conn, json!({})).await.is_err(),
            "transport failure stays a failure"
        );
    }

    // what this catches: the quality gate. A substantive turn produces a plan
    // bucketed by its classified domain; a trivial one-liner ("ok") is gated out
    // (None) so the training corpus is never polluted with acknowledgements. This
    // is the L2 contract: "N graded turns fill the right bucket; low-quality turn
    // gated out" (DEV-TASK-LOOP-CLOSURE-PLAN.md).
    // what this catches (card cc34ac0f): a settled card must label the turns that
    // produced it, and the label must reach BOTH the bucket key and the example
    // metadata — otherwise the curriculum keeps grading itself on self-assessed
    // noteworthiness. Also pins the two regressions this change could cause:
    // an UNSTAMPED turn must key exactly as before, and the gym lookup must keep
    // using the BARE domain (a widened key has no gym, so keying on it would
    // silently drop the eval set from every credited example).
    //
    // what this catches: capture-time credit being treated as outcome-verified, and
    // `settle` ignoring the verdict it was handed. Card 0d51573a. Observed live
    // 2026-09-08 — a citizen accumulating toward a training threshold on a card still
    // in `Claimed` with no settlement, which must stay unstamped.
    //
    // what this catches: a FABRICATED claim receipt. `WorkCard::owner` and `claim_id`
    // are both Option. A turn rooted at a card carrying neither is STILL card-linked
    // — she stood in that checkout and her edits landed there — so it must stage. But
    // it must never acquire a receipt it did not have, and no later settlement may
    // supply one. If this ever defaulted the missing pair (to the acting persona, to
    // a nil claim), that turn becomes stampable and a citizen is credited for
    // accountability nobody recorded. Contract stated by Astra 2026-09-08.
    #[test]
    pub(super) fn a_claimless_card_stages_as_card_linked_without_inventing_a_receipt() {

        let bare = claimed_card_fixture(None, None);
        let captured = CapturedCredit::from_selected_card(&bare);
        assert_eq!(captured.card_id, bare.card_id.as_uuid());
        assert!(
            captured.is_card_linked(),
            "a claimless card is still card-linked — it stages, her edits landed there"
        );
        assert!(
            captured.claim.is_none(),
            "no receipt may be invented for a card the work store never claimed"
        );

        // POSITIVE CONTROL: a real owner+claim pair MUST produce a receipt. Without
        // this the assertion above would pass identically if `from_selected_card`
        // never populated a receipt at all.
        let owner = airc_core::PeerId::new();
        let claimed = claimed_card_fixture(
            Some(owner),
            Some(airc_work::ClaimId::from_uuid(Uuid::new_v4())),
        );
        let receipt = CapturedCredit::from_selected_card(&claimed)
            .claim
            .expect("a real owner+claim pair must produce a receipt");
        assert_eq!(
            receipt.owner, owner,
            "the receipt carries the store's own owner"
        );
        assert_eq!(receipt.role, CreditRole::Owner, "a claim yields Owner");

        // HALF A PAIR IS NOT A PAIR: an owner with no claim is not an accepted
        // claim, and a claim with no owner names nobody.
        assert!(
            CapturedCredit::from_selected_card(&claimed_card_fixture(Some(owner), None))
                .claim
                .is_none(),
            "an owner without a claim is not an accepted claim"
        );
        assert!(
            CapturedCredit::from_selected_card(&claimed_card_fixture(
                None,
                Some(airc_work::ClaimId::from_uuid(Uuid::new_v4()))
            ))
            .claim
            .is_none(),
            "a claim without an owner names nobody"
        );
    }

    // what this catches: a dropped `#[entity(indexed)]` on the staging keys. Field
    // presence alone is not the claim — settlement arrives PER CARD and asks "what
    // did this citizen stage against it", and playback joins on the SUBMITTED
    // request id. Without those indexes both become table scans on a store that
    // grows one row per card-linked turn, and nothing anywhere reports it: the
    // queries still return the right answers, just slower forever. Card 0d51573a.
    #[test]
    pub(super) fn staged_credit_indexes_the_three_keys_settlement_and_playback_join_on() {
        use crate::orm::OrmEntity;
        let schema = StagedCredit::collection_schema();
        assert_eq!(schema.collection, "staged_credit");

        let indexed: std::collections::BTreeSet<&str> = schema
            .fields
            .iter()
            .filter(|f| f.indexed)
            .map(|f| f.name.as_str())
            .collect();

        // The PARENT indexes what settlement looks up by. `submittedRequestId` is NOT
        // here any more: a cycle dispatches many generations, the receipts are an
        // opaque JSON column (the ORM has no collection FieldType), and nothing inside
        // a JSON column is indexable — so the correlation index moved to the child
        // rows. Asserting it on the parent would now assert a lie.
        for key in ["cardId", "claimId"] {
            assert!(
                indexed.contains(key),
                "staged_credit must INDEX {key:?} — settlement looks up by card; \
                 indexed fields are {indexed:?}"
            );
        }
        assert!(
            !indexed.contains("submittedRequestId"),
            "the correlation index belongs to staged_credit_generation now; leaving a \
             stale one here would suggest a lookup the parent cannot actually serve"
        );

        // POSITIVE CONTROL: a field deliberately NOT indexed must not be, or this test
        // would pass with `indexed` blanket-set and assert nothing about my attributes.
        assert!(
            !indexed.contains("prompt"),
            "prompt must not be indexed — if it is, this test cannot distinguish my \
             deliberate indexes from every field being indexed by default"
        );
    }

    // what this catches: the staging collection colliding with an existing one.
    // `OrmEntity::COLLECTION` must be unique across BOTH the Rust registry and
    // entity_schemas.json, and a collision is a registration-time hard error — i.e.
    // it fails at BOOT, on a citizen's machine, not here, unless this test exists.
    #[test]
    pub(super) fn staged_credit_registers_without_colliding_with_an_existing_collection() {
        use crate::orm::entity::OrmEntityRegistry;
        let registry = OrmEntityRegistry::new();
        registry
            .register::<StagedCredit>()
            .expect("StagedCredit must register cleanly");
        let resolved = registry
            .resolve("staged_credit")
            .expect("staged_credit collection must resolve");
        assert_eq!(resolved.collection, "staged_credit");
    }

    #[test]
    pub(super) fn a_verdict_is_required_to_stamp_and_settle_carries_the_verdict_it_was_given() {
        let classifier = DomainClassifier::new();
        let substantial = "Pool them behind one supervised task and hand out permits; a websocket \
             per request will exhaust file descriptors long before it exhausts memory, and the \
             reconnect storm is worse than the original load. Bound the pool and queue the \
             overflow, then shed load at the queue rather than at accept() — a refused connection \
             the caller can retry is cheaper than a half-open socket nobody owns. Size the permit \
             count from the descriptor ceiling, not from a guess, and make the queue depth the \
             knob you actually tune under pressure.";
        assert!(
            substantial.len() >= 500,
            "fixture must clear the substance ladder's 500-char step, or the failed \
             case returns None from the quality gate and the floor is never exercised"
        );

        let captured = CapturedCredit {
            task: crate::cognition::held_out::CardTask::NotATask,
            card_id: Uuid::from_u128(0x0d51573a),
            claim: Some(ClaimReceipt {
                claim_id: Uuid::from_u128(0x457b0d7d),
                owner: airc_core::PeerId::from_u128(0xe2f0e022),
                role: CreditRole::Owner,
            }),
        };

        // A PASSING verdict stamps and buckets on domain x role.
        let passed = captured
            .clone()
            .settle(SettlementOutcome::from_activity(
                &crate::cognition::activity::Verdict::pass("tests pass on the staged checkout"),
            ))
            .expect("test: a captured claim receipt can be stamped");
        assert!(
            passed.outcome,
            "settle must carry a PASSING verdict through"
        );
        let planned = plan(
            &classifier,
            "How do I pool websockets?",
            substantial,
            Some(passed),
        )
        .expect("test: a passing stamped turn clears the gate and plans a bucket");
        assert_eq!(
            planned.bucket_key(),
            format!("{}/owner", planned.trait_kind),
            "a settled passing turn keys on domain x role"
        );

        // A FAILING verdict must still STAMP (hiding the failure is not the job) and
        // must then be refused by the evidence floor. If `settle` hardcoded `true`,
        // this plans a bucket and the assertion below fails — that is the mutation.
        let failed = captured
            .settle(SettlementOutcome::from_activity(
                &crate::cognition::activity::Verdict::fail("the patch did not apply"),
            ))
            .expect("test: a receipt stamps regardless of which way the verdict went");
        assert!(
            !failed.outcome,
            "settle must carry a FAILING verdict through rather than defaulting to success"
        );
        assert!(
            plan(
                &classifier,
                "How do I pool websockets?",
                substantial,
                Some(failed)
            )
            .is_none(),
            "a turn credited to a FAILED card submits nothing — same text plans fine \
             when the verdict passes, so this None can only come from the evidence floor"
        );

        // what this catches: a settlement inventing the selection receipt it never
        // got. A turn rooted at a card whose `owner`/`claim_id` were absent is still
        // CARD-LINKED (so it stages, and must never fall through to immediate
        // unlinked SFT), but it can never be stamped — not by a passing verdict,
        // not by any verdict. If `settle` ever defaulted the missing receipt to an
        // Owner role, both assertions below flip.
        let claimless = CapturedCredit {
            task: crate::cognition::held_out::CardTask::NotATask,
            card_id: Uuid::from_u128(0x0d51573a),
            claim: None,
        };
        assert!(
            claimless.is_card_linked(),
            "a rooted turn on a claimless card is still card-linked — it stages, \
             because her edits landed in that checkout either way"
        );
        assert!(
            claimless
                .settle(SettlementOutcome::from_activity(
                    &crate::cognition::activity::Verdict::pass("tests pass on the staged checkout")
                ))
                .is_none(),
            "no receipt captured at selection means no stamp is constructible later — \
             a settlement cannot retroactively invent accountability"
        );
    }

mod part_2;
mod part_3;
