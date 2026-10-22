//! `AircEventTransport` — caller-side cross-grid event subscription.
//!
//! The event-side parallel of [`AircTransport`](super::airc_transport).
//! When a substrate caller wants to subscribe to events on a remote
//! peer, this transport packages an
//! [`AircEventSubscribe`](super::AircEventSubscribe) envelope via
//! `Airc::request`, awaits the
//! [`AircEventSubscribeAck`](super::AircEventSubscribeAck), then
//! exposes the per-subscription
//! [`AircEventDeliver`](super::AircEventDeliver) frames as an
//! ongoing stream.
//!
//! ## Same testable-seams pattern as `AircTransport`
//!
//! Per PR #1529 reviewer 3 fix: every refusal branch is factored as
//! a `pub` free function so unit tests cover them without spinning
//! up real airc. The `subscribe`/`unsubscribe` methods themselves
//! are thin wrappers — request, await, decode via the free
//! functions, return.
//!
//! Pure free functions (testable WITHOUT `Airc`):
//!
//! - [`Self::resolve_subscribe`] — build the outbound `MentionTarget` +
//!   `Headers` + serialized `Body` for a subscribe request
//! - [`Self::decode_subscribe_ack`] — unpack a reply body as
//!   [`AircEventSubscribeAck`], surfacing typed errors for missing
//!   body / binary body / malformed JSON
//! - [`Self::resolve_unsubscribe`] — same for unsubscribe-request
//! - [`Self::decode_unsubscribe_ack`] — same for unsubscribe-reply
//! - [`Self::decode_deliver_frame`] — extract an
//!   [`AircEventDeliver`] from an inbound `TranscriptEvent`, or
//!   typed error
//! - [`Self::matches_subscription`] — pure predicate: does this
//!   `TranscriptEvent` belong to a given subscription_id? Used to
//!   filter the airc event stream into a per-subscription view
//!
//! ## Three-message flow
//!
//! ```text
//! caller                                                  peer-side publisher
//!   │                                                              │
//!   │ ── Airc::request: Subscribe { topic, filter } ──────────────▶│
//!   │                                                              │ mints subscription_id
//!   │ ◀── Airc::await_reply: SubscribeAck { subscription_id } ─────│
//!   │                                                              │
//!   │                                                              │ as events fire matching topic+filter:
//!   │ ◀── Deliver { subscription_id, seq, payload } ───────────────│
//!   │ ◀── Deliver { subscription_id, seq, payload } ───────────────│
//!   │ ...                                                          │
//!   │                                                              │
//!   │ ── Airc::request: Unsubscribe { subscription_id } ──────────▶│
//!   │ ◀── UnsubscribeAck { closed } ───────────────────────────────│
//! ```
//!
//! ## What this commit ships
//!
//! - `AircEventTransport` struct holding `Arc<airc_lib::Airc>` + a
//!   configurable deadline.
//! - The 6 pure free functions above + their unit tests.
//! - `subscribe(target_peer, topic, filter)` — sends the subscribe
//!   request via `Airc::request`, awaits ack, returns
//!   [`EventSubscription`] carrying the subscription_id and a
//!   `tokio::sync::mpsc::Receiver<AircEventDeliver>` that yields
//!   matching delivery frames.
//! - `unsubscribe(target_peer, subscription_id)` — sends the
//!   unsubscribe request, awaits ack, returns whether the peer
//!   considered the subscription active.
//!
//! ## What lands next
//!
//! - **Peer-side event publisher** — `ConsumerAdapter` registered
//!   against `EVENT_SUBSCRIBE_BODY_HINT`. Mints subscription_id on
//!   receiving subscribe, registers locally, subscribes to local
//!   `Events::emit()` for matching topics, fans out as
//!   `AircEventDeliver` frames to the subscriber's peer_id.
//! - **`Transport` trait extension** — adds `subscribe()` method so
//!   the dispatcher's match can route subscription URIs (e.g.
//!   `airc://<peer>/events/<topic>/subscribe`) through this
//!   transport via the same chain as commands.
//! - **`Events::subscribe::<E>(uri)` typed consumer API** — the row
//!   above wire-level; turns `Events::subscribe::<ChatMessages>(uri)`
//!   into a typed `EventStream<ChatMessage>`.
//! - **LAN-loopback integration test** — paired with the command
//!   surface integration test (#188) once `TwoAircLoopback` fixture
//!   lands.

