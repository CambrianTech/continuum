//! Substrate-side `Command` handler — bridge `CommandEnvelope` to
//! continuum's command surface; emit `ServerMessage::CommandFailed`
//! on failure.
//!
//! ## The asymmetry (v0.1.1)
//!
//! Per positron protocol §`ServerMessage::CommandFailed` doc:
//!
//! > Failures are LOUD — a rejected `CommandEnvelope` must never
//! > vanish silently. Success deliberately has no ack frame: a
//! > successful command's acknowledgement IS the state change it
//! > causes, streaming down as `State` (the unidirectional model).
//! > Consumers correlate via the `correlation_id` they sent.
//!
//! So this handler is intentionally asymmetric:
//!
//! - **Success path**: dispatch returns `Ok(())`. No protocol frame.
//!   The substrate's state mutation reaches the cache, flows through
//!   the live-broadcast layer (slice 2D), and the renderer sees a
//!   new `State` frame with a new revision. That is the ack.
//! - **Failure path**: dispatch returns `Err(error)`. The substrate
//!   emits ONE `ServerMessage::CommandFailed { correlation_id, error }`
//!   targeting ONLY the connection that submitted the failing
//!   command (per protocol §`CommandFailed` "Delivery scope").
//!
//! ## Why a trait, not a concrete executor
//!
//! Continuum's `Commands.execute` lives in the broader substrate
//! (continuum-core's `CommandExecutor`). Wiring that concrete type
//! into this crate would create a circular dependency
//! (continuum-positron → continuum-core → … ) and bind tests to a
//! full substrate boot. The [`CommandDispatch`] trait is the
//! substrate-side seam: production injects a thin wrapper around the
//! real executor; tests inject a scripted mock. Per
//! `[[strong-typing-across-boundaries]]`: the seam is a trait, not
//! a function-typed `Box<dyn Fn>` — adding methods later (e.g.
//! authorization context, command metadata) is a typed extension.
//!
//! ## What's deliberately not here
//!
//! - **No correlation map.** Success doesn't ack via the protocol,
//!   so there's nothing to correlate substrate-side. The renderer
//!   tracks its own outstanding `correlation_id`s; the substrate
//!   just echoes the id back on failure. A correlation map would
//!   only earn its keep when (and if) ack frames land — a v0.x
//!   addition the protocol deliberately deferred.
//! - **No per-connection routing.** This module returns the
//!   frames the substrate should emit; the session-task that owns
//!   the transport handles "send to THIS connection." Per-connection
//!   delivery scope is the transport layer's concern, not this
//!   pure-function handler's.

use async_trait::async_trait;
use positron_core::session::{ClientMessage, ServerMessage};
use positron_core::wire::CommandEnvelope;

/// The substrate-side seam to continuum's command surface.
/// Production implementations wrap continuum-core's
/// `CommandExecutor::execute`; tests inject scripted mocks.
///
/// Contract:
/// - `Ok(())` on success: the substrate's resulting state change is
///   the implicit ack (the unidirectional model).
/// - `Err(message)` on failure: the substrate emits
///   `ServerMessage::CommandFailed { correlation_id, error: message }`
///   on the failing connection.
///
/// ## Why the whole envelope, not `(command, params)`
///
/// Per positron's wire doctrine, `CommandEnvelope` is "first-class but
/// never anonymous" — every call carries its `source` (`Human` vs
/// `Observer { observer_id }`) and the `kind` of state it mutates.
/// Decomposing the envelope at this seam erases that provenance before
/// the dispatcher gets to see it, which kills authorization decisions
/// + audit trails downstream. The seam takes the typed envelope so
/// every implementor sees what the client actually sent. Per
/// `[[strong-typing-across-boundaries]]`.
#[async_trait]
pub trait CommandDispatch: Send + Sync {
    async fn dispatch(&self, envelope: CommandEnvelope) -> Result<(), String>;
}

