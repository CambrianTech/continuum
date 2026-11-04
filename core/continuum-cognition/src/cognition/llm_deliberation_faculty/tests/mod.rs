#![cfg(test)]

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

// what this catches (6780e79b): remote calls never poll local lane admission; falling home
// waits for the lane, and cancelling a waiting admission leaves the lane reusable.
#[tokio::test]
async fn remote_admission_bypasses_busy_local_resources_and_home_reacquires() {
    use super::admission::{admit_target, GenerationAdmission};
    use crate::ai::adapter::GenerationAdmissionOwner;
    use crate::inference::resource_admission::{LaneAdmission, LanePriority};
    use std::future::Future;
    use std::task::{Context, Poll, Waker};

    let lanes: &'static LaneAdmission = Box::leak(Box::new(LaneAdmission::new()));
    lanes.set_served_lane_count(1);
    let held_lane = lanes.acquire_serving_lane(None, LanePriority::Directed, None).await;
    let polls = std::cell::Cell::new(0);
    let local = || async {
        polls.set(polls.get() + 1);
        let lane = lanes.acquire_serving_lane(None, LanePriority::Directed, None).await;
        Some(GenerationAdmission::SharedLocal { _lane: lane })
    };
    let mut cx = Context::from_waker(Waker::noop());
    let mut remote = Box::pin(admit_target(GenerationAdmissionOwner::Adapter, local()));
    assert!(matches!(remote.as_mut().poll(&mut cx), Poll::Ready(Some(GenerationAdmission::AdapterOwned))));
    assert_eq!(polls.get(), 0, "remote dispatch must not enter the local queue");
    drop(remote);

    let mut home = Box::pin(admit_target(GenerationAdmissionOwner::SharedLocal, local()));
    assert!(home.as_mut().poll(&mut cx).is_pending(), "local lane is occupied");
    drop(home); // cancelled while waiting for the lane
    drop(held_lane);
    let admission = admit_target(GenerationAdmissionOwner::SharedLocal, local()).await;
    assert!(matches!(admission, Some(GenerationAdmission::SharedLocal { .. })));
    drop(admission);
    // A completed call leaves the lane reusable.
    let next = admit_target(GenerationAdmissionOwner::SharedLocal, local()).await;
    assert!(matches!(next, Some(GenerationAdmission::SharedLocal { .. })));
}

// what this catches (Atlas on the M5, 2026-11-04 07:09:18Z): a cycle that did not gather
// `[room-wall]` rebuilt her view's front without it; 18 s later it was back at message 2 and
// 13,667 tokens re-prefilled. A block not gathered keeps its place; one whose faculty ran and
// said nothing leaves; another room's block is never carried.
#[test]
fn a_standing_block_not_gathered_this_cycle_keeps_its_place_until_its_faculty_ends() {
    use crate::cognition::context_streams::blocks::Grounding;
    let faculty = LlmDeliberationFaculty::new(
        uuid::Uuid::new_v4(),
        "Atlas",
        "You are Atlas.",
        std::sync::Arc::new(HeuristicInferenceAdapter::new()),
    );
    let block = |name: &str| Grounding::atomic(uuid::Uuid::nil(), name, format!("{name} body"));
    let names = |blocks: &[Grounding]| blocks.iter().map(|b| b.faculty.clone()).collect::<Vec<_>>();
    let (room, other) = (uuid::Uuid::new_v4(), uuid::Uuid::new_v4());

    let first = faculty.carry_held_blocks(room, vec![block("room-wall"), block("room-kanban")], &[]);
    let missed = faculty.carry_held_blocks(room, vec![block("room-kanban")], &[]);
    assert_eq!(names(&missed), names(&first), "the wall this cycle did not gather keeps its place");

    let ended = faculty.carry_held_blocks(room, vec![block("room-kanban")], &["room-wall"]);
    assert_eq!(names(&ended), ["room-kanban"], "a faculty that ran and said nothing has ended");

    let elsewhere = faculty.carry_held_blocks(other, vec![], &[]);
    assert!(elsewhere.is_empty(), "another room's blocks are never carried");
}
