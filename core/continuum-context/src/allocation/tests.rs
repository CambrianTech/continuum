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
// whenever the window holds it beside every floor, else its coarsest level, and the plan says so.
#[test]
fn the_newest_result_is_whole_when_the_window_holds_it_and_at_its_coarsest_floor_when_not() {
    let newest = Newest { coarsest: 900, whole: 9_000 };
    let roomy = allocate(40_000, claims(newest, vec![view_source("conversation", 4, 600, 20_000)], 8));
    assert_eq!(roomy.granted(id::NEWEST), 9_000, "whole");
    let tight = allocate(9_500, claims(newest, vec![view_source("conversation", 4, 600, 20_000)], 8));
    let grant = tight.granted(id::NEWEST);
    assert!((900..9_000).contains(&grant), "at least its coarsest, less than whole: {grant}");
    assert!(tight.irreducible_shortfall().is_none(), "reducible: no growth asked");
}

// what this catches: required floors past the window answered by collapsing history (or by a
// bigger grant on a reducible overflow). Irreducible growth is typed apart.
#[test]
fn required_floors_past_the_window_are_an_irreducible_shortfall() {
    let plan = allocate(5_000, claims(Newest { coarsest: 900, whole: 9_000 }, Vec::new(), 8));
    let short = plan.irreducible_shortfall().expect("test: the floors exceed 5,000"); // JUSTIFIED: test fixture
    assert_eq!(short.window, 5_000);
    assert!(short.needed > 5_000, "{short:?}");
}

// what this catches: her view's start moving with the reply's priority (one 16,384 window started
// her conversation at 4,005 tokens on a pass and 2,402 on the act, so her prefix moved), which a
// second allocation with the reply seated at its want used to prevent. One plan, the same start.
#[test]
fn a_pass_and_the_act_after_it_start_her_view_alike() {
    let newest = Newest { coarsest: 0, whole: 0 };
    let view = || vec![view_source("conversation", 4, 600, 30_000)];
    let pass = allocate(16_384, claims(newest, view(), 4));
    let act = allocate(16_384, claims(newest, view(), 10));
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

/// The 2026-11-04 recorded inputs reproduction 7 is driven by (the dyno's `aris_view`).
mod recorded {
    /// Aris's view at the start of the window, tokens (18:49-19:22Z).
    pub const ARIS_VIEW_TOKENS: usize = 44_000;
    /// Her per-act appends, chars: the recorded range's two ends.
    pub const ARIS_APPENDS: [usize; 2] = [17_000, 43_000];
    /// The acts in that window: 9 in 30 min.
    pub const ARIS_ACTS: usize = 9;
    /// Her window that day (#5773's receipt, 16:55-17:16Z: 55,672 granted).
    pub const ARIS_WINDOW: u32 = 55_672;
    /// The shared head every resident sends (MIND-CONTEXT revision 2026-10-25: ~38.8k chars).
    pub const HEAD_CHARS: usize = 38_800;
    /// Her measured act reply (ONE-BUDGET slice 6: acts measured 809-1,641).
    pub const REPLY_TOKENS: u32 = 1_641;
    /// The M5's prefill rate on large prompts (141,739 tokens at 148 tok/s).
    pub const PREFILL_TOK_PER_S: usize = 148;
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
fn act_through_the_view(view: &mut View<String>, sources: &[&dyn Source<String>], appended_tokens: usize) -> (usize, usize, bool) {
    let source_claims = view_claims(sources);
    let as_claim = |claim: &Claim| view_source(&claim.source, claim.priority, tokens(claim.floor_chars) as u32, tokens(claim.full_chars) as u32);
    let act_claims = ActClaims {
        framing: tokens(recorded::HEAD_CHARS) as u32,
        required: 0,
        newest: Newest { coarsest: 0, whole: 0 },
        tools: Span::default(),
        grounding: Span::default(),
        history: Span::default(),
        reply: reply(recorded::REPLY_TOKENS, recorded::REPLY_TOKENS, 8),
        view: source_claims.iter().map(as_claim).collect(),
    };
    let plan = allocate(recorded::ARIS_WINDOW, act_claims);
    assert!(plan.irreducible_shortfall().is_none(), "her working set fits her window: no grant growth");
    let view_tokens = tokens(view.chars());
    if view_tokens <= (plan.view_granted() + plan.room()) as usize {
        return (tokens(recorded::HEAD_CHARS) + view_tokens, appended_tokens, false);
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
    let prompt = tokens(recorded::HEAD_CHARS) + tokens(view.chars());
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
    let mut actions = Fake::shaped("actions", 4);
    let seed_chars = recorded::ARIS_VIEW_TOKENS * continuum_base::token_estimate::GUARD_CHARS_PER_TOKEN;
    let seeds = 4;
    for n in 0..seeds {
        actions.push(&format!("seed{n}"), &output(n, seed_chars / seeds));
    }
    let whole = |claims: &[Claim]| claims.iter().map(|claim| claim.full_chars).collect();
    let (mut view, _) = View::start(Vec::new(), &[&actions as &dyn Source<String>], whole).expect("test: a start"); // JUSTIFIED: test fixture
    let mut verbatim = tokens(recorded::HEAD_CHARS) + tokens(view.chars());
    let mut rows = Vec::new();
    for act in 0..recorded::ARIS_ACTS {
        let appended = recorded::ARIS_APPENDS[act % 2];
        let key = format!("act{act}");
        actions.push(&key, &output(act, appended));
        verbatim += tokens(appended);
        let advance = view.advance(&[&actions as &dyn Source<String>]);
        let (prompt, prefill, turned_over) = act_through_the_view(&mut view, &[&actions as &dyn Source<String>], tokens(advance.chars));
        view.sent();
        assert!(
            prompt + recorded::REPLY_TOKENS as usize <= recorded::ARIS_WINDOW as usize,
            "act {act}: prompt {prompt} + reply over her {} window",
            recorded::ARIS_WINDOW
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
    assert!(rows.iter().any(|row| row.1 > recorded::ARIS_WINDOW as usize), "precondition: verbatim history outgrows her window (red before)");
    println!("act | verbatim prompt | view prompt | prefill | turned over | prefill s at {} tok/s", recorded::PREFILL_TOK_PER_S);
    for (act, verbatim, prompt, prefill, turned_over) in &rows {
        println!("{act} | {verbatim} | {prompt} | {prefill} | {turned_over} | {}", prefill / recorded::PREFILL_TOK_PER_S);
    }
}
