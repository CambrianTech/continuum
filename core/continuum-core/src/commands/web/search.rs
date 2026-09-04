//! `web/search` — search the general web for the persona.

use super::{web_search, WebSearchParams, WebSearchResult};

crate::action_command! {
    /// Search the web for current info beyond your memory (docs, APIs, errors,
    /// how-tos). Returns ranked results (title, url, snippet) to read with web/fetch.
    pub struct WebSearch;
    name: "web/search",
    access: AiSafe,
    native: true, // her hands must be able to REACH the web (a native-call model can only emit calls in its offered specs); I forage constantly, so must she — direct SWE/task score lever
    params: WebSearchParams,
    output: WebSearchResult,
    run(_this, _ctx, p) => { web_search(p).await }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::ActionCommand;

    // what this catches: the wire name (routing key — a drift unwires the hand) and
    // that the description is the CONCISE model-facing line it became when web went
    // NATIVE (2026-08-25): the adapter/keyless detail moved OUT of DESCRIPTION — which
    // rides the token-budgeted native surface — and INTO the `adapter` param doc, so
    // the surface stayed under its ceiling. The description must still say what the
    // tool DOES (search the web) and point at its partner (web/fetch); the
    // adapter/keyless guidance is asserted on the PARAM, its new home.
    #[test]
    fn name_and_description() {
        assert_eq!(WebSearch::NAME, "web/search");
        let d = WebSearch::DESCRIPTION.to_lowercase();
        assert!(d.contains("search") && d.contains("web"), "names what it does: {}", WebSearch::DESCRIPTION);
        assert!(d.contains("web/fetch"), "points at its read partner");
        // The adapter/keyless detail now lives on the param, not the surface-costed DESCRIPTION.
        let schema = serde_json::to_string(&schemars::schema_for!(WebSearchParams)).unwrap_or_default();
        assert!(schema.contains("brave") && schema.contains("duckduckgo"),
            "adapter guidance lives in the param schema now");
    }
}
