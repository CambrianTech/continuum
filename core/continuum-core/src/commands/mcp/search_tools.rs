//! `mcp/search-tools` — keyword-ranked subset of the command catalog (typed,
//! dep-holding).
//!
//! Ranks the cached catalog against a query (name/description/segment matches)
//! and returns the top `limit` hits. Reached by the model via `tools/call`, whose
//! result is passed through as text — so the migration is free to give the hits a
//! typed shape ([`McpSearchHit`]) as long as the fields survive.
//!
//! `access: AiSafe` — read-only discovery, same trust class as `mcp/list-tools`.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::modules::mcp::McpCatalog;
use crate::sdk_codegen::CommandError;

/// Default result cap when the caller omits `limit`. Transplanted verbatim from
/// the legacy `p.u64_or("limit", 10)` read.
fn default_limit() -> u64 {
    10
}

/// Params for `mcp/search-tools`: the query plus an optional result cap.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/mcp/McpSearchToolsParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct McpSearchToolsParams {
    /// What to search for — keywords matched against tool names, descriptions,
    /// and path segments (e.g. "chat", "read file", "gpu").
    pub query: String,
    #[serde(default = "default_limit")]
    #[ts(type = "number")]
    pub limit: u64,
}

/// One search hit: a matched command's name, its description, and the `/`-style
/// jtag form. Mirrors the JSON the legacy arm emitted per result.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/mcp/McpSearchHit.ts")]
pub struct McpSearchHit {
    pub name: String,
    pub description: String,
    pub jtag_command: String,
}

/// Result of `mcp/search-tools`: the echoed query, the hit count, and the ranked
/// hits.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/mcp/McpSearchToolsResult.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct McpSearchToolsResult {
    pub query: String,
    #[ts(type = "number")]
    pub count: usize,
    pub tools: Vec<McpSearchHit>,
}

crate::action_command! {
    /// Search the command catalog by keyword: rank registered tools against the query
    /// (name, description, and path-segment matches) and return the top `limit` hits.
    /// The discovery verb a persona uses to find the right command without listing all.
    pub struct McpSearchTools { catalog: Arc<McpCatalog> }
    name: "mcp/search-tools",
    access: AiSafe,
    params: McpSearchToolsParams,
    output: McpSearchToolsResult,
    run(this, _ctx, params) => {
        let tools = this.catalog.list()?;
        let hits = this
            .catalog
            .search_tools(&tools, &params.query, params.limit as usize);
        let tools: Vec<McpSearchHit> = hits
            .into_iter()
            .map(serde_json::from_value)
            .collect::<Result<_, _>>()
            .map_err(|e| CommandError::Internal(format!("search hit shape drift: {e}")))?;
        Ok(McpSearchToolsResult {
            query: params.query,
            count: tools.len(),
            tools,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::mcp::McpCatalog;
    use crate::sdk_codegen::{ActionCommand, Ctx};

    // what this catches: search against an uninitialized catalog must FAIL LOUD
    // (Internal), not silently return zero hits — a zero-hit result must mean
    // "nothing matched", never "the cache wasn't built". The ranked-hit behavior
    // over a live catalog is covered by the module-harness test in `modules/mcp.rs`.
    #[tokio::test]
    async fn search_on_uninitialized_catalog_fails_loud() {
        let cmd = McpSearchTools {
            catalog: Arc::new(McpCatalog::new()),
        };
        let err = cmd
            .run(
                &Ctx::default(),
                McpSearchToolsParams {
                    query: "chat".to_string(),
                    limit: default_limit(),
                },
            )
            .await
            .expect_err("uninitialized cache must error, not return zero hits");
        match err {
            CommandError::Internal(msg) => {
                assert!(msg.contains("not initialized"), "got: {msg}")
            }
            other => panic!("expected Internal, got {other:?}"),
        }
    }
}
