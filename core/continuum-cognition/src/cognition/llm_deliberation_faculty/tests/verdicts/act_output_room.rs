#![cfg(test)]
//! An act's output room (`act_room.rs`): her think plus answer as one required claim, priced at her
//! measured density behind the engine's exact count. Pinned from the live cut-think regression
//! (the M5, 2026-11-04 21:31Z-01:09Z, build a95afe3d1, 414 acts: 302 cut at max_tokens exactly,
//! caps 72-89 while the engine's count left a median 2,517 free).

use super::*;
use crate::ai::provenance::PromptAdmission;
use crate::ai::types::ThinkBudget;
use crate::cognition::working_set::{EmissionStop, WorkingSetRegistry};

/// A mind whose acts measured `reasoning` + `answer` (three landed acts).
fn measured(persona: Uuid, reasoning: u32, answer: u32) -> WorkingSetRegistry {
    let reg = WorkingSetRegistry::new();
    for tick in 1..=3 {
        reg.record_emission_in_memory(persona, reasoning + answer, reasoning, EmissionStop::Landed, tick);
    }
    reg
}

/// An adapter that admits by the engine's exact count, answering PASS `n` times.
fn counted_adapter(n: usize) -> Arc<ScriptedAdapter> {
    let responses = (0..n).map(|_| make_response(FinishReason::Stop, "PASS", None)).collect();
    Arc::new(ScriptedAdapter { counts_exactly: true, ..ScriptedAdapter::new(responses) })
}

// what this catches: the live numbers, at the pricing seam. An estimate of 46,317 the engine counted
// at 38,443 (0.83, its measured density) on a 40,960 window: charged at the raw estimate the act's
// room was its clipped reply grant (80 tokens, the 72-89 cut caps); behind the engine's exact count
// it is the 2,516 the engine's count leaves, past her measured need (reasoning 338 + answer 96).
// Without that count a price never falls below the estimate.
#[test]
fn the_live_cut_caps_become_her_measured_room_behind_the_engines_count() {
    let (window, estimate, counted, clipped_grant) = (40_960u32, 46_317usize, 38_443u32, 80u32);
    let (_, price, _) = Density::default()
        .observe(estimate, counted, "qwen3.6-35b-a3b")
        .expect("a measured reply");
    // The old path sent `.max(reserve)`, 80 tokens past a prompt priced over the window: now that
    // is a typed refusal (re-render), never a cap past the bound.
    let old = request_allowance(TurnKind::Act, clipped_grant, window, price.charge(estimate));
    assert!(old.is_err(), "at the raw estimate the room is below her grant: refused, not 80 tokens past it");
    let counted_price = price.behind(PromptAdmission::EngineCounted);
    let new = request_allowance(TurnKind::Act, clipped_grant, window, counted_price.charge(estimate)).expect("room");
    assert_eq!(new, window - counted - 1, "the engine's count leaves the room");
    // A grant smaller than the slot (Atlas: leased 40,282 on a 148,822 slot): her window is the
    // smaller of the two, so the cap is bounded by the grant, never by the slot.
    let (leased, slot, prompt) = (40_282u32, 148_822u32, 30_000usize);
    let bounded = request_allowance(TurnKind::Act, 434, leased.min(slot), prompt).expect("room");
    assert_eq!(bounded, leased - prompt as u32 - 1);
    assert!(prompt as u32 + bounded < leased, "prompt plus cap strictly inside her grant");
    let need = crate::cognition::working_set::OutputNeed { reasoning: 338, answer: 96, turns: 3, reasoning_at_least: None, answer_at_least: None };
    let claim = reply_claim(TurnKind::Act, Some(need), 0, true, false, None);
    assert_eq!(claim.floor_tokens, 434, "her floor is think plus answer, never the answer alone");
    assert!(new >= claim.floor_tokens);
    assert_eq!(act_reasoning_budget(crate::cognition::working_set::ActRoom::of(Some(need)), 434), ThinkBudget(338), "the answer keeps its measured room");
    assert_eq!(act_reasoning_budget(crate::cognition::working_set::ActRoom::of(Some(need)), 96), ThinkBudget(0), "a grant that holds only her answer: zero, never unbounded");
    assert_eq!(
        reply_claim(TurnKind::Act, None, 0, true, false, None).floor_tokens,
        0,
        "an unmeasured act reserves nothing: a state, never a number"
    );
}

