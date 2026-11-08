use super::*;
use crate::ai::types::{TextGenerationRequest, TextGenerationResponse};
use crate::genome::fine_tuning::types::TrainingExample;
use serde_json::json;

/// A lived call: `history` exchanges of `each` characters (the first one a tool call and its
/// result), then the turn she answered and her reply. The engine "counted" its prompt at one
/// token per three characters, the density every block is priced at.
fn lived(history: usize, each: usize) -> LivedCall {
    let body = |i: usize| (0..each / 40).map(|line| format!("line {line} of exchange {i}: some output\n")).collect::<String>();
    let mut messages = vec![
        ChatMessage {
            role: "assistant".into(),
            content: MessageContent::Parts(vec![ContentPart::ToolUse { id: "t0".into(), name: "code/run".into(), input: json!({"cmd": "cargo test"}) }]),
            name: None,
        },
        ChatMessage {
            role: "user".into(),
            content: MessageContent::Parts(vec![ContentPart::ToolResult { tool_use_id: "t0".into(), content: body(0), is_error: None }]),
            name: None,
        },
    ];
    for i in 1..history {
        messages.push(ChatMessage::text(if i % 2 == 0 { "assistant" } else { "user" }, &body(i)));
    }
    messages.push(ChatMessage::text("assistant", "I'll look at the card."));
    messages.push(ChatMessage::text("user", "card: fix the failing test in a.rs"));
    let request = TextGenerationRequest { messages, system_prompt: Some("you are Kimi".into()), ..Default::default() };
    let sent = request.messages.iter().map(|m| count(&wire(m))).sum::<usize>();
    let response = TextGenerationResponse {
        text: "reading a.rs first".into(),
        finish_reason: crate::ai::FinishReason::Stop,
        model: "m".into(),
        provider: "p".into(),
        usage: crate::ai::UsageMetrics { input_tokens: (sent / 3) as u32, output_tokens: 6, total_tokens: 0, estimated_cost: None },
        response_time_ms: 0,
        request_id: "r".into(),
        content: None,
        tool_calls: None,
        reasoning: None,
        routing: None,
        error: None,
        timing: None,
    };
    LivedCall { capture: "capture-1".into(), request, response }
}

/// A message as `/train` receives it, for comparing what was sent with what was rendered.
fn wire(message: &ChatMessage) -> String {
    serde_json::to_string(message).unwrap_or_else(|e| panic!("test: {e}")) // /train HTTP wire form; a test fails loudly if it cannot serialize
}

fn newest(call: &LivedCall) -> Vec<String> {
    let at = newest_start(&call.request.messages);
    call.request.messages[at..].iter().map(wire).collect()
}

// what this catches (card 1c071615): a capture that fits its grant changed on its way to
// training. Rendered at a grant it fits whole, it is exactly the call serving sent.
#[test]
fn a_capture_that_fits_is_rendered_exactly_as_served() {
    let call = lived(6, 400);
    let Ok(rendered) = render_at(&call, 1_000_000) else { panic!("test: it fits") };
    assert_eq!(rendered.collapsed, 0);
    let sent: Vec<String> = call.request.messages.iter().map(wire).collect();
    let got: Vec<String> = rendered.messages.iter().map(wire).collect();
    assert_eq!(got, sent);
}

