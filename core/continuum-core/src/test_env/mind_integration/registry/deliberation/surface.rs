#![cfg(test)]
use super::*;

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
        .filter(|(name, _)| crate::ai::tool_dialect::reviewer_hand(name))
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
