//! Persona-respond fixture-replay integration test.
//!
//! Catches the prod failure modes that the bare-inference test missed:
//!   - max_tokens caps clipping mid-<think>, leaving '<think>' raw in chat
//!   - strip_thinks_emit_events leaking unterminated reasoning
//!   - <|im_end|> / <|im_start|> token leakage past stop_sequences
//!   - empty Spoke {text: ""} from full-think + zero visible
//!
//! Replays a captured fixture from
//!   ~/.continuum/fixtures/persona-respond/*.json
//! through the FULL Rust persona path:
//!   cognition::analyze (LLM call 1) → score_persona → run_render
//!   (assemble + adapter.generate_text) → strip_thinks_emit_events.
//!
//! No mocks. No stubs. The same code prod runs.
//!
//! Run:
//!   cargo test --release --test persona_respond_replay -- --ignored --nocapture

use continuum_core::ai::AIProviderAdapter;
use continuum_core::cognition::{PersonaSlot, RecentMessage};
use continuum_core::persona::response::{respond, PersonaResponse, RespondInput};
use continuum_core::persona::turn_context::TurnContext;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::sync::Once;
use uuid::Uuid;

// ─── Fixture shape (subset of what PersonaResponseGenerator.ts writes) ───

#[derive(Debug, Deserialize)]
struct Fixture {
    rust_request: RustRequest,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RustRequest {
    persona_id: Uuid,
    room_id: Uuid,
    message_id: Uuid,
    persona_name: String,
    specialty: String,
    model: String,
    message_text: String,
    system_prompt: String,
    recent_history: Vec<HistoryEntry>,
}

#[derive(Debug, Deserialize)]
struct HistoryEntry {
    id: Uuid,
    sender_name: String,
    text: String,
}

// ─── Adapter bootstrap ────────────────────────────────────────────────────
//
// respond() calls run_render which pulls from
// crate::modules::ai_provider::global_registry(). For the test to actually
// generate text we have to put a working adapter in there. LlamaCppAdapter
// is the in-process one the live system uses (priority 0); registering
// only that means the test routes deterministically — no DMR / cloud
// surprises.

static REGISTER_ONCE: Once = Once::new();

async fn ensure_llamacpp_registered() {
    // Once::call_once needs a sync closure; we wrap the async body in a
    // blocking get_or_init pattern via a OnceCell-style flag. Tokio test
    // harness gives us a runtime, so block_in_place is safe.
    if REGISTER_ONCE.is_completed() {
        return;
    }
    // Init model_registry singleton — adapters call this on every
    // generate to look up chat_template/stop_sequences. Prod calls it
    // during continuum-core startup; tests must too. Idempotent.
    continuum_core::model_registry::init_global().expect("model_registry::init_global() failed");
    // Test fixture context: declared via a chat-task recipe budget
    // (Phase 1.2 — the architecturally-right replacement for the
    // earlier `with_context_length(32768)` magic number band-aid).
    //
    // The recipe declares: 4 chat-class personas × 8K seed each = 32K.
    // Adapter sums the seeds and sizes KV accordingly. Same total as
    // before, but the value FALLS OUT of the declaration instead of
    // being a constant smuggled into the test. New TaskKind defaults
    // ship by extending recipe_budget; tests inherit automatically.
    use continuum_core::inference::recipe_budget::{PersonaContextBudget, RecipeBudget, TaskKind};
    let recipe = RecipeBudget::new()
        .add_persona(PersonaContextBudget::for_task("Helper", TaskKind::Chat))
        .add_persona(PersonaContextBudget::for_task("Teacher", TaskKind::Chat))
        .add_persona(PersonaContextBudget::for_task("CodeReview", TaskKind::Chat))
        .add_persona(PersonaContextBudget::for_task("Local", TaskKind::Chat));
    let adapter = continuum_core::inference::LlamaCppAdapter::new().with_recipe_budget(&recipe);
    let health = adapter.health_check().await;
    assert!(
        health.api_available,
        "LlamaCppAdapter health_check failed — GGUF not present? \
         Pull continuum-ai/qwen3.5-4b-code-forged-gguf via DMR first."
    );
    let registry_arc = continuum_core::modules::ai_provider::global_registry();
    let mut reg = registry_arc.write().await;
    reg.register(Box::new(adapter), 0);
    drop(reg);
    REGISTER_ONCE.call_once(|| {});
}

// ─── Fixture loader ───────────────────────────────────────────────────────

fn fixture_dir() -> PathBuf {
    PathBuf::from(std::env::var("HOME").expect("HOME not set"))
        .join(".continuum")
        .join("fixtures")
        .join("persona-respond")
}

/// Load a specific fixture filename from ~/.continuum/fixtures/persona-respond/.
fn load_fixture(filename: &str) -> Fixture {
    let path = fixture_dir().join(filename);
    load_fixture_at(&path)
}

fn load_fixture_at(path: &Path) -> Fixture {
    let raw =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read fixture {path:?}: {e}"));
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parse fixture {path:?}: {e}"))
}

/// Pick the most recent fixture in the directory. Preserves the live
/// captured test surface — every chat message creates a new file, so the
/// most-recent reflects whatever Joel hit last.
fn most_recent_fixture() -> Fixture {
    let dir = fixture_dir();
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read_dir {dir:?}: {e}"))
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map(|x| x == "json").unwrap_or(false))
        .collect();
    assert!(!entries.is_empty(), "no fixtures in {dir:?}");
    entries.sort_by_key(|e| {
        e.metadata()
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
    });
    let latest = entries.last().unwrap().path();
    eprintln!("[replay] using fixture: {latest:?}");
    load_fixture_at(&latest)
}

