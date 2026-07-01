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

use crate::cognition::tool_embedding::{
    semantic_search_tools, SemanticSearchResult, SemanticSearchToolsRequest,
};
use crate::sdk_codegen::CommandError;

crate::action_command! {
    /// Rank the cached tool embeddings against a query and return the top hits with
    /// cosine similarity. Requires the cache to be warmed via `cognition/embed-tools`
    /// with the same model. Host-invoked to select tools for a turn; not a persona
    /// toolbelt verb.
    pub struct SemanticSearchTools;
    name: "cognition/semantic-search-tools",
    access: Internal,
    params: SemanticSearchToolsRequest,
    output: Vec<SemanticSearchResult>,
    run(_this, _ctx, req) => {
        semantic_search_tools(req)
            .await
            .map_err(|e| CommandError::Internal(e.to_string()))
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
        assert_eq!(
            SemanticSearchTools::NAME,
            "cognition/semantic-search-tools"
        );
        assert_eq!(SemanticSearchTools::ACCESS, AccessLevel::Internal);
    }
}
