//! Resident minds on the engine (fork #139): a mind switch is a sequence move, never a serialization.
//!
//! An engine that declares `resident_minds` on `/props` keeps every mind it serves as a sequence of its unified
//! KV pool (parked when another mind holds the lane) or, when the pool needs her cells, as a page on flash it maps
//! back in. A turn names its mind (`"page"`, [`super::slots::PageKey::engine_id`]) so the engine moves the right
//! sequence; and while she waits for a lane, her state is streamed in ahead of her turn, the way a game engine
//! streams what the camera needs next, so her switch is the sub-millisecond sequence move.

use super::slots::PageKey;
use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

/// Streams in flight, one per (engine root, mind): a mind waiting on several turns streams once.
fn in_flight() -> &'static Mutex<HashSet<(String, uuid::Uuid)>> {
    static SET: OnceLock<Mutex<HashSet<(String, uuid::Uuid)>>> = OnceLock::new();
    SET.get_or_init(Mutex::default)
}

/// Ask the engine at `root` (process `engine_pid`, from the slot pool's incarnation) to bring `key`'s mind into
/// its pool now (`POST /minds/prefetch`). Only an engine that DECLARED resident minds is asked (never a 404
/// guess); the engine refuses a request meant for another process (`engine_pid`), so a stream never lands on a
/// replacement engine; one stream per (engine, mind) at a time. It never delays her turn.
pub(crate) fn prefetch(client: reqwest::Client, root: String, engine_pid: u32, key: PageKey) {
    if !super::slots::resident_minds_by_root().contains_key(&root) {
        return; // this engine pages minds through the core: nothing to stream
    }
    let id = key.engine_id();
    match in_flight().lock() {
        Ok(mut set) => {
            if !set.insert((root.clone(), id)) {
                return; // already streaming her in
            }
        }
        Err(_) => return, // a poisoned set: no stream, her turn is not delayed
    }
    tokio::spawn(async move {
        let url = format!("{}/minds/prefetch", root.trim_end_matches('/'));
        let body = serde_json::json!({ "page": id.simple().to_string(), "engine_pid": engine_pid });
        let (status, reply) = match client.post(&url).json(&body).send().await {
            Ok(response) => {
                let status = response.status().as_u16();
                (status, response.text().await.unwrap_or_default()) // unwrap_or_default: an unreadable body keeps its status
            }
            Err(e) => (0, e.to_string()),
        };
        if let Ok(mut set) = in_flight().lock() {
            set.remove(&(root.clone(), id));
        }
        continuum_base::probe!(
            class = "inference.minds.prefetch",
            page = %key,
            engine_pid = u64::from(engine_pid),
            status = u64::from(status),
            reply = reply.as_str(),
            "her state streamed in ahead of her turn (the engine's reply names where she is)"
        );
    });
}
