#![cfg(test)]

use super::*;

use crate::cognition::workspace::Burst;

fn bodies(v: &DeliberationPromptView) -> Vec<String> {
    v.messages.iter().map(|m| m.content_text()).collect()
}

fn at(v: &DeliberationPromptView, needle: &str) -> usize {
    v.messages
        .iter()
        .position(|m| m.content_text().starts_with(needle))
        .unwrap_or_else(|| panic!("{needle} missing from:\n{:#?}", bodies(v)))
}

// what this catches (card c119ace7): the standing grounding that rides the
// conversation tail rendered in BROADCAST order — the arbiter's per-turn
// salience — so `[workspace-map]`, `[active-work]` and `[room-kanban]` traded
// places turn to turn (both orders are in the M5's 2026-09-20 captures), and
// the prefix cache lost every byte behind the swap although none had changed.
// Same set in, same bytes out, whatever attention ranked and whatever order
// the faculties bid in; and the order is the churn order, standing → board.
#[test]
fn standing_grounding_renders_in_one_canonical_order_and_the_clock_rides_last() {
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let faculty =
        LlmDeliberationFaculty::new(Uuid::new_v4(), "Ivar", "You are Ivar.", adapter)
            .with_context_window(16_384);
    let room_uuid = Uuid::new_v4();
    let view_for = |order: [usize; 4], sal: [f32; 4]| {
        let bodies = [
            ("workspace-map", "MAP: src/ tests/ docs/"),
            ("active-work", "HELD: card 1234 [InProgress] \"fix the thing\""),
            ("room-kanban", "BOARD: you hold 1 card here; 3 claimable"),
            ("room-wall", "WALL: card 1234 ledger — known: the repro is in tests/"),
        ];
        let room = crate::identity::ActivityRoom::from_uuid(room_uuid).unwrap();
        let turns = vec![
            BurstTurn::attributed(
                false,
                "Operator",
                "we shipped the fix yesterday",
                Some(1_700_000_000_000),
            ),
            BurstTurn::attributed(true, "Ivar", "I saw it land.", Some(1_700_000_060_000)),
            BurstTurn::attributed(
                false,
                "Operator",
                "what's next on the board?",
                Some(1_700_000_120_000),
            ),
        ];
        let mut ws = Workspace::new(Burst::from_turns(room, turns));
        ws.now_ms = Some(1_700_000_180_000);
        for &i in order.iter() {
            ws.broadcast.push(
                Contribution::context(
                    FacultyId::Custom(bodies[i].0.to_string()),
                    bodies[i].1,
                    sal[i],
                    "framing",
                )
                .standing_grounding()
                .trailing(),
            );
        }
        faculty.prompt_view(&ws)
    };

    let a = view_for([0, 1, 2, 3], [0.9, 0.5, 0.3, 0.7]);
    // Re-ranked AND re-ordered arrival: the bytes must not move.
    let b = view_for([3, 2, 0, 1], [0.3, 0.9, 0.7, 0.5]);
    assert_eq!(a.system, b.system, "the system prefix is a function of the persona + ground");
    assert_eq!(
        bodies(&a),
        bodies(&b),
        "a re-ranked, re-ordered standing grounding must emit IDENTICAL messages"
    );

    // The order, most stable first: the settled grounding in churn order (STANDING map,
    // then the BOARD's held card and wall, by name) leads her view; then the conversation
    // (dated history, which grows and collapses at its front); then the kanban's CLAIMS,
    // which count and change fastest; then the per-turn facts; then the clock + presence
    // framing; then the ask, last. Within an epoch every act appends, so the
    // order decides only what a turnover keeps: the prefix up to the first source whose
    // front changed (`view_sources`).
    assert!(
        a.messages[0].content_text().starts_with("[workspace-map]"),
        "the settled grounding leads the message list — nothing growing ahead of it:\n{:#?}",
        bodies(&a)
    );
    let history_first = at(&a, "[occurred 2023-11-14T22:13:20.000Z] Operator: we shipped");
    let history_last = at(&a, "[occurred 2023-11-14T22:14:20.000Z] I saw it land.");
    let map = at(&a, "[workspace-map]");
    let held = at(&a, "[active-work]");
    let board = at(&a, "[room-kanban]");
    let wall = at(&a, "[room-wall]");
    let facts = at(&a, "[context]");
    let clock = at(&a, "[now ");
    let ask = at(&a, "[occurred 2023-11-14T22:15:20.000Z] Operator: what's next");
    assert!(
        map < held
            && held < wall
            && wall < history_first
            && history_last < board
            && board < facts
            && facts < clock
            && clock < ask,
        "order must be map < active-work < wall < history < kanban < facts < clock < ask, \
                     got {map} {held} {wall} {history_first}..{history_last} {board} {facts} \
                     {clock} {ask}:\n{:#?}",
        bodies(&a)
    );

    // No timestamp and no counter in the system message: the clock is in the
    // volatile tail, the wall and the board ride her view, and the system message
    // carries neither a date nor the turn count.
    for needle in [
        "[now ",
        "2023-11",
        "input turns were available",
        "[room-wall]",
        "[room-kanban]",
    ] {
        assert!(
            !a.system.contains(needle),
            "`{needle}` must not sit in the cacheable system message:\n{}",
            a.system
        );
    }
    assert!(
        a.messages[..history_first].iter().all(|m| {
            let text = m.content_text();
            ["[workspace-map]", "[active-work]", "[room-wall]"].iter().any(|tag| text.starts_with(tag))
        }),
        "every message ahead of the history is settled grounding — no clock, no fact, no \
                     counter, nothing that grows:\n{:#?}",
        bodies(&a)
    );
}

