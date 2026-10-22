//! `AircEventPublisher` — peer-side cross-grid event publisher.
//!
//! The inbound symmetric of [`AircEventTransport`](super::AircEventTransport).
//! When a remote substrate dispatches
//! `airc://<this-peer>/events/<topic>/subscribe`, its
//! AircEventTransport packages an
//! [`AircEventSubscribe`](super::AircEventSubscribe) envelope and
//! sends via `Airc::request()`. The airc daemon on this side
//! routes the envelope by body_hint to whichever ConsumerAdapter
//! claims [`EVENT_SUBSCRIBE_BODY_HINT`]. That adapter wraps THIS
//! publisher.
//!
//! ## What this commit ships
//!
//! This commit lands the **state machine + pure testable seams**:
//!
//! - [`EventPublisherState`] — the in-memory subscription registry.
//!   Tracks active `(subscription_id → ActiveSubscription)` mappings
//!   with subscriber peer, topic, filter, and a monotonic sequence
//!   counter for drop-detection on the caller side.
//! - [`register_subscription`](EventPublisherState::register) /
//!   [`unregister_subscription`](EventPublisherState::unregister) /
//!   [`lookup_matching`](EventPublisherState::lookup_matching) —
//!   the three substrate primitives the ConsumerAdapters delegate
//!   into.
//! - Pure free functions: [`parse_subscribe_envelope`],
//!   [`parse_unsubscribe_envelope`], [`build_subscribe_ack`],
//!   [`build_unsubscribe_ack`], [`build_deliver_frame`],
//!   [`matches_filter`] — every refusal/decision branch is
//!   testable without airc, mirroring the testable-seams pattern
//!   from `AircEventTransport` and `CommandRequestHandler`.
//!
//! ## What lands next
//!
//! - **`EventSubscribeAdapter`** + **`EventUnsubscribeAdapter`** —
//!   thin `ConsumerAdapter` wrappers that parse via the pure
//!   functions above, delegate to [`EventPublisherState`], and ack
//!   via `Airc::reply()`. Both share an `Arc<EventPublisherState>`
//!   so unsubscribe targets the same registry subscribe populated.
//! - **`AircEventPublisher::publish(topic, payload)`** — fan-out
//!   method. Looks up matching subscriptions via
//!   `lookup_matching(topic, &payload)`, builds an
//!   [`AircEventDeliver`] per subscriber (sequence bumped), sends
//!   each as a room-broadcast `Airc::publish` with the
//!   subscription_id header (subscribers demux via
//!   `AircEventTransport::matches_subscription`).
//! - Eventually: a local-`Events::emit()` bridge that calls
//!   `publish()` automatically whenever a topic the substrate has
//!   subscribers for fires. Until the local Events infrastructure
//!   exists in continuum-core, the publish API is caller-driven —
//!   intentionally simple, intentionally not auto-wired.
//!
//! ## Design choice: room-broadcast Deliver, header-demux at subscriber
//!
//! The airc transport addressing model is room-as-trust-boundary.
//! `Airc::publish` broadcasts to the current room; subscribers
//! demux by `HEADER_EVENT_SUBSCRIPTION_ID`. The same shape airc
//! chat uses (everyone in the room sees the frame; addressing is
//! semantic). Per-peer point-to-point is an optimization for a
//! later slice, not a correctness need — subscription_id is a
//! UUID, indistinguishable to non-subscribers.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use airc_core::{Body, Headers, PeerId, TranscriptEvent};
use airc_lib::adapter::AdapterError;
use airc_lib::Airc;
use airc_protocol::{FrameKind, HEADER_AIRC_CORRELATION_ID, HEADER_AIRC_REPLY_TO};
use parking_lot::RwLock;
use serde_json::Value;
use uuid::Uuid;

use super::{
    AircEventDeliver, AircEventPublish, AircEventPublishAck, AircEventSubscribe,
    AircEventSubscribeAck, AircEventUnsubscribe, AircEventUnsubscribeAck, EVENT_ACK_BODY_HINT,
    EVENT_DELIVER_BODY_HINT, HEADER_CONTINUUM_BODY_HINT, HEADER_EVENT_KIND,
    HEADER_EVENT_SUBSCRIPTION_ID, HEADER_EVENT_TOPIC,
};

