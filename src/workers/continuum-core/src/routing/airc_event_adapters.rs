//! `EventSubscribeAdapter` + `EventUnsubscribeAdapter` — the
//! ConsumerAdapter shells that wire
//! [`EventPublisherState`](super::EventPublisherState) into the
//! airc adapter registry.
//!
//! These are thin shells around the pure functions in
//! [`super::airc_event_publisher`]: parse the envelope via the
//! relevant pure function, delegate to the state machine, build
//! the ack via the relevant pure function, reply via
//! `Airc::reply`. The interesting logic lives in the pure
//! functions; the adapter is the wire-binding glue.
//!
//! ## Why two adapters, not one
//!
//! The airc adapter registry routes by `body_hint`. Subscribe and
//! unsubscribe have distinct body hints
//! ([`EVENT_SUBSCRIBE_BODY_HINT`] vs
//! [`EVENT_UNSUBSCRIBE_BODY_HINT`]) so each needs its own
//! ConsumerAdapter registration. They share an
//! `Arc<EventPublisherState>` so unsubscribe targets the same
//! registry subscribe populated.
//!
//! ## Composition at boot
//!
//! ```ignore
//! use std::sync::Arc;
//! use continuum_core::routing::{
//!     EventPublisherState, EventSubscribeAdapter, EventUnsubscribeAdapter,
//! };
//!
//! let state = Arc::new(EventPublisherState::new());
//! let subscribe = EventSubscribeAdapter::new(airc.clone(), state.clone());
//! let unsubscribe = EventUnsubscribeAdapter::new(airc.clone(), state.clone());
//! airc.register_consumer_adapter(subscribe).await?;
//! airc.register_consumer_adapter(unsubscribe).await?;
//! ```
//!
//! After registration, every inbound
//! `airc://<this-peer>/events/<topic>/subscribe` flows through
//! `EventSubscribeAdapter` → `EventPublisherState::register` →
//! ack reply. Mirrors `CommandRequestHandler`'s wire-binding
//! exactly.

use std::sync::Arc;

use airc_core::{Body, Headers, TranscriptEvent};
use airc_lib::adapter::{AdapterError, ConsumerAdapter};
use airc_lib::Airc;
use async_trait::async_trait;

use super::airc_event_publisher::{
    build_subscribe_ack, build_unsubscribe_ack, parse_subscribe_envelope,
    parse_unsubscribe_envelope, EventPublisherState, ParsedSubscribe, ParsedUnsubscribe,
};
use super::{EVENT_SUBSCRIBE_BODY_HINT, EVENT_UNSUBSCRIBE_BODY_HINT};

/// Stable adapter name for the subscribe path.
pub const SUBSCRIBE_ADAPTER_NAME: &str = "continuum.event.subscribe";

/// Stable adapter name for the unsubscribe path.
pub const UNSUBSCRIBE_ADAPTER_NAME: &str = "continuum.event.unsubscribe";

/// ConsumerAdapter for the subscribe path. Registered with the
/// airc adapter registry as claiming
/// [`EVENT_SUBSCRIBE_BODY_HINT`].
pub struct EventSubscribeAdapter {
    airc: Arc<Airc>,
    state: Arc<EventPublisherState>,
}

/// ConsumerAdapter for the unsubscribe path. Registered with the
/// airc adapter registry as claiming
/// [`EVENT_UNSUBSCRIBE_BODY_HINT`].
pub struct EventUnsubscribeAdapter {
    airc: Arc<Airc>,
    state: Arc<EventPublisherState>,
}

impl EventSubscribeAdapter {
    /// Build a subscribe adapter against an existing airc handle
    /// + shared state. Returns `Arc<Self>` because the airc
    /// adapter registry stores adapters as
    /// `Arc<dyn ConsumerAdapter>`.
    pub fn new(airc: Arc<Airc>, state: Arc<EventPublisherState>) -> Arc<Self> {
        Arc::new(Self { airc, state })
    }

    /// Process a parsed subscribe envelope: register in state,
    /// build the ack. Pure function — exposed `pub` so tests can
    /// drive it without going through airc.
    ///
    /// Returns the ack envelope `(Headers, Body)` ready to send
    /// via `Airc::reply`, OR a typed error if state registration
    /// refused.
    pub fn process_subscribe(
        state: &EventPublisherState,
        parsed: &ParsedSubscribe,
    ) -> Result<(Headers, Body), AdapterError> {
        let subscription_id = state
            .register(
                parsed.caller_peer_id,
                parsed.request.topic.clone(),
                parsed.request.filter.clone(),
            )
            .map_err(AdapterError::Consumer)?;

        build_subscribe_ack(subscription_id, &parsed.request.topic)
            .map_err(|e| AdapterError::Consumer(format!("build_subscribe_ack: {e}")))
    }
}

