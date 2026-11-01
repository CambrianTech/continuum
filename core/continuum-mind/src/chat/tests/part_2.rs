#![cfg(test)]
use super::*;

    // ── Partial failure: data ok + airc fail ─────────────────────────
    #[tokio::test]
    pub(super) async fn send_with_airc_failure_returns_warning_and_null_event_id() {
        let chat = chat_with_stubs(vec![
            Arc::new(StubDataModule::new(|_cmd, _p| {
                Ok(batch_ok())
            })),
            Arc::new(StubAircModule::err(
                "airc daemon socket unreachable: ENOENT",
            )),
        ]);

        let result = chat
            .send(sample_send_params())
            .await
            .expect("airc-only failure must be degraded success, NOT command-level Err");

        assert!(
            !result.message_id.is_nil(),
            "message_id present — local store succeeded"
        );
        assert!(
            result.event_id.is_none(),
            "event_id absent when broadcast didn't land"
        );
        let warning = result.warning.as_deref().expect("warning must be set");
        assert!(
            warning.contains("airc/realtime-publish failed"),
            "warning names the failing surface: {warning}"
        );
        assert!(
            warning.contains("ENOENT"),
            "warning surfaces the underlying error so the caller can diagnose: {warning}"
        );
        assert!(
            warning.contains("stored locally"),
            "warning reassures the caller the message wasn't lost: {warning}"
        );
        assert!(
            warning.contains(&result.message_id.to_string()),
            "warning names the message id so the caller can correlate logs: {warning}"
        );
    }

    // ── Hard failure: data fail ──────────────────────────────────────
    #[tokio::test]
    pub(super) async fn send_with_data_executor_failure_propagates_as_err_and_skips_airc() {
        // Track whether airc was called — it must NOT be when data
        // failed (publishing without a local record creates the
        // bad-divergence case the ordering was designed to prevent).
        let airc_calls = Arc::new(AtomicUsize::new(0));
        let airc_calls_tracker = airc_calls.clone();

        let chat = chat_with_stubs(vec![
            Arc::new(StubDataModule::new(|_cmd, _p| {
                Err("sqlite is locked".to_string())
            })),
            Arc::new(StubAircModule::with(move |_p| {
                airc_calls_tracker.fetch_add(1, Ordering::SeqCst);
                Ok(airc_ok_response("should-never-be-called"))
            })),
        ]);

        let err = chat
            .send(sample_send_params())
            .await
            .expect_err("data executor failure must propagate as command-level Err");

        assert!(
            err.contains("chat/send: chat line: data/batch failed"),
            "error must name the failing surface: {err}"
        );
        assert!(
            err.contains("sqlite is locked"),
            "error must surface the underlying cause: {err}"
        );
        assert_eq!(
            airc_calls.load(Ordering::SeqCst),
            0,
            "airc MUST NOT be called when data failed — the ordering invariant"
        );
    }

    #[tokio::test]
    pub(super) async fn send_with_data_success_false_propagates_as_err_and_skips_airc() {
        // Subtle path: the data executor returns Ok (no transport
        // failure) but with success=false (validation error, unique
        // constraint, etc.). Still hard failure from chat's
        // perspective — the message isn't stored.
        let airc_calls = Arc::new(AtomicUsize::new(0));
        let airc_calls_tracker = airc_calls.clone();

        let chat = chat_with_stubs(vec![
            Arc::new(StubDataModule::new(|_cmd, _p| {
                Ok(json!({
                    "success": false,
                    "error": "unique constraint violated on (id)",
                }))
            })),
            Arc::new(StubAircModule::with(move |_p| {
                airc_calls_tracker.fetch_add(1, Ordering::SeqCst);
                Ok(airc_ok_response("should-never-be-called"))
            })),
        ]);

        let err = chat
            .send(sample_send_params())
            .await
            .expect_err("success=false from data must propagate as Err");

        assert!(
            err.contains("the store refused the line"),
            "error must name the failure mode: {err}"
        );
        assert!(
            err.contains("unique constraint"),
            "error must surface the underlying cause: {err}"
        );
        assert_eq!(
            airc_calls.load(Ordering::SeqCst),
            0,
            "success=false also blocks the airc publish — same ordering invariant"
        );
    }

    // ── Ordering invariant: data called BEFORE airc ──────────────────
    #[tokio::test]
    pub(super) async fn send_calls_data_before_airc() {
        // Pin the call order via shared timestamp markers. The
        // ordering invariant is the CORE of the dual-write design;
        // if it ever flips, the bad-divergence case becomes
        // reachable.
        let call_log: Arc<Mutex<Vec<&'static str>>> = Arc::new(Mutex::new(Vec::new()));
        let data_log = call_log.clone();
        let airc_log = call_log.clone();

        let chat = chat_with_stubs(vec![
            Arc::new(StubDataModule::new(move |cmd, _p| {
                if cmd == "data/batch" {
                    data_log.lock().unwrap().push("data/batch");
                }
                Ok(batch_ok())
            })),
            Arc::new(StubAircModule::with(move |_p| {
                airc_log.lock().unwrap().push("airc/realtime-publish");
                Ok(airc_ok_response("evt-order-001"))
            })),
        ]);

        chat.send(sample_send_params())
            .await
            .expect("happy path must succeed");

        let calls = call_log.lock().unwrap().clone();
        assert_eq!(
            calls,
            vec!["data/batch", "airc/realtime-publish"],
            "data MUST be called before airc — the dual-write ordering invariant"
        );
    }

    // ── Wire contract: what chat sends to data ───────────────────────
    #[tokio::test]
    pub(super) async fn send_writes_chat_messages_collection_with_canonical_entity_shape() {
        // The data write must match the TS `ChatMessageEntity` shape
        // so existing TS readers (and chat/poll's response parser)
        // see a consistent entity. Pin every field the TS readers
        // depend on.
        let room_id = Uuid::new_v4();
        let sender_id = Uuid::new_v4();
        let reply_to_id = Uuid::new_v4();

        let observed_create: Arc<Mutex<Option<Value>>> = Arc::new(Mutex::new(None));
        let observer = observed_create.clone();

        let chat = chat_with_stubs(vec![
            Arc::new(StubDataModule::new(move |cmd, params| {
                if cmd == "data/batch" {
                    *observer.lock().unwrap() = Some(params);
                }
                Ok(batch_ok())
            })),
            Arc::new(StubAircModule::ok(airc_ok_response("evt-wire-001"))),
        ]);

        let result = chat
            .send(ChatSendParams {
                room_id,
                sender_id,
                text: "wire contract message".into(),
                reply_to_id: Some(reply_to_id),
                media: Vec::new(),
            })
            .await
            .expect("send must succeed");

        let create = observed_create
            .lock()
            .unwrap()
            .clone()
            .expect("data/batch must have been called");

        assert_eq!(
            create["dbPath"], "main",
            "writes go to the main adapter handle"
        );
        assert_eq!(create["operations"][0]["collection"], "chat_messages");
        assert_eq!(
            create["operations"][0]["id"],
            result.message_id.to_string(),
            "create.id matches the returned message_id"
        );

        let entity = &create["operations"][0]["data"];
        assert_eq!(entity["id"], result.message_id.to_string());
        assert_eq!(entity["roomId"], room_id.to_string());
        assert_eq!(entity["senderId"], sender_id.to_string());
        assert_eq!(entity["content"]["text"], "wire contract message");
        assert_eq!(entity["replyToId"], reply_to_id.to_string());
        assert_eq!(
            entity["metadata"]["source"], "user",
            "default source is 'user' (system messages will need their own param)"
        );
        assert_eq!(entity["status"], "sent");
        assert!(
            entity["timestamp"].is_string(),
            "timestamp is an ISO-8601 string (matches TS ChatMessageEntity)"
        );
        assert!(
            entity["timestamp"].as_str().unwrap().ends_with('Z'),
            "timestamp is UTC"
        );
    }

    // ── Wire contract: what chat sends to airc ───────────────────────
    #[tokio::test]
    pub(super) async fn send_envelope_matches_airc_publish_wire_shape() {
        // Pin the envelope shape chat hands to airc/realtime-publish.
        // If airc's wire contract ever changes, this test catches
        // the drift even though chat doesn't import airc's typed
        // structs.
        let room_id = Uuid::new_v4();
        let sender_id = Uuid::new_v4();

        let observed_publish: Arc<Mutex<Option<Value>>> = Arc::new(Mutex::new(None));
        let observer = observed_publish.clone();

        let chat = chat_with_stubs(vec![
            Arc::new(StubDataModule::new(|_cmd, _p| {
                Ok(batch_ok())
            })),
            Arc::new(StubAircModule::with(move |params| {
                *observer.lock().unwrap() = Some(params);
                Ok(airc_ok_response("evt-envelope-001"))
            })),
        ]);

        let result = chat
            .send(ChatSendParams {
                room_id,
                sender_id,
                text: "envelope shape test".into(),
                reply_to_id: None,
                media: Vec::new(),
            })
            .await
            .expect("send must succeed");

        let publish = observed_publish
            .lock()
            .unwrap()
            .clone()
            .expect("airc/realtime-publish must have been called");

        let envelope = &publish["envelope"];
        // Top-level envelope fields per AircRealtimeEnvelope.
        assert!(
            envelope["eventId"].as_str().is_some(),
            "envelope must carry an eventId (chat mints its own UUID)"
        );
        assert_eq!(envelope["roomId"], room_id.to_string());
        assert_eq!(envelope["sourceId"], sender_id.to_string());
        assert!(envelope["createdAtMs"].is_number());
        assert_eq!(
            envelope["delivery"], "durable",
            "chat transcript → durable delivery (matches the airc payload's delivery() semantics)"
        );

        // Payload tagged-enum shape: AircRealtimePayload::ExistingSchema.
        let payload = &envelope["payload"];
        assert_eq!(
            payload["kind"], "existing_schema",
            "serde-tagged payload variant for the schema-ref shape"
        );
        let inner = &payload["payload"];
        assert_eq!(
            inner["schema"], "chat_transcript",
            "chat messages carry the ChatTranscript schema tag"
        );

        let inline = &inner["inline"];
        assert_eq!(inline["messageId"], result.message_id.to_string());
        assert_eq!(inline["text"], "envelope shape test");
        assert_eq!(inline["senderId"], sender_id.to_string());
        assert!(
            inline["replyToId"].is_null(),
            "no thread anchor for this message"
        );
    }

    // The `chat/send` typed-object path (module `commands()` contribution,
    // shared-slot invocation, legacy fail-loud) is proven alongside `chat/poll`
    // in the "typed object registry path" block above — both verbs share the
    // same `command_objects` family and the same `handle_command` fail-loud arm.
    // ════════════════════════════════════════════════════════════════
    // Multi-persona concurrency stress tests — gated behind the
    // `stress-tests` cargo feature so default `cargo test` doesn't
    // pay the compile cost of the multi-thread runtime + 50-task
    // futures::join_all bodies. Periodic CI runs:
    //     cargo test -p continuum-core --features stress-tests
    //
    // Per Joel 2026-06-08: "Yes half the battle is tests and we
    // wrote all this infra. Need to stop forgetting." Same shape as
    // `test-fixtures` — compile-time gating, not `#[ignore]`.
    // ════════════════════════════════════════════════════════════════
    //
    // Per Joel 2026-05-30: "Each persona exists in its own threads."
    // The kernel registers ONE ChatModule instance; every persona's
    // thread invokes its `&self` methods concurrently. The tests
    // below PIN the invariants the substrate is designed to uphold
    // under that load — they are not exercising rare paths, they are
    // the production scenario.
    #[cfg(feature = "stress-tests")]
    pub(super) mod stress {
        use super::super::*;
        //
        // # Runtime flavor
        //
        // Every concurrency test runs on `flavor = "multi_thread",
        // worker_threads = 4` so the tasks actually preempt each other on
        // distinct OS threads rather than cooperatively interleaving on
        // one. Single-threaded tokio would silently serialize the test
        // and pass even if the substrate had a data race.

        use std::collections::HashMap;
        use std::sync::Mutex as StdMutex;

        /// `chat/send` under N concurrent persona threads, all sharing the
        /// same `ChatModule` instance through the same executor:
        /// - every send must complete (no panics, no lost work)
        /// - every send must return a DISTINCT `message_id` (no UUID
        ///   collision; no shared mutable state holding the id)
        /// - every send's `message_id` must appear in the data layer
        ///   exactly once (no duplicate writes, no phantom writes)
        /// - the SET of stored ids must equal the SET of returned ids
        ///   (no lost writes)
        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        async fn send_under_concurrent_load_stores_all_messages_with_distinct_ids() {
            const PARALLEL: usize = 50;

            let writes: Arc<StdMutex<Vec<Uuid>>> = Arc::new(StdMutex::new(Vec::new()));
            let writes_tracker = writes.clone();

            let chat = chat_with_stubs(vec![
                Arc::new(StubDataModule::new(move |cmd, params| {
                    if cmd == "data/batch" {
                        let id_str = params["operations"][0]["id"].as_str().expect("data/batch must carry the message id");
                        let id = Uuid::parse_str(id_str).expect("id must be a UUID");
                        writes_tracker.lock().unwrap().push(id);
                    }
                    Ok(batch_ok())
                })),
                Arc::new(StubAircModule::ok(airc_ok_response("evt-conc-001"))),
            ]);
            let chat = Arc::new(chat);

            let mut tasks = Vec::with_capacity(PARALLEL);
            for i in 0..PARALLEL {
                let chat = chat.clone();
                tasks.push(tokio::spawn(async move {
                    chat.send(ChatSendParams {
                        room_id: Uuid::new_v4(),
                        sender_id: Uuid::new_v4(),
                        text: format!("concurrent message {i}"),
                        reply_to_id: None,
                        media: Vec::new(),
                    })
                    .await
                    .expect("send must succeed")
                }));
            }

            let results: Vec<ChatSendResult> = futures::future::join_all(tasks)
                .await
                .into_iter()
                .map(|r| r.expect("task must not panic"))
                .collect();

            // Every send completed.
            assert_eq!(
                results.len(),
                PARALLEL,
                "every concurrent send task must complete"
            );

            // Every send wrote.
            assert_eq!(
                writes.lock().unwrap().len(),
                PARALLEL,
                "every concurrent send must have called data/batch exactly once"
            );

            // Returned ids are all distinct.
            let mut returned_ids: Vec<Uuid> = results.iter().map(|r| r.message_id).collect();
            returned_ids.sort();
            let count_before_dedup = returned_ids.len();
            returned_ids.dedup();
            assert_eq!(
            returned_ids.len(),
            count_before_dedup,
            "concurrent sends must produce distinct message_ids (UUID collision OR shared mutable state)"
        );

            // Stored ids == Returned ids. No lost writes, no phantom writes.
            let mut stored = writes.lock().unwrap().clone();
            stored.sort();
            assert_eq!(
            stored, returned_ids,
            "stored ids must equal returned ids — no message gets persisted that the caller doesn't know about, no returned id is missing from the store"
        );
        }

        /// Per-call ordering invariant under concurrency: even when N
        /// concurrent calls interleave globally, EACH call's own
        /// `data/batch` must precede its own `airc/realtime-publish`. The
        /// dual-write design's bad-divergence safety net depends on this.
        ///
        /// Strategy: tag every observation with the `message_id` (== the
        /// stored entity id == the airc inline message id). Group by id;
        /// assert per-call ordering.
        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        async fn send_preserves_per_call_ordering_under_concurrent_load() {
            const PARALLEL: usize = 25;

            let log: Arc<StdMutex<Vec<(Uuid, &'static str)>>> = Arc::new(StdMutex::new(Vec::new()));
            let data_log = log.clone();
            let airc_log = log.clone();

            let chat = chat_with_stubs(vec![
                Arc::new(StubDataModule::new(move |cmd, params| {
                    if cmd == "data/batch" {
                        let id_str = params["operations"][0]["id"].as_str().unwrap();
                        let id = Uuid::parse_str(id_str).unwrap();
                        data_log.lock().unwrap().push((id, "data/batch"));
                    }
                    Ok(batch_ok())
                })),
                Arc::new(StubAircModule::with(move |params| {
                    let inline_id = params["envelope"]["payload"]["payload"]["inline"]["messageId"]
                        .as_str()
                        .expect("envelope must carry the message id");
                    let id = Uuid::parse_str(inline_id).unwrap();
                    airc_log.lock().unwrap().push((id, "airc/realtime-publish"));
                    Ok(airc_ok_response("evt-order-conc"))
                })),
            ]);
            let chat = Arc::new(chat);

            let mut tasks = Vec::with_capacity(PARALLEL);
            for _ in 0..PARALLEL {
                let chat = chat.clone();
                tasks.push(tokio::spawn(async move {
                    chat.send(sample_send_params()).await
                }));
            }
            futures::future::join_all(tasks).await;

            // Walk the global log, group event indices by message_id.
            let observed = log.lock().unwrap().clone();
            let mut per_call: HashMap<Uuid, Vec<(usize, &'static str)>> = HashMap::new();
            for (idx, (id, event)) in observed.iter().enumerate() {
                per_call.entry(*id).or_default().push((idx, *event));
            }

            assert_eq!(
                per_call.len(),
                PARALLEL,
                "every concurrent call must contribute its own correlation id (no aliasing)"
            );

            for (id, events) in per_call {
                assert_eq!(
                    events.len(),
                    2,
                    "each call must produce exactly 2 events (data + airc) for id={id}"
                );
                // Sort by the GLOBAL log index so we know the call-internal
                // order rather than insertion order into the per-call vec.
                let mut sorted = events.clone();
                sorted.sort_by_key(|(idx, _)| *idx);
                assert_eq!(
                sorted[0].1, "data/batch",
                "per-call ordering: data MUST come before airc for id={id}, observed={sorted:?}"
            );
                assert_eq!(
                    sorted[1].1, "airc/realtime-publish",
                    "per-call ordering: airc MUST come after data for id={id}, observed={sorted:?}"
                );
            }
        }

        /// Mixed outcomes under concurrent load: half the calls have airc
        /// fail, half succeed. Each call's result must reflect ITS OWN
        /// outcome — no cross-contamination between concurrent calls.
        ///
        /// The airc stub branches on a flag embedded in the message text
        /// so it can decide per-call. Critical invariant: the warning
        /// string for a failed call must reference THIS call's
        /// `message_id`, not a sibling concurrent call's id.
        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        async fn send_isolates_mixed_outcomes_under_concurrent_load() {
            const PARALLEL: usize = 30;

            let chat = chat_with_stubs(vec![
                Arc::new(StubDataModule::new(|_cmd, _p| {
                    Ok(batch_ok())
                })),
                Arc::new(StubAircModule::with(|params| {
                    // Drive the airc outcome from the inline message text.
                    let text = params["envelope"]["payload"]["payload"]["inline"]["text"]
                        .as_str()
                        .unwrap();
                    if text.contains("FAIL") {
                        Err(format!("simulated airc failure for: {text}"))
                    } else {
                        Ok(airc_ok_response("evt-mixed-ok"))
                    }
                })),
            ]);
            let chat = Arc::new(chat);

            let mut tasks = Vec::with_capacity(PARALLEL);
            for i in 0..PARALLEL {
                let chat = chat.clone();
                let text = if i % 2 == 0 {
                    format!("OK call {i}")
                } else {
                    format!("FAIL call {i}")
                };
                let label = text.clone();
                tasks.push(tokio::spawn(async move {
                    let result = chat
                        .send(ChatSendParams {
                            room_id: Uuid::new_v4(),
                            sender_id: Uuid::new_v4(),
                            text,
                            reply_to_id: None,
                            media: Vec::new(),
                        })
                        .await
                        .expect("send must succeed (degraded success counts)");
                    (label, result)
                }));
            }
            let results: Vec<(String, ChatSendResult)> = futures::future::join_all(tasks)
                .await
                .into_iter()
                .map(|r| r.expect("task must not panic"))
                .collect();

            let (mut ok_count, mut fail_count) = (0usize, 0usize);
            for (label, result) in &results {
                if label.contains("FAIL") {
                    fail_count += 1;
                    assert!(
                        result.event_id.is_none(),
                        "{label}: airc failed → event_id must be None"
                    );
                    let warning = result
                        .warning
                        .as_ref()
                        .expect(&format!("{label}: airc failed → warning must be set"));
                    // Cross-contamination check: the warning's message_id
                    // must match THIS call's result.message_id (not a
                    // sibling call's id that ran concurrently).
                    assert!(
                    warning.contains(&result.message_id.to_string()),
                    "{label}: warning must name THIS call's message_id ({}), not a sibling's. warning={}",
                    result.message_id, warning
                );
                    // The underlying airc error must surface unchanged.
                    assert!(
                        warning.contains(label.as_str()),
                        "{label}: warning must surface the airc-side error text, got: {warning}"
                    );
                } else {
                    ok_count += 1;
                    assert!(
                        result.event_id.is_some(),
                        "{label}: airc ok → event_id must be Some"
                    );
                    assert!(
                        result.warning.is_none(),
                        "{label}: airc ok → warning must be None"
                    );
                }
            }
            assert_eq!(ok_count, PARALLEL / 2, "half the calls should succeed");
            assert_eq!(
                fail_count,
                PARALLEL / 2,
                "half the calls should report degraded success"
            );
        }

        /// `chat/poll` under N concurrent persona threads, each polling a
        /// DIFFERENT room: every task must get back its OWN room's
        /// messages, never a sibling task's. The stub echoes the
        /// requested `roomId` so we can prove the result didn't get
        /// swapped between concurrent calls.
        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        async fn poll_isolates_results_under_concurrent_load() {
            const PARALLEL: usize = 30;

            let chat = chat_with_stubs(vec![Arc::new(StubDataModule::query_only(|params| {
                // Echo the requested roomId back in the synthetic result so
                // the caller can prove its own input flowed through.
                let echoed = params["filter"]["roomId"]["$eq"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                json!({
                    "success": true,
                    "data": [
                        {
                            "id": "echo",
                            "data": {
                                "id": "echo",
                                "roomId": echoed,
                                "timestamp": "2026-05-30T00:00:00Z",
                                "content": { "text": "echoed" },
                            }
                        }
                    ],
                })
            }))]);
            let chat = Arc::new(chat);

            let mut tasks = Vec::with_capacity(PARALLEL);
            for _ in 0..PARALLEL {
                let chat = chat.clone();
                let my_room = Uuid::new_v4();
                tasks.push(tokio::spawn(async move {
                    let result = chat
                        .poll(ChatPollParams {
                            room_id: Some(my_room),
                            ..Default::default()
                        })
                        .await
                        .expect("poll must succeed");
                    (my_room, result)
                }));
            }
            let results = futures::future::join_all(tasks).await;

            for r in results {
                let (my_room, poll_result) = r.expect("task must not panic");
                assert_eq!(poll_result.count, 1, "each task gets one echoed message");
                let echoed = poll_result.messages[0]["roomId"].as_str().unwrap();
                assert_eq!(
                echoed,
                my_room.to_string(),
                "each task MUST get back its OWN room's result; no cross-talk between concurrent polls"
            );
            }
        }
    }