use std::sync::Arc;
use std::time::Duration;

use airc_core::{Body, MentionTarget, PeerId, TranscriptEvent};
use airc_lib::Airc;
use serde_json::Value;
use tokio::sync::mpsc;
use uuid::Uuid;

use super::{
    AircEventDeliver, AircEventSubscribe, AircEventSubscribeAck, AircEventUnsubscribe,
    AircEventUnsubscribeAck, EVENT_SUBSCRIBE_BODY_HINT, EVENT_UNSUBSCRIBE_BODY_HINT,
    HEADER_CONTINUUM_BODY_HINT, HEADER_EVENT_KIND, HEADER_EVENT_SUBSCRIPTION_ID,
    HEADER_EVENT_TOPIC,
};

/// Default deadline for the subscribe/unsubscribe round-trip when
/// the caller didn't set one. Same value as
/// [`super::DEFAULT_DEADLINE`] — cross-grid event handshakes have
/// the same shape as command dispatches.
pub const DEFAULT_EVENT_DEADLINE: Duration = Duration::from_secs(30);

/// Default channel capacity for the per-subscription delivery
/// queue. Subscribers fall behind by more than this and the
/// channel sender start dropping (the mpsc semantics) — by design,
/// so a slow consumer can't backpressure the airc receive task.
pub const DEFAULT_DELIVERY_QUEUE_CAPACITY: usize = 256;

/// The caller-side cross-grid event transport.
///
/// Holds an `Arc<airc_lib::Airc>` — typically the same handle the
/// substrate uses everywhere else. Cheap clone (single Arc clone);
/// normally stored as an `Arc<AircEventTransport>` so multiple
/// callers share the connection.
pub struct AircEventTransport {
    airc: Arc<Airc>,
    deadline: Duration,
}

impl std::fmt::Debug for AircEventTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AircEventTransport")
            .field("deadline", &self.deadline)
            .finish_non_exhaustive()
    }
}

/// Per-subscription handle returned from
/// [`AircEventTransport::subscribe`]. Carries the
/// `subscription_id` (used to issue `unsubscribe` and to demux
/// when the caller holds N subscriptions), the verified
/// `publisher_peer_id` (the peer we trust to source these events),
/// and a typed [`AircEventDeliver`] receiver.
///
/// Dropping the receiver DOES tear down the per-subscription
/// filter task — the spawned task exits as soon as the channel
/// closes — but does NOT notify the peer to stop publishing.
/// Callers SHOULD call [`AircEventTransport::unsubscribe`] to
/// tear the subscription down properly. The substrate doesn't
/// auto-unsubscribe on drop because dropping mid-flight could
/// lose in-transit deliveries the caller still wants to drain;
/// explicit unsubscribe is the honest shape.
pub struct EventSubscription {
    pub subscription_id: Uuid,
    /// The peer we subscribed to — the only peer whose Deliver
    /// frames the filter task will forward for this subscription.
    /// Closes the forgery vector where any room peer could stamp
    /// matching `subscription_id` headers on a forged frame
    /// (per PR #1529 reviewer 2 BLOCK).
    pub publisher_peer_id: PeerId,
    pub topic: String,
    pub deliveries: mpsc::Receiver<AircEventDeliver>,
}

impl AircEventTransport {
    /// Build an event transport against an existing airc handle with
    /// the default deadline.
    pub fn new(airc: Arc<Airc>) -> Self {
        Self {
            airc,
            deadline: DEFAULT_EVENT_DEADLINE,
        }
    }

