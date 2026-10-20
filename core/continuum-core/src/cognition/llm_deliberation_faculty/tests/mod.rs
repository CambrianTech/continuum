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
    super::append_wire_capture(dir.path(), persona, "Kimi", crate::persona::mind_room::mind_room_id(persona), &request);
    assert!(!file.exists(), "her mind room: no wire row");
    super::append_wire_capture(dir.path(), persona, "Kimi", uuid::Uuid::new_v4(), &request);
    assert!(std::fs::metadata(&file).expect("row written").len() > 0, "an ordinary room still captures");
}

// what this catches (Kimi, 2026-09-25): a cut act must leave her a record of what
// she was composing and that it did not land; a long payload is trimmed to its two
// edges so the record cannot crowd the retry the way the payload did.
#[test]
fn a_cut_act_records_that_it_did_not_land_and_what_it_was() {
    let short = super::verdict::cut_act_fact("work/note({\"card\": \"a9c8f8ae\"");
    assert!(short.contains("did not land"), "{short}");
    assert!(short.contains("a9c8f8ae"), "{short}");
    let long: String = format!("HEAD{}TAIL", "x".repeat(5_000));
    let fact = super::verdict::cut_act_fact(&long);
    assert!(fact.contains("after 5008 chars"), "{fact}");
    assert!(fact.starts_with("my last act was CUT"), "{fact}");
    assert!(fact.contains("HEAD") && fact.contains("TAIL"), "both edges survive");
    assert!(fact.chars().count() < 800, "the record is bounded, not the payload: {}", fact.chars().count());
}

use super::*;

// what this catches: S1's whole point. A room whose recipe declares affordances
// gets EXACTLY those native verbs plus the discovery pair — the recipe is the
// rule and the rule reaches her hands. Before this, `affordances` rode into the
// manifest and every room offered the same prefix-filtered global list.
#[test]
fn a_room_that_declares_affordances_selects_exactly_those_plus_the_discovery_pair() {
    let faculty = LlmDeliberationFaculty::new(
        Uuid::new_v4(),
        "Asha",
        "You are Asha.",
        Arc::new(HeuristicInferenceAdapter::new()) as Arc<dyn AIProviderAdapter>,
    )
    .with_context_window(65_536)
    .with_tools(persona_tools::native_tool_specs());
    let mut ws = Workspace::new("find the docs for this API");
    ws.room_affordances = vec!["web/fetch".to_string(), "web/search".to_string()];

    let selected = faculty.select_tool_surface(&ws, 65_536);
    assert_eq!(selected.reason, SurfaceReason::RoomAffordances);
    let offered = selected.specs.as_deref().expect("a surface was offered");
    let names: Vec<&str> = faculty
        .native_command_names
        .iter()
        .zip(faculty.native_specs.iter())
        .filter(|(_, spec)| offered.iter().any(|o| o.name == spec.name))
        .map(|(raw, _)| raw.as_str())
        .collect();
    for must in ["web/fetch", "web/search", "commands/list", "commands/help", "focus/continue"] {
        assert!(
            names.contains(&must),
            "room-selected surface must carry {must}: {names:?}"
        );
    }
    assert!(
        !names.iter().any(|n| n.starts_with("code/")),
        "a room that did not grant code/* must not offer it: {names:?}"
    );
    assert_eq!(
        selected.tokens,
        LlmDeliberationFaculty::tool_surface_tokens_of(offered),
        "the price is the price of what was sent"
    );
}

// what this catches: a REVIEW card's holder offered hands without the verbs her card
// exists for. Kimi, holding review card deec4ac2 (2026-10-06): "work/review is not in
// this prompt", and a native-tool model can call only what it was offered. An ordinary
// holder's hands still withhold both verbs (the 9/19 "review the spec" misfires).
#[test]
fn a_review_cards_work_turn_carries_the_reviewer_verbs_and_an_ordinary_one_does_not() {
    let faculty = LlmDeliberationFaculty::new(
        Uuid::new_v4(),
        "Kimi",
        "You are Kimi.",
        Arc::new(HeuristicInferenceAdapter::new()) as Arc<dyn AIProviderAdapter>,
    )
    .with_context_window(65_536)
    .with_tools(persona_tools::native_tool_specs());
    let reviewer_specs: Vec<&NativeToolSpec> = faculty
        .native_command_names
        .iter()
        .zip(faculty.native_specs.iter())
        .filter(|(name, _)| crate::cognition::tool_dialect::reviewer_hand(name))
        .map(|(_, spec)| spec)
        .collect();
    assert_eq!(reviewer_specs.len(), 2, "work/review and work/submission are her native verbs");

    let mut holder = Workspace::new("fix the failing test");
    holder.workspace_deliverable = true;
    let mut reviewer = holder.clone();
    reviewer.reviewing = true;

    let held = faculty.select_tool_surface(&holder, 65_536);
    let reviewing = faculty.select_tool_surface(&reviewer, 65_536);
    assert_eq!(reviewing.reason, SurfaceReason::HandsForWork);
    let offered = |s: &SelectedSurface<'_>, spec: &NativeToolSpec| {
        s.specs.as_deref().is_some_and(|specs| specs.contains(spec))
    };
    for spec in &reviewer_specs {
        assert!(offered(&reviewing, spec), "the reviewer is offered {}", spec.name);
        assert!(!offered(&held, spec), "an ordinary holder is not offered {}", spec.name);
    }
    assert_eq!(
        reviewing.tokens,
        LlmDeliberationFaculty::tool_surface_tokens_of(reviewing.specs.as_deref().expect("specs")),
        "the price is the price of what was sent"
    );
}

