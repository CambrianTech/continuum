use super::*;

#[cfg(test)]
pub(super) mod piece_2_pr3_dispatch_tests {
    //! PIECE-2 PR-3 dispatch tests.
    //!
    //! Proves the registration → bus.subscribe → handle_event →
    //! on_artifact_available chain wires correctly for both
    //! ArtifactSelector::Exact and ArtifactSelector::Prefix (via the
    //! dedicated artifact-subscriber path on MessageBus added in the
    //! follow-up to PR-3), and that modules NOT opted-in see no
    //! artifact dispatch (backwards-compat guarantee).
    //!
    //! Test fixture: a tracking module that records every
    //! on_artifact_available call into a shared Vec the test asserts
    //! against after publishing.
    use super::super::*;
    use crate::runtime::artifact_handle::{ArtifactKey, ArtifactSelector};
    use crate::runtime::service_module::{
        CommandResult, ModuleConfig, ModulePriority, ServiceModule,
    };
    use async_trait::async_trait;
    use parking_lot::Mutex;
    use std::any::Any;
    use std::sync::Arc;

    /// `pub(super)` so sibling test mods can use the ONE recording fixture instead of
    /// each writing their own — the duplication CLAUDE.md's task #155 exists to stop.
    pub(in crate::runtime::runtime) struct RecordingModule {
        name: &'static str,
        subscriptions: Vec<ArtifactSelector>,
        received: Arc<Mutex<Vec<(ArtifactKey, serde_json::Value)>>>,
        event_gate: Option<Arc<tokio::sync::Notify>>,
        /// How many times the runtime asked this module to SAVE. Counting broadcasts
        /// rather than receipt rows is the difference between "the receipt mentions this
        /// module once" and "the module was stopped once" — a second broadcast could stop
        /// it again and still produce one row per publication.
        pub(in crate::runtime::runtime) saves: Arc<std::sync::atomic::AtomicUsize>,
    }

    impl RecordingModule {
        pub(in crate::runtime::runtime) fn new(
            name: &'static str,
            subscriptions: Vec<ArtifactSelector>,
        ) -> (Arc<Self>, Arc<Mutex<Vec<(ArtifactKey, serde_json::Value)>>>) {
            let received = Arc::new(Mutex::new(Vec::new()));
            let module = Arc::new(Self {
                saves: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
                name,
                subscriptions,
                received: received.clone(),
                event_gate: None,
            });
            (module, received)
        }
    }

    #[async_trait]
    impl ServiceModule for RecordingModule {
        /// Counts the runtime's save BROADCASTS at this module. The idempotence test
        /// asserts on this rather than on receipt rows: a second `begin` that ran a
        /// parallel broadcast would call this twice while still publishing one receipt.
        async fn save_state(&self) -> Result<(), String> {
            self.saves
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Ok(())
        }

