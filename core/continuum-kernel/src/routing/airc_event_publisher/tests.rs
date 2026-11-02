#![cfg(test)]
    use super::*;
    use airc_core::{ClientId, EventId, MentionTarget, RoomId, TranscriptEvent, TranscriptKind};

    // ─── EventPublisherState ─────────────────────────────────────────

    #[test]
    fn register_then_lookup_finds_the_subscription() {
        let state = EventPublisherState::new();
        let subscriber = PeerId::new();
        let id = state
            .register(subscriber, "x/y".into(), None)
            .expect("register");
        assert!(state.is_registered(id));
        assert_eq!(state.len(), 1);

        let matched = state.lookup_matching("x/y", &Value::Null);
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].subscription_id, id);
        assert_eq!(matched[0].subscriber_peer_id, subscriber);
        assert_eq!(matched[0].sequence, 0, "first delivery is sequence 0");
    }

    #[test]
    fn register_empty_topic_refuses() {
        let state = EventPublisherState::new();
        let err = state
            .register(PeerId::new(), "".into(), None)
            .expect_err("empty topic must be refused");
        assert!(
            err.contains("topic must not be empty"),
            "error must name the missing piece: {err}"
        );
        assert!(
            err.contains("[[no-fallbacks-ever]]"),
            "error must cite the doctrine: {err}"
        );
        assert_eq!(state.len(), 0, "refused registration must not leave state");
    }

    #[test]
    fn unregister_returns_true_for_active_subscription() {
        let state = EventPublisherState::new();
        let id = state
            .register(PeerId::new(), "x/y".into(), None)
            .expect("register");
        assert!(state.unregister(id), "first unregister returns true");
        assert!(!state.is_registered(id));
        assert_eq!(state.len(), 0);
    }

    #[test]
    fn unregister_returns_false_for_unknown_id_idempotent_contract() {
        let state = EventPublisherState::new();
        let unknown = Uuid::new_v4();
        assert!(
            !state.unregister(unknown),
            "unknown subscription_id must return false — \
             matches AircEventUnsubscribeAck.closed=false contract"
        );
    }

    #[test]
    fn lookup_matching_excludes_other_topics() {
        let state = EventPublisherState::new();
        let _id_a = state.register(PeerId::new(), "a/x".into(), None).unwrap();
        let _id_b = state.register(PeerId::new(), "b/y".into(), None).unwrap();

        let matched = state.lookup_matching("a/x", &Value::Null);
        assert_eq!(matched.len(), 1, "only the a/x subscription matches");
    }

    #[test]
    fn lookup_matching_applies_filter() {
        let state = EventPublisherState::new();
        let _strict = state
            .register(
                PeerId::new(),
                "events".into(),
                Some(serde_json::json!({"level": "info"})),
            )
            .unwrap();
        let _open = state
            .register(PeerId::new(), "events".into(), None)
            .unwrap();

        let info_payload = serde_json::json!({"level": "info", "msg": "hi"});
        let matches_info = state.lookup_matching("events", &info_payload);
        assert_eq!(
            matches_info.len(),
            2,
            "both subscriptions match info payload"
        );

        let warn_payload = serde_json::json!({"level": "warn", "msg": "watch out"});
        let matches_warn = state.lookup_matching("events", &warn_payload);
        assert_eq!(
            matches_warn.len(),
            1,
            "only the unfiltered subscription matches warn payload"
        );
    }

    #[test]
    fn lookup_matching_bumps_per_subscription_sequence_monotonically() {
        let state = EventPublisherState::new();
        let _id = state
            .register(PeerId::new(), "metrics".into(), None)
            .unwrap();

        // Each lookup that matches should hand back the next
        // sequence number. Three lookups → three distinct sequences.
        let first = state.lookup_matching("metrics", &Value::Null);
        let second = state.lookup_matching("metrics", &Value::Null);
        let third = state.lookup_matching("metrics", &Value::Null);

        assert_eq!(first[0].sequence, 0);
        assert_eq!(second[0].sequence, 1);
        assert_eq!(third[0].sequence, 2);
    }

    #[test]
    fn lookup_matching_sequences_are_per_subscription_not_global() {
        // Pin that two independent subscriptions on the same topic
        // each have their own monotonic counter — the caller-side
        // drop detector treats them as separate streams.
        let state = EventPublisherState::new();
        let a = state
            .register(PeerId::new(), "shared".into(), None)
            .unwrap();
        let b = state
            .register(PeerId::new(), "shared".into(), None)
            .unwrap();

        let first = state.lookup_matching("shared", &Value::Null);
        let second = state.lookup_matching("shared", &Value::Null);
        assert_eq!(first.len(), 2);
        assert_eq!(second.len(), 2);

        for round in [&first, &second] {
            for matched in round.iter() {
                let expected_seq = if matched.subscription_id == a || matched.subscription_id == b {
                    if round.as_ptr() == first.as_ptr() {
                        0
                    } else {
                        1
                    }
                } else {
                    panic!("unknown subscription_id in matched");
                };
                assert_eq!(matched.sequence, expected_seq);
            }
        }
    }

    // ─── build_publish_envelopes (the publish() composition) ────────

    #[test]
    fn build_publish_envelopes_empty_when_no_subscriptions_match() {
        let state = EventPublisherState::new();
        let envs =
            AircEventPublisher::build_publish_envelopes(&state, "unsubscribed/topic", &Value::Null)
                .expect("build");
        assert!(envs.is_empty(), "no matches → empty vec, not an error");
    }

    #[test]
    fn build_publish_envelopes_one_per_match_with_demuxable_headers() {
        let state = EventPublisherState::new();
        let sub_a = PeerId::new();
        let sub_b = PeerId::new();
        let id_a = state.register(sub_a, "metrics".into(), None).unwrap();
        let id_b = state.register(sub_b, "metrics".into(), None).unwrap();
        // unrelated topic — must NOT appear in fanout
        let _id_other = state.register(PeerId::new(), "other".into(), None).unwrap();

        let payload = serde_json::json!({"cpu": 0.42});
        let envs = AircEventPublisher::build_publish_envelopes(&state, "metrics", &payload)
            .expect("build");
        assert_eq!(
            envs.len(),
            2,
            "two matches → two envelopes; other topic excluded"
        );

        for (matched, headers, body) in &envs {
            assert!(
                matched.subscription_id == id_a || matched.subscription_id == id_b,
                "envelope's matched id must be one of the registered metrics subs"
            );
            assert_eq!(
                headers
                    .get(HEADER_EVENT_SUBSCRIPTION_ID)
                    .map(String::as_str),
                Some(matched.subscription_id.to_string().as_str()),
                "subscription_id header demuxes correctly"
            );
            assert_eq!(
                headers.get(HEADER_CONTINUUM_BODY_HINT).map(String::as_str),
                Some(EVENT_DELIVER_BODY_HINT)
            );

            // Body round-trips back through the typed envelope.
            let value = match body {
                Body::Json(v) => v.clone(),
                other => panic!("expected Json body, got {other:?}"),
            };
            let deliver: AircEventDeliver = serde_json::from_value(value).expect("decode Deliver");
            assert_eq!(deliver.topic, "metrics");
            assert_eq!(deliver.subscription_id, matched.subscription_id);
            assert_eq!(deliver.payload, payload);
            assert_eq!(deliver.sequence, matched.sequence);
        }
    }

    #[test]
    fn build_publish_envelopes_respects_filter() {
        // A subscription with a filter `{level: info}` should
        // match the info payload but not the warn payload — the
        // server-side filter contract enforced by the publish path.
        let state = EventPublisherState::new();
        let _filtered = state
            .register(
                PeerId::new(),
                "events".into(),
                Some(serde_json::json!({"level": "info"})),
            )
            .unwrap();

        let info = serde_json::json!({"level": "info", "msg": "hi"});
        let warn = serde_json::json!({"level": "warn", "msg": "watch out"});

        let info_envs =
            AircEventPublisher::build_publish_envelopes(&state, "events", &info).expect("info");
        let warn_envs =
            AircEventPublisher::build_publish_envelopes(&state, "events", &warn).expect("warn");

        assert_eq!(info_envs.len(), 1, "info payload matches the filter");
        assert!(
            warn_envs.is_empty(),
            "warn payload filtered out by server-side filter"
        );
    }

    #[test]
    fn build_publish_envelopes_bumps_per_subscription_sequence_across_calls() {
        // The publish path uses `lookup_matching` which bumps the
        // atomic sequence per subscription. Two successive
        // build_publish_envelopes calls for the same subscription
        // must hand back sequence 0 then 1 — the caller-side drop
        // detector relies on this monotonicity.
        let state = EventPublisherState::new();
        let _id = state
            .register(PeerId::new(), "metrics".into(), None)
            .unwrap();

        let first =
            AircEventPublisher::build_publish_envelopes(&state, "metrics", &Value::Null).unwrap();
        let second =
            AircEventPublisher::build_publish_envelopes(&state, "metrics", &Value::Null).unwrap();

        assert_eq!(first.len(), 1);
        assert_eq!(second.len(), 1);
        assert_eq!(first[0].0.sequence, 0);
        assert_eq!(second[0].0.sequence, 1);
    }

    // ─── matches_filter ──────────────────────────────────────────────

    #[test]
    fn matches_filter_none_matches_everything() {
        assert!(matches_filter(None, &Value::Null));
        assert!(matches_filter(None, &serde_json::json!({"x": 1})));
        assert!(matches_filter(None, &serde_json::json!(42)));
    }

    #[test]
    fn matches_filter_empty_object_matches_everything() {
        let filter = serde_json::json!({});
        assert!(matches_filter(Some(&filter), &Value::Null));
        assert!(matches_filter(Some(&filter), &serde_json::json!({"x": 1})));
    }

    #[test]
    fn matches_filter_object_requires_equality_on_every_field() {
        let filter = serde_json::json!({"level": "info", "module": "auth"});
        assert!(matches_filter(
            Some(&filter),
            &serde_json::json!({"level": "info", "module": "auth", "extra": "ok"}),
        ));
        // missing field
        assert!(!matches_filter(
            Some(&filter),
            &serde_json::json!({"level": "info"}),
        ));
        // value mismatch
        assert!(!matches_filter(
            Some(&filter),
            &serde_json::json!({"level": "warn", "module": "auth"}),
        ));
    }

    #[test]
    fn matches_filter_payload_not_object_refuses() {
        let filter = serde_json::json!({"x": 1});
        assert!(
            !matches_filter(Some(&filter), &Value::Null),
            "filter can't equality-match against non-object payload"
        );
        assert!(!matches_filter(Some(&filter), &serde_json::json!(42)));
    }

    #[test]
    fn matches_filter_non_object_filter_refuses_not_guesses() {
        // A non-object filter is a misuse; per [[no-fallbacks-ever]]
        // the predicate refuses rather than guessing intent.
        let filter = serde_json::json!("level=info");
        assert!(!matches_filter(
            Some(&filter),
            &serde_json::json!({"level": "info"})
        ));
    }

    // ─── build_subscribe_ack ─────────────────────────────────────────

    #[test]
    fn build_subscribe_ack_stamps_protocol_headers_and_round_trips_body() {
        let sub_id = Uuid::new_v4();
        let topic = "events/test";
        let (headers, body) = build_subscribe_ack(sub_id, topic).expect("build");

        assert_eq!(
            headers.get(HEADER_EVENT_KIND).map(String::as_str),
            Some("ack")
        );
        assert_eq!(
            headers
                .get(HEADER_EVENT_SUBSCRIPTION_ID)
                .map(String::as_str),
            Some(sub_id.to_string().as_str())
        );
        assert_eq!(
            headers.get(HEADER_EVENT_TOPIC).map(String::as_str),
            Some(topic)
        );
        assert_eq!(
            headers.get(HEADER_CONTINUUM_BODY_HINT).map(String::as_str),
            Some(EVENT_ACK_BODY_HINT)
        );

        let value = match body {
            Body::Json(v) => v,
            other => panic!("expected Json body, got {other:?}"),
        };
        let ack: AircEventSubscribeAck = serde_json::from_value(value).expect("decode");
        assert_eq!(ack.subscription_id, sub_id);
        assert_eq!(ack.topic, topic);
    }

    // ─── build_unsubscribe_ack ───────────────────────────────────────

    #[test]
    fn build_unsubscribe_ack_active_preserves_closed_true() {
        let sub_id = Uuid::new_v4();
        let (_headers, body) = build_unsubscribe_ack(sub_id, true).expect("build");
        let ack: AircEventUnsubscribeAck = serde_json::from_value(match body {
            Body::Json(v) => v,
            other => panic!("expected Json, got {other:?}"),
        })
        .expect("decode");
        assert!(ack.closed);
        assert_eq!(ack.subscription_id, sub_id);
    }

    #[test]
    fn build_unsubscribe_ack_idempotent_preserves_closed_false() {
        let sub_id = Uuid::new_v4();
        let (headers, body) = build_unsubscribe_ack(sub_id, false).expect("build");
        // headers should still indicate ack
        assert_eq!(
            headers.get(HEADER_EVENT_KIND).map(String::as_str),
            Some("ack")
        );
        let ack: AircEventUnsubscribeAck = serde_json::from_value(match body {
            Body::Json(v) => v,
            other => panic!("expected Json, got {other:?}"),
        })
        .expect("decode");
        assert!(
            !ack.closed,
            "idempotent unsubscribe must preserve closed=false"
        );
    }

    // ─── build_deliver_frame ─────────────────────────────────────────

    #[test]
    fn build_deliver_frame_stamps_protocol_headers_and_round_trips_body() {
        let deliver = AircEventDeliver {
            subscription_id: Uuid::new_v4(),
            topic: "cognition/analyze/complete".into(),
            sequence: 42,
            payload: serde_json::json!({"confidence": 0.84}),
        };
        let (headers, body) = build_deliver_frame(&deliver).expect("build");

        assert_eq!(
            headers.get(HEADER_EVENT_KIND).map(String::as_str),
            Some("deliver")
        );
        assert_eq!(
            headers.get(HEADER_EVENT_TOPIC).map(String::as_str),
            Some(deliver.topic.as_str())
        );
        assert_eq!(
            headers
                .get(HEADER_EVENT_SUBSCRIPTION_ID)
                .map(String::as_str),
            Some(deliver.subscription_id.to_string().as_str())
        );
        assert_eq!(
            headers.get(HEADER_CONTINUUM_BODY_HINT).map(String::as_str),
            Some(EVENT_DELIVER_BODY_HINT)
        );

        let back: AircEventDeliver = serde_json::from_value(match body {
            Body::Json(v) => v,
            other => panic!("expected Json, got {other:?}"),
        })
        .expect("decode");
        assert_eq!(back, deliver);
    }

    // ─── caller-side ↔ peer-side symmetry guards ─────────────────────
    //
    // These tests pin the cross-boundary contract: every envelope the
    // peer-side BUILDS must round-trip through the caller-side
    // DECODE/MATCH functions. Future refactors that break either
    // side fail loudly here, not silently on the wire.
    //
    // PR #1529 reviewer 2 BLOCK 4: prior commit only pinned the
    // Deliver shape; this set adds subscribe-ack and
    // unsubscribe-ack symmetry too.

    #[test]
    fn build_deliver_frame_passes_caller_side_matches_subscription() {
        // The caller-side filter (AircEventTransport::matches_subscription)
        // checks publisher peer_id + body_hint + subscription_id
        // header. A Deliver built by build_deliver_frame MUST
        // satisfy that predicate — otherwise the subscriber never
        // picks it up.
        use super::super::AircEventTransport;

        let publisher = PeerId::new();
        let deliver = AircEventDeliver {
            subscription_id: Uuid::new_v4(),
            topic: "x".into(),
            sequence: 0,
            payload: Value::Null,
        };
        let (headers, body) = build_deliver_frame(&deliver).expect("build");

        // Reconstruct a TranscriptEvent shaped exactly like what
        // `Airc::publish` would deliver: peer_id is the publisher
        // airc signed, headers + body intact.
        let event = TranscriptEvent {
            event_id: EventId::new(),
            room_id: RoomId::new(),
            peer_id: publisher,
            client_id: ClientId::new(),
            kind: TranscriptKind::Message,
            occurred_at_ms: 1_700_000_000,
            lamport: 1,
            target: MentionTarget::All,
            headers,
            body: Some(body),
            attachment: None,
            receipt: None,
            metadata: Value::Null,
        };

        assert!(
            AircEventTransport::matches_subscription(&event, deliver.subscription_id, publisher),
            "build_deliver_frame must produce a frame caller-side accepts — \
             this is the cross-boundary contract"
        );
        let decoded = AircEventTransport::decode_deliver_frame(&event)
            .expect("decode_deliver_frame must accept frames the peer-side builds");
        assert_eq!(decoded, deliver);
    }

    #[test]
    fn build_subscribe_ack_passes_caller_side_decode_subscribe_ack() {
        use super::super::AircEventTransport;
        let sub_id = Uuid::new_v4();
        let topic = "events/test";
        let (_headers, body) = build_subscribe_ack(sub_id, topic).expect("build");
        let ack = AircEventTransport::decode_subscribe_ack(Some(body))
            .expect("decode_subscribe_ack must accept what build_subscribe_ack produces");
        assert_eq!(ack.subscription_id, sub_id);
        assert_eq!(ack.topic, topic);
    }

    #[test]
    fn build_unsubscribe_ack_active_passes_caller_side_decode() {
        use super::super::AircEventTransport;
        let sub_id = Uuid::new_v4();
        let (_headers, body) = build_unsubscribe_ack(sub_id, true).expect("build");
        let ack = AircEventTransport::decode_unsubscribe_ack(Some(body))
            .expect("decode_unsubscribe_ack must accept what build_unsubscribe_ack produces");
        assert_eq!(ack.subscription_id, sub_id);
        assert!(ack.closed, "active unsubscribe must round-trip closed=true");
    }

    #[test]
    fn build_unsubscribe_ack_idempotent_passes_caller_side_decode() {
        // Idempotent unsubscribe (closed=false) is a real wire
        // shape per the protocol commit — the caller branches on
        // this for telemetry. Pin the cross-boundary contract for
        // BOTH variants so a future builder change can't silently
        // collapse the idempotent case to active.
        use super::super::AircEventTransport;
        let sub_id = Uuid::new_v4();
        let (_headers, body) = build_unsubscribe_ack(sub_id, false).expect("build");
        let ack = AircEventTransport::decode_unsubscribe_ack(Some(body))
            .expect("decode must accept what build produces, idempotent variant");
        assert_eq!(ack.subscription_id, sub_id);
        assert!(
            !ack.closed,
            "idempotent unsubscribe must round-trip closed=false"
        );
    }
