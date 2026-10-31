//! Her view's source order ([`LlmDeliberationFaculty::view_sources`]): consecutive acts share
//! the whole view, and a turnover keeps the prefix up to the first source whose front changed.
#![cfg(test)]

use super::*;
use crate::cognition::context_streams::blocks::Grounding;
use crate::cognition::context_streams::conversation::{Conversation, Turn};
use crate::cognition::context_streams::Role;

fn block(faculty: &str, text: &str) -> Grounding {
    Grounding::atomic(uuid::Uuid::nil(), faculty, text)
}

fn turn(n: usize) -> Turn {
    Turn { key: format!("turn-{n}"), role: Role::User, line: format!("[occurred {n}] Operator: turn {n}"), pointer: None }
}

/// One act's sources: a room window of six turns starting at `first_turn`, a settled map, the
/// kanban (canonical order, as `messages_with_visual_feedback` sorts them), and a recall block
/// that says something new every act.
fn streams(first_turn: usize, recall: &str) -> Streams {
    Streams {
        conversation: Conversation { turns: (first_turn..first_turn + 6).map(turn).collect(), ..Default::default() },
        blocks: vec![
            block("workspace-map", "MAP: src/ tests/ docs/"),
            block("room-kanban", "BOARD: you hold 1 card; 3 claimable"),
            block("recall", recall),
        ],
    }
}

/// The sources in the core's order, or in the order before 2026-10-24 (her conversation, her
/// actions, every block, her thoughts).
fn ordered<'a>(
    streams: &'a Streams,
    memory: &'a MemoryStreams<'a>,
    conversation_first: bool,
) -> Vec<&'a dyn Source<ContextMessage>> {
    if !conversation_first {
        return LlmDeliberationFaculty::view_sources(streams, Some(memory));
    }
    let mut sources: Vec<&dyn Source<ContextMessage>> = vec![&streams.conversation, &memory.actions];
    sources.extend(streams.blocks.iter().map(|block| block as &dyn Source<ContextMessage>));
    sources.push(&memory.thoughts);
    sources
}

fn texts(view: &View<ContextMessage>) -> Vec<String> {
    view.render().map(|message| message.text.clone()).collect()
}

fn shared(before: &[String], after: &[String]) -> usize {
    before.iter().zip(after).take_while(|(a, b)| a == b).count()
}

/// Two acts of one epoch, then a turnover that starts her growing sources at half (their
/// oldest items collapse, as `start_grants` reserves): what the second act's view shared with
/// the first's, how long the first was, and what the turnover kept of the view before it.
fn two_acts_and_a_turnover(conversation_first: bool) -> (usize, usize, usize) {
    let memory = WorkingMemory::new(8);
    memory.record_receipt("I ran code/list Result: 4 files");
    memory.record_settlement("listed", None);
    let memory_streams = MemoryStreams::of(&memory, uuid::Uuid::nil());
    let first = streams(0, "RECALL: the repro lives in tests/");
    let whole = |claims: &[Claim]| claims.iter().map(|claim| claim.full_chars).collect();
    let (mut view, _) = View::start(Vec::new(), &ordered(&first, &memory_streams, conversation_first), whole)
        .expect("test: one grant per source");
    let before = texts(&view);

    // Her next act: another result, the room slid by one turn, recall says something new.
    memory.record_receipt("I ran code/read(main.rs) Result: 120 lines");
    memory.record_settlement("read it", None);
    let next = streams(1, "RECALL: main.rs calls the parser");
    let sources = ordered(&next, &memory_streams, conversation_first);
    view.advance(&sources);
    let advanced = texts(&view);

    let half_of_growing = |claims: &[Claim]| {
        let grounding = crate::cognition::context_streams::priority::GROUNDING;
        claims
            .iter()
            .map(|claim| if claim.priority >= grounding { claim.full_chars } else { claim.full_chars / 2 })
            .collect()
    };
    view.turn_over(Vec::new(), &sources, half_of_growing).expect("test: one grant per source");
    (shared(&before, &advanced), before.len(), shared(&advanced, &texts(&view)))
}

// what this catches: a growing section rendered at the head of her view. Within an epoch the
// next act's prompt holds the whole previous view and appends; at a turnover, the one rewrite
// an epoch costs, her conversation led and its oldest turn left, so the prompt broke at its
// first message and everything behind identity and tools re-prefilled (Benchy on the M5,
// 2026-10-24 00:32Z: `delib.prompt.common_prefix` index 2 after a turnover, 20% shared; Delia on
// the IntelMac, 10-24 00:14Z: index 2 at `[actions]`, 41%). The settled blocks lead now,
// then her actions, whose collapsed items keep their pointers.
#[test]
fn consecutive_acts_share_the_whole_view_and_a_turnover_keeps_the_settled_front() {
    let memory = WorkingMemory::new(8);
    let memory_streams = MemoryStreams::of(&memory, uuid::Uuid::nil());
    let now = streams(0, "RECALL: one");
    let names: Vec<&str> =
        LlmDeliberationFaculty::view_sources(&now, Some(&memory_streams)).iter().map(|s| s.name()).collect();
    assert_eq!(
        names,
        ["workspace-map", "actions", "conversation", "room-kanban", "recall", "thoughts", "open-views", "notices"],
        "settled blocks, then her actions and conversation, then what counts or changes within an epoch"
    );

    let (kept, length, survived) = two_acts_and_a_turnover(true);
    assert_eq!(kept, length, "precondition: within an epoch every order appends");
    assert_eq!(survived, 0, "precondition: her conversation first, a turnover keeps none of her view");

    let (kept, length, survived) = two_acts_and_a_turnover(false);
    assert_eq!(kept, length, "the next act's prompt holds the whole previous view, then its tail");
    // The map and the `[actions]` banner at least: the first change is inside her actions (an
    // act the turnover collapsed, or the act the epoch had appended at its end).
    assert!(survived >= 2, "a turnover keeps the settled blocks and her actions' front: {survived}");
}