        fn config(&self) -> ModuleConfig {
            ModuleConfig {
                name: self.name,
                priority: ModulePriority::Normal,
                command_prefixes: &[],
                event_subscriptions: &[],
                needs_dedicated_thread: false,
                max_concurrency: 0,
                tick_interval: None,
            }
        }
        async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
            Ok(())
        }
        async fn handle_command(
            &self,
            _command: &str,
            _params: serde_json::Value,
        ) -> Result<CommandResult, String> {
            Err("not handled".to_string())
        }
        fn artifact_subscriptions(&self) -> Vec<ArtifactSelector> {
            self.subscriptions.clone()
        }
        async fn on_artifact_available(
            &self,
            key: &ArtifactKey,
            value: serde_json::Value,
        ) -> Result<(), String> {
            self.received.lock().push((key.clone(), value));
            if let Some(gate) = &self.event_gate {
                gate.notified().await;
            }
            Ok(())
        }
        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    // What this catches: a blocked inline subscriber must not withhold publication
    // from independent consumers or prevent an urgent event reaching the same rail.
    #[tokio::test]
    async fn publication_reaches_receivers_while_inline_handler_is_blocked() {
        let runtime = Runtime::new();
        let key = "test/background.ready";
        let gate = Arc::new(tokio::sync::Notify::new());
        let (mut module, received) = RecordingModule::new(
            "blocked-recorder",
            vec![ArtifactSelector::Exact(ArtifactKey::from(key))],
        );
        Arc::get_mut(&mut module).unwrap().event_gate = Some(gate.clone());
        runtime.register(module);
        let mut receiver = runtime.bus().receiver();
        let publication = runtime.bus().publish(key, serde_json::json!({"id": 1}), runtime.registry());
        tokio::pin!(publication);
        // Poll the real dispatcher into the blocked handler before checking the rail.
        tokio::select! {
            biased;
            _ = &mut publication => panic!("handler must still be blocked"),
            event = receiver.recv() => assert_eq!(event.unwrap().name, key),
            _ = tokio::time::sleep(Duration::from_secs(2)) => panic!("subscriber blocked publication"),
        }
        assert_eq!(received.lock().len(), 1);
        runtime.bus().publish_async_only("chat:urgent", serde_json::json!({"id": 2}));
        assert_eq!(receiver.try_recv().unwrap().name, "chat:urgent");
        gate.notify_one();
        publication.await;
        assert_eq!(received.lock().len(), 1);
        assert!(matches!(receiver.try_recv(), Err(tokio::sync::broadcast::error::TryRecvError::Empty)));
    }

    /// What this catches: ArtifactSelector::Exact translates to a
    /// literal bus pattern. Publishing the matching key delivers via
    /// the default handle_event → on_artifact_available chain;
    /// publishing a non-matching key does not.
    #[tokio::test]
    async fn exact_selector_delivers_only_matching_key() {
        let runtime = Runtime::new();
        let (module, received) = RecordingModule::new(
            "exact-recorder",
            vec![ArtifactSelector::Exact(ArtifactKey::from(
                "paging/broker.snapshot",
            ))],
        );
        runtime.register(module);

        runtime
            .bus()
            .publish(
                "paging/broker.snapshot",
                serde_json::json!({"pressure": 0.42}),
                runtime.registry(),
            )
            .await;

        // Different key — not delivered.
        runtime
            .bus()
            .publish(
                "cognition/rate_proposals.result",
                serde_json::json!({"foo": "bar"}),
                runtime.registry(),
            )
            .await;

        // Prefix-shaped collision — not delivered (Exact must be
        // string-equality, not prefix-equality).
        runtime
            .bus()
            .publish(
                "paging/broker.snapshot.delta",
                serde_json::json!({"foo": "bar"}),
                runtime.registry(),
            )
            .await;

        let calls = received.lock().clone();
        assert_eq!(
            calls.len(),
            1,
            "exact selector should deliver only the literal match; got {:?}",
            calls
                .iter()
                .map(|(k, _)| k.as_str().to_string())
                .collect::<Vec<_>>()
        );
        assert_eq!(calls[0].0.as_str(), "paging/broker.snapshot");
        assert_eq!(calls[0].1["pressure"], 0.42);
    }