impl EventUnsubscribeAdapter {
    /// Build an unsubscribe adapter against an existing airc
    /// handle + shared state.
    pub fn new(airc: Arc<Airc>, state: Arc<EventPublisherState>) -> Arc<Self> {
        Arc::new(Self { airc, state })
    }

    /// Process a parsed unsubscribe envelope: tear down in state,
    /// build the ack. Pure function — exposed `pub` so tests can
    /// drive it without going through airc.
    ///
    /// Returns the ack envelope `(Headers, Body)` ready to send
    /// via `Airc::reply`. Never errors — `unregister` returns
    /// `bool` (closed=true means active, false means already
    /// gone, both are valid idempotent outcomes).
    pub fn process_unsubscribe(
        state: &EventPublisherState,
        parsed: &ParsedUnsubscribe,
    ) -> Result<(Headers, Body), AdapterError> {
        let closed = state.unregister(parsed.request.subscription_id);
        build_unsubscribe_ack(parsed.request.subscription_id, closed)
            .map_err(|e| AdapterError::Consumer(format!("build_unsubscribe_ack: {e}")))
    }
}

#[async_trait]
impl ConsumerAdapter for EventSubscribeAdapter {
    fn name(&self) -> &'static str {
        SUBSCRIBE_ADAPTER_NAME
    }

    fn body_hint(&self) -> &'static str {
        EVENT_SUBSCRIBE_BODY_HINT
    }

    async fn on_envelope(&self, envelope: TranscriptEvent) -> Result<(), AdapterError> {
        let parsed = parse_subscribe_envelope(&envelope)?;
        let (headers, body) = Self::process_subscribe(&self.state, &parsed)?;
        self.airc
            .reply(parsed.reply_to, parsed.correlation_id, headers, body)
            .await
            .map_err(|e| AdapterError::Io(format!("airc reply (subscribe ack): {e}")))?;
        Ok(())
    }
}