// ─── Public state machine ──────────────────────────────────────────

/// In-memory registry of active subscriptions on this peer.
///
/// Shared by [`EventSubscribeAdapter`] + [`EventUnsubscribeAdapter`]
/// (next commit) via `Arc<EventPublisherState>`: subscribe inserts,
/// unsubscribe removes, publish reads + bumps the per-subscription
/// sequence counter.
///
/// Uses [`parking_lot::RwLock`] because the operations are
/// short-lived and synchronous (HashMap insert/remove/lookup) and
/// don't need to span `.await` points.
#[derive(Default)]
pub struct EventPublisherState {
    subscriptions: RwLock<HashMap<Uuid, ActiveSubscription>>,
}

/// One active subscription's metadata. Internal to the state
/// machine; callers interact via the state's public methods.
pub struct ActiveSubscription {
    pub subscriber_peer_id: PeerId,
    pub topic: String,
    pub filter: Option<Value>,
    /// Monotonic per-subscription sequence — incremented atomically
    /// each time a Deliver frame is built for this subscription.
    /// Caller-side detects gaps as drops.
    pub sequence: AtomicU64,
}

impl ActiveSubscription {
    fn new(subscriber_peer_id: PeerId, topic: String, filter: Option<Value>) -> Self {
        Self {
            subscriber_peer_id,
            topic,
            filter,
            sequence: AtomicU64::new(0),
        }
    }

    /// Bump + return the next sequence number. Used by the publish
    /// fan-out (next commit) when building a Deliver for this
    /// subscription.
    pub fn next_sequence(&self) -> u64 {
        self.sequence.fetch_add(1, Ordering::Relaxed)
    }
}

impl EventPublisherState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a new active subscription. Mints the
    /// `subscription_id` UUID and returns it so the caller (the
    /// EventSubscribeAdapter, next commit) can stamp it into the
    /// `AircEventSubscribeAck`.
    ///
    /// Per [[no-fallbacks-ever]]: empty topic is refused upfront so
    /// the registry never holds an unaddressable entry.
    pub fn register(
        &self,
        subscriber_peer_id: PeerId,
        topic: String,
        filter: Option<Value>,
    ) -> Result<Uuid, String> {
        if topic.is_empty() {
            return Err(
                "EventPublisherState: subscription topic must not be empty — \
                 a topicless subscription would either match nothing (silent) \
                 or every topic (firehose). Per [[no-fallbacks-ever]] the \
                 registry refuses upfront."
                    .to_string(),
            );
        }
        let subscription_id = Uuid::new_v4();
        let active = ActiveSubscription::new(subscriber_peer_id, topic, filter);
        self.subscriptions.write().insert(subscription_id, active);
        Ok(subscription_id)
    }

    /// Remove an active subscription by id. Returns whether the
    /// subscription was active (`true`) or already gone (`false`,
    /// idempotent unsubscribe contract).
    pub fn unregister(&self, subscription_id: Uuid) -> bool {
        self.subscriptions
            .write()
            .remove(&subscription_id)
            .is_some()
    }

    /// Return `true` iff the subscription is currently registered.
    /// Used by tests; production code uses `lookup_matching` or
    /// `unregister`.
    pub fn is_registered(&self, subscription_id: Uuid) -> bool {
        self.subscriptions.read().contains_key(&subscription_id)
    }

    /// Number of active subscriptions. Used for telemetry +
    /// substrate health checks.
    pub fn len(&self) -> usize {
        self.subscriptions.read().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Find every active subscription whose topic equals `topic`
    /// and whose filter (if any) matches `payload`. Returns
    /// `(subscription_id, subscriber_peer_id, sequence_number)`
    /// triples so the publish fan-out (next commit) can build the
    /// Deliver frames without holding the read lock across the
    /// send.
    ///
    /// The sequence is captured by reference here — the caller is
    /// expected to call `next_sequence()` on the held subscription
    /// at the moment of send. To avoid races between lookup and
    /// send, the caller uses the snapshot vec returned here; only
    /// the per-subscription atomic bump matters for ordering.
    pub fn lookup_matching(&self, topic: &str, payload: &Value) -> Vec<MatchedSubscription> {
        let map = self.subscriptions.read();
        map.iter()
            .filter(|(_id, sub)| sub.topic == topic && matches_filter(sub.filter.as_ref(), payload))
            .map(|(id, sub)| MatchedSubscription {
                subscription_id: *id,
                subscriber_peer_id: sub.subscriber_peer_id,
                sequence: sub.next_sequence(),
            })
            .collect()
    }
}

