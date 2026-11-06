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
// salience — so `[workspace-map]`, the held card and `[room-kanban]` traded
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
    let view_for = |order: [usize; 3], sal: [f32; 3]| {
        let bodies = [
            ("workspace-map", "MAP: src/ tests/ docs/"),
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
        let her = airc_core::PeerId::from_uuid(Uuid::from_u128(1));
        use crate::cognition::work_preconditions::{fixture_card, Held, FIXTURE_NOW_MS};
        let card = fixture_card(airc_work::CardState::InProgress, Some(her), None, "fix the thing");
        let held = crate::cognition::workspace::HeldWork {
            holds: Held::of(card, her, FIXTURE_NOW_MS, 1).into_iter().collect(),
            ..Default::default()
        };
        let mut ws = Workspace::new(Burst::from_turns(room, turns)).held_work(held);
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

    let a = view_for([0, 1, 2], [0.9, 0.3, 0.7]);
    // Re-ranked AND re-ordered arrival: the bytes must not move.
    let b = view_for([2, 1, 0], [0.3, 0.9, 0.5]);
    assert_eq!(a.system, b.system, "the system prefix is a function of the persona + ground");
    assert_eq!(
        bodies(&a),
        bodies(&b),
        "a re-ranked, re-ordered standing grounding must emit IDENTICAL messages"
    );

    // The order, most stable first: the settled grounding in churn order (STANDING map,
    // then the BOARD's wall), then her held work (one item per card, changing only on a
    // transition) lead her view; then the conversation
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
    let held = at(&a, "[my work]");
    let board = at(&a, "[room-kanban]");
    let wall = at(&a, "[room-wall]");
    let facts = at(&a, "[context]");
    let clock = at(&a, "[now ");
    let ask = at(&a, "[occurred 2023-11-14T22:15:20.000Z] Operator: what's next");
    assert!(
        map < wall
            && wall < held
            && held < history_first
            && history_last < board
            && board < facts
            && facts < clock
            && clock < ask,
        "order must be map < wall < my work < history < kanban < facts < clock < ask, \
                     got {map} {wall} {held} {history_first}..{history_last} {board} {facts} \
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
            ["[workspace-map]", "[my work]", "[room-wall]"].iter().any(|tag| text.starts_with(tag))
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

/// One act of hers as the act seam records it: her words and her native call, then its result.
fn acted(wm: &crate::cognition::working_memory::WorkingMemory, room: Uuid, seq: u64) {
    use crate::ai::types::{ResultCustody, ToolResult};
    use crate::cognition::act_observe::{ActStatus, Observation, ToolOutput, ToolVerb};
    let id = format!("call-{seq}");
    let call = ToolCall { id: id.clone(), name: "code/read".into(), input: json!({ "file_path": format!("src/f{seq}.rs") }) };
    wm.record_native_action_statement(&format!("Reading f{seq}.rs next."), room, std::slice::from_ref(&call));
    let content = format!("fn f{seq}() -> u32 {{ {seq} }}\n").repeat(40);
    let output = ToolOutput {
        image: None,
        additional_images: None,
        result: ToolResult { tool_use_id: id, content, is_error: None, custody: Default::default() },
        verb: ToolVerb::classify("code/read"),
        paths: Vec::new(),
        verdict: Default::default(),
    };
    let observation = Observation { call, output, status: ActStatus::Executed };
    let rendered = observation.render_recency();
    wm.record_receipt_typed(&[observation], &rendered, Some(room), ResultCustody::Whole { handle: format!("f{seq}-output") });
}

/// Each message as the wire carries it: role and content, byte for byte.
fn wire_of(messages: &[ChatMessage]) -> Vec<String> {
    messages.iter().map(|message| serde_json::to_string(message).expect("test: a message encodes")).collect() // JUSTIFIED: test fixture
}

// what this catches: the M5's re-prefill of every act's tail (core ed142f14c, 2026-11-06
// 16:13-19:17Z, 124 acts: 2,822 s over 78 acts in "changed inside her previous prompt", 2,101,494
// unshared chars in 117 of 158 recorded pairs). Her prompt ended with the facts, the clock, her
// words and her native call and result; the next act dropped the facts and re-rendered that result
// as a `[result #n]` item of her actions, so no prompt was ever the next one's prefix past her view.
// Served, a prompt is now the whole head of the next one: every message byte for byte, her result
// never rendered a second time, an unchanged fact never appended again, her words and her call one
// assistant turn as she wrote them.
#[test]
fn a_served_prompt_is_the_whole_head_of_her_next_act() {
    use crate::ai::types::{ContentPart, MessageContent};
    use crate::cognition::working_memory::WorkingMemory;

    let wm = Arc::new(WorkingMemory::new(8));
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let faculty = LlmDeliberationFaculty::new(Uuid::new_v4(), "Aris", "You are Aris.", adapter)
        .with_context_window(65_536)
        .with_working_memory(Arc::clone(&wm));
    let room = crate::identity::ActivityRoom::from_uuid(Uuid::new_v4()).expect("test: a room id");
    let ws = |minute: u64| {
        let turns = vec![
            BurstTurn::attributed(false, "Operator", "the parser test fails on main", Some(1_700_000_000_000)),
            BurstTurn::attributed(true, "Aris", "Looking at it.", Some(1_700_000_060_000)),
            BurstTurn::attributed(false, "Operator", "start with f1.rs", Some(1_700_000_120_000)),
        ];
        let mut ws = Workspace::new(Burst::from_turns(room, turns));
        ws.now_ms = Some(1_700_000_180_000 + 60_000 * minute);
        ws
    };
    let mut previous: Option<DeliberationPromptView> = None;
    for seq in 1..=3u64 {
        let workspace = ws(seq);
        acted(&wm, workspace.activity_room_id, seq);
        let now = faculty.prompt_view(&workspace);
        assert!(now.capacity_error.is_none() && now.budget_fault.is_none(), "{:?} {:?}", now.capacity_error, now.budget_fault);
        if let Some(before) = &previous {
            let (sent, next) = (wire_of(&before.messages), wire_of(&now.messages));
            let shared = sent.iter().zip(&next).take_while(|(a, b)| a == b).count();
            assert_eq!(shared, sent.len(), "act {seq}: the served prompt is not the next one's head (diverged at {shared} of {})", sent.len());
            assert_eq!(now.system, before.system, "the system message is the same bytes");
        }
        let bodies: Vec<String> = now.messages.iter().map(ChatMessage::content_text).collect();
        assert_eq!(bodies.iter().filter(|body| body.starts_with("[context]")).count(), 1, "an unchanged fact appends once:\n{bodies:#?}");
        assert_eq!(bodies.iter().filter(|body| body.ends_with("start with f1.rs")).count(), 1, "the ask is sent once:\n{bodies:#?}");
        let ring = |message: &&ChatMessage| matches!(&message.content, MessageContent::Text(text) if text.starts_with("[result #"));
        assert!(!now.messages.iter().any(|message| ring(&message)), "a served result arrived again as an item of her actions:\n{bodies:#?}");
        let turn = now.messages.iter().rev().find(|message| message.role == "assistant").expect("her newest turn");
        match &turn.content {
            MessageContent::Parts(parts) => {
                assert!(matches!(&parts[0], ContentPart::Text { text } if *text == format!("Reading f{seq}.rs next.")), "her words lead her turn: {parts:?}");
                assert!(matches!(&parts[1], ContentPart::ToolUse { id, .. } if *id == format!("call-{seq}")), "then her call: {parts:?}");
            }
            MessageContent::Text(text) => panic!("her call is native protocol, not text: {text}"),
        }
        assert_eq!(now.resumes_at(), Some(now.messages.len() as u32), "the engine checkpoints her prompt's end");
        faculty.view_served(&now);
        previous = Some(now);
    }
}
