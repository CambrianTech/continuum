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
/// llama.cpp/unsloth servers we target; idempotent — overwrites its own prior value.
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
pub(crate) const ACT_PURPOSE: &str = "cognition/act";

// context-budget-exempt: a budget for the model's REASONING channel on an act turn, not a window bound
/// How much a reasoning model may think before its tool call on an ACT turn.
/// Measured 2026-09-07 (Qwen3.8-27B off-box): committed acts ran 4,188–9,818
/// output tokens, nearly all of it thinking, 150–260 s per act on a shared lane
/// (card 12ef9c10). A tool call plus its sentence needs a few hundred tokens of
/// thought, not thousands. llama-server reads `reasoning_budget_tokens` and
/// closes the thinking block at the budget; other gateways ignore the field.
pub(crate) const ACT_REASONING_BUDGET: u32 = 1024;

/// The purpose a deliberation (speak / pass) turn announces on its request.
pub(crate) const DELIBERATION_PURPOSE: &str = "cognition/deliberation";

/// A sensory description needs answer text within its existing completion allowance.
/// Keep it distinct from persona deliberation for routing and diagnostics.
pub(crate) const VISION_DESCRIPTION_PURPOSE: &str = "cognition/vision-describe";

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
pub(crate) fn apply_reasoning_budget(purpose: Option<&str>, body: &mut Value) -> Option<u64> {
    let obj = body.as_object_mut()?;
    let budget = match purpose {
        Some(ACT_PURPOSE) => u64::from(ACT_REASONING_BUDGET),
        Some(DELIBERATION_PURPOSE | VISION_DESCRIPTION_PURPOSE) => {
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
pub(crate) fn deliberation_reasoning_budget(max_tokens: u64) -> Option<u64> {
    let answer_share = max_tokens / DELIBERATION_ANSWER_SHARE_DIVISOR;
    if answer_share == 0 {
        return None;
    }
    Some(max_tokens - answer_share)
}

pub(crate) fn finish_body(
    cfg: &OpenAICompatibleConfig,
    request: &TextGenerationRequest,
    model: &str,
    body: &mut Value,
) {
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
            Some(ACT_PURPOSE) => "delib.act.reasoning_budgeted",
            Some(VISION_DESCRIPTION_PURPOSE) => "vision.description.reasoning_budgeted",
            _ => "delib.pass.reasoning_budgeted",
        };
        crate::probe!(
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
    crate::probe!(
        class = "ai.request.tool_surface",
        model = %model,
        tools_n = body.get("tools").and_then(|t| t.as_array()).map_or(0, |a| a.len()),
        tool_choice = body.get("tool_choice").is_some(),
        stops_n = body.get("stop").and_then(|s| s.as_array()).map_or(0, |a| a.len()),
        msgs_n = body.get("messages").and_then(|m| m.as_array()).map_or(0, |a| a.len()),
        temperature = body.get("temperature").and_then(|t| t.as_f64()).unwrap_or(-1.0),
        "outbound chat request tool surface"
    );

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
pub(crate) fn openai_tools(tools: &[crate::ai::types::NativeToolSpec]) -> Vec<Value> {
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
pub(crate) fn wire_tool_call(id: &str, name: &str, input: &Value) -> Value {
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
pub(crate) fn close_trailing_assistant(messages: &mut Vec<Value>) {
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
pub(crate) fn format_messages(
    cfg: &OpenAICompatibleConfig,
    messages: &[ChatMessage],
    system_prompt: Option<&str>,
    vision_native: bool,
    audio_native: bool,
) -> Vec<Value> {
    let mut result = wire_messages(messages, system_prompt, vision_native, audio_native, &cfg.provider_id);
    // Thinking toggle: when this gateway suppresses reasoning, append Qwen3's
    // `/no_think` soft-switch to the last user turn so the model skips its
    // chain-of-thought and answers directly. Model-specific token, owned here at
    // the adapter boundary; higher layers never speak `/no_think`.
    if cfg.thinking == ThinkingMode::Suppress {
        apply_no_think_switch(&mut result);
    }
    result
}

/// The wire messages for a request: the system prompt, then each message with its
/// tool calls kept on assistant turns and its tool results split into `role: tool`
/// messages. The one mapping serving sends and the engine trainer renders a lived
/// call from, so a trained history is the history she was served. `provider` names
/// the caller in the dropped-image warning.
pub(crate) fn wire_messages(
    messages: &[ChatMessage],
    system_prompt: Option<&str>,
    vision_native: bool,
    audio_native: bool,
    provider: &str,
) -> Vec<Value> {
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

    for msg in messages {
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

    result.append(&mut observations);
    result
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
    match encoded_audio(audio) {
        Ok((format, data)) => json!({"type": "input_audio", "input_audio": {"data": data, "format": format}}),
        Err(reason) => unavailable(&reason),
    }
}

fn encoded_audio(audio: &crate::ai::types::AudioInput) -> Result<(&'static str, &str), String> {
    let (mime, data) = if let Some(data) = audio.base64.as_deref() {
        (audio.mime_type.as_deref(), data)
    } else if let Some(url) = audio.url.as_deref() {
        let Some((header, data)) = url.strip_prefix("data:").and_then(|s| s.split_once(";base64,")) else {
            return Err("Native audio requires encoded bytes before request assembly".into());
        };
        (Some(header), data)
    } else {
        return Err("Native audio has no encoded bytes".into());
    };
    if data.is_empty() {
        return Err("Native audio has empty encoded bytes".into());
    }
    let format = match mime {
        Some("audio/wav" | "audio/x-wav" | "audio/wave") => "wav",
        Some("audio/mpeg" | "audio/mp3") => "mp3",
        _ => return Err("Native audio transport requires WAV or MP3 encoding".into()),
    };
    Ok((format, data))
}

/// Build the base chat body: the wire messages (vision-gated, thinking-switched, tool
/// prompt appended for JsonInPrompt gateways, trailing assistant closed), then model /
/// temperature / stream / max_tokens / stop. `finish_body` completes it after admission.
pub(crate) fn build_base_body(
    cfg: &OpenAICompatibleConfig,
    request: &TextGenerationRequest,
    model: &str,
    bound_model: &crate::ai::types::ModelInfo,
) -> Result<Value, String> {
    validate_native_media(&request.messages, bound_model)?;
    let mut messages = format_messages(
        cfg,
        &request.messages,
        request.system_prompt.as_deref(),
        bound_model.has(crate::model_registry::Capability::Vision),
        bound_model.has(crate::model_registry::Capability::AudioInput),
    );

    // JsonInPrompt tool offering: for gateways/models that ignore the OpenAI
    // `tools` param (unsloth+GGUF), describe the tools IN the prompt and ask
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

    // max_tokens — the MODEL owns its generation length, enforced server-side
    // by unsloth / llama.cpp / the cloud provider. We forward a ceiling ONLY
    // when the caller set one explicitly; `None` → omit the field so the model
    // runs to its own stop token or context limit. We never invent a default
    // here: the old `.unwrap_or(2048)` was a second clamp duplicating a limit
    // the model already enforces, and it truncated reasoning models mid-`<think>`
    // (qwen3.5 spends ~500 tokens reasoning before the answer → empty reply).
    if let Some(max) = request.max_tokens {
        if let Some(obj) = body.as_object_mut() {
            obj.insert("max_tokens".to_string(), json!(max));
        }
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
    Ok(body)
}

/// Refuse an unrepresentable request before admission or network I/O. A text
/// placeholder is not successful media delivery, even if generation succeeds.
fn validate_native_media(messages: &[ChatMessage], model: &crate::ai::types::ModelInfo) -> Result<(), String> {
    for message in messages {
        let MessageContent::Parts(parts) = &message.content else { continue };
        let has_tool_use = parts.iter().any(|p| matches!(p, ContentPart::ToolUse { .. }));
        for part in parts {
            let reason = match part {
                ContentPart::Image { .. } if !model.has(Capability::Vision) => Some("model does not declare Vision"),
                ContentPart::Image { image } if image.url.as_deref().filter(|s| !s.is_empty()).is_none()
                    && image.base64.as_deref().filter(|s| !s.is_empty()).is_none() => Some("image has no URL or encoded bytes"),
                ContentPart::Audio { .. } if !model.has(Capability::AudioInput) => Some("model does not declare AudioInput"),
                ContentPart::Audio { audio } => {
                    encoded_audio(audio)?;
                    None
                }
                ContentPart::Video { .. } => Some("this transport cannot encode native video"),
                _ => None,
            };
            if let Some(reason) = reason {
                return Err(format!("Native media rejected for model '{}': {reason}", model.id));
            }
            if has_tool_use && matches!(part, ContentPart::Image { .. } | ContentPart::Audio { .. }) {
                return Err(format!("Native media rejected for model '{}': tool-call messages cannot encode sibling media", model.id));
            }
        }
    }
    Ok(())
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
}
