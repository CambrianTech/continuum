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
    build_publish_ack, build_subscribe_ack, build_unsubscribe_ack, parse_publish_envelope,
    parse_subscribe_envelope, parse_unsubscribe_envelope, AircEventPublisher, EventPublisherState,
    ParsedPublish, ParsedSubscribe, ParsedUnsubscribe,
};
use super::{
    AllowAllPolicy, AuthPolicy, CallerIdentity, RouteDecision, Verdict, EVENT_PUBLISH_BODY_HINT,
    EVENT_SUBSCRIBE_BODY_HINT, EVENT_UNSUBSCRIBE_BODY_HINT,
};

/// Stable adapter name for the subscribe path.
pub const SUBSCRIBE_ADAPTER_NAME: &str = "continuum.event.subscribe";

/// Stable adapter name for the unsubscribe path.
pub const UNSUBSCRIBE_ADAPTER_NAME: &str = "continuum.event.unsubscribe";

/// Stable adapter name for the publish path.
pub const PUBLISH_ADAPTER_NAME: &str = "continuum.event.publish";

/// ConsumerAdapter for the subscribe path. Registered with the
/// airc adapter registry as claiming
/// [`EVENT_SUBSCRIBE_BODY_HINT`].
///
/// Carries an `Arc<dyn AuthPolicy>` so every inbound subscribe is
/// gated through the same substrate-wide policy chokepoint the
/// command path uses. Per PR #1529 reviewer 2 BLOCK 1: without
/// this, any peer that can reach this airc node could subscribe
/// to ANY topic (including internal substrate signals like
/// `cognition/score/persona-scored`), silently leaking cognition
/// telemetry. Defaults to `AllowAllPolicy` so the wire surface
/// works out of the box; operators install an ORM-backed or
/// capability-backed impl at boot via `with_policy()`.
pub struct EventSubscribeAdapter {
    airc: Arc<Airc>,
    state: Arc<EventPublisherState>,
    policy: Arc<dyn AuthPolicy>,
}

/// ConsumerAdapter for the unsubscribe path. Registered with the
/// airc adapter registry as claiming
/// [`EVENT_UNSUBSCRIBE_BODY_HINT`].
///
/// No policy on this side because unsubscribe is idempotent
/// (`closed: false` is the already-gone outcome) and we WANT
/// peers to be able to clean up their own subscriptions even if
/// the subscribe gate would refuse a fresh request. A peer
/// gaining temporary access then losing it must still be able to
/// stop publishing — gating unsubscribe would create stuck
/// registrations.
pub struct EventUnsubscribeAdapter {
    airc: Arc<Airc>,
    state: Arc<EventPublisherState>,
}

/// ConsumerAdapter for the publish path — the `emit` half of the Event
/// primitive. Registered with the airc adapter registry as claiming
/// [`EVENT_PUBLISH_BODY_HINT`].
///
/// Holds the [`AircEventPublisher`] (the fan-out engine over the shared
/// subscription registry) so an inbound publish reaches every matching
/// subscriber, and an `Arc<dyn AuthPolicy>` so every inbound publish is
/// gated through the same substrate-wide chokepoint subscribe uses.
///
/// Publish is a WRITE: unlike subscribe (which only registers the
/// caller's own interest), a publish MUTATES every subscriber's stream
/// by fanning an event out to it. Without the gate, any peer that can
/// reach this node could inject events onto ANY topic — including
/// internal substrate signals other personas act on. The gate runs on
/// the synthetic URI `events/<topic>/publish`, so operators author
/// publish policy with the same path-prefix matching as every other
/// surface.
pub struct EventPublishAdapter {
    airc: Arc<Airc>,
    publisher: Arc<AircEventPublisher>,
    policy: Arc<dyn AuthPolicy>,
}

impl EventSubscribeAdapter {
    /// Build a subscribe adapter against an existing airc handle
    /// + shared state, with [`AllowAllPolicy`] as the default
    /// auth gate. Mirrors `CommandExecutor::new` shape so the
    /// substrate composition reads uniformly across commands +
    /// events. Builder-style `with_policy` swaps the gate.
    pub fn new(airc: Arc<Airc>, state: Arc<EventPublisherState>) -> Arc<Self> {
        Arc::new(Self {
            airc,
            state,
            policy: Arc::new(AllowAllPolicy),
        })
    }

