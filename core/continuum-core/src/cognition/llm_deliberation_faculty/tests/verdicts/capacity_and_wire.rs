#![cfg(test)]

use super::*;

// what this catches: f09424d4, Kimi's actual ask vanished while the
// newest one-line result became only board counts / a clipping notice.
// Inspect the REQUEST the adapter received, including the paid schemas
// and planned reply reserve, not an intermediate text helper.
#[tokio::test]
async fn prompt_preserves_stimulus_and_large_action_on_the_wire() {
    // Exercise every byte remainder at the grounding boundary: a floor
    // per name/body used to charge less than their concatenated bytes.
    for padding in 0..3 {
        use crate::cognition::working_memory::WorkingMemory;
        use crate::cognition::workspace::Burst;

        let window = 32_768u32;
        let persona = Uuid::new_v4();
        let adapter = Arc::new(ScriptedAdapter::new(vec![make_response(
            FinishReason::Stop,
            "PASS",
            None,
        )]));
        let wm = Arc::new(WorkingMemory::new(8));
        wm.set_served_window(window);
        let result = format!(
            "src/lib.rs:42: concrete_review_target\n{{\"payload\":\"{}\"}}\nclaimable_now: 29\ntotal_on_board: 43",
            "x".repeat(window as usize * 2),
        );
        wm.record_receipt(&result);
        let faculty =
            LlmDeliberationFaculty::new(persona, "Ivar", "You are Ivar.", adapter.clone())
                .with_context_window(window)
                .with_tools(vec![read_tool()])
                .with_working_memory(wm);
        let room = crate::identity::ActivityRoom::from_uuid(Uuid::new_v4()).unwrap();
        // The STIMULUS carries the squeeze now. It used to be the oversized action
        // result that forced `completion_reserve` to yield, and a result can no
        // longer do that: it is capped at its share of the window with the
        // remainder demoted to history (an unbounded required result was muting
        // citizens outright — 836 refusals, zero acts, 2026-09-10). The stimulus
        // IS still irreducible and unbounded — she cannot answer a question she was
        // not given — so the reserve-yield property this test covers is exercised
        // through the one required item that legitimately has no ceiling.
        let stimulus: &str = Box::leak(
            format!(
                "Review PR #3858 against its actual source. {}",
                "supporting detail the reviewer sent inline ".repeat(1_200)
            )
            .into_boxed_str(),
        );
        let update = Arc::new(crate::persona::service_loop::IncomingMessage {
            work: None,
            media: Vec::new(),
            event_id: Uuid::new_v4(),
            lamport: 1,
            peer_id: Uuid::new_v4(),
            room_id: Uuid::new_v4(),
            text: format!(
                "A colleague's additional evidence: {}",
                "retained evidence ".repeat(80)
            ),
        });
        let mut ws = Workspace::new(Burst::from_turns(
            room,
            vec![
                BurstTurn::attributed(
                    true,
                    "Ivar",
                    "I completed the previous independent activity.",
                    Some(0),
                ),
                BurstTurn::attributed(false, "Reviewer", stimulus, Some(1)),
                BurstTurn::attributed(true, "Ivar", "I fetched the board.", Some(2)),
                BurstTurn::perception("The board projection refreshed."),
            ],
        ));
        ws.workspace_deliverable = true;
        ws.room_updates = Arc::new(vec![Arc::clone(&update)]);
        ws.broadcast.push(
            Contribution::context(
                FacultyId::Custom(crate::persona::active_work_source::SOURCE_ID.into()),
                format!(
                    "Held activity context: continue independent source review.{}",
                    "x".repeat(padding)
                ),
                0.9,
                "held claims",
            )
            .standing_grounding(),
        );

        let unfitted = faculty.messages_unfitted(&ws, None);
        assert_eq!(
            unfitted
                .history
                .iter()
                .filter(|message| message.role == "assistant")
                .count(),
            2,
            "extracting the stimulus must not coalesce the self turns on opposite sides"
        );
        // The fixture's RESULT is big — that is what this test exercises. It used
        // to assert `required_tokens()` itself exceeded the prefill target, which
        // is now impossible BY DESIGN: an action result is capped at its share of
        // the window and the remainder is demoted to history (an unbounded required
        // result was muting citizens outright — 836 refusals, zero acts, 2026-09-10).
        // The fixture property is asserted where it actually lives now.
        assert!(
            LlmDeliberationFaculty::messages_cost(&[ChatMessage::text(
                "user",
                result.clone()
            )]) > crate::inference::prefill_rate::UNMEASURED_FILL_FLOOR_TOKENS,
            "fixture must present a genuinely oversized action result — measured \
                     against the substrate's own unmeasured-box fill floor, since the \
                     `30 × 500` product this used to compare with was the defect"
        );
        let view = faculty.prompt_view(&ws);
        assert!(view.capacity_error.is_none(), "{view:?}");
        // The reply and the prompt negotiate in ONE allocation now: the required
        // stimulus and result are never traded, the reply always keeps its floor,
        // and above it the reply grows by priority against optional history —
        // under the act's runaway bound. What the fixture must still prove is that
        // the oversized result rides the wire inside the window (asserted below).
        assert!(
            (LlmDeliberationFaculty::COMPLETION_FLOOR_TOKENS
                ..=LlmDeliberationFaculty::ACT_OUTPUT_CAP)
                .contains(&view.completion_reserve),
            "the reply keeps its floor and stays under the act's bound: {}",
            view.completion_reserve
        );
        let contribution = faculty
            .contribute(&ws)
            .await
            .expect("a verdict or named fault");
        assert!(contribution.fault.is_none(), "{:?}", contribution.fault);
        assert_eq!(adapter.call_count(), 1);
        let seen = adapter.seen.lock().unwrap();
        let request = &seen[0];
        assert!(
            request
                .messages
                .iter()
                .any(|message| message.content_text().ends_with(&update.text)),
            "room updates must survive alongside the original task and complete result"
        );
        assert!(request
            .messages
            .iter()
            .any(|message| message.content_text().contains(stimulus)));
        // WHAT IS GUARANTEED ON THE WIRE CHANGED HERE, deliberately.
        //
        // This used to assert the ENTIRE action result arrived byte-for-byte,
        // because the result was REQUIRED input and required input is never
        // dropped. That is precisely what muted citizens: one `code/read` of a
        // large file claimed the whole window as irreducible payload and
        // `fit_messages` refused every turn — measured 2026-09-10, required 18,629
        // against 15,688 available, 836 consecutive refusals, zero acts for an
        // entire benchmark round while she held the card.
        //
        // Now the result is capped at its share and the earlier part is DEMOTED to
        // ordinary history: rendered whenever the window allows, evicted by the
        // normal front-drop when it does not, and always re-readable through its
        // spill handle. So the honest guarantee is:
        //
        //   - the result's ENDING is always on the wire (it stays required), and
        //   - nothing is DISCARDED at the split — `split_action_result` is pinned
        //     byte-exact by `an_oversized_result_is_split_not_swallowed_whole`.
        //
        // A 16k-token single line on a 32k window cannot be both demoted and
        // retained; asserting otherwise would just be asserting the mute back.
        let wire_text: String = request
            .messages
            .iter()
            .map(|message| message.content_text().to_string())
            .collect();
        let ending = result
            .rsplit_once('\n')
            .map(|(_, last)| last)
            .filter(|last| !last.is_empty())
            .unwrap_or(result.as_str());
        assert!(
            request
                .messages
                .last()
                .unwrap()
                .content_text()
                .ends_with(ending),
            "the result's ENDING is the last thing she reads — she must always see                      what her own act produced, never only a footer"
        );
        assert!(
            wire_text.contains(ending),
            "and it is genuinely on the wire, not merely implied"
        );

        assert!(request
            .system_prompt
            .as_ref()
            .unwrap()
            .contains("Held activity context"));
        assert_eq!(
            request.max_tokens,
            Some(view.completion_reserve),
            "the wire carries the plan's grant verbatim"
        );
        let schema_tokens =
            LlmDeliberationFaculty::tool_surface_tokens_of(request.tools.as_ref().unwrap());
        assert_eq!(
            schema_tokens,
            faculty.select_tool_surface(&ws, window).tokens
        );
        let wire_tokens =
            LlmDeliberationFaculty::framing_cost(request.system_prompt.as_ref().unwrap())
                + LlmDeliberationFaculty::messages_cost(&request.messages)
                + schema_tokens
                + request.max_tokens.unwrap() as usize;
        assert!(
            wire_tokens <= window as usize,
            "wire {wire_tokens} > window {window}"
        );
    }
}

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
    wm.set_served_window(window);
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
    assert!(
        (10_808..=LlmDeliberationFaculty::ACT_OUTPUT_CAP).contains(&reply),
        "the retried write gets at least the doubled cut, under the act's bound: {reply}"
    );
    let wire_tokens =
        LlmDeliberationFaculty::framing_cost(request.system_prompt.as_ref().unwrap())
            + LlmDeliberationFaculty::messages_cost(&request.messages)
            + LlmDeliberationFaculty::tool_surface_tokens_of(request.tools.as_ref().unwrap())
            + reply as usize;
    assert!(wire_tokens <= window as usize, "prompt + reply {wire_tokens} > window {window}");
    assert!(
        request.messages.iter().any(|m| m.content_text().contains("Write the migration file now")),
        "the ask rides the wire"
    );
}

