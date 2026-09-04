//! `cognition/semantic-search-tools` — rank cached tool embeddings against a query
//! (typed, stateless).
//!
//! Oxidized semantic-search arm: embeds the query, ranks it against the cached tool
//! embeddings (warmed by `cognition/embed-tools`), and returns the top hits with cosine
//! similarity. Holds no module state — [`semantic_search_tools`] is a free async function
//! over the request — so this is a stateless
//! [`ActionCommand`](crate::sdk_codegen::ActionCommand) unit struct: `action_command!`
//! publishes both the descriptor and the runtime object via `inventory`, no `commands()`
//! ceremony.
//!
//! `access: Internal` — substrate cognition IPC the host invokes to rank tools for a
//! turn, NOT a persona toolbelt verb. Registered and grid-routable, but the trust policy
//! denies remote peers from querying another node's cache.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::cognition::tool_embedding::{
    semantic_search_tools, SemanticSearchResult, SemanticSearchToolsRequest,
};
use crate::sdk_codegen::CommandError;

/// Result of `cognition/semantic-search-tools`: the ranked hits.
///
/// A NAMED wrapper around `Vec<SemanticSearchResult>` — the command-schema
/// validator ([`crate::sdk_codegen`]) rejects a bare `Vec<T>` output because an
/// inline collection has no named TS type (it can't be `export_to`'d), and one
/// such command panics the whole `command_registry()` walk (→ `commands/list`
/// panics, uu can't fetch schemas, every schema-canonicalized flag breaks). Same
/// shape as `McpSearchToolsResult` wrapping `tools: Vec<McpSearchHit>`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/SemanticSearchToolsResult.ts"
)]
pub struct SemanticSearchToolsResult {
    pub results: Vec<SemanticSearchResult>,
}

crate::action_command! {
    /// Rank the cached tool embeddings against a query and return the top hits with
    /// cosine similarity. Requires the cache to be warmed via `cognition/embed-tools`
    /// with the same model. Host-invoked to select tools for a turn; not a persona
    /// toolbelt verb.
    pub struct SemanticSearchTools;
    name: "cognition/semantic-search-tools",
    access: Internal,
    params: SemanticSearchToolsRequest,
    output: SemanticSearchToolsResult,
    run(_this, _ctx, req) => {
        let results = semantic_search_tools(req)
            .await
            .map_err(|e| CommandError::Internal(e.to_string()))?;
        Ok(SemanticSearchToolsResult { results })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand};

    // what this catches: the name + access contract. `cognition/semantic-search-tools`
    // is host-driven cognition IPC (ranks tools for a turn), so it is Internal —
    // registered and grid-routable, but never a remote-callable persona toolbelt verb.
    #[test]
    fn name_and_access_are_the_contract() {
        assert_eq!(SemanticSearchTools::NAME, "cognition/semantic-search-tools");
        assert_eq!(SemanticSearchTools::ACCESS, AccessLevel::Internal);
    }
}
