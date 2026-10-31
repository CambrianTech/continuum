#![cfg(test)]

use std::sync::Arc;

use continuum_context::source::Source;

use super::actions::Actions;
use super::notices::Notices;
use crate::cognition::working_memory::WorkingMemory;

fn rendered(memory: &WorkingMemory) -> String {
    Actions { memory, persona: uuid::Uuid::nil() }.items().iter().map(|item| item.full().text.clone()).collect::<Vec<_>>().join("\n")
}

// what this catches: her action stream going silent or lying about her acts, the three
// honest states the steps ledger held (#151, #414). Zero acts: the explicit
// nothing-has-executed void (an implicit one became a confabulation shelter). An act on
// record: listed, the void gone, and still listed after chatty entries flood the
// working-memory ring (the starvation that showed a 2,863-act citizen one act). Acts whose
// details are gone (a counter restored without records): said so, never denied, and no
// promise that recall can retrieve them.
#[test]
fn her_actions_say_honestly_what_has_executed() {
    let memory = Arc::new(WorkingMemory::new(2));
    assert!(rendered(&memory).contains("nothing has executed yet"));

    memory.record_receipt("I ran code/shell(ls) Result: ok");
    memory.record_settlement("listed", None);
    memory.record_fact("chose silence — said nothing to the room");
    memory.record_fact("chose silence — said nothing to the room (again)");
    assert!(!memory.has_receipt(), "precondition: the receipt's ring entry aged out");
    let listed = rendered(&memory);
    assert!(listed.contains("[result #1") && listed.contains("code/shell(ls)"), "{listed}");
    assert!(!listed.contains("nothing has executed yet"), "{listed}");

    let earlier = WorkingMemory::new(2);
    let mut snapshot = WorkingMemory::new(2).snapshot();
    snapshot.next_action_seq = 4; // 3 acts happened in the old life, none recorded
    earlier.restore(snapshot);
    let aged = rendered(&earlier);
    assert!(aged.contains("3 steps executed earlier"), "{aged}");
    assert!(!aged.contains("nothing has executed yet"), "denied her real acts: {aged}");
    assert!(!aged.to_lowercase().contains("recall can retrieve"), "{aged}");
}

// what this catches: a deploy costing every resident a cold turn. Her view is checkpointed with
// her working memory, so after a restart it renders the same messages, the bytes the engine's
// saved page holds, instead of a new epoch built from scratch.
#[test]
fn her_view_survives_a_restart_in_her_checkpoint() {
    use continuum_context::view::{Claim, View};

    let memory = WorkingMemory::new(8);
    memory.record_receipt("I ran code/list Result: 4 files");
    memory.record_settlement("listed", None);
    let actions = Actions { memory: &memory, persona: uuid::Uuid::nil() };
    let whole = |claims: &[Claim]| claims.iter().map(|claim| claim.full_chars).collect();
    let (view, _) = View::start(Vec::new(), &[&actions as &dyn Source<_>], whole).unwrap();
    let rendered = |view: &View<super::ContextMessage>| view.render().map(|m| m.text.clone()).collect::<Vec<_>>();
    let before = rendered(&view);
    memory.keep_view(Some(view));

    let restored = WorkingMemory::new(8);
    restored.restore(memory.snapshot());
    let resumed = restored.take_view().expect("the checkpoint carried her view");
    assert_eq!(rendered(&resumed), before);
    assert!(restored.take_view().is_none(), "taken once, by the act that resumes it");
}

// what this catches (Cormac on #5255): a result kept in memory but never saved under a handle
// collapsing at a turnover to its one-line ledger head, which nothing can reopen (goal 7). Its
// collapsed form is the whole result, so the view can only keep it, never cut it.
#[test]
fn an_unsaved_result_never_collapses_to_a_line_nothing_reopens() {
    let memory = WorkingMemory::new(8);
    memory.record_receipt("I ran code/shell(cat Cargo.toml) Result: [package] name = \"x\"");
    memory.record_settlement("read the manifest", None);
    let (acts, _) = memory.action_renderings();
    let act = acts.last().expect("the act is on record");
    assert!(act.stored.is_none(), "an unsaved result is resident: nothing to load");
    assert!(act.full.contains("name = \"x\""), "{}", act.full);
    assert_eq!(act.pointer, act.full, "an unsaved result's pointer is the result itself");
}

