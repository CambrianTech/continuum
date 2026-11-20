//! Her turn through the model's own chat handling (serving S2b, slice 1): the request renders through
//! the GGUF's template with her tools, as llama-server would render it, and what the model writes is
//! parsed by the same render into content, reasoning and native tool calls.
//!
//! Owners reused, nothing parallel: the messages and tools are [`request_body::wire_messages`] and
//! [`request_body::openai_tools`] (what serving sends and the trainer renders a lived turn from), and
//! the calls are assembled by [`native_tool_calls::assemble`] (what the streamed path uses). One
//! [`llama::ChatRender`] serves the request from its render to its final parse.
//!
//! Into the sampler from the render: its extra stops, and a think budget bounded by its thinking tags
//! (the reasoning-budget sampler llama-server ran). Not yet: its grammar, lazy triggers and preserved
//! tokens (slice 2).

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
    /// Carries the choice she asked for, as she asked it.
    #[error("tool_choice {choice:?} is not supported in-process yet")]
    SpecificToolChoice { choice: ToolChoice },
}

/// What the render describes, read as a type: the prompt the engine decodes from, the generation
/// prompt it ends with, the template's extra stops, and its thinking tags.
#[derive(serde::Deserialize)]
pub(super) struct Described {
    prompt: Option<String>,
    #[serde(default)]
    generation_prompt: String,
    #[serde(default)]
    pub(super) additional_stops: Vec<String>,
    #[serde(default)]
    supports_thinking: bool,
    #[serde(default)]
    thinking_start_tag: String,
    #[serde(default)]
    thinking_end_tags: Vec<String>,
}

impl Described {
    /// The bound a think `budget` puts on this template, or `None` when it does not think (no start
    /// tag to count from, no end tag to force): a budget on it is moot.
    pub(super) fn think_bound(&self, budget: u32) -> Option<llama::ThinkBound> {
        let thinks = self.supports_thinking && !self.thinking_start_tag.is_empty() && !self.thinking_end_tags.is_empty();
        thinks.then(|| llama::ThinkBound { start_tag: self.thinking_start_tag.clone(), end_tags: self.thinking_end_tags.clone(), budget })
    }

    /// The prompt without its generation prompt: what a later act that keeps these messages holds
    /// unchanged. `None` when the prompt does not end in its own generation prompt.
    fn held(&self) -> Option<&str> {
        self.prompt.as_deref()?.strip_suffix(self.generation_prompt.as_str())
    }
}

/// What the render describes.
pub(super) fn described(render: &llama::ChatRender) -> Result<Described, ChatTurnError> {
    Ok(serde_json::from_str(&render.describe()?)?)
}

/// The text of her prompt her next act will hold unchanged: her view's messages
/// (`request.view_messages`, where cognition says her next act stops matching), rendered by the same
/// templates with the generation prompt taken off. The engine checkpoints her there, so a next act
/// that keeps it prefills only what follows (CONTINUUM-ENGINE 4.15). `None` for a turn with no view.
pub(super) fn view_text(
    request: &TextGenerationRequest,
    full: &Described,
    thinking: bool,
    render: impl FnOnce(&Value) -> Result<llama::ChatRender, ChatTurnError>,
) -> Result<Option<String>, ChatTurnError> {
    let Some(held) = request.view_messages.and_then(|held| usize::try_from(held).ok()) else { return Ok(None) };
    if held >= request.messages.len() {
        return Ok(full.held().map(str::to_owned));
    }
    let view = TextGenerationRequest { messages: request.messages[..held].to_vec(), ..request.clone() };
    let body = thinking_request(&view, thinking)?;
    Ok(described(&render(&body)?)?.held().map(str::to_owned))
}

/// The message her render parses from what the model wrote (common/chat's OpenAI-compatible
/// message): content, reasoning, and native calls, each absent or null when she wrote none.
#[derive(serde::Deserialize)]
struct ParsedMessage {
    content: Option<String>,
    reasoning_content: Option<String>,
    tool_calls: Option<Vec<ParsedCall>>,
}

#[derive(serde::Deserialize)]
struct ParsedCall {
    id: Option<String>,
    function: ParsedFunction,
}

#[derive(serde::Deserialize)]
struct ParsedFunction {
    name: Option<String>,
    arguments: Option<String>,
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
            Some(choice @ ToolChoice::Specific { .. }) => return Err(ChatTurnError::SpecificToolChoice { choice: choice.clone() }),
            None => {}
        }
    }
    if let Some(tail) = request.tail_tools.as_deref().filter(|tools| !tools.is_empty()) {
        body["tail_tools"] = json!(openai_tools(tail));
    }
    Ok(body)
}