// what this catches: the same receipt end to end. A mind that measured reasoning 338 + answer 96,
// on a binding whose engine counted her prose at 0.83 of its estimate: the act is sent with a cap
// at least her need and at least what the one allocation granted her reply, a think budget that
// leaves her measured answer its room, the strict admission guard at her window, and a receipt
// naming model, build and the density it was priced at.
#[tokio::test]
async fn an_act_carries_her_measured_room_the_guard_and_its_receipt() {
    let home = tempfile::tempdir().expect("isolated fixture emission storage");
    let _native = crate::paths::NativeHomeOverride::install(home.path());
    let (window, persona) = (40_960u32, Uuid::new_v4());
    let reg = measured(persona, 338, 96);
    let adapter = counted_adapter(2);
    let faculty = LlmDeliberationFaculty::new(persona, "Aiko", "You are Aiko.", adapter.clone())
        .with_context_window(window)
        .with_tools(vec![read_tool()])
        .with_working_set(reg.clone());
    let ws = writing_workspace(crate::identity::ActivityRoom::mint(), 40, true);
    // Her first act measures the engine's density (0.83 of the estimate, prose).
    let estimate = faculty.prompt_view(&ws).estimated_prompt_tokens;
    let mut first = make_response(FinishReason::Stop, "PASS", None);
    first.usage.input_tokens = (estimate * 83 / 100) as u32;
    adapter.responses.lock().unwrap().push_front(first);
    let _ = faculty.contribute(&ws).await.expect("a verdict");
    let view = faculty.prompt_view(&ws);
    let mut receipts = Vec::new();
    let contribution = faculty.contribute_with_receipts(&ws, &mut receipts).await.expect("a verdict");
    assert!(contribution.fault.is_none() && contribution.capacity_refusal.is_none(), "{:?}", contribution.fault);
    let seen = adapter.seen.lock().unwrap();
    let request = seen.last().expect("sent");
    let cap = request.max_tokens.expect("bounded");
    assert!(cap >= 434, "her measured think plus answer: {cap}");
    assert!(cap >= view.completion_reserve, "never below the reply's grant");
    let terms = *adapter.terms_seen.lock().unwrap().last().expect("dispatched under terms"); // JUSTIFIED: test fixture lock, never poisoned
    let budget = terms.think.budget().expect("an act's think is bounded by her need").0;
    assert!(budget >= 338 && budget + 96 <= cap, "the answer keeps its room inside the cap: {budget} of {cap}");
    // A large write with a bounded thought: the cap allows the long answer, the think stays at its budget.
    assert!(cap >= budget + 1_000, "room for a write far past her measured answer: think {budget}, cap {cap}");
    let admission = terms.admission.expect("every request carries the admission it was rendered under");
    assert_eq!((admission.window, admission.priced), (window, PromptAdmission::EngineCounted), "priced below the estimate only behind the engine's count");
    let allocation = receipts.last().and_then(|r| r.allocation.as_ref()).expect("the receipt names its allocation");
    assert_eq!(allocation.revision, env!("CONTINUUM_BUILD_GIT_SHA"), "the build that sized it");
    let room = allocation.output_room.as_ref().expect("an act's output room rides its receipt");
    assert_eq!((room.cap, room.need, room.reasoning_budget), (cap, Some(434), Some(budget)));
    assert_eq!(room.model, request.model.clone().unwrap_or_default()); // unwrap_or_default: test fixture, the model the request named
    assert_eq!(room.admission, PromptAdmission::EngineCounted);
    assert!(room.density_measured as u64 * 100 <= room.density_estimated * 84, "priced at her measured density: {room:?}");
}

