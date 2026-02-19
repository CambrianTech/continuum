//! Wire types for tool parsing IPC — ts-rs generated.
//!
//! Single source of truth for Rust↔TypeScript tool parsing boundary.

use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use ts_rs::TS;

/// Request to parse tool calls from AI response text.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/persona/ToolParseRequest.ts")]
pub struct ToolParseRequest {
    pub response_text: String,
    #[ts(optional)]
    pub known_tools: Option<Vec<String>>,
}

/// A single parsed tool call with format and correction metadata.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/persona/ParsedToolCall.ts")]
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
#[ts(export, export_to = "../../../shared/generated/persona/ToolParseResult.ts")]
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
#[ts(export, export_to = "../../../shared/generated/persona/CorrectedToolCall.ts")]
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
    fn ts_binding_tool_parse_request() {
        let _ts = ToolParseRequest::export_to_string().unwrap();
        assert!(_ts.contains("response_text"));
        assert!(_ts.contains("known_tools"));
    }

    #[test]
    fn ts_binding_parsed_tool_call() {
        let _ts = ParsedToolCall::export_to_string().unwrap();
        assert!(_ts.contains("tool_name"));
        assert!(_ts.contains("parameters"));
        assert!(_ts.contains("format"));
        assert!(_ts.contains("original_name"));
        assert!(_ts.contains("param_corrections"));
    }

    #[test]
    fn ts_binding_tool_parse_result() {
        let _ts = ToolParseResult::export_to_string().unwrap();
        assert!(_ts.contains("tool_calls"));
        assert!(_ts.contains("cleaned_text"));
        assert!(_ts.contains("parse_time_us"));
    }

    #[test]
    fn ts_binding_corrected_tool_call() {
        let _ts = CorrectedToolCall::export_to_string().unwrap();
        assert!(_ts.contains("tool_name"));
        assert!(_ts.contains("name_changed"));
        assert!(_ts.contains("param_corrections"));
    }
}
