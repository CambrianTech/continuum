//! `CommandUri` — the substrate's typed addressing primitive.
//!
//! Implements the Slice P grammar
//! `airc://[peer[@node]][:env]/[path][?query][#fragment]`. See
//! `docs/architecture/GRID-ADDRESSING-AND-ROUTING.md` for the
//! design contract.
//!
//! ## Variants partition by what's being addressed
//!
//! - [`CommandUri::Local`] — caller's substrate. Bare paths
//!   (`"inference/llm/generate"`) and the explicit-local form
//!   (`"airc:///inference/llm/generate"`) both parse here.
//! - [`CommandUri::Peer`] — a specific peer with optional node
//!   disambiguation and optional environment filter.
//! - [`CommandUri::Room`] — broadcast to a room (UUID-addressed),
//!   optional env filter.
//! - [`CommandUri::Broadcast`] — broadcast to every active env of
//!   a peer (the `:*` wildcard form). Kept as its own variant so the
//!   dispatcher can choose a specialized broadcast transport path.
//!
//! ## Round-trip guarantee
//!
//! For every valid input that parses successfully,
//! `CommandUri::parse(&uri.to_string()) == Ok(uri.clone())`. The
//! [`round_trip_*`](#tests) tests pin this for every variant.
//!
//! ## Naming convention
//!
//! `room` is a reserved peer-name prefix sigil — a URI authority
//! starting with `room:` parses to [`CommandUri::Room`], NOT a
//! peer named `room`. Operators who want to literally name a
//! persona `room` must use the canonical UUID form. The collision
//! is documented in the design doc and enforced here at parse time.

use std::fmt;
use std::str::FromStr;

use thiserror::Error;
use uuid::Uuid;

// ─── The typed enum ────────────────────────────────────────────────

/// The substrate's universal addressing primitive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandUri {
    /// Caller's own substrate. Bare paths default here so existing
    /// `Commands.execute("inference/llm/generate")` callers work
    /// unchanged. Also produced by the explicit-local form
    /// `airc:///inference/llm/generate`.
    Local {
        path: String,
        query: Option<String>,
        fragment: Option<String>,
    },
    /// Peer-addressed dispatch. The peer is the WHO; the node is the
    /// optional WHERE; the env is the optional WHICH embodiment.
    Peer {
        peer: PeerRef,
        node: Option<NodeId>,
        env: Option<EnvSelector>,
        path: String,
        query: Option<String>,
        fragment: Option<String>,
    },
    /// Room broadcast. The substrate fans the dispatch out to every
    /// peer currently subscribed to this room, optionally filtered by
    /// env.
    Room {
        room_id: Uuid,
        env: Option<EnvSelector>,
        path: String,
        query: Option<String>,
        fragment: Option<String>,
    },
    /// Broadcast to every active env of a specific peer (`:*`
    /// wildcard). Semantically equivalent to [`CommandUri::Peer`]
    /// with `env = Some(EnvSelector::Wildcard)`, but kept distinct so
    /// the dispatcher can pick a fan-out transport path.
    Broadcast {
        peer: PeerRef,
        node: Option<NodeId>,
        path: String,
        query: Option<String>,
        fragment: Option<String>,
    },
}

/// How a peer is identified in a URI.
///
/// `Uuid` is canonical (cryptographic peer_id from the Ed25519
/// identity); `Name` is human-readable convenience the dispatcher
/// resolves via airc whois.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PeerRef {
    Uuid(Uuid),
    Name(String),
}

/// Node-id for peer-at-node disambiguation. Opaque string per the
/// design doc; airc assigns and resolves these.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeId(pub String);

/// Environment selector — specific name or wildcard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvSelector {
    /// Specific environment. The parser accepts any non-empty string;
    /// well-known names (`web`, `tty`, `vr`, `ar`, `cli`, `headless`)
    /// are validated downstream by the env registry.
    Named(String),
    /// Match every active environment of the target. Surfaces as
    /// `[CommandUri::Broadcast]` when used with a peer authority.
    Wildcard,
}

