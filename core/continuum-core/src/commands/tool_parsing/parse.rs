//! `tool-parsing/parse` — extract tool calls from raw model response text and
//! correct them (name + param fixups) in one pass.
//!
//! Stateless: the parse + correction logic are pure free functions
//! ([`parse_and_correct_with_family`](crate::tool_parsing::parse_and_correct_with_family)),
//! so this command holds no module state and self-registers.
//!
//! ## Gating
//!
//! `AiSafe` — pure text→structured transform, no side effects, no resources.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::tool_parsing::{parse_and_correct_with_family, ToolParseResult};

/// What to parse, and which model family produced it.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/tool_parsing/ToolParseParams.ts"
)]
pub struct ToolParseParams {
    /// The raw model response text, possibly containing tool-call blocks in any
    /// supported format (XML, Hermes `<tool_call>`, DeepSeek, etc.).
    pub response_text: String,
    /// The model family hint (e.g. "hermes", "deepseek", "llama"). When omitted,
    /// generic multi-format parsing with truncation recovery is used.
    #[ts(optional)]
    pub model_family: Option<String>,
}

crate::action_command! {
    /// Parse tool calls out of raw model response text and apply name/parameter
    /// corrections in one pass. Returns the structured calls, the cleaned text with
    /// tool-call blocks removed, and the parse time. Family hint steers format
    /// priority; omitting it falls back to multi-format parsing with truncation
    /// recovery.
    pub struct ToolParsingParse;
    name: "tool-parsing/parse",
    // Internal: this is the substrate's OWN machinery for turning a model's raw
    // output into structured calls — it operates ON a persona's text, so it must
    // never be offered back to that persona as a callable tool (a category error,
    // like handing Claude Code a "parse your tool call" tool). Not a citizen task.
    access: Internal,
    params: ToolParseParams,
    output: ToolParseResult,
    run(_this, _ctx, p) => {
        Ok(parse_and_correct_with_family(
            &p.response_text,
            p.model_family.as_deref(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand, Ctx};

    // what this catches: name/access wiring — the tool-call parser is substrate
    // machinery, gated Internal so it never reaches the persona AiSafe tool surface.
    #[test]
    fn name_and_access_wired() {
        assert_eq!(ToolParsingParse::NAME, "tool-parsing/parse");
        assert!(matches!(ToolParsingParse::ACCESS, AccessLevel::Internal));
    }

    // what this catches: a single XML tool-call block is parsed to one corrected
    // call (query -> pattern), proving the transplanted body runs end to end.
    #[tokio::test]
    async fn parses_and_corrects_one_call() {
        let out = ToolParsingParse
            .run(
                &Ctx::default(),
                ToolParseParams {
                    response_text: "<tool_use><tool_name>code/search</tool_name>\
                        <parameters><query>test</query></parameters></tool_use>"
                        .to_string(),
                    model_family: None,
                },
            )
            .await
            .expect("parse must succeed");
        assert_eq!(out.tool_calls.len(), 1);
        assert_eq!(out.tool_calls[0].tool_name, "code/search");
        assert!(out.tool_calls[0].parameters.contains_key("pattern"));
    }

    // what this catches: the family hint is threaded through — a Hermes-format
    // block parses with the hermes path.
    #[tokio::test]
    async fn family_hint_is_threaded() {
        let out = ToolParsingParse
            .run(
                &Ctx::default(),
                ToolParseParams {
                    response_text:
                        "<tool_call>\n{\"name\": \"code_search\", \"arguments\": {\"pattern\": \"x\"}}\n</tool_call>"
                            .to_string(),
                    model_family: Some("hermes".to_string()),
                },
            )
            .await
            .expect("parse must succeed");
        assert_eq!(out.tool_calls.len(), 1);
    }
}
