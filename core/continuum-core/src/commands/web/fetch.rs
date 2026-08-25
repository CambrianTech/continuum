//! `web/fetch` — read the readable text of a web page (the partner to `web/search`).

use super::{web_fetch, WebFetchParams, WebFetchResult};

crate::action_command! {
    /// Fetch a URL and READ its text — a doc page, API reference, article, or a result
    /// `web/search` returned. Strips scripts/markup and returns clean, capped text you can
    /// quote or reason over. This is how you actually read what you find online instead of
    /// guessing from a snippet. Pair it with `web/search`: search to find the page, fetch to read it.
    pub struct WebFetch;
    name: "web/fetch",
    access: AiSafe,
    native: true, // her hands must be able to REACH the web (a native-call model can only emit calls in its offered specs); I forage constantly, so must she — direct SWE/task score lever
    params: WebFetchParams,
    output: WebFetchResult,
    run(_this, _ctx, p) => { web_fetch(p).await }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::ActionCommand;

    // what this catches: the wire name (the routing key — a drift unwires the hand) and a
    // description that tells the persona to READ pages and pair with search, so the web hand
    // is discoverable as a foraging pair, not a lone snippet tool.
    #[test]
    fn name_and_description() {
        assert_eq!(WebFetch::NAME, "web/fetch");
        assert!(WebFetch::DESCRIPTION.contains("web/search"));
        assert!(WebFetch::DESCRIPTION.to_lowercase().contains("read"));
    }
}