// ─── Typed parse errors ────────────────────────────────────────────

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum UriParseError {
    /// A URI started with `://` but a scheme other than `airc`.
    #[error("URI scheme must be `airc://` (got {0:?})")]
    UnknownScheme(String),

    /// The authority component (between `://` and `/`) could not be
    /// parsed as `[peer[@node]][:env]` OR `room:<uuid>[:env]`.
    #[error("invalid authority — could not parse {0:?}")]
    InvalidAuthority(String),

    /// The peer component looked like a UUID (contained dashes) but
    /// didn't parse.
    #[error("invalid peer UUID {0:?}: {1}")]
    InvalidPeerUuid(String, String),

    /// The room component's UUID didn't parse.
    #[error("invalid room UUID {0:?}: {1}")]
    InvalidRoomUuid(String, String),

    /// Authority had an `@` but the node-id was empty.
    #[error("authority {0:?} contains `@` but no node id follows")]
    EmptyNodeId(String),

    /// Authority had a `:` but the env was empty.
    #[error("authority {0:?} contains `:` but no env follows")]
    EmptyEnv(String),

    /// Authority claimed `room:` but the rest was empty.
    #[error("authority {0:?} starts with `room:` but no UUID follows")]
    EmptyRoomUuid(String),

    /// Peer name was empty (e.g. URI `airc:///path` with extra slash).
    /// Note: explicit-local `airc:///path` is allowed and parses to
    /// [`CommandUri::Local`]; this error surfaces only for malformed
    /// peer authorities like `airc:///:env/...`.
    #[error("authority {0:?} has an empty peer name")]
    EmptyPeer(String),

    /// Path was empty after parsing the authority.
    #[error("URI has no path")]
    EmptyPath,
}

// ─── Parser ────────────────────────────────────────────────────────

/// Reserved sigil for room URIs. A peer name literally equal to
/// `"room"` cannot be expressed by name — the substrate routes it
/// to [`CommandUri::Room`] instead. Operators must use the
/// canonical UUID form for any persona literally named `room`. Per
/// the design doc, this is enforced at name-registration time as
/// well.
const ROOM_SIGIL: &str = "room:";

impl CommandUri {
    /// Parse a bare path OR a fully-qualified `airc://` URI.
    ///
    /// Bare paths (`"inference/llm/generate"`) parse to
    /// [`CommandUri::Local`] for backwards compatibility with every
    /// existing `Commands.execute()` call site.
    pub fn parse(input: &str) -> Result<Self, UriParseError> {
        // Empty input is never valid.
        if input.is_empty() {
            return Err(UriParseError::EmptyPath);
        }

        // Bare path → Local. Anything that doesn't contain a scheme
        // separator is treated as a path. Per the design doc this
        // is the common case.
        if !input.contains("://") {
            return parse_local_from_path(input);
        }

        // Explicit-local: `airc:///path` (empty authority).
        if let Some(rest) = input.strip_prefix("airc:///") {
            return parse_local_from_path(rest);
        }

        // Fully-qualified: `airc://authority/path`.
        if let Some(after_scheme) = input.strip_prefix("airc://") {
            return parse_airc_uri(after_scheme);
        }

        // Anything else with `://` but not `airc://` is a wrong-scheme
        // error.
        let scheme = input.split("://").next().unwrap_or("").to_string();
        Err(UriParseError::UnknownScheme(scheme))
    }
}

fn parse_local_from_path(path_etc: &str) -> Result<CommandUri, UriParseError> {
    let (path, query, fragment) = split_path_query_fragment(path_etc);
    if path.is_empty() {
        return Err(UriParseError::EmptyPath);
    }
    Ok(CommandUri::Local {
        path: path.to_string(),
        query: query.map(str::to_string),
        fragment: fragment.map(str::to_string),
    })
}

