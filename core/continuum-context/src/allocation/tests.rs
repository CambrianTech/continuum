use super::*;
use crate::fixtures::Fake;
use crate::source::Source;
use crate::view::{claims as view_claims, Claim, View};

fn reply(floor: u32, want: u32, priority: u8) -> FlexClaim {
    FlexClaim { source_id: id::REPLY.to_owned(), priority, floor_tokens: floor, min_tokens: floor, max_tokens: want, required: true }
}

fn view_source(name: &str, priority: u8, floor: u32, want: u32) -> FlexClaim {
    FlexClaim {
        source_id: format!("{}{name}", id::VIEW),
        priority,
        floor_tokens: floor,
        min_tokens: floor,
        max_tokens: want,
        required: false,
    }
}

fn claims(window_newest: Newest, view: Vec<FlexClaim>, reply_priority: u8) -> ActClaims {
    ActClaims {
        framing: 4_000,
        required: 500,
        newest: window_newest,
        tools: Span { floor: 1_000, min: 1_000, want: 3_000 },
        grounding: Span { floor: 0, min: 300, want: 1_200 },
        history: Span { floor: 200, min: 200, want: 800 },
        reply: reply(800, 2_400, reply_priority),
        view,
    }
}

// what this catches: the side budget her newest result was sized against, beside the
// allocation (Kimi on the windows 5090, 2026-10-28, refusal 1eef970d: her result priced against
// the window less framing and tools, her view's floor then taking its room). It is a claim: whole
// whenever the window holds it beside every floor, else the largest of its levels that does, and
// what that level leaves goes to her view, never to a level she is not shown (capacity_and_wire's
// wire test: as a span her result took every token left and a room update in her view vanished).
#[test]
fn the_newest_result_is_the_largest_of_its_levels_that_fits_and_the_rest_reaches_her_view() {
    let newest = || Newest { levels: vec![9_000, 2_000, 900] };
    let view = || vec![view_source("conversation", 4, 600, 20_000)];
    let roomy = allocate(40_000, claims(newest(), view(), 8));
    assert_eq!((roomy.newest_level(), roomy.granted(id::NEWEST)), (0, 9_000), "whole");
    // Required floors: framing 4,000 + required 500 + tools 1,000 + reply 800 = 6,300; 12,000
    // leaves 5,700: the 2,000 level fits, the whole does not.
    let tight = allocate(12_000, claims(newest(), view(), 8));
    assert_eq!((tight.newest_level(), tight.granted(id::NEWEST)), (1, 2_000), "the largest level that fits");
    assert!(tight.view_granted() > 600, "what the level leaves reaches her view past its floor: {}", tight.view_granted());
    assert!(tight.irreducible_shortfall().is_none(), "reducible: no growth asked");
    let floor_only = allocate(7_500, claims(newest(), view(), 8));
    assert_eq!(floor_only.newest_level(), 2, "nothing larger fits: its coarsest");
}

// what this catches (Cormac on #5779, goal 7: never clip the newest end, shrink oldest-first):
// her newest result's level chosen against EVERY floor, optional ones included, so enrichment and
// older history pushed it to a summary on a window that held it whole (allocate(15_500): level 1,
// 2,000 granted, with required floors 6,300 + whole 9,000 = 15,300 <= 15,500). It is chosen
// against the required floors and the reply's; history and her view shed in the floor pass.
#[test]
fn optional_claims_shed_before_her_newest_result_is_shown_smaller() {
    let view = vec![view_source("conversation", 4, 600, 20_000)];
    let plan = allocate(15_500, claims(Newest { levels: vec![9_000, 2_000, 900] }, view, 8));
    assert_eq!((plan.newest_level(), plan.granted(id::NEWEST)), (0, 9_000), "whole: it fits beside every required floor");
    let optional = plan.granted(id::HISTORY) + plan.view_granted() + plan.granted(id::GROUNDING);
    assert!(optional <= 15_500 - 15_300, "history, grounding and her view shed first: {optional}");
    assert!(plan.irreducible_shortfall().is_none());
}

// what this catches: required floors past the window answered by collapsing history (or by a
// bigger grant on a reducible overflow). Irreducible growth is typed apart.
#[test]
fn required_floors_past_the_window_are_an_irreducible_shortfall() {
    let plan = allocate(5_000, claims(Newest { levels: vec![9_000, 900] }, Vec::new(), 8));
    let short = plan.irreducible_shortfall().expect("test: the floors exceed 5,000"); // JUSTIFIED: test fixture
    assert_eq!(short.window, 5_000);
    assert!(short.needed > 5_000, "{short:?}");
}

