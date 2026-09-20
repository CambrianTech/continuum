//! WebSocket framing envelope for the thin-client transport.
//!
//! A single airc peer socket carries one request/response at a time; the
//! thin-client WS socket **multiplexes** — a browser fires N concurrent
//! commands over one connection and matches replies by a correlation id.
//! So the WS wire wraps [`AircCommandRequest`]/[`AircCommandResponse`]
//! (the dispatch shape, owned by `command.rs`) in a framed envelope that
//! carries that id.
//!
//! Both enums are **tagged** (`type` discriminant) and were deliberately
//! single-variant at birth — the tag makes them forward-compatible, exactly
//! as promised: the positron state-subscription frames (`Subscribe` /
//! `Observe` client→server, `State` server→client) now slot in as new
//! variants **without touching** the `Command`/`Response` RPC wire shape. The
//! nested `AircCommandRequest`/`AircCommandResponse` keep their own `status`
//! tag at a deeper nesting level, so there is no discriminant collision.
//!
//! ## Why the state frames are flat sibling variants, not a nested positron enum
//!
//! positron's [`positron_core::session::ClientMessage`] /
//! [`positron_core::session::ServerMessage`] are themselves internally
//! `#[serde(tag = "type")]`. Nesting one of those under a newtype variant of a
//! `tag = "type"` `WsClientMessage` would put two `"type"` keys at the same
//! JSON map level — the exact discriminant collision this envelope was
//! designed to avoid. So the state frames are **flat sibling variants** here
//! (`type: "subscribe"` / `"observe"` / `"state"`, distinct from `"command"` /
//! `"response"`) whose *fields* reference positron's types directly
//! (`StateLayer`, `KindRevision`, `ObserverSpec`, `StateEnvelope`) — one source
//! of truth for the field shapes, no re-mirrored Subscribe/Observe structs.
//! The [`WsClientMessage::to_session`] / [`WsServerMessage::state`] converters
//! below are the mechanical seam onto positron's own frame enums.
//!
//! ## The two command paths coexist (the ack-semantics reconciliation)
//!
//! positron's session protocol has **no success ack** — a successful command's
//! acknowledgement IS the `State` frame it causes. The RPC path
//! (`Command` → `Response`), by contrast, replies to *every* command with a
//! correlation-matched `Response`, which is what the client's `execute()`
//! awaits to resolve. Those two completion models don't merge, so on this
//! transport **commands ride the RPC path** (`Command`/`Response`) and only the
//! *state-subscription* frames (`Subscribe`/`Observe`/`State`) ride the positron
//! path. positron's own `Command`/`CommandFailed` frames are therefore not
//! carried by this envelope — the RPC `Response{status: error}` is how a WS
//! command failure surfaces.
//!
//! One owner of the wire shape: these types derive `TS` and generate the
//! TypeScript mirror the `WebSocketTransport` consumes — never hand-written
//! on the client side.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use positron_core::session::{ClientMessage as PositronClientMessage, KindRevision};
use positron_core::wire::{ObserverSpec, StateEnvelope, StateLayer};

use crate::command::{AircCommandRequest, AircCommandResponse};

