//! `web/*` — the persona's general-web search hand.
//!
//! ## The hard lane, made polymorphic
//!
//! Unlike the HuggingFace lane ([`crate::commands::hf`], a clean keyless JSON
//! API), general web search is hostile: engines fight bots and the keyless
//! routes are poor. So `web/search` is built OpenCV-style — a
//! [`WebSearchProvider`] trait with a registry of interchangeable backends,
//! selected by an `adapter` param. Two outlier implementations prove the
//! interface:
//!
//! - [`brave::BraveSearchProvider`] — keyed, the BEST quality. A free
//!   `BRAVE_API_KEY` (https://brave.com/search/api/) in `~/.continuum/config.env`
//!   unlocks clean ranked JSON with no anti-bot fight.
//! - [`duckduckgo::DuckDuckGoProvider`] — KEYLESS, the floor. Uses the DDG
//!   Instant Answer JSON API so a persona on a fresh machine with no API key
//!   still gets results. Curated (abstract + official sites + related topics),
//!   not a full crawl — but reliable and zero-setup. (The HTML-scrape route was
//!   rejected: DDG bot-blocks server IPs with an "anomaly" page.)
//!
//! `adapter` omitted ⟹ auto-pick the best AVAILABLE provider (keyed if a key is
//! set, else the keyless one) — so **everyone is good without an API key**, and
//! anyone who adds one is immediately better. The chosen adapter is reported in
//! the result, so the selection is never hidden. Naming an adapter that needs a
//! missing key FAILS LOUD (it does not silently downgrade) — the auto path is
//! the only place selection is implicit, and it is fully surfaced.
//!
//! Like [`crate::commands::hf`], `web/search` is a stateless `AiSafe`
//! `ActionCommand` that auto-registers and routes through the live
//! `Decision::Act` → observe circuit with no extra wiring.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::sdk_codegen::CommandError;

pub mod brave;
pub mod duckduckgo;
pub mod search;

/// A pluggable web-search backend. Implementations are interchangeable; the
/// `adapter` param (or auto-selection) picks one. OpenCV `cv::Algorithm`-style:
/// one interface, many implementations, runtime-selectable.
#[async_trait::async_trait]
pub trait WebSearchProvider: Send + Sync {
    /// Stable id the `adapter` param selects and the result reports (e.g. "brave").
    fn id(&self) -> &'static str;
    /// Whether this backend needs an API key to run at all.
    fn requires_key(&self) -> bool;
    /// Usable right now? (key present if required.) Drives auto-selection.
    fn available(&self) -> bool;
    /// Run the search, returning ranked hits (the impl trims to `count`).
    async fn search(&self, query: &str, count: u32) -> Result<Vec<WebHit>, CommandError>;
}

/// Params for `web/search`.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/web/WebSearchParams.ts")]
pub struct WebSearchParams {
    /// What to search the web for.
    pub query: String,
    /// Max results. Default 8, clamped to [1, 20].
    #[serde(default)]
    #[ts(optional)]
    pub count: Option<u32>,
    /// Which backend to use: "brave" (best — needs a free BRAVE_API_KEY) or
    /// "duckduckgo" (keyless, works with zero setup). Omit to auto-pick the best
    /// available: keyed if a key is configured, else the keyless one.
    #[serde(default)]
    #[ts(optional)]
    pub adapter: Option<String>,
}

/// Result of a `web/search`.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/web/WebSearchResult.ts")]
pub struct WebSearchResult {
    pub query: String,
    /// Which adapter actually ran ("brave" | "duckduckgo") — selection is transparent.
    pub adapter: String,
    pub count: u32,
    pub hits: Vec<WebHit>,
}