fn parse_airc_uri(after_scheme: &str) -> Result<CommandUri, UriParseError> {
    // Split authority from the rest at the first `/`. The authority
    // is everything before; the path/query/fragment is everything
    // after.
    let (authority, rest) = match after_scheme.find('/') {
        Some(idx) => (&after_scheme[..idx], &after_scheme[idx + 1..]),
        None => (after_scheme, ""),
    };

    let (path, query, fragment) = split_path_query_fragment(rest);
    if path.is_empty() {
        return Err(UriParseError::EmptyPath);
    }
    let path = path.to_string();
    let query = query.map(str::to_string);
    let fragment = fragment.map(str::to_string);

    // Empty authority means `airc:///path` — explicit local. Already
    // handled by the caller via `strip_prefix("airc:///")`; if we
    // reach here with an empty authority, treat it as a defensive
    // path through Local.
    if authority.is_empty() {
        return Ok(CommandUri::Local {
            path,
            query,
            fragment,
        });
    }

    // Room URIs use the reserved `room:` sigil.
    if let Some(room_rest) = authority.strip_prefix(ROOM_SIGIL) {
        return parse_room_authority(authority, room_rest, path, query, fragment);
    }

    // Peer authority: `name[@node][:env]` (or wildcard env produces
    // the `Broadcast` variant).
    parse_peer_authority(authority, path, query, fragment)
}

fn parse_room_authority(
    full_authority: &str,
    after_sigil: &str,
    path: String,
    query: Option<String>,
    fragment: Option<String>,
) -> Result<CommandUri, UriParseError> {
    if after_sigil.is_empty() {
        return Err(UriParseError::EmptyRoomUuid(full_authority.to_string()));
    }

    // Optional `:env` suffix on a room URI: `room:<uuid>:env`.
    let (uuid_part, env) = match after_sigil.rsplit_once(':') {
        Some((u, e)) if !u.is_empty() && !e.is_empty() => {
            (u, Some(parse_env_selector(e)))
        }
        Some((_u, e)) if e.is_empty() => {
            return Err(UriParseError::EmptyEnv(full_authority.to_string()));
        }
        _ => (after_sigil, None),
    };

    let room_id =
        Uuid::parse_str(uuid_part).map_err(|e| {
            UriParseError::InvalidRoomUuid(uuid_part.to_string(), e.to_string())
        })?;

    Ok(CommandUri::Room {
        room_id,
        env,
        path,
        query,
        fragment,
    })
}

fn parse_peer_authority(
    authority: &str,
    path: String,
    query: Option<String>,
    fragment: Option<String>,
) -> Result<CommandUri, UriParseError> {
    // Split off `:env` first (if present). We scan from the right
    // because `peer@node` can't contain a colon but env can come
    // after either form.
    let (peer_and_node, env) = match authority.rsplit_once(':') {
        Some((before, after)) if !before.is_empty() && !after.is_empty() => {
            (before, Some(parse_env_selector(after)))
        }
        Some((_before, after)) if after.is_empty() => {
            return Err(UriParseError::EmptyEnv(authority.to_string()));
        }
        _ => (authority, None),
    };

    // Split off `@node` from the peer.
    let (peer_str, node) = match peer_and_node.split_once('@') {
        Some((p, n)) if !p.is_empty() && !n.is_empty() => {
            (p, Some(NodeId(n.to_string())))
        }
        Some((_p, n)) if n.is_empty() => {
            return Err(UriParseError::EmptyNodeId(authority.to_string()));
        }
        Some((p, _n)) if p.is_empty() => {
            return Err(UriParseError::EmptyPeer(authority.to_string()));
        }
        _ => (peer_and_node, None),
    };

    if peer_str.is_empty() {
        return Err(UriParseError::EmptyPeer(authority.to_string()));
    }

    let peer = parse_peer_ref(peer_str)?;

    // Wildcard env on a peer authority produces the Broadcast
    // variant.
    if matches!(env, Some(EnvSelector::Wildcard)) {
        return Ok(CommandUri::Broadcast {
            peer,
            node,
            path,
            query,
            fragment,
        });
    }

    Ok(CommandUri::Peer {
        peer,
        node,
        env,
        path,
        query,
        fragment,
    })
}

