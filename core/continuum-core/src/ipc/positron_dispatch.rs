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
use continuum_positron::{CommandDispatch, CommandEnvelope};

use crate::identity::PeerId;
use crate::routing::{CallerIdentity, CommandRequestHandler};
use crate::runtime::CommandExecutor;

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
        let request = AircCommandRequest::new(
            envelope.command,
            envelope.kind,
            None,
            envelope.params,
        );

        // Same Provisional ceiling as every other WS-originated call:
        // trust comes from the source (unauthenticated remote socket),
        // not from the envelope. A later GH-auth handshake raises it.
        let caller = CallerIdentity::ws(PeerId::from_uuid(uuid::Uuid::nil()));

        let response =
            CommandRequestHandler::execute_command_request(&self.executor, &request, caller).await;

        // positron's contract: Ok(()) → the resulting state change IS
        // the ack (streams down as State); Err(msg) → the session emits
        // one CommandFailed. Discard the success Value: the ack is the
        // state mutation, not a return payload.
        response.into_result().map(|_value| ())
    }
}