    /// Replace the default deadline. Builder-style.
    pub fn with_deadline(mut self, deadline: Duration) -> Self {
        self.deadline = deadline;
        self
    }

    // ─── Pure testable seams (per PR #1529 reviewer 3 pattern) ────

    /// Build the outbound subscribe envelope:
    /// `(MentionTarget, Headers, Body)`. Pure function — no airc
    /// involvement, fully unit-testable.
    // ─── Pure helpers: delegate to continuum-airc-protocol ───────────
    //
    // The wire-shape source of truth lives in
    // `continuum_airc_protocol::event`. These methods are thin
    // delegations preserved on the `AircEventTransport` type for
    // backward compatibility with existing call sites and tests.
    // The client crate (`continuum-client`) calls the free functions
    // directly. Same code, two entry points, zero drift.

    /// Build the outbound subscribe envelope. Delegates to
    /// [`continuum_airc_protocol::event::resolve_subscribe`].
    pub fn resolve_subscribe(
        target_peer: PeerId,
        topic: &str,
        filter: Option<Value>,
    ) -> Result<(MentionTarget, airc_core::Headers, Body), String> {
        continuum_airc_protocol::event::resolve_subscribe(target_peer, topic, filter)
    }

    /// Build the outbound unsubscribe envelope. Delegates to
    /// [`continuum_airc_protocol::event::resolve_unsubscribe`].
    pub fn resolve_unsubscribe(
        target_peer: PeerId,
        subscription_id: Uuid,
    ) -> Result<(MentionTarget, airc_core::Headers, Body), String> {
        continuum_airc_protocol::event::resolve_unsubscribe(target_peer, subscription_id)
    }

    /// Decode a subscribe-reply body. Delegates to
    /// [`continuum_airc_protocol::event::decode_subscribe_ack`].
    pub fn decode_subscribe_ack(reply_body: Option<Body>) -> Result<AircEventSubscribeAck, String> {
        continuum_airc_protocol::event::decode_subscribe_ack(reply_body)
    }

    /// Decode an unsubscribe-reply body. Delegates to
    /// [`continuum_airc_protocol::event::decode_unsubscribe_ack`].
    pub fn decode_unsubscribe_ack(
        reply_body: Option<Body>,
    ) -> Result<AircEventUnsubscribeAck, String> {
        continuum_airc_protocol::event::decode_unsubscribe_ack(reply_body)
    }

    /// Decode an inbound `TranscriptEvent` as an `AircEventDeliver`
    /// frame. Delegates to
    /// [`continuum_airc_protocol::event::decode_deliver_frame`].
    pub fn decode_deliver_frame(event: &TranscriptEvent) -> Result<AircEventDeliver, String> {
        continuum_airc_protocol::event::decode_deliver_frame(event)
    }

    /// Pure predicate: does this `TranscriptEvent` belong to the
    /// given subscription AND come from the expected publisher?
    /// Delegates to [`continuum_airc_protocol::event::matches_subscription`].
    pub fn matches_subscription(
        event: &TranscriptEvent,
        subscription_id: Uuid,
        expected_publisher: PeerId,
    ) -> bool {
        continuum_airc_protocol::event::matches_subscription(
            event,
            subscription_id,
            expected_publisher,
        )
    }

    // ─── airc-touching methods (covered by LAN-loopback in #188) ──

