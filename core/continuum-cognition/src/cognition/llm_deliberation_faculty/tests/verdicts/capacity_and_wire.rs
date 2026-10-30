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
        // saved, as the act path saves every result too large to keep in RAM (`spill`)
        wm.record_receipt_typed(&[], &result, None, crate::ai::types::ResultCustody::Whole { handle: "wire-evidence".into() });
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
        let update = Arc::new(crate::cognition::conversation::IncomingMessage {
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
                FacultyId::Custom(crate::cognition::faculty_ids::ACTIVE_WORK.into()),
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
        // The reply and the prompt negotiate in ONE allocation: the required stimulus and
        // result are never traded, the reply always keeps its floor and claims no more than
        // its measured need (the cold prior here). What the fixture must still prove is that
        // the oversized result rides the wire inside the window (asserted below).
        assert!(
            (LlmDeliberationFaculty::COMPLETION_FLOOR_TOKENS..=LlmDeliberationFaculty::COMPLETION_COLD_PRIOR_TOKENS)
                .contains(&view.completion_reserve),
            "the reply keeps its floor and claims no more than its need: {}",
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
        // WHAT IS GUARANTEED ON THE WIRE (FOCUS-CONTRACT rule 4). The 2026-09-10 mute (one
        // `code/read` required whole, 836 refusals) was first cured by a head cut at a window
        // share (`split_action_result`); that cut is gone. Her newest result is reduced by LEVEL
        // in the one allocation: whole when it fits, else its summary or pointer, which names
        // the handle the whole is read through. Either way it is the last thing she reads.
        let ending = result
            .rsplit_once('\n')
            .map(|(_, last)| last)
            .filter(|last| !last.is_empty())
            .unwrap_or(result.as_str());
        let last = request.messages.last().expect("test: a request has messages").content_text();
        assert!(
            last.ends_with(ending) || last.contains("wire-evidence"),
            "her newest result is the last thing she reads, whole or at a level naming its handle"
        );

        // The held card's context reaches her in the assembled-context message after her
        // view, never the system message, which is hers in every room (2026-10-23).
        assert!(request
            .messages
            .iter()
            .any(|m| m.content_text().contains("Held activity context")));
        assert!(!request
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
            crate::cognition::tool_surface::tool_surface_tokens_of(request.tools.as_ref().unwrap());
        assert_eq!(
            schema_tokens,
            faculty.prompt_view(&ws).tools.tokens
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
    let adapter = Arc::new(ScriptedAdapter::new(vec![make_response(
        FinishReason::Stop,
        "PASS",
        None,
    )]));
    let faculty =
        LlmDeliberationFaculty::new(persona, "Benchy", "You are Benchy.", adapter.clone())
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
            + crate::cognition::tool_surface::tool_surface_tokens_of(request.tools.as_ref().unwrap())
            + reply as usize;
    assert!(wire_tokens <= window as usize, "prompt + reply {wire_tokens} > window {window}");
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
        // Part 0 (the oversized stimulus) must never reach the adapter, so an EMPTY script
        // proves it. Parts 1-4 bound, shed or collapse and go on to think, so they need a
        // real response — reaching the model is the outcome those cases assert.
        let adapter = Arc::new(ScriptedAdapter::new(
            if oversized_part != 0 {
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
            // saved, as the act path saves every result too large to keep whole: with its handle
            // she reads it at a level, never a refusal (FOCUS-CONTRACT rule 4)
            wm.record_receipt_typed(
                &[],
                &format!("{oversized}\nclaimable_now: 29\ntotal_on_board: 43"),
                None,
                crate::ai::types::ResultCustody::Whole { handle: "capacity-evidence".into() },
            );
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
                crate::cognition::conversation::IncomingMessage {
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
        // Parts 3-4 shed their standing grounding (#3954), part 1's oversized saved
        // result arrives at a level naming its handle, and part 2's room update is a turn
        // of her view that collapses to its handle (`chat/history`) — all four now THINK
        // instead of refusing. What still faults is the genuinely irreducible payload:
        // the stimulus (part 0), the question she is answering.
        let bounded_case = oversized_part != 0;
        let grounding_case = bounded_case;
        let view = faculty.prompt_view(&ws);
        if oversized_part == 2 {
            assert!(
                view.messages.iter().any(|m| m.content_text().contains("reread with chat/history")),
                "an oversized room update is never unknown: the index names where it is"
            );
        }
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
            assert!(
                outcome.fault.is_none(),
                "capacity is not a failed generation"
            );
            assert!(outcome.capacity_refusal.is_some());
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
    assert!(fitted
        .messages
        .iter()
        .any(|message| { message.role == "assistant" && message.content_text() == public }));
    assert!(fitted.messages.iter().any(|message| {
        message
            .content_text()
            .contains("cursor declaration and its callers")
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

// what this catches: the one budget's invariant (goal 7, `prompt/budget.rs`). A prompt whose
// charged cost plus reserve is over its window is a named budget fault, never a prompt made
// to comply by cutting it: a real assembled prompt checked against its own window passes,
// and the same prompt forced against a window one token short faults, with every message it
// was handed still there and her view still leading it.
#[test]
fn an_over_budget_prompt_is_a_budget_fault_never_a_clipped_prompt() {
    use crate::cognition::llm_deliberation_faculty::prompt::BudgetLedger;
    use crate::cognition::workspace::Burst;

    let window = 32_768u32;
    let faculty = LlmDeliberationFaculty::new(
        Uuid::new_v4(),
        "Ivar",
        "You are Ivar.",
        Arc::new(ScriptedAdapter::new(vec![])),
    )
    .with_context_window(window)
    .with_tools(vec![read_tool()]);
    let room = crate::identity::ActivityRoom::from_uuid(Uuid::new_v4()).expect("fixture room is non-nil");
    let mut turns: Vec<_> = (0..40)
        .map(|i| BurstTurn::attributed(i % 2 == 0, if i % 2 == 0 { "Ivar" } else { "Peer" }, format!("turn_{i} ").repeat(40), Some(i)))
        .collect();
    turns.push(BurstTurn::attributed(false, "Peer", "What changed?", Some(41)));
    let view = faculty.prompt_view(&Workspace::new(Burst::from_turns(room, turns)));
    assert!(view.budget_fault.is_none(), "{:?}", view.budget_fault);
    assert!(view.messages.first().is_some_and(|m| m.content_text().contains("turn_0 ")), "her view leads the prompt");
    let framing = LlmDeliberationFaculty::framing_cost(&view.system);
    let tools = view.estimated_prompt_tokens - framing - LlmDeliberationFaculty::messages_cost(&view.messages);
    let charged = view.estimated_prompt_tokens + view.completion_reserve as usize;
    let ledger = |context_window: usize| BudgetLedger {
        context_window: context_window as u32,
        calibration: PromptCalibration::default(),
        completion_reserve: view.completion_reserve as usize,
        system: &view.system,
        tool_tokens: tools,
        messages: &view.messages,
        view_len: view.refit.grounding_at,
        grounding_len: view.refit.grounding_len,
        framing_grant: 0,
        required_grant: 0,
        history_grant: 0,
        grounding_grant: 0,
        view_grant: 0,
    };
    assert!(faculty.budget_violation(&ledger(charged)).is_none(), "at its window the prompt complies");
    let fault = faculty.budget_violation(&ledger(charged - 1)).expect("one token over is a budget fault");
    assert!(fault.contains("prompt budget violated"), "{fault}");
    assert!(view.messages.first().is_some_and(|m| m.content_text().contains("turn_0 ")), "nothing was cut to comply");
}

// what this catches (Codex on #5491): a tail her destination can no longer take, either silently
// dropped (she is offered a head missing verbs she was told she had) or turned into a lost turn.
// The typed refusal rebuilds her request ONCE on the activity's own surface, with no tail, and
// sends it; a second refusal after that rebuild is a named fault, never a loop.
#[tokio::test]
async fn a_refused_tail_rebuilds_her_request_without_it_once() {
    use crate::ai::inference_error::InferenceError;
    use crate::cognition::workspace::Burst;

    for refusals in [1usize, 2] {
        let persona = Uuid::new_v4();
        let adapter = Arc::new(ScriptedAdapter::new(vec![make_response(FinishReason::Stop, "PASS", None)]));
        let mut left = refusals;
        *adapter.on_request.lock().expect("fixture request hook") = Some(Box::new(move |_request| {
            (left > 0).then(|| {
                left -= 1;
                InferenceError::TailToolsUnreachable { model: "scripted".into() }
            })
        }));
        let faculty = LlmDeliberationFaculty::new(persona, "Ivar", "You are Ivar.", adapter.clone())
            .with_context_window(32_768)
            .with_tools(vec![read_tool()]);
        let ws = Workspace::new(Burst::from_turns(
            crate::identity::ActivityRoom::mint(),
            vec![BurstTurn::attributed(false, "Peer", "Read src/lib.rs and tell me what it exports.", Some(1))],
        ));
        let contribution = faculty.contribute(&ws).await.expect("a verdict or named fault");
        let seen = adapter.seen.lock().expect("fixture requests");
        assert_eq!(seen.len(), 2, "one refusal, one rebuilt send: never a third ({refusals} refusals)");
        let rebuilt = &seen[1];
        assert!(rebuilt.tail_tools.is_none(), "the rebuilt request carries no tail");
        let offered: Vec<&str> = rebuilt.tools.iter().flatten().map(|tool| tool.name.as_str()).collect();
        let activity = faculty.tool_surface().select_granted(&ws.room_affordances, faculty.prompt_view(&ws).tools_grant).expect("activity fits granted tools");
        let expected: Vec<&str> = activity.specs.iter().flat_map(|specs| specs.iter()).map(|tool| tool.name.as_str()).collect();
        assert!(!offered.is_empty() && offered == expected, "she is offered exactly her activity's surface: {offered:?}");
        if refusals == 1 {
            assert!(contribution.fault.is_none(), "the rebuilt request answered: {:?}", contribution.fault);
        } else {
            assert!(contribution.fault.is_some(), "a refusal after the rebuild is a named fault");
        }
    }
}

// what this catches: a prompt built on a broken allocator contract sent anyway (Codex on #5564).
// An allocator answering for the wrong number of view sources is a GrantMismatch; it reaches the
// turn's budget_fault and contribute refuses it before the adapter is ever called.
#[tokio::test(flavor = "current_thread")]
async fn an_allocator_answering_for_the_wrong_sources_refuses_the_turn_before_dispatch() {
    use crate::cognition::workspace::Burst;
    let adapter = Arc::new(ScriptedAdapter::new(vec![make_response(FinishReason::Stop, "PASS", None)]));
    let faculty = LlmDeliberationFaculty::new(Uuid::new_v4(), "Ivar", "You are Ivar.", adapter.clone())
        .with_context_window(32_768)
        .with_tools(vec![read_tool()]);
    let room = crate::identity::ActivityRoom::mint();
    let turns: Vec<_> = (0..20)
        .map(|i| BurstTurn::attributed(i % 2 == 0, if i % 2 == 0 { "Ivar" } else { "Peer" }, format!("line {i} ").repeat(20), Some(i)))
        .collect();
    let ws = Workspace::new(Burst::from_turns(room, turns));
    crate::cognition::llm_deliberation_faculty::prompt::SHORT_GRANTS.with(|short| short.set(true));
    let outcome = faculty.contribute(&ws).await;
    crate::cognition::llm_deliberation_faculty::prompt::SHORT_GRANTS.with(|short| short.set(false));
    let fault = outcome.expect("a contribution").fault.expect("the turn is refused");
    assert!(format!("{fault:?}").contains("could not be sized"), "{fault:?}");
    assert_eq!(adapter.call_count(), 0, "nothing was sent to inference");
}