/// Bridge one `ClientMessage::Command` through a [`CommandDispatch`]
/// implementation. Returns the (possibly-empty) Vec of
/// `ServerMessage`s the substrate must emit to the originating
/// connection.
///
/// Success → empty Vec (state change carries the ack). Failure → one
/// `CommandFailed` frame with the echoed `correlation_id` and the
/// dispatcher's error message.
///
/// Returns `Err` if `msg` is NOT a `Command` variant — single-purpose
/// handler per `[[no-fallbacks-ever]]`. Callers route by variant.
pub async fn apply_command<D: CommandDispatch + ?Sized>(
    dispatcher: &D,
    msg: ClientMessage,
) -> Result<Vec<ServerMessage>, String> {
    let envelope = match msg {
        ClientMessage::Command(e) => e,
        other => {
            return Err(format!(
                "apply_command: expected Command variant, got {other:?}"
            ));
        }
    };
    let correlation_id = envelope.correlation_id;
    match dispatcher.dispatch(envelope).await {
        Ok(()) => Ok(Vec::new()),
        Err(err) => Ok(vec![ServerMessage::CommandFailed {
            correlation_id,
            error: err,
        }]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use positron_core::wire::{CommandEnvelope, CommandSource};
    use std::sync::Mutex;
    use uuid::Uuid;

    /// Records every dispatch call + returns a scripted outcome.
    /// Per the test-fixtures doctrine: keep mocks narrow + scripted
    /// rather than half-implementing the real executor.
    struct ScriptedDispatcher {
        calls: Mutex<Vec<CommandEnvelope>>,
        outcome: Result<(), String>,
    }

    impl ScriptedDispatcher {
        fn ok() -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                outcome: Ok(()),
            }
        }
        fn err(msg: &str) -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                outcome: Err(msg.to_string()),
            }
        }
        fn calls(&self) -> Vec<CommandEnvelope> {
            self.calls.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl CommandDispatch for ScriptedDispatcher {
        async fn dispatch(&self, envelope: CommandEnvelope) -> Result<(), String> {
            self.calls.lock().unwrap().push(envelope);
            self.outcome.clone()
        }
    }

    fn cmd(command: &str, correlation_id: Uuid) -> ClientMessage {
        ClientMessage::Command(CommandEnvelope {
            kind: "chat".into(),
            command: command.to_string(),
            params: serde_json::json!({"text": "hi"}),
            correlation_id,
            source: CommandSource::Human,
        })
    }

    #[tokio::test]
    async fn success_emits_no_protocol_frame() {
        // what this catches: regression where the substrate sends a
        // success-ack frame. Per positron v0.1.1 contract: success
        // has no ack frame — the state mutation IS the ack. Adding
        // a synthetic success frame here would create a second
        // correlate-able event that breaks the unidirectional model.
        let dispatcher = ScriptedDispatcher::ok();
        let cid = Uuid::from_u128(0xabc);
        let frames = apply_command(&dispatcher, cmd("chat/send", cid))
            .await
            .unwrap();
        assert!(
            frames.is_empty(),
            "success → no protocol frame; got {frames:?}"
        );
        let calls = dispatcher.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].command, "chat/send");
    }

    #[tokio::test]
    async fn failure_emits_command_failed_with_echoed_correlation_id() {
        // what this catches: regression where the substrate fails
        // silently (the §"loud failures" doctrine bug class) OR
        // emits a different correlation_id from the one the client
        // sent. Either would break the client's ability to surface
        // "your command failed" UI keyed off the correlation it's
        // tracking.
        let dispatcher = ScriptedDispatcher::err("permission denied");
        let cid = Uuid::from_u128(0xdef);
        let frames = apply_command(&dispatcher, cmd("data/delete", cid))
            .await
            .unwrap();
        assert_eq!(frames.len(), 1);
        match &frames[0] {
            ServerMessage::CommandFailed {
                correlation_id,
                error,
            } => {
                assert_eq!(
                    *correlation_id, cid,
                    "echoed correlation_id must equal the client's"
                );
                assert_eq!(error, "permission denied");
            }
            other => panic!("expected CommandFailed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn refuses_non_command_variant_loudly() {
        let dispatcher = ScriptedDispatcher::ok();
        let err = apply_command(
            &dispatcher,
            ClientMessage::Subscribe {
                kinds: vec!["chat".into()],
                layers: vec![positron_core::wire::StateLayer::Session],
                last_seen: vec![],
            },
        )
        .await
        .unwrap_err();
        assert!(
            err.contains("Command"),
            "error must name the expected variant: {err}"
        );
    }

    #[tokio::test]
    async fn dispatcher_receives_typed_command_and_params() {
        // what this catches: regression where command/params get
        // re-serialized or mangled crossing the dispatch seam.
        let dispatcher = ScriptedDispatcher::ok();
        let env = CommandEnvelope {
            kind: "chat".into(),
            command: "chat/send".into(),
            params: serde_json::json!({"text": "specific text", "room_id": "abc"}),
            correlation_id: Uuid::nil(),
            source: CommandSource::Human,
        };
        let _ = apply_command(&dispatcher, ClientMessage::Command(env))
            .await
            .unwrap();
        let calls = dispatcher.calls();
        assert_eq!(calls[0].command, "chat/send");
        assert_eq!(calls[0].params["text"], "specific text");
        assert_eq!(calls[0].params["room_id"], "abc");
    }

    #[tokio::test]
    async fn dispatcher_receives_envelope_source_and_kind_intact() {
        // what this catches: regression where the dispatch seam erases
        // CommandEnvelope.source or .kind before reaching the
        // dispatcher. Per positron wire doctrine, commands are
        // "first-class but never anonymous" — the source (Human vs
        // Observer { observer_id }) drives authorization and audit
        // downstream; the kind tags WHICH state the command mutates.
        // Decomposing the envelope at this seam would silently strip
        // both, leaving the dispatcher unable to tell a human edit
        // from an AI observer edit and unable to scope authority by
        // kind. Sentinel BLOCK on #1602.
        let dispatcher = ScriptedDispatcher::ok();
        let observer_id = "ares-1".to_string();
        let env = CommandEnvelope {
            kind: "chat".into(),
            command: "chat/send".into(),
            params: serde_json::json!({"text": "from observer"}),
            correlation_id: Uuid::from_u128(0x1234),
            source: CommandSource::Observer {
                observer_id: observer_id.clone(),
            },
        };
        let _ = apply_command(&dispatcher, ClientMessage::Command(env))
            .await
            .unwrap();
        let calls = dispatcher.calls();
        assert_eq!(calls[0].kind, "chat", "kind must round-trip");
        match &calls[0].source {
            CommandSource::Observer { observer_id: got } => {
                assert_eq!(*got, observer_id, "observer_id must round-trip");
            }
            other => panic!("source must round-trip as Observer, got {other:?}"),
        }
    }
}