// what this catches: the traced path when her need does not fit. A measured need (reasoning 30,000
// + answer 2,000) past a 16,384 window is never sent with less: the act is refused as the typed
// IrreducibleShortfall, her need is recorded as her window's floor (required floors + her measured
// need, never a constant reply floor), and that reaches the governor's input (`requirements_for`)
// as her held need. Recording it grants nothing: her window is unchanged and a second attempt on
// the same window is refused again with no request sent. Once the governor grants a window that
// holds it, the act is sent with a cap at least her need.
#[tokio::test]
async fn a_need_past_her_window_is_refused_recorded_and_waits_for_the_governor() {
    let home = tempfile::tempdir().expect("isolated fixture emission storage");
    let _native = crate::paths::NativeHomeOverride::install(home.path());
    let (window, persona) = (16_384u32, Uuid::new_v4());
    let reg = measured(persona, 30_000, 2_000);
    let adapter = counted_adapter(1);
    let faculty = LlmDeliberationFaculty::new(persona, "Aris", "You are Aris.", adapter.clone())
        .with_context_window(window)
        .with_tools(vec![read_tool()])
        .with_working_set(reg.clone());
    let ws = writing_workspace(crate::identity::ActivityRoom::mint(), 4, true);
    for attempt in 0..2 {
        let contribution = faculty.contribute(&ws).await.expect("a typed refusal");
        let refusal = contribution.capacity_refusal.as_ref().expect("refused for capacity, never sent");
        let short = refusal.output_room.clone().expect("typed as her output room, never read from the numbers"); // JUSTIFIED: test fixture
        assert!(short.needed >= 32_000 && short.window == window, "attempt {attempt}: {short:?}");
        assert_eq!(adapter.call_count(), 0, "attempt {attempt}: no request on an insufficient window");
        assert_eq!(faculty.turn_window(), window, "recording a need grants nothing by itself");
    }
    let floor = reg.demand_of(persona).and_then(|d| d.floor_tokens).expect("her floor recorded");
    assert!(floor >= 32_000, "required floors plus her measured think and answer: {floor}");
    let requirement = crate::cognition::working_set::requirements_for(&[persona], &reg).remove(0);
    let need = requirement.measure.observed_need().expect("the governor reads her need").value().get();
    assert!(need >= 32_000, "the governor's input carries her measured need: {need}");
    // The governor's grant (a window that holds her need) is what lets the act go.
    let granted = LlmDeliberationFaculty::new(persona, "Aris", "You are Aris.", adapter.clone())
        .with_context_window(65_536)
        .with_tools(vec![read_tool()])
        .with_working_set(reg.clone());
    let contribution = granted.contribute(&ws).await.expect("a verdict");
    assert!(contribution.capacity_refusal.is_none() && contribution.fault.is_none(), "{:?}", contribution.fault);
    let cap = adapter.seen.lock().unwrap()[0].max_tokens.expect("bounded");
    assert!(cap >= 32_000, "sent with room for her measured need: {cap}");
}

// what this catches: a dense input after prose samples (code or JSON at ~1 token per estimated
// token after a ring measured at 0.83). Priced at the ring's lighter bound, the act's cap overruns
// the window by the engine's exact count; the strict admission guard refuses with that count, the
// cap is recomputed ONCE from it, and her view is sent unchanged.
#[tokio::test]
async fn a_denser_prompt_than_her_samples_is_recapped_once_from_the_engines_count() {
    use crate::ai::inference_error::InferenceError;
    let home = tempfile::tempdir().expect("isolated fixture emission storage");
    let _native = crate::paths::NativeHomeOverride::install(home.path());
    let (window, persona) = (40_960u32, Uuid::new_v4());
    let reg = measured(persona, 338, 96);
    let adapter = counted_adapter(2);
    let faculty = LlmDeliberationFaculty::new(persona, "Benchy", "You are Benchy.", adapter.clone())
        .with_context_window(window)
        .with_tools(vec![read_tool()])
        .with_working_set(reg);
    let ws = writing_workspace(crate::identity::ActivityRoom::mint(), 40, true);
    let estimate = faculty.prompt_view(&ws).estimated_prompt_tokens;
    let mut prose = make_response(FinishReason::Stop, "PASS", None);
    prose.usage.input_tokens = (estimate * 83 / 100) as u32;
    adapter.responses.lock().unwrap().push_front(prose);
    let _ = faculty.contribute(&ws).await.expect("the prose act measured her density");
    // The next prompt is dense: the engine counts it at its whole estimate.
    *adapter.on_request.lock().unwrap() = Some(Box::new(move |request| {
        let exact = (LlmDeliberationFaculty::framing_cost(request.system_prompt.as_deref().unwrap_or_default()) // unwrap_or_default: test fixture, no system prompt costs none
            + PromptCalibration::default().estimate_all(&request.messages)
            + request.tools.as_deref().map_or(0, crate::cognition::tool_surface::tool_surface_tokens_of)) as u32; // map_or: no tools, no schemas
        (exact + request.max_tokens? >= window).then_some(InferenceError::ContextExceeded { requested: exact, available: window })
    }));
    let contribution = faculty.contribute(&ws).await.expect("a verdict");
    assert!(contribution.fault.is_none() && contribution.capacity_refusal.is_none(), "{:?}", contribution.fault);
    let seen = adapter.seen.lock().unwrap();
    assert_eq!(seen.len(), 3, "the prose act, the dense act refused at its count, and one resend");
    let (refused, resent) = (&seen[1], &seen[2]);
    assert_eq!(
        serde_json::to_value(&refused.messages).unwrap(),
        serde_json::to_value(&resent.messages).unwrap(),
        "her view is sent unchanged"
    );
    let exact = (LlmDeliberationFaculty::framing_cost(resent.system_prompt.as_deref().unwrap_or_default()) // unwrap_or_default: test fixture
        + PromptCalibration::default().estimate_all(&resent.messages)
        + resent.tools.as_deref().map_or(0, crate::cognition::tool_surface::tool_surface_tokens_of)) as u32; // map_or: no tools, no schemas
    assert_eq!(resent.max_tokens, Some(window - exact - 1), "recapped from the engine's exact count");
    assert!(resent.max_tokens.unwrap() >= 434, "still past her measured need");
}