// what this catches: the reply taking from REQUIRED input. A mind whose measured
// turn is far larger than a small seat (15,000 tokens of think + answer on an 8,192
// window) must still see her whole stimulus and her act's result: the reply's floor
// is capped by what required input leaves, and above it the reply only trades
// optional history. No capacity fault, the reply keeps a usable floor, and the
// request fits the window.
#[tokio::test]
async fn an_8192_window_keeps_required_content_under_a_large_measured_need() {
    use crate::cognition::working_memory::WorkingMemory;
    use crate::cognition::working_set::{EmissionStop, WorkingSetRegistry};
    let home = tempfile::tempdir().expect("isolated fixture emission storage");
    let _native = crate::paths::NativeHomeOverride::install(home.path());
    let window = 8_192u32;
    let persona = Uuid::new_v4();
    let reg = WorkingSetRegistry::new();
    for tick in 1..=3 {
        reg.record_emission_in_memory(persona, 12_000, 9_000, EmissionStop::Landed, tick);
    }
    let need = reg.need_of(persona).expect("measured");
    assert!(need.total() > window, "the fixture's need exceeds the whole seat");
    let wm = Arc::new(WorkingMemory::new(8));
    wm.set_served_window(window);
    let result = format!("src/schema.rs:7: accounts_table_defined\n{}", "column detail ".repeat(40));
    wm.record_receipt(&result);
    let adapter = Arc::new(ScriptedAdapter::new(vec![make_response(FinishReason::Stop, "PASS", None)]));
    let faculty = LlmDeliberationFaculty::new(persona, "Benchy", "You are Benchy.", adapter.clone())
        .with_context_window(window)
        .with_tools(vec![read_tool()])
        .with_working_set(reg)
        .with_working_memory(wm);
    let ws = writing_workspace(crate::identity::ActivityRoom::mint(), 40, true);

    let view = faculty.prompt_view(&ws);
    assert!(view.capacity_error.is_none(), "{:?}", view.capacity_error);
    let contribution = faculty.contribute(&ws).await.expect("a verdict or named fault");
    assert!(contribution.fault.is_none(), "{:?}", contribution.fault);
    let seen = adapter.seen.lock().unwrap();
    let request = &seen[0];
    let wire_text: String = request.messages.iter().map(|m| m.content_text().to_string()).collect();
    assert!(wire_text.contains("Write the migration file now"), "the stimulus rides the wire");
    assert!(wire_text.contains("accounts_table_defined"), "the act's result rides the wire");
    let reply = request.max_tokens.expect("the reply is bounded");
    assert!(reply >= LlmDeliberationFaculty::COMPLETION_FLOOR_TOKENS, "a usable reply: {reply}");
    let wire_tokens =
        LlmDeliberationFaculty::framing_cost(request.system_prompt.as_ref().unwrap())
            + LlmDeliberationFaculty::messages_cost(&request.messages)
            + LlmDeliberationFaculty::tool_surface_tokens_of(request.tools.as_ref().unwrap())
            + reply as usize;
    assert!(wire_tokens <= window as usize, "prompt + reply {wire_tokens} > window {window}");
}

