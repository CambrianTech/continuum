#![cfg(test)]
//! Tests that exercise a kernel module TOGETHER with a module above it. They sat inside
//! the kernel's own files; the kernel crate has nothing above it to name, so they live in
//! the crate that has both. Each asserts exactly what it asserted before the move.

/// The GPU backend base guard: a tree-wide scan of the crate's own source.
mod gpu_base {
    /// What this catches: **a new backend bypassing this module entirely.**
    ///
    /// The split above is only load-bearing if backends actually go through it. A
    /// Vulkan / MLX / ROCm author who writes `impl GpuMonitor for VulkanMonitor`
    /// gets a compiling, plausible-looking adapter that re-owns retention and the
    /// unknown state — and is free to re-invent the `unwrap_or(total)` lie a third
    /// time. Prose in a module header cannot stop that; this can.
    ///
    /// The rule: exactly TWO `impl GpuMonitor` sites exist tree-wide — this
    /// module's blanket impl for `MonitoredGpu<P>`, and `MockMonitor` (a test
    /// double, scripted by definition, which is why it is exempt and named here
    /// rather than pattern-excused). Everything else implements `GpuDeviceProbe`.
    ///
    /// Comment-stripped before matching so a doc mention of `impl GpuMonitor`
    /// cannot satisfy OR trip the check — the same predicate discipline as the
    /// module-wiring audit in `registry.rs`.
    #[test]
    fn every_gpu_backend_goes_through_the_shared_base() {
        /// The only impls allowed to exist, each with the reason it is not a
        /// backend. Adding a row is a DESIGN decision, not a formality.
        const SANCTIONED: &[(&str, &str)] = &[
            (
                "gpu/device_probe.rs",
                "the blanket impl every backend inherits — this IS the base",
            ),
            (
                "gpu/monitor.rs",
                "MockMonitor: a scripted test double with no device behind it",
            ),
        ];

        // Split so this file's own needle is not an impl site (the guard reads this file too).
        const NEEDLE: &str = concat!("impl GpuMonitor", " for");

        // Comments are stripped (`source_tree`) so prose can neither fake nor trip the check.
        let offenders: Vec<String> = crate::test_env::source_tree::crate_sources()
            .into_iter()
            .filter(|f| f.code().contains(NEEDLE))
            .filter(|f| !SANCTIONED.iter().any(|(sanctioned, _)| f.rel == *sanctioned))
            .map(|f| f.rel)
            .collect();

        assert!(
            offenders.is_empty(),
            "these files implement GpuMonitor directly, bypassing the shared base: {offenders:?}\n\
             A GPU backend implements `GpuDeviceProbe` (how to ask THIS device) and wraps in \
             `MonitoredGpu` — it does NOT own retention, the unknown state, the tick, or pressure. \
             That ownership split is what makes the `unwrap_or(total)` class unrepresentable; \
             see this module's header. If a new impl is genuinely not a backend, add it to \
             SANCTIONED with its reason."
        );
    }
}

/// The holder table against the consumer ids the upper modules register under.
mod memory_holders {
    use crate::resources::holders::{standard_memory_holders, HolderStatus};

    // what this catches: the table drifting out of sync with the constants the code
    // actually registers under. A rename of SERVING_CONSUMER_ID that left this table
    // saying "serving" would silently make the reconciliation attribute nothing —
    // prose cannot be allowed to fake wiring (the #344 property).
    #[test]
    fn declared_holders_use_the_ids_the_code_registers_under() {
        let declared: Vec<&str> = standard_memory_holders()
            .iter()
            .filter(|h| h.status == HolderStatus::Declared)
            .map(|h| h.id)
            .collect();
        assert!(declared.contains(&crate::modules::serving_consumer::SERVING_CONSUMER_ID));
        assert!(declared.contains(&continuum_live::consumers::perception_consumer::PERCEPTION_CONSUMER_ID));
        assert!(declared.contains(&crate::cognition::measurement_staging::CONSUMER_ID));
        // `embed` has a constant but is deliberately NOT declared (#225) — if someone
        // registers it as a consumer, this assertion is the reminder to move its row.
        assert!(
            !declared.contains(&crate::inference::llamacpp_adapter::EMBED_LANE_CONSUMER_ID),
            "embed now registers as a consumer — move its row from Undeclared to Declared"
        );
    }
}

/// Dispatch-orphan audit against the real work/room/benchmark-grade modules.
mod module_registry {
    use crate::runtime::ModuleRegistry;