    /// Send a subscribe request and return a typed
    /// [`EventSubscription`] handle once the peer acks.
    ///
    /// The subscription handle's `deliveries` receiver yields
    /// matching [`AircEventDeliver`] frames as they arrive. A
    /// per-subscription background task subscribes to airc's
    /// general event stream and filters via
    /// [`Self::matches_subscription`] + [`Self::decode_deliver_frame`],
    /// pushing matches into the mpsc channel.
    ///
    /// The task lives until the receiver is dropped OR
    /// [`Self::unsubscribe`] is called. Per the doc comment on
    /// [`EventSubscription`], dropping the receiver does NOT
    /// notify the peer to stop sending — that's what
    /// `unsubscribe` is for.
    pub async fn subscribe(
        &self,
        target_peer: PeerId,
        topic: &str,
        filter: Option<Value>,
    ) -> Result<EventSubscription, String> {
        // 1. Open the airc event stream FIRST, before any
        //    peer-side state change. Per PR #1529 reviewer 1 BLOCK:
        //    spawning the filter task with `airc.subscribe().await`
        //    inside meant a stream-open failure silently returned
        //    `Ok(EventSubscription)` to the caller while the
        //    peer-side held an unobservable subscription. Open
        //    upfront so failure is reported as a typed `Err` BEFORE
        //    we touch peer state.
        //
        //    airc-lib's request contract guarantees the reply
        //    stream is armed before the request frame is sent
        //    (`Airc::request` doc); the analogous discipline here
        //    is to arm the Deliver stream before the subscribe
        //    request. No frames can be missed in the window between
        //    peer ack and filter task spawn.
        let event_stream =
            self.airc.subscribe().await.map_err(|e| {
                format!("AircEventTransport: airc subscribe stream open failed: {e}")
            })?;

        let (target, headers, body) = Self::resolve_subscribe(target_peer, topic, filter)?;

        let pending = self
            .airc
            .request(target, headers, body, self.deadline)
            .await
            .map_err(|e| format!("AircEventTransport: airc subscribe request failed: {e}"))?;

        let reply = self
            .airc
            .await_reply(pending)
            .await
            .map_err(|e| format!("AircEventTransport: subscribe await_reply failed: {e}"))?;

        let ack = Self::decode_subscribe_ack(reply.body)?;
        let subscription_id = ack.subscription_id;
        let publisher_peer_id = target_peer;

        // Spawn the per-subscription filter task on the
        // pre-opened airc stream. Forwards matching Deliver
        // frames to the mpsc; exits cleanly when the receiver
        // drops (closes `tx`) via `tokio::select!` so quiet
        // topics don't leak the task indefinitely (PR #1529
        // reviewer 2 BLOCK 3).
        let (tx, rx) = mpsc::channel::<AircEventDeliver>(DEFAULT_DELIVERY_QUEUE_CAPACITY);
        tokio::spawn(async move {
            use futures_util::StreamExt as _;
            let mut event_stream = event_stream;
            loop {
                let event = tokio::select! {
                    biased;
                    // Receiver dropped → caller doesn't want more
                    // deliveries. Exit promptly without waiting on
                    // the next airc frame (quiet topics never
                    // wake us otherwise — the leak this select
                    // closes).
                    _ = tx.closed() => break,
                    next = event_stream.next() => {
                        match next {
                            Some(Ok(e)) => e,
                            Some(Err(_)) => continue, // lag — skip; sequence gaps are caller-visible
                            None => break, // airc stream closed
                        }
                    }
                };
                if !Self::matches_subscription(&event, subscription_id, publisher_peer_id) {
                    continue;
                }
                let deliver = match Self::decode_deliver_frame(&event) {
                    Ok(d) => d,
                    Err(e) => {
                        // Frame passed publisher + body_hint +
                        // subscription_id checks but didn't decode.
                        // Either the publisher produced a malformed
                        // frame (protocol violation) OR the wire
                        // shape changed silently between versions.
                        // Either way the operator needs to see this
                        // — silent `continue` would hide the BLOCK
                        // R2.2 forgery class of bug post-mitigation.
                        tracing::warn!(
                            target: "continuum.event.transport",
                            subscription_id = %subscription_id,
                            publisher = %publisher_peer_id.0,
                            error = %e,
                            "AircEventTransport: malformed Deliver frame matching subscription — skipping"
                        );
                        continue;
                    }
                };
                if tx.send(deliver).await.is_err() {
                    // Race: receiver dropped between `tx.closed()`
                    // check and `send`. Same outcome — exit.
                    break;
                }
            }
        });

        Ok(EventSubscription {
            subscription_id,
            publisher_peer_id,
            topic: ack.topic,
            deliveries: rx,
        })
    }

