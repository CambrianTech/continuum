//! AircInterceptor — routes commands targeting airc-addressed peers via
//! the airc messaging substrate. **Stub form: trait wired, transport
//! deferred until the airc module ships its command-transport surface.**
//!
//! # Why this exists today, in stub form
//!
//! Per [docs/architecture/MODULE-ARCHITECTURE.md](../../../../../docs/architecture/MODULE-ARCHITECTURE.md)
//! §7.1: airc is "just another module" providing a transport. The
//! eventual contract is that `Commands::execute("foo/bar", { aircPeer:
//! "id" })` should route the command over the airc messaging substrate
//! to that peer's continuum-core, execute there, return the result.
//! Same primitive as grid hops; different transport.
//!
//! Why land the interceptor in stub form before the transport exists:
//!
//! 1. The interceptor chain is a sequence; landing the airc slot now
//!    pins the order before grid wires in. Today's wire order is
//!    `[airc, grid]` — explicit airc-targeted commands take precedence
//!    over grid's capability-based remote routing.
//! 2. The stub fail-loud on actual airc targets (rather than silently
//!    declining) keeps the contract honest: a caller who writes
//!    `aircPeer: "..."` learns immediately that the transport isn't
//!    ready, rather than having the request silently fall through to
//!    local dispatch where there's no airc routing at all.
//! 3. Per Joel's `[[every-error-is-an-opportunity-to-battle-harden]]`
//!    standing rule: fail-loud surfaces the gap. Silent decline would
//!    hide it under the rug until live chat traffic hits.
//!
//! # How callers signal an airc target
//!
//! `params.aircPeer: String` — explicit peer ID. The transport (when
//! wired) routes to that peer's continuum-core over the airc substrate.
//!
//! `params.aircRoom: String` — broadcast to a room's members. Useful
//! for "tell everyone in this conversation" semantics.
//!
//! Absent both, the interceptor declines and the chain continues.
//!
//! # When the transport lands
//!
//! Replace [`AircInterceptor::try_route`]'s `Err` path with a real call
//! into the airc module's `airc/send-command` (or equivalent). The
//! stub's structure already discriminates the param shape; only the
//! transport call body needs to change.

use std::sync::Arc;

use airc_lib::Airc;
use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::OnceCell;
use uuid::Uuid;

use super::command_interceptor::{CommandInterceptor, InterceptorOutcome};
use crate::ai::types::TextGenerationRequest;
use crate::inference::airc_remote::protocol::RemoteInferenceRequest;
use crate::inference::airc_remote::transport::{AircInferenceTransport, AircLiveTransport};
use crate::runtime::service_module::CommandResult;

/// The one command that rides the airc inference transport in this first cut.
/// Generic any-command routing over airc (mirroring grid's `dispatch_to_node`)
/// is the follow-up; today only inference hops to an explicit peer.
const AIRC_ROUTED_GENERATE: &str = "ai/generate";

/// AircInterceptor — sits at the head of the interceptor chain so airc-
/// targeted commands route to the messaging substrate before grid even
/// looks at them. See module docs.
///
/// Holds a LATE-BOUND concrete `Arc<Airc>` — the airc-lib handle carrying
/// `request()`, the peer RPC that `AircLiveTransport` sends over. It's a cell,
/// not a constructor arg, because the interceptor is built synchronously during
/// executor construction (`ipc/mod.rs`) while `Airc::attach_as` is async and must
/// not block the boot critical path: a spawned boot task fills the cell once the
/// daemon socket is reachable. Until it's filled, an `aircPeer` target fails loud
/// (no silent fallthrough) — same honest contract as the old stub.
pub struct AircInterceptor {
    airc: Arc<OnceCell<Arc<Airc>>>,
}

impl AircInterceptor {
    /// Stub form — an empty cell. `try_route` on an `aircPeer` target fails loud
    /// until the cell is filled. Used by tests + call sites that don't wire airc.
    pub fn new() -> Self {
        Self {
            airc: Arc::new(OnceCell::new()),
        }
    }

    /// Production form — share the cell a boot task fills with the attached
    /// `Arc<Airc>` (`Airc::attach_as` → `cell.set(...)`). Hand the SAME `Arc` to
    /// the spawned attach task via [`Self::airc_cell`].
    pub fn with_airc_cell(cell: Arc<OnceCell<Arc<Airc>>>) -> Self {
        Self { airc: cell }
    }

    /// The late-bind cell, so the boot flow can spawn the `attach_as` task that
    /// fills it with the concrete handle.
    pub fn airc_cell(&self) -> Arc<OnceCell<Arc<Airc>>> {
        self.airc.clone()
    }
}

