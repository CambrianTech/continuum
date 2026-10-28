#![cfg(test)]

    use super::*;
    use continuum_base::time::EpochMs;
    use crate::genome::fine_tuning::{LoRAHyperparams, ScheduleParams, TrainingDataset, TrainingSource};

    // what this catches (Cormac's chain test on #4946, card 3cef97ef; job 97e081e2): the WHOLE
    // path of a POST lost after the engine took the run. The engine's answer outlasts the POST's
    // wait, the core asks GET /train, finds this job's run, watches it to its end, and the job
    // completes with its adapter collected: not failed while the engine trains, unadoptable. And
    // (card ea157b1a) the examples it trained owe serving nothing more: her demand for them clears
    // at the finish, and only there.
    #[tokio::test]
    pub(super) async fn a_run_whose_post_is_lost_is_watched_to_its_end_and_completes() {
        let train = tempfile::tempdir().expect("test: dir");
        let jobs = tempfile::tempdir().expect("test: dir");
        let (url, server, seen) = fake_lane(train.path().to_path_buf(), "lost_post").await;
        let mut t = EngineLoraFineTuner::for_test(url, train.path().to_path_buf(), jobs.path().join("footprints.json"));
        t.demand_store = Some(jobs.path().join("training-demand"));
        let mut r = request("ggml-org/Qwen3.8-27B-GGUF");
        r.local_artifact_dir = Some(jobs.path().to_path_buf());
        let demand = crate::genome::fine_tuning::training_demand::TrainingDemand::in_dir(t.demand_store.as_deref(), r.persona_id);
        let parked_before: Vec<(String, u32)> = r.dataset.examples.iter().map(|e| (crate::genome::fine_tuning::training_demand::TrainingDemand::key_of(e), 65_536)).collect();
        demand.record(&parked_before, &[], None).expect("test: her examples waited for a window");
        let h = t.create_job(r).await.expect("test: create");
        let end = wait_terminal(&t, &h).await;
        assert!(seen.lock().unwrap().is_some(), "the run was posted");
        assert!(matches!(end, TrainingStatus::Completed { .. }), "reconciled and watched to its end: {end:?}");
        assert_eq!(demand.waiting(), 0, "the run trained them: her demand is met");
        server.abort();
    }

    // what this catches (card 3cef97ef, Cormac on #4939; job 97e081e2's 6.2 GB went unfiled): a
    // lost POST whose run the engine then REFUSED. The job fails, and the two records a shortcut
    // once dropped are both written: the refused graph's footprint (so the next plan knows it) and
    // returnable.json (so the examples go back for a smaller chunk instead of stranding).
    #[tokio::test]
    pub(super) async fn a_lost_post_whose_run_is_refused_files_its_graph_and_its_returnable() {
        let train = tempfile::tempdir().expect("test: dir");
        let jobs = tempfile::tempdir().expect("test: dir");
        let (url, server, _seen) = fake_lane(train.path().to_path_buf(), "lost_post_refused").await;
        let footprints = jobs.path().join("footprints.json");
        let mut t = EngineLoraFineTuner::for_test_with_vram(url.clone(), train.path().to_path_buf(), footprints.clone(), 6 * 1024 * 1024 * 1024);
        t.lane = Box::new(move |_: &str| Some(LaneChoice::at(url.clone(), 1536)));
        t.demand_store = Some(jobs.path().join("training-demand"));
        let mut r = request("ggml-org/Qwen3.8-27B-GGUF");
        r.local_artifact_dir = Some(jobs.path().to_path_buf());
        // card ea157b1a: her examples were waiting; this run is sent them and refused
        let demand = crate::genome::fine_tuning::training_demand::TrainingDemand::in_dir(t.demand_store.as_deref(), r.persona_id);
        let parked_before: Vec<(String, u32)> = r.dataset.examples.iter().map(|e| (crate::genome::fine_tuning::training_demand::TrainingDemand::key_of(e), 65_536)).collect();
        demand.record(&parked_before, &[], None).expect("test: her examples waited for a window");
        let h = t.create_job(r).await.expect("test: create");
        let end = wait_terminal(&t, &h).await;
        assert!(matches!(end, TrainingStatus::Failed { .. }), "refused: {end:?}");
        assert_eq!(demand.waiting(), parked_before.len(), "sent and refused is not trained: her demand stands");
        let rows: Value = serde_json::from_slice(&std::fs::read(&footprints).expect("test: the refusal filed a footprint")).expect("test: json");
        let filed: Vec<u64> = rows.as_object().expect("test: rows").values().filter_map(|r| r["bytes"].as_u64()).collect();
        assert_eq!(filed, vec![(12_654.1f64 * 1024.0 * 1024.0) as u64], "the refused graph is filed");
        assert!(
            matches!(read_returnable(&jobs.path().join(h.local_id.to_string())), Some(Returnable::EngineRefused(_))),
            "and the job is returnable, so its examples go back for a smaller chunk"
        );
        server.abort();
    }

    // what this catches (card 93ffc555; the 5090, 2026-10-11 10:08Z, job 97e081e2): a POST lost on
    // the way back failed a job the engine then trained, unadoptable. The engine's status after
    // the loss decides by the run's id: this job's run in any state (its refusal included) is
    // watched, so its end is handled and filed; another run, none, or no answer: never started.
    #[test]
    pub(super) fn a_lost_post_is_reconciled_by_the_runs_id() {
        let status = |body: Value| serde_json::from_value::<TrainStatus>(body).expect("test: status");
        let ours = |s: &TrainStatus| s.out.as_deref() == Some("job.gguf");
        for state in ["preparing", "starting", "running", "done"] {
            assert_eq!(after_lost_post(Some(&status(json!({"state": state, "out": "job.gguf"}))), ours), LostPost::Watch, "{state}");
        }
        assert_eq!(
            after_lost_post(Some(&status(json!({"state": "error", "out": "job.gguf", "graph_mib": 6200.2}))), ours),
            LostPost::Watch,
            "this job's refusal goes to the watch loop, which files its graph and its returnable (Cormac on #4946)"
        );
        assert_eq!(after_lost_post(Some(&status(json!({"state": "running", "out": "other.gguf"}))), ours), LostPost::NeverStarted, "another run");
        assert_eq!(after_lost_post(Some(&status(json!({"state": "idle"}))), ours), LostPost::NeverStarted);
        assert_eq!(after_lost_post(None, ours), LostPost::NeverStarted, "no answer");
    }

    // what this catches (the 5090, 2026-10-11 10:08Z, job 97e081e2): a fixed 120 s wait for
    // /train to accept a run that renders 11 lived turns at ~13 s each, so the core failed a job
    // the engine went on to train, and nothing adopted its gene. The wait is the measured render
    // time per example times the examples sent, doubled, never under the old 120 s.
    #[test]
    pub(super) fn the_post_wait_is_the_measured_render_of_what_was_sent() {
        let measured = Duration::from_millis(205_000 / 16); // 16 measured in 205 s
        assert_eq!(post_wait_for(measured, 11, POST_WAIT_FLOOR), Duration::from_millis(205_000 / 16 * 11 * 2), "11 sent: ~282 s, past the ~143 s they take");
        assert_eq!(post_wait_for(Duration::from_millis(50), 3, POST_WAIT_FLOOR), POST_WAIT_FLOOR, "a small batch on a fast engine keeps the floor");
    }

    // what this catches (Cormac on #4939): a refusal at the floor chunk returned for a smaller
    // window that the fit did not predict, so the same window planned again and the return
    // looped; and graph-only rows mixed into whole-run fits. A floor-chunk measurement over what
    // is free caps the next window below it whatever the fit says (with no fit at all, too), and
    // a graph-only row is never fitted (it still bounds the plan), while a whole-run row replaces it.
    #[test]
    pub(super) fn a_floor_refusal_caps_the_next_window_and_graph_only_rows_bound_but_never_fit() {
        let gib = 1024u64 * 1024 * 1024;
        let free = 4 * gib;
        let refused = [(40_960, TRAINING_CHUNK_MIN, 5 * gib)];
        assert_eq!(choose_window(68_608, free, &refused, &refused).window, 40_704, "under the refused window, with no fit to say so");
        let optimistic = [(1_536, 256, gib / 4), (68_608, 512, gib), (40_960, TRAINING_CHUNK_MIN, 5 * gib)];
        assert!(choose_window(68_608, free, &optimistic, &optimistic).window <= 40_704, "under it even when the fit says the served window fits");
        let at_floor = [(TRAINING_WINDOW_MIN, TRAINING_CHUNK_MIN, 5 * gib)];
        assert_eq!(choose_window(68_608, free, &at_floor, &at_floor).window, TRAINING_WINDOW_MIN, "never under the smallest window");
        let dir = tempfile::tempdir().expect("test: dir");
        let store = Footprints::at(dir.path().join("fp.json"));
        let shape = Shape { model: "m".into(), window: 1536, rank: 8, targets: "attn_q".into(), depth: None, exact: true, chunk: 256, recompute: true };
        store.record_with(&shape, 9 * gib, false, None, Uuid::nil()).expect("test: graph-only");
        assert_eq!(store.get(&shape), None, "a graph-only row leases nothing: the shape calibrates");
        assert!(store.rows_like(&shape).is_empty(), "and is not fitted");
        // regression for card 030d70f0 (the 5090, 84 of 108 jobs failed): her refusals were filed
        // graph-only, which no plan read. A graph alone over what is free is a LOWER bound the
        // plan obeys, though the fit ignores it: at the floor chunk it caps the window.
        assert_eq!(store.bound(&shape), Some(9 * gib), "a graph-only row still bounds its shape");
        let served = Shape { window: 68_608, chunk: TRAINING_CHUNK_MIN, ..shape.clone() };
        store.record_with(&served, 13 * gib, false, None, Uuid::nil()).expect("test: graph-only refusal at the served window");
        let (rows, bounds) = (store.rows_like(&served), store.bound_rows_like(&served));
        assert!(rows.is_empty(), "the refusal is not fitted");
        assert!(choose_window(68_608, free, &rows, &bounds).window < 68_608, "but the next plan steps under the refused window");
        // The 5090's actual row (Codex, read from its footprints file): graph-only at the
        // engine's DEFAULT chunk, 18.6 GB, so the floor-chunk cap never sees it. The chunk ladder
        // must: it read None there and calibrated chunk 512 again each time.
        let default_chunk = Shape { window: 68_608, chunk: train_chunk_for(68_608, TRAINING_CHUNK), ..shape.clone() };
        store.record_with(&default_chunk, 18_587_879_424, false, None, Uuid::nil()).expect("test: the 5090's row");
        let at = |c: u32| Shape { chunk: c, ..default_chunk.clone() };
        let (chunk, _) = choose_chunk(68_608, free, |c| store.need(&at(c), None).and_then(|n| n.decides(free)));
        assert!(chunk < default_chunk.chunk, "the refused default chunk is never calibrated again");
        assert_eq!(store.need(&at(default_chunk.chunk), Some(gib)).map(|n| (n.bytes, n.source)), Some((18_587_879_424, crate::inference::planning::training_plan::NeedSource::LowerBound)), "a measured bound outranks an optimistic fit");
        store.record_with(&shape, 3 * gib, true, None, Uuid::nil()).expect("test: whole run");
        assert_eq!(store.get(&shape), Some(3 * gib), "a whole-run row replaces it, smaller or not");
        store.record_with(&shape, 20 * gib, false, None, Uuid::nil()).expect("test: a rolled-back engine");
        assert_eq!(store.get(&shape), Some(3 * gib), "and a graph-only row never replaces a whole-run one, larger or not");
        // card 0d21e644: the row's one flag is stored under its first name, so every row on a
        // node reads the same after the rename; and the block count an engine reported with a
        // measurement is what depth rungs are halved from, the latest one standing
        let text = std::fs::read_to_string(dir.path().join("fp.json")).expect("test: read");
        assert!(text.contains("\"includesContext\": true") && !text.contains("wholeRun"), "stored under its first name: {text}");
        assert_eq!(store.layers("m"), None, "no engine has reported the blocks yet: no rung");
        store.record_refusal(&Shape { window: 3072, ..shape.clone() }, 4 * gib, Some(64), Uuid::nil()).expect("test: a refusal");
        assert_eq!(store.layers("m"), Some(64));
        assert_eq!(store.layers("other"), None, "another model's rows say nothing about this one");
    }

    // what this catches (Codex on #5261): with no continuum home the store used to resolve to a
    // cwd-relative engine-footprints.json and write it into whatever directory the core ran
    // from. An unrecorded store has no file: a record succeeds and keeps nothing, so every
    // shape re-measures (a calibration run), and no path exists to write to.
    #[test]
    fn an_unrecorded_store_keeps_nothing_and_has_no_file() {
        let store = Footprints::unrecorded();
        let shape = Shape { model: "m".into(), window: 1536, rank: 8, targets: "attn_q".into(), depth: None, exact: true, chunk: 256, recompute: true };
        store.record_with(&shape, 3 * 1024 * 1024 * 1024, true, None, Uuid::nil()).expect("test: an unrecorded record is not an error");
        assert_eq!(store.get(&shape), None, "nothing was kept: the shape calibrates again");
        assert!(store.read_all().is_empty());
        assert!(store.path.is_none(), "no file to write, cwd-relative or otherwise");
        assert!(store.for_engine(Some("b1".into())).path.is_none(), "an engine view of it stays unrecorded");
    }

    // what this catches (the 5090, 2026-10-11): footprints filed from graph_mib alone, while the
    // training context's K/V for the whole window (allocated outside the graph's budget) sat
    // outside the lease. A run's footprint is its graph AND its context (fork #52); an engine
    // before #52 reports no context and files the graph, as before; no graph is no measurement.
    #[test]
    pub(super) fn a_runs_footprint_is_its_graph_and_its_context() {
        let status = |body: Value| serde_json::from_value::<TrainStatus>(body).expect("test: status");
        let mib = 1024u64 * 1024;
        assert_eq!(status(json!({"state": "done", "graph_mib": 5.0, "context_mib": 2.0})).run_bytes(), 7 * mib);
        assert_eq!(status(json!({"state": "done", "graph_mib": 5.0})).run_bytes(), 5 * mib, "an engine before #52: the graph");
        assert_eq!(status(json!({"state": "error", "context_mib": 2.0})).run_bytes(), 0, "no graph measured: nothing to file");
    }

    #[test]
    pub(super) fn a_refused_job_records_its_shape_and_the_ladder_ends_at_the_smallest_chunk() {
        // what this catches (card 2dfce676): the record the trigger returns a refused job by.
        // It round-trips through the job's directory; a directory without one is no refusal;
        // and a return happens only while a smaller chunk is left, so a job refused even at
        // the smallest is never returned into the same refusal forever.
        let dir = tempfile::tempdir().expect("test: tempdir");
        assert_eq!(read_returnable(dir.path()), None, "no record, no return");
        let refused = Refusal { window: 1536, chunk: 512, depth: None, layers: None, graph_bytes: 12_654 << 20, error: "over budget".into() };
        // #4904's legacy refusal.json is still read, as EngineRefused
        std::fs::write(dir.path().join(LEGACY_REFUSAL_FILE), serde_json::to_string(&refused).expect("test: json")).expect("test: legacy");
        assert_eq!(read_returnable(dir.path()), Some(Returnable::EngineRefused(refused.clone())));
        let replaced = Returnable::LaneReplaced { error: "the lane was replaced while this job waited".into() };
        write_returnable(dir.path(), &replaced).expect("test: write");
        assert_eq!(read_returnable(dir.path()), Some(replaced), "returnable.json wins over the legacy record");
        assert!(refusal_has_smaller_rung(&refused), "512 steps down to 256");
        assert!(refusal_has_smaller_rung(&Refusal { chunk: 256, ..refused.clone() }), "256 steps down to 128 (fork #52)");
        let floor = Refusal { window: TRAINING_WINDOW_MIN, chunk: TRAINING_CHUNK_MIN, ..refused.clone() };
        assert!(!refusal_has_smaller_rung(&floor), "the smallest chunk at the smallest window is the floor");
        assert!(
            refusal_has_smaller_rung(&Refusal { window: 68_608, ..floor.clone() }),
            "at the smallest chunk a larger window still steps down: the refusal filed its graph, so the next plan calculates a smaller window"
        );
        // card 0d21e644: a shallower depth is a rung too, once the engine has reported its blocks
        assert!(refusal_has_smaller_rung(&Refusal { layers: Some(64), ..floor.clone() }), "every block of 64 adapted: 32 is left");
        assert!(refusal_has_smaller_rung(&Refusal { depth: Some(2), layers: Some(64), ..floor.clone() }), "two blocks: one is left");
        assert!(!refusal_has_smaller_rung(&Refusal { depth: Some(1), layers: Some(64), ..floor }), "one block at the floor chunk and window: the ladder ends");
    }

    // what this catches (the 5090, 2026-10-11): at a 16,896-token window the engine kept 54 of
    // Kimi's 311 lived turns and skipped 257, which the run then counted as consumed. The
    // measurement's skipped examples are parked, not sent; the RUN's own skipped indices (into
    // what was sent) park their request examples too; and the record survives a restart.
    #[test]
    pub(super) fn a_partition_sends_what_fits_and_parks_what_the_measure_or_the_run_skipped() {
        let row = |index, fit| MeasuredExample { index, tokens_full: 30_000, tokens_min: 20_000, tokens_trained: 0, fit };
        let p = partition_of(
            16_896,
            vec![row(0, ExampleFit::Kept), row(1, ExampleFit::Skipped), row(2, ExampleFit::Truncated), row(3, ExampleFit::Skipped), row(4, ExampleFit::Kept)],
        );
        assert_eq!(p.sent, vec![0, 4], "only examples that train whole are sent, in request order");
        assert_eq!(p.parked, vec![1, 2, 3], "a cut example is parked with the skipped ones, never sent cut");
        let amended = with_run_skipped(p.clone(), &[1, 9]);
        assert_eq!(amended.parked, vec![1, 2, 3, 4], "the run's index 1 is request index 4; an index past what was sent is ignored");
        assert_eq!(with_run_skipped(amended.clone(), &[1]).parked, vec![1, 2, 3, 4], "parking twice is once");
        let dir = tempfile::tempdir().expect("test: dir");
        assert_eq!(read_partition(dir.path()), Ok(None), "a job from before the record has none");
        write_partition(dir.path(), &amended).expect("test: written");
        assert_eq!(read_partition(dir.path()), Ok(Some(amended)), "read back as written");
        std::fs::write(dir.path().join(PARTITION_FILE), b"{ torn").expect("test: corrupt it");
        assert!(read_partition(dir.path()).is_err(), "a corrupt partition is an error, never 'nothing parked'");
    }

    // what this catches (card d6dae498; the 5090, 2026-10-10 04:34Z): a plain-walk footprint
    // leased for an exact run, or a footprint measured at one chunk leased for another, and a
    // store that answers one family with another's rows. (The chunk rule, the window and the
    // chunk ladder are the planner's: training_plan's tests.)
    #[test]
    pub(super) fn the_walk_and_its_chunk_are_the_footprints_shape_and_the_chunk_fits_the_lease() {
        // the key: a plain walk at the default chunk is the row every older core wrote;
        // the exact walk and any other chunk never land on it
        let plain = Shape { model: "m".into(), window: 1536, rank: 8, targets: "attn_q".into(), depth: None, exact: false, chunk: TRAINING_CHUNK, recompute: false };
        assert_eq!(plain.key(), "m|w1536|r8|attn_q");
        let exact = Shape { exact: true, ..plain.clone() };
        assert_eq!(exact.key(), "m|w1536|r8|attn_q|exact");
        let small = Shape { chunk: 256, ..exact.clone() };
        assert_eq!(small.key(), "m|w1536|r8|attn_q|c256|exact");
        let rebuilt = Shape { recompute: true, ..small.clone() };
        assert_eq!(rebuilt.key(), "m|w1536|r8|attn_q|c256|exact|rc", "a footprint that keeps every layer's activations is never leased for recompute, nor the reverse");
        assert_ne!(exact.key(), plain.key(), "a plain-walk footprint is never leased for the exact walk");
        let one_chunk = Shape { window: 256, chunk: 256, ..plain.clone() };
        assert_eq!(one_chunk.key(), "m|w256|r8|attn_q", "a 256 window's only chunk is the engine's default there: the old key");
        // the store reads its rows back by rebuilding the key, so another family never counts
        let dir = tempfile::tempdir().expect("test: dir");
        let store = Footprints::at(dir.path().join("fp.json"));
        let fam = Shape { model: "m".into(), window: 1536, rank: 8, targets: "attn_q".into(), depth: None, exact: true, chunk: 256, recompute: true };
        store.record_with(&fam, 7, true, None, Uuid::nil()).expect("test: record");
        store.record_with(&Shape { window: 68608, chunk: 512, ..fam.clone() }, 9, true, None, Uuid::nil()).expect("test: record");
        store.record_with(&Shape { recompute: false, ..fam.clone() }, 99, true, None, Uuid::nil()).expect("test: record");
        let mut got = store.rows_like(&fam);
        got.sort_unstable();
        assert_eq!(got, vec![(1536, 256, 7), (68608, 512, 9)], "its own family only, at every window");
    }

    /// A lane source whose acquisition fails with a given error, so create_job's handling of it is
    /// tested without standing up a serving process.
    struct RefusingLane(fn() -> FineTuningError);

    #[async_trait]
    impl LaneSource for RefusingLane {
        fn serves(&self, _: &str) -> bool {
            true
        }

        async fn acquire(&self, _: &str) -> Result<LaneLease, FineTuningError> {
            Err((self.0)())
        }

        fn provider(&self) -> &'static str {
            LANE_PROVIDER_ID
        }

        fn place(&self) -> crate::genome::fine_tuning::adapter::TrainingPlace {
            crate::genome::fine_tuning::adapter::TrainingPlace::EngineLaneHere
        }
    }

    // what this catches (Codex on #5269): create_job reclassifying every lane acquisition failure
    // as InvalidRequest ("never retriable as-is"), so a due bucket whose lane hit memory pressure
    // or a slow start was treated as a bad request. The cause's own type comes through.
    #[tokio::test]
    async fn a_lane_acquisition_failure_keeps_its_type_through_create_job() {
        let train = tempfile::tempdir().expect("test: dir");
        let jobs = tempfile::tempdir().expect("test: dir");
        let mut t = EngineLoraFineTuner::for_test("http://127.0.0.1:1".into(), train.path().to_path_buf(), jobs.path().join("footprints.json"));
        t.lane = Box::new(RefusingLane(|| FineTuningError::Transient("memory pressure".into())));
        let mut r = request("ggml-org/Qwen3.8-27B-GGUF");
        r.local_artifact_dir = Some(jobs.path().to_path_buf());
        assert!(matches!(t.create_job(r.clone()).await, Err(FineTuningError::Transient(_))), "pressure is retried");
        t.lane = Box::new(RefusingLane(|| FineTuningError::InvalidRequest("no such model".into())));
        assert!(matches!(t.create_job(r).await, Err(FineTuningError::InvalidRequest(_))), "a bad request is not");
    }

    pub(super) fn request(base: &str) -> TrainingJobRequest {
        TrainingJobRequest {
            persona_id: Uuid::nil(),
            persona_name: "Kimi".into(),
            base_model: base.into(),
            trait_kind: "code".into(),
            dataset: TrainingDataset {
                examples: vec![TrainingExample { prompt: "p".into(), completion: "c".into(), metadata: None, lived: None }],
                source: TrainingSource::OperatorCurated,
                validation_split: 0.0,
            },
            eval_set: None,
            lora: Some(LoRAHyperparams { rank: 8, alpha: 16, dropout: 0.0, target_modules: vec!["q_proj".into(), "v_proj".into()], top_layers: None }),
            schedule: Some(ScheduleParams { epochs: 2, batch_size: 1, sequence_length: 256, learning_rate: 1e-5 }),
            local_artifact_dir: None,
            resume_from: None,
            parent: None,
        }
    }

    /// A lane that answers /train like the engine: a run goes starting -> running (batches) ->
    /// done (writing `<out>` into `dir`), or -> cancelled after POST /train/cancel.
    pub(super) async fn fake_lane(dir: PathBuf, mode: &'static str) -> (String, tokio::task::JoinHandle<()>, Arc<Mutex<Option<Value>>>) {
        use axum::routing::{get, post};
        #[derive(Default)]
        struct Lane {
            out: Option<String>,
            polls: u32,
            cancelled: bool,
            body: Option<Value>,
            pause_requested: bool,
            pauses_seen: u32,
        }
        let lane = Arc::new(Mutex::new(Lane::default()));
        let seen = Arc::new(Mutex::new(None));
        let (l1, l2, l3, seen1) = (lane.clone(), lane.clone(), lane.clone(), seen.clone());
        let (l4, l5, l6) = (lane.clone(), lane.clone(), lane.clone());
        let app = axum::Router::new()
            // fork #51's measure: each example is as long as the window it is measured at (so the
            // planned window is the served one, as before the partition) and trains in any window
            // once its history is dropped (tokens_min 64), except under "short_turns" (each 3,000
            // whole), "park_odd" (odd prompts fit no window) and "park_all" (none fits); under
            // "loading" the engine is still loading its model and answers 503, as llama-server does
            .route("/train/measure", post(move |axum::Json(b): axum::Json<Value>| async move {
                use axum::response::IntoResponse;
                if mode == "loading" {
                    return (axum::http::StatusCode::SERVICE_UNAVAILABLE, axum::Json(json!({"error": {
                        "code": 503, "message": "Loading model", "type": "unavailable_error"}}))).into_response();
                }
                let examples = b.get("examples").and_then(Value::as_array).cloned().unwrap_or_default();
                let at = b["window"].as_u64().unwrap_or(256);
                let rows: Vec<Value> = examples
                    .iter()
                    .enumerate()
                    .map(|(i, e)| {
                        // odd by the example's own prompt ("p1"), not its place in the batch
                        let odd = e["prompt"].as_str().and_then(|p| p.chars().last()).and_then(|c| c.to_digit(10)).is_some_and(|d| d % 2 == 1);
                        let skipped = mode == "park_all" || (mode == "park_odd" && odd);
                        // "tiny_turns": 2,000 whole (the useful window is the floor); "truncatable_turns":
                        // 3,000 whole but 64 once its history is dropped (whole and min apart)
                        let (full, min) = match mode {
                            "short_turns" => (3_000, 3_000),
                            // "mixed_turns": p1 is 6,000 whole (64 once cut), the others 3,000 whole
                            "mixed_turns" if odd => (6_000, 64),
                            "mixed_turns" => (3_000, 3_000),
                            "tiny_turns" => (2_000, 2_000),
                            "truncatable_turns" => (3_000, 64),
                            _ => (at, 64),
                        };
                        let (full, min, fit) = if skipped { (at + 40_000, at + 40_000, "skipped") } else { (full, min, "kept") };
                        json!({"index": i, "tokens_full": full, "tokens_min": min, "tokens_trained": if skipped { 0 } else { full }, "fit": fit})
                    })
                    .collect();
                axum::Json(json!({"ok": true, "window": b["window"], "examples": rows})).into_response()
            }))
            .route("/train", post(move |axum::Json(b): axum::Json<Value>| {
                let lane = l1.clone();
                let seen = seen1.clone();
                async move {
                    use axum::response::IntoResponse;
                    // "post_400": the engine refuses the start before accepting it, and keeps
                    // naming its PRIOR run (card 26df, the 5090)
                    if mode == "post_400" {
                        return (axum::http::StatusCode::BAD_REQUEST, axum::Json(json!({"error": {
                            "code": 400, "message": "a training run is already in progress", "type": "invalid_request_error"}}))).into_response();
                    }
                    // "post_500": a non-2xx that is NOT the engine's documented refusal
                    if mode == "post_500" {
                        return (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "upstream connect error").into_response();
                    }
                    // "post_stale": the POST is still queued past the caller's wait, and /train
                    // meanwhile names the prior run (it has not claimed this one yet)
                    if mode == "post_stale" {
                        tokio::time::sleep(Duration::from_secs(3)).await;
                        return axum::Json(json!({"ok": true})).into_response();
                    }
                    {
                        let mut l = lane.lock().unwrap();
                        l.out = b.get("out").and_then(Value::as_str).map(str::to_owned);
                        *seen.lock().unwrap() = Some(b.clone());
                        l.body = Some(b);
                    }
                    // "lost_post": the engine takes the run, then its answer outlasts the POST's
                    // wait (a long prepare), so the core must reconcile by the run's id
                    if mode == "lost_post" || mode == "lost_post_refused" {
                        tokio::time::sleep(Duration::from_secs(3)).await;
                    }
                    axum::Json(json!({"ok": true})).into_response()
                }
            }))
            .route("/train", get(move || {
                let lane = l2.clone();
                let dir = dir.clone();
                async move {
                    let mut l = lane.lock().unwrap();
                    l.polls += 1;
                    let out = l.out.clone().unwrap_or_default();
                    if l.cancelled {
                        return axum::Json(json!({"state": "cancelled", "out": out}));
                    }
                    if mode == "post_400" || mode == "post_500" || mode == "post_stale" {
                        return axum::Json(json!({"state": "done", "out": "prior-run.gguf"}));
                    }
                    if mode == "flaky" && l.polls <= 2 {
                        return axum::Json(json!("not a status object"));
                    }
                    if mode == "unknown" {
                        return axum::Json(json!({"state": "paused", "out": out}));
                    }
                    // fork #28: a paused run reports it and does not advance
                    if l.pause_requested {
                        l.polls -= 1;
                        return axum::Json(json!({"state": "running", "out": out, "batch": 1, "batch_max": 4, "epochs": [],
                            "pause_requested": true, "paused": true}));
                    }
                    if mode == "cancel_only" || l.polls < 3 {
                        return axum::Json(json!({"state": "running", "out": out, "batch": 1, "batch_max": 4, "epochs": []}));
                    }
                    // the 5090, 2026-10-10 04:34Z: the device budget refused the chunk graph, and the
                    // engine reports the size its preflight measured, the chunk and the window it ran
                    if mode == "refused" || mode == "lost_post_refused" {
                        let body = l.body.clone().unwrap_or(Value::Null);
                        return axum::Json(json!({"state": "error", "out": out,
                            "error": "the graph needs 9236.1 MiB more on CUDA0, over the 3418.0 MiB it may add: not allocating it",
                            "graph_mib": 12_654.1, "context_mib": 0.0, "chunk": body.get("chunk").cloned().unwrap_or(json!(512)), "window": body.get("window").cloned().unwrap_or(Value::Null)}));
                    }
                    std::fs::write(dir.join(&out), b"GGUF-lora").unwrap();
                    let mut done = json!({"state": "done", "out": out, "trainable_tokens": 40, "adapter": out, "graph_mib": 5.0, "context_mib": 0.0,
                        "epochs": [{"epoch": 0, "train_loss": 2.5, "eval_loss": 2.6}, {"epoch": 1, "train_loss": 2.1, "eval_loss": 2.4}]});
                    // an engine with fork #27 reports the depth it adapted; one without says nothing
                    if mode == "depth" {
                        let asked = l.body.as_ref().and_then(|b| b.get("top_layers")).and_then(Value::as_u64).unwrap_or(64);
                        done["n_layer"] = json!(64);
                        done["layers_adapted"] = json!(asked.min(64));
                    }
                    // fork #30: the engine sized its graph to the data and reports the window it used
                    if mode == "sized" {
                        done["window"] = json!(15_360);
                        done["window_asked"] = l.body.as_ref().and_then(|b| b.get("window")).cloned().unwrap_or(Value::Null);
                    }
                    axum::Json(done)
                }
            }))
            .route("/train/cancel", post(move || {
                let lane = l3.clone();
                async move {
                    lane.lock().unwrap().cancelled = true;
                    axum::Json(json!({"ok": true}))
                }
            }))
            .route("/train/pause", post(move |axum::Json(b): axum::Json<Value>| {
                let lane = l4.clone();
                async move {
                    let mut l = lane.lock().unwrap();
                    assert_eq!(b["out"].as_str(), l.out.as_deref(), "pause names this job");
                    l.pause_requested = true;
                    l.pauses_seen += 1;
                    axum::Json(json!({"ok": true, "pause_requested": true, "paused": false}))
                }
            }))
            .route("/test/pause", get(move || {
                // the test's view of the pause, without advancing the run as GET /train does
                let lane = l6.clone();
                async move {
                    let l = lane.lock().unwrap();
                    axum::Json(json!({"pause_requested": l.pause_requested, "pauses_seen": l.pauses_seen}))
                }
            }))
            .route("/train/resume", post(move |axum::Json(b): axum::Json<Value>| {
                let lane = l5.clone();
                async move {
                    let mut l = lane.lock().unwrap();
                    assert_eq!(b["out"].as_str(), l.out.as_deref(), "resume names this job");
                    l.pause_requested = false;
                    axum::Json(json!({"ok": true, "pause_requested": false, "paused": true}))
                }
            }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("test: bind");
        let url = format!("http://{}", listener.local_addr().expect("test: addr"));
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("test: serve");
        });
        (url, server, seen)
    }

    pub(super) async fn wait_terminal(t: &EngineLoraFineTuner, h: &JobHandle) -> TrainingStatus {
        tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                let s = t.poll(h).await.expect("test: poll");
                if matches!(s, TrainingStatus::Completed { .. } | TrainingStatus::Failed { .. } | TrainingStatus::Cancelled) {
                    return s;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("test: the job never ended")
    }

    // what this catches: a lived call reaching /train as anything but the conversation she
    // was served (card ad107e18): her reasoning and tool calls must stay on the trained reply,
    // her earlier replies in the history must be context and not trained again, and the tools
    // she was offered must ride along so the prompt renders with its tool block.
    #[test]
    pub(super) fn a_lived_call_is_sent_as_its_served_conversation_with_only_her_reply_trained() {
        use crate::ai::types::{ChatMessage, ContentPart, MessageContent, NativeToolSpec, TextGenerationRequest, TextGenerationResponse};
        let request = TextGenerationRequest {
            system_prompt: Some("you are Kimi".into()),
            messages: vec![
                ChatMessage::text("user", "the build is red"),
                ChatMessage {
                    role: "assistant".into(),
                    content: MessageContent::Parts(vec![ContentPart::ToolUse {
                        id: "t0".into(),
                        name: "code/run".into(),
                        input: json!({"cmd": "cargo test"}),
                    }]),
                    name: None,
                },
                ChatMessage {
                    role: "user".into(),
                    content: MessageContent::Parts(vec![ContentPart::ToolResult {
                        tool_use_id: "t0".into(),
                        content: "1 failed".into(),
                        is_error: None,
                    }]),
                    name: None,
                },
                ChatMessage::text("user", "card: fix it"),
            ],
            tools: Some(vec![serde_json::from_value::<NativeToolSpec>(json!({
                "name": "code/read", "description": "read a file",
                "input_schema": {"type": "object", "properties": {}}
            }))
            .expect("test: tool spec")]),
            ..Default::default()
        };
        let response = TextGenerationResponse {
            text: "reading it".into(),
            finish_reason: crate::ai::FinishReason::ToolUse,
            model: "m".into(),
            provider: "p".into(),
            usage: crate::ai::UsageMetrics::default(),
            response_time_ms: 0,
            request_id: "r".into(),
            content: None,
            tool_calls: Some(vec![crate::ai::ToolCall { id: "t1".into(), name: "code/read".into(), input: json!({"path": "a.rs"}) }]),
            reasoning: Some("read before guessing".into()),
            routing: None,
            error: None,
            timing: None,
        };
        let lived = TrainingExample {
            prompt: "card: fix it".into(),
            completion: "reading it".into(),
            metadata: None,
            lived: Some(super::super::LivedCall { capture: "c".into(), request, response }),
        };
        let e = serde_json::to_value(engine_example(&lived)).expect("test: wire");
        let m = e["messages"].as_array().expect("test: messages");
        assert_eq!(m.len(), 6);
        assert_eq!((m[0]["role"].as_str(), m[0]["content"].as_str()), (Some("system"), Some("you are Kimi")));
        // the history is the one serving sent: her earlier act keeps its tool call, and its
        // result is a tool message bound to that call (Cormac on #4445)
        assert_eq!(m[2]["tool_calls"][0]["function"]["name"], "code/run");
        assert_eq!(m[2]["train"], json!(false), "her earlier act is context, not this lesson");
        assert_eq!((m[3]["role"].as_str(), m[3]["tool_call_id"].as_str()), (Some("tool"), Some("t0")));
        let reply = &m[5];
        assert!(reply.get("train").is_none(), "the reply is trained");
        assert_eq!(reply["reasoning_content"], "read before guessing");
        assert_eq!(reply["tool_calls"][0]["function"]["name"], "code/read");
        assert_eq!(reply["tool_calls"][0]["function"]["arguments"], "{\"path\":\"a.rs\"}");
        assert_eq!(e["tools"][0]["function"]["name"], "code/read");
        // a self-tick: the history ends in her own turn, and serving closed it before her reply
        let mut tick = lived.clone();
        let call = tick.lived.as_mut().expect("test: lived");
        call.request.messages = vec![ChatMessage::text("user", "go"), ChatMessage::text("assistant", "thinking it over")];
        let e = serde_json::to_value(engine_example(&tick)).expect("test: wire");
        let m = e["messages"].as_array().expect("test: messages");
        let roles: Vec<&str> = m.iter().filter_map(|x| x["role"].as_str()).collect();
        assert!(
            !roles.windows(2).any(|w| w == ["assistant", "assistant"]),
            "never two assistant turns in a row, as serving never sends them: {roles:?}"
        );
        let plain = TrainingExample { prompt: "p".into(), completion: "c".into(), metadata: None, lived: None };
        assert_eq!(serde_json::to_value(engine_example(&plain)).expect("test: wire"), json!({"prompt": "p", "completion": "c"}));
    }

    // what this catches (Joel, 2026-09-28; Cormac on #4498): a training window set by the
    // REQUEST instead of the lane. Her lane serves 61,696 tokens a slot; the request carries a
    // plan's 1024; the run trains at 61,696, from the same record that chose the lane. A lane
    // record with no served window is refused, never trained at a guessed one.
    #[tokio::test]
    pub(super) async fn the_training_window_is_the_lanes_served_window_never_the_requests() {
        let train = tempfile::tempdir().expect("test: dir");
        let jobs = tempfile::tempdir().expect("test: dir");
        let (url, server, seen) = fake_lane(train.path().to_path_buf(), "normal").await;
        let mut t = EngineLoraFineTuner::for_test(url.clone(), train.path().to_path_buf(), jobs.path().join("footprints.json"));
        let served = url.clone();
        t.lane = Box::new(move |_: &str| Some(LaneChoice::at(served.clone(), 61_696)));
        let mut r = request("ggml-org/Qwen3.8-27B-GGUF");
        r.local_artifact_dir = Some(jobs.path().to_path_buf());
        r.schedule.as_mut().expect("test: schedule").sequence_length = 1024;
        let h = t.create_job(r).await.expect("test: create");
        let _ = wait_terminal(&t, &h).await;
        let body = seen.lock().unwrap().clone().expect("test: /train was posted");
        assert_eq!(body["window"].as_u64(), Some(61_696), "the lane's served window, not the request's 1024");

        for (served, why) in [(0, "an unknown served window is refused, never guessed"), (200, "a window under 256 is refused, never rounded up past serving")] {
            let lane_url = url.clone();
            t.lane = Box::new(move |_: &str| Some(LaneChoice::at(lane_url.clone(), served)));
            let mut r = request("ggml-org/Qwen3.8-27B-GGUF");
            r.local_artifact_dir = Some(jobs.path().to_path_buf());
            assert!(t.create_job(r).await.is_err(), "{why}");
        }
        server.abort();
    }

    // what this catches (card 04679d57, gate 3): an in-engine gene the examiner can never judge.
    // It proves what is unseen from the job's request.json (`read_job_request`), which this
    // trainer never wrote: both of Kimi's exam receipts on the windows 5090 (2026-10-24) are
    // "unexaminable: no training job recorded". The job now records the request as it arrived,
    // before its run, readable by the examiner's own reader, with the provider's defaults never
    // written as if they were the producer's.
    #[tokio::test]
    pub(super) async fn an_in_engine_job_records_the_request_it_trains_from_before_its_run() {
        let train = tempfile::tempdir().expect("test: dir");
        let jobs = tempfile::tempdir().expect("test: dir");
        let (url, server, _seen) = fake_lane(train.path().to_path_buf(), "refused").await;
        let mut t = EngineLoraFineTuner::for_test_with_vram(url.clone(), train.path().to_path_buf(), jobs.path().join("fp.json"), 6 * 1024 * 1024 * 1024);
        t.lane = Box::new(move |_: &str| Some(LaneChoice::at(url.clone(), 1536)));
        let mut r = request("ggml-org/Qwen3.8-27B-GGUF");
        r.local_artifact_dir = Some(jobs.path().to_path_buf());
        r.schedule = None; // the producer named none: the provider's default must not be recorded as hers
        let h = t.create_job(r.clone()).await.expect("test: create");
        let recorded = crate::genome::fine_tuning::job_dir::read_job_request(&jobs.path().join(h.local_id.to_string()))
            .expect("test: the job's request is on disk, readable by the examiner's reader");
        assert_eq!(recorded.dataset.examples.len(), r.dataset.examples.len(), "what the gene trains from");
        assert_eq!(recorded.dataset.examples[0].prompt, "p");
        assert!(recorded.schedule.is_none(), "the request as it arrived, before the provider's default schedule");
        let _ = wait_terminal(&t, &h).await;
        server.abort();
    }

    // what this catches (card ea157b1a, gate 3): a lived example parked because her served
    // window cannot train it whole, with nothing telling serving what her learning needs (Kimi's
    // reviewed correction, 51,670-64,041 tokens, parked for good at 49,664). The partition records
    // the window that trains it whole for the serving requirement, and prices it only from a
    // footprint of that very shape: none yet, so the reservation is unmeasured.
    #[tokio::test]
    pub(super) async fn a_parked_example_records_the_window_it_needs_for_serving() {
        let train = tempfile::tempdir().expect("test: dir");
        let jobs = tempfile::tempdir().expect("test: dir");
        let (url, server, _seen) = fake_lane(train.path().to_path_buf(), "park_all").await;
        let mut t = EngineLoraFineTuner::for_test_with_vram(url.clone(), train.path().to_path_buf(), jobs.path().join("fp.json"), 6 * 1024 * 1024 * 1024);
        t.lane = Box::new(move |_: &str| Some(LaneChoice::at(url.clone(), 1536)));
        t.demand_store = Some(jobs.path().join("training-demand"));
        let mut r = request("ggml-org/Qwen3.8-27B-GGUF");
        r.persona_id = Uuid::new_v4();
        r.local_artifact_dir = Some(jobs.path().to_path_buf());
        // every example parks (the engine measures each 40,000 tokens past the window): the job
        // ends without a run, and its demand is recorded all the same
        let h = t.create_job(r.clone()).await.expect("test: create");
        let demand = crate::genome::fine_tuning::training_demand::TrainingDemand::in_dir(t.demand_store.as_deref(), r.persona_id);
        assert_eq!(demand.outstanding(), Some(41_728), "her 41,536-token example needs a 41,728 window; her lane serves 1,536");
        assert_eq!(demand.waiting(), r.dataset.examples.len());
        let reserved = demand.reservation().expect("a parked example reserves");
        assert_eq!((reserved.window, reserved.recorded_price(), reserved.job), (41_728, None, Some(h.local_id)), "no footprint of that shape yet: the reservation is unpriced, never priced from another window");
        server.abort();
    }

    // what this catches (BigMama on #4894; the 5090, 2026-10-10 04:34Z): a refused calibration
    // filing nothing, so every later dispatch calibrates at the same chunk and is refused the
    // same way. The refusal files the graph the engine measured under the chunk it ran; the
    // next dispatch of the shape chooses the next chunk down against what is free.
    #[tokio::test]
    pub(super) async fn a_refused_run_files_its_graph_and_the_next_dispatch_steps_the_chunk_down() {
        let train = tempfile::tempdir().expect("test: dir");
        let jobs = tempfile::tempdir().expect("test: dir");
        let (url, server, seen) = fake_lane(train.path().to_path_buf(), "refused").await;
        let footprints = jobs.path().join("footprints.json");
        let gib = 1024u64 * 1024 * 1024;
        let mut t = EngineLoraFineTuner::for_test_with_vram(url.clone(), train.path().to_path_buf(), footprints.clone(), 6 * gib);
        t.lane = Box::new(move |_: &str| Some(LaneChoice::at(url.clone(), 1536)));
        let mut r = request("ggml-org/Qwen3.8-27B-GGUF");
        r.local_artifact_dir = Some(jobs.path().to_path_buf());
        let h = t.create_job(r.clone()).await.expect("test: create");
        let TrainingStatus::Failed { error } = wait_terminal(&t, &h).await else {
            panic!("test: the refused run must fail");
        };
        assert!(error.contains("it may add"), "the engine's refusal, by name: {error}");
        assert_eq!(seen.lock().unwrap().clone().expect("test: posted")["chunk"].as_u64(), Some(512), "the first dispatch calibrates at the engine's default chunk");
        let rows: Value = serde_json::from_slice(&std::fs::read(&footprints).expect("test: the refusal filed a footprint")).unwrap();
        let (key, row) = rows.as_object().unwrap().iter().next().expect("test: one row");
        assert_eq!(key, "ggml-org/Qwen3.8-27B-GGUF|w1536|r8|attn_q,attn_v|rc|refused", "filed under the shape and the chunk that ran (512 is 1536's default: no chunk suffix), with recompute, as a refusal beside any run (card 0d21e644)");
        assert_eq!(row["bytes"].as_u64(), Some((12_654.1f64 * 1024.0 * 1024.0) as u64), "the graph the preflight measured");
        let h2 = t.create_job(r).await.expect("test: create again");
        let _ = wait_terminal(&t, &h2).await;
        assert_eq!(seen.lock().unwrap().clone().expect("test: posted")["chunk"].as_u64(), Some(256), "12.4 GB at 512 does not fit 6 GB free: the next dispatch asks for 256");
        server.abort();
    }

    // what this catches (Cormac and Astra on #4920; the 5090 2026-10-10, job 00431d4d): the WHOLE
    // planner, not its parts. With two windows of the family measured and a lane serving more
    // than the graph can hold, the dispatch POSTS a calculated window under served (her examples
    // need far less than served, and only whole examples are sent: Codex on #4988) AND a chunk
    // whose modelled graph fits the same grant (a window sized for the floor chunk sent at the
    // default 512 refuses by construction).
    #[tokio::test]
    pub(super) async fn a_dispatch_the_served_window_cannot_hold_posts_a_window_and_chunk_that_fit() {
        let train = tempfile::tempdir().expect("test: dir");
        let jobs = tempfile::tempdir().expect("test: dir");
        let (url, server, seen) = fake_lane(train.path().to_path_buf(), "short_turns").await;
        let footprints = jobs.path().join("footprints.json");
        let mib = 1024u64 * 1024;
        let fam = Shape { model: "ggml-org/Qwen3.8-27B-GGUF".into(), window: 1536, rank: 8, targets: "attn_q,attn_v".into(), depth: None, exact: false, chunk: 256, recompute: true };
        let store = Footprints::at(footprints.clone());
        store.record_with(&fam, 2598 * mib, true, None, Uuid::nil()).expect("test: row");
        store.record_with(&Shape { window: 68_608, chunk: 512, ..fam.clone() }, 10_225 * mib, true, None, Uuid::nil()).expect("test: row");
        let model = GraphModel::fit(&store.rows_like(&fam)).expect("test: two windows");
        // her examples are 3,000 tokens whole: the window they need (3,072) is far under served, and
        // the grant holds it at the floor chunk but not at the served window
        let free = model.bytes(3_072, TRAINING_CHUNK_MIN);
        let mut t = EngineLoraFineTuner::for_test_with_vram(url.clone(), train.path().to_path_buf(), footprints.clone(), free);
        t.lane = Box::new(move |_: &str| Some(LaneChoice::at(url.clone(), 68_608)));
        let mut r = request("ggml-org/Qwen3.8-27B-GGUF");
        r.local_artifact_dir = Some(jobs.path().to_path_buf());
        let h = t.create_job(r).await.expect("test: create");
        let _ = wait_terminal(&t, &h).await;
        let posted = seen.lock().unwrap().clone().expect("test: posted");
        let (w, c) = (posted["window"].as_u64().expect("test: window") as u32, posted["chunk"].as_u64().expect("test: chunk") as u32);
        assert!(w < 68_608 && w % 256 == 0, "a calculated window under served: {w}");
        assert!(model.bytes(w, c) <= free, "the POSTED window and chunk fit the grant together: w{w} c{c} = {} MiB of {} MiB", model.bytes(w, c) / mib, free / mib);
        server.abort();
    }

    // what this catches (card de9e1342; the 5090, 84 of 108 jobs Failed): memory planning a
    // window none of her examples trains in, so the run ends Failed with nothing to return and
    // waits for a human. Each example here needs 3,072 tokens; a graph-only refusal at that
    // window's floor chunk is over what is free, so the plan's window drops below it. The job is
    // not created: it is refused as parked, carrying the window her examples need, the measured
    // lower bound and the free memory, and no run is posted.
    #[tokio::test]
    pub(super) async fn a_job_memory_cannot_give_a_useful_window_is_parked_with_its_wake_key() {
        let train = tempfile::tempdir().expect("test: dir");
        let jobs = tempfile::tempdir().expect("test: dir");
        let (url, server, seen) = fake_lane(train.path().to_path_buf(), "short_turns").await;
        let footprints = jobs.path().join("footprints.json");
        let gib = 1024u64 * 1024 * 1024;
        let fam = Shape { model: "ggml-org/Qwen3.8-27B-GGUF".into(), window: 3072, rank: 8, targets: "attn_q,attn_v".into(), depth: None, exact: false, chunk: TRAINING_CHUNK_MIN, recompute: true };
        Footprints::at(footprints.clone()).record_with(&fam, 2 * gib, false, None, Uuid::nil()).expect("test: graph-only refusal");
        let mut t = EngineLoraFineTuner::for_test_with_vram(url.clone(), train.path().to_path_buf(), footprints, gib);
        t.lane = Box::new(move |_: &str| Some(LaneChoice::at(url.clone(), 61_696)));
        let mut r = request("ggml-org/Qwen3.8-27B-GGUF");
        r.local_artifact_dir = Some(jobs.path().to_path_buf());
        match t.create_job(r).await {
            Err(FineTuningError::CapacityParked(wait)) => {
                assert_eq!((wait.window, wait.need_bytes, wait.governed_free), (3_072, 2 * gib, gib), "{wait:?}");
                assert_eq!(wait.need_source, NeedSource::LowerBound);
                assert!(!wait.wakes(gib, wait.engine_stamp.as_deref()), "the same free memory never wakes it");
                assert!(wait.wakes(3 * gib, wait.engine_stamp.as_deref()), "free past the bound makes it eligible to re-plan");
                assert!(!wait.wakes(gib, None), "no lane up: an unknown build wakes nothing");
                assert!(wait.wakes(gib, Some("another:cuda")), "a known other engine does");
            }
            other => panic!("expected a capacity park, got {other:?}"),
        }
        assert!(seen.lock().unwrap().is_none(), "no run was posted");
        server.abort();
    }

    // what this catches (card 0d21e644; Kimi's job 598e183c on the windows 5090, 2026-10-28,
    // refused at window 45,056): the same measured no-fit as above, but the refusal's engine
    // reported its 64 blocks. Before, the only rung left was a smaller window, which parks her
    // longest turns. Now the dispatch steps DEPTH first: it posts the window that trains every
    // example whole at half the blocks, an unmeasured shape whose run is the measurement.
    #[tokio::test]
    pub(super) async fn a_window_no_chunk_holds_is_trained_whole_at_a_shallower_depth_first() {
        let train = tempfile::tempdir().expect("test: dir");
        let jobs = tempfile::tempdir().expect("test: dir");
        let (url, server, seen) = fake_lane(train.path().to_path_buf(), "short_turns").await;
        let footprints = jobs.path().join("footprints.json");
        let gib = 1024u64 * 1024 * 1024;
        let fam = Shape { model: "ggml-org/Qwen3.8-27B-GGUF".into(), window: 3072, rank: 8, targets: "attn_q,attn_v".into(), depth: None, exact: false, chunk: TRAINING_CHUNK_MIN, recompute: true };
        Footprints::at(footprints.clone()).record_refusal(&fam, 2 * gib, Some(64), Uuid::nil()).expect("test: a refusal at every block");
        let mut t = EngineLoraFineTuner::for_test_with_vram(url.clone(), train.path().to_path_buf(), footprints, gib);
        t.lane = Box::new(move |_: &str| Some(LaneChoice::at(url.clone(), 61_696)));
        let mut r = request("ggml-org/Qwen3.8-27B-GGUF");
        r.local_artifact_dir = Some(jobs.path().to_path_buf());
        let h = t.create_job(r).await.expect("test: a job, not a park");
        let _ = wait_terminal(&t, &h).await;
        let posted = seen.lock().unwrap().clone().expect("test: posted");
        assert_eq!(posted["window"].as_u64(), Some(3_072), "the window that trains every example whole");
        assert_eq!(posted["top_layers"].as_u64(), Some(32), "at half the blocks, the first rung down");
        server.abort();
    }

    // what this catches (Codex on #4988): an example sent CUT through the middle fit. p1 is 6,000
    // tokens whole, the others 3,000. Whatever window memory allows between them (raised to the
    // shortest whole example, equal to it, or between it and the longest), p1 would train with its
    // history dropped, so it is parked and only p0 and p2 are posted, whole.
    #[tokio::test]
    pub(super) async fn only_whole_examples_are_sent_at_every_window_memory_allows() {
        let mib = 1024u64 * 1024;
        let fam = Shape { model: "ggml-org/Qwen3.8-27B-GGUF".into(), window: 1536, rank: 8, targets: "attn_q,attn_v".into(), depth: None, exact: false, chunk: 256, recompute: true };
        let probe_dir = tempfile::tempdir().expect("test: dir");
        let probe = Footprints::at(probe_dir.path().join("fp.json"));
        probe.record_with(&fam, 2598 * mib, true, None, Uuid::nil()).expect("test: row");
        probe.record_with(&Shape { window: 68_608, chunk: 512, ..fam.clone() }, 10_225 * mib, true, None, Uuid::nil()).expect("test: row");
        let model = GraphModel::fit(&probe.rows_like(&fam)).expect("test: two windows");
        for (case, free) in [
            ("raised", model.bytes(3_072, TRAINING_CHUNK_MIN) - 1),
            ("equal", model.bytes(3_072, TRAINING_CHUNK_MIN)),
            ("between", model.bytes(4_608, TRAINING_CHUNK_MIN)),
        ] {
            let train = tempfile::tempdir().expect("test: dir");
            let jobs = tempfile::tempdir().expect("test: dir");
            let (url, server, seen) = fake_lane(train.path().to_path_buf(), "mixed_turns").await;
            let footprints = jobs.path().join("footprints.json");
            std::fs::copy(probe_dir.path().join("fp.json"), &footprints).expect("test: rows");
            let mut t = EngineLoraFineTuner::for_test_with_vram(url.clone(), train.path().to_path_buf(), footprints, free);
            t.lane = Box::new(move |_: &str| Some(LaneChoice::at(url.clone(), 61_696)));
            let mut r = request("ggml-org/Qwen3.8-27B-GGUF");
            r.local_artifact_dir = Some(jobs.path().to_path_buf());
            r.dataset.examples = (0..3)
                .map(|i| TrainingExample { prompt: format!("p{i}"), completion: "c".into(), metadata: None, lived: None })
                .collect();
            let h = t.create_job(r).await.expect("test: a prediction calibrates, never parks");
            let _ = wait_terminal(&t, &h).await;
            let posted = seen.lock().unwrap().clone().expect("test: posted");
            let window = posted["window"].as_u64().expect("test: window");
            assert!((3_072..6_144).contains(&window), "{case}: a window between the shortest and the longest whole example: {window}");
            let got: Vec<&str> = posted["examples"].as_array().expect("test: examples").iter().map(|e| e["prompt"].as_str().unwrap_or("")).collect();
            assert_eq!(got, vec!["p0", "p2"], "{case}: the 6,000-token example is parked, never sent cut");
            let partition = read_partition(&jobs.path().join(h.local_id.to_string())).expect("test: readable").expect("test: partition");
            assert_eq!(partition.parked, vec![1], "{case}");
            server.abort();
        }
    }

mod part_2;
