use super::*;

fn outcome(committed_steps: i64, effects_uncertain: bool) -> LearnOutcome {
    serde_json::from_value(serde_json::json!({
        "success": committed_steps > 0, "committed_steps": committed_steps, "effects_uncertain": effects_uncertain,
        "refusal": "none", "loss": 0.0, "n_labels": 0, "t_ms": 0.0, "max_chunk_ms": 0.0, "restoration_ms": 0.0
    }))
    .expect("test: an engine outcome")
}

// what this catches: the cursor moving on an example the engine did not commit, or a quarantined
// shadow kept learning. Only a committed update with certain effects counts; uncertainty wins.
#[test]
fn only_a_committed_certain_update_moves_the_cursor() {
    assert_eq!(verdict(&outcome(1, false)), Verdict::Learned);
    assert_eq!(verdict(&outcome(0, false)), Verdict::Again, "refused, deferred, or ended before an update");
    assert_eq!(verdict(&outcome(1, true)), Verdict::Quarantined, "committed, then uncertain: quarantined, never counted");
    assert_eq!(verdict(&outcome(0, true)), Verdict::Quarantined);
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
    let state = Arc::new(TrainingTriggerState::new());
    state.in_place.install(authority());
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