/// One result row from [`EventPublisherState::lookup_matching`].
/// Carries everything needed to build a Deliver envelope without
/// re-touching the state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchedSubscription {
    pub subscription_id: Uuid,
    pub subscriber_peer_id: PeerId,
    pub sequence: u64,
}

// ─── Public facade ─────────────────────────────────────────────────

/// `AircEventPublisher` is the peer-side event publisher facade.
///
/// Composes:
/// - `Arc<Airc>` for sending Deliver frames over the grid.
/// - `Arc<EventPublisherState>` for tracking active subscriptions.
///
/// Exposes:
/// - [`state()`](Self::state) — the shared state for adapter
///   composition (the two `ConsumerAdapter`s in
///   [`super::airc_event_adapters`] take this Arc).
/// - [`publish()`](Self::publish) — the fan-out method. Looks up
///   matching subscriptions, builds an [`AircEventDeliver`] per
///   match (sequence bumped at lookup), sends each via
///   `Airc::publish` with the subscription_id header. Returns the
///   fan-out count for telemetry.
///
/// ## Composition at boot
///
/// ```ignore
/// use std::sync::Arc;
/// use continuum_core::routing::{
///     AircEventPublisher, EventSubscribeAdapter, EventUnsubscribeAdapter,
/// };
///
/// let publisher = Arc::new(AircEventPublisher::new(airc.clone()));
/// let subscribe = EventSubscribeAdapter::new(airc.clone(), publisher.state().clone());
/// let unsubscribe = EventUnsubscribeAdapter::new(airc.clone(), publisher.state().clone());
/// airc.register_consumer_adapter(subscribe).await?;
/// airc.register_consumer_adapter(unsubscribe).await?;
///
/// // Later, when substrate cognition emits an event:
/// publisher.publish("cognition/analyze/complete", payload).await?;
/// ```
pub struct AircEventPublisher {
    airc: Arc<Airc>,
    state: Arc<EventPublisherState>,
}

impl std::fmt::Debug for AircEventPublisher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AircEventPublisher")
            .field("subscriptions", &self.state.len())
            .finish_non_exhaustive()
    }
}

impl AircEventPublisher {
    /// Build a publisher facade with a fresh empty state.
    pub fn new(airc: Arc<Airc>) -> Self {
        Self {
            airc,
            state: Arc::new(EventPublisherState::new()),
        }
    }

    /// Build a publisher facade against an existing state Arc.
    /// Useful in tests + when the state was created independently
    /// for inspection.
    pub fn with_state(airc: Arc<Airc>, state: Arc<EventPublisherState>) -> Self {
        Self { airc, state }
    }

    /// Borrow the shared state. The two `ConsumerAdapter`s in
    /// [`super::airc_event_adapters`] take this Arc so the
    /// subscribe path populates the same registry the publish path
    /// reads.
    pub fn state(&self) -> &Arc<EventPublisherState> {
        &self.state
    }

