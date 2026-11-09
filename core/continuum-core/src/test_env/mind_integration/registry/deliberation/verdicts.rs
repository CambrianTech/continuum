#![cfg(test)]
use super::*;

// what this catches: Benchy's first write cut mid-call (2026-10-18 02:10Z). Three
// read-only turns landed at 2,702; the reply's reserve was twice that peak, 5,404;
// the write cut mid-answer at 5,404 — and the retry was sized by the same carve-off.
// Now the cut's doubled answer is in her need and the pending cut raises the act to
// priority 10: on her 46,848 window the retry's REQUEST carries at least the doubled
// cut and never past the act's runaway bound, and prompt + reply fit the window.
#[tokio::test]
async fn a_writing_turn_after_a_cut_gets_its_reply_on_a_46848_window() {
    use crate::cognition::working_memory::WorkingMemory;
    use crate::cognition::working_set::{EmissionStop, WorkingSetRegistry};
    let home = tempfile::tempdir().expect("isolated fixture emission storage");
    let _native = crate::paths::NativeHomeOverride::install(home.path());
    let window = 46_848u32;
    let persona = Uuid::new_v4();
    let reg = WorkingSetRegistry::new();
    for tick in 1..=3 {
        reg.record_emission_in_memory(persona, 2_702, 0, EmissionStop::Landed, tick);
    }
    let cut = reg.record_emission_in_memory(persona, 5_404, 0, EmissionStop::CutMidAnswer, 4);
    assert_eq!(cut.peak_tokens, 10_808, "a cut mid-answer records the answer at double");
    let wm = Arc::new(WorkingMemory::new(8));
    let adapter = Arc::new(ScriptedAdapter::new(vec![make_response(FinishReason::Stop, "PASS", None)]));
    let faculty = LlmDeliberationFaculty::new(persona, "Benchy", "You are Benchy.", adapter.clone())
        .with_context_window(window)
        .with_tools(persona_tools::native_tool_specs())
        .with_working_set(reg)
        .with_working_memory(Arc::clone(&wm));
    let ws = writing_workspace(crate::identity::ActivityRoom::mint(), 120, true);
    assert_eq!(faculty.turn_kind(&ws), TurnKind::Act);
    wm.retain_truncated_emission(ws.activity_room_id, cut.peak_tokens);

    let contribution = faculty.contribute(&ws).await.expect("a verdict or named fault");
    assert!(contribution.fault.is_none(), "{:?}", contribution.fault);
    let seen = adapter.seen.lock().unwrap();
    let request = &seen[0];
    let reply = request.max_tokens.expect("the reply is bounded");
    assert!(reply >= 10_808, "the retried write gets at least the doubled cut: {reply}");
    let wire_tokens =
        LlmDeliberationFaculty::framing_cost(request.system_prompt.as_ref().unwrap())
            + LlmDeliberationFaculty::messages_cost(&request.messages)
            + crate::cognition::tool_surface::tool_surface_tokens_of(request.tools.as_ref().unwrap())
            + reply as usize;
    assert!(wire_tokens <= window as usize, "prompt + reply {wire_tokens} > window {window}");
    assert!(
        request.messages.iter().any(|m| m.content_text().contains("Write the migration file now")),
        "the ask rides the wire"
    );
}

