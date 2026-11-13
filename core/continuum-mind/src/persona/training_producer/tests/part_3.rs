#![cfg(test)]
use super::*;

    /// what this catches: a staged example that is not the call she lived. The read
    /// must return the exact request she was served and the response she gave, with
    /// her reasoning and tool calls; a turn with any call that cannot be read back must
    /// say so rather than stage as complete; and each settled example must carry its
    /// call and the card stamp (card ad107e18).
    #[test]
    pub(super) fn a_staged_turn_is_read_back_as_the_calls_she_lived_and_a_hole_is_named() {
        use crate::ai::types::{ChatMessage, TextGenerationRequest, TextGenerationResponse};
        use crate::cognition::prompt_capture::{CaptureLease, JsonlPromptCaptureSink, PromptCall};
        let dir = tempfile::tempdir().expect("capture dir");
        let persona = Uuid::new_v4();
        let request = TextGenerationRequest {
            system_prompt: Some("her identity".into()),
            messages: vec![
                ChatMessage::text("user", "room: the build is red on lib"),
                ChatMessage::text("user", "card: fix the failing test"),
            ],
            ..Default::default()
        };
        let response = TextGenerationResponse {
            text: "reading the failing test first".into(),
            finish_reason: crate::ai::FinishReason::ToolUse,
            model: "served-model".into(),
            provider: "fixture-provider".into(),
            usage: crate::ai::UsageMetrics::default(),
            response_time_ms: 0,
            request_id: "req-lived".into(),
            content: None,
            tool_calls: Some(vec![crate::ai::ToolCall {
                id: "t1".into(),
                name: "code/read".into(),
                input: json!({"path": "src/lib.rs"}),
            }]),
            reasoning: Some("the assertion names a path; read it before guessing".into()),
            routing: None,
            error: None,
            timing: None,
        };
        let sink = JsonlPromptCaptureSink::open(dir.path(), persona).expect("sink");
        let call = PromptCall {
            request_id: "req-lived".into(),
            persona_id: persona,
            room_id: Uuid::new_v4(),
            cycle_id: Some(1),
            context_window: Some(32_768),
            cause: "synthetic",
            cause_root: None,
            replay_of: None,
        };
        let mut lease = CaptureLease::start(Arc::new(sink), &call, &request);
        let submitted = lease.cursor().expect("durable submission").to_owned();
        // the receipt carries the COMPLETED entry's cursor: the submission holds no response
        let cursor = lease.finish(Some(&response), None).expect("completed entry");
        assert_ne!(cursor, submitted);
        let (_, only_request) =
            read_lived_calls(Some(dir.path()), persona, vec![Some(submitted)], Vec::new());
        assert!(only_request.is_some_and(|r| r.contains("no terminal response")));

        let (calls, missing) =
            read_lived_calls(Some(dir.path()), persona, vec![Some(cursor.clone())], Vec::new());
        assert_eq!(missing, None);
        assert_eq!(calls.len(), 1);
        let lived = &calls[0];
        assert_eq!(lived.capture, cursor, "the example links back to its trace");
        assert_eq!(
            serde_json::to_value(&lived.request.messages).unwrap(),
            serde_json::to_value(&request.messages).unwrap(),
            "the example is the request she was served, not a re-derivation"
        );
        assert_eq!(lived.response.reasoning, response.reasoning, "her thinking stays");
        assert_eq!(lived.response.tool_calls.as_ref().map(Vec::len), Some(1));

        // A second pass reuses what the turn already read and reads nothing again.
        let (again, missing) =
            read_lived_calls(None, persona, vec![Some(cursor.clone())], calls.clone());
        assert_eq!((again.len(), missing), (1, None));

        // A hole anywhere in the turn is named, never staged as complete.
        let (_, hole) = read_lived_calls(Some(dir.path()), persona, vec![Some(cursor), None], Vec::new());
        assert!(hole.is_some_and(|reason| reason.contains("no capture cursor")));

        let stamp = OutcomeStamp {
            card_id: Uuid::new_v4(),
            role: CreditRole::Owner,
            outcome: true,
        };
        let plan = SubmitPlan {
            trait_kind: "code".into(),
            prompt: "stimulus".into(),
            completion: "acted chain".into(),
            quality: 1.0,
            stamp: Some(stamp),
        };
        let examples = lived_examples(&plan, &calls);
        assert_eq!(examples.len(), 1);
        assert_eq!(examples[0].completion, "reading the failing test first");
        assert_eq!(examples[0].prompt, "card: fix the failing test");
        assert!(examples[0].lived.is_some());
        assert_eq!(
            examples[0].metadata.as_ref().unwrap()["cardId"],
            json!(stamp.card_id)
        );
    }

