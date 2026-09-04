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
pub(crate) mod browser;
pub mod duckduckgo;
pub mod fetch;
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
#[ts(
    export,
    export_to = "../../../protocol/typescript/web/WebSearchParams.ts"
)]
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
#[ts(
    export,
    export_to = "../../../protocol/typescript/web/WebSearchResult.ts"
)]
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
    // ORDER IS THE AUTO-PRIORITY (2026-08-25, Joel: 'utilize our own browser as a
    // possibility, priority'). The browser-driven keyless provider is FIRST, so
    // auto-selection prefers it: it drives our REAL Chromium (JS-rendered, a
    // believable UA, Cloudflare/bot-block-resistant) and returns exactly the
    // natural content a human sees — no meter, no pay-to-play, no key. The paid
    // Brave API is a deliberate OPT-IN (`adapter: "brave"`) for callers who want
    // its structured results and hold a key; it no longer silently wins auto just
    // because a key happens to exist. Same lesson as the operator running out of a
    // metered search budget: owned-browser natural content beats rented API access.
    vec![
        Box::new(duckduckgo::DuckDuckGoProvider),
        Box::new(brave::BraveSearchProvider),
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

/// Params for `web/fetch`.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/web/WebFetchParams.ts"
)]
pub struct WebFetchParams {
    /// The URL to fetch and read (http/https).
    pub url: String,
    /// FILTER MODE: a regex; return ONLY the readable lines that match it (+ `context_lines`
    /// around each), like `grep` on the page. The context-saver — don't spend working memory
    /// on a 50KB dump when you want the three lines mentioning an error or an API name.
    /// Omitted → full readable page. If the filter matches nothing you are TOLD so and get
    /// the unfiltered page back (never a silent empty result). An invalid regex fails loud.
    #[serde(default)]
    #[ts(optional)]
    pub filter: Option<String>,
    /// With `filter`, how many lines of CONTEXT to keep around each match (like
    /// `grep -C`). Default 0 (matching lines only). Bounded so a huge context can't
    /// defeat the point of filtering.
    #[serde(default)]
    #[ts(optional)]
    #[ts(optional, type = "number")]
    pub context_lines: Option<u32>,
}

/// Result of a `web/fetch`.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/web/WebFetchResult.ts"
)]
pub struct WebFetchResult {
    pub url: String,
    /// The page `<title>`, if any.
    pub title: String,
    /// Readable page text with real lines (`<pre>`/code kept verbatim). The FULL page (or,
    /// with `filter`, the matching lines) — web/fetch does not truncate; if this is large the
    /// executor bounds what you see inline and spills the rest, pageable via `tool/output`.
    pub content: String,
    /// Total readable characters of the WHOLE page (with `filter`, `content` may be a smaller
    /// matching slice — this still reports the full page size, honest scale).
    #[ts(type = "number")]
    pub chars: u32,
}