// what this catches: priority not deciding. One workspace, one measured want (a
// pending cut doubled), two turns: an ambient pass (priority 4) and an act after the
// cut (priority 10). The act's reply grows from the growth reserve and her history is
// the same on both; the required input is identical in both; and the plan is deterministic —
// two runs of the act are byte-identical.
#[test]
fn reply_priority_grows_the_reply_not_at_history_or_required_input() {
    use crate::cognition::working_memory::WorkingMemory;
    let window = 16_384u32;
    let persona = Uuid::new_v4();
    let wm = Arc::new(WorkingMemory::new(8));
    let faculty = LlmDeliberationFaculty::new(
        persona,
        "Benchy",
        "You are Benchy.",
        Arc::new(ScriptedAdapter::new(vec![])),
    )
    .with_context_window(window)
    .with_tools(vec![read_tool()])
    .with_working_memory(Arc::clone(&wm));
    let room = crate::identity::ActivityRoom::mint();
    let pass = writing_workspace(room, 200, false);
    let act = writing_workspace(room, 200, true);
    assert_eq!((faculty.turn_kind(&pass), faculty.turn_kind(&act)), (TurnKind::Pass, TurnKind::Act));
    wm.retain_truncated_emission(act.activity_room_id, 5_404);

    let required = |ws: &Workspace| faculty.messages_unfitted(ws, None).required_tokens();
    assert_eq!(required(&pass), required(&act), "the same required input on both turns");
    // Turns kept, not messages: the view may fold several turns into one message.
    let history = |view: &DeliberationPromptView| {
        let text: String = view.messages.iter().map(|m| m.content_text()).collect();
        (0..200).filter(|i| text.contains(&format!("distinct_history_{i} "))).count()
    };
    // Each turn on its own faculty: her view carries an epoch from one prompt to the next,
    // so the act on the pass's faculty would inherit the pass's epoch. What this test
    // pins is the allocation, which each turn makes from its own state.
    let fresh = || {
        LlmDeliberationFaculty::new(persona, "Benchy", "You are Benchy.", Arc::new(ScriptedAdapter::new(vec![])))
            .with_context_window(window)
            .with_tools(vec![read_tool()])
            .with_working_memory(Arc::clone(&wm))
    };
    let pass_view = fresh().prompt_view_within(&pass, window);
    // Working memory keeps her view for her checkpoint; this act starts its own.
    wm.take_view();
    let act_faculty = fresh();
    let act_view = act_faculty.prompt_view_within(&act, window);
    for view in [&pass_view, &act_view] {
        assert!(view.capacity_error.is_none(), "{:?}", view.capacity_error);
        assert!(
            view.messages.iter().any(|m| m.content_text().contains("Write the migration file now")),
            "required input is never traded"
        );
    }
    assert!(
        act_view.completion_reserve > pass_view.completion_reserve,
        "the act's reply outranks the ambient pass's: {} vs {}",
        act_view.completion_reserve,
        pass_view.completion_reserve
    );
    // An epoch start sizes history against the reply's runaway bound, the same on both
    // turns (`fit_view`, epoch.rs), so one turn's priority never reshuffles her prefix:
    // the act's larger reply comes out of the growth reserve, not out of her history.
    assert_eq!(history(&act_view), history(&pass_view), "the act's reply leaves her history as it was");
    let again = act_faculty.prompt_view_within(&act, window);
    assert_eq!(
        format!("{:?} {:?} {}", again.system, again.messages, again.completion_reserve),
        format!("{:?} {:?} {}", act_view.system, act_view.messages, act_view.completion_reserve),
        "one plan for one input, byte for byte"
    );
}

