#![cfg(test)]

// what this catches: a turn in her mind room landing in the wire capture (sink 4 of
// PRIVACY-OF-THOUGHT.md): zero bytes; an ordinary room still writes its row.
#[test]
fn a_turn_in_her_mind_room_writes_zero_wire_capture_bytes() {
    let dir = tempfile::tempdir().expect("wire capture dir");
    let persona = uuid::Uuid::new_v4();
    let request = crate::ai::types::TextGenerationRequest {
        messages: vec![crate::ai::types::ChatMessage::text("user", "a private thought")],
        ..Default::default()
    };
    let file = dir.path().join("Kimi.wire.jsonl");
    super::append_wire_capture(dir.path(), persona, "Kimi", crate::cognition::mind_room::mind_room_id(persona), &request);
    assert!(!file.exists(), "her mind room: no wire row");
    super::append_wire_capture(dir.path(), persona, "Kimi", uuid::Uuid::new_v4(), &request);
    assert!(std::fs::metadata(&file).expect("row written").len() > 0, "an ordinary room still captures");
}

// what this catches (the M5, 2026-10-29): the wire capture keeping only Text parts, so a
// native tool call or result (her newest result rides as one) was recorded as its label alone
// and read as a lost body. Every message must round-trip whole: role, name, and its typed parts
// in order with their ids and error flag.
#[test]
fn the_wire_capture_records_every_message_whole() {
    use crate::ai::types::{ChatMessage, ContentPart, MessageContent};
    let dir = tempfile::tempdir().expect("wire capture dir");
    let persona = uuid::Uuid::new_v4();
    let messages = vec![
        ChatMessage {
            role: "assistant".into(),
            content: MessageContent::Parts(vec![
                ContentPart::Text { text: "checking the tree".into() },
                ContentPart::ToolUse { id: "call-1".into(), name: "code/shell".into(), input: serde_json::json!({"cmd": "git status"}) },
            ]),
            name: Some("Aiko".into()),
        },
        ChatMessage {
            role: "tool".into(),
            content: MessageContent::Parts(vec![ContentPart::ToolResult {
                tool_use_id: "call-1".into(),
                content: "nothing to commit, working tree clean".into(),
                is_error: Some(false),
            }]),
            name: None,
        },
    ];
    let request = crate::ai::types::TextGenerationRequest { messages: messages.clone(), ..Default::default() };
    super::append_wire_capture(dir.path(), persona, "Aiko", uuid::Uuid::new_v4(), &request);
    let row: serde_json::Value = serde_json::from_str(std::fs::read_to_string(dir.path().join("Aiko.wire.jsonl")).expect("row written").trim()).expect("row is json");
    let recorded: Vec<ChatMessage> = serde_json::from_value(row["messages"].clone()).expect("messages deserialize");
    assert_eq!(serde_json::to_value(&recorded).expect("recorded"), serde_json::to_value(&messages).expect("sent"));
}

// what this catches (Kimi, 2026-09-25): a cut act must leave her a record of what
// she was composing and that it did not land; a long payload is trimmed to its two
// edges so the record cannot crowd the retry the way the payload did.
#[test]
fn a_cut_act_records_that_it_did_not_land_and_what_it_was() {
    let short = cut_act_fact("work/note({\"card\": \"a9c8f8ae\"");
    assert!(short.contains("did not land"), "{short}");
    assert!(short.contains("a9c8f8ae"), "{short}");
    let long: String = format!("HEAD{}TAIL", "x".repeat(5_000));
    let fact = cut_act_fact(&long);
    assert!(fact.contains("after 5008 chars"), "{fact}");
    assert!(fact.starts_with("my last act was CUT"), "{fact}");
    assert!(fact.contains("HEAD") && fact.contains("TAIL"), "both edges survive");
    assert!(fact.chars().count() < 800, "the record is bounded, not the payload: {}", fact.chars().count());
}

use super::*;
use super::verdict::*;
use crate::cognition::tool_surface::{core_hands, hands_surface, is_extended_hand};

// what this catches: the hands surface chosen on WIRE names — `code/` prefixes
// matched nothing after the dialect renamed them and every work turn went out
// with zero tools (2026-09-04). The filter runs on the raw command names.
#[test]
fn hands_surface_is_chosen_on_command_names_and_keeps_the_discovery_pair() {
    let raw: Vec<NativeToolSpec> = [
        "code/read",
        "work/state",
        "chat/send",
        "commands/list",
        "room/join",
        "code/git/status",
        "code/git/apply",
        "work/submission",
        "work/review",
        "work/submit",
        "web/fetch",
        "focus/continue",
    ]
    .iter()
    .map(|n| NativeToolSpec {
        name: (*n).to_string(),
        description: String::new(),
        input_schema: crate::ai::types::ToolInputSchema {
            schema_type: "object".to_string(),
            properties: serde_json::json!({}),
            required: None,
            definitions: None,
        },
    })
    .collect();
    let hands: Vec<String> = hands_surface(&raw).into_iter().map(|s| s.name).collect();
    // A capable citizen (an unknown level counts as capable: more, not less) also gets
    // the web, like Claude (Joel, 2026-09-28); her rooms and chat are hands, and so is all
    // of work/**, the reviewer verbs included: offering those only while she held a review
    // card made her card state an input to the surface (goal 10).
    assert_eq!(
        hands,
        [
            "code/read",
            "work/state",
            "chat/send",
            "commands/list",
            "room/join",
            "code/git/status",
            "work/submission",
            "work/review",
            "work/submit",
            "web/fetch",
            "focus/continue"
        ],
        "git/apply takes a peer's diff and stays one commands/help away; every other verb here is hers"
    );
    // what this also catches: a window too small for the extended verbs keeps her CORE
    // hands (never nothing), with the web and the push/PR verbs one commands/list away.
    // focus/continue is EXTENDED since #4758: the survival window keeps the newest line
    // over her self-determination verbs, as it does over the web.
    let core: Vec<String> = core_hands(&raw).into_iter().map(|s| s.name).collect();
    assert_eq!(core, ["code/read", "work/state", "commands/list", "code/git/status", "work/submit"]);
    // regression (Idris, 2026-09-29): a verb and the verb that finishes it travel
    // together. code/shell tells her to poll a running execution with code/shell-poll,
    // so a window that carries shell carries shell-poll.
    assert!(!is_extended_hand("code/shell-poll"));
    assert!(!is_extended_hand("code/shell"));
}

