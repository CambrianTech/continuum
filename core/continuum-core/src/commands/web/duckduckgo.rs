//! DuckDuckGo backend — the KEYLESS web-search floor.
//!
//! Uses the DuckDuckGo **Instant Answer API** (`api.duckduckgo.com/?format=json`).
//! No key, no signup, clean JSON — so a persona on a fresh machine still has a
//! working web hand. This was a deliberate choice over scraping the DDG HTML
//! endpoints: those serve a bot-block ("anomaly") page to non-browser/server
//! IPs, so a naive scrape silently returns nothing. The Instant Answer API is
//! the reliable keyless path.
//!
//! The honest trade-off: this returns CURATED results (a Wikipedia-grade
//! abstract + official-site links + related topics), not a full ranked web
//! crawl. It is excellent for "what is X / who is Y / official site" foraging
//! and weak for long-tail or freshness queries. That is exactly why it is the
//! floor: configure `BRAVE_API_KEY` for full web search (the `brave` adapter).

use std::time::Duration;

use serde::Deserialize;

use super::{WebHit, WebSearchProvider};
use crate::sdk_codegen::CommandError;

/// Keyless DuckDuckGo provider (Instant Answer API). Stateless.
pub struct DuckDuckGoProvider;

#[async_trait::async_trait]
impl WebSearchProvider for DuckDuckGoProvider {
    fn id(&self) -> &'static str {
        "duckduckgo"
    }

    fn requires_key(&self) -> bool {
        false
    }

    fn available(&self) -> bool {
        true
    }

    async fn search(&self, query: &str, count: u32) -> Result<Vec<WebHit>, CommandError> {
        let client = reqwest::Client::builder()
            .user_agent(concat!("continuum/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(|e| CommandError::Internal(format!("HTTP client init failed: {e}")))?;

        let resp = client
            .get("https://api.duckduckgo.com/")
            .query(&[
                ("q", query),
                ("format", "json"),
                ("no_html", "1"),
                ("no_redirect", "1"),
                ("t", "continuum"),
            ])
            .send()
            .await
            .map_err(|e| {
                CommandError::Internal(format!("DuckDuckGo search request failed: {e}"))
            })?;

        let status = resp.status();
        if !status.is_success() {
            return Err(CommandError::Internal(format!(
                "DuckDuckGo returned {status} for query '{query}' (the keyless backend \
                 may be throttling — retry, or configure BRAVE_API_KEY for full web search)"
            )));
        }

        let parsed: DdgInstant = resp.json().await.map_err(|e| {
            CommandError::Internal(format!("could not parse DuckDuckGo response: {e}"))
        })?;

        Ok(parsed.into_hits(count))
    }
}

/// The slice of the Instant Answer response we surface. Field names are DDG's
/// PascalCase, renamed to snake_case. All optional/defaulted — an instant answer
/// with no abstract (just related topics) is legitimate, not an error.
#[derive(Debug, Deserialize)]
struct DdgInstant {
    #[serde(default, rename = "Heading")]
    heading: String,
    #[serde(default, rename = "AbstractText")]
    abstract_text: String,
    #[serde(default, rename = "AbstractURL")]
    abstract_url: String,
    #[serde(default, rename = "Results")]
    results: Vec<DdgTopic>,
    #[serde(default, rename = "RelatedTopics")]
    related: Vec<DdgRelated>,
}

/// A flat topic ({Text, FirstURL}) — used for `Results` and leaf `RelatedTopics`.
#[derive(Debug, Deserialize)]
struct DdgTopic {
    #[serde(default, rename = "Text")]
    text: String,
    #[serde(default, rename = "FirstURL")]
    first_url: String,
}

/// A `RelatedTopics` entry: either a leaf topic (`Text`/`FirstURL`) or a named
/// group carrying nested `Topics`. Modeled as one struct with optional fields.
#[derive(Debug, Deserialize)]
struct DdgRelated {
    #[serde(default, rename = "Text")]
    text: Option<String>,
    #[serde(default, rename = "FirstURL")]
    first_url: Option<String>,
    #[serde(default, rename = "Topics")]
    topics: Option<Vec<DdgTopic>>,
}

impl DdgInstant {
    /// Project the instant answer into ranked hits: the abstract first (the best
    /// single answer), then official `Results`, then `RelatedTopics` (flattening
    /// groups). Empty-URL entries are dropped; the list is capped to `count`.
    fn into_hits(self, count: u32) -> Vec<WebHit> {
        let mut hits: Vec<WebHit> = Vec::new();

        if !self.abstract_text.is_empty() && !self.abstract_url.is_empty() {
            hits.push(WebHit {
                title: if self.heading.is_empty() {
                    self.abstract_text.clone()
                } else {
                    self.heading.clone()
                },
                url: self.abstract_url.clone(),
                snippet: self.abstract_text.clone(),
            });
        }

        for r in self.results {
            if let Some(h) = topic_to_hit(r) {
                hits.push(h);
            }
        }

        for entry in self.related {
            match (entry.first_url, entry.topics) {
                (Some(url), _) if !url.is_empty() => {
                    if let Some(h) = topic_to_hit(DdgTopic {
                        text: entry.text.unwrap_or_default(),
                        first_url: url,
                    }) {
                        hits.push(h);
                    }
                }
                (_, Some(group)) => {
                    for t in group {
                        if let Some(h) = topic_to_hit(t) {
                            hits.push(h);
                        }
                    }
                }
                _ => {}
            }
        }

        hits.truncate(count as usize);
        hits
    }
}

/// Build a hit from a topic, deriving a short title from the leading clause of
/// the descriptive text. Returns None for entries with no URL or no text.
fn topic_to_hit(t: DdgTopic) -> Option<WebHit> {
    if t.first_url.is_empty() || t.text.is_empty() {
        return None;
    }
    let title = t
        .text
        .split(" - ")
        .next()
        .unwrap_or(&t.text)
        .trim()
        .to_string();
    Some(WebHit {
        title,
        url: t.first_url,
        snippet: t.text,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the Instant Answer JSON shape projects into ranked hits
    // — abstract first (heading as title, abstract URL), then official Results,
    // then flattened RelatedTopics (leaf + nested group), empty-URL entries
    // dropped. This is the keyless path's load-bearing mapping, pinned offline so
    // a DDG schema change is caught here, not as silent zero results.
    #[test]
    fn projects_instant_answer_into_hits() {
        let json = r#"{
            "Heading": "Rust (programming language)",
            "AbstractText": "Rust is a general-purpose programming language.",
            "AbstractURL": "https://en.wikipedia.org/wiki/Rust_(programming_language)",
            "Results": [
                {"Text": "Official site", "FirstURL": "https://www.rust-lang.org/"}
            ],
            "RelatedTopics": [
                {"Text": "Outline of Rust - the following outline", "FirstURL": "https://duckduckgo.com/Outline"},
                {"Name": "Languages", "Topics": [
                    {"Text": "Cargo - the Rust package manager", "FirstURL": "https://doc.rust-lang.org/cargo/"}
                ]},
                {"Text": "no url topic", "FirstURL": ""}
            ]
        }"#;
        let parsed: DdgInstant = serde_json::from_str(json).expect("parse instant answer");
        let hits = parsed.into_hits(10);
        assert_eq!(hits.len(), 4, "abstract + 1 result + 2 related (empty-url dropped)");
        assert_eq!(hits[0].title, "Rust (programming language)");
        assert_eq!(hits[0].url, "https://en.wikipedia.org/wiki/Rust_(programming_language)");
        assert_eq!(hits[1].title, "Official site");
        // leading clause before " - " becomes the title
        assert_eq!(hits[2].title, "Outline of Rust");
        assert_eq!(hits[3].url, "https://doc.rust-lang.org/cargo/");
    }

    // what this catches: `count` caps the projected hits.
    #[test]
    fn respects_count_cap() {
        let json = r#"{
            "AbstractText": "x", "AbstractURL": "https://a.com",
            "Results": [
                {"Text": "b", "FirstURL": "https://b.com"},
                {"Text": "c", "FirstURL": "https://c.com"}
            ]
        }"#;
        let parsed: DdgInstant = serde_json::from_str(json).expect("parse");
        assert_eq!(parsed.into_hits(2).len(), 2);
    }

    // what this catches: a response with no abstract (only related topics) is a
    // legitimate empty-abstract result, projected without error.
    #[test]
    fn no_abstract_is_fine() {
        let parsed: DdgInstant = serde_json::from_str(r#"{"RelatedTopics":[]}"#).expect("parse");
        assert!(parsed.into_hits(10).is_empty());
    }
}
