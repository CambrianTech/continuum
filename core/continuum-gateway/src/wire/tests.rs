//! Each format round-trips an agent's tool-using turn, whole and streamed.
use super::anthropic::Anthropic;
use super::chat::Chat;
use super::responses::Responses;
use super::*;
use continuum_engine::ai::{MessageContent, ToolCall, UsageMetrics};
use serde_json::json;

/// An answer that says a word and calls one tool.
fn tool_answer() -> TextGenerationResponse {
    TextGenerationResponse {
        text: "Reading it.".into(),
        finish_reason: FinishReason::ToolUse,
        model: "qwen".into(),
        provider: "local".into(),
        usage: UsageMetrics { input_tokens: 120, output_tokens: 9, total_tokens: 129, estimated_cost: None },
        response_time_ms: 0,
        request_id: "r1".into(),
        content: None,
        tool_calls: Some(vec![ToolCall { id: "call_1".into(), name: "read_file".into(), input: json!({"path": "a.rs"}) }]),
        reasoning: None,
        routing: None,
        error: None,
        timing: None,
    }
}

fn parts(m: &continuum_engine::ai::ChatMessage) -> &[ContentPart] {
    match &m.content {
        MessageContent::Parts(parts) => parts,
        MessageContent::Text(_) => panic!("test: expected parts in {m:?}"),
    }
}

