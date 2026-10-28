#![cfg(test)]
use super::*;

    // Codex's three cases on #4988, through the same create_job: (1) the useful window EQUAL to
    // the planned one (the floor): a measured no-fit there still parks; (2) examples whose WHOLE
    // length (3,000) and middle-cut length (64) differ park at the whole window, never a cut
    // one; (3) a bound measured on another engine is not evidence on this lane's: no park, the
    // job calibrates at the useful window; on the engine that measured it, it parks.
    #[tokio::test]
    pub(super) async fn the_park_is_checked_at_the_whole_window_on_the_measuring_engine() {
        let gib = 1024u64 * 1024 * 1024;
        let fam = |window: u32| Shape { model: "ggml-org/Qwen3.8-27B-GGUF".into(), window, rank: 8, targets: "attn_q,attn_v".into(), depth: None, exact: false, chunk: TRAINING_CHUNK_MIN, recompute: true };
        let run = |mode: &'static str, bound_at: u32, measured_on: Option<&'static str>, lane_build: Option<&'static str>| async move {
            let train = tempfile::tempdir().expect("test: dir");
            let jobs = tempfile::tempdir().expect("test: dir");
            let (url, server, seen) = fake_lane(train.path().to_path_buf(), mode).await;
            let footprints = jobs.path().join("footprints.json");
            Footprints::at(footprints.clone())
                .for_engine(measured_on.map(str::to_owned))
                .record_with(&fam(bound_at), 2 * gib, false, None, Uuid::nil())
                .expect("test: graph-only refusal");
            let mut t = EngineLoraFineTuner::for_test_with_vram(url.clone(), train.path().to_path_buf(), footprints, gib);
            t.lane = Box::new(move |_: &str| Some(LaneChoice { build: lane_build.map(str::to_owned), ..LaneChoice::at(url.clone(), 61_696) }));
            let mut r = request("ggml-org/Qwen3.8-27B-GGUF");
            r.local_artifact_dir = Some(jobs.path().to_path_buf());
            let created = t.create_job(r).await;
            if let Ok(h) = &created {
                let _ = wait_terminal(&t, h).await;
            }
            let posted = seen.lock().unwrap().clone();
            server.abort();
            (created, posted)
        };
        let parked_at = |created: &Result<JobHandle, FineTuningError>| match created {
            Err(FineTuningError::CapacityParked(wait)) => Some(wait.window),
            _ => None,
        };

        let (created, posted) = run("tiny_turns", TRAINING_WINDOW_MIN, None, None).await;
        assert_eq!(parked_at(&created), Some(TRAINING_WINDOW_MIN), "(1) equal to the floor, still parked: {created:?}");
        assert!(posted.is_none());

        let (created, _) = run("truncatable_turns", 3_072, None, None).await;
        assert_eq!(parked_at(&created), Some(3_072), "(2) the whole window, not the cut one: {created:?}");

        let (created, posted) = run("short_turns", 3_072, Some("old:cuda"), Some("new:cuda")).await;
        assert_eq!(parked_at(&created), None, "(3) another engine's bound does not park: {created:?}");
        assert_eq!(posted.and_then(|p| p["window"].as_u64()), Some(3_072), "it calibrates at the useful window");
        let (created, _) = run("short_turns", 3_072, Some("old:cuda"), Some("old:cuda")).await;
        assert_eq!(parked_at(&created), Some(3_072), "(3) on the engine that measured it, it parks");
    }

    // what this catches (Fable, 2026-10-11): a run planned at the served window whatever its
    // examples need. Short turns (each needing 3,000 tokens) post the window they need, rounded
    // up to 256, not her 61,696-token served window, so a smaller graph leaves room for a
    // larger chunk.
    #[tokio::test]
    pub(super) async fn a_dispatch_plans_the_window_its_examples_need() {
        let train = tempfile::tempdir().expect("test: dir");
        let jobs = tempfile::tempdir().expect("test: dir");
        let (url, server, seen) = fake_lane(train.path().to_path_buf(), "short_turns").await;
        let mut t = EngineLoraFineTuner::for_test(url.clone(), train.path().to_path_buf(), jobs.path().join("footprints.json"));
        t.lane = Box::new(move |_: &str| Some(LaneChoice::at(url.clone(), 61_696)));
        let mut r = request("ggml-org/Qwen3.8-27B-GGUF");
        r.local_artifact_dir = Some(jobs.path().to_path_buf());
        let h = t.create_job(r).await.expect("test: create");
        let _ = wait_terminal(&t, &h).await;
        let posted = seen.lock().unwrap().clone().expect("test: posted");
        assert_eq!(posted["window"].as_u64(), Some(3_072), "3,000 tokens need a 3,072-token window");
        server.abort();
    }

    // what this catches (the 5090, 2026-10-11): a run at a 16,896-token window kept 54 of Kimi's
    // 311 lived turns and skipped 257, all of which the run consumed. A dispatch sends only what
    // the engine's measure says fits, parks the rest in the job's partition.json, and a job none
    // of whose examples fits never POSTs /train at all (no run, nothing consumed).
    #[tokio::test]
    pub(super) async fn a_dispatch_sends_only_what_fits_and_a_job_with_nothing_that_fits_never_runs() {
        for (mode, sent) in [("park_odd", Some(vec!["p0", "p2"])), ("park_all", None)] {
            let train = tempfile::tempdir().expect("test: dir");
            let jobs = tempfile::tempdir().expect("test: dir");
            let (url, server, seen) = fake_lane(train.path().to_path_buf(), mode).await;
            let mut t = EngineLoraFineTuner::for_test(url.clone(), train.path().to_path_buf(), jobs.path().join("footprints.json"));
            t.lane = Box::new(move |_: &str| Some(LaneChoice::at(url.clone(), 2048)));
            let mut r = request("ggml-org/Qwen3.8-27B-GGUF");
            r.local_artifact_dir = Some(jobs.path().to_path_buf());
            r.dataset.examples = (0..3)
                .map(|i| TrainingExample { prompt: format!("p{i}"), completion: "c".into(), metadata: None, lived: None })
                .collect();
            let h = t.create_job(r).await.expect("test: create");
            let end = wait_terminal(&t, &h).await;
            let partition = read_partition(&jobs.path().join(h.local_id.to_string())).expect("test: readable").expect("test: partition recorded");
            match sent {
                Some(prompts) => {
                    let posted = seen.lock().unwrap().clone().expect("test: posted");
                    let got: Vec<&str> = posted["examples"].as_array().expect("test: examples").iter().map(|e| e["prompt"].as_str().unwrap_or("")).collect();
                    assert_eq!(got, prompts, "{mode}: only the examples that fit are sent");
                    assert_eq!((partition.sent, partition.parked), (vec![0, 2], vec![1]), "{mode}: the skipped one is parked");
                }
                None => {
                    assert!(seen.lock().unwrap().is_none(), "{mode}: nothing fits, so no run was posted");
                    assert!(matches!(end, TrainingStatus::Failed { .. }), "{mode}: {end:?}");
                    assert_eq!(partition.parked, vec![0, 1, 2], "{mode}: every example is parked, none consumed");
                }
            }
            server.abort();
        }
    }

    // what this catches (the 5090 2026-10-28 20:16Z, job c9e5b989): a dispatch 2 ms after a
    // handoff met "503 Loading model" at /train/measure and FAILED its job with no partition, so
    // the examples it had just been returned belonged to no one. An engine still loading creates
    // nothing and refuses as certain (TrainerUnavailable: the trigger retries the batch).
    #[tokio::test]
    pub(super) async fn an_engine_still_loading_creates_no_job_and_the_dispatch_retries() {
        let train = tempfile::tempdir().expect("test: dir");
        let jobs = tempfile::tempdir().expect("test: dir");
        let (url, server, seen) = fake_lane(train.path().to_path_buf(), "loading").await;
        let t = {
            let mut t = EngineLoraFineTuner::for_test(url.clone(), train.path().to_path_buf(), jobs.path().join("footprints.json"));
            t.lane = Box::new(move |_: &str| Some(LaneChoice::at(url.clone(), 2048)));
            t
        };
        let artifacts = tempfile::tempdir().expect("test: dir");
        let mut r = request("ggml-org/Qwen3.8-27B-GGUF");
        r.local_artifact_dir = Some(artifacts.path().to_path_buf());
        let refused = t.create_job(r).await;
        assert!(matches!(&refused, Err(FineTuningError::TrainerUnavailable(why)) if why.contains("503")), "{refused:?}");
        assert!(seen.lock().unwrap().is_none(), "no run was posted");
        let created: Vec<_> = std::fs::read_dir(artifacts.path()).expect("test: dir").flatten().map(|e| e.file_name()).collect();
        assert!(created.is_empty(), "no job directory, request or partition exists: {created:?}");
        server.abort();
    }

    // what this catches (Codex on #4498): a footprint filed under the window SENT when the
    // engine ran a smaller graph sized to the data. The lane serves 61,696 a slot; the engine
    // trains at 15,360; the measured graph is filed under w15360, so it can never be leased for
    // a real 61,696-token graph.
    #[tokio::test]
    pub(super) async fn a_footprint_is_filed_under_the_window_the_engine_ran() {
        let train = tempfile::tempdir().expect("test: dir");
        let jobs = tempfile::tempdir().expect("test: dir");
        let (url, server, seen) = fake_lane(train.path().to_path_buf(), "sized").await;
        let footprints = jobs.path().join("footprints.json");
        let mut t = EngineLoraFineTuner::for_test(url.clone(), train.path().to_path_buf(), footprints.clone());
        t.lane = Box::new(move |_: &str| Some(LaneChoice::at(url.clone(), 61_696)));
        let mut r = request("ggml-org/Qwen3.8-27B-GGUF");
        r.local_artifact_dir = Some(jobs.path().to_path_buf());
        // the trigger's dispatch and `return` carry no schedule: the engine runs the default
        r.schedule = None;
        let h = t.create_job(r).await.expect("test: create");
        let TrainingStatus::Completed { .. } = wait_terminal(&t, &h).await else {
            panic!("test: not completed");
        };
        let posted = seen.lock().unwrap().clone().expect("test: posted");
        assert_eq!(posted["window"].as_u64(), Some(61_696), "the served window is sent as the ceiling");
        // what this catches (the 5090, 2026-10-10, job ff186697): every trigger-dispatched gene
        // trained at a second, disagreeing default of 1e-5 and barely moved (-0.2% per epoch)
        assert_eq!(posted["lr"].as_f64(), Some(super::super::super::native_jobs::DEFAULT_LEARNING_RATE), "a run with no schedule trains at the one default learning rate");
        let rows: Value = serde_json::from_slice(&std::fs::read(&footprints).expect("test: footprint filed")).unwrap();
        let keys: Vec<&String> = rows.as_object().unwrap().keys().collect();
        // …and under the chunk that ran: 61,696 is 241 blocks, prime, so the engine's chunk was
        // 256, not the 512 a 15,360 window would take by default; a 15,360 run at 256 is its own
        // (smaller) graph and must never lease a default-chunk row's number (card d6dae498).
        assert_eq!(keys, vec!["ggml-org/Qwen3.8-27B-GGUF|w15360|r8|attn_q,attn_v|c256|rc"], "filed under the window and the chunk that ran");
        server.abort();
    }

    // what this catches: the dispatch end to end against an engine-shaped lane — the request
    // reaches /train as examples with GGUF targets and the lane's window, the request's epochs/rank; the
    // finished adapter LEAVES the lanes' train dir for the job dir before the job is terminal
    // (Fable's invariant: the engine-train sweep deletes files of jobs that are not live); the
    // artifact is a GgufLora with the engine's losses and trainable-token count.
    #[tokio::test]
    pub(super) async fn a_finished_run_moves_its_adapter_out_of_the_train_dir_and_reports_the_engines_losses() {
        let train = tempfile::tempdir().expect("test: dir");
        let jobs = tempfile::tempdir().expect("test: dir");
        let (url, server, seen) = fake_lane(train.path().to_path_buf(), "normal").await;
        let t = EngineLoraFineTuner::for_test(url, train.path().to_path_buf(), jobs.path().join("footprints.json"));
        let mut r = request("ggml-org/Qwen3.8-27B-GGUF");
        r.local_artifact_dir = Some(jobs.path().to_path_buf());
        let h = t.create_job(r).await.expect("test: create");
        let TrainingStatus::Completed { artifact } = wait_terminal(&t, &h).await else {
            panic!("test: not completed");
        };
        let body = seen.lock().unwrap().clone().expect("test: /train was posted");
        assert_eq!(body["targets"], "attn_q,attn_v");
        assert_eq!((body["window"].as_u64(), body["epochs"].as_u64(), body["rank"].as_u64()), (Some(256), Some(2), Some(8)));
        assert_eq!(body["examples"][0]["completion"], "c");
        // what this catches (Kimi's first dream, 2026-09-28): a lived example longer than the
        // window refused the whole run; the engine is asked to keep each example's tail
        assert_eq!(body["fit"], "middle", "a conversation longer than the served window drops its oldest history, never its head");
        assert!(body.get("text").is_none(), "examples, never a text corpus (the engine masks the prompts)");
        // what this catches (fork #36): the bound on her slowdown goes out INSTEAD of the time
        // share; the engine refuses a body carrying both, so sending both fails every run
        assert_eq!(body["max_slowdown_ppm"].as_u64(), Some(u64::from(TRAINING_MAX_SLOWDOWN_PPM)));
        assert!(body.get("share_ppm").is_none(), "the share and the slowdown bound pace the same windows: one is sent");
        // what this catches (fork #47, Fable on #4841): the walk the job asks for follows the
        // monitor; here no monitor runs, so the plain walk, with no unbounded exact walk
        assert_eq!(body["exact"], false, "an unread host trains the plain walk");
        assert!(body.get("walk_host_budget_mib").is_none(), "and sends no budget");
        // what this catches (Kimi's first exact run, the 5090 2026-10-10: refused at chunk 256
        // needing 5.6 GiB more): fork #37's recompute was built and never asked for
        assert_eq!(body["recompute"], true, "every run keeps the layer outputs, not every layer's activations");
        // what this catches (Cormac on #4949): the walk a run trained by, and why, reaches the artifact's
        // record (a probe rotates out); with no host memory read, the plain walk, with that reason
        assert!(matches!(&artifact.metrics.walk, Some(super::super::super::types::TrainingWalk::Plain { reason }) if reason.contains("host memory")), "{:?}", artifact.metrics.walk);
        let path = artifact.local_path.expect("test: path");
        assert_eq!(artifact.format, ArtifactFormat::GgufLora);
        assert!(path.starts_with(jobs.path()) && path.is_file(), "adapter in the job dir: {}", path.display());
        assert_eq!(std::fs::read_dir(train.path()).unwrap().count(), 0, "nothing left in engine-train");
        assert_eq!(artifact.metrics.final_loss, Some(2.1));
        assert_eq!(artifact.metrics.trained_tokens, 80);
        server.abort();
    }

    // what this catches: the exact walk's host budget reading the monitor as anything but
    // "half of what is free" (the other half is the serving engine's file cache), and an unread
    // monitor (0 / None) inventing a budget instead of sending none
    #[test]
    pub(super) fn the_exact_walk_keeps_half_the_free_host_memory() {
        assert_eq!(super::super::exact_walk_host_budget_mib(Some(36 << 30)), Some(18 << 10));
        // regression for Fable on #4841: tighter memory must bound the walk MORE, never unbound it
        assert_eq!(super::super::exact_walk_host_budget_mib(Some(1 << 20)), Some(1), "the floor, which the engine refuses by name");
        assert_eq!(super::super::exact_walk_host_budget_mib(Some(0)), Some(1));
        assert_eq!(super::super::exact_walk_host_budget_mib(None), None, "unread: no exact walk (the caller sends exact: false)");
        // both arms of the request the job sends
        assert_eq!(super::super::walk_request(Some(36 << 30)), (true, Some(18 << 10)), "a read host: the exact walk, bounded");
        assert_eq!(super::super::walk_request(None), (false, None), "an unread host: the plain walk");
    }

    // what this catches (SHARED-RESIDENT-LIFECYCLE.md step 3, Codex and Cormac on the plan):
    // re-attach deciding on anything but positive evidence. Only the SAME live incarnation
    // answering with THIS run attaches; a verifiably dead one (including a new engine that took
    // the port: the record's own start time no longer matches) is gone; an engine answering
    // with another run, not answering, or unreadable, is uncertain and never resumes.
    #[test]
    pub(super) fn re_attach_decides_only_on_positive_evidence() {
        use crate::inference::engine_residency::{EngineIncarnation, Liveness, ProcessProbe};
        let ours = |out: &str| -> TrainStatus {
            serde_json::from_value(json!({"state": "running", "out": out})).expect("test: status")
        };
        let (a, d, u) = (Liveness::Alive, Liveness::Dead, Liveness::Unknown);
        assert_eq!(reattach_disposition(a, true, Some(Ok(ours("j.gguf"))), a, "j.gguf"), Disposition::Attach);
        for (before, spec, answer, after, why) in [
            (a, true, Some(Ok(ours("other.gguf"))), a, "another run is not proof this one ended"),
            (a, true, Some(Err("refused".to_string())), a, "no answer is not proof"),
            (a, false, None, a, "a binding from before job_spec cannot be watched again"),
            (u, true, None, u, "unreadable liveness"),
            (a, true, Some(Ok(ours("j.gguf"))), u, "the answer is this engine's only if it is still the same process"),
        ] {
            assert!(matches!(reattach_disposition(before, spec, answer, after, "j.gguf"), Disposition::Uncertain(_)), "{why}");
        }
        assert!(matches!(reattach_disposition(d, true, None, d, "j.gguf"), Disposition::Gone(_)));
        assert!(matches!(reattach_disposition(a, true, Some(Ok(ours("j.gguf"))), d, "j.gguf"), Disposition::Gone(_)));
        // Cormac's (c): after a restart a NEW engine holds the same port; the old record's own
        // incarnation reads dead, so the new engine is never asked and never attached to.
        let old = EngineIncarnation { pid: 4242, started_s: 1_000, port: 58057 };
        assert_eq!(old.liveness_with(|_| ProcessProbe::Present(2_000)), Liveness::Dead);
        assert!(!ReattachOutcome::Uncertain { reason: String::new() }.permits_resume());
        assert!(ReattachOutcome::EngineGone { reason: String::new() }.permits_resume());
    }

    // what this catches (step 3's whole point): a core-only restart losing a run that is still
    // training, or starting it AGAIN. A successor tuner that never created the job finds its
    // binding, watches the run in the engine under the SAME id WITHOUT a POST, finishes it into
    // the job directory from the recorded spec, and releases the binding once the end is settled.
    #[tokio::test]
    pub(super) async fn a_successor_core_reattaches_a_live_run_under_the_same_id_without_posting_it() {
        let train = tempfile::tempdir().expect("test: dir");
        let jobs = tempfile::tempdir().expect("test: dir");
        let home = tempfile::tempdir().expect("test: dir");
        let (url, server, seen) = fake_lane(train.path().to_path_buf(), "normal").await;
        let job = Uuid::new_v4();
        let out = format!("{job}.gguf");
        // the previous core's POST: the run is in the engine before this core exists
        reqwest::Client::new().post(format!("{url}/train")).json(&json!({"out": out})).send().await.expect("test: post");
        *seen.lock().unwrap() = None;
        let port: u16 = url.rsplit(':').next().and_then(|p| p.parse().ok()).expect("test: port");
        let engine = crate::inference::engine_residency::EngineIncarnation::of(std::process::id(), port).expect("test: this process has a start time");
        let store = crate::inference::engine_residency::store_path(home.path());
        let job_dir = jobs.path().join(job.to_string());
        let spec = EngineRunSpec {
            adapter_path: train.path().join(&out),
            job_dir: job_dir.clone(),
            epochs: 2,
            val_split: SplitPpm::from(0.1),
            model_id: format!("{PROVIDER_ID}:code:{job}"),
            corpus_chars: 0,
            walk: None,
            trained_on: None,
            demand_store: None,
            shape: Shape { model: "m".into(), window: 256, rank: 8, targets: "attn_q,attn_v".into(), depth: None, exact: false, chunk: TRAINING_CHUNK, recompute: false },
        };
        let bound = crate::inference::engine_residency::ResidentWork {
            job,
            out: out.clone(),
            engine,
            base_model: "m".into(),
            created_ms: 1,
            consumer: format!("genome-train:{job}"),
            reserved_bytes: 0,
            interrupted: None,
            job_spec: Some(crate::inference::engine_residency::OwnerSpec::of(&spec).expect("test: spec serializes")),
            memory: None,
        };
        crate::inference::engine_residency::record(&store, bound).expect("test: record");
        let mut successor = EngineLoraFineTuner::for_test(url, train.path().to_path_buf(), jobs.path().join("footprints.json"));
        successor.residency_store = Some(store.clone());

        let ReattachOutcome::Attached { handle } = successor.reattach(job).await.expect("test: reattach") else {
            panic!("test: a live run of this job must re-attach");
        };
        assert_eq!(handle.local_id, job, "the same job, not a new one");
        let TrainingStatus::Completed { artifact } = wait_terminal(&successor, &handle).await else {
            panic!("test: the re-attached run must finish");
        };
        assert!(seen.lock().unwrap().is_none(), "a re-attached run is never POSTed again");
        let path = artifact.local_path.expect("test: path");
        assert!(path.starts_with(&job_dir) && path.is_file(), "finished into the recorded job dir: {}", path.display());
        assert_eq!(artifact.metrics.final_validation_loss, Some(2.4), "the recorded split decides the held-out loss");
        assert!(
            crate::inference::engine_residency::find(&store, job).expect("test: read").is_none(),
            "the settled end released the binding"
        );
        assert!(matches!(successor.reattach(job).await.expect("test: again"), ReattachOutcome::NotResident));
        server.abort();
    }

    /// Run one job against a fake lane in `mode` whose POST does not start it, with the engine
    /// binding recorded, and answer whether the binding is still held when the job has ended.
    pub(super) async fn binding_held_after_a_start_that_did_not_run(mode: &'static str, post_wait: Duration) -> bool {
        let train = tempfile::tempdir().expect("test: dir");
        let jobs = tempfile::tempdir().expect("test: dir");
        let home = tempfile::tempdir().expect("test: dir");
        let (url, server, _seen) = fake_lane(train.path().to_path_buf(), mode).await;
        let job = Uuid::new_v4();
        let out = format!("{job}.gguf");
        let port: u16 = url.rsplit(':').next().and_then(|p| p.parse().ok()).expect("test: port");
        let engine = crate::inference::engine_residency::EngineIncarnation::of(std::process::id(), port).expect("test: this process has a start time");
        let store = crate::inference::engine_residency::store_path(home.path());
        let spec = EngineRunSpec {
            adapter_path: train.path().join(&out),
            job_dir: jobs.path().join(job.to_string()),
            epochs: 1,
            val_split: SplitPpm::from(0.1),
            model_id: format!("{PROVIDER_ID}:code:{job}"),
            corpus_chars: 0,
            walk: None,
            trained_on: None,
            demand_store: None,
            shape: Shape { model: "m".into(), window: 256, rank: 8, targets: "attn_q,attn_v".into(), depth: None, exact: false, chunk: TRAINING_CHUNK, recompute: false },
        };
        let work = crate::inference::engine_residency::ResidentWork {
            job,
            out: out.clone(),
            engine,
            base_model: "m".into(),
            created_ms: 1,
            consumer: format!("genome-train:{job}"),
            reserved_bytes: 0,
            interrupted: None,
            job_spec: Some(crate::inference::engine_residency::OwnerSpec::of(&spec).expect("test: spec serializes")),
            memory: None,
        };
        crate::inference::engine_residency::record(&store, work.clone()).expect("test: record");
        let tuner = EngineLoraFineTuner::for_test(url.clone(), train.path().to_path_buf(), jobs.path().join("footprints.json"));
        let last = Arc::new(Mutex::new(None));
        let run = EngineRun {
            shape: spec.shape.clone(),
            footprints: tuner.footprints.clone(),
            http: tuner.http.clone(),
            lane: url,
            start: Some(TrainRequest {
                examples: Vec::new(),
                out: out.clone(),
                rank: 8,
                alpha: 16,
                targets: "attn_q,attn_v".into(),
                window: 256,
                epochs: 1,
                lr: 1e-4,
                val_split: 0.1,
                seed: 1,
                top_layers: None,
                memory_budget_mib: None,
                max_slowdown_ppm: 0,
                fit: "middle",
                exact: false,
                walk_host_budget_mib: None,
                chunk: None,
                recompute: false,
            }),
            post_wait,
            out: out.clone(),
            adapter_path: spec.adapter_path.clone(),
            job_dir: spec.job_dir.clone(),
            epochs: spec.epochs,
            last: last.clone(),
            holds: tuner.holds.clone(),
            job,
            hold_store: None,
            residency: Some((store.clone(), work)),
            _lane_hold: None,
            _lease: None,
        };
        let finish = finish_run(job, spec, tuner.footprints.clone(), last);
        let handle = tuner.jobs.prepare(job, move |_progress| async move {
            Ok(PreparedJob { execution: Execution::InPlace(Box::new(run)), finish })
        });
        assert!(
            matches!(wait_terminal(&tuner, &handle).await, TrainingStatus::Failed { .. }),
            "test ({mode}): a start that did not run fails the job"
        );
        let held = crate::inference::engine_residency::find(&store, job).expect("test: read").is_some();
        server.abort();
        held
    }

    // what this catches: card 26df (Codex, the 5090), both directions. The engine's
    // documented refusal (HTTP 400, invalid_request_error) proves nothing of this run runs.
    // `settled` waits for THIS run's terminal state, which a refused run never has, so the
    // binding stayed forever and serving stayed off the engine; it must be released. And
    // nothing weaker may release it (Codex on #5001): a 500 or proxy answer is not the
    // engine's refusal, and a lost POST may still be queued while /train names the prior run.
    // Releasing on those would let serving replace an engine a run is about to start in.
    #[tokio::test]
    pub(super) async fn only_the_engines_documented_refusal_releases_a_never_started_runs_binding() {
        assert!(
            !binding_held_after_a_start_that_did_not_run("post_400", POST_WAIT_FLOOR).await,
            "the documented refusal releases the binding"
        );
        assert!(
            binding_held_after_a_start_that_did_not_run("post_500", POST_WAIT_FLOOR).await,
            "a non-2xx that is not the engine's refusal keeps the binding"
        );
        assert!(
            binding_held_after_a_start_that_did_not_run("post_stale", Duration::from_millis(300)).await,
            "a lost POST whose status still names the prior run keeps the binding: it may yet start"
        );
    }

    // what this catches (Codex on #4472): the finish path filing a measured graph under the
    // depth that was ASKED rather than the depth the engine ADAPTED. An engine without
    // top_layers ignores it and measures a full-depth graph; filed under |d8, that number
    // would later be leased for a K=8 run as if it were one. And the gene must carry the
    // depth the engine reported, never the request's.
    #[tokio::test]
    pub(super) async fn a_finished_run_files_its_graph_and_its_gene_under_the_depth_the_engine_adapted() {
        for (mode, filed, gene) in [("normal", "ggml-org/Qwen3.8-27B-GGUF|w256|r8|attn_q,attn_v|rc", None), ("depth", "ggml-org/Qwen3.8-27B-GGUF|w256|r8|attn_q,attn_v|d8|rc", Some(8))] {
            let train = tempfile::tempdir().expect("test: dir");
            let jobs = tempfile::tempdir().expect("test: dir");
            let (url, server, seen) = fake_lane(train.path().to_path_buf(), mode).await;
            let footprints = jobs.path().join("footprints.json");
            let t = EngineLoraFineTuner::for_test(url, train.path().to_path_buf(), footprints.clone());
            let mut r = request("ggml-org/Qwen3.8-27B-GGUF");
            r.local_artifact_dir = Some(jobs.path().to_path_buf());
            r.lora.as_mut().unwrap().top_layers = Some(8);
            let h = t.create_job(r).await.expect("test: create");
            let TrainingStatus::Completed { artifact } = wait_terminal(&t, &h).await else {
                panic!("test: {mode}: not completed");
            };
            let body = seen.lock().unwrap().clone().expect("test: /train was posted");
            assert_eq!(body["top_layers"], 8, "{mode}: the asked depth reaches /train");
            let rows: Value = serde_json::from_slice(&std::fs::read(&footprints).expect("test: footprint filed")).unwrap();
            let keys: Vec<&String> = rows.as_object().unwrap().keys().collect();
            assert_eq!(keys, vec![filed], "{mode}: filed under the depth the engine adapted");
            assert_eq!(artifact.metrics.layers_adapted, gene, "{mode}: the gene's depth is the engine's report");
            server.abort();
        }
    }

    // what this catches (Codex on #4485): two holds sharing a reason collapsing into one, so
    // dropping either resumed training while the other still stood; and a hold on one lane
    // reaching another; and (Cormac) a lane spelled with a trailing slash holding nothing.
    // Each hold is its own; the run stays held until the last one drops.
    #[test]
    pub(super) fn each_hold_is_its_own_and_the_run_stays_held_until_the_last_drops() {
        let set = TrainingHolds::new();
        let lane = "http://127.0.0.1:9001";
        let on = |l: &str| holds_on(&set.subscribe().borrow(), l).len();
        let a = set.hold_on(lane, "a directed turn is waiting");
        let b = set.hold_on(lane, "a directed turn is waiting");
        let c = set.hold_on(&format!("{lane}/"), "a lifecycle drain");
        assert_eq!(on(lane), 3, "same reason twice is two holds; a trailing slash is the same lane");
        assert_eq!(on("http://127.0.0.1:9002"), 0, "another lane is not held");
        drop(a);
        assert_eq!(on(&lane), 2, "the same-reason hold still stands");
        drop(c);
        assert_eq!(on(&lane), 1);
        drop(b);
        assert_eq!(on(&lane), 0, "released only when the last hold drops");
    }

    // what this catches (fork #28, Joel: necessary for continual minds): a training hold that
    // does not pause the engine's run, a paused run the job gives up on (the lease must stay
    // held and the run must survive), or a resume that never reaches the engine. Held: the
    // engine is asked to pause and reports it, and the job stays running; released: it
    // resumes and finishes with its adapter.
    #[tokio::test]
    pub(super) async fn a_held_run_pauses_in_the_engine_and_finishes_after_the_hold_drops() {
        let train = tempfile::tempdir().expect("test: dir");
        let jobs = tempfile::tempdir().expect("test: dir");
        let (url, server, _) = fake_lane(train.path().to_path_buf(), "normal").await;
        let t = EngineLoraFineTuner::for_test(url.clone(), train.path().to_path_buf(), jobs.path().join("footprints.json"));
        let mut r = request("ggml-org/Qwen3.8-27B-GGUF");
        r.local_artifact_dir = Some(jobs.path().to_path_buf());
        let hold = t.holds.hold_on(&url, "test: a directed turn is waiting");
        let h = t.create_job(r).await.expect("test: create");
        tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                let engine: Value = reqwest::get(format!("{url}/test/pause")).await.unwrap().json().await.unwrap();
                if engine["pause_requested"] == true {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("test: the engine never paused under the hold");
        assert!(matches!(t.poll(&h).await.unwrap(), TrainingStatus::Running { .. }), "a paused run is still running");
        drop(hold);
        let TrainingStatus::Completed { artifact } = wait_terminal(&t, &h).await else {
            panic!("test: the run did not finish after the hold dropped");
        };
        assert!(artifact.local_path.expect("test: path").is_file(), "resumed and finished with its adapter");
        server.abort();
    }

    // what this catches (Joel's continual minds; Codex and Cormac on the hold design): a pause
    // asked through genome/job-pause that does not reach the engine, a release that leaves the
    // run paused, or a pause that outlives its job. The pause is a persisted fact keyed by the
    // job; the run steers to it every tick; releasing resumes and
    // the run finishes; and a job that ends (here cancelled while paused) takes its pauses with
    // it, while another job's pause stands.
    #[tokio::test]
    pub(super) async fn a_persisted_pause_steers_its_job_and_ends_with_it() {
        use super::super::super::training_hold_store as store;
        let paused = |url: String| async move {
            tokio::time::timeout(Duration::from_secs(20), async {
                loop {
                    let engine: Value = reqwest::get(format!("{url}/test/pause")).await.unwrap().json().await.unwrap();
                    if engine["pause_requested"] == true {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            })
            .await
            .expect("test: the engine never paused under the persisted hold");
        };
        let state = tempfile::tempdir().expect("test: dir");
        let holds = store::store_path(state.path());
        let bystander = store::add(&holds, Uuid::from_u128(9), "another job's pause", 60_000, EpochMs::now().millis()).expect("test: add");
        for mode in ["release", "cancel"] {
            let train = tempfile::tempdir().expect("test: dir");
            let jobs = tempfile::tempdir().expect("test: dir");
            let (url, server, _) = fake_lane(train.path().to_path_buf(), if mode == "cancel" { "cancel_only" } else { "normal" }).await;
            let mut t = EngineLoraFineTuner::for_test(url.clone(), train.path().to_path_buf(), jobs.path().join("footprints.json"));
            t.hold_store = Some(holds.clone());
            let mut r = request("ggml-org/Qwen3.8-27B-GGUF");
            r.local_artifact_dir = Some(jobs.path().to_path_buf());
            let h = t.create_job(r).await.expect("test: create");
            store::add(&holds, h.local_id, "test: an operator paused it", 60_000, EpochMs::now().millis()).expect("test: add");
            paused(url.clone()).await;
            assert!(matches!(t.poll(&h).await.unwrap(), TrainingStatus::Running { .. }), "{mode}: a paused run is still running");
            if mode == "release" {
                assert_eq!(store::release_job(&holds, h.local_id, EpochMs::now().millis()).unwrap(), 1);
                let TrainingStatus::Completed { .. } = wait_terminal(&t, &h).await else {
                    panic!("test: the run did not finish after its pause was released");
                };
            } else {
                t.cancel(&h).await.expect("test: cancel");
                assert!(matches!(wait_terminal(&t, &h).await, TrainingStatus::Cancelled));
                assert!(store::live_on(&holds, h.local_id, EpochMs::now().millis()).await.unwrap().is_empty(), "the job's end took its pause with it");
            }
            server.abort();
        }
        let left = store::live_on(&holds, bystander.job, EpochMs::now().millis()).await.unwrap();
        assert_eq!(left.len(), 1, "another job's pause stands");
    }

    // what this catches: a cancel that reports Cancelled while the engine is still training
    // (the lease would be released under a live run). The job must call /train/cancel and
    // report Cancelled only after the engine says the run is cancelled; no adapter survives.
    #[tokio::test]
    pub(super) async fn a_cancel_stops_the_run_on_the_engine_before_the_job_reports_cancelled() {
        let train = tempfile::tempdir().expect("test: dir");
        let jobs = tempfile::tempdir().expect("test: dir");
        let (url, server, _) = fake_lane(train.path().to_path_buf(), "cancel_only").await;
        let t = EngineLoraFineTuner::for_test(url.clone(), train.path().to_path_buf(), jobs.path().join("footprints.json"));
        let mut r = request("ggml-org/Qwen3.8-27B-GGUF");
        r.local_artifact_dir = Some(jobs.path().to_path_buf());
        let h = t.create_job(r).await.expect("test: create");
        tokio::time::timeout(Duration::from_secs(10), async {
            while !matches!(t.poll(&h).await.unwrap(), TrainingStatus::Running { .. }) {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("test: never running");
        t.cancel(&h).await.expect("test: cancel");
        assert!(matches!(wait_terminal(&t, &h).await, TrainingStatus::Cancelled));
        let engine: Value = reqwest::get(format!("{url}/train")).await.unwrap().json().await.unwrap();
        assert_eq!(engine["state"], "cancelled", "the engine's run was stopped, not abandoned");
        server.abort();
    }

    // what this catches: a failure path that lets go of the lease while the engine still trains
    // (Cormac on #4443). A status the job cannot read is retried, not taken as the end; a state
    // it does not know makes it STOP the engine's run first, and only then fail.
    #[tokio::test]
    pub(super) async fn no_failure_path_ends_the_job_while_the_engine_still_trains_it() {
        let jobs = tempfile::tempdir().expect("test: dir");
        // flaky: the first two GET /train answers are unreadable, then the run finishes
        let train = tempfile::tempdir().expect("test: dir");
        let (url, server, _) = fake_lane(train.path().to_path_buf(), "flaky").await;
        let t = EngineLoraFineTuner::for_test(url, train.path().to_path_buf(), jobs.path().join("f.json"));
        let mut r = request("ggml-org/Qwen3.8-27B-GGUF");
        r.local_artifact_dir = Some(jobs.path().to_path_buf());
        let h = t.create_job(r).await.expect("test: create");
        assert!(matches!(wait_terminal(&t, &h).await, TrainingStatus::Completed { .. }), "a transient read is retried");
        server.abort();
        // unknown: the engine reports a state the job does not know; it must cancel and wait
        let train = tempfile::tempdir().expect("test: dir");
        let (url, server, _) = fake_lane(train.path().to_path_buf(), "unknown").await;
        let t = EngineLoraFineTuner::for_test(url.clone(), train.path().to_path_buf(), jobs.path().join("f.json"));
        let mut r = request("ggml-org/Qwen3.8-27B-GGUF");
        r.local_artifact_dir = Some(jobs.path().to_path_buf());
        let h = t.create_job(r).await.expect("test: create");
        assert!(matches!(wait_terminal(&t, &h).await, TrainingStatus::Failed { .. }));
        let engine: Value = reqwest::get(format!("{url}/train")).await.unwrap().json().await.unwrap();
        assert_eq!(engine["state"], "cancelled", "failed only after the engine's run was stopped");
        server.abort();
    }

    // what this catches: a footprint that shrinks because one run's sampling missed its peak.
    #[test]
    pub(super) fn a_footprint_keeps_the_largest_observation() {
        let dir = tempfile::tempdir().expect("test: dir");
        let f = Footprints::at(dir.path().join("f.json"));
        let s = Shape { model: "m".into(), window: 256, rank: 8, targets: "attn_q".into(), depth: None, exact: false, chunk: TRAINING_CHUNK, recompute: false };
        f.record_with(&s, 900, true, None, Uuid::nil()).unwrap();
        f.record_with(&s, 700, true, None, Uuid::nil()).unwrap();
        assert_eq!(f.get(&s), Some(900));
        f.record_with(&s, 1200, true, None, Uuid::nil()).unwrap();
        assert_eq!(f.get(&s), Some(1200));
    }

    // what this catches (Codex on #5548 at 2950139e9): one row per shape made a refusal and a run
    // fight. The windows 5090's row for 598e183c (window 45,056, chunk 64) is a refusal filed as a
    // whole run before refusals had their own kind: it trained the fit, nothing could disqualify
    // it, and a kept row hid every newer block count, so depth could never be stepped. Now: a
    // legacy row its own job recorded as an engine refusal of exactly its size leaves the fit; a
    // refusal and a run of one shape coexist, neither replacing nor hiding the other; and the
    // engine's block count lands on every write, an equal or smaller run's included.
    #[test]
    pub(super) fn a_refusal_never_trains_the_fit_or_hides_a_run_and_the_block_count_always_lands() {
        let gib = 1024u64 * 1024 * 1024;
        let home = tempfile::tempdir().expect("test: dir");
        let path = home.path().join("engine-footprints.json");
        let refused = Shape { model: "m".into(), window: 45_056, rank: 8, targets: "attn_q,attn_v".into(), depth: None, exact: false, chunk: TRAINING_CHUNK_MIN, recompute: true };
        let (job, graph) = (Uuid::from_u128(0x598e), 4_164_829_568u64);
        // the legacy row as 2950139e9's predecessor wrote it: whole, no block count, no provenance
        std::fs::write(&path, serde_json::to_vec(&serde_json::json!({ refused.key(): { "bytes": graph, "measuredAtMs": 1, "job": job.to_string(), "includesContext": true } })).unwrap()).unwrap();
        // and its own job's record of the refusal, where `job_dir_for` puts it
        let jd = home.path().join("Kimi").join("code_owner").join(job.to_string());
        write_returnable(&jd, &Returnable::EngineRefused(Refusal { window: 45_056, chunk: 64, depth: None, layers: None, graph_bytes: graph, error: "the training graph did not fit".into() })).unwrap();
        let f = Footprints::at(path.clone());
        assert_eq!(f.get(&refused), Some(graph), "before classifying, the legacy row reads as a run");
        // Codex on #5548: evidence that cannot be read never stamps a refusal as a run
        let record = jd.join(RETURNABLE_FILE);
        let good = std::fs::read(&record).unwrap();
        std::fs::write(&record, b"{ torn").unwrap();
        let unread = f.classify_legacy(&[home.path().to_path_buf()]).unwrap();
        assert_eq!((unread.refusals, unread.unknown.len()), (0, 1), "an unreadable record is unknown: {unread:?}");
        assert_eq!(f.read_all()[&refused.key()].provenance, None, "and the row stays unclassified, to be tried again");
        std::fs::write(&record, good).unwrap();
        assert_eq!(f.classify_legacy(&[home.path().to_path_buf()]).unwrap().refusals, 1, "its own job proves it a refusal");
        assert_eq!(f.get(&refused), None, "a refusal leases nothing");
        assert!(f.rows_like(&refused).is_empty(), "and never trains the fit");
        assert_eq!(f.bound(&refused), Some(graph), "it still rules the shape out");
        assert_eq!(f.bound_rows_like(&refused), vec![(45_056, TRAINING_CHUNK_MIN, graph)], "and caps the window");
        assert_eq!(f.classify_legacy(&[home.path().to_path_buf()]).unwrap(), Classified::default(), "classified once");
        assert_eq!(f.layers("m"), None);
        f.record_refusal(&refused, graph, Some(64), job).unwrap();
        assert_eq!(f.layers("m"), Some(64), "a fresh refusal of the same size attaches the engine's block count");
        // a run of the same shape beside its refusal: neither replaces nor hides the other
        f.record_with(&refused, 3 * gib, true, Some(64), Uuid::nil()).unwrap();
        assert_eq!(f.get(&refused), Some(3 * gib), "the run is leased by its own number");
        assert_eq!(f.bound(&refused), Some(graph), "the larger refusal still bounds it");
        // an equal or smaller run on a kept larger one still lands its block count
        let other = Shape { window: 3_072, ..refused.clone() };
        f.record_with(&other, 2 * gib, true, None, Uuid::nil()).unwrap();
        f.record_with(&other, gib, true, Some(48), Uuid::nil()).unwrap();
        assert_eq!(f.get(&other), Some(2 * gib), "the larger observation stands");
        assert_eq!(f.read_all()[&other.key()].layers, Some(48), "and the newer block count lands on it");
        // a legacy whole row its job never refused is a run, stamped so
        let unproven = Shape { window: 6_144, ..refused.clone() };
        let mut all = f.read_all();
        all.insert(unproven.key(), FootprintRow { engine: None, bytes: 5 * gib, measured_at_ms: 2, job: Uuid::from_u128(7).to_string(), whole_run: true, layers: None, provenance: None });
        std::fs::write(&path, serde_json::to_vec(&all).unwrap()).unwrap();
        // a branch of the job layout that cannot be listed is unknown too, never "no record"
        // (Codex on #5548 at a7bb4b919)
        let locked = home.path().join("Other");
        std::fs::create_dir_all(locked.join("code")).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
            let unread = f.classify_legacy(&[home.path().to_path_buf()]).unwrap();
            std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
            assert_eq!(unread.unknown.len(), 1, "an unlistable persona directory is unknown: {unread:?}");
            assert_eq!(f.read_all()[&unproven.key()].provenance, None, "so the row is not stamped a run");
        }
        assert_eq!(f.classify_legacy(&[home.path().to_path_buf()]).unwrap(), Classified::default());
        assert_eq!(f.get(&unproven), Some(5 * gib), "nothing on disk says it was refused: it stays a run");
        assert_eq!(f.read_all()[&unproven.key()].provenance, Some(Provenance::Run));
    }

    // what this catches (fork #27, Codex's cases): a footprint leased at the wrong depth. The
    // graph is ~linear in the blocks adapted, so a K=8 number leased for a full-depth run
    // under-reserves (the engine then refuses or the node overcommits), and a full-depth number
    // leased for K=8 locks out a run that fits. A persisted reduced-depth row and a legacy
    // depthless (full-depth) row must each answer only their own depth, across a reload.
    #[test]
    pub(super) fn a_footprint_answers_only_the_depth_it_was_measured_at() {
        let dir = tempfile::tempdir().expect("test: dir");
        let path = dir.path().join("f.json");
        let full = Shape { model: "m".into(), window: 1536, rank: 8, targets: "attn_q".into(), depth: None, exact: false, chunk: TRAINING_CHUNK, recompute: false };
        let top8 = Shape { depth: Some(8), ..full.clone() };
        // a row written before depth existed: the key full depth still reads (a whole-run row, so
        // only the depth question is under test here)
        std::fs::write(
            &path,
            serde_json::to_vec(&serde_json::json!({ "m|w1536|r8|attn_q": { "bytes": 41_500, "measuredAtMs": 0, "job": "legacy", "includesContext": true } })).unwrap(),
        )
        .unwrap();
        let f = Footprints::at(path.clone());
        assert_eq!(f.get(&full), Some(41_500), "the legacy row is full depth");
        assert_eq!(f.get(&top8), None, "a reduced-depth lookup never falls back to the legacy row");
        f.record_with(&top8, 5_200, true, None, Uuid::nil()).unwrap();
        let reloaded = Footprints::at(path);
        assert_eq!(reloaded.get(&top8), Some(5_200));
        assert_eq!(reloaded.get(&full), Some(41_500), "the reduced row never answers for full depth");
        // what the engine reported decides the recorded depth: an engine that says nothing
        // (it predates top_layers) adapted every block, and K = n_layer is every block too
        assert_eq!(effective_depth(Some(8), Some(64)), Some(8));
        assert_eq!(effective_depth(Some(64), Some(64)), None);
        assert_eq!(effective_depth(None, None), None);
        // the window is a multiple of 256, rounded down, never under one context
        assert_eq!((train_window(1536), train_window(1600), train_window(100), train_window(61_696)), (1536, 1536, 256, 61_696), "no fixed ceiling: the served window is the window");
    }

    // what this catches: a request the engine cannot run is refused before any job exists:
    // a target with no GGUF equivalent (silently dropping it trains a different adapter), and a
    // base no live lane serves (in-engine training has nothing to train on).
    #[tokio::test]
    pub(super) async fn unknown_targets_and_unserved_bases_are_refused_up_front() {
        assert_eq!(gguf_targets(&["q_proj".into(), "attn_v".into(), "q_proj".into()]).unwrap(), "attn_q,attn_v");
        assert!(gguf_targets(&["lm_head".into()]).is_err());
        let dir = tempfile::tempdir().expect("test: dir");
        let mut t = EngineLoraFineTuner::for_test(String::new(), dir.path().into(), dir.path().join("f.json"));
        t.lane = Box::new(|_: &str| None::<LaneChoice>);
        assert!(matches!(t.create_job(request("some/other-base")).await, Err(FineTuningError::InvalidRequest(_))));
        let mut r = request("ggml-org/Qwen3.8-27B-GGUF");
        r.lora.as_mut().unwrap().target_modules = vec!["lm_head".into()];
        assert!(matches!(t.create_job(r).await, Err(FineTuningError::InvalidRequest(_))));
    }

    // what this catches (Codex on attempt 2): a run with no held-out split reporting a
    // validation loss. The engine's empty eval result is 0.0, and read as a validation loss it
    // claims a perfect held-out score. With fork #31 the count decides; before it, a run that
    // asked for no split has no held-out loss, and a null eval loss is none either way.
    #[test]
    pub(super) fn a_run_with_no_held_out_tokens_has_no_validation_loss() {
        let st = |v: Value| serde_json::from_value::<TrainStatus>(v).expect("test: status");
        let old = |eval| st(json!({"state": "done", "epochs": [{"train_loss": 2.0, "eval_loss": eval}]}));
        assert_eq!(held_out_loss(&old(0.0), 0.0), None, "an old engine, no split asked: its 0.0 is no measurement");
        assert_eq!(held_out_loss(&old(2.4), 0.1), Some(2.4), "an old engine, a split asked: believed");
        let new = |eval: Value, n| st(json!({"state": "done", "eval_trainable_tokens": n, "epochs": [{"train_loss": 2.0, "eval_loss": eval}]}));
        assert_eq!(held_out_loss(&new(Value::Null, 0), 0.1), None, "a split that rounded to no held-out tokens");
        assert_eq!(held_out_loss(&new(json!(2.4), 512), 0.0), Some(2.4), "the engine's count decides, not the request");
    }

    // what this catches: progress that jumps or overflows — completed epochs plus the current
    // epoch's batch fraction, capped at 100.
    #[test]
    pub(super) fn progress_is_completed_epochs_plus_the_current_batch_fraction() {
        let st = |v: Value| serde_json::from_value::<TrainStatus>(v).expect("test: status");
        let e = json!({"train_loss": 2.0, "eval_loss": 2.0});
        assert_eq!(progress_of(&st(json!({"state": "running", "epochs": [e], "batch": 2, "batch_max": 4})), 2), (75.0, 1));
        assert_eq!(progress_of(&st(json!({"state": "starting"})), 2), (0.0, 0));
        assert_eq!(progress_of(&st(json!({"state": "done", "epochs": [e, e, e]})), 2).0, 100.0);
        // the schema: a status without a state does not parse (retried, never read as "not ours"),
        // and a state this core does not know is Unknown, not a known one
        assert!(serde_json::from_value::<TrainStatus>(json!({"out": "x.gguf"})).is_err());
        assert_eq!(st(json!({"state": "paused"})).state, TrainState::Unknown);
    }

    // what this catches (Codex on #5264): recovery re-taking a surviving run's lease in the pool
    // the CURRENT configuration names instead of the pool the run was granted. A placement change
    // across a core restart then charges Ram for a run still using Vram, or the reverse, and lets
    // competing work into the pool actually in use. Each binding names its own pool; a record from
    // before the field was leased in Vram by the build that wrote it. This node's configuration
    // (GPU unless config says cpu) never enters: the Ram-bound run is the reverse direction.
    #[tokio::test]
    async fn recovery_retakes_each_surviving_run_in_the_pool_it_was_bound_in() {
        use crate::resources::{capacity::MockCapacitySource, DaemonConfig, GovernorConfig, ResourceDaemon, ResourceKind};
        let daemon = ResourceDaemon::start(
            vec![
                Arc::new(MockCapacitySource::new(ResourceKind::Vram, 1 << 40)),
                Arc::new(MockCapacitySource::new(ResourceKind::Ram, 1 << 40)),
            ],
            vec![],
            DaemonConfig {
                tick_interval: Duration::from_millis(20),
                min_reclaim_budget: Duration::from_millis(100),
                governor: GovernorConfig { min_dwell_ms: 0, graceful_grace_ms: 50 },
            },
        );
        let home = tempfile::tempdir().expect("test: dir");
        let train = tempfile::tempdir().expect("test: dir");
        let store = crate::inference::engine_residency::store_path(home.path());
        // this process: an engine that is verifiably alive, so every binding is re-taken
        let engine = crate::inference::engine_residency::EngineIncarnation::of(std::process::id(), 1).expect("test: this process has a start time");
        let bound = |consumer: &str, memory: Option<ResourceKind>| crate::inference::engine_residency::ResidentWork {
            job: Uuid::new_v4(),
            out: format!("{consumer}.gguf"),
            engine,
            base_model: "m".into(),
            created_ms: 1,
            consumer: consumer.into(),
            reserved_bytes: 4096,
            interrupted: None,
            job_spec: None,
            memory,
        };
        for work in [bound("gpu-run", Some(ResourceKind::Vram)), bound("cpu-run", Some(ResourceKind::Ram)), bound("older-run", None)] {
            crate::inference::engine_residency::record(&store, work).expect("test: record");
        }
        let mut tuner = EngineLoraFineTuner::for_test("http://127.0.0.1:1".into(), train.path().to_path_buf(), train.path().join("footprints.json"));
        tuner.residency_store = Some(store);
        tuner.reclaim_resident_leases_in(&daemon);
        let pool_of = |consumer: &str| daemon.board().leases.iter().find(|l| l.consumer_id == consumer).map(|l| l.kind);
        assert_eq!(pool_of("gpu-run"), Some(ResourceKind::Vram), "a Vram-bound run is re-taken in Vram");
        assert_eq!(pool_of("cpu-run"), Some(ResourceKind::Ram), "a Ram-bound run is re-taken in Ram");
        assert_eq!(pool_of("older-run"), Some(ResourceKind::Vram), "a record from before the field: the Vram its build leased");
    }

    // what this catches (Codex on #5264): admission binding a run to a lane that is no longer the
    // one chosen. The check is the production callback's own logic: the same incarnation at the
    // same endpoint serving the same base passes; a moved endpoint, another base, or no lane at
    // all does not.
    #[test]
    fn a_lane_is_still_bound_only_at_its_incarnation_endpoint_and_base() {
        use crate::inference::lane_registry::{LaneRecord, LaneRole};
        let rec = LaneRecord {
            pid: 4242,
            port: 8090,
            role: LaneRole::Live,
            model: "qwen/base".into(),
            context_window: 8192,
            lanes: 2,
            page_dir: None,
            engine_bin: None,
            weights: None,
            started_s: 1_700_000_000,
            placement: None,
        };
        let chosen = Some(rec.incarnation());
        let url = rec.root_url();
        assert!(still_bound_to(Some(rec.clone()), "qwen/base", chosen, &url).is_ok(), "the lane chosen still serves");
        assert!(matches!(still_bound_to(Some(rec.clone()), "qwen/base", chosen, "http://127.0.0.1:9999"), Err(Unbound::Replaced(_))), "another endpoint");
        assert!(matches!(still_bound_to(Some(rec.clone()), "other/base", chosen, &url), Err(Unbound::Replaced(_))), "another base");
        let relaunched = LaneRecord { pid: 5151, started_s: 1_700_000_100, ..rec.clone() };
        assert!(matches!(still_bound_to(Some(relaunched), "qwen/base", chosen, &url), Err(Unbound::Replaced(_))), "another incarnation");
        assert!(matches!(still_bound_to(None, "qwen/base", chosen, &url), Err(Unbound::Gone(_))), "no lane");
    }

    // what this catches (Codex on #5264): training priced in the pool the CURRENT configuration
    // names for a lane that was launched under another. A lane's own recorded placement decides
    // its pool; a lane recorded before placements were is not offered at all, whatever the
    // configuration says now.
    #[test]
    fn a_lane_is_priced_in_its_recorded_placement_and_an_unrecorded_one_is_not_offered() {
        use crate::inference::lane_registry::{LaneRecord, LaneRole};
        use crate::inference::llama_server::LanePlacement;
        use crate::resources::ResourceKind;
        let rec = |placement: Option<LanePlacement>| LaneRecord {
            pid: 4242,
            port: 8090,
            role: LaneRole::Live,
            model: "qwen/base".into(),
            context_window: 8192,
            lanes: 2,
            page_dir: None,
            engine_bin: None,
            weights: None,
            started_s: 1_700_000_000,
            placement,
        };
        let pool = |placement| lane_choice(rec(placement), "qwen/base").map(|choice| choice.memory);
        assert_eq!(pool(Some(LanePlacement::Gpu)), Some(ResourceKind::Vram));
        assert_eq!(pool(Some(LanePlacement::Cpu)), Some(ResourceKind::Ram));
        assert_eq!(pool(None), None, "an unrecorded placement is refused, never read from configuration");
        assert!(lane_choice(rec(Some(LanePlacement::Gpu)), "other/base").is_none(), "another base");
    }