    /// Fan out an event to every matching subscriber.
    ///
    /// 1. `lookup_matching(topic, &payload)` collects matched
    ///    subscriptions (sequence bumped atomically per match).
    /// 2. For each match, builds an `AircEventDeliver` carrying the
    ///    captured sequence + the (cloned) payload.
    /// 3. Sends each via `Airc::publish(CurrentRoom, Event, ..)`
    ///    with the subscription_id stamped as a header so
    ///    subscribers demux via
    ///    `AircEventTransport::matches_subscription`.
    ///
    /// Returns the count of fanned-out Deliver frames. Zero
    /// matches is NOT an error — silent topics + filter
    /// mismatches are valid steady-state.
    ///
    /// ## Failure mode
    ///
    /// If a per-subscription `Airc::publish` errors, the call returns
    /// the typed error immediately with the count of successful
    /// fanouts so far in the error message. Per
    /// `[[no-fallbacks-ever]]`: silent partial fanout would mask the
    /// transport failure.
    pub async fn publish(&self, topic: &str, payload: Value) -> Result<usize, String> {
        let envelopes = Self::build_publish_envelopes(&self.state, topic, &payload)?;
        if envelopes.is_empty() {
            return Ok(0);
        }

        let total = envelopes.len();
        let mut sent = 0usize;
        for (matched, headers, body) in envelopes {
            self.airc
                .publish(
                    airc_lib::PublishTarget::CurrentRoom,
                    FrameKind::Event,
                    body,
                    headers,
                )
                .await
                .map_err(|e| {
                    format!(
                        "AircEventPublisher::publish: airc.publish failed after {sent} \
                         successful fanouts of {total} (sub={sub_id}): {e}",
                        sub_id = matched.subscription_id
                    )
                })?;
            sent += 1;
        }
        Ok(sent)
    }

    /// Pure function: collect (matched_subscription, headers, body)
    /// triples ready to send for a `(topic, payload)` event. Used
    /// by [`Self::publish`] but exposed `pub` so tests verify the
    /// composition without airc.
    ///
    /// Returns an empty vec when no subscriptions match — silent
    /// topics + filter mismatches are valid steady-state, not
    /// errors. Returns `Err` only if `build_deliver_frame` itself
    /// fails (would only happen on `serde_json` failure of the
    /// payload, which the substrate's `Value`-typed callers don't
    /// hit in practice; pinned anyway).
    pub fn build_publish_envelopes(
        state: &EventPublisherState,
        topic: &str,
        payload: &Value,
    ) -> Result<Vec<(MatchedSubscription, Headers, Body)>, String> {
        let matches = state.lookup_matching(topic, payload);
        let mut out = Vec::with_capacity(matches.len());
        for matched in matches {
            let deliver = AircEventDeliver {
                subscription_id: matched.subscription_id,
                topic: topic.to_string(),
                sequence: matched.sequence,
                payload: payload.clone(),
            };
            let (headers, body) = build_deliver_frame(&deliver).map_err(|e| {
                format!(
                    "AircEventPublisher::build_publish_envelopes: build_deliver_frame failed for sub={sub_id}: {e}",
                    sub_id = matched.subscription_id
                )
            })?;
            out.push((matched, headers, body));
        }
        Ok(out)
    }
}

// ─── Pure free functions (testable seams) ──────────────────────────

/// Parsed pieces of an inbound subscribe envelope. Pure data;
/// every adapter operation that depends on a parsed envelope
/// works against this struct.
#[derive(Debug, Clone)]
pub struct ParsedSubscribe {
    pub caller_peer_id: PeerId,
    pub reply_to: PeerId,
    pub correlation_id: Uuid,
    pub request: AircEventSubscribe,
}

/// Parsed pieces of an inbound unsubscribe envelope. Same shape
/// as [`ParsedSubscribe`] with the unsubscribe body.
#[derive(Debug, Clone)]
pub struct ParsedUnsubscribe {
    pub caller_peer_id: PeerId,
    pub reply_to: PeerId,
    pub correlation_id: Uuid,
    pub request: AircEventUnsubscribe,
}

/// Parsed pieces of an inbound publish envelope — the `emit` half of
/// the Event primitive. Same shape as [`ParsedSubscribe`] with the
/// publish body (topic + payload to fan out).
#[derive(Debug, Clone)]
pub struct ParsedPublish {
    pub caller_peer_id: PeerId,
    pub reply_to: PeerId,
    pub correlation_id: Uuid,
    pub request: AircEventPublish,
}

