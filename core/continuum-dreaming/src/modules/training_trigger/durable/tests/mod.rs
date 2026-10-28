#![cfg(test)]

    use super::*;
    use crate::commands::training_trigger::test_support::{build_runtime, ex, submit_params};
    use crate::runtime::ServiceModule;

    // what this catches (the 5090, 2026-10-10): a bucket that filled all day dispatched as one
    // ~100-hour run. With no measured rate a run takes the OLDEST whole submissions up to the
    // bucket's threshold (the drain: none starves) and leaves the rest pending, in order; a bucket
    // whose per-submission counts do not cover its examples is never split.
    #[test]
    pub(super) fn a_dispatch_takes_whole_oldest_submissions_and_leaves_the_rest_pending() {
        let ex = |p: &str| crate::genome::fine_tuning::TrainingExample { prompt: p.into(), completion: "c".into(), metadata: None, lived: None };
        let ids: Vec<Uuid> = (0..4).map(|_| Uuid::new_v4()).collect();
        let batch = PendingBatch {
            submission_ids: ids.clone(),
            submission_examples: vec![2, 1, 3, 1],
            submission_returned: vec![false; 4],
            returned: false,
            persona_name: "Kimi".into(),
            source: crate::genome::fine_tuning::TrainingSource::OperatorCurated,
            examples: ["a1", "a2", "b1", "c1", "c2", "c3", "d1"].iter().map(|p| ex(p)).collect(),
            lora: None,
            schedule: None,
            local_artifact_dir: None,
            preferred_provider: None,
            min_examples: 3,
            validation_split: 0.0,
            eval_set: None,
        };
        let (taken, rest) = budget_split(batch.clone(), "no-rate-for-this-base");
        let rest = rest.expect("over the threshold: split");
        assert_eq!(taken.submission_ids, ids[..2].to_vec(), "the oldest whole submissions, to the threshold");
        assert_eq!(taken.examples.iter().map(|e| e.prompt.as_str()).collect::<Vec<_>>(), ["a1", "a2", "b1"], "with exactly their examples");
        assert_eq!(rest.submission_ids, ids[2..].to_vec(), "the newer ones stay pending, in order");
        assert_eq!(rest.examples.iter().map(|e| e.prompt.as_str()).collect::<Vec<_>>(), ["c1", "c2", "c3", "d1"]);
        let mut unknown = batch;
        unknown.submission_examples.pop();
        let (whole, none) = budget_split(unknown, "no-rate-for-this-base");
        assert!(none.is_none() && whole.examples.len() == 7, "counts that do not cover the examples: never split");
    }

    pub(super) fn pause_at(state: &TrainingTriggerState, point: OperationPoint) -> Arc<OperationPause> {
        let pause = Arc::new(OperationPause {
            point,
            entered: tokio::sync::Notify::new(),
            resume: tokio::sync::Notify::new(),
        });
        *state.operation_pause.lock().unwrap() = Some(pause.clone());
        pause
    }

    // Regression for #4858: changing return identity must neither duplicate a legacy
    // successful return nor replay the original submission as if it were a return.
    // One ORM setup covers legacy ambiguity and proven equal/different-payload collisions.
    #[tokio::test]
    pub(super) async fn return_identity_upgrade_requires_original_dispatch_proof() {
        use crate::commands::training_trigger::submit::{return_request, returned_id};
        let (adapter, _dir) = crate::orm::store::fresh_adapter().await;
        let (old, executor) = build_runtime(adapter.clone(), false).await;
        let artifacts = tempfile::tempdir().unwrap();
        let board = crate::genome::fine_tuning::TrainingJobBoard::with_ledger(Some(artifacts.path().join("jobs.jsonl")));
        let mut cases = Vec::new();
        // 0: old return still pending; 1/2: original submission, equal/different
        // returned payload; 3: current job has two differently named submissions and
        // no request directory; 4: old return already dispatched into a NEW job.
        for scenario in 0..5 {
            let job = Uuid::new_v4();
            let submission = if scenario == 3 { Uuid::new_v4() } else { job };
            let persona = Uuid::new_v4();
            let original = vec![ex("original", "answer")];
            let mut params = submit_params(persona, "code", original.clone(), Some(super::super::DEFAULT_MIN_EXAMPLES));
            params["submissionId"] = serde_json::json!(submission);
            let result = executor.execute_json("genome/training-trigger/submit", params).await.unwrap();
            assert_eq!(result["success"], true, "{result}");
            let key = BucketKey { persona_id: persona, trait_kind: "code".into(), base_model: "synthetic".into() };
            let store = old.state.durable.require().unwrap();
            if scenario != 0 {
                let dispatched_job = if scenario == 4 { Uuid::new_v4() } else { job };
                let mut submission_ids = vec![submission];
                if scenario == 3 {
                    let extra = Uuid::new_v4();
                    let mut params = submit_params(persona, "code", vec![ex("extra", "answer")], Some(super::super::DEFAULT_MIN_EXAMPLES));
                    params["submissionId"] = serde_json::json!(extra);
                    assert_eq!(executor.execute_json("genome/training-trigger/submit", params).await.unwrap()["success"], true);
                    submission_ids.push(extra);
                }
                // Persist the same dispatch ownership that dispatch_pending/finish_dispatch
                // writes. Merely accepting a batch under the job UUID is NOT this proof.
                let intent = DispatchIntent {
                    base: BaseEntity::for_new_record(),
                    persona_id: persona,
                    trait_kind: key.trait_kind.clone(),
                    base_model: key.base_model.clone(),
                    submission_ids,
                    phase: DispatchPhase::Dispatched {
                        handle: JobHandle { provider_id: "fixture".into(), provider_job_id: dispatched_job.to_string(), local_id: dispatched_job },
                        provider: "fixture".into(),
                    },
                    is_active: false,
                    attempt: None,
                };
                let dispatch = entity_id(&intent.base).unwrap();
                store.dispatches.save(dispatch, &intent).await.unwrap();
                store.assign(&intent).await.unwrap();
                assert!(old.state.dispatched_request(&key, dispatch, Uuid::new_v4()).await.is_err(), "a submission's dispatch is not a different job's input");
                if scenario == 3 {
                    board.register(crate::genome::fine_tuning::job_board::WatchedJob {
                        trigger_dispatch_id: Some(dispatch),
                        handle: JobHandle { provider_id: "fixture".into(), provider_job_id: job.to_string(), local_id: job },
                        persona_id: persona, persona_name: "test-p".into(), base_model: "synthetic".into(), trait_kind: "code".into(),
                        eval_set: None, signature: None, decision: None,
                    });
                    board.claim(job, &crate::genome::fine_tuning::types::TrainingStatus::Failed { error: "refused before training".into() });
                }
            }
            let examples = match scenario {
                2 => vec![ex("returned-1", "a"), ex("returned-2", "b")],
                3 => vec![ex("original", "answer"), ex("extra", "answer")],
                _ => original,
            };
            let request: crate::genome::fine_tuning::types::TrainingJobRequest = serde_json::from_value(serde_json::json!({
                "personaId": persona, "personaName": "test-p", "baseModel": "synthetic", "traitKind": "code",
                "dataset": { "examples": examples, "source": "operator_curated", "validationSplit": 0.0 }
            })).unwrap();
            cases.push((scenario, job, key, request));
        }
        old.shutdown().await.unwrap();
        drop(executor);
        drop(old);
        let (next, _executor) = build_runtime(adapter, false).await;
        for (scenario, job, key, request) in cases {
            for attempt in 0..2 {
                let result = if scenario == 3 {
                    // The canonical job has no request.json, and its two submissions
                    // have different UUIDs. No historical directory may stand in for it.
                    crate::commands::training_trigger::return_::return_job(&next.state, &board, artifacts.path(), job).await.unwrap()
                } else {
                    return_request(&next.state, request.clone(), job, crate::commands::training_trigger::submit::ReturnScope::Whole).await.unwrap()
                };
                if scenario == 0 || scenario == 4 {
                    assert_eq!(result.error_kind.as_deref(), Some("RecoveryRequired"), "{result:?}");
                    assert!(next.state.durable.require().unwrap().submissions.find_by_id(returned_id(job)).await.unwrap().is_none(), "no duplicate legacy return acceptance");
                } else {
                    assert!(result.success, "{result:?}");
                    assert_eq!(next.state.bucket_example_count(key.persona_id, &key.trait_kind, &key.base_model), Some(request.dataset.examples.len()));
                    {
                        let pending = next.state.buckets.get(&key).unwrap();
                        assert_eq!(serde_json::to_value(&pending.examples).unwrap(), serde_json::to_value(&request.dataset.examples).unwrap(), "return preserves the dispatched examples and their order");
                    }
                    let acceptance = result.acceptance.unwrap();
                    assert_eq!(acceptance.submission_id, returned_id(job));
                    assert_eq!(acceptance.replayed, attempt != 0);
                }
            }
            if scenario == 0 || scenario == 4 {
                let legacy = next.state.durable.require().unwrap().submissions.find_by_id(job).await.unwrap().unwrap();
                assert_eq!(legacy.is_pending, scenario == 0);
                assert_eq!(legacy.batch.examples.len(), 1, "old successful acceptance remains intact");
            }
        }
        next.shutdown().await.unwrap();
    }

    // What this catches (card 8a2334d9; Astra on #5009): the remainder of a CANONICAL completed
    // job, laid out as Kimi's 55f2e6de is on the 5090: adapters/ and partition.json, NO
    // request.json, its input provable only through its durable dispatch. Sixteen originals
    // (each carrying its own provenance), sent [2,3,7], the other thirteen parked. The core dies
    // after the bucket accepted the remainder but before its receipt; the next owner replays it
    // by identity, never appends it twice, writes the receipt, and returns exactly those thirteen
    // payloads unchanged, under the native base. The full return stays refused.
    #[tokio::test]
    pub(super) async fn a_canonical_completed_jobs_remainder_survives_a_crash_before_its_receipt() {
        use crate::commands::training_trigger::return_::{return_job, return_parked_remainder, Remainder, REMAINDER_RECEIPT};
        use crate::genome::fine_tuning::job_board::{EndedJob, WatchedJob};
        use crate::genome::fine_tuning::types::{TrainingArtifact, TrainingStatus};
        for explicit in [false, true] {
        let (adapter, _dir) = crate::orm::store::fresh_adapter().await;
        let (old, executor) = build_runtime(adapter.clone(), false).await;
        let artifacts = tempfile::tempdir().unwrap();
        let board = crate::genome::fine_tuning::TrainingJobBoard::with_ledger(Some(artifacts.path().join("jobs.jsonl")));
        let (persona, job, submission) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
        let key = BucketKey { persona_id: persona, trait_kind: "code".into(), base_model: "synthetic".into() };
        let originals: Vec<_> = (0..16)
            .map(|i| {
                let mut e = ex(&format!("prompt-{i}"), &format!("completion-{i}"));
                e.metadata = Some(serde_json::json!({"provenance": {"turn": i, "room": "cb2e21a1"}, "media": [{"kind": "image", "sha256": format!("{i:064x}")}]}));
                e
            })
            .collect();
        let mut params = submit_params(persona, "code", originals.clone(), Some(64));
        params["submissionId"] = serde_json::json!(submission);
        if explicit {
            params["lora"] = serde_json::json!({"rank":4,"alpha":8,"dropout":0.0,"targetModules":["q_proj"]});
            params["schedule"] = serde_json::json!({"epochs":1,"batchSize":2,"sequenceLength":1024,"learningRate":0.0002});
        }
        assert_eq!(executor.execute_json("genome/training-trigger/submit", params).await.unwrap()["success"], true);
        let store = old.state.durable.require().unwrap();
        let intent = DispatchIntent {
            base: BaseEntity::for_new_record(),
            persona_id: persona,
            trait_kind: key.trait_kind.clone(),
            base_model: key.base_model.clone(),
            submission_ids: vec![submission],
            phase: DispatchPhase::Dispatched {
                handle: JobHandle { provider_id: crate::genome::fine_tuning::cuda_lora_adapter::PROVIDER_ID.into(), provider_job_id: job.to_string(), local_id: job },
                provider: crate::genome::fine_tuning::cuda_lora_adapter::PROVIDER_ID.into(),
            },
            is_active: false,
            attempt: None,
        };
        let dispatch = entity_id(&intent.base).unwrap();
        store.dispatches.save(dispatch, &intent).await.unwrap();
        store.assign(&intent).await.unwrap();
        board.register(WatchedJob {
            trigger_dispatch_id: Some(dispatch),
            handle: JobHandle { provider_id: crate::genome::fine_tuning::cuda_lora_adapter::PROVIDER_ID.into(), provider_job_id: job.to_string(), local_id: job },
            persona_id: persona, persona_name: "test-p".into(), base_model: "synthetic".into(), trait_kind: "code".into(),
            eval_set: None, signature: None, decision: None,
        });
        board.claim(job, &TrainingStatus::Completed {
            artifact: TrainingArtifact { model_id: "gene".into(), local_path: None, format: Default::default(), metrics: Default::default() },
        });
        let parked: Vec<usize> = vec![0, 1, 4, 5, 6, 8, 9, 10, 11, 12, 13, 14, 15];
        let jd = crate::genome::fine_tuning::job_dir::job_dir_under(artifacts.path(), "test-p", "code", job);
        std::fs::create_dir_all(jd.join("adapters")).unwrap();
        std::fs::write(jd.join("adapters/adapter.gguf"), b"gene").unwrap();
        std::fs::write(jd.join("partition.json"), serde_json::json!({"window": 40704, "sent": [2, 3, 7], "parked": parked, "rows": []}).to_string()).unwrap();
        assert!(!jd.join("request.json").exists(), "the canonical layout has no request file");
        let completed = EndedJob { local_id: job, persona_name: "test-p".into(), trait_kind: "code".into(), trained: true };

        // The dispatching core is gone; the one that boots next holds only what the durable store
        // says is pending (the dispatched batch is not). It durably accepts the remainder, and dies
        // before writing the receipt.
        old.shutdown().await.unwrap();
        drop(executor);
        drop(old);
        let (crashed, crashed_executor) = build_runtime(adapter.clone(), false).await;
        let mut adapter_request = crashed.state.dispatched_request(&key, dispatch, job).await.unwrap();
        // card 3c3634d2 (the windows 5090, 8b46862c: an in-engine job's directory held only
        // partition.json, and its resume gave up): a job left behind is resumed and returned from
        // the same verified dispatch, with no request file on disk; with no dispatch and no file
        // there is nothing to resume from
        let orphan = crate::genome::fine_tuning::job_board::OrphanedJob {
            local_id: job, trigger_dispatch_id: Some(dispatch), provider_id: crate::genome::fine_tuning::cuda_lora_adapter::PROVIDER_ID.into(),
            persona_id: persona, persona_name: "test-p".into(), base_model: "synthetic".into(), trait_kind: "code".into(), eval_set: None,
        };
        let input = crashed.state.job_input(&orphan, &jd).await.expect("its input, from its durable dispatch");
        assert_eq!(serde_json::to_value(&input).unwrap(), serde_json::to_value(&adapter_request).unwrap());
        let undispatched = crate::genome::fine_tuning::job_board::OrphanedJob { trigger_dispatch_id: None, ..orphan.clone() };
        assert!(matches!(crashed.state.job_input(&undispatched, &jd).await, Err(crate::modules::training_trigger::JobInputError::Missing(_))));
        let original_lora = adapter_request.lora.clone();
        let original_schedule = adapter_request.schedule.clone();
        assert_eq!(original_lora.is_some(), explicit);
        adapter_request.lora = Some(serde_json::from_value(serde_json::json!({"rank":8,"alpha":16,"dropout":0.0,"targetModules":["q_proj","v_proj"]})).unwrap());
        adapter_request.schedule = Some(serde_json::from_value(serde_json::json!({"epochs":3,"batchSize":4,"sequenceLength":2048,"learningRate":0.0001})).unwrap());
        std::fs::write(jd.join("request.json"), serde_json::to_vec(&adapter_request).unwrap()).unwrap();
        let store = crashed.state.durable.require().unwrap();
        let mut broken = intent.clone();
        broken.phase = DispatchPhase::Dispatched {
            handle: JobHandle { provider_id: crate::genome::fine_tuning::cuda_lora_adapter::PROVIDER_ID.into(), provider_job_id: "wrong".into(), local_id: Uuid::new_v4() },
            provider: crate::genome::fine_tuning::cuda_lora_adapter::PROVIDER_ID.into(),
        };
        store.dispatches.update(dispatch, &broken).await.unwrap();
        assert!(
            matches!(crashed.state.job_input(&orphan, &jd).await, Err(crate::modules::training_trigger::JobInputError::Unproven(_))),
            "a request file on disk never stands in for a dispatch that does not prove the job"
        );
        assert!(return_parked_remainder(&crashed.state, &board, artifacts.path(), &completed, &|_: &str| None, None).await.is_err(), "a usable request file cannot bypass a broken recorded dispatch");
        assert_eq!(crashed.state.bucket_example_count(persona, "code", "synthetic"), None);
        assert!(!jd.join(REMAINDER_RECEIPT).exists());
        store.dispatches.update(dispatch, &intent).await.unwrap();
        let returned = return_parked_remainder(&crashed.state, &board, artifacts.path(), &completed, &|_: &str| None, None).await.unwrap();
        let Remainder::Submitted(accepted) = returned else { panic!("expected a new return: {returned:?}") };
        {
            let pending = crashed.state.buckets.get(&key).unwrap();
            assert_eq!(pending.lora, original_lora, "retain both omitted and explicit producer policy");
            assert_eq!(pending.schedule, original_schedule, "adapter defaults cannot replace explicit schedule");
        }
        // Model a crash before the receipt became durable, retaining the accepted batch.
        std::fs::remove_file(jd.join(REMAINDER_RECEIPT)).unwrap();
        assert!(accepted.success, "the remainder is durably accepted before the crash: {accepted:?}");
        assert!(!accepted.acceptance.as_ref().expect("an acceptance receipt").replayed, "the first acceptance is new: {accepted:?}");
        assert_eq!(crashed.state.bucket_example_count(persona, "code", "synthetic"), Some(13), "exactly the thirteen: {accepted:?}");
        assert!(crashed.state.active_dispatches.get(&key).is_none(), "a remainder below the threshold dispatches nothing: {accepted:?}");
        let original = crashed.state.durable.require().unwrap().submissions.find_by_id(submission).await.unwrap().expect("the original submission");
        assert!(!original.is_pending && original.dispatch_id == Some(dispatch), "the original submission stays owned by its completed dispatch");
        assert!(!jd.join(REMAINDER_RECEIPT).exists());
        crashed.shutdown().await.unwrap();
        drop(crashed_executor);
        drop(crashed);

        let (next, _executor) = build_runtime(adapter, false).await;
        let replayed = return_parked_remainder(&next.state, &board, artifacts.path(), &completed, &|_: &str| None, None).await.unwrap();
        let Remainder::Submitted(outcome) = replayed else { panic!("the remainder is replayed, not skipped: {replayed:?}") };
        assert!(outcome.success, "{outcome:?}");
        assert!(outcome.acceptance.expect("an acceptance receipt").replayed, "the next owner replays the accepted remainder by identity");
        assert!(jd.join(REMAINDER_RECEIPT).is_file(), "and then writes its receipt");
        assert_eq!(next.state.bucket_example_count(persona, "code", "synthetic"), Some(13), "thirteen examples, never twice");
        {
            let pending = next.state.buckets.get(&key).unwrap();
            let expected: Vec<_> = parked.iter().map(|&i| originals[i].clone()).collect();
            assert_eq!(serde_json::to_value(&pending.examples).unwrap(), serde_json::to_value(&expected).unwrap(), "exactly the parked originals, whole and unchanged, in order");
        }
        assert!(matches!(return_parked_remainder(&next.state, &board, artifacts.path(), &completed, &|_: &str| None, None).await.unwrap(), Remainder::AlreadyReturned));
        assert!(return_job(&next.state, &board, artifacts.path(), job).await.is_err(), "the full return of a completed job stays refused");
        if !explicit {
            use crate::commands::training_trigger::policy_recover::{PolicyRecoverParams, ReturnPolicyEdge};
            use crate::commands::training_trigger::submit::ReturnScope;
            let returned_id = ReturnScope::Parked(&parked).chunk_id(job, 0);
            let store = next.state.durable.require().unwrap();
            let mut historical = store.submissions.find_by_id(returned_id).await.unwrap().unwrap();
            // Reproduce the persisted old adapter-file return, without altering its examples.
            historical.batch.lora = adapter_request.lora.clone();
            historical.batch.schedule = adapter_request.schedule.clone();
            store.submissions.update(returned_id, &historical).await.unwrap();
            next.state.restore_pending_bucket(&key).await.unwrap();
            let raw_before = serde_json::to_value(&historical.batch).unwrap();
            let actor = Uuid::new_v4();
            let mut correction = PolicyRecoverParams {
                submission_id: returned_id, expected_batch_sha256: None,
                edges: vec![ReturnPolicyEdge { dispatch_id: dispatch, job_id: job, parked: Some(parked.clone()), chunk_index: 0 }],
                reason: "fixture: original producer policy was replaced by adapter defaults".into(), apply: false,
            };
            let mut explicit_source = original.clone();
            explicit_source.batch.lora = Some(serde_json::from_value(serde_json::json!({"rank":4,"alpha":8,"dropout":0.0,"targetModules":["q_proj"]})).unwrap());
            store.submissions.update(submission, &explicit_source).await.unwrap();
            assert!(next.state.recover_return_policy(correction.clone(), actor).await.is_err(), "an explicit producer policy change is not adapter materialization");
            store.submissions.update(submission, &original).await.unwrap();
            let mut unknown_provider = intent.clone();
            unknown_provider.phase = DispatchPhase::Dispatched {
                handle: JobHandle { provider_id: "unknown-provider".into(), provider_job_id: job.to_string(), local_id: job },
                provider: "unknown-provider".into(),
            };
            store.dispatches.update(dispatch, &unknown_provider).await.unwrap();
            assert!(next.state.recover_return_policy(correction.clone(), actor).await.is_err(), "unknown providers cannot establish native default materialization");
            store.dispatches.update(dispatch, &intent).await.unwrap();
            let mut active = intent.clone();
            active.is_active = true;
            store.dispatches.update(dispatch, &active).await.unwrap();
            assert!(next.state.recover_return_policy(correction.clone(), actor).await.is_err(), "an active dispatch prevents policy correction");
            store.dispatches.update(dispatch, &intent).await.unwrap();
            let inspected = next.state.recover_return_policy(correction.clone(), actor).await.unwrap();
            assert!(!inspected.applied);
            assert!(store.policy_corrections.find_by_id(returned_id).await.unwrap().is_none());
            correction.apply = true;
            correction.expected_batch_sha256 = Some("wrong digest".into());
            assert!(next.state.recover_return_policy(correction.clone(), actor).await.is_err());
            correction.expected_batch_sha256 = Some(inspected.batch_sha256.clone());
            let mut wrong_job = correction.clone();
            wrong_job.edges[0].job_id = Uuid::new_v4();
            assert!(next.state.recover_return_policy(wrong_job, actor).await.is_err());
            assert!(store.policy_corrections.find_by_id(returned_id).await.unwrap().is_none());
            let applied = next.state.recover_return_policy(correction.clone(), actor).await.unwrap();
            assert!(applied.applied && !applied.replayed);
            let stored = store.submissions.find_by_id(returned_id).await.unwrap().unwrap();
            assert_eq!(serde_json::to_value(&stored.batch).unwrap(), raw_before, "original accepted batch stays immutable");
            let effective = store.effective_batch(&stored).await.unwrap();
            assert!(effective.lora.is_none() && effective.schedule.is_none());
            assert_eq!(serde_json::to_value(&effective.examples).unwrap(), serde_json::to_value(&stored.batch.examples).unwrap());
            assert_eq!(next.state.bucket_example_count(persona, "code", "synthetic"), Some(13));
            let raw_replay = next.state.accept(&key, Some(returned_id), stored.batch.clone()).await.unwrap();
            let corrected_replay = next.state.accept(&key, Some(returned_id), effective.clone()).await.unwrap();
            assert!(raw_replay.replayed && corrected_replay.replayed);
            let mut altered = effective;
            altered.examples[0].completion.push_str("changed");
            assert!(next.state.accept(&key, Some(returned_id), altered).await.is_err());
            for _ in 0..3 {
                let id = Uuid::new_v4();
                let mut incoming = stored.batch.clone();
                incoming.lora = None; incoming.schedule = None;
                incoming.examples = vec![ex("new graded example", "accepted")];
                let accepted = next.state.accept(&key, Some(id), incoming.clone()).await.unwrap();
                assert!(!accepted.replayed);
                assert!(next.state.accept(&key, Some(id), incoming).await.unwrap().replayed);
            }
            assert_eq!(next.state.bucket_example_count(persona, "code", "synthetic"), Some(16));
            next.shutdown().await.unwrap();
            let (restarted, _exec) = build_runtime(store.adapter.clone(), false).await;
            assert!(restarted.state.recover_return_policy(correction, actor).await.unwrap().replayed);
            assert_eq!(restarted.state.bucket_example_count(persona, "code", "synthetic"), Some(16));
            let retained = restarted.state.durable.require().unwrap().submissions.find_by_id(returned_id).await.unwrap().unwrap();
            assert_eq!(serde_json::to_value(&retained.batch).unwrap(), raw_before);
            restarted.shutdown().await.unwrap();
        } else {
            next.shutdown().await.unwrap();
        }
        }
    }

    // What this catches (278afa6c): an init timeout must not permanently disable
    // learning. Cancel the real owner's setup wait; its existing tick retries
    // against the retained actual SQLite adapter, without a global HOME/config edit.
    #[tokio::test]
    pub(super) async fn owner_tick_retries_interrupted_schema_setup() {
        let (adapter, _dir) = crate::orm::store::fresh_adapter().await;
        let trigger = Arc::new(super::super::TrainingTriggerModule::new());
        let pause = pause_at(&trigger.state, OperationPoint::BeforeSchemaSetup);
        let state = trigger.state.clone();
        let setup = tokio::spawn(async move { state.initialize_storage(adapter).await });
        tokio::time::timeout(std::time::Duration::from_secs(5), pause.entered.notified())
            .await
            .unwrap();
        setup.abort();
        assert!(setup.await.unwrap_err().is_cancelled());
        assert!(trigger.state.require_ready().is_err());
        trigger.tick().await.unwrap();
        trigger.state.require_ready().unwrap();
        assert!(trigger
            .state
            .durable
            .require()
            .unwrap()
            .submissions
            .find_all()
            .await
            .unwrap()
            .is_empty());
        trigger.shutdown().await.unwrap();
        // A late initialization publication cannot override authoritative stop.
        trigger.state.ready.store(true, Ordering::Release);
        assert!(trigger.state.require_ready().is_err());
        assert!(trigger.tick().await.is_err());
        let admission: Result<(), String> = trigger
            .state
            .run_owned(
                BucketKey {
                    persona_id: Uuid::new_v4(),
                    trait_kind: "code".into(),
                    base_model: "synthetic".into(),
                },
                |_, _| async { panic!("shutdown admitted a new operation") },
            )
            .await;
        assert!(admission.is_err());
    }

    // What this catches (278afa6c): boot never loads an entire backlog. Admission
    // advances one real page but cannot install new policy before prior pages are
    // coherent; the module tick resumes retained progress rather than page zero.
    #[tokio::test]
    pub(super) async fn large_bucket_recovers_in_pages_before_accepting_new_policy() {
        let (adapter, _dir) = crate::orm::store::fresh_adapter().await;
        let (old, executor) = build_runtime(adapter.clone(), false).await;
        let persona = Uuid::new_v4();
        let key = BucketKey {
            persona_id: persona,
            trait_kind: "code".into(),
            base_model: "synthetic".into(),
        };
        let mut first = None;
        for index in 0..66 {
            let mut request = submit_params(
                persona,
                "code",
                vec![ex(&index.to_string(), "c")],
                Some(1000),
            );
            request["submissionId"] = serde_json::json!(Uuid::new_v4());
            let receipt = executor
                .execute_json("genome/training-trigger/submit", request.clone())
                .await
                .unwrap();
            assert_eq!(receipt["success"], true, "{receipt}");
            if first.is_none() {
                first = Some(request);
            }
        }
        old.shutdown().await.unwrap();
        let (next, executor) = build_runtime(adapter, false).await;
        assert!(
            next.state.buckets.is_empty(),
            "schema setup must not hydrate pending payloads"
        );
        let mut changed = submit_params(persona, "code", vec![ex("new", "c")], Some(1000));
        changed["preferredProvider"] = serde_json::json!("different");
        let deferred = executor
            .execute_json("genome/training-trigger/submit", changed.clone())
            .await
            .unwrap();
        assert_eq!(deferred["errorKind"], "RecoveryRequired", "{deferred}");
        assert!(deferred.get("acceptance").is_none());
        assert_eq!(
            next.state
                .hydrating
                .get(&key)
                .unwrap()
                .pending
                .as_ref()
                .unwrap()
                .examples
                .len(),
            64
        );
        assert!(!next.state.hydrated.contains(&key));
        next.tick().await.unwrap();
        assert!(!next.state.hydrating.contains_key(&key));
        let bucket = next.state.buckets.get(&key).unwrap();
        assert_eq!(bucket.examples.len(), 66);
        assert_eq!(bucket.examples[0].prompt, "0");
        assert_eq!(bucket.examples[65].prompt, "65");
        drop(bucket);
        let refused = executor
            .execute_json("genome/training-trigger/submit", changed)
            .await
            .unwrap();
        assert_eq!(refused["errorKind"], "InconsistentBucket");
        let replay = executor
            .execute_json("genome/training-trigger/submit", first.unwrap())
            .await
            .unwrap();
        assert_eq!(replay["acceptance"]["replayed"], true);
        assert_eq!(
            next.state
                .bucket_example_count(persona, "code", "synthetic"),
            Some(66)
        );
        // Changed-bucket replays must not retain hydrated markers for empty keys.
        let original = next.state.buckets.get(&key).unwrap().submission_ids[0];
        for trait_kind in ["foreign-a", "foreign-b"] {
            let mut foreign = submit_params(persona, trait_kind, vec![ex("0", "c")], Some(1000));
            foreign["submissionId"] = serde_json::json!(original);
            let refused = executor
                .execute_json("genome/training-trigger/submit", foreign)
                .await
                .unwrap();
            assert_eq!(refused["errorKind"], "SubmissionConflict");
        }
        assert_eq!(next.state.hydrated.len(), 1);
    }

    // What this catches (278afa6c): real SQLite INSERT commits, then the caller
    // cancels before cache publication. The admitted owner must keep its lease,
    // preserve A's policy/order, and prevent B from installing conflicting policy.
    #[tokio::test]
    pub(super) async fn cancelled_acceptance_keeps_policy_and_order_until_commit_is_published() {
        use std::time::Duration;
        let (adapter, _dir) = crate::orm::store::fresh_adapter().await;
        let (trigger, executor) = build_runtime(adapter.clone(), false).await;
        let persona = Uuid::new_v4();
        let id = Uuid::new_v4();
        let mut request = submit_params(persona, "code", vec![ex("first", "a")], Some(100));
        request["submissionId"] = serde_json::json!(id);
        let pause = pause_at(&trigger.state, OperationPoint::AcceptanceCommitted);
        let caller_executor = executor.clone();
        let caller_request = request.clone();
        let caller = tokio::spawn(async move {
            caller_executor
                .execute_json("genome/training-trigger/submit", caller_request)
                .await
        });
        tokio::time::timeout(Duration::from_secs(5), pause.entered.notified())
            .await
            .unwrap();
        assert!(trigger
            .state
            .durable
            .require()
            .unwrap()
            .submissions
            .find_by_id(id)
            .await
            .unwrap()
            .is_some());
        caller.abort();
        assert!(caller.await.unwrap_err().is_cancelled());
        let mut changed = submit_params(persona, "code", vec![ex("second", "b")], Some(100));
        changed["preferredProvider"] = serde_json::json!("different-policy");
        let waiting = executor.execute_json("genome/training-trigger/submit", changed);
        tokio::pin!(waiting);
        assert!(
            tokio::time::timeout(Duration::from_millis(20), &mut waiting)
                .await
                .is_err(),
            "cancelled caller must not release the owner's lease"
        );
        pause.resume.notify_one();
        let refused = waiting.await.unwrap();
        assert_eq!(refused["errorKind"], "InconsistentBucket", "{refused}");
        assert!(refused.get("acceptance").is_none());
        let replay = executor
            .execute_json("genome/training-trigger/submit", request)
            .await
            .unwrap();
        assert_eq!(replay["acceptance"]["replayed"], true);
        let next = executor
            .execute_json(
                "genome/training-trigger/submit",
                submit_params(persona, "code", vec![ex("second", "b")], Some(100)),
            )
            .await
            .unwrap();
        assert_eq!(next["currentCount"], 2);
        trigger.shutdown().await.unwrap();
        let (restarted, _) = build_runtime(adapter, false).await;
        restarted.tick().await.unwrap();
        let key = BucketKey {
            persona_id: persona,
            trait_kind: "code".into(),
            base_model: "synthetic".into(),
        };
        let bucket = restarted.state.buckets.get(&key).unwrap();
        assert_eq!(
            bucket
                .examples
                .iter()
                .map(|example| example.prompt.as_str())
                .collect::<Vec<_>>(),
            ["first", "second"]
        );
    }

    // What this catches (278afa6c): a cancelled caller cannot let a later attempt
    // overtake an old Retryable write. Pause the actual transition before its real
    // SQLite update; the retry waits for that same owner to finish, then keeps IDs.
    #[tokio::test]
    pub(super) async fn cancelled_retryable_transition_cannot_be_overtaken() {
        use std::time::Duration;
        let (adapter, _dir) = crate::orm::store::fresh_adapter().await;
        let (trigger, executor) = build_runtime(adapter.clone(), true).await;
        let persona = Uuid::new_v4();
        let key = BucketKey {
            persona_id: persona,
            trait_kind: "code".into(),
            base_model: "synthetic".into(),
        };
        let mut request = submit_params(persona, "code", vec![ex("p", "c")], Some(1));
        request["submissionId"] = serde_json::json!(Uuid::new_v4());
        request["preferredProvider"] = serde_json::json!("unavailable-provider");
        let pause = pause_at(&trigger.state, OperationPoint::BeforeRetryableCommit);
        let caller_executor = executor.clone();
        let caller_request = request.clone();
        let caller = tokio::spawn(async move {
            caller_executor
                .execute_json("genome/training-trigger/submit", caller_request)
                .await
        });
        tokio::time::timeout(Duration::from_secs(5), pause.entered.notified())
            .await
            .unwrap();
        let dispatch_id = entity_id(
            &trigger
                .state
                .active_dispatches
                .get(&key)
                .unwrap()
                .intent
                .base,
        )
        .unwrap();
        let intent = trigger
            .state
            .durable
            .require()
            .unwrap()
            .dispatches
            .find_by_id(dispatch_id)
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(intent.phase, DispatchPhase::Dispatching));
        // card 55d4b9e3: the attempt is durable before job-create runs.
        assert!(intent.attempt.is_some(), "a dispatching intent names its attempt");
        caller.abort();
        assert!(caller.await.unwrap_err().is_cancelled());
        let waiting = executor.execute_json("genome/training-trigger/submit", request);
        tokio::pin!(waiting);
        assert!(
            tokio::time::timeout(Duration::from_millis(20), &mut waiting)
                .await
                .is_err()
        );
        pause.resume.notify_one();
        let replay = waiting.await.unwrap();
        assert_eq!(replay["acceptance"]["replayed"], true, "{replay}");
        assert_eq!(replay["errorKind"], "DispatchFailed");
        assert_eq!(
            entity_id(
                &trigger
                    .state
                    .active_dispatches
                    .get(&key)
                    .unwrap()
                    .intent
                    .base
            )
            .unwrap(),
            dispatch_id
        );
        trigger.shutdown().await.unwrap();
        let (restarted, _) = build_runtime(adapter, true).await;
        restarted.tick().await.unwrap();
        let active = restarted.state.active_dispatches.get(&key).unwrap();
        assert_eq!(entity_id(&active.intent.base).unwrap(), dispatch_id);
        assert!(matches!(
            active.intent.phase,
            DispatchPhase::Retryable { .. }
        ));
        assert_eq!(active.batch.examples.len(), 1);
    }

mod part_2;
