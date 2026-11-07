//! Her view's source order ([`LlmDeliberationFaculty::view_sources`]): consecutive acts share
//! the whole view, and a turnover keeps the prefix up to the first source whose front changed.
#![cfg(test)]

use super::*;
use crate::cognition::context_streams::blocks::Grounding;
use crate::cognition::context_streams::conversation::{Conversation, Turn};
use crate::cognition::context_streams::Role;

// A source's average full-message density is not necessarily the density of its
// retained newest messages. Exercise the actual start and final message pricing.
#[test]
fn a_view_start_keeps_its_token_grant_with_uneven_message_sizes() {
    use continuum_context::flex::{AllocationState, FlexAllocation, SourceAllocation};
    let conversation = Conversation {
        turns: (0..40).map(|n| Turn {
            key: format!("density-{n}"),
            role: Role::User,
            line: if n < 20 { "long old message ".repeat(100) } else { "ok".to_owned() },
            pointer: None,
        }).collect(),
        ..Default::default()
    };
    let items = conversation.items();
    let full: usize = items.iter().map(|item| LlmDeliberationFaculty::message_cost(&item.full().to_chat())).sum();
    for granted in [32, 64, 128, 256, 512] {
        let plan = FlexAllocation {
            allocations: vec![SourceAllocation {
                source_id: format!("{VIEW_CLAIM}conversation"),
                allocated_tokens: granted,
                requested_floor: 0,
                requested_min: 0,
                requested_max: full as u32,
                required: false,
                state: AllocationState::Satisfied,
            }],
            unallocated: 0,
            escalation_needed: false,
            warnings: Vec::new(),
        };
        let sources: Vec<&dyn Source<ContextMessage>> = vec![&conversation];
        let (view, _) = View::start(Vec::new(), &sources, |claims| {
            start_grants(claims, &plan, granted as usize, None).expect("funded optional source")
        }).expect("matching source grants");
        let rendered: Vec<_> = view.render().map(ContextMessage::to_chat).collect();
        let actual = LlmDeliberationFaculty::messages_cost(&rendered);
        assert!(actual <= granted as usize, "start rendered {actual} tokens against grant {granted}");
    }
}

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
        ..Default::default()
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
        ["workspace-map", "held-work", "actions", "conversation", "room-kanban", "recall", "thoughts", "tasks", "open-views", "notices", "facts"],
        "settled blocks and her held work (it changes only on a card's transition), then her actions and conversation, then what counts or changes within an epoch, then what her situation shows now, right before the act's tail"
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

// what this catches: a view that fits turning over, cold, to raise a collapsed item a level
// (the M5, 2026-10-31: 32 of 91 turnovers in 3 h, each 101-111 s of prefill instead of 6-8 s).
// A restore is taken only on an act whose next append would turn the epoch over anyway, and
// only then is a whole start priced; an overflow always turns over.
#[test]
fn a_fitting_view_restores_only_when_the_next_act_would_turn_it_over() {
    let never = || -> Result<bool, ()> { panic!("a start is priced only when the next act would overflow") };
    assert_eq!(turnover_due(21_000, 32_000, Some(5_000), never), Ok(Turnover::Keep), "room for the next act: keep appending");
    assert_eq!(turnover_due(21_000, 32_000, None, never), Ok(Turnover::Keep), "nothing measured yet: keep appending");
    assert_eq!(turnover_due(30_000, 32_000, Some(5_000), || Ok::<_, ()>(true)), Ok(Turnover::Restore), "the next act overflows: restore now");
    assert_eq!(turnover_due(30_000, 32_000, Some(5_000), || Ok::<_, ()>(false)), Ok(Turnover::Keep), "nothing to restore: keep");
    assert_eq!(turnover_due(33_000, 32_000, Some(5_000), never), Ok(Turnover::Overflow), "over its grant: turn over");
    assert_eq!(turnover_due(30_000, 32_000, Some(5_000), || Err("mismatch")), Err("mismatch"), "a broken allocation refuses");
}

// what this catches: a turn sized before her window is known (granted nothing) turning her view
// over to an empty start, after which the next act re-appends every item her sources hold and
// turns over again (2026-11-02, every M5 resident at boot). With nothing allowed the view is kept.
#[test]
fn a_view_granted_nothing_is_kept_never_started_empty() {
    let due = turnover_due::<()>(33_165, 0, Some(10_000), || Ok(true)).expect("pure");
    assert_eq!(due, Turnover::Keep, "no grant is not an overflow");
    let due = turnover_due::<()>(33_165, 29_000, Some(10_000), || Ok(true)).expect("pure");
    assert_eq!(due, Turnover::Overflow, "a real grant it outgrew still turns over");
}

