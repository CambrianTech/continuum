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

/// One item at `levels`, most detail first.
fn item(levels: &[u32]) -> Rungs {
    Rungs::new(levels.iter().copied()).expect("test: levels") // JUSTIFIED: test fixture
}

/// Her newest result at `levels`, whole first; none when empty.
fn newest_at(levels: &[u32]) -> Leveled {
    Rungs::new(levels.iter().copied()).map_or_else(Leveled::default, Leveled::one) // map_or_else: no levels is no result
}

fn claims(window_newest: Leveled, view: Vec<FlexClaim>, reply_priority: u8) -> ActClaims {
    ActClaims {
        framing: Head::free(Leveled::one(Rungs::whole(4_000))),
        required: Leveled::one(Rungs::whole(500)),
        newest: window_newest,
        tools: Head::free(Leveled::one(item(&[3_000, 1_000]))),
        grounding: Leveled::one(item(&[1_200, 300])),
        last_move: Leveled::one(Rungs::whole(200)),
        history: Span { floor: 0, min: 0, want: 600 },
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
    let newest = || newest_at(&[9_000, 2_000, 900]);
    let view = || vec![view_source("conversation", 4, 600, 20_000)];
    let roomy = allocate(40_000, claims(newest(), view(), 8));
    assert_eq!((roomy.newest_level(), roomy.granted(id::NEWEST)), (0, 9_000), "whole");
    // Required floors: framing 4,000 + required 500 + tools 1,000 + last move 200 + reply 800 =
    // 6,500; 12,000 leaves 5,500: the 2,000 level fits, the whole does not.
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
// 2,000 granted, with the required floors and the whole within 15,500). It is chosen against the
// required floors (her last move among them, now required) and the reply's; older history,
// grounding and her view shed in the floor pass.
#[test]
fn optional_claims_shed_before_her_newest_result_is_shown_smaller() {
    let view = vec![view_source("conversation", 4, 600, 20_000)];
    let plan = allocate(15_500, claims(newest_at(&[9_000, 2_000, 900]), view, 8));
    assert_eq!((plan.newest_level(), plan.granted(id::NEWEST)), (0, 9_000), "whole: it fits beside every required floor");
    assert_eq!(plan.granted(id::LAST_MOVE), 200, "her last move whole");
    let optional = plan.granted(id::HISTORY) + plan.view_granted() + plan.granted(id::GROUNDING);
    assert_eq!(optional, 0, "older history, grounding and her view shed first: the required parts fill 15,500");
    assert!(plan.irreducible_shortfall().is_none());
}

// what this catches (Cormac on #5779, capacity_and_wire's 8,192 case): a reply whose measured need
// exceeds the window displacing her view, so her view was shed (or dropped from the prompt and
// re-prefilled whole the act after, breaking append-only). On an act her view's floor is required:
// it keeps at least its floor, the reply gets exactly what the required floors leave, and the
// excess is the typed IrreducibleShortfall (needed, window) for admission, while this act proceeds.
#[test]
fn a_reply_need_past_the_window_keeps_her_view_floor_and_reports_the_shortfall() {
    let mut view = view_source("conversation", 4, 600, 20_000);
    view.required = true; // an act: her view at its collapsed minimum is required
    let mut act = claims(Leveled::default(), vec![view], 8);
    act.reply = reply(9_000, 9_000, 8);
    let window = 8_192;
    let plan = allocate(window, act);
    assert!(plan.view_granted() >= 600, "her view keeps its floor: {}", plan.view_granted());
    // Required floors: framing 4,000 + required 500 + tools 1,000 + last move 200 + view 600 = 6,300.
    assert_eq!(plan.granted(id::REPLY), window - 6_300, "the reply gets what the required floors leave");
    assert_eq!(
        plan.irreducible_shortfall(),
        Some(IrreducibleShortfall { needed: 6_300 + 9_000, window, binding: id::REPLY.to_owned(), binding_limit: 9_000 }),
        "the reply's measured need is the binding hard limit"
    );
    // At an epoch START (Codex on #5779): the reply's want beyond its grant left a start room of 0,
    // so `start_shares` could drop the required 600. The start holds every required view floor.
    let start = plan.start_room();
    assert!(start >= 600, "a start holds her required view floor: {start}");
    let source = plan.plan.of("view/conversation").expect("test: the view claim was made"); // JUSTIFIED: test fixture
    let shares = start_shares(start, &[StartSource { priority: 4, fixed: false, required: source.required, floor: source.requested_floor, grant: source.allocated_tokens, newest: 0 }])
        .expect("test: the start holds the required floor"); // JUSTIFIED: test fixture
    assert!(shares[0] >= 600, "her view starts at least at its required floor: {shares:?}");
}

// what this catches (Codex on #5779): a start too small for a required floor silently dropping it.
// It is a typed refusal the view surfaces, never a source started empty.
#[test]
fn a_start_too_small_for_a_required_floor_is_refused_typed() {
    let sources = [
        StartSource { priority: 4, fixed: false, required: true, floor: 600, grant: 600, newest: 0 },
        StartSource { priority: 6, fixed: true, required: false, floor: 50, grant: 400, newest: 0 },
    ];
    assert_eq!(start_shares(500, &sources), Err(StartRefused::OverRoom { required: 600, start: 500 }));
    let shares = start_shares(700, &sources).expect("test: the required floor fits"); // JUSTIFIED: test fixture
    assert_eq!(shares[0], 600, "the required floor first");
    assert!(shares[1] <= 100, "the optional source shares what is left: {shares:?}");
}

// what this catches (Codex on #5779: "required means refuse if underfunded, not mint budget"): a
// required floor its grant did not cover being started anyway, at more than the plan gave it. A
// floor of 600 granted 500 is refused typed even when the start itself could hold 600.
#[test]
fn an_underfunded_required_floor_is_refused_never_minted() {
    let sources = [StartSource { priority: 4, fixed: false, required: true, floor: 600, grant: 500, newest: 0 }];
    assert_eq!(start_shares(600, &sources), Err(StartRefused::Underfunded { floor: 600, grant: 500 }));
    let funded = [StartSource { grant: 600, ..sources[0] }];
    let shares = start_shares(600, &funded).expect("test: a funded floor starts"); // JUSTIFIED: test fixture
    assert!(shares.iter().zip(&funded).all(|(share, s)| *share <= s.grant), "no share past its grant: {shares:?}");
}

// what this catches (Codex on Cormac's finding, #5779, goal 7): an off-by-one at the exact
// required-floor boundary. When the required floors, the reply's floor and the whole newest result
// equal the window exactly, the whole is chosen; one token less and the next level is.
#[test]
fn the_newest_result_is_whole_at_the_exact_boundary_and_the_next_level_one_token_under() {
    let newest = || newest_at(&[9_000, 2_000, 900]);
    let view = || vec![view_source("conversation", 4, 600, 20_000)];
    // Required: framing 4,000 + required 500 + tools 1,000 + last move 200 + reply 800 = 6,500.
    let exact = 6_500 + 9_000;
    let at = allocate(exact, claims(newest(), view(), 8));
    assert_eq!((at.newest_level(), at.granted(id::NEWEST)), (0, 9_000), "exactly the window: whole");
    assert!(at.irreducible_shortfall().is_none());
    let under = allocate(exact - 1, claims(newest(), view(), 8));
    assert_eq!((under.newest_level(), under.granted(id::NEWEST)), (1, 2_000), "one token under: the next level");
}

// what this catches (goal 7: the newest complete interaction is whole; ONE-ACT stage 2; Cormac on
// #5779): her last move was the floor of an OPTIONAL history claim, so on a tight window the
// reply's cap (or any optional claim's floor) could take it. It is its own required claim: on a
// tight window her last move survives whole while older history and her optional view shed, and
// her reply keeps its measured floor.
#[test]
fn on_a_tight_window_her_last_move_and_reply_floor_survive_and_optional_claims_shed() {
    let view = vec![view_source("conversation", 4, 600, 20_000)];
    // Required: framing 4,000 + required 500 + tools 1,000 + last move 200 + reply 800 = 6,500.
    let plan = allocate(6_600, claims(Leveled::default(), view, 8));
    assert_eq!(plan.granted(id::LAST_MOVE), 200, "her last move whole");
    assert!(plan.granted(id::REPLY) >= 800, "her reply keeps its floor: {}", plan.granted(id::REPLY));
    assert!(plan.view_granted() < 600, "the optional view source shed below its floor: {}", plan.view_granted());
    assert!(plan.granted(id::HISTORY) <= 100, "older history sheds: {}", plan.granted(id::HISTORY));
    assert!(plan.irreducible_shortfall().is_none());
}

// what this catches: required floors past the window answered by collapsing history (or by a
// bigger grant on a reducible overflow). Irreducible growth is typed apart.
#[test]
fn required_floors_past_the_window_are_an_irreducible_shortfall() {
    let plan = allocate(5_000, claims(newest_at(&[9_000, 900]), Vec::new(), 8));
    let short = plan.irreducible_shortfall().expect("test: the floors exceed 5,000"); // JUSTIFIED: test fixture
    assert_eq!(short.window, 5_000);
    assert!(short.needed > 5_000, "{short:?}");
}

// what this catches: her view's start moving with the reply's priority (one 16,384 window started
// her conversation at 4,005 tokens on a pass and 2,402 on the act, so her prefix moved), which a
// second allocation with the reply seated at its want used to prevent. One plan, the same start.
#[test]
fn a_pass_and_the_act_after_it_start_her_view_alike() {
    let newest = Leveled::default;
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
        StartSource { priority: 9, fixed: true, required: false, floor: 100, grant: 2_000, newest: 0 },
        StartSource { priority: 4, fixed: false, required: false, floor: 200, grant: 6_000, newest: 0 },
        StartSource { priority: 4, fixed: false, required: false, floor: 200, grant: 2_000, newest: 0 },
    ];
    let shares = start_shares(5_000, &sources).expect("test: no required floor"); // JUSTIFIED: test fixture
    assert_eq!(shares[0], 2_000, "the fixed source starts at its grant");
    assert!(shares[1] >= 200 && shares[2] >= 200, "every floor kept: {shares:?}");
    assert!(shares.iter().zip(&sources).all(|(share, s)| *share <= s.grant), "{shares:?}");
    assert!(shares.iter().sum::<u32>() <= 5_000, "{shares:?}");
    assert!(shares[1] > shares[2], "a source that lacks more gets more: {shares:?}");
}

// what this catches: her newest turn collapsing on an act and not on the pass before it. The
// growing sources shared the start by what each lacks of its grant, and the reply's priority moves
// the grants (her conversation 3,197 tokens on a pass, 2,003 on the act, one 16,384 window), so the
// same start left the conversation 449 tokens on the pass and 437 on the act, 1 char short of her
// newest turn (#5807's CI, 2026-11-06). Each growing source's newest item comes first, so the
// start holds it on both turns whatever the grants.
#[test]
fn a_growing_source_holds_its_newest_item_whatever_the_grants_divide() {
    let start = |conversation_grant| {
        let sources = [
            StartSource { priority: 5, fixed: false, required: false, floor: 120, grant: 120, newest: 0 },
            StartSource { priority: 5, fixed: false, required: false, floor: 0, grant: conversation_grant, newest: 438 },
            StartSource { priority: 6, fixed: true, required: false, floor: 196, grant: 196, newest: 0 },
            StartSource { priority: 5, fixed: false, required: false, floor: 0, grant: 57, newest: 57 },
        ];
        start_shares(766, &sources).expect("test: no required floor") // JUSTIFIED: test fixture
    };
    let (pass, act) = (start(3_197), start(2_003));
    assert!(pass[1] >= 438 && act[1] >= 438, "the newest turn is held on both: pass {pass:?}, act {act:?}");
    assert!(pass.iter().sum::<u32>() <= 766 && act.iter().sum::<u32>() <= 766);
}

// what this catches (Joel, 2026-11-07: "It should know how each budgeted item reduces or
// collapses"): a budgeted part whose floor is declared beside its items instead of derived from
// them, so a small window refuses a turn its items could have collapsed into. Every leveled
// part's claim is at least its items' hard limit, the plan's irreducible sum is exactly those
// limits, and on a window that holds only the limits each part sits at them and nothing refuses.
#[test]
fn every_leveled_claims_floor_is_its_items_hard_limit() {
    let act = || ActClaims {
        framing: Head::free(Leveled::new(vec![Rungs::whole(1_900), item(&[40, 11]), item(&[60, 12])])),
        required: Leveled::one(Rungs::whole(300)),
        newest: newest_at(&[2_000, 781]),
        tools: Head::free(Leveled::new((0..23).map(|_| item(&[480, 120, 25])).collect())),
        grounding: Leveled::new(vec![item(&[900, 200, 30]), item(&[400, 40])]),
        last_move: Leveled::one(Rungs::whole(38)),
        history: Span { floor: 0, min: 0, want: 4_000 },
        reply: reply(446, 2_000, 10),
        view: vec![FlexClaim { required: true, ..view_source("conversation", 4, 1_500, 9_000) }],
    };
    let limits = 1_900 + 11 + 12 + 300 + 23 * 25 + 38 + 1_500;
    let roomy = allocate(60_000, act());
    for (id, limit) in [(id::FRAMING, 1_923), (id::TOOLS, 23 * 25)] {
        let claim = roomy.claims().iter().find(|c| c.source_id == id).expect("test: claimed"); // JUSTIFIED: test fixture
        assert!(claim.floor_tokens >= limit, "{id}: floor {} below its hard limit {limit}", claim.floor_tokens);
    }
    assert_eq!(roomy.chosen(id::TOOLS).map(|c| c.cost), Some(23 * 480), "a roomy window shows every verb whole");
    assert_eq!(roomy.irreducible(), limits + 2_000, "the irreducible sum is the items' hard limits, her newest whole");
    let tight = allocate(limits + 781 + 446, act());
    assert!(tight.irreducible_shortfall().is_none(), "every part at its hard limit fits: nothing refuses");
    assert_eq!(tight.chosen(id::TOOLS).map(|c| c.cost), Some(23 * 25), "every verb at its pointer");
    assert_eq!(tight.chosen(id::GROUNDING).map(|c| c.cost), Some(0), "grounding leaves rather than refusing");
    let over = allocate(limits + 781 + 445, act());
    let short = over.irreducible_shortfall().expect("test: one token under every hard limit"); // JUSTIFIED: test fixture
    assert_eq!((short.binding.as_str(), short.binding_limit), (id::FRAMING, 1_923), "the binding hard limit is named");
}

// what this catches: her head's levels moving mid-epoch (every level change at the head re-prefills
// the whole prompt), held levels kept on a window that can no longer hold them (refusing instead of
// re-choosing), or a collapse that never rises again (both ways). Held levels stay while they fit;
// a window that cannot hold them re-chooses and says so; a held level below what the window now
// buys is reported as the rise for the next start of her head's epoch.
#[test]
fn held_head_levels_stay_while_they_fit_are_lost_loudly_and_rise_at_the_next_epoch() {
    let tools = || Leveled::new((0..4).map(|_| item(&[500, 100, 20])).collect());
    let act = |held: Vec<usize>| ActClaims {
        tools: Head { leveled: tools(), held: Some(held) },
        ..claims(Leveled::default(), Vec::new(), 8)
    };
    let collapsed = allocate(40_000, act(vec![2, 2, 1, 1]));
    assert_eq!(collapsed.chosen(id::TOOLS).map(|c| c.rungs.clone()), Some(vec![2, 2, 1, 1]), "held while it fits");
    assert!(!collapsed.lost(id::TOOLS));
    assert_eq!(collapsed.rise(id::TOOLS).map(|c| c.rungs.clone()), Some(vec![0, 0, 0, 0]), "the room now holds every verb whole");
    let whole = allocate(5_800, act(vec![0, 0, 0, 0]));
    assert!(whole.lost(id::TOOLS), "2,000 of whole verbs beside 5,500 of other hard limits cannot fit 5,800");
    assert!(whole.chosen(id::TOOLS).is_some_and(|c| c.cost <= 300), "re-chosen within the window: {:?}", whole.chosen(id::TOOLS));
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
    let source_claims = view_claims(&crate::cost::Chars, sources);
    let as_claim = |claim: &Claim| view_source(&claim.source, claim.priority, tokens(claim.floor) as u32, tokens(claim.full) as u32);
    let act_claims = ActClaims {
        framing: Head::free(Leveled::one(Rungs::whole(tokens(day.head_chars.value) as u32))),
        required: Leveled::default(),
        newest: Leveled::default(),
        tools: Head::default(),
        grounding: Leveled::default(),
        last_move: Leveled::default(),
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
    view.turn_over(&crate::cost::Chars, Vec::new(), sources, |claims: &[Claim]| {
        let starts: Vec<StartSource> = claims
            .iter()
            .map(|claim| {
                let grant = grant_of(claim).map_or(0, |a| a.allocated_tokens); // map_or: an unclaimed source starts collapsed
                StartSource { priority: claim.priority, fixed: false, required: false, floor: tokens(claim.floor) as u32, grant, newest: 0 }
            })
            .collect();
        start_shares(start, &starts)
            .expect("test: no required floor in this start") // JUSTIFIED: test fixture
            .iter()
            .zip(claims)
            .map(|(&share, claim)| (share as usize * claim.full) / tokens(claim.full).max(1))
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
    let whole = |claims: &[Claim]| claims.iter().map(|claim| claim.full).collect();
    let (mut view, _) = View::start(&crate::cost::Chars, Vec::new(), &[&actions as &dyn Source<String>], whole).expect("test: a start"); // JUSTIFIED: test fixture
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
