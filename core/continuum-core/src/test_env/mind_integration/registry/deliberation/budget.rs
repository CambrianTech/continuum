#![cfg(test)]
//! Card dec1a7ff: #3829 made the tool surface a per-turn decision but left the
//! ACCOUNTING pricing the full registry, so a work turn's message budget was
//! short by (full − hands) and trimmed conversation to fit room that was never
//! occupied. These pin what is COUNTED against what is SENT, against the REAL registry.

use super::*;

/// The first aligned window whose shared allocation admits whole hands, and its predecessor.
fn calibrated_windows() -> (u32, u32) {
    let hands = faculty_with_the_real_registry(1024).tool_surface().hands_tokens();
    let admits = (1..=512)
        .map(|k| k * 1024)
        .find(|w| {
            faculty_with_the_real_registry(*w).prompt_view(&work_turn()).tools.tokens >= hands
        })
        .expect("some served window admits her hands");
    (admits, admits - 1024)
}

fn faculty_with_the_real_registry(window: u32) -> LlmDeliberationFaculty {
    LlmDeliberationFaculty::new(
        Uuid::new_v4(),
        "Asha",
        "You are Asha.",
        Arc::new(HeuristicInferenceAdapter::new()) as Arc<dyn AIProviderAdapter>,
    )
    .with_context_window(window)
    .with_tools(persona_tools::native_tool_specs())
}

fn work_turn() -> Workspace {
    let mut ws = Workspace::new("fix the failing test");
    ws.workspace_deliverable = true;
    ws
}

// what this catches: the original defect (card dec1a7ff). A turn sends her HANDS, so the
// budget must subtract the hands' cost; subtracting the whole registry silently cost her
// (registry - hands) tokens of conversation on a window that had room for it.
// MUTATION-PROVED: point `SelectedSurface.tokens` at the registry's cost and this fails on
// the strict inequality.
#[test]
fn a_turn_is_budgeted_for_her_hands_not_for_the_registry() {
    let (fits, _narrow) = calibrated_windows();
    let faculty = faculty_with_the_real_registry(fits);
    for ws in [work_turn(), Workspace::new("anything open?")] {
        let selected = faculty.prompt_view(&ws).tools;
        assert_eq!(selected.reason, SurfaceReason::Hands);
        assert_eq!(selected.tokens, faculty.tool_surface().hands_tokens(), "charged for what goes on the wire");
        assert!(
            selected.tokens < faculty.describe_tool_tokens(),
            "the saving must be REAL: hands ({}) must cost less than the registry ({})",
            selected.tokens,
            faculty.describe_tool_tokens()
        );
    }
}

// what this catches: silently re-deriving a cap from a send. The demand probe's contract is
// that every term is UNTRUNCATED, so when the WINDOW withheld her extended hands, demand
// must still price her whole hands; report the core here and the pressure the probe exists
// to measure disappears exactly when it starts mattering.
// MUTATION-PROVED: make `demand_tokens` return `self.tokens` unconditionally and this fails.
// The window is calibrated on a work turn, so the test asks with one: under the one allocation a
// smaller turn leaves room and her hands fit, which is the allocation working, not a withheld surface.
#[test]
fn a_window_withheld_surface_still_reports_her_whole_hands_as_demand() {
    let (_fits, narrow) = calibrated_windows();
    let faculty = faculty_with_the_real_registry(narrow);
    let selected = faculty.prompt_view(&work_turn()).tools;
    assert_eq!(selected.reason, SurfaceReason::CoreHandsForWindow, "this window's share cannot hold her hands");
    assert!(selected.tokens < faculty.tool_surface().hands_tokens(), "she is SENT her core hands");
    assert_eq!(
        selected.demand_tokens(),
        faculty.tool_surface().hands_tokens(),
        "she WANTED her hands and the window refused them: demand is the untruncated number"
    );
}

// what this catches: the mirror error, inflating `over_window` on a turn that sent what it
// wanted. Nothing was truncated, so demand must not pretend otherwise.
#[test]
fn an_untruncated_surface_does_not_inflate_demand() {
    let (fits, _narrow) = calibrated_windows();
    let faculty = faculty_with_the_real_registry(fits);
    let selected = faculty.prompt_view(&work_turn()).tools;
    assert_eq!(selected.demand_tokens(), selected.tokens);
}

// what this catches: the whole defect class, stated as the invariant rather than case by
// case. The reported cost must be the cost OF the reported surface.
// MUTATION-PROVED: swap either arm's `tokens` for the other memo and this fails on that arm.
#[test]
fn the_cost_reported_is_always_the_cost_of_the_surface_reported() {
    let (fits, narrow) = calibrated_windows();
    for (label, window) in [("hands", fits), ("core/window", narrow)] {
        let faculty = faculty_with_the_real_registry(window);
        let selected = faculty.prompt_view(&work_turn()).tools;
        let specs = selected.specs.as_deref().expect("a tool surface was offered");
        assert_eq!(
            selected.tokens,
            crate::cognition::tool_surface::tool_surface_tokens_of(specs),
            "[{label}] the price must be the price of THESE specs, not of a surface that stayed home"
        );
    }
}