/// Grep the readable text: keep lines matching `re` plus `ctx` lines of context around
/// each (like `grep -C`), de-duplicating overlapping windows so context never doubles a
/// line. Empty result when nothing matches (an honest "the page does not mention this").
fn grep_lines(text: &str, re: &regex::Regex, ctx: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut keep = vec![false; lines.len()];
    for (i, l) in lines.iter().enumerate() {
        if re.is_match(l) {
            let lo = i.saturating_sub(ctx);
            let hi = (i + ctx + 1).min(lines.len());
            for k in keep.iter_mut().take(hi).skip(lo) {
                *k = true;
            }
        }
    }
    lines
        .iter()
        .zip(keep)
        .filter_map(|(l, k)| k.then_some(*l))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Fetch a URL and return its readable text — the persona's "read the doc/page I
/// found" hand, the natural partner to `web/search`. GETs via the host's real browser,
/// drops script/style blocks, keeps `<pre>`/code verbatim, and returns line-oriented
/// readable text. http/https only; a non-2xx status FAILS LOUD (never returns an error
/// page as if it were content).
///
/// **web/fetch does NOT truncate.** It returns the full readable page; the executor
/// result-fold is the ONE truncation+spill authority (the same one `code/run`/`code/read`
/// use), so a large page is bounded inline and the remainder is pageable via `tool/output`
/// — instead of being silently discarded here. And a `filter` that matches nothing is
/// never silent: it says so and returns the unfiltered page so she can adjust.
pub async fn web_fetch(p: WebFetchParams) -> Result<WebFetchResult, CommandError> {
    let url = p.url.trim().to_string();
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(CommandError::Invalid(format!(
            "web/fetch url must start with http:// or https://, got '{url}'"
        )));
    }

    // STATUS PRE-CHECK: the browser's `--dump-dom` renders a page regardless of HTTP
    // status, so a 404 comes back as a 14-char "404: Not Found" DOM that reads as content
    // (this is exactly what beat astropy-13236 — her v5.1.0 URL 404'd and she saw a silent
    // near-empty result, never learning the tag was wrong). A cheap headers-only GET
    // surfaces the status so a definitively-wrong URL FAILS LOUD with the code, instead of
    // blinding her with a rendered error page.
    if let Ok(client) = reqwest::Client::builder()
        .user_agent(browser::RENDER_UA)
        .timeout(std::time::Duration::from_secs(10))
        .build()
    {
        if let Ok(resp) = client.get(&url).send().await {
            if let Some(e) = url_error_for_status(
                resp.status().as_u16(),
                resp.status().canonical_reason().unwrap_or(""), // some statuses have no canonical reason; empty is the honest display
                &url,
            ) {
                return Err(e);
            }
        }
        // A probe network error / bot-block / 5xx is NOT fatal here: fall through to the
        // real browser, which renders JS and gets past anti-bot walls a bare client can't.
    }

    // Drive the host's REAL browser (renders JS, isn't bot-blocked) rather than an HTTP
    // scrape. 4s virtual-time budget is ample for a doc/article's first paint.
    let body = browser::render_dom(&url, 4000).await?;

    let title = extract_title(&body);
    let readable = extract_readable(&body);
    // Honest scale of the WHOLE page, independent of any filter — so `chars` always
    // reports how much is really there even when `content` is a filtered slice.
    let chars = readable.chars().count() as u32;
    let content = filtered_or_fallback(readable, p.filter.as_deref(), p.context_lines.unwrap_or(0))?; // 0 context lines = exact-match only; documented default of the param

    Ok(WebFetchResult {
        url,
        title,
        content,
        chars,
    })
}

/// Decide whether an HTTP status means "fail loud — this URL is wrong" or "proceed and
/// let the browser try". A DEFINITIVE client error (404/400/410/…) means the resource is
/// not there, so we fail loud with the code and a nudge to fix the URL — instead of
/// letting a rendered error page masquerade as content. `403`/`429`/`408` are bot-block /
/// rate-limit / timeout signals a real browser can often get past, and `5xx` is transient,
/// so those fall through to `render_dom`. Pure → unit-testable.
fn url_error_for_status(code: u16, reason: &str, url: &str) -> Option<CommandError> {
    let definitive_client_error =
        (400..500).contains(&code) && !matches!(code, 403 | 408 | 429);
    definitive_client_error.then(|| {
        CommandError::Invalid(format!(
            "web/fetch: {url} returned HTTP {code} {reason} — the URL is wrong. Check the \
             path, the ref/tag/branch (e.g. `v5.1` not `v5.1.0`), and spelling; or use \
             web/search to find the right link."
        ))
    })
}

/// Apply a persona's `filter` regex to the readable page, or pass it through whole.
///
/// **Never silently empty.** A zero-match filter used to return `chars: 0` with no
/// reason — which blinds her: she cannot tell "no match" from "404" from "broken tool"
/// (this is exactly what cost the astropy-13236 solve — five gold-patch fetches all
/// came back `chars: 0`). On a miss we SAY so and hand back the UNFILTERED page: the
/// executor result-fold then bounds the inline view and spills the rest with a
/// `tool/output` handle, so it stays cheap and pageable — collapse, never clip to nothing.
/// Pure over its inputs → unit-testable without the network.
fn filtered_or_fallback(
    readable: String,
    filter: Option<&str>,
    context_lines: u32,
) -> Result<String, CommandError> {
    let Some(pat) = filter.filter(|s| !s.trim().is_empty()) else {
        return Ok(readable);
    };
    let re = regex::Regex::new(pat).map_err(|e| {
        CommandError::Invalid(format!("web/fetch filter is not a valid regex: {e}"))
    })?;
    let ctx = context_lines.min(10) as usize;
    let filtered = grep_lines(&readable, &re, ctx);
    if filtered.trim().is_empty() {
        let total_lines = readable.lines().count();
        Ok(format!(
            "[web/fetch: filter /{pat}/ matched 0 of {total_lines} lines — showing the \
             unfiltered page so you can adjust the pattern or read it directly; page it \
             with tool/output if it is large]\n\n{readable}"
        ))
    } else {
        Ok(filtered)
    }
}