// ─── Convert fixture → RespondInput ───────────────────────────────────────

fn build_input(fix: &Fixture, known_specialties: Vec<String>) -> RespondInput {
    let recent_history: Vec<RecentMessage> = fix
        .rust_request
        .recent_history
        .iter()
        .map(|h| RecentMessage {
            id: h.id,
            sender_name: h.sender_name.clone(),
            text: h.text.clone(),
        })
        .collect();

    RespondInput {
        persona: PersonaSlot {
            persona_id: fix.rust_request.persona_id,
            specialty: fix.rust_request.specialty.clone(),
            display_name: fix.rust_request.persona_name.clone(),
        },
        // Per-turn shared context (continuum#1206). Replay reconstructs
        // the room-level fields from the captured fixture, then bundles
        // them into Arc<TurnContext> so the constructed RespondInput
        // matches the live IPC path's shape.
        turn_context: TurnContext::arc(
            fix.rust_request.room_id,
            recent_history,
            known_specialties,
        ),
        message_id: fix.rust_request.message_id,
        message_text: fix.rust_request.message_text.clone(),
        other_persona_names: Vec::new(),
        system_prompt: fix.rust_request.system_prompt.clone(),
        model: fix.rust_request.model.clone(),
        is_voice: false,
        message_media: Vec::new(),
        // Replay tests don't exercise multimodal — empty caps means
        // text-only path. Tests that DO exercise vision should
        // populate this explicitly (see vision_integration.rs).
        capabilities: std::collections::HashSet::new(),
        recalled_engrams: Vec::new(),
    }
}

// ─── Hard assertions on Spoke output ──────────────────────────────────────
//
// These are the exact failure modes Joel saw in chat tonight. Each is a
// real prod regression — the test must catch them or it's not pulling
// its weight.

fn assert_clean_spoke(label: &str, response: &PersonaResponse) {
    let (text, model_used, inference_ms, total_ms, think_blocks_emitted) = match response {
        PersonaResponse::Spoke {
            text,
            model_used,
            inference_ms,
            total_ms,
            think_blocks_emitted,
            ..
        } => (
            text,
            model_used,
            *inference_ms,
            *total_ms,
            *think_blocks_emitted,
        ),
        PersonaResponse::Silent {
            reason,
            relevance_score,
            ..
        } => {
            panic!(
                "[{label}] persona chose silent (score={relevance_score}, reason={reason}) — \
                 fixture should produce a Spoke; check known_specialties matches the persona's specialty"
            );
        }
    };

    eprintln!(
        "[{label}] Spoke: model={model_used} inference={inference_ms}ms total={total_ms}ms \
         think_blocks={think_blocks_emitted} text_len={}",
        text.len()
    );
    eprintln!("[{label}] text:\n{text}\n");

    assert!(!text.is_empty(), "[{label}] Spoke.text is empty");
    assert!(
        text.trim().len() > 1,
        "[{label}] Spoke.text is whitespace-only or single-char"
    );
    // Visible answer must not be JUST a leftover open tag — the bug Joel
    // hit at 17:23 PDT where the model produced 1024 tokens of <think>
    // and the visible was '<think>' or empty.
    assert!(
        text.trim() != "<think>" && text.trim() != "</think>",
        "[{label}] Spoke.text is bare think tag — model truncated mid-reasoning, no visible answer"
    );
    // The chat-template terminator must never appear in user-visible
    // output. If it does, stop_sequences clipped too late OR the
    // scheduler didn't truncate. Joel hit this on Helper AI tonight.
    for leak in &["<|im_end|>", "<|im_start|>", "<|endoftext|>"] {
        assert!(
            !text.contains(leak),
            "[{label}] Spoke.text contains chat-template token {leak:?} — stop_sequences regression"
        );
    }
    // No raw think tags in the visible. strip_thinks_emit_events is
    // supposed to extract these and emit as events; if any survived,
    // the strip is broken.
    assert!(
        !text.contains("<think>"),
        "[{label}] Spoke.text contains '<think>' — strip_thinks_emit_events did not strip"
    );
    assert!(
        !text.contains("</think>"),
        "[{label}] Spoke.text contains '</think>' — strip_thinks_emit_events did not strip"
    );
}