fn parse_peer_ref(s: &str) -> Result<PeerRef, UriParseError> {
    // A UUID has the canonical 8-4-4-4-12 hyphenated form. We
    // try-parse and fall back to Name on failure — but only treat
    // the input as "intended as UUID" when it looks like one
    // (contains four hyphens at the right positions), so a typo'd
    // name like `maya-the-helper` doesn't surface as InvalidPeerUuid.
    if looks_like_uuid(s) {
        return Uuid::parse_str(s)
            .map(PeerRef::Uuid)
            .map_err(|e| UriParseError::InvalidPeerUuid(s.to_string(), e.to_string()));
    }
    Ok(PeerRef::Name(s.to_string()))
}

/// Cheap shape check — five segments split by `-` with the right
/// lengths. Cheap enough to run on the parse hot path; accurate
/// enough to distinguish UUIDs from arbitrary names.
fn looks_like_uuid(s: &str) -> bool {
    let segs: Vec<&str> = s.split('-').collect();
    segs.len() == 5
        && segs[0].len() == 8
        && segs[1].len() == 4
        && segs[2].len() == 4
        && segs[3].len() == 4
        && segs[4].len() == 12
}

fn parse_env_selector(s: &str) -> EnvSelector {
    if s == "*" {
        EnvSelector::Wildcard
    } else {
        EnvSelector::Named(s.to_string())
    }
}

/// Split a `path?query#fragment` triple. Path is everything before
/// the first `?` or `#`; query is between `?` and `#` (or end);
/// fragment is after `#`. Per RFC 3986 the query and fragment are
/// both optional and can appear in either order... but in practice
/// callers use `?query#fragment`. We tolerate `#frag?query` by
/// treating whichever appears first as terminating the path.
fn split_path_query_fragment(s: &str) -> (&str, Option<&str>, Option<&str>) {
    // Find the first `?` and the first `#`. Path is min of those (or
    // end of string).
    let q_idx = s.find('?');
    let f_idx = s.find('#');
    let path_end = match (q_idx, f_idx) {
        (Some(q), Some(f)) => q.min(f),
        (Some(q), None) => q,
        (None, Some(f)) => f,
        (None, None) => s.len(),
    };
    let path = &s[..path_end];

    let rest = &s[path_end..];
    // Common case: `?query#fragment` OR `?query` OR `#fragment`.
    if let Some(q_rest) = rest.strip_prefix('?') {
        match q_rest.find('#') {
            Some(idx) => (path, Some(&q_rest[..idx]), Some(&q_rest[idx + 1..])),
            None => (path, Some(q_rest), None),
        }
    } else if let Some(f_rest) = rest.strip_prefix('#') {
        // `#fragment?query` is tolerated; treat the whole tail as
        // fragment (substrate doesn't interpret fragment internals).
        (path, None, Some(f_rest))
    } else {
        (path, None, None)
    }
}

// ─── Display (canonical round-trip form) ──────────────────────────

