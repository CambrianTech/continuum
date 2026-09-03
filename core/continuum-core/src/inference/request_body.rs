//! Request body — the tail of chat-body assembly: the per-gateway thinking policy, the
//! structured-output format, the native tool surface, and the wire-truth probe. Carved
//! out of `openai_adapter::generate_stream` (pure code-motion, 2026-09-03, the S3b
//! decompose). Behaviour-identical to the inline block. The head of assembly
//! (`format_messages` → base body → sampling knobs) is the next carve.

use serde_json::{json, Value};

use crate::ai::openai_adapter::{OpenAICompatibleConfig, ThinkingMode};
use crate::ai::types::{TextGenerationRequest, ToolChoice};
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