    #[test]
    fn dispatch_orphans_reads_descriptors_and_clears_on_module_registration() {
        let registry = ModuleRegistry::new();
        let before = registry.dispatch_orphans();
        let work_names = [
            "work/list",
            "work/get",
            "work/claim",
            "work/create",
            "work/release",
            "work/state",
            "work/heartbeat",
        ];
        for name in work_names {
            assert!(
                before.contains(&name),
                "{name} must be an orphan before its module registers"
            );
        }

        registry.register(std::sync::Arc::new(crate::modules::work_seams::work_module(
            crate::persona::PersonaAircRuntimeRegistry::new(),
            std::sync::Arc::new(crate::memory::DeterministicEmbeddingProvider),
        )));
        registry.register(std::sync::Arc::new(crate::modules::room::RoomModule::new(
            crate::persona::PersonaAircRuntimeRegistry::new(),
        )));
        // Event-driven SWE grade-on-done: subscribes to work.card.state_changed (emitted
        // by work/state) and grades a finished bench SWE card's workspace against the
        // held-out oracle. No commands, no tick — pure event subscriber.
        registry.register(std::sync::Arc::new(
            crate::modules::benchmark_grade::BenchmarkGradeModule::new(
                crate::persona::PersonaAircRuntimeRegistry::new(),
            ),
        ));
        let after = registry.dispatch_orphans();
        for name in work_names {
            assert!(
                !after.contains(&name),
                "{name} still orphaned after WorkModule registered — a verb is \
                 missing from commands() (the #309 two-line gap)"
            );
        }
    }
}

/// The executor against the real ping module, grid policy and airc interceptor.
mod command_executor {
    use crate::runtime::{CommandExecutor, CommandResult, ModuleRegistry};
    use crate::inference::airc_remote::interceptor::AircInterceptor;
    use async_trait::async_trait;
    use serde_json::Value;
    use std::sync::Arc;

    // what this catches: the TYPED PATH end-to-end through the REAL executor —
    // `ping` (migrated to a DynCommand object via ActionCommand) dispatches all the
    // way through CommandExecutor::execute, which consults the registry's object map
    // (step 2) BEFORE any prefix routing, and returns the bare PingResult. This is
    // the integration proof that the per-module match arm is gone for ping and the
    // self-routing object map serves it. Pure headless Rust — no Node, no socket,
    // no npm start.
    #[tokio::test]
    async fn ping_dispatches_through_executor_via_typed_object_path() {
        let registry = Arc::new(ModuleRegistry::new());
        registry.register(Arc::new(crate::modules::health::HealthModule::new()));
        let executor = CommandExecutor::new(registry);

        let result = executor
            .execute("ping", serde_json::json!({}))
            .await
            .expect("ping dispatches through the executor");
        match result {
            CommandResult::Json(v) => {
                assert_eq!(v["ok"], true, "ping returned the bare PingResult");
                assert!(
                    v.get("success").is_none(),
                    "Bare wire — no envelope wrapping on the typed path"
                );
            }
            other => panic!("expected Json, got {other:?}"),
        }
    }

