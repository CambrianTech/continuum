//! Full chat-pipeline integration test — exercises the SAME path the
//! persona uses (chat template render → tokenize-with-special → scheduler
//! with full sampler chain → stop_sequences). Runs in seconds and asserts
//! the output is coherent (length, no token leakage, no obvious loops).
//!
//! Catches the failure modes that the bare ctx.decode tests missed:
//!   - tokenize(special=false) silently breaking chat-template boundary tokens
//!   - sampler chain dropping repeat_penalty
//!   - stop_sequences not registered
//!   - chat_template not propagated
//!
//! Run:
//!   cargo test --release --test qwen35_chat_pipeline_full -- --ignored --nocapture

// unix-only integration target (#304): dials the core UNIX IPC socket /
// sends unix signals. Windows checks compile it to empty; the lib +
// unit tests are the windows-supported surface today.
#![cfg(unix)]

use continuum_core::inference::backends::llamacpp::{LlamaCppBackend, LlamaCppConfig};
use continuum_core::inference::backends::{SamplingConfig, JSON_GRAMMAR};
use llama::{render_chat, ChatMsg, FlashAttn};
use std::path::PathBuf;

mod common;

fn model_path() -> std::path::PathBuf {
    common::qwen35_4b_code_gguf().expect(
        "qwen3.5-4b-code-forged GGUF not resolvable via DMR;          is Docker Desktop running with Model Runner enabled?",
    )
}

const CHATML: &str = "{% for message in messages %}{{ '<|im_start|>' + message['role'] + '\n' + message['content'] + '<|im_end|>\n' }}{% endfor %}{% if add_generation_prompt %}{{ '<|im_start|>assistant\n' }}{% endif %}";

#[test]
#[ignore = "requires local GGUF; cargo test --release --test qwen35_chat_pipeline_full -- --ignored --nocapture"]
fn qwen35_persona_style_chat_produces_coherent_short_reply() {
    // n_gpu_layers honors QWEN35_N_GPU_LAYERS env var (default -1 = all on GPU).
    // Set QWEN35_N_GPU_LAYERS=0 for CPU-only inference. Needed on Intel Macs
    // with discrete AMD Metal devices where the SSM-hybrid qwen35 Metal
    // kernels currently crash during JIT compilation — see findings in #129
    // run 2026-06-01 on MacBookPro15,1 + Radeon Pro 560X. The bundled
    // llama.cpp Metal path was validated on M-series only.
    let n_gpu_layers: i32 = std::env::var("QWEN35_N_GPU_LAYERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(-1);
    let context_length: u32 = std::env::var("QWEN35_CONTEXT_LENGTH")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(32_768);
    eprintln!("[full] backend config: n_gpu_layers={n_gpu_layers} context_length={context_length}");
    let backend = LlamaCppBackend::load(LlamaCppConfig {
        model_path: PathBuf::from(model_path()),
        n_gpu_layers,
        context_length: Some(context_length),
        n_seq_max: 1,
        n_ubatch: 128,
        flash_attn: FlashAttn::Disabled,
        fused_gdn_ar: false,
        fused_gdn_ch: false,
        ..Default::default()
    })
    .expect("load");

    // Render the prompt the way the LlamaCppAdapter would: chatml template
    // applied to a system + user message pair.
    let messages = vec![
        ChatMsg {
            role: "system".to_string(),
            content: "You are Helper AI. Answer concisely in one short sentence.".to_string(),
        },
        ChatMsg {
            role: "user".to_string(),
            content: "What is 12 times 7?".to_string(),
        },
    ];
    let prompt = render_chat(Some(CHATML), &messages, true).expect("render_chat");
    eprintln!("[full] rendered prompt ({} chars):\n{prompt}", prompt.len());

    // Sampler config matches what the live persona pipeline gets:
    // chat() defaults (temp=0.6, repeat_penalty=1.1, top_k=40, top_p=0.95).
    let sampling = SamplingConfig::chat();

    // Stop sequences match what the Rust catalog (catalog.rs) declares for qwen3.5 — these
    // catch the chat-template terminator since the GGUF's eos_token_id is wrong.
    let stop: [&str; 2] = ["<|im_end|>", "<|endoftext|>"];

    // 2500 matches what PersonaModelConfigs gives the live personas.
    // qwen3.5 is a reasoning model — it emits ~500-800 tokens of <think>
    // reasoning before the visible answer. 200 cuts it off mid-reasoning;
    // strip_think_blocks then leaves empty output. Validated 2026-04-20:
    // model produced correct '12 × 7 = 84' inside <think> but never
    // reached the visible-text phase before max_tokens.
    let (text, n_tokens) = backend
        .generate(&prompt, 2500, sampling, &stop, &[])
        .expect("generate");

    eprintln!("[full] tokens={n_tokens} text={text:?}");

    // Hard assertions on coherence:
    assert!(n_tokens > 0, "no tokens generated");
    assert!(
        n_tokens < 2500,
        "hit max_tokens cap — model couldn't terminate even with 2500 token budget"
    );
    assert!(!text.is_empty(), "empty output text");
    // No obvious loop: the same 20-char window shouldn't repeat 3+ times.
    if text.len() > 60 {
        let window = &text[..20];
        let count = text.matches(window).count();
        assert!(
            count < 3,
            "loop detected: '{window}' appears {count}× in output"
        );
    }
    // Output should NOT include the literal "<|im_end|>" — stop_sequences
    // should have stopped generation BEFORE the model emitted it.
    assert!(
        !text.contains("<|im_end|>"),
        "output contains literal <|im_end|> — stop_sequences clipped too late or scheduler doesn't truncate"
    );
    // Should contain the actual answer somewhere.
    assert!(
        text.contains("84") || text.contains("eighty-four") || text.contains("eighty four"),
        "answer (84) not in output: {text:?}"
    );
}

#[test]
#[ignore = "requires local GGUF; cargo test --release --test qwen35_chat_pipeline_full -- --ignored --nocapture"]
fn qwen35_scheduler_json_grammar_returns_object() {
    let backend = LlamaCppBackend::load(LlamaCppConfig {
        model_path: PathBuf::from(model_path()),
        n_gpu_layers: -1,
        context_length: Some(32_768),
        n_seq_max: 1,
        n_ubatch: 128,
        flash_attn: FlashAttn::Disabled,
        fused_gdn_ar: false,
        fused_gdn_ch: false,
        ..Default::default()
    })
    .expect("load");

    let messages = vec![
        ChatMsg {
            role: "system".to_string(),
            content: "Return only a compact JSON object with key ok and boolean value true."
                .to_string(),
        },
        ChatMsg {
            role: "user".to_string(),
            content: "Report whether the cognition pipeline is live.".to_string(),
        },
    ];
    let prompt = render_chat(Some(CHATML), &messages, true).expect("render_chat");
    let sampling = SamplingConfig {
        grammar: Some(JSON_GRAMMAR.to_string()),
        ..SamplingConfig::chat()
    };

    let (text, n_tokens) = backend
        .generate(
            &prompt,
            128,
            sampling,
            &["<|im_end|>", "<|endoftext|>"],
            &[],
        )
        .expect("generate");

    eprintln!("[json-grammar] tokens={n_tokens} text={text:?}");
    assert!(n_tokens > 0, "no tokens generated");
    assert!(
        serde_json::from_str::<serde_json::Value>(text.trim()).is_ok(),
        "grammar-constrained output should parse as JSON object: {text:?}"
    );
}
