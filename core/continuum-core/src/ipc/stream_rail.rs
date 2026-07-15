//! Process-local ephemeral rail for live persona-turn tokens (#170).
//!
//! The token forwarder ([`crate::persona::service_loop::spawn_token_forwarder`])
//! tees each flushed chunk here; every connected WS client drains it and pushes a
//! [`WsServerMessage::StreamDelta`](continuum_airc_protocol::ws::WsServerMessage).
//! So a persona visibly *types* token-by-token in the browser instead of freezing
//! then dumping a wall of text.
//!
//! Deliberately a `tokio::broadcast` (fan-out, lossy-on-lag), NOT the positron
//! `Broadcast` watch (latest-wins coalescing): a token stream must never collapse
//! intermediate tokens into a merge. Each subscriber sees every token, or under
//! extreme lag cleanly skips some — cosmetic, because the durable `say()` transcript
//! row is the authoritative text; a dropped typing token never loses information.
//! Ephemeral: nothing persisted, no replay; a late subscriber joins mid-stream.

use std::sync::OnceLock;
use tokio::sync::broadcast;

/// One flushed token (or the `done` end-marker) from a persona's in-progress turn.
/// Correlated to the eventual durable row by `room_id` + `sender_id` (the per-turn
/// `stream_id` is minted at stream start and is NOT the final message id).
#[derive(Debug, Clone)]
pub struct StreamDelta {
    pub room_id: String,
    pub sender_id: String,
    pub stream_id: String,
    pub seq: u64,
    pub token: String,
    pub done: bool,
}

/// Buffered deltas per subscriber before the SLOWEST lags out. At ~4 flushes/sec per
/// persona (the forwarder's 250ms coalesce) across a room, 256 is several seconds of
/// slack; a client that lags past it skips tokens (cosmetic — `say()` is truth).
const RAIL_CAPACITY: usize = 256;

static STREAM_RAIL: OnceLock<broadcast::Sender<StreamDelta>> = OnceLock::new();

fn rail() -> &'static broadcast::Sender<StreamDelta> {
    STREAM_RAIL.get_or_init(|| broadcast::channel(RAIL_CAPACITY).0)
}

/// Tee one delta onto the rail. Never blocks, never errors upward: with no
/// subscribers (no WS clients connected) the send is a no-op drop — the forwarder
/// must not care whether a browser is watching.
pub fn publish(delta: StreamDelta) {
    let _ = rail().send(delta);
}

/// Subscribe a WS connection to the live token rail. Each connection gets its own
/// receiver; dropping it (on disconnect) unsubscribes.
pub fn subscribe() -> broadcast::Receiver<StreamDelta> {
    rail().subscribe()
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (#170): a delta teed onto the rail reaches a live subscriber
    // intact, and publishing with no subscribers is a silent no-op (the forwarder
    // never blocks/errors when no browser is watching).
    #[tokio::test]
    async fn delta_reaches_subscriber_and_noop_without_one() {
        // No subscriber yet → publish must not panic or block.
        publish(StreamDelta {
            room_id: "r".into(),
            sender_id: "s".into(),
            stream_id: "st".into(),
            seq: 0,
            token: "dropped".into(),
            done: false,
        });

        let mut rx = subscribe();
        publish(StreamDelta {
            room_id: "room-1".into(),
            sender_id: "asha".into(),
            stream_id: "st-1".into(),
            seq: 1,
            token: "Hi".into(),
            done: false,
        });
        let got = rx.recv().await.expect("subscriber receives the teed delta");
        assert_eq!(got.sender_id, "asha");
        assert_eq!(got.token, "Hi");
        assert!(!got.done);
    }
}
