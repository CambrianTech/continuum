#![cfg(test)]

use super::*;

/// The smallest 1 KiB-aligned window whose tool-surface share ADMITS the
/// whole registry — and the largest that does not.
///
/// ASKED of [`ContextBudget`], never re-derived from its denominator. Two
/// literals stood here (50,944 and 24,832, read off that module's
/// calibration table) and the crate's own de-hardcode guard caught them in
/// CI, correctly: a window constant that does not scale with the served
/// window is exactly what that guard exists to stop, and a test fixture is
/// not an exemption from it. Spelling `/ 4` here instead would have
/// duplicated `TOOL_SURFACE_DENOM`, which is the same defect wearing a
/// different hat — so this searches with the real predicate and stays
/// correct if the share is ever re-cut.
fn window_admitting(full_surface_tokens: usize) -> u32 {
    (1..=512)
        .map(|k| k * 1024)
        .find(|w| {
            crate::cognition::context_budget::ContextBudget::from_window(*w)
                .tool_surface_tokens()
                >= full_surface_tokens
        })
        .expect("some served window admits the full surface")
}

/// `(admits_the_full_surface, does_not)` — both derived from the registry's
/// measured cost, so neither can drift away from the bound it is meant to
/// straddle. The narrow one is one alignment step below the smallest
/// admitting window, i.e. the largest that still refuses.
fn calibrated_windows() -> (u32, u32) {
    let full = faculty_with_the_real_registry(1024).tool_surface_tokens;
    let admits = window_admitting(full);
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

// what this catches: the original defect. A work turn sends HANDS, so the
// budget must subtract the hands' cost. Before this, `budget` subtracted
// the whole registry — the citizen silently lost (full − hands) tokens of
// conversation on every act turn, on a window that had room for it.
// MUTATION-PROVED: point `SelectedSurface.tokens` at `tool_surface_tokens`
// on the hands arms and this fails on the strict inequality.
#[test]
fn a_work_turn_is_budgeted_for_its_hands_not_for_the_registry_it_withheld() {
    let (fits, _narrow) = calibrated_windows();
    let faculty = faculty_with_the_real_registry(fits);
    let selected = faculty.select_tool_surface(&work_turn(), fits);

    assert_eq!(
        selected.reason,
        SurfaceReason::HandsForWork,
        "a deliverable turn takes hands for FOCUS — and on this window the \
         full surface would have fit, so nothing but focus can explain it"
    );
    assert_eq!(
        selected.tokens, faculty.hands_surface_tokens,
        "the budget must be charged for what goes on the wire"
    );
    assert!(
        selected.tokens < faculty.tool_surface_tokens,
        "the saving must be REAL: hands ({}) must cost less than the registry \
         ({}), otherwise this test would pass while pricing nothing",
        selected.tokens,
        faculty.tool_surface_tokens
    );
}

// what this catches: silently re-deriving a cap from a send. The demand
// probe's contract is that every term is UNTRUNCATED, so when the WINDOW is
// what withheld the surface, demand must still price the full registry —
// report the hands here and the pressure the probe exists to measure
// disappears exactly when it starts mattering.
// MUTATION-PROVED: make `demand_tokens` return `self.tokens` unconditionally
// and this fails.
#[test]
fn a_budget_withheld_surface_still_reports_the_full_registry_as_demand() {
    let (_fits, narrow) = calibrated_windows();
    let faculty = faculty_with_the_real_registry(narrow);
    // A MESSAGE turn — so the narrowing can only be the window.
    let ws = Workspace::new("anything open?");
    let selected = faculty.select_tool_surface(&ws, narrow);

    assert_eq!(
        selected.reason,
        SurfaceReason::HandsForBudget,
        "this window's share cannot hold the registry, and it is not a work turn"
    );
    assert_eq!(
        selected.tokens, faculty.hands_surface_tokens,
        "she is SENT her hands"
    );
    assert_eq!(
        selected.demand_tokens(faculty.describe_tool_tokens()),
        faculty.tool_surface_tokens,
        "…but she WANTED the registry and the window refused it — demand is \
         the untruncated number or it is worthless for provisioning"
    );
}

// what this catches: the mirror error — inflating `over_window` on a turn
// that never wanted the full surface. A work turn narrows for FOCUS, so
// nothing was truncated and demand must not pretend otherwise; otherwise
// every act turn reports as near-overflow and the number stops meaning
// anything.
// MUTATION-PROVED: make `demand_tokens` return the full cost unconditionally
// and this fails.
#[test]
fn a_focus_narrowed_surface_does_not_inflate_demand_with_what_it_never_wanted() {
    let (fits, _narrow) = calibrated_windows();
    let faculty = faculty_with_the_real_registry(fits);
    let selected = faculty.select_tool_surface(&work_turn(), fits);

    assert_eq!(
        selected.demand_tokens(faculty.describe_tool_tokens()),
        faculty.hands_surface_tokens,
        "focus is not truncation — the registry was never wanted on this turn"
    );
}

// what this catches: the whole defect class, stated as the invariant rather
// than case by case. The reported cost must be the cost OF the reported
// surface — which is exactly what `delib.tool_surface.withheld` violated by
// printing a count of her hands beside the registry's price.
// MUTATION-PROVED: swap either arm's `tokens` for the other memo and this
// fails on that arm.
#[test]
fn the_cost_reported_is_always_the_cost_of_the_surface_reported() {
    let (fits, narrow) = calibrated_windows();
    for (label, window, ws) in [
        ("full", fits, Workspace::new("anything open?")),
        ("hands/work", fits, work_turn()),
        ("hands/budget", narrow, Workspace::new("anything open?")),
    ] {
        let faculty = faculty_with_the_real_registry(window);
        let selected = faculty.select_tool_surface(&ws, window);
        let specs = selected
            .specs
            .as_deref()
            .expect("a tool surface was offered");
        assert_eq!(
            selected.tokens,
            LlmDeliberationFaculty::tool_surface_tokens_of(specs),
            "[{label}] the price must be the price of THESE specs, not of a \
             surface that stayed home"
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
    let hands_based = faculty.hands_surface_tokens as u32
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
