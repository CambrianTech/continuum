#![cfg(test)]

use super::*;

// what this catches: the live 2026-09-09 mute. Kimi held `hard-rs/expr_eval`
// on a 28,160-token lane with a 39,532-token conversation; every 15 s she took
// a work turn that produced `act=0 model_ms=0`, because 613 tokens of standing
// grounding did not fit a budget the chained subtraction had saturated to 0,
// and that grounding error was promoted ahead of every other check into a hard
// deliberation fault. She kept the card while refusing, so the benchmark round
// could never finish. Enrichment is not a reason to stop thinking.
#[test]
fn an_unaffordable_standing_source_does_not_fault_the_turn() {
    // A SMALL stimulus — the irreducible payload comfortably fits, so nothing
    // about the conversation justifies a refusal. Only the standing sources
    // are unaffordable, and that must not be fatal.
    let mut ws = Workspace::new("What is the state of the card?");
    for id in ["room-roster", "room-wall"] {
        ws.broadcast.push(
            Contribution::context(
                FacultyId::Custom(id.into()),
                "standing world state that cannot possibly fit ".repeat(2_000),
                0.9,
                "standing",
            )
            .standing_grounding(),
        );
    }
    let faculty = LlmDeliberationFaculty::new(
        Uuid::new_v4(),
        "Kimi",
        "You are Kimi.",
        crate::cognition::llm_deliberation_faculty::model_binding(Arc::new(HeuristicInferenceAdapter::new()), None, 4_096),
    )
    .with_context_window(4_096);

    // The fixture must actually be unaffordable whole, or this test proves nothing.
    assert!(
        LlmDeliberationFaculty::assembled_context_cost(&ws) > 4_096,
        "fixture must make the standing grounding genuinely unaffordable whole"
    );

    let view = faculty.prompt_view(&ws);
    assert!(
        view.capacity_error.is_none(),
        "a citizen whose ENRICHMENT does not fit must still think — she holds                      her card while refusing, so a fault here strands the round forever:                      {:?}",
        view.capacity_error
    );
    assert!(
        !view.messages.is_empty(),
        "and she must be given something to think ABOUT"
    );
    // Collapse, never vanish (Aiko on the M5, 2026-11-07: "assembled context suppressed: zero
    // token budget"): the standing grounding reaches her at a level that fits.
    let text: String = view.messages.iter().map(|m| m.content_text()).collect();
    assert!(text.contains("[room-roster]") || text.contains("[room-wall]"), "her standing grounding reaches her reduced, not dropped");
}

// what this catches: a reduction that cannot state its own magnitude (the saturating-subtract
// mistake that destroyed the diagnosis one layer up: "over by 31,000" and "exactly at the line"
// arrived as the same number). The grounding consumer's want is every contribution whole, whatever
// level a budget shows it at, and its levels reach below any budget the fixture could need.
#[test]
fn the_grounding_consumer_keeps_its_want_whatever_level_it_is_shown_at() {
    let mut ws = Workspace::new("stimulus");
    ws.broadcast.push(
        Contribution::context(
            FacultyId::Custom("room-roster".into()),
            "standing world state".repeat(200),
            0.9,
            "standing",
        )
        .standing_grounding(),
    );
    let offer = super::super::super::prompt::grounding::GroundingOffer::of(&ws, &|tokens| tokens as u32);
    let whole = LlmDeliberationFaculty::assembled_context_cost(&ws) as u32;
    assert!(whole > 0, "the fixture must actually want something");
    assert!(offer.leveled.want() >= whole, "the WANT survives every reduction: {} of {whole}", offer.leveled.want());
    let reduced = offer.leveled.reduce(whole / 4);
    assert!(reduced.cost <= whole / 4, "a smaller budget is met by a smaller level: {} for {}", reduced.cost, whole / 4);
    assert!(reduced.cost > 0, "a level that still carries content, not nothing");
}
