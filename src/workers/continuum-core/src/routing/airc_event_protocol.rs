//! The substrate's wire shape for cross-grid event subscription
//! over airc. The event-side parallel of
//! [`AircCommandProtocol`](super::airc_command_protocol).
//!
//! Per [[events-are-the-organic-rtos-substrate]]: commands and
//! events are two **temporal shapes** of the same URI-addressable
//! coordination primitive. Slice P shipped the command shape; this
//! module ships the typed wire envelopes for the event shape that
//! the subsequent AircEventTransport (next commit) and peer-side
//! event publisher (commit after) consume.
//!
//! ## The three-message protocol
//!
//! Event subscription is a **3-message** dance on the wire:
//!
//! 1. **Subscribe** — caller dispatches
//!    `airc://<peer>/events/<topic>/subscribe`. The transport
//!    packages an [`AircEventSubscribe`] envelope and uses
//!    `Airc::request` (same shape as commands). The peer-side
//!    publisher acks with [`AircEventSubscribeAck`] carrying a
//!    `subscription_id` UUID.
//!
//! 2. **Deliver** — the peer publishes [`AircEventDeliver`] frames
//!    to the caller as events fire. Each frame carries the
//!    `subscription_id` so the caller's stream demultiplexes
//!    correctly when multiple subscriptions are active. Sequence
//!    numbers let the caller detect drops.
//!
//! 3. **Unsubscribe** — caller dispatches
//!    `airc://<peer>/events/<topic>/unsubscribe` with
//!    [`AircEventUnsubscribe`]. The peer-side publisher tears
//!    down the subscription. Acked with [`AircEventUnsubscribeAck`].
//!
//! ## Why three messages, not pub/sub directly on airc rooms
//!
//! airc's `Airc::subscribe()` already exists as a room-scoped
//! live stream — the personas use it for chat. But room-scoped
//! subscriptions don't address the substrate's needs:
//!
//! - **Typed topics per URI** — the substrate emits at
//!   `airc://maya/cognition/analyze/complete`, not at a chat
//!   room. The topic-URI-namespace is what makes the
//!   substrate composable.
//! - **Server-side filtering** — the caller might subscribe to
//!   `events/cognition/analyze/complete` with a confidence
//!   threshold; the peer filters before sending so the wire
//!   doesn't carry every event.
//! - **Per-subscription handles** — the caller might be
//!   subscribed to N topics concurrently; each gets its own
//!   `subscription_id` so unsubscribe is targeted.
//! - **Sequence + drop detection** — `Airc::subscribe` doesn't
//!   carry per-subscription sequence numbers; the event
//!   protocol does.
//!
//! airc's underlying transport (the LAN/grid/relay wire) carries
//! these messages; the protocol shape is the substrate's contract
//! on top.
//!
//! ## Header constants
//!
//! Three new headers, mirroring the command protocol:
//!
//! - [`HEADER_EVENT_TOPIC`] — the URI path being subscribed to
//!   (e.g. `"events/cognition/analyze/complete"`)
//! - [`HEADER_EVENT_KIND`] — `"subscribe" | "deliver" |
//!   "unsubscribe" | "ack"` — lets middleware dispatch without
//!   parsing the body
//! - [`HEADER_EVENT_SUBSCRIPTION_ID`] — the per-subscription
//!   UUID, present on Deliver and Unsubscribe frames
//!
//! Plus the body hints:
//!
//! - [`EVENT_SUBSCRIBE_BODY_HINT`] = `"continuum.event.subscribe.v1"`
//! - [`EVENT_DELIVER_BODY_HINT`] = `"continuum.event.deliver.v1"`
//! - [`EVENT_UNSUBSCRIBE_BODY_HINT`] = `"continuum.event.unsubscribe.v1"`
//! - [`EVENT_ACK_BODY_HINT`] = `"continuum.event.ack.v1"`
//!
//! ## Body shape
//!
//! All bodies are JSON. Same rationale as the command protocol:
//! inspectable, ts-rs-friendly, aligns with the cross-runtime
//! widget story.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

