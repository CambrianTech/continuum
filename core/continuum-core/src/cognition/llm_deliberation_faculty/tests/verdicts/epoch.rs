//! Her epoch through the real `contribute()` path: what consecutive requests carry, when the
//! epoch turns over, and what the engine is told about its stable prefix (`epoch_view`).
#![cfg(test)]

use super::*;

/// A faculty at `window` whose model answers every act, reporting that it reused the
/// whole prompt it was sent (as a warm slot does); receipts are saved through `save`.
fn epoch_faculty(window: u32, acts: usize) -> (LlmDeliberationFaculty, Arc<ScriptedAdapter>, impl Fn(usize, &str)) {
    let (faculty, adapter, wm) = epoch_faculty_with_memory(window, acts, u32::MAX);
    let save = move |i: usize, text: &str| {
        let custody = crate::ai::types::ResultCustody::Whole { handle: format!("{i:016x}") };
        wm.record_receipt_typed(&[], text, None, custody);
    };
    (faculty, adapter, save)
}

/// `reused`: the prefix the scripted engine reports it reused on every reply.
fn epoch_faculty_with_memory(window: u32, acts: usize, reused: u32) -> (LlmDeliberationFaculty, Arc<ScriptedAdapter>, Arc<crate::cognition::working_memory::WorkingMemory>) {
    let answered = || TextGenerationResponse {
        usage: UsageMetrics { input_tokens: 1, ..Default::default() },
        timing: Some(crate::ai::types::GenerationTiming { cached_tokens: reused, ..Default::default() }),
        ..make_response(FinishReason::Stop, "PASS", None)
    };
    let adapter = Arc::new(ScriptedAdapter::new((0..acts).map(|_| answered()).collect()));
    let wm = Arc::new(crate::cognition::working_memory::WorkingMemory::new(64));
    wm.set_served_window(window);
    let faculty = LlmDeliberationFaculty::new(Uuid::new_v4(), "Ivar", "You are Ivar.", adapter.clone())
        .with_model_binding(model_binding(adapter.clone(), Some("epoch-model".into()), window))
        .with_working_memory(Arc::clone(&wm));
    (faculty, adapter, wm)
}

/// The body of the request act `n` sent (everything before the facts and the ask),
/// bounded by the view rendered for it just before it was sent.
fn sent_body_of(view: &DeliberationPromptView, adapter: &ScriptedAdapter, n: usize) -> Vec<(String, String)> {
    let seen = adapter.seen.lock().expect("recorded request lock");
    seen[n].messages[..view.stable_prefix_messages()]
        .iter()
        .map(|m| (m.role.clone(), m.content_text()))
        .collect()
}

// what this catches: an act whose request rewrites the last act's body. On the M5
// (2026-10-19, `delib.prompt.reuse_split`) the results ring sat mid-prompt and grew by
// one result per act, so everything behind it re-prefilled: 17-29k tokens per act.
#[tokio::test]
async fn consecutive_acts_send_the_last_acts_body_as_their_prefix() {
    let (faculty, adapter, save) = epoch_faculty(32_768, 2);
    save(0, "RESULT-1");
    save(1, "RESULT-2");
    let recall = |text: &str| Contribution::context(FacultyId::Recall, text, 0.6, "recall").trailing();
    let mut ws = Workspace::new("continue the card");
    ws.broadcast.push(recall("RECALL v1"));
    let mut bodies = Vec::new();
    for act in 0..2 {
        let view = faculty.prompt_view(&ws);
        assert!(faculty.contribute(&ws).await.is_some_and(|v| v.fault.is_none()), "act {act} answered");
        bodies.push(sent_body_of(&view, &adapter, act));
        if act == 0 {
            save(2, "RESULT-3");
            ws.broadcast = vec![recall("RECALL v2")];
        }
    }
    let (first, second) = (&bodies[0], &bodies[1]);
    assert!(!first.is_empty(), "the first act has a body");
    assert_eq!(second[..first.len()], first[..], "the second request starts with the first's body");
    let news: Vec<&str> = second[first.len()..].iter().map(|(_, t)| t.as_str()).collect();
    assert!(news.iter().any(|t| t.contains("RESULT-2")), "the settled result appended: {news:?}");
    assert!(news.iter().any(|t| t.contains("RECALL v2")), "the changed grounding appended: {news:?}");
}