// what this catches: a served-model change. Her density ring is keyed by the model the engine
// reported serving: a reply from another model starts afresh, so a lighter price measured on one
// model never prices another.
#[test]
fn a_served_model_change_resets_her_density() {
    let density = Density::default();
    let (prose, _, _) = density.observe(10_000, 8_300, "model-a").expect("measured");
    assert_eq!(prose.samples(), 1);
    let (other, price, _) = prose.observe(10_000, 10_000, "model-b").expect("measured");
    assert_eq!(other.samples(), 1, "a new served model starts a fresh ring");
    assert_eq!(price.behind(PromptAdmission::EngineCounted).charge(10_000), 10_000, "priced at the new model's own count");
}

// what this catches: Atlas on the M5 (0b2854327, 01:25Z): rendered against 148,838, her grant
// shrank to 40,282 while she waited 26.3 s for a lane, and 139,007 counted tokens were sent. The
// engine refuses at dispatch (`serving_guard::dispatch_window`, typed GrantChanged, nothing sent);
// the turn is re-rendered ONCE against the grant she now has, its prompt and cap inside it; moved
// again, she parks on readiness instead of a third render.
#[tokio::test]
async fn a_grant_that_shrinks_before_dispatch_re_renders_her_turn_against_it() {
    use crate::ai::inference_error::InferenceError;
    let home = tempfile::tempdir().expect("isolated fixture emission storage");
    let _native = crate::paths::NativeHomeOverride::install(home.path());
    let (rendered, current, persona) = (65_536u32, 16_384u32, Uuid::new_v4());
    let adapter = counted_adapter(1);
    adapter
        .rejections
        .lock()
        .unwrap() // JUSTIFIED: test fixture lock, never poisoned
        .push_back(InferenceError::GrantChanged { rendered, current: Some(current) });
    let faculty = LlmDeliberationFaculty::new(persona, "Atlas", "You are Atlas.", adapter.clone())
        .with_context_window(rendered)
        .with_tools(vec![read_tool()])
        .with_working_set(measured(persona, 338, 96));
    let ws = writing_workspace(crate::identity::ActivityRoom::mint(), 200, true);
    let contribution = faculty.contribute(&ws).await.expect("a verdict");
    assert!(contribution.fault.is_none() && contribution.capacity_refusal.is_none(), "{:?}", contribution.fault);
    let seen = adapter.seen.lock().unwrap(); // JUSTIFIED: test fixture lock, never poisoned
    assert_eq!(seen.len(), 2, "the refused dispatch and one re-render");
    let fresh = &seen[1];
    let terms = adapter.terms_seen.lock().unwrap().clone(); // JUSTIFIED: test fixture lock, never poisoned
    assert_eq!(terms[0].admission.map(|a| a.window), Some(rendered));
    let admission = terms[1].admission.expect("the re-render carries its admission");
    assert_eq!(admission.window, current, "rendered against the grant she has now");
    let prompt = LlmDeliberationFaculty::framing_cost(fresh.system_prompt.as_deref().unwrap_or_default()) // unwrap_or_default: test fixture
        + PromptCalibration::default().estimate_all(&fresh.messages)
        + fresh.tools.as_deref().map_or(0, crate::cognition::tool_surface::tool_surface_tokens_of); // map_or: no tools, no schemas
    assert!(prompt + fresh.max_tokens.unwrap_or_default() as usize <= current as usize, "inside her current grant: {prompt}"); // unwrap_or_default: test fixture
    drop(seen);
    // Moved again before the next dispatch: refused for capacity and parked, never a third render.
    for grant in [Some(12_000), Some(8_192)] {
        adapter
            .rejections
            .lock()
            .unwrap() // JUSTIFIED: test fixture lock, never poisoned
            .push_back(InferenceError::GrantChanged { rendered: current, current: grant }); // the engine reports what it saw: re-rendered at 12,000, then moved again
    }
    let parked = faculty.contribute(&ws).await.expect("a typed refusal");
    assert!(parked.capacity_refusal.is_some(), "her grant moved twice: she waits for it to settle");
    assert_eq!(adapter.call_count(), 4, "two refused dispatches, no third render");
}

