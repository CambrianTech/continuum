#![cfg(test)]

use super::*;
use crate::cognition::workspace::TurnFraming;
use crate::cognition::workspace::WorkspaceCycle;
use crate::cognition::workspace::Faculty;
use crate::cognition::workspace::SalienceArbiter;
use crate::ai::types::ToolCall;
use uuid::Uuid;
use std::sync::Arc;
use std::sync::Mutex;
use crate::cognition::working_memory::WorkingMemory;
use crate::cognition::working_memory::WorkingMemoryFaculty;

// what this catches:41f4e3ef — a directed drive must persist real receipt-linked
// progress before settlement; cancellation/rollback cannot lose the last whole
// snapshot or attach another card's work to this turn's identity.
#[tokio::test]
async fn directed_credit_survives_partial_cancellation_and_atomic_retry() {
    use crate::ai::heuristic_adapter::HeuristicInferenceAdapter;
    use crate::ai::types::{
        FinishReason, TextGenerationRequest, TextGenerationResponse, UsageMetrics,
    };
    use crate::cognition::llm_deliberation_faculty::LlmDeliberationFaculty;
    use crate::persona::scripted_conversation::ScriptedConversation;
    use crate::persona::service_loop::PersonaConversation;
    use crate::persona::training_producer::{
        tests::turn_capture, CapturedCredit, StagedCredit,
    };

    let home = tempfile::tempdir().unwrap();
    let _home = crate::test_env::HomeGuard::set(home.path()).await;
    crate::persona::register_substrate_orm_entities(crate::orm::OrmEntityRegistry::global())
        .unwrap();
    let entered = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let mut hands = ScriptedExecutor::new([
        "read one",
        "read two",
        "read three",
        "read four",
        "read five",
    ]);
    hands.pause_before = Some((5, Arc::clone(&entered), release));
    let wm = Arc::new(WorkingMemory::new(8));
    wm.set_served_window(32_768);
    let body = body_with_wm(Arc::new(hands), admission(), Arc::clone(&wm));
    let persona = body.persona_id;
    let responses = (0..5)
        .map(|n| TextGenerationResponse {
            text: String::new(),
            finish_reason: FinishReason::ToolUse,
            model: "actual-served-model".into(),
            provider: "fixture".into(),
            usage: UsageMetrics::default(),
            response_time_ms: 0,
            request_id: format!("provider-{n}"),
            content: None,
            tool_calls: Some(vec![ToolCall {
                id: format!("call-{n}"),
                name: "code/read".into(),
                input: serde_json::json!({"path":format!("src/{n}.rs")}),
            }]),
            reasoning: None,
            routing: None,
            error: None,
            timing: None,
        })
        .collect();
    let recorded = Arc::new(Mutex::new(Vec::<TextGenerationRequest>::new()));
    let adapter = Arc::new(
        HeuristicInferenceAdapter::new()
            .with_responses(responses)
            .with_request_recorder(Arc::clone(&recorded)),
    );
    let faculty = LlmDeliberationFaculty::new(persona, "Asha", "Review the project.", adapter)
        .with_context_window(32_768)
        .with_working_memory(Arc::clone(&wm))
        .with_tools(vec![crate::ai::types::NativeToolSpec {
            name: "code/read".into(),
            description: "Read a workspace file".into(),
            input_schema: crate::ai::types::ToolInputSchema {
                schema_type: "object".into(),
                properties: serde_json::json!({ "path": { "type": "string" } }),
                required: Some(vec!["path".into()]),
                definitions: None,
            },
        }]);
    let cycle = WorkspaceCycle::new(
        vec![
            Arc::new(WorkingMemoryFaculty::new(wm)) as Arc<dyn Faculty>,
            Arc::new(faculty),
        ],
        Arc::new(SalienceArbiter),
        8,
    )
    .with_acting(body);
    let card = Uuid::new_v4();
    let mut capture = turn_capture(
        persona,
        "capture-owner".into(),
        "review first card".into(),
        CapturedCredit {
            task: crate::cognition::held_out::CardTask::NotATask,
            card_id: card,
            claim: None,
        },
    );
    let mut conversation = ScriptedConversation::new();
    conversation.prime().await.unwrap();
    let mut drive = Box::pin(drive_to_settle_with_credit(
        &cycle,
        "review first card".into(),
        8,
        TurnFraming::ambient(),
        &mut conversation,
        Some(&mut capture),
    ));
    tokio::select! {
        _ = entered.notified() => {},
        _ = &mut drive => panic!("driver must still be working after its fourth persisted act"),
        _ = tokio::time::sleep(std::time::Duration::from_secs(10)) => panic!("fifth act never reached its pause"),
    }
    let db = rusqlite::Connection::open_with_flags(
        home.path()
            .join(".continuum/personas/capture-owner/data/longterm.db"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let read = || -> StagedCredit {
        let text: String = db.query_row("SELECT json_object('id',id,'cardId',card_id,'claimId',claim_id,'owner',owner,'role',json(role),'receipts',json(receipts),'served',json(served),'prompt',prompt,'completion',completion,'stagedAtMs',staged_at_ms) FROM staged_credit WHERE card_id=?", [card.to_string()], |row| row.get(0)).unwrap();
        serde_json::from_str(&text).unwrap()
    };
    let partial = read();
    assert_eq!(
        partial.receipts.len(),
        4,
        "actual partial stage, not just final capture"
    );
    let expected: Vec<_> = recorded
        .lock()
        .unwrap()
        .iter()
        .take(4)
        .map(|r| r.request_id.clone().unwrap())
        .collect();
    assert_eq!(
        partial
            .receipts
            .iter()
            .map(|r| r.submitted_request_id.clone())
            .collect::<Vec<_>>(),
        expected
    );
    assert_eq!(
        partial.served.as_ref().unwrap().model,
        "actual-served-model"
    );
    for (n, receipt) in partial.receipts.iter().enumerate() {
        assert!(matches!(&receipt.outcome,
            crate::cognition::provenance::GenerationOutcome::Served { model, provider, provider_request_id }
                if model == "actual-served-model" && provider == "fixture"
                    && provider_request_id.as_deref() == Some(format!("provider-{n}").as_str())));
    }
    assert!(
        partial.claim_id.is_none(),
        "staging must not invent ownership or a verdict"
    );
    drop(drive); // Cancels the fifth act; the fourth-act durable row survives.
    assert_eq!(read().id, partial.id);
    let acts: Vec<_> = (0..8)
        .map(|n| {
            (
                format!("act {n}"),
                vec![ToolCall {
                    id: format!("a{n}"),
                    name: "code/read".into(),
                    input: serde_json::json!({"path":n}),
                }],
            )
        })
        .collect();
    let mut duplicate = partial.receipts.clone();
    duplicate.push(partial.receipts[0].clone());
    assert!(
        !capture.record(&acts, &duplicate, None, false).await,
        "duplicate child must roll back the replacement"
    );
    assert_eq!(
        read().receipts,
        partial.receipts,
        "rollback preserves the previous whole row"
    );
    let mut final_receipts = partial.receipts.clone();
    final_receipts.push(crate::cognition::provenance::GenerationReceipt::faulted(
        "fault-after-four",
        "transport refused",
    ));
    assert!(
        capture
            .record(&acts, &final_receipts, Some("honest final report"), true)
            .await
    );
    let finished = read();
    assert_ne!(
        finished.id, partial.id,
        "changed content gets a new immutable revision"
    );
    assert_eq!(finished.receipts, final_receipts);
    assert!(finished.completion.ends_with("honest final report"));
    assert!(
        capture
            .record(&acts, &final_receipts, Some("honest final report"), true)
            .await
    );
    assert_eq!(
        read().id,
        finished.id,
        "identical retry reuses its revision identity"
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM staged_credit_generation", [], |r| {
            r.get::<_, usize>(0)
        })
        .unwrap(),
        5
    );
    drop(capture);
    let other_card = Uuid::new_v4();
    let mut other = turn_capture(
        persona,
        "capture-owner".into(),
        "another activity".into(),
        CapturedCredit {
            task: crate::cognition::held_out::CardTask::NotATask,
            card_id: other_card,
            claim: None,
        },
    );
    assert!(other.record(&acts, &final_receipts, None, true).await);
    assert_eq!(
        read().id,
        finished.id,
        "same Persona's next turn cannot replace this card"
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM staged_credit", [], |r| r
            .get::<_, usize>(0))
            .unwrap(),
        2
    );
}

