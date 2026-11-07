#![cfg(test)]
//! Every budgeted part of her prompt collapses by its levels instead of refusing her turn, priced
//! against the REAL registry the M5 served (Aiko, 2026-11-07).

use super::*;

/// A standing board the size of the live one: its leads, then one line per card.
fn board_like(units: usize) -> Contribution {
    let mut parts = vec!["[your work] you HOLD 1 card — 0dd1123c \"wire the gather fallback\"".to_string(), format!("[available work] {units} card(s) are claimable — pick one up with work/claim")];
    parts.extend((0..units).map(|i| format!("card {i:08x} [Open] \"a card whose title is about as long as a real one\" (Normal, unclaimed)")));
    Contribution::context(FacultyId::Custom("room-kanban".to_string()), parts.join("\n"), 0.9, "board").with_parts(parts)
}
use continuum_context::allocation::{allocate, id, ActClaims, Head, Span};
use continuum_context::ladder::{Leveled, Rungs};

/// Aiko's served window on the M5 (`per_slot_window`, fe91c2b55, 2026-11-07 ~07:00-07:40Z).
// context-budget-exempt: the MEASURED served window in Aiko's incident, a fixture input that
// reproduces it, not a budget the code sizes anything by.
const AIKO_WINDOW: u32 = 30_566;

/// Her act as the M5 served it that morning: her whole native surface (the live registry's own
/// schemas, every verb her room put in her hands), a long conversation, her newest result, a
/// standing board and an indivisible repo map in her grounding, a deliverable (an act), and her
/// act's measured output need (`need`: think plus answer, recorded as her emissions are).
fn aikos_act(persona: Uuid, need: Option<u32>) -> (LlmDeliberationFaculty, Workspace) {
    use crate::ai::types::ResultCustody;
    use crate::cognition::working_memory::WorkingMemory;
    use crate::cognition::working_set::{EmissionStop, WorkingSetRegistry};
    use crate::cognition::workspace::Burst;
    let wm = Arc::new(WorkingMemory::new(8));
    let result: String = (0..60).map(|i| format!("src/lib.rs:{i}: pub fn handle_{i}(request: &Request) -> Response {{ … }}\n")).collect();
    wm.record_receipt_typed(&[], &result, None, ResultCustody::Whole { handle: "aiko-read".into() });
    let registry = WorkingSetRegistry::new();
    if let Some(need) = need {
        for tick in 1..=3 {
            registry.record_emission_in_memory(persona, need, need / 2, EmissionStop::Landed, tick);
        }
    }
    let faculty = LlmDeliberationFaculty::new(persona, "Aiko", "You are Aiko.", Arc::new(HeuristicInferenceAdapter::new()))
        .with_context_window(AIKO_WINDOW)
        .with_tools(persona_tools::native_tool_specs())
        .with_working_set(registry)
        .with_working_memory(wm);
    let room = crate::identity::ActivityRoom::mint();
    let turns: Vec<_> = (0..120)
        .map(|i| {
            BurstTurn::attributed(i % 2 == 1, if i % 2 == 1 { "Aiko" } else { "Peer" }, format!("turn {i}: what the room said about the change she is making, at length. ").repeat(4), Some(i as u64))
        })
        .collect();
    let mut ws = Workspace::new(Burst::from_turns(room, turns));
    ws.workspace_deliverable = true;
    // Every native verb in her hands: her surface whole is what refused her (11,377 tokens claimed
    // whole on the M5; this tree's registry offers more).
    ws = ws.room_affordances(faculty.tool_surface().names().to_vec());
    ws.broadcast.push(board_like(60).with_expand_command(Some("work/list")).standing_grounding());
    let map: String = (0..400).map(|i| format!("src/module_{i:03}/mod.rs\n")).collect();
    ws.broadcast.push(Contribution::context(FacultyId::Custom("workspace-map".into()), map, 0.9, "map"));
    (faculty, ws)
}

/// The receipt's sum over `pick` of `field`.
fn sum(view: &DeliberationPromptView, pick: impl Fn(&str) -> bool, field: impl Fn(&crate::ai::provenance::ClaimGrant) -> u32) -> u32 {
    view.allocation.as_ref().map_or(0, |r| r.claims.iter().filter(|c| pick(&c.source)).map(&field).sum()) // map_or: no receipt sums nothing
}

