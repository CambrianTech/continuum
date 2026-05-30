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

use async_trait::async_trait;
use serde_json::Value;

use super::command_interceptor::{CommandInterceptor, InterceptorOutcome};

/// AircInterceptor — sits at the head of the interceptor chain so airc-
/// targeted commands route to the messaging substrate before grid even
/// looks at them. See module docs for the stub contract.
pub struct AircInterceptor;

impl AircInterceptor {
    pub fn new() -> Self {
        Self
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
    ) -> Result<InterceptorOutcome, String> {
        let peer = params.get("aircPeer").and_then(|v| v.as_str());
        let room = params.get("aircRoom").and_then(|v| v.as_str());

        match (peer, room) {
            // Neither airc target field set — this isn't an airc-routed
            // command. Decline cleanly, let the chain continue.
            (None, None) => Ok(InterceptorOutcome::Decline),

            // Airc target set, but the transport isn't wired yet. Fail
            // loudly with a concrete pointer to the missing piece, so a
            // caller writing `aircPeer` finds out at request time rather
            // than from silent fallthrough.
            (Some(target), _) | (_, Some(target)) => Err(format!(
                "airc routing requested for command '{command}' \
                 (target: '{target}'), but the airc transport is not \
                 yet wired into the kernel — see MODULE-ARCHITECTURE.md \
                 §7.1. Until @continuum-modules/airc exposes the \
                 send-command primitive this interceptor delegates to, \
                 callers must omit aircPeer/aircRoom params."
            )),
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
            .try_route("chat/send", &serde_json::json!({ "roomId": "abc", "content": "hi" }))
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
            )
            .await
            .expect_err(
                "explicit aircPeer must surface a real error until the \
                 transport is wired — silent decline would hide the gap",
            );
        assert!(
            err.contains("airc"),
            "error must name the missing transport: {err}"
        );
        assert!(
            err.contains("MODULE-ARCHITECTURE"),
            "error must point at the canonical doc for the design: {err}"
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