impl Default for AircInterceptor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CommandInterceptor for AircInterceptor {
    async fn try_route(
        &self,
        command: &str,
        params: &Value,
        _caller: Option<&crate::routing::CallerIdentity>,
    ) -> Result<InterceptorOutcome, String> {
        let peer = params.get("aircPeer").and_then(|v| v.as_str());
        let room = params.get("aircRoom").and_then(|v| v.as_str());

        match (peer, room) {
            // Neither airc target field set — not an airc-routed command.
            // Decline cleanly, let the chain (grid, then local) continue.
            (None, None) => Ok(InterceptorOutcome::Decline),

            // Room broadcast isn't a request/response inference hop. Fail loud
            // rather than pretend a single-peer RPC — only aircPeer routes today.
            // Echo the target (like the peer path) so callers can correlate logs.
            (None, Some(room)) => Err(format!(
                "airc room-broadcast routing (aircRoom '{room}') isn't wired into the \
                 kernel yet — only aircPeer (a single-peer command RPC) routes over airc today."
            )),

            // Explicit single-peer target: route the command over airc to that
            // peer's continuum-core and return its result — the E=mc² primitive,
            // remote-transparent to the caller.
            (Some(target), _) => {
                // The concrete airc handle, late-bound by the boot attach task.
                // Absent until attach completes → fail loud, never silent-decline.
                let airc = self.airc.get().ok_or_else(|| {
                    format!(
                        "airc routing requested for '{command}' (target '{target}') but the \
                         airc handle hasn't attached yet — retry once the airc daemon is \
                         reachable (Airc::attach_as fills the interceptor cell on boot)."
                    )
                })?;

                // First cut: only inference rides the airc transport. Generic
                // any-command routing (mirroring grid's dispatch_to_node) is the follow-up.
                if command != AIRC_ROUTED_GENERATE {
                    return Err(format!(
                        "airc routing for '{command}' isn't wired yet — only \
                         '{AIRC_ROUTED_GENERATE}' hops to an explicit aircPeer today. \
                         Generic command-over-airc is the follow-up (mirrors grid dispatch)."
                    ));
                }

                let peer_id = Uuid::parse_str(target).map_err(|e| {
                    format!("aircPeer must be a peer UUID, got {target:?}: {e}")
                })?;

                // For ai/generate the command params ARE the TextGenerationRequest.
                let text_request: TextGenerationRequest = serde_json::from_value(params.clone())
                    .map_err(|e| {
                        format!(
                            "aircPeer '{AIRC_ROUTED_GENERATE}' params aren't a \
                             TextGenerationRequest: {e}"
                        )
                    })?;

                let transport = AircLiveTransport::new(airc.clone(), peer_id);
                let request = RemoteInferenceRequest::new(text_request).with_target_peer(target);
                let response = transport.send_request(request).await.map_err(|e| {
                    format!("airc remote inference to peer '{target}' failed: {e}")
                })?;

                let result = CommandResult::json(&response.text_response)?;
                Ok(InterceptorOutcome::Handled(result))
            }
        }
    }

    fn name(&self) -> &'static str {
        "airc"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn declines_when_no_airc_target() {
        let interceptor = AircInterceptor::new();
        let outcome = interceptor
            .try_route(
                "chat/send",
                &serde_json::json!({ "roomId": "abc", "content": "hi" }),
                None,
            )
            .await
            .expect("no-target call must not error");
        assert!(
            matches!(outcome, InterceptorOutcome::Decline),
            "interceptor must Decline when no aircPeer/aircRoom param is present, \
             so the chain falls through to grid + local dispatch"
        );
    }

    #[tokio::test]
    async fn fails_loud_when_airc_peer_targeted_but_transport_missing() {
        let interceptor = AircInterceptor::new();
        let err = interceptor
            .try_route(
                "chat/send",
                &serde_json::json!({
                    "aircPeer": "peer-uuid-here",
                    "content": "hi"
                }),
                None,
            )
            .await
            .expect_err(
                "explicit aircPeer must surface a real error until the \
                 transport is wired — silent decline would hide the gap",
            );
        assert!(
            err.contains("airc"),
            "error must name the airc transport: {err}"
        );
        // With an empty cell (new()), the aircPeer target fails loud on the
        // not-yet-attached handle — the honest "retry when the daemon is
        // reachable" contract, never a silent decline.
        assert!(
            err.contains("attach"),
            "error must explain the airc handle hasn't attached yet: {err}"
        );
        assert!(
            err.contains("peer-uuid-here"),
            "error must echo the target so the caller can correlate logs: {err}"
        );
    }

    #[tokio::test]
    async fn fails_loud_when_airc_room_targeted_but_transport_missing() {
        let interceptor = AircInterceptor::new();
        let err = interceptor
            .try_route(
                "chat/send",
                &serde_json::json!({
                    "aircRoom": "room-uuid",
                    "content": "hi"
                }),
                None,
            )
            .await
            .expect_err("explicit aircRoom must surface a real error");
        assert!(err.contains("room-uuid"), "error echoes the target: {err}");
    }

    #[tokio::test]
    async fn name_is_stable() {
        let interceptor = AircInterceptor::new();
        assert_eq!(interceptor.name(), "airc");
    }
}
