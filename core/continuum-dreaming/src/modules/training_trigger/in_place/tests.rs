#![cfg(test)]
use super::*;
use crate::genome::learner::seat::Authority;


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


/// Hold one owned operation running until the trigger is dropped: what a seat's act in flight looks
/// like to the drain.
fn hold_an_operation(state: &TrainingTriggerState) {
    state.operations.lock().spawn(state.running.named(super::super::OwnedKind::Test, None, std::future::pending()));
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

/// The test binary's one probe router, installed once as the global subscriber: a thread-local
/// default misses events when the suite runs in parallel (this test, 2026-11-22: it passed alone and
/// with --test-threads=1, and received nothing in the full parallel suite).
fn probe_router() -> &'static continuum_kernel::routing::ProbeRouterLayer {
    static ROUTER: std::sync::OnceLock<continuum_kernel::routing::ProbeRouterLayer> = std::sync::OnceLock::new();
    ROUTER.get_or_init(|| {
        use tracing_subscriber::layer::SubscriberExt;
        let layer = continuum_kernel::routing::ProbeRouterLayer::new();
        let handle = layer.clone();
        tracing::subscriber::set_global_default(tracing_subscriber::registry().with(layer)).expect("test: this suite's only global subscriber"); // test fixture
        handle
    })
}

/// What one `training.in_place.no_learning_engine` probe says waits, decoded once from its fields.
#[derive(serde::Deserialize)]
struct Waiting {
    examples_waiting: String,
    buckets: String,
}

/// Every no-learning-engine probe received so far that names `persona_name`'s bucket. Other tests in
/// this binary emit the same class for their own buckets, so a test reads only its own.
fn named_for(received: &mut tokio::sync::broadcast::Receiver<continuum_kernel::routing::ProbeEvent>, persona_name: &str) -> Vec<Waiting> {
    let mut mine = Vec::new();
    loop {
        match received.try_recv() {
            Ok(event) => {
                let waiting: Waiting = serde_json::from_value(serde_json::to_value(&event.fields).expect("test: probe fields")).expect("test: the probe names what waits"); // test fixture
                if waiting.buckets.contains(persona_name) {
                    mine.push(waiting);
                }
            }
            Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => continue,
            Err(_) => return mine,
        }
    }
}

// what this catches (Codex, the 5090 2026-11-22: 8 accepted examples, learned=0, no checkpoint,
// and silence): a node with accepted examples and no learning engine read as nothing to do. The
// emitted probe itself is asserted: none for her bucket while nothing waits, exactly one across two
// passes once her example waits, naming her bucket and its count.
#[tokio::test]
async fn accepted_examples_with_no_learning_engine_are_named_once() {
    let mut received = probe_router().subscribe("training.in_place.no_learning_engine");
    let home = tempfile::tempdir().expect("test: home"); // test fixture
    let _home = crate::paths::test_home::HomeGuard::set(home.path()).await;
    assert!(crate::modules::training_trigger::on_engine::port().is_none(), "test: no node engine in this process");
    let (trigger, executor, _dir) = crate::commands::training_trigger::test_support::build_runtime_trigger_only().await;
    let state = trigger.state.clone();
    let persona_name = format!("no-engine-fixture-{}", Uuid::new_v4());
    state.recover_pass().await.expect("test: the pass runs"); // test fixture
    assert!(named_for(&mut received, &persona_name).is_empty(), "nothing accepted: nothing said");
    let params = serde_json::json!({
        "personaId": Uuid::new_v4(), "personaName": persona_name, "baseModel": "qwen", "traitKind": "geography",
        "examples": [crate::commands::training_trigger::test_support::ex("What is the capital of Kansas?", "Topeka.")], "source": "operator_curated",
    });
    let answer = executor.execute_json("genome/training-trigger/submit", params).await.expect("test: submit routes"); // test fixture
    assert_eq!(answer["success"], true, "test: the arrival is accepted: {answer}");
    state.recover_pass().await.expect("test: the pass runs"); // test fixture
    state.recover_pass().await.expect("test: the pass runs"); // test fixture
    let named = named_for(&mut received, &persona_name);
    assert_eq!(named.len(), 1, "named once across two passes, never a quiet Ok and never every pass");
    assert_eq!(named[0].examples_waiting.parse::<u64>().ok(), Some(1), "the total waiting");
    assert!(named[0].buckets.contains(&format!("{persona_name}/geography/qwen: 1")), "her bucket and its count: {}", named[0].buckets);
}

