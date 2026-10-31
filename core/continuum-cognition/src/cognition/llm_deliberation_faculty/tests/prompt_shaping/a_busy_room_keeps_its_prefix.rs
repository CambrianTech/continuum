#![cfg(test)]

//! Her view at a turnover leaves a growth reserve ([`start_grants`]): in a room whose
//! window slides, the acts after a turnover append to an unchanged prefix instead of
//! turning over again (Atlas and Benchy on the M5, 2026-10-23: 3-12% shared).

use continuum_context::source::{Item, Source};
use continuum_context::view::{claims, Claim, View};

use crate::cognition::llm_deliberation_faculty::prompt::start_grants;
use crate::cognition::rag_budget::{flex_allocate, FlexAllocation, RagSourceBudget};

/// One of her sources: keyed items, each collapsible to a short pointer.
struct Stream {
    name: &'static str,
    priority: u8,
    items: Vec<(String, String)>,
}

impl Source<String> for Stream {
    fn name(&self) -> &str {
        self.name
    }
    fn priority(&self) -> u8 {
        self.priority
    }
    fn items(&self) -> Vec<Item<String>> {
        self.items
            .iter()
            .map(|(key, text)| {
                // A fixture turn: its id from its key, its cause unrecorded (this test is about starts).
                let id = crate::cognition::context_streams::element(uuid::Uuid::nil(), crate::cognition::context_streams::Producer::Turn, key.as_bytes());
                Item::whole(id, continuum_context::cause::Cause::Unrecorded, key.clone(), text.clone()).with_pointer(format!("[{key} collapsed]"))
            })
            .collect()
    }
}

const GROUNDING: u8 = crate::cognition::context_streams::priority::GROUNDING;
const CONVERSATION: u8 = crate::cognition::context_streams::priority::CONVERSATION;
const TURNS_SHOWN: usize = 40;

/// One room turn of 500 chars.
fn turn(n: usize) -> (String, String) {
    (format!("turn-{n:04}"), format!("{n:04}:{}", "x".repeat(495)))
}

fn turn_chars() -> usize {
    turn(0).1.chars().count()
}

/// The window's one allocation over her view's claims, priced one token per char as
/// `view_claims` prices them before calibration.
fn window_plan(claims: &[Claim], window: u32) -> FlexAllocation {
    let budgets: Vec<RagSourceBudget> = claims
        .iter()
        .map(|claim| RagSourceBudget {
            source_id: format!("view/{}", claim.source),
            priority: claim.priority,
            floor_tokens: claim.floor_chars as u32,
            min_tokens: claim.floor_chars as u32,
            max_tokens: claim.full_chars as u32,
            required: false,
        })
        .collect();
    flex_allocate(window, &budgets)
}

fn rendered(view: &View<String>) -> String {
    view.render().map(String::as_str).collect::<Vec<_>>().join("\n")
}

/// Run `acts` acts of a room whose window slides by one turn an act, turning over the way
/// `fit_view` does (over what the window granted plus the room no claim took), and return
/// how many acts turned over and the longest run of acts that only appended.
fn slide(window: u32, acts: usize, sized: impl Fn(&[Claim], &FlexAllocation, usize, Option<usize>) -> Vec<usize>) -> (usize, usize) {
    let grounding = Stream { name: "identity", priority: GROUNDING, items: vec![("block".into(), "g".repeat(2_000))] };
    let mut conversation = Stream { name: "conversation", priority: CONVERSATION, items: (0..TURNS_SHOWN).map(turn).collect() };
    let whole = |claims: &[Claim]| claims.iter().map(|claim| claim.full_chars).collect();
    let (mut view, _) = View::start(Vec::new(), &[&grounding as &dyn Source<String>, &conversation], whole).expect("one grant per source");
    let (mut turnovers, mut run, mut longest) = (0, 0, 0);
    for act in TURNS_SHOWN..TURNS_SHOWN + acts {
        conversation.items.remove(0);
        conversation.items.push(turn(act));
        let sources = [&grounding as &dyn Source<String>, &conversation];
        let before = rendered(&view);
        view.advance(&sources);
        let plan = window_plan(&claims(&sources), window);
        let available = plan.allocations.iter().map(|a| a.allocated_tokens as usize).sum::<usize>() + plan.unallocated as usize;
        if view.chars() > available {
            let per_act = view.appended_per_act();
            view.turn_over(Vec::new(), &sources, |claims: &[Claim]| sized(claims, &plan, available, per_act)).expect("one grant per source");
            turnovers += 1;
        }
        if rendered(&view).starts_with(&before) {
            run += 1;
            longest = longest.max(run);
        } else {
            run = 0;
        }
    }
    (turnovers, longest)
}