impl fmt::Display for CommandUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CommandUri::Local {
                path,
                query,
                fragment,
            } => {
                // Bare path form (the common case). Round-trips
                // exactly via `parse`.
                write!(f, "{path}")?;
                write_query_fragment(f, query.as_deref(), fragment.as_deref())
            }
            CommandUri::Peer {
                peer,
                node,
                env,
                path,
                query,
                fragment,
            } => {
                write!(f, "airc://{peer}")?;
                if let Some(n) = node {
                    write!(f, "@{n}")?;
                }
                if let Some(e) = env {
                    write!(f, ":{e}")?;
                }
                write!(f, "/{path}")?;
                write_query_fragment(f, query.as_deref(), fragment.as_deref())
            }
            CommandUri::Room {
                room_id,
                env,
                path,
                query,
                fragment,
            } => {
                write!(f, "airc://room:{room_id}")?;
                if let Some(e) = env {
                    write!(f, ":{e}")?;
                }
                write!(f, "/{path}")?;
                write_query_fragment(f, query.as_deref(), fragment.as_deref())
            }
            CommandUri::Broadcast {
                peer,
                node,
                path,
                query,
                fragment,
            } => {
                write!(f, "airc://{peer}")?;
                if let Some(n) = node {
                    write!(f, "@{n}")?;
                }
                write!(f, ":*/{path}")?;
                write_query_fragment(f, query.as_deref(), fragment.as_deref())
            }
        }
    }
}

fn write_query_fragment(
    f: &mut fmt::Formatter<'_>,
    query: Option<&str>,
    fragment: Option<&str>,
) -> fmt::Result {
    if let Some(q) = query {
        write!(f, "?{q}")?;
    }
    if let Some(fr) = fragment {
        write!(f, "#{fr}")?;
    }
    Ok(())
}

impl fmt::Display for PeerRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PeerRef::Uuid(u) => write!(f, "{u}"),
            PeerRef::Name(n) => write!(f, "{n}"),
        }
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Display for EnvSelector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EnvSelector::Named(n) => write!(f, "{n}"),
            EnvSelector::Wildcard => write!(f, "*"),
        }
    }
}

impl FromStr for CommandUri {
    type Err = UriParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

// ─── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // Helper for round-trip assertions.
    fn round_trip(input: &str, expected: CommandUri) {
        let parsed = CommandUri::parse(input).unwrap_or_else(|e| {
            panic!("parse({input:?}) failed: {e}")
        });
        assert_eq!(parsed, expected, "parsed mismatch for {input:?}");
        let printed = parsed.to_string();
        let reparsed = CommandUri::parse(&printed).unwrap_or_else(|e| {
            panic!("re-parse of {printed:?} failed: {e}")
        });
        assert_eq!(reparsed, parsed, "round-trip mismatch for {input:?}");
    }

    // Bare path
    #[test]
    fn bare_path_parses_to_local() {
        round_trip(
            "inference/llm/generate",
            CommandUri::Local {
                path: "inference/llm/generate".into(),
                query: None,
                fragment: None,
            },
        );
    }

    #[test]
    fn bare_path_with_query() {
        round_trip(
            "inference/llm/generate?model=qwen2.5-0.5b",
            CommandUri::Local {
                path: "inference/llm/generate".into(),
                query: Some("model=qwen2.5-0.5b".into()),
                fragment: None,
            },
        );
    }

    #[test]
    fn bare_path_with_fragment() {
        round_trip(
            "turn/current#chunk:42",
            CommandUri::Local {
                path: "turn/current".into(),
                query: None,
                fragment: Some("chunk:42".into()),
            },
        );
    }

    // Explicit local
    #[test]
    fn explicit_local_parses_to_local() {
        round_trip(
            "airc:///inference/llm/generate",
            CommandUri::Local {
                path: "inference/llm/generate".into(),
                query: None,
                fragment: None,
            },
        );
    }

    // Peer by name
    #[test]
    fn peer_by_name() {
        round_trip(
            "airc://maya/inference/llm/generate",
            CommandUri::Peer {
                peer: PeerRef::Name("maya".into()),
                node: None,
                env: None,
                path: "inference/llm/generate".into(),
                query: None,
                fragment: None,
            },
        );
    }

