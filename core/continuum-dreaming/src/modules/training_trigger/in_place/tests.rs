#![cfg(test)]
use super::*;

fn outcome(committed_steps: i64, effects_uncertain: bool) -> LearnOutcome {
    outcome_with_loss(committed_steps, effects_uncertain, 0.0)
}

fn outcome_with_loss(committed_steps: i64, effects_uncertain: bool, loss: f64) -> LearnOutcome {
    let mut outcome: LearnOutcome = serde_json::from_value(serde_json::json!({
        "success": committed_steps > 0, "committed_steps": committed_steps, "effects_uncertain": effects_uncertain,
        "refusal": "none", "loss": 0.0, "n_labels": 0, "t_ms": 0.0, "max_chunk_ms": 0.0, "restoration_ms": 0.0,
        "entry_ms": 0.0, "max_graph_ms": 0.0, "max_restore_ms": 0.0, "cleanup_ms": 0.0
    }))
    .expect("test: an engine outcome");
    outcome.loss = loss; // JSON has no NaN: a non-finite loss is set on the decoded outcome
    outcome
}

// what this catches: the cursor moving on an example the engine did not commit, or a quarantined
// shadow kept learning. Only a committed update with certain effects counts; uncertainty wins.
#[test]
fn only_a_committed_certain_update_moves_the_cursor() {
    assert_eq!(verdict(&outcome(1, false)), Verdict::Learned);
    assert_eq!(verdict(&outcome(0, false)), Verdict::Again, "refused, deferred, or ended before an update");
    assert_eq!(verdict(&outcome(1, true)), Verdict::Quarantined, "committed, then uncertain: quarantined, never counted");
    assert_eq!(verdict(&outcome(0, true)), Verdict::Quarantined);
    assert_eq!(verdict(&outcome_with_loss(1, false, f64::NAN)), Verdict::Quarantined, "a non-finite loss is a poisoned update, never counted");
    assert_eq!(verdict(&outcome_with_loss(1, false, f64::INFINITY)), Verdict::Quarantined);
}

// what this catches: two buckets whose files share a name in one train dir, so one bucket's sweep
// deletes the other's (Codex on f13a6abea): traits that sanitize alike, one trait on two bases.
#[test]
fn a_buckets_files_are_named_for_its_whole_key() {
    let key = |trait_kind: &str, base: &str| BucketKey { persona_id: Uuid::from_u128(1), trait_kind: trait_kind.into(), base_model: base.into() };
    assert_ne!(bucket_stem(&key("code/owner", "qwen")), bucket_stem(&key("code-owner", "qwen")), "traits that sanitize alike stay apart");
    assert_ne!(bucket_stem(&key("code", "qwen")), bucket_stem(&key("code", "llama")), "one trait on two bases stays apart");
    assert_eq!(bucket_stem(&key("code", "qwen")), bucket_stem(&key("code", "qwen")), "the stem is the same on every call");
}

fn authority() -> Authority {
    let resources = crate::resources::ResourceDaemon::start(
        vec![Arc::new(crate::resources::MockCapacitySource::new(crate::resources::ResourceKind::Vram, 1 << 30))],
        vec![],
        crate::resources::DaemonConfig::default(),
    );
    Authority { resources, serving: crate::inference::serving_lifecycle::LifecycleGate::unowned(true), custody: crate::genome::learner::custody::Custody::isolated() }
}

fn engine(port: u16) -> EngineIncarnation {
    EngineIncarnation::of(std::process::id(), port).expect("test: this process's start time is readable")
}

// what this catches (Codex on B, card 23069702): her learner waiting for her act while holding her
// bucket's gate, so her own submit stalled behind her background work. The learner waits for her
// acts holding nothing of the trigger; the gate stays free, and once her act ends the step is
// admitted (here refused: this engine has no pool) and the pass ends deferred, never failed.
#[tokio::test]
async fn her_waiting_learner_never_holds_her_bucket_gate() {
    let state = Arc::new(TrainingTriggerState::new(authority()));
    let persona = Uuid::new_v4();
    let key = BucketKey { persona_id: persona, trait_kind: "code".into(), base_model: "qwen".into() };
    let example = crate::genome::fine_tuning::types::TrainingExample { prompt: "p".into(), completion: "c".into(), metadata: None, lived: None };
    let row = Arrival { id: Uuid::new_v4(), sequence: 1, key: key.clone(), persona_name: "her".into(), examples: vec![example], learned: 0, lora: None, schedule: None };
    let acting = crate::inference::slots::begin_act_for_test(persona).await;
    let mut pass = Box::pin(state.learn_bucket(key.clone(), vec![row], engine(9)));
    assert!(futures::poll!(&mut pass).is_pending(), "her act comes first: the learner waits");
    assert!(state.submit_gates.try_acquire(&key).is_some(), "her bucket's gate is free while her learner waits");
    drop(acting);
    let pass = tokio::time::timeout(std::time::Duration::from_secs(5), pass)
        .await
        .expect("test: the pass ends once her act does")
        .expect("test: deferred, not failed");
    assert_eq!(pass, BucketPass { deferred: true, ..Default::default() });
}