// what this catches: a section that grows every act rendered ahead of what is stable, so each
// act rewrote the head of her prompt and re-prefilled everything behind it (Delia on the
// IntelMac, 2026-10-24 00:14Z: `[actions]` at `delib.prompt.common_prefix` index 2, 41%
// reused). Two consecutive acts, each adding a result, a room turn and new recall: the second
// prompt's system message and its whole leading view are the first's, byte for byte, and
// the act appended to that view rather than rewriting it.
#[test]
fn the_next_act_holds_the_whole_previous_view_as_its_head() {
    use crate::cognition::working_memory::WorkingMemory;

    let wm = Arc::new(WorkingMemory::new(8));
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let faculty = LlmDeliberationFaculty::new(Uuid::new_v4(), "Ivar", "You are Ivar.", adapter)
        .with_context_window(32_768)
        .with_working_memory(Arc::clone(&wm));
    let room = crate::identity::ActivityRoom::from_uuid(Uuid::new_v4()).expect("test: a room id");
    let act = |count: usize, recall: &str| {
        let lines = ["we shipped the fix yesterday", "I saw it land.", "what's next on the board?", "the parser first"];
        let turns: Vec<BurstTurn> = lines[..count]
            .iter()
            .enumerate()
            .map(|(i, line)| BurstTurn::attributed(i % 2 == 1, if i % 2 == 1 { "Ivar" } else { "Operator" }, *line, Some(1_700_000_000_000 + 60_000 * i as u64)))
            .collect();
        let mut ws = Workspace::new(Burst::from_turns(room, turns));
        ws.now_ms = Some(1_700_000_000_000 + 60_000 * count as u64);
        for (faculty, body) in [("workspace-map", "MAP: src/ tests/ docs/"), ("room-kanban", "BOARD: you hold 1 card here; 3 claimable")] {
            ws.broadcast.push(Contribution::context(FacultyId::Custom(faculty.to_string()), body, 0.9, "framing").standing_grounding().trailing());
        }
        ws.broadcast.push(Contribution::context(FacultyId::Recall, recall, 0.6, "recall").trailing());
        faculty.prompt_view(&ws)
    };

    wm.record_receipt("I ran code/list({\"path\":\".\"}) Result: 4 files");
    wm.record_settlement("listed", None);
    let first = act(3, "RECALL: the repro lives in tests/");
    wm.record_receipt("I ran code/read({\"path\":\"main.rs\"}) Result: 120 lines");
    wm.record_settlement("read it", None);
    let second = act(4, "RECALL: main.rs calls the parser");

    let head = first.refit.grounding_at;
    assert!(head > 0, "precondition: her view leads the messages:\n{:#?}", bodies(&first));
    assert!(bodies(&first)[0].starts_with("[workspace-map]"), "settled grounding leads:\n{:#?}", bodies(&first));
    assert_eq!(first.system, second.system, "the system message is the same bytes");
    assert_eq!(
        bodies(&first)[..head],
        bodies(&second)[..head],
        "the next act's prompt holds the whole previous view as its head"
    );
    assert!(second.refit.grounding_at > head, "the act appended to her view");
}
