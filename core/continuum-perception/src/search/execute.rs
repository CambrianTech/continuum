//! `search/execute` — rank a corpus of strings against a query with a named
//! text-search algorithm (bm25 [default], bow, cosine).

use std::collections::HashMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use super::engine::{AlgorithmRegistry, SearchInput, SearchResult};
use crate::sdk_codegen::CommandError;

/// Params for `search/execute`.
///
/// `params` is the algorithm tuning bag (e.g. `{"k1": 1.5, "b": 0.6}` for bm25).
/// Each entry is applied through the algorithm's own `set_param`, which validates
/// the value — a bad type is rejected with a clear error instead of crashing the
/// search. A widget or persona tunes the algorithm by passing this; the defaults
/// stand when it is empty.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/search/SearchExecuteParams.ts"
)]
pub struct SearchExecuteParams {
    /// Algorithm to rank with: bm25 (default), bow, or cosine.
    #[serde(default)]
    pub algorithm: Option<String>,
    /// The query to rank the corpus against.
    pub query: String,
    /// The documents to rank.
    #[serde(default)]
    pub corpus: Vec<String>,
    /// Optional per-call algorithm tuning, applied via the algorithm's own
    /// validated `set_param` (defaults stand when empty).
    #[serde(default)]
    #[ts(type = "Record<string, unknown>")]
    pub params: HashMap<String, Value>,
}

crate::action_command! {
    /// Rank a corpus of strings against a query using a text-search algorithm
    /// (bm25 [default], bow, or cosine). Returns per-document relevance scores
    /// (0-1) and the corpus indices sorted best-first. Use to find the most
    /// relevant items in a list of strings.
    pub struct SearchExecute;
    name: "search/execute",
    access: AiSafe,
    params: SearchExecuteParams,
    output: SearchResult,
    run(_this, _ctx, p) => {
        let algorithm = p.algorithm.as_deref().unwrap_or("bm25").to_string();
        let registry = AlgorithmRegistry::new();
        let algo = if p.params.is_empty() {
            registry
                .create(&algorithm)
                .ok_or_else(|| CommandError::Invalid(format!("Unknown algorithm: {algorithm}")))?
        } else {
            registry
                .create_with_params(&algorithm, &p.params)
                .map_err(CommandError::Invalid)?
        };
        let output = algo.execute(&SearchInput {
            query: p.query,
            corpus: p.corpus,
        });
        Ok(SearchResult {
            algorithm,
            scores: output.scores,
            ranked_indices: output.ranked_indices,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::ActionCommand;

    // what this catches: the command NAME stays mirrored to its path and the
    // model-facing DESCRIPTION fills from the doc comment — the contract the
    // persona tool surface and ACL read.
    #[test]
    fn name_and_description_are_wired() {
        assert_eq!(SearchExecute::NAME, "search/execute");
        assert!(SearchExecute::DESCRIPTION.contains("Rank a corpus"));
    }

    // what this catches: an unknown algorithm name fails as Invalid params (not a
    // panic, not Internal) — the fail-loud contract for bad caller input.
    #[tokio::test]
    async fn unknown_algorithm_is_invalid() {
        let cmd = SearchExecute;
        let err = cmd
            .run(
                &crate::sdk_codegen::Ctx::default(),
                SearchExecuteParams {
                    algorithm: Some("nope".into()),
                    query: "x".into(),
                    corpus: vec!["a".into()],
                    params: HashMap::new(),
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(err, CommandError::Invalid(_)));
    }
}
