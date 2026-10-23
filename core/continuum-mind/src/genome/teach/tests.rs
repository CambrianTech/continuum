#![cfg(test)]

use super::*;

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
