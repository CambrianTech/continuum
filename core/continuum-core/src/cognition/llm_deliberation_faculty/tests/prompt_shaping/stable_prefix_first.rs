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

    // The order, most stable first: the conversation (dated history) leads;
    // then the standing grounding in churn order (STANDING map, then the
    // BOARD's held card and wall, by name, then the kanban's CLAIMS, which
    // change fastest of the three); then the per-turn facts;
    // then the clock + presence framing; then the ask, last.
    assert!(
        a.messages[0].content_text().starts_with("[occurred "),
        "the conversation leads the message list — nothing volatile ahead of it:\n{:#?}",
        bodies(&a)
    );
    let history_last = at(&a, "[occurred 2023-11-14T22:14:20.000Z] I saw it land.");
    let map = at(&a, "[workspace-map]");
    let held = at(&a, "[active-work]");
    let board = at(&a, "[room-kanban]");
    let wall = at(&a, "[room-wall]");
    let facts = at(&a, "[context]");
    let clock = at(&a, "[now ");
    let ask = at(&a, "[occurred 2023-11-14T22:15:20.000Z] Operator: what's next");
    assert!(
        history_last < map
            && map < held
            && held < wall
            && wall < board
            && board < facts
            && facts < clock
            && clock < ask,
        "order must be history < map < active-work < wall < kanban < facts < clock \
                     < ask, got {history_last} {map} {held} {wall} {board} {facts} {clock} \
                     {ask}:\n{:#?}",
        bodies(&a)
    );

    // No timestamp, no counter and no BOARD-churn block ahead of the
    // conversation: the clock is in the volatile tail, the wall and the board
    // ride behind the history, and the system message carries neither a date
    // nor the turn count.
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
        a.messages[..=history_last]
            .iter()
            .all(|m| m.content_text().starts_with("[occurred ")),
        "every message up to the end of history IS history — no clock, no fact, no \
                     grounding ahead of the conversation:\n{:#?}",
        bodies(&a)
    );
}
