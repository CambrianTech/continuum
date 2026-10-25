#![cfg(test)]
    use super::*;

    // what this catches (the 5090, 2026-10-10): a durable dispatch from before #4794
    // holding `took` as the bare id of the joined job, refused by the tagged enum so that
    // every settlement behind it failed. A bare id reads as Joined; the tagged shapes
    // round-trip unchanged; and the persisted row shape itself decodes.
    #[test]
    fn a_took_written_as_a_bare_job_id_before_4794_reads_as_joined() {
        let job = uuid::Uuid::from_u128(0xcc4f33b7);
        let legacy: Took = serde_json::from_value(serde_json::json!(job.to_string())).expect("a bare id");
        assert_eq!(legacy, Took::Joined { job });
        // EVERY variant round-trips through the hand-written wire shape: the list below is
        // built by a wildcard-free match, so a variant added to `Took` without a line here
        // (and so, without a `TookTagged` twin) fails to compile instead of becoming a row
        // nothing can read (Cormac on #4893).
        let trial = uuid::Uuid::from_u128(5);
        let gene = GeneRef::Hub { repo: "cambriantech/kimi-code".into() };
        let every: Vec<Took> = [
            Took::Joined { job },
            Took::Awaited { trial },
            Took::Reused { trial, gene: gene.clone() },
            Took::TrialFileUnreadable,
            Took::Unsurprised { s: 0.25 },
        ]
        .into_iter()
        .map(|took| match took {
            Took::Joined { job } => Took::Joined { job },
            Took::Awaited { trial } => Took::Awaited { trial },
            Took::Reused { trial, gene } => Took::Reused { trial, gene },
            Took::TrialFileUnreadable => Took::TrialFileUnreadable,
            Took::Unsurprised { s } => Took::Unsurprised { s },
        })
        .collect();
        for took in every {
            let wire = serde_json::to_value(&took).expect("serialize");
            assert!(wire.get("kind").and_then(|k| k.as_str()).is_some(), "still tagged on the wire: {wire}");
            assert_eq!(serde_json::from_value::<Took>(wire).expect("round trip"), took);
        }
        // The persisted row, in whatever tag layout DispatchPhase serializes with, with its
        // `took` as the bare id the pre-#4794 writer stored.
        use crate::modules::training_trigger::DispatchPhase;
        let mut row = serde_json::to_value(DispatchPhase::Held { took: Took::Joined { job } }).expect("serialize");
        fn put_legacy_took(v: &mut serde_json::Value, job: uuid::Uuid) -> bool {
            match v {
                serde_json::Value::Object(map) => {
                    if map.contains_key("took") {
                        map.insert("took".into(), serde_json::Value::String(job.to_string()));
                        return true;
                    }
                    map.values_mut().any(|child| put_legacy_took(child, job))
                }
                _ => false,
            }
        }
        assert!(put_legacy_took(&mut row, job), "the row carries a took: {row}");
        let phase: DispatchPhase = serde_json::from_value(row).expect("the persisted Held row from before #4794");
        assert!(matches!(phase, DispatchPhase::Held { took: Took::Joined { job: j } } if j == job));
    }
    use crate::commands::genome::test_support::{registry_with, request_for};
    use crate::sdk_codegen::{ActionCommand, Ctx};

    fn cmd(ids: &[&'static str]) -> GenomeJobCreate {
        let registry = registry_with(ids);
        GenomeJobCreate {
            attempts: Arc::new(crate::commands::genome::attempt_ledger::AttemptLedger::new()),
            coordinator: Arc::new(FineTuningCoordinator::new(registry)),
            test_job_board: Arc::new(crate::genome::fine_tuning::TrainingJobBoard::default()),
            test_artifacts: Arc::new(tempfile::tempdir().unwrap()),
        }
    }

    /// The decision's world for one test: a recording adapter (so a created job is
    /// visible and its request readable), the test artifacts dir (manifest, trial file
    /// and signature store beside each other, as in production), and a signature for a
    /// gene at `gene_path` minted from the SAME texts the request carries, so the store's
    /// nearest gene is this competence exactly (similarity 1.0 in the lexical space the
    /// tests embed in).
    struct DecisionWorld {
        command: GenomeJobCreate,
        adapter: Arc<crate::genome::fine_tuning::RecordingFineTuningAdapter>,
        board: Arc<crate::genome::fine_tuning::TrainingJobBoard>,
        artifacts: Arc<tempfile::TempDir>,
        persona: uuid::Uuid,
        request: TrainingJobRequest,
        gene_path: std::path::PathBuf,
    }

    async fn decision_world() -> DecisionWorld {
        use crate::genome::fine_tuning::{FineTuningRegistry, RecordingFineTuningAdapter, TrainingJobBoard, RECORDING_BASE_PREFIX};
        let artifacts = Arc::new(tempfile::tempdir().unwrap());
        let board = Arc::new(TrainingJobBoard::with_test_storage(artifacts.clone()));
        let adapter = Arc::new(RecordingFineTuningAdapter::new());
        let registry = Arc::new(FineTuningRegistry::new());
        registry.register(adapter.clone());
        let command = GenomeJobCreate {
            attempts: Arc::new(crate::commands::genome::attempt_ledger::AttemptLedger::new()),
            coordinator: Arc::new(FineTuningCoordinator::new(registry)),
            test_job_board: board.clone(),
            test_artifacts: artifacts.clone(),
        };
        let persona = uuid::Uuid::new_v4(); // unique: a resident region registered for one world must not be seen by another's fill
        let mut request = request_for(RECORDING_BASE_PREFIX);
        request.persona_id = persona;
        let texts: Vec<String> = request.dataset.examples.iter().map(|ex| format!("{}\n{}", ex.prompt, ex.completion)).collect();
        let embedder = crate::ai::embedding::resolve_recall_embedder_local().await;
        let sig = crate::genome::signature::GeneSignature::mint(
            &texts,
            crate::genome::corpus_ref::CorpusRef { name: "near".into(), content_hash: "sha256:near".into(), size_bytes: 1, source_url: None },
            &embedder,
            1,
        )
        .await
        .expect("test: a signature mints in the lexical space");
        let gene_path = artifacts.path().join("genes").join("near.gguf");
        crate::genome::signature::SignatureStore::stamp_at(&artifacts.path().join("signatures.json"), &gene_path.display().to_string(), sig)
            .expect("test: the store takes the signature");
        DecisionWorld { command, adapter, board, artifacts, persona, request, gene_path }
    }

    impl DecisionWorld {
        fn trials(&self) -> crate::genome::gene_trial::GeneTrials {
            crate::genome::gene_trial::GeneTrials::at(self.artifacts.path().join("trials.json"))
        }
        fn manifest(&self) -> std::path::PathBuf {
            self.artifacts.path().join("adapters.json")
        }
        async fn fill(&self) -> JobCreateOutcome {
            self.command
                .run(
                    &Ctx::default(),
                    JobCreateParams { trigger_attempt_id: None, trigger_dispatch_id: None, request: self.request.clone(), preferred_provider: None, dataset_name: None },
                )
                .await
                .unwrap()
        }
        fn open_trial(&self, state: crate::genome::gene_trial::TrialState) -> crate::genome::gene_trial::GeneTrial {
            let trials = self.trials();
            let t = trials.open(self.persona, &self.request.trait_kind, &self.gene_path, &self.request.base_model, continuum_base::time::EpochMs::now().millis()).unwrap();
            if state != crate::genome::gene_trial::TrialState::Trial {
                let mut all = trials.load().unwrap();
                all[0].state = state;
                trials.save_for_test(&all).unwrap();
            }
            t
        }
    }

    // what this catches (card 17dc0a7b, Reuse ACTS): a gene near her competence that her
    // work does not hold is adopted for trial, no job created: the trial file has the row,
    // the serving manifest has the path, the outcome names both, and the adapter saw
    // nothing.
    #[tokio::test]
    async fn a_near_gene_she_lacks_is_adopted_for_trial_and_nothing_trains() {
        let w = decision_world().await;
        let out = w.fill().await;
        assert!(out.success, "{out:?}");
        let Some(Took::Reused { trial, gene }) = out.took else { panic!("reuse expected: {out:?}") };
        assert_eq!(gene, GeneRef::Local { path: w.gene_path.clone() });
        let rows = w.trials().load().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!((rows[0].id, rows[0].persona_id, rows[0].state), (trial, w.persona, crate::genome::gene_trial::TrialState::Trial));
        let registered = crate::genome::adapter_manifest::load_from(&w.manifest()).unwrap();
        assert!(registered.iter().any(|a| a.path == w.gene_path), "the gene serves, dormant: {registered:?}");
        assert_eq!(w.adapter.captured_job_count(), 0, "a reuse trains nothing");
        assert!(w.board.snapshot().is_empty());
    }

    // what this catches (Cormac on #4794, point 4: ONE matching rule): a trial open for the
    // bucket's key holds the fill as Awaited, and the fill after her verdict decides again.
    #[tokio::test]
    async fn a_trial_open_for_the_key_holds_the_fill_until_it_is_decided() {
        let w = decision_world().await;
        let t = w.open_trial(crate::genome::gene_trial::TrialState::Trial);
        let out = w.fill().await;
        assert_eq!(out.took, Some(Took::Awaited { trial: t.id }), "{out:?}");
        assert_eq!(w.adapter.captured_job_count(), 0);
    }

    // what this catches (card 17dc0a7b, Fork carries lineage): a gene her work holds
    // (promoted) near her competence is a parent: the job is created with the parent on
    // its request, and the signature the child is adopted with records it.
    #[tokio::test]
    async fn a_resident_gene_is_forked_with_its_lineage_on_the_request_and_the_signature() {
        let w = decision_world().await;
        w.open_trial(crate::genome::gene_trial::TrialState::Promoted);
        let out = w.fill().await;
        assert!(out.success && out.result.is_some() && out.took.is_none(), "a fork trains: {out:?}");
        let parent = GeneRef::Local { path: w.gene_path.clone() };
        let captured = w.adapter.captures();
        let captured = captured.lock().unwrap();
        assert_eq!(captured.len(), 1);
        assert_eq!(captured[0].parent, Some(parent.clone()), "the trainer is told the parent");
        let jobs = w.board.snapshot();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].signature.as_ref().and_then(|s| s.parent.clone()), Some(parent), "the child's signature carries its lineage");
        assert!(matches!(jobs[0].decision, Some(Decision::Fork { .. })), "{:?}", jobs[0].decision);
    }

    // what this catches (Cormac on #4794, point 2): a gene her work retired (or left
    // unjudged) is never a candidate again and is never adopted again: the decision mints
    // past it, and a direct adoption refuses before touching the manifest.
    #[tokio::test]
    async fn a_decided_gene_is_never_offered_or_adopted_again() {
        let w = decision_world().await;
        w.open_trial(crate::genome::gene_trial::TrialState::Retired);
        let out = w.fill().await;
        assert!(out.result.is_some() && out.took.is_none(), "a retired gene is not reused: {out:?}");
        assert!(matches!(w.board.snapshot()[0].decision, Some(Decision::Mint)));
        let adoption = Adoption::at(w.manifest(), w.trials());
        let refused = adoption.adopt(w.persona, &w.request.trait_kind, &w.gene_path, &w.request.base_model, None, 2);
        assert_eq!(refused, Err(crate::genome::gene_trial::AdoptRefusal::AlreadyDecided(crate::genome::gene_trial::TrialState::Retired)));
        assert!(!w.manifest().exists(), "a refused adoption never registered the gene");
    }

    // what this catches (Cormac on #4796, point 2: the e2e never reached this arm): the
    // DECISION itself, not the trigger's gate, returns Unsurprised when her resident strip
    // reads a measured surprise below the floor, before any reuse is considered; and the
    // same world with no region (never judged) reuses, since not measured is not low.
    #[tokio::test]
    async fn the_decision_itself_holds_an_unsurprised_mind_and_trains_a_never_judged_one() {
        use crate::cognition::attention::{AttentionDial, Continuation};
        use crate::persona::perception_region::PerceptionRegion;
        use crate::cognition::conversation::salience::{BoardChange, Expectation, ExpectedVerdict, ObservedVerdict};
        use std::sync::{Arc as StdArc, Mutex};
        let w = decision_world().await;
        let activity = uuid::Uuid::new_v4();
        let mine = uuid::Uuid::new_v4();
        let now = continuum_base::time::EpochMs::now().millis();
        let mind = tempfile::tempdir().unwrap();
        let (mut region, _strip) = PerceptionRegion::boot(airc_core::PeerId::from_uuid(w.persona), mind.path(), 1_000, now);
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
        for i in 1..=3u64 {
            region.observe_board(activity, vec![BoardChange::Reviewed { card_id: mine, outcome: ObservedVerdict::Passed, reviewer: uuid::Uuid::new_v4(), review: uuid::Uuid::new_v4() }], now + i);
        }
        let region = StdArc::new(Mutex::new(region));
        let (wake_tx, _wake_rx) = tokio::sync::mpsc::channel(4);
        crate::persona::perception_feed::register(w.persona, airc_core::PeerId::from_uuid(w.persona), "Kimi", region, wake_tx, None);
        let out = w.fill().await;
        crate::persona::perception_feed::unregister(w.persona);
        assert_eq!(out.took, Some(Took::Unsurprised { s: 0.0 }), "3 confirmed, 0 contradicted: held before any reuse: {out:?}");
        assert_eq!(w.adapter.captured_job_count(), 0);
        assert!(w.trials().load().unwrap().is_empty(), "no reuse was adopted while she is unsurprised");
        // Never judged: the same near gene is reused.
        let out = w.fill().await;
        assert!(matches!(out.took, Some(Took::Reused { .. })), "not measured is not low: {out:?}");
    }

    // what this catches (card cb14cc13, Cormac on #4794 point 6): a job claimed off the
    // board at Completed is still PENDING while its gene is adopted (convert, register,
    // trial, stamp); a fill in that window joins it, never mints beside it; once the
    // adoption ends the competence decides again.
    #[tokio::test]
    async fn a_completed_job_being_adopted_still_holds_its_competence() {
        use crate::genome::fine_tuning::adapter::FineTuningAdapter;
        let w = decision_world().await;
        let first = w.fill().await;
        // the world's near gene is reused by the first fill; take it off the table so the
        // next fill is about the job alone
        assert!(matches!(first.took, Some(Took::Reused { .. })), "{first:?}");
        let mut all = w.trials().load().unwrap();
        all[0].state = crate::genome::gene_trial::TrialState::Retired;
        w.trials().save_for_test(&all).unwrap();
        let minted = w.fill().await;
        let handle = minted.result.expect("a mint after the reuse was retired").handle;
        // the sentinel sees Completed and claims the job: it leaves the in-flight set and
        // enters adoption
        let status = w.adapter.poll(&handle).await.unwrap();
        assert!(w.board.claim(handle.local_id, &status).is_some());
        assert!(w.board.snapshot().is_empty(), "no longer in flight");
        assert_eq!(w.board.pending().len(), 1, "still pending: being adopted");
        let during = w.fill().await;
        assert_eq!(during.took, Some(Took::Joined { job: handle.local_id }), "a fill during adoption joins: {during:?}");
        w.board.adoption_done(handle.local_id);
        assert!(w.board.pending().is_empty());
        let after = w.fill().await;
        assert!(after.result.is_some(), "the adoption ended (no trial opened in this world): the competence decides again: {after:?}");
    }

    // what this catches (Cormac on #4794, point 5): a trial file that cannot be read
    // refuses the decision loudly and retryably; nothing is minted or reused blind.
    #[tokio::test]
    async fn an_unreadable_trial_file_refuses_the_decision_rather_than_deciding_blind() {
        let w = decision_world().await;
        std::fs::write(w.artifacts.path().join("trials.json"), b"{not json").unwrap();
        let out = w.fill().await;
        assert!(!out.success);
        assert_eq!(out.error_kind.as_deref(), Some(ERROR_KIND_TRIAL_FILE_UNREADABLE), "{out:?}");
        assert_eq!(w.adapter.captured_job_count(), 0);
    }

    // what this catches: name/access wiring — creating a training job spends compute +
    // touches provider credentials, so it lives on the Privileged surface, not AiSafe.
    #[test]
    fn name_and_access_wired() {
        assert_eq!(GenomeJobCreate::NAME, "genome/job-create");
        assert!(matches!(
            GenomeJobCreate::ACCESS,
            crate::sdk_codegen::AccessLevel::Privileged
        ));
    }

    // what this catches: end-to-end happy path. The verb dispatches through the
    // coordinator to the registered adapter and returns success=true + handle +
    // selectedProvider. A refactor that changes the outcome shape breaks every caller.
    #[tokio::test]
    async fn happy_path_returns_handle_and_selected_provider() {
        let out = cmd(&["openai"])
            .run(
                &Ctx::default(),
                JobCreateParams {
                    trigger_attempt_id: None, trigger_dispatch_id: None,
                    request: request_for("gpt-4o-mini"),
                    preferred_provider: None,
                    dataset_name: None,
                },
            )
            .await
            .unwrap();
        assert!(out.success);
        let result = out.result.expect("success carries a result");
        assert_eq!(result.selected_provider, "openai");
        assert_eq!(result.handle.provider_id, "openai");
        assert_eq!(result.handle.provider_job_id, "openai-job-1");
    }

    // what this catches (Codex review of 98180b88b, card 55d4b9e3): at the job-create
    // boundary, an attempted create is ON THE BOARD by the time its attempt reads Created (so
    // Created always implies a watcher), and a repeat of the same attempt returns the same job
    // without calling the trainer again, re-journaling it, or "joining" its own job.
    #[tokio::test]
    async fn an_attempted_create_is_watched_when_created_and_a_repeat_returns_the_same_job() {
        use crate::commands::genome::attempt_ledger::{AttemptLedger, Reconciled};
        use crate::genome::fine_tuning::{FineTuningRegistry, RecordingFineTuningAdapter, TrainingJobBoard, RECORDING_BASE_PREFIX};
        let (store_adapter, _dir) = crate::orm::store::fresh_adapter().await;
        let ledger = Arc::new(AttemptLedger::new());
        ledger.install(Arc::new(crate::orm::OrmStore::new(store_adapter).await.unwrap()));
        let artifacts = Arc::new(tempfile::tempdir().unwrap());
        let board = Arc::new(TrainingJobBoard::with_test_storage(artifacts.clone()));
        let trainer = Arc::new(RecordingFineTuningAdapter::new());
        let registry = Arc::new(FineTuningRegistry::new());
        registry.register(trainer.clone());
        let command = GenomeJobCreate {
            attempts: Arc::clone(&ledger),
            coordinator: Arc::new(FineTuningCoordinator::new(registry)),
            test_job_board: board.clone(),
            test_artifacts: artifacts.clone(),
        };
        let attempt = uuid::Uuid::new_v4();
        let mut request = request_for(RECORDING_BASE_PREFIX);
        request.persona_id = uuid::Uuid::new_v4();
        let params = || JobCreateParams {
            trigger_attempt_id: Some(attempt),
            trigger_dispatch_id: Some(uuid::Uuid::new_v4()),
            request: request.clone(),
            preferred_provider: None,
            dataset_name: None,
        };
        let first = command.run(&Ctx::default(), params()).await.unwrap();
        let job = first.result.expect("created").handle;
        let watched = |id| board.pending().iter().filter(|j| j.handle.local_id == id).count();
        assert_eq!(watched(job.local_id), 1, "on the board");
        assert_eq!(ledger.reconcile(attempt).await, Reconciled::Created(job.clone()), "and only then Created");

        let again = command.run(&Ctx::default(), params()).await.unwrap();
        assert_eq!(again.result.expect("the same job").handle, job);
        assert_eq!(trainer.captured_job_count(), 1, "the trainer ran once");
        assert_eq!(watched(job.local_id), 1, "never registered twice");
    }

    // what this catches (Codex review of 30b89a759): two creates for the SAME attempt can never
    // interleave. Here the test plays the first request mid-decision by holding the attempt's
    // run; a second request must wait, not read "absent" and race. While it waits the first
    // request creates, registers and settles its job; released, the second returns THAT job:
    // no second trainer call, no "join" of its own job, no batch handed back.
    #[tokio::test]
    async fn a_second_create_for_the_same_attempt_waits_and_returns_the_first_job() {
        use crate::commands::genome::attempt_ledger::{Admission, AttemptLedger, AttemptState};
        use std::future::Future;
        use crate::genome::fine_tuning::{FineTuningRegistry, JobHandle, RecordingFineTuningAdapter, TrainingJobBoard, WatchedJob, RECORDING_BASE_PREFIX};
        let (store_adapter, _dir) = crate::orm::store::fresh_adapter().await;
        let ledger = Arc::new(AttemptLedger::new());
        ledger.install(Arc::new(crate::orm::OrmStore::new(store_adapter).await.unwrap()));
        let artifacts = Arc::new(tempfile::tempdir().unwrap());
        let board = Arc::new(TrainingJobBoard::with_test_storage(artifacts.clone()));
        let trainer = Arc::new(RecordingFineTuningAdapter::new());
        let registry = Arc::new(FineTuningRegistry::new());
        registry.register(trainer.clone());
        let command = Arc::new(GenomeJobCreate {
            attempts: Arc::clone(&ledger),
            coordinator: Arc::new(FineTuningCoordinator::new(registry)),
            test_job_board: board.clone(),
            test_artifacts: artifacts.clone(),
        });
        let attempt = uuid::Uuid::new_v4();
        let mut request = request_for(RECORDING_BASE_PREFIX);
        request.persona_id = uuid::Uuid::new_v4();
        let params = JobCreateParams {
            trigger_attempt_id: Some(attempt),
            trigger_dispatch_id: None,
            request: request.clone(),
            preferred_provider: None,
            dataset_name: None,
        };

        let first = ledger.run(attempt).await; // the first request, mid-decision
        assert_eq!(ledger.run_holders(attempt), 2, "the ledger and the first request");
        let ctx = Ctx::default();
        let second = command.run(&ctx, params);
        tokio::pin!(second);
        // Drive the second create until it is parked on the attempt's run lock: its own
        // holder appears beside the first's. Each poll must stay Pending: it cannot pass.
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            while ledger.run_holders(attempt) < 3 {
                std::future::poll_fn(|cx| {
                    assert!(second.as_mut().poll(cx).is_pending(), "the second create waits for the first");
                    std::task::Poll::Ready(())
                })
                .await;
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("the second create reaches the attempt's run lock");
        std::future::poll_fn(|cx| {
            assert!(second.as_mut().poll(cx).is_pending(), "parked on the lock, it stays parked");
            std::task::Poll::Ready(())
        })
        .await;
        assert_eq!(trainer.captured_job_count(), 0);

        // The first request finishes: admitted, its job created, watched, then settled.
        let id = uuid::Uuid::new_v4();
        let job = JobHandle { provider_id: "recording".into(), provider_job_id: id.to_string(), local_id: id };
        assert_eq!(ledger.admit(attempt, None, "recording", "local").await, Ok(Admission::Fresh));
        board.register(WatchedJob {
            trigger_dispatch_id: None,
            handle: job.clone(),
            persona_id: request.persona_id,
            persona_name: request.persona_name.clone(),
            base_model: request.base_model.clone(),
            trait_kind: request.trait_kind.clone(),
            eval_set: None,
            signature: None,
            decision: None,
        });
        ledger.settle(attempt, AttemptState::Created { handle: job.clone() }).await.unwrap();
        drop(first);

        let outcome = tokio::time::timeout(std::time::Duration::from_secs(10), second)
            .await
            .expect("released, the second create answers")
            .unwrap();
        assert!(outcome.success && outcome.took.is_none(), "the job, not a join: {outcome:?}");
        assert_eq!(outcome.result.expect("the first job").handle, job);
        assert_eq!(trainer.captured_job_count(), 0, "the second never called the trainer");
        assert_eq!(board.pending().iter().filter(|w| w.handle.local_id == id).count(), 1);
    }

    // What this catches (278afa6c): the real command must register to its owned
    // ledger and pass fixture output to the provider, preserving explicit paths.
    #[tokio::test]
    async fn job_fixture_owns_ledger_and_supplies_artifact_directory() {
        use crate::genome::fine_tuning::{
            job_board::DispatchLookup, FineTuningRegistry, RecordingFineTuningAdapter,
            TrainingJobBoard, RECORDING_BASE_PREFIX,
        };
        let artifacts = Arc::new(tempfile::tempdir().unwrap());
        let ledger = artifacts.path().join("jobs-ledger.jsonl");
        let board = Arc::new(TrainingJobBoard::with_test_storage(artifacts.clone()));
        let adapter = Arc::new(RecordingFineTuningAdapter::new());
        let registry = Arc::new(FineTuningRegistry::new());
        registry.register(adapter.clone());
        let command = GenomeJobCreate {
            attempts: Arc::new(crate::commands::genome::attempt_ledger::AttemptLedger::new()),
            coordinator: Arc::new(FineTuningCoordinator::new(registry)),
            test_job_board: board.clone(),
            test_artifacts: artifacts.clone(),
        };
        let explicit_output = artifacts.path().join("explicit");
        for output in [None, Some(explicit_output.clone())] {
            let mut request = request_for(RECORDING_BASE_PREFIX);
            request.local_artifact_dir = output;
            // Each round is its own citizen: the first round's job is still on the board,
            // and a second fill of the same competence by the same persona JOINS it (#4791)
            // instead of creating the job this round asserts on.
            request.persona_id = uuid::Uuid::new_v4();
            let dispatch_id = uuid::Uuid::new_v4();
            let outcome = command
                .run(
                    &Ctx::default(),
                    JobCreateParams {
                        trigger_attempt_id: None, trigger_dispatch_id: Some(dispatch_id),
                        request,
                        preferred_provider: None,
                        dataset_name: None,
                    },
                )
                .await
                .unwrap();
            assert!(outcome.success, "{outcome:?}");
            let handle = outcome.result.unwrap().handle;
            assert!(
                matches!(board.lookup_trigger_dispatch(dispatch_id, 0).unwrap(),
                DispatchLookup::Observed(ref observed) if observed.local_id == handle.local_id)
            );
            // A second board can find the receipt only from this fixture's journal.
            let replay = TrainingJobBoard::with_ledger(Some(ledger.clone()));
            assert!(
                matches!(replay.lookup_trigger_dispatch(dispatch_id, 0).unwrap(),
                DispatchLookup::Observed(ref observed) if observed.local_id == handle.local_id)
            );
        }
        let captures = adapter.captures();
        let captures = captures.lock().unwrap();
        assert_eq!(captures.len(), 2);
        assert_eq!(
            captures[0].local_artifact_dir.as_deref(),
            Some(artifacts.path())
        );
        assert_eq!(
            captures[1].local_artifact_dir.as_deref(),
            Some(explicit_output.as_path())
        );
    }

    // what this catches: empty registry → success=false with the NoCapableAdapter
    // text, NOT a transport Err. The outcome-as-data contract: expected domain
    // failures come through success=false, not an Err that would read as a substrate
    // dispatch fault.
    #[tokio::test]
    async fn empty_registry_returns_no_capable_outcome() {
        let out = cmd(&[])
            .run(
                &Ctx::default(),
                JobCreateParams {
                    trigger_attempt_id: None, trigger_dispatch_id: None,
                    request: request_for("gpt-4o-mini"),
                    preferred_provider: None,
                    dataset_name: None,
                },
            )
            .await
            .unwrap();
        assert!(!out.success);
        assert!(out.result.is_none());
        assert!(out.error.unwrap().contains("no fine-tuning adapter"));
    }

    // what this catches: the dataset resolution gate — no data at all, or BOTH
    // an inline dataset and a datasetName, is rejected as success=false BEFORE any
    // adapter is selected. An empty dataset reaching an adapter would burn a real
    // training job on nothing; two datasets is ambiguous.
    #[tokio::test]
    async fn dataset_gate_rejects_empty_and_ambiguous() {
        let mut empty_req = request_for("gpt-4o-mini");
        empty_req.dataset.examples.clear();
        let out = cmd(&["openai"])
            .run(
                &Ctx::default(),
                JobCreateParams {
                    trigger_attempt_id: None, trigger_dispatch_id: None,
                    request: empty_req,
                    preferred_provider: None,
                    dataset_name: None,
                },
            )
            .await
            .unwrap();
        assert!(!out.success);
        assert!(out.error.unwrap().contains("no training data"));

        let out = cmd(&["openai"])
            .run(
                &Ctx::default(),
                JobCreateParams {
                    trigger_attempt_id: None, trigger_dispatch_id: None,
                    request: request_for("gpt-4o-mini"),
                    preferred_provider: None,
                    dataset_name: Some("also-named".into()),
                },
            )
            .await
            .unwrap();
        assert!(!out.success);
        assert!(out.error.unwrap().contains("not both"));
    }

    // what this catches: a datasetName that doesn't exist on disk fails loud with
    // the path convention + the discovery pointer (dataset/list) in the message —
    // the denial teaches the path.
    #[tokio::test]
    async fn missing_dataset_name_fails_with_guidance() {
        let mut req = request_for("gpt-4o-mini");
        req.dataset.examples.clear();
        let out = cmd(&["openai"])
            .run(
                &Ctx::default(),
                JobCreateParams {
                    trigger_attempt_id: None, trigger_dispatch_id: None,
                    request: req,
                    preferred_provider: None,
                    dataset_name: Some("no-such-dataset-xyz".into()),
                },
            )
            .await
            .unwrap();
        assert!(!out.success);
        let err = out.error.unwrap();
        assert!(
            err.contains("no-such-dataset-xyz") && err.contains("dataset/list"),
            "{err}"
        );
    }

    // what this catches: preferredProvider is honored and surfaced in
    // selectedProvider. A refactor that drops the preference would silently route to
    // whichever adapter the rank function preferred — exactly the silent-fallback the
    // coordinator's typed PreferredUnavailable exists to prevent.
    #[tokio::test]
    async fn preferred_provider_is_honored_when_capable() {
        let out = cmd(&["openai", "mistral"])
            .run(
                &Ctx::default(),
                JobCreateParams {
                    trigger_attempt_id: None, trigger_dispatch_id: None,
                    request: request_for("gpt-4o-mini"),
                    preferred_provider: Some("mistral".into()),
                    dataset_name: None,
                },
            )
            .await
            .unwrap();
        assert!(out.success);
        assert_eq!(out.result.unwrap().selected_provider, "mistral");
    }

    // what this catches (card 975d27fa, binding first; Codex on #5412): a repeat that chooses a
    // trainer before it reads its own record. With no trainer selectable (a peer whose
    // advertisement lapsed) it must still answer with its job; with ANOTHER trainer selectable
    // it must still report its own job's provider, never the one selection would pick now.
    #[tokio::test]
    async fn a_repeat_answers_with_its_job_and_its_provider_whatever_is_selectable_now() {
        use crate::commands::genome::attempt_ledger::{Admission, AttemptLedger, AttemptState};
        use crate::genome::fine_tuning::{FineTuningRegistry, JobHandle, TrainingJobBoard};
        let (store_adapter, _dir) = crate::orm::store::fresh_adapter().await;
        let ledger = Arc::new(AttemptLedger::new());
        ledger.install(Arc::new(crate::orm::OrmStore::new(store_adapter).await.unwrap())); // JUSTIFIED: test fixture
        let artifacts = Arc::new(tempfile::tempdir().unwrap()); // JUSTIFIED: test fixture
        let command = GenomeJobCreate {
            attempts: Arc::clone(&ledger),
            coordinator: Arc::new(FineTuningCoordinator::new(Arc::new(FineTuningRegistry::new()))),
            test_job_board: Arc::new(TrainingJobBoard::with_test_storage(artifacts.clone())),
            test_artifacts: artifacts,
        };
        let attempt = uuid::Uuid::new_v4();
        let id = uuid::Uuid::new_v4();
        let job = JobHandle { provider_id: "grid-peer".into(), provider_job_id: id.to_string(), local_id: id };
        assert_eq!(ledger.admit(attempt, None, "grid-peer", "peer:00000000-0000-0000-0000-000000000001").await, Ok(Admission::Fresh));
        ledger.settle(attempt, AttemptState::Created { handle: job.clone() }).await.unwrap(); // JUSTIFIED: test fixture
        let out = command
            .run(
                &Ctx::default(),
                JobCreateParams {
                    trigger_attempt_id: Some(attempt),
                    trigger_dispatch_id: None,
                    request: request_for("qwen3.5-4b"),
                    preferred_provider: None,
                    dataset_name: None,
                },
            )
            .await
            .unwrap(); // JUSTIFIED: test asserts the outcome
        assert!(out.success, "{out:?}");
        let result = out.result.expect("the attempt's job"); // JUSTIFIED: test asserts the result
        assert_eq!((result.handle, result.selected_provider.as_str()), (job.clone(), "grid-peer"));

        // Now another trainer IS selectable: the repeat still answers with its own job and provider.
        let artifacts = Arc::new(tempfile::tempdir().unwrap()); // JUSTIFIED: test fixture
        let with_another = GenomeJobCreate {
            attempts: Arc::clone(&ledger),
            coordinator: Arc::new(FineTuningCoordinator::new(registry_with(&["another-trainer"]))),
            test_job_board: Arc::new(TrainingJobBoard::with_test_storage(artifacts.clone())),
            test_artifacts: artifacts,
        };
        let out = with_another
            .run(
                &Ctx::default(),
                JobCreateParams {
                    trigger_attempt_id: Some(attempt),
                    trigger_dispatch_id: None,
                    request: request_for("qwen3.5-4b"),
                    preferred_provider: None,
                    dataset_name: None,
                },
            )
            .await
            .unwrap(); // JUSTIFIED: test asserts the outcome
        let result = out.result.expect("the attempt's job"); // JUSTIFIED: test asserts the result
        assert_eq!((result.handle, result.selected_provider.as_str()), (job, "grid-peer"), "never the newly selectable trainer");
    }
