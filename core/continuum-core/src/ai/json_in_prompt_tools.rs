//! Runtime binding for the pure tool protocol component.
use super::types::{NativeToolSpec, ToolCall};
use continuum_tool_protocol::parser as syntax;
pub use syntax::{
    claims_past_tool_run, has_fenced_block, narrates_fenced_action, narrates_stage_direction,
    paren_call_args, render_tool_instructions,
};
pub fn parse_tool_call(text: &str) -> Option<ToolCall> {
    syntax::parse_tool_call(text, crate::cognition::tool_dialect::resolve_wire_name)
}
pub fn parse_tool_calls(text: &str) -> Vec<ToolCall> {
    syntax::parse_tool_calls(text, crate::cognition::tool_dialect::resolve_wire_name)
}
pub fn attempted_tool_name(text: &str) -> Option<String> {
    syntax::attempted_tool_name(text, crate::cognition::tool_dialect::resolve_wire_name)
}
pub fn nameless_args_fence(text: &str) -> Option<String> {
    syntax::nameless_args_fence(text, crate::cognition::tool_dialect::resolve_wire_name)
}

// The tool-exchange protocol enum is the ONE `model_registry::ToolProtocol`
// (#69) — it's catalog data a provider declares. The rendering/parsing
// BEHAVIOR lives here in `ai` (it depends on this module's `NativeToolSpec` /
// `ToolCall` / parser), hung off that type as an inherent impl. Same crate, so
// the impl is legal here even though the type is defined in `model_registry`.
use crate::model_registry::ToolProtocol;

impl ToolProtocol {
    /// The prompt block to inject when offering `tools`, or `None` when tools are
    /// offered via the API `tools` param (native) or not at all. Empty `tools`
    /// → `None`.
    pub fn tool_prompt(self, tools: &[NativeToolSpec]) -> Option<String> {
        match self {
            ToolProtocol::NativeFunctionCalling | ToolProtocol::None => None,
            ToolProtocol::JsonInPrompt if tools.is_empty() => None,
            ToolProtocol::JsonInPrompt => Some(render_tool_instructions(tools)),
        }
    }

    /// Extract a tool call from the model's TEXT response. `None` for
    /// `ToolProtocol::None` (no tools were offered — lifting prose would
    /// fabricate agency) and when the model answered normally (no call).
    ///
    /// BELT-AND-SUSPENDERS (#293, glass-boxed live 2026-07-31): this arm used
    /// to return `None` for `NativeFunctionCalling` by design ("the adapter
    /// reads structured `tool_calls` instead") — and four resident personas
    /// (Asha/Atlas/Anwen/Benchy) looped for HOURS because their lane declared
    /// native function-calling while the model emitted well-formed ```json
    /// tool calls as TEXT. Nothing parsed them; every call starved into Speak.
    /// A model that emits a well-formed textual call must never starve over a
    /// wrong protocol declaration
    /// ([[local-first-tool-call-robustness-is-the-differentiator]]), so the
    /// native arm now runs the same text parse. Native-calls-win precedence is
    /// preserved at every call site: structured `tool_calls` are consumed
    /// FIRST, and text is only consulted when the response carried none (the
    /// adapter's universal fallback and the deliberation verdict branch both
    /// order it that way).
    pub fn parse_text_call(self, text: &str) -> Option<ToolCall> {
        match self {
            ToolProtocol::None => None,
            ToolProtocol::NativeFunctionCalling | ToolProtocol::JsonInPrompt => {
                parse_tool_call(text)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // what this catches: #293 starvation, layer 1 — a lane mis-declared
    // NativeFunctionCalling and the protocol's text-parse arm returned None BY
    // DESIGN, so four resident personas' well-formed ```json fences never
    // executed and they looped for hours. The native arm must run the same text
    // parse (belt-and-suspenders); native structured calls still win because
    // every call site consults `tool_calls` FIRST and only reads text when the
    // response carried none. `ToolProtocol::None` still refuses: no tools were
    // offered at all.
    #[test]
    fn native_protocol_text_parse_lifts_instead_of_starving() {
        let text = "Let me check.\n```json\n{\"function\": \"code/list\", \"path\": \".\"}\n```";
        let tc = ToolProtocol::NativeFunctionCalling
            .parse_text_call(text)
            .expect("belt-and-suspenders: the textual call must lift under a native lane");
        assert_eq!(tc.name, "code/list");
        assert_eq!(tc.input, serde_json::json!({ "path": "." }));
        assert!(ToolProtocol::None.parse_text_call(text).is_none());
    }
}
