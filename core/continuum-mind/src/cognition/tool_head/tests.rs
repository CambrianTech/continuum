#![cfg(test)]

use std::sync::Arc;

use super::entity::{ToolHeadRecord, ToolHeadStore};
use super::*;
use tool_working_set::vocab::ToolSchema;

fn vocab() -> Vocabulary {
    let schema = |name: &str, tokens| ToolSchema { name: name.into(), description: format!("{name} does one thing"), tokens };
    Vocabulary::new(vec![
        schema("commands/help", 100),
        schema("code/read", 100),
        schema("code/edit", 300),
        schema("web/fetch", 200),
    ])
    .expect("small vocabulary")
}

fn cost() -> CostModel {
    // one extra act costs 2,000 prompt-token equivalents: decode 200 tokens at a 10x rate ratio
    CostModel { prefill_tokens_per_s: 100.0, decode_tokens_per_s: 10.0, act_decode_tokens: 200.0, act_overhead_ms: 0.0, samples: 1 }
}

fn verbs(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
}

fn at(minutes: u64) -> EpochMs {
    EpochMs::from_millis(1_800_000_000_000 + minutes * 60_000)
}

fn names(head: &HeadSet, vocab: &Vocabulary) -> Vec<String> {
    head.tools().iter().map(|tool| vocab.name(*tool).to_owned()).collect()
}

// what this catches: the head moving mid-epoch (a re-prefill of everything after the system
// prompt), or a turnover adopting something other than the head the budget reserved for.
#[test]
fn the_head_changes_only_at_a_turnover_and_to_the_head_that_was_reserved() {
    let head = ToolHead::default();
    head.set_vocabulary(vocab());
    let room = Some(Uuid::from_u128(7));
    for minute in 0..6 {
        head.note(at(minute), room, &verbs(&["code/read"]), &[], "code/read(file_path=a.rs)");
    }
    let next = head.next(at(6), room, 10_000, &cost()).expect("a call to learn from");
    assert!(names(&next, &vocab()).contains(&FINDER.to_owned()), "the finder is always held");
    assert_eq!(head.held(), None, "nothing is adopted before a turnover");
    head.note(at(7), room, &verbs(&["code/read"]), &[], "code/read(file_path=b.rs)");
    assert_eq!(head.held(), None, "an act inside the epoch never moves the head");
    head.turn_over(at(8), Some(&next));
    assert_eq!(head.held(), Some(names(&next, &vocab())));
}

// what this catches: a verb found with commands/help not becoming callable for the rest of the
// epoch, a held verb counted as loaded, or the tail surviving into the next epoch's head.
#[test]
fn a_found_verb_is_loaded_until_the_epoch_ends() {
    let head = ToolHead::default();
    head.set_vocabulary(vocab());
    let vocab = vocab();
    let held = HeadSet::of(&vocab, vec![vocab.id("commands/help").expect("listed"), vocab.id("code/read").expect("listed")]);
    head.turn_over(at(0), Some(&held));
    head.note(at(1), None, &verbs(&["commands/help"]), &verbs(&["web/fetch", "code/read"]), "commands/help(name=web/fetch)");
    assert_eq!(head.loaded(), verbs(&["web/fetch"]));
    head.turn_over(at(2), Some(&held));
    assert!(head.loaded().is_empty());
}

// what this catches: the miss/waste receipt counting a verb she called twice as two finds, or
// charging waste for a held verb she did use.
#[test]
fn an_ended_epoch_reports_its_misses_and_waste_against_the_head_it_held() {
    let head = ToolHead::default();
    head.set_vocabulary(vocab());
    let vocab = vocab();
    let held = HeadSet::of(&vocab, vec![vocab.id("commands/help").expect("listed"), vocab.id("code/edit").expect("listed")]);
    head.turn_over(at(0), Some(&held));
    for minute in 1..4 {
        head.note(at(minute), None, &verbs(&["code/read"]), &[], "code/read(file_path=a.rs)");
    }
    let cost = head.turn_over(at(5), Some(&held)).expect("an epoch with calls ended");
    assert_eq!((cost.calls, cost.misses, cost.waste_tokens, cost.head_tokens, cost.learned), (3, 1, 400, 400, true));
}

// what this catches: her learned head not surviving a restart, or a second save inserting a
// second row for the same mind.
#[tokio::test]
async fn her_head_survives_a_restart_through_her_store() {
    let (adapter, _dir) = crate::orm::store::fresh_adapter().await;
    let store = Arc::new(crate::orm::OrmStore::<ToolHeadRecord>::new(adapter).await.expect("schema"));
    let persona = Uuid::from_u128(42);
    let kept = ToolHeadStore::new(Arc::clone(&store), persona);
    assert_eq!(kept.load().await.expect("read"), None);
    let head = ToolHead::default();
    head.set_vocabulary(vocab());
    head.note(at(1), None, &verbs(&["code/read"]), &[], "code/read(file_path=a.rs)");
    kept.keep(head.snapshot()).await.expect("first save");
    head.note(at(2), None, &verbs(&["code/edit"]), &[], "code/edit(file_path=a.rs)");
    kept.keep(head.snapshot()).await.expect("second save updates");
    assert_eq!(store.find_all().await.expect("rows").len(), 1);
    let restored = ToolHead::default();
    restored.restore(kept.load().await.expect("read").expect("kept"));
    assert_eq!(restored.snapshot(), head.snapshot());
}
