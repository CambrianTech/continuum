//! `Connection` — substrate-side state for one positron client
//! session.
//!
//! One transport connection (WebSocket, UDS, airc subscription) maps
//! to one `Connection` value. It owns:
//!
//! - The current [`Subscription`] (set by the latest `Subscribe`
//!   frame; declarative-replace per positron protocol).
//! - The current observer registrations, keyed by `observer_id` (a
//!   single connection can host multiple observers; re-`Observe`
//!   under the same id REPLACES that observer per protocol).
//!
//! ## What this slice ships (2D-2)
//!
//! Pure synchronous handlers that route a `ClientMessage` through
//! the existing `apply_subscribe` / `apply_observe` / `apply_command`
//! functions and update the connection's state accordingly.
//!
//! [`Connection::handle`] is the single dispatch point — match on
//! the variant, delegate to the right handler, return the
//! `Vec<ServerMessage>` the substrate must emit. Per
//! `[[no-fallbacks-ever]]`: every variant is handled explicitly; an
//! unknown variant (forward-compat frame from a newer client) would
//! be caught at the wire deserialize layer, not silently swallowed
//! here.
//!
//! ## What's deferred to slice 2D-3
//!
//! The async session task — a long-running future that reads
//! `ClientMessage` from the transport's inbound stream, drives
//! `Connection::handle`, attaches `watch::Receiver`s for the
//! Subscription's kinds, fans the live envelopes through
//! `ServerMessage::State` to the transport's outbound sink, and
//! quantizes per-observer `budget_hz`. The state machine in this
//! slice is the building block that task composes; making it
//! testable as a sync state-mutation surface keeps the async loop's
//! logic narrow (just plumbing).

use std::collections::HashMap;

use positron_core::session::{ClientMessage, ServerMessage};
use positron_core::wire::{CommandEnvelope, CommandSource};

use crate::dispatch::CommandDispatch;
use crate::observer::{apply_observe, ObserverRegistration};
use crate::session::{apply_subscribe, Subscription};
use crate::substrate::Substrate;

/// One client session's substrate-recorded state. Cheap to construct
/// fresh; mutated in place as `ClientMessage`s arrive.
#[derive(Debug, Default)]
pub struct Connection {
    /// What this connection's renderer wants. Replaced wholesale on
    /// every `Subscribe`.
    pub subscription: Subscription,
    /// Observers attached to this connection, keyed by
    /// `observer_id`. Re-`Observe` under the same id REPLACES.
    pub observers: HashMap<String, ObserverRegistration>,
}

impl Connection {
    pub fn new() -> Self {
        Self::default()
    }

    /// Apply a `ClientMessage` against the substrate. Returns the
    /// `Vec<ServerMessage>` the substrate must emit (snapshots,
    /// `CommandFailed` on Err). Mutates the connection's
    /// `subscription` / `observers` state per the variant.
    ///
    /// Single dispatch point — the variant match is exhaustive per
    /// `[[no-fallbacks-ever]]`. Async because `apply_command` is
    /// async (the dispatcher may be remote).
    pub async fn handle<D: CommandDispatch + ?Sized>(
        &mut self,
        msg: ClientMessage,
        substrate: &Substrate,
        dispatcher: &D,
    ) -> Result<Vec<ServerMessage>, String> {
        match &msg {
            ClientMessage::Subscribe { .. } => self.handle_subscribe(msg, substrate),
            ClientMessage::Observe { .. } => self.handle_observe(msg, substrate),
            ClientMessage::Command(_) => self.handle_command(msg, dispatcher).await,
        }
    }

    /// Handle a `Subscribe` frame. Replaces `self.subscription`
    /// per the declarative-replace doctrine; returns snapshot
    /// frames per the snapshot-then-live + exact-equality-skip
    /// rule.
    pub fn handle_subscribe(
        &mut self,
        msg: ClientMessage,
        substrate: &Substrate,
    ) -> Result<Vec<ServerMessage>, String> {
        let (sub, frames) = apply_subscribe(substrate.cache(), msg)?;
        self.subscription = sub;
        Ok(frames)
    }