use crate::ai::heuristic_adapter::HeuristicInferenceAdapter;

// what this catches: her INITIATIVE turn (holding nothing, offered by her own heartbeat)
// going out as a message turn: the message surface and the PASS allowance, so she could
// talk about work but never do it. It must take exactly a work turn's surface, reason and
// ACT kind without being a deliverable turn, and those hands must keep the verbs that let
// her choose work for herself: find it, take it, start it, write it, land it.
#[test]
fn an_initiative_turn_takes_a_work_turns_hands_and_act_kind() {
    let own_work = [
        "work/list",
        "work/claim",
        "work/create",
        "code/write",
        "code/edit",
        "code/git/commit",
        "code/git/push",
        "code/github/pr-create",
        "room/join",
        "chat/send",
    ];
    let raw: Vec<NativeToolSpec> = own_work
        .iter()
        .map(|n| NativeToolSpec {
            name: (*n).to_string(),
            description: String::new(),
            input_schema: crate::ai::types::ToolInputSchema {
                schema_type: "object".to_string(),
                properties: serde_json::json!({}),
                required: None,
                definitions: None,
            },
        })
        .collect();
    let hands: Vec<String> = hands_surface(&raw).into_iter().map(|s| s.name).collect();
    assert_eq!(hands, own_work, "every verb of choosing and doing her own work is in her hands");

    let window = 65_536;
    let faculty = LlmDeliberationFaculty::new(
        uuid::Uuid::new_v4(),
        "Aris",
        "You are Aris.",
        std::sync::Arc::new(HeuristicInferenceAdapter::new()),
    )
    .with_context_window(window)
    .with_tools(vec![super::test_support::read_tool()]);
    let room = crate::identity::ActivityRoom::mint();
    let work = super::test_support::writing_workspace(room, 0, true);
    let mut initiative = super::test_support::writing_workspace(room, 0, false);
    initiative.initiative = true;
    assert!(!initiative.workspace_deliverable, "an initiative is not a deliverable turn");
    assert_eq!(faculty.turn_kind(&initiative), faculty.turn_kind(&work), "the act allowance");
    let (i, w) = (faculty.prompt_view(&initiative).tools, faculty.prompt_view(&work).tools);
    assert_eq!((i.reason, i.specs), (w.reason, w.specs), "one surface for every kind of turn");
}

use crate::ai::types::{ToolCall, ToolInputSchema};

use crate::cognition::workspace::BurstTurn;

use serde_json::json;

use std::sync::Mutex;

// ─── Prompt shaping ───────────────────────────────────────────────────────
//
// These drive a real adapter but assert on the COMPOSED PROMPT (the glass box
// over what the faculty sends the model) — window budgeting, KV-prefix
// ordering, the bookmarked tool menu, role attribution, own-time / directed
// framing — never on the generated text.
mod prompt_shaping;

// ─── Verdict production (single-shot) ─────────────────────────────────────
//
// The faculty is SINGLE-SHOT: one generation → one verdict. When the model
// chooses to use a tool, the faculty surfaces a `Decision::Act` — it does NOT
// execute. Executing the calls, remembering the result, and re-perceiving is
// the organism's job (the act→observe driver, `super::act_observe`), tested
// there. These tests prove the faculty turns a tool-use response (native OR
// text-emitted JSON) into an `Act` verdict, and prose into Speak/Pass. A
// `ScriptedAdapter` replays canned responses + records the requests it saw.
mod verdicts;

mod turn_bound;

// what this catches (6780e79b): a remote call never polls local admission; a local call is
// dispatched without taking a permit (the adapter's turn admission and the slot pool are the
// one queue in front of the engine, SERVING-ENGINE §1c).
#[tokio::test]
async fn remote_admission_bypasses_local_and_local_takes_no_permit() {
    use super::admission::{admit_target, GenerationAdmission};
    use crate::ai::adapter::GenerationAdmissionOwner;
    use std::future::Future;
    use std::task::{Context, Poll, Waker};

    let polls = std::cell::Cell::new(0);
    let local = || async {
        polls.set(polls.get() + 1);
        Some(GenerationAdmission::SharedLocal)
    };
    let mut cx = Context::from_waker(Waker::noop());
    let mut remote = Box::pin(admit_target(GenerationAdmissionOwner::Adapter, local()));
    assert!(matches!(remote.as_mut().poll(&mut cx), Poll::Ready(Some(GenerationAdmission::AdapterOwned))));
    assert_eq!(polls.get(), 0, "remote dispatch must not enter the local path");
    drop(remote);
    let mut home = Box::pin(admit_target(GenerationAdmissionOwner::SharedLocal, local()));
    assert!(matches!(home.as_mut().poll(&mut cx), Poll::Ready(Some(GenerationAdmission::SharedLocal))), "local dispatch is ready at once: nothing to wait on");
}