    // what this catches: NO ESCALATION THROUGH COMPOSITION, via a REAL composing
    // handler (not just the gate). `Composer` is an ActionCommand whose `run`
    // composes a sub-command with `ctx.caller.clone()` — exactly the pattern a
    // dep-holding command uses. Invoked as the local owner the sub-call passes the
    // gate; invoked as an airc/Provisional caller the propagated identity is gated,
    // so it CANNOT reach the Owner-only `data/delete`. This pins that identity flows
    // through composition (and, by the same mechanism, across the grid), never
    // escalating — the guarantee the COMMAND-ORGANIZATION doc claims.
    #[tokio::test]
    async fn composing_handler_propagates_ctx_caller_no_escalation() {
        use crate::routing::{CallerIdentity};
        use crate::modules::grid::{GridTrustAuthPolicy};
        use crate::sdk_codegen::{ActionCommand, CommandError, Ctx};

        #[derive(
            Default, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema,
        )]
        struct NoParams {}
        #[derive(serde::Serialize, serde::Deserialize, ts_rs::TS)]
        struct Out {
            forbidden: bool,
        }

        // A command that COMPOSES another, propagating its own caller (ctx.caller).
        struct Composer {
            exec: Arc<CommandExecutor>,
        }
        #[async_trait]
        impl ActionCommand for Composer {
            const NAME: &'static str = "test/composer";
            type Params = NoParams;
            type Output = Out;
            async fn run(&self, ctx: &Ctx, _p: NoParams) -> Result<Out, CommandError> {
                let r = self
                    .exec
                    .execute_with_caller(
                        "data/delete",
                        Value::Object(Default::default()),
                        ctx.caller.clone(),
                    )
                    .await;
                Ok(Out {
                    forbidden: r
                        .as_ref()
                        .err()
                        .map(|e| e.contains("forbidden"))
                        .unwrap_or(false),
                })
            }
        }

        let registry = Arc::new(ModuleRegistry::new());
        let exec = Arc::new(
            CommandExecutor::new(registry).with_policy(Arc::new(GridTrustAuthPolicy::new())),
        );
        let composer = Composer { exec: exec.clone() };

        // Composed as the local owner (ctx.caller None) → sub-call NOT gate-forbidden.
        let owner = composer.run(&Ctx::default(), NoParams {}).await.unwrap();
        assert!(
            !owner.forbidden,
            "owner composing data/delete is not forbidden"
        );

        // Composed as an airc/Provisional caller → identity propagated → FORBIDDEN.
        let airc_ctx = Ctx {
            caller: Some(CallerIdentity::airc(crate::identity::PeerId::new())),
            ..Default::default()
        };
        let escalated = composer.run(&airc_ctx, NoParams {}).await.unwrap();
        assert!(
            escalated.forbidden,
            "airc composing data/delete must be forbidden — handler propagated ctx.caller, no escalation"
        );
    }

    #[tokio::test]
    async fn airc_interceptor_declines_when_no_airc_target_params() {
        // The airc interceptor at the head of the chain must NOT block
        // existing local-Rust commands that don't carry airc routing
        // params. This is the back-compat guarantee that lets the airc
        // interceptor be safely installed at init_executor.
        //
        // Without a registered Rust module for "test/cmd", the executor
        // walks the interceptor chain (airc Declines because no
        // aircPeer in params), then the registry (no match), then
        // returns CommandNotFound per [[no-fallbacks-ever]] (task #219).
        // The test is asserting the airc interceptor did NOT
        // short-circuit — the failure source MUST be the registry miss,
        // NOT the airc interceptor.
        let registry = Arc::new(ModuleRegistry::new());
        let executor =
            CommandExecutor::new(registry).with_interceptor(Arc::new(AircInterceptor::new()));

        let result = executor
            .execute("test/cmd", serde_json::json!({ "ordinaryParam": "value" }))
            .await;

        // Failure must be the CommandNotFound shape (no Rust module),
        // not the airc interceptor short-circuiting. The substring
        // "no Rust module handles command" is the contract we depend on
        // — if the airc interceptor had wrongly intercepted, the error
        // would mention "airc" instead.
        let err = result.expect_err("command has no Rust handler; expect typed error");
        assert!(
            err.contains("no Rust module handles command"),
            "error must come from registry miss (the no-fallbacks surface), \
             not from airc interceptor. err: {err}"
        );
        assert!(
            !err.contains("aircPeer") && !err.contains("airc interceptor"),
            "error must NOT mention airc routing — proves the airc \
             interceptor declined cleanly. err: {err}"
        );
    }

    #[tokio::test]
    async fn airc_interceptor_fails_loud_when_airc_peer_targeted() {
        // The airc interceptor MUST short-circuit with a loud error when
        // a caller passes aircPeer, even before the transport is wired.
        // Silent fall-through would hide the missing transport from the
        // caller, who would then see local-dispatch results (or worse,
        // success on the wrong machine) and not know airc wasn't used.
        let registry = Arc::new(ModuleRegistry::new());
        let executor =
            CommandExecutor::new(registry).with_interceptor(Arc::new(AircInterceptor::new()));

        let err = executor
            .execute(
                "chat/send",
                serde_json::json!({ "aircPeer": "peer-id", "content": "hello" }),
            )
            .await
            .expect_err(
                "explicit aircPeer must error until transport is wired — \
                 not silently fall through to local",
            );
        assert!(
            err.contains("airc"),
            "error must identify airc as the unresolved transport: {err}"
        );
        assert!(
            err.contains("peer-id"),
            "error must echo the target so the caller can correlate logs: {err}"
        );
    }
}

/// The socket route against the real airc interceptor.
mod runtime_interceptors {
    use crate::runtime::Runtime;
    use std::sync::Arc;

