//! Format-specific tool call parsers.
//!
//! Formats supported (matching TypeScript ToolFormatAdapter hierarchy):
//! 1. Anthropic XML: `<tool_use><tool_name>X</tool_name><parameters>...</parameters></tool_use>`
//! 2. Function-style: `<function=tool_name>{"param": "value"}</function>`
//! 3. Bare JSON: `tool/name {"param": "value"}` or `tool_name {"param": "value"}</function>`
//! 4. JSON Object: `{"name": "tool_name", "parameters": {"param": "value"}}`
//! 5. Array-style: `["tool/name", {"param": "value"}]`
//! 6. Curly shorthand: `{"tool_name": {"param": "value"}}`
//! 7. Markdown backtick: `` `tool: name` `param=value` ``
//! 8. Old-style XML: `<tool name="X"><param>value</param></tool>`
//! 9. Colon shorthand: `tool/name: {param: "value"}` — QAT local model native format
//! 10. Colon bare: `tool/name: bare_value` — simplified positional variant (degraded models)
//!
//! Handles both canonical (slash) and sanitized (underscore) tool names.
//! Sanitized names from native tool protocol (code_tree → code/tree) are
//! automatically unsanitized back to canonical form.

use once_cell::sync::Lazy;
use super::static_regex;
use regex::Regex;
use std::collections::HashMap;

// ═══════════════════════════════════════════════════════════════════════
// Model-family parsers — prioritized by model_family hint
// ═══════════════════════════════════════════════════════════════════════
use crate::tool_parsing::types::ModelFamily;

mod raw_tool_match;
pub use raw_tool_match::*;
mod parse_deepseek;
pub use parse_deepseek::*;

#[cfg(test)]
mod tests;
