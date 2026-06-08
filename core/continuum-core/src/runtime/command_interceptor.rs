//! CommandInterceptor — the routing-decision chain that runs before local
//! dispatch in [`super::command_executor::CommandExecutor`].
//!
//! # Why this exists
//!
//! Per [docs/architecture/MODULE-ARCHITECTURE.md](../../../../../docs/architecture/MODULE-ARCHITECTURE.md)
//! §5 ("Composition: Commands Call Commands") and §7.1 ("airc as just
//! another module"): the kernel composes routing decisions by walking a
//! chain of interceptors before falling back to local Rust dispatch and
//! finally to TypeScript. No transport is special at the kernel level —
//! grid, airc, future mesh transports, future caching layers all sit
//! behind the same trait and the same dispatch loop.
//!
//! Today's `CommandExecutor::execute` already does the local-Rust-then-TS
//! pair. This trait adds the prefix: interceptors get FIRST look. The
//! result is a single primitive that handles four transport modes
//! (local Rust, IPC to TS, grid hop to a peer, airc routing to a peer)
//! with one entry point and one signature.
//!
//! # The contract
//!
//! Implementations decide per call whether to handle the command or step
//! aside:
//!
//! - [`InterceptorOutcome::Handled`] — interceptor took the command;
//!   the chain stops and this result is returned to the caller.
//! - [`InterceptorOutcome::Decline`] — interceptor passed; the next
//!   interceptor (or the local-dispatch fallthrough) takes over.
//! - `Err(_)` — interceptor failed in a way the caller should see;
//!   the chain stops and the error propagates. No silent fallthrough
//!   on Err — that would hide exactly the routing bugs interceptors
//!   exist to surface.
//!
//! # Composition order
//!
//! Interceptors are walked in insertion order. Wire order is therefore
//! policy: the earlier an interceptor sits, the higher its priority.
//! Today the intended order is `[airc, grid]` so explicit airc-targeted
//! commands take precedence over grid's capability-based remote routing.
//! Both currently decline by default (airc has no transport yet; grid is
//! not yet wired here) so adding the chain is a no-op until those land.
//!
//! # Why not just modify `CommandExecutor::execute` per-transport
//!
//! The legacy TS-side dispatch [pre-#1198] grew a `_gridInterceptor`
//! shim on the singleton specifically to hop work over to the grid before
//! local dispatch. That worked but baked grid into the kernel signature.
//! The same pressure exists for airc, and any future transport (mesh,
//! tower-relay, etc.) would re-bake the kernel each time. The interceptor
//! trait is the generalization: kernel knows "walk a list, fall through
//! when no one bites"; transports register themselves.

use async_trait::async_trait;
use serde_json::Value;

use super::CommandResult;

/// What an interceptor returns from a `try_route` attempt.
#[derive(Debug)]
pub enum InterceptorOutcome {
    /// Interceptor took the command. The kernel returns this result
    /// without consulting later interceptors or the local dispatch.
    Handled(CommandResult),
    /// Interceptor passed. The next interceptor (or the local-dispatch
    /// fallthrough) gets to try.
    Decline,
}

/// A pluggable routing-decision step. See module docs for the contract.
///
/// Implementations must be `Send + Sync` because the executor holds them
/// in a singleton and dispatches commands concurrently.
#[async_trait]
pub trait CommandInterceptor: Send + Sync {
    /// First look at the command + params + caller. Return
    /// [`InterceptorOutcome::Handled`] to short-circuit the chain, or
    /// [`InterceptorOutcome::Decline`] to let the next interceptor (or
    /// the local dispatcher) handle it.
    ///
    /// `caller` is `None` for substrate-internal in-process invocations
    /// (substrate's own code dispatching), `Some(CallerIdentity::airc(verified_sender))`
    /// for cross-grid invocations the peer-side handler routes through
    /// `execute_with_caller`. Interceptors that gate on caller (e.g.
    /// future capability-token interceptors, sentinel quorum
    /// interceptors) inspect this field; interceptors that don't can
    /// ignore it. Per PR #1529 reviewer 2: threading caller through the
    /// trait closes the silent-privilege-escalation seam where the
    /// gate sees the caller but interceptors didn't.
    ///
    /// Returning `Err` aborts the chain — no silent fall-through on
    /// error, so a misconfigured interceptor surfaces loudly rather
    /// than masking the work.
    async fn try_route(
        &self,
        command: &str,
        params: &Value,
        caller: Option<&crate::routing::CallerIdentity>,
    ) -> Result<InterceptorOutcome, String>;

    /// Static name for logging + telemetry. e.g. `"grid"`, `"airc"`.
    fn name(&self) -> &'static str;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// Interceptor that counts calls + always declines. Used to assert
    /// the chain walks every interceptor in order when no one handles.
    struct DeclineCounter {
        name: &'static str,
        count: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl CommandInterceptor for DeclineCounter {
        async fn try_route(
            &self,
            _command: &str,
            _params: &Value,
            _caller: Option<&crate::routing::CallerIdentity>,
        ) -> Result<InterceptorOutcome, String> {
            self.count.fetch_add(1, Ordering::SeqCst);
            Ok(InterceptorOutcome::Decline)
        }

        fn name(&self) -> &'static str {
            self.name
        }
    }