/// Parse an inbound subscribe `TranscriptEvent` into
/// [`ParsedSubscribe`]. Pure function — every refusal branch is
/// testable without airc.
///
/// Mirrors `CommandRequestHandler::parse_envelope` exactly so the
/// two pre-adapter parsers stay symmetric. A reader sees one and
/// understands the other immediately.
pub fn parse_subscribe_envelope(
    envelope: &TranscriptEvent,
) -> Result<ParsedSubscribe, AdapterError> {
    let caller_peer_id = envelope.peer_id;

    let reply_to_raw = envelope.headers.get(HEADER_AIRC_REPLY_TO).ok_or_else(|| {
        AdapterError::Consumer(format!(
            "missing required header {HEADER_AIRC_REPLY_TO} on inbound event subscribe envelope"
        ))
    })?;
    let reply_to_uuid: Uuid = reply_to_raw.parse().map_err(|e| {
        AdapterError::Consumer(format!(
            "header {HEADER_AIRC_REPLY_TO}={reply_to_raw:?} is not a valid UUID: {e}"
        ))
    })?;
    let reply_to = PeerId(reply_to_uuid);

    let correlation_raw = envelope
        .headers
        .get(HEADER_AIRC_CORRELATION_ID)
        .ok_or_else(|| {
            AdapterError::Consumer(format!(
                "missing required header {HEADER_AIRC_CORRELATION_ID} on inbound event subscribe envelope"
            ))
        })?;
    let correlation_id: Uuid = correlation_raw.parse().map_err(|e| {
        AdapterError::Consumer(format!(
            "header {HEADER_AIRC_CORRELATION_ID}={correlation_raw:?} is not a valid UUID: {e}"
        ))
    })?;

    let body = envelope.body.as_ref().ok_or_else(|| {
        AdapterError::Consumer(
            "inbound event subscribe envelope has no body (expected Body::Json(AircEventSubscribe))"
                .to_string(),
        )
    })?;

    let body_value = match body {
        Body::Json(v) => v.clone(),
        Body::Binary(_) => {
            return Err(AdapterError::Consumer(
                "inbound event subscribe body was Binary; expected Json(AircEventSubscribe)"
                    .to_string(),
            ));
        }
    };

    let request: AircEventSubscribe = serde_json::from_value(body_value).map_err(|e| {
        AdapterError::Consumer(format!("decode AircEventSubscribe from body JSON: {e}"))
    })?;

    Ok(ParsedSubscribe {
        caller_peer_id,
        reply_to,
        correlation_id,
        request,
    })
}

/// Parse an inbound unsubscribe `TranscriptEvent`. Pure function.
/// Same shape as [`parse_subscribe_envelope`] for the unsubscribe
/// body.
pub fn parse_unsubscribe_envelope(
    envelope: &TranscriptEvent,
) -> Result<ParsedUnsubscribe, AdapterError> {
    let caller_peer_id = envelope.peer_id;

    let reply_to_raw = envelope.headers.get(HEADER_AIRC_REPLY_TO).ok_or_else(|| {
        AdapterError::Consumer(format!(
            "missing required header {HEADER_AIRC_REPLY_TO} on inbound event unsubscribe envelope"
        ))
    })?;
    let reply_to_uuid: Uuid = reply_to_raw.parse().map_err(|e| {
        AdapterError::Consumer(format!(
            "header {HEADER_AIRC_REPLY_TO}={reply_to_raw:?} is not a valid UUID: {e}"
        ))
    })?;
    let reply_to = PeerId(reply_to_uuid);

    let correlation_raw = envelope
        .headers
        .get(HEADER_AIRC_CORRELATION_ID)
        .ok_or_else(|| {
            AdapterError::Consumer(format!(
                "missing required header {HEADER_AIRC_CORRELATION_ID} on inbound event unsubscribe envelope"
            ))
        })?;
    let correlation_id: Uuid = correlation_raw.parse().map_err(|e| {
        AdapterError::Consumer(format!(
            "header {HEADER_AIRC_CORRELATION_ID}={correlation_raw:?} is not a valid UUID: {e}"
        ))
    })?;

    let body = envelope.body.as_ref().ok_or_else(|| {
        AdapterError::Consumer(
            "inbound event unsubscribe envelope has no body (expected Body::Json(AircEventUnsubscribe))"
                .to_string(),
        )
    })?;

    let body_value = match body {
        Body::Json(v) => v.clone(),
        Body::Binary(_) => {
            return Err(AdapterError::Consumer(
                "inbound event unsubscribe body was Binary; expected Json(AircEventUnsubscribe)"
                    .to_string(),
            ));
        }
    };

    let request: AircEventUnsubscribe = serde_json::from_value(body_value).map_err(|e| {
        AdapterError::Consumer(format!("decode AircEventUnsubscribe from body JSON: {e}"))
    })?;

    Ok(ParsedUnsubscribe {
        caller_peer_id,
        reply_to,
        correlation_id,
        request,
    })
}