// what this catches: repeated rejection is bounded, the next tick uses
// observed density, and unrelated small prompts do not pay a fixed excess.
#[tokio::test]
async fn provider_context_feedback_bounds_replay_and_scales_next_turn() {
    use crate::ai::inference_error::InferenceError;
    use crate::cognition::working_set::WorkingSetRegistry;
    use crate::cognition::workspace::Burst;

    let home = tempfile::tempdir().expect("isolated demand storage");
    let _native = crate::paths::NativeHomeOverride::install(home.path());
    let persona = Uuid::new_v4();
    let window = 29_440;
    let adapter = Arc::new(ScriptedAdapter::new(vec![
        make_response(FinishReason::Stop, "PASS", None),
        make_response(FinishReason::Stop, "PASS", None),
    ]));
    adapter
        .rejections
        .lock()
        .expect("fixture rejection queue")
        .extend([
            InferenceError::ContextExceeded {
                requested: 29_722,
                available: window,
            },
            InferenceError::ContextExceeded {
                requested: 29_722,
                available: window,
            },
        ]);
    let registry = WorkingSetRegistry::new();
    for tick in 0..3 {
        registry.record_emission_in_memory(persona, 834, 0, crate::cognition::working_set::EmissionStop::Landed, tick);
    }
    let faculty =
        LlmDeliberationFaculty::new(persona, "Ivar", "You are Ivar.", adapter.clone())
            .with_context_window(window)
            .with_tools(vec![read_tool()])
            .with_working_set(registry);
    let mut turns: Vec<_> = (0..160)
        .map(|i| {
            BurstTurn::attributed(
                i % 2 == 0,
                if i % 2 == 0 { "Ivar" } else { "Peer" },
                format!("distinct_history_{i} ").repeat(100),
                Some(i),
            )
        })
        .collect();
    turns.push(BurstTurn::attributed(
        false,
        "Peer",
        "Review the current artifact.",
        Some(161),
    ));
    let ws = Workspace::new(Burst::from_turns(
        crate::identity::ActivityRoom::mint(),
        turns,
    ));
    assert_eq!(
        faculty
            .messages_unfitted(&ws, None)
            .history
            .iter()
            .filter(|m| m.content_text().contains("distinct_history_"))
            .count(),
        160
    );
    let fault = faculty.contribute(&ws).await.expect("bounded refusal");
    // Bounded: a fault or a typed capacity refusal (the newest-result refit's end), never a
    // third request (asserted by the call count below).
    assert!(fault.fault.is_some() || fault.capacity_refusal.is_some(), "{:?}", fault.fault);
    assert_eq!(
        adapter.call_count(),
        2,
        "unchanged exact measurement terminates corrective replay"
    );
    let next = faculty.contribute(&ws).await.expect("next tick");
    assert!(next.fault.is_none(), "{:?}", next.fault);
    {
        let seen = adapter.seen.lock().expect("actual requests");
        assert_eq!(seen.len(), 3);
        assert!(
            LlmDeliberationFaculty::messages_cost(&seen[2].messages)
                < LlmDeliberationFaculty::messages_cost(&seen[1].messages),
            "the previously rejected prompt must not be sent unchanged next tick"
        );
    }
    let small = Workspace::in_room("What changed?", ws.room_id);
    let (effective, calibration) = faculty.prompt_fit(&faculty.binding.load());
    let small_view = faculty.prompt_view(&small);
    assert!(
        small_view.capacity_error.is_none(),
        "{:?}",
        small_view.capacity_error
    );
    assert!(
        calibration.charge(small_view.estimated_prompt_tokens)
            + small_view.completion_reserve as usize
            <= effective as usize
    );
    let outcome = faculty
        .contribute(&small)
        .await
        .expect("small unrelated turn");
    assert!(outcome.fault.is_none(), "{:?}", outcome.fault);
    assert_eq!(adapter.call_count(), 4);

    // Re-home to a distinct provider, even at the same declared capacity.
    let replacement = Arc::new(ScriptedAdapter::new(vec![make_response(
        FinishReason::Stop,
        "PASS",
        None,
    )]));
    faculty.binding.store(Arc::new(ModelBinding {
        adapter: replacement.clone(),
        model: None,
        context_window: window,
    }));
    let pristine = faculty.prompt_view_within(&ws, window);
    assert_eq!(
        faculty.prompt_view(&ws).estimated_prompt_tokens,
        pristine.estimated_prompt_tokens
    );
    assert!(faculty
        .contribute(&ws)
        .await
        .expect("new route")
        .fault
        .is_none());
    assert_eq!(replacement.call_count(), 1);
}

