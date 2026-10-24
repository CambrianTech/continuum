use std::sync::Arc;

use continuum_context::source::Source;

use super::actions::Actions;
use super::notices::Notices;
use crate::cognition::working_memory::WorkingMemory;

fn rendered(memory: &WorkingMemory) -> String {
    Actions { memory }.items().iter().map(|item| item.full.text.clone()).collect::<Vec<_>>().join("\n")
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
    let actions = Actions { memory: &memory };
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
    let (_, full, pointer) = acts.last().expect("the act is on record");
    assert!(full.contains("name = \"x\""), "{full}");
    assert_eq!(pointer, full, "an unsaved result's pointer is the result itself");
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
    let notices = Notices { memory: &memory };
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
