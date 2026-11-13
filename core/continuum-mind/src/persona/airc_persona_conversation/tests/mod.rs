#![cfg(test)]

    use super::*;
    use crate::persona::airc_citizen::StubAircCitizen;

    // Regression for a4747386: stream/heartbeat traffic reached the actual pump
    // queue and exhausted ready intake before a complete directed room message.
    #[tokio::test]
    pub(super) async fn pump_drops_non_perceptual_flood_before_ready_intake() {
        use super::*;
        use crate::persona::airc_citizen::StubAircCitizen;
        use futures::StreamExt;

        let own = Uuid::new_v4();
        let peer = Uuid::new_v4();
        let room = Uuid::new_v4();
        let runtime = Arc::new(StubAircCitizen::new(own).with_rooms(vec![room]));
        let mut conversation = AircPersonaConversation::new(runtime);
        conversation.rooms = Some(vec![room]);
        let event = |text: &str| {
            event_from_row(
                room,
                crate::persona::durable_history::RoomRow {
                    id: Uuid::new_v4(),
                    sender: peer,
                    occurred_at_ms: 1,
                    media: Vec::new(),
                    text: text.into(),
                },
            )
        };
        let mut frames = Vec::new();
        for index in 0..CATCH_UP_PAGE * 2 {
            let mut noise = event("not a completed utterance");
            noise.headers.insert(
                match index % 3 {
                    0 => airc_lib::HEADER_STREAM_ID,
                    1 => airc_lib::HEADER_HEARTBEAT_KIND,
                    _ => airc_protocol::HEADER_AIRC_CORRELATION_ID,
                }
                .into(),
                "fixture".into(),
            );
            frames.push(Ok(Arc::new(noise)));
        }
        // Even textual RPC results are not a colleague speaking. A completed
        // directed room message still passes below.
        let mut rpc = event("RPC result, not speech");
        rpc.headers.insert(airc_protocol::HEADER_AIRC_CORRELATION_ID.into(), Uuid::new_v4().to_string());
        assert_eq!(crate::cognition::conversation::room_event::room_turn_from_event(&rpc), Err("command_frame"));
        for _ in 0..CATCH_UP_PAGE * 2 {
            let mut telemetry = event("");
            telemetry.kind = airc_core::TranscriptKind::System;
            let envelope = serde_json::from_value(serde_json::json!({
                "eventId": telemetry.event_id.to_string(), "roomId": room.to_string(),
                "sourceId": Uuid::new_v4().to_string(), "createdAtMs": 1,
                "delivery": "ephemeral_coalesced",
                "payload": { "kind": "existing_schema", "payload": {
                    "schema": "grid_capacity", "inline": {} }
                }
            })).unwrap();
            telemetry.headers = crate::routing::airc_realtime_wire::headers_for_envelope(&envelope);
            telemetry.body = Some(crate::routing::airc_realtime_wire::body_for_envelope(&envelope).unwrap());
            assert!(matches!(perceptual_from_event(&telemetry), Err("non_chat_schema")));
            frames.push(Ok(Arc::new(telemetry)));
        }
        let mut control = event("");
        control.kind = airc_core::TranscriptKind::System;
        control.body = None;
        let control = Arc::new(control);
        frames.push(Ok(Arc::clone(&control)));
        // Regression #4592: completed work broadcasts are durable presence,
        // not fresh conversational requests, even when they arrive mid-action.
        let presence = event("⚙ code/read fixture.rs ✓");
        assert!(matches!(perceptual_from_event(&presence), Err("work_presence")));
        frames.push(Ok(Arc::new(presence)));
        // 2dcaacc1: typed claim renewals retain their durable board owner, but
        // must not flood either the one feed or the live/catch-up attention
        // classifier. A released claim is still a meaningful work fact.
        let card_id = airc_work::WorkCardId::new();
        let claim_id = airc_work::ClaimId::new();
        for index in 0..CATCH_UP_PAGE * 2 {
            let renewal = airc_work::WorkEvent::ClaimHeartbeat(airc_work::ClaimHeartbeat {
                card_id, claim_id, owner: airc_core::PeerId::from_uuid(peer),
                ttl_ms: 60_000, heartbeat_at_ms: index as u64,
            });
            let mut renewed = event("");
            let (mut headers, body) = airc_work::encode_work_event(&renewal).unwrap();
            // A routing hint cannot turn a decoded renewal into a task change.
            headers.insert(airc_work::HEADER_FORGE_WORK_EVENT_KIND.into(), "card_state_changed".into());
            renewed.headers = headers;
            renewed.body = Some(body);
            assert!(matches!(crate::persona::perception_feed::classify(&renewed),
                crate::persona::perception_feed::Fed::NotPerception("work_lease_renewal")));
            assert!(AttentionEvent::classify(Arc::new(renewed.clone())).is_none());
            frames.push(Ok(Arc::new(renewed)));
        }
        let release = airc_work::WorkEvent::ClaimReleased(airc_work::ClaimReleased {
            card_id, claim_id, owner: airc_core::PeerId::from_uuid(peer),
            reason: Some("work remains available".into()), released_at_ms: 1,
            taken_over_from: Some(airc_core::PeerId::from_uuid(own)),
        });
        let mut released = event("");
        let (headers, body) = airc_work::encode_work_event(&release).unwrap();
        released.headers = headers;
        released.body = Some(body);
        assert!(matches!(crate::persona::perception_feed::classify(&released),
            crate::persona::perception_feed::Fed::OtherWork(_)));
        assert!(AttentionEvent::classify(Arc::new(released)).is_some());
        let mut work = event("");
        work.kind = airc_core::TranscriptKind::System;
        let (headers, body) = airc_work::encode_work_event(
            &airc_work::WorkEvent::CardStateChanged(airc_work::CardStateChanged {
                card_id: airc_work::WorkCardId::new(), state: airc_work::CardState::Open,
                changed_by: airc_core::PeerId::from_uuid(peer), changed_at_ms: 1,
            }),
        ).unwrap();
        work.headers = headers;
        // Optional routing metadata must not gate valid work-bridge payloads.
        work.headers.remove(airc_work::HEADER_FORGE_WORK_EVENT_KIND);
        work.body = Some(body);
        // Both bridge consumers accept valid body hints without a kind header.
        // Exercise the review variant through the same admission owner too.
        let media = serde_json::from_value(serde_json::json!({
            "hash": "b".repeat(64), "size_bytes": 12
        })).unwrap();
        let review = airc_work::WorkEvent::WorkSubmissionReviewed(airc_work::WorkSubmissionReview {
            review_id: airc_work::WorkReviewId::new(), card_id: airc_work::WorkCardId::new(),
            submission_id: airc_work::SubmissionId::new(), artifact: media,
            review_card_id: airc_work::WorkCardId::new(), review_claim_id: airc_work::ClaimId::new(),
            reviewer: airc_core::PeerId::from_uuid(peer), outcome: airc_work::WorkReviewOutcome::Passed,
            evidence: serde_json::from_value(serde_json::json!({ "hash": "c".repeat(64), "size_bytes": 12 })).unwrap(),
            evidence_text: None, reviewed_at_ms: 1,
        });
        let mut review_event = event("");
        let (mut headers, body) = airc_work::encode_work_event(&review).unwrap();
        headers.remove(airc_work::HEADER_FORGE_WORK_EVENT_KIND);
        review_event.headers = headers;
        review_event.body = Some(body);
        assert!(matches!(AttentionEvent::classify(Arc::new(review_event)).unwrap().perception.unwrap().work,
            Some(crate::cognition::conversation::room_event::RoomWork::Reviewed { .. })));
        let work_id = work.event_id.as_uuid();
        frames.push(Ok(Arc::new(work)));
        let mut directed = event("@citizen Shared Cargo owner: please wait");
        directed.target = airc_core::MentionTarget::Peer(airc_core::PeerId::from_uuid(own));
        let directed = Arc::new(directed);
        frames.push(Ok(Arc::clone(&directed)));
        // Terminal stream fence: observing it proves every preceding frame went
        // through the real pump's send/skip branch. No sleep-based absence claim.
        let (drained, ready) = tokio::sync::oneshot::channel();
        let stream = futures::stream::iter(frames).chain(futures::stream::once(async move {
            drained.send(()).unwrap();
            std::future::pending().await
        }));
        conversation.install_stream(stream);
        tokio::time::timeout(std::time::Duration::from_secs(5), ready)
            .await
            .unwrap()
            .unwrap();
        let inbox = conversation.inbox.as_mut().unwrap();
        assert_eq!(
            inbox.len(),
            2,
            "work facts and completed input reach attention; telemetry and presence keep their durable owners"
        );
        assert!(conversation
            .seen
            .lock()
            .unwrap()
            .was_seen(room, directed.event_id.as_uuid()));
        let perceived = conversation.perceive_ready().await.unwrap();
        assert_eq!(perceived.len(), 2);
        assert_eq!(perceived[0].event_id, work_id);
        assert!(matches!(perceived[0].work, Some(crate::cognition::conversation::room_event::RoomWork::StateChanged { .. })));
        assert_eq!(perceived[1].event_id, directed.event_id.as_uuid());
        assert_eq!(perceived[1].room_id, room);
        assert_eq!(perceived[1].peer_id, peer);
        assert!(perceived[1].text.contains("Shared Cargo owner"));

        // Decode faults remain observable, but exhausting their scan slice must
        // yield cooperatively, not pretend the next directed input does not exist.
        // Distinct events: the pump admits one event id once, so N copies of one
        // control frame would no longer fill N positions.
        let mut page: Vec<_> = (0..CATCH_UP_PAGE * 2)
            .map(|_| {
                let mut distinct = (*control).clone();
                distinct.event_id = airc_core::EventId::new();
                distinct.headers.insert(airc_work::HEADER_FORGE_WORK_EVENT_KIND.into(), "malformed-fixture".into());
                assert!(matches!(AttentionEvent::classify(Arc::new(distinct.clone())).unwrap().perception, Err("work_event_decode_error")));
                Ok(Arc::new(distinct))
            })
            .collect();
        page.push(Ok(Arc::new(event("directed input after more than one decode-fault page"))));
        let (drained, ready) = tokio::sync::oneshot::channel();
        conversation.install_stream(futures::stream::iter(page).chain(
            futures::stream::once(async move {
                drained.send(()).unwrap();
                std::future::pending().await
            }),
        ));
        tokio::time::timeout(std::time::Duration::from_secs(5), ready).await.unwrap().unwrap();
        let last = conversation.next_message_inner(false, false).await.unwrap().unwrap();
        assert_eq!(last.text, "directed input after more than one decode-fault page");

        // what this catches (5090, 2026-10-04): the one feed runs beside the private
        // subscription until that retires, so one event can arrive on both. The pump
        // admits it once; a second copy must not become a second turn.
        let twice = Arc::new(event("one event, two sources"));
        let (drained, ready) = tokio::sync::oneshot::channel();
        conversation.install_stream(
            futures::stream::iter([Ok(Arc::clone(&twice)), Ok(Arc::clone(&twice))]).chain(
                futures::stream::once(async move {
                    drained.send(()).unwrap();
                    std::future::pending().await
                }),
            ),
        );
        tokio::time::timeout(std::time::Duration::from_secs(5), ready).await.unwrap().unwrap();
        assert_eq!(conversation.inbox.as_mut().unwrap().len(), 1, "one event on two sources is admitted once");

        // Error frames must still reach the consumer, never disappear as noise.
        let (drained, ready) = tokio::sync::oneshot::channel();
        conversation.install_stream(
            futures::stream::iter([Err(airc_lib::LiveLag { skipped: 7 })]).chain(
                futures::stream::once(async move {
                    drained.send(()).unwrap();
                    std::future::pending().await
                }),
            ),
        );
        tokio::time::timeout(std::time::Duration::from_secs(5), ready)
            .await
            .unwrap()
            .unwrap();
        let error = conversation
            .next_message_inner(false, false)
            .await
            .unwrap_err();
        assert!(error.contains("lagged 7"), "{error}");
        conversation.stop_stream();
    }

    // what this catches: speech in her mind room published to airc (PRIVACY-OF-THOUGHT.md
    // §4). It reaches its only member, her, and nothing goes out: without the gate the
    // runtime would publish to a room NAMED by the mind room's uuid, which this scope
    // has never joined.
    #[tokio::test]
    pub(super) async fn speech_in_her_mind_room_is_never_published() {
        use crate::persona::identity_provider::PersonaIdentitySource;
        use crate::persona::PersonaAircRuntime;
        let home = tempfile::tempdir().unwrap();
        let airc = Arc::new(airc_lib::Airc::open_with_wire_root_for_test(home.path(), home.path())
            .await.unwrap());
        let room = airc.join("open-room").await.unwrap().channel;
        let runtime = Arc::new(PersonaAircRuntime::from_attached(
            airc.peer_id().as_uuid(), "open-room", home.path().to_path_buf(),
            airc.clone(), room, PersonaIdentitySource::FreshlyMinted,
        ));
        let conversation = AircPersonaConversation::new(runtime.clone());
        let mind = crate::cognition::mind_room::mind_room_id(airc.peer_id().as_uuid());
        conversation.say_in(mind, "a private word").await.expect("said to herself, never published");
        conversation.say_in(room.as_uuid(), "an open word").await.expect("an open room still publishes");
    }

    // Regression: priming establishes readiness once; the live loop must not
    // make a second fallible history RPC after the host reports attachment.
    #[tokio::test]
    pub(super) async fn initial_cutoff_is_established_by_prime_and_remains_stable() {
        use crate::persona::identity_provider::PersonaIdentitySource;
        use crate::persona::PersonaAircRuntime;
        let home = tempfile::tempdir().unwrap();
        let airc = Arc::new(airc_lib::Airc::open_with_wire_root_for_test(home.path(), home.path())
            .await.unwrap());
        let room = airc.join("readiness-test").await.unwrap().channel;
        let runtime = Arc::new(PersonaAircRuntime::from_attached(
            airc.peer_id().as_uuid(), "readiness-test", home.path().to_path_buf(),
            airc.clone(), room, PersonaIdentitySource::FreshlyMinted,
        ));
        runtime.say_in(room.as_uuid(), "before priming").await.unwrap();
        let mut conversation = AircPersonaConversation::new(runtime.clone());
        assert!(conversation.initial_water_mark(64).await.is_err());
        conversation.prime().await.unwrap();
        let cutoff = conversation.initial_water_mark(64).await.unwrap();
        assert!(cutoff > 0);
        runtime.say_in(room.as_uuid(), "after priming").await.unwrap();
        assert!(conversation.high_water_mark(64).await.unwrap() > cutoff);
        conversation.prime().await.unwrap();
        assert_eq!(conversation.initial_water_mark(1).await.unwrap(), cutoff);
    }

    // what this catches: c5910be2 — ready intake must use the real decoder, keep
    // both rooms, return promptly on an empty inbox, and retain source ownership
    // if a drive drops its snapshot before submitting another request.
    #[tokio::test]
    pub(super) async fn ready_room_input_is_shared_and_retained_for_later_attention() {
        use super::*;
        use crate::persona::airc_citizen::StubAircCitizen;
        let own = Uuid::new_v4();
        let peer = Uuid::new_v4();
        let rooms = vec![Uuid::new_v4(), Uuid::new_v4()];
        let runtime = Arc::new(StubAircCitizen::new(own).with_rooms(rooms.clone()));
        let mut conversation = AircPersonaConversation::new(runtime);
        // Lease the existing in-process inbox at the pump/consumer boundary;
        // no daemon or transport owner is started by this regression.
        let (tx, rx) = tokio::sync::mpsc::channel(4);
        conversation.rooms = Some(rooms.clone());
        conversation.inbox = Some(rx);
        let mut ids = Vec::new();
        for (index, room) in rooms.iter().enumerate() {
            let id = Uuid::new_v4();
            ids.push(id);
            let event = event_from_row(
                *room,
                crate::persona::durable_history::RoomRow {
                    id,
                    sender: peer,
                    occurred_at_ms: index as u64,
                    media: Vec::new(),
                    text: format!("colleague input {index}"),
                },
            );
            tx.send(Ok(AttentionEvent::classify(Arc::new(event)).unwrap())).await.unwrap();
        }
        crate::cognition::directed_pending::signal(own);
        let batch = crate::cognition::directed_pending::observe(own);
        let snapshot = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            conversation.perceive_ready(),
        )
        .await
        .expect("ready drain must not wait for another arrival")
        .unwrap();
        let snapshot = snapshot.clone();
        assert_eq!(snapshot.len(), 2);
        let repeated = conversation.perceive_ready().await.unwrap();
        for (index, message) in snapshot.iter().enumerate() {
            assert_eq!(message.event_id, ids[index]);
            assert_eq!(message.room_id, rooms[index]);
            assert_eq!(message.peer_id, peer);
            assert!(
                Arc::ptr_eq(message, &repeated[index]),
                "snapshots lease the same input object"
            );
        }
        drop(snapshot);
        for id in ids {
            assert_eq!(
                conversation.next_message().await.unwrap().unwrap().event_id,
                id
            );
        }
        assert!(conversation.perceive_ready().await.unwrap().is_empty());

        // A directed arrival crosses the actual inbox after the older batch was
        // consumed, but before its driver acknowledges it. The wake and source
        // must both survive that stale acknowledgement.
        let late_id = Uuid::new_v4();
        let late = event_from_row(rooms[0], crate::persona::durable_history::RoomRow {
            id: late_id,
            sender: peer,
            occurred_at_ms: 3,
            media: Vec::new(),
            text: "new steering after the drained batch".into(),
        });
        tx.send(Ok(AttentionEvent::classify(Arc::new(late)).unwrap())).await.unwrap();
        crate::cognition::directed_pending::signal(own);
        crate::cognition::directed_pending::acknowledge(own, batch);
        tokio::time::timeout(
            std::time::Duration::from_secs(1),
            crate::cognition::directed_pending::wait(own),
        ).await.expect("old batch acknowledgement must preserve the newer wake");
        let late_batch = crate::cognition::directed_pending::observe(own);
        assert_eq!(conversation.next_message().await.unwrap().unwrap().event_id, late_id);
        crate::cognition::directed_pending::acknowledge(own, late_batch);
        assert!(!crate::cognition::directed_pending::is_pending(own));
        assert!(conversation.perceive_ready().await.unwrap().is_empty());
    }

    // what this catches: the dead-live-stream detector. The first tick after an
    // open admits the backlog and must not re-open; a later tick that admits
    // store events while zero live frames arrived is the dead stream (the 03:11Z
    // daemon restart); a tick that admits nothing, or one where live frames flowed,
    // is healthy. usize::MAX is the inbox-gone sentinel, never a count.
    #[test]
    pub(super) fn the_live_stream_is_dead_only_when_the_store_delivers_and_the_stream_did_not() {
        assert!(
            !live_stream_missed(0, 5, true),
            "first tick admits the backlog"
        );
        assert!(
            live_stream_missed(0, 5, false),
            "store delivered, stream silent"
        );
        assert!(!live_stream_missed(3, 5, false), "live frames flowed");
        assert!(!live_stream_missed(0, 0, false), "nothing to deliver");
        assert!(
            !live_stream_missed(0, usize::MAX, false),
            "inbox gone is not a count"
        );
    }

mod part_2;
