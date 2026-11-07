use super::*;
use crate::allocation::{allocate, Head, Leveled, Rungs, Span, ViewClaim};
use crate::flex::FlexClaim;
use crate::price::Line;

/// The M5's measured contexts (`replay --act-price`, 2026-11-06T13Z..2026-11-07T13Z).
const MEASURED: (u32, u32) = (10_840, 145_858);

fn line(intercept: f64, slope: f64, context: (u32, u32)) -> Line {
    Line { intercept, slope, slope_se: 0.0, intercept_se: 0.0, acts: 1_124, context }
}

/// The M5's measured act time (Qwen3.6-35B-A3B, 1,124 acts): ~3.5 s per 1k tokens held at 30k,
/// measured over `context`.
fn m5_over(context: (u32, u32)) -> ActTime {
    ActTime {
        decode: line(0.06432, 6.5677e-7, context),
        prefill: line(0.00697, 2.1139e-7, context),
        fresh: line(5_421.0, 0.13209, context),
        observed: line(28.97, 3.7327e-3, context),
        output: 276.0,
    }
}

fn m5() -> ContextPrice {
    ContextPrice::Measured(m5_over(MEASURED))
}

fn item(levels: &[u32]) -> Rungs {
    Rungs::new(levels.iter().copied()).expect("test: levels") // JUSTIFIED: test fixture
}

const VERBS: u32 = 20;
/// Each verb whole, at its summary (name and description), at its name.
const VERB: [u32; 3] = [450, 120, 15];

fn source(name: &str, priority: u8, floor: u32, want: u32, consumer: Consumer) -> ViewClaim {
    let claim = FlexClaim { source_id: format!("{}{name}", id::VIEW), priority, floor_tokens: floor, min_tokens: floor, max_tokens: want, required: true };
    ViewClaim { claim, consumer }
}

/// An act whose view wants far more than it needs (a conversation of 300k, results of 100k),
/// with twenty verbs she acts through and her held card.
fn act(newest: &[u32], price: ContextPrice) -> ActClaims {
    ActClaims {
        framing: Head::free(Leveled::one(Rungs::whole(4_000))),
        required: Leveled::one(Rungs::whole(500)),
        newest: Rungs::new(newest.iter().copied()).map_or_else(Leveled::default, Leveled::one), // map_or_else: no levels is no result
        tools: Head { consumer: Consumer::Actionable, ..Head::free(Leveled::new((0..VERBS).map(|_| item(&VERB)).collect())) },
        grounding: Leveled::one(item(&[3_000, 400])),
        last_move: Leveled::one(Rungs::whole(1_200)),
        history: Span { floor: 0, min: 0, want: 8_000 },
        reply: FlexClaim { source_id: id::REPLY.to_owned(), priority: 10, floor_tokens: 2_000, min_tokens: 2_000, max_tokens: 8_000, required: true },
        view: vec![
            source("held", 6, 80, 900, Consumer::Actionable),
            source("conversation", 5, 1_000, 300_000, Consumer::Context),
            source("results", 5, 800, 100_000, Consumer::Context),
        ],
        price,
    }
}

fn held(plan: &ActPlan) -> u32 {
    plan.window - plan.granted(id::REPLY)
}

// what this catches (the M5, 2026-11-07: the pool regrew to 755,456 and her prompts went 30k ->
// 75k, median act 50 s -> 104 s): a plan that fills whatever window it is served. Priced, a large
// window holds only what pays for its seconds, inside the contexts her act time was measured over,
// while the unpriced plan still fills it.
#[test]
fn a_large_window_is_not_filled_when_its_tokens_cost_more_than_they_add() {
    let window = 755_456;
    let filled = allocate(window, act(&[6_000, 600], ContextPrice::Unmeasured));
    assert_eq!(filled.priced, Priced::Unmeasured);
    assert!(held(&filled) > 400_000, "unpriced, the view takes the window: {}", held(&filled));
    let priced = allocate(window, act(&[6_000, 600], m5()));
    let Priced::Held { held: kept, at_window, measured, .. } = priced.priced else {
        panic!("a measured act time on a large window holds a priced window: {:?}", priced.priced)
    };
    assert!(kept < 60_000, "held {kept} of {window}");
    assert!(held(&priced) >= MEASURED.0 && measured == MEASURED, "inside the measured contexts: {}", held(&priced));
    assert_eq!(at_window, None, "the served window holds far past the measured contexts: never priced by extrapolation");
    assert!(priced.view_granted() + priced.room() < filled.view_granted() + filled.room());
    assert!(priced.view_granted() >= 3_600, "her view well above its floors (1,880): {}", priced.view_granted());
}

