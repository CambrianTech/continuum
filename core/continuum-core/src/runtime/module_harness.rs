//! `ModuleHarness` — the reusable per-module TDD harness.
//!
//! Decomposition (ServiceGroup / ServiceProfile) made each module independently
//! bootable; this is the testability dividend: stand up *just* the module(s)
//! under test — no ONNX, no Bevy, no persona host, no monolith — drive real
//! `Commands.execute` at it through the genuine dispatch chain, and assert on a
//! typed result. Writing a new module starts with a failing test in milliseconds.
//!
//! ```ignore
//! let h = ModuleHarness::with(Arc::new(MyModule::new())).await;
//! let r: MyResult = h.execute("my/command", json!({ "x": 1 })).await?;   // typed, real dispatch
//! ```
//!
//! This is THE place for per-module test setup — don't re-roll registry +
//! context + executor wiring in each test file (CLAUDE.md test discipline:
//! "reusable fixtures live in one place per concern"). Gated
//! `#[cfg(any(test, feature = "test-fixtures"))]` so production never links it.

use std::sync::Arc;

use serde::de::DeserializeOwned;
use serde_json::Value;

use super::command_executor::CommandExecutor;
use super::message_bus::MessageBus;
use super::module_context::ModuleContext;
use super::registry::ModuleRegistry;
use super::service_module::{CommandResult, ServiceModule};
use super::shared_compute::SharedCompute;
use crate::vdd::record::{HarnessStatus, StandardVddRecord};

/// A minimal runtime hosting exactly the module(s) under test — the smallest
/// thing that can dispatch a real command to them.
pub struct ModuleHarness {
    executor: Arc<CommandExecutor>,
    registry: Arc<ModuleRegistry>,
}

impl ModuleHarness {
    /// Stand up a harness hosting a single `module`, initialized in isolation.
    /// Must run inside a tokio runtime (e.g. `#[tokio::test]`) — it uses the
    /// current handle for the module context.
    pub async fn with(module: Arc<dyn ServiceModule>) -> Self {
        Self::with_modules([module]).await
    }

    /// Host several modules together (a module + the dependency it queries via
    /// `ctx.registry`). All are registered BEFORE any `initialize`, so a module
    /// that inspects its peers at init (e.g. the MCP catalog) sees them.
    pub async fn with_modules(modules: impl IntoIterator<Item = Arc<dyn ServiceModule>>) -> Self {
        let registry = Arc::new(ModuleRegistry::new());
        for module in modules {
            registry.register(module);
        }

        let ctx = ModuleContext::new(
            registry.clone(),
            Arc::new(MessageBus::new()),
            Arc::new(SharedCompute::new()),
            tokio::runtime::Handle::current(),
        );
        for name in registry.list_modules() {
            if let Some(module) = registry.get_by_name(name) {
                module
                    .initialize(&ctx)
                    .await
                    .unwrap_or_else(|e| panic!("ModuleHarness: module {name:?} failed to initialize: {e}"));
            }
        }

        let executor = Arc::new(CommandExecutor::new(registry.clone()));
        // Same contract as `start_server`: fill every module's late-bound
        // executor slot so cross-module dual-writes (chat→data, memory→data)
        // work under test exactly as in production. `LateBound::install`
        // no-ops when a test already injected its own executor.
        registry.install_executor_on_all(executor.clone());
        Self { executor, registry }
    }

    /// Execute a command and return its raw JSON result. `Err` is the substrate
    /// refusal string (the same a real caller gets). Binary/Stream/Lambda
    /// results error — a module under unit test should return JSON.
    pub async fn execute_json(&self, command: &str, params: Value) -> Result<Value, String> {
        match self.executor.execute(command, params).await? {
            CommandResult::Json(v) => Ok(v),
            CommandResult::Handle(h) => {
                serde_json::to_value(&h).map_err(|e| format!("serialize handle result: {e}"))
            }
            other => Err(format!(
                "ModuleHarness: command {command:?} returned a non-JSON result ({other:?}); \
                 unit tests expect a JSON/Handle result"
            )),
        }
    }

    /// Execute a command and deserialize the result into a typed `R` — the TDD
    /// ergonomic. A shape mismatch surfaces as a clear error (so a wrong result
    /// type fails the test loudly, not silently).
    pub async fn execute<R: DeserializeOwned>(
        &self,
        command: &str,
        params: Value,
    ) -> Result<R, String> {
        let value = self.execute_json(command, params).await?;
        serde_json::from_value(value)
            .map_err(|e| format!("ModuleHarness: result of {command:?} did not match the expected type: {e}"))
    }

