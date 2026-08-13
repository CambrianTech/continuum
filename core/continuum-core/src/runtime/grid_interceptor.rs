//! GridInterceptor — bridges the existing [`crate::modules::grid`] routing
//! into the kernel's [`super::command_interceptor::CommandInterceptor`]
//! chain.
//!
//! # What this connects
//!
//! The grid module already owns the routing policy + the send-frame
//! dispatch:
//!
//! - `crate::modules::grid::router::GridRouter::route(command, params, registry)`
//!   returns `Local` or `Remote { node }` based on explicit `nodeId`
//!   params, `routingHint` hints, and capability matching.
//! - `crate::modules::grid::handlers::dispatch_to_node(state, node, cmd, params)`
//!   opens a transport connection, sends a CommandRequest frame, awaits
//!   the matching CommandResult frame, audits the round-trip, returns
//!   the deserialized result.
//!
//! Pre this interceptor, the only callers were:
//!
//! - `grid/send` (explicit) — the user (or a Rust caller) names the
//!   target node and command, dispatches over the grid wire.
//!
//! Post this interceptor, capability-based routing works for ANY
//! command: a caller writing `ai/generate { routingHint: "max-compute"
//! }` triggers the router → picks a remote node with the most VRAM →
//! dispatches the command there → returns the remote result. All
//! through the same kernel `Commands.execute` primitive; the routing
//! decision is invisible to the caller.
//!
//! # Position in the chain
//!
//! Wire order (`init_executor`): `[airc, grid]`. Explicit airc-targeted
//! commands take precedence over grid's capability-based routing so a
//! caller who writes `aircPeer: "..."` doesn't get accidentally hopped
//! over grid's max-compute heuristic.
//!
//! # Why not in the grid module
//!
//! GridInterceptor lives in `runtime/` (not `modules/grid/`) because the
//! interceptor TRAIT is a runtime concept — every transport interceptor
//! sits behind it, and the runtime is what walks the chain. The
//! interceptor's *implementation* delegates to grid; that's just a
//! dependency the runtime takes on the grid module, mediated by the
//! `Arc<GridState>` public handle.

use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;

use super::command_interceptor::{CommandInterceptor, InterceptorOutcome};
use crate::modules::grid::GridState;

/// GridInterceptor — wraps `GridState::try_route_remote` and bridges it
/// into the kernel dispatch chain.
pub struct GridInterceptor {
    state: Arc<GridState>,
}

impl GridInterceptor {
    pub fn new(state: Arc<GridState>) -> Self {
        Self { state }
    }
}

#[async_trait]
impl CommandInterceptor for GridInterceptor {
    async fn try_route(
        &self,
        command: &str,
        params: &Value,
        _caller: Option<&crate::routing::CallerIdentity>,
    ) -> Result<InterceptorOutcome, String> {
        match self.state.try_route_remote(command, params).await? {
            Some(result) => Ok(InterceptorOutcome::Handled(result)),
            None => Ok(InterceptorOutcome::Decline),
        }
    }

    fn name(&self) -> &'static str {
        "grid"
    }
}

#[cfg(test)]
mod tests {
    //! Integration tests for the wired interceptor live in
    //! `tests/grid_interceptor_routes.rs` — they stand up a `GridState`
    //! with a mock transport + a synthetic node registry and assert
    //! the round-trip. The unit tests here pin the trait wiring:
    //! `name()` and that the interceptor declines cleanly when the
    //! router decision is `Local` (no remote node configured).

    use super::*;
    use crate::modules::grid::GridModule;
    use std::path::PathBuf;

    fn make_state() -> Arc<GridState> {
        // Construct a GridModule without a GPU + minimal grid_dir.
        // The router defaults to Local for commands with no nodeId /
        // routingHint and no remote nodes registered.
        let tmpdir =
            std::env::temp_dir().join(format!("grid-interceptor-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&tmpdir);
        let module = GridModule::new(tmpdir, false, 0);
        module.state()
    }

    #[tokio::test]
    async fn name_is_stable() {
        let state = make_state();
        let interceptor = GridInterceptor::new(state);
        assert_eq!(interceptor.name(), "grid");
    }

    #[tokio::test]
    async fn declines_when_router_picks_local() {
        // Router with no remote nodes registered + a command with no
        // routing params → Local decision → interceptor declines.
        let state = make_state();
        let interceptor = GridInterceptor::new(state);
        let outcome = interceptor
            .try_route("anything", &serde_json::json!({}), None)
            .await
            .expect("local routing must not error");
        assert!(
            matches!(outcome, InterceptorOutcome::Decline),
            "no remote node + no routing hint → router picks Local → interceptor declines, \
             so the chain falls through to local Rust + TS dispatch"
        );
    }

    #[tokio::test]
    async fn declines_for_local_only_hint() {
        // routingHint: "local-only" forces Local regardless of capability.
        let state = make_state();
        let interceptor = GridInterceptor::new(state);
        let outcome = interceptor
            .try_route(
                "ai/generate",
                &serde_json::json!({ "routingHint": "local-only" }),
                None,
            )
            .await
            .expect("local-only routing must not error");
        assert!(
            matches!(outcome, InterceptorOutcome::Decline),
            "local-only hint must short-circuit to Decline so the chain stays local"
        );
    }

    #[tokio::test]
    async fn declines_when_target_node_not_in_registry() {
        // Explicit nodeId pointing at a node that doesn't exist in the
        // registry → router falls back to Local (per its existing
        // behavior at router.rs:54-64) → interceptor declines.
        let state = make_state();
        let interceptor = GridInterceptor::new(state);
        let outcome = interceptor
            .try_route(
                "anything",
                &serde_json::json!({ "nodeId": "nonexistent-node-id" }),
                None,
            )
            .await
            .expect("unknown-node routing must not error");
        assert!(
            matches!(outcome, InterceptorOutcome::Decline),
            "unknown nodeId must fall through (not error) so the kernel can serve the command \
             locally — the existing GridRouter contract"
        );
    }
}
