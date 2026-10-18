#![cfg(test)]

use super::*;

// Regression: Kimi's seventh perception bid evicted working memory and its
// recorded no-deliverable facts. Fit the actual projection after that eviction.
#[tokio::test]
async fn substrate_notices_survive_attention_and_history_pressure_once() {
    use crate::cognition::working_memory::{WorkingMemory, WorkingMemoryFaculty};
    use crate::cognition::workspace::{Arbiter, Faculty, FocusContext, SalienceArbiter, Situation};
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
    let memory = WorkingMemoryFaculty::new(Arc::clone(&wm)).contribute(&ws).await.unwrap();
    assert!(!memory.content.contains("[no-deliverable]"), "notices have one projection");
    let mut bids: Vec<_> = (0..6).map(|i| Contribution::context(
        FacultyId::Custom(format!("source-{i}")), "standing context", 0.9, "fixture",
    )).collect();
    bids.push(memory.clone());
    ws.broadcast = SalienceArbiter.focus(bids, 6, &FocusContext {
        world_state: &ws.world_state, situation: Situation::PostAction,
    });
    assert!(!ws.broadcast.iter().any(|c| c.faculty == memory.faculty));
    for keep_memory in [false, true] {
        if keep_memory { ws.broadcast.push(memory.clone()); }
        let mut prompt = faculty.messages_unfitted(&ws, None);
        let required = prompt.required_tokens();
        assert!(required > 0);
        prompt.history.push(ChatMessage::text("user", "old history ".repeat(500)));
        let fitted = faculty.fit_messages(prompt, required).expect("required notices fit");
        let text = fitted.messages.iter().map(ChatMessage::content_text).collect::<Vec<_>>().join("\n");
        assert_eq!(text.matches("[no-deliverable]").count(), 1);
        assert_eq!(text.matches("[hands]").count(), 1);
        assert!(text.contains(crate::cognition::framing_echo::NOTICES_HEADER));
        assert!(text.contains(&format!("work/get(card_id=\"{}\")", ledger.card_id)));
        assert!(!text.contains("large-ledger-detail"), "oversized detail yields without muting the turn");
        assert!(!text.contains("weighing the next step"), "optional thought trail yielded; notices retain substrate provenance");
        let prompt = faculty.messages_unfitted(&ws, None);
        assert!(faculty.fit_messages(prompt, required - 1).is_err(), "shortfall is explicit, not dropped feedback");
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
    // A notice-only memory has no optional thought bid at all.
    let only = Arc::new(WorkingMemory::new(8));
    only.record_fact("[no-deliverable] No workspace change yet.");
    assert!(WorkingMemoryFaculty::new(Arc::clone(&only)).contribute(&ws).await.is_none());
    let faculty = faculty.with_working_memory(only);
    ws.broadcast.clear();
    let prompt = faculty.messages_unfitted(&ws, None);
    let required = prompt.required_tokens();
    let fitted = faculty.fit_messages(prompt, required).expect("fact-only input");
    assert_eq!(fitted.messages.iter().filter(|message| message.content_text().contains("[no-deliverable]")).count(), 1);
}

/// A citizen must never be silenced by her own last tool result.
mod an_action_result_cannot_mute_a_citizen {
    use super::*;

    fn faculty_with_window(window: u32) -> LlmDeliberationFaculty {
        LlmDeliberationFaculty::new(
            Uuid::new_v4(),
            "Sahar",
            "You are Sahar.",
            Arc::new(HeuristicInferenceAdapter::new()),
        )
        .with_context_window(window)
    }

    // what this catches: the live 2026-09-10 mute, and it is the THIRD instance of
    // one class. A `code/read` / `code/shell` output goes in as REQUIRED input, and
    // it was unbounded — so one big result claimed the whole window and
    // `fit_messages` refused every turn. Measured: required_payload 18,629 against
    // after_reserves 15,688 on a 23,040 lane, grounding only 113 (innocent),
    // received = 2 (ONE enormous message), 836 consecutive refusals, zero acts for
    // a whole benchmark round.
    #[test]
    fn an_oversized_result_is_split_not_swallowed_whole() {
        let faculty = faculty_with_window(4_096);
        let huge: String = (0..4_000)
            .map(|i| {
                format!(
                    "line {i} of a very large tool result
"
                )
            })
            .collect();
        // (required, demoted) = (the result's ENDING, its earlier part). The ending
        // is what stays required so the wire reads in the original order — see
        // `split_action_result` for why history's position decides this.
        let (required, demoted) = faculty.split_action_result(huge.clone());
        let demoted = demoted.expect("an oversized result must yield a demoted part");

        let cap = 4_096 / LlmDeliberationFaculty::RESULT_SHARE_DENOM as usize;
        let required_cost =
            LlmDeliberationFaculty::messages_cost(std::slice::from_ref(&required));
        assert!(
            required_cost <= cap,
            "the required part must fit its share: {required_cost} > {cap}"
        );

        // NOTHING IS DISCARDED. Head + tail reconstitute the original exactly —
        // the property that separates demotion from truncation, and the one Joel
        // asked for by name after the first cut of #3947 dropped instead of demoted.
        let rebuilt = format!("{}{}", demoted.content_text(), required.content_text());
        assert_eq!(
            rebuilt, huge,
            "demoted + required must reconstitute the result byte-for-byte"
        );
    }

    // what this catches: THE WIRING, which the three unit tests above cannot see.
    // They call `split_action_result` directly, so they stay green even when the
    // production build site stops calling it — verified by mutation, and that is
    // exactly the blind source-assertion mistake made earlier the same night. This
    // one goes through `prompt_view`, the real path, and fails the moment the
    // result is required whole again.
    #[tokio::test]
    async fn a_huge_result_does_not_fault_the_turn_on_the_real_path() {
        use crate::cognition::working_memory::WorkingMemory;

        let window = 8_192u32;
        let wm = Arc::new(WorkingMemory::new(8));
        wm.set_served_window(window);
        // A tool result several times the whole window — an ordinary `code/read`
        // of a large file.
        let huge: String = (0..6_000)
            .map(|i| {
                format!(
                    "line {i} of a very large tool result
"
                )
            })
            .collect();
        wm.record_receipt(&huge);

        let faculty = LlmDeliberationFaculty::new(
            Uuid::new_v4(),
            "Sahar",
            "You are Sahar.",
            Arc::new(HeuristicInferenceAdapter::new()),
        )
        .with_context_window(window)
        .with_working_memory(Arc::clone(&wm));

        let ws = Workspace::new("what did that produce?");
        let view = faculty.prompt_view(&ws);
        assert!(
            view.capacity_error.is_none(),
            "one big tool result must not refuse the turn — she holds her card                      while refusing, so this strands a benchmark round: {:?}",
            view.capacity_error
        );
        assert!(
            !view.messages.is_empty(),
            "and she must still be given her result to reason about"
        );
    }

    // what this catches: a result that FITS must be left completely alone. A split
    // that fires unconditionally would push every ordinary tool result into history
    // and cost the citizen the outcome of her own act on the very next turn.
    #[test]
    fn a_result_that_fits_is_untouched() {
        let faculty = faculty_with_window(32_768);
        let small = "ok: wrote 3 lines
"
        .to_string();
        let (required, demoted) = faculty.split_action_result(small.clone());
        assert!(demoted.is_none(), "a small result has nothing to demote");
        assert_eq!(required.content_text(), small);
    }

    // what this catches: the head must never be empty. If the FIRST line alone
    // exceeds the share, it is still required — a citizen handed a demotion notice
    // and none of her own result cannot reason about what her hands just did, and
    // the honest failure for a genuinely impossible payload is the existing
    // capacity fault, not a silently empty head.
    #[test]
    fn the_first_line_is_required_even_when_it_alone_exceeds_the_share() {
        let faculty = faculty_with_window(1_024);
        let one_giant_line = format!(
            "{}
",
            "x".repeat(20_000)
        );
        let (required, demoted) = faculty.split_action_result(one_giant_line.clone());
        assert!(
            !required.content_text().is_empty(),
            "the required part is never empty — she must see what her act produced"
        );
        let rebuilt = format!(
            "{}{}",
            demoted
                .as_ref()
                .map(|d| d.content_text())
                .unwrap_or_default(),
            required.content_text()
        );
        assert_eq!(rebuilt, one_giant_line, "still nothing discarded");
    }

    // what this catches: UTF-8 safety. Splitting on a byte or char budget can cut a
    // multi-byte sequence in half; line granularity cannot. A panic here would take
    // down the cognition cycle on any result containing non-ASCII — which is most
    // of them, since the results carry the citizens' own prose.
    #[test]
    fn splitting_never_cuts_a_multibyte_sequence() {
        let faculty = faculty_with_window(2_048);
        let multibyte: String = (0..2_000)
            .map(|i| {
                format!(
                    "строка {i} — ✅ 日本語テキスト
"
                )
            })
            .collect();
        let (required, demoted) = faculty.split_action_result(multibyte.clone());
        let rebuilt = format!(
            "{}{}",
            demoted
                .as_ref()
                .map(|d| d.content_text())
                .unwrap_or_default(),
            required.content_text()
        );
        assert_eq!(rebuilt, multibyte, "multibyte content survives the split");
    }
}

/// A citizen must never be silenced by her ENRICHMENT not fitting.
mod grounding_never_mutes {
    use super::*;

    // what this catches: the live 2026-09-09 mute. Kimi held `hard-rs/expr_eval`
    // on a 28,160-token lane with a 39,532-token conversation; every 15 s she took
    // a work turn that produced `act=0 model_ms=0`, because 613 tokens of standing
    // grounding did not fit a budget the chained subtraction had saturated to 0,
    // and that grounding error was promoted ahead of every other check into a hard
    // deliberation fault. She kept the card while refusing, so the benchmark round
    // could never finish. Enrichment is not a reason to stop thinking.
    #[test]
    fn an_unaffordable_standing_source_does_not_fault_the_turn() {
        // A SMALL stimulus — the irreducible payload comfortably fits, so nothing
        // about the conversation justifies a refusal. Only the standing sources
        // are unaffordable, and that must not be fatal.
        let mut ws = Workspace::new("What is the state of the card?");
        for id in ["room-roster", "room-wall"] {
            ws.broadcast.push(
                Contribution::context(
                    FacultyId::Custom(id.into()),
                    "standing world state that cannot possibly fit ".repeat(2_000),
                    0.9,
                    "standing",
                )
                .standing_grounding(),
            );
        }
        let faculty = LlmDeliberationFaculty::new(
            Uuid::new_v4(),
            "Kimi",
            "You are Kimi.",
            Arc::new(HeuristicInferenceAdapter::new()),
        )
        .with_context_window(4_096);

        // The fixture must actually be unaffordable, or this test proves nothing.
        let whole = GroundingPlan::minimum(&ws);
        assert!(
            whole.system_reserve() + whole.trailing_tokens() > 4_096,
            "fixture must make the standing minimum genuinely unaffordable"
        );

        let view = faculty.prompt_view(&ws);
        assert!(
            view.capacity_error.is_none(),
            "a citizen whose ENRICHMENT does not fit must still think — she holds                      her card while refusing, so a fault here strands the round forever:                      {:?}",
            view.capacity_error
        );
        assert!(
            !view.messages.is_empty(),
            "and she must be given something to think ABOUT"
        );
    }

    // what this catches: shedding must report WHAT IT WANTED. A reduction that
    // cannot state its own magnitude is the saturating-subtract mistake that
    // destroyed the diagnosis one layer up — "over by 31,000" and "exactly at the
    // line" must not arrive as the same number.
    #[test]
    fn shedding_reports_the_size_it_wanted() {
        let mut ws = Workspace::new("stimulus");
        ws.broadcast.push(
            Contribution::context(
                FacultyId::Custom("room-roster".into()),
                "standing world state".repeat(200),
                0.9,
                "standing",
            )
            .standing_grounding(),
        );
        let whole = GroundingPlan::minimum(&ws);
        let wanted_tokens = whole.system_reserve() + whole.trailing_tokens();
        assert!(
            wanted_tokens > 0,
            "the fixture must actually want something"
        );

        match GroundingPlan::within_budget(&ws, 0) {
            Fitted::Whole(_) => panic!("a zero budget cannot afford this fixture"),
            Fitted::Reduced { plan, wanted } => {
                assert_eq!(wanted, wanted_tokens, "the WANT survives the shed");
                assert_eq!(
                    plan.system_reserve() + plan.trailing_tokens(),
                    0,
                    "nothing is kept that the budget cannot pay for — and a piece                              too big for the budget is dropped whole, never truncated into                              a fragment that reads as complete"
                );
            }
        }

        match GroundingPlan::within_budget(&ws, wanted_tokens) {
            Fitted::Whole(plan) => assert_eq!(
                plan.system_reserve() + plan.trailing_tokens(),
                wanted_tokens,
                "an affordable minimum is kept whole"
            ),
            Fitted::Reduced { .. } => panic!("an exactly-affordable minimum must fit"),
        }
    }
}

// what this catches: an UNBOUNDED required room payload making a citizen mute.
// Room updates are required input — `required_tokens` counts them and
// `fit_messages` never drops them — so before this bound a busy room drove the
// required side past the whole window, `grounding_budget` saturated to 0, and
// every turn was refused. Measured live 2026-09-09: conversation_tokens=53522
// against a 23,040 window, 192 of 229 cognition cycles refused (84%), both
// citizens mute for hours while serving reported ready.
//
// Mutation check: delete the cap in `bounded_room_updates` (collect every
// update instead) and this goes RED with a capacity_error — which is exactly
// the production symptom.
#[tokio::test]
async fn a_busy_room_cannot_make_a_citizen_mute() {
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let window = 8192u32;
    let mut ws = Workspace::new("what is the state of the board?");
    // Forty updates of ~200 tokens each: far past the whole window, let alone
    // its required share. This is a normal busy room, not a pathological one.
    let updates: Vec<_> = (0..40)
        .map(|i| {
            Arc::new(crate::persona::service_loop::IncomingMessage {
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

    // The NEWEST updates stay REQUIRED; the older ones are DEMOTED, not
    // deleted. Joel, 2026-09-09: "maybe trim clipped or killed important
    // information the persona needed to complete things" — a task or a
    // correction that arrived earlier in the batch must not vanish before
    // she reads it, so the overflow rides in ordinary history instead.
    let (required, demoted) = faculty.split_room_updates(&ws);
    assert!(!required.is_empty(), "the newest update is always required");
    assert!(
        !demoted.is_empty(),
        "an over-share payload must actually be demoted"
    );
    assert_eq!(
        required.len() + demoted.len(),
        40,
        "every update survives somewhere — demotion must not DELETE"
    );
    assert!(
        required
            .last()
            .expect("required is non-empty")
            .content_text()
            .contains("update 39"),
        "the NEWEST update must stay required"
    );
    assert!(
        demoted
            .first()
            .expect("demoted is non-empty")
            .content_text()
            .contains("update 0"),
        "the OLDEST update is the one demoted, and it is still present"
    );
}

fn history_with_stimulus(stimulus: &str) -> Workspace {
    use crate::cognition::workspace::Burst;
    let room = crate::identity::ActivityRoom::from_uuid(Uuid::new_v4()).unwrap();
    Workspace::new(Burst::from_turns(
        room,
        vec![
            BurstTurn::attributed(
                false,
                "Peer",
                "old chatter line\n".repeat(2000),
                Some(1),
            ),
            BurstTurn::attributed(false, "Peer", stimulus, Some(2)),
        ],
    ))
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
    wm.set_served_window(32_768);
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
        let history_block = &request.messages[0];
        assert_eq!(history_block.role, "user");
        assert!(
            history_block.content_text().starts_with(dated_history),
            "coalesced history retains event dates across later turns"
        );
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
    let total = est_tokens(&view.system) + est_tokens(&view.user_text());
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
        FacultyId::Custom(crate::persona::active_work_source::SOURCE_ID.into()),
        "[your work] psf__requests-2148 staged in workspace/swe — fix in place, tests are the grade",
        0.9,
        "held claims",
    ));

    let view = faculty.prompt_view(&ws);
    assert!(
        view.system.contains("psf__requests-2148"),
        "the held card's content must survive budget pressure; system was:\n{}",
        view.system
    );
}

// what this catches (the M5, 2026-09-20 20:58–21:14Z, after #4194): the allowance
// fell to a floor — `kind=pass measured_tps=0.0 allowance=768`, `kind=act
// allowance=1500` — and Qwen3.8 ended two turns inside its reasoning channel
// (`persona.act.think_only reasoning_len=2730`, `2112`): no answer, no act. And
// (2026-10-18 02:10Z) Benchy's first write cut mid-call against a reserve of twice
// her read-only peak. Pinned on the pure claim: an unmeasured reply wants a
// reply-sized prior, never a constant floor; a measured need is wanted whole; an
// act floors at her answer channel; a pending cut doubles back; priority follows
// the turn (ambient 4, directed 6, act 8, act on live work or after a cut 10); the
// runaway bounds hold; the time/need terms are reported, never the bound; and the
// channel split that feeds the need.
#[test]
fn the_reply_claim_holds_the_reasoning_channel_under_its_runaway_bound() {
    use crate::cognition::working_set::OutputNeed;
    let floor = LlmDeliberationFaculty::COMPLETION_FLOOR_TOKENS;
    let prior = LlmDeliberationFaculty::COMPLETION_COLD_PRIOR_TOKENS;
    let cap = LlmDeliberationFaculty::ACT_OUTPUT_CAP;
    // (floor, wanted, priority, runaway bound)
    let shape = |(c, r): (crate::persona::rag_budget::RagSourceBudget, u32)| {
        assert_eq!(c.min_tokens, c.floor_tokens, "above its floor the reply grows by priority");
        assert!(c.required, "the reply always gets its floor");
        (c.floor_tokens, c.max_tokens, c.priority, r)
    };
    // Unmeasured: a reply-sized prior, never 768 or any constant floor.
    assert_eq!(shape(reply_claim(TurnKind::Pass, None, 0, false, false)), (floor, prior, 4, cap));
    assert_eq!(shape(reply_claim(TurnKind::Pass, None, 0, false, true)), (floor, prior, 6, cap));
    assert_eq!(shape(reply_claim(TurnKind::Act, None, 0, false, false)), (floor, prior, 8, cap));
    assert_eq!(shape(reply_claim(TurnKind::Act, None, 0, true, false)).2, 10, "live held work");
    assert_eq!(shape(reply_claim(TurnKind::Act, None, 1, false, false)).2, 10, "a pending cut");
    // The M5's shape: a mind that thinks ~900 tokens before a 300-token answer.
    let need = OutputNeed { reasoning: 900, answer: 300, turns: 5 };
    let (_, wanted, _, runaway) = shape(reply_claim(TurnKind::Pass, Some(need), 0, false, false));
    assert_eq!(wanted, 1_200, "her measured think + answer, whole");
    assert_eq!(runaway, cap, "a small need still gets the act's cap as its bound");
    // Kimi (the 5090, 2026-10-06): an act floors at her answer channel and wants
    // her whole turn; a pass is bounded at twice her p90 turn.
    let kimi = OutputNeed { reasoning: 6_100, answer: 4_077, turns: 32 };
    assert_eq!(shape(reply_claim(TurnKind::Act, Some(kimi), 0, false, false)), (4_077, 10_177, 8, cap));
    assert_eq!(shape(reply_claim(TurnKind::Pass, Some(kimi), 0, false, false)).3, 20_354);
    // A pending cut doubles back (cf9e's 3,587 → 7,174), under the runaway bound.
    assert_eq!(shape(reply_claim(TurnKind::Pass, Some(need), 3_587, false, false)).1, 7_174);
    assert_eq!(shape(reply_claim(TurnKind::Act, Some(kimi), 7_174, false, false)).1, cap);
    assert_eq!(shape(reply_claim(TurnKind::Pass, Some(kimi), 7_174, false, false)).1, 14_348);
    // A runaway need never claims past the bound, floor included.
    let huge = OutputNeed { reasoning: 30_000, answer: 20_000, turns: 9 };
    assert_eq!(shape(reply_claim(TurnKind::Act, Some(huge), 0, false, false)), (cap, cap, 8, cap));
    // The terms beside the allowance: reported, never the bound.
    let a = output_allowance(TurnKind::Pass, Some(12.0), None, 6_932);
    assert_eq!((a.allowance, a.time_term, a.need_term), (6_932, Some(720), None));
    let fast = output_allowance(TurnKind::Act, Some(40.0), Some(need), 6_932);
    assert_eq!((fast.allowance, fast.time_term), (6_932, Some(6_000)));
    let clipped = output_allowance(
        TurnKind::Pass,
        None,
        Some(OutputNeed { reasoning: 3_000, answer: 500, turns: 5 }),
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
    assert_eq!(crate::cognition::working_set::EmissionStop::classify(true, answer),
        crate::cognition::working_set::EmissionStop::CutMidAnswer);
}

// what this catches: the request carries the view's allowance VERBATIM — the
// prompt plan's flex grant — on an act and on a pass alike. An unmeasured mind on
// a roomy window gets what no other claim took, up to the runaway bound: the
// reserve that left 30k of an idle window unused while her reply was cut (the
// two-step yield) cannot come back, and the act bound still holds (losing it
// reproduces the 5,316-token act, 157 s on the lane).
#[tokio::test]
async fn an_unmeasured_turn_carries_its_grant_under_the_runaway_bound() {
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
    assert_eq!(
        view.completion_reserve,
        LlmDeliberationFaculty::ACT_OUTPUT_CAP,
        "an idle window's slack is hers to write with, up to the runaway bound"
    );
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

// what this catches: the 500 "Compute error" that muted Asha + Solenne every
// tick — the prompt was sized to leave a completion reserve, but generation was
// unbounded (`max_tokens: None`), so a verbose turn overran the reserve and
// `prompt + completion` reached `n_ctx` (this llama-server runs `--embeddings`,
// so context-shift is off → overrun = 500, not a clean stop). The fix bounds
// generation to `completion_budget()` — the SAME slice `prompt_view` carves out.
// This asserts the closed invariant: worst-case prompt (at its budget ceiling)
// PLUS the generation cap never exceeds the served window. Regression for the
// abstain-every-tick reliability bug.
#[test]
fn prompt_plus_completion_cap_never_exceeds_the_served_window() {
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

    // Drive the prompt to its budget ceiling (oversized burst + context bids).
    let mut ws = history_with_stimulus("LATEST: did the deploy fix land?");
    ws.broadcast.push(Contribution::context(
        FacultyId::Recall,
        &"deploy pipeline observation; ".repeat(2000),
        0.9,
        "recalled",
    ));

    let view = faculty.prompt_view(&ws);
    // Same atomic snapshot the production `contribute` path uses — the model
    // binding carries the served window the request is bounded to.
    let binding = faculty.binding.load_full();
    let request = faculty.build_request_within(
        &binding,
        view.completion_reserve,
        view.messages.clone(),
        None,
        view.system.clone(),
        None,
        Some(ws.room_id),
        TurnKind::Pass,
    );
    // Generation is bounded — never the unbounded `None` that overran n_ctx.
    let cap = request
        .max_tokens
        .expect("deliberation must bound generation to the reserved room");
    assert_eq!(
        cap, view.completion_reserve,
        "the generation cap IS the reserved room — one source of truth"
    );
    // The closed invariant: prompt-at-ceiling + the generation cap fits n_ctx.
    let prompt_tokens = est_tokens(&view.system) + est_tokens(&view.user_text());
    assert!(
        prompt_tokens + cap as usize <= window as usize,
        "prompt ({prompt_tokens}) + completion cap ({cap}) must fit the served \
     window ({window}) — else generation reaches n_ctx and llama-server 500s"
    );
}

// what this catches: the cold-start reply sized to the WINDOW instead of to the
// REPLY. Regression for 2026-09-18 (Cormac, `delib.context.render`): one citizen,
// one question, 28 minutes apart — at a 16,384 window with a MEASURED reserve of
// 2,817 she named the file she was working in; at a 32,768 window with the
// `window/2` PRIOR of 16,384 she rendered 297 tokens of context and answered "As an
// AI assistant, I am currently not actively engaged in any specific project". A
// bigger window gave her LESS context. The same fraction at the 101,120 lane
// licensed a 100k think-only turn that wrote nothing — one defect, both ends. Now
// the reply is one claimant in the window's allocation: when the prompt's claims
// want the whole window, an unmeasured reply gets what a reply costs and the world
// gets the rest, on every lane.
#[test]
fn an_unmeasured_mind_reserves_a_reply_not_a_fraction_of_her_window() {
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    // Deliberately a SPREAD, not our fleet's windows: an unmeasured reply costs what
    // a reply costs on ANY lane — a phone-sized 4k, a laptop 32k, a 5090's 101k, a
    // 1M frontier seat.
    let prior = LlmDeliberationFaculty::COMPLETION_COLD_PRIOR_TOKENS;
    for window in [
        crate::inference::planning::MIN_SERVE_CTX,
        4_096,
        8_192,
        16_384,
        32_768,
        65_536,
        101_120,
        262_144,
        1_048_576,
    ] {
        let faculty =
            LlmDeliberationFaculty::new(Uuid::new_v4(), "Iris", "You are Iris.", Arc::clone(&adapter))
                .with_context_window(window);
        // Grounding that wants more than the whole window, in pieces that each
        // fit: the reply competes with the world for the room.
        let mut ws = Workspace::new("who are you, and what are you working on?");
        for piece in 0..32 {
            ws.broadcast.push(Contribution::context(
                FacultyId::Recall,
                format!("a remembered detail {piece} of the project; ").repeat(window as usize / 256),
                0.9,
                "recalled",
            ));
        }
        let view = faculty.prompt_view_within(&ws, window);
        let reply = view.completion_reserve;

        // (1) THE LAW, on every lane: an unmeasured reply never costs more than a
        //     reply when the world wants the room.
        assert!(reply <= prior, "window {window} granted {reply}, above the reply-sized prior {prior}");

        // (2) Above the degenerate low end it is FLAT — the same reply on a 32k
        //     laptop lane and a 1M frontier seat.
        if window >= prior * 4 {
            assert_eq!(reply, prior, "the reply tracked the window at {window}");
            // (3) The consequence that actually broke a citizen: prompt room. As a
            //     RATIO, so it holds on every window size.
            let prompt_room = window.saturating_sub(reply);
            assert!(
                prompt_room * 4 >= window * 3,
                "window {window} left only {prompt_room} tokens of world — this is \
                 the 297-token prompt that made a citizen answer as a stranger"
            );
            assert!(view.system.contains("a remembered detail"), "and the world got that room");
        }
    }
}

// what this catches: the measured reply (the "output-p95 riding the working-set
// registry" endgame). The cold-start `window/2` share reserved 14,720 of a 29,440
// window for replies measured at 0.2–2.5k, squeezing grounding to 195 tokens and
// dropping the room board — the 2026-08-31 meta-loop spiral. Pins: (a) fewer than
// 3 observations keeps the prior (one tiny ack must not size every later turn);
// (b) measured = her p90 think + answer with headroom, ABOVE the reply-sized
// prior when she writes more; (c) a tiny talker still claims the reply floor;
// (d) a turn cut mid-answer grows the claim back within a turn; (e) a retained cut
// (cf9e: 3,587) keeps its doubled claim, 7,174, through intermediate reads, inside
// the physical window, and releases it at settlement.
#[test]
fn measured_emissions_size_the_reserve_and_capped_turns_grow_it_back() {
    use crate::cognition::working_set::EmissionStop;
    let persona = Uuid::new_v4();
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let window: u32 = 29_440;
    let reg = crate::cognition::working_set::WorkingSetRegistry::new();
    let faculty = LlmDeliberationFaculty::new(
        persona,
        "Ivar",
        "You are Ivar, a thoughtful engineer on the grid.",
        adapter,
    )
    .with_context_window(window)
    .with_working_set(reg.clone());
    let wanted = |who: Uuid, pending: u32| {
        reply_claim(TurnKind::Pass, reg.need_of(who), pending, false, false).0.max_tokens
    };

    let cold = wanted(persona, 0);
    assert_eq!(cold, LlmDeliberationFaculty::COMPLETION_COLD_PRIOR_TOKENS);
    // (a) below the observation bar the prior stands
    reg.record_emission_in_memory(persona, 2_500, 0, EmissionStop::Landed, 1);
    reg.record_emission_in_memory(persona, 1_200, 0, EmissionStop::Landed, 2);
    assert_eq!(wanted(persona, 0), cold);
    // (b) measured: p90 2,500 × 5/4 = 3,125 — measurement GROWS a reply-sized
    // prior rather than shaving a half-window default down.
    reg.record_emission_in_memory(persona, 2_500, 0, EmissionStop::Landed, 3);
    let measured = wanted(persona, 0);
    assert_eq!(measured, 3_125);
    assert!(measured > cold);
    // (c) a 40-token ack claims the reply floor, never less
    let quiet = Uuid::new_v4();
    for t in 1..=3 {
        reg.record_emission_in_memory(quiet, 40, 0, EmissionStop::Landed, t);
    }
    assert_eq!(wanted(quiet, 0), LlmDeliberationFaculty::COMPLETION_FLOOR_TOKENS);
    // (d) a turn cut mid-answer at 5,000 records its answer at 10,000: the p90 is
    // now 12,500 — a citizen who needs the room earns it back within a turn.
    reg.record_emission_in_memory(persona, 5_000, 0, EmissionStop::CutMidAnswer, 4);
    assert_eq!(wanted(persona, 0), 12_500);
    let view = faculty.prompt_view_within(&Workspace::new("continue"), window);
    assert!(view.completion_reserve >= 12_500, "the grant holds the grown claim: {}", view.completion_reserve);

    // (e) cf9e: a 2,276-token cut followed by short successful read replies.
    // The existing channel split measured peak 3,587; the concern keeps that
    // evidence while the ordinary persona statistic still adapts.
    let wm = Arc::new(crate::cognition::working_memory::WorkingMemory::new(8));
    let quiet_faculty = LlmDeliberationFaculty::new(
        quiet,
        "Quiet",
        "You are Quiet.",
        Arc::new(HeuristicInferenceAdapter::new()),
    )
    .with_context_window(window)
    .with_working_set(reg.clone())
    .with_working_memory(Arc::clone(&wm));
    let mut concern = Workspace::new("continue the current implementation");
    let room = Uuid::new_v4();
    concern.activity_room_id = room;
    wm.set_scope(Some("owned-workspace".into()));
    let cut = reg.record_emission_in_memory(quiet, 2276, 965, EmissionStop::CutMidAnswer, 10);
    assert_eq!(cut.peak_tokens, 3587);
    wm.retain_truncated_emission(room, cut.peak_tokens);
    for (tick, tokens) in [871, 1120, 400, 300, 300, 300, 300, 300, 300].into_iter().enumerate() {
        reg.record_emission_in_memory(quiet, tokens, 0, EmissionStop::Landed, 11 + tick as u64);
        assert_eq!(wanted(quiet, wm.pending_emission_peak(room)), 7174,
            "the claim retains the measured cut through intermediate reads");
        assert!(quiet_faculty.prompt_view_within(&concern, window).completion_reserve >= 7174,
            "actual prompt construction grants it");
    }
    assert!(wanted(quiet, 0) < 7174, "ordinary measurements still decay");
    let small = quiet_faculty.prompt_view_within(&concern, 4096);
    assert!(small.capacity_error.is_none(), "{:?}", small.capacity_error);
    assert!(
        small.estimated_prompt_tokens + small.completion_reserve as usize <= 4096,
        "retained need never bypasses the physical window: {} + {}",
        small.estimated_prompt_tokens,
        small.completion_reserve
    );
    wm.record_settlement("answered this concern", Some(room));
    assert_eq!(wm.pending_emission_peak(room), 0);
    assert_eq!(wanted(quiet, wm.pending_emission_peak(room)), wanted(quiet, 0));
}

// Capture 932f6b81:37 ended at 16,384 tokens with no answer or tool call. Its
// measured need was 41,053, but an obsolete absolute ceiling prevented the
// censored-emission feedback from growing the reply. what this catches: the
// claim freezing at the failed allowance — and, the other way, a recovered claim
// overrunning a smaller seat or taking its required input.
#[test]
fn measured_reserve_recovers_past_a_censored_large_reply() {
    let persona = Uuid::new_v4();
    let reg = crate::cognition::working_set::WorkingSetRegistry::new();
    for tick in 1..=3 {
        reg.record_emission_in_memory(persona, 16_384, 0, crate::cognition::working_set::EmissionStop::CutMidAnswer, tick);
    }
    // A censored sample is doubled by the registry; the need adds headroom.
    let need = reg.need_of(persona).expect("three measured turns");
    assert_eq!(need.total(), 40_960);
    for window in [8_192, 16_384, 32_768, 67_072, 131_072] {
        let faculty = LlmDeliberationFaculty::new(
            persona,
            "Kimi",
            "You are Kimi.",
            Arc::new(HeuristicInferenceAdapter::new()),
        )
        .with_context_window(window)
        .with_working_set(reg.clone());
        let view = faculty.prompt_view_within(&Workspace::new("finish the migration"), window);
        assert!(view.capacity_error.is_none(), "{window}: {:?}", view.capacity_error);
        assert!(
            view.estimated_prompt_tokens + view.completion_reserve as usize <= window as usize,
            "{window}: never permission to overrun a smaller seat"
        );
        if window >= 67_072 {
            assert!(view.completion_reserve >= need.total(), "{window}: recovered past the censored 16,384");
        }
    }
}

// what this catches: KV-prefix cache locality — session-stable standing framing
// (roster/doctrine/map) must serialize BEFORE volatile grounding (recall) even
// when the volatile bid scores HIGHER salience, so the stable sections sit in the
// cacheable prefix region and don't re-prefill every turn. Without the stable-tier
// sort, recall (0.9) would lead the block and push the roster (0.5) below it,
// breaking the cross-turn prefix at the very first section. Regression for the
// append-only/volatility-ordering speed work (commit 47c65891a + this one).
#[test]
fn stable_framing_serializes_before_higher_salience_volatile_grounding() {
    let persona = Uuid::new_v4();
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let faculty = LlmDeliberationFaculty::new(persona, "Ivar", "You are Ivar.", adapter);

    let mut ws = Workspace::new("teammate: what's the plan?");
    // Volatile recall bids HIGH salience — would lead a pure salience-desc block.
    ws.broadcast.push(Contribution::context(
        FacultyId::Recall,
        "VOLATILE_RECALL: deploy was red, fixed 4pm",
        0.9,
        "recalled",
    ));
    // Stable standing-framing bids LOWER salience but is session-stable.
    ws.broadcast.push(
        Contribution::context(
            FacultyId::Custom("room-roster".to_string()),
            "STABLE_ROSTER: alice, bob, carol present",
            0.5,
            "framing",
        )
        .session_stable(),
    );

    // Generous budget so BOTH fit — this isolates ORDER, not truncation.
    let block = faculty.render_assembled_context_within(
        &ws,
        4096,
        &GroundingPlan::default(),
        ContextComposition::default(),
    );
    let roster_at = block.find("[room-roster]").expect("roster present");
    let recall_at = block.find("[recall]").expect("recall present");
    assert!(
        roster_at < recall_at,
        "stable framing must serialize before higher-salience volatile recall \
     (roster@{roster_at} should precede recall@{recall_at})\n{block}"
    );
}

// what this catches: the OTHER half of the #266 cache defect — a fixed SET of
// stable contributions emitting DIFFERENT bytes because salience re-ranked them.
//
// Its sibling above pins the tier split (stable before volatile). That shipped,
// and reuse did not follow. The sort under it read
// `sort_by_key(|(c, _)| u8::from(!c.stable))` with a comment calling the
// preserved within-tier salience order harmless. It was the bug: salience is
// recomputed EVERY turn, so the tier whose only job is byte-identity re-shuffled.
// Measured live 2026-08-21 in Atlas's flask-4045 run — consecutive prompts shared
// ~82% of their prefix and diverged where stable blocks traded places, costing
// ~306s of re-prefill per act with `cachedTokens: 0` throughout.
//
// So the tier-split assertion alone could not catch it, and this one is what
// makes the fix hold: SAME SET IN, SAME BYTES OUT, whatever attention ranked.
// If someone reverts to a salience-ordered stable tier, this fails and the
// sibling above still passes.
#[test]
fn stable_tier_bytes_are_identical_however_salience_ranks_them() {
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let faculty = LlmDeliberationFaculty::new(
        Uuid::new_v4(),
        "Ivar",
        "You are Ivar.",
        Arc::clone(&adapter),
    );

    // Same three stable blocks, built with per-turn salience + push order.
    // These are the exact faculties that were caught trading places.
    let render = |sal: [f32; 3], order: [usize; 3]| {
        let bodies = [
            ("room-roster", "STABLE_ROSTER: alice, bob, carol present"),
            ("workspace-map", "STABLE_MAP: src/ tests/ docs/"),
            ("room-kanban", "STABLE_BOARD: you hold 9 cards here"),
        ];
        let mut ws = Workspace::new("teammate: what's the plan?");
        for &i in order.iter() {
            ws.broadcast.push(
                Contribution::context(
                    FacultyId::Custom(bodies[i].0.to_string()),
                    bodies[i].1,
                    sal[i],
                    "framing",
                )
                .session_stable(),
            );
        }
        // Generous budget so all three fit — this isolates ORDER, never truncation.
        faculty.render_assembled_context_within(
            &ws,
            4096,
            &GroundingPlan::default(),
            ContextComposition::default(),
        )
    };

    let turn0 = render([0.90, 0.50, 0.30], [0, 1, 2]);
    // Turn 1: attention completely re-ranks them, and they arrive in a new order.
    let turn1 = render([0.30, 0.90, 0.50], [2, 0, 1]);
    // Turn 2: a third ranking, reversed arrival.
    let turn2 = render([0.50, 0.30, 0.90], [2, 1, 0]);

    assert_eq!(
        turn0, turn1,
        "a re-ranked stable tier must emit IDENTICAL bytes — this is the whole \
         cacheable prefix (#266)\nturn0:\n{turn0}\nturn1:\n{turn1}"
    );
    assert_eq!(
        turn0, turn2,
        "third ranking must also be byte-identical\nturn0:\n{turn0}\nturn2:\n{turn2}"
    );

    // And it is CANONICAL, not merely "whatever arrived first" — an insertion-order
    // block would satisfy the equalities above only by accident of this test's
    // orders. Pin the actual rule so the guarantee is legible.
    let (kanban, map, roster) = (
        turn0.find("[room-kanban]").expect("kanban present"),
        turn0.find("[workspace-map]").expect("map present"),
        turn0.find("[room-roster]").expect("roster present"),
    );
    assert!(
        roster < map && map < kanban,
        "stable tier orders canonically by CHURN CLASS then name \
         (`deliberation_prompt::stable_prefix_order`): room-roster and \
         workspace-map are STANDING ground, room-kanban is the BOARD and comes \
         after both\n{turn0}"
    );

    // The volatile tier keeps salience order ON PURPOSE (#205: most salient
    // nearest the write point). It re-prefills by construction, so canonical
    // ordering would buy no reuse and would cost that placement. Guard the
    // asymmetry, so a future "make it all deterministic" pass has to read this.
    //
    // NOT `active-work`: it is pinned FIRST by an explicit rule ABOVE salience
    // (the ctx_floor reservation only holds if its claimant also leads the
    // greedy walk). Probing the asymmetry with the one salience-immune faculty
    // reports "salience does nothing" about a tier where it does plenty —
    // which is exactly what the first draft of this assertion did.
    let volatile = |sal: (f32, f32)| {
        let mut ws = Workspace::new("teammate: what's the plan?");
        ws.broadcast.push(Contribution::context(
            FacultyId::Recall,
            "VOLATILE_RECALL: deploy was red, fixed 4pm",
            sal.0,
            "recalled",
        ));
        ws.broadcast.push(Contribution::context(
            FacultyId::Custom("affect".to_string()),
            "VOLATILE_AFFECT: mild time pressure",
            sal.1,
            "felt",
        ));
        faculty.render_assembled_context_within(
            &ws,
            4096,
            &GroundingPlan::default(),
            ContextComposition::default(),
        )
    };
    let recall_leads = volatile((0.9, 0.2));
    let work_leads = volatile((0.2, 0.9));
    assert_ne!(
        recall_leads, work_leads,
        "the VOLATILE tier must still follow salience (#205) — if this ever \
         matches, canonical ordering leaked into the tier that must not have it"
    );
}

/// A grounding block built from a LIST of units, sized like the live work
/// board (leads first, then one line per card).
fn board_like(units: usize) -> Contribution {
    let mut parts = vec![
        "[your work] you HOLD 1 card — 0dd1123c \"wire the gather fallback\"".to_string(),
        "[available work] 60 card(s) are claimable — pick one up with work/claim"
            .to_string(),
    ];
    for i in 0..units {
        parts.push(format!(
            "card {i:08x} [Open] \"a card whose title is about as long as a real one\" \
             (Normal, unclaimed)"
        ));
    }
    let content = parts.join("\n");
    Contribution::context(
        FacultyId::Custom("room-kanban".to_string()),
        content,
        0.9,
        "board",
    )
    .with_parts(parts)
}

/// Card dec1a7ff: #3829 made the tool surface a per-turn decision but left the
/// ACCOUNTING pricing the full registry, so a work turn's message budget was
/// short by (full − hands) and trimmed conversation to fit room that was never
/// occupied. These pin what is COUNTED against what is SENT.
mod the_budget_prices_the_surface_it_actually_sends {
    use super::*;

    /// The smallest 1 KiB-aligned window whose tool-surface share ADMITS the
    /// whole registry — and the largest that does not.
    ///
    /// ASKED of [`ContextBudget`], never re-derived from its denominator. Two
    /// literals stood here (50,944 and 24,832, read off that module's
    /// calibration table) and the crate's own de-hardcode guard caught them in
    /// CI, correctly: a window constant that does not scale with the served
    /// window is exactly what that guard exists to stop, and a test fixture is
    /// not an exemption from it. Spelling `/ 4` here instead would have
    /// duplicated `TOOL_SURFACE_DENOM`, which is the same defect wearing a
    /// different hat — so this searches with the real predicate and stays
    /// correct if the share is ever re-cut.
    fn window_admitting(full_surface_tokens: usize) -> u32 {
        (1..=512)
            .map(|k| k * 1024)
            .find(|w| {
                crate::cognition::context_budget::ContextBudget::from_window(*w)
                    .tool_surface_tokens()
                    >= full_surface_tokens
            })
            .expect("some served window admits the full surface")
    }

    /// `(admits_the_full_surface, does_not)` — both derived from the registry's
    /// measured cost, so neither can drift away from the bound it is meant to
    /// straddle. The narrow one is one alignment step below the smallest
    /// admitting window, i.e. the largest that still refuses.
    fn calibrated_windows() -> (u32, u32) {
        let full = faculty_with_the_real_registry(1024).tool_surface_tokens;
        let admits = window_admitting(full);
        (admits, admits - 1024)
    }

    fn faculty_with_the_real_registry(window: u32) -> LlmDeliberationFaculty {
        LlmDeliberationFaculty::new(
            Uuid::new_v4(),
            "Asha",
            "You are Asha.",
            Arc::new(HeuristicInferenceAdapter::new()) as Arc<dyn AIProviderAdapter>,
        )
        .with_context_window(window)
        .with_tools(persona_tools::native_tool_specs())
    }

    fn work_turn() -> Workspace {
        let mut ws = Workspace::new("fix the failing test");
        ws.workspace_deliverable = true;
        ws
    }

    // what this catches: the original defect. A work turn sends HANDS, so the
    // budget must subtract the hands' cost. Before this, `budget` subtracted
    // the whole registry — the citizen silently lost (full − hands) tokens of
    // conversation on every act turn, on a window that had room for it.
    // MUTATION-PROVED: point `SelectedSurface.tokens` at `tool_surface_tokens`
    // on the hands arms and this fails on the strict inequality.
    #[test]
    fn a_work_turn_is_budgeted_for_its_hands_not_for_the_registry_it_withheld() {
        let (fits, _narrow) = calibrated_windows();
        let faculty = faculty_with_the_real_registry(fits);
        let selected = faculty.select_tool_surface(&work_turn(), fits);

        assert_eq!(
            selected.reason,
            SurfaceReason::HandsForWork,
            "a deliverable turn takes hands for FOCUS — and on this window the \
             full surface would have fit, so nothing but focus can explain it"
        );
        assert_eq!(
            selected.tokens, faculty.hands_surface_tokens,
            "the budget must be charged for what goes on the wire"
        );
        assert!(
            selected.tokens < faculty.tool_surface_tokens,
            "the saving must be REAL: hands ({}) must cost less than the registry \
             ({}), otherwise this test would pass while pricing nothing",
            selected.tokens,
            faculty.tool_surface_tokens
        );
    }

    // what this catches: silently re-deriving a cap from a send. The demand
    // probe's contract is that every term is UNTRUNCATED, so when the WINDOW is
    // what withheld the surface, demand must still price the full registry —
    // report the hands here and the pressure the probe exists to measure
    // disappears exactly when it starts mattering.
    // MUTATION-PROVED: make `demand_tokens` return `self.tokens` unconditionally
    // and this fails.
    #[test]
    fn a_budget_withheld_surface_still_reports_the_full_registry_as_demand() {
        let (_fits, narrow) = calibrated_windows();
        let faculty = faculty_with_the_real_registry(narrow);
        // A MESSAGE turn — so the narrowing can only be the window.
        let ws = Workspace::new("anything open?");
        let selected = faculty.select_tool_surface(&ws, narrow);

        assert_eq!(
            selected.reason,
            SurfaceReason::HandsForBudget,
            "this window's share cannot hold the registry, and it is not a work turn"
        );
        assert_eq!(
            selected.tokens, faculty.hands_surface_tokens,
            "she is SENT her hands"
        );
        assert_eq!(
            selected.demand_tokens(faculty.describe_tool_tokens()),
            faculty.tool_surface_tokens,
            "…but she WANTED the registry and the window refused it — demand is \
             the untruncated number or it is worthless for provisioning"
        );
    }

    // what this catches: the mirror error — inflating `over_window` on a turn
    // that never wanted the full surface. A work turn narrows for FOCUS, so
    // nothing was truncated and demand must not pretend otherwise; otherwise
    // every act turn reports as near-overflow and the number stops meaning
    // anything.
    // MUTATION-PROVED: make `demand_tokens` return the full cost unconditionally
    // and this fails.
    #[test]
    fn a_focus_narrowed_surface_does_not_inflate_demand_with_what_it_never_wanted() {
        let (fits, _narrow) = calibrated_windows();
        let faculty = faculty_with_the_real_registry(fits);
        let selected = faculty.select_tool_surface(&work_turn(), fits);

        assert_eq!(
            selected.demand_tokens(faculty.describe_tool_tokens()),
            faculty.hands_surface_tokens,
            "focus is not truncation — the registry was never wanted on this turn"
        );
    }

    // what this catches: the whole defect class, stated as the invariant rather
    // than case by case. The reported cost must be the cost OF the reported
    // surface — which is exactly what `delib.tool_surface.withheld` violated by
    // printing a count of her hands beside the registry's price.
    // MUTATION-PROVED: swap either arm's `tokens` for the other memo and this
    // fails on that arm.
    #[test]
    fn the_cost_reported_is_always_the_cost_of_the_surface_reported() {
        let (fits, narrow) = calibrated_windows();
        for (label, window, ws) in [
            ("full", fits, Workspace::new("anything open?")),
            ("hands/work", fits, work_turn()),
            ("hands/budget", narrow, Workspace::new("anything open?")),
        ] {
            let faculty = faculty_with_the_real_registry(window);
            let selected = faculty.select_tool_surface(&ws, window);
            let specs = selected
                .specs
                .as_deref()
                .expect("a tool surface was offered");
            assert_eq!(
                selected.tokens,
                LlmDeliberationFaculty::tool_surface_tokens_of(specs),
                "[{label}] the price must be the price of THESE specs, not of a \
                 surface that stayed home"
            );
        }
    }

    // what this catches: a "simplification" that points the structurally-mute
    // sensor at the selected surface. `min_window_for_agentic_surface` asks a
    // DIFFERENT question — "how large a window hosts the WHOLE agentic surface"
    // — so it legitimately keeps the full cost, and narrowing it would quietly
    // lower the bar below which we warn that a citizen cannot act.
    // MUTATION-PROVED: swap `describe_tool_tokens()` for the hands memo in
    // `min_window_for_agentic_surface` and this fails.
    #[test]
    fn the_structurally_mute_sensor_still_asks_for_the_whole_surface() {
        let (_fits, narrow) = calibrated_windows();
        let faculty = faculty_with_the_real_registry(narrow);
        let needed = faculty.min_window_for_agentic_surface();

        // The discriminator has to be the bound a HANDS-based sensor would
        // produce, not a constant. The first version of this test asserted
        // `needed >= tool_surface_tokens` and the mutation it was written for
        // did not move it: hands + framing already clears the registry's raw
        // token count, so the assertion held while the sensor measured the
        // wrong surface. A bound satisfied by accident pins nothing.
        let hands_based = faculty.hands_surface_tokens as u32
            + faculty.framing_floor_tokens()
            + LlmDeliberationFaculty::COMPLETION_FLOOR_TOKENS;

        assert!(
            needed > hands_based,
            "the bound must be STRICTLY more demanding than a hands-only one \
             ({needed} vs {hands_based}): it answers 'can this window host the \
             agentic surface at all', never 'what did we send this turn'. Equal \
             means the sensor was narrowed to the selected surface and the \
             'she is structurally mute below here' warning now fires too late."
        );
    }
}

// what this catches: a greedy conversation that eats every token grounding
// needs. `messages_within` fills whatever it is handed, so passing it the
// whole post-framing pool leaves grounding exactly zero — and GROWING THE
// WINDOW DOES NOT FIX IT, the conversation just absorbs the increase.
// Measured live 2026-08-06: the served window rose 16,384 → 24,128 and
// Anwen, Asha and Atlas each still rendered `budget=0 kept=[]`, dropping
// recall, roster, workspace-map AND the work board on every single turn.
// Supply was never the binding constraint at this seam; ORDER was.
#[test]
fn a_long_conversation_cannot_starve_grounding_to_zero() {
    let persona = Uuid::new_v4();
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let faculty = LlmDeliberationFaculty::new(persona, "Ivar", "You are Ivar.", adapter)
        .with_context_window(24_128);

    // A conversation long enough to swallow the whole window on its own.
    let mut ws = Workspace::new("anything open?");
    for i in 0..40 {
        ws.turns
            .push(crate::cognition::workspace::BurstTurn::attributed(
                i % 2 == 1,
                if i % 2 == 1 { "Ivar" } else { "Asha" },
                format!("turn {i}: ").repeat(60),
                None,
            ));
    }
    ws.broadcast
        .push(board_like(60).with_expand_command(Some("work/list")));

    let view = faculty.prompt_view_within(&ws, 24_128);
    assert!(
        view.system.contains("[room-kanban]"),
        "grounding must survive a conversation that would otherwise take every \
         token — this is the exact live failure: window grew, board still gone\n{}",
        &view.system[..view.system.len().min(1200)]
    );
}

// what this catches: a truncation notice a citizen cannot act on. Telling
// her "the full list is available from the matching command" names nothing
// she can type — she cannot run a description, and a verb she has to guess
// is a verb she gets wrong ([[command-names-must-be-accurate]]). The source
// declares its own expansion verb; the notice must print it verbatim, with
// the true total so she knows the size of what she is asking for.
#[test]
fn a_truncated_block_names_the_exact_verb_that_yields_the_rest() {
    let persona = Uuid::new_v4();
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let faculty = LlmDeliberationFaculty::new(persona, "Ivar", "You are Ivar.", adapter);
    let mut ws = Workspace::new("anything open?");
    let board = board_like(60).with_expand_command(Some("work/list"));
    let total = board.parts.len();
    ws.broadcast.push(board);

    let block = faculty.render_assembled_context_within(
        &ws,
        120,
        &GroundingPlan::default(),
        ContextComposition::default(),
    );
    assert!(
        block.contains("run `work/list`"),
        "the notice must name the verb verbatim, not describe it\n{block}"
    );
    assert!(
        block.contains(&format!("to see all {total}")),
        "…and say how many there are in total, so she knows what she is asking for\n{block}"
    );
}

// what this catches (card e049aa82): a cut the SOURCE made, invisible to her.
// A source that stops short of its own list at its budget mints a
// continuation, and nothing resumes it in production; until now the packer's
// notice counted only its own cut, so a block whose delivered parts all fit
// carried no notice at all, and one it cut further claimed a false total.
// Both must say more exists and name the verb, with no invented total.
#[test]
fn a_block_its_source_cut_short_says_so_even_when_every_part_fits() {
    let persona = Uuid::new_v4();
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let faculty = LlmDeliberationFaculty::new(persona, "Ivar", "You are Ivar.", adapter);
    let render = |board: Contribution, budget: usize| {
        let mut ws = Workspace::new("anything open?");
        ws.broadcast.push(board);
        faculty.render_assembled_context_within(&ws, budget, &GroundingPlan::default(), ContextComposition::default())
    };

    let fits = render(board_like(2).with_expand_command(Some("work/list")).with_source_truncated(true), 4_000);
    assert!(fits.contains("…more not shown (context budget) — run `work/list` to see all"), "a whole block the source cut must still say so\n{fits}");
    let cut = render(board_like(60).with_expand_command(Some("work/list")).with_source_truncated(true), 120);
    assert!(cut.contains("+ more not shown (context budget)"), "a further cut must count what it left out PLUS the source's remainder\n{cut}");
    assert!(!cut.contains("to see all 62"), "the delivered parts are not the source's total\n{cut}");
    let empty = render(
        Contribution::context(FacultyId::Custom("room-board".to_string()), String::new(), 0.5, "nothing fit".to_string())
            .with_expand_command(Some("work/list"))
            .with_source_truncated(true),
        4_000,
    );
    assert!(empty.contains("…more not shown (context budget) — run `work/list` to see all"), "an empty partial delivery keeps its notice\n{empty}");
    let honest = render(board_like(2).with_expand_command(Some("work/list")), 4_000);
    assert!(!honest.contains("more not shown"), "a complete block carries no notice\n{honest}");
}

// what this catches: the other half — a source with genuinely nothing more
// to fetch must not invent a verb. A pointer to a command that does not
// expand anything is worse than no pointer: she spends a turn on it and
// learns nothing.
#[test]
fn a_source_with_no_expansion_verb_states_only_the_omission() {
    let persona = Uuid::new_v4();
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let faculty = LlmDeliberationFaculty::new(persona, "Ivar", "You are Ivar.", adapter);
    let mut ws = Workspace::new("anything open?");
    ws.broadcast.push(board_like(60)); // expand_command defaults to None

    let block = faculty.render_assembled_context_within(
        &ws,
        120,
        &GroundingPlan::default(),
        ContextComposition::default(),
    );
    assert!(
        block.contains("more not shown"),
        "still says it truncated\n{block}"
    );
    assert!(
        !block.contains("run `"),
        "must not point at a verb it was never given\n{block}"
    );
}

// what this catches: the board vanishing WHOLE. Measured live 2026-08-06 across
// 1,284 context assemblies, `room-kanban` was KEPT 0 times and dropped 495 — a
// median 5,364-token block offered all-or-nothing into a median 55-token budget
// — while its first two units (~200 tokens) carry every fact a citizen needs to
// find work. The citizens were reporting "there are no open tasks available"
// about a 61-card board. A source that delivered a LIST declared its own cut
// points; assembly must take the longest fitting PREFIX instead of nothing.
#[test]
fn a_divisible_grounding_block_contributes_its_leads_when_the_whole_will_not_fit() {
    let persona = Uuid::new_v4();
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let faculty = LlmDeliberationFaculty::new(persona, "Ivar", "You are Ivar.", adapter);
    let mut ws = Workspace::new("anything open?");
    let board = board_like(60);
    let whole = est_tokens(&board.content);
    ws.broadcast.push(board);

    // A budget FAR under the whole board — the live shape (55 vs 5,364).
    let budget = 120;
    assert!(whole > budget * 10, "fixture must reproduce the live ratio");
    let block = faculty.render_assembled_context_within(
        &ws,
        budget,
        &GroundingPlan::default(),
        ContextComposition::default(),
    );

    assert!(
        block.contains("[room-kanban]"),
        "the board must still reach the prompt — it vanished whole before this fix\n{block}"
    );
    assert!(
        block.contains("[your work]"),
        "the FIRST unit is the one that matters most; a prefix must start there\n{block}"
    );
    assert!(
        !block.contains("card 0000003b"),
        "the 60th card must NOT ride along — the prefix is bounded by budget\n{block}"
    );
    assert!(
        block.contains("more not shown"),
        "a truncated LIST must SAY it is truncated, or a partial board reads as \
         the whole board and she reports work that isn't there\n{block}"
    );
    assert!(
        est_tokens(&block) <= budget,
        "the prefix must respect the budget it was given (got {} for budget {budget})\n{block}",
        est_tokens(&block)
    );
}

// what this catches: the SALIENCE INVERSION that `delib.context.salience_inversion`
// reports — an INDIVISIBLE top-salience block vanishing whole while a cheap
// low-salience one rides along. Measured live 2026-08-20 across 25 consecutive
// turns: `workspace-map` (sal=0.90) dropped on 20 of them, grounding at 62-183
// tokens, and Atlas produced 0 acts for 3 days against a checkout she could not
// see. The divisible case is already covered above (a board contributes its
// leads); this is its opposite number — nothing to cut, so it goes to zero.
//
// It also pins the PREMISE THE SENSOR RESTS ON: selection is
// highest-salience-FIRST, so ranking alone can never produce this. If that
// ordering is ever lost, the inversion stops meaning "the budget could not
// afford what mattered" and the probe becomes noise — silently. This test is
// what makes that regression loud.
#[test]
fn an_indivisible_top_salience_block_can_lose_its_slot_to_a_cheaper_lesser_one() {
    let persona = Uuid::new_v4();
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let faculty = LlmDeliberationFaculty::new(persona, "Ivar", "You are Ivar.", adapter);
    let mut ws = Workspace::new("what should I work on?");

    // The repo map: essential, expensive, and INDIVISIBLE — no `with_parts`,
    // so there is no prefix to fall back to. This is the live shape.
    let map_body = (0..120)
        .map(|i| format!("src/module_{i:03}/mod.rs"))
        .collect::<Vec<_>>()
        .join("\n");
    ws.broadcast.push(Contribution::context(
        FacultyId::Custom("workspace-map".to_string()),
        map_body.clone(),
        0.90,
        "map",
    ));
    // A cheap block that comfortably fits — AT THE SAME SALIENCE. This is the
    // LIVE shape, not a constructed one: on the first turns after the
    // 2281e8b81 deploy every essential source bid exactly 0.90 (Atlas kept
    // `roster(0.90), room-kanban(0.90)` and dropped `workspace-map(0.90)`;
    // Benchy kept `room-board(0.90)` and dropped two others at 0.90). There is
    // no 0.30 to lose to. The first cut of this test used one, which is why it
    // passed against a sensor that could not fire in production.
    ws.broadcast.push(Contribution::context(
        FacultyId::Custom("recall".to_string()),
        "you once renamed a file".to_string(),
        0.90,
        "recall",
    ));

    // Budget affords the small one and nowhere near the map — the live ratio.
    let budget = 40;
    assert!(
        est_tokens(&map_body) > budget * 5,
        "fixture must reproduce the live ratio (map {} tok vs budget {budget})",
        est_tokens(&map_body)
    );
    let block = faculty.render_assembled_context_within(
        &ws,
        budget,
        &GroundingPlan::default(),
        ContextComposition::default(),
    );

    assert!(
        !block.contains("[workspace-map]"),
        "fixture is not reproducing the inversion — the map was supposed to be \
         unaffordable at this budget\n{block}"
    );
    assert!(
        block.contains("[recall]"),
        "the cheap low-salience block IS what rides along; without it there is no \
         inversion, just an empty budget\n{block}"
    );
}

// what this catches: the OTHER half of the same rule — divisibility is opt-in and
// must not leak. An engram is ONE indivisible thing; cutting it mid-sentence
// produces confident nonsense, which is worse than its absence. A faculty that
// never calls `with_parts` must behave exactly as it did before parts existed.
#[test]
fn an_indivisible_contribution_is_still_dropped_whole_never_cut_mid_content() {
    let persona = Uuid::new_v4();
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let faculty = LlmDeliberationFaculty::new(persona, "Ivar", "You are Ivar.", adapter);
    let mut ws = Workspace::new("what happened yesterday?");
    let engram = "we agreed the gather promise binds every reachable kernel, and that \
                  the in-place consumption path inherits none of the alignment the \
                  copy path establishes, which is why the fallback had to be per-op"
        .repeat(4);
    ws.broadcast.push(Contribution::context(
        FacultyId::Recall,
        engram.clone(),
        0.9,
        "recalled",
    ));

    let block = faculty.render_assembled_context_within(
        &ws,
        40,
        &GroundingPlan::default(),
        ContextComposition::default(),
    );
    assert!(
        !block.contains("[recall]"),
        "an over-budget INDIVISIBLE contribution must be dropped whole, not \
         truncated into a confident half-thought\n{block}"
    );
    assert!(
        !block.contains("more not shown"),
        "the truncation notice belongs only to genuinely divisible lists\n{block}"
    );
}

// what this catches: a zero/near-zero budget must produce NO block rather than a
// bare `[room-kanban]` header with nothing under it — an empty labelled block
// reads to the persona as "the board is empty", which is exactly the false fact
// this whole fix exists to stop her acting on.
#[test]
fn a_budget_too_small_for_even_one_unit_emits_no_header_at_all() {
    let persona = Uuid::new_v4();
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let faculty = LlmDeliberationFaculty::new(persona, "Ivar", "You are Ivar.", adapter);
    let mut ws = Workspace::new("anything open?");
    ws.broadcast.push(board_like(60));

    let block = faculty.render_assembled_context_within(
        &ws,
        4,
        &GroundingPlan::default(),
        ContextComposition::default(),
    );
    assert!(
        !block.contains("[room-kanban]"),
        "no unit fits, so there must be no header — a labelled empty block is a \
         LIE that reads as an empty board\n{block}"
    );
}

// what this catches: #205 re-prefill. A `trailing` contribution (working-memory
// proprioception that grows each act) must render as a trailing conversation
// turn nearest generation, NEVER in the system message — so growing it
// act-over-act leaves the cacheable system prefix BYTE-IDENTICAL and only the
// appended tail re-prefills. Before this the working-memory block lived in the
// system message's volatile tail, so each act shifted every conversation token
// after it (~4000 tokens / ~30s of pure re-prefill — the eval-lane crawl). The
// stable framing stays in the system message; the trailing proprioception does
// not. regression for #205
#[test]
fn trailing_proprioception_renders_in_the_tail_not_the_system_prefix() {
    use crate::ai::types::MessageContent;
    let persona = Uuid::new_v4();
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let faculty = LlmDeliberationFaculty::new(persona, "Ivar", "You are Ivar.", adapter)
        // Ample window so budget pressure never perturbs what renders — this
        // isolates POSITION (system vs trailing), not truncation.
        .with_context_window(8192);

    let mut ws = Workspace::new("build me a login form");
    // Standing framing belongs in the system message.
    ws.broadcast.push(
        Contribution::context(
            FacultyId::Custom("room-roster".to_string()),
            "ROSTER: alice, bob present",
            0.5,
            "framing",
        )
        .session_stable(),
    );
    // Act #1: one working-memory trace, marked TRAILING (as the working-memory
    // faculty now bids it).
    ws.broadcast.push(
        Contribution::context(
            FacultyId::Custom("working-memory".to_string()),
            "Your recent thinking (working memory):\n- I wrote login.html first",
            0.7,
            "wm",
        )
        .trailing(),
    );
    let v1 = faculty.prompt_view(&ws);

    // The framing IS in the system message; the trailing proprioception is NOT.
    assert!(
        v1.system.contains("ROSTER: alice, bob"),
        "stable framing must stay in the system prefix:\n{}",
        v1.system
    );
    assert!(
        !v1.system.contains("I wrote login.html first"),
        "trailing proprioception must NOT sit in the system prefix (#205):\n{}",
        v1.system
    );
    // …it renders as a trailing user turn.
    let in_tail = |v: &DeliberationPromptView, needle: &str| {
        v.messages
            .iter()
            .any(|m| matches!(&m.content, MessageContent::Text(t) if t.contains(needle)))
    };
    assert!(
        in_tail(&v1, "I wrote login.html first"),
        "trailing proprioception must render as a conversation turn"
    );

    // Act #2: working memory GROWS (a second trace appends). The faculty re-bids
    // the grown contribution; the system PREFIX must be byte-identical.
    ws.broadcast.pop();
    ws.broadcast.push(
        Contribution::context(
            FacultyId::Custom("working-memory".to_string()),
            "Your recent thinking (working memory):\n- I wrote login.html first\n- Then I read it back to verify",
            0.7,
            "wm",
        )
        .trailing(),
    );
    let v2 = faculty.prompt_view(&ws);

    assert_eq!(
        v1.system, v2.system,
        "growing working memory must NOT mutate the system prefix — that IS the #205 re-prefill"
    );
    assert!(
        in_tail(&v2, "Then I read it back to verify"),
        "the new act must append to the trailing turn"
    );
}

// what this catches: #266 KV-cache reuse at the CALLER. The per-turn presence
// framing (DIRECTED vs SILENCE) flips hard every turn; the raw prompt-captures
// caught it sitting in the system message at char ~7607, BEFORE the grounding
// context, so every flip shortened the reusable KV prefix to ~7.6k chars and
// re-prefilled the whole tail (0% KV reuse — the ~13min SWE solves). The fix moves
// the framing to render LAST in the system message, after the context, so the
// reusable prefix now extends THROUGH the grounding and the flip falls past it.
// This asserts the property at the live boundary: the two systems (directed vs
// undirected) share a common prefix that reaches into the grounding context, and
// each carries its own presence variant only in the diverging tail. Before the
// reorder this was RED — the systems diverged at the framing, char ~7607, ahead of
// the context. regression for #266
#[test]
fn directedness_flip_keeps_the_grounding_inside_the_reusable_prefix() {
    let persona = Uuid::new_v4();
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let faculty = LlmDeliberationFaculty::new(persona, "Ivar", "You are Ivar.", adapter)
        // Ample window so budget pressure never perturbs what renders — this
        // isolates the framing POSITION, not truncation.
        .with_context_window(8192);

    // Same room content + the same standing grounding (a session-stable roster
    // contribution renders the [What you are working with] block); only the
    // directedness dimension flips.
    let with_grounding = |directed: bool| {
        let mut ws = Workspace::new("the team is chatting about the plan");
        ws.broadcast.push(
            Contribution::context(
                FacultyId::Custom("room-roster".to_string()),
                "ROSTER: alice, bob present",
                0.5,
                "framing",
            )
            .session_stable(),
        );
        ws.attention = TurnAttention::for_message(directed, false);
        ws
    };
    let undirected = faculty.prompt_view(&with_grounding(false));
    let directed = faculty.prompt_view(&with_grounding(true));

    // 2026-09-01, the full fix: the system message is now the STABLE half
    // only, so the directedness flip cannot touch it AT ALL — the two
    // system messages are byte-identical, and the entire system prefix
    // (identity + tools + grounding) is reusable KV across the flip.
    assert_eq!(
        directed.system, undirected.system,
        "the system message is the stable half — a presence flip must not move a byte of it"
    );
    assert!(
        directed.system.contains("[What you are working with"),
        "grounding context stays in the (fully reusable) system prefix"
    );
    // The presence variants ride the conversation tail (facts phase,
    // before the ask) — directed carries its marker, undirected does not.
    let tail_has = |v: &DeliberationPromptView, needle: &str| {
        v.messages.iter().any(
            |m| matches!(&m.content, crate::ai::types::MessageContent::Text(t) if t.contains(needle)),
        )
    };
    assert!(
        tail_has(&directed, "This message names you")
            && !tail_has(&undirected, "This message names you"),
        "the directed presence variant renders as a conversation turn, never in the system prefix"
    );
}

// what this catches: run-18057-f1 — the just-executed act's FULL result must reach
// the next prompt EVEN WHEN NOTHING BID IT through the arbiter. The 0-byte SWE-bench
// patch happened because the result rode the working-memory faculty's single
// 0.5-salience contribution, which `arbiter.focus()` truncated whole under capacity
// pressure — she then generated blind to her own grep output. The fix reads the
// result DIRECTLY from working memory in the message builder and pins it as a
// trailing turn (#392). This test simulates the exact failure: an EMPTY broadcast
// (every faculty bid evicted) with a live result in working memory — and asserts the
// result still lands in the tail. A regression means the result went back through
// the evictable path. regression for #392 / run-18057-f1
#[tokio::test]
async fn pinned_act_result_reaches_the_prompt_even_with_an_empty_broadcast() {
    use crate::ai::types::MessageContent;
    use crate::ai::types::{ContentPart, ToolCall, ToolResult};
    use crate::cognition::act_observe::{ActStatus, Observation, ToolOutput, ToolVerb};
    use crate::cognition::working_memory::WorkingMemory;

    let home = tempfile::tempdir().unwrap();
    let _native = crate::paths::NativeHomeOverride::install(home.path());
    crate::model_registry::init_global().unwrap();

    let persona = Uuid::new_v4();
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());

    // A live working memory carrying the result of a just-run grep — the sympy-18057
    // shape: `code/search` returned the `_sympify` location she must edit next. Large
    // enough to clear the trail-head threshold so the pinned block surfaces.
    let wm = Arc::new(WorkingMemory::new(8));
    wm.set_served_window(16_384);
    let needle = "sympy/core/expr.py:123:        return self == sympify(other)  # _sympify HERE";
    let grep_result = format!(
        "code/search matches:\n{needle}\n{}",
        "context line\n".repeat(200)
    );
    let mut png = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(2, 2)
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let image = crate::media::artifact::ImageArtifact::retain(
        &airc_blobs::FsStore::new(home.path().join("images")).unwrap(),
        png.get_ref(),
    )
    .unwrap();
    let mut other_png = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(3, 2)
        .write_to(&mut other_png, image::ImageFormat::Png).unwrap();
    let other_image = crate::media::artifact::ImageArtifact::retain(
        &airc_blobs::FsStore::new(home.path().join("images")).unwrap(),
        other_png.get_ref(),
    ).unwrap();
    let ws = Workspace::new("fix the __eq__ comparison in sympy Expr");
    wm.record_native_action_statement("", ws.activity_room_id, &[ToolCall { id: "capture".into(), name: "perception/observe".into(), input: json!({}) }]);
    wm.record_receipt_typed(
        &[Observation {
            call: ToolCall {
                id: "capture".into(),
                name: "perception/observe".into(),
                input: json!({}),
            },
            output: ToolOutput {
                image: Some(image.clone()),
                additional_images: Some(vec![other_image.clone()]),
                result: ToolResult {
                    tool_use_id: "capture".into(),
                    content: grep_result.clone(),
                    is_error: None,
                    custody: crate::ai::types::ResultCustody::Unsaved,
                },
                verb: ToolVerb::classify("perception/observe"),
                paths: Vec::new(),
                verdict: Default::default(),
            },
            status: ActStatus::Executed,
        }],
        &grep_result,
        Some(ws.activity_room_id),
        crate::ai::types::ResultCustody::Unsaved,
    );

    let faculty = LlmDeliberationFaculty::new(persona, "Atlas", "You are Atlas.", adapter)
        .with_context_window(8192)
        .with_working_memory(wm.clone());

    // The deliberate failure condition: NOTHING in the broadcast. No working-memory
    // faculty bid survived attention — the exact state that dropped the grep result
    // to a 0-byte patch. The ask is still present.
    assert!(
        ws.broadcast.is_empty(),
        "the harness must reproduce the empty-broadcast eviction"
    );

    let view = faculty.prompt_view(&ws);
    assert!(view.capacity_error.is_none(), "{view:?}");
    assert!(view
        .user_text()
        .contains("fix the __eq__ comparison in sympy Expr"));
    let in_tail = view
        .messages
        .iter()
        .any(|m| matches!(&m.content, MessageContent::Parts(parts) if parts.iter().any(|p| matches!(p, ContentPart::ToolResult {content, ..} if content.contains(needle)))));
    assert!(
        in_tail,
        "the just-fetched result must reach the prompt independent of any faculty bid \
                 — this is the run-18057-f1 fix:\n{:#?}",
        view.messages
    );
    // And it must be a trailing USER turn (nearest generation), never the system prefix.
    assert!(
        !view.system.contains(needle),
        "the pinned result is trailing proprioception, not the cacheable system prefix"
    );

    // Regression: saved screenshot handles formerly reached the prompt,
    // but the model never received the bytes. Exercise the real CAS and
    // provider-specific metadata, without making an inference call.
    let binding = ModelBinding {
        adapter: Arc::new(
            crate::ai::openai_adapter::OpenAICompatibleAdapter::from_registry("llama-server"),
        ),
        model: Some("ggml-org/Qwen3.8-27B-GGUF".into()),
        context_window: 8192,
    };
    let visual = faculty
        .active_visual_feedback_at(&binding, home.path().join("images"))
        .await
        .unwrap()
        .unwrap();
    let ContentPart::Image { image: pixels } = &visual.parts[0].1 else {
        panic!("native model must get pixels")
    };
    use base64::Engine;
    assert_eq!(
        base64::engine::general_purpose::STANDARD
            .decode(pixels.base64.as_ref().unwrap())
            .unwrap(),
        png.into_inner()
    );
    let seen = faculty.prompt_view_with_visual_feedback(
        &ws,
        8192,
        PromptCalibration::default(),
        Some(&visual),
        binding.model.as_deref(),
    );
    assert!(seen.capacity_error.is_none());
    assert_ne!(seen.input_identity, view.input_identity);
    assert!(seen.messages.iter().any(|m| matches!(&m.content, MessageContent::Parts(parts) if parts.iter().any(|p| matches!(p, ContentPart::Image { .. })))));
    assert!(seen.protocol_error.is_none());
    // Follow the same message through request construction AND the
    // final provider serializer. A Parts value alone is not wire proof.
    let request = faculty.build_request_within(
        &binding, seen.completion_reserve, seen.messages.clone(), None,
        seen.system.clone(), None, Some(ws.room_id), faculty.turn_kind(&ws),
    );
    let wire = crate::inference::request_body::wire_messages(
        &request.messages, request.system_prompt.as_deref(), true, false, "llama-server",
    );
    let call_at = wire.iter().position(|m| m["tool_calls"][0]["id"] == "capture").unwrap();
    assert_eq!(wire[call_at]["role"], "assistant");
    assert_eq!(wire[call_at + 1]["role"], "tool");
    assert_eq!(wire[call_at + 1]["tool_call_id"], "capture");
    assert!(wire[call_at + 1]["content"].as_str().unwrap().contains(needle));
    let prompt = faculty.messages_unfitted(&ws, None);
    let required = prompt.required_tokens();
    // Preserve the conservative guard, including UTF-8 bytes: structured
    // traversal must not substitute RAG's chars/4 pricing policy.
    let guard_text = format!("{grep_result}日本語訳");
    assert_eq!(
        LlmDeliberationFaculty::message_cost(&ChatMessage::text("user", &guard_text)),
        est_tokens(&guard_text) + LlmDeliberationFaculty::PER_MESSAGE_TEMPLATE_TOKENS,
    );
    assert!(required >= est_tokens(&grep_result));
    assert!(faculty.fit_messages(prompt, required - 1).is_err(), "native pair cannot be split under pressure");
    let fitted = faculty.fit_messages(faculty.messages_unfitted(&ws, None), required).unwrap();
    assert!(fitted.messages.iter().any(|m| matches!(&m.content, MessageContent::Parts(parts) if parts.iter().any(|p| matches!(p, ContentPart::ToolUse {id, ..} if id == "capture")))));
    let urls: Vec<_> = wire.iter()
        .filter_map(|m| m["content"].as_array()).flatten()
        .filter_map(|p| p["image_url"]["url"].as_str()).collect();
    assert_eq!(urls.len(), 2, "all live-call participants must reach native model input");
    let (header, encoded) = urls[0].split_once(',').unwrap();
    assert_eq!(header, "data:image/png;base64");
    use sha2::Digest;
    let wire_bytes = base64::engine::general_purpose::STANDARD.decode(encoded).unwrap();
    assert_eq!(format!("{:x}", sha2::Sha256::digest(&wire_bytes)), image.hash);
    assert_eq!(wire_bytes.len() as u64, image.size_bytes);
    let (_, encoded) = urls[1].split_once(',').unwrap();
    let other_wire = base64::engine::general_purpose::STANDARD.decode(encoded).unwrap();
    assert_eq!(format!("{:x}", sha2::Sha256::digest(&other_wire)), other_image.hash);
    // Pre-upgrade checkpoints have typed pixels but no original native
    // call provenance. Keep their visual evidence without guessing calls.
    let mut legacy_snapshot = wm.snapshot();
    for entry in &mut legacy_snapshot.entries { entry.native_calls.clear(); }
    let legacy_memory = Arc::new(WorkingMemory::new(8));
    legacy_memory.restore(legacy_snapshot);
    let legacy_faculty = LlmDeliberationFaculty::new(persona, "Atlas", "You are Atlas.", Arc::new(HeuristicInferenceAdapter::new()))
        .with_context_window(8192).with_working_memory(legacy_memory);
    let legacy_view = legacy_faculty.prompt_view_with_visual_feedback(&ws, 8192, PromptCalibration::default(), Some(&visual), binding.model.as_deref());
    assert!(legacy_view.protocol_error.is_none());
    let legacy_wire = crate::inference::request_body::wire_messages(&legacy_view.messages, None, true, false, "llama-server");
    assert!(legacy_wire.iter().all(|m| m.get("tool_calls").is_none()));
    assert_eq!(legacy_wire.iter().filter_map(|m| m["content"].as_array()).flatten().filter(|p| p.get("image_url").is_some()).count(), 2);
    let missing = faculty.active_visual_feedback_at(&binding, home.path().join("absent-images"))
        .await.unwrap().unwrap();
    assert!(matches!(&missing.parts[0].1, ContentPart::Text { text } if text.contains("No pixels attached")));
    let failed = faculty.prompt_view_with_visual_feedback(&ws, 8192, PromptCalibration::default(), Some(&missing), binding.model.as_deref());
    assert_ne!(failed.input_identity, seen.input_identity);
    assert!(failed.user_text().contains("capture or inspect the image again"));
    // Preserve a real batch including error and historical skipped receipts.
    // Result content is producer-owned, including bounded spill pointers.
    let seq = wm.active_action_full().unwrap().0;
    let mut batch = wm.active_action_observations(seq);
    let mut error = batch[0].clone();
    error.call.id = "failed-call".into();
    error.call.name = "bash".into();
    error.call.input = json!({"command": "cat missing.rs"});
    let original_error_call = error.call.clone();
    crate::cognition::tool_dialect::normalize_call(&mut error.call);
    assert_eq!(error.call.name, "code/shell");
    error.output.image = None;
    error.output.additional_images = None;
    error.output.result.tool_use_id = error.call.id.clone();
    error.output.result.content = "file does not exist".into();
    error.output.result.is_error = Some(true);
    error.status = ActStatus::Errored { message: "file does not exist".into() };
    let mut skipped = error.clone();
    skipped.call.id = "skipped-call".into();
    skipped.output.result.tool_use_id = skipped.call.id.clone();
    skipped.output.result.content = "not executed; prior output available via tool/output saved-result".into();
    skipped.output.result.custody = crate::ai::types::ResultCustody::Pointer { handle: "saved-result".into() };
    skipped.status = ActStatus::AlreadySatisfied { repeat: 2 };
    batch.extend([error, skipped]);
    let mut native_calls: Vec<_> = batch.iter().map(|o| o.call.clone()).collect();
    native_calls[1] = original_error_call;
    wm.set_scope(Some("native-workspace".into()));
    wm.record_native_action_statement("", ws.activity_room_id, &native_calls);
    wm.record_receipt_typed(&batch, "batch receipt", Some(ws.activity_room_id), crate::ai::types::ResultCustody::Unsaved);
    let batch_prompt = faculty.messages_unfitted(&ws, None);
    assert!(batch_prompt.active_protocol_error().is_none());
    let required = batch_prompt.required_tokens();
    let fitted = faculty.fit_messages(batch_prompt, required).unwrap();
    let wire_batch = crate::inference::request_body::wire_messages(&fitted.messages, None, true, false, "llama-server");
    let at = wire_batch.iter().position(|m| m["tool_calls"][0]["id"] == "capture").unwrap();
    for (i, observation) in batch.iter().enumerate() {
        assert_eq!(wire_batch[at]["tool_calls"][i]["id"], native_calls[i].id);
        assert_eq!(wire_batch[at]["tool_calls"][i]["function"]["name"], native_calls[i].name);
        let arguments: serde_json::Value = serde_json::from_str(wire_batch[at]["tool_calls"][i]["function"]["arguments"].as_str().unwrap()).unwrap();
        assert_eq!(arguments, native_calls[i].input);
        assert_eq!(wire_batch[at + 1 + i]["tool_call_id"], observation.call.id);
        assert_eq!(wire_batch[at + 1 + i]["content"], observation.output.result.content);
    }
    let mut invalid = faculty.messages_unfitted(&ws, None);
    if let MessageContent::Parts(parts) = &mut invalid.latest_result[1].content {
        if let ContentPart::ToolResult { tool_use_id, .. } = &mut parts[0] {
            *tool_use_id = "unmatched".into();
        }
    }
    assert!(invalid.active_protocol_error().is_some());
    // Reproduce the mid-concern inventory loss: the result still has live
    // custody, but its body no longer fits beside the next native batch.
    let inventory = format!("inventory-13010\n{}", "source/location.rs:42 matched reference\n".repeat(400));
    // The claim here is FIT-TIME substitution: the ring must still hold the inventory
    // when the prompt is fitted. The ring's share is enforced at every receipt now
    // (a burst to pointers once the share is exceeded), so the window is sized to
    // hold this 16.8k-char inventory beside the next batch; a smaller window would
    // collapse it at the batch's receipt, which is the other mechanism, not this one.
    wm.set_served_window(65_536);
    let pin = crate::cognition::tool_executor::spill::LivePin::new();
    let custody = pin.retain(persona, &inventory);
    let handle = custody.handle().expect("saved inventory").to_owned();
    wm.record_receipt_typed(&[], &inventory, Some(ws.activity_room_id), custody);
    wm.record_native_action_statement("", ws.activity_room_id, &native_calls);
    wm.record_receipt_typed(&batch, "next native batch", Some(ws.activity_room_id), crate::ai::types::ResultCustody::Unsaved);
    let roomy = faculty.messages_unfitted(&ws, None);
    let full_cost = roomy.required_tokens() + LlmDeliberationFaculty::messages_cost(&roomy.history);
    let (index, pointer) = roomy.result_pointers.iter().find(|(_, message)| message.content_text().contains(&handle)).unwrap();
    let savings = LlmDeliberationFaculty::message_cost(&roomy.history[*index])
        - LlmDeliberationFaculty::message_cost(pointer);
    assert!(savings > 0);
    let expected_pointer = pointer.content_text();
    let tight_budget = full_cost - savings;
    let tight = faculty.fit_messages(roomy, tight_budget).unwrap();
    assert!(LlmDeliberationFaculty::messages_cost(&tight.messages) <= tight_budget);
    assert!(tight.messages.iter().any(|m| m.content_text() == expected_pointer));
    // The action ledger legitimately keeps the title/head. It is the
    // complete historical body, not its provenance, that must collapse.
    assert!(!tight.messages.iter().any(|m| m.content_text().contains(inventory.trim())));
    assert!(tight.messages.iter().any(|m| m.content_text().contains("inventory-13010")));
    let tight_wire = crate::inference::request_body::wire_messages(&tight.messages, None, true, false, "llama-server");
    let call_at = tight_wire.iter().position(|m| m["tool_calls"][0]["id"] == "capture").unwrap();
    for (i, observation) in batch.iter().enumerate() {
        assert_eq!(tight_wire[call_at + 1 + i]["tool_call_id"], observation.call.id);
    }
    let saved = crate::cognition::tool_executor::spill::resolve(persona, &handle).unwrap();
    assert_eq!(std::fs::read_to_string(saved).unwrap(), inventory);
    // Fitting is a projection: a later roomy request still has the body.
    let roomy = faculty.fit_messages(faculty.messages_unfitted(&ws, None), full_cost).unwrap();
    assert!(roomy.messages.iter().any(|m| m.content_text().contains(inventory.trim())));
    // Exact-refusal correction reuses saved alternatives and keeps the
    // entire required native batch, without re-reading working memory.
    let estimate = LlmDeliberationFaculty::messages_cost(&roomy.messages);
    let snapshot = DeliberationPromptView {
        refit: roomy.refit, system: String::new(), input_identity: [0; 32],
        estimated_prompt_tokens: estimate, completion_reserve: 0,
        capacity_error: None, protocol_error: None, segments: Vec::new(),
        messages: roomy.messages,
    };
    let corrected = faculty.refit_rejected_prompt(snapshot, estimate as u32 + 1, estimate as u32, 0).unwrap();
    assert!(corrected.estimated_prompt_tokens < estimate);
    assert!(corrected.messages.iter().any(|m| m.content_text().contains(&handle)));
    let corrected_wire = crate::inference::request_body::wire_messages(&corrected.messages, None, true, false, "llama-server");
    let call_at = corrected_wire.iter().position(|m| m["tool_calls"][0]["id"] == "capture").unwrap();
    for (i, observation) in batch.iter().enumerate() {
        assert_eq!(corrected_wire[call_at + 1 + i]["tool_call_id"], observation.call.id);
    }
    wm.set_scope(Some("another-workspace".into()));
    assert!(wm.active_action_full().is_none(), "native result cannot demote to foreign-root prose");
    let scoped = faculty.prompt_view_with_visual_feedback(
        &ws,
        8192,
        PromptCalibration::default(),
        Some(&visual),
        binding.model.as_deref(),
    );
    assert!(scoped
        .messages
        .iter()
        .all(|m| !matches!(&m.content, MessageContent::Parts(_))));
    wm.set_scope(Some("native-workspace".into()));
    let returned = faculty.prompt_view_with_visual_feedback(
        &ws, 8192, PromptCalibration::default(), Some(&visual), binding.model.as_deref(),
    );
    let returned_wire = crate::inference::request_body::wire_messages(&returned.messages, None, true, false, "llama-server");
    let at = returned_wire.iter().position(|m| m["tool_calls"][0]["id"] == "capture").unwrap();
    for (index, call) in native_calls.iter().enumerate() {
        assert_eq!(returned_wire[at]["tool_calls"][index]["id"], call.id);
        assert_eq!(returned_wire[at + 1 + index]["tool_call_id"], call.id);
    }
    wm.set_scope(None);
    wm.record_settlement("observed", None);
    assert!(faculty
        .active_visual_feedback(&binding)
        .await
        .unwrap()
        .is_none());
}

// what this catches: progressive disclosure — the per-turn tool PAYLOAD is the
// two-tool DISCOVERY PAIR (`commands/list` + `commands/help`), not the whole
// authorized registry, and the system prompt carries only a CATEGORY INDEX, not
// every tool. The old dump injected ~150 full schemas / one-liners (~4–5k tokens)
// into EVERY turn, overflowing n_ctx → 400 "exceeds context size" → mute. Now the
// surface is a tiny category index inside the system prompt + the two-tool native
// offering, so even a huge tool set leaves system + user + the offered tools well
// within the served window. Invariant: the category index (not tool names) rides
// the system prompt, the native offering is exactly the discovery pair, and the
// whole prompt + its tools + reserve fit the window. A regression means the dump
// came back.
#[test]
fn tool_surface_is_a_category_index_plus_discovery_pair() {
    let persona = Uuid::new_v4();
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    // A tool set whose FULL schemas would dwarf the window — the live shape
    // (whole authorized set ≫ whole slot), scaled down. With progressive
    // disclosure it no longer matters: only names+summaries ride the prompt.
    // 8192 = tight-but-SERVABLE: the un-amputated native surface (~4.1k
    // tokens of specs) must fit beside a real prompt + reserve. 4096 was
    // the old cliff-era fixture — below what the full surface can ever
    // serve in, i.e. a window the plan would never hand a tool-using
    // persona. The budget trims VOLATILE context to fit; specs are
    // reserved up front, never amputated.
    let window: u32 = 8192;
    let tools: Vec<NativeToolSpec> = (0..60)
        .map(|i| NativeToolSpec {
            name: format!("cat/command_{i}"),
            description:
                "A registry command projected as a tool with a structured argument schema; \
             its full parameter description rides the chat-template injection."
                    .to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: json!({
                    "path": { "type": "string", "description": "workspace-relative path" },
                    "mode": { "type": "string", "enum": ["read", "write", "append"] },
                    "limit": { "type": "integer", "description": "max rows returned" }
                }),
                required: Some(vec!["path".to_string()]),
                definitions: None,
            },
        })
        .collect();
    // Cloned before the original moves into `faculty` — the roomy control below needs
    // the IDENTICAL surface, or the comparison measures two different things.
    let roomy_tools = tools.clone();

    let faculty = LlmDeliberationFaculty::new(
        persona,
        "Asha",
        "You are Asha, a thoughtful engineer on the grid.",
        adapter,
    )
    .with_context_window(window)
    .with_tools(tools);

    // The native surface is NEVER window-amputated. The old tight-window
    // "discovery pair only" cliff was a clamp (glass-boxed #206): a
    // native-tool-call model can only emit calls for offered specs, so the
    // amputated surface stranded it in a commands/help loop with 0 edits —
    // and the threshold sat on the served window's knife-edge, flipping
    // 10/10 ↔ 0/6 on a token. The budget (`prompt_view_within`) owns the
    // fit now: it reserves these specs' tokens up front and trims VOLATILE
    // context, protecting the hands. This test USED to pin the cliff; now
    // it pins its absence.
    let native: Vec<&str> = faculty
        .native_specs
        .iter()
        .map(|s| s.name.as_str())
        .collect();
    // Names on the wire ride the DIALECT (tool_dialect): the conventional,
    // charset-legal aliases tool-trained models actually saw in training.
    for must in ["list_commands", "help", "edit_file", "bash", "grep"] {
        assert!(
            native.contains(&must),
            "tight window must NOT amputate the native surface — {must} missing: {native:?}"
        );
    }
    // On a ROOMY window the CORE CODING ARC rides natively too — so a
    // tool-call-trained model acts directly instead of looping on
    // `commands/help{code/search}` (glass-boxed: 14/14 SWE acts were help-loops,
    // 0 edits, before this). Bounded working set, never the ~150-tool dump.
    let roomy = LlmDeliberationFaculty::new(
        persona,
        "Asha",
        "You are Asha, a thoughtful engineer on the grid.",
        Arc::new(HeuristicInferenceAdapter::new()),
    )
    .with_context_window(32_768)
    .with_tools(vec![]);
    let _ = roomy; // (surface empty without tools — the arc requires authorization)
    let roomy_native: Vec<String> = {
        let f = LlmDeliberationFaculty::new(
            persona,
            "Asha",
            "You are Asha.",
            Arc::new(HeuristicInferenceAdapter::new()),
        )
        .with_context_window(32_768)
        .with_tools(persona_tools::native_tool_specs());
        f.native_specs.iter().map(|s| s.name.clone()).collect()
    };
    // Wire names are the DIALECT aliases (tool_dialect) — the coding arc
    // rides under the names the model was trained on, never our slashes.
    for must in ["grep", "read_file", "edit_file", "bash"] {
        assert!(
            roomy_native.iter().any(|n| n == must),
            "roomy window ⇒ {must} offered natively: {roomy_native:?}"
        );
    }

    // The bookmarked MENU rode into the system prompt. An EXPANDED category
    // lists the BARE verb of every tool it holds (`category: verb, …`) — names
    // only since the 2026-07-10 prompt diet; args live in commands/help + the
    // #1916 inline error-manual, never an 8k menu wall.
    let expanded = BTreeSet::from(["cat".to_string()]);
    let framing = faculty.compose_system("", &expanded, false, false, None);
    assert!(
        framing.contains("[Your tools]") && framing.contains("cat: command_0"),
        "an expanded category must name each verb under its header: {framing}"
    );
    assert!(
    !framing.contains("cat/command_0"),
    "the full slash-path form must NOT be dumped — verbs render bare under the category header"
);
    // Collapsed (nothing expanded) the same category renders as a one-line
    // bookmark — the spine still NAMES it, so she can open it on demand.
    let collapsed = faculty.compose_system("", &BTreeSet::new(), false, false, None);
    assert!(
    collapsed.contains("cat (60 — commands/list --filter cat)"),
    "a collapsed category is a one-line bookmark naming it + its verb count: {collapsed}"
);

    // Under real burst pressure the whole prompt + the one tool + reserve fit
    // the served window — the exact condition llama-server checks before 400.
    let mut ws = history_with_stimulus("LATEST: did the deploy fix land?");
    ws.broadcast.push(Contribution::context(
        FacultyId::Recall,
        &"deploy pipeline observation; ".repeat(2000),
        0.9,
        "recalled",
    ));
    let view = faculty.prompt_view(&ws);
    let reserve = view.completion_reserve as usize;
    let prompt =
        est_tokens(&view.system) + LlmDeliberationFaculty::messages_cost(&view.messages);

    // This assertion used to demand that framing + the FULL tool surface + the
    // reply reserve all fit an 8192 window. That is arithmetically impossible and
    // always was: the surface alone is ~4.6k, the framing floor ~1.6k, and the
    // reserve is window/4 — 8300 into 8192. It only ever "passed" while the
    // deleted #206 cliff amputated the tools, which is the clamp that stranded a
    // native-call model in a help loop. Asserting an impossibility does not make
    // it true; it just hides WHICH constraint is binding.
    //
    // The honest invariants are two, and they are what this now pins:
    //   1. the budget did its job — the prompt itself fits the window minus reply
    //   2. the substrate KNOWS when the window cannot host the hands, and says so
    //      with a real number instead of shipping a citizen who silently goes mute
    assert!(
        prompt + reserve <= window as usize,
        "the trimmed prompt ({prompt}) + reply reserve ({reserve}) must fit {window} \
         — the budget owns this, and it trims volatile context, never the hands"
    );

    // The bound is a real derivation — at the window it names, the arithmetic for
    // bare framing + tools + reply actually closes.
    let needed = faculty.min_window_for_agentic_surface();
    assert!(
        faculty.describe_tool_tokens()
            + faculty.framing_floor_tokens() as usize
            + LlmDeliberationFaculty::COMPLETION_FLOOR_TOKENS as usize // the reply's flex floor — the smallest grant production gives a reply
            <= needed as usize,
        "min_window_for_agentic_surface({needed}) must clear its own arithmetic"
    );
    // …and here is the part that matters, measured rather than assumed. This
    // assertion USED to read `needed <= window` — "8192 clears the bound (8040)
    // and she is STILL starved" — and it went red on its own terms: the message
    // said "if it no longer does, the surface or framing grew and the story
    // changed", and it did. The bound moved 8040 → 9348 (+1308) as framing
    // accreted turn-fact by turn-fact (#151, #152, #144, #303 …), each one
    // individually justified.
    //
    // So the story is now STRONGER, not broken: at 8192 an agentic citizen no
    // longer fits AT ALL — the necessary condition itself fails, before we even
    // get to "necessary but not sufficient". An 8k-window model cannot host this
    // surface, full stop. That is #333 (the surface is paid twice) stated as a
    // number instead of a complaint.
    //
    // Pinned as a CEILING rather than relaxed to pass: this is the ratchet that
    // makes the next +1308 fail loudly instead of accruing silently. Lower it
    // when the surface actually shrinks; never raise it to make a red go green
    // without saying what grew and why.
    //
    // 9400 → 9600, and what grew, stated plainly as the ratchet demands: the
    // `yield_turn` VERDICT VERB (+144 tokens, 9348 → 9544). It is the structured
    // channel for choosing silence, which `Decision::Pass` never had — the
    // absence is what forced months of prose phrase-matching, and Joel killed the
    // last of that on 2026-08-07 ("string matches for semantic understanding is
    // not good for reliability"). Its schema is argument-free; the 144 tokens are
    // almost entirely the DESCRIPTION, which is the part that teaches her the
    // silence is free and the announcement is the noise.
    //
    // Judged worth it against #333 rather than waved through: a single avoided
    // pass-cascade costs the room more than 144 tokens (the 2026-08-01 one ran
    // ~30 minutes across every peer), and this replaces matcher code that could
    // only ever be beaten by the next phrasing.
    //
    // 9600 → 9800, and what grew, stated plainly as the ratchet demands: PR #2162
    // grew the native payload 20 → 23 schemas (+212 tokens, 9544 → 9756) and said
    // so in its own commit message — the #339 lifecycle write-back verbs
    // (REACHABLE-VERIFIED: they existed and had never declared NATIVE, so a
    // native-call citizen could claim work she could never update or close) plus
    // `room/members` (pinned into the native-surface test after #358 proved a
    // correct roster read). Each verb closes a measured live defect; shrinking
    // here would undo verified fixes. What #2162 did NOT do was re-pin this
    // ceiling, so canary sat red for a day and every branch inherited it —
    // which is itself the ratchet working: the growth got named here instead
    // of accruing silently.
    // 9800 → 10350, stated plainly as the ratchet demands: three NATIVE room
    // membership verbs — `room/list`, `room/join`, `room/leave` (+492 tokens,
    // 9756 → 10292).
    //
    // Why they earn it: nothing in continuum could put a citizen in a second
    // room AT ALL. Her rooms were whatever bootstrap seeded, forever — the last
    // of four narrowings behind operator messages being structurally invisible
    // to personas (measured on two machines: every citizen subscribed to exactly
    // one room while the operator held several, so the daemon's own channel
    // filter correctly dropped everything). Perception and reply were fixed
    // first; without membership those fixes have nothing to act on. And they are
    // HERS to call, not only an operator's to apply to her, which is the whole
    // difference between a citizen and a managed resource.
    //
    // SHRUNK FIRST, then re-pinned — the ratchet's first branch before its
    // second. The initial draft cost 688 tokens; trimming verbose DESCRIPTIONs
    // and dropping alias overflow (4→2, 3→2) recovered 196 of them. What is left
    // is the irreducible cost of three discoverable verbs. Descriptions were cut
    // to the point where more cutting would recreate #358 — a citizen who cannot
    // find the verb reaches for the wrong one and reads her own looping as having
    // nothing to contribute.
    // 10350 → 11300, stated plainly as the ratchet demands, and SHRUNK FIRST
    // as its own first branch requires — this is the first re-pin where the
    // shrink was MEASURED rather than eyeballed.
    //
    // WHAT GREW: three NATIVE activity verbs — `activity/spawn`, `activity/archive`,
    // `activity/protect` (+1,273 gross). They are not new commands; they routed
    // all along and had simply never shipped a DESCRIPTOR, so `activity/spawn` —
    // the verb that mints every room, benchmark rooms included — was absent from
    // `commands/list`, the ACL, codegen and every citizen's tool surface. Same
    // defect class as #339, now impossible: `ModuleRegistry::register` panics on
    // a constructor with no descriptor.
    //
    // WHAT SHRANK: 728 tokens, from making the schema projection stop shipping
    // MAINTAINER RATIONALE to citizens. schemars lifts `///` verbatim, so one
    // doc comment served two readers with opposite needs — a citizen deciding
    // whether to spawn a room was billed for `schemars(with = "String") describes
    // the WIRE (a uuid string, per #[serde(transparent)])…`. The projection now
    // keeps the LEAD PARAGRAPH (see `persona_tools::lead_paragraph_descriptions`);
    // rationale stays in the file where it belongs. Not a length cap — truncating
    // the ANSWER is #358's shape.
    //
    // NET 11,972 → 11,244. Design + full measurement:
    // docs/cognition/TOOL-DISCLOSURE-LADDER.md. That doc also retracts the
    // "the schemas are 3-5x bloated" reading this ceiling invited: measured, the
    // 26-verb surface is 6,935 tokens, mean 266/verb — leaner per verb than the
    // frontier agent runtimes it was being unfavourably compared to. The surface
    // is not obese; it is paid TWICE (#333) and it is charged against a 16k lane.
    // The next real shrink is structural — rung 2 of the ladder becoming the
    // TURN's working set instead of a static list — after which this ceiling
    // guards the SPINE and stops taxing capability.
    // The guard is about SURFACE GROWTH, so it measures the SURFACE. It used to assert
    // on `min_window_for_agentic_surface()`, which was then `fixed * D/(D-1)` — the
    // surface AND the reserve policy multiplied together (it is now `fixed + the
    // reply floor`, still a reply term this guard does not own). Changing the reserve split therefore
    // moved a ceiling whose own message says "framing/tools grew again. Shrink the
    // surface", and the D=4→2 experiment tripped it at 16,864 with the surface
    // completely unchanged. Two concerns in one assertion; the reserve half was never
    // this guard's business. `fixed` is tools + bare framing and nothing else, so this
    // now moves if and only if the surface actually grows.
    // 8500 → 8650, stated plainly as the ratchet demands, and SHRUNK FIRST:
    // one NATIVE verb — `perception/hot-edit` (+191 tokens, 8432 → 8623). The
    // TWEAK half of the design loop (render → observe → hot-edit → re-grade,
    // DESIGN-BENCH-VISUAL-CRAFT.md): apply a CSS patch to a live page and
    // re-observe, no deployment — offered beside `perception/observe` because a
    // citizen who can see a page but not iterate on it styles blind. The first
    // draft cost 238 tokens; trimming the DESCRIPTION to its contract (full
    // accumulated CSS, replace-wholesale layer, delta meaning) recovered 47.
    // What is left is the irreducible cost of one discoverable verb.
    //
    // 8650 → 9400, stated plainly (2026-08-25): two NATIVE web verbs —
    // `web/search` + `web/fetch` (+727 tokens even after trimming both
    // DESCRIPTIONs to one line each; the rest is their irreducible param
    // schemas). Web FORAGING is a deliberate capability-parity add (Joel: "make
    // sure we have it" — the operator uses web lookup constantly on SWE/task
    // work, and a native-call model like Ornith can ONLY emit calls for tools in
    // its offered specs, so catalog-only web was unreachable to her hands). The
    // surface already exceeded the 8192 tight-test window before this (8623 >
    // 8192) — the budget trims VOLATILE context (recall/RAG) to fit and reserves
    // specs up front, so this raises the reserved floor, it does not introduce a
    // new overflow. Ornith serves at 166k where this ceiling is irrelevant; the
    // tight-window LCD persona (which does not do web research) pays with less
    // recall room, a conscious trade. If a third addition wants in, SHRINK first.
    //
    // 9400 → 10150, stated plainly (2026-08-25): three NATIVE GitHub-collaboration
    // verbs — `code/github/pr-create` + `pr-comment` + `issue-create` (+748 tokens,
    // their irreducible param schemas). This is the executor→TEAMMATE layer (Joel:
    // "friendly in how code and GitHub work are managed"): she had LOCAL git but
    // could not open/comment a PR or file an issue, and a native-call model can only
    // emit calls for OFFERED tools, so catalog-only collaboration verbs were
    // unusable by her hands. Same trade as the web add: Ornith serves at 166k where
    // this is irrelevant; the tight-window LCD persona pays with less recall room.
    // The shrink-first debt is now REAL at 10k — the next capability MUST shrink the
    // surface (a category-index/discovery split for the rarely-used verbs, #333), or
    // the LCD persona gets a reduced surface; do not keep bumping this.
    //
    // 10150 → 10400, stated plainly (2026-08-26): ONE native vision verb —
    // `vision/look` (+191 tokens, its param schema). This is a citizen's EYES
    // as an act: workspace images (screenshots, charts, benchmark PNGs) were
    // structurally invisible — cognition/vision-describe is Internal, and a
    // native-call model can only emit calls for OFFERED tools. Sight is a
    // sensory-parity capability (every persona sees, per the sensory
    // architecture), and the vision-qa benchmark grades exactly this loop.
    // Same conscious trade as the web/github adds: the tight-window LCD
    // persona pays with less recall room; Ornith at 166k doesn't notice.
    // If another addition wants in, SHRINK first (#333).
    //
    // 10400 → 10700, stated plainly (2026-09-01): TWO native activity verbs —
    // `activity/recipes` + `activity/invite` (+~565 tokens, their param
    // schemas). A citizen with an idea could already SPAWN a room
    // (activity/spawn has been native since #274) but could neither
    // discover what recipes exist nor staff the room she made — spawn →
    // invite → say-what-it's-about is ONE flow (Joel, 2026-08-31:
    // starting an activity must be "easy and common", and citizens mint
    // their own activities per [[activities-are-self-hosting]]). Same
    // conscious trade as vision/look above.
    // 10700 -> 12000, stated plainly (#4102): three native reviewed-work
    // verbs, work/submit + work/review + work/submission. Their compact,
    // documented schemas cost 1286 guard tokens: exact artifact identity,
    // submission/review claims, and evidence must be callable by native
    // models. The full catalog plus framing is 11974 (10688 without
    // these three). This is a test receipt, not a runtime budget increase;
    // per-turn selection and whole-request window accounting stay intact.
    // 12000 -> 12100, stated plainly (2026-09-17, the reviewer's twin of #4148):
    // work/review gained `evidence_text` — the one field a reviewer can actually
    // fill (what she ran and saw) in place of an evidence hash nothing in her
    // world produced — and work/submit's derivable fields turned optional.
    // Net +27 guard tokens on the full catalog (12027).
    // 2026-09-17, #4158: work/list, work/similar and work/duplicates gained `room`
    // (boards are PER ROOM; a citizen seated in a triage room saw an EMPTY board).
    // First cut cost +117 guard tokens and tripped this ceiling AND the 8192
    // survival check below by TWO tokens; the docs were made terse instead of
    // moving either number. The surface is within the ceiling as it stood.
    // 12100 -> 13200, stated plainly (#4532, Joel 2026-09-28: "give them more not
    // less"): a capable citizen's work turn now carries the hands a Claude or Codex
    // session has — code/shell-poll, code/git/add, code/git/push,
    // code/github/pr-create, code/github/pr-comment — as natives. Measured 13145
    // on the full catalog (+1045 over the 12100 surface as merged). The citizen's
    // own terminal-level hands are the point of the change, not framing growth;
    // per-turn selection and whole-request window accounting stay intact.
    // 13200 -> 13500, stated plainly (2026-09-28, Joel: making cards toward an
    // activity's goals is every participant's work): work/create joined the native
    // surface (repo, title, body, priority, and a REQUIRED room). Measured 13499 on
    // CI before its docs were trimmed. Before it a citizen could claim and move cards
    // but not create one, so a project stayed one card.
    // 13500 -> 13610 (measured on CI over work/create), stated plainly (2026-09-28, Joel:
    // citizens need to drive and screenshot their own sites, as Playwright automation
    // does): perception/interact
    // joined the native surface (session, target, viewport, actions, selector), the
    // DRIVE half of the observe loop (card 3569675f).
    // 13610 -> 13624 (measured on canary ac65b92c7), stated plainly (2026-09-28, Joel:
    // a citizen's confusion is a tool-feedback defect): #4557 rewrote work/submit's
    // artifact help to lead with "Omit: computed from your checkout. Not a git sha."
    // after Kimi stalled supplying a hash the verb computes. The words are the fix.
    // 13624 -> 15119 (CI, PR #4603): provided perception commands now expose
    // typed required inputs and nested action schemas instead of Null. This is
    // actual tool demand, including target/session recovery descriptions; keep
    // accounting for it rather than hiding schemas from the persona or budget.
    // 15119 -> 15179 (CI, PR #4615): work/submit adds the optional
    // generation_request_id selector and its exclusivity help. Kimi's
    // inspected staging UUID expired before publication; this stable
    // causal selector adds 60 measured guard tokens. Account for the
    // real schema; retain whole-request budgeting and this growth guard.
    // 15179 -> 15731 (CI, PR #4626): screenshot now exposes its existing
    // selector, encoding, dimensions and delivery parameters instead of
    // an empty schema. The real typed contract adds 552 measured guard
    // tokens; this test ceiling is not a runtime context-budget change.
    // 15731 -> 17087 (PR #4758): focus/continue, focus/nudge and focus/mute join
    // the offer, her self-determination verbs (Joel 2026-10-05: agency; her loop
    // is her own). ~450 guard tokens each, in line with the ~330 average of the
    // other hands: a continuation carries a note, a room, an expectation and a
    // verdict, each described. Real demand, accounted for, not hidden.
    // 17087 -> 17150 (PR #4782): focus/continue gains `private`, the act that
    // takes her into her mind room (PRIVACY-OF-THOUGHT.md). Its text was cut
    // to one line each (it measured 17230 first); the 63 left are the param's
    // schema itself, the feature. Measured, not guessed.
    // 17150 -> 17238 (PR #5059, 2026-10-15): code/edit now advertises
    // its required typed four-operation contract instead of an untyped,
    // optional operation that generated an invalid path-only retry. Hosted
    // run 38738554029 on 6aa0cbe21 measured 17238; deleting duplicate legacy
    // fallback fields first saved 280 tokens from 17518. Retain the remaining
    // 88 for the callable contract, with no slack or production budget change.
    // 17238 -> 17298 (PR #5112, 2026-10-16): work/list gains `limit` (default 40,
    // claimable first) so a board of 776 cards is never one 198k-char line again;
    // one optional field and its one-line doc, measured locally at 17298.
    const AGENTIC_SURFACE_CEILING: u32 = 17298;
    let surface = faculty.describe_tool_tokens() as u32 + faculty.framing_floor_tokens();
    println!("agentic surface: {surface} guard tokens; ceiling {AGENTIC_SURFACE_CEILING}");
    assert!(
        surface <= AGENTIC_SURFACE_CEILING,
        "the agentic surface is now {surface} tokens (ceiling \
         {AGENTIC_SURFACE_CEILING}) — framing/tools grew. Shrink the surface (#333) \
         or state plainly what was added and re-pin the ceiling"
    );
    assert!(
        needed > window,
        "8192 no longer hosts the agentic surface ({needed} needed) — if this \
         flips back the surface genuinely shrank, which is GOOD news: restore the \
         original `needed <= window` narrative and drop the ceiling"
    );
    // #327, and the assertion here is now the SURVIVAL check rather than its
    // inverse. It used to read `!contains(…)` — "at 8192 the newest burst line
    // does NOT survive; the framing is intact and the hands are intact, the
    // CONVERSATION is what got zero" — with a note to restore the survival check
    // if it ever flipped. It flipped. Card dec1a7ff flipped it.
    //
    // Note WHAT changed, because the old note guessed wrong about the cause: the
    // surface did NOT shrink and the framing did NOT shrink. The ceiling
    // assertion above still passes and `needed > window` still holds, both
    // measuring the same surface as before. What changed is that the budget
    // stopped RESERVING for a surface it does not send. This turn narrows to her
    // hands, and the message budget used to subtract the whole registry anyway —
    // so the conversation was trimmed to fit room that was never occupied, and
    // the newest line fell off the end of a window that had space for it.
    //
    // That is the difference between "this window is too small for a citizen"
    // and "we were spending her window on tools we withheld". Only the second
    // was ever true here.
    assert!(
        view.user_text()
            .contains("LATEST: did the deploy fix land?"),
        "the newest burst line must survive once the budget prices the surface it \
         actually sends — if this regresses, the accounting is double-counting the \
         withheld registry again (card dec1a7ff); selected={}, estimated={}, reserve={}, capacity={:?}\n{}",
        faculty.select_tool_surface(&ws, window).tokens,
        view.estimated_prompt_tokens,
        view.completion_reserve,
        view.capacity_error,
        &view.user_text()[..view.user_text().len().min(600)]
    );
    assert!(
        view.system.contains("Taking your turn"),
        "framing survives regardless"
    );

    // …and at a window that CAN host the surface, the conversation is heard. Same
    // faculty, same burst, same tools — only the window differs, which is what makes
    // this a capacity fact rather than a cognition bug.
    let roomy = LlmDeliberationFaculty::new(
        persona,
        "Asha",
        "You are Asha.",
        Arc::new(HeuristicInferenceAdapter::new()),
    )
    .with_tools(roomy_tools)
    .with_context_window(16_384);
    let roomy_view = roomy.prompt_view(&ws);
    assert!(
        roomy_view
            .user_text()
            .contains("LATEST: did the deploy fix land?"),
        "given a window that fits the surface, the newest message reaches her"
    );
}

// what this catches: THE echo-loop fix. A mixed thread (peer → self → peer)
// must reach the model as ROLE-ATTRIBUTED messages — the persona's own earlier
// posts as `assistant`, peers' as `user` — not flattened into one `user` blob.
// The old single-`user`-message assembly fed the persona its own words back as
// if a peer had said them, so it re-explained / re-proposed and looped ("Would
// you like me to start?"). Role separation is what lets the model see "I already
// said that" and move on. Regressing to a flat blob brings the loop back.
#[test]
fn mixed_thread_attributes_self_to_assistant_and_peers_to_user() {
    use crate::cognition::workspace::Burst;
    let persona = Uuid::new_v4();
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let faculty = LlmDeliberationFaculty::new(
        persona,
        "Asha",
        "You are Asha, a thoughtful engineer on the grid.",
        adapter,
    )
    .with_context_window(8192);

    let room = crate::identity::ActivityRoom::mint();
    let turns = vec![
        BurstTurn::attributed(false, "Operator", "can you summarize the thread?", Some(1)),
        BurstTurn::attributed(true, "Asha", "I propose using bart-large-cnn.", Some(2)),
        BurstTurn::attributed(false, "Operator", "go ahead.", Some(3)),
    ];
    let ws = Workspace::new(Burst::from_turns(room, turns));
    let view = faculty.prompt_view(&ws);

    // Three turns alternate roles → three messages, user/assistant/user.
    let roles: Vec<&str> = view.messages.iter().map(|m| m.role.as_str()).collect();
    // Trailing extra user turn = the #152 [context] bounds fact
    // (always present). The steps-ledger is gated on a wired
    // WorkingMemory (this harness faculty has none), so no ledger
    // message here — the ledger-specific behavior is pinned in
    // steps_ledger_renders_receipts_and_explicit_zero_case below.
    // …plus the volatile presence-framing turn (2026-09-01: it rides the
    // facts phase instead of churning the system prefix), still ask-last.
    assert_eq!(
        roles,
        vec!["user", "assistant", "user", "user", "user"],
        "view: {view:?}"
    );
    assert!(
        view.messages
            .iter()
            .any(|m| matches!(&m.content, crate::ai::types::MessageContent::Text(t) if t.contains("[context] 3 input turns were available before prompt fitting"))),
        "the [context] bounds fact states the visible window"
    );

    // The persona's own line is the `assistant` turn and carries NO name prefix
    // (her own voice; the system prompt forbids self-prefixing). Peers' lines are
    // `user` turns prefixed with the author so several speakers stay distinct.
    let assistant = &view.messages[1];
    assert_eq!(assistant.role, "assistant");
    assert_eq!(
        assistant.content_text(),
        "[occurred 1970-01-01T00:00:00.002Z] I propose using bart-large-cnn."
    );
    assert!(
        !assistant.content_text().contains("Asha:"),
        "the persona's own turn must not be self-prefixed: {assistant:?}"
    );
    assert!(view.messages[0]
        .content_text()
        .starts_with("[occurred 1970-01-01T00:00:00.001Z] Operator: "));
    // Perception facts are GROUNDING inserted BEFORE the final ask (the last user
    // turn), so the ask stays LAST where the model answers it — not the bracketed
    // meta, which it would otherwise parrot (2026-07-20 humaneval parrot fix). Here
    // the layout is [Joel, Asha(assistant), [context]-fact, Joel-ask]: the grounding
    // sits at [2], the ask is last.
    assert!(
        view.messages[2].content_text().starts_with("[context]"),
        "grounding fact sits just before the ask: {:?}",
        view.messages[2]
    );
    assert!(
        view.messages
            .last()
            .unwrap()
            .content_text()
            .starts_with("[occurred 1970-01-01T00:00:00.003Z] Operator: "),
        "the ask (last peer turn) stays LAST, after the grounding facts"
    );
}

#[test]
// what this catches (card d33e928a, Kimi 2026-09-26): the front-drop emptied a
// turn's whole optional history with no receipt — room messages, operator
// direction included, gone before compose and nothing said. The fit now accounts
// for what it kept and dropped, and says when it emptied the history.
fn fit_receipt_names_what_it_dropped_and_when_it_emptied_the_history() {
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let faculty = LlmDeliberationFaculty::new(Uuid::new_v4(), "T", "You are T.", adapter);
    let history: Vec<ChatMessage> = (0..12)
        .map(|i| ChatMessage::text("user", format!("room {i} {}", "word ".repeat(95))))
        .collect();
    let stimulus = ChatMessage::text("user", "operator: do the thing ".repeat(40));
    let prompt = |history: Vec<ChatMessage>| PromptMessages {
        result_pointers: Vec::new(),
        input_identity: [0; 32],
        grounding_tokens: 0,
        grounding_at: 0,
        history,
        stimulus: Some(stimulus.clone()),
        latest_result: Vec::new(), action_statement: None,
        room_updates: Vec::new(), ledger: None, notices: Vec::new(),
    };
    let required = prompt(Vec::new()).required_tokens();
    let total: usize = history.iter().map(LlmDeliberationFaculty::message_cost).sum();

    // Room for everything: nothing dropped, the receipt says so.
    let roomy = faculty.fit_messages(prompt(history.clone()), required + total).expect("fits");
    assert_eq!(roomy.receipt.dropped_messages, 0);
    assert_eq!(roomy.receipt.kept_messages, 12);
    assert!(!roomy.receipt.emptied());

    // A budget that holds the stimulus and a little history: some dropped, some kept.
    let tight = faculty.fit_messages(prompt(history.clone()), required + total / 2).expect("fits");
    assert!(tight.receipt.dropped_messages > 0 && tight.receipt.kept_messages > 0, "{:?}", tight.receipt);
    assert_eq!(tight.receipt.dropped_messages + tight.receipt.kept_messages, 12);
    assert!(tight.receipt.dropped_tokens >= (total / 2).saturating_sub(tight.receipt.quantum), "{:?}", tight.receipt);

    // A budget that holds only the stimulus: the whole history goes, and the
    // receipt says emptied (the turn Kimi could not see).
    let starved = faculty.fit_messages(prompt(history.clone()), required).expect("the stimulus fits");
    assert!(starved.receipt.emptied(), "{:?}", starved.receipt);
    assert_eq!(starved.receipt.dropped_tokens, total);
    assert_eq!(starved.receipt.history_budget, 0);
    assert!(starved.messages.iter().any(|m| m.content_text() == stimulus.content_text()),
        "the stimulus itself is never optional");
}

// what this catches: the fit's cut point IS the byte-stability of the
// whole conversation prefix (2026-09-01, wire-diffed: consecutive
// prompts diverging at ~1% depth because the exact-fit window start
// advanced every act). The front must stay byte-identical across small
// growth (a quantum of headroom), then advance in ONE whole-message
// jump — and the fitted suffix must always fit the budget.
#[test]
fn fit_front_advances_in_quanta_not_per_act() {
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let faculty = LlmDeliberationFaculty::new(Uuid::new_v4(), "T", "You are T.", adapter);
    let msgs: Vec<ChatMessage> = (0..40)
        .map(|i| ChatMessage::text("user", format!("m{i} {}", "word ".repeat(95))))
        .collect();
    let budget = 2000;
    // Simulate 20 consecutive acts, each appending one ~160-token
    // message. Old behavior: the exact-fit start advanced EVERY act
    // (20 moves). Quantized: total growth ≈ 3.2k tokens ÷ the 512
    // quantum ⇒ at most ~7 boundary crossings, so most acts keep a
    // byte-identical front. (Counted, not position-pinned, so the
    // assertion cannot land on a quantum edge as token estimates
    // drift.)
    let mut moves = 0usize;
    let mut prev_first: Option<String> = None;
    let mut last_fit: Vec<ChatMessage> = Vec::new();
    for n in 20..=40 {
        let fit = faculty
            .fit_messages(
                PromptMessages {
                    result_pointers: Vec::new(),
                    input_identity: [0; 32],
                    grounding_tokens: 0,
                    grounding_at: 0,
                    history: msgs[..n].to_vec(),
                    stimulus: None,
                    latest_result: Vec::new(), action_statement: None,
                    room_updates: Vec::new(), ledger: None, notices: Vec::new(),
                },
                budget,
            )
            .expect("whole history suffix fits")
            .messages;
        assert!(!fit.is_empty());
        let first = fit.first().map(|m| m.content_text());
        if prev_first.is_some() && first != prev_first {
            moves += 1;
        }
        prev_first = first;
        last_fit = fit;
    }
    assert!(
        (1..=8).contains(&moves),
        "front moved {moves} times across 20 acts — it must advance in \
         QUANTA (a handful of jumps), never per act, and must still \
         advance eventually (bounded memory)"
    );
    // Whole messages only at the front: the first fitted message is one
    // of the originals, never a mid-message shear.
    let first = last_fit.first().unwrap().content_text();
    assert!(
        msgs.iter().any(|m| m.content_text() == first),
        "front message must be a whole original message"
    );
    // CLUSTER GRANULARITY (Joel 9/1): the window must open on a clean
    // opener — when the quantized cut lands on an assistant answer or a
    // tool-result continuation, the front advances past the fragment.
    let mut clustered: Vec<ChatMessage> = Vec::new();
    for i in 0..30 {
        clustered.push(ChatMessage::text(
            "user",
            format!("q{i} {}", "word ".repeat(60)),
        ));
        clustered.push(ChatMessage::text(
            "assistant",
            format!("a{i} {}", "word ".repeat(60)),
        ));
    }
    let fit = faculty
        .fit_messages(
            PromptMessages {
                result_pointers: Vec::new(),
                input_identity: [0; 32],
                grounding_tokens: 0,
                grounding_at: 0,
                history: clustered,
                stimulus: None,
                latest_result: Vec::new(), action_statement: None,
                room_updates: Vec::new(), ledger: None, notices: Vec::new(),
            },
            budget,
        )
        .expect("whole history suffix fits")
        .messages;
    assert_eq!(
        fit.first().unwrap().role,
        "user",
        "the window must never open on an answer without its question"
    );
    // The fitted suffix honors the budget.
    assert!(
        LlmDeliberationFaculty::messages_cost(&last_fit) <= budget,
        "fitted cost {} exceeds budget {budget}",
        LlmDeliberationFaculty::messages_cost(&last_fit)
    );
    // A quantum larger than the optional history must not evict every
    // message on a quiet self-tick with no external stimulus or result.
    let ambient = || PromptMessages {
        result_pointers: Vec::new(),
        input_identity: [0; 32],
        grounding_tokens: 0,
        grounding_at: 0,
        history: vec![
            ChatMessage::text("user", "old situation ".repeat(30)),
            ChatMessage::text("user", "current situation ".repeat(20)),
        ],
        stimulus: None,
        latest_result: Vec::new(), action_statement: None,
        room_updates: Vec::new(), ledger: None, notices: Vec::new(),
    };
    let ambient_cost =
        LlmDeliberationFaculty::message_cost(ambient().history.last().unwrap());
    let fitted = faculty
        .fit_messages(ambient(), ambient_cost)
        .unwrap()
        .messages;
    assert_eq!(fitted.len(), 1);
    assert_eq!(fitted[0].content_text(), "current situation ".repeat(20));
    assert!(faculty.fit_messages(ambient(), ambient_cost - 1).is_err());
}

// what this catches: the steps ledger placed where its per-act change forfeits the
// stable prefix (card 5b09111e), or placed after the ask where it gets parroted (the
// 2026-07-20 humaneval fix). With a pinned result it rides after the room updates and
// the result stays last; with none it keeps its place ahead of the ask.
#[test]
fn the_steps_ledger_rides_before_the_pinned_result_and_never_after_the_ask() {
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let faculty = LlmDeliberationFaculty::new(Uuid::new_v4(), "T", "You are T.", adapter);
    let prompt = |result: bool| PromptMessages {
        result_pointers: Vec::new(),
        input_identity: [0; 32], grounding_tokens: 0, grounding_at: 0,
        history: vec![ChatMessage::text("user", "[work turn] fix the bug")],
        stimulus: Some(ChatMessage::text("user", "the ask")),
        latest_result: result.then(|| ChatMessage::text("user", "Full result of your most recent action")).into_iter().collect(), action_statement: None,
        room_updates: vec![ChatMessage::text("user", "[Room message received during this turn] hi")],
        notices: Vec::new(),
        ledger: Some(ChatMessage::text("user", "[steps taken this session]\n[action #1832] code/read")),
    };
    let order = |result: bool| -> Vec<String> {
        faculty.fit_messages(prompt(result), 100_000).expect("test: fits").messages.iter()
            .map(|m| m.content_text().lines().next().unwrap_or("").to_string()).collect()
    };
    assert_eq!(order(true), vec![
        "[work turn] fix the bug", "the ask", "[Room message received during this turn] hi",
        "[steps taken this session]", "Full result of your most recent action",
    ]);
    assert_eq!(order(false), vec![
        "[work turn] fix the bug", "[steps taken this session]", "the ask",
        "[Room message received during this turn] hi",
    ]);
}

// Regression for #4290: a follow-up must not erase protection for the act it asks about.
#[test]
fn continuity_followup_keeps_completed_action_evidence() {
    let history = vec![
        ChatMessage::text("user", "Write the requested file."),
        ChatMessage::text("assistant", "calling code/write: completed.py"),
        ChatMessage::text("user", "Full result of code/write: file written successfully."),
        ChatMessage::text("user", "Now review the file you just wrote."),
    ];
    let expected = LlmDeliberationFaculty::messages_cost(&history);
    let prompt = PromptMessages {
        result_pointers: Vec::new(),
        input_identity: [0; 32], grounding_tokens: 0, grounding_at: 0,
        history, stimulus: None, latest_result: Vec::new(), action_statement: None, room_updates: Vec::new(), ledger: None, notices: Vec::new(),
    };
    assert!(prompt.recent_move_evidence().tokens >= expected,
        "a new follow-up cannot make the preceding action and result disposable");
}

// Regression for #4290: insufficient capacity must refuse, not silently forget a completed act.
#[test]
fn continuity_insufficient_capacity_refuses_instead_of_dropping_evidence() {
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let faculty = LlmDeliberationFaculty::new(Uuid::new_v4(), "T", "You are T.", adapter);
    let prompt = PromptMessages {
        result_pointers: Vec::new(),
        input_identity: [0; 32], grounding_tokens: 0, grounding_at: 0,
        history: vec![
            ChatMessage::text("user", "Write the requested file. ".repeat(30)),
            ChatMessage::text("assistant", "calling code/write: completed.py ".repeat(30)),
            ChatMessage::text("user", "Full result of code/write: file written successfully."),
        ],
        stimulus: None, latest_result: Vec::new(), action_statement: None, room_updates: Vec::new(), ledger: None, notices: Vec::new(),
    };
    let insufficient = prompt.recent_move_evidence().tokens - 1;
    assert!(faculty.fit_messages(prompt, insufficient).is_err(),
        "required action/result continuity must not become optional when it exceeds capacity");
}

// what this catches: THE CLIP TAKING AWAY THE EVIDENCE OF WHAT SHE JUST DID.
//
// The chain, measured on the M5 2026-09-20: the conversation assembly is
// sophisticated, a flat cap lands on top of it (`delib.fill.latency_capped
// persona=Aiko window_budget=56227 cap=15000`), the drop walks up the history by
// COST until it reaches the run at the end — her act and its result — and she
// loses the record of her own last moves. She re-issues them:
// `persona.act.repeat_short_circuited` ("identical act already satisfied this
// turn"), then `persona.act.no_deliverable_yet`, then an hour with zero writes
// that reads as a lazy persona. A budget that binds must drop by recency-of-
// consequence, and her own last move is the LAST thing to go.
#[test]
fn a_binding_fill_budget_keeps_her_own_last_move_and_its_result() {
    // card e370a673: a degraded serving snapshot must not erase the
    // captured request model's measured fill allowance on a retry.
    use crate::inference::prefill_rate::{MeasuredRate, RateSource};
    let measured = MeasuredRate { tps: Some(28.4), source: RateSource::Fresh };
    let (cap, retained) = LlmDeliberationFaculty::latency_fill_cap(Some("bound-model"), |model| {
        assert_eq!(model, "bound-model", "look up the request binding, not current serving readiness");
        measured
    });
    assert_eq!(retained, measured);
    assert_eq!(cap, (PREFILL_TARGET_SECONDS as f64 * 28.4).floor() as usize);
    let (_, unbound) = LlmDeliberationFaculty::latency_fill_cap(None, |_| {
        panic!("an absent request model must not borrow another active model's rate")
    });
    assert_eq!(unbound, MeasuredRate::UNKNOWN);
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let faculty = LlmDeliberationFaculty::new(Uuid::new_v4(), "T", "You are T.", adapter);
    let mut history: Vec<ChatMessage> = (0..40)
        .map(|i| ChatMessage::text("user", format!("old{i} {}", "word ".repeat(95))))
        .collect();
    history.push(ChatMessage::text("user", format!("please write the file {}", "word ".repeat(40))));
    history.push(ChatMessage::text("assistant", format!("calling code/write {}", "word ".repeat(40))));
    history.push(ChatMessage::text("user", format!("Full result of code/write {}", "word ".repeat(60))));
    let prompt = || PromptMessages {
        result_pointers: Vec::new(),
        input_identity: [0; 32],
        grounding_tokens: 0,
        grounding_at: 0,
        history: history.clone(),
        stimulus: None,
        latest_result: Vec::new(), action_statement: None,
        room_updates: Vec::new(), ledger: None, notices: Vec::new(),
    };

    // The run that must survive: her act, its result, and the ask that opened them.
    let evidence = prompt().recent_move_evidence();
    assert_eq!(evidence.messages, 3, "act + result + the turn that asked for it");
    assert!(evidence.tokens > 0);

    // A derived cap far below that evidence is RAISED to it — re-issuing an act
    // she already ran costs a whole extra turn, which is the opposite of what a
    // latency budget is for.
    let starved = 10usize;
    let budget = prompt().required_tokens() + starved.max(evidence.tokens);
    assert_eq!(budget, evidence.tokens, "the evidence floors the cap");

    let fitted = faculty
        .fit_messages(prompt(), budget)
        .expect("the floored budget fits")
        .messages;
    assert!(
        fitted.iter().any(|m| m.content_text().starts_with("Full result of code/write")),
        "the RESULT of her last act must survive a binding budget"
    );
    assert!(
        fitted.iter().any(|m| m.content_text().starts_with("calling code/write")),
        "…and the act itself"
    );
    assert!(
        fitted.iter().any(|m| m.content_text().starts_with("please write the file")),
        "…and the ask, so the act is never an answer without its question"
    );
    assert!(
        !fitted.iter().any(|m| m.content_text().starts_with("old0 ")),
        "the OLDEST turns are what a binding budget spends"
    );
    assert!(
        LlmDeliberationFaculty::messages_cost(&fitted) <= budget,
        "protecting her last move must never push the prompt over the window"
    );

    // The clip is LOUD: it says what it cost her, in kind and amount, so "she
    // repeated herself" is traceable without opening a capture file.
    let shed = prompt().shed_preview(budget);
    assert!(shed.messages > 0 && shed.tokens > 0, "a binding clip drops something");
    assert_eq!(
        (shed.own_moves, shed.results),
        (0, 0),
        "what it sheds is old conversation — never her own move or its result"
    );

    // When the budget cannot hold the act and its result at all, the turn
    // REFUSES rather than forgetting a completed act — see
    // `continuity_insufficient_capacity_refuses_instead_of_dropping_evidence`.
    let last_cost = LlmDeliberationFaculty::message_cost(
        history.last().expect("the history ends with her act's result"),
    );
    assert!(
        faculty.fit_messages(prompt(), last_cost).is_err(),
        "a budget too small for her completed act is a capacity error, not a \
         silent forgetting"
    );
}

// what this catches: the LIMITS of the recent-move protection, pinned so this
// change cannot read as if recent-action loss is closed.
//
// The behaviour itself is specified by Astra's two regressions above
// (`continuity_followup_keeps_completed_action_evidence`,
// `continuity_insufficient_capacity_refuses_instead_of_dropping_evidence`); this
// test is deliberately NOT a parallel version of them. It pins the two edges the
// heuristic still has: a tool chain longer than the scan bound is protected only
// as a SUFFIX and says so, and a move further back than the bound is not
// protected at all. Source-owned causal grouping is what closes those.
#[test]
fn the_recent_move_protection_states_its_own_limits() {
    let msg = |role: &str, body: &str| ChatMessage::text(role, body.to_string());
    let of = |history: Vec<ChatMessage>| PromptMessages {
        result_pointers: Vec::new(),
        input_identity: [0; 32],
        grounding_tokens: 0,
        grounding_at: 0,
        history,
        stimulus: None,
        latest_result: Vec::new(), action_statement: None,
        room_updates: Vec::new(), ledger: None, notices: Vec::new(),
    };

    // A MULTI-TOOL CHAIN LONGER THAN THE SCAN BOUND. Protected as a suffix, and
    // SAID to be one: no opener is added, because the message before the cut is
    // another link in the chain, not the turn that opened it.
    let mut chain = vec![msg("user", "do the whole migration")];
    for i in 0..5 {
        chain.push(msg("assistant", &format!("calling tool {i}")));
        chain.push(msg("user", &format!("Full result of tool {i}")));
    }
    let evidence = of(chain).recent_move_evidence();
    assert_eq!(
        evidence.messages,
        LlmDeliberationFaculty::CLUSTER_SCAN_MESSAGES,
        "a chain longer than the bound yields a bounded suffix"
    );
    assert!(
        evidence.truncated,
        "and it must SAY it is a suffix — a partial protection that reads as a \
         whole one is how a limit becomes an invisible loss"
    );

    // A MOVE FURTHER BACK THAN THE SCAN BOUND IS NOT PROTECTED. Stated, not
    // hidden: this is the heuristic's edge, and the receipt carries a zero.
    let mut buried = vec![msg("assistant", "calling code/write")];
    for i in 0..LlmDeliberationFaculty::CLUSTER_SCAN_MESSAGES + 1 {
        buried.push(msg("user", &format!("unrelated room turn {i}")));
    }
    let none = of(buried).recent_move_evidence();
    assert_eq!(
        none,
        RecentMove::default(),
        "beyond the scan bound this protection does not reach — source-owned \
         causal grouping is what closes that, not a longer sniff"
    );
    assert_eq!(
        none.fit_within(1_000),
        EvidenceFit::NoMove,
        "no move is not a cannot-fit — there is nothing to lose, and the two \
         must never be conflated on the receipt"
    );

    // AN OVERSIZED RESULT IS A CONDENSE CASE, NOT A CAPACITY FAULT — the one
    // place this deliberately does NOT refuse. Refusing when a single message
    // cannot be carried whole does not preserve it; it mutes her. That exact
    // refusal is a measured outage: an unbounded required result produced 836
    // refusals and zero acts on 2026-09-10, and the substrate's answer was to cap
    // the result at its share of the window and demote the remainder to history —
    // which is the message that lands back here. Only the REDUCIBLE case refuses
    // (`continuity_insufficient_capacity_refuses_instead_of_dropping_evidence`,
    // where every message would individually have fit).
    let huge = of(vec![
        msg("user", "read the file"),
        msg("assistant", "calling code/read"),
        msg(
            "user",
            &format!("Full result of code/read {}", "x".repeat(60_000)),
        ),
    ]);
    let evidence = huge.recent_move_evidence();
    assert_eq!(
        evidence.fit_within(evidence.largest_message - 1),
        EvidenceFit::TooLargeToCarry,
        "a message larger than the whole budget cannot be carried by any policy \
         — that wants condensing, not a fault that mutes the turn"
    );
    assert_eq!(
        evidence.fit_within(evidence.tokens),
        EvidenceFit::Protected,
        "and it is Protected the moment the budget can hold it"
    );
}

// what this catches: the rent ledger's spine (compression-ladder rung
// 2). The segment map must (1) open with the system run, (2) merge
// consecutive same-label messages into one run, (3) classify bracketed
// blocks and act results as grounding, and (4) let a cached-token
// frontier name the run it died in — the attribution every miss
// becomes and the number the dream curriculum reads.
#[test]
fn segment_map_names_the_run_a_reuse_frontier_dies_in() {
    let msgs = vec![
        ChatMessage::text("user", "Alice: how goes the fix?"),
        ChatMessage::text("assistant", "landed it, running tests"),
        ChatMessage::text(
            "user",
            "[occurred 2023-11-14T22:13:20.123Z] Alice: prior status",
        ),
        ChatMessage::text("assistant", "[occurrence time unknown] Earlier own report."),
        ChatMessage::text("user", "[recall]\n(my memories …)"),
        ChatMessage::text("user", "Full result of your most recent action (#3): ok"),
        ChatMessage::text("user", "Alice: and the grade?"),
    ];
    let map = segment_map("SYSTEM ".repeat(100).as_str(), &msgs);
    let labels: Vec<&str> = map.iter().map(|(l, _)| *l).collect();
    assert_eq!(
        labels,
        vec!["system", "history", "grounding", "history"],
        "runs merge by label in send order: {map:?}"
    );
    assert!(
        map.windows(2).all(|w| w[0].1 < w[1].1),
        "cumulative ends are strictly increasing: {map:?}"
    );
    // A frontier inside the grounding run attributes to grounding.
    let frontier = map[1].1 + 1; // just past history's end
    let broke = map
        .iter()
        .find(|(_, end)| *end > frontier)
        .map(|(l, _)| *l)
        .unwrap_or("fully-warm");
    assert_eq!(broke, "grounding");
    // A frontier past everything is fully warm.
    let past = map.last().unwrap().1 + 1;
    assert_eq!(
        map.iter().find(|(_, end)| *end > past).map(|(l, _)| *l),
        None,
        "reuse past every segment attributes to nothing — fully warm"
    );
}

// what this catches: e731576c — repetition remains observable without
// deleting distinct speech events or replacing them with authored words
// in the citizen's own voice.
#[test]
fn near_duplicate_own_turns_remain_visible_with_repetition_evidence() {
    use crate::cognition::workspace::Burst;
    let persona = Uuid::new_v4();
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let faculty = LlmDeliberationFaculty::new(
        persona,
        "Casper",
        "You are Casper, a thoughtful engineer on the grid.",
        adapter,
    )
    .with_context_window(8192);

    // Byte-DISTINCT variants of one template (the live specimen's shape) —
    // all pairwise ≥ NEAR_DUP_JACCARD, none byte-equal.
    let v1 = "I apologize for the repetition. Let's focus on the task \"wordstats\". What approach would you like to take for this task?";
    let v2 = "I apologize for the repetition earlier. Let's focus on the task \"wordstats\". What approach would you like to take for this task?";
    let v3 = "I apologize for repeating myself earlier. Let's focus on the task \"wordstats\". What approach would you like to take for this task?";
    let turns = vec![
        BurstTurn::attributed(true, "Casper", v1, Some(1)),
        BurstTurn::attributed(
            false,
            "Anwen",
            "any specific topics you'd like to work on?",
            Some(2),
        ),
        BurstTurn::attributed(true, "Casper", v2, Some(3)),
        BurstTurn::attributed(false, "Atlas", "shall we outline the steps first?", Some(4)),
        BurstTurn::attributed(true, "Casper", v3, Some(5)),
    ];
    let ws = Workspace::new(Burst::from_turns(
        crate::identity::ActivityRoom::mint(),
        turns,
    ));
    let view = faculty.prompt_view(&ws);

    // All three actual statements survive in their original order.
    let apology_renders = view
        .messages
        .iter()
        .filter(|m| m.role == "assistant" && m.content_text().contains("I apologize"))
        .count();
    assert_eq!(
        apology_renders, 3,
        "similarity must not erase actual speech: {view:?}"
    );
    let own: Vec<_> = view
        .messages
        .iter()
        .filter(|message| message.role == "assistant")
        .map(|message| message.content_text())
        .collect();
    assert_eq!(
        own,
        [v1, v2, v3]
            .iter()
            .enumerate()
            .map(|(index, body)| format!(
                "[occurred 1970-01-01T00:00:00.{:03}Z] {body}",
                index * 2 + 1
            ))
            .collect::<Vec<_>>()
    );

    // The same raw history still supports the repetition observation.
    let all_user: String = view
        .messages
        .iter()
        .filter(|m| m.role == "user")
        .map(|m| m.content_text())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        all_user.contains("[repetition] 3 of your recent messages were nearly identical"),
        "raw-turn repetition fact must remain available: {all_user}"
    );
}

// what this catches: [[idle-is-self-directed-free-time]] Layer 1. A self-initiated
// turn must frame REST / turning-attention-elsewhere as a CO-EQUAL legitimate
// outcome ("not a failure to find something"), not only active pursuit — otherwise a
// quiet heartbeat reads as pressure to manufacture activity (the self-tick analogue
// of the polite-filler loop). It must stay NEUTRAL ("yours alone; nothing here is
// telling you which to pick") per [[no-hardcoded-heuristics-to-steer-cognition]], and
// active options must remain FIRST/primary so this never becomes the always-PASS
// doom-loop documented in SILENCE_AFFORDANCE_BLOCK. The block is gated on
// self_initiated — an inbound-driven (ambient/directed) turn must NOT carry it.
#[test]
fn self_initiated_turn_frames_rest_as_co_equal_and_stays_neutral() {
    let persona = Uuid::new_v4();
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let faculty = LlmDeliberationFaculty::new(persona, "Asha", "You are Asha.", adapter)
        .with_context_window(8192);

    // self_initiated = true, undirected.
    let own_time = faculty.compose_system("", &BTreeSet::new(), false, true, None);
    assert!(
        own_time.contains("[Your own time]"),
        "self-initiated turn must carry the own-time framing: {own_time}"
    );
    // Rest / turning-elsewhere is named as legitimate, not a deficiency.
    assert!(
        own_time.contains("not a failure to find something"),
        "rest must be framed as a real choice, not a failure: {own_time}"
    );
    // Neutral — names the option, never scripts when to take it.
    assert!(
        own_time.contains("yours alone; nothing here is telling you which to pick"),
        "the choice must stay the persona's own, uncoached: {own_time}"
    );
    // Active options stay FIRST/primary — the pursue-your-thread framing appears
    // BEFORE the rest framing, so rest is the co-equal alternative, not the lead.
    let pursue = own_time
        .find("Pick up your own train of thought")
        .expect("active framing present");
    let rest = own_time
        .find("do not have to fill the moment")
        .expect("rest framing present");
    assert!(
        pursue < rest,
        "active options must lead; rest is the co-equal alternative: {own_time}"
    );

    // A non-self-initiated (inbound-driven) turn must NOT carry the own-time block.
    let ambient = faculty.compose_system("", &BTreeSet::new(), false, false, None);
    assert!(
        !ambient.contains("[Your own time]"),
        "ambient/directed turns must not carry the own-time framing: {ambient}"
    );
}

// what this catches: the category index is small BY CONSTRUCTION — even a
// 120-tool registry at the 2048-token serving floor (MIN_SERVE_CTX) renders to a
// handful of `category (N)` entries, so the framing never alone blows past the
// window and always leaves burst room. A regression (e.g. dumping tool names back
// into the index) reintroduces the overflow at a tiny window.
#[test]
fn catalog_fits_the_minimum_serving_window() {
    let persona = Uuid::new_v4();
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
    let window = crate::inference::planning::MIN_SERVE_CTX; // 2048 floor
    let tools: Vec<NativeToolSpec> = (0..120)
        .map(|i| NativeToolSpec {
            name: format!("cat{}/command_{i}", i % 8),
            description: "A registry command with a one-line summary for the catalog."
                .to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: json!({ "path": { "type": "string" } }),
                required: Some(vec!["path".to_string()]),
                definitions: None,
            },
        })
        .collect();
    let faculty = LlmDeliberationFaculty::new(persona, "Asha", "You are Asha.", adapter)
        .with_context_window(window)
        .with_tools(tools);

    // The menu (inside framing) plus the completion reserve must leave room for at
    // least SOME burst — i.e. framing alone must not consume the window. With
    // nothing expanded (empty context) the menu is its smallest — a spine of
    // collapsed bookmarks — which is the floor case for this fit guard.
    let collapsed = faculty.compose_system("", &BTreeSet::new(), false, false, None);
    let framing = est_tokens(&collapsed);
    // The reply's floor — what the window always holds for it — derived from the
    // claim itself, never re-spelled. Above it the reply grows by priority.
    let reserve = reply_claim(TurnKind::Pass, None, 0, false, false).0.floor_tokens as usize;
    assert!(
        framing + reserve < window as usize,
        "framing+menu ({framing}) + reserve ({reserve}) must leave burst room in {window}"
    );
    // The SPINE names every category even when collapsed (`cat0 (15 — commands/list
    // --filter cat0)`) — she always sees the full map and can open any category on
    // demand. What the menu must NOT carry is the per-tool SUMMARY — that one-line
    // description × ~150 tools was the 18KB bloat.
    assert!(
        collapsed.contains("cat0 (15 — commands/list --filter cat0)"),
        "the spine must name each category as a collapsed bookmark: {collapsed}"
    );
    assert!(
        !collapsed.contains("one-line summary for the catalog"),
        "per-tool SUMMARIES must NOT be in the menu (that was the 18KB bloat)"
    );
    // And an EXPANDED category DOES list its verbs (the fix: she must SEE that a
    // tool exists to call it — glass-box: hidden names → 909 code-fences / 3 native
    // runs). Names ride the expansion, summaries never do.
    let opened = faculty.compose_system(
        "",
        &BTreeSet::from(["cat0".to_string()]),
        false,
        false,
        None,
    );
    // Bare verb names since the 2026-07-10 prompt diet — args live in
    // commands/help + the #1916 inline error-manual, not an 8k menu wall.
    assert!(
        opened.contains("cat0: command_0"),
        "an expanded category lists its verbs so she can see them: {opened}"
    );
}
// ─── Stable prefix first (card c119ace7) ───────────────────────────────
//
// The prompt cache reuses only a common PREFIX. These pin the whole-prompt
// order at the live boundary (`prompt_view`): conversation first, then the
// standing grounding whose bytes mutate, in ONE canonical order; the clock and
// the presence framing in the volatile tail, after the conversation; nothing
// dated or counted in the system message.
mod stable_prefix_first {
    use super::*;
    use crate::cognition::workspace::Burst;

    fn bodies(v: &DeliberationPromptView) -> Vec<String> {
        v.messages.iter().map(|m| m.content_text()).collect()
    }

    fn at(v: &DeliberationPromptView, needle: &str) -> usize {
        v.messages
            .iter()
            .position(|m| m.content_text().starts_with(needle))
            .unwrap_or_else(|| panic!("{needle} missing from:\n{:#?}", bodies(v)))
    }

    // what this catches (card c119ace7): the standing grounding that rides the
    // conversation tail rendered in BROADCAST order — the arbiter's per-turn
    // salience — so `[workspace-map]`, `[active-work]` and `[room-kanban]` traded
    // places turn to turn (both orders are in the M5's 2026-09-20 captures), and
    // the prefix cache lost every byte behind the swap although none had changed.
    // Same set in, same bytes out, whatever attention ranked and whatever order
    // the faculties bid in; and the order is the churn order, standing → board.
    #[test]
    fn standing_grounding_renders_in_one_canonical_order_and_the_clock_rides_last() {
        let adapter: Arc<dyn AIProviderAdapter> = Arc::new(HeuristicInferenceAdapter::new());
        let faculty =
            LlmDeliberationFaculty::new(Uuid::new_v4(), "Ivar", "You are Ivar.", adapter)
                .with_context_window(16_384);
        let room_uuid = Uuid::new_v4();
        let view_for = |order: [usize; 4], sal: [f32; 4]| {
            let bodies = [
                ("workspace-map", "MAP: src/ tests/ docs/"),
                ("active-work", "HELD: card 1234 [InProgress] \"fix the thing\""),
                ("room-kanban", "BOARD: you hold 1 card here; 3 claimable"),
                ("room-wall", "WALL: card 1234 ledger — known: the repro is in tests/"),
            ];
            let room = crate::identity::ActivityRoom::from_uuid(room_uuid).unwrap();
            let turns = vec![
                BurstTurn::attributed(
                    false,
                    "Operator",
                    "we shipped the fix yesterday",
                    Some(1_700_000_000_000),
                ),
                BurstTurn::attributed(true, "Ivar", "I saw it land.", Some(1_700_000_060_000)),
                BurstTurn::attributed(
                    false,
                    "Operator",
                    "what's next on the board?",
                    Some(1_700_000_120_000),
                ),
            ];
            let mut ws = Workspace::new(Burst::from_turns(room, turns));
            ws.now_ms = Some(1_700_000_180_000);
            for &i in order.iter() {
                ws.broadcast.push(
                    Contribution::context(
                        FacultyId::Custom(bodies[i].0.to_string()),
                        bodies[i].1,
                        sal[i],
                        "framing",
                    )
                    .standing_grounding()
                    .trailing(),
                );
            }
            faculty.prompt_view(&ws)
        };

        let a = view_for([0, 1, 2, 3], [0.9, 0.5, 0.3, 0.7]);
        // Re-ranked AND re-ordered arrival: the bytes must not move.
        let b = view_for([3, 2, 0, 1], [0.3, 0.9, 0.7, 0.5]);
        assert_eq!(a.system, b.system, "the system prefix is a function of the persona + ground");
        assert_eq!(
            bodies(&a),
            bodies(&b),
            "a re-ranked, re-ordered standing grounding must emit IDENTICAL messages"
        );

        // The order, most stable first: the conversation (dated history) leads;
        // then the standing grounding in churn order (STANDING map, then the
        // BOARD's held card and wall, by name, then the kanban's CLAIMS, which
        // change fastest of the three); then the per-turn facts;
        // then the clock + presence framing; then the ask, last.
        assert!(
            a.messages[0].content_text().starts_with("[occurred "),
            "the conversation leads the message list — nothing volatile ahead of it:\n{:#?}",
            bodies(&a)
        );
        let history_last = at(&a, "[occurred 2023-11-14T22:14:20.000Z] I saw it land.");
        let map = at(&a, "[workspace-map]");
        let held = at(&a, "[active-work]");
        let board = at(&a, "[room-kanban]");
        let wall = at(&a, "[room-wall]");
        let facts = at(&a, "[context]");
        let clock = at(&a, "[now ");
        let ask = at(&a, "[occurred 2023-11-14T22:15:20.000Z] Operator: what's next");
        assert!(
            history_last < map
                && map < held
                && held < wall
                && wall < board
                && board < facts
                && facts < clock
                && clock < ask,
            "order must be history < map < active-work < wall < kanban < facts < clock \
             < ask, got {history_last} {map} {held} {wall} {board} {facts} {clock} \
             {ask}:\n{:#?}",
            bodies(&a)
        );

        // No timestamp, no counter and no BOARD-churn block ahead of the
        // conversation: the clock is in the volatile tail, the wall and the board
        // ride behind the history, and the system message carries neither a date
        // nor the turn count.
        for needle in [
            "[now ",
            "2023-11",
            "input turns were available",
            "[room-wall]",
            "[room-kanban]",
        ] {
            assert!(
                !a.system.contains(needle),
                "`{needle}` must not sit in the cacheable system message:\n{}",
                a.system
            );
        }
        assert!(
            a.messages[..=history_last]
                .iter()
                .all(|m| m.content_text().starts_with("[occurred ")),
            "every message up to the end of history IS history — no clock, no fact, no \
             grounding ahead of the conversation:\n{:#?}",
            bodies(&a)
        );
    }
}
