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
        // Diagnostic — print the request body exactly as serialized so we
        // can see which fields actually reach DMR. Helps catch silent
        // serialization drops (caught one 2026-04-19 — entry chain wasn't
        // mutating body in place).
        tracing::info!(
            target: "openai_adapter",
            "request body to {}: {}",
            cfg.name,
            serde_json::to_string(&body).unwrap_or_default()
        );
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
            let openai_tools: Vec<Value> = tools
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
                .collect();
            body["tools"] = json!(openai_tools);

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
) -> Vec<Value> {
    // Pre-size: one wire message per input message + the optional system
    // prompt. The common text path lands exactly; tool-result turns push a
    // few extra and realloc once. Runs on every inference call — no
    // grow-from-zero reallocation on the hot path.
    let mut result = Vec::with_capacity(messages.len() + usize::from(system_prompt.is_some()));

    // Add system prompt if provided
    if let Some(sys) = system_prompt {
        result.push(json!({
            "role": "system",
            "content": sys
        }));
    }

    for msg in messages {
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
                            ContentPart::ToolUse { id, name, input } => Some(json!({
                                "id": id,
                                "type": "function",
                                "function": {
                                    "name": name,
                                    "arguments": serde_json::to_string(input).unwrap_or_default()
                                }
                            })),
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
                } else {
                    // Standard multimodal content
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
                                        provider = %cfg.provider_id,
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
                            _ => None,
                        })
                        .collect();

                    result.push(json!({
                        "role": msg.role,
                        "content": content
                    }));
                }
            }
        }
    }

    // Thinking toggle: when this gateway suppresses reasoning, append Qwen3's
    // `/no_think` soft-switch to the last user turn so the model skips its
    // chain-of-thought and answers directly. Model-specific token, owned here at
    // the adapter boundary; higher layers never speak `/no_think`.
    if cfg.thinking == ThinkingMode::Suppress {
        apply_no_think_switch(&mut result);
    }

    result
}

/// Build the base chat body: the wire messages (vision-gated, thinking-switched, tool
/// prompt appended for JsonInPrompt gateways, trailing assistant closed), then model /
/// temperature / stream / max_tokens / stop. `finish_body` completes it after admission.
pub(crate) fn build_base_body(
    cfg: &OpenAICompatibleConfig,
    request: &TextGenerationRequest,
    model: &str,
    vision_native: bool,
) -> Value {
    // Build request body
    let mut messages = format_messages(
                cfg,
        &request.messages,
        request.system_prompt.as_deref(),
        vision_native,
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
    body
}
