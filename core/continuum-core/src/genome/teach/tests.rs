#![cfg(test)]

use super::*;
use crate::modules::dataset::DatasetService;

// what this catches: a refused teacher lane must fail the real synthesis path,
// not fall through to registry inference and return an empty/failed-item corpus.
#[tokio::test]
async fn refused_lane_stops_both_synthesis_paths_before_generation() {
    let refused = || std::future::ready(Err(CommandError::Internal("lane refused".into())));
    let tasks = [EvalTask {
        id: "refused-lane".into(),
        prompt: "write a function".into(),
        test: Some("assert!(true);".into()),
        ..Default::default()
    }];
    let remediation =
        synthesize_remediation_with_lane(&tasks, "teacher", 0.0, 1, refused()).await;
    assert!(
        matches!(remediation, Err(CommandError::Internal(ref message)) if message == "lane refused")
    );
    let expansion = synthesize_lived_expansion_with_lane(
        &["a lived question".into()],
        "teacher",
        0.0,
        refused(),
    )
    .await;
    assert!(
        matches!(expansion, Err(CommandError::Internal(ref message)) if message == "lane refused")
    );

    // Empty lived input remains a no-op and never polls lane acquisition.
    let empty = synthesize_lived_expansion_with_lane(&["  ".into()], "teacher", 0.0, async {
        panic!("empty input acquired a lane")
    })
    .await
    .unwrap();
    assert!(empty.is_empty());
}

// what this catches: a selected model is not proof of who answered; a provider
// may return a different model/request id or an Ok envelope containing a fault.
// Only public text and canonical receipts cross into persisted dataset rows.
#[tokio::test]
async fn teacher_rows_preserve_actual_receipts_and_refuse_faulted_output() {
    use crate::ai::adapter::AIProviderAdapter;
    use crate::ai::heuristic_adapter::HeuristicInferenceAdapter;
    use crate::ai::types::{FinishReason, UsageMetrics};
    use std::sync::{Arc, Mutex};

    let response = TextGenerationResponse {
        text: "public answer".into(),
        finish_reason: FinishReason::Stop,
        model: "actual-teacher".into(),
        provider: "actual-provider".into(),
        usage: UsageMetrics::default(),
        response_time_ms: 1,
        request_id: "provider-request".into(),
        content: None,
        tool_calls: None,
        reasoning: Some("synthetic-private-reasoning-sentinel".into()),
        routing: None,
        error: None,
        timing: None,
    };
    let mut fault = response.clone();
    fault.finish_reason = FinishReason::Error;
    let mut error_field = response.clone();
    error_field.error = Some("provider failure".into());
    let requests = Arc::new(Mutex::new(Vec::new()));
    let adapter: Arc<dyn AIProviderAdapter> = Arc::new(
        HeuristicInferenceAdapter::new()
            .with_responses(vec![response, fault, error_field])
            .with_request_recorder(requests.clone()),
    );
    let generated = teacher_generate(
        &adapter,
        "requested-alias",
        vec![ChatMessage::text("user", "public task")],
        0.0,
    )
    .await
    .expect("served response");
    let submitted = requests.lock().unwrap()[0]
        .request_id
        .clone()
        .expect("assigned at dispatch");
    assert_eq!(generated.receipt.submitted_request_id, submitted);
    assert_eq!(
        generated.receipt.outcome,
        GenerationOutcome::Served {
            model: "actual-teacher".into(),
            provider: "actual-provider".into(),
            provider_request_id: Some("provider-request".into()),
        }
    );
    let row = build_sharegpt(
        &[
            ChatMessage::text("user", "public task"),
            ChatMessage::text("assistant", generated.text),
        ],
        &[generated.receipt],
    );
    let dir = tempfile::tempdir().unwrap();
    DatasetService::split_and_write("teacher", dir.path(), &[row.clone()], 1.0, None).unwrap();
    let persisted = std::fs::read_to_string(dir.path().join("train.jsonl")).unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(persisted.trim()).unwrap(),
        row
    );
    assert!(!persisted.contains("synthetic-private-reasoning-sentinel"));
    assert!(persisted.contains("actual-teacher"));
    assert!(!persisted.contains("requested-alias"));
    let loaded = crate::genome::fine_tuning::TrainingDataset::from_chat_jsonl(
        &dir.path().join("train.jsonl"),
        crate::genome::fine_tuning::TrainingSource::TeacherSynthesized,
    )
    .unwrap();
    assert_eq!(loaded.examples[0].metadata.as_ref(), Some(&row["metadata"]));
    assert_eq!(loaded.examples[0].completion, "public answer");
    assert!(!serde_json::to_string(&loaded)
        .unwrap()
        .contains("synthetic-private-reasoning-sentinel"));
    for _ in 0..2 {
        let failed = teacher_generate(
            &adapter,
            "requested-alias",
            vec![ChatMessage::text("user", "public task")],
            0.0,
        )
        .await;
        assert!(
            matches!(failed, Err(CommandError::Internal(ref error)) if error.contains("teacher generation faulted")),
            "a generation fault must never yield text for grading"
        );
    }
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 3);
    assert_ne!(requests[0].request_id, requests[1].request_id);
    assert_ne!(requests[1].request_id, requests[2].request_id);
}

// what this catches: a validated trajectory flattens to the ShareGPT shape
// mlx_lm.lora/dataset consume — role+content per turn, ORDER preserved (the
// write→error→fix→pass ordering IS the lesson). Drift here silently corrupts
// every example the teacher emits.
#[test]
fn build_sharegpt_preserves_role_content_and_order() {
    let traj = vec![
        ChatMessage::text("system", "be an engineer"),
        ChatMessage::text("user", "write add"),
        ChatMessage::text("assistant", "```rust\nfn add(){}\n```"),
        ChatMessage::text("user", "Your solution failed: compile error"),
        ChatMessage::text("assistant", "```rust\nfn add(a:i32,b:i32)->i32{a+b}\n```"),
    ];
    let v = build_sharegpt(&traj, &[]);
    let msgs = v["messages"].as_array().expect("messages array");
    assert_eq!(msgs.len(), 5);
    assert_eq!(msgs[0]["role"], "system");
    assert_eq!(msgs[1]["content"], "write add");
    assert_eq!(msgs[3]["role"], "user");
    assert_eq!(msgs[4]["role"], "assistant");
    assert!(msgs[4]["content"].as_str().unwrap().contains("a+b"));
}