    // what this catches: the card itself — the real airc interceptor on the
    // socket route refuses a peer-addressed generate by NAME instead of
    // letting it run locally.
    #[tokio::test]
    async fn a_peer_addressed_generate_on_the_socket_route_never_runs_locally() {
        let runtime = Runtime::new();
        runtime
            .interceptor_chain()
            .write()
            .unwrap_or_else(|e| e.into_inner()) // unwrap_or_else: test lock
            .push(Arc::new(crate::inference::airc_remote::interceptor::AircInterceptor::new()));
        let out = runtime
            .route_command(
                "ai/generate",
                serde_json::json!({"aircPeer": "not-a-uuid", "prompt": "hi"}),
                None,
            )
            .await
            .expect("the airc interceptor answers a peer-addressed call");
        let err = out.expect_err("a peer-addressed generate must not run locally");
        assert!(
            err.contains("airc") && (err.contains("attached") || err.contains("aircPeer")),
            "the refusal names the seam: {err}"
        );
    }
}

/// Probe tracing installed with the real logger queue sink.
mod tracing_install {
    use crate::routing::probe_file_sink::ProbeFileSinkError;
    use crate::routing::tracing_init::{install_probe_tracing, ProbeTracingConfig};
    use std::collections::HashSet;
    use std::path::PathBuf;

    /// `install_probe_tracing` accepts a typed config and is
    /// parallel-safe in tests — no env-var mutation, no shared
    /// global state beyond `tracing`'s own default subscriber
    /// (which is exactly the thing we're testing the idempotency
    /// of).
    #[test]
    fn install_is_idempotent_with_no_disk_capture() {
        let config = ProbeTracingConfig {
            probe_dir: None,
            probe_classes: HashSet::new(),
            default_filter: "warn".to_string(),
            log_dir: None,
        };
        let first = install_probe_tracing(config.clone(), crate::modules::logger_clog_sink::LoggerQueueSink);
        let second = install_probe_tracing(config, crate::modules::logger_clog_sink::LoggerQueueSink);
        assert!(first.is_ok(), "first install must succeed");
        assert!(second.is_ok(), "second install must no-op cleanly");
        assert!(first.unwrap().probe_log_path.is_none());
        assert!(second.unwrap().probe_log_path.is_none());
    }

    /// Typed-error contract: an unwritable `probe_dir` path must
    /// surface `ProbeFileSinkError::OpenFailed` per
    /// `[[no-fallbacks-ever]]`. Operators must see the
    /// configuration problem; substrate refuses to silently drop
    /// probes.
    ///
    /// Parallel-safe because the bad path is passed via typed
    /// config — no env-var mutation, no racing on
    /// `CONTINUUM_PROBE_DIR`.
    #[test]
    fn install_surfaces_open_failed_for_unwritable_path() {
        let config = ProbeTracingConfig {
            probe_dir: Some(PathBuf::from(
                "/this/path/definitely/does/not/exist/probes.jsonl",
            )),
            probe_classes: HashSet::new(),
            default_filter: "warn".to_string(),
            log_dir: None,
        };
        let result = install_probe_tracing(config, crate::modules::logger_clog_sink::LoggerQueueSink);
        // Can't `{:?}` the Ok branch — ProbeInstall holds a
        // WorkerGuard which isn't Debug. Match the variant
        // explicitly instead so the assertion error is still
        // informative.
        match &result {
            Err(ProbeFileSinkError::OpenFailed { .. }) => {}
            Err(other) => panic!("expected OpenFailed, got error {other:?}"),
            Ok(_) => panic!("expected Err, got Ok(_)"),
        }
    }
}

/// The ✓-on-failure defect (card f6c50a49). The executor set `is_error` only on
/// the transport `Err` arm, so a handler that returned its failure AS DATA took
/// the `Ok` arm and rendered a success tick — `code/run` reporting `ok: false`
/// with the compiler's stderr, `code/shell` reporting `status: Running` before
/// the command had finished. These pin the projection that fixes it.
mod outcome_projection {
    use crate::sdk_codegen::{project_result, ActVerdict, ToolVerdict};

    /// what this catches: the projector silently doing NOTHING. It decodes the
    /// value the executor actually holds — `execute_value` returns the handler's
    /// typed `Output` verbatim (`CommandClient::execute` deserializes it straight
    /// into `R`, no envelope), so a shape change that broke the decode would send
    /// every verdict back to a tick.
    #[test]
    fn a_failing_run_projects_failed_from_its_own_typed_output() {
        let failed = crate::commands::code::run::CodeRunResult {
            exit_code: Some(1),
            ok: false,
            stdout: String::new(),
            stderr: "error[E0425]: cannot find value `x`".into(),
            duration_ms: 12,
            timed_out: false,
            interpreter: "rustc".into(),
        };
        let value = serde_json::to_value(&failed).expect("serializes");
        let (verdict, handle) = project_result("code/run", &value);
        assert_eq!(verdict, ActVerdict::Declared(ToolVerdict::Failed));
        assert!(verdict.failed(), "a nonzero exit must read as failed");
        assert_eq!(handle, None, "code/run hands back no running handle");
    }

