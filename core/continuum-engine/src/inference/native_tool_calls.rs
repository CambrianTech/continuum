//! A model's native tool calls, assembled once for every path that reads them: the streamed
//! OpenAI-compatible response (its `delta.tool_calls` fragments) and the in-process engine (the
//! model's own chat parser). One owner decides a call's fate, so a call reads the same whichever
//! engine served her turn.

use continuum_tool_protocol::{GenerationEnd, ToolCall, UnparsedArguments};
use serde_json::Value;

use crate::ai::types::ContentPart;

/// A response's content blocks: her visible text, then each call as a `ToolUse` part. `None` when
/// she wrote nothing and called nothing.
pub(crate) fn content_blocks(text: &str, calls: Option<&[ToolCall]>) -> Option<Vec<ContentPart>> {
    let mut blocks = Vec::new();
    if !text.is_empty() {
        blocks.push(ContentPart::Text { text: text.to_owned() });
    }
    for call in calls.into_iter().flatten() {
        blocks.push(ContentPart::ToolUse { id: call.id.clone(), name: call.name.clone(), input: call.input.clone() });
    }
    (!blocks.is_empty()).then_some(blocks)
}

use super::sse_stream::StreamToolAccum;

/// The calls a generation made, or `None` when it made none. A call with no name is no call. A
/// call whose arguments do not parse as JSON is a real failure of the generation, decided here, the
/// one seam that knows why it stopped: the output limit cut it (never a retry: the same call
/// re-cuts) or she wrote bad JSON (a retry). The fate is typed in the call, so no reader infers it
/// from a JSON EOF (`continuum_tool_protocol::call_ending`, ONE-ACT 6b). The old wrap of the text as
/// `{"_raw": …}` had no reader and destroyed the cause (#334).
pub(crate) fn assemble(raw: Vec<StreamToolAccum>, end: GenerationEnd) -> Option<Vec<ToolCall>> {
    let calls: Vec<ToolCall> = raw
        .into_iter()
        .filter(|call| !call.name.is_empty())
        .map(|call| {
            let arg_len = call.arguments.len();
            let input: Value = match UnparsedArguments::parse(call.arguments, end) {
                Ok(value) => value,
                Err(unparsed) => {
                    continuum_base::probe!(
                        class = "ai.tool_call.unparseable_args",
                        tool = call.name.as_str(),
                        cut = matches!(unparsed, UnparsedArguments::Cut { .. }),
                        arg_len,
                        // The whole blob rides in the call's input below; the probe names its size.
                        "model emitted tool arguments that are not valid JSON — the call cannot be honored as written",
                    );
                    unparsed.into_input()
                }
            };
            ToolCall { id: call.id, name: call.name, input }
        })
        .collect();
    (!calls.is_empty()).then_some(calls)
}
