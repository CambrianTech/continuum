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
        Arc::new(HeuristicInferenceAdapter::new()),
    )
    .with_context_window(4_096);

    // The fixture must actually be unaffordable, or this test proves nothing.
    let whole = GroundingPlan::minimum(&ws);
    assert!(
        whole.context_reserve() + whole.trailing_tokens() > 4_096,
        "fixture must make the standing minimum genuinely unaffordable"
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
}

// what this catches: shedding must report WHAT IT WANTED. A reduction that
// cannot state its own magnitude is the saturating-subtract mistake that
// destroyed the diagnosis one layer up — "over by 31,000" and "exactly at the
// line" must not arrive as the same number.
#[test]
fn shedding_reports_the_size_it_wanted() {
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
    let whole = GroundingPlan::minimum(&ws);
    let wanted_tokens = whole.context_reserve() + whole.trailing_tokens();
    assert!(
        wanted_tokens > 0,
        "the fixture must actually want something"
    );

    match GroundingPlan::within_budget(&ws, 0) {
        Fitted::Whole(_) => panic!("a zero budget cannot afford this fixture"),
        Fitted::Reduced { plan, wanted } => {
            assert_eq!(wanted, wanted_tokens, "the WANT survives the shed");
            assert_eq!(
                plan.context_reserve() + plan.trailing_tokens(),
                0,
                "nothing is kept that the budget cannot pay for — and a piece                              too big for the budget is dropped whole, never truncated into                              a fragment that reads as complete"
            );
        }
    }

    match GroundingPlan::within_budget(&ws, wanted_tokens) {
        Fitted::Whole(plan) => assert_eq!(
            plan.context_reserve() + plan.trailing_tokens(),
            wanted_tokens,
            "an affordable minimum is kept whole"
        ),
        Fitted::Reduced { .. } => panic!("an exactly-affordable minimum must fit"),
    }
}
