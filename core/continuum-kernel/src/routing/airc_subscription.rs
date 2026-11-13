//! How this node subscribes to airc: every room, no router-side filter, filtering on receive.
//! One decision for every holder of an airc handle (the persona layer, the engine's remote
//! transport), so it lives with the airc transport in the kernel, below all of them.

/// THE one subscribe shape over a raw airc handle — a citizen's `subscribe_all_rooms`,
/// the remote-inference transport waiting for its answer's chunks, and the inbound pump
/// all call this one rather than re-deciding which subscribe shape to use.
///
/// The whole defect this fixes was two shapes existing and continuum
/// picking the narrowing one at every call site independently. Making
/// the choice ONCE is what stops it drifting back, so a future citizen
/// impl inherits the right answer instead of re-choosing.
pub async fn subscribe_every_room(
    airc: &airc_lib::Airc,
) -> Result<airc_lib::FilteredEventStream, airc_lib::AircError> {
    // NO router-side header filter (2026-09-04). The `Not(Has(heartbeat))`
    // filter this used to send (#445) arrived at the daemon INVERTED: every
    // citizen received ONLY the presence heartbeats and never a message —
    // measured in the daemon store (message rows lack the header, heartbeat
    // rows carry it) and in the inbound probes (100% of live inbound =
    // heartbeats, 0 messages) while an unfiltered `airc join` client on the
    // same daemon got every message. Citizens had heard rooms only through
    // the store page at turn time since the filter landed. Heartbeats are
    // dropped on RECEIVE instead (the persona layer's `is_heartbeat`) — a header check per
    // event, the same receive-side shape as the stream-chunk guard, correct
    // regardless of how the daemon treats a negated filter.
    // NO delivery filter either (2026-09-04, second cut): with `Some([Durable])`
    // citizens received `event`-kind frames live but never a `message` — while an
    // `airc join` client on the same daemon (no delivery filter) received every
    // message. Subscribe exactly as the CLI does; stream chunks and heartbeats
    // are dropped on receive (`is_stream_chunk`, `is_heartbeat`), where the
    // guard is correct regardless of what the daemon does with a filter.
    let filter = airc_lib::EventFilter::default();
    airc.subscribe_subscribed_delivery(filter, None).await
}
