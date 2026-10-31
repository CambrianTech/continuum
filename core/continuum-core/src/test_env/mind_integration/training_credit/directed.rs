#![cfg(test)]
//! From `cognition::act_observe` (its credit scenarios): a directed drive's credit, staged
//! through the real data runtime, survives a partial cancellation and an atomic retry.
use super::*;
use std::sync::Mutex;
use crate::ai::types::ToolCall;
use crate::cognition::act_observe::test_support::{admission, body_with_wm, ScriptedExecutor};
use crate::cognition::act_observe::*;
use crate::cognition::working_memory::WorkingMemory;
use crate::cognition::workspace::{Faculty, SalienceArbiter, TurnFraming, WorkspaceCycle};

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
    use crate::cognition::conversation::scripted_conversation::ScriptedConversation;
    use crate::cognition::conversation::PersonaConversation;
    use crate::persona::training_producer::{
        CapturedCredit, StagedCredit, StagedCreditGeneration,
    };

    let home = tempfile::tempdir().unwrap();
    let _home = crate::paths::test_home::HomeGuard::set(home.path()).await;
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
            Arc::new(faculty),
        ],
        Arc::new(SalienceArbiter),
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
    // Her staged credit, read back through the ORM like every other reader.
    let longterm: Arc<dyn crate::orm::StorageAdapter> = Arc::new(crate::orm::SqliteAdapter::open(&home.path().join(".continuum/personas/capture-owner/data/longterm.db")).await.expect("open her longterm store"));
    let staged = crate::orm::OrmStore::<StagedCredit>::new(Arc::clone(&longterm)).await.expect("staged credit store");
    let read = || async {
        let mut rows = staged.find_eq("cardId", card.to_string()).await.expect("read staged credit");
        assert_eq!(rows.len(), 1, "one staged credit row per card");
        rows.remove(0).1
    };
    let partial = read().await;
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
            crate::ai::provenance::GenerationOutcome::Served { model, provider, provider_request_id }
                if model == "actual-served-model" && provider == "fixture"
                    && provider_request_id.as_deref() == Some(format!("provider-{n}").as_str())));
    }
    assert!(
        partial.claim_id.is_none(),
        "staging must not invent ownership or a verdict"
    );
    drop(drive); // Cancels the fifth act; the fourth-act durable row survives.
    assert_eq!(read().await.id, partial.id);
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
        read().await.receipts,
        partial.receipts,
        "rollback preserves the previous whole row"
    );
    let mut final_receipts = partial.receipts.clone();
    final_receipts.push(crate::ai::provenance::GenerationReceipt::faulted(
        "fault-after-four",
        "transport refused",
    ));
    assert!(
        capture
            .record(&acts, &final_receipts, Some("honest final report"), true)
            .await
    );
    let finished = read().await;
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
        read().await.id,
        finished.id,
        "identical retry reuses its revision identity"
    );
    let generations = crate::orm::OrmStore::<StagedCreditGeneration>::new(Arc::clone(&longterm)).await.expect("generation store");
    assert_eq!(generations.find_all().await.expect("read generations").len(), 5);
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
        read().await.id,
        finished.id,
        "same Persona's next turn cannot replace this card"
    );
    assert_eq!(staged.find_all().await.expect("read staged credits").len(), 2);
}