// what this catches: priority not deciding. One workspace, one measured want (a
// pending cut doubled), two turns: an ambient pass (priority 4) and an act after the
// cut (priority 10). The act's reply grows at history's expense; the required input
// is identical in both (never part of the trade); and the plan is deterministic —
// two runs of the act are byte-identical.
#[test]
fn reply_priority_trades_history_not_required_input() {
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
    let history = |view: &DeliberationPromptView| {
        view.messages.iter().filter(|m| m.content_text().contains("distinct_history_")).count()
    };
    let pass_view = faculty.prompt_view_within(&pass, window);
    let act_view = faculty.prompt_view_within(&act, window);
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
    assert!(
        history(&act_view) < history(&pass_view),
        "history pays for the act's reply: {} vs {} messages",
        history(&act_view),
        history(&pass_view)
    );
    let again = faculty.prompt_view_within(&act, window);
    assert_eq!(
        format!("{:?} {:?} {}", again.system, again.messages, again.completion_reserve),
        format!("{:?} {:?} {}", act_view.system, act_view.messages, act_view.completion_reserve),
        "one plan for one input, byte for byte"
    );
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
        wm.set_served_window(window);
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
                "",
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
                + faculty.select_tool_surface(&ws, window).tokens
                + minimum.system_reserve()
                + minimum.trailing_tokens()
                + unfitted.required_tokens()
                // The reply's floor as the plan sizes it — what it takes before history grows.
                + reply_claim(
                    faculty.turn_kind(&ws),
                    faculty.working_set.as_ref().and_then(|reg| reg.need_of(persona)),
                    0,
                    LlmDeliberationFaculty::holds_live_work(&ws),
                    ws.directed_at_self(),
                )
                .0
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
        assert_eq!(system.contains("[room-roster]"), !trailing);
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
        let tool_tokens = LlmDeliberationFaculty::tool_surface_tokens_of(tools);
        assert_eq!(tool_tokens, faculty.select_tool_surface(&ws, window).tokens);
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

// what this catches: an indivisible oversized stimulus, room update or active
// result must fault before inference, including self-ticks with no chat turns.
// It must still publish demand, so refusal cannot freeze a small window.
//
// An oversized STANDING GROUNDING unit is the deliberate exception, and cases 3-4
// pin it: enrichment sheds, the turn proceeds. This test asserted the opposite
// until 2026-09-09, which is how the mute survived — a green test defending the
// defect. Live that day: Kimi held `hard-rs/expr_eval` on a 28,160-token lane and
// took a `work_turn` every 15 s that produced `act=0 model_ms=0`, because 613
// tokens of standing grounding did not fit and the grounding error outranked every
// other check. She kept the card while refusing, so the round could never end.
// Payload requiredness is unchanged; only the enrichment's shortfall stopped being
// fatal.
#[tokio::test]
async fn prompt_capacity_fault_keeps_demand_and_never_calls_the_adapter() {
    use crate::cognition::working_memory::WorkingMemory;
    use crate::cognition::working_set::WorkingSetRegistry;

    let window = 8192u32;
    for oversized_part in 0..5 {
        let persona = Uuid::new_v4();
        // Parts 0-2 must never reach the adapter, so an EMPTY script proves it.
        // Parts 3-4 shed their enrichment and go on to think, so they need a real
        // response — reaching the model is the outcome those cases assert.
        let adapter = Arc::new(ScriptedAdapter::new(
            if oversized_part >= 3 || oversized_part == 1 {
                vec![make_response(FinishReason::Stop, "PASS", None)]
            } else {
                vec![]
            },
        ));
        let registry = WorkingSetRegistry::new();
        let wm = Arc::new(WorkingMemory::new(8));
        wm.set_served_window(window);
        let oversized = "payload ".repeat(window as usize);
        let mut ws = if oversized_part == 0 {
            Workspace::new(&oversized)
        } else if oversized_part == 1 {
            wm.record_receipt(&format!(
                "{oversized}\nclaimable_now: 29\ntotal_on_board: 43"
            ));
            let mut ws = Workspace::new("");
            ws.turns.clear();
            ws.self_initiated = true;
            ws
        } else if oversized_part >= 3 {
            let mut ws = Workspace::new("original task stays required");
            let grounding = Contribution::context(
                FacultyId::Custom("standing-activity".into()),
                oversized,
                0.9,
                "activity",
            )
            .standing_grounding();
            ws.broadcast.push(if oversized_part == 4 {
                grounding.trailing()
            } else {
                grounding.session_stable()
            });
            ws
        } else {
            wm.record_receipt("complete active result");
            let mut ws = Workspace::new("original task stays required");
            ws.room_updates = Arc::new(vec![Arc::new(
                crate::persona::service_loop::IncomingMessage {
                    work: None,
                    media: Vec::new(),
                    event_id: Uuid::new_v4(),
                    lamport: 1,
                    peer_id: Uuid::new_v4(),
                    room_id: Uuid::new_v4(),
                    text: oversized,
                },
            )]);
            ws
        };
        ws.now_ms = Some(1);
        let faculty =
            LlmDeliberationFaculty::new(persona, "Ivar", "You are Ivar.", adapter.clone())
                .with_context_window(window)
                .with_working_memory(wm)
                .with_working_set(registry.clone());
        // Parts 3-4 shed their standing grounding (#3954) and part 1 caps its
        // oversized action result and demotes the rest — all three now THINK
        // instead of refusing. What still faults is a genuinely irreducible
        // payload: the stimulus (part 0) and a room update (part 2), neither of
        // which any bound may drop without answering a question she was never
        // shown.
        let bounded_case = oversized_part >= 3 || oversized_part == 1;
        let grounding_case = bounded_case;
        let view = faculty.prompt_view(&ws);
        if grounding_case {
            assert!(
                view.capacity_error.is_none(),
                "an unaffordable STANDING SOURCE sheds; it must not refuse the                          turn (part {oversized_part}): {:?}",
                view.capacity_error
            );
        } else {
            let error = view
                .capacity_error
                .expect("indivisible input exceeds the real window");
            assert!(error.required_tokens > error.budget_tokens);
        }
        let outcome = faculty
            .contribute(&ws)
            .await
            .expect("capacity is a named substrate fault");
        if grounding_case {
            assert!(
                outcome.fault.is_none(),
                "shedding enrichment is not a fault (part {oversized_part}): {:?}",
                outcome.fault
            );
            assert_eq!(
                adapter.call_count(),
                1,
                "she reached the model — that IS the fix (part {oversized_part})"
            );
        } else {
            assert!(outcome.fault.is_some());
            assert!(
                outcome.decision.is_none(),
                "capacity refusal is not a citizen choosing Pass"
            );
            assert_eq!(
                adapter.call_count(),
                0,
                "do not submit a footer or clipping notice"
            );
        }
        // Demand is published either way — a refusal must never freeze a small
        // window, and neither must a shed.
        assert!(registry.demand_of(persona).unwrap().peak_tokens > window);
    }
}

// what this catches: a native tool-use response (FinishReason::ToolUse with
// calls) becomes a `Decision::Act` carrying those exact calls — NOT executed
// here, NOT re-generated. Single shot: exactly one model call. Regression here
// means the faculty either swallowed the action or silently ran it (the old
// in-faculty agent loop we removed). The model's separated reasoning rides
// along as the act's `intent` (so the resulting engram records WHY she acted).
#[tokio::test]
async fn tool_use_response_becomes_an_act_verdict() {
    let persona = Uuid::new_v4();
    let room = Uuid::new_v4();
    let public = "I will remove the cursor, preserve truncation, then verify the callers.";
    let memory = Arc::new(crate::cognition::working_memory::WorkingMemory::new(16));
    memory.set_scope(Some("task-a".into()));
    let call = ToolCall {
        id: "t1".to_string(),
        name: "code/read".to_string(),
        input: json!({ "path": "deploy.md" }),
    };
    let mut resp = make_response(FinishReason::ToolUse, public, Some(vec![call.clone()]));
    resp.reasoning = Some("I should check deploy.md to answer.".to_string());
    let adapter = Arc::new(ScriptedAdapter::new(vec![resp]));

    // Real window: at the ctor default the framing alone starves msg_budget
    // to 0 — the exact state the delib.prompt.starved guard now refuses
    // (these two tests silently exercised the blank-prompt bug path for
    // months; the scripted adapter masked it).
    let faculty =
        LlmDeliberationFaculty::new(persona, "Ivar", "You are Ivar.", adapter.clone())
            .with_tools(vec![read_tool()])
            .with_working_memory(memory.clone())
            .with_context_window(32_768);

    let mut ws = Workspace::new("teammate asks: did the deploy fix land?");
    ws.activity_room_id = room;
    let c = faculty.contribute(&ws).await.expect("verdict");

    match c.decision {
        Some(Decision::Act { calls, intent }) => {
            assert_eq!(calls.len(), 1, "the one requested call rides the verdict");
            assert_eq!(calls[0].name, "code/read");
            assert_eq!(calls[0].input, json!({ "path": "deploy.md" }));
            assert_eq!(intent, "I should check deploy.md to answer.");
        }
        other => panic!("expected Act, got {other:?}"),
    }
    // SINGLE SHOT — the faculty never re-generated (no in-faculty loop).
    assert_eq!(adapter.call_count(), 1, "one generation, one verdict");
    memory.record_receipt("code/read returned the cursor declaration and its callers");
    // No optional WM bid: seven competing faculties evicted it in the live
    // receipt. Authored action continuity must survive that same boundary.
    assert!(ws.broadcast.is_empty());
    let prompt = faculty.messages_unfitted(&ws, None);
    let required = prompt.required_tokens();
    let fitted = faculty
        .fit_messages(prompt, required)
        .expect("required continuity fits");
    assert!(fitted.messages.iter().any(|message| {
        message.role == "assistant" && message.content_text() == public
    }));
    assert!(fitted.messages.iter().any(|message| {
        message.content_text().contains("cursor declaration and its callers")
    }));
    ws.activity_room_id = Uuid::new_v4();
    assert!(faculty.messages_unfitted(&ws, None).action_statement.is_none(), "another room cannot inherit it");
    ws.activity_room_id = room;
    memory.set_scope(Some("task-b".into()));
    assert!(faculty.messages_unfitted(&ws, None).action_statement.is_none(), "another task cannot inherit it");
    memory.set_scope(Some("task-a".into()));
    memory.record_settlement("finished", Some(room));
    assert!(faculty.messages_unfitted(&ws, None).action_statement.is_none(), "settlement ends this decision");
}
