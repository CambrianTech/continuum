#![cfg(test)]

use super::*;
use crate::cognition::workspace::TurnFraming;
use crate::cognition::workspace::WorkspaceCycle;
use crate::cognition::workspace::SalienceArbiter;
use crate::ai::types::ToolCall;
use uuid::Uuid;
use std::sync::Arc;
use std::sync::Mutex;
use crate::cognition::working_memory::WorkingMemory;

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
    use crate::cognition::conversation::scripted_conversation::ScriptedConversation;
    use crate::cognition::conversation::{IncomingMessage, PersonaConversation};

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
                Arc::new(faculty),
            ],
            Arc::new(SalienceArbiter),
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
        // The driver's own outcome carries each room message (Cormac on #5385): the academy's
        // lived record is built from this outcome, so its half of the test can only be as
        // true as these.
        for update in messages.iter().chain(std::iter::once(&later_message)) {
            assert!(outcome.room_updates.iter().any(|u| **u == *update), "the driver's outcome lost a room update");
        }
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
                && edge.kind == crate::cognition::engram_graph::EdgeKind::CausedBy));
        assert!(adm
            .engram_neighbors(&second_act.id)
            .iter()
            .any(|edge| edge.target == first_act.id
                && edge.kind == crate::cognition::engram_graph::EdgeKind::CausedBy));
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
