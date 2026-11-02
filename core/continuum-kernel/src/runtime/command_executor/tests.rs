#![cfg(test)]
    use super::*;
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn test_executor_creation() {
        let registry = Arc::new(ModuleRegistry::new());
        let _executor = CommandExecutor::new(registry);
        // Just verify it compiles and can be created
    }

    #[test]
    fn empty_chain_by_default() {
        let registry = Arc::new(ModuleRegistry::new());
        let executor = CommandExecutor::new(registry);
        assert_eq!(
            executor.interceptor_count(),
            0,
            "fresh executor must have NO interceptors; \
             interceptors are opt-in via with_interceptor or init_executor wiring"
        );
    }

    #[test]
    fn with_interceptor_grows_chain_in_insertion_order() {
        let registry = Arc::new(ModuleRegistry::new());
        let executor = CommandExecutor::new(registry).with_interceptor(Arc::new(RecordingDecliner {
            name: "any",
            seen: Arc::new(AtomicUsize::new(0)),
            mark: 0,
        }));
        assert_eq!(
            executor.interceptor_count(),
            1,
            "with_interceptor must append, not replace"
        );
    }

    /// Test interceptor that records the call order so we can prove the
    /// chain walks in insertion order.
    struct RecordingDecliner {
        name: &'static str,
        seen: Arc<AtomicUsize>,
        mark: usize,
    }

    #[async_trait]
    impl CommandInterceptor for RecordingDecliner {
        async fn try_route(
            &self,
            _command: &str,
            _params: &Value,
            _caller: Option<&crate::routing::CallerIdentity>,
        ) -> Result<InterceptorOutcome, String> {
            // Record which slot was consulted. The test asserts the
            // observed counter equals the expected slot, proving order.
            self.seen.store(self.mark, Ordering::SeqCst);
            Ok(InterceptorOutcome::Decline)
        }

        fn name(&self) -> &'static str {
            self.name
        }
    }

    /// Test interceptor that always handles, used to short-circuit the
    /// fall-through to local Rust + TS dispatch (which would require
    /// actual modules and a live TS bridge — out of scope for unit tests).
    struct AlwaysHandle;

    #[async_trait]
    impl CommandInterceptor for AlwaysHandle {
        async fn try_route(
            &self,
            _command: &str,
            _params: &Value,
            _caller: Option<&crate::routing::CallerIdentity>,
        ) -> Result<InterceptorOutcome, String> {
            Ok(InterceptorOutcome::Handled(CommandResult::Json(
                serde_json::json!({ "handled": true }),
            )))
        }

        fn name(&self) -> &'static str {
            "always-handle"
        }
    }

    #[tokio::test]
    async fn interceptors_walked_in_insertion_order_when_all_decline() {
        let last_seen = Arc::new(AtomicUsize::new(0));
        let registry = Arc::new(ModuleRegistry::new());
        let executor = CommandExecutor::new(registry)
            .with_interceptor(Arc::new(RecordingDecliner {
                name: "first",
                seen: last_seen.clone(),
                mark: 1,
            }))
            .with_interceptor(Arc::new(RecordingDecliner {
                name: "second",
                seen: last_seen.clone(),
                mark: 2,
            }))
            .with_interceptor(Arc::new(AlwaysHandle));

        let result = executor
            .execute("anything", Value::Null)
            .await
            .expect("AlwaysHandle should resolve the dispatch");

        match result {
            CommandResult::Json(v) => assert_eq!(v["handled"], true),
            other => panic!("expected Json, got {other:?}"),
        }
        // The last decliner to run was `second` (mark 2). If the chain
        // walked out of order, this would be `1` or `0`.
        assert_eq!(
            last_seen.load(Ordering::SeqCst),
            2,
            "interceptors must be consulted in insertion order"
        );
    }

    #[tokio::test]
    async fn first_handler_short_circuits_later_interceptors() {
        let later_called = Arc::new(AtomicUsize::new(0));
        let registry = Arc::new(ModuleRegistry::new());
        let executor = CommandExecutor::new(registry)
            .with_interceptor(Arc::new(AlwaysHandle))
            .with_interceptor(Arc::new(RecordingDecliner {
                name: "should-never-run",
                seen: later_called.clone(),
                mark: 99,
            }));

        let _ = executor.execute("anything", Value::Null).await.unwrap();
        assert_eq!(
            later_called.load(Ordering::SeqCst),
            0,
            "interceptors after the first Handled must not be consulted"
        );
    }

    // ════════════════════════════════════════════════════════════════
    // No-fallbacks contract (task #219)
    // ════════════════════════════════════════════════════════════════

    // what this catches: a command not handled by any interceptor and
    // not registered in the Rust module registry returns a typed
    // `CommandNotFound`-shaped error per [[no-fallbacks-ever]]. Pre-
    // PR #1585 the executor silently routed to the TS bridge on
    // `/tmp/jtag-command-router.sock`, which in headless mode
    // surfaced as a cryptic "Failed to connect" error and in
    // hybrid-host mode silently delegated to TS — both breaking the
    // mental model. The fix: refuse the implicit fallthrough,
    // surface a typed error that names the missing command and the
    // explicit escape hatch (`execute_ts_json`).
    #[tokio::test]
    async fn unknown_command_returns_typed_no_fallback_error_not_ts_attempt() {
        let registry = Arc::new(ModuleRegistry::new());
        let executor = CommandExecutor::new(registry);

        let err = executor
            .execute("totally/made-up/command", Value::Null)
            .await
            .expect_err("unknown command must produce a typed error");

        // Must NOT mention the TS socket or "CommandRouterServer" —
        // those strings would prove the implicit fallthrough was
        // still alive. Substring assertions over exact-string
        // matches so we don't pin too tightly on phrasing.
        assert!(
            !err.contains("CommandRouterServer"),
            "error must NOT attempt the TS fallthrough: {err}"
        );
        assert!(
            !err.contains("/tmp/jtag-command-router.sock"),
            "error must NOT mention the TS socket path: {err}"
        );

        // MUST name the missing command so operators know what to
        // migrate or remove.
        assert!(
            err.contains("totally/made-up/command"),
            "error must name the missing command: {err}"
        );
        // MUST point at the explicit escape hatch so a caller that
        // genuinely meant to hit TS knows what method to call.
        assert!(
            err.contains("execute_ts_json") || err.contains("execute_ts"),
            "error must point at the explicit TS-bridge API: {err}"
        );
    }

    // ════════════════════════════════════════════════════════════════
    // command:completed event emission (PERSONA-AS-DEVELOPER-GAP §P3)
    // ════════════════════════════════════════════════════════════════
    //
    // Every dispatch through `execute()` should publish ONE
    // command:completed event on the wired bus, with the command
    // name + duration + success flag + optional error. Tests pin the
    // wire shape, the success/failure parity, the no-bus no-op
    // path, and the multi-thread emission invariants.

    use super::super::command_events::{CommandCompletedEvent, COMMAND_COMPLETED_TOPIC};
    use super::super::message_bus::MessageBus;

    use super::super::module_harness::CannedModule;

    /// Drain the bus receiver until we find an event named
    /// `command:completed`. Returns the parsed payload.
    async fn next_command_completed(
        rx: &mut tokio::sync::broadcast::Receiver<crate::runtime::message_bus::BusEvent>,
    ) -> CommandCompletedEvent {
        // Bound the wait so a missing event fails the test loudly
        // instead of hanging.
        let recv = tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                let event = rx.recv().await.expect("bus channel must not close");
                if event.name == COMMAND_COMPLETED_TOPIC {
                    return event;
                }
            }
        })
        .await
        .expect("expected a command:completed event within 2s");
        serde_json::from_value((*recv.payload).clone()).expect("event payload must parse")
    }

    #[tokio::test]
    async fn dispatch_emits_completed_event_on_success() {
        let registry = Arc::new(ModuleRegistry::new());
        registry.register(Arc::new(CannedModule::answering(Ok(serde_json::json!({ "ok": true })))));
        let bus = Arc::new(MessageBus::new());
        let mut rx = bus.receiver();
        let executor = CommandExecutor::new(registry).with_message_bus(bus);

        executor
            .execute("canned/ping", serde_json::json!({}))
            .await
            .expect("dispatch succeeds");

        let event = next_command_completed(&mut rx).await;
        assert_eq!(event.command_name, "canned/ping");
        assert!(event.success);
        assert!(
            event.error.is_none(),
            "success path must not carry an error: {event:?}"
        );
        // Duration is wall-clock — should be non-pathological. The
        // canned module returns immediately; even on slow CI 500ms
        // is generous.
        assert!(
            event.duration_ms < 500,
            "trivial dispatch should be fast: {} ms",
            event.duration_ms
        );
        // A synchronous dispatch stays thin: no handle, no result on the event (the caller
        // already holds the return value).
        assert_eq!(
            event.handle, None,
            "sync command carries no dispatch handle"
        );
        assert_eq!(
            event.result, None,
            "sync command's result is not duplicated onto the event"
        );
    }

    // what this catches: dispatch_background returns after durable acceptance, before execution,
    // and its completion event carries BOTH the handle and the result — so a subscriber
    // (a persona that sent a sentinel away) matches the completion to its dispatch and
    // folds the outcome in without a second call. This is the producer for the WM async
    // recency channel.
    #[tokio::test]
    async fn dispatch_background_completion_carries_handle_and_result() -> Result<(), String> {
        let registry = Arc::new(ModuleRegistry::new());
        registry.register(Arc::new(CannedModule::answering(Ok(serde_json::json!({ "built": true, "warnings": 0 })))));
        let bus = Arc::new(MessageBus::new());
        let mut rx = bus.receiver();
        let executor = Arc::new(CommandExecutor::new(registry).with_message_bus(bus));

        let (adapter, _directory) = crate::orm::store::fresh_adapter().await;
        let ledger = Arc::new(crate::runtime::tasks::TaskLedger::new(adapter).await?);
        executor.set_task_ledger(ledger.clone())?;
        let owner = uuid::Uuid::new_v4();
        let caller = crate::routing::CallerIdentity::local_persona(crate::identity::PeerId::from_uuid(owner));
        // Await durable acceptance, not the command's execution.
        let handle = executor.dispatch_background("canned/ping", serde_json::json!({}), Some(caller))
            .await.map_err(|e| e.message)?;

        let event = next_command_completed(&mut rx).await;
        assert_eq!(event.command_name, "canned/ping");
        assert!(event.success);
        assert_eq!(
            event.handle,
            Some(handle),
            "completion carries the dispatch handle"
        );
        assert_eq!(
            event.result,
            Some(serde_json::json!({ "built": true, "warnings": 0 })),
            "completion carries the result — no second call needed"
        );
        assert!(matches!(ledger.pending(handle, owner).await?, Some(crate::runtime::tasks::TaskEnd::Done(_))),
            "terminal receipt must be durable before the completion event");
        Ok(())
    }

    #[tokio::test]
    async fn background_acceptance_refusal_never_reaches_dispatch() {
        let seen = Arc::new(AtomicUsize::new(0));
        let executor = Arc::new(CommandExecutor::new(Arc::new(ModuleRegistry::new()))
            .with_interceptor(Arc::new(RecordingDecliner { name: "effect", seen: seen.clone(), mark: 1 })));
        let anonymous = executor.dispatch_background("effect/run", serde_json::json!({}), None).await;
        assert!(matches!(anonymous, Err(DispatchFailure { outcome: DispatchOutcome::NotCommitted, .. })));
        let caller = crate::routing::CallerIdentity::local_persona(crate::identity::PeerId::from_uuid(uuid::Uuid::new_v4()));
        // Identity alone is insufficient: no storage provider means no durable acceptance.
        let no_store = executor.dispatch_background("effect/run", serde_json::json!({}), Some(caller)).await;
        assert!(matches!(no_store, Err(DispatchFailure { outcome: DispatchOutcome::NotCommitted, .. })));
        tokio::task::yield_now().await;
        assert_eq!(seen.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn background_failure_retains_effect_classification_after_reopen() -> Result<(), String> {
        use crate::runtime::tasks::{TaskEnd, TaskLedger};
        let (adapter, _directory) = crate::orm::store::fresh_adapter().await;
        let ledger = Arc::new(TaskLedger::new(adapter.clone()).await?);
        let registry = Arc::new(ModuleRegistry::new());
        registry.register(Arc::new(CannedModule::answering(Err("handler failed after possible effect".into()))));
        let bus = Arc::new(MessageBus::new());
        let mut rx = bus.receiver();
        let executor = Arc::new(CommandExecutor::new(registry).with_message_bus(bus));
        executor.set_task_ledger(ledger)?;
        let owner = uuid::Uuid::new_v4();
        for (command, expected) in [
            ("canned/boom", DispatchOutcome::Uncertain),
            ("absent/command", DispatchOutcome::NotCommitted),
        ] {
            let caller = crate::routing::CallerIdentity::local_persona(crate::identity::PeerId::from_uuid(owner));
            let handle = executor.dispatch_background(command, serde_json::json!({}), Some(caller))
                .await.map_err(|error| error.message)?;
            let event = next_command_completed(&mut rx).await;
            assert_eq!(event.handle, Some(handle));
            assert!(!event.success);
            let reopened = TaskLedger::new(adapter.clone()).await?;
            match reopened.pending(handle, owner).await? {
                Some(TaskEnd::Failed { message, outcome }) => {
                    assert_eq!(outcome, expected);
                    assert_eq!(Some(message), event.error);
                }
                other => return Err(format!("expected durable failure, got {other:?}")),
            }
        }
        Ok(())
    }

    #[tokio::test]
    async fn dispatch_emits_completed_event_on_handler_error() {
        let registry = Arc::new(ModuleRegistry::new());
        registry.register(Arc::new(CannedModule::answering(Err("simulated handler failure".to_string()))));
        let bus = Arc::new(MessageBus::new());
        let mut rx = bus.receiver();
        let executor = CommandExecutor::new(registry).with_message_bus(bus);

        let err = executor
            .execute("canned/boom", serde_json::json!({}))
            .await
            .expect_err("handler returned Err");
        assert_eq!(err, "simulated handler failure");

        let event = next_command_completed(&mut rx).await;
        assert_eq!(event.command_name, "canned/boom");
        assert!(!event.success, "handler Err → success=false");
        assert_eq!(
            event.error.as_deref(),
            Some("simulated handler failure"),
            "error field carries the underlying message"
        );
    }

    #[tokio::test]
    async fn dispatch_without_wired_bus_is_no_op_telemetry() {
        // No bus = no event emission, but the dispatch itself must
        // still complete normally. This is the back-compat path for
        // tests + the old init_executor calls.
        let registry = Arc::new(ModuleRegistry::new());
        registry.register(Arc::new(CannedModule::answering(Ok(serde_json::json!({ "ok": true })))));
        let executor = CommandExecutor::new(registry);
        assert!(!executor.has_message_bus(), "no bus wired");

        // Must succeed; no events emitted (nothing to subscribe to).
        let r = executor.execute("canned/ping", serde_json::json!({})).await;
        assert!(r.is_ok());
    }

    #[tokio::test]
    async fn no_rust_module_failure_still_emits_completed_event() {
        // When all dispatch tiers fail (no interceptor handled, no
        // Rust module registered, no implicit fallthrough per task
        // #219) — the event should still emit with success=false +
        // the typed CommandNotFound error. Telemetry must cover
        // every dispatch path's terminal state, including the new
        // no-fallback surface.
        let registry = Arc::new(ModuleRegistry::new());
        let bus = Arc::new(MessageBus::new());
        let mut rx = bus.receiver();
        let executor = CommandExecutor::new(registry).with_message_bus(bus);

        let err = executor
            .execute("nonexistent/command", serde_json::json!({}))
            .await
            .expect_err("unknown command produces typed error");
        // Don't pin specific error text here; just confirm it's an
        // Err. The dedicated `unknown_command_returns_typed_no_fallback_error_not_ts_attempt`
        // test pins the exact contract.
        let _ = err;

        let event = next_command_completed(&mut rx).await;
        assert_eq!(event.command_name, "nonexistent/command");
        assert!(!event.success);
        assert!(
            event.error.is_some(),
            "no-fallback failure path must populate error: {event:?}"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_dispatches_each_emit_their_own_event() {
        // N parallel dispatches must each emit ONE event with the
        // correct command_name + success flag. No event interleaving
        // corruption, no event loss, no event duplication.
        const PARALLEL: usize = 32;
        let registry = Arc::new(ModuleRegistry::new());
        registry.register(Arc::new(CannedModule::answering(Ok(serde_json::json!({ "ok": true })))));
        let bus = Arc::new(MessageBus::new());
        let mut rx = bus.receiver();
        let executor = Arc::new(CommandExecutor::new(registry).with_message_bus(bus));

        let mut tasks = Vec::with_capacity(PARALLEL);
        for i in 0..PARALLEL {
            let exec = executor.clone();
            let cmd = format!("canned/op-{i:02}");
            tasks.push(tokio::spawn(async move {
                exec.execute(&cmd, serde_json::json!({})).await
            }));
        }
        for t in tasks {
            t.await.unwrap().expect("each dispatch succeeds");
        }

        // Drain bus; collect every command:completed event up to N
        // (with a deadline so a missing event fails loud).
        let mut events: Vec<CommandCompletedEvent> = Vec::with_capacity(PARALLEL);
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
        while events.len() < PARALLEL {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                break;
            }
            match tokio::time::timeout(remaining, rx.recv()).await {
                Ok(Ok(event)) if event.name == COMMAND_COMPLETED_TOPIC => {
                    let parsed: CommandCompletedEvent =
                        serde_json::from_value((*event.payload).clone()).expect("payload parses");
                    events.push(parsed);
                }
                Ok(Ok(_)) => continue, // unrelated event topic — skip
                Ok(Err(_)) => break,
                Err(_) => break,
            }
        }

        assert_eq!(
            events.len(),
            PARALLEL,
            "each concurrent dispatch must emit exactly one event"
        );

        // Every emitted command_name must be unique and match a
        // dispatched op. No event corruption from interleaved
        // publish().
        let mut names: Vec<String> = events.iter().map(|e| e.command_name.clone()).collect();
        names.sort();
        let expected: Vec<String> = (0..PARALLEL).map(|i| format!("canned/op-{i:02}")).collect();
        let mut expected_sorted = expected.clone();
        expected_sorted.sort();
        assert_eq!(
            names, expected_sorted,
            "every dispatched command must appear exactly once in the event stream"
        );

        // Every event reports success (the canned module returns Ok).
        for e in &events {
            assert!(e.success, "all canned dispatches succeed: {e:?}");
            assert!(e.error.is_none());
        }
    }

    // Note: the URI-propagation-across-await assertion lives in
    // `crate::routing::uri_layer::tests` where it can run against the
    // Layer directly without the noise of CommandExecutor's other
    // tokio-runtime tests sharing the cargo test process.

    // ─── Slice P: auth policy gate ──────────────────────────────────

    /// The policy gate runs BEFORE the dispatcher's transport match —
    /// a Forbidden verdict short-circuits to a typed error without
    /// hitting any interceptor or module. Proves the chokepoint
    /// behavior the design doc names: "every URI has a gate."
    #[tokio::test]
    async fn forbidden_policy_short_circuits_before_local_dispatch() {
        let later_called = Arc::new(AtomicUsize::new(0));
        let registry = Arc::new(ModuleRegistry::new());
        let executor = CommandExecutor::new(registry)
            .with_policy(Arc::new(crate::routing::deny_path_prefix("forbidden/")))
            // An interceptor that would normally short-circuit — the
            // policy should reject the request BEFORE it gets here.
            .with_interceptor(Arc::new(RecordingDecliner {
                name: "should-never-run",
                seen: later_called.clone(),
                mark: 42,
            }));

        let err = executor
            .execute("forbidden/some-op", Value::Null)
            .await
            .expect_err("forbidden policy must reject the dispatch");
        assert!(
            err.contains("forbidden"),
            "error must name the forbidden verdict, got: {err}"
        );
        assert!(
            err.contains("forbidden/some-op"),
            "error must name the URI path that was denied, got: {err}"
        );
        assert_eq!(
            later_called.load(Ordering::SeqCst),
            0,
            "interceptors must NOT be consulted after a Forbidden verdict"
        );
    }

    /// A Deferred verdict also short-circuits — the dispatcher's
    /// error names the prompt target env so the operator knows where
    /// consent will be routed (once the consent transport lands).
    #[tokio::test]
    async fn deferred_policy_short_circuits_with_target_env_in_error() {
        let registry = Arc::new(ModuleRegistry::new());
        let executor = CommandExecutor::new(registry).with_policy(Arc::new(
            crate::routing::defer_path_prefix(
                "persona/state/",
                crate::routing::EnvironmentId::Named("web".into()),
            ),
        ));

        let err = executor
            .execute("persona/state/mutate", Value::Null)
            .await
            .expect_err("deferred policy returns a typed error");
        assert!(
            err.contains("deferred"),
            "error must name the deferred verdict, got: {err}"
        );
        assert!(
            err.contains("web"),
            "error must name the consent target env, got: {err}"
        );
    }

    /// Proves the Transport trait extraction is wired correctly: a
    /// Peer URI flows through `route()` → policy gate → the
    /// installed remote Transport. When AircTransport lands, it
    /// slots into the same call site and personas talk
    /// cross-machine.
    #[tokio::test]
    async fn peer_uri_routes_through_installed_remote_transport() {
        use crate::routing::{ClosureTransport, RouteDecision};
        use std::sync::Mutex;

        let captured_path: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let captured_path_clone = captured_path.clone();

        let transport =
            ClosureTransport::new(
                "test-peer-transport",
                move |decision, _params| match &decision {
                    RouteDecision::Peer { path, .. } => {
                        *captured_path_clone.lock().unwrap() = Some(path.clone());
                        Ok(CommandResult::Json(serde_json::json!({
                            "routed-through": "test-peer-transport",
                        })))
                    }
                    other => panic!("expected Peer, got {other:?}"),
                },
            );

        let registry = Arc::new(ModuleRegistry::new());
        let executor = CommandExecutor::new(registry).with_remote_transport(Arc::new(transport));

        let result = executor
            .execute("airc://maya/inference/llm/generate", Value::Null)
            .await
            .expect("transport routes the peer URI");

        match result {
            CommandResult::Json(v) => {
                assert_eq!(v["routed-through"], "test-peer-transport");
            }
            other => panic!("expected Json, got {other:?}"),
        }
        assert_eq!(
            captured_path.lock().unwrap().as_deref(),
            Some("inference/llm/generate"),
            "transport must receive the parsed Peer path"
        );
    }

    /// AllowAllPolicy (the default) is transparent — no behavior
    /// difference from a substrate without a gate. Proves the
    /// retrofit doesn't break existing call sites.
    #[tokio::test]
    async fn default_policy_lets_dispatch_through() {
        let later_called = Arc::new(AtomicUsize::new(0));
        let registry = Arc::new(ModuleRegistry::new());
        let executor = CommandExecutor::new(registry)
            // No with_policy call — AllowAllPolicy is the default
            .with_interceptor(Arc::new(RecordingDecliner {
                name: "must-run",
                seen: later_called.clone(),
                mark: 99,
            }))
            .with_interceptor(Arc::new(AlwaysHandle));

        let _ = executor
            .execute("anything", Value::Null)
            .await
            .expect("default policy allows dispatch");
        assert_eq!(
            later_called.load(Ordering::SeqCst),
            99,
            "decliner must have been consulted (proves policy was Allow)"
        );
    }
