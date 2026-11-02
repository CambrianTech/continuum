#![cfg(test)]
    use super::*;
    use super::test_support::*;
    use crate::runtime::{
        CommandExecutor, CommandResult, ModuleConfig, ModuleContext, ModulePriority,
        ModuleRegistry, ServiceModule,
    };
    use serde_json::json;
    use std::any::Any;
    use std::sync::Arc;

    // what this catches: a live persona's hands (built via for_persona) EXPOSE the core
    // executor, so apply_act can fire a long-running command in the background; a bare
    // Connection-built executor (harness) does NOT — it runs every command synchronously.
    // This is the reachability seam that lets a persona send a sentinel/compile away.
    #[test]
    fn for_persona_hands_expose_core_executor_for_dispatch() {
        // Registry audit (mind cut): stays in the mind: it reads only whether the hands expose an
        // executor; it dispatches nothing from the auto-seeded surface.
        let exec = Arc::new(CommandExecutor::new(Arc::new(ModuleRegistry::new())));
        let hands = CommandToolExecutor::for_persona(exec, uuid::Uuid::new_v4());
        assert!(
            hands.command_executor().is_some(),
            "live persona hands expose the core executor for fire-and-poll dispatch"
        );
        let bare = CommandToolExecutor::new(hands.conn.clone());
        assert!(
            bare.command_executor().is_none(),
            "harness hands (bare Connection) run every command synchronously"
        );
    }

    /// Minimal module that echoes its params back under `test/echo`.
    struct EchoModule;

    #[async_trait]
    impl ServiceModule for EchoModule {
        fn config(&self) -> ModuleConfig {
            ModuleConfig {
                name: "echo",
                priority: ModulePriority::Normal,
                command_prefixes: &["test/"],
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
            command: &str,
            params: Value,
        ) -> Result<CommandResult, String> {
            match command {
                "test/echo" => Ok(CommandResult::Json(params)),
                other => Err(format!("unknown command: {other}")),
            }
        }
        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    // Registry audit (mind cut): stays in the mind: the withheld-verb text is the tool
    // dialect's, not a registry lookup (unknown-command and bad-args feedback, which read the
    // registry, run in core's test_env/mind_integration/registry/tool_executor.rs).
    // A compact-menu omission must not erase actionable failures or falsely
    // revoke a reviewer's discoverable command. Own-work guidance remains.
    #[test]
    fn a_verb_withheld_from_her_hands_preserves_failure_and_purpose() {
        let out = persona_tool_error(
            "work_review",
            "an explicit activity room is required".to_string(),
        );
        assert!(!out.contains("failed:"), "the status line frames the refusal once, never a second frame: {out}");
        assert!(out.contains("`work/get`") && out.contains("`work/submit`"), "{out}");
        assert!(out.contains("activity room is required"), "{out}");
        let out = persona_tool_error(
            "work/review",
            "[invalid] card d33e928a is not a review card (it reviews nothing) — pass card_id if you are reviewing out of band".to_string(),
        );
        assert!(out.contains("not a review card") && out.contains("`work/get`"), "{out}");
        let transport = persona_tool_error("work/review", "connection closed before response".into());
        assert!(transport.contains("connection closed before response"), "{transport}");
        assert!(!transport.contains("Nothing you type") && !transport.contains("did not run"), "{transport}");
        assert!(persona_tool_error("work/submission", "x".into()).contains("`work/submit`"));
        assert!(persona_tool_error("code/git/apply", "x".into()).contains("`code/edit`"));
    }

    #[test]
    fn cut_act_sentinel_says_nothing_ran_and_how_to_land_the_act() {
        // what this catches (Kimi, 2026-09-25): a cut act re-sampled with no word of the
        // cut replays the same oversized payload and re-derives work that landed. The
        // teacher must say the act did not land, point at the record, and name the way
        // out — never the unknown-tool wording (she called nothing).
        let raw = "no Rust module handles command: 'tools/<cut at the output limit>'".to_string();
        let out = persona_tool_error(CUT_AT_LIMIT_SENTINEL, raw);
        assert!(out.contains("NOTHING ran"), "{out}");
        assert!(out.contains("working memory"), "{out}");
        assert!(out.contains("smaller payload"), "{out}");
        assert!(!out.contains("not a tool you can call"), "{out}");
    }

    // what this catches (2026-10-06): a thought cut at the limit is told that what she was
    // weighing was NOT run for her, where her thinking is, and that the decision is hers;
    // never the cut-act advice about payload size (she was not composing a payload).
    #[test]
    fn thought_cut_sentinel_says_nothing_was_run_for_her_and_the_decision_is_hers() {
        let raw = "no Rust module handles command: 'tools/<thought cut at the output limit>'".to_string();
        let out = persona_tool_error(THOUGHT_CUT_SENTINEL, raw);
        assert!(out.contains("NOT run for you"), "{out}");
        assert!(out.contains("working"), "{out}");
        assert!(out.contains("PASS"), "{out}");
        assert!(!out.contains("payload"), "{out}");
        assert!(!out.contains("not a tool you can call"), "{out}");
    }

    // what this catches: the think-only sentinel gets its OWN teacher sentence — it
    // must name what happened (the generation ended inside the reasoning channel) and
    // the three commitment affordances (tool call / plain answer / PASS). The
    // "not a tool you can call" wording would be false here: she called nothing.
    #[test]
    fn think_only_sentinel_teaches_commitment_not_unknown_tool() {
        let raw = "no Rust module handles command: 'tools/<think-only turn>'".to_string();
        let out = persona_tool_error(THINK_ONLY_SENTINEL, raw);
        assert!(out.contains("reasoning"), "{out}");
        assert!(out.contains("PASS"), "{out}");
        assert!(
            !out.contains("not a tool you can call"),
            "she called nothing — the unknown-tool wording is false here: {out}"
        );
    }

    // what this catches: the manual's placeholder sent as a value — `<replace-with-string>`,
    // `<string>`, a sixty-four-zero hash, the nil uuid — named by FIELD and never dispatched;
    // a real call with a real path, id and hash walks through untouched.
    #[test]
    fn the_manuals_blank_is_named_by_field_and_a_real_call_is_not() {
        use serde_json::json;
        let blanks = manual_blanks_in(&json!({
            "file_path": "<replace-with-string>",
            "card_id": "<string>",
            "artifact": {"hash": "0".repeat(64), "size_bytes": 0},
            "review_id": "00000000-0000-0000-0000-000000000000",
            "nested": [{"ok": "src/main.rs"}, {"bad": "<any>"}],
        }));
        assert_eq!(blanks, ["artifact.hash", "card_id", "file_path", "nested.1.bad", "review_id"]);
        assert!(manual_blanks_in(&json!({
            "file_path": "core/continuum-core/src/lib.rs",
            "card_id": "1f58fc16",
            "artifact": {"hash": "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08", "size_bytes": 4},
            "text": "a <b> tag inside prose is not a blank",
            "cmd": "echo '<html>'",
        })).is_empty());
    }

    // what this catches: a refusal that already carries a real, persona-readable
    // reason (e.g. an authorization denial) passes through unchanged — we don't
    // bury a genuine cause under boilerplate. [[fallbacks-are-illegal-fail-loud]]
    #[test]
    fn other_refusals_pass_through_unchanged() {
        for raw in [
            "data/delete: refused — requires Owner trust",
            "data/delete: [invalid] domain evidence contains [arguments] but is not a schema failure",
        ] {
            let out = persona_tool_error("data/delete", raw.to_string());
            assert_eq!(out, raw);
        }
    }

    fn executor_with_echo() -> CommandToolExecutor {
        // Registry audit (mind cut): stays in the mind: these tests dispatch only `test/echo`,
        // declared by this module's own `EchoModule`, never the auto-seeded surface.
        let registry = Arc::new(ModuleRegistry::new());
        registry.register(Arc::new(EchoModule));
        exec_over(registry, Uuid::new_v4())
    }

    // what this catches: THE thing that turns "talks" into "acts" — a native tool
    // call routes to the real command and the command's result comes back,
    // correlated by tool_use_id, no error. If this regresses, the persona is back
    // to a chatbot that can't touch the world.
    #[tokio::test]
    async fn routes_native_tool_call_to_the_command() {
        let exec = executor_with_echo();
        let calls = vec![NativeToolCall {
            id: "t1".to_string(),
            name: "test/echo".to_string(),
            input: json!({ "path": "deploy.md" }),
        }];
        let out = exec
            .execute_native_batch(&calls, &ctx())
            .await
            .unwrap();
        assert_eq!(out.results.len(), 1);
        assert_eq!(out.results[0].tool_use_id, "t1");
        assert!(out.results[0].is_error.is_none(), "successful tool call");
        assert!(
            out.results[0].content.contains("deploy.md"),
            "command result fed back: {}",
            out.results[0].content
        );
    }

    // what this catches: the underscore→slash mapping for models that emit
    // `test_echo` instead of `test/echo`.
    #[tokio::test]
    async fn maps_underscore_tool_name_to_slash_command() {
        let exec = executor_with_echo();
        let calls = vec![NativeToolCall {
            id: "t1".to_string(),
            name: "test_echo".to_string(),
            input: json!({ "ok": true }),
        }];
        let out = exec
            .execute_native_batch(&calls, &ctx())
            .await
            .unwrap();
        assert!(
            out.results[0].is_error.is_none(),
            "test_echo → test/echo routed"
        );
    }

    // what this catches: a failed tool call is fed back as an ERROR RESULT (so the
    // model can recover), NOT a batch-level failure that aborts the turn.
    #[tokio::test]
    async fn failed_call_becomes_error_result_not_batch_failure() {
        let exec = executor_with_echo();
        let calls = vec![NativeToolCall {
            id: "t1".to_string(),
            name: "test/nonexistent".to_string(),
            input: json!({}),
        }];
        let out = exec
            .execute_native_batch(&calls, &ctx())
            .await
            .expect("batch itself succeeds");
        assert_eq!(
            out.results[0].is_error,
            Some(true),
            "per-call error, batch ok"
        );
        // the model sees the SUBSTRATE's reason, not the client-wrapper prefix —
        // so it recovers on the real message (regression-pins the Err arm that
        // unwraps ClientError::Refused.reason instead of Display).
        let content = &out.results[0].content;
        assert!(
            content.contains("nonexistent"),
            "surfaces the real reason: {content}"
        );
        // Line 1 is the one frame (answer.rs); the client wrapper's own text never reaches her.
        assert!(content.starts_with("refused test/nonexistent()"), "one status line: {content}");
        assert_eq!(content.matches("refused").count(), 1, "no client-wrapper prefix leaks to the model: {content}");
    }

    /// Concurrency + load proofs. Gated behind `stress-tests` per the test
    /// doctrine (timing/multi-thread tests are compile-time gated, not `#[ignore]`).
    /// Run them: `cargo test -p continuum-core --features stress-tests \
    ///   cognition::tool_executor::command_executor::tests::stress -- --nocapture`
    #[cfg(feature = "stress-tests")]
    mod stress {
        use super::*;
        use crate::logging::timing::PerformanceStats;
        use std::sync::Arc;
        use std::time::{Duration, Instant};
        use tokio::sync::Barrier;

        /// A command that PARKS at a barrier before returning. A barrier of width
        /// W releases only when W calls are simultaneously in-flight — so the
        /// batch completes IFF dispatch is concurrent. If anything serializes the
        /// calls (a shared lock, a FIFO queue), the W-th call never arrives, the
        /// barrier never trips, and the surrounding timeout fails the test. A
        /// deterministic concurrency proof, not a flaky wall-clock threshold.
        struct BarrierModule {
            gate: Arc<Barrier>,
        }

        #[async_trait]
        impl ServiceModule for BarrierModule {
            fn config(&self) -> ModuleConfig {
                ModuleConfig {
                    name: "barrier",
                    priority: ModulePriority::Normal,
                    command_prefixes: &["load/"],
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
                params: Value,
            ) -> Result<CommandResult, String> {
                // Every concurrent caller must reach here before any proceeds.
                self.gate.wait().await;
                Ok(CommandResult::Json(params))
            }
            fn as_any(&self) -> &dyn Any {
                self
            }
        }

        fn load_calls(n: usize) -> Vec<NativeToolCall> {
            (0..n)
                .map(|i| NativeToolCall {
                    id: format!("t{i}"),
                    name: "load/work".to_string(),
                    input: json!({ "i": i }),
                })
                .collect()
        }

        /// One persona, many citizens — a substrate executor over an echo module.
        fn echo_executor() -> Arc<CommandExecutor> {
            // Registry audit (mind cut): stays in the mind: dispatches only the `EchoModule` declared here.
            let registry = Arc::new(ModuleRegistry::new());
            registry.register(Arc::new(EchoModule));
            Arc::new(CommandExecutor::new(registry))
        }

        fn persona_over(executor: Arc<CommandExecutor>) -> CommandToolExecutor {
            let transport = InProcessTransport::new(
                executor,
                Some(CallerIdentity::airc(crate::identity::PeerId::from_uuid(
                    Uuid::new_v4(),
                ))),
            );
            CommandToolExecutor::new(Connection::new(transport))
        }

        // what this catches: a single persona's tool BATCH dispatches concurrently
        // (join_all), not one-at-a-time. 50 calls all park at a Barrier(50); the
        // batch returns only if all 50 are in-flight at once. A regression to a
        // serial `for` loop would deadlock — the timeout converts that to a clean
        // failure instead of a hang.
        #[tokio::test(flavor = "multi_thread")]
        async fn intra_batch_dispatches_concurrently() {
            const N: usize = 50;
            // Registry audit (mind cut): stays in the mind: dispatches only the `BarrierModule` declared here.
            let registry = Arc::new(ModuleRegistry::new());
            registry.register(Arc::new(BarrierModule {
                gate: Arc::new(Barrier::new(N)),
            }));
            let exec = exec_over(registry, Uuid::new_v4());

            let out = tokio::time::timeout(
                Duration::from_secs(5),
                exec.execute_native_batch(&load_calls(N), &ctx()),
            )
            .await
            .expect("batch must finish — a timeout means the batch serialized")
            .expect("batch ok");

            assert_eq!(out.results.len(), N, "every concurrent call returned");
            assert!(out.results.iter().all(|r| r.is_error.is_none()), "all ok");
        }

        // what this catches: THE failure mode Joel named — "14 personas all locking
        // each other." 50 separate personas (each its own Connection + identity)
        // share ONE substrate executor and fire a tool call simultaneously. All 50
        // park at a Barrier(50); the join completes only if no persona blocks
        // another. A global lock / FIFO in the dispatch path would stop the 50th
        // from entering → deadlock → timeout failure.
        #[tokio::test(flavor = "multi_thread")]
        async fn personas_do_not_serialize_on_each_other() {
            const PERSONAS: usize = 50;
            // Registry audit (mind cut): stays in the mind: dispatches only the `BarrierModule` declared here.
            let registry = Arc::new(ModuleRegistry::new());
            registry.register(Arc::new(BarrierModule {
                gate: Arc::new(Barrier::new(PERSONAS)),
            }));
            let executor = Arc::new(CommandExecutor::new(registry));

            let handles: Vec<_> = (0..PERSONAS)
                .map(|_| {
                    let exec = persona_over(executor.clone());
                    tokio::spawn(async move {
                        exec.execute_native_batch(&load_calls(1), &ctx())
                            .await
                    })
                })
                .collect();

            let outs = tokio::time::timeout(Duration::from_secs(5), join_all(handles))
                .await
                .expect("all personas must finish — a timeout means they serialized");

            assert_eq!(outs.len(), PERSONAS);
            for out in outs {
                let out = out.expect("join").expect("persona batch ok");
                assert_eq!(out.results.len(), 1);
                assert!(out.results[0].is_error.is_none());
            }
        }

        // The actual LOAD TEST (Joel: "see where it starts to degrade and then
        // iterate"). Ramps the persona fleet against ONE shared substrate executor,
        // each persona firing a 50-call tool batch (genuine parallelism via
        // tokio::spawn on the multi-thread runtime), and prints the throughput
        // curve so the knee is visible. Latency per batch is recorded through our
        // own `PerformanceStats` (atomic avg/min/max) — not eprintln. (Per-call
        // dispatch is intentionally NOT separately probed on the hot path; the
        // batch latency here is the measurement.)
        //
        // This is exploratory, not a brittle perf-threshold gate: it asserts only
        // correctness (every op completes). The printed curve is the artifact you
        // read to find where dispatch degrades, then iterate.
        #[tokio::test(flavor = "multi_thread")]
        async fn load_scaling_curve() {
            const CALLS_PER_PERSONA: usize = 50;
            let tiers = [1usize, 10, 50, 100, 200, 400, 800];
            let executor = echo_executor();

            println!(
                "\n  cores={}  calls/persona={CALLS_PER_PERSONA}",
                num_cpus::get()
            );
            println!(
                "  {:>8} │ {:>7} │ {:>8} │ {:>10} │ {:>12} │ {:>12}",
                "personas", "ops", "wall_ms", "ops/sec", "batch_avg_us", "batch_max_us"
            );

            for &p in &tiers {
                let stats = Arc::new(PerformanceStats::new());
                let start = Instant::now();
                let handles: Vec<_> = (0..p)
                    .map(|_| {
                        let exec = persona_over(executor.clone());
                        let stats = Arc::clone(&stats);
                        tokio::spawn(async move {
                            let calls = load_calls(CALLS_PER_PERSONA);
                            let c = ctx();
                            let t0 = Instant::now();
                            let out = exec
                                .execute_native_batch(&calls, &c)
                                .await
                                .expect("batch ok");
                            stats.record(t0.elapsed().as_micros() as u64);
                            out.results.len()
                        })
                    })
                    .collect();

                let counts = join_all(handles).await;
                let wall = start.elapsed();
                let ops: usize = counts.into_iter().map(|r| r.expect("join")).sum();
                assert_eq!(ops, p * CALLS_PER_PERSONA, "every op completed at p={p}");

                let snap = stats.snapshot();
                let ops_per_sec = ops as f64 / wall.as_secs_f64();
                println!(
                    "  {:>8} │ {:>7} │ {:>8.1} │ {:>10.0} │ {:>12} │ {:>12}",
                    p,
                    ops,
                    wall.as_secs_f64() * 1000.0,
                    ops_per_sec,
                    snap.avg_duration_us,
                    snap.max_duration_us
                );
            }
        }
    }
