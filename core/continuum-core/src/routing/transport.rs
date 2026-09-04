//! `Transport` — the substrate's cross-grid dispatch seam.
//!
//! Per `docs/architecture/GRID-ADDRESSING-AND-ROUTING.md` §"Transport
//! selector": every non-Local [`RouteDecision`] is handed to a
//! `Transport` impl. The dispatcher's match shape stays clean —
//! Local stays inline (it operates on this substrate's owned
//! modules + interceptors), and every Peer / Room / Broadcast
//! variant routes through the same trait method.
//!
//! ## What this commit lands
//!
//! - The `Transport` trait
//! - [`NotImplementedRemoteTransport`] — the substrate's default,
//!   producing typed errors per variant that name the specific
//!   missing transport
//! - [`ClosureTransport`] — fixture-grade impl that delegates to a
//!   closure, used by tests
//!
//! ## What the next commit lands
//!
//! - **AircTransport** — real cross-grid dispatch. Peer / Room /
//!   Broadcast envelopes constructed, signed with the local
//!   substrate's airc identity, sent through the existing
//!   [`AircCitizen`](crate::persona::AircCitizen) primitive, awaited
//!   and unpacked.
//!
//! ## Why only the remote side has a trait
//!
//! The Local arm operates on this substrate's owned state —
//! `ModuleRegistry`, `Vec<CommandInterceptor>`, the TS bridge
//! socket. Extracting it behind a trait would require either
//! circular references back to `CommandExecutor` or rebuilding
//! the transport every time an interceptor is added. The
//! refactoring earnings are tiny and the surface gets messier.
//!
//! By contrast, the cross-grid transport is independent of those
//! local concerns — its dispatch is a self-contained
//! envelope-and-await against airc. A trait there cleanly
//! abstracts "this peer's state isn't local; ask airc to deliver
//! the call to wherever the peer lives."
//!
//! When the substrate gains a second non-Local concern (a grid
//! capability transport, an HTTP bridge to legacy systems, etc.),
//! either chain them like the interceptors or — more likely —
//! introduce a transport selector that dispatches by
//! [`RouteKind`](super::RouteKind). For now: one trait, one slot.
//!
//! ## Caller identity
//!
//! Today's trait method doesn't take a [`CallerIdentity`]. The
//! local arm doesn't need one (this substrate's own code is
//! implicitly trusted by the default policy); the remote arm
//! doesn't carry one outbound (the substrate IS the caller from
//! the remote peer's perspective). When the
//! [`AircTransport`] commit lands, it adds caller propagation
//! ON THE INBOUND side — extracting the verified sender from
//! the airc envelope and threading it into the gate.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use super::RouteDecision;
use crate::runtime::CommandResult;

/// The substrate's transport dispatch trait. Implementations route
/// non-Local [`RouteDecision`] variants to wherever the work actually
/// lives — another peer over airc, a room of subscribers, etc.
///
/// `Send + Sync` because transports live behind `Arc<dyn Transport>`
/// in [`CommandExecutor`](crate::runtime::CommandExecutor).
///
/// `Debug` so tests + diagnostics can name the transport in
/// assertions and error messages.
#[async_trait]
pub trait Transport: Send + Sync + std::fmt::Debug {
    /// Dispatch the routed decision. The Local case never reaches
    /// here — `CommandExecutor::dispatch` handles it inline against
    /// its owned modules. Implementations only see
    /// [`RouteDecision::Peer`] / [`RouteDecision::Room`] /
    /// [`RouteDecision::Broadcast`].
    ///
    /// The implementation OWNS the decision + params (no borrows
    /// across `.await` for the dispatcher to manage) and returns
    /// the canonical [`CommandResult`] shape every other command
    /// produces. Errors propagate as the substrate's standard
    /// `String` result.
    async fn dispatch(
        &self,
        decision: RouteDecision,
        params: Value,
    ) -> Result<CommandResult, String>;
}

/// The substrate's default cross-grid transport — returns typed
/// not-yet-implemented errors that name the specific missing
/// transport. Same shape the previous inline match produced; the
/// trait extraction preserves the error contract while moving the
/// formatting behind one type.
///
/// Operators install a real transport ([`AircTransport`] in the
/// next commit) at boot via
/// [`CommandExecutor::with_remote_transport`].
#[derive(Debug, Default, Clone, Copy)]
pub struct NotImplementedRemoteTransport;

#[async_trait]
impl Transport for NotImplementedRemoteTransport {
    async fn dispatch(
        &self,
        decision: RouteDecision,
        _params: Value,
    ) -> Result<CommandResult, String> {
        match decision {
            RouteDecision::Local { .. } => Err(
                "BUG: NotImplementedRemoteTransport received a Local decision — \
                 the dispatcher handles Local inline; remote transports never \
                 see this variant."
                    .to_string(),
            ),
            RouteDecision::Peer {
                peer,
                node,
                env,
                path,
                ..
            } => Err(format!(
                "Peer dispatch not yet implemented — \
                 AircTransport lands in a subsequent Slice P commit. \
                 Routing was: peer={peer:?}, node={node:?}, env={env:?}, path={path}"
            )),
            RouteDecision::Room {
                room_id, env, path, ..
            } => Err(format!(
                "Room broadcast not yet implemented — \
                 AircTransport lands in a subsequent Slice P commit. \
                 Routing was: room={room_id}, env={env:?}, path={path}"
            )),
            RouteDecision::Broadcast {
                peer, node, path, ..
            } => Err(format!(
                "Env-wildcard broadcast not yet implemented — \
                 AircTransport lands in a subsequent Slice P commit. \
                 Routing was: peer={peer:?}, node={node:?}, path={path}"
            )),
        }
    }
}

