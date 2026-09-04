//! DuckDuckGo backend — the KEYLESS web-search floor, driven through a REAL browser.
//!
//! Loads `html.duckduckgo.com/html/?q=…` in the host's headless Chromium (via
//! [`super::browser::render_dom`]) and parses the rendered results table into
//! [`WebHit`]s. Driving a real browser is what makes keyless search actually work:
//! a raw HTTP scrape gets DuckDuckGo's "anomaly" bot-block page (verified — it
//! returns 200 with zero results), while a real Chrome with a real UA gets real
//! organic results (also verified). This replaces both the old Instant-Answer API
//! (trivia only, `count:0` for developer queries) and the reqwest scrape (blocked).
//!
//! Still keyless and zero-setup — a persona on a fresh machine with Chrome/Chromium
//! installed has a working search hand. `BRAVE_API_KEY` (the `brave` adapter)
//! remains the higher-quality paid path; this is the floor that must actually work.

use super::{browser, WebHit, WebSearchProvider};
use crate::sdk_codegen::CommandError;

/// How long to let the results page render before dumping the DOM.
const SEARCH_SETTLE_MS: u64 = 3500;

/// Keyless DuckDuckGo provider (real browser → html endpoint → organic results). Stateless.
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
        // Needs a Chromium-family browser to drive; report honestly so auto-selection
        // and the tool surface reflect reality rather than promising a dead hand.
        crate::commands::interface::capture::web::locate_browser().is_some()
    }

    async fn search(&self, query: &str, count: u32) -> Result<Vec<WebHit>, CommandError> {
        let url = format!("https://html.duckduckgo.com/html/?q={}", url_encode(query));
        let dom = browser::render_dom(&url, SEARCH_SETTLE_MS).await?;
        let hits = parse_ddg_html(&dom, count);
        if hits.is_empty() && dom.contains("anomaly") {
            return Err(CommandError::Internal(
                "DuckDuckGo served its bot-block page even through the browser (heavy \
                 rate-limiting). For reliable search get a FREE key at \
                 https://brave.com/search/api/ and add BRAVE_API_KEY=<key> to \
                 ~/.continuum/config.env — web/search then auto-uses it."
                    .to_string(),
            ));
        }
        Ok(hits)
    }
}

