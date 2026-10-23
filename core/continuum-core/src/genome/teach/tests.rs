#![cfg(test)]

use super::*;

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