// what this catches: a "simplification" that points the structurally-mute
// sensor at the selected surface. `min_window_for_agentic_surface` asks a
// DIFFERENT question — "how large a window hosts the WHOLE agentic surface"
// — so it legitimately keeps the full cost, and narrowing it would quietly
// lower the bar below which we warn that a citizen cannot act.
// MUTATION-PROVED: swap `describe_tool_tokens()` for the hands memo in
// `min_window_for_agentic_surface` and this fails.
#[test]
fn the_structurally_mute_sensor_still_asks_for_the_whole_surface() {
    let (_fits, narrow) = calibrated_windows();
    let faculty = faculty_with_the_real_registry(narrow);
    let needed = faculty.min_window_for_agentic_surface();

    // The discriminator has to be the bound a HANDS-based sensor would
    // produce, not a constant. The first version of this test asserted
    // `needed >= tool_surface_tokens` and the mutation it was written for
    // did not move it: hands + framing already clears the registry's raw
    // token count, so the assertion held while the sensor measured the
    // wrong surface. A bound satisfied by accident pins nothing.
    let hands_based = faculty.tool_surface().hands_tokens() as u32
        + faculty.framing_floor_tokens()
        + LlmDeliberationFaculty::COMPLETION_FLOOR_TOKENS;

    assert!(
        needed > hands_based,
        "the bound must be STRICTLY more demanding than a hands-only one \
                     ({needed} vs {hands_based}): it answers 'can this window host the \
                     agentic surface at all', never 'what did we send this turn'. Equal \
                     means the sensor was narrowed to the selected surface and the \
                     'she is structurally mute below here' warning now fires too late."
    );
}

// what this catches: the head clip a passing test hid (goal 7). After a provider refusal
// scaled the calibration (~1.9x) and her view turned over twice, an unrelated small turn in
// the room priced its allocation at 1.0, so its charged prompt plus reserve overran the
// window by 4,326; on canary it fit only because the final fit cut her whole view from the
// front. Then, back in the room, one act re-appended all of it, and the turnover reserved
// that act's appends out of the start: her view rendered empty. Every part is a charged
// claim now, the small turn fits, and back in the room her view is present with its newest
// turn whole within the window.
#[tokio::test]
async fn a_scaled_calibration_after_turnovers_keeps_her_view_within_the_window() {
    use crate::ai::inference_error::InferenceError;
    use crate::cognition::working_set::WorkingSetRegistry;
    use crate::cognition::workspace::Burst;

    let home = tempfile::tempdir().expect("isolated demand storage");
    let _native = crate::paths::NativeHomeOverride::install(home.path());
    let persona = Uuid::new_v4();
    let window = 29_440;
    let adapter = Arc::new(ScriptedAdapter::new(vec![
        make_response(FinishReason::Stop, "PASS", None),
        make_response(FinishReason::Stop, "PASS", None),
        make_response(FinishReason::Stop, "PASS", None),
    ]));
    // The engine counted 29,722 for a prompt estimated at about half that: a text density.
    adapter.rejections.lock().expect("fixture rejection queue").extend([
        InferenceError::ContextExceeded { requested: 29_722, available: window },
        InferenceError::ContextExceeded { requested: 29_722, available: window },
    ]);
    let registry = WorkingSetRegistry::new();
    for tick in 0..3 {
        registry.record_emission_in_memory(persona, 834, 0, crate::cognition::working_set::EmissionStop::Landed, tick);
    }
    let faculty = LlmDeliberationFaculty::new(persona, "Ivar", "You are Ivar.", adapter.clone())
        .with_context_window(window)
        .with_tools(vec![read_tool()])
        .with_working_set(registry);
    let room = crate::identity::ActivityRoom::mint();
    let mut turns: Vec<_> = (0..160)
        .map(|i| {
            BurstTurn::attributed(
                i % 2 == 0,
                if i % 2 == 0 { "Ivar" } else { "Peer" },
                format!("distinct_history_{i} ").repeat(100),
                Some(i),
            )
        })
        .collect();
    turns.push(BurstTurn::attributed(false, "Peer", "Review the current artifact.", Some(161)));
    let ws = Workspace::new(Burst::from_turns(room, turns.clone()));
    // Bounded: two refusals end the turn, as a fault or a typed capacity refusal (the
    // newest-result refit's end), never a third request.
    let refused = faculty.contribute(&ws).await.expect("bounded refusal");
    assert!(refused.fault.is_some() || refused.capacity_refusal.is_some(), "{:?}", refused.fault);
    let next = faculty.contribute(&ws).await.expect("next tick");
    assert!(next.fault.is_none(), "{:?}", next.fault);
    let (effective, calibration) = faculty.prompt_fit(&faculty.binding.load());
    assert!(calibration.charge(10_000) > 10_000, "the refusal scaled the calibration");
    let within = |view: &DeliberationPromptView| {
        assert!(view.capacity_error.is_none(), "{:?}", view.capacity_error);
        assert!(view.budget_fault.is_none(), "{:?}", view.budget_fault);
        assert!(
            calibration.charge(view.estimated_prompt_tokens) + view.completion_reserve as usize
                <= effective as usize,
            "charged prompt + reserve fits the window"
        );
    };

    // An unrelated small turn: her carried view rides room no source claims any more.
    within(&faculty.prompt_view(&Workspace::in_room("What changed?", ws.room_id)));

    // Back in the room, one turn on: her view's sources hold its history again.
    turns.push(BurstTurn::attributed(false, "Peer", "What changed?", Some(162)));
    let follow = Workspace::new(Burst::from_turns(room, turns));
    let view = faculty.prompt_view(&follow);
    within(&view);
    assert!(
        view.messages.iter().any(|m| m.content_text().contains("distinct_history_159 ")),
        "her view is present with its newest turn whole"
    );
    let outcome = faculty.contribute(&follow).await.expect("follow-up turn");
    assert!(outcome.fault.is_none(), "{:?}", outcome.fault);
}
