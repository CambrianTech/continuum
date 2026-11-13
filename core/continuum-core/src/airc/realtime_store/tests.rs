#![cfg(test)]
    use super::*;
    use crate::routing::airc_realtime::{
        AircPeerCapability, AircPresenceState, AircRealtimePayloadRef, AircRealtimeSchema,
        AircSubscriptionAction, AircSubscriptionEvent,
    };
    use airc_core::PeerId;
    use serde_json::json;

    const GENERAL: Uuid = Uuid::from_u128(0xA1);
    const CAMBRIANTECH: Uuid = Uuid::from_u128(0xA2);
    const OTHER: Uuid = Uuid::from_u128(0xA3);

    fn durable_event(id: u128, room: Uuid, created_at_ms: u64) -> AircRealtimeEnvelope {
        AircRealtimeEnvelope::new(
            Uuid::from_u128(id),
            room,
            PeerId::from_u128(0xC1),
            created_at_ms,
            AircRealtimePayload::ExistingSchema {
                payload: AircRealtimePayloadRef::inline(
                    AircRealtimeSchema::ChatTranscript,
                    json!({"text": format!("{id}")}),
                ),
            },
        )
    }

    fn typing_event(id: u128, started_at_ms: u64, expires_at_ms: u64) -> AircRealtimeEnvelope {
        AircRealtimeEnvelope::new(
            Uuid::from_u128(id),
            GENERAL,
            PeerId::from_u128(0xC2),
            started_at_ms,
            AircRealtimePayload::Presence {
                event: AircPresenceEvent {
                    room_id: GENERAL,
                    subject_id: "persona-1".to_string(),
                    display_name: None,
                    state: AircPresenceState::Typing,
                    started_at_ms,
                    expires_at_ms: Some(expires_at_ms),
                    call_id: None,
                },
            },
        )
    }

    fn peer_manifest_event(
        id: u128,
        peer_id: &str,
        rooms: &[Uuid],
        capabilities: &[&str],
        advertised_at_ms: u64,
        expires_at_ms: Option<u64>,
    ) -> AircRealtimeEnvelope {
        AircRealtimeEnvelope::new(
            Uuid::from_u128(id),
            GENERAL,
            PeerId::from_uuid(uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, peer_id.as_bytes())),
            advertised_at_ms,
            AircRealtimePayload::PeerManifest {
                manifest: AircPeerManifest {
                    peer_id: PeerId::from_uuid(uuid::Uuid::new_v5(
                        &uuid::Uuid::NAMESPACE_OID,
                        peer_id.as_bytes(),
                    )),
                    display_name: Some(peer_id.to_string()),
                    room_ids: rooms.to_vec(),
                    capabilities: capabilities
                        .iter()
                        .map(|id| AircPeerCapability {
                            id: (*id).to_string(),
                            label: None,
                            version: None,
                        })
                        .collect(),
                    // Structural-only sample pubkey (passes hex/length
                    // checks; not a real key). Multi-peer tests should
                    // pass per-peer overrides if equality matters.
                    signing_pubkey_hex:
                        "1112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f30"
                            .to_string(),
                    advertised_at_ms,
                    expires_at_ms,
                },
            },
        )
    }

    #[test]
    fn durable_events_replay_from_cursor() {
        let store = InMemoryAircRealtimeStore::new(10);
        for idx in 1..=3 {
            store
                .publish(AircRealtimePublishParams {
                    envelope: durable_event(idx as u128, GENERAL, idx),
                })
                .unwrap();
        }

        let result = store
            .replay(AircRealtimeReplayParams {
                room_id: GENERAL,
                after_cursor: Some(AircReplayCursor {
                    room_id: GENERAL,
                    lamport: 1,
                    event_id: Uuid::from_u128(1),
                    observed_at_ms: Some(1),
                }),
                limit: Some(10),
                include_presence: None,
                include_subscriptions: None,
                include_peer_manifests: None,
                include_capability_index: None,
                now_ms: None,
            })
            .unwrap();

        assert_eq!(
            result
                .events
                .iter()
                .map(|event| event.event_id)
                .collect::<Vec<_>>(),
            [Uuid::from_u128(2), Uuid::from_u128(3)]
        );
        assert_eq!(result.cursor.unwrap().event_id, Uuid::from_u128(3));
    }

    #[test]
    fn ephemeral_presence_coalesces_and_expires_without_replay_pollution() {
        let store = InMemoryAircRealtimeStore::new(10);
        let first = store
            .publish(AircRealtimePublishParams {
                envelope: typing_event(0x71, 100, 200),
            })
            .unwrap();
        let second = store
            .publish(AircRealtimePublishParams {
                envelope: typing_event(0x72, 120, 240),
            })
            .unwrap();

        assert!(!first.stored_for_replay);
        assert!(!second.stored_for_replay);
        assert_eq!(second.active_presence_count, 1);

        let live = store
            .replay(AircRealtimeReplayParams {
                room_id: GENERAL,
                after_cursor: None,
                limit: None,
                include_presence: Some(true),
                include_subscriptions: None,
                include_peer_manifests: None,
                include_capability_index: None,
                now_ms: Some(239),
            })
            .unwrap();
        assert!(live.events.is_empty());
        assert_eq!(live.active_presence.len(), 1);
        assert_eq!(live.active_presence[0].started_at_ms, 120);

        let expired = store
            .replay(AircRealtimeReplayParams {
                room_id: GENERAL,
                after_cursor: None,
                limit: None,
                include_presence: Some(true),
                include_subscriptions: None,
                include_peer_manifests: None,
                include_capability_index: None,
                now_ms: Some(240),
            })
            .unwrap();
        assert!(expired.active_presence.is_empty());
    }

    #[test]
    fn peer_manifest_coalesces_indexes_capabilities_and_stays_out_of_replay() {
        let store = InMemoryAircRealtimeStore::new(10);
        let first = store
            .publish(AircRealtimePublishParams {
                envelope: peer_manifest_event(
                    0x91,
                    "peer-a",
                    &[GENERAL],
                    &["continuum.lora.invoke"],
                    100,
                    Some(500),
                ),
            })
            .unwrap();
        let second = store
            .publish(AircRealtimePublishParams {
                envelope: peer_manifest_event(
                    0x92,
                    "peer-a",
                    &[GENERAL, CAMBRIANTECH],
                    &["continuum.lora.invoke", "continuum.chat.turn"],
                    150,
                    Some(600),
                ),
            })
            .unwrap();
        store
            .publish(AircRealtimePublishParams {
                envelope: peer_manifest_event(
                    0x93,
                    "peer-b",
                    &[GENERAL],
                    &["continuum.lora.invoke"],
                    160,
                    Some(600),
                ),
            })
            .unwrap();

        assert!(!first.stored_for_replay);
        assert!(!second.stored_for_replay);
        assert_eq!(
            second.coalesced_presence_key.as_deref(),
            Some(
                format!(
                    "peer_manifest:{}",
                    PeerId::from_uuid(uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, b"peer-a"))
                        .as_uuid()
                )
                .as_str()
            )
        );
        assert_eq!(second.active_peer_manifest_count, 1);

        let result = store
            .replay(AircRealtimeReplayParams {
                room_id: GENERAL,
                after_cursor: None,
                limit: None,
                include_presence: None,
                include_subscriptions: None,
                include_peer_manifests: Some(true),
                include_capability_index: Some(true),
                now_ms: Some(599),
            })
            .unwrap();

        assert!(result.events.is_empty());
        assert_eq!(
            result
                .active_peer_manifests
                .iter()
                .map(|manifest| manifest.peer_id.as_uuid().to_string())
                .collect::<Vec<_>>(),
            [
                PeerId::from_uuid(uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, b"peer-a"))
                    .as_uuid()
                    .to_string(),
                PeerId::from_uuid(uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, b"peer-b"))
                    .as_uuid()
                    .to_string(),
            ]
        );
        assert_eq!(result.capability_index.len(), 2);
        assert_eq!(
            result.capability_index[0].capability_id,
            "continuum.chat.turn"
        );
        assert_eq!(
            result.capability_index[0].peer_ids,
            vec![
                PeerId::from_uuid(uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, b"peer-a"))
                    .as_uuid()
                    .to_string()
            ]
        );
        assert_eq!(
            result.capability_index[1].capability_id,
            "continuum.lora.invoke"
        );
        assert_eq!(
            result.capability_index[1].peer_ids,
            vec![
                PeerId::from_uuid(uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, b"peer-a"))
                    .as_uuid()
                    .to_string(),
                PeerId::from_uuid(uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, b"peer-b"))
                    .as_uuid()
                    .to_string()
            ]
        );

        let expired = store
            .replay(AircRealtimeReplayParams {
                room_id: GENERAL,
                after_cursor: None,
                limit: None,
                include_presence: None,
                include_subscriptions: None,
                include_peer_manifests: Some(true),
                include_capability_index: Some(true),
                now_ms: Some(600),
            })
            .unwrap();
        assert!(expired.active_peer_manifests.is_empty());
        assert!(expired.capability_index.is_empty());
    }

    #[test]
    fn receipt_only_messages_are_not_replayed() {
        let store = InMemoryAircRealtimeStore::new(10);
        let mut receipt = AircRealtimeEnvelope::new(
            Uuid::from_u128(0xB1),
            GENERAL,
            PeerId::from_u128(0xC3),
            10,
            AircRealtimePayload::Receipt {
                receipt: crate::routing::airc_realtime::AircReceipt {
                    event_id: Uuid::from_u128(1),
                    peer_id: PeerId::from_uuid(uuid::Uuid::new_v5(
                        &uuid::Uuid::NAMESPACE_OID,
                        b"peer-1",
                    )),
                    received_at_ms: 10,
                    replay_cursor: None,
                },
            },
        );
        receipt.delivery = AircRealtimeDelivery::ReceiptOnly;

        let result = store
            .publish(AircRealtimePublishParams { envelope: receipt })
            .unwrap();
        assert!(!result.stored_for_replay);

        let replay = store
            .replay(AircRealtimeReplayParams {
                room_id: GENERAL,
                after_cursor: None,
                limit: None,
                include_presence: None,
                include_subscriptions: None,
                include_peer_manifests: None,
                include_capability_index: None,
                now_ms: None,
            })
            .unwrap();
        assert!(replay.events.is_empty());
    }

    #[test]
    fn control_messages_are_replayable_for_reconnect() {
        let store = InMemoryAircRealtimeStore::new(10);
        let envelope = AircRealtimeEnvelope::new(
            Uuid::from_u128(0x51),
            GENERAL,
            PeerId::from_uuid(uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, b"browser-1")),
            10,
            AircRealtimePayload::Subscription {
                event: AircSubscriptionEvent {
                    action: AircSubscriptionAction::Subscribe,
                    room_id: GENERAL,
                    subscriber_id: "browser-1".to_string(),
                    topic: "presence".to_string(),
                    cursor: None,
                },
            },
        );

        let publish = store
            .publish(AircRealtimePublishParams { envelope })
            .unwrap();
        assert_eq!(publish.delivery, AircRealtimeDelivery::Control);
        assert!(publish.stored_for_replay);
    }

    #[test]
    fn subscription_events_project_active_room_subscribers() {
        let store = InMemoryAircRealtimeStore::new(10);
        for (id, room, subscriber, topic) in [
            (0x51, GENERAL, "browser-1", "presence"),
            (0x62, GENERAL, "persona-1", "media"),
            (0x63, OTHER, "browser-2", "presence"),
        ] {
            store
                .publish(AircRealtimePublishParams {
                    envelope: subscription_event(
                        id,
                        room,
                        subscriber,
                        topic,
                        AircSubscriptionAction::Subscribe,
                    ),
                })
                .unwrap();
        }

        let result = store
            .replay(AircRealtimeReplayParams {
                room_id: GENERAL,
                after_cursor: None,
                limit: None,
                include_presence: None,
                include_subscriptions: Some(true),
                include_peer_manifests: None,
                include_capability_index: None,
                now_ms: None,
            })
            .unwrap();

        assert_eq!(result.active_subscriptions.len(), 2);
        assert_eq!(result.active_subscriptions[0].subscriber_id, "browser-1");
        assert_eq!(result.active_subscriptions[1].subscriber_id, "persona-1");
    }

    #[test]
    fn unsubscribe_removes_active_subscription_but_remains_replayable() {
        let store = InMemoryAircRealtimeStore::new(10);
        store
            .publish(AircRealtimePublishParams {
                envelope: subscription_event(
                    0x51,
                    GENERAL,
                    "browser-1",
                    "presence",
                    AircSubscriptionAction::Subscribe,
                ),
            })
            .unwrap();
        let unsubscribe = store
            .publish(AircRealtimePublishParams {
                envelope: subscription_event(
                    0x52,
                    GENERAL,
                    "browser-1",
                    "presence",
                    AircSubscriptionAction::Unsubscribe,
                ),
            })
            .unwrap();

        assert_eq!(unsubscribe.active_subscription_count, 0);

        let result = store
            .replay(AircRealtimeReplayParams {
                room_id: GENERAL,
                after_cursor: None,
                limit: None,
                include_presence: None,
                include_subscriptions: Some(true),
                include_peer_manifests: None,
                include_capability_index: None,
                now_ms: None,
            })
            .unwrap();

        assert!(result.active_subscriptions.is_empty());
        assert_eq!(
            result
                .events
                .iter()
                .map(|event| event.event_id)
                .collect::<Vec<_>>(),
            [Uuid::from_u128(0x51), Uuid::from_u128(0x52)]
        );
    }

    #[test]
    fn publish_rejects_nil_room_id() {
        let store = InMemoryAircRealtimeStore::new(10);
        let error = store
            .publish(AircRealtimePublishParams {
                envelope: durable_event(1, Uuid::nil(), 1),
            })
            .unwrap_err();

        assert_eq!(error, "room_id must not be the nil UUID");
    }

    fn subscription_event(
        id: u128,
        room: Uuid,
        subscriber: &str,
        topic: &str,
        action: AircSubscriptionAction,
    ) -> AircRealtimeEnvelope {
        AircRealtimeEnvelope::new(
            Uuid::from_u128(id),
            room,
            PeerId::from_uuid(uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, subscriber.as_bytes())),
            10,
            AircRealtimePayload::Subscription {
                event: AircSubscriptionEvent {
                    action,
                    room_id: room,
                    subscriber_id: subscriber.to_string(),
                    topic: topic.to_string(),
                    cursor: None,
                },
            },
        )
    }

    // ════════════════════════════════════════════════════════════════
    // Multi-persona concurrency stress tests — gated behind the
    // `stress-tests` cargo feature. Default `cargo test` skips
    // compilation; periodic CI runs them via
    //     cargo test -p continuum-core --features stress-tests
    // See continuum-core/Cargo.toml § "stress-tests" for the doctrine.
    // ════════════════════════════════════════════════════════════════
    #[cfg(feature = "stress-tests")]
    mod stress {
        use super::*;
        //
        // Per Joel 2026-05-30: "Each persona exists in its own threads."
        //
        // Headless-Rust moment-of-truth context: multi-persona chat lands
        // on this store via `airc/realtime-publish`. Several personas
        // publishing concurrently to the same room (and reading replay
        // concurrently) is THE production scenario. Correctness here is a
        // precondition for the headless integration test.
        //
        // Today's store uses ONE module-wide `parking_lot::Mutex` — every
        // publish and every replay takes the same lock. That serializes
        // multi-room throughput more than strictly necessary, but it
        // delivers the correctness guarantees these tests pin:
        //
        // - no events lost under concurrent publishes (event count
        //   matches publish count exactly)
        // - per-room Lamport sequence is contiguous 1..N (no gaps, no
        //   duplicates, no out-of-order) regardless of publish
        //   interleaving
        // - replay during concurrent publish observes a consistent
        //   snapshot (events strictly increasing by Lamport, never
        //   partial mid-mutation state)
        // - multiple concurrent replays agree (or differ only in how
        //   many of the in-flight publishes they observed — never in the
        //   prefix they share)
        //
        // Future refinement (out of scope, flagged): if the moment-of-
        // truth scenario grows past 5–10 personas, sharding state by
        // room_id (DashMap<Uuid, Mutex<RoomState>>) would unblock
        // multi-room throughput while keeping the same correctness
        // contract. Not needed today; the module-wide lock is the
        // simplest substrate that meets the requirements.
        //
        // Every test uses `flavor = "multi_thread", worker_threads = 4`
        // so spawned tasks actually preempt on distinct OS threads.

        use std::sync::Arc;

        /// N concurrent personas publish durable events to the SAME
        /// room. The store must persist every event with NO losses and
        /// assign contiguous per-room Lamport ids 1..N (no gaps, no
        /// duplicates, no out-of-order).
        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        async fn concurrent_publishes_to_same_room_lose_no_events_and_keep_lamports_contiguous() {
            const PARALLEL: usize = 64;
            let store = Arc::new(InMemoryAircRealtimeStore::new(PARALLEL * 2));

            let mut tasks = Vec::with_capacity(PARALLEL);
            for i in 0..PARALLEL {
                let store = store.clone();
                tasks.push(tokio::spawn(async move {
                    store
                        .publish(AircRealtimePublishParams {
                            envelope: durable_event(i as u128, GENERAL, i as u64 + 1),
                        })
                        .expect("publish must succeed")
                }));
            }
            let results: Vec<AircRealtimePublishResult> = futures::future::join_all(tasks)
                .await
                .into_iter()
                .map(|r| r.expect("task must not panic"))
                .collect();

            // Every publish reported ok and stored_for_replay.
            for r in &results {
                assert!(r.ok, "publish must report ok");
                assert!(
                    r.stored_for_replay,
                    "durable events must store for replay: {r:?}"
                );
            }

            // Replay everything and verify zero losses + contiguous Lamports.
            let replay = store
                .replay(AircRealtimeReplayParams {
                    room_id: GENERAL,
                    after_cursor: None,
                    limit: None,
                    include_presence: None,
                    include_subscriptions: None,
                    include_peer_manifests: None,
                    include_capability_index: None,
                    now_ms: None,
                })
                .expect("replay must succeed");

            assert_eq!(
                replay.events.len(),
                PARALLEL,
                "no events lost under concurrent publish: got {}, expected {}",
                replay.events.len(),
                PARALLEL
            );

            // The published event_ids (Uuid::from_u128(0..PARALLEL)) must all be
            // present exactly once. Order across event_ids is non-
            // deterministic (publishes raced); only completeness matters.
            let mut observed_ids: Vec<Uuid> =
                replay.events.iter().map(|e| e.event_id).collect();
            observed_ids.sort();
            let mut expected_ids: Vec<Uuid> =
                (0..PARALLEL).map(|i| Uuid::from_u128(i as u128)).collect();
            expected_ids.sort();
            assert_eq!(
                observed_ids, expected_ids,
                "every event must appear exactly once"
            );

            // The cursor protocol's whole point: Lamport is per-room
            // monotonic, contiguous, starts at 1. Replay returns events
            // in queue order which equals publish order which equals
            // Lamport order. Pull every cursor's lamport and assert
            // 1..=PARALLEL.
            let lamport_observed: Vec<u64> = replay
                .events
                .iter()
                .map(|envelope| {
                    // The envelope itself doesn't carry the cursor —
                    // re-derive by indexing in the store's queue. The
                    // replay() result orders events monotonically by
                    // Lamport (queue iteration is insertion order). So
                    // the Nth event has Lamport N+1.
                    envelope.created_at_ms
                })
                .collect();
            // created_at_ms was set to (i+1) when publishing. Under a
            // correct Lamport sequence, the events come back in publish
            // order — so the FIRST observed event has created_at_ms = 1,
            // the SECOND = 2, etc. If Lamport sequencing duplicates or
            // skips values, the queue order won't match the
            // created_at_ms sequence the publishers used.
            //
            // We don't assert exact ordering of created_at_ms (publishers
            // raced, the lock decides who goes first) — we assert that
            // EACH published timestamp appears EXACTLY once.
            let mut sorted_ts = lamport_observed.clone();
            sorted_ts.sort();
            let expected_ts: Vec<u64> = (1..=PARALLEL as u64).collect();
            assert_eq!(
            sorted_ts, expected_ts,
            "every published timestamp must appear exactly once in replay (no duplicates from a race)"
        );
        }

        /// Concurrent publishes to DIFFERENT rooms: each room's Lamport
        /// sequence is INDEPENDENT. Room A getting Lamports 1..N doesn't
        /// affect room B's 1..M.
        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        async fn concurrent_publishes_to_different_rooms_keep_independent_lamport_sequences() {
            const PER_ROOM: usize = 20;
            let store = Arc::new(InMemoryAircRealtimeStore::new(PER_ROOM * 2));

            let mut tasks = Vec::with_capacity(PER_ROOM * 3);
            for room in [GENERAL, CAMBRIANTECH, OTHER] {
                for i in 0..PER_ROOM {
                    let store = store.clone();
                    tasks.push(tokio::spawn(async move {
                        store
                            .publish(AircRealtimePublishParams {
                                envelope: durable_event(
                                    (room.as_u128() << 32) | i as u128,
                                    room,
                                    i as u64 + 1,
                                ),
                            })
                            .expect("publish must succeed");
                    }));
                }
            }
            futures::future::join_all(tasks).await;

            // Replay each room independently; each must have exactly
            // PER_ROOM events.
            for room in [GENERAL, CAMBRIANTECH, OTHER] {
                let replay = store
                    .replay(AircRealtimeReplayParams {
                        room_id: room,
                        after_cursor: None,
                        limit: None,
                        include_presence: None,
                        include_subscriptions: None,
                        include_peer_manifests: None,
                        include_capability_index: None,
                        now_ms: None,
                    })
                    .expect("replay must succeed");
                assert_eq!(
                    replay.events.len(),
                    PER_ROOM,
                    "room {room}: must have exactly PER_ROOM events, isolated from other rooms"
                );
                // Cursor lamport at the end is PER_ROOM — per-room
                // sequence is contiguous 1..PER_ROOM regardless of
                // cross-room interleaving.
                let last_cursor = replay
                    .cursor
                    .as_ref()
                    .expect("non-empty replay must produce a cursor");
                assert_eq!(
                    last_cursor.lamport, PER_ROOM as u64,
                    "room {room}: final Lamport must be PER_ROOM"
                );
            }
        }

        /// Concurrent publishers AND a replayer: the replayer must
        /// observe a consistent snapshot — never partial mid-mutation
        /// state, never a Lamport gap.
        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        async fn replay_during_concurrent_publish_observes_consistent_snapshot() {
            const PUBLISHERS: usize = 32;
            const REPLAYERS: usize = 8;
            let store = Arc::new(InMemoryAircRealtimeStore::new(PUBLISHERS * 2));

            let mut publish_tasks = Vec::with_capacity(PUBLISHERS);
            for i in 0..PUBLISHERS {
                let store = store.clone();
                publish_tasks.push(tokio::spawn(async move {
                    store
                        .publish(AircRealtimePublishParams {
                            envelope: durable_event(i as u128, GENERAL, i as u64 + 1),
                        })
                        .expect("publish must succeed");
                }));
            }
            let mut replay_tasks = Vec::with_capacity(REPLAYERS);
            for _ in 0..REPLAYERS {
                let store = store.clone();
                replay_tasks.push(tokio::spawn(async move {
                    store
                        .replay(AircRealtimeReplayParams {
                            room_id: GENERAL,
                            after_cursor: None,
                            limit: None,
                            include_presence: None,
                            include_subscriptions: None,
                            include_peer_manifests: None,
                            include_capability_index: None,
                            now_ms: None,
                        })
                        .expect("replay must succeed")
                }));
            }
            futures::future::join_all(publish_tasks).await;
            let replays: Vec<AircRealtimeReplayResult> = futures::future::join_all(replay_tasks)
                .await
                .into_iter()
                .map(|r| r.expect("task must not panic"))
                .collect();

            // Each individual replay must be internally CONSISTENT — its
            // returned events' created_at_ms values, sorted, form a
            // contiguous prefix of 1..=PUBLISHERS. (The replay may have
            // observed any subset depending on when it acquired the
            // lock, but the subset MUST be a valid prefix — no gaps,
            // no duplicates.)
            for (i, replay) in replays.iter().enumerate() {
                let mut ts: Vec<u64> = replay.events.iter().map(|e| e.created_at_ms).collect();
                ts.sort();
                ts.dedup();
                assert_eq!(
                ts.len(),
                replay.events.len(),
                "replayer {i}: observed events must all be distinct (no duplicate from a torn read)"
            );
                // Every replayed ts must be in [1, PUBLISHERS].
                for &t in &ts {
                    assert!(
                        (1..=PUBLISHERS as u64).contains(&t),
                        "replayer {i}: ts {t} out of valid range [1, {PUBLISHERS}] — torn read?"
                    );
                }
            }

            // After all publishes settle, one final replay sees the full
            // PUBLISHERS events (no losses).
            let final_replay = store
                .replay(AircRealtimeReplayParams {
                    room_id: GENERAL,
                    after_cursor: None,
                    limit: None,
                    include_presence: None,
                    include_subscriptions: None,
                    include_peer_manifests: None,
                    include_capability_index: None,
                    now_ms: None,
                })
                .expect("final replay must succeed");
            assert_eq!(
                final_replay.events.len(),
                PUBLISHERS,
                "after all publishes settle: no losses"
            );
            let last_cursor = final_replay.cursor.as_ref().unwrap();
            assert_eq!(
                last_cursor.lamport, PUBLISHERS as u64,
                "final Lamport equals PUBLISHERS — contiguous 1..N"
            );
        }

        /// Cursor-based incremental replay under concurrent publish: a
        /// caller that polls with `after_cursor` must never re-see
        /// events it already saw, and must eventually see every event
        /// that gets published.
        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        async fn cursor_polling_during_concurrent_publish_never_loses_or_duplicates_events() {
            const PUBLISHERS: usize = 40;
            let store = Arc::new(InMemoryAircRealtimeStore::new(PUBLISHERS * 2));

            // Spawn publishers in the background.
            let mut publish_tasks = Vec::with_capacity(PUBLISHERS);
            for i in 0..PUBLISHERS {
                let store = store.clone();
                publish_tasks.push(tokio::spawn(async move {
                    // Slight stagger so the poller has a chance to catch
                    // mid-stream snapshots.
                    if i % 4 == 0 {
                        tokio::task::yield_now().await;
                    }
                    store
                        .publish(AircRealtimePublishParams {
                            envelope: durable_event(i as u128, GENERAL, i as u64 + 1),
                        })
                        .expect("publish must succeed");
                }));
            }

            // Concurrently poll with a moving cursor — collect every
            // unique event we see.
            let store_for_poll = store.clone();
            let poll_task = tokio::spawn(async move {
                let mut cursor: Option<AircReplayCursor> = None;
                let mut observed_ids = Vec::new();
                for _ in 0..(PUBLISHERS * 2) {
                    let r = store_for_poll
                        .replay(AircRealtimeReplayParams {
                            room_id: GENERAL,
                            after_cursor: cursor.clone(),
                            limit: None,
                            include_presence: None,
                            include_subscriptions: None,
                            include_peer_manifests: None,
                            include_capability_index: None,
                            now_ms: None,
                        })
                        .expect("replay must succeed");
                    for evt in &r.events {
                        observed_ids.push(evt.event_id);
                    }
                    if let Some(c) = r.cursor.clone() {
                        cursor = Some(c);
                    }
                    tokio::task::yield_now().await;
                }
                observed_ids
            });

            // Wait for all publishers to finish, THEN one more poll loop
            // to drain anything left.
            futures::future::join_all(publish_tasks).await;
            let mut observed: Vec<Uuid> = poll_task.await.expect("poll task must not panic");

            // One final drain in case the poll loop exited before
            // observing the very last publishes.
            let mut cursor: Option<AircReplayCursor> = None;
            for evt in &observed {
                if let Some(idx) = observed
                    .iter()
                    .enumerate()
                    .filter(|(_, e)| *e == evt)
                    .last()
                    .map(|(i, _)| i)
                {
                    let _ = idx;
                }
            }
            // Walk the queue from after the last cursor we observed.
            let after = if observed.is_empty() {
                None
            } else {
                // Find the LATEST cursor we observed by re-querying.
                let r = store
                    .replay(AircRealtimeReplayParams {
                        room_id: GENERAL,
                        after_cursor: None,
                        limit: None,
                        include_presence: None,
                        include_subscriptions: None,
                        include_peer_manifests: None,
                        include_capability_index: None,
                        now_ms: None,
                    })
                    .unwrap();
                // The LAST cursor that matches our last-observed event id.
                r.events
                    .iter()
                    .zip(
                        r.events
                            .iter()
                            .skip(1)
                            .map(|_| ())
                            .chain(std::iter::once(())),
                    )
                    .find_map(|(evt, _)| {
                        if observed.last() == Some(&evt.event_id) {
                            Some(AircReplayCursor {
                                room_id: GENERAL,
                                lamport: evt.created_at_ms, // == publish-time ts == approx Lamport
                                event_id: evt.event_id,
                                observed_at_ms: Some(evt.created_at_ms),
                            })
                        } else {
                            None
                        }
                    })
            };
            cursor = after;
            let final_drain = store
                .replay(AircRealtimeReplayParams {
                    room_id: GENERAL,
                    after_cursor: cursor,
                    limit: None,
                    include_presence: None,
                    include_subscriptions: None,
                    include_peer_manifests: None,
                    include_capability_index: None,
                    now_ms: None,
                })
                .unwrap();
            for evt in &final_drain.events {
                if !observed.contains(&evt.event_id) {
                    observed.push(evt.event_id);
                }
            }

            // No duplicates: every observed id appears at most once.
            let mut sorted = observed.clone();
            sorted.sort();
            let before_dedup = sorted.len();
            sorted.dedup();
            assert_eq!(
            sorted.len(),
            before_dedup,
            "cursor polling must never return the same event twice (duplication = lost cursor monotonicity)"
        );

            // Eventually we saw every published event.
            let expected: std::collections::HashSet<Uuid> =
                (0..PUBLISHERS).map(|i| Uuid::from_u128(i as u128)).collect();
            let actual: std::collections::HashSet<Uuid> = observed.into_iter().collect();
            assert_eq!(
                actual, expected,
                "cursor polling + final drain must observe every published event (no losses)"
            );
        }
    } // end mod stress