    #[test]
    fn peer_by_uuid() {
        let uuid = Uuid::parse_str("18c04c5b-e059-4129-816f-75e8e58fd74c").unwrap();
        round_trip(
            "airc://18c04c5b-e059-4129-816f-75e8e58fd74c/inference/llm/generate",
            CommandUri::Peer {
                peer: PeerRef::Uuid(uuid),
                node: None,
                env: None,
                path: "inference/llm/generate".into(),
                query: None,
                fragment: None,
            },
        );
    }

    #[test]
    fn peer_with_node() {
        round_trip(
            "airc://maya@5090-rig/inference/llm/generate",
            CommandUri::Peer {
                peer: PeerRef::Name("maya".into()),
                node: Some(NodeId("5090-rig".into())),
                env: None,
                path: "inference/llm/generate".into(),
                query: None,
                fragment: None,
            },
        );
    }

    #[test]
    fn peer_with_env() {
        round_trip(
            "airc://maya:web/widget/show",
            CommandUri::Peer {
                peer: PeerRef::Name("maya".into()),
                node: None,
                env: Some(EnvSelector::Named("web".into())),
                path: "widget/show".into(),
                query: None,
                fragment: None,
            },
        );
    }

    #[test]
    fn peer_with_node_and_env() {
        round_trip(
            "airc://maya@5090-rig:vr/scene/spawn",
            CommandUri::Peer {
                peer: PeerRef::Name("maya".into()),
                node: Some(NodeId("5090-rig".into())),
                env: Some(EnvSelector::Named("vr".into())),
                path: "scene/spawn".into(),
                query: None,
                fragment: None,
            },
        );
    }

    // Broadcast (wildcard env on a peer)
    #[test]
    fn peer_wildcard_env_is_broadcast() {
        round_trip(
            "airc://maya:*/notification/post",
            CommandUri::Broadcast {
                peer: PeerRef::Name("maya".into()),
                node: None,
                path: "notification/post".into(),
                query: None,
                fragment: None,
            },
        );
    }

    #[test]
    fn broadcast_with_node() {
        round_trip(
            "airc://maya@5090-rig:*/notification/post",
            CommandUri::Broadcast {
                peer: PeerRef::Name("maya".into()),
                node: Some(NodeId("5090-rig".into())),
                path: "notification/post".into(),
                query: None,
                fragment: None,
            },
        );
    }

    // Room
    #[test]
    fn room_by_uuid() {
        let uuid = Uuid::parse_str("cb2e21a1-999a-5a03-a184-df06e4ee7097").unwrap();
        round_trip(
            "airc://room:cb2e21a1-999a-5a03-a184-df06e4ee7097/render/start",
            CommandUri::Room {
                room_id: uuid,
                env: None,
                path: "render/start".into(),
                query: None,
                fragment: None,
            },
        );
    }

    #[test]
    fn room_with_env_filter() {
        let uuid = Uuid::parse_str("cb2e21a1-999a-5a03-a184-df06e4ee7097").unwrap();
        round_trip(
            "airc://room:cb2e21a1-999a-5a03-a184-df06e4ee7097:web/render/start",
            CommandUri::Room {
                room_id: uuid,
                env: Some(EnvSelector::Named("web".into())),
                path: "render/start".into(),
                query: None,
                fragment: None,
            },
        );
    }

    // Persona-internal address space (substrate paths under a peer)
    #[test]
    fn persona_internal_address_space() {
        round_trip(
            "airc://maya/cognition/genome/lora:typescript-expertise",
            CommandUri::Peer {
                peer: PeerRef::Name("maya".into()),
                node: None,
                env: None,
                path: "cognition/genome/lora:typescript-expertise".into(),
                query: None,
                fragment: None,
            },
        );
    }

    #[test]
    fn debug_namespace_routing() {
        round_trip(
            "airc://maya/debug/probes/latency/stream",
            CommandUri::Peer {
                peer: PeerRef::Name("maya".into()),
                node: None,
                env: None,
                path: "debug/probes/latency/stream".into(),
                query: None,
                fragment: None,
            },
        );
    }

