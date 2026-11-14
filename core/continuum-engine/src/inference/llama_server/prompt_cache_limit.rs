//! The engine's host prompt cache, resized live (card 494c99f1).
//!
//! `--cache-ram` sized the cache once at launch and it never gave memory back: on the M5
//! (64 GB, 2026-11-13) a 9,369 MiB cache held its states while the box sat at 60 MB free,
//! 34 GB compressed and 8.6 of 10 GB swap, and prefill fell from ~190 to 92 tok/s. Our fork's
//! `POST /prompt-cache/limit` (fork #110, 9269444f6) moves that limit on the running engine: the
//! cache evicts its oldest states to fit and answers what it holds and what it freed. This file
//! is the one place that speaks that route; serving's reclaim calls it through
//! [`LlamaServerControl::set_prompt_cache_limit`](super::LlamaServerControl::set_prompt_cache_limit).

use serde::Deserialize;

use super::{LlamaServerError, PROBE_TIMEOUT};

/// What the engine's host prompt cache holds after a limit was applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PromptCacheLimit {
    /// Bytes the cache holds now, within the new limit.
    pub held_bytes: u64,
    /// Bytes the cache evicted to fit the new limit (0 when raised, or when it already fit).
    pub freed_bytes: u64,
}

/// Why the engine did not apply a prompt-cache limit. Callers branch on the variant; the
/// text is display only.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PromptCacheRefusal {
    /// The engine runs without a host prompt cache: there is nothing to shrink.
    #[error("the engine has no host prompt cache")]
    Disabled,
    /// The engine predates the route (older than fork 9269444f6): it answered 404/405.
    #[error("the engine has no /prompt-cache/limit route (older than fork 9269444f6)")]
    RouteMissing,
    /// The engine named a refusal this core does not know; its word is carried verbatim.
    #[error("the engine refused the prompt-cache limit: {0}")]
    Refused(String),
}

/// The route's `refusal` field: `"none"` on success, a named refusal otherwise.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum WireRefusal {
    None,
    PromptCacheDisabled,
    #[serde(untagged)]
    Other(String),
}

/// The route's task result. The receipt fields are present only when `refusal` is `none`.
#[derive(Debug, Deserialize)]
struct LimitReply {
    refusal: WireRefusal,
    held_bytes: Option<u64>,
    freed_bytes: Option<u64>,
}

impl LimitReply {
    fn into_result(self) -> Result<PromptCacheLimit, LlamaServerError> {
        match (self.refusal, self.held_bytes, self.freed_bytes) {
            (WireRefusal::None, Some(held_bytes), Some(freed_bytes)) => Ok(PromptCacheLimit { held_bytes, freed_bytes }),
            (WireRefusal::None, _, _) => Err(LlamaServerError::Unreachable(
                "/prompt-cache/limit answered refusal none without held_bytes and freed_bytes".into(),
            )),
            (WireRefusal::PromptCacheDisabled, _, _) => Err(LlamaServerError::PromptCacheRefused(PromptCacheRefusal::Disabled)),
            (WireRefusal::Other(word), _, _) => Err(LlamaServerError::PromptCacheRefused(PromptCacheRefusal::Refused(word))),
        }
    }
}

/// `POST {root}/prompt-cache/limit {"limit_mib": N}`. The cache evicts its oldest states to fit;
/// the reply is decoded straight into [`LimitReply`]. A transport failure or an engine error
/// status is `Unreachable`, like every other control-plane call on this endpoint.
pub(super) async fn post_prompt_cache_limit(
    client: &reqwest::Client,
    root: &str,
    limit_mib: std::num::NonZeroU64,
) -> Result<PromptCacheLimit, LlamaServerError> {
    let url = format!("{root}/prompt-cache/limit");
    let resp = client
        .post(&url)
        .json(&serde_json::json!({ "limit_mib": limit_mib.get() }))
        .timeout(PROBE_TIMEOUT)
        .send()
        .await
        .map_err(|e| LlamaServerError::Unreachable(e.to_string()))?;
    match resp.status().as_u16() {
        200..=299 => resp
            .json::<LimitReply>()
            .await
            .map_err(|e| LlamaServerError::Unreachable(format!("/prompt-cache/limit reply: {e}")))?
            .into_result(),
        404 | 405 => Err(LlamaServerError::PromptCacheRefused(PromptCacheRefusal::RouteMissing)),
        status => {
            let text = match resp.text().await {
                Ok(text) => text,
                Err(e) => format!("(body unreadable: {e})"),
            };
            Err(LlamaServerError::Unreachable(format!("/prompt-cache/limit status {status}: {text}")))
        }
    }
}
