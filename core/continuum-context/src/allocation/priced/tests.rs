use super::*;
use crate::allocation::{allocate, ActNeed, Head, Leveled, NeedWindow, Newest, NewestShown, Rungs, Span, TrainedContext, ViewClaim};
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
fn act(newest: Newest, price: ContextPrice) -> ActClaims {
    ActClaims {
        framing: Head::free(Leveled::one(Rungs::whole(4_000))),
        required: Leveled::one(Rungs::whole(500)),
        newest,
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
        trained: TrainedContext::Unreported,
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
    let filled = allocate(window, act(Newest::Pageable { whole: 6_000, least: 600 }, ContextPrice::Unmeasured));
    assert_eq!(filled.priced, Priced::Unmeasured);
    assert!(held(&filled) > 400_000, "unpriced, the view takes the window: {}", held(&filled));
    let priced = allocate(window, act(Newest::Pageable { whole: 6_000, least: 600 }, m5()));
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
    let priced = allocate(window, act(Newest::Pageable { whole: 6_000, least: 600 }, ContextPrice::Measured(m5_over(high))));
    assert!(matches!(priced.priced, Priced::Held { .. }), "{:?}", priced.priced);
    assert!(held(&priced) >= high.0, "never below the measured contexts: {}", held(&priced));
    let low = (5_000, 9_000);
    let before = allocate(window, act(Newest::Pageable { whole: 6_000, least: 600 }, ContextPrice::Unmeasured));
    let unmeasured_there = allocate(window, act(Newest::Pageable { whole: 6_000, least: 600 }, ContextPrice::Measured(m5_over(low))));
    assert_eq!(unmeasured_there.priced, Priced::OutsideMeasured { measured: low });
    assert_eq!(ActPlan { priced: Priced::Unmeasured, need: ActNeed::Unpriced, ..unmeasured_there }, before, "planned as before");
}

// what this catches (the dyno on #5822: the price took her tool schemas to their names, a help
// call and an extra act each, which no value the allocation weighs can see): a price choice that
// shows what she acts through below its summary. Priced, every verb keeps at least its summary and
// her held card stays whole; a window too small for that still reaches their pointers through the
// hard-limit path.
#[test]
fn what_she_acts_through_is_never_priced_below_its_summary() {
    let priced = allocate(755_456, act(Newest::Pageable { whole: 6_000, least: 600 }, m5()));
    let tools = priced.chosen(id::TOOLS).map_or(0, |choice| choice.cost); // map_or: tools are always chosen
    assert!(tools >= VERBS * VERB[1], "every verb at least its summary: {tools}");
    assert_eq!(priced.granted("view/held"), 900, "her held card whole");
    assert_eq!(priced.hard_limit(id::TOOLS), Some(VERBS * VERB[2]), "her need reads the tools' own hard limit, never the price's floor");
    // Every hard limit (verbs at their names), her newest result whole and her reply's need: 15,880;
    // with every verb at its summary and her held card whole: 18,800. A window between the two
    // plans as before, names and all.
    let tight = 18_500;
    let before = allocate(tight, act(Newest::Pageable { whole: 6_000, least: 600 }, ContextPrice::Unmeasured));
    let binds = allocate(tight, act(Newest::Pageable { whole: 6_000, least: 600 }, m5()));
    assert_eq!(binds.priced, Priced::WindowBinds);
    assert_eq!(ActPlan { priced: Priced::Unmeasured, need: ActNeed::Unpriced, ..binds }, before);
}

// what this catches: the price changing a plan whose window binds first. A window no larger than
// what she must hold plans exactly as before; so does a window the price would fill anyway.
#[test]
fn a_small_window_plans_as_before() {
    let at_least = least(&choosable(&act(Newest::Pageable { whole: 6_000, least: 600 }, m5())));
    for window in [at_least / 2, at_least - 1, at_least] {
        let before = allocate(window, act(Newest::Pageable { whole: 6_000, least: 600 }, ContextPrice::Unmeasured));
        let priced = allocate(window, act(Newest::Pageable { whole: 6_000, least: 600 }, m5()));
        assert_eq!(priced.priced, Priced::WindowBinds, "{window}");
        assert_eq!(ActPlan { priced: Priced::Unmeasured, need: ActNeed::Unpriced, ..priced }, before, "{window}");
    }
}

// what this catches (goal 7: never clip the newest end): the price shrinking her newest result,
// her last move or her reply's need to buy seconds. Her newest result is whole whenever the served
// window holds it whole, beside her last move whole and her reply's need.
#[test]
fn her_newest_result_is_never_priced_out() {
    let newest = Newest::Pageable { whole: 40_000, least: 400 };
    for window in [70_000, 200_000, 755_456] {
        let plan = allocate(window, act(newest, m5()));
        assert_eq!((plan.newest_shown(), plan.granted(id::NEWEST)), (NewestShown::Whole, 40_000), "{window}: {:?}", plan.priced);
        assert_eq!(plan.granted(id::LAST_MOVE), 1_200, "{window}");
        assert!(plan.granted(id::REPLY) >= 2_000, "{window}: her reply's need");
        assert!(plan.irreducible_shortfall().is_none(), "{window}");
    }
}

// what this catches (both ways, never a ratchet down): a price that only ever shrinks her window.
// The same act on a node where context costs a tenth as much holds more of it.
#[test]
fn a_cheaper_context_holds_more() {
    let held_at = |price: ContextPrice| match allocate(755_456, act(Newest::Pageable { whole: 6_000, least: 600 }, price)).priced {
        Priced::Held { held, .. } => held,
        other => panic!("{other:?}"),
    };
    let m5_time = m5_over(MEASURED);
    let tenth = |l: Line| Line { slope: l.slope / 10.0, ..l };
    let cheap = ContextPrice::Measured(ActTime { decode: tenth(m5_time.decode), prefill: tenth(m5_time.prefill), fresh: tenth(m5_time.fresh), ..m5_time });
    let (dear, cheaper) = (held_at(m5()), held_at(cheap));
    assert!(cheaper > dear, "a tenth of the price holds {cheaper}, the M5's {dear}");
}

/// Qwen3.6-35B-A3B's trained context, the model the M5 serves.
fn trained() -> TrainedContext {
    TrainedContext::Reported(std::num::NonZeroU32::new(262_144).expect("test: nonzero")) // JUSTIFIED: test fixture
}

/// The act above on the M5's model, her view's conversation wanting `overflow` uncollapsed.
fn overflowing(overflow: u32, price: ContextPrice) -> ActClaims {
    let mut claims = act(Newest::Pageable { whole: 6_000, least: 600 }, price);
    claims.trained = trained();
    for view in claims.view.iter_mut().filter(|view| view.claim.source_id == format!("{}conversation", id::VIEW)) {
        view.claim.max_tokens = overflow;
    }
    claims
}

fn need_tokens(need: ActNeed) -> Option<u32> {
    match need {
        ActNeed::Chosen(window) | ActNeed::Censored(window) => Some(window.tokens()),
        ActNeed::Unpriced | ActNeed::BeyondTrained(_) => None,
    }
}

// what this catches (the M5, 2026-11-07 16:21Z, `grid.mind.unservable`: Aiko's need read 902,158 on
// a 262,144-token model and the grid's demand 23.6M): her need recorded as the window plus what her
// view lacked UNCOLLAPSED (whole files, folded navigation). A view whose uncollapsed overflow is
// 900k records her need at the priced point, the window her act held, never above the trained
// context.
#[test]
fn a_900k_overflow_records_her_need_at_the_priced_point_never_above_the_trained_context() {
    let window = 262_144;
    let plan = allocate(window, overflowing(900_000, m5()));
    let wanted: u32 = plan.claims().iter().map(|claim| claim.max_tokens).fold(0, u32::saturating_add);
    assert!(wanted > 900_000, "the uncollapsed view the old need recorded: {wanted}");
    let Priced::Held { held, .. } = plan.priced else { panic!("a measured act time holds a priced window: {:?}", plan.priced) };
    assert_eq!(plan.need, ActNeed::Chosen(NeedWindow::within(held, trained()).expect("test: within")), "her need is the window the price chose"); // JUSTIFIED: test fixture
    assert!(held < window, "the priced point is inside her grant: {held}");
}

// what this catches: a grant that censors her need recording the grant (her need could then never
// rise past it) or the uncollapsed overflow (900k). When the priced point lies past her grant, her
// need is that point as a lower bound: past the grant, never past the trained context.
#[test]
fn a_grant_below_the_priced_point_records_that_point_censored() {
    let free = allocate(262_144, overflowing(900_000, m5()));
    let point = need_tokens(free.need).expect("the free act has a priced need"); // JUSTIFIED: test fixture
    // Between what she must hold and the priced point: the price wants more than her grant.
    let window = (least(&choosable(&overflowing(900_000, m5()))) + point) / 2;
    let censored = allocate(window, overflowing(900_000, m5()));
    let ActNeed::Censored(need) = censored.need else { panic!("a grant below the priced point censors her need: {:?}", censored.need) };
    assert!(need.tokens() > window && need.tokens() <= 262_144, "past her grant of {window}, inside the trained context: {}", need.tokens());
    assert!(need.tokens().abs_diff(point) <= point / 50, "the same peak the free act chose ({point}): {}", need.tokens());
    let mut unreported = overflowing(900_000, m5());
    unreported.trained = TrainedContext::Unreported;
    assert_eq!(allocate(window, unreported).need, ActNeed::Censored(NeedWindow::within(window, TrainedContext::Unreported).expect("test: within")), "an unreported model leaves her grant as the bound"); // JUSTIFIED: test fixture
}

// what this catches: a need that no lane of the model can hold being recorded as a size, or a
// need invented where no price was measured. What she must hold past the trained context is the
// typed defect; an unmeasured act time records no need at all.
#[test]
fn a_need_past_the_trained_context_is_a_defect_and_an_unmeasured_price_records_none() {
    let mut small = act(Newest::Pageable { whole: 6_000, least: 600 }, m5());
    small.trained = TrainedContext::Reported(std::num::NonZeroU32::new(12_000).expect("test: nonzero")); // JUSTIFIED: test fixture
    let binds = allocate(12_000, small);
    assert_eq!(binds.priced, Priced::WindowBinds);
    assert!(matches!(binds.need, ActNeed::BeyondTrained(beyond) if beyond.needed > 12_000), "{:?}", binds.need);
    assert_eq!(allocate(262_144, overflowing(900_000, ContextPrice::Unmeasured)).need, ActNeed::Unpriced);
}

// what this catches (the M5, 2026-11-10): the price holding a window below what her live view
// already sent, so her epoch turned over every second act and re-prefilled what the engine held.
// A view source whose flex basis (what the engine received) sits above its floor is part of what
// she must hold: the priced window never drops below it.
#[test]
fn the_price_never_holds_less_than_her_view_already_sent() {
    let window = 755_456;
    let free = allocate(window, act(Newest::Pageable { whole: 6_000, least: 600 }, m5()));
    let Priced::Held { held: free_held, .. } = free.priced else { panic!("fixture: priced {:?}", free.priced) };
    let mut claims = act(Newest::Pageable { whole: 6_000, least: 600 }, m5());
    let sent = free_held + 20_000;
    claims.view[1].claim.min_tokens = sent; // her conversation: what the engine already holds
    let plan = allocate(window, claims);
    assert!(held(&plan) >= sent, "the price holds what she sent: {} < {sent}", held(&plan));
}

// what this catches (the M5, 2026-11-10): her need recording only what this act holds, so her grant
// matched it, her view grew into it and turned over every second act. Her need carries a growth
// reserve as large as what her view already sent.
#[test]
fn her_need_carries_her_views_growth() {
    let window = 755_456;
    let bare = allocate(window, act(Newest::Pageable { whole: 6_000, least: 600 }, m5()));
    let mut claims = act(Newest::Pageable { whole: 6_000, least: 600 }, m5());
    let sent = 30_000;
    claims.view[1].claim.min_tokens = claims.view[1].claim.floor_tokens + sent;
    let grown = allocate(window, claims);
    let tokens = |need: ActNeed| match need {
        ActNeed::Chosen(n) | ActNeed::Censored(n) => Some(n.tokens()),
        _ => None,
    };
    let (Some(before), Some(after)) = (tokens(bare.need), tokens(grown.need)) else { panic!("fixture: priced needs {:?} {:?}", bare.need, grown.need) };
    assert!(after >= before + sent, "her need grows by what she sent: {before} -> {after}");
}

// what this catches (the M5, 2026-11-10: decode at 9-10 tok/s with 65-88k held, against ~15 at
// 20-30k): a view kept whole however much it slows every act, or collapsed on a whim. Collapsing
// costs a rebuild spread over the acts until it regrows: with warm acts (a small fresh tail) it
// repays and she collapses; when each act re-prefills a lot, it regrows at once and she keeps.
#[test]
fn she_collapses_only_when_the_rebuild_repays() {
    let window = 755_456;
    let sent = 110_000;
    let with_sent = |time: ActTime| {
        let mut claims = act(Newest::Pageable { whole: 6_000, least: 600 }, ContextPrice::Measured(time));
        claims.view[1].claim.min_tokens = claims.view[1].claim.floor_tokens + sent;
        allocate(window, claims)
    };
    let mut warm = m5_over(MEASURED);
    warm.fresh = line(1_500.0, 0.0, MEASURED);
    let collapsed = with_sent(warm);
    assert!(held(&collapsed) < sent, "warm acts repay the rebuild: she collapses ({} held)", held(&collapsed));
    let kept = with_sent(m5_over(MEASURED));
    assert!(held(&kept) >= sent, "re-prefilling acts regrow at once: she keeps ({} held)", held(&kept));
}