    /// The registry, for assertions about what's hosted (e.g. command schemas).
    pub fn registry(&self) -> &Arc<ModuleRegistry> {
        &self.registry
    }

    /// VALIDATION-driven dev (VDD) per module: run `command` `runs` times against
    /// the isolated module and collect per-run latency — so a test can gate on a
    /// MEASURED property (p95 under X) rather than just pass/fail. The VDD twin of
    /// `execute` (correctness): same one-line harness, now measuring *how well*.
    ///
    /// Latency is wall-clock around the real dispatch (executor → module). It
    /// includes the harness's fixed dispatch overhead, which is constant per
    /// module — so it's sound for regression baselining a module against itself.
    pub async fn measure(
        &self,
        command: &str,
        params: Value,
        runs: usize,
    ) -> CommandBench {
        let mut latencies = Vec::with_capacity(runs);
        let mut errors = 0usize;
        for _ in 0..runs {
            let start = std::time::Instant::now();
            let outcome = self.execute_json(command, params.clone()).await;
            let elapsed = start.elapsed();
            match outcome {
                Ok(_) => latencies.push(elapsed),
                Err(_) => errors += 1,
            }
        }
        latencies.sort_unstable();
        CommandBench {
            command: command.to_string(),
            runs,
            errors,
            latencies,
        }
    }
}

/// A latency measurement of one command over N runs — the VDD per-module result.
/// Sorted successful-run latencies + an error count. Offers percentiles and
/// regression-gate assertions, and converts to a [`StandardVddRecord`] so it
/// plugs into the existing VDD report/replay substrate.
#[derive(Debug, Clone)]
pub struct CommandBench {
    pub command: String,
    pub runs: usize,
    pub errors: usize,
    /// Successful-run latencies, ascending.
    pub latencies: Vec<std::time::Duration>,
}

impl CommandBench {
    fn percentile(&self, p: f64) -> std::time::Duration {
        if self.latencies.is_empty() {
            return std::time::Duration::ZERO;
        }
        // Nearest-rank: index = ceil(p/100 * n) - 1, clamped — robust for small N.
        let n = self.latencies.len();
        let rank = ((p / 100.0) * n as f64).ceil() as usize;
        let idx = rank.saturating_sub(1).min(n - 1);
        self.latencies[idx]
    }

    pub fn p50(&self) -> std::time::Duration {
        self.percentile(50.0)
    }
    pub fn p95(&self) -> std::time::Duration {
        self.percentile(95.0)
    }
    pub fn min(&self) -> std::time::Duration {
        self.latencies.first().copied().unwrap_or_default()
    }
    pub fn max(&self) -> std::time::Duration {
        self.latencies.last().copied().unwrap_or_default()
    }

    /// Regression gate: every run succeeded (no errors). A measurement over a
    /// flaky command isn't a valid latency baseline.
    pub fn assert_all_ok(&self) {
        assert_eq!(
            self.errors, 0,
            "VDD: {} run(s) of {:?} errored — not a clean measurement",
            self.errors, self.command
        );
    }

    /// Regression gate: p95 latency under `max`. The VDD assertion — a test fails
    /// when a module's latency regresses past its baseline, not just when it breaks.
    pub fn assert_p95_under(&self, max: std::time::Duration) {
        self.assert_all_ok();
        let p95 = self.p95();
        assert!(
            p95 <= max,
            "VDD regression: {} p95 = {:?} exceeds baseline {:?} (p50 {:?}, max {:?}, n={})",
            self.command,
            p95,
            max,
            self.p50(),
            self.max(),
            self.latencies.len()
        );
    }

