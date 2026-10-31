#![cfg(test)]

use super::*;

// Regression: Kimi's seventh perception bid evicted working memory and its
// recorded no-deliverable facts. Fit the actual projection after that eviction.
#[tokio::test]
async fn substrate_notices_survive_attention_and_history_pressure_once() {
    use crate::cognition::working_memory::WorkingMemory;
    use crate::cognition::workspace::{Arbiter, FocusContext, SalienceArbiter, Situation};
    let wm = Arc::new(WorkingMemory::new(8));
    wm.record("weighing the next step");
    wm.record_fact("[no-deliverable] No workspace change yet.");
    wm.pin_fact("hands", "[hands] The workspace remains bound.");
    // Arbitrary ledger author text uses the source-owned expansion route.
    use crate::experience::ledger::{CardLedger, render_for_turn, render_reference_for_turn};
    let ledger = CardLedger {
        card_id: Uuid::new_v4(), by: Uuid::new_v4(),
        decided_fix: Some("large-ledger-detail ".repeat(4_000)),
        ..Default::default()
    };
    let reference = render_reference_for_turn(&ledger, ledger.by);
    wm.pin_expandable_fact("ledger", &reference, Some(render_for_turn(&ledger, ledger.by)));

    let faculty = LlmDeliberationFaculty::new(
        Uuid::new_v4(), "T", "You are T.", Arc::new(HeuristicInferenceAdapter::new()),
    ).with_working_memory(Arc::clone(&wm));
    let mut ws = Workspace::new("continue the existing task");
    let bids: Vec<_> = (0..6).map(|i| Contribution::context(
        FacultyId::Custom(format!("source-{i}")), "standing context", 0.9, "fixture",
    )).collect();
    ws.broadcast = SalienceArbiter.focus(bids, &FocusContext {
        world_state: &ws.world_state, situation: Situation::PostAction,
    });
    // Her notices ride her view as a required source (`context_streams::notices`): the
    // allocation grants their floor before anything optional, and the final fit never cuts
    // her view. So through the production path, at every window from 256 tokens (too small) to roomy,
    // a prompt that would be sent carries each notice exactly once, and a window that
    // cannot hold them says so instead of sending a prompt without them.
    {
        let (mut sent, mut refused) = (0, 0);
        for window in (256u32..=65_536).step_by(2_048) {
            let view = faculty.prompt_view_within(&ws, window);
            if view.capacity_error.is_some() || view.budget_fault.is_some() {
                refused += 1;
                continue;
            }
            sent += 1;
            let text = view.messages.iter().map(ChatMessage::content_text).collect::<Vec<_>>().join("\n");
            assert_eq!(text.matches("[no-deliverable]").count(), 1, "window {window}");
            assert_eq!(text.matches("[hands]").count(), 1, "window {window}");
            assert_eq!(text.matches(crate::cognition::framing_echo::NOTICES_HEADER).count(), 1, "window {window}");
            assert!(text.contains(&format!("work/get(card_id=\"{}\")", ledger.card_id)), "window {window}");
        }
        assert!(sent > 0, "some window holds her notices");
        // Cormac on #5376: both sides, so a refusal path that never runs cannot pass.
        assert!(refused > 0, "a window too small for her notices refuses the turn");
    }
    let prompt = faculty.messages_unfitted(&ws, None);
    let all = prompt.required_tokens() + LlmDeliberationFaculty::messages_cost(&prompt.history);
    let fitted = faculty.fit_messages(prompt, all).expect("inline ledger fits");
    assert!(fitted.messages.iter().any(|message| message.content_text().contains("large-ledger-detail")));
    wm.pin_fact("ledger", "[ledger] No saved note remains.");
    assert!(wm.notice_details().is_empty(), "replacement retires old expandable detail");
    wm.pin_expandable_fact("ledger", &reference, Some(render_for_turn(&ledger, ledger.by)));
    wm.end_of_work_turn();
    assert!(wm.notice_details().is_empty(), "detail and reference share expiry");
    assert!(!wm.notices().iter().any(|notice| notice.contains("work/get")));
    // A notice-only memory still fits: its notices are her view's required source.
    let only = Arc::new(WorkingMemory::new(8));
    only.record_fact("[no-deliverable] No workspace change yet.");
    let faculty = LlmDeliberationFaculty::new(
        Uuid::new_v4(), "T", "You are T.", Arc::new(HeuristicInferenceAdapter::new()),
    ).with_working_memory(only);
    ws.broadcast.clear();
    let view = faculty.prompt_view_within(&ws, 16_384);
    assert!(view.capacity_error.is_none() && view.budget_fault.is_none(), "fact-only input fits");
    assert_eq!(view.messages.iter().filter(|message| message.content_text().contains("[no-deliverable]")).count(), 1);
}

