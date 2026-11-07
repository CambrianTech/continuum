use super::*;
use crate::allocation::{allocate, Head, Leveled, Rungs, Span};
use crate::flex::FlexClaim;
use crate::price::Line;

fn line(intercept: f64, slope: f64) -> Line {
    Line { intercept, slope, slope_se: 0.0, intercept_se: 0.0, acts: 1_124, context: (10_840, 145_858) }
}

/// The M5's measured act time (Qwen3.6-35B-A3B, 1,124 acts, 2026-11-06T13Z..2026-11-07T13Z,
/// `replay --act-price`): ~3.5 s per 1k tokens held at 30k.
fn m5() -> ContextPrice {
    ContextPrice::Measured(ActTime {
        decode: line(0.06432, 6.5677e-7),
        prefill: line(0.00697, 2.1139e-7),
        fresh: line(5_421.0, 0.13209),
        observed: line(28.97, 3.7327e-3),
        output: 276.0,
    })
}

fn item(levels: &[u32]) -> Rungs {
    Rungs::new(levels.iter().copied()).expect("test: levels") // JUSTIFIED: test fixture
}

/// An act whose view wants far more than it needs: a conversation of 300k, results of 100k.
fn act(newest: &[u32], price: ContextPrice) -> ActClaims {
    let view = |name: &str, priority: u8, floor: u32, want: u32| FlexClaim {
        source_id: format!("{}{name}", id::VIEW),
        priority,
        floor_tokens: floor,
        min_tokens: floor,
        max_tokens: want,
        required: true,
    };
    ActClaims {
        framing: Head::free(Leveled::one(Rungs::whole(4_000))),
        required: Leveled::one(Rungs::whole(500)),
        newest: Rungs::new(newest.iter().copied()).map_or_else(Leveled::default, Leveled::one), // map_or_else: no levels is no result
        tools: Head::free(Leveled::one(item(&[6_000, 1_500]))),
        grounding: Leveled::one(item(&[3_000, 400])),
        last_move: Leveled::one(Rungs::whole(1_200)),
        history: Span { floor: 0, min: 0, want: 8_000 },
        reply: FlexClaim { source_id: id::REPLY.to_owned(), priority: 10, floor_tokens: 2_000, min_tokens: 2_000, max_tokens: 8_000, required: true },
        view: vec![view("conversation", 5, 1_000, 300_000), view("results", 5, 800, 100_000)],
        price,
    }
}

fn held(plan: &ActPlan) -> u32 {
    plan.window - plan.granted(id::REPLY)
}

// what this catches (the M5, 2026-11-07: the pool regrew to 755,456 and her prompts went 30k ->
// 75k, median act 50 s -> 104 s): a plan that fills whatever window it is served. Priced, a large
// window holds only what pays for its seconds: far less than the window, at a higher value per
// second than the filled one, and the unpriced plan still fills it.
#[test]
fn a_large_window_is_not_filled_when_its_tokens_cost_more_than_they_add() {
    let window = 755_456;
    let filled = allocate(window, act(&[6_000, 600], ContextPrice::Unmeasured));
    assert_eq!(filled.priced, Priced::Unmeasured);
    assert!(held(&filled) > 400_000, "unpriced, the view takes the window: {}", held(&filled));
    let priced = allocate(window, act(&[6_000, 600], m5()));
    let Priced::Held { held: kept, value_per_second, value_per_second_at_window, seconds, seconds_at_window } = priced.priced else {
        panic!("a measured act time on a large window holds a priced window: {:?}", priced.priced)
    };
    assert!(kept < 60_000, "held {kept} of {window}");
    assert!(value_per_second > value_per_second_at_window && seconds < seconds_at_window, "{:?}", priced.priced);
    assert!(priced.view_granted() + priced.room() < filled.view_granted() + filled.room());
    // Her view still holds well above its floors (1,800): it is priced, never starved.
    assert!(priced.view_granted() >= 3_600, "{}", priced.view_granted());
}

// what this catches (both ways, never a ratchet down): a price that only ever shrinks her window.
// The same act on a node where context costs a tenth as much holds more of it.
#[test]
fn a_cheaper_context_holds_more() {
    let held_at = |price: ContextPrice| match allocate(755_456, act(&[6_000, 600], price)).priced {
        Priced::Held { held, .. } => held,
        other => panic!("{other:?}"),
    };
    let ContextPrice::Measured(m5_time) = m5() else { panic!("m5 is measured") };
    let tenth = |l: Line| Line { slope: l.slope / 10.0, ..l };
    let cheap = ContextPrice::Measured(ActTime { decode: tenth(m5_time.decode), prefill: tenth(m5_time.prefill), fresh: tenth(m5_time.fresh), ..m5_time });
    let (dear, cheaper) = (held_at(m5()), held_at(cheap));
    assert!(cheaper > dear, "a tenth of the price holds {cheaper}, the M5's {dear}");
}

// what this catches: the price changing a plan whose window binds first. A window no larger than
// what she must hold (every hard limit, her newest result whole, her reply's need) plans exactly
// as before; so does a window the price would fill anyway.
#[test]
fn a_small_window_plans_as_before() {
    let at_least = least(&act(&[6_000, 600], m5()));
    for window in [at_least / 2, at_least - 1, at_least] {
        let before = allocate(window, act(&[6_000, 600], ContextPrice::Unmeasured));
        let priced = allocate(window, act(&[6_000, 600], m5()));
        assert_eq!(priced.priced, Priced::WindowBinds, "{window}");
        assert_eq!(ActPlan { priced: Priced::Unmeasured, ..priced }, before, "{window}");
    }
    // A window just past the least: wherever the price holds all of it, the plan is the plan as before.
    for window in [at_least + 500, at_least + 2_000, at_least + 8_000] {
        let before = allocate(window, act(&[6_000, 600], ContextPrice::Unmeasured));
        let priced = allocate(window, act(&[6_000, 600], m5()));
        if matches!(priced.priced, Priced::Held { held, .. } if held == window) {
            assert_eq!(ActPlan { priced: Priced::Unmeasured, ..priced }, before, "{window}");
        }
    }
}

// what this catches (goal 7: never clip the newest end): the price shrinking her newest result,
// her last move or her reply's need to buy seconds. However high the price, her newest result is
// whole whenever the served window holds it whole, beside her last move whole and her reply's need.
#[test]
fn her_newest_result_is_never_priced_out() {
    let newest = [40_000, 4_000, 400];
    for window in [60_000, 200_000, 755_456] {
        let plan = allocate(window, act(&newest, m5()));
        assert_eq!((plan.newest_level(), plan.granted(id::NEWEST)), (0, 40_000), "{window}: {:?}", plan.priced);
        assert_eq!(plan.granted(id::LAST_MOVE), 1_200, "{window}");
        assert!(plan.granted(id::REPLY) >= 2_000, "{window}: her reply's need");
        assert!(plan.irreducible_shortfall().is_none(), "{window}");
    }
}
