//! Request body — the tail of chat-body assembly: the per-gateway thinking policy, the
//! structured-output format, the native tool surface, and the wire-truth probe. Carved
//! out of `openai_adapter::generate_stream` (pure code-motion, 2026-09-03, the S3b
//! decompose). Behaviour-identical to the inline block. The head of assembly
//! (`format_messages` → base body → sampling knobs) is the next carve.

use serde_json::{json, Value};

use crate::ai::openai_adapter::{OpenAICompatibleConfig, ThinkingMode};
use crate::ai::types::{ChatMessage, ContentPart, MessageContent, TextGenerationRequest, ToolChoice};
use crate::model_registry::Capability;

/// Set `chat_template_kwargs.enable_thinking = false` on a built request body — the
/// ROBUST thinking-suppression lever for qwen3-family chat templates. Where
/// `apply_no_think_switch` appends a soft text token (which a forged template may
/// ignore entirely), this drives the template's own `enable_thinking` branch so it
/// emits an empty `<think></think>` and the model goes straight to content. Inserting
/// at the body's top level (not inside an existing kwargs map) is correct for the
/// llama.cpp servers we target; idempotent — overwrites its own prior value.
/// Harmless where unsupported: cloud providers ignore unknown body fields and a
/// template without `enable_thinking` ignores the kwarg.
pub(crate) fn apply_enable_thinking_false(body: &mut Value) {
    if let Some(obj) = body.as_object_mut() {
        obj.insert(
            "chat_template_kwargs".to_string(),
            json!({ "enable_thinking": false }),
        );
    }
}

/// Finish `body` for the wire: thinking suppression, `response_format`, native tools +
/// `tool_choice` (NativeFunctionCalling gateways only), then the tool-surface probe.
/// The purpose a work (ACT) turn announces on its request; the faculty sets it
/// when the hands surface is offered.
pub const ACT_PURPOSE: &str = "cognition/act";

// context-budget-exempt: a budget for the model's REASONING channel on an act turn, not a window bound
/// How much a reasoning model may think before its tool call on an ACT turn.
/// Measured 2026-09-07 (Qwen3.8-27B off-box): committed acts ran 4,188–9,818
/// output tokens, nearly all of it thinking, 150–260 s per act on a shared lane
/// (card 12ef9c10). A tool call plus its sentence needs a few hundred tokens of
/// thought, not thousands. llama-server reads `reasoning_budget_tokens` and
/// closes the thinking block at the budget; other gateways ignore the field.
pub(crate) const ACT_REASONING_BUDGET: u32 = 1024;

/// The purpose a deliberation (speak / pass) turn announces on its request.
pub const DELIBERATION_PURPOSE: &str = "cognition/deliberation";

/// A FORK's act and deliberation: a measurement copy of a mind (an eval exam) acting
/// beside the living one. The same reasoning budgets as [`ACT_PURPOSE`] and
/// [`DELIBERATION_PURPOSE`]; only the page differs: a fork warms its own page per room,
/// never the mind's one page (`slots::PageKey::Fork`), and never waits on her act gate.
/// An external agent's call through the driver gateway: a Turn of its session's mind, so the
/// session keeps a warm, paged page exactly as a resident does (DRIVER-GATEWAY.md G2).
pub const DRIVER_PURPOSE: &str = "driver";
pub const FORK_ACT_PURPOSE: &str = "cognition/fork-act";
pub const FORK_DELIBERATION_PURPOSE: &str = "cognition/fork-deliberation";

/// A sensory description needs answer text within its existing completion allowance.
/// Keep it distinct from persona deliberation for routing and diagnostics.
pub const VISION_DESCRIPTION_PURPOSE: &str = "cognition/vision-describe";

/// The share of a deliberation turn's allowance the ANSWER keeps once thinking is
/// bounded: three quarters may be thought, one quarter is left for what she says.
// derived-or-floor: a share of the turn's own allowance (`output_allowance`, measured need
// under the lane's reserve), never a token count — a capable model with a large allowance
// thinks proportionally more. The quarter is the floor the answer can never lose.
pub(crate) const DELIBERATION_ANSWER_SHARE_DIVISOR: u64 = 4;