// what this catches (Codex on B): one mind's pass waiting (for her act, her slot, an engine) holding
// every other bucket's pass behind it. Passes run together; a bucket that never finishes does not
// stop another from running and finishing.
#[tokio::test]
async fn a_waiting_bucket_never_holds_another_buckets_pass() {
    let state = Arc::new(TrainingTriggerState::new(authority()));
    let (waiting, other) = (
        BucketKey { persona_id: Uuid::new_v4(), trait_kind: "code".into(), base_model: "qwen".into() },
        BucketKey { persona_id: Uuid::new_v4(), trait_kind: "code".into(), base_model: "qwen".into() },
    );
    let finished = Arc::new(tokio::sync::Notify::new());
    let seen = Arc::clone(&finished);
    let stuck = waiting.clone();
    let passes = state.learn_each(vec![(waiting, vec![]), (other, vec![])], move |_state, key, _rows| {
        let (seen, stuck) = (Arc::clone(&seen), stuck.clone());
        async move {
            if key == stuck {
                std::future::pending::<()>().await;
            }
            seen.notify_one();
            Ok(BucketPass { learned: 1, ..Default::default() })
        }
    });
    tokio::pin!(passes);
    tokio::select! {
        _ = finished.notified() => {}
        _ = &mut passes => panic!("the waiting bucket's pass never ends"),
        _ = tokio::time::sleep(std::time::Duration::from_secs(5)) => panic!("the other bucket was held behind the waiting one"),
    }
}

// what this catches: a restored learner re-teaching examples its generation already holds, when the
// journal row was read before the generation's positions were projected onto it (Codex on 76f829e2b).
#[test]
fn a_step_starts_past_the_generation_even_on_a_stale_journal_row() {
    assert_eq!(start(0, Some(3), None), 3, "stale journal row, generation holds three");
    assert_eq!(start(3, Some(3), Some(5)), 5, "volatile progress past the generation");
    assert_eq!(start(4, None, None), 4, "no generation names it: the journal");
}

// what this catches: two of her buckets on one base each minting a gene for one competence (Codex
// on e473f72b1). Her other same-base learner is in flight for the decision; this bucket, another
// persona's and another base's are not, and a learner without a signature cannot be compared.
#[test]
fn her_other_learners_on_this_base_are_in_flight() {
    let her = Uuid::new_v4();
    let key = |persona: Uuid, trait_kind: &str, base: &str| BucketKey { persona_id: persona, trait_kind: trait_kind.into(), base_model: base.into() };
    let this = key(her, "code", "qwen");
    let (sibling, unsigned, other_base, other_persona) = (key(her, "code/owner", "qwen"), key(her, "review", "qwen"), key(her, "code", "llama"), key(Uuid::new_v4(), "code", "qwen"));
    let ids: Vec<Uuid> = (0..5).map(|_| Uuid::new_v4()).collect();
    let seats = vec![(&this, ids[0], Some(&1u8)), (&sibling, ids[1], Some(&2u8)), (&unsigned, ids[2], None), (&other_base, ids[3], Some(&4u8)), (&other_persona, ids[4], Some(&5u8))];
    assert_eq!(in_flight(&this, seats.into_iter()), vec![(ids[1], 2u8)]);
}

// what this catches: a candidate gene per checkpoint (a trial per example, her cards split across
// them), or none ever. One is due once a competence's worth is learned since the last.
#[test]
fn a_candidate_is_due_once_a_competence_is_learned_since_the_last() {
    let competence = crate::genome::competence::MIN_EXAMPLES as u32;
    assert!(!snapshot::due(competence - 1, 0), "memories, not yet a competence");
    assert!(snapshot::due(competence, 0));
    assert!(!snapshot::due(competence + 3, competence), "three since the last candidate");
    assert!(snapshot::due(2 * competence, competence));
}