// what this catches: a system head re-rendered because the body grew (Codex on #5210):
// the head's grounding took whatever room the conversation left, so every act that
// grew the body moved the first bytes of the request and nothing behind them was reused.
#[tokio::test]
async fn the_whole_request_holds_its_prefix_while_the_body_grows() {
    let window = 16_384;
    let (faculty, adapter, wm) = epoch_faculty_with_memory(window, 2, u32::MAX);
    let save = |i: usize, text: String| {
        let custody = crate::ai::types::ResultCustody::Whole { handle: format!("{i:016x}") };
        wm.record_receipt_typed(&[], &text, None, custody);
    };
    // Two system-side sources larger than the room left over, so their admitted size
    // is decided by that room: one standing, one ordinary.
    let list = |name: &str, n: usize| {
        let parts: Vec<String> = (0..n).map(|i| format!("{name} item {i:04} with a line of ordinary length")).collect();
        Contribution::context(FacultyId::Custom(name.into()), parts.join("\n"), 0.9, "list").with_parts(parts)
    };
    let mut ws = Workspace::new("continue the card");
    ws.broadcast.push(list("room-kanban", 600).standing_grounding());
    ws.broadcast.push(list("workspace-map", 600));
    save(0, "RESULT-0".into());
    // The pinned result is a cold start's evidence of growth (it joins the body next act).
    save(1, format!("RESULT-1 {}", "x".repeat(3_000)));
    // The body grows between the acts by a changed recall: within that growth, so the
    // epoch holds, and enough that the room left over for the head changes.
    let recall = |v: &str| Contribution::context(FacultyId::Recall, format!("RECALL {v} {}", "y".repeat(600)), 0.6, "recall").trailing();
    let mut views = Vec::new();
    for (act, version) in ["v1", "v2"].into_iter().enumerate() {
        ws.broadcast.retain(|c| c.faculty != FacultyId::Recall);
        ws.broadcast.push(recall(version));
        views.push(faculty.prompt_view(&ws));
        assert!(faculty.contribute(&ws).await.is_some_and(|v| v.fault.is_none()), "act {act} answered");
    }
    let seen = adapter.seen.lock().expect("recorded request lock");
    let (first, second) = (&seen[0], &seen[1]);
    assert!(first.system_prompt.as_deref().is_some_and(|s| s.contains("room-kanban item 0000")));
    let (h1, h2) = (first.system_prompt.clone().unwrap_or_default(), second.system_prompt.clone().unwrap_or_default());
    let at = h1.bytes().zip(h2.bytes()).position(|(a, b)| a != b).unwrap_or(h1.len().min(h2.len()));
    let near = |h: &str| h.get(at.saturating_sub(200)..(at + 200).min(h.len())).unwrap_or("").to_string();
    assert_eq!(h2, h1, "the head is the epoch's, not the leftover's; first difference at {at}:\n{}\n---\n{}", near(&h1), near(&h2));
    assert_eq!(
        second.tools.as_ref().map(|t| t.iter().map(|s| s.name.clone()).collect::<Vec<_>>()),
        first.tools.as_ref().map(|t| t.iter().map(|s| s.name.clone()).collect::<Vec<_>>()),
    );
    let body = views[0].refit.grounding_at + views[0].refit.grounding_len;
    let said = |r: &TextGenerationRequest| r.messages[..body].iter().map(|m| (m.role.clone(), m.content_text())).collect::<Vec<_>>();
    assert_eq!(said(second), said(first), "after the head, the second request starts with the first's body");
    // Not vacuous: a head rendered fresh for the second act's state takes less room.
    let fresh = LlmDeliberationFaculty::new(Uuid::new_v4(), "Ivar", "You are Ivar.", adapter.clone())
        .with_model_binding(model_binding(adapter.clone(), Some("epoch-model".into()), window))
        .with_working_memory(Arc::clone(&wm));
    assert_ne!(
        Some(fresh.prompt_view(&ws).system),
        second.system_prompt,
        "rendered fresh for this state the head differs (it takes the room left over): only the binding held it"
    );
}

