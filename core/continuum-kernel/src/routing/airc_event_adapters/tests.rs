#![cfg(test)]
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
