//! Resident minds on the engine (fork #139): a mind switch is a sequence move, never a serialization.
//!
//! An engine that declares `resident_minds` on `/props` keeps every mind it serves as a sequence of its unified
//! KV pool (parked when another mind holds the lane) or, when the pool needs her cells, as a page on flash it maps
//! back in. A turn names its mind (`"page"`, [`super::slots::PageKey::engine_id`]) so the engine moves the right
//! sequence; and while she waits for a lane her state streams in, the way a game engine streams what the camera
//! needs next, so her switch is the sub-millisecond sequence move.

use super::slots::PageKey;
use super::weight_residency::ResidentMinds;

/// Bring `key`'s mind into the pool of the engine at `root` (`POST /minds/prefetch`), carrying the incarnation
/// token that engine declared (`minds`), so a replaced engine refuses it. The turn waiting for a lane owns this
/// future: admitted, or cancelled, it is dropped with her wait; it never runs for a turn that did not wait.
pub(crate) async fn prefetch(client: &reqwest::Client, root: &str, minds: ResidentMinds, key: PageKey) {
    let url = format!("{}/minds/prefetch", root.trim_end_matches('/'));
    let body = serde_json::json!({ "page": key.engine_id().simple().to_string(), "incarnation": minds.incarnation });
    let outcome = match client.post(&url).json(&body).send().await {
        Ok(response) => response.status().as_u16().to_string(),
        Err(e) => e.to_string(),
    };
    continuum_base::probe!(
        class = "inference.minds.prefetch",
        page = %key,
        outcome = outcome.as_str(),
        "her state streamed in while she waited for a lane (the engine's reply names where she is)"
    );
}