// what this catches: the live loop on the M5 (#5210 at 9f97bc775, 2026-10-20): the whole
// prompt was bounded by prefill time credited with the engine's lagging reuse, so low
// reuse made a small budget, the epoch turned over at 3-15k tokens of a 185,856-token
// window (16 turnovers in 25 acts, all budget) and reuse never came. Her epoch knows
// its own cached share: small tails never turn it over before the window is reached.
#[tokio::test]
async fn small_tails_never_turn_the_epoch_over_before_the_window_is_reached() {
    const ACTS: usize = 64;
    // The engine reports no reuse at all, the worst the lagging report can say.
    let (faculty, adapter, wm) = epoch_faculty_with_memory(185_856, ACTS, 0);
    let ws = Workspace::new("continue the card");
    let (mut previous, mut turned) = (None::<Vec<(String, String)>>, Vec::new());
    for act in 0..ACTS {
        let custody = crate::ai::types::ResultCustody::Whole { handle: format!("{act:016x}") };
        wm.record_receipt_typed(&[], &format!("RESULT-{act} {}", "x".repeat(1_500)), None, custody);
        let view = faculty.prompt_view(&ws);
        assert!(faculty.contribute(&ws).await.is_some_and(|v| v.fault.is_none()), "act {act} answered");
        let body = sent_body_of(&view, &adapter, act);
        if previous.is_some_and(|p| !body.starts_with(&p)) {
            turned.push(act);
        }
        previous = Some(body);
    }
    assert!(turned.is_empty(), "small tails far from a full window turned the epoch over at acts {turned:?}");
}

// what this catches: an epoch turned over because one act's tail ran past the prefill-time
// bound. A turnover re-prefills the whole new epoch, so it never makes an act cheaper than
// its tail: on the M5 it took reuse from 0.92 to 0.35 (c30e660d9, 2026-10-20), and priced
// short it refused Benchy's required input on 125 acts in an hour (c3e9616e6). A long tail,
// required result and optional recall both, extends her epoch and reaches the model.
#[tokio::test]
async fn a_long_tail_extends_her_epoch_and_never_turns_it_over() {
    let (faculty, adapter, wm) = epoch_faculty_with_memory(185_856, 3, u32::MAX);
    let save = |i: usize, text: &str| {
        let custody = crate::ai::types::ResultCustody::Whole { handle: format!("{i:016x}") };
        wm.record_receipt_typed(&[], text, None, custody);
    };
    save(0, "RESULT-0");
    wm.pin_fact("hands", "your hands are rooted at the card's worktree");
    let mut ws = Workspace::new("[work turn] continue the card").self_initiated(true);
    let mut bodies = Vec::new();
    for act in 0..3 {
        if act == 1 {
            save(1, &format!("RESULT-1 {}", "r".repeat(GUARD_CHARS_PER_TOKEN * 20_000)));
            ws.broadcast = vec![Contribution::context(FacultyId::Recall, format!("RECALL {}", "y".repeat(GUARD_CHARS_PER_TOKEN * 40_000)), 0.6, "recall").trailing()];
        }
        let view = faculty.prompt_view(&ws);
        let verdict = faculty.contribute(&ws).await;
        assert!(verdict.as_ref().is_some_and(|v| v.fault.is_none()), "act {act} reached the model: {verdict:?}");
        bodies.push(sent_body_of(&view, &adapter, act));
    }
    assert!(bodies[1].starts_with(&bodies[0]), "the long tail extended her epoch, never turned it over");
    assert!(bodies[2].starts_with(&bodies[1]), "and the next act extends it again");
    let seen = adapter.seen.lock().expect("recorded request lock");
    assert!(seen[1].messages.iter().any(|m| m.content_text().contains("RESULT-1 rrrr")), "her required result was sent");
}

// what this catches: an epoch STARTED at the window. A start is all fresh, so a restarted
// core put every mind's whole history (~110k tokens) into one prefill and no act finished in
// 42 minutes (the M5 at 52f61fc29, 2026-10-20). It starts at what the lane prefills in her
// time to first token, then grows warm: the next act extends it.
#[tokio::test]
async fn an_epoch_starts_at_what_the_lane_prefills_in_time_then_grows_warm() {
    let window = 185_856;
    let (faculty, adapter, wm) = epoch_faculty_with_memory(window, 2, u32::MAX);
    let start = crate::cognition::context_budget::ContextBudget::fresh_prefill_tokens(
        crate::inference::prefill_rate::measured_rate_for("epoch-model"),
        window,
        crate::cognition::audience::current(),
    );
    let results = 4 * start / 1_500 + 8;
    for i in 0..results {
        let custody = crate::ai::types::ResultCustody::Whole { handle: format!("{i:016x}") };
        wm.record_receipt_typed(&[], &format!("RESULT-{i} {}", "x".repeat(GUARD_CHARS_PER_TOKEN * 1_500)), None, custody);
    }
    let ws = Workspace::new("continue the card");
    let mut bodies = Vec::new();
    for act in 0..2 {
        let view = faculty.prompt_view(&ws);
        assert!(faculty.contribute(&ws).await.is_some_and(|v| v.fault.is_none()), "act {act} answered");
        bodies.push(sent_body_of(&view, &adapter, act));
    }
    let seen = adapter.seen.lock().expect("recorded request lock");
    let first = LlmDeliberationFaculty::messages_cost(&seen[0].messages);
    assert!(first <= start + start / 4, "the epoch started near what the lane prefills in time ({start}), not the window: {first} tokens");
    assert!(seen[0].messages.iter().any(|m| m.content_text().contains(&format!("RESULT-{} ", results - 1))), "and kept her newest result");
    assert!(bodies[1].starts_with(&bodies[0]), "the next act extends the started epoch");
}

