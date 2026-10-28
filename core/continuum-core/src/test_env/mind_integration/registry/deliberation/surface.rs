#![cfg(test)]
use super::*;

fn faculty_on(window: u32) -> LlmDeliberationFaculty {
    LlmDeliberationFaculty::new(
        Uuid::new_v4(),
        "Aiko",
        "You are Aiko.",
        Arc::new(HeuristicInferenceAdapter::new()) as Arc<dyn AIProviderAdapter>,
    )
    .with_context_window(window)
    .with_tools(persona_tools::native_tool_specs())
}

// what this catches: the tool schemas moving between consecutive acts in one activity,
// which re-prefills everything after her system prompt. Aiko on the M5, 2026-10-24:
// `delib.prompt.reuse_split` diverged in `tools` (`edit_file` one act, `conclude_activity`
// the next) because a message turn took the whole registry (51 schemas), a work or
// initiative turn her hands (23), a review card two more verbs, and the tool share
// followed the turn's audience and measured prefill rate. Every kind of turn, under either
// audience, now sends the same bytes, and those bytes carry the reviewer verbs and her
// coding arc. Goal 10 (card ecd5dff3).
#[tokio::test]
async fn consecutive_acts_in_one_activity_send_byte_identical_tool_schemas() {
    use crate::cognition::audience;
    use crate::inference::prefill_rate::Audience;
    let window = 87_296;
    let faculty = faculty_on(window);
    let message = Workspace::new("anything open?");
    let mut work = Workspace::new("fix the failing test");
    work.workspace_deliverable = true;
    let mut initiative = Workspace::new("your own time");
    initiative.initiative = true;
    let mut held_review = Workspace::new("review card deec4ac2: is the fix right?");
    held_review.workspace_deliverable = true;
    let wire = |ws: &Workspace| {
        let selected = faculty.prompt_view(ws).tools;
        (selected.reason, serde_json::to_string(&selected.specs).expect("specs serialize"))
    };
    let first = wire(&message);
    assert_eq!(first.0, SurfaceReason::Hands);
    for (kind, ws) in [("work", &work), ("initiative", &initiative), ("held review", &held_review), ("message", &message)] {
        for who in [Audience::Interactive, Audience::Unattended] {
            let act = audience::with(who, async { wire(ws) }).await;
            assert_eq!(act, first, "a {who:?} {kind} act changed the tool schemas");
        }
    }
    for must in ["edit_file", "bash", "list_commands", "help", "work_review"] {
        assert!(first.1.contains(&format!("\"{must}\"")), "{must} rides natively: {}", first.1);
    }
    assert!(
        !first.1.contains("conclude_activity"),
        "the long tail stays one commands/help away, never a schema that comes and goes"
    );
}

// what this catches: S1's whole point. A room whose recipe declares affordances
// gets EXACTLY those native verbs plus the discovery pair — the recipe is the
// rule and the rule reaches her hands. Before this, `affordances` rode into the
// manifest and every room offered the same prefix-filtered global list.
#[test]
fn a_room_that_declares_affordances_selects_exactly_those_plus_the_discovery_pair() {
    let faculty = faculty_on(65_536);
    let mut ws = Workspace::new("find the docs for this API");
    ws.room_affordances = vec!["web/fetch".to_string(), "web/search".to_string()];

    let selected = faculty.prompt_view(&ws).tools;
    assert_eq!(selected.reason, SurfaceReason::RoomAffordances);
    let offered = selected.specs.as_deref().expect("a surface was offered");
    let surface = faculty.tool_surface();
    let names: Vec<&str> = surface
        .names()
        .iter()
        .zip(surface.registry().iter())
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
        crate::cognition::tool_surface::tool_surface_tokens_of(offered),
        "the price is the price of what was sent"
    );
}

// what this catches: the regression S1 must not cause. A room whose recipe declares NO
// affordances (every chat, profile, theme and settings page today) offers her hands, the
// same on a message turn as on a work turn.
#[test]
fn a_room_that_declares_nothing_offers_her_hands_on_every_turn() {
    let faculty = faculty_on(65_536);
    let mut work = Workspace::new("fix the failing test");
    work.workspace_deliverable = true;
    for ws in [Workspace::new("anything open?"), work] {
        assert!(ws.room_affordances.is_empty(), "the ordinary room declares none");
        let selected = faculty.prompt_view(&ws).tools;
        assert_eq!(selected.reason, SurfaceReason::Hands);
        assert_eq!(selected.specs.as_deref(), Some(faculty.tool_surface().hands()));
    }
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

// what this catches: a rule that binds nothing must never bind to NOTHING. A
// recipe naming only verbs this node has no native tool for (`mail/send` before
// mail exists) falls back to her hands and says so in the ledger —
// never an empty offer (the #206 cliff: 14/14 acts on commands/help, 0 edits).
#[test]
fn a_room_whose_affordances_match_no_native_verb_falls_back_never_to_nothing() {
    let faculty = faculty_on(65_536);
    let mut ws = Workspace::new("chase the recruiter");
    ws.room_affordances = vec!["mail/send".to_string()];
    let selected = faculty.prompt_view(&ws).tools;
    assert_eq!(selected.reason, SurfaceReason::Hands, "fell through to her hands");
    assert_eq!(selected.specs.as_deref(), Some(faculty.tool_surface().hands()));
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
            include_str!("../../../../../../continuum-experience/src/experience/recipes/benchmark.json"),
        ),
        (
            "benchmark-round.json",
            include_str!("../../../../../../continuum-experience/src/experience/recipes/benchmark-round.json"),
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
