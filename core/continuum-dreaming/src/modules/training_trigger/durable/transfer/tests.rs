#![cfg(test)]
use super::*;
use crate::commands::training_trigger::test_support::{build_runtime, ex};
use crate::inference::learning::{LearnPort, Learning, Unavailable};

fn key(persona: Uuid, base: &str) -> BucketKey {
    BucketKey { persona_id: persona, trait_kind: "code".into(), base_model: base.into() }
}

fn batch(prompts: &[&str]) -> PendingBatch {
    PendingBatch {
        returned: false,
        persona_name: "test-p".into(),
        source: serde_json::from_value(serde_json::json!("operator_curated")).expect("test: source"),
        examples: prompts.iter().map(|p| ex(p, "c")).collect(),
        lora: None,
        schedule: None,
        local_artifact_dir: None,
        preferred_provider: None,
        validation_split: 0.1,
        eval_set: None,
    }
}

/// A row accepted under `persona`'s bucket on `base`.
async fn seed(state: &Arc<TrainingTriggerState>, persona: Uuid, base: &str, prompts: &[&str]) -> Uuid {
    let id = Uuid::new_v4();
    let rows = batch(prompts);
    state
        .run_owned(key(persona, base), crate::modules::training_trigger::OwnedKind::Test, move |state, key| async move { state.accept(&key, Some(id), rows).await })
        .await
        .expect("test: admitted")
        .expect("test: accepted");
    id
}

async fn runtime() -> (Arc<super::super::super::TrainingTriggerModule>, Arc<crate::runtime::CommandExecutor>, tempfile::TempDir) {
    let (adapter, dir) = crate::orm::store::fresh_adapter().await;
    let (trigger, executor) = build_runtime(adapter).await;
    (trigger, executor, dir)
}

/// An engine that takes every command and answers none: a seat told to it stays seated.
struct Kept;

impl LearnPort for Kept {
    fn tell(&self, _: Learning) -> Result<(), Unavailable> {
        Ok(())
    }
}

// what this catches (Codex on 9913596d): a transfer that carries what she already learned (that
// position learned twice), changes the source beyond leaving pending (its lived text, its base, its
// learned positions), loses the link to the origin, or carries again on a repeat. Only her examples
// not learned yet are carried, once; the source keeps what it learned.
#[tokio::test]
async fn only_her_outstanding_examples_are_carried_once_and_the_source_keeps_what_it_learned() {
    let (trigger, _executor, _dir) = runtime().await;
    let state = &trigger.state;
    let persona = Uuid::new_v4();
    let from = key(persona, "test");
    let id = seed(state, persona, "test", &["a", "b", "c"]).await;
    let store = state.durable.require().expect("test: store");
    let generation = Generation { name: "g1".into(), path: "/nowhere".into(), binding: "b".into(), steps: 1, provenance: serde_json::json!({}), persona_name: "test-p".into(), signature: None };
    store.commit_checkpoint(&from, generation, &[Position { submission: id, learned: 1 }]).await.expect("test: she learned one on the source base");

    let done = state.transfer_apply(&from, "qwen").await.expect("test: carried");
    assert_eq!(done.to, key(persona, "qwen"));
    assert_eq!(done.carried, Carried { rows: 1, examples: 2 });
    let source = store.submissions.find_by_id(id).await.expect("test: read").expect("test: the source stays");
    assert!(!source.is_pending, "the source leaves pending");
    assert_eq!((source.learned, source.base_model.as_str(), source.batch.examples.len()), (1, "test", 3), "what it learned, its base and its lived text stay as written");
    let into = source.carried_to.expect("test: the source names where its rest went");
    let carried = store.submissions.find_by_id(into).await.expect("test: read").expect("test: the carried row");
    assert_eq!((carried.carried_from, carried.carried_offset, carried.learned, carried.is_pending), (Some(id), 1, 0, true), "it names its origin and starts where the source stopped");
    assert_eq!(carried.batch.examples.iter().map(|e| e.prompt.as_str()).collect::<Vec<_>>(), ["b", "c"], "only the examples not learned yet");
    assert_eq!(state.bucket_example_count(persona, "code", "qwen").await, Some(2), "they wait on the base she runs");
    assert_eq!(state.bucket_example_count(persona, "code", "test").await, None, "nothing of hers waits on the source base");

    let again = state.transfer_apply(&from, "qwen").await;
    assert!(matches!(again, Err(TransferRefused::NothingOutstanding)), "a repeat carries nothing: {again:?}");
}

// what this catches (Codex on 9913596d): a transfer while a seat holds either bucket, so a seat's
// volatile progress (naming the source) or a target seat's lesson picks up a position the transfer
// also moves. Refused by name, and nothing moves.
#[tokio::test]
async fn a_seat_on_either_bucket_refuses_the_transfer_and_nothing_moves() {
    let (trigger, _executor, _dir) = runtime().await;
    let state = &trigger.state;
    let port: Arc<dyn LearnPort> = Arc::new(Kept);
    for (seated, which) in [("test", "the source bucket"), ("qwen", "the target bucket")] {
        let persona = Uuid::new_v4();
        seed(state, persona, "test", &["a"]).await;
        seed(state, persona, "qwen", &["q"]).await;
        state.seat_bucket(&port, &key(persona, seated)).await.expect("test: seated");
        assert!(state.seated_on_engine(&key(persona, seated)));
        let refused = state.transfer_apply(&key(persona, "test"), "qwen").await;
        assert!(matches!(refused, Err(TransferRefused::Seated(named)) if named == which), "{refused:?}");
        assert_eq!(state.bucket_example_count(persona, "code", "test").await, Some(1), "the source keeps its row");
    }
}

// what this catches: a transfer onto the base the bucket is already on (a no-op that would write a
// second row for the same examples), or onto a base no catalog row names.
#[tokio::test]
async fn a_transfer_onto_the_same_base_or_an_unknown_one_is_refused() {
    let (trigger, _executor, _dir) = runtime().await;
    let persona = Uuid::new_v4();
    seed(&trigger.state, persona, "test", &["a"]).await;
    let same = trigger.state.transfer_apply(&key(persona, "test"), "test").await;
    assert!(matches!(same, Err(TransferRefused::SameBase(_))), "{same:?}");
    let unknown = trigger.state.transfer_apply(&key(persona, "test"), "no-such-base").await;
    assert!(matches!(unknown, Err(TransferRefused::Base(_))), "{unknown:?}");
}