/// Pull the `<title>` text out of an HTML document (empty if none).
fn extract_title(html: &str) -> String {
    let lower = html.to_lowercase();
    let (Some(open), Some(close)) = (lower.find("<title"), lower.find("</title>")) else {
        return String::new();
    };
    let Some(gt) = html[open..].find('>') else {
        return String::new();
    };
    let start = open + gt + 1;
    if start >= close {
        return String::new();
    }
    strip_tags(&html[start..close])
}

use std::sync::LazyLock;

/// Every HTML tag span (`<…>`). Module-level so both `strip_tags` and the readable
/// extractor share ONE definition.
static TAG_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"<[^>]+>").expect("static tag regex is valid")); // static regex literal, validated at first use forever

/// A `<pre>…</pre>` block — captured so its content is preserved VERBATIM (its
/// whitespace and newlines are significant: a raw source file renders as one big
/// `<pre>`, and a doc's code samples are `<pre>`/`<code>`). `(?is)` = case-insensitive
/// + `.` spans newlines; non-greedy so adjacent pre blocks don't merge.
static PRE_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"(?is)<pre\b[^>]*>(.*?)</pre>").expect("static pre regex is valid") // static regex literal, validated at first use forever
});

/// Block-level close tags + `<br>` — a rendered line break. Turned into `\n` BEFORE
/// tag-stripping so the readable text has REAL lines for `grep`/paging. (The old
/// `split_whitespace().join(" ")` erased EVERY newline — including a raw source file's
/// own — which made a `.py` one giant line and `filter` all-or-nothing.)
static BLOCK_BREAK_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(
        r"(?i)</(p|div|li|tr|h[1-6]|blockquote|section|article|ul|ol|table|thead|tbody|header|footer|nav|main|aside|dd|dt|figure)>|<br\s*/?>",
    )
    .expect("static block-break regex is valid") // static regex literal, validated at first use forever
});

/// Decode the handful of HTML entities the pipeline emits. Shared so tag-strip and
/// readable-extraction never drift on which entities they know.
fn decode_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#x27;", "'")
        .replace("&#39;", "'")
}

