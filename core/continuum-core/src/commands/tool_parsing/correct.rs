//! `tool-parsing/correct` — apply name + parameter corrections to a single,
//! already-parsed tool call.
//!
//! Stateless: [`correct_tool_call`](crate::tool_parsing::correction::correct_tool_call)
//! is a pure free function, so this command holds no module state and
//! self-registers.
//!
//! ## Gating
//!
//! `AiSafe` — pure structured transform, no side effects.

use std::collections::HashMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::tool_parsing::{correction::correct_tool_call, CorrectedToolCall};

/// One tool call to correct: its (possibly mangled) name and string parameters.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/tool_parsing/ToolCorrectParams.ts"
)]
pub struct ToolCorrectParams {
    /// The model-produced tool name (may be an alias or mis-namespaced form).
    pub tool_name: String,
    /// The parameters as the model emitted them (string values). Corrected param
    /// keys (e.g. `query -> pattern`) are reported in the result.
    #[serde(default)]
    pub parameters: HashMap<String, String>,
}

crate::action_command! {
    /// Correct a single parsed tool call: canonicalise its name (alias / namespace
    /// fixups) and rename mistaken parameter keys to the schema's expected keys.
    /// Returns the corrected name, the corrected params, whether the name changed,
    /// and the list of parameter corrections applied.
    pub struct ToolParsingCorrect;
    name: "tool-parsing/correct",
    // Internal: substrate machinery that fixes up a model's emitted tool call —
    // it operates ON a persona's output, so it is never a citizen-facing task tool.
    access: Internal,
    params: ToolCorrectParams,
    output: CorrectedToolCall,
    run(_this, _ctx, p) => {
        Ok(correct_tool_call(&p.tool_name, &p.parameters))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand, Ctx};

    // what this catches: name/access wiring — tool-call correction is substrate
    // machinery, gated Internal so it never reaches the persona AiSafe tool surface.
    #[test]
    fn name_and_access_wired() {
        assert_eq!(ToolParsingCorrect::NAME, "tool-parsing/correct");
        assert!(matches!(ToolParsingCorrect::ACCESS, AccessLevel::Internal));
    }

    // what this catches: a mis-namespaced name + mistaken param key are both
    // corrected (workspace/tree -> code/tree, directory -> path), proving the
    // transplanted body runs.
    #[tokio::test]
    async fn corrects_name_and_param_key() {
        let mut parameters = HashMap::new();
        parameters.insert("directory".to_string(), "./src".to_string());
        let out = ToolParsingCorrect
            .run(
                &Ctx::default(),
                ToolCorrectParams {
                    tool_name: "workspace/tree".to_string(),
                    parameters,
                },
            )
            .await
            .expect("correct must succeed");
        assert_eq!(out.tool_name, "code/tree");
        assert!(out.name_changed);
        assert_eq!(
            out.parameters.get("path").map(String::as_str),
            Some("./src")
        );
    }
}
