//! Event protocol — typed wire envelopes AND pure helper functions for
//! the substrate event subscribe / deliver / unsubscribe flow over airc.
//!
//! Two consumers, one source of truth:
//!
//! - **continuum-core** (substrate) uses these helpers from its
//!   `routing::airc_event_transport::AircEventTransport` (caller-side
//!   cross-grid event subscription) and `airc_event_publisher`
//!   (peer-side fan-out).
//! - **continuum-client** uses the same helpers in its
//!   `AircIpcTransport::subscribe()` so the CLI + per-language SDKs
//!   speak the same wire shape as the substrate, no drift.
//!
//! See `core/continuum-core/src/routing/airc_event_protocol.rs` for the
//! full three-message flow doc.
//!
//! ## What's pure here vs what stays in the substrate
//!
//! This module owns:
//! - **Envelope structs** (`AircEventSubscribe`, `AircEventSubscribeAck`,
//!   `AircEventDeliver`, etc.) — typed wire shapes.
//! - **`resolve_subscribe` / `resolve_unsubscribe`** — build outbound
//!   (target, headers, body) for the airc request.
//! - **`decode_subscribe_ack` / `decode_unsubscribe_ack` /
//!   `decode_deliver_frame`** — typed parsing of inbound bodies.
//! - **`matches_subscription`** — pure predicate over a
//!   `TranscriptEvent` for per-subscription demux.
//!
//! The substrate-internal coupling — the `AircEventTransport` struct
//! itself, the per-subscription tokio task, `EventSubscription` handle,
//! `EventPublisherState` registry — stays in continuum-core. This
//! module is the wire shape; the substrate composes it with airc-lib
//! to drive the async I/O.

use airc_core::{Body, MentionTarget, PeerId, TranscriptEvent};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::command::HEADER_CONTINUUM_BODY_HINT;

// ─── Header constants ────────────────────────────────────────────────

/// airc header naming the event URI topic the message refers to.
pub const HEADER_EVENT_TOPIC: &str = "continuum.event.topic";

/// airc header naming the message kind: `"subscribe" | "deliver" |
/// "unsubscribe" | "ack"`.
pub const HEADER_EVENT_KIND: &str = "continuum.event.kind";

/// airc header carrying the per-subscription UUID.
pub const HEADER_EVENT_SUBSCRIPTION_ID: &str = "continuum.event.subscription_id";

/// Body-hint values for the four envelope kinds.
pub const EVENT_SUBSCRIBE_BODY_HINT: &str = "continuum.event.subscribe.v1";
pub const EVENT_DELIVER_BODY_HINT: &str = "continuum.event.deliver.v1";
pub const EVENT_UNSUBSCRIBE_BODY_HINT: &str = "continuum.event.unsubscribe.v1";
/// A client PUBLISHING an event into the substrate's fan-out — the publish twin
/// of subscribe (the `emit` half of the Event primitive).
pub const EVENT_PUBLISH_BODY_HINT: &str = "continuum.event.publish.v1";
pub const EVENT_ACK_BODY_HINT: &str = "continuum.event.ack.v1";

// ─── Typed envelopes ─────────────────────────────────────────────────

/// Caller-side subscribe-request envelope.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AircEventSubscribe {
    pub topic: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub filter: Option<Value>,
}

/// Peer-side ack to a subscribe request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AircEventSubscribeAck {
    pub subscription_id: Uuid,
    pub topic: String,
}

/// A single event delivery from peer to subscriber.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AircEventDeliver {
    pub subscription_id: Uuid,
    pub topic: String,
    pub sequence: u64,
    pub payload: Value,
}

/// Caller-side unsubscribe request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AircEventUnsubscribe {
    pub subscription_id: Uuid,
}

/// Peer-side ack to an unsubscribe.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AircEventUnsubscribeAck {
    pub subscription_id: Uuid,
    pub closed: bool,
}

/// Caller-side publish request — a client emitting an event into the
/// substrate's fan-out. The publish twin of [`AircEventSubscribe`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AircEventPublish {
    pub topic: String,
    pub payload: Value,
}

/// Peer-side ack to a publish: how many subscribers the event fanned out to.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AircEventPublishAck {
    pub topic: String,
    pub delivered: u64,
}

