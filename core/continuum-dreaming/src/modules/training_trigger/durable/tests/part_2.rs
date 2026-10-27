#![cfg(test)]
use super::*;

    // What this catches (278afa6c): a persisted intent is the recovery owner
    // even when its multi-row assignments only partly reached disk. The actual
    // flush path must dispatch the original examples once, with correlated proof.
    #[tokio::test]
    pub(super) async fn partial_intent_assignment_recovers_then_dispatches_once() {
        let (adapter, _dir) = crate::orm::store::fresh_adapter().await;
        let (old, executor) = build_runtime(adapter.clone(), true).await;
        let persona = Uuid::new_v4();
        let key = BucketKey {
            persona_id: persona,
            trait_kind: "test-trait".into(),
            base_model: "synthetic".into(),
        };
        let mut requests = Vec::new();
        for (id, count) in [(Uuid::new_v4(), 2), (Uuid::new_v4(), 3)] {
            let mut request = submit_params(
                persona,
                "test-trait",
                (0..count)
                    .map(|i| ex(&format!("{id}-{i}"), "completion"))
                    .collect(),
                Some(100),
            );
            request["submissionId"] = serde_json::json!(id);
            let receipt = executor
                .execute_json("genome/training-trigger/submit", request.clone())
                .await
                .unwrap();
            assert_eq!(receipt["success"], true, "{receipt}");
            requests.push(request);
        }
        let batch = old.state.buckets.remove(&key).unwrap().1;
        let intent = DispatchIntent {
            base: BaseEntity::for_new_record(),
            persona_id: key.persona_id,
            trait_kind: key.trait_kind.clone(),
            base_model: key.base_model.clone(),
            submission_ids: batch.submission_ids.clone(),
            phase: DispatchPhase::Prepared,
            is_active: true,
            attempt: None,
        };
        let dispatch_id = entity_id(&intent.base).unwrap();
        let store = old.state.durable.require().unwrap();
        store.dispatches.save(dispatch_id, &intent).await.unwrap();
        let first = intent.submission_ids[0];
        let mut row = store.submissions.find_by_id(first).await.unwrap().unwrap();
        row.is_pending = false;
        row.dispatch_id = Some(dispatch_id);
        store.submissions.update(first, &row).await.unwrap();
        drop(executor);
        let (next, executor) = build_runtime(adapter, true).await;
        assert!(
            next.state.buckets.is_empty(),
            "boot must not hydrate payloads"
        );
        assert!(
            next.state.active_dispatches.is_empty(),
            "intent recovery is owned by command/tick pages"
        );
        // Hold a real tick's metadata snapshot while flush settles that intent.
        // The stale head must neither replay it nor retain an empty cache marker.
        let pause = pause_at(&next.state, OperationPoint::AfterMetadataScan);
        let ticking = next.clone();
        let tick = tokio::spawn(async move { ticking.tick().await });
        tokio::time::timeout(std::time::Duration::from_secs(5), pause.entered.notified())
            .await
            .unwrap();
        let params = serde_json::json!({"personaId": persona, "traitKind": "test-trait", "baseModel": "synthetic"});
        let dispatched = executor
            .execute_json("genome/training-trigger/flush", params.clone())
            .await
            .unwrap();
        assert_eq!(dispatched["outcome"], "JobDispatched", "{dispatched}");
        let handle_id = dispatched["jobHandle"]["localId"].as_str().unwrap();
        assert_eq!(dispatched["examplesUsed"], 5);
        let evidence = next
            .state
            .test_job_board
            .lookup_trigger_dispatch(dispatch_id, 0)
            .unwrap();
        assert!(
            matches!(evidence, DispatchLookup::Observed(ref handle) if handle.local_id.to_string() == handle_id)
        );
        pause.resume.notify_one();
        tick.await.unwrap().unwrap();
        assert!(next.state.hydrated.is_empty());
        let again = executor
            .execute_json("genome/training-trigger/flush", params)
            .await
            .unwrap();
        assert_eq!(again["outcome"], "NothingToFlush");
        let replay = executor
            .execute_json("genome/training-trigger/submit", requests.remove(0))
            .await
            .unwrap();
        assert_eq!(replay["outcome"], "AlreadyAccepted");
        assert_eq!(replay["acceptance"]["replayed"], true);
        assert_eq!(next.state.pending_bucket_count(), 0);
    }

    // What this catches (278afa6c): cancellation after a successful insert must
    // not require the producer to retry. The existing owner's tick discovers
    // committed rows absent from RAM without duplicating already-cached peers.
    #[tokio::test]
    pub(super) async fn owner_tick_restores_committed_but_uncached_acceptance() {
        let (adapter, _dir) = crate::orm::store::fresh_adapter().await;
        let (trigger, executor) = build_runtime(adapter, false).await;
        let persona = Uuid::new_v4();
        let key = BucketKey {
            persona_id: persona,
            trait_kind: "code".into(),
            base_model: "synthetic".into(),
        };
        let mut request = submit_params(persona, "code", vec![ex("earlier", "c")], Some(100));
        request["submissionId"] = serde_json::json!(Uuid::new_v4());
        let receipt = executor
            .execute_json("genome/training-trigger/submit", request.clone())
            .await
            .unwrap();
        assert_eq!(receipt["success"], true);
        executor
            .execute_json(
                "genome/training-trigger/submit",
                submit_params(persona, "code", vec![ex("later", "c")], Some(100)),
            )
            .await
            .unwrap();
        {
            let mut bucket = trigger.state.buckets.get_mut(&key).unwrap();
            bucket.submission_ids.remove(0);
            bucket.examples.remove(0); // real durable row absent from the live cache
        }
        trigger.tick().await.unwrap();
        trigger.tick().await.unwrap();
        {
            let bucket = trigger.state.buckets.get(&key).unwrap();
            assert_eq!(
                bucket
                    .examples
                    .iter()
                    .map(|example| example.prompt.as_str())
                    .collect::<Vec<_>>(),
                ["earlier", "later"]
            );
        }
        let replay = executor
            .execute_json("genome/training-trigger/submit", request)
            .await
            .unwrap();
        assert_eq!(replay["acceptance"]["replayed"], true);
        assert_eq!(
            trigger
                .state
                .bucket_example_count(persona, "code", "synthetic"),
            Some(2)
        );
    }

    // What this catches (278afa6c): an interrupted provider call is NOT pending
    // work to repeat. Recreating the real owner keeps it explicitly uncertain,
    // even though a fully capable provider is now available.
    #[tokio::test]
    pub(super) async fn interrupted_dispatch_is_recovery_required_not_redispatched() {
        let (adapter, _dir) = crate::orm::store::fresh_adapter().await;
        let (old, executor) = build_runtime(adapter.clone(), false).await;
        let persona = Uuid::new_v4();
        let key = BucketKey {
            persona_id: persona,
            trait_kind: "code".into(),
            base_model: "synthetic".into(),
        };
        let receipt = executor
            .execute_json(
                "genome/training-trigger/submit",
                submit_params(persona, "code", vec![ex("p", "c")], Some(100)),
            )
            .await
            .unwrap();
        assert_eq!(receipt["success"], true);
        let batch = old.state.buckets.remove(&key).unwrap().1;
        let intent = DispatchIntent {
            base: BaseEntity::for_new_record(),
            persona_id: key.persona_id,
            trait_kind: key.trait_kind.clone(),
            base_model: key.base_model.clone(),
            submission_ids: batch.submission_ids,
            phase: DispatchPhase::Dispatching,
            is_active: true,
            attempt: None,
        };
        let id = entity_id(&intent.base).unwrap();
        old.state
            .durable
            .require()
            .unwrap()
            .dispatches
            .save(id, &intent)
            .await
            .unwrap();
        old.state
            .durable
            .require()
            .unwrap()
            .assign(&intent)
            .await
            .unwrap();
        let (next, executor) = build_runtime(adapter, true).await;
        next.tick().await.unwrap();
        let flushed = executor
            .execute_json(
                "genome/training-trigger/flush",
                serde_json::json!({
                    "personaId": persona, "traitKind": "code", "baseModel": "synthetic",
                }),
            )
            .await
            .unwrap();
        assert_eq!(flushed["success"], false);
        assert!(flushed.get("jobHandle").is_none());
        let active = next.state.active_dispatches.get(&key).unwrap();
        assert_eq!(entity_id(&active.intent.base).unwrap(), id);
        assert!(matches!(
            active.intent.phase,
            DispatchPhase::RecoveryRequired { .. }
        ));
        assert_eq!(active.batch.examples.len(), 1);
        drop(active);
        // card 55d4b9e3: an intent written before attempts existed is surfaced as unknown.
        let row = next
            .state
            .pending_views()
            .into_iter()
            .find(|row| row.dispatch_id == Some(id))
            .expect("the stranded dispatch is listed");
        assert!(row.unknown_legacy, "no attempt names what it may have started");
        assert_eq!(row.dispatch_attempt, None);
    }

    // what this catches (card 55d4b9e3, IntelMac 10-24: eight buckets waited forever): an
    // interrupted dispatch that names its attempt is no longer stranded. Its attempt never
    // reached the ledger, so reconciliation closes it and proves "not created"; the intent
    // becomes retryable instead of RecoveryRequired, and the closed attempt stays closed.
    #[tokio::test]
    pub(super) async fn an_interrupted_dispatch_with_an_unadmitted_attempt_becomes_retryable() {
        let (adapter, _dir) = crate::orm::store::fresh_adapter().await;
        let (old, executor) = build_runtime(adapter.clone(), false).await;
        let persona = Uuid::new_v4();
        let key = BucketKey {
            persona_id: persona,
            trait_kind: "code".into(),
            base_model: "synthetic".into(),
        };
        let receipt = executor
            .execute_json(
                "genome/training-trigger/submit",
                submit_params(persona, "code", vec![ex("p", "c")], Some(100)),
            )
            .await
            .unwrap();
        assert_eq!(receipt["success"], true);
        let batch = old.state.buckets.remove(&key).unwrap().1;
        let attempt = Uuid::new_v4();
        let intent = DispatchIntent {
            base: BaseEntity::for_new_record(),
            persona_id: key.persona_id,
            trait_kind: key.trait_kind.clone(),
            base_model: key.base_model.clone(),
            submission_ids: batch.submission_ids,
            phase: DispatchPhase::Dispatching,
            is_active: true,
            attempt: Some(attempt),
        };
        let id = entity_id(&intent.base).unwrap();
        let store = old.state.durable.require().unwrap();
        store.dispatches.save(id, &intent).await.unwrap();
        store.assign(&intent).await.unwrap();
        let (next, executor) = build_runtime(adapter, true).await;
        next.tick().await.unwrap();
        let active = next.state.active_dispatches.get(&key).unwrap();
        assert_eq!(entity_id(&active.intent.base).unwrap(), id);
        // Reconciled, never stranded: retryable with the proof, or (if the same tick already
        // retried it) dispatched under a NEW attempt. Never RecoveryRequired, never the old attempt.
        let retried = matches!(&active.intent.phase, DispatchPhase::Retryable { error } if error.contains("not created"))
            || (matches!(active.intent.phase, DispatchPhase::Dispatched { .. }) && active.intent.attempt != Some(attempt));
        assert!(retried, "reconciled, not stranded: {:?} attempt {:?}", active.intent.phase, active.intent.attempt);
        drop(active);
        let closed = executor
            .execute_json("genome/job-attempt-reconcile", serde_json::json!({ "attempt": attempt }))
            .await
            .unwrap();
        assert_eq!(closed["outcome"], "notCreated", "the attempt stays closed");
    }

    // what this catches (the 5090, 2026-10-10: thirteen cargo-test fixture rows from
    // September in the production jobs ledger refused every resolve): a malformed row BEHIND
    // the intent's recovery cursor is the prefix recovery already scanned and no longer refuses;
    // the same row read from 0, or one AFTER the cursor, still refuses; a registration after it
    // is still found.
    #[test]
    pub(super) fn resolve_reads_the_ledger_from_the_intents_recovery_cursor() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("jobs-ledger.jsonl");
        let junk = "{this is not a ledger row\n";
        std::fs::write(&path, junk).unwrap();
        let board = crate::genome::fine_tuning::TrainingJobBoard::with_ledger(Some(path.clone()));
        let dispatch = Uuid::new_v4();
        let past = junk.len() as u64;
        assert!(registration_from(&board, dispatch, 0).is_err(), "from 0 the fixture row could be anything");
        assert!(registration_from(&board, dispatch, past).unwrap().is_none(), "behind the cursor: already judged");
        let job = Uuid::new_v4();
        let row = serde_json::json!({
            "event": "registered", "local_id": job.to_string(), "trigger_dispatch_id": dispatch.to_string(),
            "provider_id": "engine-local", "provider_job_id": job.to_string(),
            "persona_id": Uuid::new_v4().to_string(), "persona_name": "p", "base_model": "m", "trait_kind": "code",
        });
        let mut text = std::fs::read_to_string(&path).unwrap();
        text.push_str(&format!("{row}\n{junk}"));
        std::fs::write(&path, text).unwrap();
        let found = registration_from(&board, dispatch, past).unwrap().expect("the registration after the cursor is found");
        assert_eq!(found.local_id, job);
        assert!(registration_from(&board, Uuid::new_v4(), past).is_err(), "a malformed row after the cursor still refuses");
    }

    /// The fixture every resolve test starts from: an intent persisted mid-dispatch by one
    /// core, found RecoveryRequired by the next, with no job registered under it.
    /// The retry decisions committed for `dispatch`, read from the durable store.
    pub(super) async fn retry_decisions_of(module: &crate::modules::training_trigger::TrainingTriggerModule, dispatch: Uuid) -> Vec<RetryDecision> {
        let store = module.state.durable.require().unwrap();
        store.retry_decisions.find_all().await.unwrap().into_iter().map(|(_, d)| d).filter(|d| d.dispatch_id == dispatch).collect()
    }

    pub(super) async fn recovery_required_intent() -> (
        Arc<crate::modules::training_trigger::TrainingTriggerModule>,
        Arc<crate::runtime::CommandExecutor>,
        BucketKey,
        Uuid,
        tempfile::TempDir,
    ) {
        let (adapter, dir) = crate::orm::store::fresh_adapter().await;
        let (old, executor) = build_runtime(adapter.clone(), false).await;
        let persona = Uuid::new_v4();
        let key = BucketKey { persona_id: persona, trait_kind: "code".into(), base_model: "synthetic".into() };
        let receipt = executor
            .execute_json(
                "genome/training-trigger/submit",
                // five: the local trainer needs one full batch (as flush_dispatches_partial_bucket)
                submit_params(persona, "code", (0..5).map(|i| ex(&format!("p-{i}"), "c")).collect(), Some(100)),
            )
            .await
            .unwrap();
        assert_eq!(receipt["success"], true);
        let batch = old.state.buckets.remove(&key).unwrap().1;
        let intent = DispatchIntent {
            base: BaseEntity::for_new_record(),
            persona_id: key.persona_id,
            trait_kind: key.trait_kind.clone(),
            base_model: key.base_model.clone(),
            submission_ids: batch.submission_ids,
            phase: DispatchPhase::Dispatching,
            is_active: true,
            attempt: None,
        };
        let id = entity_id(&intent.base).unwrap();
        let store = old.state.durable.require().unwrap();
        store.dispatches.save(id, &intent).await.unwrap();
        store.assign(&intent).await.unwrap();
        let (next, executor) = build_runtime(adapter, true).await;
        next.tick().await.unwrap();
        let active = next.state.active_dispatches.get(&key).unwrap();
        assert!(matches!(active.intent.phase, DispatchPhase::RecoveryRequired { .. }));
        drop(active);
        (next, executor, key, id, dir)
    }

    // what this catches (card d24e3f25, the 5090 on 2026-10-10: 113 examples held on
    // dispatch 0512a303, which never registered a job): recovery refuses forever, correctly;
    // an operator settling it as not-dispatched makes it retryable and the SAME batch
    // dispatches on the next pass, through the normal path, with nothing re-created by guess.
    #[tokio::test]
    pub(super) async fn a_recovery_required_intent_settled_as_not_dispatched_retries_from_its_own_batch() {
        let (next, executor, key, id, _dir) = recovery_required_intent().await;
        let report = next
            .state
            .resolve_dispatch(id, DispatchResolution::NotDispatched, "the ledger has no line for it".into(), None)
            .await
            .expect("settled");
        assert_eq!(report.resolution, Resolved::Retryable);
        assert_eq!(report.examples, 5);
        assert!(matches!(
            next.state.active_dispatches.get(&key).unwrap().intent.phase,
            DispatchPhase::Retryable { ref error } if error.contains("not dispatched")
        ));
        let flushed = executor
            .execute_json(
                "genome/training-trigger/flush",
                serde_json::json!({ "personaId": key.persona_id, "traitKind": "code", "baseModel": "synthetic" }),
            )
            .await
            .unwrap();
        assert_eq!(flushed["success"], true, "{flushed}");
        assert_eq!(flushed["outcome"], "JobDispatched", "{flushed}");
        assert_eq!(flushed["examplesUsed"], 5, "the same batch, not a copy: {flushed}");
        let settled_twice = next
            .state
            .resolve_dispatch(id, DispatchResolution::NotDispatched, "again".into(), None)
            .await;
        assert!(matches!(settled_twice, Err(("Invalid", _))), "a finished intent is not settled again: {settled_twice:?}");
    }

    // what this catches (Codex root on Sahar's 2b4e7272; Astra on #5028): "safe to retry"
    // smuggled in as "no job was created", and a known job retried under its own dispatch (which
    // recovery would re-adopt after a crash). Retrying an UNCONFIRMED dispatch is its own audited
    // decision: it needs named evidence; its receipt commits WITH the intent it releases; the
    // intent's note says "outcome unconfirmed", never "not dispatched"; and a
    // dispatch with a registered job is refused toward the dispatched + return path.
    #[tokio::test]
    pub(super) async fn retrying_an_unconfirmed_dispatch_is_an_audited_decision_that_asserts_no_job_either_way() {
        use crate::genome::fine_tuning::job_board::WatchedJob;
        let (next, _executor, _key, id, _dir) = recovery_required_intent().await;
        let no_evidence = next
            .state
            .resolve_dispatch(id, DispatchResolution::RetryUnconfirmed { evidence: vec![" ".into()] }, "unknown".into(), None)
            .await;
        assert!(matches!(no_evidence, Err(("Invalid", _))), "a retry names its evidence: {no_evidence:?}");

        let evidence = vec!["adapters: none under her genome since the intent".to_string(), "ledger: no registration under it".to_string()];
        let by = Uuid::new_v4();
        let settled = next
            .state
            .resolve_dispatch(id, DispatchResolution::RetryUnconfirmed { evidence: evidence.clone() }, "interrupted by the reboot".into(), Some(by))
            .await
            .expect("an unconfirmed dispatch with no registered job is retried by decision");
        assert_eq!(settled.resolution, Resolved::Retryable);
        let audit = retry_decisions_of(&next, id).await;
        assert_eq!(audit.len(), 1, "one receipt: {audit:?}");
        assert_eq!(audit[0].evidence, evidence);
        assert_eq!(audit[0].resolved_by, Some(by));
        let store = next.state.durable.require().unwrap();
        let intent = store.dispatches.find_by_id(id).await.unwrap().unwrap();
        assert!(
            matches!(intent.phase, DispatchPhase::Retryable { ref error } if error.contains("outcome unconfirmed") && !error.contains("not dispatched")),
            "noted as a decision on an unconfirmed outcome: {:?}",
            intent.phase
        );

        // A producer of the same bucket still in flight on this core (registered without this
        // dispatch, e.g. its ledger append failed) refuses: ledger absence isn't quiescence.
        let (busy, _executor3, key3, id3, _dir3) = recovery_required_intent().await;
        let running = Uuid::new_v4();
        busy.state.test_job_board.register(WatchedJob {
            trigger_dispatch_id: None,
            handle: JobHandle { provider_id: "engine-local".into(), provider_job_id: running.to_string(), local_id: running },
            persona_id: key3.persona_id,
            persona_name: "test-p".into(),
            base_model: key3.base_model.clone(),
            trait_kind: key3.trait_kind.clone(),
            eval_set: None,
            signature: None,
            decision: None,
        });
        let unquiet = busy
            .state
            .resolve_dispatch(id3, DispatchResolution::RetryUnconfirmed { evidence: vec!["ledger: no registration under it".into()] }, "interrupted".into(), None)
            .await;
        assert!(matches!(unquiet, Err(("Contradicted", ref why)) if why.contains("still in flight")), "a live producer of the bucket refuses the retry: {unquiet:?}");
        assert!(retry_decisions_of(&busy, id3).await.is_empty(), "and records nothing");

        // A dispatch WITH a registered job is never retried under its own id (Astra on #5028).
        let (other, _executor2, key2, id2, _dir2) = recovery_required_intent().await;
        let job = Uuid::new_v4();
        other.state.test_job_board.register(WatchedJob {
            trigger_dispatch_id: Some(id2),
            handle: JobHandle { provider_id: "engine-local".into(), provider_job_id: job.to_string(), local_id: job },
            persona_id: key2.persona_id,
            persona_name: "test-p".into(),
            base_model: "synthetic".into(),
            trait_kind: "code".into(),
            eval_set: None,
            signature: None,
            decision: None,
        });
        let known = other
            .state
            .resolve_dispatch(id2, DispatchResolution::RetryUnconfirmed { evidence }, "interrupted".into(), None)
            .await;
        assert!(matches!(known, Err(("Contradicted", ref why)) if why.contains("resolve it as dispatched")), "a known job goes the dispatched + return path: {known:?}");
        assert!(retry_decisions_of(&other, id2).await.is_empty(), "a refused retry records nothing");
    }

    // what this catches (Codex root on #5028 at 497255024): a retry's audit written apart from
    // the intent it releases (a ledger write_all, then the ORM update), so a power loss could
    // keep the released intent and lose the decision and its evidence. They commit in ONE
    // transaction: when the receipt can't be written, the intent is NOT released either.
    #[tokio::test]
    pub(super) async fn a_retry_decision_and_the_intent_it_releases_commit_together_or_not_at_all() {
        let (next, _executor, _key, id, _dir) = recovery_required_intent().await;
        let store = next.state.durable.require().unwrap();
        // the receipt's write fails: a row already holds the id this decision takes
        let held = store.dispatches.find_by_id(id).await.unwrap().unwrap();
        let decision_id = retry_decision_id(id, held.base.version);
        let squatter = RetryDecision {
            base: BaseEntity { id: decision_id.to_string(), ..BaseEntity::for_new_record() },
            dispatch_id: Uuid::new_v4(),
            persona_id: Uuid::new_v4(),
            trait_kind: "code".into(),
            base_model: "synthetic".into(),
            evidence: vec!["squatter".into()],
            reason: "squatter".into(),
            resolved_by: None,
            decided_at_ms: 0,
        };
        store.retry_decisions.save(decision_id, &squatter).await.unwrap();
        let refused = next
            .state
            .resolve_dispatch(id, DispatchResolution::RetryUnconfirmed { evidence: vec!["checked".into()] }, "interrupted".into(), None)
            .await;
        assert!(matches!(refused, Err(("PersistenceFailed", ref why)) if why.contains("neither did")), "the failed receipt fails the retry: {refused:?}");
        let after = store.dispatches.find_by_id(id).await.unwrap().unwrap();
        assert_eq!(format!("{:?}", after.phase), format!("{:?}", held.phase), "the intent was not released without its receipt");
        assert!(retry_decisions_of(&next, id).await.is_empty(), "and no receipt names it");
    }

    // what this catches (Codex root on #5028 at 7279392df): a terminal ledger row taken as proof
    // that the engine stopped. Engine work whose engine is Alive or Unknown fences a retry even
    // when the board has concluded the job; only a verified-dead engine, or another base's work,
    // lets it through.
    #[test]
    pub(super) fn engine_work_on_a_live_or_unknown_engine_fences_a_retry_whatever_the_board_concluded() {
        use crate::inference::engine_residency::{EngineIncarnation, Liveness, ResidentWork};
        let dir = tempfile::tempdir().unwrap();
        let ledger = dir.path().join("jobs-ledger.jsonl");
        let job = Uuid::new_v4();
        std::fs::write(&ledger, format!("{}\n", serde_json::json!({ "event": "terminal", "reason": "claimed", "local_id": job.to_string() }))).unwrap();
        let board = crate::genome::fine_tuning::TrainingJobBoard::with_ledger(Some(ledger));
        assert!(board.is_terminal(job), "the board has concluded the job");
        let work = |base: &str| ResidentWork {
            job,
            out: format!("{job}.gguf"),
            engine: EngineIncarnation { pid: 4242, started_s: 1, port: 1 },
            base_model: base.into(),
            created_ms: 1,
            consumer: format!("genome-train:{job}"),
            reserved_bytes: 0,
            interrupted: None,
            job_spec: None,
            memory: None,
        };
        let persona = Uuid::new_v4();
        let on_base = [work("synthetic")];
        for state in [Liveness::Alive, Liveness::Unknown] {
            let fenced = producers_quiet(&board, persona, "code", "synthetic", &on_base, |_| state);
            assert!(matches!(fenced, Err(ref why) if why.contains(&format!("{state:?}"))), "{state:?} engine work fences: {fenced:?}");
        }
        assert_eq!(producers_quiet(&board, persona, "code", "synthetic", &on_base, |_| Liveness::Dead), Ok(()), "a verified-dead engine releases it");
        assert_eq!(producers_quiet(&board, persona, "code", "synthetic", &[work("other-base")], |_| Liveness::Alive), Ok(()), "another base's work is not this bucket's producer");
    }

    // what this catches: named evidence never beats the ledger. A job registered under the
    // dispatch contradicts not-dispatched; the dispatched arm adopts exactly that job and
    // refuses any other; a job from another bucket is refused by name.
    #[tokio::test]
    pub(super) async fn a_registered_job_contradicts_not_dispatched_and_is_what_the_dispatched_arm_adopts() {
        use crate::genome::fine_tuning::job_board::WatchedJob;
        let (next, _executor, key, id, _dir) = recovery_required_intent().await;
        let job = Uuid::new_v4();
        let watched = |local: Uuid, dispatch: Option<Uuid>, trait_kind: &str| WatchedJob {
            trigger_dispatch_id: dispatch,
            handle: JobHandle { provider_id: "engine-local".into(), provider_job_id: local.to_string(), local_id: local },
            persona_id: key.persona_id,
            persona_name: "test-p".into(),
            base_model: "synthetic".into(),
            trait_kind: trait_kind.into(),
            eval_set: None,
            signature: None,
            decision: None,
        };
        next.state.test_job_board.register(watched(job, Some(id), "code"));
        let other = Uuid::new_v4();
        next.state.test_job_board.register(watched(other, None, "chat"));
        let contradicted = next
            .state
            .resolve_dispatch(id, DispatchResolution::NotDispatched, "wrong".into(), None)
            .await;
        assert!(matches!(contradicted, Err(("Contradicted", _))), "{contradicted:?}");
        let wrong_job = next
            .state
            .resolve_dispatch(id, DispatchResolution::Dispatched { job: other }, "wrong job".into(), None)
            .await;
        assert!(matches!(wrong_job, Err(("Contradicted", _))), "{wrong_job:?}");
        let adopted = next
            .state
            .resolve_dispatch(id, DispatchResolution::Dispatched { job }, "the ledger names it".into(), None)
            .await
            .expect("adopted");
        assert_eq!(adopted.resolution, Resolved::Dispatched);
        assert_eq!(adopted.job_id, Some(job));
        let store = next.state.durable.require().unwrap();
        let intent = store.dispatches.find_by_id(id).await.unwrap().unwrap();
        assert!(matches!(intent.phase, DispatchPhase::Dispatched { ref handle, .. } if handle.local_id == job));
        assert!(!intent.is_active);
        assert!(next.state.active_dispatches.get(&key).is_none_or(|a| entity_id(&a.intent.base).ok() != Some(id)));
    }

    // what this catches (enrico 2026-10-25: 9 unchanged refusals re-fired every 30 s, 792
    // probes in 44 min): recovery re-reporting a refusal nothing changed. Said once; a changed
    // refusal is said again; after the bucket recovers (its entry cleared), a recurrence is news.
    #[test]
    pub(super) fn an_unchanged_recovery_refusal_is_said_once() {
        use super::super::training_trigger_state_2::refusal_is_news;
        let reported = dashmap::DashMap::new();
        assert!(refusal_is_news(&reported, &"kimi/code", "RecoveryRequired: no confirmed result".into()));
        assert!(!refusal_is_news(&reported, &"kimi/code", "RecoveryRequired: no confirmed result".into()));
        assert!(refusal_is_news(&reported, &"kimi/code", "RecoveryRequired: no trainer".into()), "a changed refusal is news");
        assert!(refusal_is_news(&reported, &"sahar/code", "RecoveryRequired: no trainer".into()), "per bucket");
        reported.remove(&"kimi/code");
        assert!(refusal_is_news(&reported, &"kimi/code", "RecoveryRequired: no trainer".into()), "a recurrence after recovery");
    }