// what this catches: a prompt that consumes her reply's reserve sent anyway with a cap past the
// window (the old `.max(reserve)`). The engine's exact count leaves less than her grant: the act is
// never resent at that size with the reserve stacked past the window; it is re-rendered smaller or
// refused, and every request that does go out fits strictly inside the window.
#[tokio::test]
async fn a_prompt_that_eats_the_reserve_is_re_rendered_or_refused_never_sent_past_the_window() {
    use crate::ai::inference_error::InferenceError;
    let home = tempfile::tempdir().expect("isolated fixture emission storage");
    let _native = crate::paths::NativeHomeOverride::install(home.path());
    let (window, persona) = (40_960u32, Uuid::new_v4());
    let adapter = counted_adapter(2);
    let mut once = true;
    *adapter.on_request.lock().unwrap() = Some(Box::new(move |_request| { // JUSTIFIED: test fixture lock, never poisoned
        // The engine counts the prompt at all but 200 tokens of the window: below her 434 reserve.
        std::mem::take(&mut once).then_some(InferenceError::ContextExceeded { requested: window - 200, available: window })
    }));
    let faculty = LlmDeliberationFaculty::new(persona, "Aris", "You are Aris.", adapter.clone())
        .with_context_window(window)
        .with_tools(vec![read_tool()])
        .with_working_set(measured(persona, 338, 96));
    let ws = writing_workspace(crate::identity::ActivityRoom::mint(), 200, true);
    let _ = faculty.contribute(&ws).await.expect("a verdict or a typed refusal");
    let seen = adapter.seen.lock().unwrap(); // JUSTIFIED: test fixture lock, never poisoned
    let first = &seen[0];
    for resent in seen.iter().skip(1) {
        let prompt = LlmDeliberationFaculty::framing_cost(resent.system_prompt.as_deref().unwrap_or_default()) // unwrap_or_default: test fixture
            + PromptCalibration::default().estimate_all(&resent.messages)
            + resent.tools.as_deref().map_or(0, crate::cognition::tool_surface::tool_surface_tokens_of); // map_or: no tools, no schemas
        let cap = resent.max_tokens.unwrap_or_default(); // unwrap_or_default: test fixture
        assert!(prompt + (cap as usize) < window as usize, "inside the window: {prompt} + {cap}");
        assert!(cap >= 434, "never below her reserve: {cap}");
        assert_ne!(
            serde_json::to_value(&resent.messages).unwrap(), // JUSTIFIED: test fixture, serializable messages
            serde_json::to_value(&first.messages).unwrap(), // JUSTIFIED: test fixture, serializable messages
            "re-rendered smaller, never the same prompt with the reserve stacked past the window"
        );
    }
}
