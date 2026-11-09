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

#[test]
fn a_trait_kind_becomes_a_file_name_part() {
    assert_eq!(file_safe("code/owner"), "code-owner");
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

// what this catches: one engine's step or fence (a wedged, unproven-dead engine) holding every other
// engine's buckets (Codex on d177df17f). A busy engine defers its own bucket at once; another engine
// is not waited on.
#[tokio::test]
async fn a_busy_engine_defers_its_bucket_and_never_holds_another() {
    let state = Arc::new(TrainingTriggerState::new(authority()));
    let key = BucketKey { persona_id: Uuid::new_v4(), trait_kind: "code".into(), base_model: "qwen".into() };
    let (wedged, free) = (engine(1), engine(2));
    let _held = state.in_place.engine(wedged).lock_owned().await;
    let pass = tokio::time::timeout(std::time::Duration::from_secs(1), state.in_place.learn_bucket(&state, &key, vec![], wedged))
        .await
        .expect("test: a busy engine answers at once")
        .expect("test: deferred, not failed");
    assert_eq!(pass, BucketPass { deferred: true, ..Default::default() });
    let other = tokio::time::timeout(std::time::Duration::from_secs(1), state.in_place.learn_bucket(&state, &key, vec![], free))
        .await
        .expect("test: another engine is not held by the wedged one")
        .expect("test: nothing to learn");
    assert_eq!(other, BucketPass::default());
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

/// The trigger learning in place on a LIVE engine, through its real route (card 4114a2c6; Codex on
/// 086d60123): arrivals through `genome/training-trigger/submit`, the pass the tick runs, a
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
        let (engine, base) = named_engine();
        // the engine's slots as serving registers a launched engine: a pool at its address, stamped
        // with its incarnation (3+ slots keep a scratch slot, which a cold learning step takes)
        let root = engine.root_url();
        let slots: u32 = std::env::var("CONTINUUM_LEARN_SLOTS").ok().and_then(|n| n.parse().ok()).expect("live-engine-tests: CONTINUUM_LEARN_SLOTS is the engine's -np (3 or more)");
        crate::inference::slots::register_engine_for_fixture(&root, slots, engine.pid, engine.started_s);
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
        trigger.state.learn_in_place(engine, &base).await.expect("test: the pass runs");
        let store = trigger.state.durable.require().expect("test: storage");
        let first = store.checkpoint_of(&key).await.expect("test: read").expect("the pass published a confirmed generation");
        assert_eq!(first.learned(), 2, "both arrivals are in the generation: {:?}", first.positions);
        assert!(std::path::Path::new(&first.path).join("adapter.gguf").exists(), "the generation is on disk at {}", first.path);

        // shutdown: nothing stays seated, nothing stays in custody, the engine holds none of hers
        trigger.state.drain_operations().await.expect("test: drained");
        assert_eq!(custody.held(), (0, 0), "every lease left custody on a confirmed release");
        assert_eq!(residencies(engine).await, before, "the engine holds no residency of hers after shutdown");

        // second life over the same journal: restore the generation, learn the next arrival on it
        let (trigger, executor) = build_runtime_under(adapter.clone(), live_authority()).await;
        submit(&executor, persona, &base, "What is the capital of Maine?", "Augusta.").await;
        trigger.state.learn_in_place(engine, &base).await.expect("test: the pass runs");
        let store = trigger.state.durable.require().expect("test: storage");
        let second = store.checkpoint_of(&key).await.expect("test: read").expect("the second life published a generation");
        assert_ne!(second.name, first.name, "a new generation, on top of the restored one");
        assert_eq!(second.learned(), 3, "the first two stay learned and are not learned again; the third joins: {:?}", second.positions);
        trigger.state.drain_operations().await.expect("test: drained");
        assert_eq!(residencies(engine).await, before, "the second life leaves nothing on the engine either");
    }
}