/// Bound the reasoning channel on a DELIBERATION request from the allowance the turn
/// already carries. Returns the budget it applied.
///
/// Until now only an act carried a budget, and a deliberation could spend its whole
/// allowance thinking: the generation ended inside the reasoning channel with no answer
/// and no call (`persona.act.think_only`, the think-only sentinel). Kimi on the 5090 lost
/// whole turns that way on 2026-09-26 while composing an answer (`[health] think-only`);
/// the control on the M5 the same morning showed the budget BINDS on our llama.cpp build
/// (budget 64 → 73 completion tokens with an answer; no budget → 287 tokens, empty). The
/// budget is derived from her own `max_tokens`, so it is not a clamp on capable models:
/// it only guarantees the answer its share of what the turn was already allowed.
///
/// ONE seam for both kinds (Fable's review of #4413): an ACT thinks the fixed
/// [`ACT_REASONING_BUDGET`] before its call; a DELIBERATION turn thinks
/// [`deliberation_reasoning_budget`] of its own allowance, the answer keeping its share —
/// and vision descriptions reuse that same answer share. This applies only when
/// the body carries an allowance to derive it from. Any other purpose is
/// left to the model. Returns the budget it applied.
pub fn apply_reasoning_budget(purpose: Option<&str>, body: &mut Value) -> Option<u64> {
    let obj = body.as_object_mut()?;
    let budget = match purpose {
        Some(ACT_PURPOSE | FORK_ACT_PURPOSE) => u64::from(ACT_REASONING_BUDGET),
        Some(DELIBERATION_PURPOSE | FORK_DELIBERATION_PURPOSE | VISION_DESCRIPTION_PURPOSE) => {
            deliberation_reasoning_budget(obj.get("max_tokens")?.as_u64()?)?
        }
        _ => return None,
    };
    obj.insert("reasoning_budget_tokens".to_string(), json!(budget));
    Some(budget)
}

/// THE ONE RULE for a deliberation turn's reasoning budget, read by the request builder
/// (to set it) and by the emission classifier (to know a think that stopped AT it was
/// censored by it, not measured): three quarters of the allowance; `None` when the
/// allowance is too small for the answer to keep a quarter.
pub fn deliberation_reasoning_budget(max_tokens: u64) -> Option<u64> {
    let answer_share = max_tokens / DELIBERATION_ANSWER_SHARE_DIVISOR;
    if answer_share == 0 {
        return None;
    }
    Some(max_tokens - answer_share)
}