// what this catches: one bucket's persistent failure (a checkpoint the engine answers with no
// optimizer state, say) ending the whole pass, so every later bucket and the candidate sweep never
// ran again (Codex on 086d60123). The first bucket fails; the second still runs, through the real
// loop and its owned gates.
#[tokio::test]
async fn a_failing_bucket_never_starves_the_next() {
    let (trigger, _executor, _dir) = crate::commands::training_trigger::test_support::build_runtime_trigger_only().await;
    let state = trigger.state.clone();
    let (failing, healthy) = (
        BucketKey { persona_id: Uuid::new_v4(), trait_kind: "code".into(), base_model: "qwen".into() },
        BucketKey { persona_id: Uuid::new_v4(), trait_kind: "code".into(), base_model: "qwen".into() },
    );
    let ran = Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen = Arc::clone(&ran);
    let refused = failing.clone();
    state
        .learn_each(vec![(failing.clone(), vec![]), (healthy.clone(), vec![])], move |_state, key, _rows| {
            let (seen, refused) = (Arc::clone(&seen), refused.clone());
            async move {
                seen.lock().expect("test: lock").push(key.clone());
                if key == refused {
                    Err("the engine holds no optimizer state for her shadow".to_string())
                } else {
                    Ok(BucketPass { learned: 1, ..Default::default() })
                }
            }
        })
        .await
        .expect("test: a bucket's own failure does not fail the pass");
    assert_eq!(*ran.lock().expect("test: lock"), vec![failing, healthy], "the healthy bucket after the failing one still ran");
}

// what this catches: an eviction that deletes a file it does not own (Codex on f13a6abea): the same
// persona and trait learning on two bases in one train dir, a shadow, or a staged gene; and one that
// keeps the bucket's superseded generations forever.
#[test]
fn eviction_deletes_only_this_buckets_superseded_generations() {
    let dir = tempfile::tempdir().expect("test: dir");
    let key = |base: &str| BucketKey { persona_id: Uuid::from_u128(1), trait_kind: "code".into(), base_model: base.into() };
    let (mine, other) = (bucket_stem(&key("qwen")), bucket_stem(&key("llama")));
    let (old, newest, others_newest, seat) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    let generation = |stem: &str, id: Uuid| format!("{stem}-{}.gguf.checkpoint", id.simple());
    for name in [generation(&mine, old), generation(&mine, newest), generation(&other, others_newest), format!("{mine}-{}.gguf.pending", seat.simple())] {
        std::fs::create_dir(dir.path().join(name)).expect("test: dir");
    }
    for name in [format!("{mine}-{}.gguf", seat.simple()), format!("gene-{}.gguf", "ab".repeat(32))] {
        std::fs::write(dir.path().join(name), b"kept").expect("test: file");
    }
    assert_eq!(evict::named_id(&mine, &format!("{mine}-{}.gguf", newest.simple())), Some(newest), "a checkpoint row's bare name");
    assert_eq!(evict::sweep(dir.path(), &mine, newest).expect("test: swept"), 1, "only this bucket's superseded generation");
    let mut left: Vec<String> = std::fs::read_dir(dir.path()).expect("test: list").map(|e| e.expect("test: entry").file_name().into_string().expect("test: utf8")).collect();
    left.sort();
    let mut want = vec![
        generation(&mine, newest),
        generation(&other, others_newest),
        format!("{mine}-{}.gguf", seat.simple()),
        format!("{mine}-{}.gguf.pending", seat.simple()),
        format!("gene-{}.gguf", "ab".repeat(32)),
    ];
    want.sort();
    assert_eq!(left, want, "the other base's newest generation, the shadow, its pending dir and the staged gene all stay");
}

/// The trigger learning in place on a LIVE engine, through its real route (card 4114a2c6; Codex on
/// 086d60123 and a8f687b06): arrivals through `genome/training-trigger/submit`, the module's tick
/// waking its recovery pass over the engine registered as the live lane, a
/// confirmed generation, a shutdown that leaves nothing seated, a restart over the same journal
/// that restores the generation, and a later arrival learned on top of it.
///
///   CONTINUUM_LEARN_ENGINE=<pid>:<port> CONTINUUM_LEARN_BASE=<served model id> CONTINUUM_LEARN_SLOTS=<-np> \
///     cargo test -p continuum-dreaming --lib --features live-engine-tests live_engine
#[cfg(feature = "live-engine-tests")]
mod live_engine {
    use super::*;
    use crate::commands::training_trigger::test_support::build_runtime_under;
    use crate::inference::llama_server::{LearningEngine, LedgerAnswer};