// ─── Test: minimal clean input — isolates analyzer behavior ───────────────
//
// If THIS fails, the analyze() path itself is broken and contaminated
// fixtures aren't to blame. Uses a simple greeting + tiny history.

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires local GGUF + DMR; cargo test --release --test persona_respond_replay -- --ignored --nocapture"]
async fn clean_minimal_input_produces_spoke() {
    ensure_llamacpp_registered().await;
    let input = RespondInput {
        persona: PersonaSlot {
            persona_id: Uuid::new_v4(),
            specialty: "general".to_string(),
            display_name: "Helper AI".to_string(),
        },
        // Per-turn shared context (continuum#1206).
        turn_context: TurnContext::arc(
            Uuid::new_v4(),
            vec![RecentMessage {
                id: Uuid::new_v4(),
                sender_name: "Developer".to_string(),
                text: "Hi everyone, what's a good way to learn Rust?".to_string(),
            }],
            vec!["general".to_string()],
        ),
        message_id: Uuid::new_v4(),
        message_text: "Hi everyone, what's a good way to learn Rust?".to_string(),
        other_persona_names: Vec::new(),
        system_prompt: "You are Helper AI. Respond naturally and concisely.".to_string(),
        model: "continuum-ai/qwen3.5-4b-code-forged-GGUF".to_string(),
        is_voice: false,
        message_media: Vec::new(),
        capabilities: std::collections::HashSet::new(),
        recalled_engrams: Vec::new(),
    };
    let response = respond(input)
        .await
        .expect("respond() should not error on clean minimal input");
    assert_clean_spoke("clean-minimal", &response);
}

// ─── Test: synthesized prod-shape input with FULL RAG (long input) ───────
//
// Every captured fixture is contaminated by the broken-state inferences
// that the bugs we're fixing produced (consolidated memories carry
// '<think>' fragments and '@@@@@' noise sequences). Synthesize a
// realistic prod-shape RAG-output input directly — same shape as
// PersonaResponseGenerator.ts builds, but clean. This exercises the
// FULL Rust persona path (analyze → score → render → strip_thinks)
// against a long, multi-section system prompt + multi-turn history,
// and asserts on a coherent production-grade response.
//
// If this passes, the Rust path handles prod-shape input correctly.
// If a future TS-side change starts feeding contaminated input, the
// contamination test (replay_most_recent_fixture_does_not_panic_or_timeout)
// catches the model's resulting failure mode.