/// Parse the `html.duckduckgo.com` results table into ranked hits. Each organic result
/// is an `<a class="result__a" href="…">title</a>` with a sibling
/// `<a class="result__snippet">snippet</a>`; the two streams are zipped by rank, the DDG
/// redirect is unwrapped, tags/entities are stripped, empty rows dropped, capped to `count`.
fn parse_ddg_html(html: &str, count: u32) -> Vec<WebHit> {
    use regex::Regex;
    // Capture the anchor's attrs (any order) + inner text; pull href out of the attrs.
    let a_re = Regex::new(r#"(?s)<a\b([^>]*\bclass="result__a"[^>]*)>(.*?)</a>"#)
        .expect("static result__a regex");
    let href_re = Regex::new(r#"href="([^"]+)""#).expect("static href regex");
    let snip_re = Regex::new(r#"(?s)class="result__snippet"[^>]*>(.*?)</a>"#)
        .expect("static result__snippet regex");

    let snippets: Vec<String> = snip_re
        .captures_iter(html)
        .map(|c| clean_text(&c[1]))
        .collect();

    let mut hits = Vec::new();
    for (i, cap) in a_re.captures_iter(html).enumerate() {
        let Some(hm) = href_re.captures(&cap[1]) else {
            continue;
        };
        let url = decode_ddg_url(&hm[1]);
        let title = clean_text(&cap[2]);
        if url.is_empty() || title.is_empty() {
            continue;
        }
        let snippet = snippets.get(i).cloned().unwrap_or_default();
        hits.push(WebHit {
            title,
            url,
            snippet,
        });
        if hits.len() >= count as usize {
            break;
        }
    }
    hits
}

/// Percent-encode a query for a URL query string (RFC 3986 unreserved passes through).
fn url_encode(q: &str) -> String {
    let mut out = String::with_capacity(q.len() * 3);
    for b in q.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Resolve a DDG result href to the real destination. DDG wraps organic links as
/// `//duckduckgo.com/l/?uddg=<percent-encoded-real-url>&rut=…`; unwrap + decode it.
/// A bare protocol-relative `//host/…` gets `https:` prepended; a direct URL passes through.
fn decode_ddg_url(href: &str) -> String {
    if let Some(idx) = href.find("uddg=") {
        let rest = &href[idx + "uddg=".len()..];
        let enc = rest.split('&').next().unwrap_or(rest);
        return percent_decode(enc);
    }
    if let Some(stripped) = href.strip_prefix("//") {
        return format!("https://{stripped}");
    }
    href.to_string()
}

/// Minimal percent-decoder (`%XX` → byte, `+` → space) — enough for a uddg URL.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                if let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                    out.push(b);
                    i += 3;
                    continue;
                }
                out.push(bytes[i]);
                i += 1;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Strip HTML tags, decode the handful of entities DDG emits, collapse whitespace.
pub(super) fn clean_text(html: &str) -> String {
    let mut s = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => s.push(c),
            _ => {}
        }
    }
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#x27;", "'")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    // A trimmed sample of the real html.duckduckgo.com results markup.
    const SAMPLE: &str = r##"
      <div class="result results_links">
        <a rel="nofollow" class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fdocs.rs%2Fserde_json&amp;rut=abc">serde_json - Rust</a>
        <a class="result__snippet" href="//duckduckgo.com/l/?uddg=x">Serde JSON provides efficient <b>parsing</b> of JSON.</a>
      </div>
      <div class="result results_links">
        <a rel="nofollow" class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fx">Example &amp; Guide</a>
        <a class="result__snippet" href="#">A worked example of the API.</a>
      </div>"##;

    // what this catches: the real DDG html results table projects into ranked hits with
    // the redirect UNWRAPPED to the true URL, tags/entities stripped from title+snippet,
    // link↔snippet zipped by rank. Pinned offline so a DDG markup change is caught here,
    // not as silent zero results (the failure mode that made the web hand look dead).
    #[test]
    fn parses_ddg_html_and_unwraps_redirect() {
        let hits = parse_ddg_html(SAMPLE, 10);
        assert_eq!(hits.len(), 2, "two organic results");
        assert_eq!(hits[0].title, "serde_json - Rust");
        assert_eq!(
            hits[0].url, "https://docs.rs/serde_json",
            "uddg redirect decoded"
        );
        assert_eq!(
            hits[0].snippet,
            "Serde JSON provides efficient parsing of JSON."
        );
        assert_eq!(
            hits[1].title, "Example & Guide",
            "entities decoded in title"
        );
        assert_eq!(hits[1].url, "https://example.com/x");
    }

    // what this catches: `count` caps the projected hits.
    #[test]
    fn respects_count_cap() {
        assert_eq!(parse_ddg_html(SAMPLE, 1).len(), 1);
    }

    // what this catches: the anomaly/bot-block page (no result markup) yields an honest
    // empty list from the parser, not a spurious hit.
    #[test]
    fn bot_block_page_parses_empty() {
        assert!(parse_ddg_html("<html><body>anomaly</body></html>", 10).is_empty());
    }

    // what this catches: percent-decode of a uddg-wrapped URL + query url-encoding.
    #[test]
    fn url_helpers() {
        assert_eq!(
            decode_ddg_url(
                "//duckduckgo.com/l/?uddg=https%3A%2F%2Fdoc.rust-lang.org%2Fbook%2F&rut=z"
            ),
            "https://doc.rust-lang.org/book/"
        );
        assert_eq!(decode_ddg_url("//example.org/a"), "https://example.org/a");
        assert_eq!(url_encode("rust serde_json"), "rust%20serde_json");
    }
}
