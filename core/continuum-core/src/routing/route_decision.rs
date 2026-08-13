//! `RouteDecision` — the substrate's typed answer to "what should
//! happen with this URI?"
//!
//! Per `docs/architecture/GRID-ADDRESSING-AND-ROUTING.md` §"Transport
//! selector": every dispatched [`CommandUri`] goes through one pure
//! function — [`route`] — that returns a `RouteDecision`. The
//! decision is exhaustive: every URI shape maps to exactly one
//! variant, and the dispatcher's match handles each.
//!
//! This is the substrate's seam between the addressing layer and
//! the transport layer. Today's variants:
//!
//! - [`RouteDecision::Local`] — execute in this substrate, walk the
//!   existing interceptor + module + TS-fallback chain.
//! - [`RouteDecision::Peer`] — route to a specific peer via airc
//!   (transport implementation lands separately).
//! - [`RouteDecision::Room`] — broadcast to subscribers of an airc
//!   room.
//! - [`RouteDecision::Broadcast`] — fan-out to every active env of a
//!   peer (the `:*` wildcard form).
//!
//! ## Why a typed enum and not (e.g.) a `String` transport name
//!
//! Joel's compression principle applied to routing: ONE enum shape,
//! every consumer's match is exhaustive, the compiler enforces that
//! adding a new variant breaks every site that didn't update.
//! Strings or untyped routing tags would let a future transport
//! sneak past the dispatcher without anyone noticing — exactly the
//! drift Slice P is fighting.
//!
//! ## What this commit lands
//!
//! - The `RouteDecision` enum
//! - The pure `route(&CommandUri) -> RouteDecision` function
//! - 100% mapping coverage — every `CommandUri` variant maps to
//!   exactly one `RouteDecision` variant
//! - Tests pinning the mapping
//!
//! ## What lands in the follow-up commits
//!
//! - **Auth gate** — `gate(decision, caller_peer_id) -> Verdict`
//!   consults policy and returns Allowed / Forbidden / Deferred
//!   BEFORE the decision reaches transport. Reuses
//!   [`Verdict`](super::Verdict) (already a typed enum).
//! - **Transport implementation** — `LocalTransport` + future
//!   `AircTransport` implement a common `Transport` trait; the
//!   dispatcher's match calls into the right one based on the
//!   decision. Today's `CommandExecutor::execute_inner` becomes
//!   `LocalTransport::dispatch`.
//! - **CommandExecutor::dispatch** swaps its `if !is_local()`
//!   error for a `match route(uri) { ... }` that uses the
//!   transport selector.
//!
//! Each of those follow-ups is small because the typed primitive
//! is in place. That's the point of starting here.

use super::{CommandUri, EnvironmentId, NodeId, PeerRef};
use uuid::Uuid;

/// The substrate's transport-routing decision for a single
/// dispatched URI. Every `CommandUri` maps to exactly one variant.
///
/// Pattern matching against `RouteDecision` is the canonical way for
/// the dispatcher (and future auth gate, transport selector,
/// observability surfaces) to ask "what does this URI mean?" — the
/// compiler enforces exhaustiveness so adding a new variant
/// surfaces every consumer that needs updating.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteDecision {
    /// Execute in this substrate. The dispatcher walks the existing
    /// interceptor → registry → TS-fallback chain. The bare path
    /// (without scheme prefix) is the command name the registry
    /// looks up.
    Local {
        path: String,
        query: Option<String>,
        fragment: Option<String>,
    },

    /// Route to a specific peer over airc. `node` disambiguates
    /// when a peer is reachable through multiple nodes; `env`
    /// pins a specific embodiment. The transport layer (follow-up)
    /// packages caller identity + URI + params into an airc
    /// envelope.
    Peer {
        peer: PeerRef,
        node: Option<NodeId>,
        env: Option<EnvironmentId>,
        path: String,
        query: Option<String>,
        fragment: Option<String>,
    },

    /// Broadcast to subscribers of an airc room (UUID-addressed).
    /// Optionally env-filtered. The transport fans out to every
    /// peer subscribed to the room and matching the env filter.
    Room {
        room_id: Uuid,
        env: Option<EnvironmentId>,
        path: String,
        query: Option<String>,
        fragment: Option<String>,
    },

    /// Fan-out to every active env of a specific peer (the `:*`
    /// wildcard form). Distinct variant — not `Peer { env:
    /// Wildcard }` — so the dispatcher can pick a specialized
    /// fan-out transport path without re-inspecting the env field.
    Broadcast {
        peer: PeerRef,
        node: Option<NodeId>,
        path: String,
        query: Option<String>,
        fragment: Option<String>,
    },
}

impl RouteDecision {
    /// Cheap discriminant accessor — useful for logs and probes
    /// that want to record the routing class without cloning the
    /// whole decision.
    pub fn kind(&self) -> RouteKind {
        match self {
            RouteDecision::Local { .. } => RouteKind::Local,
            RouteDecision::Peer { .. } => RouteKind::Peer,
            RouteDecision::Room { .. } => RouteKind::Room,
            RouteDecision::Broadcast { .. } => RouteKind::Broadcast,
        }
    }