// what this catches: her view's start moving with the reply's priority (one 16,384 window started
// her conversation at 4,005 tokens on a pass and 2,402 on the act, so her prefix moved), which a
// second allocation with the reply seated at its want used to prevent. One plan, the same start.
#[test]
fn a_pass_and_the_act_after_it_start_her_view_alike() {
    let newest = Newest::default;
    let view = || vec![view_source("conversation", 4, 600, 30_000)];
    let pass = allocate(16_384, claims(newest(), view(), 4));
    let act = allocate(16_384, claims(newest(), view(), 10));
    let (pass_start, act_start) = (pass.start_room(), act.start_room());
    assert!(pass_start.abs_diff(act_start) <= 1, "pass {pass_start}, act {act_start}");
}

// what this catches: the start's second allocation (a flex run inside the start) replaced by
// arithmetic that loses a floor or overspends: fixed sources start at their grant, growing ones
// share what is left by what each lacks, nothing past its grant or past the start.
#[test]
fn start_shares_keep_floors_fill_fixed_sources_and_share_the_rest() {
    let sources = [
        StartSource { priority: 9, fixed: true, floor: 100, grant: 2_000 },
        StartSource { priority: 4, fixed: false, floor: 200, grant: 6_000 },
        StartSource { priority: 4, fixed: false, floor: 200, grant: 2_000 },
    ];
    let shares = start_shares(5_000, &sources);
    assert_eq!(shares[0], 2_000, "the fixed source starts at its grant");
    assert!(shares[1] >= 200 && shares[2] >= 200, "every floor kept: {shares:?}");
    assert!(shares.iter().zip(&sources).all(|(share, s)| *share <= s.grant), "{shares:?}");
    assert!(shares.iter().sum::<u32>() <= 5_000, "{shares:?}");
    assert!(shares[1] > shares[2], "a source that lacks more gets more: {shares:?}");
}

/// One recorded fact and its receipt, the dyno's shape (fable/dyno `capacity/dyno/2026-11-04.json`).
#[derive(serde::Deserialize)]
struct Fact<T> {
    value: T,
    receipt: String,
}

/// Aris's view that day, as the dyno records it.
#[derive(serde::Deserialize)]
struct ArisView {
    view_tokens: usize,
    append_chars_per_act: [usize; 2],
    prompt_tokens: usize,
}

/// The 2026-11-04 recorded inputs reproduction 7 is driven by (`2026-11-04.json` beside this file).
#[derive(serde::Deserialize)]
struct Recorded {
    aris_view: Fact<ArisView>,
    aris_acts: Fact<usize>,
    aris_window: Fact<u32>,
    head_chars: Fact<usize>,
    reply_tokens: Fact<u32>,
    m5_prefill_tok_per_s_large: Fact<usize>,
}

/// The day's recording; every fact carries its receipt.
fn recorded() -> Recorded {
    let day: Recorded = serde_json::from_str(include_str!("2026-11-04.json")).expect("test: the recording parses"); // JUSTIFIED: test fixture
    for receipt in [
        &day.aris_view.receipt,
        &day.aris_acts.receipt,
        &day.aris_window.receipt,
        &day.head_chars.receipt,
        &day.reply_tokens.receipt,
        &day.m5_prefill_tok_per_s_large.receipt,
    ] {
        assert!(!receipt.is_empty(), "a recorded number without a receipt is not a receipt");
    }
    day
}


/// Prompt tokens of ASCII text, in the faculty's estimate unit (`token_estimate`).
fn tokens(chars: usize) -> usize {
    chars / continuum_base::token_estimate::GUARD_CHARS_PER_TOKEN
}

/// A read's output of `chars` characters, in lines (so it has an output summary level).
fn output(act: usize, chars: usize) -> String {
    let line = format!("act {act}: a line of the file she read, as long as source lines run\n");
    line.repeat(chars / line.len() + 1)
}