    /// What this catches (PR-3 follow-up): ArtifactSelector::Prefix
    /// now actually delivers. Original PR-3 (#1339) pinned this as
    /// no-op because the routing crammed ArtifactKeys through the
    /// bus's colon-segmented glob_matches. This follow-up adds a
    /// dedicated artifact-subscriber path on MessageBus that uses
    /// ArtifactSelector::matches directly, so Prefix("cognition/")
    /// matches anything starting with that string.
    ///
    /// Also asserts that a non-matching key is NOT delivered — the
    /// bound on the prefix matters, it's not a wildcard.
    #[tokio::test]
    async fn prefix_selector_delivers_matching_keys_and_skips_others() {
        let runtime = Runtime::new();
        let (module, received) = RecordingModule::new(
            "prefix-recorder",
            vec![ArtifactSelector::Prefix("cognition/".to_string())],
        );
        runtime.register(module);

        runtime
            .bus()
            .publish(
                "cognition/rate_proposals.result",
                serde_json::json!({"score": 0.7}),
                runtime.registry(),
            )
            .await;
        runtime
            .bus()
            .publish(
                "cognition/generate_recipe.result",
                serde_json::json!({"recipe_id": "abc"}),
                runtime.registry(),
            )
            .await;

        // Non-matching key — must NOT deliver.
        runtime
            .bus()
            .publish(
                "paging/broker.snapshot",
                serde_json::json!({"pressure": 0.1}),
                runtime.registry(),
            )
            .await;

        let calls = received.lock().clone();
        let delivered_keys: Vec<String> =
            calls.iter().map(|(k, _)| k.as_str().to_string()).collect();
        assert_eq!(
            calls.len(),
            2,
            "Prefix selector should deliver both cognition/* keys; got {:?}",
            delivered_keys
        );
        assert!(delivered_keys.contains(&"cognition/rate_proposals.result".to_string()));
        assert!(delivered_keys.contains(&"cognition/generate_recipe.result".to_string()));
        assert!(
            !delivered_keys.contains(&"paging/broker.snapshot".to_string()),
            "Prefix is a bound, not a wildcard — keys outside the prefix must not deliver"
        );
    }

    /// What this catches: a module that declares NO artifact_subscriptions
    /// receives NOTHING. Backwards-compat: every existing module
    /// (HealthModule, PressureBrokerModule, …) keeps its current
    /// behavior — the new default handle_event is a no-op for
    /// non-opted-in modules.
    #[tokio::test]
    async fn module_without_artifact_subscriptions_receives_nothing() {
        let runtime = Runtime::new();
        let (module, received) = RecordingModule::new("non-opted-in", vec![]);
        runtime.register(module);

        runtime
            .bus()
            .publish(
                "paging/broker.snapshot",
                serde_json::json!({}),
                runtime.registry(),
            )
            .await;
        runtime
            .bus()
            .publish("anything/at/all", serde_json::json!({}), runtime.registry())
            .await;

        assert!(
            received.lock().is_empty(),
            "module with empty subscriptions must receive nothing"
        );
    }

    /// What this catches: two modules with different subscription
    /// sets each receive ONLY their matching events. Multi-subscriber
    /// isolation.
    #[tokio::test]
    async fn multi_module_isolation_each_gets_only_matching_artifacts() {
        let runtime = Runtime::new();
        let (a, received_a) = RecordingModule::new(
            "module-a",
            vec![ArtifactSelector::Exact(ArtifactKey::from(
                "persona/inbox.frame_ready",
            ))],
        );
        let (b, received_b) = RecordingModule::new(
            "module-b",
            vec![ArtifactSelector::Exact(ArtifactKey::from(
                "paging/broker.snapshot",
            ))],
        );
        runtime.register(a);
        runtime.register(b);

        runtime
            .bus()
            .publish(
                "persona/inbox.frame_ready",
                serde_json::json!({"id": "frame-1"}),
                runtime.registry(),
            )
            .await;
        runtime
            .bus()
            .publish(
                "paging/broker.snapshot",
                serde_json::json!({"pressure": 0.5}),
                runtime.registry(),
            )
            .await;

        let a_keys: Vec<String> = received_a
            .lock()
            .iter()
            .map(|(k, _)| k.as_str().to_string())
            .collect();
        let b_keys: Vec<String> = received_b
            .lock()
            .iter()
            .map(|(k, _)| k.as_str().to_string())
            .collect();
        assert_eq!(a_keys, vec!["persona/inbox.frame_ready".to_string()]);
        assert_eq!(b_keys, vec!["paging/broker.snapshot".to_string()]);
    }