// what this catches: c5910be2 — an ordinary room drive could not see coaching
// until it settled. Inject through the conversation DURING a real tool action,
// then inspect the actual second/third adapter requests, not a prompt helper.
#[tokio::test]
async fn live_room_input_reaches_next_action_without_replacing_task_or_room() {
    use crate::ai::heuristic_adapter::HeuristicInferenceAdapter;
    use crate::ai::types::{
        FinishReason, TextGenerationRequest, TextGenerationResponse, UsageMetrics,
    };
    use crate::cognition::llm_deliberation_faculty::LlmDeliberationFaculty;
    use crate::cognition::workspace::{Burst, BurstTurn, Cause, TurnAttention};
    use crate::persona::scripted_conversation::ScriptedConversation;
    use crate::persona::service_loop::{IncomingMessage, PersonaConversation};

    for framing in [
        TurnFraming::message(TurnAttention::Addressed),
        TurnFraming::self_thread(false).on_workspace(),
    ] {
        let room = Uuid::new_v4();
        let other_room = Uuid::new_v4();
        let colleague = Uuid::new_v4();
        let messages = vec![
            IncomingMessage {
                work: None,
                media: Vec::new(),
                event_id: Uuid::new_v4(),
                lamport: 1,
                peer_id: colleague,
                text: "Asha, the review target moved to src/new.rs; preserve the regression."
                    .into(),
                room_id: room,
            },
            IncomingMessage {
                work: None,
                media: Vec::new(),
                event_id: Uuid::new_v4(),
                lamport: 1,
                peer_id: colleague,
                text: "Another activity has a question when you have attention available."
                    .into(),
                room_id: other_room,
            },
        ];
        let later_message = IncomingMessage {
            work: None,
            media: Vec::new(),
            event_id: Uuid::new_v4(),
            lamport: 2,
            peer_id: colleague,
            room_id: other_room,
            text: "Asha, newer evidence arrived during your second read; inspect src/final.rs."
                .into(),
        };
        let mut conversation = ScriptedConversation::new().require_prime_before_next_message();
        conversation.prime().await.unwrap();
        let exec = Arc::new(
            ScriptedExecutor::new([
                "FIRST_COMPLETE_TOOL_RESULT",
                "SECOND_COMPLETE_TOOL_RESULT",
            ])
            .with_incoming(conversation.event_feed(), messages.clone())
            .with_incoming(conversation.event_feed(), vec![later_message.clone()]),
        );
        let mut responses = Vec::new();
        for step in 0..3 {
            responses.push(TextGenerationResponse {
                text: if step == 2 {
                    "PASS".into()
                } else {
                    String::new()
                },
                finish_reason: if step == 2 {
                    FinishReason::Stop
                } else {
                    FinishReason::ToolUse
                },
                model: "scripted".into(),
                provider: "scripted".into(),
                usage: UsageMetrics::default(),
                response_time_ms: 0,
                request_id: format!("step-{step}"),
                content: None,
                tool_calls: (step < 2).then(|| {
                    vec![ToolCall {
                        id: format!("call-{step}"),
                        name: "code/read".into(),
                        input: serde_json::json!({ "path": format!("src/{step}.rs") }),
                    }]
                }),
                reasoning: None,
                routing: None,
                error: None,
                timing: None,
            });
        }
        let recorded = Arc::new(Mutex::new(Vec::<TextGenerationRequest>::new()));
        let adapter = Arc::new(
            HeuristicInferenceAdapter::new()
                .with_responses(responses)
                .with_request_recorder(Arc::clone(&recorded)),
        );
        let wm = Arc::new(WorkingMemory::new(8));
        wm.set_served_window(32_768);
        let adm = admission();
        let body = body_with_wm(exec.clone(), adm.clone(), Arc::clone(&wm));
        let persona = body.persona_id;
        *exec.pending_persona.lock().unwrap() = Some(persona);
        let faculty =
            LlmDeliberationFaculty::new(body.persona_id, "Asha", "You are Asha.", adapter)
                .with_context_window(32_768)
                .with_working_memory(Arc::clone(&wm))
                .with_tools(vec![crate::ai::types::NativeToolSpec {
                    name: "code/read".into(),
                    description: "Read a workspace file".into(),
                    input_schema: crate::ai::types::ToolInputSchema {
                        schema_type: "object".into(),
                        properties: serde_json::json!({ "path": { "type": "string" } }),
                        required: Some(vec!["path".into()]),
                        definitions: None,
                    },
                }]);
        let cycle = WorkspaceCycle::new(
            vec![
                Arc::new(WorkingMemoryFaculty::new(wm)) as Arc<dyn Faculty>,
                Arc::new(faculty),
            ],
            Arc::new(SalienceArbiter),
            8,
        )
        .with_acting(body);
        let task = "Review the original src/lib.rs change and report a reproducible defect.";
        let cause = Cause::Stimulus(Uuid::new_v4());
        let burst = Burst::from_turns_at(
            crate::identity::ActivityRoom::from_uuid(room).unwrap(),
            vec![BurstTurn::attributed(
                false,
                colleague.to_string(),
                task,
                Some(1),
            )],
            Some(1),
            cause,
        );
        let original_world = burst.rendered.clone();
        let outcome =
            drive_to_settle_with_input(&cycle, burst, 8, framing, &mut conversation).await;
        assert!(
            outcome.inference_error.is_none(),
            "{:?}",
            outcome.inference_error
        );
        assert_eq!(outcome.acts, 2);
        assert_eq!(outcome.room, room);
        assert_eq!(outcome.world_state, original_world);
        assert_eq!(outcome.room_updates.len(), 3);
        let lived =
            crate::cognition::experience::ExperienceRecord::from_lived_turn(task, &outcome);
        let encoded = serde_json::to_string(&lived).unwrap();
        let mut restored: crate::cognition::experience::ExperienceRecord =
            serde_json::from_str(&encoded).unwrap();
        assert_eq!(restored.task.prompt, task);
        assert_eq!(restored.world_state, original_world);
        assert_eq!(restored.room, Some(room));
        for update in messages.iter().chain(std::iter::once(&later_message)) {
            assert!(restored.room_updates.contains(update));
            assert!(restored
                .training_prompt()
                .contains(&update.render_room_update()));
        }
        // A later failed episode is taught with the same complete input,
        // through the ordinary lived-curriculum selector.
        restored.ok = false;
        let teaching = crate::commands::genome::curriculum::LivedExpansionSynthesizer::new()
            .select(std::slice::from_ref(&restored));
        assert_eq!(teaching, vec![restored.training_prompt()]);
        let mut legacy: serde_json::Value = serde_json::from_str(&encoded).unwrap();
        legacy.as_object_mut().unwrap().remove("room_updates");
        assert!(
            serde_json::from_value::<crate::cognition::experience::ExperienceRecord>(legacy)
                .unwrap()
                .room_updates
                .is_empty()
        );
        assert_eq!(*exec.rooms.lock().unwrap(), vec![room, room]);
        let engrams = adm.recall_recent(adm.engram_count());
        let first_act = engrams
            .iter()
            .find(|e| e.content.contains("FIRST_COMPLETE_TOOL_RESULT"))
            .expect("first action retained as an experience");
        let second_act = engrams
            .iter()
            .find(|e| e.content.contains("SECOND_COMPLETE_TOOL_RESULT"))
            .expect("second action retained as an experience");
        assert!(adm
            .engram_neighbors(&first_act.id)
            .iter()
            .any(|edge| Some(edge.target) == cause.root()
                && edge.kind == crate::persona::engram_graph::EdgeKind::CausedBy));
        assert!(adm
            .engram_neighbors(&second_act.id)
            .iter()
            .any(|edge| edge.target == first_act.id
                && edge.kind == crate::persona::engram_graph::EdgeKind::CausedBy));
        {
            let requests = recorded.lock().unwrap();
            assert_eq!(requests.len(), 3);
            for (index, request) in requests.iter().enumerate() {
                assert_eq!(request.room_id.as_deref(), Some(room.to_string().as_str()));
                // What the model receives: text AND native tool results (content_text reads
                // text parts only; the latest result rides as a tool_result part).
                let text = request
                    .messages
                    .iter()
                    .map(|m| match &m.content {
                        crate::ai::types::MessageContent::Parts(parts) => parts
                            .iter()
                            .map(|p| match p {
                                crate::ai::types::ContentPart::Text { text } => text.clone(),
                                crate::ai::types::ContentPart::ToolResult { content, .. } => content.clone(),
                                _ => String::new(),
                            })
                            .collect::<Vec<_>>()
                            .join(""),
                        crate::ai::types::MessageContent::Text(text) => text.clone(),
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                assert!(text.contains(task));
                if framing.self_initiated {
                    assert!(!text.contains("This message names you"),
                    "a priority input must not rewrite the original self-turn's addressing fact");
                }
                assert_eq!(
                    text.matches(&later_message.text).count(),
                    usize::from(index > 1)
                );
                for message in &messages {
                    assert_eq!(text.matches(&message.text).count(), usize::from(index > 0));
                    if index > 0 {
                        assert!(text.contains(&format!(
                            "room {}; peer {}; event {}",
                            message.room_id, colleague, message.event_id
                        )));
                    }
                }
                if index > 0 {
                    assert!(text.contains(if index == 1 {
                        "FIRST_COMPLETE_TOOL_RESULT"
                    } else {
                        "SECOND_COMPLETE_TOOL_RESULT"
                    }), "act {index}'s prompt lacks the previous complete result:\n{text}");
                }
            }
        }
        // Perception did not steal these inputs from later attention in their own
        // rooms. Each retained source is returned once by the ordinary driver.
        assert!(
            crate::cognition::directed_pending::is_pending(persona),
            "cooperative intake must never acknowledge the outer driver's shared wake flag"
        );
        for expected in messages.into_iter().chain(std::iter::once(later_message)) {
            assert_eq!(conversation.next_message().await.unwrap(), Some(expected));
        }
        assert!(conversation.next_message().await.unwrap().is_none());
        assert!(
            conversation.said().is_empty(),
            "intake must not impose a reply"
        );
        crate::cognition::directed_pending::clear(persona);
    }
}