// what this catches: the M5 on 2026-10-23 (Atlas in 11c1a7ac, Benchy in cb2e21a1), where a
// source granted all it wants restarted whole, so the start filled its grant and the next
// act, holding the turn that scrolled out plus the new one, turned over again: every act's
// prompt began at a different oldest turn. With the reserve, the acts after a turnover
// append to an unchanged prefix.
#[test]
fn a_sliding_room_appends_for_many_acts_after_a_turnover() {
    // The window holds exactly what the sources want whole: no room left over, as when the
    // reply has taken its runaway bound.
    let window = (2_000 + TURNS_SHOWN * turn_chars()) as u32;
    let restart_whole = |claims: &[Claim], _: &FlexAllocation, _: usize, _: Option<usize>| {
        claims.iter().map(|claim| claim.full_chars).collect()
    };
    let (turnovers, longest) = slide(window, 30, restart_whole);
    assert_eq!((turnovers, longest), (30, 0), "precondition: restarting whole turns over every act of a sliding room");

    let (turnovers, longest) = slide(window, 30, start_grants);
    assert!(turnovers <= 2, "a reserved start lives many acts: {turnovers} turnovers in 30 acts");
    assert!(longest >= 15, "the acts after a turnover only append: longest run {longest}");
}

// what this catches: a reserve taken evenly from every source would cut her identity, tools
// and policy blocks, which change least and lead the prefix; the reserve must come out of
// the lowest-priority sources (her conversation, oldest turns collapsed first) while the
// newest turn stays whole. And a reserve smaller than one act's appends would turn over on
// the very next act.
#[test]
fn a_turnover_keeps_grounding_whole_and_takes_the_reserve_from_the_conversation() {
    let grounding = Stream { name: "identity", priority: GROUNDING, items: vec![("block".into(), "g".repeat(6_000))] };
    let conversation = Stream { name: "conversation", priority: CONVERSATION, items: (0..TURNS_SHOWN).map(turn).collect() };
    let sources = [&grounding as &dyn Source<String>, &conversation];
    let claimed = claims(&sources);
    let available = 6_000 + TURNS_SHOWN * turn_chars();
    let plan = window_plan(&claimed, available as u32);

    let grants = start_grants(&claimed, &plan, available, None);
    assert_eq!(grants[0], 6_000, "grounding starts whole");
    assert!(grants[1] <= (available - 6_000) / 2, "the conversation leaves a reserve as large as its start: {grants:?}");
    let (view, _) = View::start(Vec::new(), &sources, |_: &[Claim]| grants.clone()).expect("one grant per source");
    let text = rendered(&view);
    assert!(text.contains("[turn-0000 collapsed]"), "the oldest turn collapses first");
    assert!(text.ends_with(&turn(TURNS_SHOWN - 1).1), "the newest turn stays whole");

    // An act that appends more than half of what the conversation may hold: the reserve
    // still holds one act.
    let per_act = (available - 6_000) * 3 / 4;
    let grants = start_grants(&claimed, &plan, available, Some(per_act));
    assert!(grants[1] <= (available - 6_000).saturating_sub(per_act), "the reserve holds one act's appends: {grants:?}");
}

// what this catches: a start held below the derivation's bound by a share constant. A
// quarter, replayed on one machine's captures, kept one file read whole on the M5
// (2026-10-30: Benchy granted 21,346 tokens, 16,565 appended per act): every act turned over
// and collapsed what she had just read, so she read it again (75 reads, 2 edits in an hour).
// The start is the derived bound on every machine: half of what grounding leaves, or less
// only when one act's appends need the room.
#[test]
fn a_start_keeps_the_derived_half_of_what_she_read() {
    let grounding = Stream { name: "identity", priority: GROUNDING, items: vec![("block".into(), "g".repeat(6_000))] };
    let conversation = Stream { name: "conversation", priority: CONVERSATION, items: (0..TURNS_SHOWN).map(turn).collect() };
    let sources = [&grounding as &dyn Source<String>, &conversation];
    let claimed = claims(&sources);
    let available = 6_000 + TURNS_SHOWN * turn_chars();
    let plan = window_plan(&claimed, available as u32);
    let growable = available - 6_000;

    let grants = start_grants(&claimed, &plan, available, None);
    assert!(grants[1] + turn_chars() >= growable / 2, "unmeasured, the start is the half bound: {grants:?}");

    let per_act = growable / 8;
    let grants = start_grants(&claimed, &plan, available, Some(per_act));
    assert!(grants[1] + turn_chars() >= growable / 2, "a small append leaves the half bound standing: {grants:?}");
}
