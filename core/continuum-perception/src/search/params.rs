//! `search/params` — inspect an algorithm's tunable parameters and their current
//! values. The read half of the algorithm-backed tuning surface: a widget or
//! persona reads these to know what it can modify; writes go back through the
//! algorithm's validated `set_param` (via `search/execute`'s `params` bag), so a
//! bad value is rejected, never crashes the search.

use std::collections::HashMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use super::engine::AlgorithmRegistry;
use crate::sdk_codegen::CommandError;

/// Params for `search/params`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/search/SearchParamsParams.ts"
)]
pub struct SearchParamsParams {
    /// Which algorithm's parameters to report (bow, bm25, cosine).
    pub algorithm: String,
}

/// An algorithm's tunable parameters and their default values.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/search/SearchParamsResult.ts"
)]
pub struct SearchParamsResult {
    /// The algorithm these parameters belong to.
    pub algorithm: String,
    /// The names of the tunable parameters.
    pub params: Vec<String>,
    /// Current value of each parameter, keyed by name.
    #[ts(type = "Record<string, unknown>")]
    pub values: HashMap<String, Value>,
}

crate::action_command! {
    /// Report the tunable parameters of a search algorithm (e.g. bm25's `k1`/`b`,
    /// cosine's `normalize`/`threshold`) and their current values. Use to discover
    /// what `search/execute`'s `params` bag can tune before adjusting a search.
    pub struct SearchParams;
    name: "search/params",
    access: AiSafe,
    params: SearchParamsParams,
    output: SearchParamsResult,
    run(_this, _ctx, p) => {
        let registry = AlgorithmRegistry::new();
        let algo = registry
            .create(&p.algorithm)
            .ok_or_else(|| CommandError::Invalid(format!("Unknown algorithm: {}", p.algorithm)))?;
        let values: HashMap<String, Value> = algo
            .param_names()
            .iter()
            .filter_map(|name| algo.get_param(name).map(|v| (name.to_string(), v)))
            .collect();
        Ok(SearchParamsResult {
            algorithm: algo.name().to_string(),
            params: algo.param_names().iter().map(|s| s.to_string()).collect(),
            values,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::ActionCommand;

    // what this catches: bm25 reports its k1/b tunables with concrete current
    // values — the contract a widget/persona reads before tuning.
    #[tokio::test]
    async fn bm25_reports_k1_and_b() {
        let out = SearchParams
            .run(
                &crate::sdk_codegen::Ctx::default(),
                SearchParamsParams {
                    algorithm: "bm25".into(),
                },
            )
            .await
            .unwrap();
        assert_eq!(out.algorithm, "bm25");
        assert!(out.params.contains(&"k1".to_string()));
        assert!(out.values.contains_key("b"));
    }
}
