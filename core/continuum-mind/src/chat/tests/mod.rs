#![cfg(test)]

    use super::*;
    use crate::runtime::ModuleRegistry;
    use uuid::Uuid;
    // ════════════════════════════════════════════════════════════════
    // chat/send: dual-write composition stress tests
    // ════════════════════════════════════════════════════════════════
    //
    // The chat module's first multi-cross-module-call handler:
    // chat → data (persist) then chat → airc (publish). Each test
    // pins one cell of the (data ok/fail × airc ok/fail) matrix,
    // plus the wire-contract invariants the dual-write design
    // promised.
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    /// Construct a `ChatModule` driving a freshly-built executor over a
    /// registry containing the given stub modules. The chat module's
    /// `with_executor` constructor takes the executor by `Arc`, so the
    /// resulting module routes all `executor()` calls through the
    /// in-test registry — no global `OnceLock` involvement.
    pub(super) fn chat_with_stubs(stubs: Vec<Arc<dyn ServiceModule>>) -> ChatModule {
        let registry = Arc::new(ModuleRegistry::new());
        for module in stubs {
            registry.register(module);
        }
        let executor = Arc::new(CommandExecutor::new(registry));
        ChatModule::with_executor(executor)
    }

    /// Stub data module: handles any `data/*` command by returning a
    /// canned response built by the test's closure. The closure
    /// receives BOTH the command name and the params so tests can
    /// branch on command (`data/query` vs `data/batch` etc.) or
    /// inspect the inbound shape.
    ///
    /// `chat/poll` tests use the params-only `Self::query_only`
    /// constructor (back-compat); `chat/send` tests use the full
    /// `Self::new` constructor with command-aware dispatch.
    pub(super) struct StubDataModule {
        pub(super) responder: Box<dyn Fn(&str, Value) -> Result<Value, String> + Send + Sync>,
    }

    impl StubDataModule {
        pub(super) fn new<F>(responder: F) -> Self
        where
            F: Fn(&str, Value) -> Result<Value, String> + Send + Sync + 'static,
        {
            Self {
                responder: Box::new(responder),
            }
        }

        /// Construct a stub that only handles `data/query` and runs
        /// the given params-only closure on inbound params. Asserts
        /// the command name to catch unintended calls. Convenience
        /// for chat/poll tests that pre-date dual-command testing.
        pub(super) fn query_only<F>(responder: F) -> Self
        where
            F: Fn(Value) -> Value + Send + Sync + 'static,
        {
            Self::new(move |command, params| {
                assert_eq!(
                    command, "data/query",
                    "query_only stub received unexpected command: {command}"
                );
                Ok(responder(params))
            })
        }
    }

    #[async_trait]
    impl ServiceModule for StubDataModule {
        fn config(&self) -> ModuleConfig {
            ModuleConfig {
                name: "data",
                priority: ModulePriority::Normal,
                command_prefixes: &["data/"],
                event_subscriptions: &[],
                needs_dedicated_thread: false,
                max_concurrency: 0,
                tick_interval: None,
            }
        }

        async fn initialize(&self, _ctx: &crate::runtime::ModuleContext) -> Result<(), String> {
            Ok(())
        }

        async fn handle_command(
            &self,
            command: &str,
            params: Value,
        ) -> Result<CommandResult, String> {
            (self.responder)(command, params).map(CommandResult::Json)
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    // ── config + dispatch ────────────────────────────────────────────
    /// The `data/batch` reply the store gives a committed line: the message row, then its
    /// arrival at sequence 1. ONE construction site for every chat test's data stub.
    pub(super) fn batch_ok() -> Value {
        json!({ "success": true, "data": [{ "success": true }, { "success": true, "seq": 1, "fresh": true }] })
    }

    fn imported_line(text: &str) -> crate::chat::line_store::PostedLine {
        crate::chat::line_store::PostedLine {
            message_id: Uuid::new_v4(),
            room: airc_core::RoomId::new(),
            author: airc_core::PeerId::new(),
            kind: crate::chat::arrival::ArrivalKind::Message,
            direct_to: Some(airc_core::PeerId::new()),
            occurred_at_ms: 1_784_227_923_000,
            text: text.to_string(),
        }
    }

    // what this catches: #140/#177 and Slice P — a room line imported from the inbound seam
    // must land as ONE atomic batch: the ChatMessageEntity row (ensured, so a replay is
    // idempotent) and its arrival carrying the typed routing header. If either half goes
    // missing, persona speech stops surviving the process or stops being perceivable in order.
    #[tokio::test]
    pub(super) async fn an_imported_line_commits_its_row_and_arrival_in_one_batch() {
        let seen: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
        let seen_in = seen.clone();
        let data = StubDataModule::new(move |command, params| {
            assert_eq!(command, "data/batch", "a line is one atomic batch");
            seen_in.lock().unwrap().push(params);
            Ok(batch_ok())
        });
        let chat = chat_with_stubs(vec![Arc::new(data)]);
        let line = imported_line("I see the word LIGHTHOUSE in the room.");
        let imported = chat
            .commit_line(
                &line,
                crate::chat::line_store::LineWrite::Import,
                None,
                json!({ "text": line.text }),
            )
            .await
            .expect("the import commits");
        assert_eq!(
            imported,
            crate::chat::line_store::Imported::Fresh { arrival_seq: 1 }
        );
        let calls = seen.lock().unwrap();
        assert_eq!(calls.len(), 1, "exactly one durable write per line");
        let [row, arrival] = calls[0]["operations"]
            .as_array()
            .expect("operations")
            .as_slice()
        else {
            panic!("a line is exactly its row and its arrival: {}", calls[0]);
        };
        assert_eq!(
            row["operationType"], "ensure",
            "an import never fails on a line already stored"
        );
        assert_eq!(row["collection"], CHAT_MESSAGES_COLLECTION);
        assert_eq!(row["data"]["senderId"], line.author.as_uuid().to_string());
        assert_eq!(
            row["data"]["content"]["text"],
            "I see the word LIGHTHOUSE in the room."
        );
        assert!(row["data"]["timestamp"]
            .as_str()
            .unwrap_or("")
            .starts_with("2026-"));
        assert_eq!(arrival["collection"], "chat_arrivals");
        assert_eq!(arrival["data"]["messageId"], line.message_id.to_string());
        assert_eq!(arrival["data"]["kind"], "message");
        assert_eq!(
            arrival["data"]["directTo"],
            json!(line.direct_to.map(|p| p.as_uuid())),
            "the addressee rides the header"
        );
    }

    // what this catches: the two-writer convergence. A human line stored by send() arrives
    // again through its own projection under the SAME id: the import commits and reports the
    // existing arrival, never an error and never a second arrival.
    #[tokio::test]
    pub(super) async fn a_duplicate_import_reports_its_existing_arrival() {
        let data = StubDataModule::new(|_command, _params| {
            Ok(
                json!({ "success": true, "data": [{ "success": true, "created": false }, { "success": true, "seq": 7, "fresh": false }] }),
            )
        });
        let chat = chat_with_stubs(vec![Arc::new(data)]);
        let line = imported_line("already stored by send()");
        let imported = chat
            .commit_line(
                &line,
                crate::chat::line_store::LineWrite::Import,
                None,
                json!({ "text": line.text }),
            )
            .await
            .expect("a duplicate is a committed import");
        assert_eq!(
            imported,
            crate::chat::line_store::Imported::Duplicate { arrival_seq: 7 }
        );
    }

    // what this catches: a store refusal swallowed as a calm skip. The import is awaited by the
    // inbound seam so the attach watermark never passes a line the store lacks; a refusal must
    // therefore be an error the seam sees, never a quiet probe.
    #[tokio::test]
    pub(super) async fn a_refused_import_is_an_error() {
        let data = StubDataModule::new(|_command, _params| {
            Ok(json!({ "success": false, "error": "disk full" }))
        });
        let chat = chat_with_stubs(vec![Arc::new(data)]);
        let line = imported_line("lost?");
        let err = chat
            .commit_line(
                &line,
                crate::chat::line_store::LineWrite::Import,
                None,
                json!({ "text": line.text }),
            )
            .await
            .expect_err("a refusal is an error");
        assert!(err.contains("disk full"), "the cause is named: {err}");
    }

    #[test]
    pub(super) fn config_advertises_both_command_prefixes() {
        let chat = ChatModule::new();
        let config = chat.config();
        assert_eq!(config.name, "chat");
        // Both surfaces route to this module so consumers can migrate
        // off the legacy `collaboration/` prefix at their own pace.
        assert!(
            config.command_prefixes.contains(&"chat/")
                && config.command_prefixes.contains(&"collaboration/chat/"),
            "chat module must own BOTH prefixes during the migration window"
        );
    }

    #[tokio::test]
    pub(super) async fn unknown_command_returns_loud_error_naming_supported_commands() {
        let chat = chat_with_stubs(vec![]);
        let err = chat
            .handle_command("chat/whatever", json!({}))
            .await
            .expect_err("unknown chat command must Err, not silently succeed");
        assert!(
            err.contains("not handled by chat module"),
            "error must name the module so the caller knows which layer failed: {err}"
        );
        assert!(
            err.contains("chat/poll"),
            "error must name the known commands so the caller can self-correct: {err}"
        );
    }

    // ── Unmigrated stubs still name the follow-up PR ─────────────────
    //
    // chat/send migrated in this PR; analyze + export still on TS.
    #[tokio::test]
    pub(super) async fn unmigrated_commands_fail_loud_and_name_followup() {
        let chat = chat_with_stubs(vec![]);
        for cmd in [
            "chat/analyze",
            "collaboration/chat/analyze",
            "chat/export",
            "collaboration/chat/export",
        ] {
            let err = chat
                .handle_command(cmd, json!({}))
                .await
                .expect_err(&format!("{cmd}: unmigrated stub must Err"));
            assert!(
                err.contains("not yet migrated"),
                "stub error must announce the migration state: {err}"
            );
            assert!(
                err.contains("issue #57"),
                "stub error must point to the issue so the consumer can follow the migration: {err}"
            );
        }
    }

    // ── chat/poll: empty-result path ──────────────────────────────────
    #[tokio::test]
    pub(super) async fn poll_returns_empty_result_when_data_module_returns_no_messages() {
        let chat = chat_with_stubs(vec![Arc::new(StubDataModule::query_only(
            |_p| json!({ "success": true, "data": [] }),
        ))]);

        let result = chat
            .poll(ChatPollParams::default())
            .await
            .expect("poll over empty data must succeed");
        assert_eq!(result.count, 0);
        assert!(result.messages.is_empty());
        assert!(result.after_message_id.is_none());
    }

    // what this catches: #201 boot-race — a chat command that arrives BEFORE
    // install_executor_on_all runs fails LOUD to its caller (naming the contract),
    // instead of .expect()-panicking the whole core and taking every other module
    // down with it. Regression for the .expect() → Result conversion in executor().
    #[tokio::test]
    pub(super) async fn command_before_executor_installed_fails_loud_not_panics() {
        let chat = ChatModule::new(); // executor slot deliberately NOT installed
        let err = chat
            .poll(ChatPollParams::default())
            .await
            .expect_err("poll before install_executor_on_all must fail, not panic");
        assert!(err.contains("not installed"), "loud contract error, got: {err}");
        assert!(
            err.contains("install_executor_on_all"),
            "error names the contract so the operator sees the real problem: {err}"
        );
    }

    // ── chat/poll: latest-N path (no anchor) ──────────────────────────
    #[tokio::test]
    pub(super) async fn poll_without_anchor_queries_data_desc_and_returns_chronological() {
        let chat = chat_with_stubs(vec![Arc::new(StubDataModule::query_only(|params| {
            // Validate the chat module built the expected query shape.
            assert_eq!(params["collection"], "chat_messages");
            assert_eq!(params["sort"][0]["direction"], "desc");
            // Caller didn't specify a limit → chat uses DEFAULT_POLL_LIMIT.
            assert_eq!(params["limit"], 50);
            // No filter fields set → empty filter map.
            assert_eq!(params["filter"], json!({}));

            json!({
                "success": true,
                "data": [
                    { "id": "id-2", "data": { "id": "id-2", "timestamp": "2026-05-30T15:00:00Z", "content": { "text": "second" } } },
                    { "id": "id-1", "data": { "id": "id-1", "timestamp": "2026-05-30T14:00:00Z", "content": { "text": "first" } } }
                ]
            })
        }))]);

        let result = chat
            .poll(ChatPollParams::default())
            .await
            .expect("latest-N poll must succeed");
        assert_eq!(result.count, 2);
        // Chronological normalization: even though data returned DESC,
        // chat sorts the result ASC for display.
        assert_eq!(
            result.messages[0]["timestamp"], "2026-05-30T14:00:00Z",
            "earliest message comes first after normalization"
        );
        assert_eq!(result.messages[1]["timestamp"], "2026-05-30T15:00:00Z");
    }

    // ── chat/poll: room filter applied ────────────────────────────────
    #[tokio::test]
    pub(super) async fn poll_with_room_id_passes_filter_to_data_module() {
        let room_id = Uuid::new_v4();
        let room_str = room_id.to_string();
        let chat = chat_with_stubs(vec![Arc::new(StubDataModule::query_only(move |params| {
            assert_eq!(params["filter"]["roomId"]["$eq"], room_str);
            json!({ "success": true, "data": [] })
        }))]);

        chat.poll(ChatPollParams {
            room_id: Some(room_id),
            ..Default::default()
        })
        .await
        .expect("room-filtered poll must succeed");
    }

    // ── chat/poll: cursor pages (both directions) live in `paging.rs`, run over
    // an in-memory store that evaluates the filters and sorts the queries send.

    // what this catches: the two cursors are opposite scroll directions —
    // accepting both would silently produce an incoherent page. Must
    // fail loud, and must fail BEFORE any storage round-trip.
    #[tokio::test]
    pub(super) async fn poll_rejects_both_cursor_directions_at_once() {
        let chat = chat_with_stubs(vec![Arc::new(StubDataModule::query_only(|_| {
            panic!("mutually-exclusive cursors must be rejected before any data/query");
        }))]);
        let err = chat
            .poll(ChatPollParams {
                after_message_id: Some(Uuid::new_v4()),
                before_message_id: Some(Uuid::new_v4()),
                ..Default::default()
            })
            .await
            .expect_err("both cursors at once must be rejected");
        assert!(err.contains("mutually exclusive"), "got: {err}");
    }

    // ── chat/poll: missing anchor fails loud ──────────────────────────
    #[tokio::test]
    pub(super) async fn poll_with_anchor_returns_err_when_anchor_missing() {
        let anchor_id = Uuid::new_v4();
        let chat = chat_with_stubs(vec![Arc::new(StubDataModule::query_only(|_p| {
            // Empty data → anchor lookup yields no rows.
            json!({ "success": true, "data": [] })
        }))]);

        let err = chat
            .poll(ChatPollParams {
                after_message_id: Some(anchor_id),
                ..Default::default()
            })
            .await
            .expect_err("missing anchor must surface as an Err");
        assert!(
            err.contains("anchor message not found"),
            "error must name the failure mode: {err}"
        );
        assert!(
            err.contains(&anchor_id.to_string()),
            "error must name the offending id: {err}"
        );
    }

    // ── chat/poll + chat/send: the typed object registry path ─────────
    //
    // These commands now route via `route_object` as typed
    // `ActionCommand`s (`crate::chat::commands`), NOT the legacy
    // `handle_command` envelope. The tests below prove the module
    // contributes exactly those two objects, that invoking one reaches
    // the shared executor slot and runs the canonical `ChatModule::poll`
    // body, and that the retired legacy arms fail loud.
    #[test]
    pub(super) fn commands_contributes_poll_and_send_objects() {
        // what this catches: a regression that drops a command from the
        // module's `commands()` (so it never reaches `command_registry()`,
        // the persona tool surface, or the ACL) — or renames its wire key
        // away from the canonical `chat/poll` / `chat/send`.
        let names: Vec<&str> = ChatModule::new()
            .commands()
            .iter()
            .map(|c| c.name())
            .collect();
        assert!(names.contains(&"chat/poll"), "missing chat/poll: {names:?}");
        assert!(names.contains(&"chat/send"), "missing chat/send: {names:?}");
        assert_eq!(
            names.len(),
            2,
            "chat contributes exactly poll + send: {names:?}"
        );
    }

    #[tokio::test]
    pub(super) async fn typed_chat_poll_object_invokes_over_shared_slot() {
        // what this catches: the typed `chat/poll` object must resolve the
        // module's shared late-bound executor (via `from_slot`) and delegate
        // to the canonical `ChatModule::poll` body — a regression that broke
        // the shared slot (empty executor) or the delegation would fail here.
        let chat = chat_with_stubs(vec![Arc::new(StubDataModule::query_only(
            |_p| json!({ "success": true, "data": [] }),
        ))]);
        let poll = chat
            .commands()
            .into_iter()
            .find(|c| c.name() == "chat/poll")
            .expect("chat/poll object must be contributed");

        let result = poll
            .invoke(json!({ "limit": 7 }), None)
            .await
            .expect("typed chat/poll must succeed");
        let CommandResult::Json(value) = result else {
            panic!("chat/poll must return CommandResult::Json");
        };
        assert_eq!(value["count"], 0);
        assert!(value["messages"].is_array());
    }

    #[tokio::test]
    pub(super) async fn legacy_chat_arms_fail_loud() {
        // what this catches: the legacy `handle_command` path for the
        // migrated verbs must fail loud naming the command (never silently
        // re-handle), so a descriptor that failed to register surfaces
        // instead of a second implementation forking off the typed object.
        let chat = ChatModule::new();
        for command in [
            "chat/poll",
            "collaboration/chat/poll",
            "chat/send",
            "collaboration/chat/send",
        ] {
            let err = chat
                .handle_command(command, json!({}))
                .await
                .expect_err("migrated chat arm must fail loud");
            assert!(err.contains("migrated"), "got {err}");
            assert!(err.contains(command), "got {err}");
        }
    }

    /// Stub airc module: handles `airc/realtime-publish` by returning
    /// either a canned success Value or a fail-loud Err. Lets each
    /// chat/send test pick the airc outcome independently of data's.
    pub(super) struct StubAircModule {
        pub(super) publish_responder: Box<dyn Fn(Value) -> Result<Value, String> + Send + Sync>,
    }

    impl StubAircModule {
        pub(super) fn ok(canned: Value) -> Self {
            Self {
                publish_responder: Box::new(move |_p| Ok(canned.clone())),
            }
        }

        pub(super) fn err(message: impl Into<String>) -> Self {
            let msg = message.into();
            Self {
                publish_responder: Box::new(move |_p| Err(msg.clone())),
            }
        }

        pub(super) fn with<F>(responder: F) -> Self
        where
            F: Fn(Value) -> Result<Value, String> + Send + Sync + 'static,
        {
            Self {
                publish_responder: Box::new(responder),
            }
        }
    }

    #[async_trait]
    impl ServiceModule for StubAircModule {
        fn config(&self) -> ModuleConfig {
            ModuleConfig {
                name: "airc",
                priority: ModulePriority::Normal,
                command_prefixes: &["airc/"],
                event_subscriptions: &[],
                needs_dedicated_thread: false,
                max_concurrency: 0,
                tick_interval: None,
            }
        }

        async fn initialize(&self, _ctx: &crate::runtime::ModuleContext) -> Result<(), String> {
            Ok(())
        }

        async fn handle_command(
            &self,
            command: &str,
            params: Value,
        ) -> Result<CommandResult, String> {
            assert_eq!(
                command, "airc/realtime-publish",
                "chat/send must only reach airc via realtime-publish, got {command}"
            );
            (self.publish_responder)(params).map(CommandResult::Json)
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    /// Build a chat/send params instance with sensible defaults. Tests
    /// override only the fields they care about.
    pub(super) fn sample_send_params() -> ChatSendParams {
        ChatSendParams {
            room_id: Uuid::new_v4(),
            sender_id: Uuid::new_v4(),
            text: "hello world".into(),
            reply_to_id: None,
            media: Vec::new(),
        }
    }

    /// Standard "airc broadcast succeeded" canned response. Mirrors
    /// the actual `AircRealtimePublishResult` wire shape (camelCase,
    /// `eventId` field).
    pub(super) fn airc_ok_response(event_id: &str) -> Value {
        json!({
            "ok": true,
            "eventId": event_id,
            "roomId": Uuid::new_v4().to_string(),
            "delivery": "durable",
            "storedForReplay": true,
            "replayDepth": 0,
            "activePresenceCount": 0,
            "activeSubscriptionCount": 0,
            "activePeerManifestCount": 0,
        })
    }

    // ── Happy path: both succeed ─────────────────────────────────────
    #[tokio::test]
    pub(super) async fn send_happy_path_returns_message_id_and_event_id() {
        let chat = chat_with_stubs(vec![
            Arc::new(StubDataModule::new(|cmd, _p| {
                assert_eq!(
                    cmd, "data/batch",
                    "happy path only writes (no other data ops)"
                );
                Ok(batch_ok())
            })),
            Arc::new(StubAircModule::ok(airc_ok_response("evt-happy-001"))),
        ]);

        let result = chat
            .send(sample_send_params())
            .await
            .expect("happy path must succeed");

        // Both surfaces' ids are present: message stored locally AND
        // airc event id returned for broadcast correlation.
        assert!(
            !result.message_id.is_nil(),
            "message_id must be a real UUID"
        );
        assert_eq!(
            result.event_id.as_deref(),
            Some("evt-happy-001"),
            "happy path must surface the airc-side event id"
        );
        assert!(
            result.warning.is_none(),
            "no warning on happy path: {result:?}"
        );
    }

    // what this catches: media surviving local storage but disappearing from the
    // room envelope (or vice versa). No image bytes enter either message surface.
    #[tokio::test]
    pub(super) async fn send_preserves_the_same_media_reference_in_store_and_envelope() {
        let media = serde_json::json!({
            "type": "image", "mimeType": "image/png",
            "blobHash": "sha256:1234", "url": null, "description": null
        });
        let stored = media.clone();
        let sent = media.clone();
        let chat = chat_with_stubs(vec![
            Arc::new(StubDataModule::new(move |cmd, p| {
                assert_eq!(cmd, "data/batch");
                assert_eq!(p["operations"][0]["data"]["content"]["media"], json!([stored]));
                assert_eq!(p["operations"][0]["data"]["content"]["text"], "Look at the page");
                Ok(batch_ok())
            })),
            Arc::new(StubAircModule::with(move |p| {
                assert_eq!(p["envelope"]["payload"]["payload"]["inline"]["media"], json!([sent]));
                Ok(airc_ok_response("evt-media"))
            })),
        ]);
        let mut params = sample_send_params();
        params.text = "Look at the page".into();
        params.media = vec![serde_json::from_value(media).expect("existing media reference")];
        chat.send(params).await.expect("media reference persisted and published");
        let legacy: ChatSendParams = serde_json::from_value(json!({
            "roomId": Uuid::new_v4(), "senderId": Uuid::new_v4(), "text": "old text"
        })).expect("legacy text-only request");
        assert!(legacy.media.is_empty());
    }

mod paging;
mod part_2;
mod part_3;
