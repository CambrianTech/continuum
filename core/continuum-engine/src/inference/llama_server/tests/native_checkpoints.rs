#![cfg(test)]
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

/// Start the engine with a checkpoint store of `store` entries and a small step, returning once it says
/// it is listening (its own readiness line, never a timer).
async fn start(store: usize) -> Engine {
    let port = free_port();
    let store = store.to_string();
    let mut child = tokio::process::Command::new(required("CONTINUUM_NATIVE_ENGINE_BIN"))
        .args(["-m", &required("CONTINUUM_NATIVE_CHECKPOINT_MODEL"), "--host", "127.0.0.1", "--port", &port.to_string()])
        .args(["-np", "1", "-c", "4096", "--jinja", "--ctx-checkpoints", &store, "--checkpoint-min-step", "16"])
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

/// The engine's own tokens for `messages` under its chat template (`/apply-template`, then `/tokenize`),
/// so a test knows exactly where two prompts diverge instead of guessing from message counts.
async fn prompt_tokens(client: &reqwest::Client, engine: &Engine, messages: &[serde_json::Value]) -> Vec<u64> {
    let templated: serde_json::Value = client
        .post(format!("{}/apply-template", engine.root))
        .json(&serde_json::json!({"messages": messages}))
        .send()
        .await
        .expect("test: /apply-template answers") // test: a live engine
        .json()
        .await
        .expect("test: /apply-template is JSON"); // test: the engine's own reply shape
    let tokenized: serde_json::Value = client
        .post(format!("{}/tokenize", engine.root))
        .json(&serde_json::json!({"content": templated["prompt"], "add_special": true, "parse_special": true}))
        .send()
        .await
        .expect("test: /tokenize answers") // test: a live engine
        .json()
        .await
        .expect("test: /tokenize is JSON"); // test: the engine's own reply shape
    tokenized["tokens"]
        .as_array()
        .expect("test: /tokenize names its tokens") // test: the engine's own reply shape
        .iter()
        .map(|t| t.as_u64().expect("test: a token id")) // test: ids are integers
        .collect()
}

/// How many leading tokens `a` and `b` share.
fn shared_prefix(a: &[u64], b: &[u64]) -> u64 {
    a.iter().zip(b).take_while(|(x, y)| x == y).count() as u64
}

/// Fill a store of `store` entries blind, then act with spans: her head keeps its exact position through
/// every act and the store never grows past its size. Then a prompt that diverges just past her head
/// resumes from exactly the largest checkpoint held at or before the divergence (her head), never from
/// a position the store does not hold and never from nothing.
async fn head_survives_and_resumes_exactly(store: usize) {
    let engine = start(store).await;
    let client = reqwest::Client::new();
    for acts in 1..=6 {
        act(&client, &engine, &conversation(acts), false).await;
    }
    let blind = positions(&client, &engine).await;
    assert_eq!(blind.len(), store, "the blind store is full: {blind:?}");
    let head = blind[0];
    let mut held = blind;
    for acts in 7..=9 {
        act(&client, &engine, &conversation(acts), true).await;
        held = positions(&client, &engine).await;
        assert_eq!(held.first(), Some(&head), "store {store}: her head kept its exact position after act {acts}: {held:?}");
        assert!(held.len() <= store, "store {store}: placed checkpoints stay within the store: {held:?}");
    }
    // The slot holds her ninth act's prompt. Diverge at the first act boundary at or past her head: an
    // earlier act's messages with a different ask, chosen by the engine's own tokens.
    let held_prompt = prompt_tokens(&client, &engine, &conversation(9)).await;
    let diverged_at = |acts: usize| {
        let mut messages = conversation(acts);
        messages.pop();
        messages.push(serde_json::json!({"role": "user", "content": "Instead, list the tools in the shed."}));
        messages
    };
    let mut chosen = None;
    for acts in 1..=9 {
        let messages = diverged_at(acts);
        let divergence = shared_prefix(&held_prompt, &prompt_tokens(&client, &engine, &messages).await);
        if divergence >= head {
            chosen = Some((messages, divergence));
            break;
        }
    }
    let (diverged, divergence) = chosen.expect("test: some act boundary lies past her head"); // test: nine acts outgrow the head
    let expected = held.iter().copied().filter(|&at| at <= divergence).max().expect("test: her head is at or before the divergence"); // test: chosen so
    let cached = act(&client, &engine, &diverged, true).await;
    // A restored checkpoint resumes at its pos_max, one before its token count, so the last token is
    // re-evaluated and the step is never empty (fork server-context.cpp: `pos_next = min(pos_next,
    // max(pos_min + 1, pos_max))`, upstream's TAG_PROMPT_LOGITS). Measured on #121 e6cd328ce with
    // gemma-3-270m: held 194 -> cached 193, held 210 -> cached 209.
    assert_eq!(
        cached,
        expected - 1,
        "store {store}: diverging at token {divergence} resumes from exactly the largest held checkpoint at or before it ({expected}); head {head}, held {held:?}"
    );
}

// what this catches (Codex on #121): a full store migrating to placed checkpoints where the generic
// count eviction took the true head before the placed pruning ran, and a head kept only by name (the
// next survivor relabelled). The head's exact position must survive acts with spans, and a prompt
// diverging after it must resume from exactly a checkpoint the store holds.
#[tokio::test]
#[ignore = "native: needs CONTINUUM_NATIVE_ENGINE_BIN and a checkpointing CONTINUUM_NATIVE_CHECKPOINT_MODEL"]
async fn her_head_checkpoint_keeps_its_exact_position_through_a_full_store() {
    head_survives_and_resumes_exactly(4).await;
}

// what this catches (Codex on #121): at low capacity (a store of 2) the placed pruning and the count
// eviction both press on the head; it must still be the one kept, at its exact position.
#[tokio::test]
#[ignore = "native: needs CONTINUUM_NATIVE_ENGINE_BIN and a checkpointing CONTINUUM_NATIVE_CHECKPOINT_MODEL"]
async fn her_head_checkpoint_survives_a_low_capacity_store() {
    head_survives_and_resumes_exactly(2).await;
}