/// Her act's parts as canary priced them: her hands, her framing and her grounding with no level
/// between whole and absent (hands and framing required whole, grounding whole or nothing), from
/// this act's own measured parts (`receipt`).
fn as_canary_priced(view: &DeliberationPromptView) -> ActClaims {
    let receipt = view.allocation.as_ref().expect("test: the view names its allocation"); // JUSTIFIED: test fixture
    let want = |source: &str| receipt.claims.iter().filter(|c| c.source == source).map(|c| c.want).sum::<u32>();
    let floor = |source: &str| receipt.claims.iter().filter(|c| c.source == source).map(|c| c.floor).sum::<u32>();
    let whole = |source: &str| Leveled::one(Rungs::whole(want(source)));
    let view_claims = receipt
        .claims
        .iter()
        .filter(|c| c.source.starts_with(id::VIEW))
        .map(|c| continuum_context::flex::FlexClaim { source_id: c.source.clone(), priority: c.priority, floor_tokens: c.floor, min_tokens: c.floor, max_tokens: c.want, required: true })
        .collect();
    let reply = receipt.claims.iter().find(|c| c.source == id::REPLY).expect("test: a reply claim"); // JUSTIFIED: test fixture
    ActClaims {
        framing: Head::free(whole(id::FRAMING)),
        required: whole(id::REQUIRED),
        newest: Rungs::new([want(id::NEWEST), floor(id::NEWEST)]).map_or_else(Leveled::default, Leveled::one), // map_or_else: no result
        tools: Head::free(whole(id::TOOLS)),
        grounding: Rungs::new([want(id::GROUNDING), 0]).map_or_else(Leveled::default, Leveled::one), // map_or_else: no grounding offered
        last_move: whole(id::LAST_MOVE),
        history: Span { floor: 0, min: 0, want: want(id::HISTORY) },
        reply: continuum_context::flex::FlexClaim { source_id: id::REPLY.into(), priority: reply.priority, floor_tokens: reply.floor, min_tokens: reply.floor, max_tokens: reply.want, required: true },
        view: view_claims,
        price: continuum_context::price::ContextPrice::Unmeasured,
    }
}

