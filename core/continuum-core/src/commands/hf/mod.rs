//! `hf/*` — the persona's HuggingFace Hub retrieval hands.
//!
//! ## Why HF search is the reliable foraging lane
//!
//! General web search is hostile to write well — search engines and content
//! sites fight bots, and the keyless APIs (DuckDuckGo HTML, `ddgs`) are poor and
//! brittle. The HuggingFace Hub, by contrast, exposes a clean public JSON search
//! (`/api/models`, `/api/datasets`) with no anti-bot wall and no key required for
//! public repos. So the FIRST foraging hand a persona gets is HF: find a base
//! model to forge from, find a training dataset to learn from. This is the
//! "search the web, not just your mind, to get smarter" capability for the
//! training/academy side — teacher and students discover content and datasets
//! here.
//!
//! ## How it reaches the persona
//!
//! Both commands are stateless `AiSafe` [`ActionCommand`](crate::sdk_codegen::ActionCommand)s.
//! By construction they auto-register onto the one command registry, so they
//! appear on the persona tool surface (registry × ACL) and route through the
//! acting-organism `Decision::Act` path with ZERO extra wiring — the result of a
//! search re-enters the mind as an Episodic memory next tick (see
//! `docs/cognition/ACTING-ORGANISM.md`). A `models/pull` of a discovered id is the
//! natural follow-on hand.

use std::time::Duration;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::sdk_codegen::CommandError;

pub mod search_datasets;
pub mod search_models;

/// Which kind of Hub repo a search targets. Selects the API endpoint and the
/// browsable web-url shape — the only two things that differ between a model
/// search and a dataset search (one search engine, two faces).
#[derive(Debug, Clone, Copy)]
pub enum HubKind {
    Models,
    Datasets,
}

impl HubKind {
    fn api_endpoint(self) -> &'static str {
        match self {
            HubKind::Models => "https://huggingface.co/api/models",
            HubKind::Datasets => "https://huggingface.co/api/datasets",
        }
    }

    /// The browsable page for a repo id — what a persona cites or opens next, and
    /// (for a model) the id it hands to `models/pull`.
    fn web_url(self, id: &str) -> String {
        match self {
            HubKind::Models => format!("https://huggingface.co/{id}"),
            HubKind::Datasets => format!("https://huggingface.co/datasets/{id}"),
        }
    }

    fn label(self) -> &'static str {
        match self {
            HubKind::Models => "model",
            HubKind::Datasets => "dataset",
        }
    }
}

/// Shared params for both `hf/search-models` and `hf/search-datasets` — one search
/// contract, two faces (compression: the query shape is identical).
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/hf/HfSearchParams.ts"
)]
pub struct HfSearchParams {
    /// Full-text query, matched against repo id, author, tags and description —
    /// e.g. "qwen2.5 coder gguf", "medical dialogue", "function calling dataset".
    pub query: String,
    /// Max results. Default 10, clamped to [1, 50] so the result stays small
    /// enough to re-enter cognition as a memory.
    #[serde(default)]
    #[ts(optional)]
    pub limit: Option<u32>,
    /// Sort key: "downloads", "likes", "trending", "modified", "created". Omit to
    /// let the Hub rank by search relevance.
    #[serde(default)]
    #[ts(optional)]
    pub sort: Option<String>,
    /// Optional Hub filter tag to narrow results — e.g. "text-generation", "gguf",
    /// "en", "conversational". Omit for an unfiltered search.
    #[serde(default)]
    #[ts(optional)]
    pub filter: Option<String>,
}

/// The result of a Hub search: the query echoed, what kind it searched, and the
/// ranked hits.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/hf/HfSearchResult.ts"
)]
pub struct HfSearchResult {
    pub query: String,
    /// "model" or "dataset".
    pub kind: String,
    /// How many hits are in `hits` (after the limit clamp).
    pub count: u32,
    pub hits: Vec<HfHit>,
}

/// One Hub repo hit, trimmed to what a foraging mind needs to choose: the id
/// (the handle to pull/import), the page url, popularity signals, task, and tags.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/hf/HfHit.ts")]
pub struct HfHit {
    /// The repo id, "<owner>/<name>" — the handle for `models/pull` or dataset import.
    pub id: String,
    /// The browsable Hub page for this repo.
    pub url: String,
    #[ts(type = "number")]
    pub downloads: u64,
    #[ts(type = "number")]
    pub likes: u64,
    /// For a model, the pipeline/task tag (e.g. "text-generation"); None for a dataset.
    #[serde(default)]
    #[ts(optional)]
    pub task: Option<String>,
    /// A few descriptive tags (trimmed) — language, task, and license hints.
    pub tags: Vec<String>,
}

/// HF Hub's per-repo JSON shape. Every field but `id` is optional on the wire —
/// the Hub omits zeros/absent values — so each carries `#[serde(default)]`. This
/// is tolerating a genuinely-optional upstream field, NOT a fallback hiding a
/// missing precondition: a repo legitimately can have 0 downloads or no task.
#[derive(Debug, Deserialize)]
struct HfRepoRaw {
    id: String,
    #[serde(default)]
    downloads: u64,
    #[serde(default)]
    likes: u64,
    #[serde(default)]
    pipeline_tag: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
}

const MAX_TAGS: usize = 8;
const MAX_LIMIT: u32 = 50;
const DEFAULT_LIMIT: u32 = 10;

