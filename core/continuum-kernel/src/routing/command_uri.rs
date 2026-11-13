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

use crate::routing::EnvironmentId;

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
        env: Option<EnvironmentId>,
        path: String,
        query: Option<String>,
        fragment: Option<String>,
    },
    /// Room broadcast. The substrate fans the dispatch out to every
    /// peer currently subscribed to this room, optionally filtered by
    /// env.
    Room {
        room_id: Uuid,
        env: Option<EnvironmentId>,
        path: String,
        query: Option<String>,
        fragment: Option<String>,
    },
    /// Broadcast to every active env of a specific peer (`:*`
    /// wildcard). Semantically equivalent to [`CommandUri::Peer`]
    /// with `env = Some(EnvironmentId::Wildcard)`, but kept distinct so
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

impl From<&str> for NodeId {
    fn from(s: &str) -> Self {
        NodeId(s.to_string())
    }
}

impl From<String> for NodeId {
    fn from(s: String) -> Self {
        NodeId(s)
    }
}

// The CommandUri's `env` field now references `EnvironmentId`
// directly (defined in `routing/environment.rs`). The previous
// `EnvSelector` type was a duplicate — same Named/Wildcard variants,
// same semantics — and got merged into `EnvironmentId` in this
// commit so the substrate has ONE typed embodiment primitive
// instead of two parallel ones.
//
// Per the session's compression principle: when two types describe
// the same concept, they're not "decoupled" — they're drift waiting
// to happen.

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
    /// Construct a [`CommandUri::Local`] from a path string —
    /// infallible, no parsing surprises at the call site.
    ///
    /// The common pattern at the migration point:
    ///
    /// ```ignore
    /// // before:
    /// executor.execute_json("ai/generate", params).await?;
    /// // after:
    /// executor.execute_json(CommandUri::local("ai/generate"), params).await?;
    /// ```
    pub fn local(path: impl Into<String>) -> Self {
        CommandUri::Local {
            path: path.into(),
            query: None,
            fragment: None,
        }
    }

    /// Borrow the path component of the URI. Every variant carries a
    /// path; this accessor unifies them so the dispatcher can look up
    /// the handler without matching on the variant first.
    pub fn path(&self) -> &str {
        match self {
            CommandUri::Local { path, .. }
            | CommandUri::Peer { path, .. }
            | CommandUri::Room { path, .. }
            | CommandUri::Broadcast { path, .. } => path,
        }
    }

    /// `true` iff this URI dispatches to the caller's own substrate.
    /// Used by the executor to short-circuit transport selection for
    /// the common case.
    pub fn is_local(&self) -> bool {
        matches!(self, CommandUri::Local { .. })
    }

    /// Parse a bare path OR a fully-qualified `airc://` URI.
    ///
    /// Bare paths (`"inference/llm/generate"`) parse to
    /// [`CommandUri::Local`] — preserves the call-shape used by every
    /// existing `Commands.execute()` site as the migration lands.
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
        Some((u, e)) if !u.is_empty() && !e.is_empty() => (u, Some(parse_env_selector(e))),
        Some((_u, e)) if e.is_empty() => {
            return Err(UriParseError::EmptyEnv(full_authority.to_string()));
        }
        _ => (after_sigil, None),
    };

    let room_id = Uuid::parse_str(uuid_part)
        .map_err(|e| UriParseError::InvalidRoomUuid(uuid_part.to_string(), e.to_string()))?;

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
        Some((p, n)) if !p.is_empty() && !n.is_empty() => (p, Some(NodeId(n.to_string()))),
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
    if matches!(env, Some(EnvironmentId::Wildcard)) {
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

/// Parse an env component (the part after `:` in URI authority) into
/// an [`EnvironmentId`]. Delegates to the type's own `From<&str>` impl
/// so the substrate has ONE place that decides what an env-string
/// means; the parser doesn't re-implement the wildcard rule.
fn parse_env_selector(s: &str) -> EnvironmentId {
    EnvironmentId::from(s)
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

// `Display for EnvironmentId` lives in `routing/environment.rs` — the
// single source of truth for the embodiment-axis Display contract.

impl FromStr for CommandUri {
    type Err = UriParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

/// Infallible `&str` -> [`CommandUri`] conversion for backwards-compatible
/// migration of `execute("path", params)` call sites. Any string that
/// fails strict URI parsing (e.g. malformed scheme) falls back to
/// [`CommandUri::Local`] — the dispatcher's downstream handler-not-found
/// path produces the typed error, not a parse error at the boundary.
///
/// Combined with `pub async fn execute(command: impl Into<CommandUri>, ...)`,
/// every existing `execute("ai/generate", params)` site keeps compiling
/// AND new code can pass `CommandUri::Peer { ... }` directly.
impl From<&str> for CommandUri {
    fn from(s: &str) -> Self {
        CommandUri::parse(s).unwrap_or_else(|_| CommandUri::local(s))
    }
}

impl From<String> for CommandUri {
    fn from(s: String) -> Self {
        CommandUri::from(s.as_str())
    }
}

impl From<&String> for CommandUri {
    fn from(s: &String) -> Self {
        CommandUri::from(s.as_str())
    }
}

// ─── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests;