// what this catches (Kimi's 27B on the Windows node, 2026-11-08): a capture sized by a larger
// window than the training node grants was parked whole. At a tight grant it renders within it:
// the oldest exchanges collapse first, the turn she answered stays byte for byte, and a tool call
// keeps the result that answers it (one exchange, so no level separates them).
#[test]
fn a_tight_grant_collapses_the_oldest_first_and_keeps_the_newest_turn_whole() {
    let call = lived(12, 4_000);
    let Ok(whole) = render_at(&call, 1_000_000) else { panic!("test: whole") };
    let grant = whole.cost / 3;
    let Ok(rendered) = render_at(&call, grant) else { panic!("test: it renders within {grant}") };
    assert!(rendered.cost <= grant, "{} within {grant}", rendered.cost);
    assert!(rendered.collapsed > 0 && rendered.collapsed < rendered.exchanges, "some, not all, collapse: {}/{}", rendered.collapsed, rendered.exchanges);
    let tail: Vec<String> = rendered.messages[rendered.messages.len() - newest(&call).len()..].iter().map(wire).collect();
    assert_eq!(tail, newest(&call), "the turn she answered is whole");
    let text = wire;
    let first = text(&rendered.messages[1]);
    assert!(first.contains("\"tool_use_id\":\"t0\"") && text(&rendered.messages[0]).contains("\"id\":\"t0\""), "the collapsed tool result still answers its call");
    // the oldest long exchange against the newest long one (the last history message is her short
    // "I'll look at the card.", whole at any level)
    let oldest_kept = count(&text(&rendered.messages[2]));
    let newest_kept = count(&text(&rendered.messages[rendered.messages.len() - newest(&call).len() - 2]));
    assert!(oldest_kept <= newest_kept, "an older exchange never keeps more than a newer one: {oldest_kept} vs {newest_kept}");
}

// what this catches: a grant too small for what cannot collapse answered with a smaller render
// that was not this lesson. It is Unfit, with what it needs and what it was granted.
#[test]
fn a_grant_below_what_cannot_collapse_is_unfit_with_the_numbers() {
    let call = lived(12, 4_000);
    let Err(NotRendered::Unfit { needed, grant }) = render_at(&call, 10) else { panic!("test: 10 tokens cannot hold it") };
    assert_eq!(grant, 10);
    assert!(needed > 10, "{needed}");
    assert!(render_at(&call, needed).is_ok(), "exactly what it said it needs is enough");
}

// what this catches (Codex on #5881): a capture without the engine's count of its prompt priced
// at a made-up density (`input_tokens.max(1)`). It has no price, so it is not rendered.
#[test]
fn a_capture_without_the_engines_count_is_unmeasured() {
    let mut call = lived(12, 4_000);
    call.response.usage.input_tokens = 0;
    assert_eq!(render_at(&call, 1_000_000).err(), Some(NotRendered::Unmeasured));
    let example = TrainingExample { prompt: String::new(), completion: String::new(), metadata: None, lived: Some(call) };
    assert_eq!(smallest_window(&example), None, "no price, no collapsed floor: its measured length decides");
}

// what this catches (Codex on #5881): her newest act was a tool call and the turn she answered is
// only its results. Split after her last message, the call itself sat in the history and could
// collapse away from its results; it is part of the newest exchange and stays whole.
#[test]
fn her_newest_tool_call_stays_whole_with_its_results() {
    let mut call = lived(12, 4_000);
    let messages = &mut call.request.messages;
    messages.truncate(messages.len() - 2);
    messages.push(ChatMessage {
        role: "assistant".into(),
        content: MessageContent::Parts(vec![ContentPart::ToolUse { id: "t9".into(), name: "code/read".into(), input: json!({"path": "a.rs"}) }]),
        name: None,
    });
    messages.push(ChatMessage {
        role: "user".into(),
        content: MessageContent::Parts(vec![ContentPart::ToolResult { tool_use_id: "t9".into(), content: "fn a() {}\n".repeat(200), is_error: None }]),
        name: None,
    });
    assert_eq!(newest(&call).len(), 2, "the newest exchange is her call and its results");
    let Ok(whole) = render_at(&call, 1_000_000) else { panic!("test: whole") };
    let Ok(rendered) = render_at(&call, whole.cost / 3) else { panic!("test: it renders") };
    assert!(rendered.collapsed > 0);
    let tail: Vec<String> = rendered.messages[rendered.messages.len() - 2..].iter().map(wire).collect();
    assert_eq!(tail, newest(&call), "her call and its results, byte for byte");
}