    /// Send an unsubscribe request and return the peer's
    /// `closed` verdict.
    pub async fn unsubscribe(
        &self,
        target_peer: PeerId,
        subscription_id: Uuid,
    ) -> Result<bool, String> {
        let (target, headers, body) = Self::resolve_unsubscribe(target_peer, subscription_id)?;

        let pending = self
            .airc
            .request(target, headers, body, self.deadline)
            .await
            .map_err(|e| format!("AircEventTransport: airc unsubscribe request failed: {e}"))?;

        let reply = self
            .airc
            .await_reply(pending)
            .await
            .map_err(|e| format!("AircEventTransport: unsubscribe await_reply failed: {e}"))?;

        let ack = Self::decode_unsubscribe_ack(reply.body)?;
        Ok(ack.closed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::EVENT_DELIVER_BODY_HINT;
    use airc_core::{ClientId, EventId, RoomId, TranscriptKind};

    // ─── resolve_subscribe ───────────────────────────────────────────

    #[test]
    fn resolve_subscribe_with_topic_and_filter_produces_envelope() {
        let peer = PeerId::new();
        let topic = "cognition/analyze/complete";
        let filter = Some(serde_json::json!({"min_confidence": 0.6}));
        let (target, headers, body) =
            AircEventTransport::resolve_subscribe(peer, topic, filter.clone()).expect("happy path");
        assert!(matches!(target, MentionTarget::Peer(p) if p == peer));
        assert_eq!(
            headers.get(HEADER_EVENT_TOPIC).map(String::as_str),
            Some(topic)
        );
        assert_eq!(
            headers.get(HEADER_EVENT_KIND).map(String::as_str),
            Some("subscribe")
        );
        assert_eq!(
            headers.get(HEADER_CONTINUUM_BODY_HINT).map(String::as_str),
            Some(EVENT_SUBSCRIBE_BODY_HINT)
        );
        // Body must round-trip back to the original request shape
        match body {
            Body::Json(v) => {
                let back: AircEventSubscribe = serde_json::from_value(v).unwrap();
                assert_eq!(back.topic, topic);
                assert_eq!(back.filter, filter);
            }
            other => panic!("expected Json body, got {other:?}"),
        }
    }

    #[test]
    fn resolve_subscribe_with_none_filter_omits_filter_in_body() {
        let peer = PeerId::new();
        let (_target, _headers, body) =
            AircEventTransport::resolve_subscribe(peer, "x/y", None).expect("happy");
        match body {
            Body::Json(v) => {
                let s = v.to_string();
                assert!(!s.contains("\"filter\""), "filter omitted, got: {s}");
            }
            other => panic!("expected Json body, got {other:?}"),
        }
    }

    #[test]
    fn resolve_subscribe_empty_topic_refuses() {
        let peer = PeerId::new();
        let err = AircEventTransport::resolve_subscribe(peer, "", None)
            .expect_err("empty topic must be refused");
        assert!(
            err.contains("topic must not be empty"),
            "error must name the missing piece: {err}"
        );
        assert!(
            err.contains("[[no-fallbacks-ever]]"),
            "error must cite the doctrine: {err}"
        );
    }

    // ─── resolve_unsubscribe ─────────────────────────────────────────

    #[test]
    fn resolve_unsubscribe_packages_subscription_id_in_body_and_header() {
        let peer = PeerId::new();
        let sub_id = Uuid::new_v4();
        let (target, headers, body) =
            AircEventTransport::resolve_unsubscribe(peer, sub_id).expect("happy");
        assert!(matches!(target, MentionTarget::Peer(p) if p == peer));
        assert_eq!(
            headers.get(HEADER_EVENT_KIND).map(String::as_str),
            Some("unsubscribe")
        );
        assert_eq!(
            headers
                .get(HEADER_EVENT_SUBSCRIPTION_ID)
                .map(String::as_str),
            Some(sub_id.to_string().as_str())
        );
        assert_eq!(
            headers.get(HEADER_CONTINUUM_BODY_HINT).map(String::as_str),
            Some(EVENT_UNSUBSCRIBE_BODY_HINT)
        );
        match body {
            Body::Json(v) => {
                let back: AircEventUnsubscribe = serde_json::from_value(v).unwrap();
                assert_eq!(back.subscription_id, sub_id);
            }
            other => panic!("expected Json body, got {other:?}"),
        }
    }

    // ─── decode_subscribe_ack ────────────────────────────────────────

    #[test]
    fn decode_subscribe_ack_round_trips() {
        let ack = AircEventSubscribeAck {
            subscription_id: Uuid::new_v4(),
            topic: "x/y".into(),
        };
        let body = Body::Json(serde_json::to_value(&ack).unwrap());
        let back = AircEventTransport::decode_subscribe_ack(Some(body)).expect("decode");
        assert_eq!(back, ack);
    }

    #[test]
    fn decode_subscribe_ack_refuses_missing_body() {
        let err = AircEventTransport::decode_subscribe_ack(None).expect_err("None body must fail");
        assert!(
            err.contains("no body"),
            "must name the missing piece: {err}"
        );
    }

    #[test]
    fn decode_subscribe_ack_refuses_binary_body() {
        let err = AircEventTransport::decode_subscribe_ack(Some(Body::Binary(vec![1, 2, 3])))
            .expect_err("Binary body must fail");
        assert!(
            err.contains("Binary"),
            "must name the shape mismatch: {err}"
        );
    }

    #[test]
    fn decode_subscribe_ack_refuses_malformed_json() {
        let body = Body::Json(serde_json::json!({"wrong": "shape"}));
        let err = AircEventTransport::decode_subscribe_ack(Some(body))
            .expect_err("malformed JSON must fail");
        assert!(
            err.contains("deserialize"),
            "must name decode failure: {err}"
        );
    }

    // ─── decode_unsubscribe_ack ─────────────────────────────────────

    // Reviewer 1 BLOCK 4: decode_unsubscribe_ack's three typed
    // rejection branches were untested while decode_subscribe_ack's
    // were. Mirror coverage so the symmetric promise is paid.

    #[test]
    fn decode_unsubscribe_ack_refuses_missing_body() {
        let err =
            AircEventTransport::decode_unsubscribe_ack(None).expect_err("None body must fail");
        assert!(
            err.contains("no body"),
            "must name the missing piece: {err}"
        );
    }

    #[test]
    fn decode_unsubscribe_ack_refuses_binary_body() {
        let err = AircEventTransport::decode_unsubscribe_ack(Some(Body::Binary(vec![1, 2, 3])))
            .expect_err("Binary body must fail");
        assert!(
            err.contains("Binary"),
            "must name the shape mismatch: {err}"
        );
    }

    #[test]
    fn decode_unsubscribe_ack_refuses_malformed_json() {
        let body = Body::Json(serde_json::json!({"wrong": "shape"}));
        let err = AircEventTransport::decode_unsubscribe_ack(Some(body))
            .expect_err("malformed JSON must fail");
        assert!(
            err.contains("deserialize"),
            "must name decode failure: {err}"
        );
    }

    #[test]
    fn decode_unsubscribe_ack_round_trips_active() {
        let ack = AircEventUnsubscribeAck {
            subscription_id: Uuid::new_v4(),
            closed: true,
        };
        let body = Body::Json(serde_json::to_value(&ack).unwrap());
        let back = AircEventTransport::decode_unsubscribe_ack(Some(body)).expect("decode");
        assert_eq!(back, ack);
        assert!(back.closed);
    }

    #[test]
    fn decode_unsubscribe_ack_round_trips_idempotent() {
        // closed=false signals the peer already lost the subscription
        // (idempotent unsubscribe). Pin that the wire shape preserves
        // this — production callers branch on `closed` for telemetry.
        let ack = AircEventUnsubscribeAck {
            subscription_id: Uuid::new_v4(),
            closed: false,
        };
        let body = Body::Json(serde_json::to_value(&ack).unwrap());
        let back = AircEventTransport::decode_unsubscribe_ack(Some(body)).expect("decode");
        assert!(!back.closed);
    }

    // ─── decode_deliver_frame ───────────────────────────────────────

    /// Build a Deliver-shaped TranscriptEvent. `sender` populates
    /// the event's `peer_id` field — what airc would have signed
    /// as the verified source on a real wire. Tests that want a
    /// specific publisher pass it explicitly; tests that don't
    /// care (decode-only) pass `PeerId::new()`.
    fn make_deliver_event(
        sender: PeerId,
        deliver: &AircEventDeliver,
        body_hint: &str,
        sub_id_header: Option<String>,
    ) -> TranscriptEvent {
        let body_value = serde_json::to_value(deliver).expect("serialize");
        let mut headers = airc_core::Headers::new();
        headers.insert(
            HEADER_CONTINUUM_BODY_HINT.to_string(),
            body_hint.to_string(),
        );
        if let Some(id) = sub_id_header {
            headers.insert(HEADER_EVENT_SUBSCRIPTION_ID.to_string(), id);
        }
        TranscriptEvent {
            event_id: EventId::new(),
            room_id: RoomId::new(),
            peer_id: sender,
            client_id: ClientId::new(),
            kind: TranscriptKind::Message,
            occurred_at_ms: 1_700_000_000,
            lamport: 1,
            target: MentionTarget::All,
            headers,
            body: Some(Body::Json(body_value)),
            attachment: None,
            receipt: None,
            metadata: Value::Null,
        }
    }

    #[test]
    fn decode_deliver_frame_round_trips_a_real_deliver() {
        let deliver = AircEventDeliver {
            subscription_id: Uuid::new_v4(),
            topic: "cognition/analyze/complete".into(),
            sequence: 7,
            payload: serde_json::json!({"confidence": 0.84}),
        };
        let event = make_deliver_event(
            PeerId::new(),
            &deliver,
            EVENT_DELIVER_BODY_HINT,
            Some(deliver.subscription_id.to_string()),
        );
        let back = AircEventTransport::decode_deliver_frame(&event).expect("decode");
        assert_eq!(back, deliver);
    }

    #[test]
    fn decode_deliver_frame_refuses_missing_body() {
        let deliver = AircEventDeliver {
            subscription_id: Uuid::new_v4(),
            topic: "x".into(),
            sequence: 0,
            payload: Value::Null,
        };
        let mut event = make_deliver_event(PeerId::new(), &deliver, EVENT_DELIVER_BODY_HINT, None);
        event.body = None;
        let err = AircEventTransport::decode_deliver_frame(&event).expect_err("None body");
        assert!(err.contains("no body"), "must name missing piece: {err}");
    }

    #[test]
    fn decode_deliver_frame_refuses_binary_body() {
        let deliver = AircEventDeliver {
            subscription_id: Uuid::new_v4(),
            topic: "x".into(),
            sequence: 0,
            payload: Value::Null,
        };
        let mut event = make_deliver_event(PeerId::new(), &deliver, EVENT_DELIVER_BODY_HINT, None);
        event.body = Some(Body::Binary(vec![1, 2]));
        let err = AircEventTransport::decode_deliver_frame(&event).expect_err("Binary body");
        assert!(err.contains("Binary"), "must name the shape: {err}");
    }

    // ─── matches_subscription ───────────────────────────────────────

    #[test]
    fn matches_subscription_yes_for_matching_id_body_hint_and_publisher() {
        let sub_id = Uuid::new_v4();
        let publisher = PeerId::new();
        let deliver = AircEventDeliver {
            subscription_id: sub_id,
            topic: "x".into(),
            sequence: 0,
            payload: Value::Null,
        };
        let event = make_deliver_event(
            publisher,
            &deliver,
            EVENT_DELIVER_BODY_HINT,
            Some(sub_id.to_string()),
        );
        assert!(AircEventTransport::matches_subscription(
            &event, sub_id, publisher
        ));
    }

    #[test]
    fn matches_subscription_no_for_wrong_subscription_id() {
        let sub_id_a = Uuid::new_v4();
        let sub_id_b = Uuid::new_v4();
        let publisher = PeerId::new();
        let deliver = AircEventDeliver {
            subscription_id: sub_id_a,
            topic: "x".into(),
            sequence: 0,
            payload: Value::Null,
        };
        let event = make_deliver_event(
            publisher,
            &deliver,
            EVENT_DELIVER_BODY_HINT,
            Some(sub_id_a.to_string()),
        );
        // matching against a different subscription
        assert!(!AircEventTransport::matches_subscription(
            &event, sub_id_b, publisher
        ));
    }

    #[test]
    fn matches_subscription_no_for_non_deliver_body_hint() {
        // A command-request envelope is on the same airc stream as
        // deliver frames. The predicate must drop it.
        let sub_id = Uuid::new_v4();
        let publisher = PeerId::new();
        let deliver = AircEventDeliver {
            subscription_id: sub_id,
            topic: "x".into(),
            sequence: 0,
            payload: Value::Null,
        };
        let event = make_deliver_event(
            publisher,
            &deliver,
            "continuum.command.request.v1", // wrong hint
            Some(sub_id.to_string()),
        );
        assert!(!AircEventTransport::matches_subscription(
            &event, sub_id, publisher
        ));
    }

    #[test]
    fn matches_subscription_no_for_missing_subscription_id_header() {
        // A Deliver frame without the header is malformed — should
        // not match anyone. (decode_deliver_frame would also reject
        // it; matches_subscription is the cheap pre-filter.)
        let sub_id = Uuid::new_v4();
        let publisher = PeerId::new();
        let deliver = AircEventDeliver {
            subscription_id: sub_id,
            topic: "x".into(),
            sequence: 0,
            payload: Value::Null,
        };
        let event = make_deliver_event(publisher, &deliver, EVENT_DELIVER_BODY_HINT, None);
        assert!(!AircEventTransport::matches_subscription(
            &event, sub_id, publisher
        ));
    }

    /// PR #1529 reviewer 2 BLOCK 2: Deliver-frame forgery defense.
    /// A Deliver frame from a DIFFERENT peer than the one we
    /// subscribed to must be rejected even if every other header
    /// matches. Closes the room-broadcast forgery vector where
    /// any room peer could re-stamp matching `subscription_id` +
    /// `body_hint` headers on a forged frame.
    #[test]
    fn matches_subscription_no_for_wrong_publisher_forgery_defense() {
        let sub_id = Uuid::new_v4();
        let expected_publisher = PeerId::new();
        let forger = PeerId::new();
        assert_ne!(expected_publisher, forger, "test setup: distinct peers");

        let deliver = AircEventDeliver {
            subscription_id: sub_id,
            topic: "x".into(),
            sequence: 0,
            payload: Value::Null,
        };
        // Forger stamps every header correctly + uses the right
        // subscription_id. The ONLY thing that distinguishes is
        // the verified airc sender (event.peer_id), which the
        // forger cannot spoof.
        let forged_event = make_deliver_event(
            forger,
            &deliver,
            EVENT_DELIVER_BODY_HINT,
            Some(sub_id.to_string()),
        );
        assert!(
            !AircEventTransport::matches_subscription(&forged_event, sub_id, expected_publisher),
            "forged Deliver frame from non-publisher peer must be rejected — \
             this is the [[no-fallbacks-ever]] + reviewer 2 BLOCK 2 contract"
        );
    }
}