    // Query + fragment on every variant
    #[test]
    fn peer_with_query_and_fragment() {
        round_trip(
            "airc://maya/debug/profile/flamegraph?window=5m&format=svg#frame:42",
            CommandUri::Peer {
                peer: PeerRef::Name("maya".into()),
                node: None,
                env: None,
                path: "debug/profile/flamegraph".into(),
                query: Some("window=5m&format=svg".into()),
                fragment: Some("frame:42".into()),
            },
        );
    }

    // Error cases
    #[test]
    fn unknown_scheme_errors() {
        assert!(matches!(
            CommandUri::parse("https://example.com/foo"),
            Err(UriParseError::UnknownScheme(_))
        ));
    }

    #[test]
    fn empty_input_errors() {
        assert!(matches!(
            CommandUri::parse(""),
            Err(UriParseError::EmptyPath)
        ));
    }

    #[test]
    fn empty_path_after_authority_errors() {
        assert!(matches!(
            CommandUri::parse("airc://maya/"),
            Err(UriParseError::EmptyPath)
        ));
        assert!(matches!(
            CommandUri::parse("airc://maya"),
            Err(UriParseError::EmptyPath)
        ));
    }

    #[test]
    fn invalid_room_uuid_errors() {
        assert!(matches!(
            CommandUri::parse("airc://room:not-a-uuid/render/start"),
            Err(UriParseError::InvalidRoomUuid(_, _))
        ));
    }

    #[test]
    fn empty_room_uuid_errors() {
        assert!(matches!(
            CommandUri::parse("airc://room:/render/start"),
            Err(UriParseError::EmptyRoomUuid(_))
        ));
    }

    #[test]
    fn empty_node_id_errors() {
        assert!(matches!(
            CommandUri::parse("airc://maya@/inference/llm/generate"),
            Err(UriParseError::EmptyNodeId(_))
        ));
    }

    #[test]
    fn empty_env_errors() {
        assert!(matches!(
            CommandUri::parse("airc://maya:/inference/llm/generate"),
            Err(UriParseError::EmptyEnv(_))
        ));
    }

    #[test]
    fn from_str_delegates_to_parse() {
        let uri: CommandUri = "airc://maya/inference/llm/generate".parse().unwrap();
        assert!(matches!(uri, CommandUri::Peer { .. }));
    }

    // Name-vs-UUID detection
    #[test]
    fn name_with_hyphens_does_not_false_positive_as_uuid() {
        // Five segments but wrong lengths → name, not UUID.
        let r = CommandUri::parse("airc://maya-the-helper-of-joel/foo")
            .expect("hyphenated name should parse as Name");
        match r {
            CommandUri::Peer {
                peer: PeerRef::Name(n),
                ..
            } => assert_eq!(n, "maya-the-helper-of-joel"),
            other => panic!("expected Peer{{Name}}, got {other:?}"),
        }
    }

    #[test]
    fn malformed_uuid_at_segment_lengths_does_error() {
        // Looks like a UUID (right segment lengths) but contains
        // non-hex chars → InvalidPeerUuid.
        let bad = "zzzzzzzz-zzzz-zzzz-zzzz-zzzzzzzzzzzz";
        assert!(matches!(
            CommandUri::parse(&format!("airc://{bad}/foo")),
            Err(UriParseError::InvalidPeerUuid(_, _))
        ));
    }

    // looks_like_uuid contract
    #[test]
    fn looks_like_uuid_recognizes_canonical_form() {
        assert!(looks_like_uuid("18c04c5b-e059-4129-816f-75e8e58fd74c"));
    }

    #[test]
    fn looks_like_uuid_rejects_names() {
        assert!(!looks_like_uuid("maya"));
        assert!(!looks_like_uuid("maya-the-helper"));
        assert!(!looks_like_uuid("18c04c5b"));
    }
}
