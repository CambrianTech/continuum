//! `mcp/tool-help` — one command's params + description (typed, dep-holding).
//!
//! Looks a single tool up in the cached catalog (accepting the `name`, `a/b`, or
//! `mcp__jtag__name` forms) and returns its help payload. A tool that isn't found
//! is a legitimate `found: false` result carrying a hint — NOT an error — because
//! the model routinely probes names it isn't sure exist, and a hard error would
//! read as a substrate fault rather than "try search first". This not-found-as-Ok
//! contract is transplanted verbatim from the legacy arm.
//!
//! `access: AiSafe` — read-only discovery, same trust class as the other catalog verbs.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::modules::mcp::McpCatalog;

/// Params for `mcp/tool-help`: the tool to describe (any accepted name form).
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/mcp/McpToolHelpParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct McpToolHelpParams {
    /// The exact tool/command name to get the call shape for (e.g. `code/read`) —
    /// as listed by `commands/list` or `mcp/search-tools`.
    pub tool: String,
}

/// Result of `mcp/tool-help`. When `found`, `help` carries the tool's
/// name/description/params payload; when not, `error` + `hint` explain and point
/// to `mcp/search-tools`. `help` keeps the legacy JSON shape (`unknown` on the
/// TS side) — the consumer passes it through to the model as text, so a typed
/// projection would add cost without a reader.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/mcp/McpToolHelpResult.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct McpToolHelpResult {
    pub found: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[ts(optional, type = "unknown")]
    pub help: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub hint: Option<String>,
}

crate::action_command! {
    /// Show one command's help: its description and each parameter (name, type, required,
    /// description). Accepts the plain name, the `a/b` jtag form, or the `mcp__jtag__` form.
    /// An unknown tool returns `found: false` with a hint to search — not an error.
    pub struct McpToolHelp { catalog: Arc<McpCatalog> }
    name: "mcp/tool-help",
    access: AiSafe,
    params: McpToolHelpParams,
    output: McpToolHelpResult,
    run(this, _ctx, params) => {
        let tools = this.catalog.list()?;
        match this.catalog.get_tool_help(&tools, &params.tool) {
            Some(help) => Ok(McpToolHelpResult {
                found: true,
                help: Some(help),
                error: None,
                hint: None,
            }),
            None => Ok(McpToolHelpResult {
                found: false,
                help: None,
                error: Some(format!("Tool not found: {}", params.tool)),
                hint: Some("Use mcp/search-tools to find available tools".to_string()),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::mcp::McpCatalog;
    use crate::sdk_codegen::{ActionCommand, CommandError, Ctx};

    // what this catches: an uninitialized catalog must FAIL LOUD (Internal) — the
    // not-found-as-Ok contract only kicks in once the cache is built; before that,
    // a missing cache is a real fault and must not masquerade as `found: false`.
    // The found + not-found-over-a-live-catalog paths are covered by the
    // module-harness test in `modules/mcp.rs`.
    #[tokio::test]
    async fn tool_help_on_uninitialized_catalog_fails_loud() {
        let cmd = McpToolHelp {
            catalog: Arc::new(McpCatalog::new()),
        };
        let err = cmd
            .run(
                &Ctx::default(),
                McpToolHelpParams {
                    tool: "chat/send".to_string(),
                },
            )
            .await
            .expect_err("uninitialized cache must error, not return found:false");
        match err {
            CommandError::Internal(msg) => {
                assert!(msg.contains("not initialized"), "got: {msg}")
            }
            other => panic!("expected Internal, got {other:?}"),
        }
    }
}
