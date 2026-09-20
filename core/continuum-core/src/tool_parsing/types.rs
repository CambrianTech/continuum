//! Wire types for tool parsing IPC — ts-rs generated.
//!
//! Single source of truth for Rust↔TypeScript tool parsing boundary.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use ts_rs::TS;

/// Model family hint for parser prioritization.
/// When provided, the model-family-specific parser runs FIRST before generic fallbacks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/ModelFamily.ts"
)]
pub enum ModelFamily {
    /// DeepSeek v3, R1, Coder — Unicode fullwidth delimiters
    DeepSeek,
    /// Llama 3, 3.1, 4 — `<|python_tag|>` prefix
    Llama,
    /// Mistral, Mixtral — `[TOOL_CALLS]` prefix
    Mistral,
    /// Hermes-tuned (Nous Research) — `<tool_call>` XML with JSON body
    Hermes,
    /// Qwen, Qwen3-Coder — `<tool_call>` with double-encoded arguments
    Qwen,
    /// Fallback — try all formats in order
    Generic,
}

/// Parse a model_family string hint into the enum.
pub fn parse_model_family(hint: &str) -> ModelFamily {
    match hint.to_lowercase().as_str() {
        "deepseek" => ModelFamily::DeepSeek,
        "llama" => ModelFamily::Llama,
        "mistral" | "mixtral" => ModelFamily::Mistral,
        "hermes" => ModelFamily::Hermes,
        "qwen" => ModelFamily::Qwen,
        _ => ModelFamily::Generic,
    }
}

/// Request to parse tool calls from AI response text.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/ToolParseRequest.ts"
)]
pub struct ToolParseRequest {
    pub response_text: String,
    #[ts(optional)]
    pub known_tools: Option<Vec<String>>,
    /// Optional model family hint for parser prioritization
    #[ts(optional)]
    pub model_family: Option<String>,
}

/// A single parsed tool call with format and correction metadata.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/ParsedToolCall.ts"
)]
pub struct ParsedToolCall {
    pub tool_name: String,
    pub parameters: HashMap<String, String>,
    /// Which format adapter parsed this call
    pub format: String,
    /// Original name before correction (None if unchanged)
    #[ts(optional)]
    pub original_name: Option<String>,
    /// Parameter corrections applied (e.g. ["path -> filePath"])
    pub param_corrections: Vec<String>,
}

/// Result of parsing tool calls from response text.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/ToolParseResult.ts"
)]
pub struct ToolParseResult {
    pub tool_calls: Vec<ParsedToolCall>,
    /// Response text with tool call blocks removed
    pub cleaned_text: String,
    /// Parse time in microseconds
    #[ts(type = "number")]
    pub parse_time_us: u64,
}

/// Result of correcting a single tool call (name + params + content cleaning).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/CorrectedToolCall.ts"
)]
pub struct CorrectedToolCall {
    pub tool_name: String,
    pub parameters: HashMap<String, String>,
    pub name_changed: bool,
    pub param_corrections: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ts_binding_model_family() {
        let cfg = ts_rs::Config::default();
        let _ts = ModelFamily::export_to_string(&cfg).unwrap();
        assert!(_ts.contains("DeepSeek"));
        assert!(_ts.contains("Llama"));
        assert!(_ts.contains("Mistral"));
        assert!(_ts.contains("Hermes"));
        assert!(_ts.contains("Qwen"));
        assert!(_ts.contains("Generic"));
    }

    #[test]
    fn ts_binding_tool_parse_request() {
        let cfg = ts_rs::Config::default();
        let _ts = ToolParseRequest::export_to_string(&cfg).unwrap();
        assert!(_ts.contains("response_text"));
        assert!(_ts.contains("known_tools"));
        assert!(_ts.contains("model_family"));
    }

    #[test]
    fn ts_binding_parsed_tool_call() {
        let cfg = ts_rs::Config::default();
        let _ts = ParsedToolCall::export_to_string(&cfg).unwrap();
        assert!(_ts.contains("tool_name"));
        assert!(_ts.contains("parameters"));
        assert!(_ts.contains("format"));
        assert!(_ts.contains("original_name"));
        assert!(_ts.contains("param_corrections"));
    }

    #[test]
    fn ts_binding_tool_parse_result() {
        let cfg = ts_rs::Config::default();
        let _ts = ToolParseResult::export_to_string(&cfg).unwrap();
        assert!(_ts.contains("tool_calls"));
        assert!(_ts.contains("cleaned_text"));
        assert!(_ts.contains("parse_time_us"));
    }

    #[test]
    fn ts_binding_corrected_tool_call() {
        let cfg = ts_rs::Config::default();
        let _ts = CorrectedToolCall::export_to_string(&cfg).unwrap();
        assert!(_ts.contains("tool_name"));
        assert!(_ts.contains("name_changed"));
        assert!(_ts.contains("param_corrections"));
    }

    #[test]
    fn parse_model_family_string() {
        assert_eq!(parse_model_family("deepseek"), ModelFamily::DeepSeek);
        assert_eq!(parse_model_family("LLAMA"), ModelFamily::Llama);
        assert_eq!(parse_model_family("mixtral"), ModelFamily::Mistral);
        assert_eq!(parse_model_family("unknown"), ModelFamily::Generic);
    }
}
