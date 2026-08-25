//! `web/search` — search the general web for the persona.

use super::{web_search, WebSearchParams, WebSearchResult};

crate::action_command! {
    /// Search the WEB for current information beyond your memory — docs, articles,
    /// papers, news, how-tos. Returns ranked results (title, url, snippet) you can
    /// cite or read further. Pick a backend with `adapter`: "brave" (best — needs a
    /// free BRAVE_API_KEY) or "duckduckgo" (keyless, works with no setup). Omit
    /// `adapter` to auto-use the best available — so this works even with no API
    /// key configured. Use it to forage for what you don't know yet.
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

    // what this catches: the wire name + a description that names BOTH adapters and
    // the keyless guarantee — the persona is offered "web/search" with guidance that
    // it works without a key. Name is the routing key; a drift unwires the hand.
    #[test]
    fn name_and_description() {
        assert_eq!(WebSearch::NAME, "web/search");
        assert!(WebSearch::DESCRIPTION.contains("brave"));
        assert!(WebSearch::DESCRIPTION.contains("duckduckgo"));
        assert!(WebSearch::DESCRIPTION.contains("no API key"));
    }
}