/// `tail_tools`: whether the engine this request is routed to takes our fork's `tail_tools`
/// (the adapter's `route_now`, decided with the URL from one snapshot). A request that carries
/// a tail the body cannot send is refused as [`InferenceError::TailToolsUnreachable`]: dropping
/// it would leave her a head without verbs she was told she could call.
///
/// [`InferenceError::TailToolsUnreachable`]: crate::ai::inference_error::InferenceError::TailToolsUnreachable
pub(crate) fn finish_body(
    cfg: &OpenAICompatibleConfig,
    request: &TextGenerationRequest,
    model: &str,
    tail_tools: bool,
    body: &mut Value,
) -> Result<(), crate::ai::inference_error::InferenceError> {
    // Thinking suppression — the REAL lever for qwen3-family forged templates.
    // When this gateway suppresses reasoning, the adapter already appends the
    // `/no_think` soft-switch to the last user turn (build path above). But the
    // forged qwen3.5 chat template implements `enable_thinking`, NOT the
    // `/no_think` text token — so the soft-switch is a NO-OP for it, and absent
    // the kwarg the template's default branch OPENS `<think>` itself, forcing the
    // model to reason. Verified empirically 2026-06-27 on the CPU eval lane: the
    // 4B forged model spent its whole ~90-token budget in the `reasoning` channel
    // and emitted EMPTY `content` (`finish_reason: stop`), so every settled answer
    // was blank and base/gene/lift were all 0.0 — a broken measurement, not a real
    // null result. The chat-template hatch `enable_thinking=false` makes the
    // template emit an empty `<think></think>` so the model goes straight to
    // content. Set it for ALL turns under suppression (not only the JSON branch
    // below, which is where it used to be misgated). Harmless where unsupported:
    // cloud providers ignore unknown body fields; a template without
    // `enable_thinking` ignores the kwarg. The `/no_think` switch is left in place
    // for any template that DOES honor the soft token.
    if cfg.thinking == ThinkingMode::Suppress {
        apply_enable_thinking_false(body);
    }
    // The reasoning channel is bounded by the turn's kind, whichever gateway serves it
    // — the remote path builds this same body on the responder. An ACT thinks a fixed
    // amount before its call; a DELIBERATION turn keeps its answer's share of its own
    // allowance, so a turn can no longer end inside the reasoning channel with nothing
    // said.
    if let Some(budget) = apply_reasoning_budget(request.purpose.as_deref(), body) {
        let budget_class = match request.purpose.as_deref() {
            Some(ACT_PURPOSE | FORK_ACT_PURPOSE) => "delib.act.reasoning_budgeted",
            Some(VISION_DESCRIPTION_PURPOSE) => "vision.description.reasoning_budgeted",
            _ => "delib.pass.reasoning_budgeted",
        };
        continuum_base::probe!(
            class = budget_class,
            model = %model,
            budget,
            max_tokens = request.max_tokens.map(u64::from).unwrap_or(0), // unwrap_or: an act's fixed budget needs no allowance; absent is said as 0
            "reasoning channel bounded by the turn's kind — the answer keeps its room"
        );
    }

    // Forward response_format when set. Llama.cpp/DMR DO grammar-constrain
    // JSON output, but for qwen3.5 reasoning models the model still
    // emits its <think> reasoning BEFORE the constrained JSON region,
    // which is no help to a JSON parser. Verified empirically 2026-04-19:
    // `response_format=json_object` alone returns "<think>\nThinking
    // Process:..." with no JSON.
    if let Some(format) = &request.response_format {
        if let Ok(value) = serde_json::to_value(format) {
            body["response_format"] = value;

            // qwen3-family-specific kicker: when caller asks for JSON,
            // ALSO disable thinking via the chat_template_kwargs hatch.
            // Verified the same model returns "<think></think>\n\n{...JSON...}"
            // in 434ms with this flag set — empty think block, clean JSON,
            // parser-friendly. Same lever the suppression path above uses, so
            // it routes through the same helper (one place sets the kwarg).
            // Idempotent if suppression already set it.
            apply_enable_thinking_false(body);
        }
        // The completed body is encoded once at dispatch. Its size and native
        // surface are probed there; exact request inspection uses the capture
        // owner rather than serializing another full body into a log message.
    }

    // Add tools via the native OpenAI `tools` param — ONLY for
    // NativeFunctionCalling providers. JsonInPrompt providers already had
    // the tools described in the prompt above (sending the param too would
    // be ignored or confuse them).
    if let Some(tools) = &request.tools {
        if !tools.is_empty()
            && cfg.capabilities.contains(&Capability::ToolUse)
            && cfg.tool_protocol
                == crate::model_registry::ToolProtocol::NativeFunctionCalling
        {
            body["tools"] = json!(openai_tools(tools));
            if let Some(tail) = request.tail_tools.as_deref().filter(|tail| !tail.is_empty()) {
                // Our fork's field: only an engine verified to take it gets it (a cloud provider
                // would reject an unknown field, a stock llama-server would ignore it).
                if tail_tools {
                    body["tail_tools"] = json!(openai_tools(tail));
                }
            }

            // Add tool_choice if specified
            if let Some(choice) = &request.tool_choice {
                match choice {
                    ToolChoice::Mode(mode) => {
                        body["tool_choice"] = json!(mode);
                    }
                    ToolChoice::Specific { name } => {
                        body["tool_choice"] = json!({
                            "type": "function",
                            "function": { "name": name }
                        });
                    }
                }
            }
        }
    }

    // Wire truth for the tool surface (glass-box, 2026-08-03): live residents
    // narrated for hours with zero tool calls while every offline replay of the
    // same context+tools+sampling called instantly — the ONLY remaining unknown
    // was what this body actually carried. This probe states it per request so
    // "tools offered" is never inferred from a capture again.
    continuum_base::probe!(
        class = "ai.request.tool_surface",
        model = %model,
        tools_n = body.get("tools").and_then(|t| t.as_array()).map_or(0, |a| a.len()),
        tail_tools_n = body.get("tail_tools").and_then(|t| t.as_array()).map_or(0, |a| a.len()),
        tool_choice = body.get("tool_choice").is_some(),
        stops_n = body.get("stop").and_then(|s| s.as_array()).map_or(0, |a| a.len()),
        msgs_n = body.get("messages").and_then(|m| m.as_array()).map_or(0, |a| a.len()),
        temperature = body.get("temperature").and_then(|t| t.as_f64()).unwrap_or(-1.0),
        "outbound chat request tool surface"
    );
    let carried_tail = request.tail_tools.as_deref().is_some_and(|tail| !tail.is_empty());
    if carried_tail && body.get("tail_tools").is_none() {
        // Every path that leaves a requested tail off the wire lands here: refused, never dropped.
        continuum_base::probe!(
            class = "ai.request.tail_tools_unreachable",
            model = %model,
            tail_tools_accepted = tail_tools,
            "the request carries tail verbs its destination cannot take: refused for a rebuild"
        );
        return Err(crate::ai::inference_error::InferenceError::TailToolsUnreachable { model: model.to_owned() });
    }
    Ok(())
}

// ── the head of assembly (carve 4): messages → base body ─────────────────────────

/// Append Qwen3's `/no_think` soft-switch to the LAST user message in a built
/// OpenAI message array, suppressing chain-of-thought for the turn (the model emits
/// an empty `<think></think>` then answers directly — which [`extract_reasoning`]
/// reduces to clean text + no reasoning). Operates on string content (chat turns);
/// multimodal/array content is left untouched (a follow-up can append a text part).
/// No user message → no-op.
/// The OpenAI `tools` param for a set of tool specs. One mapping: serving sends it,
/// and the engine trainer sends the same list so a lived turn renders with the tool
/// block it was served with.
pub fn openai_tools(tools: &[crate::ai::types::NativeToolSpec]) -> Vec<Value> {
    tools
        .iter()
        .map(|tool| {
            json!({
                "type": "function",
                "function": {
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": tool.input_schema
                }
            })
        })
        .collect()
}

