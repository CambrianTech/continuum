//! Event protocol — typed wire envelopes for substrate event subscribe /
//! deliver / unsubscribe over airc. See module-level docs on
//! `routing::airc_event_protocol` in continuum-core for the full flow.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

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
    }
}