/// Non-`<pre>` HTML → readable prose with REAL lines: block tags become newlines,
/// inline tags are stripped, horizontal whitespace within a line collapses, blank
/// lines drop. Preserving line breaks (not erasing them) is what makes `filter`/`grep`
/// work line-by-line instead of all-or-nothing on one giant line.
fn readable_prose(html: &str) -> String {
    let with_breaks = BLOCK_BREAK_RE.replace_all(html, "\n");
    let no_tags = TAG_RE.replace_all(&with_breaks, "");
    decode_entities(&no_tags)
        .lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Turn an HTML page into readable text with real lines. `<script>/<style>/…` block
/// TEXT is dropped (it would leak as garbage); `<pre>`/`<code>` blocks are kept
/// VERBATIM (their indentation and newlines carry meaning — this is how a fetched raw
/// `table.py` stays valid Python instead of one collapsed line); everything else
/// becomes line-oriented prose. Line structure is preserved throughout so a persona
/// can `filter`/`grep` a page the way she greps a file.
fn extract_readable(html: &str) -> String {
    let cleaned = strip_blocks(html, &["script", "style", "noscript", "svg", "head"]);
    let mut out: Vec<String> = Vec::new();
    let mut last = 0usize;
    for cap in PRE_RE.captures_iter(&cleaned) {
        let whole = cap.get(0).expect("regex match always has group 0"); // group 0 is the whole match, always present by regex contract
        let prose = readable_prose(&cleaned[last..whole.start()]);
        if !prose.is_empty() {
            out.push(prose);
        }
        // <pre> content verbatim: strip only inner inline tags + decode entities;
        // NEVER collapse its whitespace/newlines.
        let inner = cap.get(1).map_or("", |g| g.as_str());
        let code = decode_entities(&TAG_RE.replace_all(inner, ""));
        let code = code.trim_matches('\n');
        if !code.is_empty() {
            out.push(code.to_string());
        }
        last = whole.end();
    }
    let tail = readable_prose(&cleaned[last..]);
    if !tail.is_empty() {
        out.push(tail);
    }
    out.join("\n")
}

/// Remove `<tag …>…</tag>` spans (case-insensitive) for each named tag, replacing
/// each with a space. An unclosed tag drops to end-of-document.
fn strip_blocks(html: &str, tags: &[&str]) -> String {
    let mut s = html.to_string();
    for tag in tags {
        let open = format!("<{tag}");
        let close = format!("</{tag}>");
        loop {
            let lower = s.to_lowercase();
            let Some(start) = lower.find(&open) else {
                break;
            };
            let end = match lower[start..].find(&close) {
                Some(rel) => start + rel + close.len(),
                None => s.len(),
            };
            s.replace_range(start..end, " ");
        }
    }
    s
}

/// Strip HTML tags from a snippet/title fragment (engines return `<strong>`
/// highlight markup). Shared by both providers. Pure → unit-testable.
pub(crate) fn strip_tags(s: &str) -> String {
    let no_tags = TAG_RE.replace_all(s, "");
    decode_entities(&no_tags).trim().to_string()
}

#[cfg(test)]
mod tests {
    // what this catches: FILTER mode — web/fetch grep returns only matching lines
    // (+context), so a persona spends working memory on the 3 relevant lines, not a
    // 50KB dump. The context-efficiency win Joel named ("regex or other filters").
    #[test]
    fn fetch_filter_greps_readable_lines_with_context() {
        use super::grep_lines;
        let text = "intro line\nTimeDelta is the class\nunrelated middle\nsee TimeDelta.sec\ntail";
        let re = regex::Regex::new("TimeDelta").unwrap();
        // ctx=0: only matching lines.
        let out = grep_lines(text, &re, 0);
        assert!(out.contains("TimeDelta is the class") && out.contains("see TimeDelta.sec"));
        assert!(!out.contains("unrelated middle"), "non-matching line dropped: {out}");
        assert!(!out.contains("intro line"));
        // ctx=1: one line of context around each match, de-duped (no doubled lines).
        let ctx = grep_lines(text, &re, 1);
        assert!(ctx.contains("intro line") && ctx.contains("unrelated middle"));
        assert_eq!(ctx.matches("TimeDelta is the class").count(), 1, "overlap must not double a line");
        // no match → honest empty.
        assert_eq!(grep_lines(text, &regex::Regex::new("nonexistent").unwrap(), 0), "");
    }

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
        assert!(
            msg.contains("BRAVE_API_KEY"),
            "names the missing key: {msg}"
        );
        assert!(
            msg.contains("duckduckgo"),
            "points at the keyless fallback: {msg}"
        );
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

    // what this catches: web/fetch turns a real HTML page into readable prose — the
    // <title> is pulled out, and script/style TEXT (which naive tag-stripping would leak
    // as garbage) is dropped, leaving only the body copy with collapsed whitespace. This is
    // the "actually read the doc" hand; if it leaked script source the persona would reason
    // over noise.
    #[test]
    fn fetch_extracts_title_and_readable_body_dropping_scripts() {
        let html = "<html><head><title>Serde JSON</title><style>body{color:red}</style></head>\
                    <body><script>var x = 'noise';</script><h1>Parsing</h1>\
                    <p>Use   serde_json::from_str  to parse.</p></body></html>";
        assert_eq!(extract_title(html), "Serde JSON");
        let body = extract_readable(html);
        assert!(body.contains("Parsing"), "keeps heading: {body}");
        assert!(
            body.contains("Use serde_json::from_str to parse."),
            "collapses ws: {body}"
        );
        assert!(!body.contains("noise"), "drops script text: {body}");
        assert!(!body.contains("color:red"), "drops style text: {body}");
    }

    // what this catches: regression for the astropy-13236 miss — a `<pre>` block (how a
    // raw source file renders) is preserved VERBATIM, indentation and newlines intact, so
    // fetched code is readable/greppable line-by-line instead of collapsed into one line.
    // The old `split_whitespace().join(" ")` erased every newline, which made `filter`
    // all-or-nothing and returned the gold table.py as garbage.
    #[test]
    fn fetch_preserves_pre_blocks_verbatim_with_real_lines() {
        let html = "<html><body><p>Intro   prose here.</p>\
                    <pre>def f():\n    if len(data.dtype) &gt; 1:\n        data = data.view(NdarrayMixin)\n</pre>\
                    </body></html>";
        let body = extract_readable(html);
        // prose collapses horizontal ws but keeps its own line
        assert!(body.contains("Intro prose here."), "prose collapsed on one line: {body:?}");
        // <pre> keeps indentation AND newlines verbatim (entities decoded)
        assert!(
            body.contains("    if len(data.dtype) > 1:"),
            "pre indentation preserved: {body:?}"
        );
        assert!(
            body.contains("        data = data.view(NdarrayMixin)"),
            "pre deep indentation preserved: {body:?}"
        );
        // real lines exist — grep can target one line, not the whole page
        let hit = grep_lines(&body, &regex::Regex::new("NdarrayMixin").unwrap(), 0);
        assert_eq!(hit, "        data = data.view(NdarrayMixin)", "grep isolates ONE line: {hit:?}");
    }

    // what this catches: a zero-match filter is NEVER silently empty (the exact blindness
    // that cost astropy-13236 — five gold fetches all returned chars:0). It must announce
    // the miss AND return the unfiltered page so she can adjust the pattern / read directly.
    #[test]
    fn filter_miss_is_never_silent_and_returns_the_page() {
        use super::filtered_or_fallback;
        let page = "line one\nline two\nline three".to_string();
        let out = filtered_or_fallback(page.clone(), Some("NOPE_NO_MATCH"), 0).unwrap();
        assert!(!out.trim().is_empty(), "must never be silently empty");
        assert!(out.contains("matched 0 of 3 lines"), "announces the miss + scale: {out}");
        assert!(out.contains("line one") && out.contains("line three"), "returns the page: {out}");
    }

    // what this catches: regression for astropy-13236 — a 404 (her v5.1.0 URL) FAILS LOUD
    // with the code and a fix-the-URL nudge, instead of the browser rendering "404: Not
    // Found" as content. Bot-block/rate-limit/transient statuses (403/408/429/5xx) do NOT
    // fail here — a real browser can get past them — and 2xx never fails.
    #[test]
    fn url_error_for_status_fails_loud_only_on_definitive_client_errors() {
        use super::url_error_for_status;
        // definitive client errors → fail loud, name the code
        for code in [400u16, 404, 410, 451] {
            let e = url_error_for_status(code, "Not Found", "https://x/y")
                .unwrap_or_else(|| panic!("HTTP {code} must fail loud"));
            assert!(format!("{e:?}").contains(&code.to_string()), "names the code {code}: {e:?}");
        }
        // bot-block / rate-limit / timeout / transient → proceed (browser may get past)
        for code in [200u16, 301, 403, 408, 429, 500, 503] {
            assert!(url_error_for_status(code, "x", "https://x/y").is_none(), "HTTP {code} must proceed");
        }
    }

    // what this catches: a matching filter returns ONLY the matching lines (the context
    // saver), and no filter returns the whole page — the two non-miss branches.
    #[test]
    fn filter_hit_returns_only_matches_and_none_returns_whole() {
        use super::filtered_or_fallback;
        let page = "alpha\nbeta target\ngamma".to_string();
        let hit = filtered_or_fallback(page.clone(), Some("target"), 0).unwrap();
        assert_eq!(hit, "beta target", "only the matching line: {hit}");
        let whole = filtered_or_fallback(page.clone(), None, 0).unwrap();
        assert_eq!(whole, page, "no filter → whole page unchanged");
    }
}