// ─── Pure helper functions ───────────────────────────────────────────
//
// Every refusal / decision branch is testable WITHOUT spinning up
// airc — same pattern PR #1529 reviewer 3 fix established for the
// substrate-side AircEventTransport. The substrate's transport and
// the client's AircIpcTransport BOTH compose these helpers with
// `airc_lib::Airc` for the async I/O, but the wire-shape decisions
// (what bytes go on the wire, which inbound shapes are accepted)
// live here so client and substrate cannot drift.

/// Build the outbound subscribe envelope.
///
/// Returns the airc-side `(target, headers, body)` tuple ready to
/// pass to `Airc::request()`. Refuses an empty topic upfront — the
/// peer-side publisher matches subscriptions by topic and an empty
/// topic would match nothing (or everything, depending on the
/// publisher's loop shape); either way silent. Per
/// `[[no-fallbacks-ever]]` the helper refuses upfront rather than
/// emitting a frame the peer will silently ignore.
pub fn resolve_subscribe(
    target_peer: PeerId,
    topic: &str,
    filter: Option<Value>,
) -> Result<(MentionTarget, airc_core::Headers, Body), String> {
    if topic.is_empty() {
        return Err(
            "airc event subscribe: topic must not be empty — the peer-side \
             publisher matches subscriptions by topic and an empty topic would \
             match nothing (or everything, depending on the publisher's loop \
             shape); either way silent. Per [[no-fallbacks-ever]] the transport \
             refuses upfront."
                .to_string(),
        );
    }
    let req = AircEventSubscribe {
        topic: topic.to_string(),
        filter,
    };
    let body_value = serde_json::to_value(&req)
        .map_err(|e| format!("airc event subscribe: serialize AircEventSubscribe to JSON: {e}"))?;
    let body = Body::Json(body_value);

    let mut headers = airc_core::Headers::new();
    headers.insert(HEADER_EVENT_TOPIC.to_string(), topic.to_string());
    headers.insert(HEADER_EVENT_KIND.to_string(), "subscribe".to_string());
    headers.insert(
        HEADER_CONTINUUM_BODY_HINT.to_string(),
        EVENT_SUBSCRIBE_BODY_HINT.to_string(),
    );

    Ok((MentionTarget::Peer(target_peer), headers, body))
}

/// Build the outbound unsubscribe envelope.
pub fn resolve_unsubscribe(
    target_peer: PeerId,
    subscription_id: Uuid,
) -> Result<(MentionTarget, airc_core::Headers, Body), String> {
    let req = AircEventUnsubscribe { subscription_id };
    let body_value = serde_json::to_value(&req).map_err(|e| {
        format!("airc event unsubscribe: serialize AircEventUnsubscribe to JSON: {e}")
    })?;
    let body = Body::Json(body_value);

    let mut headers = airc_core::Headers::new();
    headers.insert(HEADER_EVENT_KIND.to_string(), "unsubscribe".to_string());
    headers.insert(
        HEADER_EVENT_SUBSCRIPTION_ID.to_string(),
        subscription_id.to_string(),
    );
    headers.insert(
        HEADER_CONTINUUM_BODY_HINT.to_string(),
        EVENT_UNSUBSCRIBE_BODY_HINT.to_string(),
    );

    Ok((MentionTarget::Peer(target_peer), headers, body))
}

/// Build the outbound publish envelope — a client emitting `payload` to
/// `topic`. The publish twin of [`resolve_subscribe`]; refuses an empty topic
/// upfront for the same reason (the publisher matches by topic).
pub fn resolve_publish(
    target_peer: PeerId,
    topic: &str,
    payload: Value,
) -> Result<(MentionTarget, airc_core::Headers, Body), String> {
    if topic.is_empty() {
        return Err(
            "airc event publish: topic must not be empty — the peer-side publisher \
             fans out by topic and an empty topic would match nothing. Per \
             [[no-fallbacks-ever]] the transport refuses upfront."
                .to_string(),
        );
    }
    let req = AircEventPublish {
        topic: topic.to_string(),
        payload,
    };
    let body_value = serde_json::to_value(&req)
        .map_err(|e| format!("airc event publish: serialize AircEventPublish to JSON: {e}"))?;
    let body = Body::Json(body_value);

    let mut headers = airc_core::Headers::new();
    headers.insert(HEADER_EVENT_TOPIC.to_string(), topic.to_string());
    headers.insert(HEADER_EVENT_KIND.to_string(), "publish".to_string());
    headers.insert(
        HEADER_CONTINUUM_BODY_HINT.to_string(),
        EVENT_PUBLISH_BODY_HINT.to_string(),
    );

    Ok((MentionTarget::Peer(target_peer), headers, body))
}