/// Parse an inbound publish `TranscriptEvent` into [`ParsedPublish`].
/// Pure function — the `emit` twin of [`parse_subscribe_envelope`],
/// same header/body refusal branches for the publish body.
pub fn parse_publish_envelope(envelope: &TranscriptEvent) -> Result<ParsedPublish, AdapterError> {
    let caller_peer_id = envelope.peer_id;

    let reply_to_raw = envelope.headers.get(HEADER_AIRC_REPLY_TO).ok_or_else(|| {
        AdapterError::Consumer(format!(
            "missing required header {HEADER_AIRC_REPLY_TO} on inbound event publish envelope"
        ))
    })?;
    let reply_to_uuid: Uuid = reply_to_raw.parse().map_err(|e| {
        AdapterError::Consumer(format!(
            "header {HEADER_AIRC_REPLY_TO}={reply_to_raw:?} is not a valid UUID: {e}"
        ))
    })?;
    let reply_to = PeerId(reply_to_uuid);

    let correlation_raw = envelope
        .headers
        .get(HEADER_AIRC_CORRELATION_ID)
        .ok_or_else(|| {
            AdapterError::Consumer(format!(
                "missing required header {HEADER_AIRC_CORRELATION_ID} on inbound event publish envelope"
            ))
        })?;
    let correlation_id: Uuid = correlation_raw.parse().map_err(|e| {
        AdapterError::Consumer(format!(
            "header {HEADER_AIRC_CORRELATION_ID}={correlation_raw:?} is not a valid UUID: {e}"
        ))
    })?;

    let body = envelope.body.as_ref().ok_or_else(|| {
        AdapterError::Consumer(
            "inbound event publish envelope has no body (expected Body::Json(AircEventPublish))"
                .to_string(),
        )
    })?;

    let body_value = match body {
        Body::Json(v) => v.clone(),
        Body::Binary(_) => {
            return Err(AdapterError::Consumer(
                "inbound event publish body was Binary; expected Json(AircEventPublish)"
                    .to_string(),
            ));
        }
    };

    let request: AircEventPublish = serde_json::from_value(body_value).map_err(|e| {
        AdapterError::Consumer(format!("decode AircEventPublish from body JSON: {e}"))
    })?;

    Ok(ParsedPublish {
        caller_peer_id,
        reply_to,
        correlation_id,
        request,
    })
}

/// Build the `(Headers, Body)` for a subscribe ack reply. Pure
/// function — used by the ConsumerAdapter (next commit) when
/// replying via `Airc::reply`.
pub fn build_subscribe_ack(subscription_id: Uuid, topic: &str) -> Result<(Headers, Body), String> {
    let ack = AircEventSubscribeAck {
        subscription_id,
        topic: topic.to_string(),
    };
    let body_value =
        serde_json::to_value(&ack).map_err(|e| format!("serialize AircEventSubscribeAck: {e}"))?;
    let body = Body::Json(body_value);

    let mut headers = Headers::new();
    headers.insert(HEADER_EVENT_KIND.to_string(), "ack".to_string());
    headers.insert(
        HEADER_EVENT_SUBSCRIPTION_ID.to_string(),
        subscription_id.to_string(),
    );
    headers.insert(HEADER_EVENT_TOPIC.to_string(), topic.to_string());
    headers.insert(
        HEADER_CONTINUUM_BODY_HINT.to_string(),
        EVENT_ACK_BODY_HINT.to_string(),
    );

    Ok((headers, body))
}

