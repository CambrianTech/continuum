//! `cognition/embed-tools` — embed a batch of tool descriptions (typed, stateless).
//!
//! Oxidized tool-embedding arm: computes an embedding vector per tool surface and caches
//! it (name + description → vector) for later semantic search. Holds no module state —
//! [`embed_tools`] is a free async function over the request — so this is a stateless
//! [`ActionCommand`](crate::sdk_codegen::ActionCommand) unit struct: `action_command!`
//! publishes both the descriptor and the runtime object via `inventory`, no `commands()`
//! ceremony.
//!
//! `access: Internal` — substrate cognition IPC the host invokes to warm the tool
//! embedding cache, NOT a persona toolbelt verb. Registered and grid-routable, but the
//! trust policy denies remote peers from driving another node's cache.

use crate::cognition::tool_embedding::{embed_tools, EmbedToolsRequest, EmbedToolsResponse};
use crate::sdk_codegen::CommandError;

crate::action_command! {
    /// Embed a batch of tool descriptions and cache the vectors for semantic search.
    /// Returns per-tool embeddings plus provenance (model, dimensionality). Host-invoked
    /// to warm the tool-embedding cache; not a persona toolbelt verb.
    pub struct EmbedTools;
    name: "cognition/embed-tools",
    access: Internal,
    params: EmbedToolsRequest,
    output: EmbedToolsResponse,
    run(_this, _ctx, req) => {
        embed_tools(req)
            .await
            .map_err(|e| CommandError::Internal(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand};

    // what this catches: the name + access contract. `cognition/embed-tools` is
    // host-driven cognition IPC (warms the tool-embedding cache), so it is Internal —
    // registered and grid-routable, but never a remote-callable persona toolbelt verb.
    #[test]
    fn name_and_access_are_the_contract() {
        assert_eq!(EmbedTools::NAME, "cognition/embed-tools");
        assert_eq!(EmbedTools::ACCESS, AccessLevel::Internal);
    }
}