/// Project a raw Hub repo onto the trimmed [`HfHit`] a persona reads. Pure (no
/// network) so the field mapping + url shaping is unit-testable offline.
fn to_hit(raw: HfRepoRaw, kind: HubKind) -> HfHit {
    let url = kind.web_url(&raw.id);
    let tags = raw.tags.into_iter().take(MAX_TAGS).collect();
    HfHit {
        url,
        id: raw.id,
        downloads: raw.downloads,
        likes: raw.likes,
        task: raw.pipeline_tag,
        tags,
    }
}

/// Run a HuggingFace Hub search and project the hits. Shared by both commands;
/// `kind` selects the endpoint and url shape. Fails loud (no fallback) on a
/// transport error, a non-2xx status, or an unparseable body — naming the cause.
pub async fn search_hub(kind: HubKind, p: HfSearchParams) -> Result<HfSearchResult, CommandError> {
    let limit = p.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);

    let client = reqwest::Client::builder()
        .user_agent(concat!("continuum/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| CommandError::Internal(format!("HTTP client init failed: {e}")))?;

    let mut params: Vec<(&str, String)> =
        vec![("search", p.query.clone()), ("limit", limit.to_string())];
    if let Some(sort) = p.sort.as_ref().filter(|s| !s.is_empty()) {
        params.push(("sort", sort.clone()));
        params.push(("direction", "-1".to_string()));
    }
    if let Some(filter) = p.filter.as_ref().filter(|s| !s.is_empty()) {
        params.push(("filter", filter.clone()));
    }

    let mut req = client.get(kind.api_endpoint()).query(&params);
    // HF_TOKEN lifts the anonymous rate limit and reaches gated repos. Optional —
    // public search needs none; this is an opportunistic header, not a precondition.
    if let Some(token) = std::env::var("HF_TOKEN").ok().filter(|t| !t.is_empty()) {
        req = req.bearer_auth(token);
    }

    let resp = req.send().await.map_err(|e| {
        CommandError::Internal(format!(
            "HuggingFace Hub {} search request failed: {e}",
            kind.label()
        ))
    })?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp
            .text()
            .await
            .unwrap_or_else(|_| "<error body unreadable>".to_string());
        return Err(CommandError::Internal(format!(
            "HuggingFace Hub returned {status} for {} search '{}': {}",
            kind.label(),
            p.query,
            body.chars().take(200).collect::<String>()
        )));
    }

    let raw: Vec<HfRepoRaw> = resp.json().await.map_err(|e| {
        CommandError::Internal(format!("could not parse HuggingFace Hub response: {e}"))
    })?;

    let hits: Vec<HfHit> = raw.into_iter().map(|r| to_hit(r, kind)).collect();
    Ok(HfSearchResult {
        query: p.query,
        kind: kind.label().to_string(),
        count: hits.len() as u32,
        hits,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the two Hub kinds produce the correct public web-url shape
    // (models at /<id>, datasets at /datasets/<id>) and the matching api endpoint —
    // the handle a persona follows to pull a model or import a dataset. A swap here
    // sends a foraging persona to a 404.
    #[test]
    fn hub_kind_urls_and_endpoints() {
        assert_eq!(
            HubKind::Models.web_url("Qwen/Qwen2.5-Coder-7B"),
            "https://huggingface.co/Qwen/Qwen2.5-Coder-7B"
        );
        assert_eq!(
            HubKind::Datasets.web_url("openai/gsm8k"),
            "https://huggingface.co/datasets/openai/gsm8k"
        );
        assert!(HubKind::Models.api_endpoint().ends_with("/api/models"));
        assert!(HubKind::Datasets.api_endpoint().ends_with("/api/datasets"));
    }

    // what this catches: the HF JSON shape (id/downloads/likes/pipeline_tag/tags,
    // any of which the Hub may omit) deserializes leniently and maps to an HfHit
    // with the model task carried and tags trimmed to MAX_TAGS — the offline core
    // of search_hub, proven without a network call.
    #[test]
    fn maps_hf_json_to_hits() {
        let json = r#"[
            {"id":"Qwen/Qwen2.5-Coder-7B-Instruct","downloads":123456,"likes":789,"pipeline_tag":"text-generation","tags":["a","b","c","d","e","f","g","h","i","j"]},
            {"id":"some/sparse-repo"}
        ]"#;
        let raw: Vec<HfRepoRaw> = serde_json::from_str(json).expect("parse HF json");
        let hits: Vec<HfHit> = raw
            .into_iter()
            .map(|r| to_hit(r, HubKind::Models))
            .collect();

        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].id, "Qwen/Qwen2.5-Coder-7B-Instruct");
        assert_eq!(
            hits[0].url,
            "https://huggingface.co/Qwen/Qwen2.5-Coder-7B-Instruct"
        );
        assert_eq!(hits[0].downloads, 123_456);
        assert_eq!(hits[0].likes, 789);
        assert_eq!(hits[0].task.as_deref(), Some("text-generation"));
        assert_eq!(hits[0].tags.len(), MAX_TAGS, "tags trimmed to the cap");

        // A sparse repo (HF omitted the optional fields) still maps — zeros, no task.
        assert_eq!(hits[1].downloads, 0);
        assert_eq!(hits[1].likes, 0);
        assert!(hits[1].task.is_none());
        assert!(hits[1].tags.is_empty());
    }
}
