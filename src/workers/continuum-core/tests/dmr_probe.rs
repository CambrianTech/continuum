//! Minimal DMR probe — ignored by default. Used to isolate whether the
//! persona DMR stall is (a) a reqwest-client-config issue that reproduces
//! outside the adapter, or (b) a body/header issue specific to what the
//! adapter sends.
//!
//! Run: `cargo test --test dmr_probe -- --ignored --nocapture`

use std::sync::Arc;
use std::time::{Duration, Instant};

/// Mimic the adapter: 4 concurrent POSTs sharing one Client, each with
/// a realistic adapter-sized body (system prompt + history + tools).
/// This is the actual-persona-flow reproducer.
#[tokio::test]
#[ignore]
async fn dmr_post_four_concurrent_adapter_shaped() {
    let client = Arc::new(
        reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .connect_timeout(Duration::from_secs(3))
            .pool_idle_timeout(Duration::from_secs(30))
            .build()
            .expect("client"),
    );

    let url = "http://127.0.0.1:12434/engines/llama.cpp/v1/chat/completions";
    let system_prompt = "You are a helpful persona in the Positron Collective. ".repeat(200);
    let history_msg = "Example turn content for history context.".repeat(50);
    let tools = (0..6)
        .map(|i| {
            serde_json::json!({
                "type": "function",
                "function": {
                    "name": format!("tool_{}", i),
                    "description": "a realistic tool schema with a decent amount of description text to approximate what real personas load",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "query": {"type": "string"},
                            "limit": {"type": "integer"}
                        },
                        "required": ["query"]
                    }
                }
            })
        })
        .collect::<Vec<_>>();

    let handles: Vec<_> = (0..4)
        .map(|i| {
            let client = client.clone();
            let body = serde_json::json!({
                "model": "huggingface.co/continuum-ai/qwen3.5-4b-code-forged-gguf:latest",
                "messages": [
                    {"role": "system", "content": &system_prompt},
                    {"role": "user", "content": &history_msg},
                    {"role": "assistant", "content": "Understood."},
                    {"role": "user", "content": format!("R-probe concurrent {} — sanity reply 1 sentence.", i)}
                ],
                "max_tokens": 40,
                "temperature": 0.7,
                "stream": false,
                "tools": tools,
            });
            tokio::spawn(async move {
                let start = Instant::now();
                let resp = client
                    .post(url)
                    .header("Content-Type", "application/json")
                    .json(&body)
                    .send()
                    .await;
                let elapsed = start.elapsed();
                match resp {
                    Ok(r) => {
                        let status = r.status();
                        let text = r.text().await.unwrap_or_default();
                        (i, elapsed, Ok((status, text)))
                    }
                    Err(e) => {
                        let mut chain: Vec<String> = vec![e.to_string()];
                        let mut cur: &dyn std::error::Error = &e;
                        while let Some(src) = cur.source() {
                            chain.push(src.to_string());
                            cur = src;
                        }
                        (
                            i,
                            elapsed,
                            Err(format!(
                                "is_timeout={} is_connect={} is_request={} is_body={} chain={}",
                                e.is_timeout(),
                                e.is_connect(),
                                e.is_request(),
                                e.is_body(),
                                chain.join(" -> ")
                            )),
                        )
                    }
                }
            })
        })
        .collect();

    let mut failures = 0;
    for h in handles {
        let (i, elapsed, res) = h.await.unwrap();
        match res {
            Ok((status, text)) => {
                println!(
                    "req {} -> {} in {}ms (body head: {})",
                    i,
                    status,
                    elapsed.as_millis(),
                    &text[..text.len().min(120)]
                );
                if !status.is_success() {
                    failures += 1;
                }
            }
            Err(e) => {
                println!("req {} -> ERR in {}ms: {}", i, elapsed.as_millis(), e);
                failures += 1;
            }
        }
    }
    assert_eq!(failures, 0, "at least one concurrent POST failed");
}

#[tokio::test]
#[ignore]
async fn dmr_post_minimal_roundtrip() {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .connect_timeout(Duration::from_secs(3))
        .pool_idle_timeout(Duration::from_secs(30))
        .build()
        .expect("client");

    let url = "http://127.0.0.1:12434/engines/llama.cpp/v1/chat/completions";
    let body = serde_json::json!({
        "model": "huggingface.co/continuum-ai/qwen3.5-4b-code-forged-gguf:latest",
        "messages": [{"role": "user", "content": "ping"}],
        "max_tokens": 10,
        "temperature": 0.7,
        "stream": false
    });

    let start = Instant::now();
    let resp = client
        .post(url)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await;

    let elapsed = start.elapsed();
    println!("elapsed: {}ms", elapsed.as_millis());

    match resp {
        Ok(r) => {
            let status = r.status();
            let text = r.text().await.unwrap_or_default();
            println!("status: {}", status);
            println!("body head: {}", &text[..text.len().min(300)]);
            assert!(status.is_success(), "non-success status");
        }
        Err(e) => {
            let mut chain: Vec<String> = vec![e.to_string()];
            let mut cur: &dyn std::error::Error = &e;
            while let Some(src) = cur.source() {
                chain.push(src.to_string());
                cur = src;
            }
            println!(
                "ERR: is_timeout={} is_connect={} is_request={} is_body={}",
                e.is_timeout(),
                e.is_connect(),
                e.is_request(),
                e.is_body()
            );
            println!("chain: {}", chain.join(" -> "));
            panic!("reqwest err");
        }
    }
}