// what this catches (FOCUS-CONTRACT rule 7, FLOW §2d): RAM storage deciding what she sees. A
// result's body evicted from RAM for its handle left only its pointer in her view, so the RAM
// bound was a visibility bound. Evicted, it is priced at its whole (`stored`) and loaded back
// whole when the allocation grants it, labelled exactly as it read resident.
#[test]
fn an_evicted_result_is_still_seen_whole_through_its_stored_level() {
    let persona = uuid::Uuid::new_v4();
    let memory = WorkingMemory::new(8);
    let body = "a line of the evicted result\n".repeat(400);
    let pin = crate::cognition::tool_executor::spill::LivePin::new();
    memory.record_receipt_typed(&[], &body, None, pin.retain(persona, &body));
    memory.record_receipt_typed(&[], "the newest stays resident", None, pin.retain(persona, "the newest stays resident"));
    let (before, _) = memory.action_renderings();
    let resident_whole = before.first().map(|act| act.full.clone()).expect("test: the first act");
    memory.evict_resident_results_over(1, &|_, _| true);
    let (acts, _) = memory.action_renderings();
    let act = acts.first().cloned().expect("test: the first act");
    let (seq, full, pointer, stored) = (act.seq, act.full, act.pointer, act.stored);
    let (chars, _) = stored.expect("evicted to its handle and priced");
    assert_eq!(full, pointer, "what stays resident is its pointer");
    assert_eq!(chars, resident_whole.chars().count(), "priced at exactly the whole it rendered");
    let source = super::actions::Actions { memory: &memory, persona };
    let item = continuum_context::source::Source::items(&source)
        .into_iter()
        .find(|item| item.key() == seq.to_string())
        .expect("test: the act's item");
    assert_eq!(continuum_context::source::whole_chars(&item), chars);
    let whole = continuum_context::source::whole(&source, item);
    assert_eq!(whole.text, resident_whole, "loaded back whole, exactly as it read resident");
}

// what this catches: her substrate's notices re-rendered as new text on every act. They were one
// block after her view, ~3.6k chars re-prefilled per act though almost none of it changed
// (Benchy and Aiko on the M5, 2026-10-24). As items, an act with the same notices appends
// nothing, a new notice appends only itself, and a condition that ended is retired from her view
// instead of reading as still holding until the next turnover.
#[test]
fn her_notices_append_once_and_an_ended_one_is_retired() {
    use continuum_context::view::{Claim, View};

    let memory = WorkingMemory::new(8);
    memory.pin_fact("hands", "[hands] your hands stand in the card's checkout");
    memory.record_fact("[rebuilt] the substrate was rebuilt while you were away");
    let notices = Notices { memory: &memory, persona: uuid::Uuid::nil() };
    let whole = |claims: &[Claim]| claims.iter().map(|claim| claim.full_chars).collect();
    let (mut view, _) = View::start(Vec::new(), &[&notices as &dyn Source<_>], whole).unwrap();
    let text = |view: &View<super::ContextMessage>| view.render().map(|m| m.text.clone()).collect::<Vec<_>>().join("\n");
    assert!(text(&view).contains(crate::cognition::framing_echo::NOTICES_HEADER), "the framing that says these are not her words");

    assert_eq!(view.advance(&[&notices as &dyn Source<_>]).chars, 0, "the same notices append nothing");

    memory.record_fact("chose silence — said nothing to the room");
    let grew = view.advance(&[&notices as &dyn Source<_>]);
    assert_eq!(grew.items, 1, "only the new notice is appended");

    memory.unpin_fact("hands");
    let before = text(&view);
    view.advance(&[&notices as &dyn Source<_>]);
    let after = text(&view);
    assert!(after.starts_with(&before) && after.contains("[ended] the notice pin/hands"), "{after}");
    assert_eq!(view.advance(&[&notices as &dyn Source<_>]).chars, 0, "retired once");

    memory.pin_fact("hands", "[hands] your hands stand in the card's checkout again");
    assert!(memory.ended_notice_keys().is_empty(), "pinned again, it is no longer ended");
}