/// One web result.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/web/WebHit.ts")]
pub struct WebHit {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

const DEFAULT_COUNT: u32 = 8;
const MAX_COUNT: u32 = 20;

/// The registry of known providers, in preference order (best first, keyless
/// floor last). The single source of truth for "which backends exist" — add a
/// provider here and it is selectable by id and eligible for auto-selection.
fn all_providers() -> Vec<Box<dyn WebSearchProvider>> {
    vec![
        Box::new(brave::BraveSearchProvider),
        Box::new(duckduckgo::DuckDuckGoProvider),
    ]
}

/// Match an adapter id, accepting common aliases ("ddg" ⟹ "duckduckgo").
fn id_matches(provider_id: &str, requested: &str) -> bool {
    let requested = requested.trim().to_lowercase();
    provider_id == requested
        || (provider_id == "duckduckgo" && (requested == "ddg" || requested == "duck"))
}

/// Select the provider for an optional adapter id.
///
/// - `Some(name)` ⟹ that exact provider. Unknown name ⟹ fail loud listing the
///   valid ids. Known but needs a missing key ⟹ fail loud naming the key and
///   pointing at the keyless alternative (NO silent downgrade).
/// - `None` ⟹ auto: the first AVAILABLE provider in preference order, so a user
///   with no API key still gets the keyless floor.
fn select_provider(adapter: Option<&str>) -> Result<Box<dyn WebSearchProvider>, CommandError> {
    let providers = all_providers();
    match adapter.map(str::trim).filter(|s| !s.is_empty()) {
        Some(name) => {
            let mut chosen = None;
            for p in providers {
                if id_matches(p.id(), name) {
                    chosen = Some(p);
                    break;
                }
            }
            let p = chosen.ok_or_else(|| {
                CommandError::Invalid(format!(
                    "unknown web/search adapter '{name}'. Valid adapters: 'brave' \
                     (needs BRAVE_API_KEY), 'duckduckgo' (keyless). Omit the adapter \
                     param to auto-pick the best available."
                ))
            })?;
            if p.requires_key() && !p.available() {
                return Err(CommandError::Invalid(format!(
                    "web/search adapter '{}' needs an API key that is not configured. \
                     Get a free key at https://brave.com/search/api/ and add \
                     BRAVE_API_KEY=<key> to ~/.continuum/config.env — or omit the \
                     adapter param to use the keyless 'duckduckgo' adapter.",
                    p.id()
                )));
            }
            Ok(p)
        }
        None => providers
            .into_iter()
            .find(|p| p.available())
            .ok_or_else(|| {
                CommandError::Internal(
                    "no web-search provider is available (this should not happen — \
                     the keyless adapter is always available)"
                        .to_string(),
                )
            }),
    }
}

/// Run a `web/search`: select the backend, search, report which adapter ran.
pub async fn web_search(p: WebSearchParams) -> Result<WebSearchResult, CommandError> {
    let count = p.count.unwrap_or(DEFAULT_COUNT).clamp(1, MAX_COUNT);
    let provider = select_provider(p.adapter.as_deref())?;
    let adapter = provider.id().to_string();
    let hits = provider.search(&p.query, count).await?;
    Ok(WebSearchResult {
        query: p.query,
        adapter,
        count: hits.len() as u32,
        hits,
    })
}

/// Strip HTML tags from a snippet/title fragment (engines return `<strong>`
/// highlight markup). Shared by both providers. Pure → unit-testable.
pub(crate) fn strip_tags(s: &str) -> String {
    use std::sync::LazyLock;
    static TAG_RE: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"<[^>]+>").expect("static tag regex is valid"));
    let no_tags = TAG_RE.replace_all(s, "");
    // Collapse the handful of entities the engines emit; leave the rest verbatim.
    no_tags
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#x27;", "'")
        .replace("&#39;", "'")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: auto-selection (adapter=None) always resolves to an
    // available provider — the keyless floor guarantees a user with no API key
    // still gets a working backend. If Brave is unconfigured (the default test
    // env), auto must land on duckduckgo, never error.
    #[test]
    fn auto_selects_keyless_when_no_key() {
        // No BRAVE_API_KEY in the test env ⟹ Brave unavailable ⟹ auto picks DDG.
        let p = select_provider(None).expect("auto-select always yields a provider");
        assert_eq!(p.id(), "duckduckgo");
        assert!(p.available());
    }

    // what this catches: an explicitly named keyed adapter with no key FAILS LOUD
    // (names the key + the keyless alternative) instead of silently downgrading —
    // the discipline that keeps the only implicit selection in the auto path.
    #[test]
    fn named_keyed_adapter_without_key_fails_loud() {
        let err = match select_provider(Some("brave")) {
            Ok(_) => panic!("brave must fail loud without a key"),
            Err(e) => e,
        };
        let msg = format!("{err:?}");
        assert!(msg.contains("BRAVE_API_KEY"), "names the missing key: {msg}");
        assert!(msg.contains("duckduckgo"), "points at the keyless fallback: {msg}");
    }

    // what this catches: unknown adapter id is rejected with the valid set, and
    // the "ddg" alias resolves to duckduckgo (the names a persona is likely to try).
    #[test]
    fn unknown_adapter_rejected_and_alias_resolves() {
        assert!(select_provider(Some("bing")).is_err());
        let p = select_provider(Some("ddg")).expect("ddg alias resolves");
        assert_eq!(p.id(), "duckduckgo");
    }

    // what this catches: tag stripping removes highlight markup and decodes the
    // common entities, so a hit's title/snippet is clean prose not raw HTML.
    #[test]
    fn strip_tags_cleans_highlight_markup() {
        assert_eq!(
            strip_tags("a <strong>fast</strong> &amp; clean result"),
            "a fast & clean result"
        );
    }
}