// what this catches: Claude Code's tools and tool results reaching the engine as text, or a
// tool call coming back without its id (Claude Code then cannot answer it).
#[test]
fn anthropic_carries_tools_both_ways_and_streams_blocks_in_order() {
    let call = Anthropic::decode(json!({
        "model": "qwen", "max_tokens": 512, "stream": true,
        "system": [{"type": "text", "text": "You are a coder."}],
        "tools": [{"name": "read_file", "description": "Read", "input_schema": {"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]}}],
        "messages": [
            {"role": "user", "content": "open a.rs"},
            {"role": "assistant", "content": [{"type": "tool_use", "id": "call_0", "name": "read_file", "input": {"path": "b.rs"}}]},
            {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "call_0", "content": [{"type": "text", "text": "fn b() {}"}]}]},
        ],
    }))
    .expect("test: decodes");
    assert!(call.stream);
    assert_eq!(call.request.system_prompt.as_deref(), Some("You are a coder."));
    assert_eq!(call.request.tools.as_ref().map(|t| t[0].input_schema.required.clone()), Some(Some(vec!["path".to_string()])));
    assert!(matches!(&parts(&call.request.messages[1])[0], ContentPart::ToolUse { id, .. } if id == "call_0"));
    assert!(matches!(&parts(&call.request.messages[2])[0], ContentPart::ToolResult { content, .. } if content == "fn b() {}"));

    let whole = Anthropic::answer(&tool_answer(), "msg_1");
    assert_eq!(whole["stop_reason"], "tool_use");
    assert_eq!(whole["content"][1]["id"], "call_1");

    let mut s = Anthropic::streamer("qwen", "msg_1");
    let mut frames = s.opened();
    frames.extend(s.chunk(&GenerationChunk::Reasoning("hmm".into())));
    frames.extend(s.chunk(&GenerationChunk::Token("Reading".into())));
    frames.extend(s.chunk(&GenerationChunk::Token(" it.".into())));
    frames.extend(s.finished(&tool_answer()));
    let names: Vec<_> = frames.iter().map(|f| f.name.clone().expect("test: named")).collect();
    assert_eq!(names, [
        "message_start",
        "content_block_start", "content_block_delta", "content_block_stop", // thinking, index 0
        "content_block_start", "content_block_delta", "content_block_delta", "content_block_stop", // text, index 1
        "content_block_start", "content_block_delta", "content_block_stop", // tool_use, index 2
        "message_delta", "message_stop",
    ]);
    assert_eq!(frames[4].json()["index"], 1);
    assert_eq!(frames[8].json()["content_block"]["id"], "call_1");
    assert_eq!(frames[11].json()["delta"]["stop_reason"], "tool_use");
}

// what this catches: a mid-conversation system message reaching a template that rejects it,
// tool-call arguments left as a string, and a stream that never terminates.
#[test]
fn chat_completions_keeps_one_leading_system_and_ends_its_stream() {
    let call = Chat::decode(json!({
        "model": "qwen",
        "messages": [
            {"role": "system", "content": "Be terse."},
            {"role": "user", "content": "go"},
            {"role": "assistant", "content": null, "tool_calls": [{"id": "call_0", "type": "function", "function": {"name": "ls", "arguments": "{\"dir\":\".\"}"}}]},
            {"role": "tool", "tool_call_id": "call_0", "content": "a.rs"},
            {"role": "system", "content": "Budget: 10 steps left."},
        ],
        "tools": [{"type": "function", "function": {"name": "ls", "parameters": {"type": "object", "properties": {"dir": {"type": "string"}}}}}],
        "tool_choice": "required",
    }))
    .expect("test: decodes");
    assert_eq!(call.request.system_prompt.as_deref(), Some("Be terse."));
    assert!(matches!(&parts(&call.request.messages[1])[0], ContentPart::ToolUse { input, .. } if input["dir"] == "."));
    assert!(matches!(&parts(&call.request.messages[2])[0], ContentPart::ToolResult { tool_use_id, .. } if tool_use_id == "call_0"));
    assert_eq!(call.request.messages[3].role, "user", "a later system message is not sent as system");
    assert!(matches!(&call.request.tool_choice, Some(ToolChoice::Mode(m)) if m == "any"));

    let whole = Chat::answer(&tool_answer(), "chatcmpl_1");
    assert_eq!(whole["choices"][0]["finish_reason"], "tool_calls");
    assert_eq!(whole["choices"][0]["message"]["tool_calls"][0]["function"]["arguments"], "{\"path\":\"a.rs\"}");

    let mut s = Chat::streamer("qwen", "chatcmpl_1");
    let mut frames = s.opened();
    frames.extend(s.chunk(&GenerationChunk::Token("Reading it.".into())));
    frames.extend(s.finished(&tool_answer()));
    assert_eq!(frames.last(), Some(&Frame::done()));
    assert_eq!(frames[2].json()["choices"][0]["delta"]["tool_calls"][0]["index"], 0);
    assert_eq!(frames[3].json()["choices"][0]["finish_reason"], "tool_calls");
    assert_eq!(frames[4].json()["usage"]["prompt_tokens"], 120);
}

// what this catches: Grok Build's and Codex's items mis-grouped (two calls as two assistant
// turns), a stale `previous_response_id` answered without its history, and an event stream
// whose sequence numbers skip or that never completes.
#[test]
fn responses_groups_items_refuses_missing_state_and_numbers_its_events() {
    let refused = Responses::decode(json!({"model": "qwen", "input": "hi", "previous_response_id": "resp_9"}));
    assert!(refused.is_err(), "no session memory yet: refuse, never answer without history");

    let call = Responses::decode(json!({
        "model": "qwen", "instructions": "You are Grok.", "stream": true,
        "input": [
            {"role": "user", "content": [{"type": "input_text", "text": "fix it"}]},
            {"type": "function_call", "call_id": "c1", "name": "read", "arguments": "{\"p\":\"a\"}"},
            {"type": "function_call", "call_id": "c2", "name": "read", "arguments": "{\"p\":\"b\"}"},
            {"type": "function_call_output", "call_id": "c1", "output": "A"},
            {"type": "function_call_output", "call_id": "c2", "output": "B"},
            {"type": "reasoning", "summary": []},
        ],
        "tools": [{"type": "function", "name": "read", "parameters": {"type": "object", "properties": {"p": {"type": "string"}}}}, {"type": "web_search"}],
    }))
    .expect("test: decodes");
    assert_eq!(call.request.system_prompt.as_deref(), Some("You are Grok."));
    assert_eq!(call.request.messages.len(), 4, "user, one assistant turn with both calls, two tool replies");
    assert_eq!(parts(&call.request.messages[1]).len(), 2);
    assert_eq!(call.request.tools.as_ref().map(Vec::len), Some(1), "provider built-ins are not ours");

    let mut s = Responses::streamer("qwen", "resp_1");
    let mut frames = s.opened();
    frames.extend(s.chunk(&GenerationChunk::Token("Reading".into())));
    frames.extend(s.chunk(&GenerationChunk::Token(" it.".into())));
    frames.extend(s.finished(&tool_answer()));
    let sequence: Vec<u64> = frames.iter().map(|f| f.json()["sequence_number"].as_u64().expect("test: numbered")).collect();
    assert_eq!(sequence, (0..frames.len() as u64).collect::<Vec<_>>());
    let last = frames.last().expect("test: frames").json();
    assert_eq!(last["type"], "response.completed");
    assert_eq!(last["response"]["output"][0]["content"][0]["text"], "Reading it.");
    assert_eq!(last["response"]["output"][1]["call_id"], "call_1");
}