/// One tool call as the OpenAI wire carries it: arguments as a JSON string.
pub fn wire_tool_call(id: &str, name: &str, input: &Value) -> Value {
    json!({
        "id": id,
        "type": "function",
        "function": {
            "name": name,
            "arguments": serde_json::to_string(input).unwrap_or_default() // OpenAI wire boundary: tool-call arguments travel as a JSON string
        }
    })
}

pub(crate) fn apply_no_think_switch(messages: &mut [Value]) {
    for m in messages.iter_mut().rev() {
        if m.get("role").and_then(|r| r.as_str()) != Some("user") {
            continue;
        }
        if let Some(content) = m.get_mut("content") {
            if let Some(s) = content.as_str() {
                *content = Value::String(format!("{s}\n/no_think"));
            }
        }
        return;
    }
}

/// Close a message thread that ENDS with an assistant turn — wire-illegal on
/// thinking models: llama-server treats a trailing assistant message as response
/// PREFILL and rejects the request 400 ("Assistant response prefill is
/// incompatible with enable_thinking"). Glass-boxed 2026-07-11: 1000+ self-tick
/// deliberations silently died over two days whenever the persona had spoken
/// last (her own posts are attributed role=assistant, task #92). We never intend
/// prefill semantics — those are past TURNS — so append a structural continuation
/// fact (true by construction, decides nothing about her reply;
/// [[no-hardcoded-heuristics-to-steer-cognition]]). Thinking stays ON
/// ([[thinking-is-primary-never-suppress]]); suppressing it instead would trade
/// a wire bug for a cognition downgrade. No-op on threads already ending with a
/// user/system/tool message.
pub fn close_trailing_assistant(messages: &mut Vec<Value>) {
    let ends_with_assistant = messages
        .last()
        .and_then(|m| m.get("role"))
        .and_then(|r| r.as_str())
        .map(|r| r == "assistant")
        .unwrap_or(false);
    if ends_with_assistant {
        messages.push(json!({
            "role": "user",
            "content": "[continuation] The transcript above ends with your own \
                        last turn; nothing external arrived after it. You are \
                        continuing your own thread."
        }));
    }
}

/// Convert ChatMessage to OpenAI format.
///
/// `vision_native` is the TARGET MODEL's verdict (the row's
/// `Capability::Vision` via `sensory::route`, resolved by the caller): when
/// true, `ContentPart::Image` becomes a proper OpenAI multimodal
/// `image_url` content part (base64 data-URI or URL) so a vision model —
/// cloud or the multimodal llama-server lane — receives RAW PIXELS
/// natively. When false, image parts are DROPPED here (with a loud log):
/// a non-vision model reads the VisionDescriptionService bridge text that
/// the sensory layer already put in the message, and POSTing `image_url`
/// parts at a text-only endpoint is at best an API error and at worst a
/// silent drop the persona would mistake for having seen
/// ([[fallbacks-are-illegal-fail-loud]], CLAUDE.md "Sensory Architecture").
/// With `view`, also where input message `view` starts on the wire ([`wire_messages_marking`]).
pub(crate) fn format_messages(
    cfg: &OpenAICompatibleConfig,
    messages: &[ChatMessage],
    system_prompt: Option<&str>,
    vision_native: bool,
    audio_native: bool,
    view: Option<usize>,
) -> (Vec<Value>, Option<usize>) {
    let (mut result, view_end) =
        wire_messages_marking(messages, system_prompt, vision_native, audio_native, &cfg.provider_id, view);
    // Thinking toggle: when this gateway suppresses reasoning, append Qwen3's
    // `/no_think` soft-switch to the last user turn so the model skips its
    // chain-of-thought and answers directly. Model-specific token, owned here at
    // the adapter boundary; higher layers never speak `/no_think`.
    if cfg.thinking == ThinkingMode::Suppress {
        apply_no_think_switch(&mut result);
    }
    (result, view_end)
}

/// The wire messages for a request: the system prompt, then each message with its
/// tool calls kept on assistant turns and its tool results split into `role: tool`
/// messages. The one mapping serving sends and the engine trainer renders a lived
/// call from, so a trained history is the history she was served. `provider` names
/// the caller in the dropped-image warning.
pub fn wire_messages(
    messages: &[ChatMessage],
    system_prompt: Option<&str>,
    vision_native: bool,
    audio_native: bool,
    provider: &str,
) -> Vec<Value> {
    wire_messages_marking(messages, system_prompt, vision_native, audio_native, provider, None).0
}

