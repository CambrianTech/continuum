//! Architecture test — proves the no-fallbacks doctrine clause at the
//! integration-test layer, complementing the unit-level regression test
//! in `runtime::command_executor::tests`.
//!
//! See `docs/architecture/PROVING-THE-DOCTRINE.md` for the matrix this
//! file populates. The clause this file pins:
//!
//! > "No silent TS fallthrough" — when no Rust module owns a command,
//! > `CommandExecutor::execute` returns a typed `CommandNotFound`-shaped
//! > error rather than silently routing to the TS bridge.
//!
//! Tag convention: `// proves: <clause>` per the proof-discipline doc.
//! Reviewers `git grep '// proves:'` to walk every proof.
//!
//! ## Why this is shape-1 + an integration test (not just shape-1 unit)
//!
//! The in-module unit test in `runtime/command_executor.rs::tests`
//! exercises the executor with a hand-built registry. This file
//! exercises the executor through the same public API a real caller
//! would use — `CommandExecutor::new` + `execute` — without any
//! interceptors, without a wired registry, without a bus. Catches the
//! class of bug where the unit test passes because of in-module access
//! to private state but the integration boundary is broken.
//!
//! Per the proof-discipline doc § "Shape 1 — unit-level invariant":
//! this file is the integration-tier analog of the same shape.

use continuum_core::runtime::{CommandExecutor, ModuleRegistry};
use serde_json::Value;
use std::sync::Arc;

/// what this catches: a command not handled by any interceptor and
/// not registered in the Rust module registry returns a typed error
/// per [[no-fallbacks-ever]]. Pre-PR #1585 the executor silently
/// routed to the TS bridge on `/tmp/jtag-command-router.sock`. The
/// error MUST NOT mention the TS socket and MUST name the missing
/// command + the explicit escape hatch.
///
/// proves: no-fallback fallthrough
#[tokio::test]
async fn unknown_command_returns_typed_no_fallback_error() {
    let registry = Arc::new(ModuleRegistry::new());
    let executor = CommandExecutor::new(registry);

    let err = executor
        .execute("totally/imaginary/command", Value::Null)
        .await
        .expect_err("unknown command must produce a typed error");

    // MUST NOT attempt the TS bridge.
    assert!(
        !err.contains("CommandRouterServer"),
        "error must NOT attempt the TS fallthrough: {err}"
    );
    assert!(
        !err.contains("/tmp/jtag-command-router.sock"),
        "error must NOT mention the TS socket path: {err}"
    );

    // MUST name the missing command.
    assert!(
        err.contains("totally/imaginary/command"),
        "error must name the missing command: {err}"
    );

    // MUST point at the explicit TS-bridge escape hatch.
    assert!(
        err.contains("execute_ts_json") || err.contains("execute_ts"),
        "error must point at the explicit TS-bridge API: {err}"
    );
}

/// what this catches: many unrelated commands ALL produce the same
/// shape of error. The contract is structural, not per-command.
/// Without this, a regression could partially restore the silent
/// fallthrough (e.g., only for commands matching a specific prefix)
/// and the single-command test above wouldn't catch it.
///
/// proves: no-fallback fallthrough (structural)
#[tokio::test]
async fn no_fallback_error_is_structural_across_command_shapes() {
    let registry = Arc::new(ModuleRegistry::new());
    let executor = CommandExecutor::new(registry);

    let unknown_commands = [
        "single-token",
        "slash/separated",
        "deeply/nested/path/here",
        "with-dashes",
        "UPPER/case",
        "n0/spec1al/ch4rs",
        "",
    ];

    for cmd in unknown_commands {
        let err = executor
            .execute(cmd, Value::Null)
            .await
            .expect_err("unknown command must produce a typed error: {cmd}");

        assert!(
            !err.contains("CommandRouterServer") && !err.contains("jtag-command-router.sock"),
            "structural no-fallback violation on '{cmd}': error mentions TS bridge: {err}"
        );
        // For empty string the contract still holds — we just don't
        // require the empty string to appear in the error message.
        if !cmd.is_empty() {
            assert!(
                err.contains(cmd),
                "no-fallback error must name the rejected command. cmd='{cmd}' err='{err}'"
            );
        }
    }
}

/// what this catches: a registered Rust module DOES handle its
/// command — the no-fallback contract is "no silent fallback when
/// nobody handles it," not "always fail." If a regression broke the
/// registry's dispatch path, every command would return the no-fallback
/// error, including ones that should resolve. This test pins the
/// happy path so that future no-fallback-strengthening changes don't
/// over-shoot into breaking real dispatch.
///
/// We use a minimal canned module so the test doesn't depend on the
/// substrate's full module set.
///
/// proves: no-fallback fallthrough (positive path preserved)
#[tokio::test]
async fn registered_command_still_dispatches() {
    use async_trait::async_trait;
    use continuum_core::runtime::{
        CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule,
    };
    use std::any::Any;

    struct CannedModule;

    #[async_trait]
    impl ServiceModule for CannedModule {
        fn config(&self) -> ModuleConfig {
            ModuleConfig {
                name: "canned",
                priority: ModulePriority::Normal,
                command_prefixes: &["canned/"],
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
            _params: Value,
        ) -> Result<CommandResult, String> {
            Ok(CommandResult::Json(serde_json::json!({ "handled": true })))
        }

        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    let registry = Arc::new(ModuleRegistry::new());
    registry.register(Arc::new(CannedModule));
    let executor = CommandExecutor::new(registry);

    let result = executor
        .execute("canned/echo", Value::Null)
        .await
        .expect("registered command must dispatch successfully");

    match result {
        CommandResult::Json(v) => {
            assert_eq!(
                v["handled"], true,
                "canned module's handler ran and returned its sentinel value"
            );
        }
        other => panic!("expected Json, got {other:?}"),
    }
}