    // what this catches: the SOCKET route (`route_command`, used by uu / IPC / MCP)
    // reaches a connected Provided-command provider through the SAME
    // `provider_registry` the in-process persona route uses via the interceptor —
    // no dispatch path can gain (or lack) a capability the other has. Both call the
    // ONE `route_provided`. Regression-pins the "one infrastructure, every caller"
    // invariant for Provided commands (perception/observe) — the exact divergence
    // that let benchmarks take a different command path before.
    #[tokio::test]
    async fn route_command_reaches_the_provider_registry_for_a_provided_command() {
        use crate::runtime::ProvidedCommandProvider;
        use serde_json::json;

        struct FakeEye;
        #[async_trait]
        impl ProvidedCommandProvider for FakeEye {
            async fn fulfill(
                &self,
                command: &str,
                params: serde_json::Value,
            ) -> Result<serde_json::Value, String> {
                Ok(json!({ "success": true, "command": command, "echoedTarget": params["target"] }))
            }
            fn label(&self) -> &str {
                "fake-eye"
            }
        }

        let runtime = Runtime::new();

        // No eye-node connected → the socket route fails loud, named (not "Unknown
        // command", not a fabricated observation).
        match runtime
            .route_command("perception/observe", json!({ "target": "https://x" }), None)
            .await
        {
            Some(Err(e)) => {
                assert!(e.contains("perception/observe"), "names the command: {e}");
                assert!(e.contains("eye-node"), "names the missing adapter: {e}");
            }
            _ => panic!("a Provided command with no provider must fail loud on the socket route"),
        }

        // Bind a provider on the SAME registry the IPC layer binds eye-nodes into.
        runtime
            .provider_registry()
            .register(&["perception/observe"], Arc::new(FakeEye));

        match runtime
            .route_command("perception/observe", json!({ "target": "https://x" }), None)
            .await
        {
            Some(Ok(CommandResult::Json(v))) => {
                assert_eq!(v["success"], true);
                assert_eq!(
                    v["echoedTarget"], "https://x",
                    "forwarded params reached the eye-node"
                );
            }
            _ => {
                panic!("the socket route must forward a Provided command to its connected provider")
            }
        }
    }
}

#[cfg(test)]
pub(super) mod ready_edge_tests {
    //! RTOS Slice A.1 — `ServiceModule::ready_edge` + `Runtime::wait_for_ready`.
    //!
    //! Pins the four interesting paths:
    //!   1. Default impl returns None → wait_for_ready resolves immediately.
    //!   2. Override returns Some with already-true watch → fast path Ok.
    //!   3. Override returns Some with false → wait_for_ready awaits the
    //!      transition to true.
    //!   4. Override's sender drops without publishing true → wait_for_ready
    //!      returns Err (substrate is in trouble; the caller decides).
    //!   5. Unknown module name → Err.
    use super::super::*;
    use crate::runtime::service_module::{
        CommandResult, ModuleConfig, ModulePriority, ServiceModule,
    };
    use async_trait::async_trait;
    use std::any::Any;
    use std::sync::Arc;
    use tokio::sync::watch;

    /// Minimal module fixture. Optionally exposes a ready watch.
    struct ReadyModule {
        name: &'static str,
        ready_rx: Option<watch::Receiver<bool>>,
    }

    impl ReadyModule {
        fn without_ready_edge(name: &'static str) -> Arc<Self> {
            Arc::new(Self {
                name,
                ready_rx: None,
            })
        }

        /// Construct with a sender the test retains, so the test can
        /// publish `true` (or drop the sender) at the right moment.
        fn with_sender(name: &'static str, initial: bool) -> (Arc<Self>, watch::Sender<bool>) {
            let (tx, rx) = watch::channel(initial);
            let module = Arc::new(Self {
                name,
                ready_rx: Some(rx),
            });
            (module, tx)
        }
    }