// what this catches: a thought re-rendered by the act's budget, so the view appends every
// unchanged thought again on every act. Cut to a share of the served window, the same thought
// rendered 1,284 chars on one act and 1,445 on the next (Benchy on the M5, 2026-10-24, ~7k chars
// of unchanged thoughts re-prefilled per act). Its item must not depend on the window: how much
// of it she reads is the one allocation's call at an epoch start.
#[test]
fn a_thought_renders_the_same_whatever_the_served_window() {
    use super::thoughts::Thoughts;

    let memory = WorkingMemory::new(8);
    memory.record(&"a long line of reasoning about the failing test. ".repeat(80));
    let render = |memory: &WorkingMemory| {
        Thoughts { memory, persona: uuid::Uuid::nil() }.items().into_iter().map(|item| item.full().text.clone()).collect::<Vec<_>>()
    };
    let narrow = render(&memory);
    assert_eq!(render(&memory), narrow, "the window changed and the thought's rendering did not");
}

// what this catches: her floor growing with every act she ever took (Aris, the M5, 2026-10-29:
// 95 pointers at ~190 chars before anything whole fit). Her oldest acts fold into ONE line that
// names the first and last #n, how many, and how one reopens (tool/output act=<n>), never a list
// of ids. The banner stands ahead of the run (the view never folds a standing head), so the
// run is acts alone; a run whose keys are not act numbers names nothing rather than a guess.
#[test]
fn her_folded_acts_name_their_range_and_count_in_one_line() {
    let memory = WorkingMemory::new(2);
    let actions = Actions { memory: &memory, persona: uuid::Uuid::nil() };
    let item = |key: &str| {
        let id = super::element(uuid::Uuid::nil(), super::Producer::Act, key.as_bytes());
        continuum_context::source::Item::whole(id, continuum_context::cause::Cause::Unrecorded, key, super::ContextMessage::user(format!("act {key}")))
    };
    let run: Vec<_> = (3200..=3330).map(|seq: u64| item(&seq.to_string())).collect();
    let Some(block) = actions.fold(&run) else { panic!("a run with acts folds") };
    assert_eq!(block.text, "[actions #3200-#3330: 131 earlier acts folded; reopen one with tool/output act=<n>]");
    assert!(actions.fold(&[item("banner")]).is_none(), "a run that is not acts names nothing");
}

// what this catches: a repeated settlement collapsing into the earlier one because thoughts were
// keyed by their text: she settled passed, failed, passed and her view read passed once and
// failed once, so her latest state looked like "failed" (found porting e731576c, 2026-10-31).
// Each settlement is its own occurrence, in order.
#[test]
fn a_repeated_settlement_reaches_her_view_as_its_own_occurrence() {
    use continuum_context::view::{Claim, View};
    let memory = WorkingMemory::new(8);
    let thoughts = super::thoughts::Thoughts { memory: &memory, persona: uuid::Uuid::nil() };
    let whole = |claims: &[Claim]| claims.iter().map(|claim| claim.full_chars).collect();
    let (mut view, _) = View::start(Vec::new(), &[&thoughts as &dyn Source<_>], whole).unwrap(); // unwrap: one grant per claim cannot mismatch
    for state in ["passed", "failed", "passed"] {
        memory.record_settlement(&format!("all regression tests {state}"), None);
        view.advance(&[&thoughts as &dyn Source<_>]);
    }
    let text = view.render().map(|m: &super::ContextMessage| m.text.clone()).collect::<Vec<_>>().join("\n");
    assert_eq!(text.matches("tests passed").count(), 2, "{text}");
    assert!(text.rfind("tests passed") > text.rfind("tests failed"), "her latest settlement reads last: {text}");
}