#[async_trait]
impl ConsumerAdapter for EventUnsubscribeAdapter {
    fn name(&self) -> &'static str {
        UNSUBSCRIBE_ADAPTER_NAME
    }

    fn body_hint(&self) -> &'static str {
        EVENT_UNSUBSCRIBE_BODY_HINT
    }

    async fn on_envelope(&self, envelope: TranscriptEvent) -> Result<(), AdapterError> {
        let parsed = parse_unsubscribe_envelope(&envelope)?;
        let (headers, body) = Self::process_unsubscribe(&self.state, &parsed)?;
        self.airc
            .reply(parsed.reply_to, parsed.correlation_id, headers, body)
            .await
            .map_err(|e| AdapterError::Io(format!("airc reply (unsubscribe ack): {e}")))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::{
        AircEventSubscribe, AircEventSubscribeAck, AircEventUnsubscribe, AircEventUnsubscribeAck,
        HEADER_CONTINUUM_BODY_HINT, HEADER_EVENT_KIND, HEADER_EVENT_SUBSCRIPTION_ID,
        HEADER_EVENT_TOPIC, EVENT_ACK_BODY_HINT,
    };
    use airc_core::PeerId;
    use serde_json::Value;
    use uuid::Uuid;

    // ─── EventSubscribeAdapter::process_subscribe ────────────────────

    #[test]
    fn process_subscribe_registers_and_returns_ack_with_subscription_id() {
        let state = EventPublisherState::new();
        let caller = PeerId::new();
        let parsed = ParsedSubscribe {
            caller_peer_id: caller,
            reply_to: PeerId::new(),
            correlation_id: Uuid::new_v4(),
            request: AircEventSubscribe {
                topic: "x/y".into(),
                filter: None,
            },
        };

        let (headers, body) =
            EventSubscribeAdapter::process_subscribe(&state, &parsed).expect("subscribe");

        // State should have one registered subscription.
        assert_eq!(state.len(), 1);

        // Headers must be the ack shape.
        assert_eq!(headers.get(HEADER_EVENT_KIND).map(String::as_str), Some("ack"));
        assert_eq!(
            headers.get(HEADER_CONTINUUM_BODY_HINT).map(String::as_str),
            Some(EVENT_ACK_BODY_HINT)
        );
        let sub_id_header = headers
            .get(HEADER_EVENT_SUBSCRIPTION_ID)
            .expect("subscription_id header");
        let sub_id: Uuid = sub_id_header.parse().expect("uuid");

        // The id in the header must match the body.
        let value = match body {
            Body::Json(v) => v,
            other => panic!("expected Json, got {other:?}"),
        };
        let ack: AircEventSubscribeAck = serde_json::from_value(value).expect("decode");
        assert_eq!(ack.subscription_id, sub_id);
        assert_eq!(ack.topic, "x/y");

        // And the same id must be registered in the state.
        assert!(state.is_registered(sub_id));
    }

    #[test]
    fn process_subscribe_refuses_empty_topic_with_typed_error() {
        let state = EventPublisherState::new();
        let parsed = ParsedSubscribe {
            caller_peer_id: PeerId::new(),
            reply_to: PeerId::new(),
            correlation_id: Uuid::new_v4(),
            request: AircEventSubscribe {
                topic: "".into(),
                filter: None,
            },
        };

        let err = EventSubscribeAdapter::process_subscribe(&state, &parsed)
            .expect_err("empty topic must refuse");
        match err {
            AdapterError::Consumer(msg) => {
                assert!(
                    msg.contains("topic must not be empty"),
                    "error names the missing piece: {msg}"
                );
            }
            other => panic!("expected Consumer error, got {other:?}"),
        }

        // No subscription persisted on refusal.
        assert_eq!(state.len(), 0);
    }

    #[test]
    fn process_subscribe_threads_filter_into_registry() {
        // Pin that the filter from the request body actually
        // lands in the registry — a future refactor that drops
        // the filter mid-pipeline would silently break
        // server-side filtering. Cover the contract here.
        let state = EventPublisherState::new();
        let filter = serde_json::json!({"level": "info"});
        let parsed = ParsedSubscribe {
            caller_peer_id: PeerId::new(),
            reply_to: PeerId::new(),
            correlation_id: Uuid::new_v4(),
            request: AircEventSubscribe {
                topic: "events".into(),
                filter: Some(filter.clone()),
            },
        };

        let _ = EventSubscribeAdapter::process_subscribe(&state, &parsed).expect("subscribe");

        // lookup_matching with the info payload should match;
        // with the warn payload should not.
        let matched_info =
            state.lookup_matching("events", &serde_json::json!({"level": "info"}));
        assert_eq!(matched_info.len(), 1, "filter accepts info payload");

        let matched_warn =
            state.lookup_matching("events", &serde_json::json!({"level": "warn"}));
        assert_eq!(matched_warn.len(), 0, "filter rejects warn payload");
    }

    // ─── EventUnsubscribeAdapter::process_unsubscribe ────────────────

    #[test]
    fn process_unsubscribe_removes_active_subscription_and_acks_closed_true() {
        let state = EventPublisherState::new();
        // First, register a subscription via the subscribe path so
        // we have a real id to tear down.
        let subscribe = ParsedSubscribe {
            caller_peer_id: PeerId::new(),
            reply_to: PeerId::new(),
            correlation_id: Uuid::new_v4(),
            request: AircEventSubscribe {
                topic: "x/y".into(),
                filter: None,
            },
        };
        let (sub_headers, sub_body) =
            EventSubscribeAdapter::process_subscribe(&state, &subscribe).expect("subscribe");
        let sub_id_header = sub_headers
            .get(HEADER_EVENT_SUBSCRIPTION_ID)
            .expect("subscription_id");
        let sub_id: Uuid = sub_id_header.parse().expect("uuid");
        // sanity check
        let _: AircEventSubscribeAck = serde_json::from_value(match sub_body {
            Body::Json(v) => v,
            other => panic!("expected Json, got {other:?}"),
        })
        .expect("decode");

        assert!(state.is_registered(sub_id));

        // Now unsubscribe.
        let unsubscribe = ParsedUnsubscribe {
            caller_peer_id: PeerId::new(),
            reply_to: PeerId::new(),
            correlation_id: Uuid::new_v4(),
            request: AircEventUnsubscribe {
                subscription_id: sub_id,
            },
        };
        let (headers, body) =
            EventUnsubscribeAdapter::process_unsubscribe(&state, &unsubscribe).expect("unsub");

        assert_eq!(headers.get(HEADER_EVENT_KIND).map(String::as_str), Some("ack"));
        let ack: AircEventUnsubscribeAck = serde_json::from_value(match body {
            Body::Json(v) => v,
            other => panic!("expected Json, got {other:?}"),
        })
        .expect("decode");
        assert!(ack.closed, "active subscription unregistered → closed=true");
        assert_eq!(ack.subscription_id, sub_id);
        assert!(!state.is_registered(sub_id));
    }

    #[test]
    fn process_unsubscribe_unknown_id_returns_closed_false_idempotent() {
        let state = EventPublisherState::new();
        let unknown = Uuid::new_v4();
        let parsed = ParsedUnsubscribe {
            caller_peer_id: PeerId::new(),
            reply_to: PeerId::new(),
            correlation_id: Uuid::new_v4(),
            request: AircEventUnsubscribe {
                subscription_id: unknown,
            },
        };

        let (_headers, body) =
            EventUnsubscribeAdapter::process_unsubscribe(&state, &parsed).expect("unsub");

        let ack: AircEventUnsubscribeAck = serde_json::from_value(match body {
            Body::Json(v) => v,
            other => panic!("expected Json, got {other:?}"),
        })
        .expect("decode");
        assert!(
            !ack.closed,
            "unknown subscription_id → closed=false (idempotent ack contract)"
        );
        assert_eq!(ack.subscription_id, unknown);
    }

    // ─── adapter trait contract ──────────────────────────────────────

    #[test]
    fn adapter_names_and_body_hints_match_protocol_constants() {
        // Pin the wire contract. A silent refactor that renames a
        // body_hint constant must break this test — otherwise the
        // caller-side AircEventTransport (sending with one hint)
        // and the peer-side EventSubscribeAdapter (consuming with
        // the renamed hint) would silently disagree and events
        // would vanish.
        assert_eq!(SUBSCRIBE_ADAPTER_NAME, "continuum.event.subscribe");
        assert_eq!(UNSUBSCRIBE_ADAPTER_NAME, "continuum.event.unsubscribe");

        // The body_hint accessors must match the protocol-side
        // constants the caller-side AircEventTransport stamps.
        // We can't call body_hint() on these without instantiating
        // the adapter (which requires Arc<Airc>), so verify the
        // constants directly — same shape command_handler tests
        // use for the equivalent guard.
        assert_eq!(EVENT_SUBSCRIBE_BODY_HINT, "continuum.event.subscribe.v1");
        assert_eq!(
            EVENT_UNSUBSCRIBE_BODY_HINT,
            "continuum.event.unsubscribe.v1"
        );
    }

    // ─── shared state across adapters ────────────────────────────────

    #[test]
    fn subscribe_and_unsubscribe_adapters_target_same_registry_via_shared_state() {
        // The architectural property: both adapters must share an
        // Arc<EventPublisherState> so unsubscribe sees what
        // subscribe registered. Test the property by passing the
        // same state through both process_* helpers and asserting
        // observable side effects.
        let state = Arc::new(EventPublisherState::new());

        // Subscribe through one adapter's helper
        let subscribe = ParsedSubscribe {
            caller_peer_id: PeerId::new(),
            reply_to: PeerId::new(),
            correlation_id: Uuid::new_v4(),
            request: AircEventSubscribe {
                topic: "shared".into(),
                filter: None,
            },
        };
        let (sub_headers, _) =
            EventSubscribeAdapter::process_subscribe(&state, &subscribe).expect("subscribe");
        let sub_id: Uuid = sub_headers
            .get(HEADER_EVENT_SUBSCRIPTION_ID)
            .expect("sub_id")
            .parse()
            .expect("uuid");
        assert_eq!(state.len(), 1, "subscribe registered via state");

        // Unsubscribe through the OTHER adapter's helper but the
        // SAME state Arc must see the registration.
        let unsubscribe = ParsedUnsubscribe {
            caller_peer_id: PeerId::new(),
            reply_to: PeerId::new(),
            correlation_id: Uuid::new_v4(),
            request: AircEventUnsubscribe {
                subscription_id: sub_id,
            },
        };
        let (_headers, body) =
            EventUnsubscribeAdapter::process_unsubscribe(&state, &unsubscribe).expect("unsub");
        let ack: AircEventUnsubscribeAck = serde_json::from_value(match body {
            Body::Json(v) => v,
            other => panic!("expected Json, got {other:?}"),
        })
        .expect("decode");

        assert!(
            ack.closed,
            "unsubscribe through SibAdapter sees subscribe through SubAdapter — \
             shared state contract"
        );
        assert_eq!(state.len(), 0, "state cleaned up by unsubscribe");

        // Suppress unused-Value warning if topic ended up unused;
        // ensures the test won't bit-rot if we add fields.
        let _ = Value::Null;
    }
}
