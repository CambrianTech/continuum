use super::*;

#[cfg(test)]
pub(super) mod conditional_modules_tests {
    use super::super::*;

    /// The shutdown result must survive the disappearance of everyone watching it.
    mod receipt_retention {
        use super::super::*;
        // The ONE recording fixture, borrowed from the dispatch tests rather than
        // reimplemented here.
        use crate::runtime::runtime::piece_2_pr3_dispatch_tests::RecordingModule;

        // what this catches: `watch::Sender::send` dropping the terminal receipt when the
        // last receiver has gone. That receiver is the CLI, and its disconnect is the case
        // this whole design exists to survive — so `send` loses the receipt in exactly the
        // situation it matters, and the next subscriber (a retried request, or the signal
        // path joining the same broadcast) waits forever on a `None` that never changes.
        //
        // DRIVEN THROUGH THE REAL OWNER. The first version of this test built its own
        // `watch` and called `send_replace` itself, so it asserted a property of tokio and
        // would have stayed GREEN if the production publisher regressed to `send` — a
        // SOURCE-REVIEW finding by Astra, reasoned from the code rather than executed as a
        // mutation run, and labelled that way because she insisted on the distinction when
        // a relay upgraded it to "she ran it". S6 supplied the shape of the fix —
        // make the operation an instance so a test can hold the same one production holds.
        // This constructs a real `ShutdownOperation` over a real `Runtime` with the
        // existing `RecordingModule`, drops every caller, and only then resubscribes.
        #[tokio::test]
        async fn the_receipt_survives_every_observer_going_away() {
            // A plain local: `begin` needs only `&self` — it clones the owned sender
            // before the spawn and the returned Receiver borrows nothing. My first version
            // leaked a `'static` instance to satisfy a lifetime the code never required.
            let op = ShutdownOperation::new();

            let runtime = Arc::new(Runtime::new());
            let (module, _received) = RecordingModule::new("stop-recorder", Vec::new());
            runtime.register(module);

            // The only observer disconnects — the CLI died, was interrupted, timed out.
            let rx = op.begin(Some(runtime), std::time::Duration::ZERO);
            drop(rx);

            // Wait for the owner to publish. It is a detached task, so this polls a FRESH
            // subscription rather than the one we dropped.
            let mut seen = None;
            for _ in 0..200 {
                if let Some(r) = op.result.borrow().clone() {
                    seen = Some(r);
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }

            let receipt = seen.expect("the owner must publish even with no receivers left");
            assert!(
                receipt.modules.iter().any(|m| m.module == "stop-recorder"),
                "the receipt must describe the module the runtime actually stopped, got {:?}",
                receipt.modules
            );

            // And a subscriber arriving AFTER every observer left still learns what
            // happened — which is the property `send` would have destroyed.
            let late = op.result.subscribe();
            assert_eq!(
                late.borrow().clone(),
                Some(receipt),
                "a late subscriber must still see the terminal receipt"
            );
        }

        // what this catches: `begin` running the broadcast twice. A signal racing the stop
        // verb, or a retried request, must JOIN the first operation — running `save_state`
        // twice over the same state is the corruption the idempotence exists to prevent.
        #[tokio::test]
        async fn a_second_begin_joins_the_first_instead_of_restarting_it() {
            let op = ShutdownOperation::new();
            let runtime = Arc::new(Runtime::new());
            let (module, _r) = RecordingModule::new("join-recorder", Vec::new());
            let saves = module.saves.clone();
            runtime.register(module);

            let _first = op.begin(Some(runtime.clone()), std::time::Duration::ZERO);
            // A second caller with a runtime it would otherwise stop.
            let _second = op.begin(Some(runtime), std::time::Duration::ZERO);

            let mut receipt = None;
            for _ in 0..200 {
                if let Some(r) = op.result.borrow().clone() {
                    receipt = Some(r);
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
            let _receipt = receipt.expect("the operation must complete");
            // ONE BROADCAST, not one receipt row. Counting rows would pass even if the
            // second `begin` ran a parallel broadcast, because each publication produces
            // one row per module regardless of how many times the module was stopped.
            // What must be true is that the module's `save_state` ran ONCE — saving twice
            // over the same state is the corruption the idempotence exists to prevent.
            assert_eq!(
                saves.load(std::sync::atomic::Ordering::Relaxed),
                1,
                "a second begin must JOIN the first, not stop the module again"
            );
        }

        // what this catches: `await_shutdown` treating a still-running stop as a finished
        // one. Its timeout does not cancel anything — the operation is in a task no
        // connection owns — so `None` means UNKNOWN, and a caller that read it as failure
        // or as success would be wrong in opposite directions.
        #[tokio::test]
        async fn an_unfinished_shutdown_reports_unknown_rather_than_a_verdict() {
            let (tx, rx) = tokio::sync::watch::channel(None::<ShutdownReceipt>);
            // A SECOND receiver, held here, is what makes the liveness assertion mean
            // anything: `await_shutdown` consumes the one it is given and drops it, so
            // with only that one alive `is_closed()` becomes true purely because WE
            // stopped watching — which is not the fact under test and would have made
            // this assertion fail (or, worse, pass for the wrong reason on a later
            // refactor). Astra caught it at runtime.rs:1754.
            let _observer = tx.subscribe();
            let out = await_shutdown(rx, std::time::Duration::from_millis(30)).await;
            assert!(
                out.is_none(),
                "a stop still in flight must not produce a receipt"
            );
            // Nothing was cancelled by our giving up: the operation can still publish,
            // and `_observer` proves the channel is live rather than merely unobserved.
            assert!(
                !tx.is_closed(),
                "giving up watching must not end the shutdown"
            );
            let receipt = ShutdownReceipt {
                tasks: DrainOutcome::Drained,
                modules: Vec::new(),
                total_ms: 3,
            };
            tx.send_replace(Some(receipt.clone()));
            assert_eq!(
                _observer.borrow().clone(),
                Some(receipt),
                "the operation we stopped waiting for must still be able to report"
            );
        }
    }

    /// Outcomes the runtime PRODUCES, not ones a test writes down.
    ///
    /// Every other receipt test constructs `ModuleStopOutcome` values as literals and
    /// asserts on them, which describes the enum rather than the code that fills it — so
    /// deleting the timeout handling in `shutdown` would have left them all green. These
    /// drive a real `Runtime` with modules that genuinely exceed the phase bound. Found by
    /// IntelMac.
    mod outcomes_the_runtime_actually_produces {
        use super::super::*;
        use crate::runtime::runtime::piece_2_pr3_dispatch_tests::RecordingModule;
        // The ServiceModule surface is not in this mod's `use super::*` reach; naming the
        // imports beats a glob here because the two stub modules below implement the trait
        // and a missing one shows up as four unrelated-looking errors.
        use crate::runtime::service_module::{ModuleConfig, ModulePriority, ServiceModule};
        // NOT from `service_module` — it does not export this; the trait itself names
        // `super::ModuleContext`.
        use crate::runtime::ModuleContext;
        use async_trait::async_trait;
        use std::any::Any;

        /// A module whose SAVE never finishes inside the bound.
        struct SlowSaver;
        #[async_trait]
        impl ServiceModule for SlowSaver {
            fn config(&self) -> ModuleConfig {
                ModuleConfig {
                    name: "slow-saver",
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
                // Required: `ServiceModule::handle_command` has no default. These stubs
                // exist to exceed a phase bound, not to serve commands.
                Err("not handled".to_string())
            }
            async fn save_state(&self) -> Result<(), String> {
                tokio::time::sleep(std::time::Duration::from_millis(400)).await;
                Ok(())
            }
            fn as_any(&self) -> &dyn Any {
                self
            }
        }

        /// A module that saves promptly and then never lets go.
        struct SlowJoiner;
        #[async_trait]
        impl ServiceModule for SlowJoiner {
            fn config(&self) -> ModuleConfig {
                ModuleConfig {
                    name: "slow-joiner",
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
                // Required: `ServiceModule::handle_command` has no default. These stubs
                // exist to exceed a phase bound, not to serve commands.
                Err("not handled".to_string())
            }
            async fn shutdown(&self) -> Result<(), String> {
                tokio::time::sleep(std::time::Duration::from_millis(400)).await;
                Ok(())
            }
            fn as_any(&self) -> &dyn Any {
                self
            }
        }

        // what this catches: the save timeout arm being deleted or inverted. A module that
        // cannot save inside the bound must produce `SaveTimedOut` and make the stop
        // non-durable — this is the outcome the CLI's exit code keys on, and until now no
        // test had ever caused one.
        // `start_paused` — time is VIRTUAL here, and that is not a speed optimisation.
        //
        // With a real clock, a 400ms sleep against a 50ms timeout can still return
        // `Clean`: pinned Tokio's `Timeout::poll` polls the INNER future FIRST, so if the
        // scheduler stalls long enough that the sleep has already completed by the time
        // the timeout is polled, the inner future wins. Under CI contention that is not
        // hypothetical.
        //
        // The flake is a FALSE NEGATIVE, not a false positive: these tests ASSERT
        // `SaveTimedOut`, so a stall that yields `Clean` makes the assertion FAIL and the
        // test go red. Spurious failure on a loaded machine — annoying, worth removing,
        // and it never certifies anything wrongly.
        //
        // Saying so precisely because the first version of this comment claimed the
        // opposite ("fails open", certifying durability on a machine that cannot tell).
        // That was wrong: it conflated the production path RETURNING Clean with the TEST
        // PASSING, and the assertion sits between them. Mechanism from Astra and Popper;
        // the retraction of the stronger claim from IntelMac, who traced it rather than
        // just accepting the correction.
        //
        // Paused, the clock advances only when every task is idle and only to the nearest
        // deadline, so the 50ms timeout fires before the 400ms sleep can complete, on any
        // machine, every time.
        #[tokio::test(start_paused = true)]
        async fn a_module_that_cannot_save_in_time_produces_save_timed_out() {
            let runtime = Runtime::new();
            runtime.register(Arc::new(SlowSaver));
            let receipt = runtime
                .shutdown_within(std::time::Duration::from_millis(50))
                .await;
            let row = receipt
                .modules
                .iter()
                .find(|m| m.module == "slow-saver")
                .expect("the slow module must appear in the receipt");
            assert_eq!(row.outcome, ModuleStopOutcome::SaveTimedOut);
            assert!(
                !row.state_is_durable(),
                "a save that timed out leaves the state UNKNOWN, not merely old"
            );
            assert!(!receipt.state_is_durable());
            assert!(receipt.summary().contains("slow-saver"));
        }

        // what this catches: the join timeout arm. The module SAVED, so an implementation
        // that stopped bounding the join would report `Clean` — and with `shutdown`
        // contractually being "release resources, FLUSH BUFFERS", that would claim
        // durability over a flush that may not have happened.
        // `start_paused` — time is VIRTUAL here, and that is not a speed optimisation.
        //
        // With a real clock, a 400ms sleep against a 50ms timeout can still return
        // `Clean`: pinned Tokio's `Timeout::poll` polls the INNER future FIRST, so if the
        // scheduler stalls long enough that the sleep has already completed by the time
        // the timeout is polled, the inner future wins. Under CI contention that is not
        // hypothetical.
        //
        // The flake is a FALSE NEGATIVE, not a false positive: these tests ASSERT
        // `SaveTimedOut`, so a stall that yields `Clean` makes the assertion FAIL and the
        // test go red. Spurious failure on a loaded machine — annoying, worth removing,
        // and it never certifies anything wrongly.
        //
        // Saying so precisely because the first version of this comment claimed the
        // opposite ("fails open", certifying durability on a machine that cannot tell).
        // That was wrong: it conflated the production path RETURNING Clean with the TEST
        // PASSING, and the assertion sits between them. Mechanism from Astra and Popper;
        // the retraction of the stronger claim from IntelMac, who traced it rather than
        // just accepting the correction.
        //
        // Paused, the clock advances only when every task is idle and only to the nearest
        // deadline, so the 50ms timeout fires before the 400ms sleep can complete, on any
        // machine, every time.
        #[tokio::test(start_paused = true)]
        async fn a_module_that_cannot_join_in_time_produces_join_timed_out() {
            let runtime = Runtime::new();
            runtime.register(Arc::new(SlowJoiner));
            let receipt = runtime
                .shutdown_within(std::time::Duration::from_millis(50))
                .await;
            let row = receipt
                .modules
                .iter()
                .find(|m| m.module == "slow-joiner")
                .expect("the slow module must appear in the receipt");
            assert_eq!(row.outcome, ModuleStopOutcome::JoinTimedOut);
            assert!(!row.state_is_durable());
        }

        // what this catches: the ordinary path degrading. A module that drains, saves and
        // joins promptly must still come back Clean and durable under the same bound — or
        // the two tests above would pass for the wrong reason.
        // Paused for the same reason, and it matters MORE here: a control that ran on a
        // different clock from the cases it controls is not a control.
        #[tokio::test(start_paused = true)]
        async fn a_prompt_module_is_clean_under_the_same_bound() {
            let runtime = Runtime::new();
            let (module, _r) = RecordingModule::new("prompt", Vec::new());
            runtime.register(module);
            let receipt = runtime
                .shutdown_within(std::time::Duration::from_millis(50))
                .await;
            let row = receipt
                .modules
                .iter()
                .find(|m| m.module == "prompt")
                .expect("present");
            assert_eq!(row.outcome, ModuleStopOutcome::Clean);
            assert!(row.state_is_durable());
            assert!(receipt.state_is_durable());
        }
    }

    /// The shutdown receipt's semantics. These are pure and cheap, and they exist because
    /// the whole rail turns on ONE distinction: a module that failed to SAVE lost state,
    /// and a module that failed to JOIN merely exited untidily. Collapse them and
    /// `continuum stop` goes back to reporting success for a stop that lost a citizen's
    /// working memory.
    mod shutdown_receipt {
        use super::super::*;

        fn stop(module: &str, outcome: ModuleStopOutcome) -> ModuleStop {
            ModuleStop {
                module: module.to_string(),
                drain: DrainOutcome::Drained,
                outcome,
                ms: 1,
            }
        }

        // what this catches: a save timeout treated as a successful stop. This is the
        // exact value the CLI's exit code keys on.
        // what this catches: a failed JOIN being read as durable state. I originally
        // ruled that a module which saved and then failed to let go was untidy rather
        // than lossy — but `ServiceModule::shutdown` is contractually "release resources,
        // FLUSH BUFFERS", and the logger's implementation is `flush_all`. A join that did
        // not finish is a flush that may not have happened, which makes the last writes
        // exactly as gone as a failed save's. Astra read the contract I had written and
        // I had not.
        #[test]
        fn only_a_fully_completed_stop_can_support_a_durability_claim() {
            assert!(ModuleStopOutcome::Clean.completed());
            assert!(!ModuleStopOutcome::SaveTimedOut.completed());
            assert!(!ModuleStopOutcome::SaveFailed {
                error: "disk full".into()
            }
            .completed());
            // The two that used to pass, and must not.
            assert!(!ModuleStopOutcome::JoinTimedOut.completed());
            assert!(!ModuleStopOutcome::JoinFailed {
                error: "task refused to join".into()
            }
            .completed());
        }

        // what this catches: THE clobber. When drain and save/join shared one enum, an
        // incomplete drain followed by a join timeout produced `JoinTimedOut`, the drain
        // result was discarded, and the module reported its state durable — a save taken
        // mid-turn, described as clean. Found in review by Astra, not by me.
        #[test]
        fn an_incomplete_drain_survives_a_failing_join_and_stays_non_durable() {
            let torn = ModuleStop {
                module: "cognition".into(),
                drain: DrainOutcome::Incomplete { in_flight: 3 },
                outcome: ModuleStopOutcome::JoinTimedOut,
                ms: 1,
            };
            // The join outcome alone would once have said "saved, just untidy".
            assert!(!torn.outcome.completed());
            // The whole module is NOT durable, because the save was taken over a turn.
            assert!(!torn.state_is_durable());
            assert_eq!(torn.drain, DrainOutcome::Incomplete { in_flight: 3 });
        }

        // what this catches: a drain that FAILED being reported as a measured zero.
        // "I could not find out how much was in flight" is not "nothing was in flight".
        #[test]
        fn a_failed_drain_is_unknown_not_a_measured_zero() {
            let unknown = DrainOutcome::Unknown {
                reason: "drain exceeded 2s".into(),
            };
            assert!(!unknown.is_quiet());
            assert_ne!(unknown, DrainOutcome::Incomplete { in_flight: 0 });
            assert_ne!(unknown, DrainOutcome::Drained);
        }

        // what this catches: a summary that hides which module lost state. "3 modules did
        // not save" without names sends the reader to a log that the exiting process may
        // not have flushed.
        #[test]
        fn the_summary_names_the_modules_that_did_not_save() {
            let receipt = ShutdownReceipt {
                tasks: DrainOutcome::Drained,
                modules: vec![
                    stop("logger", ModuleStopOutcome::Clean),
                    stop("cognition", ModuleStopOutcome::SaveTimedOut),
                ],
                total_ms: 42,
            };
            assert!(!receipt.state_is_durable());
            assert_eq!(receipt.unsaved().len(), 1);
            let summary = receipt.summary();
            assert!(summary.contains("cognition"), "got: {summary}");
            assert!(
                !summary.contains("logger"),
                "clean modules are noise here: {summary}"
            );
        }

        // what this catches: an EMPTY receipt reading as a successful stop. A core that
        // registered no modules, or a shutdown that never ran one, produces an empty list
        // — and "all state durable" over zero modules is true but says nothing. The
        // summary must show the count so a reader can see it was zero.
        #[test]
        fn an_empty_receipt_reports_the_count_it_actually_stopped() {
            let receipt = ShutdownReceipt {
                tasks: DrainOutcome::Drained,
                modules: Vec::new(),
                total_ms: 0,
            };
            assert!(receipt.state_is_durable());
            assert!(
                receipt.summary().contains("0 modules"),
                "an empty stop must say so: {}",
                receipt.summary()
            );
        }
    }

    /// Card 506a388c: the socket route (uu / IPC / MCP / desktop) dispatched
    /// typed and legacy commands directly and never consulted the interceptor
    /// chain, so a peer-addressed `ai/generate {aircPeer}` from the CLI ran
    /// LOCALLY on the wrong box with nothing saying the address was dropped.
    mod socket_route_runs_the_interceptor_chain {
        use super::super::*;
        use crate::runtime::command_interceptor::{CommandInterceptor, InterceptorOutcome};
        use async_trait::async_trait;

        struct Handles;
        #[async_trait]
        impl CommandInterceptor for Handles {
            fn name(&self) -> &'static str {
                "handles"
            }
            async fn try_route(
                &self,
                _command: &str,
                _params: &serde_json::Value,
                _caller: Option<&crate::routing::CallerIdentity>,
            ) -> Result<InterceptorOutcome, String> {
                Ok(InterceptorOutcome::Handled(CommandResult::Json(
                    serde_json::json!({"via": "interceptor"}),
                )))
            }
        }

        struct Refuses;
        #[async_trait]
        impl CommandInterceptor for Refuses {
            fn name(&self) -> &'static str {
                "refuses"
            }
            async fn try_route(
                &self,
                _command: &str,
                _params: &serde_json::Value,
                _caller: Option<&crate::routing::CallerIdentity>,
            ) -> Result<InterceptorOutcome, String> {
                Err("the address was bad".to_string())
            }
        }

        // what this catches: the socket route bypassing the chain — a Handled
        // interceptor must be the answer, before any typed or legacy dispatch.
        #[tokio::test]
        async fn a_handling_interceptor_answers_the_socket_route() {
            let runtime = Runtime::new();
            runtime
                .interceptor_chain()
                .write()
                .unwrap_or_else(|e| e.into_inner()) // unwrap_or_else: test lock
                .push(Arc::new(Handles));
            let out = runtime
                .route_command("nothing/registered", serde_json::json!({}), None)
                .await
                .expect("the chain answers even when no module handles the command");
            match out {
                Ok(CommandResult::Json(v)) => assert_eq!(v["via"], "interceptor"),
                other => panic!("expected the interceptor's result, got {other:?}"),
            }
        }

        // what this catches: an interceptor error laundered into a silent local
        // run — the exact shape of card 506a388c. It must be loud and name the
        // interceptor.
        #[tokio::test]
        async fn an_interceptor_error_is_loud_never_a_local_fallback() {
            let runtime = Runtime::new();
            runtime
                .interceptor_chain()
                .write()
                .unwrap_or_else(|e| e.into_inner()) // unwrap_or_else: test lock
                .push(Arc::new(Refuses));
            let out = runtime
                .route_command("nothing/registered", serde_json::json!({}), None)
                .await
                .expect("an error is an answer, not None");
            let err = out.expect_err("the refusal must surface");
            assert!(
                err.contains("refuses") && err.contains("the address was bad"),
                "{err}"
            );
        }
    }
    /// The mesh states the census reads, as the kernel sees them: through `MeshPresence`
    /// alone (airc's `AircDiscovery` is the real implementor, above the kernel).
    struct Mesh {
        can_host: bool,
        kind: &'static str,
    }

    impl MeshPresence for Mesh {
        fn can_host_personas(&self) -> bool {
            self.can_host
        }
        fn kind(&self) -> &'static str {
            self.kind
        }
    }

    fn healthy() -> Mesh {
        Mesh {
            can_host: true,
            kind: "healthy",
        }
    }

    fn degraded() -> Mesh {
        Mesh {
            can_host: false,
            kind: "degraded",
        }
    }

    /// FullCitizen + Healthy → must include persona hosting.
    #[test]
    fn full_citizen_healthy_requires_persona_hosting_modules() {
        let req = required_modules(&healthy(), BootMode::FullCitizen);
        assert!(req.contains(&"persona_instance_manager"));
        assert!(req.contains(&"persona-rag-inspect"));
        // The bare `inference` shell module was retired (89519a899); the engine
        // is now required via the coordinator.
        assert!(req.contains(&"inference-coordinator"));
        assert!(req.contains(&"airc"));
    }

    /// InferenceOnly → must NOT require persona hosting modules.
    /// This is the case Slice A broke (R1#1).
    #[test]
    fn inference_only_does_not_require_persona_hosting_modules() {
        let req = required_modules(&healthy(), BootMode::InferenceOnly);
        assert!(!req.contains(&"persona_instance_manager"));
        assert!(!req.contains(&"persona-rag-inspect"));
        // Core inference must still be there (coordinator carries the engine
        // since the bare `inference` shell module was retired in 89519a899).
        assert!(req.contains(&"inference-coordinator"));
        assert!(req.contains(&"embedding"));
        // airc module itself stays (it provides queue commands etc.
        // even when there's no live daemon to attach to)
        assert!(req.contains(&"airc"));
    }

    /// FailFast + Healthy → persona hosting required (strictest).
    #[test]
    fn fail_fast_healthy_requires_persona_hosting_modules() {
        let req = required_modules(&healthy(), BootMode::FailFast);
        assert!(req.contains(&"persona_instance_manager"));
        assert!(req.contains(&"persona-rag-inspect"));
    }

    /// FullCitizen + Degraded would normally bail before reaching
    /// verify_registration. If it somehow does reach, return the
    /// core-only set (no persona hosting modules required) so the
    /// resulting error message is coherent instead of complaining
    /// about modules that couldn't be registered against a
    /// degraded daemon.
    #[test]
    fn full_citizen_degraded_uses_core_only_set() {
        let req = required_modules(&degraded(), BootMode::FullCitizen);
        assert!(!req.contains(&"persona_instance_manager"));
        assert!(!req.contains(&"persona-rag-inspect"));
    }

    // ── ServiceProfile (the operational decomposition — slice 2) ────────
    //
    // A profile = the set of ServiceGroups a process hosts. These pin that the
    // all-groups profile reproduces the monolith EXACTLY (zero regression) and
    // that a slim profile yields only its groups' modules.

    /// `required_modules` (the monolith) == `required_modules_for_profile(.., all())`
    /// for every (discovery, mode) — the delegation is behavior-preserving.
    #[test]
    fn all_profile_reproduces_the_monolith_exactly() {
        let cells = [
            (healthy(), BootMode::FullCitizen),
            (healthy(), BootMode::InferenceOnly),
            (healthy(), BootMode::FailFast),
            (degraded(), BootMode::FullCitizen),
            (degraded(), BootMode::InferenceOnly),
        ];
        for (d, m) in cells {
            assert_eq!(
                required_modules(&d, m),
                required_modules_for_profile(&d, m, &ServiceProfile::all()),
                "all() profile must equal the monolith required set for ({}, {})",
                d.kind(),
                m.label()
            );
        }
    }

    /// A slim profile hosts ONLY its groups' modules (+ always-on RuntimeShell),
    /// and nothing from excluded groups. This is the operational decomposition:
    /// a minimal node boots the shell + bus and routes the rest over the grid.
    #[test]
    fn slim_profile_hosts_only_its_groups() {
        let profile = ServiceProfile::from_groups([ServiceGroup::GridTransport]);
        let req = required_modules_for_profile(&healthy(), BootMode::FullCitizen, &profile);

        // Hosts RuntimeShell (always) + GridTransport.
        for name in modules_in_group(ServiceGroup::RuntimeShell) {
            assert!(req.contains(&name), "shell module {name:?} must be hosted");
        }
        for name in modules_in_group(ServiceGroup::GridTransport) {
            assert!(req.contains(&name), "grid module {name:?} must be hosted");
        }
        // Hosts NOTHING from excluded groups (Inference, Cognition, Forge, Live, ResourceGov).
        for g in [
            ServiceGroup::Inference,
            ServiceGroup::Cognition,
            ServiceGroup::Forge,
            ServiceGroup::Live,
            ServiceGroup::ResourceGov,
        ] {
            for name in modules_in_group(g) {
                assert!(
                    !req.contains(&name),
                    "excluded {:?} module {name:?} must NOT be hosted",
                    g
                );
            }
        }
    }

    /// RuntimeShell is implicitly always hosted — even a profile that didn't name
    /// it gets it (a node must be addressable). Guards against composing a node
    /// that can't serve health/commands.
    #[test]
    fn runtime_shell_is_always_hosted() {
        let profile = ServiceProfile::from_groups([ServiceGroup::Forge]);
        assert!(profile.hosts(ServiceGroup::RuntimeShell));
        let req = required_modules_for_profile(&healthy(), BootMode::InferenceOnly, &profile);
        assert!(req.contains(&"health") && req.contains(&"data") && req.contains(&"events"));
    }

    /// Profile parsing: comma list round-trips; `all`/empty → every group;
    /// unknown group → actionable error. The "AWS template" surface.
    #[test]
    fn profile_parses_from_group_list() {
        use std::str::FromStr;
        let p = ServiceProfile::from_str("grid-transport,cognition").unwrap();
        assert!(p.hosts(ServiceGroup::GridTransport));
        assert!(p.hosts(ServiceGroup::Cognition));
        assert!(p.hosts(ServiceGroup::RuntimeShell)); // always
        assert!(!p.hosts(ServiceGroup::Live));

        assert_eq!(
            ServiceProfile::from_str("all").unwrap(),
            ServiceProfile::all()
        );
        assert_eq!(ServiceProfile::from_str("").unwrap(), ServiceProfile::all());

        let err = ServiceProfile::from_str("grid,bogus").unwrap_err();
        assert!(format!("{err}").contains("bogus"));

        // Group labels round-trip.
        for g in ServiceGroup::ALL {
            assert_eq!(g.label().parse::<ServiceGroup>().unwrap(), g);
        }
    }

    // ── ServiceGroup (the decomposition dimension) ──────────────────────
    //
    // These pin the concern-grouping that profiles/containers select on, and
    // that it's a clean PARTITION of MODULES derived from the single source.

    /// Every module belongs to exactly one group, and the groups partition
    /// `all_known_modules` (sum of group sizes == total). A profile selecting a
    /// set of groups thus yields a well-defined, gap-free module set.
    #[test]
    fn service_groups_partition_all_modules() {
        use ServiceGroup::*;
        let all_groups = [
            RuntimeShell,
            ResourceGov,
            Inference,
            Cognition,
            Forge,
            GridTransport,
            Live,
        ];
        let total: usize = all_groups.iter().map(|g| modules_in_group(*g).len()).sum();
        assert_eq!(
            total,
            all_known_modules().len(),
            "group sizes must sum to the total — every module grouped exactly once"
        );
        for name in all_known_modules() {
            assert!(
                group_of(name).is_some(),
                "module {name:?} has no ServiceGroup"
            );
        }
        for g in all_groups {
            assert!(
                !modules_in_group(g).is_empty(),
                "ServiceGroup {g:?} is empty"
            );
        }
    }

    /// The RuntimeShell — the minimal substrate every node hosts — is exactly
    /// the addressable-core set. Pinned so a future move can't silently drop a
    /// load-bearing module out of the shell (or smuggle a heavy one in).
    #[test]
    fn runtime_shell_is_the_minimal_addressable_set() {
        let mut shell = modules_in_group(ServiceGroup::RuntimeShell);
        shell.sort();
        let mut expected = vec![
            "auth", "data", "events", "health", "logger", "mcp", "runtime", "system",
        ];
        expected.sort();
        assert_eq!(
            shell, expected,
            "RuntimeShell must be exactly the addressable core"
        );
    }

    /// Live = Bevy render + LiveKit SFU — the CO-LOCATED group (must share the
    /// GPU/VM; the framebuffer→WebRTC transfer can't cross a process boundary).
    #[test]
    fn live_group_is_the_colocated_gpu_pair() {
        let mut live = modules_in_group(ServiceGroup::Live);
        live.sort();
        assert_eq!(
            live,
            vec!["avatar", "live"],
            "Live = the co-located Bevy+LiveKit pair"
        );
    }

    /// ServiceGroup (concern) and ModuleCategory (conditionality) are
    /// ORTHOGONAL: the persona-host conditionals live in Cognition (a concern)
    /// yet are PersonaHosting (a conditionality), and that gating is unchanged.
    #[test]
    fn group_and_category_are_orthogonal() {
        let cognition = modules_in_group(ServiceGroup::Cognition);
        assert!(
            cognition.contains(&"cognition"),
            "Cognition holds the always-on brain"
        );
        assert!(
            cognition.contains(&"persona_instance_manager"),
            "Cognition also holds the PersonaHosting-conditional modules"
        );
        // …still gated by ModuleCategory: absent under InferenceOnly even though
        // it's in the Cognition group (behavior preserved by this refactor).
        assert!(!required_modules(&healthy(), BootMode::InferenceOnly)
            .contains(&"persona_instance_manager"));
    }

    /// Drift catcher: every entry in `MODULES` MUST have a unique
    /// name. Duplicate registrations would make the conditional
    /// dispatch ambiguous (which category wins?).
    #[test]
    fn modules_list_has_unique_names() {
        let mut names: Vec<&str> = MODULES.iter().map(|spec| spec.name).collect();
        let original_len = names.len();
        names.sort();
        names.dedup();
        assert_eq!(
            names.len(),
            original_len,
            "MODULES contains duplicate module names — every name must appear at most once"
        );
    }

    /// `required_modules` returns the correct size for each
    /// `(discovery, mode)` cell.
    ///
    /// `Healthy + FullCitizen` should include both Core and
    /// PersonaHosting categories. Every other cell should include
    /// only Core. This is the structural test the convergent
    /// review demanded as a R2#3+R3#3 closer: the dispatch is
    /// derived from the single source of truth, so any change to
    /// the category of a row immediately changes what this test
    /// expects to see, not a parallel list that drifts.
    #[test]
    fn required_modules_size_matches_category_dispatch() {
        let core_count = MODULES
            .iter()
            .filter(|spec| matches!(spec.category, ModuleCategory::Core))
            .count();
        let hosting_count = MODULES
            .iter()
            .filter(|spec| matches!(spec.category, ModuleCategory::PersonaHosting))
            .count();
        let all_count = core_count + hosting_count;

        assert_eq!(
            required_modules(&healthy(), BootMode::FullCitizen).len(),
            all_count,
            "FullCitizen + Healthy must include both Core and PersonaHosting"
        );
        assert_eq!(
            required_modules(&healthy(), BootMode::InferenceOnly).len(),
            core_count,
            "InferenceOnly must include only Core"
        );
        assert_eq!(
            required_modules(&degraded(), BootMode::FullCitizen).len(),
            core_count,
            "Degraded must include only Core regardless of mode"
        );
        assert_eq!(
            required_modules(&degraded(), BootMode::InferenceOnly).len(),
            core_count,
        );
    }

    /// `all_known_modules()` must match the names projected from
    /// `MODULES`. Derivation correctness — if the projection ever
    /// diverges (e.g. someone adds a filter the const doesn't have),
    /// this catches it.
    #[test]
    fn all_known_modules_derives_from_modules() {
        let expected: Vec<&str> = MODULES.iter().map(|spec| spec.name).collect();
        assert_eq!(all_known_modules(), expected);
    }

    /// Every Core module must be present in EVERY required-modules
    /// result. Catches any future regression where someone
    /// accidentally categorizes a load-bearing infrastructure
    /// module as `PersonaHosting`.
    #[test]
    fn every_core_module_appears_in_every_required_set() {
        let cells = [
            (healthy(), BootMode::FullCitizen),
            (healthy(), BootMode::InferenceOnly),
            (healthy(), BootMode::FailFast),
            (degraded(), BootMode::FullCitizen),
            (degraded(), BootMode::InferenceOnly),
            (degraded(), BootMode::FailFast),
        ];
        for (discovery, mode) in cells {
            let req = required_modules(&discovery, mode);
            for spec in MODULES {
                let (name, cat) = (spec.name, spec.category);
                if matches!(cat, ModuleCategory::Core) {
                    assert!(
                        req.contains(&name),
                        "Core module {name:?} missing from required set for \
                         (discovery={}, mode={})",
                        discovery.kind(),
                        mode.label()
                    );
                }
            }
        }
    }

    /// PersonaHosting modules must appear ONLY when the dispatch
    /// is `(Healthy, requires_persona_hosting)`. The R1#1 BLOCK
    /// regression — having them in the required set under
    /// `--mode=inference-only` — is structurally impossible to
    /// reintroduce because this test pins both polarities.
    #[test]
    fn persona_hosting_modules_appear_only_when_dispatched() {
        let host_modules: Vec<&str> = MODULES
            .iter()
            .filter(|spec| matches!(spec.category, ModuleCategory::PersonaHosting))
            .map(|spec| spec.name)
            .collect();

        for name in &host_modules {
            assert!(required_modules(&healthy(), BootMode::FullCitizen).contains(name));
            assert!(required_modules(&healthy(), BootMode::FailFast).contains(name));
        }

        let must_not_contain = [
            (healthy(), BootMode::InferenceOnly),
            (degraded(), BootMode::FullCitizen),
            (degraded(), BootMode::InferenceOnly),
            (degraded(), BootMode::FailFast),
        ];
        for (discovery, mode) in must_not_contain {
            let req = required_modules(&discovery, mode);
            for name in &host_modules {
                assert!(
                    !req.contains(name),
                    "PersonaHosting module {name:?} unexpectedly required for \
                     (discovery={}, mode={}) — this would reintroduce R1#1",
                    discovery.kind(),
                    mode.label()
                );
            }
        }
    }
}