/// A closure-backed transport useful for tests that want to assert
/// "the dispatcher routed a Peer decision to whatever-transport
/// without it being NotImplemented" without spinning up a real
/// airc transport.
pub struct ClosureTransport {
    name: &'static str,
    f: Arc<dyn Fn(RouteDecision, Value) -> Result<CommandResult, String> + Send + Sync>,
}

impl ClosureTransport {
    pub fn new(
        name: &'static str,
        f: impl Fn(RouteDecision, Value) -> Result<CommandResult, String> + Send + Sync + 'static,
    ) -> Self {
        Self {
            name,
            f: Arc::new(f),
        }
    }
}

impl std::fmt::Debug for ClosureTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClosureTransport")
            .field("name", &self.name)
            .finish()
    }
}

#[async_trait]
impl Transport for ClosureTransport {
    async fn dispatch(
        &self,
        decision: RouteDecision,
        params: Value,
    ) -> Result<CommandResult, String> {
        (self.f)(decision, params)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::{route, CommandUri, PeerRef};
    use uuid::Uuid;

    fn peer_decision() -> RouteDecision {
        route(&CommandUri::parse("airc://maya/inference/llm/generate").expect("peer"))
    }

    fn room_decision() -> RouteDecision {
        let room_id = Uuid::new_v4();
        route(&CommandUri::parse(&format!("airc://room:{room_id}/chat/post")).expect("room"))
    }

    fn broadcast_decision() -> RouteDecision {
        route(&CommandUri::parse("airc://maya:*/notification/send").expect("broadcast"))
    }

    #[tokio::test]
    async fn not_implemented_returns_peer_specific_error() {
        let t = NotImplementedRemoteTransport;
        let err = t
            .dispatch(peer_decision(), Value::Null)
            .await
            .expect_err("not-implemented must error");
        assert!(err.contains("Peer dispatch"), "error must name Peer: {err}");
        assert!(err.contains("maya"), "error must name the peer: {err}");
        assert!(
            err.contains("AircTransport"),
            "error must name the future transport for grep-ability: {err}"
        );
    }

    #[tokio::test]
    async fn not_implemented_returns_room_specific_error() {
        let t = NotImplementedRemoteTransport;
        let err = t
            .dispatch(room_decision(), Value::Null)
            .await
            .expect_err("not-implemented must error");
        assert!(
            err.contains("Room broadcast"),
            "error must name Room: {err}"
        );
    }

    #[tokio::test]
    async fn not_implemented_returns_broadcast_specific_error() {
        let t = NotImplementedRemoteTransport;
        let err = t
            .dispatch(broadcast_decision(), Value::Null)
            .await
            .expect_err("not-implemented must error");
        assert!(
            err.contains("Env-wildcard broadcast"),
            "error must name Broadcast: {err}"
        );
    }

    #[tokio::test]
    async fn not_implemented_receiving_local_is_a_bug() {
        // The dispatcher routes Local INLINE and never calls a
        // remote transport for it. If a future refactor breaks that
        // invariant, the transport returns a loud BUG error rather
        // than silently ignoring the decision.
        let t = NotImplementedRemoteTransport;
        let local = route(&CommandUri::local("x/y"));
        let err = t
            .dispatch(local, Value::Null)
            .await
            .expect_err("Local decision must surface as a bug");
        assert!(err.contains("BUG"), "error must name the bug: {err}");
    }

    #[tokio::test]
    async fn closure_transport_invokes_inner_function() {
        // Build a transport that produces a recognizable response so
        // tests can assert "the trait method ran" without doing real
        // routing.
        let t = ClosureTransport::new("test", |decision, _params| {
            let path = decision.path().to_string();
            Ok(CommandResult::Json(serde_json::json!({
                "routed": true,
                "path": path,
            })))
        });

        let result = t
            .dispatch(peer_decision(), Value::Null)
            .await
            .expect("closure transport succeeds");

        match result {
            CommandResult::Json(v) => {
                assert_eq!(v["routed"], true);
                assert_eq!(v["path"], "inference/llm/generate");
            }
            other => panic!("expected Json, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn closure_transport_can_inspect_routing_fields() {
        // Prove the closure receives the typed routing fields — not
        // just the path. This is the property that lets AircTransport
        // pick the right peer/node/env when it lands.
        let captured_peer = Arc::new(std::sync::Mutex::new(None));
        let captured_peer_clone = captured_peer.clone();

        let t = ClosureTransport::new("inspect", move |decision, _params| {
            if let RouteDecision::Peer { peer, .. } = &decision {
                *captured_peer_clone.lock().unwrap() = Some(peer.clone());
            }
            Ok(CommandResult::Json(Value::Null))
        });

        t.dispatch(peer_decision(), Value::Null).await.unwrap();

        assert_eq!(
            *captured_peer.lock().unwrap(),
            Some(PeerRef::Name("maya".to_string())),
            "transport must see the typed peer field"
        );
    }

    /// The trait must be object-safe so the dispatcher can hold it
    /// behind `Arc<dyn Transport>`. This test won't compile if the
    /// trait stops being object-safe (e.g. someone adds a generic
    /// method).
    #[tokio::test]
    async fn transport_is_object_safe_and_arc_able() {
        let t: Arc<dyn Transport> = Arc::new(NotImplementedRemoteTransport);
        let result = t.dispatch(peer_decision(), Value::Null).await;
        assert!(result.is_err());
    }
}