// ─── Header constants ────────────────────────────────────────────────

/// airc header naming the event URI topic the message refers to.
/// On subscribe + unsubscribe + deliver. Lets middleware filter or
/// route by topic without parsing the body.
pub const HEADER_EVENT_TOPIC: &str = "continuum.event.topic";

/// airc header naming the message kind:
/// `"subscribe" | "deliver" | "unsubscribe" | "ack"`. Lets
/// middleware dispatch without parsing the body.
pub const HEADER_EVENT_KIND: &str = "continuum.event.kind";

/// airc header carrying the per-subscription UUID. Present on
/// `deliver` and `unsubscribe` frames. On `ack`, it's the id the
/// peer minted in response to the matching subscribe.
pub const HEADER_EVENT_SUBSCRIPTION_ID: &str = "continuum.event.subscription_id";

/// Body hint for subscribe-request envelopes.
pub const EVENT_SUBSCRIBE_BODY_HINT: &str = "continuum.event.subscribe.v1";

/// Body hint for delivery (event payload) envelopes.
pub const EVENT_DELIVER_BODY_HINT: &str = "continuum.event.deliver.v1";

/// Body hint for unsubscribe-request envelopes.
pub const EVENT_UNSUBSCRIBE_BODY_HINT: &str = "continuum.event.unsubscribe.v1";

/// Body hint for ack envelopes (subscribe-ack and unsubscribe-ack
/// share this).
pub const EVENT_ACK_BODY_HINT: &str = "continuum.event.ack.v1";

// ─── Typed envelopes ─────────────────────────────────────────────────

/// The caller-side subscribe-request envelope.
///
/// `topic` is the URI path (e.g.
/// `"cognition/analyze/complete"`) the subscriber wants events for.
/// `filter` is an arbitrary JSON predicate the peer evaluates
/// against each event payload before sending. `None` means "send
/// everything matching the topic."
///
/// The peer-side publisher matches the topic against its
/// registered event sources (typically per-persona cognition stage
/// emissions per [[addressable-cognition-makes-triggers-trivial]])
/// and arms a delivery stream.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AircEventSubscribe {
    pub topic: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub filter: Option<Value>,
}

/// The peer-side ack to a subscribe request. Carries the
/// `subscription_id` the caller uses to demultiplex deliveries
/// and to issue the eventual unsubscribe.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AircEventSubscribeAck {
    pub subscription_id: Uuid,
    pub topic: String,
}

/// A single event delivery from peer to subscriber.
///
/// `subscription_id` demuxes when the caller holds multiple
/// subscriptions. `sequence` is per-subscription monotonic; the
/// caller can detect drops by gaps. `payload` is the typed event
/// data (whatever the source URI's event type is).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AircEventDeliver {
    pub subscription_id: Uuid,
    pub topic: String,
    pub sequence: u64,
    pub payload: Value,
}

/// The caller-side unsubscribe request. Carries the
/// `subscription_id` minted by the original subscribe-ack.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AircEventUnsubscribe {
    pub subscription_id: Uuid,
}