/// Client→server WS frame.
///
/// `id` correlates the eventual [`WsServerMessage::Response`]. It is a
/// per-connection monotonic counter minted by the client; the server echoes
/// it back verbatim and never interprets it. `u64` is mapped to a TS `number`
/// (correlation ids never approach 2^53 on a single connection).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export, export_to = "../../../protocol/typescript/transport/WsClientMessage.ts")]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsClientMessage {
    /// Dispatch a command; reply arrives as [`WsServerMessage::Response`]
    /// with the same `id`. This is the RPC path — every command gets a
    /// correlation-matched `Response` (success or error), which is what the
    /// client's `execute()` awaits. positron commands do NOT ride this
    /// envelope (see the module doc's ack-semantics note).
    Command {
        #[ts(type = "number")]
        id: u64,
        request: AircCommandRequest,
    },
    /// Declare (or re-declare, after a reconnect) what state kinds/layers
    /// this connection renders. Maps 1:1 onto positron's
    /// `ClientMessage::Subscribe`; triggers snapshot-then-live with the
    /// exact-equality skip. Declarative-replace: subscribing again is a
    /// resync, never a duplicate live stream.
    Subscribe {
        kinds: Vec<String>,
        // `#[ts(type = ...)]`: positron-core pins ts-rs 10, this crate ts-rs
        // 12 — a v12 derive cannot visit a v10 `TS` impl. Project these
        // positron types by their TS name; the field stays the real Rust
        // type (single source on the Rust side). Import wiring for the names
        // is task #80 (binding-path reconciliation).
        #[ts(type = "Array<StateLayer>")]
        layers: Vec<StateLayer>,
        #[serde(default)]
        #[ts(type = "Array<KindRevision>")]
        last_seen: Vec<KindRevision>,
    },
    /// Register (or re-register) an AI observer on this connection. Maps 1:1
    /// onto positron's `ClientMessage::Observe`; same snapshot-then-live +
    /// exact-equality resync as `Subscribe`, with the observer's `budget_hz`
    /// bounding live cadence.
    Observe {
        #[ts(type = "ObserverSpec")]
        spec: ObserverSpec,
        #[serde(default)]
        #[ts(type = "Array<KindRevision>")]
        last_seen: Vec<KindRevision>,
    },
}

impl WsClientMessage {
    /// Project a state-subscription frame onto positron's own
    /// [`positron_core::session::ClientMessage`] so it can be fed to the
    /// substrate session task. Returns `None` for [`WsClientMessage::Command`]
    /// — commands ride the RPC path on this transport, not the positron
    /// session path (see the module doc). Single-purpose per
    /// `[[no-fallbacks-ever]]`: the caller routes `Command` to the RPC
    /// executor and everything else through this seam.
    pub fn to_session(self) -> Option<PositronClientMessage> {
        match self {
            WsClientMessage::Command { .. } => None,
            WsClientMessage::Subscribe {
                kinds,
                layers,
                last_seen,
            } => Some(PositronClientMessage::Subscribe {
                kinds,
                layers,
                last_seen,
            }),
            WsClientMessage::Observe { spec, last_seen } => {
                Some(PositronClientMessage::Observe { spec, last_seen })
            }
        }
    }
}

/// Server→client WS frame.
///
/// `id` mirrors the request's correlation id so the client can resolve the
/// matching pending promise.
///
/// Not `Eq`: the `State` variant carries a [`StateEnvelope`] whose `payload`
/// is a `serde_json::Value` (only `PartialEq`). `PartialEq` is what the tests
/// and any dedup need; `Eq` was never load-bearing here.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export, export_to = "../../../protocol/typescript/transport/WsServerMessage.ts")]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsServerMessage {
    /// The reply to a [`WsClientMessage::Command`] carrying the same `id`.
    Response {
        #[ts(type = "number")]
        id: u64,
        response: AircCommandResponse,
    },
    /// A pushed state update — snapshot (immediately after `Subscribe` /
    /// `Observe`) or live change. Carries positron's
    /// [`positron_core::wire::StateEnvelope`] verbatim; the newtype mirrors
    /// positron's `ServerMessage::State(StateEnvelope)`. There is no `id`:
    /// state frames are not request/response, the client reconciles by
    /// `envelope.kind` + `envelope.revision`. `StateEnvelope`'s own fields
    /// (`kind`/`revision`/`layer`/`payload`) carry no `"type"` key, so there
    /// is no collision with the outer `type: "state"` tag.
    ///
    /// `#[ts(type = ...)]`: same ts-rs 10↔12 skew as the `Subscribe` fields —
    /// project `StateEnvelope` by TS name (import wiring = task #80).
    State(#[ts(type = "StateEnvelope")] StateEnvelope),
    /// A pushed EPHEMERAL token from a persona's in-progress turn — the live
    /// "typing" surface (#170). NOT durable state: the authoritative message still
    /// arrives as a `chat:posted` transcript row folded into a `State` envelope;
    /// this rail only carries the turn token-by-token so a persona visibly *types*
    /// instead of freezing then dumping a wall of text. Its own ephemeral rail,
    /// matching the airc substrate's Event-class-vs-Message-class split — the
    /// durable `State`/`StateEnvelope` path is untouched.
    ///
    /// Correlated to the eventual durable row by `room_id` + `sender_id` (the
    /// per-turn `stream_id` is minted at stream start and is NOT the final message
    /// id): the client keys a transient bubble on `sender_id`, grows it per token,
    /// and retires it when the durable row from that sender lands OR `done` (the
    /// `text_end` chunk) arrives — whichever comes first.
    StreamDelta {
        room_id: String,
        sender_id: String,
        stream_id: String,
        #[ts(type = "number")]
        seq: u64,
        token: String,
        done: bool,
    },
}