// what this catches (Aris on the M5, 2026-11-04 16:55-17:16Z): a view of 26-37k tokens appending
// 5-12k an act against a ~44k allowance turned over on most acts, and her need, measured only from
// sizes her grant bounded, never rose past the grant. An overflow measures what she would have
// held: the view plus the next act's appends, beyond what was allowed. A deliberate start (her
// whole history rendered at once) and an act that kept or restored within its grant measure none.
#[test]
fn an_overflow_measures_what_her_view_would_have_held() {
    let (allowed, view, next) = (44_000, 46_000, 9_000);
    let due = turnover_due::<()>(view, allowed, Some(next), || Ok(false)).expect("pure");
    assert_eq!(due, Turnover::Overflow, "precondition: over its grant");
    let beyond = overflow_beyond(due, false, view, allowed, Some(next)).expect("an overflow measures a shortfall");
    assert!(allowed + beyond >= view + next, "need >= view + one act's appends: {beyond}");
    assert_eq!(overflow_beyond(due, false, view, allowed, None), Some(view - allowed), "no append measured yet: the view alone");
    assert_eq!(overflow_beyond(due, true, view, allowed, Some(next)), None, "a start renders her whole history: no demand");
    let restore = turnover_due::<()>(40_000, allowed, Some(next), || Ok(true)).expect("pure");
    assert_eq!(restore, Turnover::Restore, "precondition: a restore within the grant");
    assert_eq!(overflow_beyond(restore, false, 40_000, allowed, Some(next)), None, "a restore fit: no shortfall");
    assert_eq!(overflow_beyond(Turnover::Keep, false, 40_000, allowed, Some(next)), None, "a kept view fit: no shortfall");
}

/// One growing source whose measured depth prices her newest item a token under
/// `StartSource::newest`: at 0.01 tokens per char a 501-char newest item is 5.01 tokens (ceiled to
/// 6), and the depth's base, her 20-char floor with it (521 chars), rounds to 5. Its one older item
/// is measured as never asked for again, so the depth collapses it and starts the source at its base.
fn depth_one_token_under_her_newest() -> (Vec<continuum_context::view::Claim>, continuum_context::flex::FlexAllocation) {
    use continuum_context::depth::Collapsible;
    use continuum_context::rereads::{Hazard, Tally};
    let claim = continuum_context::view::Claim {
        source: "results".to_owned(),
        priority: 0,
        floor_chars: 20,
        full_chars: 1_000,
        newest_chars: 501,
        logged_chars: 0,
        instrumented: true,
        collapsible: vec![Collapsible { whole_chars: 489, saves_chars: 479, age: 1, hazard: Hazard::Measured(Tally { exposure: 1_000.0, events: 0.0 }) }],
    };
    let grant = continuum_context::flex::SourceAllocation {
        source_id: format!("{VIEW_CLAIM}results"),
        allocated_tokens: 10,
        requested_floor: 1,
        requested_min: 1,
        requested_max: 10,
        required: false,
        state: continuum_context::flex::AllocationState::Satisfied,
    };
    let plan = continuum_context::flex::FlexAllocation { allocations: vec![grant], unallocated: 0, escalation_needed: false, warnings: Vec::new() };
    (vec![claim], plan)
}

// what this catches: #5807's failure again, through the measured depth: her newest turn collapsed
// by a char (`reply_priority_grows_the_reply_not_at_history_or_required_input`, 2026-11-06). The
// depth prices her newest item in rounded tokens and `StartSource::newest` in ceiled ones, so a
// depth that lowered her grant, or sized her start, to its own sum started her a token (ten chars)
// under her newest item whole.
#[test]
fn a_measured_depth_never_starts_her_under_her_newest_item() {
    let (claims, plan) = depth_one_token_under_her_newest();
    let grants = start_grants(&claims, &plan, 100, Some(50)).expect("an optional source is never refused");
    assert!(grants[0] >= claims[0].newest_chars, "her newest item whole: granted {} of {} chars", grants[0], claims[0].newest_chars);
    // The lowered grant itself, at the fixture's tokens: the depth chose 5, her newest needs 6.
    let source = continuum_context::allocation::StartSource { priority: 0, fixed: false, required: false, floor: 1, grant: 10, newest: 6 };
    assert_eq!(depth_grant(5, &source), 6, "never below her newest item");
    assert_eq!(depth_grant(0, &continuum_context::allocation::StartSource { floor: 7, ..source }), 7, "never below its floor");
    assert_eq!(depth_grant(40, &source), 10, "never above the plan's grant");
}