// what this catches: an UNBOUNDED required room payload making a citizen mute.
// Room updates are required input — `required_tokens` counts them and
// `fit_messages` never drops them — so before this bound a busy room drove the
// required side past the whole window, `grounding_budget` saturated to 0, and
// every turn was refused. Measured live 2026-09-09: conversation_tokens=53522
// against a 23,040 window, 192 of 229 cognition cycles refused (84%), both
// citizens mute for hours while serving reported ready.
//
#[tokio::test]
async fn a_busy_room_cannot_make_a_citizen_mute() {
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let window = 8192u32;
    let mut ws = Workspace::new("what is the state of the board?");
    // Forty updates of ~200 tokens each: far past the whole window, let alone
    // its required share. This is a normal busy room, not a pathological one.
    let updates: Vec<_> = (0..40)
        .map(|i| {
            Arc::new(crate::cognition::conversation::IncomingMessage {
                work: None,
                media: Vec::new(),
                event_id: Uuid::new_v4(),
                lamport: i as u64 + 1,
                peer_id: Uuid::new_v4(),
                room_id: Uuid::new_v4(),
                text: format!("update {i} ") + &"room chatter ".repeat(200),
            })
        })
        .collect();
    ws.room_updates = Arc::new(updates);
    ws.now_ms = Some(1);

    let faculty =
        LlmDeliberationFaculty::new(Uuid::new_v4(), "Ivar", "You are Ivar.", adapter)
            .with_context_window(window);

    let view = faculty.prompt_view(&ws);
    assert!(
        view.capacity_error.is_none(),
        "a busy room must not exhaust the required budget: {:?}",
        view.capacity_error
    );

    // Every update is a conversation turn of her view keyed by its event, so the room
    // cannot inflate the required input at all (it was required under a quarter-window
    // share); the newest reaches the prompt whole.
    let text: Vec<String> = view.messages.iter().map(|m| m.content_text().to_string()).collect();
    assert!(text.iter().any(|t| t.contains("update 39")), "the newest update reaches her prompt");
    assert!(
        !view.messages.last().is_some_and(|m| m.content_text().contains("[Room message received during this turn")),
        "room updates ride her view, not the required tail"
    );
}