/// Her request with the turn's thinking term: off only when the turn states a zero think budget, so
/// the template renders no thinking for it; otherwise the template's default, on.
pub(super) fn thinking_request(request: &TextGenerationRequest, thinking: bool) -> Result<Value, ChatTurnError> {
    let mut body = chat_request(request)?;
    body["enable_thinking"] = json!(thinking);
    Ok(body)
}

/// The prompt the render describes, which the engine decodes from.
pub(super) fn prompt_of(described: &Described) -> Result<String, ChatTurnError> {
    described.prompt.clone().ok_or(ChatTurnError::NoPrompt)
}

/// What the model wrote, read by her render: the visible text, her reasoning, and her native calls.
pub(super) struct ParsedTurn {
    pub(super) text: String,
    pub(super) reasoning: Option<String>,
    pub(super) tool_calls: Option<Vec<ToolCall>>,
}

impl ParsedTurn {
    /// A turn that kept no text and no call, told apart by its raw tail and the stop it ended on: a
    /// stop string firing where her call began, the model ending inside its thinking, or a call the
    /// parse dropped (Aiko, 20 turns from 15:36Z 2026-11-19, text 0, reasoning only).
    pub(super) fn probe_if_empty(&self, written: &str, tokens: usize, end: GenerationEnd, stops: &[String], request: &TextGenerationRequest) {
        if !self.text.is_empty() || self.tool_calls.as_ref().is_some_and(|calls| !calls.is_empty()) {
            return;
        }
        // Diagnostic only: correlate a Qwen function marker with this render's own request.
        // A marker in generated text is never authority to dispatch a call.
        let marker = written.rsplit_once("<function=").and_then(|(_, rest)| rest.split_once('>'))
            .map(|(name, _)| name)
            .filter(|name| !name.is_empty() && name.len() <= 128 && name.bytes().all(|b| b.is_ascii_alphanumeric() || b"/._-".contains(&b)));
        let offered = marker.map(|name| request.tools.as_deref().unwrap_or_default().iter()
            .chain(request.tail_tools.as_deref().unwrap_or_default()).any(|tool| tool.name == name));
        let chars: Vec<char> = written.chars().collect();
        let tail: String = chars[chars.len().saturating_sub(160)..].iter().collect();
        continuum_base::probe!(
            class = "inference.turn.wrote_nothing",
            persona = ?request.persona_id,
            function_marker = ?marker,
            offered_on_request = ?offered,
            tokens = tokens as u64,
            end = ?end,
            stop = ?stops.iter().find(|stop| written.ends_with(stop.as_str())),
            reasoning_chars = ?self.reasoning.as_ref().map(|r| r.chars().count()),
            written_chars = chars.len() as u64,
            tail = %tail,
            "a turn ended with no text and no call: the raw tail and the stop it ended on"
        );
    }
}

/// Parse the finished output with the render that produced its prompt. `end` is why the generation
/// stopped, so a call the output limit cut is typed as cut, never as bad JSON.
pub(super) fn parse_turn(render: &llama::ChatRender, written: &str, end: GenerationEnd) -> Result<ParsedTurn, ChatTurnError> {
    let message: ParsedMessage = serde_json::from_str(&render.parse(written, false)?)?;
    let written_text = |text: Option<String>| text.filter(|text| !text.is_empty());
    let raw = message.tool_calls.into_iter().flatten().map(|call| StreamToolAccum {
        // A template that names no call id (Qwen's) gets one minted here, as llama-server minted one
        // for its /v1 reply: her result must answer an id, and an empty one is refused downstream
        // (the M5 at 6d1396e62f, 06:21-06:26Z: 19 turns refused as mismatched call/result ids).
        id: call.id.filter(|id| !id.is_empty()).unwrap_or_else(|| format!("call_{}", uuid::Uuid::new_v4().simple())), // unwrap_or_else: an unnamed call gets a fresh unique id
        name: call.function.name.unwrap_or_default(), // unwrap_or_default: a nameless call is dropped by the assembler, never run
        arguments: call.function.arguments.unwrap_or_default(), // unwrap_or_default: no arguments parse as no JSON and are typed by the assembler
    });
    Ok(ParsedTurn {
        text: written_text(message.content).unwrap_or_default(), // unwrap_or_default: a turn that only called a tool has no visible text
        reasoning: written_text(message.reasoning_content),
        tool_calls: native_tool_calls::assemble(raw, end),
    })
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
        assert!(matches!(
            chat_request(&request),
            Err(ChatTurnError::SpecificToolChoice { choice: ToolChoice::Specific { name } }) if name == "code_read"
        ));
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