    #[async_trait]
    impl ServiceModule for ReadyModule {
        fn config(&self) -> ModuleConfig {
            ModuleConfig {
                name: self.name,
                priority: ModulePriority::Normal,
                command_prefixes: &[],
                event_subscriptions: &[],
                needs_dedicated_thread: false,
                max_concurrency: 0,
                tick_interval: None,
            }
        }
        async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
            Ok(())
        }
        async fn handle_command(
            &self,
            _: &str,
            _: serde_json::Value,
        ) -> Result<CommandResult, String> {
            Err("not handled".into())
        }
        fn ready_edge(&self) -> Option<watch::Receiver<bool>> {
            self.ready_rx.clone()
        }
        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    /// What this catches: a module without a custom ready_edge falls
    /// through the runtime's "ready after initialize" semantics
    /// immediately. Regression here would block the entire substrate
    /// boot waiting on the ~30 existing modules that take the default.
    #[tokio::test]
    async fn default_ready_edge_resolves_immediately() {
        let runtime = Runtime::new();
        runtime.register(ReadyModule::without_ready_edge("no-edge"));
        let result = tokio::time::timeout(
            std::time::Duration::from_millis(50),
            runtime.wait_for_ready("no-edge"),
        )
        .await
        .expect("default ready_edge must NOT block");
        assert!(result.is_ok());
    }

    /// What this catches: when the watch is already `true` at the call,
    /// wait_for_ready returns without calling `.changed()` — the fast
    /// path. Regression here = spurious wakeup on first poll.
    #[tokio::test]
    async fn fast_path_when_already_ready() {
        let runtime = Runtime::new();
        let (module, _tx) = ReadyModule::with_sender("already-ready", true);
        runtime.register(module);
        let result = tokio::time::timeout(
            std::time::Duration::from_millis(50),
            runtime.wait_for_ready("already-ready"),
        )
        .await
        .expect("fast path must NOT block");
        assert!(result.is_ok());
    }

    /// What this catches: wait_for_ready actually awaits the watch's
    /// transition from false → true. Regression here = it returns
    /// before the module is actually ready, which is the exact race
    /// the primitive exists to eliminate.
    #[tokio::test]
    async fn awaits_transition_to_true() {
        let runtime = Runtime::new();
        let (module, tx) = ReadyModule::with_sender("not-yet", false);
        runtime.register(module);

        // Publish ready after a short delay; wait_for_ready must observe.
        let _publisher = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            let _ = tx.send(true);
            // Keep tx alive so wait_for_ready sees the change before drop.
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        });

        let result = tokio::time::timeout(
            std::time::Duration::from_millis(200),
            runtime.wait_for_ready("not-yet"),
        )
        .await
        .expect("wait_for_ready must observe within timeout");
        assert!(result.is_ok());
    }

    /// What this catches: if the module's sender drops without ever
    /// publishing true, wait_for_ready surfaces the error rather than
    /// hanging forever. The substrate's job is to refuse to lie about
    /// readiness; silent hang would mask the actual failure.
    #[tokio::test]
    async fn returns_err_when_sender_dropped_before_ready() {
        let runtime = Runtime::new();
        let (module, tx) = ReadyModule::with_sender("doomed", false);
        runtime.register(module);

        // Drop the sender without publishing true.
        drop(tx);

        let result = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            runtime.wait_for_ready("doomed"),
        )
        .await
        .expect("must return without hanging");
        assert!(
            result.is_err(),
            "dropped sender before ready → Err, got Ok(()) — substrate just lied"
        );
    }

    /// What this catches: an unknown module name returns Err rather
    /// than hanging forever or panicking. Operators typo module names;
    /// the failure must be visible.
    #[tokio::test]
    async fn unknown_module_returns_err() {
        let runtime = Runtime::new();
        let result = runtime.wait_for_ready("does-not-exist").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not registered"));
    }
}