    /// Replace the auth policy. Operators wire their substrate
    /// gate here at boot. Returns `Arc<Self>` for chaining with
    /// the airc adapter registry.
    pub fn with_policy(
        airc: Arc<Airc>,
        state: Arc<EventPublisherState>,
        policy: Arc<dyn AuthPolicy>,
    ) -> Arc<Self> {
        Arc::new(Self {
            airc,
            state,
            policy,
        })
    }

    /// Process a parsed subscribe envelope: GATE the caller via
    /// AuthPolicy, then register in state, build the ack. Pure
    /// function — exposed `pub` so tests can drive it without
    /// going through airc.
    ///
    /// Returns the ack envelope `(Headers, Body)` ready to send
    /// via `Airc::reply`, OR a typed `AdapterError::Consumer` if
    /// the policy refused (`Forbidden`/`Deferred`) or state
    /// registration refused (e.g. empty topic).
    pub fn process_subscribe(
        state: &EventPublisherState,
        policy: &dyn AuthPolicy,
        parsed: &ParsedSubscribe,
    ) -> Result<(Headers, Body), AdapterError> {
        // PR #1529 reviewer 2 BLOCK 1: thread the verified airc
        // sender into the substrate's auth chokepoint BEFORE we
        // touch state. The synthetic URI for the decision is
        // `events/<topic>/subscribe` — policies match on path
        // prefixes the same way they do for commands, keeping the
        // gate authoring uniform across the two surfaces.
        let caller = CallerIdentity::airc(parsed.caller_peer_id);
        let decision = RouteDecision::Local {
            path: format!("events/{}/subscribe", parsed.request.topic),
            query: None,
            fragment: None,
        };
        match policy.gate(&decision, Some(&caller)) {
            Verdict::Allowed => {}
            Verdict::Forbidden { reason } => {
                return Err(AdapterError::Consumer(format!(
                    "EventSubscribeAdapter: forbidden by policy ({reason:?}) — \
                     caller peer={} topic={:?}",
                    parsed.caller_peer_id.0, parsed.request.topic
                )));
            }
            Verdict::Deferred {
                reason,
                prompt_target_env,
            } => {
                return Err(AdapterError::Consumer(format!(
                    "EventSubscribeAdapter: deferred by policy ({reason:?}, prompt_target_env={prompt_target_env:?}) — \
                     caller peer={} topic={:?}",
                    parsed.caller_peer_id.0, parsed.request.topic
                )));
            }
        }

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

impl EventPublishAdapter {
    /// Build a publish adapter against an existing airc handle +
    /// publisher, with [`AllowAllPolicy`] as the default gate. The
    /// publisher carries the shared `EventPublisherState`, so a publish
    /// fans out to exactly the subscriptions the subscribe adapter
    /// registered. Builder-style `with_policy` swaps the gate.
    pub fn new(airc: Arc<Airc>, publisher: Arc<AircEventPublisher>) -> Arc<Self> {
        Arc::new(Self {
            airc,
            publisher,
            policy: Arc::new(AllowAllPolicy),
        })
    }

    /// Replace the auth policy. Operators wire their substrate gate here
    /// at boot — the same `Arc<dyn AuthPolicy>` instance shared with the
    /// subscribe adapter, so read (subscribe) and write (publish) are
    /// governed by one policy.
    pub fn with_policy(
        airc: Arc<Airc>,
        publisher: Arc<AircEventPublisher>,
        policy: Arc<dyn AuthPolicy>,
    ) -> Arc<Self> {
        Arc::new(Self {
            airc,
            publisher,
            policy,
        })
    }

    /// Gate an inbound publish — the security seam, pure and exposed
    /// `pub` so the WRITE contract is testable without airc.
    ///
    /// Refuses an empty topic upfront (it would fan out to nothing —
    /// silent; refused per `[[no-fallbacks-ever]]`), then runs the
    /// caller through the AuthPolicy on the synthetic URI
    /// `events/<topic>/publish`. Returns `Ok(())` when allowed, or a
    /// typed `AdapterError::Consumer` when the topic is empty or the
    /// policy refuses (`Forbidden`/`Deferred`).
    pub fn gate_publish(
        policy: &dyn AuthPolicy,
        parsed: &ParsedPublish,
    ) -> Result<(), AdapterError> {
        if parsed.request.topic.is_empty() {
            return Err(AdapterError::Consumer(
                "EventPublishAdapter: topic must not be empty — an empty topic fans \
                 out to nothing; refuse upfront per [[no-fallbacks-ever]]"
                    .to_string(),
            ));
        }

        let caller = CallerIdentity::airc(parsed.caller_peer_id);
        let decision = RouteDecision::Local {
            path: format!("events/{}/publish", parsed.request.topic),
            query: None,
            fragment: None,
        };
        match policy.gate(&decision, Some(&caller)) {
            Verdict::Allowed => Ok(()),
            Verdict::Forbidden { reason } => Err(AdapterError::Consumer(format!(
                "EventPublishAdapter: forbidden by policy ({reason:?}) — \
                 caller peer={} topic={:?}",
                parsed.caller_peer_id.0, parsed.request.topic
            ))),
            Verdict::Deferred {
                reason,
                prompt_target_env,
            } => Err(AdapterError::Consumer(format!(
                "EventPublishAdapter: deferred by policy ({reason:?}, prompt_target_env={prompt_target_env:?}) — \
                 caller peer={} topic={:?}",
                parsed.caller_peer_id.0, parsed.request.topic
            ))),
        }
    }

    /// Gate, fan out, ack. Gated FIRST (no fan-out on a refused publish),
    /// then the payload is fanned to every matching subscriber, and the
    /// ack carries the delivered count back to the caller's `emit()`.
    async fn process_publish(
        &self,
        parsed: &ParsedPublish,
    ) -> Result<(Headers, Body), AdapterError> {
        Self::gate_publish(&*self.policy, parsed)?;

        let delivered = self
            .publisher
            .publish(&parsed.request.topic, parsed.request.payload.clone())
            .await
            .map_err(AdapterError::Consumer)?;

        build_publish_ack(&parsed.request.topic, delivered as u64)
            .map_err(|e| AdapterError::Consumer(format!("build_publish_ack: {e}")))
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
        let (headers, body) = Self::process_subscribe(&self.state, &*self.policy, &parsed)?;
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

#[async_trait]
impl ConsumerAdapter for EventPublishAdapter {
    fn name(&self) -> &'static str {
        PUBLISH_ADAPTER_NAME
    }

    fn body_hint(&self) -> &'static str {
        EVENT_PUBLISH_BODY_HINT
    }

    async fn on_envelope(&self, envelope: TranscriptEvent) -> Result<(), AdapterError> {
        let parsed = parse_publish_envelope(&envelope)?;
        let (headers, body) = self.process_publish(&parsed).await?;
        self.airc
            .reply(parsed.reply_to, parsed.correlation_id, headers, body)
            .await
            .map_err(|e| AdapterError::Io(format!("airc reply (publish ack): {e}")))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::{
        AircEventPublish, AircEventPublishAck, AircEventSubscribe, AircEventSubscribeAck,
        AircEventUnsubscribe, AircEventUnsubscribeAck, ClosurePolicy, ForbiddenReason,
        EVENT_ACK_BODY_HINT, HEADER_CONTINUUM_BODY_HINT, HEADER_EVENT_KIND,
        HEADER_EVENT_SUBSCRIPTION_ID, HEADER_EVENT_TOPIC,
    };
    use airc_core::PeerId;
    use std::sync::{Arc as StdArc, Mutex};
    use uuid::Uuid;

    /// The "no gate" policy used by all happy-path tests below.
    /// Avoids repeating `&AllowAllPolicy` inline; readers see the
    /// intent at a glance.
    fn allow_all() -> AllowAllPolicy {
        AllowAllPolicy
    }

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
            EventSubscribeAdapter::process_subscribe(&state, &allow_all(), &parsed)
                .expect("subscribe");

        // State should have one registered subscription.
        assert_eq!(state.len(), 1);

        // Headers must be the ack shape.
        assert_eq!(
            headers.get(HEADER_EVENT_KIND).map(String::as_str),
            Some("ack")
        );
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

        let err = EventSubscribeAdapter::process_subscribe(&state, &allow_all(), &parsed)
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

        let _ = EventSubscribeAdapter::process_subscribe(&state, &allow_all(), &parsed)
            .expect("subscribe");

        // lookup_matching with the info payload should match;
        // with the warn payload should not.
        let matched_info = state.lookup_matching("events", &serde_json::json!({"level": "info"}));
        assert_eq!(matched_info.len(), 1, "filter accepts info payload");

        let matched_warn = state.lookup_matching("events", &serde_json::json!({"level": "warn"}));
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
            EventSubscribeAdapter::process_subscribe(&state, &allow_all(), &subscribe)
                .expect("subscribe");
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

        assert_eq!(
            headers.get(HEADER_EVENT_KIND).map(String::as_str),
            Some("ack")
        );
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
        assert_eq!(PUBLISH_ADAPTER_NAME, "continuum.event.publish");
        assert_eq!(EVENT_PUBLISH_BODY_HINT, "continuum.event.publish.v1");

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
            EventSubscribeAdapter::process_subscribe(&state, &allow_all(), &subscribe)
                .expect("subscribe");
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
    }

    // ─── AuthPolicy gate threading (PR #1529 reviewer 2 BLOCK 1) ──────

    /// Mirror of `command_handler.rs::process_request_via_threads_caller_into_gate`.
    /// Builds a `ClosurePolicy` that captures the caller it
    /// receives, dispatches `process_subscribe`, then asserts the
    /// captured caller's peer_id is the envelope sender. Closes
    /// the silent-privilege-escalation gap the prior reviewer
    /// caught for commands.
    #[test]
    fn process_subscribe_threads_caller_into_gate() {
        let captured: StdArc<Mutex<Option<CallerIdentity>>> = StdArc::new(Mutex::new(None));
        let captured_clone = captured.clone();

        let policy = ClosurePolicy::new(
            "record-caller-event-subscribe",
            move |_decision: &RouteDecision, caller: Option<&CallerIdentity>| {
                *captured_clone.lock().unwrap() = caller.cloned();
                Verdict::Allowed
            },
        );

        let state = EventPublisherState::new();
        let sender_peer_id = PeerId::new();
        let parsed = ParsedSubscribe {
            caller_peer_id: sender_peer_id,
            reply_to: PeerId::new(),
            correlation_id: Uuid::new_v4(),
            request: AircEventSubscribe {
                topic: "cognition/analyze/complete".into(),
                filter: None,
            },
        };

        let _ = EventSubscribeAdapter::process_subscribe(&state, &policy, &parsed)
            .expect("AllowAllPolicy verdict → process_subscribe succeeds");

        let observed = captured
            .lock()
            .unwrap()
            .clone()
            .expect("AuthPolicy::gate must have been invoked with Some(caller)");
        assert_eq!(
            observed.peer_id, sender_peer_id,
            "caller's peer_id must match the envelope sender — \
             closes the reviewer 2 BLOCK 1 silent-privilege-escalation seam"
        );
        assert!(
            matches!(observed.source, crate::routing::CallerSource::Airc),
            "caller source must be Airc (cross-grid), not Local: {observed:?}"
        );
    }

    /// Mirror of `command_handler.rs::process_request_via` Forbidden
    /// path: if the policy refuses, process_subscribe must error
    /// BEFORE touching state. No subscription persisted on
    /// refusal — same invariant as `process_subscribe_refuses_empty_topic_with_typed_error`.
    #[test]
    fn process_subscribe_refuses_when_policy_forbids() {
        let policy = ClosurePolicy::new("forbid-everything", |_decision, _caller| {
            Verdict::Forbidden {
                reason: ForbiddenReason::NoPermissionForUri("events/internal/subscribe".into()),
            }
        });

        let state = EventPublisherState::new();
        let parsed = ParsedSubscribe {
            caller_peer_id: PeerId::new(),
            reply_to: PeerId::new(),
            correlation_id: Uuid::new_v4(),
            request: AircEventSubscribe {
                topic: "events/internal".into(),
                filter: None,
            },
        };

        let err = EventSubscribeAdapter::process_subscribe(&state, &policy, &parsed)
            .expect_err("Forbidden verdict must refuse");
        match err {
            AdapterError::Consumer(msg) => {
                assert!(
                    msg.contains("forbidden by policy"),
                    "error must signal policy refusal: {msg}"
                );
                assert!(
                    msg.contains("NoPermissionForUri"),
                    "error must include the typed reason: {msg}"
                );
            }
            other => panic!("expected Consumer error, got {other:?}"),
        }
        assert_eq!(
            state.len(),
            0,
            "no subscription persisted when policy refuses"
        );
    }

    /// The synthetic URI shape the gate sees: `events/<topic>/subscribe`.
    /// Pin this so a future change to `process_subscribe`'s URI
    /// construction can't silently break policies that match on
    /// the path prefix (e.g., "events/cognition/" allow-list).
    #[test]
    fn process_subscribe_decision_path_is_stable() {
        let observed: StdArc<Mutex<Option<String>>> = StdArc::new(Mutex::new(None));
        let observed_clone = observed.clone();

        let policy = ClosurePolicy::new("record-decision-path", move |decision, _caller| {
            if let RouteDecision::Local { path, .. } = decision {
                *observed_clone.lock().unwrap() = Some(path.clone());
            }
            Verdict::Allowed
        });

        let state = EventPublisherState::new();
        let parsed = ParsedSubscribe {
            caller_peer_id: PeerId::new(),
            reply_to: PeerId::new(),
            correlation_id: Uuid::new_v4(),
            request: AircEventSubscribe {
                topic: "cognition/score/persona-scored".into(),
                filter: None,
            },
        };

        let _ =
            EventSubscribeAdapter::process_subscribe(&state, &policy, &parsed).expect("subscribe");
        let path = observed
            .lock()
            .unwrap()
            .clone()
            .expect("policy saw a Local decision");
        assert_eq!(
            path, "events/cognition/score/persona-scored/subscribe",
            "URI shape must be events/<topic>/subscribe so policies can match \
             prefix authoring stays stable across refactors"
        );
    }

    // ─── EventPublishAdapter::gate_publish (the WRITE gate) ───────────
    //
    // process_publish's fan-out half is the async glue over
    // AircEventPublisher::publish, whose composition is covered without
    // airc by airc_event_publisher::build_publish_envelopes tests. Here
    // we test the security-critical seam — the gate — directly.

    #[test]
    fn gate_publish_threads_caller_into_gate() {
        let captured: StdArc<Mutex<Option<CallerIdentity>>> = StdArc::new(Mutex::new(None));
        let captured_clone = captured.clone();

        let policy = ClosurePolicy::new(
            "record-caller-event-publish",
            move |_decision: &RouteDecision, caller: Option<&CallerIdentity>| {
                *captured_clone.lock().unwrap() = caller.cloned();
                Verdict::Allowed
            },
        );

        let sender_peer_id = PeerId::new();
        let parsed = ParsedPublish {
            caller_peer_id: sender_peer_id,
            reply_to: PeerId::new(),
            correlation_id: Uuid::new_v4(),
            request: AircEventPublish {
                topic: "cognition/analyze/complete".into(),
                payload: serde_json::json!({ "k": 1 }),
            },
        };

        EventPublishAdapter::gate_publish(&policy, &parsed)
            .expect("AllowAll-style verdict → gate_publish succeeds");

        let observed = captured
            .lock()
            .unwrap()
            .clone()
            .expect("AuthPolicy::gate must have been invoked with Some(caller)");
        assert_eq!(
            observed.peer_id, sender_peer_id,
            "caller's peer_id must match the envelope sender — a publish is a \
             WRITE; the gate must see who is emitting"
        );
        assert!(
            matches!(observed.source, crate::routing::CallerSource::Airc),
            "caller source must be Airc (cross-grid), not Local: {observed:?}"
        );
    }

    #[test]
    fn gate_publish_refuses_empty_topic_with_typed_error() {
        let parsed = ParsedPublish {
            caller_peer_id: PeerId::new(),
            reply_to: PeerId::new(),
            correlation_id: Uuid::new_v4(),
            request: AircEventPublish {
                topic: "".into(),
                payload: serde_json::json!({}),
            },
        };

        let err = EventPublishAdapter::gate_publish(&allow_all(), &parsed)
            .expect_err("empty topic must refuse");
        match err {
            AdapterError::Consumer(msg) => assert!(
                msg.contains("topic must not be empty"),
                "error names the missing piece: {msg}"
            ),
            other => panic!("expected Consumer error, got {other:?}"),
        }
    }

    #[test]
    fn gate_publish_refuses_when_policy_forbids() {
        let policy = ClosurePolicy::new("forbid-everything", |_decision, _caller| {
            Verdict::Forbidden {
                reason: ForbiddenReason::NoPermissionForUri("events/internal/publish".into()),
            }
        });

        let parsed = ParsedPublish {
            caller_peer_id: PeerId::new(),
            reply_to: PeerId::new(),
            correlation_id: Uuid::new_v4(),
            request: AircEventPublish {
                topic: "events/internal".into(),
                payload: serde_json::json!({}),
            },
        };

        let err = EventPublishAdapter::gate_publish(&policy, &parsed)
            .expect_err("Forbidden verdict must refuse");
        match err {
            AdapterError::Consumer(msg) => {
                assert!(
                    msg.contains("forbidden by policy"),
                    "error must signal policy refusal: {msg}"
                );
                assert!(
                    msg.contains("NoPermissionForUri"),
                    "error must include the typed reason: {msg}"
                );
            }
            other => panic!("expected Consumer error, got {other:?}"),
        }
    }

    /// The synthetic URI the gate sees: `events/<topic>/publish` — the
    /// WRITE twin of `.../subscribe`. Pin it so a future change to the URI
    /// construction can't silently break publish policies that match on
    /// the path prefix.
    #[test]
    fn gate_publish_decision_path_is_stable() {
        let observed: StdArc<Mutex<Option<String>>> = StdArc::new(Mutex::new(None));
        let observed_clone = observed.clone();

        let policy = ClosurePolicy::new("record-decision-path", move |decision, _caller| {
            if let RouteDecision::Local { path, .. } = decision {
                *observed_clone.lock().unwrap() = Some(path.clone());
            }
            Verdict::Allowed
        });

        let parsed = ParsedPublish {
            caller_peer_id: PeerId::new(),
            reply_to: PeerId::new(),
            correlation_id: Uuid::new_v4(),
            request: AircEventPublish {
                topic: "cognition/score/persona-scored".into(),
                payload: serde_json::json!({}),
            },
        };

        EventPublishAdapter::gate_publish(&policy, &parsed).expect("allowed");
        let path = observed
            .lock()
            .unwrap()
            .clone()
            .expect("policy saw a Local decision");
        assert_eq!(
            path, "events/cognition/score/persona-scored/publish",
            "URI shape must be events/<topic>/publish — the WRITE twin of \
             subscribe — so publish policy prefix authoring stays stable"
        );
    }

    // what this catches: the publish ack carries BOTH the topic and the
    // fan-out count back to the caller's emit(). A drift that drops
    // `delivered` would make emit() unable to report how many subscribers
    // an event reached.
    #[test]
    fn build_publish_ack_carries_topic_and_delivered_count() {
        let (headers, body) = build_publish_ack("metrics/cpu", 3).expect("ack");

        assert_eq!(
            headers.get(HEADER_EVENT_KIND).map(String::as_str),
            Some("ack")
        );
        assert_eq!(
            headers.get(HEADER_EVENT_TOPIC).map(String::as_str),
            Some("metrics/cpu")
        );
        assert_eq!(
            headers.get(HEADER_CONTINUUM_BODY_HINT).map(String::as_str),
            Some(EVENT_ACK_BODY_HINT)
        );

        let ack: AircEventPublishAck = serde_json::from_value(match body {
            Body::Json(v) => v,
            other => panic!("expected Json, got {other:?}"),
        })
        .expect("decode");
        assert_eq!(ack.topic, "metrics/cpu");
        assert_eq!(ack.delivered, 3);
    }
}
