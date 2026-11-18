//! Her turn through the model's own chat handling (serving S2b, slice 1): the request renders through
//! the GGUF's template with her tools, as llama-server would render it, and what the model writes is
//! parsed by the same render into content, reasoning and native tool calls.
//!
//! Owners reused, nothing parallel: the messages and tools are [`request_body::wire_messages`] and
//! [`request_body::openai_tools`] (what serving sends and the trainer renders a lived turn from), and
//! the calls are assembled by [`native_tool_calls::assemble`] (what the streamed path uses). One
//! [`llama::ChatRender`] serves the request from its render to its final parse.
//!
//! Not yet (slice 2): the render's grammar, lazy triggers, preserved tokens and extra stops into
//! the sampler. Until then no resident is routed here.

use continuum_tool_protocol::GenerationEnd;
use serde_json::{json, Value};

use super::*;
use crate::ai::types::{ToolCall, ToolChoice};
use crate::inference::native_tool_calls;
use crate::inference::request_body::{openai_tools, wire_messages};
use crate::inference::sse_stream::StreamToolAccum;

/// Why her turn could not go through the model's chat handling.
#[derive(Debug, thiserror::Error)]
pub(super) enum ChatTurnError {
    #[error(transparent)]
    Refused(#[from] llama::ChatRefusal),
    #[error("the chat render returned unreadable JSON: {0}")]
    Unreadable(#[from] serde_json::Error),
    #[error("the chat render describes no prompt")]
    NoPrompt,
    /// The model's own tool-choice parser takes a mode; a named tool is not forced through it yet.
    #[error("tool_choice naming {name:?} is not supported in-process yet")]
    SpecificToolChoice { name: String },
}

/// The OpenAI-shaped request her template renders: her wire messages, and her tools when she has
/// any, exactly as serving sends them.
pub(super) fn chat_request(request: &TextGenerationRequest) -> Result<Value, ChatTurnError> {
    let mut body = json!({
        "messages": wire_messages(&request.messages, request.system_prompt.as_deref(), false, false, LLAMACPP_PROVIDER_ID),
        "add_generation_prompt": true,
    });
    if let Some(tools) = request.tools.as_deref().filter(|tools| !tools.is_empty()) {
        body["tools"] = json!(openai_tools(tools));
        match &request.tool_choice {
            Some(ToolChoice::Mode(mode)) => body["tool_choice"] = json!(mode),
            Some(ToolChoice::Specific { name }) => return Err(ChatTurnError::SpecificToolChoice { name: name.clone() }),
            None => {}
        }
    }
    Ok(body)
}

/// The prompt the render describes, which the engine decodes from.
pub(super) fn prompt_of(render: &llama::ChatRender) -> Result<String, ChatTurnError> {
    let described: Value = serde_json::from_str(&render.describe()?)?;
    described.get("prompt").and_then(Value::as_str).map(str::to_owned).ok_or(ChatTurnError::NoPrompt)
}

/// What the model wrote, read by her render: the visible text, her reasoning, and her native calls.
pub(super) struct ParsedTurn {
    pub(super) text: String,
    pub(super) reasoning: Option<String>,
    pub(super) tool_calls: Option<Vec<ToolCall>>,
}

/// Parse the finished output with the render that produced its prompt. `end` is why the generation
/// stopped, so a call the output limit cut is typed as cut, never as bad JSON.
pub(super) fn parse_turn(render: &llama::ChatRender, written: &str, end: GenerationEnd) -> Result<ParsedTurn, ChatTurnError> {
    let message: Value = serde_json::from_str(&render.parse(written, false)?)?;
    let text_of = |field: &str| message.get(field).and_then(Value::as_str).filter(|text| !text.is_empty()).map(str::to_owned);
    let raw: Vec<StreamToolAccum> = message
        .get("tool_calls")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|call| StreamToolAccum {
            id: call.get("id").and_then(Value::as_str).map(str::to_owned).unwrap_or_default(), // unwrap_or_default: a call the template gave no id is still her call; the assembler keys on its name
            name: call.pointer("/function/name").and_then(Value::as_str).map(str::to_owned).unwrap_or_default(), // unwrap_or_default: a nameless call is dropped by the assembler, never run
            arguments: call.pointer("/function/arguments").and_then(Value::as_str).map(str::to_owned).unwrap_or_default(), // unwrap_or_default: no arguments parse as no JSON and are typed by the assembler
        })
        .collect();
    Ok(ParsedTurn { text: text_of("content").unwrap_or_default(), reasoning: text_of("reasoning_content"), tool_calls: native_tool_calls::assemble(raw, end) }) // unwrap_or_default: a turn that only called a tool has no visible text
}

/// Whether her turn carries media (an image, audio, or a video the media path refuses): only then
/// does it take the plain template's media path; every other turn renders through her chat render.
pub(super) fn carries_media(request: &TextGenerationRequest) -> bool {
    use crate::ai::types::ContentPart;
    request.messages.iter().any(|message| match &message.content {
        MessageContent::Parts(parts) => parts
            .iter()
            .any(|part| matches!(part, ContentPart::Image { .. } | ContentPart::Audio { .. } | ContentPart::Video { .. })),
        MessageContent::Text(_) => false,
    })
}

/// The response's finish reason: the output limit first, so a turn the cap cut (even mid-call) is
/// reported as cut, never as a finished call; then a tool call; then a stop.
pub(super) fn finish_reason(end: GenerationEnd, called: bool) -> FinishReason {
    match end {
        GenerationEnd::OutputLimit => FinishReason::Length,
        _ if called => FinishReason::ToolUse,
        _ => FinishReason::Stop,
    }
}

/// Why a decode of `tokens` with a cap of `max_tokens` stopped: the cap, or the model's own end.
pub(super) fn generation_end(tokens: usize, max_tokens: usize) -> GenerationEnd {
    if tokens >= max_tokens {
        GenerationEnd::OutputLimit
    } else {
        GenerationEnd::Finished
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(name: &str) -> crate::ai::types::NativeToolSpec {
        crate::ai::types::NativeToolSpec {
            name: name.to_owned(),
            description: format!("{name} does one thing"),
            input_schema: crate::ai::types::ToolInputSchema {
                schema_type: "object".to_owned(),
                properties: json!({}),
                required: None,
                definitions: None,
            },
        }
    }

    // what this catches: the in-process render seeing other messages or tools than serving sends
    // (a second mapping drifting from `wire_messages` / `openai_tools`), or tools rendered for a turn
    // that has none.
    #[test]
    fn her_turn_renders_the_wire_messages_and_tools_serving_sends() {
        let request = TextGenerationRequest {
            system_prompt: Some("be brief".into()),
            tools: Some(vec![spec("code_read")].into()),
            tool_choice: Some(ToolChoice::Mode("auto".into())),
            ..Default::default()
        };
        let body = chat_request(&request);
        let expected_messages = json!(wire_messages(&request.messages, Some("be brief"), false, false, LLAMACPP_PROVIDER_ID));
        assert!(matches!(&body, Ok(body) if body["messages"] == expected_messages), "{body:?}");
        assert!(matches!(&body, Ok(body) if body["tools"] == json!(openai_tools(&[spec("code_read")]))), "{body:?}");
        assert!(matches!(&body, Ok(body) if body["tool_choice"] == json!("auto")), "{body:?}");

        let bare = chat_request(&TextGenerationRequest::default());
        assert!(matches!(&bare, Ok(body) if body.get("tools").is_none() && body.get("tool_choice").is_none()), "{bare:?}");
    }

    // what this catches: a named tool choice silently widened to any tool (the model's own parser
    // takes a mode, not a name): refused until it is forced through the grammar (slice 2).
    #[test]
    fn a_named_tool_choice_is_refused_not_widened() {
        let request = TextGenerationRequest {
            tools: Some(vec![spec("code_read")].into()),
            tool_choice: Some(ToolChoice::Specific { name: "code_read".into() }),
            ..Default::default()
        };
        assert!(matches!(chat_request(&request), Err(ChatTurnError::SpecificToolChoice { name }) if name == "code_read"));
    }

    // what this catches: a call the output limit cut read as bad JSON (a retry that re-cuts the
    // same call) instead of as cut.
    #[test]
    fn a_decode_that_reached_its_cap_ended_at_the_output_limit() {
        assert_eq!(generation_end(64, 64), GenerationEnd::OutputLimit);
        assert_eq!(generation_end(12, 64), GenerationEnd::Finished);
    }

    // what this catches (Codex on #6145): a turn the output limit cut reported as a finished call or
    // a stop (unconditionally ToolUse/Stop), so a truncated call is retried as if complete.
    #[test]
    fn a_cut_turn_reports_its_length_even_mid_call() {
        assert!(matches!(finish_reason(GenerationEnd::OutputLimit, true), FinishReason::Length));
        assert!(matches!(finish_reason(GenerationEnd::OutputLimit, false), FinishReason::Length));
        assert!(matches!(finish_reason(GenerationEnd::Finished, true), FinishReason::ToolUse));
        assert!(matches!(finish_reason(GenerationEnd::Finished, false), FinishReason::Stop));
    }

    // what this catches (Codex on #6145): a text turn paying to build the plain template's messages
    // and media it discards, or a media turn (video included, which the media path refuses) taking
    // the chat render.
    #[test]
    fn only_a_turn_with_media_takes_the_media_path() {
        use crate::ai::types::{ChatMessage, ContentPart};
        let turn = |content: MessageContent| TextGenerationRequest {
            messages: vec![ChatMessage { role: "user".into(), content, name: None }],
            ..Default::default()
        };
        assert!(!carries_media(&turn(MessageContent::Text("hello".into()))));
        assert!(!carries_media(&turn(MessageContent::Parts(vec![ContentPart::Text { text: "hello".into() }]))));
        let video = crate::ai::types::VideoInput { url: None, base64: None, mime_type: None };
        assert!(carries_media(&turn(MessageContent::Parts(vec![ContentPart::Video { video }]))));
    }
}
