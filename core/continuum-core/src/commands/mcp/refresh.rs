//! `mcp/refresh` — signal that the tool catalog should be rebuilt (typed,
//! stateless).
//!
//! The catalog cache is built once at `MCPModule::initialize` from the live
//! registry, and neither the legacy arm nor this command has a
//! [`ModuleContext`](crate::runtime::ModuleContext) to rebuild it from — so this
//! is a no-op signal: the cache refreshes on the next initialization. It carries
//! no module state (nothing to read), so it's the **stateless** auto-registered
//! form rather than a dep-holding catalog verb.
//!
//! `access: Privileged` — a control/refresh op, not read-only discovery; gated a
//! notch above the `mcp/list|search|help` catalog reads.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Params for `mcp/refresh`: none.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/mcp/McpRefreshParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct McpRefreshParams {}

/// Result of `mcp/refresh`: the refresh-deferred acknowledgement.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/mcp/McpRefreshResult.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct McpRefreshResult {
    pub message: String,
}

crate::action_command! {
    /// Request a rebuild of the MCP tool catalog. The cache is built at startup from the
    /// live registry; this signals a refresh that takes effect on the next initialization.
    pub struct McpRefresh;
    name: "mcp/refresh",
    access: Privileged,
    params: McpRefreshParams,
    output: McpRefreshResult,
    run(_this, _ctx, _params) => {
        Ok(McpRefreshResult {
            message: "Tools will be refreshed on next initialization".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{ActionCommand, Ctx};

    // what this catches: refresh is a pure signal — it must succeed with the
    // deferred-refresh message and never touch module state (regression guard for
    // anyone tempted to make it synchronously rebuild without a ModuleContext).
    #[tokio::test]
    async fn refresh_returns_deferred_message() {
        let out = McpRefresh
            .run(&Ctx::default(), McpRefreshParams {})
            .await
            .expect("refresh is infallible");
        assert!(
            out.message.contains("next initialization"),
            "got: {}",
            out.message
        );
    }
}
