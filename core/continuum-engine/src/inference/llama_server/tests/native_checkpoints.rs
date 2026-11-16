//! Placed checkpoints on a REAL engine (CONTINUUM-ENGINE 4.7.3; llama.cpp#121, Codex's gate): the
//! fork's llama-server fills a slot's checkpoint store with blind step checkpoints, then her acts carry
//! view spans. Her head, the oldest checkpoint and her restore point when a prompt diverges before her
//! view end, must keep its exact position through the placed pruning and the count eviction, and a
//! prompt that diverges after it must resume from exactly that position.
//!
//! Ignored by default: it needs the engine binary and a model whose prompts checkpoint (sliding-window
//! or recurrent attention: tinygemma3, a Qwen3.5/3.6 hybrid). Run it with
//! `CONTINUUM_NATIVE_ENGINE_BIN=<fork llama-server> CONTINUUM_NATIVE_CHECKPOINT_MODEL=<gguf>
//! cargo test -p continuum-engine --lib native_checkpoints -- --ignored --nocapture`. Against the pinned
//! engine (before #121) it fails: `/slots` names no checkpoint positions, and the count eviction takes
//! the head.

use std::process::Stdio;

use tokio::io::{AsyncBufReadExt, BufReader};

/// A real engine process, killed when dropped (`kill_on_drop`).
struct Engine {
    _child: tokio::process::Child,
    root: String,
}

fn required(var: &str) -> String {
    std::env::var(var).unwrap_or_else(|_| panic!("test: set {var} to run the native checkpoint proof")) // test: names the missing input
}

/// A free local port, released for the engine to bind.
fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("test: a local port"); // test: loopback bind
    listener.local_addr().expect("test: its address").port() // test: a bound socket has an address
}

/// Start the engine with a small checkpoint store and step, returning once it says it is listening
/// (its own readiness line, never a timer).
async fn start() -> Engine {
    let port = free_port();
    let mut child = tokio::process::Command::new(required("CONTINUUM_NATIVE_ENGINE_BIN"))
        .args(["-m", &required("CONTINUUM_NATIVE_CHECKPOINT_MODEL"), "--host", "127.0.0.1", "--port", &port.to_string()])
        .args(["-np", "1", "-c", "4096", "--jinja", "--ctx-checkpoints", "4", "--checkpoint-min-step", "16"])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("test: the engine binary starts"); // test: the named binary exists
    let mut lines = BufReader::new(child.stderr.take().expect("test: stderr was piped")).lines(); // test: piped above
    let mut printed = Vec::new();
    loop {
        match lines.next_line().await.expect("test: the engine's stderr reads") { // test: a pipe read
            Some(line) if line.contains("listening on") => break, // printed after the model loads
            Some(line) => printed.push(line),
            None => panic!("test: the engine exited before listening:\n{}", printed.join("\n")),
        }
    }
    // keep draining stderr so the engine never blocks on a full pipe
    tokio::spawn(async move { while let Ok(Some(_)) = lines.next_line().await {} });
    Engine { _child: child, root: format!("http://127.0.0.1:{port}") }
}

/// Her conversation after `acts` acts: each act appends her entry and the next ask.
fn conversation(acts: usize) -> Vec<serde_json::Value> {
    let mut messages = vec![
        serde_json::json!({"role": "system", "content": "You keep notes about the garden, one line per entry."}),
        serde_json::json!({"role": "user", "content": "Start the notes with the roses along the south wall."}),
    ];
    for act in 0..acts {
        messages.push(serde_json::json!({"role": "assistant", "content": format!("Entry {act}: watered and checked for aphids.")}));
        messages.push(serde_json::json!({"role": "user", "content": format!("Now add entry {} about the herb bed and the path.", act + 1)}));
    }
    messages
}

/// One act on slot 0, with her view spans (everything but her newest ask) or blind; the cached prefix
/// the engine resumed from.
async fn act(client: &reqwest::Client, engine: &Engine, messages: &[serde_json::Value], spans: bool) -> u64 {
    let mut body = serde_json::json!({"messages": messages, "max_tokens": 4, "temperature": 0.0, "id_slot": 0, "cache_prompt": true});
    if spans {
        body["view_messages"] = serde_json::json!(messages.len() - 1);
    }
    let reply: serde_json::Value = client
        .post(format!("{}/v1/chat/completions", engine.root))
        .json(&body)
        .send()
        .await
        .expect("test: the chat route answers") // test: a live engine
        .json()
        .await
        .expect("test: the reply is JSON"); // test: the engine's own reply shape
    reply["timings"]["cache_n"].as_u64().expect("test: the reply names its cached prefix") // test: the fork reports timings.cache_n
}

/// Slot 0's checkpoint positions, oldest first (`/slots` checkpoints.at, llama.cpp#121).
async fn positions(client: &reqwest::Client, engine: &Engine) -> Vec<u64> {
    let slots: serde_json::Value = client
        .get(format!("{}/slots", engine.root))
        .send()
        .await
        .expect("test: /slots answers") // test: a live engine
        .json()
        .await
        .expect("test: /slots is JSON"); // test: the engine's own reply shape
    slots[0]["checkpoints"]["at"]
        .as_array()
        .expect("test: the engine names its checkpoint positions (llama.cpp#121)") // test: red on an engine without #121
        .iter()
        .map(|c| c["n_tokens"].as_u64().expect("test: each checkpoint names its position")) // test: the fork's report shape
        .collect()
}

// what this catches (Codex on #121): a full store migrating to placed checkpoints where the generic
// count eviction took the true head before the placed pruning ran, and a head kept only by name (the
// next survivor relabelled). The head's exact position must survive acts with spans, and a prompt
// diverging after it must resume from exactly there.
#[tokio::test]
#[ignore = "native: needs CONTINUUM_NATIVE_ENGINE_BIN and a checkpointing CONTINUUM_NATIVE_CHECKPOINT_MODEL"]
async fn her_head_checkpoint_keeps_its_exact_position_through_a_full_store() {
    let engine = start().await;
    let client = reqwest::Client::new();
    for acts in 1..=6 {
        act(&client, &engine, &conversation(acts), false).await;
    }
    let blind = positions(&client, &engine).await;
    assert_eq!(blind.len(), 4, "the blind store is full: {blind:?}");
    let head = blind[0];
    for acts in 7..=9 {
        act(&client, &engine, &conversation(acts), true).await;
        let placed = positions(&client, &engine).await;
        assert_eq!(placed.first(), Some(&head), "her head kept its exact position after act {acts}: {placed:?}");
        assert!(placed.len() <= 4, "placed checkpoints stay within the store: {placed:?}");
    }
    // diverge after her head, before her newest view end: the engine resumes from her head
    let mut diverged: Vec<serde_json::Value> = conversation(9).into_iter().take(4).collect();
    diverged.push(serde_json::json!({"role": "user", "content": "Instead, list the tools in the shed."}));
    let cached = act(&client, &engine, &diverged, true).await;
    assert!(cached >= head, "resumed from at least her head ({head} tokens), cached {cached}");
}
