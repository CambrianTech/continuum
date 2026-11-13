//! `search/list` — enumerate the available search algorithms.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::engine::AlgorithmRegistry;

/// Params for `search/list` (none — it reports the full algorithm catalog).
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/search/SearchListParams.ts"
)]
pub struct SearchListParams {}

/// The available search algorithms.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/search/SearchListResult.ts"
)]
pub struct SearchListResult {
    /// Algorithm names that `search/execute` accepts (bow, bm25, cosine).
    pub algorithms: Vec<String>,
}

crate::action_command! {
    /// List the search algorithms available to `search/execute` (bow, bm25,
    /// cosine). Use to discover what ranking methods this node offers before
    /// running a search.
    pub struct SearchList;
    name: "search/list",
    access: AiSafe,
    params: SearchListParams,
    output: SearchListResult,
    run(_this, _ctx, _p) => {
        let registry = AlgorithmRegistry::new();
        let mut algorithms: Vec<String> =
            registry.list().iter().map(|s| s.to_string()).collect();
        algorithms.sort();
        Ok(SearchListResult { algorithms })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::ActionCommand;

    // what this catches: the command reports all three algorithms — a dropped
    // factory would shrink the catalog the persona surface advertises.
    #[tokio::test]
    async fn lists_all_three() {
        let out = SearchList
            .run(&crate::sdk_codegen::Ctx::default(), SearchListParams {})
            .await
            .unwrap();
        assert_eq!(out.algorithms, vec!["bm25", "bow", "cosine"]);
    }
}