    /// True for the variant the dispatcher executes through the
    /// local chain. Every other variant requires a remote transport
    /// to land.
    pub fn is_local(&self) -> bool {
        matches!(self, RouteDecision::Local { .. })
    }

    /// The bare command path (no scheme, no authority, no query) —
    /// what the local registry / remote handler uses to find the
    /// command implementation.
    pub fn path(&self) -> &str {
        match self {
            RouteDecision::Local { path, .. }
            | RouteDecision::Peer { path, .. }
            | RouteDecision::Room { path, .. }
            | RouteDecision::Broadcast { path, .. } => path,
        }
    }
}

/// Cheap discriminant for [`RouteDecision`] — mirrors its variant
/// shape without carrying any of the routing fields. Useful for
/// logs, probes, and routing tables keyed by transport class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RouteKind {
    Local,
    Peer,
    Room,
    Broadcast,
}

impl RouteKind {
    /// Canonical lowercase name — `"local"`, `"peer"`, `"room"`,
    /// `"broadcast"`. Used as a `probe!` field tag and as the
    /// stable identifier for telemetry / routing config.
    pub fn as_str(&self) -> &'static str {
        match self {
            RouteKind::Local => continuum_airc_protocol::KIND_LOCAL,
            RouteKind::Peer => continuum_airc_protocol::KIND_PEER,
            RouteKind::Room => continuum_airc_protocol::KIND_ROOM,
            RouteKind::Broadcast => continuum_airc_protocol::KIND_BROADCAST,
        }
    }
}