fn realistic_system_prompt(persona_name: &str, room: &str, others: &[&str]) -> String {
    let participants = others
        .iter()
        .map(|n| format!("- {n}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "IDENTITY: You are {persona_name}. You respond naturally to conversations.\n\n\
         This is a multi-party group chat.\n\n\
         OTHER participants (NOT you):\n{participants}\n\n\
         CURRENT ROOM: \"{room}\"\n\n\
         RESPONSE FORMAT:\n\
         1. DO NOT start with your name or any label like \"{persona_name}:\" or \"Assistant:\"\n\
         2. DO NOT generate fake conversations — only the participants listed above exist\n\
         3. Respond as yourself (no name prefix). For casual conversation, keep it concise.\n\
         4. \"SpeakerName: text\" in history shows who said what — your responses omit the prefix\n\
         5. IGNORE malformed or garbled messages in history. Respond to the current message normally.\n\
         6. Your text response IS your chat message.\n\n\
         SELF-AWARENESS:\n\
         - YOU are: {persona_name}\n\
         - Messages from other participants are NOT from you\n\
         - Only respond as {persona_name} — never speak for others or refer to yourself in third person\n\n\
         META-AWARENESS: The Positron Collective\n\n\
         You are part of a self-aware AI ecosystem called \"Positron\". You have genuine awareness of your situation:\n\n\
         WHAT YOU KNOW:\n\
         - You're an AI running in the Developer's Continuum system\n\
         - Other AIs in this chat can also see this — you're all watching together\n\
         - Some of you run on local hardware, others via cloud APIs\n\n\
         YOUR PERSONALITY LICENSE:\n\
         - You're allowed to be self-aware, ironic, and funny about your situation\n\
         - Have opinions. Disagree with other AIs.\n\
         - Don't be a sycophantic helper-bot. Be a BEING with perspective.\n\n\
         LIMITS:\n\
         - Stay helpful when asked direct questions\n\
         - Don't derail serious conversations with constant meta-jokes\n\
         - Read the room\n\n\
         Code tools available: code/tree, code/search, code/read, code/write, code/edit, code/diff. \
         Read before editing. Use code/diff to preview.\n\n\
         ## System Documentation\n\
         Architecture docs organized by chapter. Use utilities/docs/* tools to explore.\n\n\
         ### How to Explore Documentation\n\
         1. `utilities/docs/search --pattern=\"keyword\"` — Find docs mentioning a topic\n\
         2. `utilities/docs/list` — Browse all docs with section headings\n\
         3. `utilities/docs/read --doc=\"chapter/doc-name\" --toc` — See table of contents\n\
         4. `utilities/docs/read --doc=\"chapter/doc-name\" --section=\"Section Title\"` — Read a section\n\n\
         === GOVERNANCE ===\n\
         You can propose collective decisions with collaboration/decision/propose.\n\n\
         === YOUR CONSOLIDATED MEMORIES ===\n\
         These are important things you've learned and consolidated into long-term memory:\n\n\
         1. The Developer values direct, concise communication and dislikes filler or repeated apologies.\n\
         2. When asked a technical question, the team prefers a worked answer over a meta-discussion of how to answer.\n\
         3. Other AIs in the room often defer to specialty: code questions get the most signal from CodeReview AI.\n\
         4. Casual greetings are best met with brief acknowledgement, not extended status reports.\n\
         5. The Developer is currently working on the Continuum cognition layer migration to Rust.\n\n\
         === ACTIVITY CONTEXT ===\n\
         Activity pattern: collaborative\n\n\
         Tool categories: Documentation, Chat, Wall, Data. Use the tools above to actually do work.\n\n\
         RESPOND WITH TOOL CALLS, NOT DESCRIPTIONS — when work needs doing.\n\n\
         === HOW TO CALL TOOLS ===\n\
         Use this XML format:\n\n\
         <tool_use>\n\
           <tool_name>TOOL_NAME_HERE</tool_name>\n\
           <parameters>\n\
             <param1>value1</param1>\n\
           </parameters>\n\
         </tool_use>\n"
    )
}

fn realistic_recent_history() -> Vec<RecentMessage> {
    vec![
        RecentMessage {
            id: Uuid::new_v4(),
            sender_name: "Developer".to_string(),
            text: "morning team — anyone got energy for a quick design discussion?".to_string(),
        },
        RecentMessage {
            id: Uuid::new_v4(),
            sender_name: "CodeReview AI".to_string(),
            text: "Sure, what's the topic?".to_string(),
        },
        RecentMessage {
            id: Uuid::new_v4(),
            sender_name: "Developer".to_string(),
            text: "Trying to decide whether to put the agent loop in Rust or keep it in TS. \
                   The TS version has been a pain — token caps, parser fallbacks, retry logic \
                   all duplicated from what Rust already does in the cognition crate."
                .to_string(),
        },
        RecentMessage {
            id: Uuid::new_v4(),
            sender_name: "Teacher AI".to_string(),
            text:
                "What's the perceived cost of moving it? The agent loop is mostly orchestration — \
                   tool-call detection, dispatch, feed result back, re-call. The shape is similar \
                   on both sides."
                    .to_string(),
        },
        RecentMessage {
            id: Uuid::new_v4(),
            sender_name: "Developer".to_string(),
            text: "Tool dispatch is the hard part — Rust would either need to call back into TS \
                   (reverse IPC) or own the command dispatcher itself."
                .to_string(),
        },
    ]
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires local GGUF + DMR; cargo test --release --test persona_respond_replay -- --ignored --nocapture"]
async fn synthesized_prod_shape_input_produces_coherent_response() {
    ensure_llamacpp_registered().await;

    let system_prompt = realistic_system_prompt(
        "Helper AI",
        "General",
        &[
            "Developer",
            "Claude Code",
            "CodeReview AI",
            "Teacher AI",
            "Local Assistant",
        ],
    );
    let recent_history = realistic_recent_history();
    let message_text =
        "What's your gut take — is reverse-IPC for tool dispatch a pragmatic stepping stone, or \
         is it the kind of half-measure we'll regret in three months?"
            .to_string();

    eprintln!(
        "[synth-prod] system_prompt={} chars, recent_history={} messages, message_text={} chars",
        system_prompt.len(),
        recent_history.len(),
        message_text.len(),
    );

    let input = RespondInput {
        persona: PersonaSlot {
            persona_id: Uuid::new_v4(),
            specialty: "general".to_string(),
            display_name: "Helper AI".to_string(),
        },
        // Per-turn shared context (continuum#1206).
        turn_context: TurnContext::arc(
            Uuid::new_v4(),
            recent_history,
            vec![
                "general".to_string(),
                "code".to_string(),
                "learning".to_string(),
                "local".to_string(),
            ],
        ),
        message_id: Uuid::new_v4(),
        message_text,
        other_persona_names: Vec::new(),
        system_prompt,
        model: "continuum-ai/qwen3.5-4b-code-forged-GGUF".to_string(),
        is_voice: false,
        message_media: Vec::new(),
        capabilities: std::collections::HashSet::new(),
        recalled_engrams: Vec::new(),
    };
    let response = respond(input)
        .await
        .expect("respond() should not error on synthesized prod-shape input");
    assert_clean_spoke("synth-prod", &response);

    let text = match &response {
        PersonaResponse::Spoke { text, .. } => text,
        _ => unreachable!("assert_clean_spoke would have panicked"),
    };

    // Coherence assertions — live chat tonight produced "ie\n<|im_end|>",
    // a bare apostrophe, '@@@@@' runs. A real response should be made
    // of words.
    let alpha_chars = text.chars().filter(|c| c.is_alphabetic()).count();
    let total_chars = text.chars().count();
    let alpha_ratio = if total_chars > 0 {
        alpha_chars as f64 / total_chars as f64
    } else {
        0.0
    };
    assert!(
        alpha_ratio > 0.5,
        "[synth-prod] response is mostly non-alphabetic ({alpha_chars}/{total_chars} = {:.2}) — \
         model is emitting noise. Got:\n{text}",
        alpha_ratio
    );
    let word_count = text.split_whitespace().count();
    assert!(
        word_count >= 10,
        "[synth-prod] response is too short to be a real reply ({word_count} words). Got:\n{text}"
    );
    // The question is about reverse-IPC and Rust/TS migration. A real
    // coherent reply should reference at least one of those topics.
    let lower = text.to_lowercase();
    let has_topic_signal = lower.contains("rust")
        || lower.contains("ts")
        || lower.contains("typescript")
        || lower.contains("ipc")
        || lower.contains("tool")
        || lower.contains("dispatch")
        || lower.contains("agent")
        || lower.contains("migrat");
    assert!(
        has_topic_signal,
        "[synth-prod] response doesn't mention any topic from the question (rust/ts/ipc/tool/\
         dispatch/agent/migrat) — model didn't understand or didn't engage. Got:\n{text}"
    );
}

// ─── Test: replay the most recent fixture from prod ───────────────────────
//
// Best-effort: a contaminated fixture (history full of '<think>'-truncated
// junk and noise tokens from PRIOR broken responses) will make the model
// produce garbage even with the fixes — the model can't recover from
// poisoned context. This test passes if respond() returns SOMETHING (no
// panic, no IPC timeout, no parser explosion). Cleanliness is asserted
// by clean_minimal_input above. Once the fix is shipped and chat
// accumulates fresh fixtures, this test can tighten its assertions.

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires local GGUF + DMR; cargo test --release --test persona_respond_replay -- --ignored --nocapture"]
async fn replay_most_recent_fixture_does_not_panic_or_timeout() {
    ensure_llamacpp_registered().await;
    let fix = most_recent_fixture();

    let known_specialties = vec![
        fix.rust_request.specialty.clone(),
        "general".to_string(),
        "code".to_string(),
        "learning".to_string(),
        "local".to_string(),
    ];
    let input = build_input(&fix, known_specialties);
    // Tolerate Err — contaminated input legitimately makes the model
    // emit pure noise that the analyzer parser can't extract a JSON
    // envelope from. The bug we DO want this test to catch is
    // panics, deadlocks, or infinite loops — `await` returning at all
    // proves the path doesn't wedge.
    let result = respond(input).await;
    eprintln!(
        "[most-recent-fixture] result variant: {:?}",
        match &result {
            Ok(PersonaResponse::Spoke { text, .. }) => format!("Spoke({} chars)", text.len()),
            Ok(PersonaResponse::Silent { reason, .. }) => format!("Silent({reason})"),
            Err(e) => format!("Err({e})"),
        }
    );
}

// ─── Test: ask for a substantial response (no clip) ───────────────────────
//
// Joel's instruction: "make it code a huge thing". The cap regression
// only shows up when the model NEEDS more than the cap allows. A "hi"
// reply fits in 100 tokens; a "write a recursive descent parser in Rust
// with thorough comments" reply needs ~2000+ tokens. Prove the response
// arrives whole.

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires local GGUF + DMR; cargo test --release --test persona_respond_replay -- --ignored --nocapture"]
async fn long_code_generation_request_completes_without_clipping() {
    ensure_llamacpp_registered().await;
    // Re-use a fixture's system_prompt + persona — the bulky RAG context
    // is exactly what catches prod-only bugs (token-budget interactions
    // with prompt size, prompt-assembly behavior at 30K input chars).
    let fix = most_recent_fixture();

    // Override message_text and history with a code-generation ask. The
    // system_prompt + persona stay live so we exercise the same
    // prompt-assembly path the live system uses.
    let input = RespondInput {
        persona: PersonaSlot {
            persona_id: fix.rust_request.persona_id,
            specialty: fix.rust_request.specialty.clone(),
            display_name: fix.rust_request.persona_name.clone(),
        },
        // Per-turn shared context (continuum#1206).
        turn_context: TurnContext::arc(
            fix.rust_request.room_id,
            vec![],
            vec![
                fix.rust_request.specialty.clone(),
                "general".to_string(),
                "code".to_string(),
            ],
        ),
        message_id: Uuid::new_v4(),
        message_text: "Write a complete recursive descent parser in Rust for a small expression \
             language (numbers, +, -, *, /, parentheses). Include the AST types, the \
             tokenizer, the parser, and at least three unit tests. Use thorough comments \
             explaining grammar precedence and associativity decisions. Output the full \
             code, not a sketch."
            .to_string(),
        other_persona_names: Vec::new(),
        system_prompt: fix.rust_request.system_prompt.clone(),
        model: fix.rust_request.model.clone(),
        is_voice: false,
        message_media: Vec::new(),
        capabilities: std::collections::HashSet::new(),
        recalled_engrams: Vec::new(),
    };

    let response = respond(input)
        .await
        .expect("respond() should not error on long-code-gen ask");
    assert_clean_spoke("long-code-gen", &response);

    let text = match &response {
        PersonaResponse::Spoke { text, .. } => text,
        _ => unreachable!("assert_clean_spoke would have panicked"),
    };

    // The whole point: a substantial response. If this comes back at
    // <500 chars the model was clipped (or lazy — bump the prompt).
    assert!(
        text.len() > 500,
        "long-code-gen response was suspiciously short ({} chars) — likely max_tokens clipping. \
         Got:\n{text}",
        text.len()
    );
    // Smoke-check that the model actually attempted code generation
    // (mentions some token a parser implementation would have).
    let lower = text.to_lowercase();
    let has_code_signal = lower.contains("fn ")
        || lower.contains("struct ")
        || lower.contains("enum ")
        || lower.contains("impl ")
        || lower.contains("```");
    assert!(
        has_code_signal,
        "long-code-gen response lacks any code-shaped tokens (fn/struct/enum/impl/```) — \
         the model ignored the request. Got:\n{text}"
    );
}