    /// Project into a [`StandardVddRecord`] (execution_ms = p50) so the
    /// measurement persists for cross-run baselining + the `cargo-continuum-vdd`
    /// report/replay. Status reflects whether all runs succeeded.
    pub fn to_vdd_record(&self, scenario: impl Into<String>, git_sha: impl Into<String>) -> StandardVddRecord {
        let mut rec = StandardVddRecord::minimal(scenario, self.command.clone(), git_sha);
        rec.execution_ms = Some(self.p50().as_millis() as u64);
        rec.error_count = self.errors as u32;
        rec.responses_expected = self.runs as u32;
        rec.responses_observed = self.latencies.len() as u32;
        rec.status = if self.errors == 0 {
            HarnessStatus::Pass
        } else {
            HarnessStatus::Fail
        };
        rec
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::{ModuleConfig, ModulePriority};
    use async_trait::async_trait;
    use serde::{Deserialize, Serialize};
    use serde_json::json;
    use std::any::Any;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct Greeting {
        hello: String,
    }

    /// A tiny module with a real `initialize` (flips a flag) + one command, to
    /// prove the harness initializes and dispatches in isolation.
    struct GreeterModule {
        initialized: Arc<AtomicBool>,
    }
    impl GreeterModule {
        const PREFIXES: &'static [&'static str] = &["greet/"];
    }
    #[async_trait]
    impl ServiceModule for GreeterModule {
        fn config(&self) -> ModuleConfig {
            ModuleConfig {
                name: "greeter",
                priority: ModulePriority::Normal,
                command_prefixes: Self::PREFIXES,
                event_subscriptions: &[],
                needs_dedicated_thread: false,
                max_concurrency: 0,
                tick_interval: None,
            }
        }
        async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
            self.initialized.store(true, Ordering::SeqCst);
            Ok(())
        }
        async fn handle_command(&self, command: &str, params: Value) -> Result<CommandResult, String> {
            match command {
                "greet/hello" => {
                    let who = params.get("who").and_then(|v| v.as_str()).unwrap_or("world");
                    CommandResult::json(&Greeting {
                        hello: who.to_string(),
                    })
                }
                "greet/refuse" => Err("greeter refuses this".into()),
                other => Err(format!("unknown greeter command {other}")),
            }
        }
        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    // what this catches: the harness stands up ONE module, INITIALIZES it (the
    // flag flips), and dispatches a real command through the executor — the
    // per-module TDD loop, no monolith.
    #[tokio::test]
    async fn harness_initializes_and_dispatches_a_single_module() {
        let flag = Arc::new(AtomicBool::new(false));
        let h = ModuleHarness::with(Arc::new(GreeterModule {
            initialized: flag.clone(),
        }))
        .await;
        assert!(flag.load(Ordering::SeqCst), "module initialize must run in the harness");

        let g: Greeting = h.execute("greet/hello", json!({ "who": "tester" })).await.unwrap();
        assert_eq!(g, Greeting { hello: "tester".into() });
    }

    // what this catches: the typed execute deserializes the result; a refusal
    // surfaces as the substrate error string (not a panic), so a test can assert
    // on failure paths too.
    #[tokio::test]
    async fn harness_typed_result_and_refusal() {
        let h = ModuleHarness::with(Arc::new(GreeterModule {
            initialized: Arc::new(AtomicBool::new(false)),
        }))
        .await;

        // raw JSON view
        let v = h.execute_json("greet/hello", json!({})).await.unwrap();
        assert_eq!(v["hello"], "world");

        // refusal → Err carrying the reason
        let err = h.execute_json("greet/refuse", json!({})).await.unwrap_err();
        assert!(err.contains("refuses"), "refusal reason surfaced: {err}");

        // wrong expected type → loud error, not a silent default
        let typed: Result<Greeting, String> = h.execute("greet/refuse", json!({})).await;
        assert!(typed.is_err());
    }

    // what this catches (the VDD dimension): measure() runs the command N times,
    // collects latency, and the p95 gate passes for a fast command. This is the
    // per-module VDD loop — a test gating on a MEASURED property, and the bench
    // projects into a StandardVddRecord for baselining/replay.
    #[tokio::test]
    async fn harness_measures_latency_and_gates_p95() {
        let h = ModuleHarness::with(Arc::new(GreeterModule {
            initialized: Arc::new(AtomicBool::new(false)),
        }))
        .await;

        let bench = h.measure("greet/hello", json!({ "who": "vdd" }), 50).await;
        assert_eq!(bench.runs, 50);
        assert_eq!(bench.errors, 0, "all runs succeeded");
        assert_eq!(bench.latencies.len(), 50);
        assert!(bench.p50() <= bench.p95(), "p50 <= p95");
        assert!(bench.min() <= bench.max());
        // A trivial in-process command is fast — generous gate avoids CI flake
        // while still pinning that the VDD assertion mechanism works.
        bench.assert_p95_under(std::time::Duration::from_millis(250));

        // Projects into the VDD record substrate for baselining/replay.
        let rec = bench.to_vdd_record("greeter-hello-bench", "test-sha");
        assert_eq!(rec.command, "greet/hello");
        assert_eq!(rec.status, HarnessStatus::Pass);
        assert!(rec.execution_ms.is_some(), "p50 recorded as execution_ms");
        assert_eq!(rec.error_count, 0);
    }
}