    fn named_engine() -> (EngineIncarnation, String) {
        let spec = std::env::var("CONTINUUM_LEARN_ENGINE").expect("live-engine-tests: CONTINUUM_LEARN_ENGINE=<pid>:<port> names the engine");
        let (pid, port) = spec.split_once(':').expect("live-engine-tests: CONTINUUM_LEARN_ENGINE is <pid>:<port>");
        let engine = EngineIncarnation::of(pid.parse().expect("test: pid"), port.parse().expect("test: port")).expect("test: the engine's start time is readable");
        let base = std::env::var("CONTINUUM_LEARN_BASE").expect("live-engine-tests: CONTINUUM_LEARN_BASE names the model the engine serves");
        (engine, base)
    }

    /// Mock capacity for both kinds: a CPU engine holds learning memory in RAM, a GPU one in VRAM.
    fn live_authority() -> Authority {
        let resources = crate::resources::ResourceDaemon::start(
            vec![
                Arc::new(crate::resources::MockCapacitySource::new(crate::resources::ResourceKind::Vram, 64 << 30)),
                Arc::new(crate::resources::MockCapacitySource::new(crate::resources::ResourceKind::Ram, 64 << 30)),
            ],
            vec![],
            crate::resources::DaemonConfig::default(),
        );
        Authority { resources, serving: crate::inference::serving_lifecycle::LifecycleGate::unowned(true), custody: crate::genome::learner::custody::Custody::isolated() }
    }

    async fn submit(executor: &crate::runtime::CommandExecutor, persona: Uuid, base: &str, prompt: &str, completion: &str) {
        let params = serde_json::json!({
            "personaId": persona, "personaName": "live-fixture", "baseModel": base, "traitKind": "geography",
            "examples": [crate::commands::training_trigger::test_support::ex(prompt, completion)], "source": "operator_curated",
        });
        let answer = executor.execute_json("genome/training-trigger/submit", params).await.expect("test: submit routes");
        assert_eq!(answer["success"], true, "test: the arrival is accepted: {answer}");
    }

    /// Name `engine` the live lane serving `base`, where `live_lane` reads it: the record and pidfile
    /// serving's launch writes. Under the fixture's pinned home, never the machine's.
    fn register_live_lane(engine: EngineIncarnation, base: &str, slots: u32) {
        let record = crate::inference::lane_registry::LaneRecord {
            pid: engine.pid,
            port: engine.port,
            role: crate::inference::lane_registry::LaneRole::Live,
            model: base.to_owned(),
            context_window: 0,
            lanes: slots,
            page_dir: None,
            engine_bin: None,
            started_s: engine.started_s,
            placement: None,
            weights: None,
        };
        crate::inference::lane_registry::record(&record).expect("test: lane record");
        crate::inference::lane_pidfile::write(engine.pid).expect("test: lane pidfile");
        assert_eq!(crate::inference::lane_registry::live_lane().map(|lane| lane.incarnation()), Some(engine), "test: the engine reads as the live lane");
    }