// what this catches: a new epoch fitted to the window instead of the start bound (Codex on
// #5210 at 852c16551): a live epoch's window turnover re-prefilled a whole window (34,068
// tokens against a 16,384 start bound, mutation-checked). Every new epoch is all fresh, so
// every turnover fits to the start bound, which also covers a cold prompt past the window
// (this fixture's working memory caps a cold prompt below the window, so that case is held by
// the one budget for every turnover, not by the cold assertion below).
#[tokio::test]
async fn every_new_epoch_starts_at_the_start_bound_cold_or_after_a_window_turnover() {
    const WINDOW: u32 = 65_536;
    let start = crate::cognition::context_budget::ContextBudget::fresh_prefill_tokens(
        crate::inference::prefill_rate::measured_rate_for("epoch-model"),
        WINDOW,
        crate::cognition::audience::current(),
    );
    assert!(start < WINDOW as usize / 2, "the fixture needs a start bound well under the window: {start}");
    let near_start = |tokens: usize| tokens <= start + start / 4;
    // Cold, with a large history.
    let (faculty, adapter, wm) = epoch_faculty_with_memory(WINDOW, 1, u32::MAX);
    for i in 0..(WINDOW as usize / 1_500 + 16) {
        let custody = crate::ai::types::ResultCustody::Whole { handle: format!("{i:016x}") };
        wm.record_receipt_typed(&[], &format!("RESULT-{i} {}", "x".repeat(GUARD_CHARS_PER_TOKEN * 1_500)), None, custody);
    }
    assert!(faculty.contribute(&Workspace::new("continue the card")).await.is_some_and(|v| v.fault.is_none()));
    let cold = LlmDeliberationFaculty::messages_cost(&adapter.seen.lock().expect("recorded request lock")[0].messages);
    assert!(near_start(cold), "a cold history started near the start bound {start}: {cold} tokens");
    // Warm, growing until the window turns it over.
    const ACTS: usize = 120;
    let (faculty, adapter, save) = epoch_faculty(WINDOW, ACTS);
    let ws = Workspace::new("continue the card");
    let mut previous: Option<Vec<(String, String)>> = None;
    let mut turnovers = Vec::new();
    for act in 0..ACTS {
        save(act, &format!("RESULT-{act} {}", "x".repeat(GUARD_CHARS_PER_TOKEN * 1_000)));
        let view = faculty.prompt_view(&ws);
        assert!(faculty.contribute(&ws).await.is_some_and(|v| v.fault.is_none()), "act {act} answered");
        let body = sent_body_of(&view, &adapter, act);
        if previous.as_ref().is_some_and(|p| !body.starts_with(p)) {
            turnovers.push((act, LlmDeliberationFaculty::messages_cost(&adapter.seen.lock().expect("recorded request lock")[act].messages)));
        }
        previous = Some(body);
    }
    assert!(!turnovers.is_empty(), "{ACTS} results of ~1k tokens outgrow a {WINDOW}-token window");
    for (act, tokens) in &turnovers {
        assert!(near_start(*tokens), "the window turnover at act {act} started near the start bound {start}: {tokens} tokens");
    }
}