/// The peer-side ack to an unsubscribe request. `closed: true`
/// when the subscription was active and is now torn down;
/// `closed: false` when the id was already gone (idempotent
/// unsubscribe — same shape as `data/query-close`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AircEventUnsubscribeAck {
    pub subscription_id: Uuid,
    pub closed: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subscribe_round_trips_json() {
        let req = AircEventSubscribe {
            topic: "cognition/analyze/complete".into(),
            filter: Some(serde_json::json!({"min_confidence": 0.6})),
        };
        let json = serde_json::to_string(&req).expect("serialize");
        let back: AircEventSubscribe = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, req);
    }

    #[test]
    fn subscribe_omits_filter_when_none() {
        let req = AircEventSubscribe {
            topic: "any".into(),
            filter: None,
        };
        let json = serde_json::to_string(&req).expect("serialize");
        assert!(
            !json.contains("\"filter\""),
            "None filter should be skipped on the wire, got: {json}"
        );
        let back: AircEventSubscribe = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.filter, None);
    }

    #[test]
    fn subscribe_ack_round_trips() {
        let ack = AircEventSubscribeAck {
            subscription_id: Uuid::new_v4(),
            topic: "events/grid/peer/connected".into(),
        };
        let json = serde_json::to_string(&ack).expect("serialize");
        let back: AircEventSubscribeAck = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, ack);
    }

    #[test]
    fn deliver_round_trips() {
        let deliver = AircEventDeliver {
            subscription_id: Uuid::new_v4(),
            topic: "cognition/score/persona-scored".into(),
            sequence: 42,
            payload: serde_json::json!({"score": 0.91, "why": "auth expert"}),
        };
        let json = serde_json::to_string(&deliver).expect("serialize");
        let back: AircEventDeliver = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, deliver);
    }

    #[test]
    fn unsubscribe_round_trips() {
        let req = AircEventUnsubscribe {
            subscription_id: Uuid::new_v4(),
        };
        let json = serde_json::to_string(&req).expect("serialize");
        let back: AircEventUnsubscribe = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, req);
    }

    #[test]
    fn unsubscribe_ack_round_trips_active() {
        let ack = AircEventUnsubscribeAck {
            subscription_id: Uuid::new_v4(),
            closed: true,
        };
        let json = serde_json::to_string(&ack).expect("serialize");
        let back: AircEventUnsubscribeAck = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, ack);
    }

    #[test]
    fn unsubscribe_ack_round_trips_idempotent() {
        // closed=false signals "already gone" — idempotent
        // unsubscribe semantics matching data/query-close.
        let ack = AircEventUnsubscribeAck {
            subscription_id: Uuid::new_v4(),
            closed: false,
        };
        let json = serde_json::to_string(&ack).expect("serialize");
        let back: AircEventUnsubscribeAck = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, ack);
    }

    #[test]
    fn deliver_sequence_distinguishes_drops_per_subscription() {
        // Two Deliver frames with the same subscription_id and
        // increasing sequence numbers should round-trip
        // independently. The caller's stream demuxer detects drops
        // by sequence gaps.
        let sub_id = Uuid::new_v4();
        let topic = "x/y";

        let d1 = AircEventDeliver {
            subscription_id: sub_id,
            topic: topic.into(),
            sequence: 100,
            payload: serde_json::json!({"n": 1}),
        };
        let d2 = AircEventDeliver {
            subscription_id: sub_id,
            topic: topic.into(),
            sequence: 102, // gap — sequence 101 was dropped
            payload: serde_json::json!({"n": 2}),
        };

        let j1 = serde_json::to_string(&d1).unwrap();
        let j2 = serde_json::to_string(&d2).unwrap();
        let b1: AircEventDeliver = serde_json::from_str(&j1).unwrap();
        let b2: AircEventDeliver = serde_json::from_str(&j2).unwrap();
        assert_eq!(b1, d1);
        assert_eq!(b2, d2);
        // The drop is detectable at the consumer layer:
        assert_eq!(b2.sequence - b1.sequence, 2); // gap of 1
    }

    /// Headers are constants — pin them so a silent rename breaks
    /// the wire-test loudly. The peer-side publisher and middleware
    /// filter on these names; drift breaks the wire.
    #[test]
    fn header_names_are_stable_strings() {
        assert_eq!(HEADER_EVENT_TOPIC, "continuum.event.topic");
        assert_eq!(HEADER_EVENT_KIND, "continuum.event.kind");
        assert_eq!(
            HEADER_EVENT_SUBSCRIPTION_ID,
            "continuum.event.subscription_id"
        );
        assert_eq!(EVENT_SUBSCRIBE_BODY_HINT, "continuum.event.subscribe.v1");
        assert_eq!(EVENT_DELIVER_BODY_HINT, "continuum.event.deliver.v1");
        assert_eq!(EVENT_UNSUBSCRIBE_BODY_HINT, "continuum.event.unsubscribe.v1");
        assert_eq!(EVENT_ACK_BODY_HINT, "continuum.event.ack.v1");
    }
}
