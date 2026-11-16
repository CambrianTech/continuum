//! Resident minds on the engine (fork #139): a mind switch is a sequence move, never a serialization.
//!
//! The engine keeps every mind it serves as a sequence of its unified KV pool (parked when another mind holds
//! the lane) or, when the pool needs her cells, as a page on flash that it maps back in. A turn names its mind
//! (`"page"`, [`super::slots::PageKey::engine_id`]) so the engine moves the right sequence; and while she waits
//! for a lane, her state is streamed in ahead of her turn, the way a game engine streams what the camera needs
//! next, so her switch is the sub-millisecond sequence move.

use super::slots::PageKey;

/// Ask the engine at `root` to bring `key`'s mind into its pool now (`POST /minds/prefetch`). Fire and forget:
/// it never delays her turn. An engine that keeps no minds answers 404, which is not a failure.
pub(crate) fn prefetch(client: reqwest::Client, root: String, key: PageKey) {
    tokio::spawn(async move {
        let url = format!("{}/minds/prefetch", root.trim_end_matches('/'));
        let body = serde_json::json!({ "page": key.engine_id().simple().to_string() });
        let outcome = client.post(&url).json(&body).send().await;
        let (status, message) = match outcome {
            Ok(response) => {
                let status = response.status().as_u16();
                let text = response.text().await.unwrap_or_default(); // unwrap_or_default: an unreadable body still has its status
                (status, text)
            }
            Err(e) => (0, e.to_string()),
        };
        continuum_base::probe!(
            class = "inference.minds.prefetch",
            page = %key,
            status = u64::from(status),
            reply = message.as_str(),
            "her state streamed in ahead of her turn (404: the engine keeps no minds)"
        );
    });
}
