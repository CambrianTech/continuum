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

// what this catches: the 2026-10-31 M5 outage. #5639 added `level` to the view's events, so every
// resident's checkpoint (written by the build before) failed to decode and registration refused all
// four for 32 minutes. A real view with only its `level` fields removed must be dropped (it is
// derived) while everything authoritative restores and saves again; a corrupt AUTHORITATIVE field
// must still refuse (Codex on #5659).
#[test]
fn a_view_from_an_older_build_is_dropped_and_authoritative_state_survives() {
    use continuum_context::view::{Claim, View};
    use crate::cognition::working_memory::VolatileSnapshot;

    let memory = WorkingMemory::new(8);
    memory.record_receipt("I ran code/list Result: 4 files");
    memory.record_settlement("listed the four files", None);
    let actions = Actions { memory: &memory, persona: uuid::Uuid::nil() };
    let whole = |claims: &[Claim]| claims.iter().map(|claim| claim.full_chars).collect();
    let (view, _) = View::start(Vec::new(), &[&actions as &dyn Source<_>], whole).unwrap();
    memory.keep_view(Some(view));
    // Text, never `serde_json::Value`: the view's element ids are u128, which a Value cannot hold.
    let good = serde_json::to_string(&memory.snapshot()).expect("encode");

    // The older build's shape: the same real view, its `level` pairs removed and nothing else.
    let (head, view_and_rest) = good.split_at(good.find("\"context_view\":").expect("the checkpoint carries her view"));
    let mut older_view = view_and_rest.to_owned();
    let mut stripped = 0;
    for variant in ["Full", "Summary", "Pointer", "Folded"] {
        for pattern in [format!("\"level\":\"{variant}\","), format!(",\"level\":\"{variant}\"")] {
            stripped += older_view.matches(pattern.as_str()).count();
            older_view = older_view.replace(pattern.as_str(), "");
        }
    }
    assert!(stripped > 0, "the fixture reproduces the missing-level view");
    let older = format!("{head}{older_view}");

    let snap: VolatileSnapshot = serde_json::from_str(&older).expect("an older view never refuses the checkpoint");
    let restored = WorkingMemory::new(8);
    restored.restore(snap);
    assert!(restored.take_view().is_none(), "the stale view is dropped: her next act opens a new epoch");
    assert!(restored.has_receipt(), "her newest result survives");
    let settled = |m: &WorkingMemory| m.recent_entries().iter().any(|entry| entry.text.contains("listed the four files"));
    assert!(settled(&restored), "her settlement survives");

    // A VALID view (u128 ids and all) still restores whole: the derived decoder is lossless.
    let current: VolatileSnapshot = serde_json::from_str(&good).expect("the current view decodes");
    let resumed = WorkingMemory::new(8);
    resumed.restore(current);
    assert!(resumed.take_view().is_some(), "a view that decodes is kept");

    // The next save keeps every authoritative field and carries no view until she renders one.
    let saved = serde_json::to_string(&restored.snapshot()).expect("encode");
    assert!(!saved.contains("\"context_view\""));
    let again = WorkingMemory::new(8);
    again.restore(serde_json::from_str(&saved).expect("the repaired save decodes"));
    assert!(again.has_receipt() && settled(&again), "authoritative state survives the repaired save");

    // A corrupt authoritative field is still refused: only the derived view is forgiven.
    let corrupt = good.replacen("\"next_action_seq\":", "\"next_action_seq\":\"not a number\",\"_was\":", 1);
    assert!(serde_json::from_str::<VolatileSnapshot>(&corrupt).is_err());
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
    // It saves through the production store: the class is the test fixture's, never ~/.continuum.
    crate::cognition::tool_executor::spill::prepare_store_fixture();
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

// what this catches: her newest thought dropped or cut when her thoughts outgrow their grant, or
// a level chosen by the source instead of the allocation. Each thought is its own item with its
// levels: a start that grants less than all of them shows her newest whole and the older ones at
// the level the grant affords (their summary or pointer), and a start that grants every whole
// shows every one whole.
#[test]
fn her_newest_thought_is_whole_and_the_allocation_levels_the_older_ones() {
    use continuum_context::view::{Claim, Level, View};
    let memory = WorkingMemory::new(2);
    for n in 0..12 {
        memory.record(&format!("thought {n}: {}", "weighing the parser branch and its index. ".repeat(30)));
    }
    let newest = "thought 12: so the fix is at checks.py:118, include the index in the label";
    memory.record(newest);
    let thoughts = super::thoughts::Thoughts { memory: &memory, persona: uuid::Uuid::nil() };
    let levels = |grant: &dyn Fn(&[Claim]) -> Vec<usize>| {
        let (view, _) = View::start(Vec::new(), &[&thoughts as &dyn Source<_>], grant).unwrap(); // unwrap: one grant per claim cannot mismatch
        let text = view.render().map(|m: &super::ContextMessage| m.text.clone()).collect::<Vec<_>>().join("\n");
        (text, view.receipt().items.into_iter().map(|item| item.level).collect::<Vec<Level>>())
    };
    let (text, tight) = levels(&|claims: &[Claim]| claims.iter().map(|claim| claim.newest_chars + claim.floor_chars).collect());
    assert!(text.contains(newest), "her newest thought is whole: {text}");
    assert!(tight.iter().any(|level| *level != Level::Full), "a tight grant shows older thoughts smaller: {tight:?}");
    let (_, roomy) = levels(&|claims: &[Claim]| claims.iter().map(|claim| claim.full_chars).collect());
    assert!(roomy.iter().all(|level| *level == Level::Full), "a grant of every whole shows every one whole: {roomy:?}");
}

// what this catches (Cormac on #5782): a restore reordering her interior chain. Her thoughts kept
// their occurrences while ring entries were renumbered after them, so after a restart she read
// "so the fix is X" before the "failed" settlement that preceded it. One checkpointed sequence:
// the restored chain has the same items, keys, ids and order.
#[test]
fn a_restore_keeps_her_thoughts_and_settlements_in_order_under_the_same_keys() {
    use super::thoughts::Thoughts;
    let memory = WorkingMemory::new(8);
    memory.record("checking the parser branch");
    memory.record_settlement("the regression tests failed", None);
    memory.record("so the fix is at checks.py:118");
    memory.record_settlement("the regression tests passed", None);
    let chain = |memory: &WorkingMemory| {
        Thoughts { memory, persona: uuid::Uuid::nil() }.items().into_iter().map(|item| (item.key().to_owned(), item.id(), item.full().text.clone())).collect::<Vec<_>>()
    };
    let before = chain(&memory);
    let restored = WorkingMemory::new(8);
    restored.restore(memory.snapshot());
    assert_eq!(chain(&restored), before, "same items, keys, ids and order after the restore");
    let order: Vec<String> = before.into_iter().map(|(_, _, text)| text).collect();
    let at = |needle: &str| order.iter().position(|text| text.contains(needle));
    assert!(at("failed") < at("so the fix") && at("so the fix") < at("passed"), "{order:?}");
}

// what this catches (Cormac on #5782): a thought log that never sheds, growing her RAM and her
// checkpoint for her whole life. Her older thoughts are saved and shed to their handles by the
// one shed verb her results take; her newest stays resident; a shed thought is still seen whole
// (loaded through its handle) and her checkpoint carries its handle, not its body.
#[test]
fn an_older_thought_sheds_to_its_handle_and_is_still_seen_whole() {
    use super::thoughts::Thoughts;
    crate::cognition::tool_executor::spill::prepare_store_fixture();
    let persona = uuid::Uuid::new_v4();
    let memory = WorkingMemory::new(8);
    let older = format!("weighing the parser branch {}", "and its index ".repeat(200));
    memory.record(&older);
    memory.record("so the fix is at checks.py:118");
    let whole = |memory: &WorkingMemory| {
        let thoughts = Thoughts { memory, persona };
        thoughts.items().into_iter().map(|item| continuum_context::source::whole(&thoughts, item).text).collect::<Vec<_>>()
    };
    let resident = whole(&memory);
    memory.save_unsaved_thoughts(persona);
    memory.evict_resident_results_over(0, &|_, _| true);
    let records = memory.thoughts();
    assert!(records[0].text.is_none() && records[0].handle.is_some(), "the older thought is shed to its handle");
    assert!(records[1].text.is_some(), "her newest thought stays resident");
    assert_eq!(whole(&memory), resident, "she sees it whole exactly as before the shed");
    let snapshot = memory.snapshot();
    assert!(snapshot.thoughts[0].text.is_none() && snapshot.thoughts[0].handle.is_some(), "the checkpoint carries its handle, not its body");
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

// what this catches: invariant 4 left unreachable from her real acts (a correct view rule no
// producer feeds). Two `read_file` calls of one path with one result, recorded the way the act
// seam records them, reach her view as the whole once and then "repeat of #1": the alias resolves
// to `code/read`, whose descriptor declares it observes. Two identical `code/write` calls, which
// declare no effect, both arrive whole (an undeclared effect authorizes nothing; Codex).
#[test]
fn her_unchanged_reread_reaches_her_view_as_a_repeat_and_an_undeclared_act_never_does() {
    use crate::cognition::act_observe::test_support::act;
    use continuum_context::view::{Claim, View};

    let whole = |claims: &[Claim]| claims.iter().map(|claim| claim.full_chars).collect();
    let body = "fn parse(&mut self) -> Result<Tree, Error> { self.step()?; }\n".repeat(40);
    let memory = WorkingMemory::new(8);
    let actions = Actions { memory: &memory, persona: uuid::Uuid::nil() };
    let (mut view, _) = View::start(Vec::new(), &[&actions as &dyn Source<_>], whole).unwrap(); // unwrap: one grant per claim cannot mismatch
    let unsaved = || crate::ai::types::ResultCustody::Unsaved;
    // Each act settles before the next, as her turn does: an act still in flight rides whole
    // after her view, so it enters the actions stream once it is settled.
    memory.record_receipt_typed(&[act("read_file", Some("src/parse.rs"))], &body, None, unsaved());
    memory.record_settlement("read", None);
    assert_eq!(view.advance(&[&actions as &dyn Source<_>]).repeats, 0);
    memory.record_receipt_typed(&[act("read_file", Some("src/parse.rs"))], &body, None, unsaved());
    memory.record_settlement("read again", None);
    assert_eq!(view.advance(&[&actions as &dyn Source<_>]).repeats, 1);
    let text = view.render().map(|m: &super::ContextMessage| m.text.clone()).collect::<Vec<_>>().join("\n");
    assert_eq!(text.matches(body.as_str()).count(), 1, "the whole arrives once");
    assert!(text.contains("repeat of #1, outcome unchanged; reopen with tool/output act=1]"), "{text}");

    let memory = WorkingMemory::new(8);
    let actions = Actions { memory: &memory, persona: uuid::Uuid::nil() };
    let (mut view, _) = View::start(Vec::new(), &[&actions as &dyn Source<_>], whole).unwrap(); // unwrap: one grant per claim cannot mismatch
    for _ in 0..2 {
        memory.record_receipt_typed(&[act("code/write", Some("src/parse.rs"))], "wrote 40 lines", None, unsaved());
        memory.record_settlement("wrote", None);
        assert_eq!(view.advance(&[&actions as &dyn Source<_>]).repeats, 0, "an undeclared act is never a repeat");
    }

    // Card 7d7b1ace: the read-only verbs declare it too. `git_status` resolves to `code/git/status`,
    // declared through action_command!'s `effect:` clause, so an unchanged status is a repeat.
    let memory = WorkingMemory::new(8);
    let actions = Actions { memory: &memory, persona: uuid::Uuid::nil() };
    let (mut view, _) = View::start(Vec::new(), &[&actions as &dyn Source<_>], whole).unwrap(); // unwrap: one grant per claim cannot mismatch
    let status = "On branch main\nnothing to commit, working tree clean\n".repeat(20);
    let mut repeats = 0;
    for _ in 0..2 {
        memory.record_receipt_typed(&[act("git_status", None)], &status, None, unsaved());
        memory.record_settlement("checked", None);
        repeats += view.advance(&[&actions as &dyn Source<_>]).repeats;
    }
    assert_eq!(repeats, 1, "an unchanged status, asked again, arrives as a repeat");
}

// what this catches: the facts that re-prefilled her tail every act (the M5, 2026-11-06), and a
// fact kept as if it still held. As items of her view a fact that holds unchanged appends nothing,
// one that stops holding is ended once, and a perception the system spoke into her room is an
// event: it stays what happened.
#[test]
fn her_facts_append_once_end_when_they_stop_and_her_perceived_events_stay() -> Result<(), continuum_context::view::GrantMismatch> {
    use super::facts::{Fact, Facts};
    use continuum_context::view::{Claim, View};

    let whole = |claims: &[Claim]| claims.iter().map(|claim| claim.full_chars).collect();
    let bounds = || Fact::state("context_bounds", "[context] 3 input turns were available".to_owned());
    let mut facts = Facts { persona: uuid::Uuid::nil(), facts: vec![bounds(), Fact::state("own_repetition", "[pattern] I said that twice".to_owned())] };
    let (mut view, _) = View::start(Vec::new(), &[&facts as &dyn Source<_>], whole)?;
    let text = |view: &View<super::ContextMessage>| view.render().map(|m| m.text.clone()).collect::<Vec<_>>().join("\n");
    assert_eq!(view.advance(&[&facts as &dyn Source<_>]).items, 0, "unchanged facts append nothing");
    facts.facts = vec![bounds(), Fact::event("e1", "[board] card 1b23 moved to review".to_owned())];
    view.advance(&[&facts as &dyn Source<_>]);
    let now = text(&view);
    assert!(now.ends_with("[board] card 1b23 moved to review\n[own repetition: no longer observed]"), "the event, then the fact that stopped:\n{now}");
    facts.facts = vec![bounds()];
    view.advance(&[&facts as &dyn Source<_>]);
    let later = text(&view);
    assert_eq!(later.matches("no longer observed").count(), 1, "ended once, and the event is never ended:\n{later}");
    assert_eq!(later.matches("[context] 3").count(), 1, "{later}");
    Ok(())
}

// what this catches: a count in a block's first line (the kanban's "192 claimable", the M5,
// 2026-11-06) rewriting the whole block, 30k chars of cards, every time anyone files a card, and
// breaking every epoch start's shared prefix at message 2-4. Split, the cards stay put and only
// the tally appends.
#[test]
fn a_blocks_tally_moves_without_rewriting_its_body() -> Result<(), continuum_context::view::GrantMismatch> {
    use super::blocks::Grounding;
    use crate::cognition::rag_budget::RagItem;
    use continuum_context::view::{Claim, View};

    let board = |claimable: usize| {
        let units: Vec<RagItem> = std::iter::once(RagItem::whole("headline", format!("[board] {claimable} claimable here."), serde_json::json!({})).as_tally())
            .chain((0..30).map(|n| RagItem::whole(format!("card:{n}"), format!("card {n:04} [Open] \"a card\""), serde_json::json!({}))))
            .collect();
        let mut block = Grounding::atomic(uuid::Uuid::nil(), "room-kanban", "");
        block.units = units.into();
        block.split_tally()
    };
    let first = board(192);
    assert_eq!(first.iter().map(|b| b.faculty.as_str()).collect::<Vec<_>>(), ["room-kanban", "room-kanban counts"]);
    let whole = |claims: &[Claim]| claims.iter().map(|claim| claim.full_chars).collect();
    fn sources(blocks: &[Grounding]) -> Vec<&dyn Source<super::ContextMessage>> {
        blocks.iter().map(|b| b as &dyn Source<super::ContextMessage>).collect()
    }
    let (mut view, _) = View::start(Vec::new(), &sources(&first), whole)?;
    let before: Vec<String> = view.render().map(|m| m.text.clone()).collect();
    assert!(before[0].starts_with("[room-kanban]\ncard 0000"), "the body leads with its cards: {}", before[0]);
    let next = board(195);
    let advance = view.advance(&sources(&next));
    assert_eq!(advance.items, 1, "only the tally appends");
    assert!(advance.chars < 60, "a few chars, never the cards again: {}", advance.chars);
    Ok(())
}
