//! Brave Search backend — the keyed, best-quality web-search provider.

use std::time::Duration;

use serde::Deserialize;

use super::{strip_tags, WebHit, WebSearchProvider};
use crate::sdk_codegen::CommandError;

/// Reads the Brave key fresh from `~/.continuum/config.env` (so a just-pasted
/// key works without a core restart) ∪ the boot-cached env/secret reader. Same
/// logical secret, two legitimate locations — not a fallback.
fn brave_key() -> Option<String> {
    crate::config_env::read("BRAVE_API_KEY")
        .filter(|v| !v.trim().is_empty())
        .or_else(|| crate::secrets::get_secret("BRAVE_API_KEY").map(|s| s.to_string()))
        .map(|s| s.trim().to_string())
}

/// Brave Search API provider. Stateless — reads the key per call.
pub struct BraveSearchProvider;

#[async_trait::async_trait]
impl WebSearchProvider for BraveSearchProvider {
    fn id(&self) -> &'static str {
        "brave"
    }

    fn requires_key(&self) -> bool {
        true
    }

    fn available(&self) -> bool {
        brave_key().is_some()
    }

    async fn search(&self, query: &str, count: u32) -> Result<Vec<WebHit>, CommandError> {
        // available() is checked by the selector before we get here, but read
        // again and fail loud if the key vanished between selection and call.
        let key = brave_key().ok_or_else(|| {
            CommandError::Invalid(
                "BRAVE_API_KEY is not configured. Get a free key at \
                 https://brave.com/search/api/ and add it to ~/.continuum/config.env."
                    .to_string(),
            )
        })?;

        let client = reqwest::Client::builder()
            .user_agent(concat!("continuum/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(|e| CommandError::Internal(format!("HTTP client init failed: {e}")))?;

        let resp = client
            .get("https://api.search.brave.com/res/v1/web/search")
            .header("Accept", "application/json")
            .header("X-Subscription-Token", key)
            .query(&[("q", query), ("count", &count.to_string())])
            .send()
            .await
            .map_err(|e| CommandError::Internal(format!("Brave search request failed: {e}")))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp
                .text()
                .await
                .unwrap_or_else(|_| "<error body unreadable>".to_string());
            return Err(CommandError::Internal(format!(
                "Brave Search returned {status} for query '{query}': {}",
                body.chars().take(200).collect::<String>()
            )));
        }

        let parsed: BraveResp = resp
            .json()
            .await
            .map_err(|e| CommandError::Internal(format!("could not parse Brave response: {e}")))?;

        let hits = parsed
            .web
            .map(|w| w.results)
            .unwrap_or_default()
            .into_iter()
            .map(|item| WebHit {
                title: strip_tags(&item.title),
                url: item.url,
                snippet: strip_tags(&item.description),
            })
            .collect();
        Ok(hits)
    }
}

/// Brave's response shape (only the fields we surface). `web` is absent when a
/// query yields no web results — a legitimate empty, not an error.
#[derive(Debug, Deserialize)]
struct BraveResp {
    #[serde(default)]
    web: Option<BraveWeb>,
}

#[derive(Debug, Deserialize)]
struct BraveWeb {
    #[serde(default)]
    results: Vec<BraveItem>,
}

#[derive(Debug, Deserialize)]
struct BraveItem {
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    description: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the Brave JSON shape (web.results[] with title/url/
    // description, web absent on no-results) parses and maps to WebHits with the
    // highlight markup stripped — the offline core of the Brave path, no key/net.
    #[test]
    fn parses_brave_response_and_strips_markup() {
        let json = r#"{"web":{"results":[
            {"title":"<strong>Rust</strong> lang","url":"https://rust-lang.org","description":"A <strong>fast</strong> language"}
        ]}}"#;
        let parsed: BraveResp = serde_json::from_str(json).expect("parse brave");
        let hits: Vec<WebHit> = parsed
            .web
            .map(|w| w.results)
            .unwrap_or_default()
            .into_iter()
            .map(|item| WebHit {
                title: strip_tags(&item.title),
                url: item.url,
                snippet: strip_tags(&item.description),
            })
            .collect();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "Rust lang");
        assert_eq!(hits[0].url, "https://rust-lang.org");
        assert_eq!(hits[0].snippet, "A fast language");
    }

    // what this catches: a no-web-results response (the `web` key absent) yields
    // an empty hit list, never a parse error — an empty search is legitimate.
    #[test]
    fn no_web_results_is_empty_not_error() {
        let parsed: BraveResp =
            serde_json::from_str(r#"{"query":{"original":"x"}}"#).expect("parse");
        assert!(parsed.web.is_none());
    }
}