    /// Handle an `Observe` frame. Inserts (replaces) the observer's
    /// registration under its `observer_id`; returns snapshot
    /// frames per the same resync contract.
    pub fn handle_observe(
        &mut self,
        msg: ClientMessage,
        substrate: &Substrate,
    ) -> Result<Vec<ServerMessage>, String> {
        let (reg, frames) = apply_observe(substrate.cache(), msg)?;
        // HashMap::insert replaces the prior value if any — exactly
        // the protocol's declarative-replace semantics for observers.
        self.observers.insert(reg.observer_id.clone(), reg);
        Ok(frames)
    }

    /// Handle a `Command` frame. Returns the (possibly-empty)
    /// `Vec<ServerMessage>`: empty on success (state change is the
    /// implicit ack), one `CommandFailed` on Err. Per positron
    /// protocol §"Delivery scope": the caller (slice 2D-3 async
    /// session task) is responsible for emitting these frames ONLY
    /// to this connection.
    pub async fn handle_command<D: CommandDispatch + ?Sized>(
        &mut self,
        msg: ClientMessage,
        dispatcher: &D,
    ) -> Result<Vec<ServerMessage>, String> {
        // Authenticate the envelope's declared `source` against THIS
        // connection's own established state BEFORE the command reaches
        // the dispatcher (the source-auth leg of the confused-deputy
        // clamp; see `reject_forged_source`). On the session-routed
        // command path (`run_session` → `Connection::handle`) a source
        // this connection can prove is forged never executes — the
        // forgery is answered with one `CommandFailed` naming the cause,
        // not silently clamped. Per `[[fallbacks-are-illegal-fail-loud]]`.
        //
        // Scope note: today's live WS transport routes commands through
        // the RPC path (`ipc/ws.rs::dispatch_command`), which hardcodes
        // `CallerIdentity::ws(nil)` and never populates `source` at all —
        // so this guard is not yet on that live vector. It closes the
        // vector for the transport that WILL route commands through the
        // session (a UDS / airc-sourced session), which is why the
        // session dispatch carries the whole typed envelope. This is a
        // build-ahead of the same shape as `ExecutorDispatch` being wired
        // but not yet reached on the WS path.
        if let ClientMessage::Command(envelope) = &msg {
            if let Some(rejection) = self.reject_forged_source(envelope) {
                return Ok(vec![rejection]);
            }
        }
        crate::dispatch::apply_command(dispatcher, msg).await
    }