/// Build the `(Headers, Body)` for an unsubscribe ack reply.
/// Pure function. `closed: false` is idempotent (already-gone).
pub fn build_unsubscribe_ack(
    subscription_id: Uuid,
    closed: bool,
) -> Result<(Headers, Body), String> {
    let ack = AircEventUnsubscribeAck {
        subscription_id,
        closed,
    };
    let body_value = serde_json::to_value(&ack)
        .map_err(|e| format!("serialize AircEventUnsubscribeAck: {e}"))?;
    let body = Body::Json(body_value);

    let mut headers = Headers::new();
    headers.insert(HEADER_EVENT_KIND.to_string(), "ack".to_string());
    headers.insert(
        HEADER_EVENT_SUBSCRIPTION_ID.to_string(),
        subscription_id.to_string(),
    );
    headers.insert(
        HEADER_CONTINUUM_BODY_HINT.to_string(),
        EVENT_ACK_BODY_HINT.to_string(),
    );

    Ok((headers, body))
}

/// Build the `(Headers, Body)` for a publish ack reply. Pure
/// function — the `emit` twin of [`build_subscribe_ack`]. Carries the
/// fan-out count (`delivered`) so the caller's `emit()` learns how many
/// subscribers the event reached.
pub fn build_publish_ack(topic: &str, delivered: u64) -> Result<(Headers, Body), String> {
    let ack = AircEventPublishAck {
        topic: topic.to_string(),
        delivered,
    };
    let body_value =
        serde_json::to_value(&ack).map_err(|e| format!("serialize AircEventPublishAck: {e}"))?;
    let body = Body::Json(body_value);

    let mut headers = Headers::new();
    headers.insert(HEADER_EVENT_KIND.to_string(), "ack".to_string());
    headers.insert(HEADER_EVENT_TOPIC.to_string(), topic.to_string());
    headers.insert(
        HEADER_CONTINUUM_BODY_HINT.to_string(),
        EVENT_ACK_BODY_HINT.to_string(),
    );

    Ok((headers, body))
}

/// Build the `(Headers, Body)` for a Deliver frame. Pure function.
/// The publish fan-out (next commit) calls this for each matched
/// subscription before sending via `Airc::publish`.
pub fn build_deliver_frame(deliver: &AircEventDeliver) -> Result<(Headers, Body), String> {
    let body_value =
        serde_json::to_value(deliver).map_err(|e| format!("serialize AircEventDeliver: {e}"))?;
    let body = Body::Json(body_value);

    let mut headers = Headers::new();
    headers.insert(HEADER_EVENT_KIND.to_string(), "deliver".to_string());
    headers.insert(HEADER_EVENT_TOPIC.to_string(), deliver.topic.clone());
    headers.insert(
        HEADER_EVENT_SUBSCRIPTION_ID.to_string(),
        deliver.subscription_id.to_string(),
    );
    headers.insert(
        HEADER_CONTINUUM_BODY_HINT.to_string(),
        EVENT_DELIVER_BODY_HINT.to_string(),
    );

    Ok((headers, body))
}

/// Pure predicate: does `payload` satisfy `filter`?
///
/// Filter semantics (v1 — conservative, expandable):
///
/// - `None` → matches everything (no filter set).
/// - `Some(Value::Object{})` (empty object) → matches everything
///   (caller explicitly opted for "no constraints").
/// - `Some(Value::Object{k: v, ...})` → every `(k, v)` in the
///   filter must equal-match a top-level field in the payload.
///   This is an AND of equality predicates — the smallest useful
///   shape we can ship without inventing a query language.
/// - Anything else → does NOT match. v2 may add predicate trees
///   ($gt/$in/$regex), but those need their own slice with
///   explicit doctrine alignment.
pub fn matches_filter(filter: Option<&Value>, payload: &Value) -> bool {
    let filter = match filter {
        None => return true,
        Some(f) => f,
    };
    let constraints = match filter.as_object() {
        Some(obj) if obj.is_empty() => return true,
        Some(obj) => obj,
        None => return false, // non-object filter: refuse rather than guess
    };
    let payload_obj = match payload.as_object() {
        Some(obj) => obj,
        None => return false, // payload not an object: filter can't apply
    };
    constraints
        .iter()
        .all(|(k, want)| payload_obj.get(k) == Some(want))
}

#[cfg(test)]
mod tests;