    /// Interceptor that always handles with a fixed result. Used to
    /// assert the chain short-circuits on the first Handle.
    struct AlwaysHandle {
        name: &'static str,
        value: i64,
    }

    #[async_trait]
    impl CommandInterceptor for AlwaysHandle {
        async fn try_route(
            &self,
            _command: &str,
            _params: &Value,
            _caller: Option<&crate::routing::CallerIdentity>,
        ) -> Result<InterceptorOutcome, String> {
            Ok(InterceptorOutcome::Handled(CommandResult::Json(
                serde_json::json!({ "value": self.value }),
            )))
        }

        fn name(&self) -> &'static str {
            self.name
        }
    }

    /// Interceptor that always errors. Used to assert errors propagate
    /// (no silent fall-through to later interceptors or local dispatch).
    struct AlwaysErr {
        name: &'static str,
    }

    #[async_trait]
    impl CommandInterceptor for AlwaysErr {
        async fn try_route(
            &self,
            _command: &str,
            _params: &Value,
            _caller: Option<&crate::routing::CallerIdentity>,
        ) -> Result<InterceptorOutcome, String> {
            Err(format!("{} failed loudly", self.name))
        }

        fn name(&self) -> &'static str {
            self.name
        }
    }

    /// Walk-the-chain helper that mirrors the loop in `CommandExecutor`.
    /// Lets us test the contract here without standing up the full
    /// executor + module registry.
    async fn walk(
        interceptors: &[Arc<dyn CommandInterceptor>],
        command: &str,
        params: &Value,
    ) -> Result<Option<CommandResult>, String> {
        for interceptor in interceptors {
            match interceptor.try_route(command, params, None).await? {
                InterceptorOutcome::Handled(result) => return Ok(Some(result)),
                InterceptorOutcome::Decline => continue,
            }
        }
        Ok(None)
    }

    #[tokio::test]
    async fn empty_chain_returns_none() {
        let chain: Vec<Arc<dyn CommandInterceptor>> = vec![];
        let result = walk(&chain, "anything", &Value::Null).await.unwrap();
        assert!(result.is_none(), "empty chain must fall through (None)");
    }

    #[tokio::test]
    async fn all_decline_falls_through() {
        let count = Arc::new(AtomicUsize::new(0));
        let chain: Vec<Arc<dyn CommandInterceptor>> = vec![
            Arc::new(DeclineCounter {
                name: "a",
                count: count.clone(),
            }),
            Arc::new(DeclineCounter {
                name: "b",
                count: count.clone(),
            }),
            Arc::new(DeclineCounter {
                name: "c",
                count: count.clone(),
            }),
        ];
        let result = walk(&chain, "anything", &Value::Null).await.unwrap();
        assert!(result.is_none(), "all-decline chain must fall through");
        assert_eq!(
            count.load(Ordering::SeqCst),
            3,
            "every interceptor must be consulted when all decline"
        );
    }

    #[tokio::test]
    async fn first_to_handle_wins_short_circuits_later() {
        let count = Arc::new(AtomicUsize::new(0));
        let chain: Vec<Arc<dyn CommandInterceptor>> = vec![
            Arc::new(DeclineCounter {
                name: "a",
                count: count.clone(),
            }),
            Arc::new(AlwaysHandle {
                name: "b",
                value: 42,
            }),
            Arc::new(DeclineCounter {
                name: "c-never-called",
                count: count.clone(),
            }),
        ];
        let result = walk(&chain, "anything", &Value::Null)
            .await
            .unwrap()
            .expect("middle interceptor should have handled");
        match result {
            CommandResult::Json(v) => assert_eq!(v["value"], 42),
            other => panic!("expected Json, got {other:?}"),
        }
        assert_eq!(
            count.load(Ordering::SeqCst),
            1,
            "interceptors AFTER the handler must NOT be consulted"
        );
    }

    #[tokio::test]
    async fn err_aborts_chain_no_silent_fallthrough() {
        let count = Arc::new(AtomicUsize::new(0));
        let chain: Vec<Arc<dyn CommandInterceptor>> = vec![
            Arc::new(AlwaysErr { name: "boom" }),
            Arc::new(DeclineCounter {
                name: "never-called",
                count: count.clone(),
            }),
        ];
        let err = walk(&chain, "anything", &Value::Null)
            .await
            .expect_err("Err must propagate");
        assert!(
            err.contains("boom"),
            "error must carry the interceptor identity for diagnosis: {err}"
        );
        assert_eq!(
            count.load(Ordering::SeqCst),
            0,
            "interceptors AFTER an error must NOT be consulted — \
             silent fallthrough on err would hide the routing bug"
        );
    }

    #[tokio::test]
    async fn name_propagates_through_dyn_trait() {
        // Pin that `name()` survives the trait-object boundary so logs
        // and telemetry can identify which interceptor handled which
        // command without storing extra metadata.
        let handler: Arc<dyn CommandInterceptor> = Arc::new(AlwaysHandle {
            name: "diagnostic",
            value: 0,
        });
        assert_eq!(handler.name(), "diagnostic");
    }
}