/// Decode a publish-reply body as [`AircEventPublishAck`] (the fan-out count).
pub fn decode_publish_ack(reply_body: Option<Body>) -> Result<AircEventPublishAck, String> {
    let body = reply_body.ok_or_else(|| {
        "airc event publish: reply has no body (peer-side publisher must \
         attach Body::Json(AircEventPublishAck))"
            .to_string()
    })?;
    let value = match body {
        Body::Json(v) => v,
        Body::Binary(_) => {
            return Err("airc event publish: reply body was Binary; expected Json \
                 (AircEventPublishAck is a JSON envelope)"
                .to_string());
        }
    };
    serde_json::from_value(value)
        .map_err(|e| format!("airc event publish: deserialize reply as AircEventPublishAck: {e}"))
}

/// Decode a subscribe-reply body as [`AircEventSubscribeAck`].
///
/// Every error path (no body, binary body, malformed JSON) is
/// testable without airc.
pub fn decode_subscribe_ack(reply_body: Option<Body>) -> Result<AircEventSubscribeAck, String> {
    let body = reply_body.ok_or_else(|| {
        "airc event subscribe: reply has no body (peer-side publisher must \
         attach Body::Json(AircEventSubscribeAck))"
            .to_string()
    })?;
    let value = match body {
        Body::Json(v) => v,
        Body::Binary(_) => {
            return Err(
                "airc event subscribe: reply body was Binary; expected Json \
                 (AircEventSubscribeAck is a JSON envelope)"
                    .to_string(),
            );
        }
    };
    serde_json::from_value(value).map_err(|e| {
        format!("airc event subscribe: deserialize reply as AircEventSubscribeAck: {e}")
    })
}

/// Decode an unsubscribe-reply body.
pub fn decode_unsubscribe_ack(reply_body: Option<Body>) -> Result<AircEventUnsubscribeAck, String> {
    let body = reply_body.ok_or_else(|| {
        "airc event unsubscribe: reply has no body (peer-side publisher must \
         attach Body::Json(AircEventUnsubscribeAck))"
            .to_string()
    })?;
    let value = match body {
        Body::Json(v) => v,
        Body::Binary(_) => {
            return Err("airc event unsubscribe: reply body was Binary; expected Json".to_string());
        }
    };
    serde_json::from_value(value).map_err(|e| {
        format!("airc event unsubscribe: deserialize reply as AircEventUnsubscribeAck: {e}")
    })
}

/// Decode an inbound `TranscriptEvent` as an [`AircEventDeliver`]
/// frame. Returns a typed error if the event isn't a valid Deliver
/// (no body, binary body, malformed JSON).
///
/// The per-subscription filter loop calls this AFTER
/// [`matches_subscription`] has already cheaply rejected non-matching
/// frames, so this only runs on frames the caller actually wants.
pub fn decode_deliver_frame(event: &TranscriptEvent) -> Result<AircEventDeliver, String> {
    let body = event.body.as_ref().ok_or_else(|| {
        "airc event Deliver frame has no body (peer-side publisher must attach \
         Body::Json(AircEventDeliver))"
            .to_string()
    })?;
    let value = match body {
        Body::Json(v) => v.clone(),
        Body::Binary(_) => {
            return Err("airc event Deliver frame body was Binary; expected Json".to_string());
        }
    };
    serde_json::from_value(value)
        .map_err(|e| format!("airc event Deliver: deserialize as AircEventDeliver: {e}"))
}

