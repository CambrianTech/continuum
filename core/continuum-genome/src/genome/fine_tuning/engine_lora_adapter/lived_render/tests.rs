use super::*;
use crate::ai::types::{TextGenerationRequest, TextGenerationResponse};
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
    let Err(unfit) = render_at(&call, 10) else { panic!("test: 10 tokens cannot hold it") };
    assert_eq!(unfit.grant, 10);
    assert!(unfit.needed > 10, "{unfit:?}");
    assert!(render_at(&call, unfit.needed).is_ok(), "exactly what it said it needs is enough");
}