// what this catches: standing pieces re-sent after the body every act (the M5 at
// c9bc93951: ~11k tokens of own-time, work-turn, steps, open views and notices,
// always fresh). Unchanged, they live in her epoch and the tail carries none of them.
#[tokio::test]
async fn unchanged_standing_pieces_ride_her_epoch_never_the_tail() {
    let (faculty, adapter, wm) = epoch_faculty_with_memory(32_768, 2, u32::MAX);
    for i in 0..3 {
        let custody = crate::ai::types::ResultCustody::Whole { handle: format!("{i:016x}") };
        wm.record_receipt_typed(&[], &format!("RESULT-{i}"), None, custody);
    }
    wm.pin_fact("hands", "your hands are rooted at the card's worktree");
    let ws = Workspace::new("[work turn] continue the card").self_initiated(true);
    let mut sent = Vec::new();
    for act in 0..2 {
        let view = faculty.prompt_view(&ws);
        assert!(faculty.contribute(&ws).await.is_some_and(|v| v.fault.is_none()), "act {act} answered");
        let seen = adapter.seen.lock().expect("recorded request lock");
        let texts: Vec<String> = seen[act].messages.iter().map(|m| m.content_text()).collect();
        sent.push((view.refit.grounding_at + view.refit.grounding_len, texts));
    }
    let standing = ["[steps taken this session]", crate::cognition::framing_echo::NOTICES_HEADER, "[Your own time]", "[work turn]"];
    let holds = |texts: &[String], needle: &str| texts.iter().any(|t| t.contains(needle));
    let (body1, first) = &sent[0];
    let (body2, second) = &sent[1];
    for needle in standing {
        assert!(holds(&first[..*body1], needle), "act 1's epoch holds {needle}");
        assert!(!holds(&second[*body1..], needle), "act 2 re-sent {needle} although it did not change: {:#?}", &second[*body1..]);
    }
    assert_eq!(second[..*body1], first[..*body1], "act 2 starts with act 1's body");
    assert!(*body2 >= *body1);
}

// what this catches: a request that names no stable prefix, or names it somewhere other
// than the first message after her epoch body, so a local engine's checkpoint lands
// inside the per-act tail or before the body's end (CambrianTech/llama.cpp #68).
#[tokio::test]
async fn the_request_names_the_tails_first_message_as_its_checkpoint_anchor() {
    let (faculty, adapter, wm) = epoch_faculty_with_memory(32_768, 1, u32::MAX);
    for i in 0..3 {
        let custody = crate::ai::types::ResultCustody::Whole { handle: format!("{i:016x}") };
        wm.record_receipt_typed(&[], &format!("RESULT-{i}"), None, custody);
    }
    let ws = Workspace::new("[work turn] continue the card").self_initiated(true);
    let view = faculty.prompt_view(&ws);
    assert!(faculty.contribute(&ws).await.is_some_and(|v| v.fault.is_none()));
    let seen = adapter.seen.lock().expect("recorded request lock");
    let stable = seen[0].stable_prefix_messages.expect("a deliberation request names its stable prefix");
    assert_eq!(stable, view.stable_prefix_messages());
    let said = |ms: &[ChatMessage]| ms.iter().map(|m| (m.role.clone(), m.content_text())).collect::<Vec<_>>();
    assert_eq!(said(&seen[0].messages[..stable]), said(&view.messages[..stable]), "the body is what precedes it");
    let first_tail = seen[0].messages[stable].content_text();
    assert!(!first_tail.contains("[work turn]") && !first_tail.contains("[steps taken"), "the tail opens after the standing pieces: {first_tail}");
    let anchor = crate::inference::request_body::checkpoint_user_message(&seen[0].messages, stable)
        .expect("the tail opens with a user turn");
    let wire = crate::inference::request_body::wire_messages(&seen[0].messages, seen[0].system_prompt.as_deref(), false, false, "llama-server");
    let users: Vec<_> = wire.iter().filter(|m| m["role"] == "user").collect();
    assert_eq!(users[anchor]["content"].as_str(), Some(first_tail.as_str()), "the engine's anchor-th user turn is the tail's first message");
}

// what this catches: an epoch that outgrows the window instead of turning over, or a
// turnover that leaves no room to grow (every act a cold prefill again). In a window
// this small, one act's measured growth of room means a turnover every other act at
// most; the live windows are ~10x larger.
#[tokio::test]
async fn the_window_turns_the_epoch_over_and_the_next_act_extends_it() {
    const ACTS: usize = 80;
    let (faculty, adapter, save) = epoch_faculty(16_384, ACTS);
    let ws = Workspace::new("continue the card");
    let (mut kept, mut turned) = (0usize, Vec::new());
    let mut previous: Option<Vec<(String, String)>> = None;
    for act in 0..ACTS {
        save(act, &format!("RESULT-{act} {}", "x".repeat(900)));
        let view = faculty.prompt_view(&ws);
        assert!(view.capacity_error.is_none(), "act {act} fits its window");
        assert!(faculty.contribute(&ws).await.is_some_and(|v| v.fault.is_none()), "act {act} answered");
        let body = sent_body_of(&view, &adapter, act);
        match &previous {
            Some(p) if body.starts_with(p) => kept += 1,
            Some(_) => turned.push(act),
            None => {}
        }
        previous = Some(body);
    }
    assert!(!turned.is_empty(), "{ACTS} results of ~300 tokens outgrow a 16k window: the epoch turned over");
    assert!(kept > turned.len(), "after a turnover the next acts extend the new epoch; turnovers at {turned:?}");
}