// what this catches: e731576c — fuzzy and verbatim history dedupe both
// lose real state changes. Inspect the actual adapter request at each
// step: passed -> failed -> passed must remain three distinct events.
// 7a519f86: dated events and unknown times survive same-role coalescing
// on the actual adapter wire; advancing now must not rewrite old dates.
#[tokio::test]
async fn own_history_preserves_corrections_at_each_inference_step() {
    use crate::ai::types::{ToolCall, ToolResult};
    use crate::cognition::act_observe::{ActStatus, Observation, ToolOutput, ToolVerb};
    use crate::cognition::working_memory::WorkingMemory;

    let home = tempfile::tempdir().expect("isolated demand and emission storage");
    let _native = crate::paths::NativeHomeOverride::install(home.path());
    let persona = Uuid::new_v4();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let wm = Arc::new(WorkingMemory::new(16));
    let faculty = LlmDeliberationFaculty::new(
        persona,
        "Anwen",
        "You are Anwen.",
        Arc::new(
            HeuristicInferenceAdapter::new().with_request_recorder(Arc::clone(&calls)),
        ),
    )
    .with_context_window(32_768)
    .with_working_memory(Arc::clone(&wm));
    let own_reports = [
        "I verified the build and the tests passed.",
        "I verified the build and the tests failed.",
        "I verified the build and the tests passed.",
    ];
    let peer_updates = [
        "The initial candidate is ready for review.",
        "The regression test exposed a failure in that candidate.",
        "The corrected candidate is ready for another review.",
    ];
    let room = crate::identity::ActivityRoom::mint();
    let result_rooms = [Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
    let mut turns = vec![
        BurstTurn::attributed(
            false,
            "Operator",
            "Deployment pending.",
            Some(1_699_913_600_123),
        ),
        BurstTurn::attributed(
            false,
            "Operator",
            "Deployment completed.",
            Some(1_700_000_000_123),
        ),
        BurstTurn::attributed(false, "Operator", "Undated imported report.", None),
        BurstTurn::attributed(false, "Operator", "Unmeasured imported report.", Some(0)),
    ];
    let dated_history =
        "[occurred 2023-11-13T22:13:20.123Z] Operator: Deployment pending.\n\
                [occurred 2023-11-14T22:13:20.123Z] Operator: Deployment completed.\n\
                [occurrence time unknown] Operator: Undated imported report.\n\
                [occurrence time unknown] Operator: Unmeasured imported report.";
    for (step, (report, update)) in own_reports.iter().zip(peer_updates).enumerate() {
        let occurred_at = 1_700_000_000_123 + step as u64 * 86_400_000;
        turns.push(BurstTurn::attributed(
            false,
            "Asha",
            update,
            Some(occurred_at),
        ));
        turns.push(BurstTurn::attributed(
            true,
            "Anwen",
            *report,
            Some(occurred_at),
        ));
        let call_id = format!("verify-{step}");
        wm.record_receipt_typed(
            &[Observation {
                call: ToolCall {
                    id: call_id.clone(),
                    name: "code/run".into(),
                    input: serde_json::json!({"attempt": step}),
                },
                output: ToolOutput {
                    image: None,
                    additional_images: None,
                    result: ToolResult {
                        tool_use_id: call_id,
                        content: (*report).into(),
                        is_error: None,
                        custody: crate::ai::types::ResultCustody::Unsaved,
                    },
                    verb: ToolVerb::classify("code/run"),
                    paths: Vec::new(),
                    verdict: Default::default(),
                },
                status: ActStatus::Executed,
            }],
            report,
            Some(result_rooms[step]),
            crate::ai::types::ResultCustody::Unsaved,
        );
        let mut ws = Workspace::new(crate::cognition::workspace::Burst::from_turns(
            room,
            turns.clone(),
        ));
        let required_identity = faculty.messages_unfitted(&ws, None).input_identity;
        ws.now_ms = Some(occurred_at + 86_400_000);
        assert_eq!(
            faculty.messages_unfitted(&ws, None).input_identity,
            required_identity
        );
        let stimulus_index = ws.turns.len() - 2;
        ws.turns[stimulus_index].occurred_at_ms = Some(occurred_at + 1);
        assert_ne!(
            faculty.messages_unfitted(&ws, None).input_identity,
            required_identity,
            "required stimulus identity includes its rendered occurrence time"
        );
        ws.turns[stimulus_index].occurred_at_ms = Some(occurred_at);
        let outcome = faculty.contribute(&ws).await.expect("deliberation outcome");
        assert!(outcome.fault.is_none());
        let recorded = calls.lock().expect("actual request recorder");
        assert_eq!(recorded.len(), step + 1);
        let request = recorded.last().expect("submitted request");
        // One message per turn (her view keys each turn), each keeping its event date. Her
        // actions lead her history in the view (`view_sources`), so the turns start after them.
        let first_turn = request
            .messages
            .iter()
            .position(|message| message.content_text().starts_with("[occurred "))
            .expect("her conversation is in the request");
        for (index, line) in dated_history.lines().enumerate() {
            assert_eq!(request.messages[first_turn + index].role, "user");
            assert_eq!(
                request.messages[first_turn + index].content_text(),
                line,
                "history retains event dates across later turns"
            );
        }
        let history: Vec<_> = request
            .messages
            .iter()
            .filter(|message| message.role == "assistant" && matches!(&message.content, crate::ai::types::MessageContent::Text(_)))
            .map(|message| message.content_text())
            .collect();
        let expected: Vec<_> = own_reports[..=step]
            .iter()
            .enumerate()
            .map(|(index, report)| {
                format!(
                    "[occurred 2023-11-{:02}T22:13:20.123Z] {report}",
                    14 + index
                )
            })
            .collect();
        assert_eq!(history, expected);
        for update in &peer_updates[..=step] {
            assert!(request.messages.iter().any(|message| {
                message.role == "user"
                    && message.content_text().contains(&format!("Asha: {update}"))
            }));
        }
        for (index, result_room) in result_rooms[..=step].iter().enumerate() {
            let provenance =
                format!("#{}; room {result_room}; operation code/run", index + 1);
            assert!(request.messages.iter().any(|message| {
                message.role == "user" && message.content_text().contains(&provenance)
            }));
        }
    }
}

// what this catches: end-to-end through a REAL adapter (the deterministic
// heuristic stand-in) — the faculty calls inference and produces a verdict
// Contribution. Proves the faculty wires to the AIProviderAdapter trait the
// live path uses; swap in LlamaCppAdapter for a live persona, unchanged.
#[tokio::test]
async fn deliberates_through_a_real_adapter() {
    let persona = Uuid::new_v4();
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let faculty = LlmDeliberationFaculty::new(
        persona,
        "Ivar",
        "You are Ivar, a thoughtful engineer on the grid.",
        adapter,
    );

    assert!(faculty.reacts_to_broadcast(), "deliberation is phase 2");

    // A workspace with a phase-1 recall context bid already broadcast.
    let mut ws = Workspace::new("teammate asks: where did we land on the deploy?");
    ws.broadcast.push(Contribution::context(
        FacultyId::Recall,
        "deploy pipeline was red; fix was merged at 4pm",
        0.8,
        "recalled",
    ));

    let c = faculty
        .contribute(&ws)
        .await
        .expect("deliberation should bid a verdict when inference succeeds");
    assert_eq!(c.faculty, FacultyId::Deliberation);
    assert!(
        c.decision.is_some(),
        "deliberation carries the participation verdict"
    );
    // The heuristic adapter acks (never emits PASS), so this is a Speak.
    assert!(matches!(c.decision, Some(Decision::Speak { .. })));
}

// The actual faculty/adapter boundary, not a hand-written lifecycle:
// request is visible while generate awaits; completion, failure and
// cancellation are distinct. Reading playback cannot call the adapter.
#[tokio::test]
async fn playback_captures_actual_dispatch_before_completion_and_cancellation() {
    use crate::cognition::prompt_capture::{self, CallStatus, JsonlPromptCaptureSink};
    let dir = tempfile::tempdir().expect("capture fixture directory");
    let persona = Uuid::new_v4();
    let room = Uuid::new_v4();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let adapter = Arc::new(
        HeuristicInferenceAdapter::new()
            .with_delay_ms(60)
            .with_request_recorder(Arc::clone(&calls)),
    );
    let faculty = LlmDeliberationFaculty::new(persona, "Ivar", "You are Ivar.", adapter)
        .with_prompt_capture(Arc::new(
            JsonlPromptCaptureSink::open(dir.path(), persona).expect("real capture writer"),
        ));
    let ws = Workspace::in_room("Review this real task and its receipt.", room)
        .with_cycle(crate::cognition::workspace::CycleId(17));
    let mut turn = Box::pin(faculty.contribute(&ws));
    tokio::select! {
        biased;
        _ = &mut turn => panic!("delayed fixture must remain in flight"),
        _ = tokio::time::sleep(std::time::Duration::from_millis(5)) => {}
    }
    let pending =
        prompt_capture::page(dir.path(), persona, None, false, 10).expect("pending page");
    assert_eq!(pending.entries.len(), 1);
    let first = &pending.entries[0];
    assert!(matches!(first.status, CallStatus::Submitted));
    assert_eq!(first.cycle_id, Some(17));
    assert_eq!(first.room_id, room);
    let input =
        prompt_capture::detail(dir.path(), persona, &first.cursor).expect("pending input");
    assert!(input.terminal.is_none());
    assert!(
        input.submitted.as_ref().expect("submitted")["request"]["messages"]
            .to_string()
            .contains("Review this real task")
    );
    let verdict = turn.await.expect("completed deliberation");
    assert!(verdict.decision.is_some());
    let completed =
        prompt_capture::page(dir.path(), persona, Some(&first.cursor), true, 10)
            .expect("terminal page");
    assert_eq!(completed.entries.len(), 1);
    assert!(matches!(completed.entries[0].status, CallStatus::Completed));
    let detail = prompt_capture::detail(dir.path(), persona, &completed.entries[0].cursor)
        .expect("completed payload");
    assert!(detail.issues.is_empty());
    {
        let actual = calls.lock().expect("fixture request recorder");
        assert_eq!(actual.len(), 1, "playback reads never invoke inference");
        assert_eq!(
            actual[0].request_id.as_deref(),
            Some(first.request_id.as_str())
        );
        // Match the actual serialized wire representation: f32 JSON
        // numbers need not equal serde_json::to_value's widened f64.
        let wire_request: serde_json::Value = serde_json::from_str(
            &serde_json::to_string(&actual[0]).expect("typed request wire serialization"),
        )
        .expect("serialized wire request JSON");
        assert_eq!(
            wire_request,
            detail.submitted.as_ref().expect("submitted")["request"]
        );
    }
    let legacy =
        prompt_capture::completed_file(&dir.path().join(format!("{persona}.jsonl")), 10)
            .expect("existing prompt/dataset reader");
    assert_eq!(legacy.len(), 1);
    assert!(legacy[0]["messages"]
        .to_string()
        .contains("Review this real task"));
    assert!(legacy[0]["response"]["text"].is_string());

    let mut malformed =
        prompt_capture::detail(dir.path(), persona, &completed.entries[0].cursor)
            .expect("completed payload for integrity check");
    malformed.submitted.as_mut().expect("submitted request")["request"]
        .as_object_mut()
        .expect("typed request object")
        .remove("messages");
    assert!(
        prompt_capture::legacy_projection(malformed).is_err(),
        "missing required payload must stay explicit, not resemble a filtered provider failure"
    );

    let mut cancelled = Box::pin(faculty.contribute(&ws));
    tokio::select! {
        biased;
        _ = &mut cancelled => panic!("delayed fixture must remain in flight"),
        _ = tokio::time::sleep(std::time::Duration::from_millis(5)) => {}
    }
    drop(cancelled);
    let page =
        prompt_capture::page(dir.path(), persona, None, false, 10).expect("cancelled page");
    assert!(matches!(
        page.entries.last().expect("terminal").status,
        CallStatus::Cancelled
    ));
    assert_ne!(
        page.entries[0].request_id, page.entries[2].request_id,
        "two calls on one workspace have distinct actual request IDs"
    );
}

#[tokio::test]
async fn playback_records_inference_failure_without_a_fake_response() {
    use crate::cognition::prompt_capture::{self, CallStatus, JsonlPromptCaptureSink};
    let dir = tempfile::tempdir().expect("capture fixture directory");
    let persona = Uuid::new_v4();
    let faculty = LlmDeliberationFaculty::new(
        persona,
        "Ivar",
        "You are Ivar.",
        Arc::new(HeuristicInferenceAdapter::new().with_responses(Vec::new())),
    )
    .with_prompt_capture(Arc::new(
        JsonlPromptCaptureSink::open(dir.path(), persona).expect("capture sink"),
    ));
    let ws = Workspace::in_room("Review the pending task.", Uuid::new_v4());
    let result = faculty.contribute(&ws).await.expect("fault contribution");
    assert!(result.fault.is_some());
    let page = prompt_capture::page(dir.path(), persona, None, false, 10)
        .expect("failed call page");
    let last = page.entries.last().expect("failed terminal");
    assert!(matches!(last.status, CallStatus::Failed));
    let detail =
        prompt_capture::detail(dir.path(), persona, &last.cursor).expect("failure detail");
    let terminal = detail.terminal.expect("failed terminal payload");
    assert!(terminal["response"].is_null());
    assert!(terminal["error"]
        .as_str()
        .expect("failure cause")
        .contains("exhausted"));
    assert!(
        prompt_capture::completed_file(&dir.path().join(format!("{persona}.jsonl")), 10)
            .expect("compatibility reader")
            .is_empty(),
        "failed calls must not become training examples"
    );
}

// A transport-successful provider failure must retain its partial output
// for inspection without accepting it as a decision or training example.
#[tokio::test]
async fn playback_excludes_typed_error_responses_from_completed_training_rows() {
    use crate::cognition::prompt_capture::{self, CallStatus, JsonlPromptCaptureSink};
    let dir = tempfile::tempdir().expect("capture fixture directory");
    let persona = Uuid::new_v4();
    let mut response = HeuristicInferenceAdapter::new()
        .generate_text(TextGenerationRequest {
            messages: vec![ChatMessage::text("user", "fixture input")],
            ..Default::default()
        })
        .await
        .expect("shared fixture response");
    // This is a valid participation decision when the provider succeeds;
    // the identical partial text must not override either error carrier.
    response.text = "Ship it.".into();
    response.finish_reason = FinishReason::Stop;
    let mut finish_error = response.clone();
    finish_error.finish_reason = FinishReason::Error;
    let mut field_error = response.clone();
    field_error.error = Some("provider reported a typed failure".into());
    let faculty = LlmDeliberationFaculty::new(
        persona,
        "Ivar",
        "You are Ivar.",
        Arc::new(HeuristicInferenceAdapter::new().with_responses(vec![
            finish_error,
            field_error,
            response,
        ])),
    )
    .with_prompt_capture(Arc::new(
        JsonlPromptCaptureSink::open(dir.path(), persona).expect("capture sink"),
    ));
    let ws = Workspace::in_room("Review the task.", Uuid::new_v4());
    for _ in 0..2 {
        let contribution = faculty
            .contribute(&ws)
            .await
            .expect("visible provider fault");
        assert!(contribution.fault.is_some());
        assert!(
            contribution.decision.is_none(),
            "a failed partial answer is not a decision"
        );
    }
    let accepted = faculty.contribute(&ws).await.expect("successful control");
    assert!(accepted.fault.is_none());
    assert!(
        matches!(accepted.decision, Some(Decision::Speak { text }) if text == "Ship it.")
    );
    let page = prompt_capture::page(dir.path(), persona, None, false, 10)
        .expect("typed error page");
    assert_eq!(page.entries.len(), 6);
    let failures: Vec<_> = page
        .entries
        .iter()
        .filter(|entry| matches!(entry.status, CallStatus::Failed))
        .collect();
    assert_eq!(failures.len(), 2);
    for terminal in failures {
        let detail = prompt_capture::detail(dir.path(), persona, &terminal.cursor)
            .expect("actual error response");
        assert_eq!(
            detail.terminal.expect("terminal record")["response"]["text"],
            "Ship it.",
            "the actual failed partial response remains inspectable"
        );
    }
    let completed =
        prompt_capture::completed_file(&dir.path().join(format!("{persona}.jsonl")), 10)
            .expect("completed reader");
    assert_eq!(
        completed.len(),
        1,
        "only the successful control can become a training candidate"
    );
    assert_eq!(completed[0]["response"]["text"], "Ship it.");
}

// what this catches: the live-airc bug where a grown room burst + many full
// engrams made the deliberation prompt exceed the served window, so
// llama-server 500'd ("Context size has been exceeded") on EVERY tick and the
// persona went mute (logged as "chose silence"). prompt_view must keep
// system+user within `context_window` minus the completion reserve — context
// is enrichment and yields first, the burst trims from the head (newest kept),
// the essential framing always survives. Regression for the 8192-overflow.
#[test]
fn prompt_view_stays_within_the_served_window() {
    let persona = Uuid::new_v4();
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    // A deliberately small window so the test is cheap but exercises the same
    // arithmetic the live 8192 path uses.
    let window: u32 = 1024;
    let faculty = LlmDeliberationFaculty::new(
        persona,
        "Ivar",
        "You are Ivar, a thoughtful engineer on the grid.",
        adapter,
    )
    .with_context_window(window);

    // A burst far bigger than the whole window (a grown room history), with a
    // recognizable LAST line that must survive the head-trim.
    let mut ws = history_with_stimulus("LATEST: did the deploy fix land?");
    // Several oversized context bids — recall engrams that alone blow the budget.
    ws.broadcast.push(Contribution::context(
        FacultyId::Recall,
        &"deploy pipeline observation; ".repeat(2000),
        0.9,
        "recalled",
    ));
    ws.broadcast.push(Contribution::context(
        FacultyId::Recall,
        "small high-value note: fix merged 4pm",
        0.5,
        "recalled",
    ));

    let view = faculty.prompt_view(&ws);
    let total = estimate_guard_tokens(&view.system) + estimate_guard_tokens(&view.user_text());
    assert!(
        total <= window as usize,
        "prompt must fit the served window: {total} tokens > {window}"
    );
    // The newest burst line survives the tail-keep — the turn is about it.
    assert!(
        view.user_text()
            .contains("LATEST: did the deploy fix land?"),
        "the most recent burst content must survive under budget pressure"
    );
    // The framing is essential and always present even under extreme pressure.
    assert!(
        view.system.contains("Taking your turn"),
        "the how-to-participate framing must never be dropped"
    );
}

// what this catches: the starved-claimant regression (card f6a9fe5c, live
// specimen 2026-08-07) — under a squeezed window the grounding floor reserved
// only the SMALLEST offered contribution, so a citizen HOLDING a work card got
// a ~50-token status note while active-work (the card content itself) dropped;
// her whole knowledge of her claim was a bare receipt and she yielded. The fix
// is a priority ordering: the floor reserves the active-work contribution when
// one is offered AND selection considers it first, so her held card survives
// budget pressure that drops everything else.
#[test]
fn a_held_work_card_survives_budget_pressure_that_drops_everything_else() {
    let persona = Uuid::new_v4();
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let window: u32 = 1024;
    let faculty = LlmDeliberationFaculty::new(
        persona,
        "Ivar",
        "You are Ivar, a thoughtful engineer on the grid.",
        adapter,
    )
    .with_context_window(window);

    // A conversation big enough to absorb every token the floor doesn't hold.
    let mut ws = history_with_stimulus("LATEST: how is the card going?");
    // A recall bid that outranks active-work on salience and dwarfs the budget —
    // without reserved-first selection it would spend the floor's reservation.
    ws.broadcast.push(Contribution::context(
        FacultyId::Recall,
        &"deploy pipeline observation; ".repeat(2000),
        1.0,
        "recalled",
    ));
    // The tiny note the OLD floor would have sized the reservation to.
    ws.broadcast.push(Contribution::context(
        FacultyId::Custom("session-note".into()),
        "note: room quiet",
        0.95,
        "noted",
    ));
    // Her held card — what a claim-holding citizen must never lose sight of.
    ws.broadcast.push(Contribution::context(
        FacultyId::Custom(crate::cognition::faculty_ids::ACTIVE_WORK.into()),
        "[your work] psf__requests-2148 staged in workspace/swe — fix in place, tests are the grade",
        0.9,
        "held claims",
    ));

    let view = faculty.prompt_view(&ws);
    // The room's context rides her messages (after her view), never the system message.
    assert!(
        view.messages.iter().any(|m| m.content_text().contains("psf__requests-2148")),
        "the held card's content must survive budget pressure; messages were:\n{}",
        view.user_text()
    );
}

// what this catches: the reply's claim as one claimant in the allocation: its floor (an
// act's measured answer channel), its want (her measured need, or the reply-sized prior),
// its priority from the turn (ambient 4, directed 6, act 8, act on live work or after a
// pending cut 10), a pending cut's measured peak retained, and NO
// token cap: a cap claimed above her context starved her reads (the M5, 2026-10-30,
// ONE-BUDGET.md slice 6). The time/need terms are reported, never the bound.
#[test]
fn the_reply_claims_her_measured_need_and_no_cap() {
    use crate::cognition::working_set::OutputNeed;
    let floor = LlmDeliberationFaculty::COMPLETION_FLOOR_TOKENS;
    let prior = LlmDeliberationFaculty::COMPLETION_COLD_PRIOR_TOKENS;
    // (floor, wanted, priority)
    let shape = |c: crate::cognition::rag_budget::RagSourceBudget| {
        assert_eq!(c.min_tokens, c.floor_tokens, "above its floor the reply grows by priority");
        assert!(c.required, "the reply always gets its floor");
        (c.floor_tokens, c.max_tokens, c.priority)
    };
    // Unmeasured: a reply-sized prior, never a window fraction.
    assert_eq!(shape(reply_claim(TurnKind::Pass, None, 0, false, false)), (floor, prior, 4));
    assert_eq!(shape(reply_claim(TurnKind::Pass, None, 0, false, true)), (floor, prior, 6));
    assert_eq!(shape(reply_claim(TurnKind::Act, None, 0, false, false)), (floor, prior, 8));
    assert_eq!(shape(reply_claim(TurnKind::Act, None, 0, true, false)).2, 10, "live held work");
    assert_eq!(shape(reply_claim(TurnKind::Act, None, 1, false, false)).2, 10, "a pending cut");
    // Measured: her think + answer, whole; an act floors at her answer channel.
    let need = OutputNeed { reasoning: 900, answer: 300, turns: 5, reasoning_at_least: None };
    assert_eq!(shape(reply_claim(TurnKind::Pass, Some(need), 0, false, false)).1, 1_200);
    let kimi = OutputNeed { reasoning: 6_100, answer: 4_077, turns: 32, reasoning_at_least: None };
    assert_eq!(shape(reply_claim(TurnKind::Act, Some(kimi), 0, false, false)), (4_077, 10_177, 8));
    // A large measured need claims all of it: the window and the allocation bound it.
    let huge = OutputNeed { reasoning: 30_000, answer: 20_000, turns: 9, reasoning_at_least: None };
    assert_eq!(shape(reply_claim(TurnKind::Act, Some(huge), 0, false, false)), (20_000, 50_000, 8));
    // A pending cut's measured peak is retained as demand, not doubled again (cf9e's 3,587).
    assert_eq!(shape(reply_claim(TurnKind::Pass, Some(need), 3_587, false, false)).1, 3_587);
    // The terms beside the allowance: reported, never the bound.
    let a = output_allowance(TurnKind::Pass, Some(12.0), None, 6_932);
    assert_eq!((a.allowance, a.time_term, a.need_term), (6_932, Some(720), None));
    let fast = output_allowance(TurnKind::Act, Some(40.0), Some(need), 6_932);
    assert_eq!((fast.allowance, fast.time_term), (6_932, Some(6_000)));
    let clipped = output_allowance(
        TurnKind::Pass,
        None,
        Some(OutputNeed { reasoning: 3_000, answer: 500, turns: 5, reasoning_at_least: None }),
        2_172,
    );
    assert_eq!((clipped.allowance, clipped.need_clipped), (2_172, true), "a need past the grant is said");
    assert_eq!(output_allowance(TurnKind::Pass, None, None, 0).allowance, 1, "never a zero bound");
    // A rate of 0.0 or NaN is no rate (the M5's `measured_tps=0.0`), never a term.
    assert_eq!(output_allowance(TurnKind::Act, Some(0.0), Some(need), 6_932).time_term, None);
    assert_eq!(output_allowance(TurnKind::Act, Some(f64::NAN), Some(need), 6_932).time_term, None);
    // The split that feeds the need: the server's count, apportioned by channel bytes.
    assert_eq!(reasoning_answer_split(1_000, 2_730, 910), (750, 250));
    assert_eq!(reasoning_answer_split(500, 0, 0), (0, 500), "no channel text (a native call): all answer");
    assert_eq!(reasoning_answer_split(300, 2_112, 0), (300, 0), "a think-only turn is all reasoning");
    // what this catches: a thinking model's native file-writing arguments
    // being counted as zero answer, so an output cut never grows that channel.
    let parts = vec![
        crate::ai::types::ContentPart::Text { text: "note".into() },
        crate::ai::types::ContentPart::ToolUse {
            id: "call-1".into(),
            name: "write_file".into(),
            input: serde_json::json!({"path": "migration.sql", "content": "CREATE TABLE example(id INTEGER);"}),
        },
    ];
    let tool_bytes = answer_payload_bytes("", Some(&parts));
    assert!(tool_bytes > 0);
    assert_eq!(answer_payload_bytes("note", Some(&parts)), tool_bytes + 4,
        "mirrored Text parts are counted once");
    let (think, answer) = reasoning_answer_split(1000, 2000, tool_bytes);
    assert_eq!(think + answer, 1000);
    assert!(answer > 0);
    assert_eq!(crate::cognition::working_set::EmissionStop::classify(true, answer > 0),
        crate::cognition::working_set::EmissionStop::CutMidAnswer);
}

// what this catches: the request carries the view's grant verbatim, on an act and on a pass
// alike, and an unmeasured reply's grant is the cold prior, never a runaway cap claimed above
// her context: a 12,288-token act cap starved her reads on the M5 (2026-10-30, ONE-BUDGET.md
// slice 6). An idle window's slack is her view's room, not the reply's.
#[tokio::test]
async fn an_unmeasured_turn_carries_its_measured_grant_and_no_cap() {
    let window = 32_768u32;
    let persona = Uuid::new_v4();
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let faculty = LlmDeliberationFaculty::new(
        persona,
        "Ivar",
        "You are Ivar, a thoughtful engineer on the grid.",
        adapter,
    )
    .with_context_window(window);
    let ws = Workspace::new("act on the card");
    let view = faculty.prompt_view(&ws);
    assert_eq!(view.completion_reserve, LlmDeliberationFaculty::COMPLETION_COLD_PRIOR_TOKENS, "an unmeasured reply claims the prior, no cap");
    let binding = faculty.binding.load_full();
    for (kind, purpose) in [
        (TurnKind::Act, "cognition/act"),
        (TurnKind::Pass, "cognition/deliberation"),
    ] {
        let request = faculty.build_request_within(
            &binding,
            view.completion_reserve,
            view.messages.clone(),
            None,
            view.system.clone(),
            None,
            Some(ws.room_id),
            kind,
        );
        assert_eq!(request.max_tokens, Some(view.completion_reserve), "{purpose}: the grant, verbatim");
        assert_eq!(request.purpose.as_deref(), Some(purpose));
    }
}