    /// what this catches: a run that DID succeed being flagged as an error — the
    /// opposite regression, which would feed the model a false failure.
    #[test]
    fn a_clean_run_projects_succeeded() {
        let ok = crate::commands::code::run::CodeRunResult {
            exit_code: Some(0),
            ok: true,
            stdout: "hello".into(),
            stderr: String::new(),
            duration_ms: 3,
            timed_out: false,
            interpreter: "python3".into(),
        };
        let value = serde_json::to_value(&ok).expect("serializes");
        let (verdict, _) = project_result("code/run", &value);
        assert_eq!(verdict, ActVerdict::Declared(ToolVerdict::Succeeded));
        assert!(!verdict.failed());
    }

    /// what this catches: the ACCEPTED-is-not-COMPLETED case collapsing into a
    /// terminal verdict. `code/shell` documents itself as "always returns
    /// immediately with the execution handle", so `Running` is the COMMON case,
    /// not an edge one — mapping it to Succeeded would re-create the defect for
    /// every async shell call. Also pins that the handle rides along, because the
    /// act seam registers the dispatch from it instead of re-parsing the text.
    #[test]
    fn an_unfinished_shell_projects_running_and_carries_its_handle() {
        use crate::code::shell_types::{ShellExecuteResponse, ShellExecutionStatus};
        let id = uuid::Uuid::new_v4();
        let running = ShellExecuteResponse {
            execution_id: id.to_string(),
            status: ShellExecutionStatus::Running,
            stdout: None,
            stderr: None,
            exit_code: None,
                    next: None,
        };
        let value = serde_json::to_value(&running).expect("serializes");
        let (verdict, handle) = project_result("code/shell", &value);
        assert_eq!(verdict, ActVerdict::Declared(ToolVerdict::Running));
        assert!(verdict.running() && !verdict.failed() && !verdict.succeeded());
        assert_eq!(handle, Some(id), "the command declares its own handle");
    }

    /// what this catches: the poll half drifting from the execute half. They
    /// share one `ShellExecuteResponse` and must share one reading of it, or a
    /// single execution could be Failed on one seam and Succeeded on the other.
    #[test]
    fn shell_and_shell_poll_read_the_same_execution_the_same_way() {
        use crate::code::shell_types::{ShellExecuteResponse, ShellExecutionStatus};
        for (status, expected) in [
            (ShellExecutionStatus::Running, ToolVerdict::Running),
            (ShellExecutionStatus::Completed, ToolVerdict::Succeeded),
            (ShellExecutionStatus::Failed, ToolVerdict::Failed),
            (ShellExecutionStatus::TimedOut, ToolVerdict::Failed),
            (ShellExecutionStatus::Killed, ToolVerdict::Failed),
        ] {
            let value = serde_json::to_value(ShellExecuteResponse {
                execution_id: uuid::Uuid::new_v4().to_string(),
                status: status.clone(),
                stdout: None,
                stderr: Some("boom".into()),
                exit_code: Some(2),
                    next: None,
            })
            .expect("serializes");
            assert_eq!(
                project_result("code/shell", &value).0,
                ActVerdict::Declared(expected),
                "{status:?} on execute"
            );
            assert_eq!(
                project_result("code/shell-poll", &value).0,
                ActVerdict::Declared(expected),
                "{status:?} on poll"
            );
        }
    }

    /// what this catches — and this test REPLACES one that codified the bug
    /// (Astra, #4352 review): a REGISTERED projector whose own declared output no
    /// longer decodes must NOT be indistinguishable from an unregistered command.
    /// Collapsing both into "no verdict" is what let schema drift render as a
    /// success tick. `Undecodable` is a third state precisely so the receipt can
    /// refuse to claim anything.
    #[test]
    fn a_registered_projector_that_cannot_decode_says_so_instead_of_succeeding() {
        let (verdict, handle) =
            project_result("code/run", &serde_json::json!({"unrelated": true}));
        assert_eq!(verdict, ActVerdict::Undecodable);
        assert_ne!(
            verdict,
            ActVerdict::Unprojected,
            "schema drift must not masquerade as a command that never opted in"
        );
        assert!(
            !verdict.succeeded(),
            "a result nobody could decode is NOT a success"
        );
        assert!(
            !verdict.failed(),
            "nor is it a failure — the payload is kept and no claim is made"
        );
        assert_eq!(handle, None);
    }

