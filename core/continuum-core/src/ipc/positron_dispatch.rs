//! Production [`CommandDispatch`] — the substrate-side seam that lets a
//! positron session run a `CommandEnvelope` through continuum's real
//! command surface (task #29, slice 2D-3).
//!
//! continuum-positron defines the [`CommandDispatch`] trait but cannot
//! depend on continuum-core (that would be a cycle:
//! continuum-positron → continuum-core → continuum-positron). So the
//! concrete bridge lives HERE, on the continuum-core side of the seam,
//! wrapping the same [`CommandRequestHandler::execute_command_request`]
//! owner every other ingress uses ([[the-compression-principle]] — one
//! dispatch owner, no forked command path).
//!
//! ## Where this is (and isn't) exercised
//!
//! On the WS transport, a `WsClientMessage::Command` rides the RPC path
//! (correlation-matched `WsServerMessage::Response`) because the client's
//! `execute()` awaits that reply — positron's session protocol has no
//! success ack (see `ipc::ws` module docs). So `run_session` on the WS
//! transport is fed only `Subscribe`/`Observe`, never a `Command`, and
//! this dispatcher is not reached on that path today.
//!
//! It is wired regardless because it is the *real* command surface, not
//! a stub: `run_session` requires a [`CommandDispatch`], and a future
//! positron transport that DOES route commands through the session (a
//! UDS binding, an airc-sourced session) gets a correct dispatcher for
//! free. Per [[fallbacks-are-illegal-fail-loud]], the honest wiring is
//! the real executor — not a panicking placeholder that pretends the
//! path is unreachable.

use std::sync::Arc;

use async_trait::async_trait;
use continuum_airc_protocol::AircCommandRequest;
use continuum_positron::{CommandDispatch, CommandEnvelope, CommandSource};

use crate::identity::PeerId;
use crate::routing::{CallerIdentity, CommandRequestHandler};
use crate::runtime::CommandExecutor;

/// Map a positron [`CommandSource`] onto the [`CallerIdentity`] the dispatch
/// owner gates against — the confused-deputy seam.
///
/// A positron session multiplexes two principals over ONE socket: the human at
/// the surface, and any AI **observer** perceiving that surface. They must not
/// share authority. Both ride the same anonymous socket today (a nil peer_id,
/// pre-GH-auth), so the peer_id can't distinguish them — the SOURCE does:
///
/// - [`Human`](CommandSource::Human) → [`CallerIdentity::ws`]: the socket's own
///   unauthenticated identity, which a future GH-auth handshake (task #29)
///   elevates through the trust bridge.
/// - [`Observer`](CommandSource::Observer) → [`CallerIdentity::positron_observer`]:
///   an attenuated identity clamped at Provisional that NEVER rises with socket
///   auth, so the AI observing a human's screen can never inherit the human's
///   elevated authority (see
///   [`CallerSource::PositronObserver`](crate::routing::CallerSource::PositronObserver)).
///
/// Pure + total so the security-relevant decision is unit-tested in isolation,
/// not buried behind an executor.
fn caller_for_source(source: CommandSource) -> CallerIdentity {
    // The same anonymous socket peer for both — trust diverges on the source,
    // never the peer_id.
    let socket_peer = PeerId::from_uuid(uuid::Uuid::nil());
    match source {
        CommandSource::Human => CallerIdentity::ws(socket_peer),
        CommandSource::Observer { observer_id } => {
            CallerIdentity::positron_observer(socket_peer, observer_id)
        }
    }
}

/// Bridges a positron [`CommandEnvelope`] onto continuum's command
/// surface. Holds the same `Arc<CommandExecutor>` the WS RPC path and
/// the airc peer path dispatch through.
pub struct ExecutorDispatch {
    executor: Arc<CommandExecutor>,
}

impl ExecutorDispatch {
    pub fn new(executor: Arc<CommandExecutor>) -> Self {
        Self { executor }
    }
}

#[async_trait]
impl CommandDispatch for ExecutorDispatch {
    async fn dispatch(&self, envelope: CommandEnvelope) -> Result<(), String> {
        // Map the positron envelope onto the wire request the dispatch
        // owner takes. `command` is the command path, `kind` is the
        // state kind it mutates; positron carries no `env` selector, so
        // it's absent (not defaulted to a guess — [[fallbacks-are-illegal]]).
        let request =
            AircCommandRequest::new(envelope.command, envelope.kind, None, envelope.params);

        // Trust comes from the envelope's SOURCE, not the socket: a human keeps
        // the socket's own (unauthenticated) Ws identity; an AI observer gets an
        // attenuated identity clamped strictly below any authority that socket
        // can ever reach — the confused-deputy defense (see `caller_for_source`).
        let caller = caller_for_source(envelope.source);

        let response =
            CommandRequestHandler::execute_command_request(&self.executor, &request, caller).await;

        // positron's contract: Ok(()) → the resulting state change IS
        // the ack (streams down as State); Err(msg) → the session emits
        // one CommandFailed. Discard the success Value: the ack is the
        // state mutation, not a return payload.
        response.into_result().map(|_value| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::CallerSource;

    // what this catches: the confused-deputy seam at the positron→continuum
    // boundary. A `Human` envelope must carry the socket's own `Ws` identity (which
    // GH-auth can later elevate), and an `Observer` envelope must carry an
    // ATTENUATED `PositronObserver` identity — a DIFFERENT source, so the trust
    // policy clamps it below the human (grid_trust_policy proves the ceiling half).
    // A regression that collapsed both to `ws(nil)` — the pre-attenuation behavior —
    // would let an AI observing a human's screen inherit the human's authority the
    // moment the socket authenticates. The observer_id must also survive for audit.
    #[test]
    fn observer_and_human_sources_map_to_distinct_attenuated_identities() {
        let human = caller_for_source(CommandSource::Human);
        assert!(
            matches!(human.source, CallerSource::Ws),
            "a human command keeps the socket's own Ws identity"
        );

        let observer = caller_for_source(CommandSource::Observer {
            observer_id: "asha-brain".to_string(),
        });
        assert!(
            matches!(&observer.source, CallerSource::PositronObserver { observer_id } if observer_id == "asha-brain"),
            "an observer command is stamped PositronObserver, carrying the audit id"
        );

        // Both ride the same anonymous socket peer today — the SOURCE is the only
        // thing that diverges the trust ceiling, never the peer_id.
        assert_eq!(
            human.peer_id, observer.peer_id,
            "same socket, different principal"
        );
        assert!(
            !matches!(observer.source, CallerSource::Ws),
            "the observer must NOT be indistinguishable from the human socket"
        );
    }
}