/// Pure predicate: does this `TranscriptEvent` belong to the given
/// subscription AND come from the expected publisher?
///
/// Matches on (cheapest checks first):
///
/// 1. `event.peer_id == expected_publisher` — closes the forgery
///    vector (PR #1529 reviewer 2 BLOCK). The airc daemon has
///    already validated the signature on the sender's peer_id field;
///    we trust that, and we trust only the peer we explicitly
///    subscribed to. Without this check, any room peer could re-stamp
///    matching headers on a forged Deliver frame and inject it into
///    our cognition.
/// 2. `HEADER_CONTINUUM_BODY_HINT == EVENT_DELIVER_BODY_HINT` —
///    drops non-Deliver frames; the airc event stream carries chat,
///    status, command-side events, etc.
/// 3. `HEADER_EVENT_SUBSCRIPTION_ID == subscription_id` — the
///    subscription identity demux for callers holding N active
///    subscriptions concurrently.
///
/// Used by the per-subscription filter task to drop the vast majority
/// of inbound frames cheaply (one PeerId equality + one HashMap get +
/// one string compare) without parsing the body.
pub fn matches_subscription(
    event: &TranscriptEvent,
    subscription_id: Uuid,
    expected_publisher: PeerId,
) -> bool {
    if event.peer_id != expected_publisher {
        return false;
    }
    let body_hint_ok = event
        .headers
        .get(HEADER_CONTINUUM_BODY_HINT)
        .map(|s| s.as_str() == EVENT_DELIVER_BODY_HINT)
        .unwrap_or(false);
    if !body_hint_ok {
        return false;
    }
    event
        .headers
        .get(HEADER_EVENT_SUBSCRIPTION_ID)
        .and_then(|s| Uuid::parse_str(s).ok())
        .map(|id| id == subscription_id)
        .unwrap_or(false)
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
        let d = AircEventDeliver {
            subscription_id: Uuid::new_v4(),
            topic: "cognition/score/complete".into(),
            sequence: 42,
            payload: serde_json::json!({"verdict": "respond"}),
        };
        let json = serde_json::to_string(&d).expect("serialize");
        let back: AircEventDeliver = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, d);
    }

    #[test]
    fn unsubscribe_ack_round_trips() {
        let a = AircEventUnsubscribeAck {
            subscription_id: Uuid::new_v4(),
            closed: true,
        };
        let json = serde_json::to_string(&a).expect("serialize");
        let back: AircEventUnsubscribeAck = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, a);
    }

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
        assert_eq!(
            EVENT_UNSUBSCRIBE_BODY_HINT,
            "continuum.event.unsubscribe.v1"
        );
        assert_eq!(EVENT_ACK_BODY_HINT, "continuum.event.ack.v1");
        assert_eq!(EVENT_PUBLISH_BODY_HINT, "continuum.event.publish.v1");
    }

    #[test]
    fn publish_round_trips_json() {
        let req = AircEventPublish {
            topic: "cognition/analyze/complete".into(),
            payload: serde_json::json!({ "confidence": 0.9 }),
        };
        let json = serde_json::to_string(&req).expect("serialize");
        let back: AircEventPublish = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, req);
    }

    #[test]
    fn publish_ack_round_trips() {
        let ack = AircEventPublishAck {
            topic: "events/x".into(),
            delivered: 3,
        };
        let json = serde_json::to_string(&ack).expect("serialize");
        let back: AircEventPublishAck = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, ack);
    }

    #[test]
    fn resolve_publish_builds_envelope_with_publish_hint() {
        // what this catches: the publish envelope must carry the topic + the
        // publish body-hint so the substrate's EventPublishAdapter routes it (the
        // mirror of resolve_subscribe).
        let peer = PeerId::new();
        let (target, headers, body) =
            resolve_publish(peer, "events/x", serde_json::json!({ "k": 1 })).expect("resolves");
        assert_eq!(target, MentionTarget::Peer(peer));
        assert_eq!(
            headers.get(HEADER_CONTINUUM_BODY_HINT).map(String::as_str),
            Some(EVENT_PUBLISH_BODY_HINT)
        );
        assert_eq!(
            headers.get(HEADER_EVENT_KIND).map(String::as_str),
            Some("publish")
        );
        let Body::Json(v) = body else {
            panic!("publish body must be JSON")
        };
        let decoded: AircEventPublish = serde_json::from_value(v).expect("decode body");
        assert_eq!(decoded.topic, "events/x");
    }

    #[test]
    fn resolve_publish_refuses_empty_topic() {
        // what this catches: an empty topic fans out to nothing (silent) — refuse
        // upfront per no-fallbacks, same as resolve_subscribe.
        assert!(resolve_publish(PeerId::new(), "", serde_json::json!({})).is_err());
    }

    #[test]
    fn decode_publish_ack_round_trips_the_fanout_count() {
        let ack = AircEventPublishAck {
            topic: "events/x".into(),
            delivered: 7,
        };
        let body = Body::Json(serde_json::to_value(&ack).unwrap());
        let decoded = decode_publish_ack(Some(body)).expect("decodes");
        assert_eq!(decoded.delivered, 7);
    }
}