    /// what this catches: the whole mechanism being dead for commands that never
    /// opted in. They must project `Unprojected`, so their behaviour is
    /// byte-for-byte what it was before this change.
    #[test]
    fn a_command_that_did_not_opt_in_is_unprojected_not_undecodable() {
        let v = project_result("code/read", &serde_json::json!({"anything": 1})).0;
        assert_eq!(v, ActVerdict::Unprojected);
        assert!(!v.failed() && !v.running());
        assert_eq!(
            project_result("definitely/not/a/command", &serde_json::json!({})).0,
            ActVerdict::Unprojected
        );
    }

    /// what this catches: a projector keyed on the RAW wire name. The model does
    /// not always send the canonical one — `code/shell` declares `bash` among its
    /// aliases and small tiers emit `code_run` — and a lookup that missed those
    /// would leave the defect fully alive for exactly the calls that hit it most.
    #[test]
    fn the_lookup_is_by_canonical_name_so_aliases_resolve() {
        use crate::ai::tool_dialect::resolve_wire_name;
        assert_eq!(resolve_wire_name("bash"), "code/shell");
        assert_eq!(resolve_wire_name("code_run"), "code/run");
        let failed = crate::commands::code::run::CodeRunResult {
            exit_code: Some(2),
            ok: false,
            stdout: String::new(),
            stderr: "boom".into(),
            duration_ms: 1,
            timed_out: false,
            interpreter: "python3".into(),
        };
        let value = serde_json::to_value(&failed).expect("serializes");
        assert_eq!(
            project_result(&resolve_wire_name("code_run"), &value).0,
            ActVerdict::Declared(ToolVerdict::Failed),
            "the underscore form must reach its own command's projector"
        );
    }
}

/// A harness bench projects into the VDD record substrate for baselining/replay: the half of
/// `harness_measures_latency_and_gates_p95` that names the VDD layer (the measuring half stays
/// with the harness in the kernel).
mod vdd_projection {
    use crate::runtime::module_harness::CommandBench;
    use std::time::Duration;

    // what this catches: the projection of a clean bench into a record dropping the command,
    // the p50, or the error count — the baseline a regression gate would compare against.
    #[test]
    fn a_clean_bench_projects_into_a_passing_vdd_record() {
        let bench = CommandBench {
            command: "greet/hello".into(),
            runs: 50,
            errors: 0,
            latencies: (1..=50).map(Duration::from_millis).collect(),
        };
        let rec = crate::vdd::record::StandardVddRecord::from_bench(&bench, "greeter-hello-bench", "test-sha");
        assert_eq!(rec.command, "greet/hello");
        assert_eq!(rec.status, crate::vdd::record::HarnessStatus::Pass);
        assert!(rec.execution_ms.is_some(), "p50 recorded as execution_ms");
        assert_eq!(rec.error_count, 0);
    }
}

/// The argument-recovery rendering of a real dispatch refusal: the half of
/// `dispatch_bad_params_has_argument_recovery` that names cognition's tool executor.
mod argument_recovery {
    use crate::sdk_codegen::handler::tests::BareHandler;
    use crate::sdk_codegen::{dispatch, CommandError};

    // Regression for 6dd8befa: malformed params get argument recovery through
    // real dispatch; decoded domain refusals must not share that category.
    // what this catches: the persona-facing rendering losing the real decode failure or the
    // pointer to the command's manual.
    #[tokio::test]
    async fn the_persona_tool_error_keeps_the_refusal_and_names_the_manual() {
        let err = dispatch(&BareHandler, serde_json::json!({ "text": 123 }))
            .await
            .expect_err("type mismatch must refuse");
        let rendered = crate::cognition::tool_executor::command_executor::persona_tool_error(
            "test/echo-bare", err.clone(),
        );
        assert!(rendered.contains(&err), "the real decode failure survives: {rendered}");
        assert!(CommandError::is_argument_refusal("test/echo-bare", &err));
        assert!(rendered.starts_with(&format!("{err}\n→ Call `commands/help` with name \"test/echo-bare\"")),
            "the argument recovery branch preserves the refusal and names its manual: {rendered}");
    }
}