/// [`wire_messages`], and how many leading wire messages come from `messages[..mark]`: the
/// wire position where input message `mark` starts, or the end when `mark` is every message.
/// Tool replies regroup on the wire, so the input index is not the wire index. The position
/// is taken before replies still waiting for their group are flushed, so the wire prefix it
/// names depends on `messages[..mark]` alone and the next act, whose view extends this one,
/// sends it again byte for byte. `None` when `mark` is past the end.
pub(crate) fn wire_messages_marking(
    messages: &[ChatMessage],
    system_prompt: Option<&str>,
    vision_native: bool,
    audio_native: bool,
    provider: &str,
    mark: Option<usize>,
) -> (Vec<Value>, Option<usize>) {
    // Pre-size: one wire message per input message + the optional system
    // prompt. The common text path lands exactly; tool-result turns push a
    // few extra and realloc once. Runs on every inference call — no
    // grow-from-zero reallocation on the hot path.
    let mut result = Vec::with_capacity(messages.len() + usize::from(system_prompt.is_some()));
    let mut observations = Vec::new();

    // Add system prompt if provided
    if let Some(sys) = system_prompt {
        result.push(json!({
            "role": "system",
            "content": sys
        }));
    }

    let mut marked = None;
    for (index, msg) in messages.iter().enumerate() {
        if mark == Some(index) {
            marked = Some(result.len());
        }
        // Keep consecutive tool replies together, including replies carried by
        // separate input messages. User media must not interrupt their pairing.
        let is_tool_reply = matches!(&msg.content, MessageContent::Parts(parts)
            if parts.iter().any(|p| matches!(p, ContentPart::ToolResult { .. }))
                && !parts.iter().any(|p| matches!(p, ContentPart::ToolUse { .. })));
        if !is_tool_reply {
            result.append(&mut observations);
        }
        match &msg.content {
            MessageContent::Text(text) => {
                result.push(json!({
                    "role": msg.role,
                    "content": text
                }));
            }
            MessageContent::Parts(parts) => {
                // Check for tool protocol blocks
                let has_tool_use = parts
                    .iter()
                    .any(|p| matches!(p, ContentPart::ToolUse { .. }));
                let has_tool_result = parts
                    .iter()
                    .any(|p| matches!(p, ContentPart::ToolResult { .. }));

                if has_tool_use {
                    // Assistant message with tool_calls
                    let text_content: String = parts
                        .iter()
                        .filter_map(|p| match p {
                            ContentPart::Text { text } => Some(text.as_str()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join("");

                    let tool_calls: Vec<Value> = parts
                        .iter()
                        .filter_map(|p| match p {
                            ContentPart::ToolUse { id, name, input } => Some(wire_tool_call(id, name, input)),
                            _ => None,
                        })
                        .collect();

                    result.push(json!({
                        "role": "assistant",
                        "content": if text_content.is_empty() { Value::Null } else { Value::String(text_content) },
                        "tool_calls": tool_calls
                    }));
                } else if has_tool_result {
                    // Tool results as separate messages
                    for part in parts {
                        if let ContentPart::ToolResult {
                            tool_use_id,
                            content,
                            ..
                        } = part
                        {
                            result.push(json!({
                                "role": "tool",
                                "tool_call_id": tool_use_id,
                                "content": content
                            }));
                        }
                    }
                }
                if !has_tool_use {
                    // Tool results may carry sibling pixels/audio. Emit their
                    // protocol replies first, then preserve the observation.
                    let content: Vec<Value> = parts
                        .iter()
                        .filter_map(|p| match p {
                            ContentPart::Text { text } => Some(json!({
                                "type": "text",
                                "text": text
                            })),
                            ContentPart::Image { image } => {
                                if !vision_native {
                                    // Target model can't see: the sensory bridge's
                                    // text description (already a Text part /
                                    // upstream) is what it reads. Never ship
                                    // image_url at a text-only endpoint.
                                    tracing::warn!(
                                        target: "openai_adapter",
                                        provider = %provider,
                                        "dropping image content part for a non-vision \
                                         model — the description bridge is its sight; \
                                         if this model CAN see, its catalog row must \
                                         declare Capability::Vision"
                                    );
                                    None
                                } else if let Some(url) = &image.url {
                                    Some(json!({
                                        "type": "image_url",
                                        "image_url": { "url": url }
                                    }))
                                } else {
                                    image.base64.as_ref().map(|b64| json!({
                                        "type": "image_url",
                                        "image_url": {
                                            "url": format!("data:{};base64,{}",
                                                image.mime_type.as_deref().unwrap_or("image/png"), b64)
                                        }
                                    }))
                                }
                            }
                            ContentPart::Audio { audio } => Some(wire_audio(audio, audio_native)),
                            ContentPart::Video { .. } => Some(json!({
                                "type": "text",
                                "text": "[Video was not attached: this adapter requires decoded timestamped frames and audio. The video contents have not been perceived.]"
                            })),
                            ContentPart::ToolUse { .. } | ContentPart::ToolResult { .. } => None,
                        })
                        .collect();

                    if !has_tool_result || !content.is_empty() {
                        let observation = json!({
                            "role": if has_tool_result { "user" } else { msg.role.as_str() },
                            "content": content
                        });
                        if has_tool_result {
                            observations.push(observation);
                        } else {
                            result.push(observation);
                        }
                    }
                }
            }
        }
    }

    if mark == Some(messages.len()) {
        marked = Some(result.len());
    }
    result.append(&mut observations);
    (result, marked)
}

/// OpenAI-compatible audio uses encoded WAV/MP3, not an image data URL.
/// Unsupported inputs stay visible as failures rather than vanishing from the ask.
fn wire_audio(audio: &crate::ai::types::AudioInput, native: bool) -> Value {
    let unavailable = |reason: &str| json!({
        "type": "text", "text": format!("[Audio was not attached: {reason}. Audio contents have not been perceived.]")
    });
    if !native {
        return unavailable("the target model has no declared AudioInput capability");
    }
    let (mime, data) = if let Some(data) = audio.base64.as_deref() {
        (audio.mime_type.as_deref(), data)
    } else if let Some(url) = audio.url.as_deref() {
        let Some((header, data)) = url.strip_prefix("data:").and_then(|s| s.split_once(";base64,")) else {
            return unavailable("audio must be resolved to encoded bytes before request assembly");
        };
        (Some(header), data)
    } else {
        return unavailable("no encoded bytes were supplied");
    };
    let format = match mime {
        Some("audio/wav" | "audio/x-wav" | "audio/wave") => "wav",
        Some("audio/mpeg" | "audio/mp3") => "mp3",
        _ => return unavailable("the encoding must be converted to WAV or MP3"),
    };
    json!({"type": "input_audio", "input_audio": {"data": data, "format": format}})
}

/// Build the base chat body: the wire messages (vision-gated, thinking-switched, tool
/// prompt appended for JsonInPrompt gateways, trailing assistant closed), then model /
/// temperature / stream / max_tokens / stop. `finish_body` completes it after admission.
pub(crate) fn build_base_body(
    cfg: &OpenAICompatibleConfig,
    request: &TextGenerationRequest,
    model: &str,
    vision_native: bool,
    audio_native: bool,
) -> Value {
    // Build request body
    let (mut messages, view_end) = format_messages(
        cfg,
        &request.messages,
        request.system_prompt.as_deref(),
        vision_native,
        audio_native,
        request.view_messages.map(|len| len as usize),
    );

    // JsonInPrompt tool offering: for gateways/models that ignore the OpenAI
    // `tools` param (a local GGUF), describe the tools IN the prompt and ask
    // for a strict JSON call. Appended as a system message; the matching parse
    // happens on the response below. Native providers skip this (tool_prompt →
    // None) and use the `tools` param instead.
    if let Some(tools) = request.tools.as_ref() {
        if let Some(block) = cfg.tool_protocol.tool_prompt(tools) {
            messages.push(json!({ "role": "system", "content": block }));
        }
    }

    close_trailing_assistant(&mut messages);

    let mut body = json!({
        "model": model,
        "messages": messages,
        "temperature": request.temperature.unwrap_or(0.7),
        // Stream tokens the instant they're decoded. `include_usage` makes the
        // backend emit a final usage-only frame so we still get token counts.
        "stream": true,
        "stream_options": { "include_usage": true }
    });
    // Where her view ends, in wire messages: our fork checkpoints her recurrent state there
    // and keeps that checkpoint in her saved page (card 3512f496). A llama.cpp-family field:
    // a stock server ignores it, a cloud provider would reject it.
    if let (true, Some(view_end)) = (cfg.llamacpp_sampling_extensions, view_end) {
        body["view_messages"] = json!(view_end);
    }

    // max_tokens — the MODEL owns its generation length, enforced server-side
    // by llama.cpp / the cloud provider. We forward a ceiling ONLY
    // when the caller set one explicitly; `None` → omit the field so the model
    // runs to its own stop token or context limit. We never invent a default
    // here: the old `.unwrap_or(2048)` was a second clamp duplicating a limit
    // the model already enforces, and it truncated reasoning models mid-`<think>`
    // (qwen3.5 spends ~500 tokens reasoning before the answer → empty reply).
    if let Some(max) = request.max_tokens {
        if let Some(obj) = body.as_object_mut() {
            obj.insert("max_tokens".to_string(), json!(max));
        }
    } else {
        // The lane's `--n-predict` bounds it to one window (card 912e606d); name the
        // caller, whose cap is still owed: a window of decode holds a lane for minutes.
        continuum_base::probe!(
            class = "inference.request.uncapped",
            purpose = request.purpose.as_deref().unwrap_or("none"), // unwrap_or: an unnamed caller is reported as such
            model = %model,
            "a request with no max_tokens: bounded only by the lane's one-window --n-predict default"
        );
    }

    // stop — the turn-boundary + reserved-marker stop sequences (#150, #158).
    // GLASS-BOXED 2026-07-13: the body above shipped WITHOUT this field, so
    // every stop the deliberation faculty threaded in (peer-name stops so a
    // model can't speak AS teammates; `\n[action`/`\nI ran ` so it can't
    // fabricate receipts) was silently dropped before reaching llama-server —
    // the decode-level hygiene never actually ran on local models. llama.cpp's
    // OpenAI-compatible server honors `stop` as an array of strings; forward it
    // whenever the caller set any.
    if let Some(stops) = &request.stop_sequences {
        if !stops.is_empty() {
            if let Some(obj) = body.as_object_mut() {
                obj.insert("stop".to_string(), json!(stops));
            }
        }
    }

    // DMR-specific: llama.cpp's OpenAI-compatible server accepts the
    // llama.cpp-native `repeat_penalty` field as an extension. Until
    // this patch the POST body shipped ONLY the 5 fields above, so
    // DMR inference ran with repeat_penalty=1.0 (llama.cpp default,
    // disabled) and produced runaway repetition — empirically verified
    // 2026-04-24 on Linux/CUDA Carl stack: qwen3.5-4b-code-forged
    // reprinted the same <think> paragraph 10-40 times then burned
    // max_tokens without emitting a real reply. Meanwhile the
    // in-process llamacpp_adapter path defaults
    // `sampling.repeat_penalty = 1.1` (backends/mod.rs:195,205) and
    // does NOT exhibit this failure mode on Mac Metal. Classic RULE 1
    // divergence (integration test path ≠ production path).
    //
    // Scoped to llama.cpp-family gateways (DMR, llama-server) via the TYPED
    // `llamacpp_sampling_extensions` capability (#55), NOT the provider id:
    // cloud OpenAI-compat providers (openai, groq, xai, fireworks, together)
    // do NOT accept `repeat_penalty` (non-standard field) — some ignore it
    // silently, others reject — so they leave the flag false and the field
    // is omitted. llama-server inherits the same protection DMR had: the
    // forged 4B loops its `<think>` block to the token budget without it.
    //
    body
}

#[cfg(test)]
mod tests {
    use super::*;

    // Tool protocol results must not silently discard sibling sensory evidence.
    #[test]
    fn tool_result_keeps_sibling_media_after_protocol_reply() {
        let message: ChatMessage = serde_json::from_value(json!({
            "role": "user", "content": [
                {"type":"tool_result", "tool_use_id":"capture-1", "content":"captured"},
                {"type":"text", "text":"Current rendering"},
                {"type":"image", "image":{"base64":"pixels", "mimeType":"image/png"}},
                {"type":"audio", "audio":{"base64":"sound", "mimeType":"audio/wav"}}
            ]
        })).unwrap();
        let wire = wire_messages(&[message.clone()], None, true, true, "test");
        assert_eq!(wire.len(), 2);
        assert_eq!(wire[0]["tool_call_id"], "capture-1");
        assert_eq!(wire[1]["role"], "user");
        assert_eq!(wire[1]["content"][1]["image_url"]["url"], "data:image/png;base64,pixels");
        assert_eq!(wire[1]["content"][2]["input_audio"]["data"], "sound");
        let mut second = message.clone();
        if let MessageContent::Parts(parts) = &mut second.content {
            if let ContentPart::ToolResult { tool_use_id, .. } = &mut parts[0] {
                *tool_use_id = "capture-2".into();
            }
        }
        let batch = wire_messages(&[message.clone(), second], None, true, true, "test");
        assert_eq!(batch.iter().map(|m| m["role"].as_str().unwrap()).collect::<Vec<_>>(),
            ["tool", "tool", "user", "user"]);
        assert_eq!(batch[1]["tool_call_id"], "capture-2");
        let mut only_result = message;
        if let MessageContent::Parts(parts) = &mut only_result.content { parts.truncate(1); }
        assert_eq!(wire_messages(&[only_result], None, true, true, "test").len(), 1);
    }

    // what this catches: through the ONE seam, the ACT budget is the fixed
    // ACT_REASONING_BUDGET count, applied to an act request and to nothing else — a
    // deliberation derives its own from its allowance (and carries none without one),
    // and a turn with no purpose carries none.
    #[test]
    fn the_act_budget_is_a_fixed_count_and_only_an_act_carries_it() {
        let mut act = json!({ "model": "m" });
        assert_eq!(apply_reasoning_budget(Some(ACT_PURPOSE), &mut act), Some(u64::from(ACT_REASONING_BUDGET)));
        assert_eq!(act["reasoning_budget_tokens"], json!(ACT_REASONING_BUDGET));
        let mut delib = json!({ "model": "m" });
        assert_eq!(apply_reasoning_budget(Some("cognition/deliberation"), &mut delib), None);
        assert!(delib.get("reasoning_budget_tokens").is_none());
        let mut none = json!({ "model": "m" });
        assert_eq!(apply_reasoning_budget(None, &mut none), None);
    }

    fn gateway(llamacpp: bool) -> OpenAICompatibleConfig {
        OpenAICompatibleConfig {
            provider_id: "test-gateway".into(),
            name: "Test Gateway".into(),
            base_url: "http://127.0.0.1:0".into(),
            api_key_env: None,
            default_model: "test-model".into(),
            capabilities: [Capability::ToolUse].into_iter().collect(),
            models: Vec::new(),
            model_prefixes: Vec::new(),
            requires_auth: false,
            tool_protocol: crate::model_registry::ToolProtocol::NativeFunctionCalling,
            thinking: ThinkingMode::Default,
            single_resident_model: false,
            dynamic_model_catalog: false,
            llamacpp_sampling_extensions: llamacpp,
            tail_tools: llamacpp,
        }
    }

    // what this catches: a view end that names a wire position the next act does not resend
    // (a tool reply's held observation flushed into it, so the engine's checkpoint lands past
    // where her next prompt diverges), an input index sent as if it were a wire index, or the
    // fork's field reaching a cloud provider (card 3512f496).
    #[test]
    fn the_view_end_is_a_wire_prefix_the_next_act_resends_and_only_llamacpp_hears_it() {
        let text = |role: &str, body: &str| ChatMessage::text(role, body);
        let reply = |id: &str| -> ChatMessage { serde_json::from_value(json!({ "role": "user", "content": [
            {"type":"tool_result", "tool_use_id": id, "content":"done"},
            {"type":"image", "image":{"base64":"pixels", "mimeType":"image/png"}}
        ]})).unwrap() };
        // This act: her view is [her note, a tool reply with pixels]; the ask is the tail.
        let this_act = [text("user", "note"), reply("call-1"), text("user", "ask")];
        let (wire, end) = wire_messages_marking(&this_act, Some("system"), true, true, "test", Some(2));
        // system, note, the reply; its observation is still held when the tail starts.
        assert_eq!(end, Some(3));
        assert_eq!(wire.len(), 5);
        // The next act's view appends another reply to the same group, then a new tail.
        let next_act = [text("user", "note"), reply("call-1"), reply("call-2"), text("user", "ask again")];
        let (next, _) = wire_messages_marking(&next_act, Some("system"), true, true, "test", Some(3));
        assert_eq!(wire[..3], next[..3], "the prefix the view end names is resent byte for byte");
        assert_ne!(wire[3], next[3], "the held observation is not part of it");
        assert_eq!(wire_messages_marking(&this_act, None, true, true, "test", Some(3)).1, Some(4), "every message is view");
        assert_eq!(wire_messages_marking(&this_act, None, true, true, "test", Some(4)).1, None);

        let request = TextGenerationRequest { messages: this_act.to_vec(), view_messages: Some(2), ..Default::default() };
        let served = build_base_body(&gateway(true), &request, "m", true, true);
        assert_eq!(served["view_messages"], json!(2), "no system prompt: note and the reply");
        assert!(build_base_body(&gateway(false), &request, "m", true, true).get("view_messages").is_none());
    }

    // what this catches: a tail-loaded verb not reaching a llama.cpp-family server (she could
    // read its schema and never call it), the non-standard field going to a cloud provider
    // that rejects unknown fields, or a tail silently dropped (Codex on #5491).
    #[test]
    fn tail_tools_reach_only_a_llamacpp_family_server_and_never_join_the_head() {
        let spec = |name: &str| crate::ai::types::NativeToolSpec {
            name: name.to_owned(),
            description: format!("{name} does one thing"),
            input_schema: crate::ai::types::ToolInputSchema {
                schema_type: "object".to_owned(),
                properties: json!({}),
                required: None,
                definitions: None,
            },
        };
        let request = TextGenerationRequest {
            tools: Some(vec![spec("commands_help")]),
            tail_tools: Some(vec![spec("code_git_apply")]),
            ..Default::default()
        };
        let cfg = gateway;
        let mut served = json!({ "model": "m" });
        assert!(finish_body(&cfg(true), &request, "m", true, &mut served).is_ok());
        assert_eq!(served["tools"].as_array().map(Vec::len), Some(1), "the head holds only its own verbs");
        assert_eq!(served["tail_tools"][0]["function"]["name"], "code_git_apply");
        // A tail the destination cannot take is refused for a rebuild, never silently dropped.
        let mut cloud = json!({ "model": "m" });
        assert_eq!(
            finish_body(&cfg(false), &request, "m", false, &mut cloud),
            Err(crate::ai::inference_error::InferenceError::TailToolsUnreachable { model: "m".into() })
        );
        let headless = TextGenerationRequest { tail_tools: None, ..request.clone() };
        assert!(finish_body(&cfg(false), &headless, "m", false, &mut json!({ "model": "m" })).is_ok(), "no tail, nothing to refuse");
    }
}