impl WsServerMessage {
    /// Build a `Response` frame for a given correlation id.
    pub fn response(id: u64, response: AircCommandResponse) -> Self {
        Self::Response { id, response }
    }

    /// Wrap one ephemeral turn token as a pushed `StreamDelta` frame (#170) — the
    /// mechanical seam for fanning a persona's live token stream out over this
    /// transport, alongside (never replacing) the durable `State` path.
    pub fn stream_delta(
        room_id: String,
        sender_id: String,
        stream_id: String,
        seq: u64,
        token: String,
        done: bool,
    ) -> Self {
        Self::StreamDelta {
            room_id,
            sender_id,
            stream_id,
            seq,
            token,
            done,
        }
    }

    /// Wrap a positron [`StateEnvelope`] as a pushed `State` frame — the
    /// mechanical seam for fanning the substrate session task's
    /// `ServerMessage::State` out over this transport.
    pub fn state(envelope: StateEnvelope) -> Self {
        Self::State(envelope)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // what this catches: the WS envelope must round-trip AND keep the outer
    // `type` tag distinct from the nested `status` tag — a flattening
    // regression would collide the two discriminants and corrupt dispatch.
    #[test]
    fn client_command_round_trips_with_nested_request() {
        let msg = WsClientMessage::Command {
            id: 7,
            request: AircCommandRequest::new(
                "data/list".into(),
                "peer".into(),
                None,
                json!({"collection": "users"}),
            ),
        };
        let wire = serde_json::to_string(&msg).expect("serialize");
        assert!(wire.contains("\"type\":\"command\""), "outer tag: {wire}");
        assert!(wire.contains("\"path\":\"data/list\""), "nested request: {wire}");
        let back: WsClientMessage = serde_json::from_str(&wire).expect("deserialize");
        assert_eq!(back, msg);
    }

    // what this catches: server Response nests a `status`-tagged
    // AircCommandResponse under the `type`-tagged frame; both tags must
    // survive at their own nesting level.
    #[test]
    fn server_response_round_trips_both_tags() {
        let msg = WsServerMessage::response(7, AircCommandResponse::ok(json!({"n": 3})));
        let wire = serde_json::to_string(&msg).expect("serialize");
        assert!(wire.contains("\"type\":\"response\""), "outer tag: {wire}");
        assert!(wire.contains("\"status\":\"ok\""), "nested status tag: {wire}");
        let back: WsServerMessage = serde_json::from_str(&wire).expect("deserialize");
        assert_eq!(back, msg);
    }

    // what this catches (#170): the ephemeral token frame serializes under the
    // snake_case outer tag "stream_delta" with its correlation fields (room_id,
    // sender_id) flat, and round-trips — so the client can key a typing bubble on
    // sender_id and grow it per token. A tag-rename regression would silently drop
    // the whole live-typing rail.
    #[test]
    fn server_stream_delta_round_trips_with_correlation_fields() {
        let msg = WsServerMessage::stream_delta(
            "room-1".into(),
            "peer-asha".into(),
            "stream-abc".into(),
            3,
            "Hello".into(),
            false,
        );
        let wire = serde_json::to_string(&msg).expect("serialize");
        assert!(wire.contains("\"type\":\"stream_delta\""), "outer tag: {wire}");
        assert!(wire.contains("\"sender_id\":\"peer-asha\""), "sender flat: {wire}");
        assert!(wire.contains("\"token\":\"Hello\""), "token present: {wire}");
        let back: WsServerMessage = serde_json::from_str(&wire).expect("deserialize");
        assert_eq!(back, msg);
    }

    // what this catches: the state-subscription frames must slot in as FLAT
    // sibling variants with distinct `type` tags (subscribe/observe), not a
    // nested positron enum — nesting positron's own `tag = "type"`
    // ClientMessage would collide two `"type"` keys at one map level. If a
    // refactor nested them, this asserts the flat tag AND that positron's
    // Subscribe fields (layers) live at the top level, not under a wrapper.
    #[test]
    fn client_subscribe_round_trips_flat_and_projects_to_positron() {
        let msg = WsClientMessage::Subscribe {
            kinds: vec!["chat".into()],
            layers: vec![StateLayer::Session],
            last_seen: vec![KindRevision {
                kind: "chat".into(),
                revision: 4,
            }],
        };
        let wire = serde_json::to_string(&msg).expect("serialize");
        assert!(wire.contains("\"type\":\"subscribe\""), "flat outer tag: {wire}");
        assert!(wire.contains("\"kinds\":[\"chat\"]"), "kinds at top level: {wire}");
        let back: WsClientMessage = serde_json::from_str(&wire).expect("deserialize");
        assert_eq!(back, msg);

        // and it projects 1:1 onto positron's Subscribe frame.
        match msg.to_session() {
            Some(PositronClientMessage::Subscribe {
                kinds,
                layers,
                last_seen,
            }) => {
                assert_eq!(kinds, vec!["chat".to_string()]);
                assert_eq!(layers, vec![StateLayer::Session]);
                assert_eq!(last_seen.len(), 1);
                assert_eq!(last_seen[0].revision, 4);
            }
            other => panic!("expected positron Subscribe, got {other:?}"),
        }
    }

    // what this catches: Observe carries the ObserverSpec (budget_hz drives
    // live cadence) intact through both the wire round-trip and the positron
    // projection. A dropped budget_hz would silently un-throttle AI perception.
    #[test]
    fn client_observe_projects_spec_intact() {
        let msg = WsClientMessage::Observe {
            spec: ObserverSpec {
                observer_id: "ares-1".into(),
                budget_hz: 3,
                kinds: vec!["chat".into()],
                layers: vec![StateLayer::Session, StateLayer::Semantic],
            },
            last_seen: vec![],
        };
        let back: WsClientMessage =
            serde_json::from_str(&serde_json::to_string(&msg).unwrap()).unwrap();
        assert_eq!(back, msg);
        match msg.to_session() {
            Some(PositronClientMessage::Observe { spec, .. }) => {
                assert_eq!(spec.observer_id, "ares-1");
                assert_eq!(spec.budget_hz, 3, "budget must survive the seam");
            }
            other => panic!("expected positron Observe, got {other:?}"),
        }
    }

    // what this catches: Command must NOT project onto a positron session
    // frame — on this transport commands ride the RPC Command/Response path
    // (which has the ack execute() awaits), never the ack-less positron path.
    // A regression returning Some here would double-dispatch the command.
    #[test]
    fn command_does_not_project_to_a_session_frame() {
        let msg = WsClientMessage::Command {
            id: 1,
            request: AircCommandRequest::new("chat/send".into(), "peer".into(), None, json!({})),
        };
        assert!(
            msg.to_session().is_none(),
            "Command rides the RPC path, not the positron session path"
        );
    }

    // what this catches: the pushed State frame carries a StateEnvelope with
    // its own `kind` field at `type: "state"` — asserts no discriminant
    // collision (the envelope's `kind`/`revision`/`payload` are NOT a second
    // `type` key) and that it round-trips.
    #[test]
    fn server_state_frame_round_trips_without_tag_collision() {
        let env = StateEnvelope {
            kind: "chat".into(),
            revision: Some(9),
            layer: StateLayer::Session,
            payload: json!({"room_name": "general"}),
        };
        let msg = WsServerMessage::state(env);
        let wire = serde_json::to_string(&msg).expect("serialize");
        assert!(wire.contains("\"type\":\"state\""), "outer tag: {wire}");
        assert!(wire.contains("\"kind\":\"chat\""), "envelope kind survives: {wire}");
        let back: WsServerMessage = serde_json::from_str(&wire).expect("deserialize");
        assert_eq!(back, msg);
    }
}