/// One act of the epoch policy over recorded items: the act's one allocation, then her view kept,
/// or turned over at the start the plan's arithmetic sizes. Answers (prompt tokens, prefill tokens,
/// turned over).
fn act_through_the_view(day: &Recorded, view: &mut View<String>, sources: &[&dyn Source<String>], appended_tokens: usize) -> (usize, usize, bool) {
    let source_claims = view_claims(sources);
    let as_claim = |claim: &Claim| view_source(&claim.source, claim.priority, tokens(claim.floor_chars) as u32, tokens(claim.full_chars) as u32);
    let act_claims = ActClaims {
        framing: tokens(day.head_chars.value) as u32,
        required: 0,
        newest: Newest::default(),
        tools: Span::default(),
        grounding: Span::default(),
        history: Span::default(),
        reply: reply(day.reply_tokens.value, day.reply_tokens.value, 8),
        view: source_claims.iter().map(as_claim).collect(),
    };
    let plan = allocate(day.aris_window.value, act_claims);
    assert!(plan.irreducible_shortfall().is_none(), "her working set fits her window: no grant growth");
    let view_tokens = tokens(view.chars());
    if view_tokens <= (plan.view_granted() + plan.room()) as usize {
        return (tokens(day.head_chars.value) + view_tokens, appended_tokens, false);
    }
    let start = plan.start_room();
    let grant_of = |claim: &Claim| plan.plan.of(&format!("{}{}", id::VIEW, claim.source)).cloned();
    view.turn_over(Vec::new(), sources, |claims: &[Claim]| {
        let starts: Vec<StartSource> = claims
            .iter()
            .map(|claim| {
                let grant = grant_of(claim).map_or(0, |a| a.allocated_tokens); // map_or: an unclaimed source starts collapsed
                StartSource { priority: claim.priority, fixed: false, floor: tokens(claim.floor_chars) as u32, grant }
            })
            .collect();
        start_shares(start, &starts)
            .iter()
            .zip(claims)
            .map(|(&share, claim)| (share as usize * claim.full_chars) / tokens(claim.full_chars).max(1))
            .collect()
    })
    .expect("test: the grants answer every claim"); // JUSTIFIED: test fixture
    let prompt = tokens(day.head_chars.value) + tokens(view.chars());
    (prompt, prompt, true)
}

// what this catches (the dyno's reproduction 7; the M5, 2026-11-04 18:49-19:22Z): history kept
// verbatim. Aris appended 17-43k chars an act on a ~44k-token view, her prompts reached ~89k
// tokens and turned over on most acts (at 148 tok/s a cold re-prefill of 89k is ~600 s). Driven
// by her recorded items through the act's one allocation and the view: every prompt fits her
// window, older items collapse to a level she re-expands by handle, her newest item stays whole,
// and nothing asks for a bigger grant. Reports prompt and prefill tokens per act, verbatim vs the
// view (no fixed target; run with --nocapture).
#[test]
fn reproduction_7_an_overflowing_view_collapses_older_items_and_prompt_tokens_stay_at_the_working_set() {
    let day = recorded();
    let mut actions = Fake::shaped("actions", 4);
    let seed_chars = day.aris_view.value.view_tokens * continuum_base::token_estimate::GUARD_CHARS_PER_TOKEN;
    let seeds = 4;
    for n in 0..seeds {
        actions.push(&format!("seed{n}"), &output(n, seed_chars / seeds));
    }
    let whole = |claims: &[Claim]| claims.iter().map(|claim| claim.full_chars).collect();
    let (mut view, _) = View::start(Vec::new(), &[&actions as &dyn Source<String>], whole).expect("test: a start"); // JUSTIFIED: test fixture
    let mut verbatim = tokens(day.head_chars.value) + tokens(view.chars());
    let mut rows = Vec::new();
    for act in 0..day.aris_acts.value {
        let appended = day.aris_view.value.append_chars_per_act[act % 2];
        let key = format!("act{act}");
        actions.push(&key, &output(act, appended));
        verbatim += tokens(appended);
        let advance = view.advance(&[&actions as &dyn Source<String>]);
        let (prompt, prefill, turned_over) = act_through_the_view(&day, &mut view, &[&actions as &dyn Source<String>], tokens(advance.chars));
        view.sent();
        assert!(
            prompt + day.reply_tokens.value as usize <= day.aris_window.value as usize,
            "act {act}: prompt {prompt} + reply over her {} window",
            day.aris_window.value
        );
        assert!(view.renders("actions", &key), "act {act}: her newest item is shown, whole or at its summary");
        rows.push((act, verbatim, prompt, prefill, turned_over));
    }
    let seed0 = crate::cause::ElementId::of(&[b"seed0".as_slice()]);
    let oldest = view.receipt().items.into_iter().find(|rendered| rendered.element == Some(seed0));
    assert!(
        oldest.is_none_or(|rendered| rendered.level != crate::level::Level::Full),
        "her oldest read is at a level she re-expands by handle, never kept verbatim"
    );
    assert!(rows.iter().any(|row| row.1 > day.aris_window.value as usize), "precondition: verbatim history outgrows her window (red before)");
    println!("recorded that day: her prompts reached {} tokens", day.aris_view.value.prompt_tokens);
    println!("act | verbatim prompt | view prompt | prefill | turned over | prefill s at {} tok/s", day.m5_prefill_tok_per_s_large.value);
    for (act, verbatim, prompt, prefill, turned_over) in &rows {
        println!("{act} | {verbatim} | {prompt} | {prefill} | {turned_over} | {}", prefill / day.m5_prefill_tok_per_s_large.value);
    }
}