// what this catches (the 5090, 2026-11-22: the trainer's drain exceeded 2 s and its join timed out,
// with no step named): a shutdown step that reports only after it ends says nothing when it is the one
// that hung and the runtime drops it. Each step announces its begin first, then its end with its ms.
/// One `training.shutdown.step` probe, decoded once from its fields.
#[derive(serde::Deserialize)]
struct ShutdownStep {
    step: String,
    phase: String,
    ms: Option<String>,
}

/// The shutdown-step probes received so far for `step` alone (other tests shut triggers down too).
fn steps_named(received: &mut tokio::sync::broadcast::Receiver<continuum_kernel::routing::ProbeEvent>, step: &str) -> Vec<ShutdownStep> {
    std::iter::from_fn(|| received.try_recv().ok())
        .map(|event| serde_json::from_value::<ShutdownStep>(serde_json::to_value(&event.fields).expect("test: probe fields")).expect("test: a shutdown step names itself")) // test fixture
        .filter(|seen| seen.step == step)
        .collect()
}

#[tokio::test]
async fn a_shutdown_step_announces_its_begin_before_it_runs_and_its_end_after() {
    let mut received = probe_router().subscribe("training.shutdown.step");
    const STEP: &str = "test.a_shutdown_step_announces_its_begin";
    let ran = crate::modules::training_trigger::shutdown_step(STEP, async {
        let begun = steps_named(&mut received, STEP);
        assert_eq!(begun.len(), 1, "its begin is out before the step's work runs");
        assert_eq!(begun[0].phase, "begin");
        7
    })
    .await;
    assert_eq!(ran, 7, "the step's own result passes through");
    let ended = steps_named(&mut received, STEP);
    assert_eq!(ended.len(), 1, "one end after it finishes");
    assert_eq!(ended[0].phase, "end");
    assert!(ended[0].ms.is_some(), "the end carries how long the step took");
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
// what this catches (Codex, BigMama 8bed install 2026-11-11): the trigger answering the runtime's
// drain with 0 while a learning operation still runs, so the shutdown receipt said Drained and the
// join then timed out. A running operation is counted, and nothing new is admitted after it.

// what this catches (Codex, BigMama 8bed install 2026-11-11): the trigger answering the runtime's
// drain with 0 while a learning operation still runs, so the shutdown receipt said Drained and the
// join then timed out. A running operation is counted, and nothing new is admitted after it.
#[tokio::test]
async fn a_drain_counts_the_learning_still_running() {
    let state = Arc::new(TrainingTriggerState::new(authority(), &crate::commands::training_trigger::test_support::fixture_catalog()));
    hold_an_operation(&state);
    assert_eq!(state.stop_admitting().await, 1, "the running operation is in flight, not drained");
    assert!(state.require_ready().is_err(), "a stopping trigger admits nothing new");
}

// what this catches (Codex on #5928 at f8dfb9bcc): the trigger's drain or save override removed, so
// the runtime's hooks fall back to the trait defaults (drain 0, save a no-op) and the shutdown
// receipt says Drained while learning runs. The runtime's hooks reach the trigger: the drain counts
// the operation in flight, and the save checkpoints the seats (none here: no node engine).
#[tokio::test]
async fn the_runtime_hooks_drain_and_save_through_the_trigger() {
    use crate::runtime::ServiceModule;
    let trigger = crate::modules::training_trigger::TrainingTriggerModule::new(authority(), &crate::commands::training_trigger::test_support::fixture_catalog());
    hold_an_operation(&trigger.state);
    assert_eq!(trigger.drain().await, Ok(1), "the runtime's drain counts the learning still running");
    trigger.save_state().await.expect("test: the save reaches the trigger and checkpoints its seats");
}
