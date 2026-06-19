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
}
