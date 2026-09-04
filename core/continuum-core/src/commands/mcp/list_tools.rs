//! `mcp/list-tools` — the full command catalog as MCP tool definitions (typed,
//! dep-holding).
//!
//! Reads the shared [`McpCatalog`] cache the owning
//! [`MCPModule`](crate::modules::mcp::MCPModule) built at `initialize` from the
//! live registry. THE contract the Rust MCP server (`mcp_protocol.rs`)
//! deserializes: it reads only the `tools` field, so that field is load-bearing
//! and the migration preserves it verbatim.
//!
//! `access: AiSafe` — read-only capability discovery, safe for any authorized
//! caller (it IS how a persona/agent learns what verbs exist).

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::modules::mcp::{MCPTool, McpCatalog};

/// Params for `mcp/list-tools`: none — the catalog is machine-wide, not
/// caller-scoped. An empty struct keeps the typed contract explicit (rather than
/// an untyped `()`), so the generated TS + JSON schema render `{}`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/mcp/McpListToolsParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct McpListToolsParams {}

/// Result of `mcp/list-tools`: the full tool catalog plus its count. `tools` is
/// the field `mcp_protocol.rs::CatalogResult` deserializes (`count` is parity
/// carried from the legacy arm — consumers ignore it, but dropping it would be a
/// silent shape change).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/mcp/McpListToolsResult.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct McpListToolsResult {
    pub tools: Vec<MCPTool>,
    #[ts(type = "number")]
    pub count: usize,
}

crate::action_command! {
    /// List every registered command as an MCP tool definition (name, description,
    /// input schema). The canonical capability-discovery verb: how a persona or agent
    /// learns what the substrate can do. Reads the cached catalog built at startup.
    pub struct McpListTools { catalog: Arc<McpCatalog> }
    name: "mcp/list-tools",
    access: AiSafe,
    params: McpListToolsParams,
    output: McpListToolsResult,
    run(this, _ctx, _params) => {
        let tools = this.catalog.list()?;
        let count = tools.len();
        Ok(McpListToolsResult { tools, count })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::mcp::McpCatalog;
    use crate::sdk_codegen::{ActionCommand, CommandError, Ctx};

    // what this catches: an uninitialized catalog (cache never built) must FAIL
    // LOUD (CommandError::Internal), never return an empty/"success" list that a
    // caller would mistake for "no tools exist". The success path needs a live
    // ModuleContext to build the cache, so it's covered by the module-harness
    // integration test in `modules/mcp.rs`.
    #[tokio::test]
    async fn list_on_uninitialized_catalog_fails_loud() {
        let cmd = McpListTools {
            catalog: Arc::new(McpCatalog::new()),
        };
        let err = cmd
            .run(&Ctx::default(), McpListToolsParams {})
            .await
            .expect_err("uninitialized cache must error, not return an empty list");
        match err {
            CommandError::Internal(msg) => {
                assert!(msg.contains("not initialized"), "got: {msg}")
            }
            other => panic!("expected Internal, got {other:?}"),
        }
    }
}
