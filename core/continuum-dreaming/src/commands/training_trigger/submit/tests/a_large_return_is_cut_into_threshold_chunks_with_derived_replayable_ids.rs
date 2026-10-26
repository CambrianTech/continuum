#![cfg(test)]
use super::*;

    // what this catches (the 5090, 2026-10-11 02:15Z): a job's 311 examples returned as ONE
    // submission dispatched whole, past the run budget, which splits only at submissions. A return
    // larger than the threshold is cut into chunks of it, in order; chunk 0 keeps the job's
    // returned id (a small return is unchanged), every later chunk's id is distinct and derived,
    // so re-running the same return replays each chunk instead of copying it.
    #[test]
    pub(super) fn a_large_return_is_cut_into_threshold_chunks_with_derived_replayable_ids() {
        let chunks = return_chunks((0..40).collect::<Vec<u32>>(), 16);
        assert_eq!(chunks.iter().map(Vec::len).collect::<Vec<_>>(), vec![16, 16, 8]);
        assert_eq!(chunks.concat(), (0..40).collect::<Vec<u32>>(), "every example once, in order");
        let job = Uuid::new_v4();
        assert_eq!(returned_chunk_id(job, 0), returned_id(job), "chunk 0 is the job's returned id");
        let ids: std::collections::HashSet<Uuid> = (0..3).map(|i| returned_chunk_id(job, i)).collect();
        assert_eq!(ids.len(), 3, "each chunk its own identity");
        assert_eq!(returned_chunk_id(job, 2), returned_chunk_id(job, 2), "derived, so a re-run replays");
        assert_ne!(returned_chunk_id(job, 1), returned_chunk_id(Uuid::new_v4(), 1), "and per job");
        // A trained job's parked remainder (card 8a2334d9) never wears a whole return's id,
        // and its identity is the exact index set: the same set replays, another set is new.
        let parked = ReturnScope::Parked(&[0, 1, 4]);
        assert_eq!(ReturnScope::Whole.chunk_id(job, 0), returned_id(job), "a whole return is unchanged");
        assert_ne!(parked.chunk_id(job, 0), returned_id(job), "a remainder never collides with a whole return");
        assert_eq!(parked.chunk_id(job, 1), ReturnScope::Parked(&[0, 1, 4]).chunk_id(job, 1), "the same remainder replays");
        assert_ne!(parked.chunk_id(job, 0), ReturnScope::Parked(&[0, 1]).chunk_id(job, 0), "another index set is another return");
        assert_ne!(parked.chunk_id(job, 0), parked.chunk_id(job, 1), "each chunk its own identity");
    }

    // What this catches: received lessons replayed after loss of the source watermark
    // must use destination durability, not append another copy with a fresh UUID.
    #[tokio::test]
    pub(super) async fn received_lesson_identity_survives_restart_without_duplicate_examples() {
        use crate::commands::training_trigger::test_support::build_runtime;
        use crate::persona::domain_classifier::DomainClassifier;
        use crate::persona::training_producer::received_submission_params;
        let (adapter, _dir) = crate::orm::store::fresh_adapter().await;
        let (first, executor) = build_runtime(adapter.clone(), false).await;
        let persona = Uuid::from_u128(71);
        let classifier = DomainClassifier::new();
        let record: crate::memory::MemoryRecord = serde_json::from_value(serde_json::json!({
            "id": "lesson-71", "persona_id": persona.to_string(), "memory_type": "shared",
            "content": "Rust compiler diagnostics identify the actual type error; verify the correction with cargo check.",
            "context": { "scope": "rust code", "shared_by": "teacher-9", "original_author": "teacher-9" },
            "timestamp": "2026-09-23T00:00:00Z", "importance": 0.6, "access_count": 0,
            "tags": [], "related_to": [], "source": "shared:teacher-9"
        })).expect("test: valid persisted shared lesson");
        let request =
            received_submission_params(persona, "recipient", "synthetic", &classifier, &record);
        let first_receipt = executor
            .execute_json("genome/training-trigger/submit", request.clone())
            .await
            .expect("test: trigger dispatch");
        assert_eq!(first_receipt["outcome"], "BatchAppended", "{first_receipt}");
        assert_eq!(
            first_receipt["acceptance"]["submissionId"],
            request["submissionId"]
        );
        drop(executor);
        drop(first);
        let (next, executor) = build_runtime(adapter, false).await;
        // Rebuild the producer request, just as a fresh dream owner with no watermark does.
        let replay =
            received_submission_params(persona, "recipient", "synthetic", &classifier, &record);
        let receipt = executor
            .execute_json("genome/training-trigger/submit", replay.clone())
            .await
            .expect("test: replay dispatch");
        assert_eq!(receipt["acceptance"]["replayed"], true, "{receipt}");
        assert_eq!(
            receipt["acceptance"]["submissionId"],
            request["submissionId"]
        );
        let key = BucketKey {
            persona_id: persona,
            trait_kind: request["traitKind"]
                .as_str()
                .expect("test: producer trait")
                .into(),
            base_model: "synthetic".into(),
        };
        {
            let batch = next
                .state
                .buckets
                .get(&key)
                .expect("test: replay hydrated bucket");
            assert_eq!(batch.examples.len(), 1);
            let metadata = batch.examples[0]
                .metadata
                .as_ref()
                .expect("test: received provenance");
            assert_eq!(metadata["memoryRecordId"], record.id);
            assert_eq!(metadata["shared_by"], "teacher-9");
            assert!(
                metadata.get("cardId").is_none(),
                "received knowledge cannot fabricate work credit"
            );
        }
        let mut changed = record.clone();
        changed
            .content
            .push_str(" The correction must also pass its focused regression.");
        for conflicting in [
            received_submission_params(persona, "recipient", "synthetic", &classifier, &changed),
            received_submission_params(
                persona,
                "renamed-recipient",
                "synthetic",
                &classifier,
                &record,
            ),
        ] {
            assert_eq!(conflicting["submissionId"], request["submissionId"]);
            let refused = executor
                .execute_json("genome/training-trigger/submit", conflicting)
                .await
                .expect("test: conflicting replay dispatch");
            assert_eq!(refused["errorKind"], "SubmissionConflict", "{refused}");
            assert!(refused.get("acceptance").is_none());
        }
        assert_eq!(
            next.state
                .bucket_example_count(persona, &key.trait_kind, "synthetic"),
            Some(1)
        );
        for distinct in [
            received_submission_params(
                Uuid::from_u128(72),
                "recipient",
                "synthetic",
                &classifier,
                &record,
            ),
            received_submission_params(persona, "recipient", "another-base", &classifier, &record),
        ] {
            assert_ne!(distinct["submissionId"], request["submissionId"]);
        }
    }

    // What this catches (278afa6c): a BatchAppended receipt must survive a new
    // owner/connection, preserve metadata/order, and dedupe exact retries only.
    #[tokio::test]
    pub(super) async fn keyed_acceptance_survives_restart_and_refuses_changed_replay() {
        use crate::commands::training_trigger::test_support::build_runtime;
        use crate::orm::SqliteAdapter;
        let (adapter, dir) = crate::orm::store::fresh_adapter().await;
        let (first, first_exec) = build_runtime(adapter, false).await;
        let persona = Uuid::new_v4();
        // Reverse UUID order proves restart uses accepted order, not random IDs.
        let first_id = Uuid::from_u128(900);
        let second_id = Uuid::from_u128(100);
        let mut a = submit_params(persona, "code", vec![ex("earlier", "answer-a")], Some(100));
        a["submissionId"] = serde_json::json!(first_id);
        a["examples"][0]["metadata"] = serde_json::json!({"cardId": "card-a", "role": "reviewer"});
        let mut b = submit_params(persona, "code", vec![ex("later", "answer-b")], Some(100));
        b["submissionId"] = serde_json::json!(second_id);
        for request in [&a, &b] {
            let receipt = first_exec
                .execute_json("genome/training-trigger/submit", request.clone())
                .await
                .unwrap();
            assert_eq!(receipt["success"], true, "{receipt}");
            assert_eq!(receipt["acceptance"]["replayed"], false);
        }
        drop(first_exec);
        drop(first);
        let adapter = SqliteAdapter::open(&dir.path().join("orm-store-test.sqlite"))
            .await
            .expect("reopen the same database");
        let (next, executor) = build_runtime(Arc::new(adapter), false).await;
        crate::runtime::ServiceModule::tick(next.as_ref())
            .await
            .unwrap();
        let key = BucketKey {
            persona_id: persona,
            trait_kind: "code".into(),
            base_model: "synthetic".into(),
        };
        {
            let batch = next.state.buckets.get(&key).unwrap();
            assert_eq!(batch.submission_ids, vec![first_id, second_id]);
            // the per-submission counts the run budget splits by are rebuilt from the durable rows
            // (they are not serialized), so a restart never merges pending chunks into one run
            assert_eq!(batch.submission_examples, vec![1, 1]);
            assert_eq!(batch.examples[0].prompt, "earlier");
            assert_eq!(
                batch.examples[0].metadata.as_ref().unwrap()["role"],
                "reviewer"
            );
            assert_eq!(batch.examples[1].completion, "answer-b");
        }
        let replay = executor
            .execute_json("genome/training-trigger/submit", a.clone())
            .await
            .unwrap();
        assert_eq!(replay["acceptance"]["submissionId"], first_id.to_string());
        assert_eq!(replay["acceptance"]["replayed"], true);
        assert_eq!(
            next.state
                .bucket_example_count(persona, "code", "synthetic"),
            Some(2)
        );
        for field in ["completion", "policy"] {
            let mut changed = a.clone();
            if field == "completion" {
                changed["examples"][0]["completion"] = "different".into();
            } else {
                changed["minExamples"] = 99.into();
            }
            let refused = executor
                .execute_json("genome/training-trigger/submit", changed)
                .await
                .unwrap();
            assert_eq!(refused["success"], false);
            assert_eq!(refused["errorKind"], "SubmissionConflict");
            assert!(refused.get("acceptance").is_none());
        }
        // Legacy calls retain explicit at-least-once semantics: no key means a
        // new durable submission, even if its contents equal an earlier call.
        a.as_object_mut().unwrap().remove("submissionId");
        let one = executor
            .execute_json("genome/training-trigger/submit", a.clone())
            .await
            .unwrap();
        let two = executor
            .execute_json("genome/training-trigger/submit", a)
            .await
            .unwrap();
        assert_ne!(
            one["acceptance"]["submissionId"],
            two["acceptance"]["submissionId"]
        );
        assert_eq!(
            next.state
                .bucket_example_count(persona, "code", "synthetic"),
            Some(4)
        );
    }

    // What this catches (278afa6c): no storage readiness, no ownership receipt;
    // a successful in-memory append may never stand in for durable acceptance.
    #[tokio::test]
    pub(super) async fn unavailable_persistence_refuses_without_acceptance() {
        use crate::modules::training_trigger::TrainingTriggerModule;
        use crate::runtime::{CommandExecutor, ModuleRegistry};
        let registry = Arc::new(ModuleRegistry::new());
        let trigger = Arc::new(TrainingTriggerModule::new());
        registry.register(trigger.clone());
        let executor = CommandExecutor::new(registry);
        let receipt = executor
            .execute_json(
                "genome/training-trigger/submit",
                submit_params(Uuid::new_v4(), "code", vec![ex("p", "c")], Some(100)),
            )
            .await
            .unwrap();
        assert_eq!(receipt["success"], false);
        assert_eq!(receipt["errorKind"], "PersistenceUnavailable");
        assert!(receipt.get("acceptance").is_none());
        assert_eq!(trigger.state.pending_bucket_count(), 0);
    }

    // what this catches: name/access wiring — submitting a batch can spend training
    // compute (threshold-crossing dispatch), so it lives on the Privileged surface,
    // not AiSafe.
    #[test]
    pub(super) fn name_and_access_wired() {
        assert_eq!(
            TrainingTriggerSubmit::NAME,
            "genome/training-trigger/submit"
        );
        assert!(matches!(
            TrainingTriggerSubmit::ACCESS,
            AccessLevel::Privileged
        ));
    }

    // what this catches: under the threshold, submit returns BatchAppended and the
    // examples accumulate in the bucket. A regression that fired job-create on every
    // submit would surface as JobDispatched on the very first call.
    #[tokio::test]
    pub(super) async fn submit_below_threshold_appends_and_does_not_fire() {
        let (trigger, executor, _dir) = build_runtime_with_trigger_and_genome().await;
        let persona = Uuid::new_v4();

        let params = submit_params(persona, "test-trait", vec![ex("a", "b")], Some(5));
        let json = executor
            .execute_json("genome/training-trigger/submit", params)
            .await
            .expect("submit ok");

        assert_eq!(json["success"], true);
        assert_eq!(json["outcome"], "BatchAppended");
        assert_eq!(json["currentCount"], 1);
        assert_eq!(json["threshold"], 5);
        assert_eq!(
            trigger
                .state
                .bucket_example_count(persona, "test-trait", "synthetic"),
            Some(1)
        );
    }

    // what this catches: hitting threshold dispatches and clears the bucket. Without
    // this, examples would pile up forever and the substrate's training loop would
    // never close.
    #[tokio::test]
    pub(super) async fn submit_at_threshold_dispatches_and_clears() {
        let (trigger, executor, _dir) = build_runtime_with_trigger_and_genome().await;
        let persona = Uuid::new_v4();

        // First submit: 4 examples, threshold 5 → BatchAppended.
        let _ = executor
            .execute_json(
                "genome/training-trigger/submit",
                submit_params(
                    persona,
                    "test-trait",
                    vec![ex("a", "b"), ex("c", "d"), ex("e", "f"), ex("g", "h")],
                    Some(5),
                ),
            )
            .await
            .unwrap();
        assert_eq!(
            trigger
                .state
                .bucket_example_count(persona, "test-trait", "synthetic"),
            Some(4)
        );

        // Second submit: 1 more example → 5 → fires.
        let json = executor
            .execute_json(
                "genome/training-trigger/submit",
                submit_params(persona, "test-trait", vec![ex("i", "j")], Some(5)),
            )
            .await
            .unwrap();
        assert_eq!(json["success"], true, "submit must succeed, got: {json}");
        assert_eq!(json["outcome"], "JobDispatched");
        assert_eq!(json["examplesUsed"], 5);
        assert_eq!(json["selectedProvider"], "local-candle");
        assert!(json["jobHandle"]["localId"].is_string());

        // Bucket must be cleared.
        assert_eq!(
            trigger
                .state
                .bucket_example_count(persona, "test-trait", "synthetic"),
            None
        );
        assert_eq!(trigger.state.pending_bucket_count(), 0);

        // The per-key gate map MUST also be empty after a successful dispatch — the
        // RAII Lease evicts structurally on drop. This pins the structural invariant
        // in a production-shaped test (full submit → dispatch → success path).
        assert_eq!(
            trigger.state.submit_gates.len(),
            0,
            "submit_gates MUST be empty after successful dispatch — \
             a non-zero count indicates the structural-eviction-on-drop \
             invariant is broken"
        );
    }

    // what this catches: same persona + same trait_kind submitted with DIFFERENT
    // base_model values gets DIFFERENT buckets — a persona legitimately trains the
    // same trait against multiple bases for routing flexibility. The pre-fix behavior
    // rejected the second submit with InconsistentBucket and silently dropped its
    // data; base_model is now in the bucket key so the submits accumulate
    // independently.
    // what this catches (the 5090, 2026-10-05 13:17Z: ten Mints for one competence from
    // one card's credit, GENE-REUSE-FORK-MINT.md falsifier #2): while a job for this
    // bucket's (persona, trait, base) is on the job board, a second fill to threshold is
    // HELD with every example retained, never dispatched beside the job that is training
    // the same competence; the fill after that job lands decides against its gene.
    #[tokio::test]
    pub(super) async fn a_second_fill_while_a_job_trains_the_competence_is_held_not_minted() {
        let (trigger, executor, _dir) = build_runtime_with_trigger_and_genome().await;
        let persona = Uuid::new_v4();
        let first = executor
            .execute_json(
                "genome/training-trigger/submit",
                submit_params(persona, "test-trait", vec![ex("a", "b"), ex("c", "d"), ex("e", "f"), ex("g", "h"), ex("i", "j")], Some(5)),
            )
            .await
            .unwrap();
        assert_eq!(first["outcome"], "JobDispatched", "{first}");
        assert_eq!(trigger.state.test_job_board.snapshot().len(), 1, "one job on the board for this competence");

        let second = executor
            .execute_json(
                "genome/training-trigger/submit",
                submit_params(persona, "test-trait", vec![ex("k", "l"), ex("m", "n"), ex("o", "p"), ex("q", "r"), ex("s", "t")], Some(5)),
            )
            .await
            .unwrap();
        assert_eq!(second["success"], true, "{second}");
        assert_ne!(second["outcome"], "JobDispatched", "a second job beside the first is the falsifier: {second}");
        assert_eq!(second["heldBy"]["job"], first["jobHandle"]["localId"], "the outcome names the job the examples wait for: {second}");
        assert_eq!(
            trigger.state.bucket_example_count(persona, "test-trait", "synthetic"),
            Some(5),
            "every example of the held fill is retained for the fill after the job lands"
        );
        assert_eq!(trigger.state.test_job_board.snapshot().len(), 1, "still one job");
    }

    // what this catches (GENE-REUSE-FORK-MINT §2, Joel: a gene is minted only when recall
    // alone leaves her surprised): while the room confirms her stated expectations (her
    // verdict surprise below the floor) a full bucket is HELD, not minted, and the
    // outcome says so with the number; one contradicted expectation lifts her surprise
    // over the floor and the next fill dispatches. A mind never judged is not held.
    #[tokio::test]
    pub(super) async fn a_fill_while_her_expectations_hold_is_held_and_a_contradiction_releases_it() {
        use crate::cognition::attention::{AttentionDial, Continuation};
        use crate::persona::perception_region::PerceptionRegion;
        use crate::cognition::conversation::salience::{BoardChange, Expectation, ExpectedVerdict, ObservedVerdict};
        use std::sync::{Arc, Mutex};
        let (trigger, executor, _dir) = build_runtime_with_trigger_and_genome().await;
        let persona = Uuid::new_v4();
        let activity = Uuid::new_v4();
        let mine = Uuid::new_v4();
        let reviewer = Uuid::new_v4();
        // The real clock: a tally is read inside its window, at the snapshot's own time.
        let now = continuum_base::time::EpochMs::now().millis();
        let mind = tempfile::tempdir().unwrap();
        let (mut region, _strip) = PerceptionRegion::boot(airc_core::PeerId::from_uuid(persona), mind.path(), 1_000, now);
        region.join(activity, "career-wrangler");
        region.set_identity_facts(vec![], vec![mine]);
        region.set_dial(AttentionDial::broad(), now);
        region.set_continuation(
            Some(Continuation {
                activity,
                note: "submitted; expect a pass".into(),
                expectation: Some(Expectation { text: "review passes".into(), by_ms: None, verdict: Some(ExpectedVerdict::Passed) }),
                written_at_ms: now,
            }),
            now,
        );
        // Three reviews that confirm her (MIN_JUDGED): surprise 0 of 3.
        for at in 1..=3u64 {
            region.observe_board(activity, vec![BoardChange::Reviewed { card_id: mine, outcome: ObservedVerdict::Passed, reviewer, review: Uuid::new_v4() }], now + at);
        }
        let region = Arc::new(Mutex::new(region));
        let (wake_tx, _wake_rx) = tokio::sync::mpsc::channel(4);
        crate::persona::perception_feed::register(persona, airc_core::PeerId::from_uuid(persona), "Kimi", region.clone(), wake_tx, None);

        let fill = executor
            .execute_json(
                "genome/training-trigger/submit",
                submit_params(persona, "test-trait", vec![ex("a", "b"), ex("c", "d"), ex("e", "f"), ex("g", "h"), ex("i", "j")], Some(5)),
            )
            .await
            .unwrap();
        assert_eq!(fill["success"], true, "{fill}");
        assert_ne!(fill["outcome"], "JobDispatched", "a mint while her expectations hold is the falsifier: {fill}");
        assert_eq!(fill["heldBy"]["kind"], "unsurprised", "{fill}");
        assert_eq!(fill["heldBy"]["s"], 0.0, "{fill}");
        assert!(trigger.state.test_job_board.snapshot().is_empty(), "no job while she is unsurprised");

        // The room contradicts her once: 1 of 4, at the floor. The next fill decides.
        region.lock().unwrap().observe_board(activity, vec![BoardChange::Reviewed { card_id: mine, outcome: ObservedVerdict::Failed, reviewer, review: Uuid::new_v4() }], now + 4);
        let next = executor
            .execute_json("genome/training-trigger/submit", submit_params(persona, "test-trait", vec![ex("k", "l")], Some(5)))
            .await
            .unwrap();
        assert_eq!(next["outcome"], "JobDispatched", "her surprise rose: the bucket decides: {next}");
        crate::persona::perception_feed::unregister(persona);
    }

    // what this catches (card 17dc0a7b, then 70a35a29): a gene of hers that JUST LANDED
    // for this bucket's competence holds the bucket while it settles, exactly as a job in
    // flight does. Without it, the next fill reads the gene as resident and forks it before
    // her turns have told what it does: a fork storm in place of the mint storm Join closed.
    // Past the settling window the fill decides on its own; the gene stays in her head.
    #[tokio::test]
    pub(super) async fn a_fill_while_a_landed_gene_is_settling_is_held_not_forked() {
        let (trigger, executor, _dir) = build_runtime_with_trigger_and_genome().await;
        let persona = Uuid::new_v4();
        let gene = std::env::temp_dir().join(format!("on-trial-{persona}.gguf"));
        let trial = trigger
            .state
            .test_trials
            .open(persona, "test-trait", &gene, "synthetic", continuum_base::time::EpochMs::now().millis())
            .expect("the trial file takes a trial");
        assert_eq!(trial.state, crate::genome::gene_trial::TrialState::Trial);

        let fill = executor
            .execute_json(
                "genome/training-trigger/submit",
                submit_params(persona, "test-trait", vec![ex("a", "b"), ex("c", "d"), ex("e", "f"), ex("g", "h"), ex("i", "j")], Some(5)),
            )
            .await
            .unwrap();
        assert_eq!(fill["success"], true, "{fill}");
        assert_ne!(fill["outcome"], "JobDispatched", "a job beside an open trial is the falsifier: {fill}");
        assert_eq!(trigger.state.bucket_example_count(persona, "test-trait", "synthetic"), Some(5), "every example waits for her verdict");
        assert!(trigger.state.test_job_board.snapshot().is_empty(), "no job while the trial is open");
        assert_eq!(fill["heldBy"]["trial"], trial.id.to_string(), "the outcome names the trial the examples wait for: {fill}");

        // The gene has settled (its window passed; moved by hand on the same row the
        // clock would read): the file no longer holds the bucket, and the gene is still hers.
        let mut all = trigger.state.test_trials.load().unwrap();
        all[0].opened_at_ms = all[0].opened_at_ms.saturating_sub(crate::genome::gene_trial::SETTLE_WINDOW_MS + 1);
        trigger.state.test_trials.save_for_test(&all).unwrap();
        assert_eq!(all[0].state, crate::genome::gene_trial::TrialState::Trial, "settling is not a verdict");
        let next = executor
            .execute_json(
                "genome/training-trigger/submit",
                submit_params(persona, "test-trait", vec![ex("k", "l")], Some(5)),
            )
            .await
            .unwrap();
        assert_eq!(next["outcome"], "JobDispatched", "a settled gene releases the bucket: {next}");
    }

    #[tokio::test]
    pub(super) async fn different_base_models_create_separate_buckets() {
        let (trigger, executor, _dir) = build_runtime_with_trigger_and_genome().await;
        let persona = Uuid::new_v4();

        // First submit: base_model = "synthetic".
        let _ = executor
            .execute_json(
                "genome/training-trigger/submit",
                submit_params(persona, "test-trait", vec![ex("a", "b")], Some(100)),
            )
            .await
            .unwrap();

        // Second submit: SAME persona, SAME trait, DIFFERENT base.
        let mut other_base = submit_params(persona, "test-trait", vec![ex("c", "d")], Some(100));
        other_base
            .as_object_mut()
            .unwrap()
            .insert("baseModel".into(), Value::String("synthetic-tiny".into()));
        let json = executor
            .execute_json("genome/training-trigger/submit", other_base)
            .await
            .unwrap();
        // No more InconsistentBucket — the second submit succeeds because it lives in
        // its own bucket.
        assert_eq!(
            json["success"], true,
            "second-base submit must succeed: {json}"
        );
        assert_eq!(json["outcome"], "BatchAppended");

        // Two distinct buckets pending, one example each.
        assert_eq!(
            trigger
                .state
                .bucket_example_count(persona, "test-trait", "synthetic"),
            Some(1)
        );
        assert_eq!(
            trigger
                .state
                .bucket_example_count(persona, "test-trait", "synthetic-tiny"),
            Some(1)
        );
        assert_eq!(trigger.state.pending_bucket_count(), 2);
    }

    // what this catches: coherence check covers `lora` hyperparameters. A second
    // submit with different lora params gets InconsistentBucket so the caller learns
    // their config was rejected instead of silently overridden.
    #[tokio::test]
    pub(super) async fn inconsistent_lora_in_same_bucket_is_rejected() {
        use crate::genome::fine_tuning::types::LoRAHyperparams;
        let (trigger, executor, _dir) = build_runtime_with_trigger_and_genome().await;
        let persona = Uuid::new_v4();

        let mut first = submit_params(persona, "test-trait", vec![ex("a", "b")], Some(100));
        first.as_object_mut().unwrap().insert(
            "lora".into(),
            serde_json::to_value(LoRAHyperparams {
                rank: 8,
                alpha: 16,
                dropout: 0.0,
                target_modules: vec![],
                top_layers: None,
            })
            .unwrap(),
        );
        let _ = executor
            .execute_json("genome/training-trigger/submit", first)
            .await
            .unwrap();

        let mut wrong_lora = submit_params(persona, "test-trait", vec![ex("c", "d")], Some(100));
        wrong_lora.as_object_mut().unwrap().insert(
            "lora".into(),
            serde_json::to_value(LoRAHyperparams {
                rank: 16, // different
                alpha: 32,
                dropout: 0.0,
                target_modules: vec![],
                top_layers: None,
            })
            .unwrap(),
        );
        let json = executor
            .execute_json("genome/training-trigger/submit", wrong_lora)
            .await
            .unwrap();
        assert_eq!(json["success"], false, "got: {json}");
        assert_eq!(json["errorKind"], "InconsistentBucket");
        // First-arrival's bucket survives intact.
        assert_eq!(
            trigger
                .state
                .bucket_example_count(persona, "test-trait", "synthetic"),
            Some(1)
        );
    }

    // what this catches: same coherence guarantee for ScheduleParams. A submit with
    // different `epochs` or `learning_rate` is rejected with InconsistentBucket rather
    // than silently using the first-arrival's schedule.
    #[tokio::test]
    pub(super) async fn inconsistent_schedule_in_same_bucket_is_rejected() {
        use crate::genome::fine_tuning::types::ScheduleParams;
        let (_trigger, executor, _dir) = build_runtime_with_trigger_and_genome().await;
        let persona = Uuid::new_v4();

        let mut first = submit_params(persona, "test-trait", vec![ex("a", "b")], Some(100));
        first.as_object_mut().unwrap().insert(
            "schedule".into(),
            serde_json::to_value(ScheduleParams {
                epochs: 3,
                batch_size: 4,
                sequence_length: 32,
                learning_rate: 1e-4,
            })
            .unwrap(),
        );
        let _ = executor
            .execute_json("genome/training-trigger/submit", first)
            .await
            .unwrap();

        let mut wrong_schedule =
            submit_params(persona, "test-trait", vec![ex("c", "d")], Some(100));
        wrong_schedule.as_object_mut().unwrap().insert(
            "schedule".into(),
            serde_json::to_value(ScheduleParams {
                epochs: 5, // different
                batch_size: 4,
                sequence_length: 32,
                learning_rate: 1e-4,
            })
            .unwrap(),
        );
        let json = executor
            .execute_json("genome/training-trigger/submit", wrong_schedule)
            .await
            .unwrap();
        assert_eq!(json["success"], false);
        assert_eq!(json["errorKind"], "InconsistentBucket");
    }

    // what this catches: source coherence is still enforced even after base_model
    // moved into the key. A submit with `source: "operator_curated"` to a bucket that
    // already exists from a `source: "teacher_synthesized"` submit must be rejected —
    // the alloy provenance contract distinguishes those origins.
    #[tokio::test]
    pub(super) async fn inconsistent_source_in_same_bucket_is_rejected() {
        let (trigger, executor, _dir) = build_runtime_with_trigger_and_genome().await;
        let persona = Uuid::new_v4();

        let _ = executor
            .execute_json(
                "genome/training-trigger/submit",
                submit_params(persona, "test-trait", vec![ex("a", "b")], Some(100)),
            )
            .await
            .unwrap();

        let mut wrong_source = submit_params(persona, "test-trait", vec![ex("c", "d")], Some(100));
        wrong_source
            .as_object_mut()
            .unwrap()
            .insert("source".into(), Value::String("teacher_synthesized".into()));
        let json = executor
            .execute_json("genome/training-trigger/submit", wrong_source)
            .await
            .unwrap();
        assert_eq!(json["success"], false);
        assert_eq!(json["errorKind"], "InconsistentBucket");
        // First-arrival's source survives intact.
        assert_eq!(
            trigger
                .state
                .bucket_example_count(persona, "test-trait", "synthetic"),
            Some(1)
        );
    }