/// The substrate's pure routing function. Maps a [`CommandUri`] to
/// the [`RouteDecision`] the dispatcher acts on.
///
/// Every URI shape maps to exactly one decision shape — this is the
/// single source of truth. Consumers (the dispatcher, the auth gate
/// once it lands, the transport selector) call this function rather
/// than re-implementing the mapping locally.
///
/// Side-effect free; safe to call from any context, including
/// observability probes and dry-run audit tools.
pub fn route(uri: &CommandUri) -> RouteDecision {
    match uri {
        CommandUri::Local {
            path,
            query,
            fragment,
        } => RouteDecision::Local {
            path: path.clone(),
            query: query.clone(),
            fragment: fragment.clone(),
        },
        CommandUri::Peer {
            peer,
            node,
            env,
            path,
            query,
            fragment,
        } => RouteDecision::Peer {
            peer: peer.clone(),
            node: node.clone(),
            env: env.clone(),
            path: path.clone(),
            query: query.clone(),
            fragment: fragment.clone(),
        },
        CommandUri::Room {
            room_id,
            env,
            path,
            query,
            fragment,
        } => RouteDecision::Room {
            room_id: *room_id,
            env: env.clone(),
            path: path.clone(),
            query: query.clone(),
            fragment: fragment.clone(),
        },
        CommandUri::Broadcast {
            peer,
            node,
            path,
            query,
            fragment,
        } => RouteDecision::Broadcast {
            peer: peer.clone(),
            node: node.clone(),
            path: path.clone(),
            query: query.clone(),
            fragment: fragment.clone(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::CommandUri;

    #[test]
    fn local_uri_routes_to_local_decision() {
        let uri = CommandUri::local("inference/llm/generate");
        let decision = route(&uri);
        assert_eq!(decision.kind(), RouteKind::Local);
        assert_eq!(decision.path(), "inference/llm/generate");
        assert!(decision.is_local());
        match decision {
            RouteDecision::Local {
                path,
                query,
                fragment,
            } => {
                assert_eq!(path, "inference/llm/generate");
                assert!(query.is_none());
                assert!(fragment.is_none());
            }
            other => panic!("expected Local, got {other:?}"),
        }
    }

    #[test]
    fn local_uri_with_query_and_fragment_preserved() {
        let uri =
            CommandUri::parse("airc:///inference/llm/generate?model=qwen#layer-3").expect("parse");
        let decision = route(&uri);
        match decision {
            RouteDecision::Local {
                path,
                query,
                fragment,
            } => {
                assert_eq!(path, "inference/llm/generate");
                assert_eq!(query.as_deref(), Some("model=qwen"));
                assert_eq!(fragment.as_deref(), Some("layer-3"));
            }
            other => panic!("expected Local, got {other:?}"),
        }
    }

    #[test]
    fn peer_uri_routes_to_peer_decision() {
        let uri = CommandUri::parse("airc://maya/inference/llm/generate").expect("parse");
        let decision = route(&uri);
        assert_eq!(decision.kind(), RouteKind::Peer);
        assert!(!decision.is_local());
        assert_eq!(decision.path(), "inference/llm/generate");
        match decision {
            RouteDecision::Peer {
                peer, node, env, ..
            } => {
                assert_eq!(peer, PeerRef::Name("maya".to_string()));
                assert!(node.is_none());
                assert!(env.is_none());
            }
            other => panic!("expected Peer, got {other:?}"),
        }
    }

    #[test]
    fn peer_with_node_and_env_preserved() {
        let uri = CommandUri::parse("airc://maya@5090-rig:vr/debug/probes/decision/stream")
            .expect("parse");
        let decision = route(&uri);
        match decision {
            RouteDecision::Peer {
                peer,
                node,
                env,
                path,
                ..
            } => {
                assert_eq!(peer, PeerRef::Name("maya".to_string()));
                assert_eq!(node, Some(NodeId::from("5090-rig")));
                assert_eq!(env, Some(EnvironmentId::from("vr")));
                assert_eq!(path, "debug/probes/decision/stream");
            }
            other => panic!("expected Peer, got {other:?}"),
        }
    }

    #[test]
    fn peer_by_uuid_preserves_uuid() {
        let id = Uuid::new_v4();
        let uri = CommandUri::parse(&format!("airc://{id}/inference/llm/generate")).expect("parse");
        let decision = route(&uri);
        match decision {
            RouteDecision::Peer { peer, .. } => {
                assert_eq!(peer, PeerRef::Uuid(id));
            }
            other => panic!("expected Peer, got {other:?}"),
        }
    }

    #[test]
    fn room_uri_routes_to_room_decision() {
        let room_id = Uuid::new_v4();
        let uri = CommandUri::parse(&format!("airc://room:{room_id}/chat/post")).expect("parse");
        let decision = route(&uri);
        assert_eq!(decision.kind(), RouteKind::Room);
        assert!(!decision.is_local());
        match decision {
            RouteDecision::Room {
                room_id: got_id,
                env,
                path,
                ..
            } => {
                assert_eq!(got_id, room_id);
                assert!(env.is_none());
                assert_eq!(path, "chat/post");
            }
            other => panic!("expected Room, got {other:?}"),
        }
    }

    #[test]
    fn room_with_env_filter_preserved() {
        let room_id = Uuid::new_v4();
        let uri = CommandUri::parse(&format!("airc://room:{room_id}:vr/chat/post")).expect("parse");
        let decision = route(&uri);
        match decision {
            RouteDecision::Room { env, .. } => {
                assert_eq!(env, Some(EnvironmentId::from("vr")));
            }
            other => panic!("expected Room, got {other:?}"),
        }
    }

    #[test]
    fn broadcast_uri_routes_to_broadcast_decision() {
        let uri = CommandUri::parse("airc://maya:*/notification/send").expect("parse");
        let decision = route(&uri);
        assert_eq!(decision.kind(), RouteKind::Broadcast);
        assert!(!decision.is_local());
        match decision {
            RouteDecision::Broadcast {
                peer, node, path, ..
            } => {
                assert_eq!(peer, PeerRef::Name("maya".to_string()));
                assert!(node.is_none());
                assert_eq!(path, "notification/send");
            }
            other => panic!("expected Broadcast, got {other:?}"),
        }
    }

    #[test]
    fn broadcast_with_node_preserved() {
        let uri = CommandUri::parse("airc://maya@5090-rig:*/notification/send").expect("parse");
        let decision = route(&uri);
        match decision {
            RouteDecision::Broadcast { peer, node, .. } => {
                assert_eq!(peer, PeerRef::Name("maya".to_string()));
                assert_eq!(node, Some(NodeId::from("5090-rig")));
            }
            other => panic!("expected Broadcast, got {other:?}"),
        }
    }

    #[test]
    fn route_kind_canonical_names() {
        assert_eq!(RouteKind::Local.as_str(), "local");
        assert_eq!(RouteKind::Peer.as_str(), "peer");
        assert_eq!(RouteKind::Room.as_str(), "room");
        assert_eq!(RouteKind::Broadcast.as_str(), "broadcast");
    }

    #[test]
    fn route_is_pure_repeated_calls_identical() {
        // Smell test: route() has no hidden state. Two calls with the
        // same URI produce equal decisions.
        let uri =
            CommandUri::parse("airc://maya:vr/inference/llm/generate?token=abc").expect("parse");
        let d1 = route(&uri);
        let d2 = route(&uri);
        assert_eq!(d1, d2);
    }

    #[test]
    fn all_uri_variants_have_path_accessor_parity() {
        // The path() accessor must agree with the underlying CommandUri's
        // path for every variant — pins the mapping at the path level.
        let cases = vec![
            CommandUri::local("a/b/c"),
            CommandUri::parse("airc://maya/x/y").expect("peer"),
            CommandUri::parse(&format!("airc://room:{}/r/s", Uuid::new_v4())).expect("room"),
            CommandUri::parse("airc://maya:*/q/r").expect("broadcast"),
        ];
        for uri in cases {
            let decision = route(&uri);
            assert_eq!(
                decision.path(),
                uri.path(),
                "RouteDecision.path() must mirror CommandUri.path() for {uri}"
            );
        }
    }
}