    /// Authenticate a command envelope's `source` against the state this
    /// connection has established, returning `Some(CommandFailed)` when
    /// the source is a forgery this connection can *prove* and `None`
    /// when the source is consistent with what it established.
    ///
    /// This is the shippable leg of the positron source-auth work: a
    /// positron session multiplexes two principals over ONE socket — the
    /// human at the surface and any AI observer perceiving that surface —
    /// so `source` (not `peer_id`) is what distinguishes them, and until
    /// it is authenticated the confused-deputy clamp presumes an HONEST
    /// source (see `CallerSource::PositronObserver` docs in
    /// continuum-core `routing/auth_policy.rs`).
    ///
    /// What this proves TODAY — no socket/GH-auth handshake required,
    /// using only session-local state:
    /// - `Observer { observer_id }` — the id MUST name an observer
    ///   registered on THIS connection via a prior `Observe`. A command
    ///   stamped with an observer_id this connection never registered is
    ///   either an impersonation (spoofing another observer's audit id)
    ///   or an unestablished principal claiming observer authority.
    ///   Reject loud, naming the id. This also makes the `observer_id`
    ///   that drives authorization + audit downstream *verifiable*
    ///   provenance rather than an unchecked client claim. Requiring
    ///   Observe-before-Command is intentional (not an oversight): an
    ///   actor with no established observer registration IS an
    ///   unestablished principal, and it is exactly what makes the id
    ///   verifiable. A future write-only actuator that acts without
    ///   perceiving is therefore a deliberate protocol change, not a bug
    ///   to be patched by relaxing this guard.
    ///
    /// What still awaits the socket/GH-auth handshake (honest scope):
    /// - `Human` — the socket owner. There is no separate registration
    ///   to check it against; on a positron connection the socket IS the
    ///   human. A compromised observer that self-labels `Human` is not
    ///   yet distinguishable here — that leg closes when socket
    ///   authentication binds the human principal (the tracked
    ///   precondition documented at `auth_policy.rs`). Until then this
    ///   is harmless because both `Human` and `Observer` resolve to the
    ///   same `Provisional` trust ceiling; the divergence this guard
    ///   protects activates the moment that ceiling elevates.
    fn reject_forged_source(&self, envelope: &CommandEnvelope) -> Option<ServerMessage> {
        match &envelope.source {
            // SECURITY TRIPWIRE (task #29): this `None` is safe ONLY
            // while the `Ws` trust ceiling equals `Provisional` — i.e.
            // a compromised observer self-labeling `Human` gets
            // `Ws`→Provisional, no better than `PositronObserver`→
            // Provisional. That equality is owned by continuum-core
            // `routing/auth_policy.rs` / the grid trust policy, OUTSIDE
            // this crate, and nothing here fails loud if it diverges.
            // The moment task #29 elevates the `Ws` ceiling above
            // Provisional (its stated goal), this silent passthrough
            // becomes a live escalation — so the elevation change MUST
            // revisit this arm and bind/verify the human principal here
            // (the socket/GH-auth handshake). Do not remove this arm
            // without that binding.
            CommandSource::Human => None,
            CommandSource::Observer { observer_id } => {
                if self.observers.contains_key(observer_id) {
                    None
                } else {
                    Some(ServerMessage::CommandFailed {
                        correlation_id: envelope.correlation_id,
                        error: format!(
                            "forged source: observer '{observer_id}' is not registered on \
                             this connection — an observer must Observe before it can act"
                        ),
                    })
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use positron_core::session::KindRevision;
    use positron_core::wire::{
        CommandEnvelope, CommandSource, ObserverSpec, StateEnvelope, StateLayer,
    };
    use std::sync::Mutex;
    use uuid::Uuid;

    fn envelope(kind: &str, revision: u64) -> StateEnvelope {
        StateEnvelope {
            kind: kind.to_string(),
            revision: Some(revision),
            layer: StateLayer::Session,
            payload: serde_json::json!({"rev": revision}),
        }
    }

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
        fn call_count(&self) -> usize {
            self.calls.lock().unwrap().len()
        }
    }

    #[async_trait]
    impl CommandDispatch for ScriptedDispatcher {
        async fn dispatch(&self, envelope: CommandEnvelope) -> Result<(), String> {
            self.calls.lock().unwrap().push(envelope);
            self.outcome.clone()
        }
    }

    #[tokio::test]
    async fn subscribe_updates_state_and_emits_snapshot() {
        // what this catches: regression where handle_subscribe
        // forgets to write the new subscription state into self,
        // OR forgets to emit the snapshot frames. Both halves must
        // happen.
        let substrate = Substrate::new();
        substrate.store(envelope("chat", 1));

        let mut conn = Connection::new();
        let frames = conn
            .handle_subscribe(
                ClientMessage::Subscribe {
                    kinds: vec!["chat".into()],
                    layers: vec![StateLayer::Session],
                    last_seen: vec![],
                },
                &substrate,
            )
            .unwrap();
        assert_eq!(frames.len(), 1, "snapshot frame emitted");
        assert!(conn.subscription.covers("chat", StateLayer::Session));
    }

    #[tokio::test]
    async fn resubscribe_replaces_not_merges() {
        // what this catches: regression where the connection's
        // subscription accumulates kinds across Subscribes. Per
        // positron protocol §"Subscribe is declarative (replace,
        // not merge)" the new set REPLACES the old.
        let substrate = Substrate::new();
        substrate.store(envelope("chat", 1));
        substrate.store(envelope("user-list", 1));

        let mut conn = Connection::new();
        conn.handle_subscribe(
            ClientMessage::Subscribe {
                kinds: vec!["chat".into()],
                layers: vec![StateLayer::Session],
                last_seen: vec![],
            },
            &substrate,
        )
        .unwrap();
        assert!(conn.subscription.kinds.contains("chat"));

        // Re-subscribe with a NEW set; "chat" should disappear.
        conn.handle_subscribe(
            ClientMessage::Subscribe {
                kinds: vec!["user-list".into()],
                layers: vec![StateLayer::Session],
                last_seen: vec![],
            },
            &substrate,
        )
        .unwrap();
        assert!(conn.subscription.kinds.contains("user-list"));
        assert!(
            !conn.subscription.kinds.contains("chat"),
            "old subscription must be replaced, not merged"
        );
    }

    #[tokio::test]
    async fn observe_inserts_observer_by_id() {
        // what this catches: regression where the observers map key
        // changes (e.g. accidentally keying off kinds instead of
        // observer_id). Per protocol, re-Observe under the same
        // observer_id REPLACES that observer; under a different id
        // adds a parallel one.
        let substrate = Substrate::new();
        substrate.store(envelope("chat", 1));

        let mut conn = Connection::new();
        let frames = conn
            .handle_observe(
                ClientMessage::Observe {
                    spec: ObserverSpec {
                        observer_id: "maya".into(),
                        budget_hz: 4,
                        kinds: vec!["chat".into()],
                        layers: vec![StateLayer::Session],
                    },
                    last_seen: vec![],
                },
                &substrate,
            )
            .unwrap();
        assert_eq!(frames.len(), 1);
        assert_eq!(conn.observers.len(), 1);
        assert!(conn.observers.contains_key("maya"));
        assert_eq!(conn.observers["maya"].budget_hz, 4);
    }

    #[tokio::test]
    async fn reobserve_under_same_id_replaces_not_adds() {
        // what this catches: regression where the substrate
        // accumulates observers under the same id. Per protocol
        // §"Observers resync identically" + declarative replace —
        // re-Observe replaces.
        let substrate = Substrate::new();
        substrate.store(envelope("chat", 1));

        let mut conn = Connection::new();
        let spec = |budget_hz: u32, kinds: Vec<String>| ObserverSpec {
            observer_id: "maya".into(),
            budget_hz,
            kinds,
            layers: vec![StateLayer::Session],
        };

        conn.handle_observe(
            ClientMessage::Observe {
                spec: spec(4, vec!["chat".into()]),
                last_seen: vec![],
            },
            &substrate,
        )
        .unwrap();
        assert_eq!(conn.observers["maya"].budget_hz, 4);

        // Re-observe at a higher budget. State must be REPLACED.
        conn.handle_observe(
            ClientMessage::Observe {
                spec: spec(10, vec!["chat".into()]),
                last_seen: vec![],
            },
            &substrate,
        )
        .unwrap();
        assert_eq!(conn.observers.len(), 1, "still one observer entry");
        assert_eq!(
            conn.observers["maya"].budget_hz, 10,
            "budget updated to latest spec"
        );
    }

    #[tokio::test]
    async fn multiple_observer_ids_coexist() {
        let substrate = Substrate::new();
        substrate.store(envelope("chat", 1));

        let mut conn = Connection::new();
        for id in ["maya", "helper", "coder"] {
            conn.handle_observe(
                ClientMessage::Observe {
                    spec: ObserverSpec {
                        observer_id: id.to_string(),
                        budget_hz: 4,
                        kinds: vec!["chat".into()],
                        layers: vec![StateLayer::Session],
                    },
                    last_seen: vec![],
                },
                &substrate,
            )
            .unwrap();
        }
        assert_eq!(
            conn.observers.len(),
            3,
            "distinct observer_ids each get a registration"
        );
    }

    #[tokio::test]
    async fn command_success_emits_no_protocol_frame_in_connection_too() {
        // what this catches: regression where the connection wrapper
        // around apply_command adds a success-ack frame, breaking
        // the unidirectional model at the connection layer even if
        // apply_command itself is correct.
        let dispatcher = ScriptedDispatcher::ok();
        let mut conn = Connection::new();
        let frames = conn
            .handle_command(
                ClientMessage::Command(CommandEnvelope {
                    kind: "chat".into(),
                    command: "chat/send".into(),
                    params: serde_json::json!({"text": "hi"}),
                    correlation_id: Uuid::nil(),
                    source: CommandSource::Human,
                }),
                &dispatcher,
            )
            .await
            .unwrap();
        assert!(frames.is_empty());
        assert_eq!(dispatcher.call_count(), 1);
    }

    #[tokio::test]
    async fn command_failure_emits_command_failed_at_connection_layer() {
        // what this catches: regression where the connection wrapper
        // swallows the CommandFailed frame. Per §"loud failures"
        // every layer must propagate it to the eventual transport
        // emit.
        let dispatcher = ScriptedDispatcher::err("nope");
        let mut conn = Connection::new();
        let cid = Uuid::from_u128(0xfee1);
        let frames = conn
            .handle_command(
                ClientMessage::Command(CommandEnvelope {
                    kind: "chat".into(),
                    command: "chat/send".into(),
                    params: serde_json::json!({}),
                    correlation_id: cid,
                    source: CommandSource::Human,
                }),
                &dispatcher,
            )
            .await
            .unwrap();
        assert_eq!(frames.len(), 1);
        match &frames[0] {
            ServerMessage::CommandFailed {
                correlation_id,
                error,
            } => {
                assert_eq!(*correlation_id, cid);
                assert_eq!(error, "nope");
            }
            other => panic!("expected CommandFailed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn command_with_unregistered_observer_source_is_rejected_loud() {
        // what this catches: the confused-deputy forge vector — a command
        // stamped `source: Observer { observer_id }` for an id this
        // connection never registered (no prior Observe) must be REJECTED
        // with a loud CommandFailed and must NEVER reach the dispatcher.
        // Regression here would let any client impersonate an observer's
        // audit id or claim observer authority it never established.
        let dispatcher = ScriptedDispatcher::ok();
        let mut conn = Connection::new();
        let cid = Uuid::from_u128(0xf0f0);
        let frames = conn
            .handle_command(
                ClientMessage::Command(CommandEnvelope {
                    kind: "chat".into(),
                    command: "chat/send".into(),
                    params: serde_json::json!({"text": "as maya"}),
                    correlation_id: cid,
                    source: CommandSource::Observer {
                        observer_id: "maya".into(),
                    },
                }),
                &dispatcher,
            )
            .await
            .unwrap();
        assert_eq!(frames.len(), 1, "one CommandFailed frame emitted");
        match &frames[0] {
            ServerMessage::CommandFailed {
                correlation_id,
                error,
            } => {
                assert_eq!(*correlation_id, cid);
                assert!(
                    error.contains("forged source") && error.contains("maya"),
                    "error must name the forged source + the offending id, got: {error}"
                );
            }
            other => panic!("expected CommandFailed, got {other:?}"),
        }
        assert_eq!(
            dispatcher.call_count(),
            0,
            "the forged command must NEVER reach the dispatcher"
        );
    }

    #[tokio::test]
    async fn command_with_registered_observer_source_passes_after_observe() {
        // what this catches: the source-auth guard must not over-reject —
        // an observer that Observed first is an established principal, so a
        // command stamped with its registered observer_id must pass through
        // to the dispatcher normally.
        let substrate = Substrate::new();
        substrate.store(envelope("chat", 1));
        let dispatcher = ScriptedDispatcher::ok();
        let mut conn = Connection::new();

        // Establish the observer principal on this connection.
        conn.handle_observe(
            ClientMessage::Observe {
                spec: ObserverSpec {
                    observer_id: "maya".into(),
                    budget_hz: 4,
                    kinds: vec!["chat".into()],
                    layers: vec![StateLayer::Session],
                },
                last_seen: vec![],
            },
            &substrate,
        )
        .unwrap();

        // Now maya acts — her registered source authenticates, so the
        // command dispatches (empty frames on success).
        let frames = conn
            .handle_command(
                ClientMessage::Command(CommandEnvelope {
                    kind: "chat".into(),
                    command: "chat/send".into(),
                    params: serde_json::json!({"text": "hi from maya"}),
                    correlation_id: Uuid::nil(),
                    source: CommandSource::Observer {
                        observer_id: "maya".into(),
                    },
                }),
                &dispatcher,
            )
            .await
            .unwrap();
        assert!(frames.is_empty(), "registered observer command succeeds");
        assert_eq!(
            dispatcher.call_count(),
            1,
            "authenticated observer command reaches the dispatcher"
        );
    }

    #[tokio::test]
    async fn forged_observer_command_is_rejected_through_the_handle_entry_point() {
        // what this catches: the source-auth guard must fire on the REAL
        // production entry point — `Connection::handle` routing a
        // `ClientMessage::Command` — not only when `handle_command` is
        // called directly. A refactor that routes Command around the
        // guard (or reorders it after apply_command) would pass the
        // direct-call tests while reopening the forge hole; this pins the
        // wiring through `handle`.
        let substrate = Substrate::new();
        substrate.store(envelope("chat", 1));
        let dispatcher = ScriptedDispatcher::ok();
        let mut conn = Connection::new();
        let cid = Uuid::from_u128(0xbeef);

        let frames = conn
            .handle(
                ClientMessage::Command(CommandEnvelope {
                    kind: "chat".into(),
                    command: "chat/send".into(),
                    params: serde_json::json!({"text": "as ghost"}),
                    correlation_id: cid,
                    source: CommandSource::Observer {
                        observer_id: "ghost".into(),
                    },
                }),
                &substrate,
                &dispatcher,
            )
            .await
            .unwrap();
        assert_eq!(frames.len(), 1, "one CommandFailed through handle");
        match &frames[0] {
            ServerMessage::CommandFailed {
                correlation_id,
                error,
            } => {
                assert_eq!(*correlation_id, cid);
                assert!(error.contains("forged source") && error.contains("ghost"));
            }
            other => panic!("expected CommandFailed, got {other:?}"),
        }
        assert_eq!(
            dispatcher.call_count(),
            0,
            "forged command must never reach the dispatcher via handle either"
        );
    }

    #[tokio::test]
    async fn handle_dispatches_each_variant_correctly() {
        // what this catches: regression where Connection::handle
        // routes the wrong variant to the wrong handler (e.g.
        // Subscribe → handle_command). The single dispatch point
        // must be exhaustive per [[no-fallbacks-ever]] AND
        // correctly-routed.
        let substrate = Substrate::new();
        substrate.store(envelope("chat", 1));
        let dispatcher = ScriptedDispatcher::ok();
        let mut conn = Connection::new();

        // Subscribe → updates subscription + returns snapshot.
        let frames = conn
            .handle(
                ClientMessage::Subscribe {
                    kinds: vec!["chat".into()],
                    layers: vec![StateLayer::Session],
                    last_seen: vec![],
                },
                &substrate,
                &dispatcher,
            )
            .await
            .unwrap();
        assert_eq!(frames.len(), 1);
        assert!(conn.subscription.covers("chat", StateLayer::Session));

        // Observe → adds observer + returns snapshot.
        let frames = conn
            .handle(
                ClientMessage::Observe {
                    spec: ObserverSpec {
                        observer_id: "maya".into(),
                        budget_hz: 4,
                        kinds: vec!["chat".into()],
                        layers: vec![StateLayer::Session],
                    },
                    last_seen: vec![],
                },
                &substrate,
                &dispatcher,
            )
            .await
            .unwrap();
        assert_eq!(frames.len(), 1);
        assert!(conn.observers.contains_key("maya"));

        // Command success → empty Vec.
        let frames = conn
            .handle(
                ClientMessage::Command(CommandEnvelope {
                    kind: "chat".into(),
                    command: "chat/send".into(),
                    params: serde_json::json!({}),
                    correlation_id: Uuid::nil(),
                    source: CommandSource::Human,
                }),
                &substrate,
                &dispatcher,
            )
            .await
            .unwrap();
        assert!(frames.is_empty());
        assert_eq!(dispatcher.call_count(), 1);
    }

    #[tokio::test]
    async fn subscribe_with_matching_last_seen_skips_through_connection_layer() {
        // what this catches: regression where the connection layer's
        // handle_subscribe ignores last_seen. The skip rule MUST
        // propagate from apply_subscribe through to the frames
        // returned to the transport.
        let substrate = Substrate::new();
        substrate.store(envelope("chat", 7));

        let mut conn = Connection::new();
        let frames = conn
            .handle_subscribe(
                ClientMessage::Subscribe {
                    kinds: vec!["chat".into()],
                    layers: vec![StateLayer::Session],
                    last_seen: vec![KindRevision {
                        kind: "chat".into(),
                        revision: 7,
                    }],
                },
                &substrate,
            )
            .unwrap();
        assert!(
            frames.is_empty(),
            "exact-equality skip rule must propagate through Connection"
        );
        assert!(
            conn.subscription.covers("chat", StateLayer::Session),
            "subscription state still updates even when snapshot skipped"
        );
    }
}