// what this catches (Joel, 2026-11-07: "The rag budgeting should collapse rather than crap out and
// positron ought to flexbox not kill"; Aiko on the M5, fe91c2b55: tools 11,190 granted of 11,377
// whole, framing 2,749, history 38, grounding 0, then "the turn's IRREDUCIBLE floors do not fit
// the served window" and "assembled context suppressed: zero token budget"): a budgeted part with
// no level between whole and nothing. Her act on her 30,566 window, priced as canary priced it,
// refuses or starves her history and grounding; through the real `prompt_view` every part reduces
// by its levels: nothing refuses, her hands collapse with every verb still sent and callable or
// expandable, her grounding arrives at a level, her history is granted and her newest result is
// whole. Prints both allocations (run with --nocapture): the PR's Measured section.
#[test]
fn aikos_act_fits_her_window_by_collapsing_her_hands_and_grounding_instead_of_refusing() {
    let persona = Uuid::new_v4();
    // Her act's measured need, derived from this act's own parts (never a guessed size): what puts
    // her parts with her hands WHOLE past her window by half of what her hands' levels give back,
    // the shape of her refusal on the M5 (her hands whole and framing past what her other floors
    // left). With her hands at their hard limit the same act fits by the other half.
    let probe = {
        let (wide, ws) = aikos_act(persona, None);
        wide.prompt_view_within(&ws, AIKO_WINDOW * 4)
    };
    let hard = |source: &str| ["framing", "required_input", "newest_result", "tools", "last_move"].contains(&source) || source.starts_with("view/");
    let limits = sum(&probe, hard, |c| c.floor);
    let tools_whole = sum(&probe, |s| s == "tools", |c| c.grant);
    let tools_limit = sum(&probe, |s| s == "tools", |c| c.floor);
    let gives_back = tools_whole - tools_limit;
    let target = AIKO_WINDOW - (limits - tools_limit + tools_whole) + gives_back / 2;
    // Recorded as her emissions are; her act reserves her need with its measured headroom, so the
    // recording is scaled once by what the first one reserved.
    let reserved = |recorded: u32| {
        let (faculty, ws) = aikos_act(persona, Some(recorded));
        sum(&faculty.prompt_view_within(&ws, AIKO_WINDOW * 4), |s| s == "reply", |c| c.floor)
    };
    let first = reserved(target).max(1);
    let need = (u64::from(target) * u64::from(target) / u64::from(first)) as u32;
    let (faculty, ws) = aikos_act(persona, Some(need));
    let roomy = {
        let (wide, _) = aikos_act(persona, Some(need));
        wide.prompt_view_within(&ws, AIKO_WINDOW * 4)
    };
    let whole_tools = roomy.tools.tokens;
    let view = faculty.prompt_view_within(&ws, AIKO_WINDOW);
    let receipt = view.allocation.as_ref().expect("test: the view names its allocation"); // JUSTIFIED: test fixture
    let grant = |source: &str| receipt.claims.iter().filter(|c| c.source == source).map(|c| c.grant).sum::<u32>();
    let before = allocate(AIKO_WINDOW, as_canary_priced(&roomy));
    let row = |label: &str, framing: u32, tools: u32, newest: u32, history: u32, grounding: u32, view: u32, reply: u32, short: &str| {
        println!("{label:<8} | {framing:>7} | {tools:>6} | {newest:>6} | {history:>7} | {grounding:>9} | {view:>6} | {reply:>5} | {short}");
    };
    println!(
        "window {AIKO_WINDOW}; her hands whole {whole_tools} tokens, at their pointers {tools_limit} ({} verbs, the live registry's schemas); her act's reply need {}",
        faculty.tool_surface().names_of(&roomy.tools).len(),
        sum(&roomy, |s| s == "reply", |c| c.floor)
    );
    println!("          | framing |  tools | newest | history | grounding |   view | reply | refused");
    let short_before = before.irreducible_shortfall().map_or_else(String::new, |s| format!("{} of {} needed (binding {})", s.needed, s.window, s.binding)); // map_or_else: nothing short
    row("canary", before.granted(id::FRAMING), before.granted(id::TOOLS), before.granted(id::NEWEST), before.granted(id::HISTORY) + before.granted(id::LAST_MOVE), before.granted(id::GROUNDING), before.view_granted(), before.granted(id::REPLY), &short_before);
    let view_grant: u32 = receipt.claims.iter().filter(|c| c.source.starts_with(id::VIEW)).map(|c| c.grant).sum();
    let short_after = view.capacity_error.as_ref().map_or_else(String::new, |e| format!("{e:?}")); // map_or_else: nothing refused
    row("collapse", grant(id::FRAMING), grant(id::TOOLS), grant(id::NEWEST), grant(id::HISTORY) + grant(id::LAST_MOVE), grant(id::GROUNDING), view_grant, grant(id::REPLY), &short_after);

    assert!(whole_tools > AIKO_WINDOW as usize / 4, "precondition: her hands whole are a large share of her window ({whole_tools})");
    assert!(
        before.irreducible_shortfall().is_some(),
        "precondition (red on canary): priced with no levels, her act's floors do not fit her window"
    );
    assert!(view.capacity_error.is_none() && view.budget_fault.is_none(), "nothing refuses: {:?} {:?}", view.capacity_error, view.budget_fault);
    assert!(view.output_shortfall.is_none(), "her act's need fits beside her collapsed parts: {:?}", view.output_shortfall);
    assert!(view.tools.tokens < whole_tools, "her hands collapsed: {} of {whole_tools}", view.tools.tokens);
    assert!(grant(id::GROUNDING) > 0, "her grounding arrives at a level, never suppressed");
    assert!(grant(id::HISTORY) + grant(id::LAST_MOVE) + view_grant > 0, "her history is granted");
    assert!(!view.newest_result_reduced, "her newest result whole");
    // Every verb still in her hands, each whole, at its summary (callable with every parameter) or
    // at its pointer, which names how to open it.
    let names = faculty.tool_surface().names_of(&roomy.tools);
    assert_eq!(faculty.tool_surface().names_of(&view.tools), names, "no verb dropped");
    let wholes = roomy.tools.specs.as_deref().expect("test: her surface"); // JUSTIFIED: test fixture
    for (spec, whole) in view.tools.specs.as_deref().expect("test: her surface").iter().zip(wholes) { // JUSTIFIED: test fixture
        let params = |s: &crate::ai::types::NativeToolSpec| s.input_schema.properties.as_object().map(|p| p.keys().cloned().collect::<Vec<_>>()).unwrap_or_default(); // JUSTIFIED: test fixture
        assert_eq!(params(spec), params(whole), "{}: every parameter at every level", spec.name);
        let summary = crate::cognition::tool_surface::VerbLevels::of(whole, "", "", crate::cognition::tool_surface::Floor::Summary);
        let is_summary = (0..summary.len()).any(|level| summary.spec(level) == spec);
        assert!(spec == whole || is_summary || spec.description.ends_with("shows its parameters"), "{}: whole, its summary, or its pointer", spec.name);
    }
    let text: String = view.messages.iter().map(|m| m.content_text()).collect();
    assert!(text.contains("[room-kanban]"), "the standing board reaches her at a level");
}