// what this catches (the dyno on #5822: four in five priced windows fell below the smallest context
// ever measured, where the fresh-tokens line clamps): a price chosen where the curve says nothing.
// A curve measured only over larger contexts never holds below them; one measured only below what
// she must hold says nothing, and the window plans exactly as before.
#[test]
fn the_price_never_chooses_outside_the_measured_contexts() {
    let window = 755_456;
    let high = (60_000, 145_858);
    let priced = allocate(window, act(&[6_000, 600], ContextPrice::Measured(m5_over(high))));
    assert!(matches!(priced.priced, Priced::Held { .. }), "{:?}", priced.priced);
    assert!(held(&priced) >= high.0, "never below the measured contexts: {}", held(&priced));
    let low = (5_000, 9_000);
    let before = allocate(window, act(&[6_000, 600], ContextPrice::Unmeasured));
    let unmeasured_there = allocate(window, act(&[6_000, 600], ContextPrice::Measured(m5_over(low))));
    assert_eq!(unmeasured_there.priced, Priced::OutsideMeasured { measured: low });
    assert_eq!(ActPlan { priced: Priced::Unmeasured, ..unmeasured_there }, before, "planned as before");
}

// what this catches (the dyno on #5822: the price took her tool schemas to their names, a help
// call and an extra act each, which no value the allocation weighs can see): a price choice that
// shows what she acts through below its summary. Priced, every verb keeps at least its summary and
// her held card stays whole; a window too small for that still reaches their pointers through the
// hard-limit path.
#[test]
fn what_she_acts_through_is_never_priced_below_its_summary() {
    let priced = allocate(755_456, act(&[6_000, 600], m5()));
    let tools = priced.chosen(id::TOOLS).map_or(0, |choice| choice.cost); // map_or: tools are always chosen
    assert!(tools >= VERBS * VERB[1], "every verb at least its summary: {tools}");
    assert_eq!(priced.granted("view/held"), 900, "her held card whole");
    assert_eq!(priced.hard_limit(id::TOOLS), Some(VERBS * VERB[2]), "her need reads the tools' own hard limit, never the price's floor");
    // Every hard limit (verbs at their names), her newest result whole and her reply's need: 15,880;
    // with every verb at its summary and her held card whole: 18,800. A window between the two
    // plans as before, names and all.
    let tight = 18_500;
    let before = allocate(tight, act(&[6_000, 600], ContextPrice::Unmeasured));
    let binds = allocate(tight, act(&[6_000, 600], m5()));
    assert_eq!(binds.priced, Priced::WindowBinds);
    assert_eq!(ActPlan { priced: Priced::Unmeasured, ..binds }, before);
}

// what this catches: the price changing a plan whose window binds first. A window no larger than
// what she must hold plans exactly as before; so does a window the price would fill anyway.
#[test]
fn a_small_window_plans_as_before() {
    let at_least = least(&choosable(&act(&[6_000, 600], m5())));
    for window in [at_least / 2, at_least - 1, at_least] {
        let before = allocate(window, act(&[6_000, 600], ContextPrice::Unmeasured));
        let priced = allocate(window, act(&[6_000, 600], m5()));
        assert_eq!(priced.priced, Priced::WindowBinds, "{window}");
        assert_eq!(ActPlan { priced: Priced::Unmeasured, ..priced }, before, "{window}");
    }
}

// what this catches (goal 7: never clip the newest end): the price shrinking her newest result,
// her last move or her reply's need to buy seconds. Her newest result is whole whenever the served
// window holds it whole, beside her last move whole and her reply's need.
#[test]
fn her_newest_result_is_never_priced_out() {
    let newest = [40_000, 4_000, 400];
    for window in [70_000, 200_000, 755_456] {
        let plan = allocate(window, act(&newest, m5()));
        assert_eq!((plan.newest_level(), plan.granted(id::NEWEST)), (0, 40_000), "{window}: {:?}", plan.priced);
        assert_eq!(plan.granted(id::LAST_MOVE), 1_200, "{window}");
        assert!(plan.granted(id::REPLY) >= 2_000, "{window}: her reply's need");
        assert!(plan.irreducible_shortfall().is_none(), "{window}");
    }
}

// what this catches (both ways, never a ratchet down): a price that only ever shrinks her window.
// The same act on a node where context costs a tenth as much holds more of it.
#[test]
fn a_cheaper_context_holds_more() {
    let held_at = |price: ContextPrice| match allocate(755_456, act(&[6_000, 600], price)).priced {
        Priced::Held { held, .. } => held,
        other => panic!("{other:?}"),
    };
    let m5_time = m5_over(MEASURED);
    let tenth = |l: Line| Line { slope: l.slope / 10.0, ..l };
    let cheap = ContextPrice::Measured(ActTime { decode: tenth(m5_time.decode), prefill: tenth(m5_time.prefill), fresh: tenth(m5_time.fresh), ..m5_time });
    let (dear, cheaper) = (held_at(m5()), held_at(cheap));
    assert!(cheaper > dear, "a tenth of the price holds {cheaper}, the M5's {dear}");
}
