#![cfg(test)]
    use super::*;

    fn make_voice() -> VoiceQueueItem {
        VoiceQueueItem {
            id: Uuid::new_v4(),
            room_id: Uuid::new_v4(),
            content: "Hello from voice".into(),
            sender_id: Uuid::new_v4(),
            sender_name: "test-user".into(),
            sender_type: SenderType::Human,
            voice_session_id: Uuid::new_v4(),
            timestamp: EpochMs::now().millis(),
            enqueued_at: EpochMs::now().millis(),
            priority: 1.0,
            media: Vec::new(),
        }
    }

    fn make_chat(room_id: Uuid, mentions: bool, priority: f32) -> ChatQueueItem {
        ChatQueueItem {
            id: Uuid::new_v4(),
            room_id,
            content: "Chat message".into(),
            sender_id: Uuid::new_v4(),
            sender_name: "User".into(),
            sender_type: SenderType::Human,
            mentions,
            timestamp: EpochMs::now().millis(),
            enqueued_at: EpochMs::now().millis(),
            priority,
            consolidated_context: Vec::new(),
            media: Vec::new(),
            embedding_cell: std::sync::OnceLock::new(),
            #[cfg(any(test, feature = "test-fixtures"))]
            compute_calls: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    fn make_task(domain: &str, context_id: Uuid) -> TaskQueueItem {
        TaskQueueItem {
            id: Uuid::new_v4(),
            task_id: Uuid::new_v4(),
            assignee_id: Uuid::new_v4(),
            created_by: Uuid::new_v4(),
            task_domain: domain.into(),
            task_type: "review".into(),
            context_id,
            description: "Test task".into(),
            priority: 0.5,
            status: "pending".into(),
            timestamp: EpochMs::now().millis(),
            enqueued_at: EpochMs::now().millis(),
            due_date: None,
            estimated_duration: None,
            depends_on: Vec::new(),
            blocked_by: Vec::new(),
            related_task_ids: Vec::new(),
            consolidated_count: 1,
        }
    }

    #[test]
    fn test_voice_always_urgent() {
        let voice = make_voice();
        assert!(voice.is_urgent());
        assert!(!voice.can_be_kicked());
        assert_eq!(voice.base_priority(), 1.0);
        assert_eq!(voice.max_aging_boost(), 0.0);
        assert_eq!(voice.item_type(), "voice");
        assert_eq!(voice.domain(), ActivityDomain::Audio);
    }

    #[test]
    fn test_chat_mention_urgency() {
        let room = Uuid::new_v4();
        let with_mention = make_chat(room, true, 0.8);
        let without_mention = make_chat(room, false, 0.5);

        assert!(with_mention.is_urgent());
        assert!(!without_mention.is_urgent());
    }

    #[test]
    fn test_chat_same_room_consolidation() {
        let room = Uuid::new_v4();
        let other_room = Uuid::new_v4();
        let chat1 = make_chat(room, false, 0.5);
        let chat2 = make_chat(room, false, 0.7);
        let chat3 = make_chat(other_room, false, 0.6);

        // Same room: should consolidate
        assert!(chat1.should_consolidate_with(&chat2));
        // Different room: should NOT consolidate
        assert!(!chat1.should_consolidate_with(&chat3));
    }

    #[test]
    fn test_chat_consolidation_merges() {
        let room = Uuid::new_v4();
        let mut chat1 = make_chat(room, false, 0.5);
        chat1.content = "First message".into();
        chat1.timestamp = 1000;

        let mut chat2 = make_chat(room, true, 0.8);
        chat2.content = "Second message with @mention".into();
        chat2.timestamp = 2000;

        let consolidated = chat1.consolidate_with_items(&[&chat2]);

        // Trigger is the latest message (chat2, timestamp 2000)
        assert_eq!(consolidated.timestamp, 2000);
        assert_eq!(consolidated.content, "Second message with @mention");
        // Highest priority
        assert_eq!(consolidated.priority, 0.8);
        // Mentions carried forward
        assert!(consolidated.mentions);
        // Prior message is in context
        assert_eq!(consolidated.consolidated_context.len(), 1);
        assert_eq!(
            consolidated.consolidated_context[0].content,
            "First message"
        );
    }

    #[test]
    fn test_task_overdue_urgency() {
        let ctx = Uuid::new_v4();
        let mut task = make_task("code", ctx);
        assert!(!task.is_urgent()); // No due date

        task.due_date = Some(EpochMs::now().millis() + 60_000); // Due in 1 min
        assert!(!task.is_urgent()); // Not yet overdue

        task.due_date = Some(EpochMs::now().millis() - 1000); // 1 second overdue
        assert!(task.is_urgent());
    }

    #[test]
    fn test_task_in_progress_not_kickable() {
        let ctx = Uuid::new_v4();
        let mut task = make_task("code", ctx);
        assert!(task.can_be_kicked()); // pending

        task.status = "in_progress".into();
        assert!(!task.can_be_kicked()); // in progress
    }

    #[test]
    fn test_task_same_domain_context_consolidation() {
        let ctx = Uuid::new_v4();
        let task1 = make_task("code", ctx);
        let task2 = make_task("code", ctx);
        let task3 = make_task("memory", ctx);
        let task4 = make_task("code", Uuid::new_v4());

        // Same domain + context: consolidate
        assert!(task1.should_consolidate_with(&task2));
        // Different domain: no
        assert!(!task1.should_consolidate_with(&task3));
        // Different context: no
        assert!(!task1.should_consolidate_with(&task4));
    }

    #[test]
    fn test_effective_priority_aging() {
        let room = Uuid::new_v4();
        let chat = make_chat(room, false, 0.3);

        let now = EpochMs::now().millis();
        let enqueued = now; // Just enqueued — no aging
        let p0 = chat.effective_priority(now, enqueued);
        assert!((p0 - 0.3).abs() < 0.01, "No aging expected, got {p0}");

        // After 15s (half of 30s aging window) → 0.25 boost
        let p15 = chat.effective_priority(now + 15_000, enqueued);
        assert!((p15 - 0.55).abs() < 0.05, "Expected ~0.55, got {p15}");

        // After 30s (full aging) → 0.5 boost → capped at 0.8
        let p30 = chat.effective_priority(now + 30_000, enqueued);
        assert!((p30 - 0.8).abs() < 0.05, "Expected ~0.8, got {p30}");

        // After 60s → still capped at 0.8 (max boost is 0.5)
        let p60 = chat.effective_priority(now + 60_000, enqueued);
        assert!(
            (p60 - 0.8).abs() < 0.05,
            "Expected ~0.8 (capped), got {p60}"
        );
    }

    #[test]
    fn test_voice_no_aging() {
        let voice = make_voice();
        let now = EpochMs::now().millis();
        let p0 = voice.effective_priority(now, now);
        let p60 = voice.effective_priority(now + 60_000, now);
        assert_eq!(p0, 1.0);
        assert_eq!(p60, 1.0); // No aging boost
    }

    #[test]
    fn test_voice_does_not_consolidate_with_chat() {
        let voice = make_voice();
        let chat = make_chat(Uuid::new_v4(), false, 0.5);
        assert!(!voice.should_consolidate_with(&chat));
    }

    #[test]
    fn test_ipc_request_roundtrip() {
        let req = ChannelEnqueueRequest::Chat {
            id: Uuid::new_v4().to_string(),
            room_id: Uuid::new_v4().to_string(),
            content: "Hello".into(),
            sender_id: Uuid::new_v4().to_string(),
            sender_name: "test-user".into(),
            sender_type: "human".into(),
            mentions: true,
            timestamp: EpochMs::now().millis(),
            priority: 0.8,
            media: Vec::new(),
        };

        let item = req.to_queue_item().unwrap();
        assert_eq!(item.item_type(), "chat");
        assert!(item.is_urgent()); // mentions = true
        assert_eq!(item.domain(), ActivityDomain::Chat);
    }

    #[test]
    fn test_chat_media_roundtrip_through_request_and_json() {
        // Going-in: request with media → ChatQueueItem with media → to_json carries it.
        // This is the regression guard for the bug Joel hit on 2026-04-21:
        // vision bytes were enqueuing fine but the inbox round-trip stripped media,
        // so PRG always saw 0 attachments and Vision AI hallucinated descriptions.
        let blob_hash = "sha256:deadbeef".to_string();
        let req = ChannelEnqueueRequest::Chat {
            id: Uuid::new_v4().to_string(),
            room_id: Uuid::new_v4().to_string(),
            content: "look at this".into(),
            sender_id: Uuid::new_v4().to_string(),
            sender_name: "operator".into(),
            sender_type: "human".into(),
            mentions: false,
            timestamp: EpochMs::now().millis(),
            priority: 0.5,
            media: vec![MediaItemRequest {
                kind: "image".into(),
                mime_type: Some("image/jpeg".into()),
                blob_hash: Some(blob_hash.clone()),
                url: None,
                description: None,
            }],
        };

        let item = req.to_queue_item().unwrap();
        let json = item.to_json();
        let media = json.get("media").expect("media key present in JSON");
        let media_arr = media.as_array().expect("media is an array");
        assert_eq!(media_arr.len(), 1, "exactly one media item survives");
        let first = &media_arr[0];
        assert_eq!(first.get("type").and_then(|v| v.as_str()), Some("image"));
        assert_eq!(
            first.get("blobHash").and_then(|v| v.as_str()),
            Some(blob_hash.as_str())
        );
        assert_eq!(
            first.get("mimeType").and_then(|v| v.as_str()),
            Some("image/jpeg")
        );
    }

    /// proves: lazy embedding cell shares compute across consumers
    ///
    /// First `embedding()` call triggers compute; every subsequent call on
    /// the SAME `Arc<ChatQueueItem>` returns the cached `Arc<Vec<f32>>`.
    /// `Arc::ptr_eq` witnesses the share: if two calls returned different
    /// Arcs, the cell would be re-computing each time.
    ///
    /// Per `[[pass-by-reference-lazy-metadata-with-data]]`: this is the
    /// item-level witness for the doctrine — the data IS the cache.
    #[test]
    fn embedding_cell_returns_same_arc_across_calls() {
        let item = std::sync::Arc::new(ChatQueueItem {
            id: Uuid::new_v4(),
            room_id: Uuid::new_v4(),
            content: "hello world from the channel".into(),
            sender_id: Uuid::new_v4(),
            sender_name: "test-sender".into(),
            sender_type: SenderType::Human,
            mentions: false,
            timestamp: EpochMs::now().millis(),
            enqueued_at: EpochMs::now().millis(),
            priority: 0.5,
            consolidated_context: Vec::new(),
            media: Vec::new(),
            embedding_cell: std::sync::OnceLock::new(),
            #[cfg(any(test, feature = "test-fixtures"))]
            compute_calls: std::sync::atomic::AtomicUsize::new(0),
        });

        let e1 = item.embedding();
        let e2 = item.embedding();
        let e3 = item.embedding();

        // All three calls on the same item return the SAME underlying Arc.
        // If the cell were re-computing, ptr_eq would fail because each
        // compute would allocate a fresh Arc.
        assert!(
            std::sync::Arc::ptr_eq(&e1, &e2),
            "second embedding() call must return the cached Arc, not recompute"
        );
        assert!(
            std::sync::Arc::ptr_eq(&e2, &e3),
            "third embedding() call must return the cached Arc, not recompute"
        );

        // Same content → same embedding (verifies the placeholder decoder
        // is deterministic; real decoder will obviously also be
        // deterministic against the same input).
        assert_eq!(e1.len(), 8, "placeholder decoder produces 8-dim vectors");
    }

    /// proves: lazy embedding cell shares across multiple Arc consumers
    /// (multi-persona-in-room property)
    ///
    /// The item is shared via `Arc<ChatQueueItem>`. Cloning the Arc gives
    /// multiple handles to the SAME item. Each handle calling `embedding()`
    /// must hit the same OnceLock cell and return the same underlying
    /// `Arc<Vec<f32>>`. This is what makes "N personas in a room with M
    /// arrivals cost M × decode_cost, not N × M × decode_cost" true.
    #[test]
    fn embedding_cell_shared_across_arc_clones() {
        let item = std::sync::Arc::new(ChatQueueItem {
            id: Uuid::new_v4(),
            room_id: Uuid::new_v4(),
            content: "shared content across personas".into(),
            sender_id: Uuid::new_v4(),
            sender_name: "test-sender".into(),
            sender_type: SenderType::Human,
            mentions: false,
            timestamp: EpochMs::now().millis(),
            enqueued_at: EpochMs::now().millis(),
            priority: 0.5,
            consolidated_context: Vec::new(),
            media: Vec::new(),
            embedding_cell: std::sync::OnceLock::new(),
            #[cfg(any(test, feature = "test-fixtures"))]
            compute_calls: std::sync::atomic::AtomicUsize::new(0),
        });

        // Simulate 4 personas each holding their own Arc to the same item.
        let persona1 = std::sync::Arc::clone(&item);
        let persona2 = std::sync::Arc::clone(&item);
        let persona3 = std::sync::Arc::clone(&item);
        let persona4 = std::sync::Arc::clone(&item);

        let e1 = persona1.embedding();
        let e2 = persona2.embedding();
        let e3 = persona3.embedding();
        let e4 = persona4.embedding();

        // All four personas see the SAME cached embedding Arc — proves the
        // shared-decode property: the embedding was computed ONCE by the
        // first caller (whichever persona happened to demand it first),
        // and the other three got the cached share.
        assert!(std::sync::Arc::ptr_eq(&e1, &e2));
        assert!(std::sync::Arc::ptr_eq(&e2, &e3));
        assert!(std::sync::Arc::ptr_eq(&e3, &e4));
    }