    /// One pass the way the module runs it: its tick wakes the serving-wake worker, which runs the
    /// recovery pass over the live lane. Waits until `key`'s generation holds `learned` examples.
    async fn pass_by_tick(trigger: &crate::modules::training_trigger::TrainingTriggerModule, key: &BucketKey, learned: u32) -> LearnCheckpoint {
        use crate::runtime::ServiceModule;
        trigger.serving_wake.start(trigger.state.clone()).await;
        trigger.tick().await.expect("test: the tick runs");
        let store = trigger.state.durable.require().expect("test: storage");
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(300);
        loop {
            if let Some(generation) = store.checkpoint_of(key).await.expect("test: read") {
                if generation.learned() >= learned {
                    return generation;
                }
            }
            assert!(tokio::time::Instant::now() < deadline, "the tick's pass published no generation holding {learned} within 300s");
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
    }

    async fn residencies(engine: EngineIncarnation) -> usize {
        match LearningEngine::of(engine).learn_ledger().await.expect("test: the engine answers its ledger") {
            LedgerAnswer::Ledger(ledger) => ledger.residencies.len(),
            LedgerAnswer::NoLedger => panic!("test: the named engine has no learning ledger"),
        }
    }

    // what this catches: in-place learning proven only by driving MindLearner by hand, while the
    // trigger's own route (journal -> pass -> generation -> shutdown -> restart -> restore) is unrun.
    #[tokio::test(flavor = "multi_thread")]
    async fn the_trigger_learns_checkpoints_quiesces_and_resumes_on_a_live_engine() {
        // the probes are the fixture's account of each refusal
        let _ = tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::new("info")).with_test_writer().try_init();
        let home = tempfile::tempdir().expect("test: home");
        let _home = crate::paths::test_home::HomeGuard::set(home.path()).await; // the lane pidfile resolves HOME
        let (engine, base) = named_engine();
        // the engine's slots as serving registers a launched engine: a pool at its address, stamped
        // with its incarnation (3+ slots keep a scratch slot, which a cold learning step takes)
        let root = engine.root_url();
        let slots: u32 = std::env::var("CONTINUUM_LEARN_SLOTS").ok().and_then(|n| n.parse().ok()).expect("live-engine-tests: CONTINUUM_LEARN_SLOTS is the engine's -np (3 or more)");
        crate::inference::slots::register_engine_for_fixture(&root, slots, engine.pid, engine.started_s);
        register_live_lane(engine, &base, slots);
        let before = residencies(engine).await;
        let (adapter, _dir) = crate::orm::store::fresh_adapter().await;
        let persona = Uuid::new_v4();
        let key = BucketKey { persona_id: persona, trait_kind: "geography".into(), base_model: base.clone() };

        // first life: two arrivals, one pass, a confirmed generation
        let authority = live_authority();
        let custody = Arc::clone(&authority.custody);
        let (trigger, executor) = build_runtime_under(adapter.clone(), authority).await;
        submit(&executor, persona, &base, "What is the capital of Kansas?", "Topeka.").await;
        submit(&executor, persona, &base, "What is the capital of Oregon?", "Salem.").await;
        let first = pass_by_tick(&trigger, &key, 2).await;
        assert_eq!(first.learned(), 2, "both arrivals are in the generation: {:?}", first.positions);
        assert_eq!(first.steps, 2, "one committed update per arrival, as the engine counted them");
        assert!(std::path::Path::new(&first.path).join("adapter.gguf").exists(), "the generation is on disk at {}", first.path);
        // the pass's sweep ran with her seated: her shadow and its save dir must survive it
        let train_dir = std::path::Path::new(&first.path).parent().expect("test: train dir").to_path_buf();
        let prefix = format!("{}-", bucket_stem(&key));
        let shadows: Vec<String> = std::fs::read_dir(&train_dir)
            .expect("test: list")
            .filter_map(|e| e.ok()?.file_name().into_string().ok())
            .filter(|name| name.starts_with(&prefix) && name.ends_with(".gguf"))
            .collect();
        assert_eq!(shadows.len(), 1, "her seated shadow survives the sweep: {shadows:?}");

        // shutdown: nothing stays seated, nothing stays in custody, the engine holds none of hers
        crate::runtime::ServiceModule::shutdown(trigger.as_ref()).await.expect("test: shut down");
        assert_eq!(custody.held(), (0, 0), "every lease left custody on a confirmed release");
        assert_eq!(residencies(engine).await, before, "the engine holds no residency of hers after shutdown");

        // second life over the same journal: restore the generation, learn the next arrival on it
        let (trigger, executor) = build_runtime_under(adapter.clone(), live_authority()).await;
        submit(&executor, persona, &base, "What is the capital of Maine?", "Augusta.").await;
        let second = pass_by_tick(&trigger, &key, 3).await;
        assert_ne!(second.name, first.name, "a new generation, on top of the restored one");
        assert_eq!(second.learned(), 3, "the first two stay learned and are not learned again; the third joins: {:?}", second.positions);
        assert_eq!(second.steps, 3, "the restored generation's 2 updates plus exactly 1: a replayed arrival would commit more");
        assert!(!std::path::Path::new(&first.path).exists(), "the first generation left the train dir once the second was committed");
        assert!(std::path::Path::new(&second.path).join("adapter.gguf").exists(), "the newest generation stays: a reseat restores it");
        crate::runtime::ServiceModule::shutdown(trigger.as_ref()).await.expect("test: shut down");
        assert_eq!(residencies(engine).await, before, "the second life leaves nothing on the engine either");
    }
}