// what this catches: the regression S1 must not cause. A room whose recipe
// declares NO affordances — every chat, profile, theme and settings page today —
// must get a surface byte-identical to the pre-S1 one on every arm, or the slice
// has amputated hands nobody asked it to touch.
#[test]
fn a_room_that_declares_nothing_is_byte_identical_to_the_pre_s1_surface() {
    let faculty = LlmDeliberationFaculty::new(
        Uuid::new_v4(),
        "Asha",
        "You are Asha.",
        Arc::new(HeuristicInferenceAdapter::new()) as Arc<dyn AIProviderAdapter>,
    )
    .with_context_window(65_536)
    .with_tools(persona_tools::native_tool_specs());
    let mut work = Workspace::new("fix the failing test");
    work.workspace_deliverable = true;
    for (ws, want) in [
        (Workspace::new("anything open?"), SurfaceReason::Full),
        (work, SurfaceReason::HandsForWork),
    ] {
        assert!(
            ws.room_affordances.is_empty(),
            "the ordinary room declares none"
        );
        let selected = faculty.select_tool_surface(&ws, 65_536);
        assert_eq!(selected.reason, want);
        let expected: &[NativeToolSpec] = match want {
            SurfaceReason::Full => &faculty.native_specs,
            _ => &faculty.hands_specs,
        };
        assert_eq!(selected.specs.as_deref(), Some(expected));
    }
}

// what this catches: a rule that binds nothing must never bind to NOTHING. A
// recipe naming only verbs this node has no native tool for (`mail/send` before
// mail exists) falls back to the pre-S1 surface and says so in the ledger —
// never an empty offer (the #206 cliff: 14/14 acts on commands/help, 0 edits).
#[test]
fn a_room_whose_affordances_match_no_native_verb_falls_back_never_to_nothing() {
    let faculty = LlmDeliberationFaculty::new(
        Uuid::new_v4(),
        "Asha",
        "You are Asha.",
        Arc::new(HeuristicInferenceAdapter::new()) as Arc<dyn AIProviderAdapter>,
    )
    .with_context_window(65_536)
    .with_tools(persona_tools::native_tool_specs());
    let mut ws = Workspace::new("chase the recruiter");
    ws.room_affordances = vec!["mail/send".to_string()];
    let selected = faculty.select_tool_surface(&ws, 65_536);
    assert_eq!(
        selected.reason,
        SurfaceReason::Full,
        "fell through to the pre-S1 arm"
    );
    assert_eq!(
        selected.specs.as_deref(),
        Some(faculty.native_specs.as_slice())
    );
}

// what this catches: the shipped benchmark recipe is now the RULE for every
// benchmark room, so it must authorize at least her hands — every verb the
// pre-S1 work turn offered — or S1 would take tools away from the citizens
// solving cards right now. If a new native hand is added without being
// authored here, this names it.
#[test]
fn the_shipped_benchmark_recipe_authorizes_every_hand_a_work_turn_offers() {
    let raw = persona_tools::native_tool_specs();
    for (file, json) in [
        (
            "benchmark.json",
            include_str!("../../../experience/recipes/benchmark.json"),
        ),
        (
            "benchmark-round.json",
            include_str!("../../../experience/recipes/benchmark-round.json"),
        ),
    ] {
        let recipe = crate::experience::recipe::ExperienceRecipe::from_json(json)
            .expect("shipped benchmark recipe parses");
        let authorized: std::collections::HashSet<&str> = recipe
            .affordances
            .iter()
            .map(|a| a.command.as_str())
            .collect();
        let missing: Vec<String> = hands_surface(&raw)
            .iter()
            .map(|s| s.name.clone())
            .filter(|n| !authorized.contains(n.as_str()))
            .collect();
        assert!(
            missing.is_empty(),
            "{file} must authorize every hand; missing: {missing:?}"
        );
    }
}

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
    // the web, like Claude (Joel, 2026-09-28); her rooms and chat are hands, and
    // the misread reviewer verbs stay out of her hands.
    assert_eq!(
        hands,
        [
            "code/read",
            "work/state",
            "chat/send",
            "commands/list",
            "room/join",
            "code/git/status",
            "work/submit",
            "web/fetch",
            "focus/continue"
        ],
        "git/apply, work/submission and work/review are reviewer verbs, not hands; work/submit is the holder's"
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

// what this catches: the live registry's command names drifting away from the
// hands prefixes (a rename to `files/*` would mute every work turn again) — a
// self-running check that the REAL registry yields a hands surface that is
// non-empty and smaller than the whole surface.
#[test]
fn the_real_registry_yields_a_non_empty_hands_surface() {
    let raw = persona_tools::native_tool_specs();
    if raw.is_empty() {
        return; // no registry in this test build — nothing to assert against
    }
    let hands = hands_surface(&raw);
    assert!(
        !hands.is_empty(),
        "hands surface is empty against the live registry"
    );
    assert!(
        hands.len() < raw.len(),
        "hands surface should be a strict subset"
    );
    assert!(
        hands.iter().any(|s| s.name == "commands/list"),
        "the discovery pair must survive"
    );
}
use crate::ai::heuristic_adapter::HeuristicInferenceAdapter;
use crate::ai::types::{ToolCall, ToolInputSchema, UsageMetrics};
use crate::cognition::workspace::BurstTurn;
use serde_json::json;
use std::collections::VecDeque;
use std::sync::Mutex;
use super::prompt::{EvidenceFit, PromptMessages, RecentMove};
use super::request::output_allowance;
use super::verdict::{answer_payload_bytes, reasoning_answer_split};
use super::tool_surface::SelectedSurface;

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