// what this catches: b61ef8d2, saturated history bought a roster prefix
// using the room wall's reservation and silently dropped that wall. Inspect
// actual adapter requests, both KV placements, and source-order ties.
#[tokio::test]
async fn standing_activity_grounding_survives_history_on_the_wire() {
    use crate::cognition::working_memory::WorkingMemory;
    use crate::cognition::working_set::WorkingSetRegistry;
    use crate::cognition::workspace::Burst;

    let home = tempfile::tempdir().expect("isolated fixture demand and emission storage");
    let _native = crate::paths::NativeHomeOverride::install(home.path());
    for (window, trailing, reverse, unit_size, history_count) in [
        (29_440, false, false, 500, 200),
        (29_440, true, true, 500, 200),
        (58_880, true, false, 500, 0),
        (29_440, false, true, 6_000, 200),
        (29_440, true, false, 6_000, 200),
    ] {
        let persona = Uuid::new_v4();
        let adapter = Arc::new(ScriptedAdapter::new(vec![make_response(
            FinishReason::Stop,
            "PASS",
            None,
        )]));
        let wm = Arc::new(WorkingMemory::new(8));
        wm.record_receipt("latest complete tool receipt");
        // Optional history does not outrank the reply reserve. Give this
        // established citizen measured emissions so the large source
        // minimum leaves real history room, rather than assuming the
        // cold half-window prior must yield to optional conversation.
        let registry = WorkingSetRegistry::new();
        for now in 1..=3 {
            registry.record_emission_in_memory(persona, 1_000, 0, crate::cognition::working_set::EmissionStop::Landed, now);
        }
        let faculty =
            LlmDeliberationFaculty::new(persona, "Ivar", "You are Ivar.", adapter.clone())
                .with_context_window(window)
                .with_tools(persona_tools::native_tool_specs())
                .with_working_memory(wm)
                .with_working_set(registry);
        let mut turns: Vec<_> = (0..history_count)
            .map(|i| {
                BurstTurn::attributed(
                    i % 2 == 1,
                    if i % 2 == 1 { "Ivar" } else { "Peer" },
                    format!("distinct_history_{i} ").repeat(100),
                    Some(i),
                )
            })
            .collect();
        turns.push(BurstTurn::attributed(
            false,
            "Peer",
            "Review the team's current activity.",
            Some(201),
        ));
        let room = crate::identity::ActivityRoom::from_uuid(Uuid::new_v4())
            .expect("fixture room is non-nil");
        let mut ws = Workspace::new(Burst::from_turns(room, turns));
        ws.workspace_deliverable = true;
        let parts: Vec<_> = (0..7)
            .map(|i| format!("peer_{i} role and activity: {}", "scope_".repeat(unit_size)))
            .collect();
        let roster_text = parts.join("\n");
        let roster = Contribution::context(
            FacultyId::Custom("room-roster".into()),
            roster_text.clone(),
            0.9,
            "colleagues",
        )
        .with_parts(parts.clone())
        .standing_grounding()
        .session_stable();
        let wall_text = "Current activity: review the substrate with your colleagues; source evidence is on this wall.";
        let wall = Contribution::context(
            FacultyId::Custom("room-wall".into()),
            wall_text,
            0.9,
            "activity",
        )
        .standing_grounding();
        // The requiredness contract is independent of cache placement.
        let (roster, wall) = if trailing {
            (roster.trailing(), wall.trailing())
        } else {
            (roster, wall.session_stable())
        };
        let full_roster_tokens = GroundingPiece::whole(&roster).tokens;
        let whole_roster_exceeds_window = full_roster_tokens > window as usize;
        let case = format!(
            "window={window}, trailing={trailing}, reverse={reverse}, units={unit_size}, history={history_count}, full_roster_tokens={full_roster_tokens}"
        );
        assert_eq!(
            whole_roster_exceeds_window,
            unit_size == 6_000,
            "fixture must distinguish a potentially fitting list from an oversized list: {case}"
        );
        ws.broadcast = if reverse {
            vec![wall, roster]
        } else {
            vec![roster, wall]
        };
        if history_count > 0 {
            let framing = faculty.compose_system_split(
                &faculty.expanded_categories(&ws),
                ws.directed_at_self(),
                ws.self_initiated,
                ws.now_ms,
                LlmDeliberationFaculty::holds_live_work(&ws),
            );
            let unfitted = faculty.messages_unfitted(&ws, Some(&framing.trailing));
            let history: Vec<_> = unfitted
                .history
                .iter()
                .filter(|m| m.content_text().contains("distinct_history_"))
                .collect();
            assert_eq!(
                history.len(),
                history_count as usize,
                "fixture history must survive near-duplicate filtering and coalescing"
            );
            // The standing minimum, unfitted — this fixture asserts the WHOLE
            // minimum fits its window, so it wants the plan itself, not a
            // budget-shed one.
            let minimum = GroundingPlan::minimum(&ws);
            let fixed = LlmDeliberationFaculty::framing_cost(&framing.stable)
                + faculty.prompt_view(&ws).tools.tokens
                + minimum.context_reserve()
                + minimum.trailing_tokens()
                + unfitted.required_tokens()
                // The reply's floor as the plan sizes it — what it takes before history grows.
                + reply_claim(
                    faculty.turn_kind(&ws),
                    faculty.working_set.as_ref().and_then(|reg| reg.need_of(persona)),
                    0,
                    LlmDeliberationFaculty::holds_live_work(&ws),
                    ws.directed_at_self(),
                    None, // the floor (min_tokens) does not read the time term
                )
                .min_tokens as usize;
            let cluster = history
                .iter()
                .rev()
                .take(2)
                .map(|m| LlmDeliberationFaculty::message_cost(m))
                .sum::<usize>();
            assert!(fixed + cluster < window as usize,
                "fixture must admit a whole historical pair after its minimum and reply reserve: fixed={fixed}, pair={cluster}, window={window}, units={unit_size}");
            assert!(
                unfitted.tokens() > window as usize,
                "fixture must actually saturate history"
            );
        }
        let verdict = faculty
            .contribute(&ws)
            .await
            .expect("verdict or capacity fault");
        assert!(verdict.fault.is_none(), "{case}: {:?}", verdict.fault);
        assert_eq!(adapter.call_count(), 1);
        let seen = adapter.seen.lock().expect("recording fixture lock");
        let request = &seen[0];
        let system = request
            .system_prompt
            .as_deref()
            .expect("assembled system prompt");
        let contains = |text: &str| {
            system.contains(text)
                || request
                    .messages
                    .iter()
                    .any(|m| m.content_text().contains(text))
        };
        assert!(
            contains(wall_text),
            "the complete atomic wall must survive its equal-salience roster: {case}"
        );
        if history_count == 0 {
            assert!(
                contains(&roster_text),
                "a roomy model keeps all offered units: {case}"
            );
        } else {
            assert!(
                contains(&parts[0]),
                "keep the source's complete first unit: {case}"
            );
            // History is already bounded by the existing prefill cap.
            // A small list may therefore fit whole alongside retained
            // history; saturation alone does not require its omission.
            if whole_roster_exceeds_window {
                assert!(
                    !contains(&parts[6]),
                    "a roster larger than the entire window must retain only a prefix: {case}"
                );
            }
            if !contains(&roster_text) {
                assert!(
                    contains("more not shown (context budget)"),
                    "a partial list must disclose omitted source units: {case}"
                );
            }
            assert!(
                request
                    .messages
                    .iter()
                    .any(|m| m.content_text().contains("distinct_history_")),
                "optional history retains a share after reserving the minimum: {case}"
            );
        }
        // A room's grounding never enters the system message, whichever placement its
        // source declared (2026-10-23): the system message is hers in every room. A roster
        // its source did not mark as a view block rides the assembled-context message
        // after her view.
        assert!(!system.contains("[room-roster]"), "the roster is the room's, never the system message's: {case}");
        if !trailing {
            assert!(
                request.messages.iter().any(|m| {
                    let t = m.content_text();
                    t.starts_with("[What you are working with right now]") && t.contains("[room-roster]")
                }),
                "an unmarked standing roster renders in the assembled-context message: {case}"
            );
        }
        if trailing {
            let position = request
                .messages
                .iter()
                .position(|m| m.content_text().starts_with("[room-roster]"))
                .expect("reserved trailing roster");
            assert!(
                request.messages[position + 1..]
                    .iter()
                    .any(|m| m.content_text().contains("latest complete tool receipt")),
                "standing grounding retains its pre-result-ring placement"
            );
            assert_eq!(
                request
                    .messages
                    .iter()
                    .filter(|m| m.content_text().starts_with("[room-roster]"))
                    .count(),
                1
            );
        }
        assert!(contains("Review the team's current activity."));
        assert!(request
            .messages
            .last()
            .expect("required result")
            .content_text()
            .ends_with("latest complete tool receipt"));
        assert!(
            !contains("distinct_history_0 "),
            "optional history must yield"
        );
        let tools = request.tools.as_ref().expect("actual native surface");
        let tool_tokens = crate::cognition::tool_surface::tool_surface_tokens_of(tools);
        assert_eq!(tool_tokens, faculty.prompt_view(&ws).tools.tokens);
        let wire = LlmDeliberationFaculty::framing_cost(system)
            + LlmDeliberationFaculty::messages_cost(&request.messages)
            + tool_tokens
            + request.max_tokens.expect("planned completion reserve") as usize;
        assert!(
            wire <= window as usize,
            "actual request {wire} exceeds its window: {case}"
        );
    }
}
