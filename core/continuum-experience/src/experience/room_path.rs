//! A room's place in the ROOM TREE: its path, the room part of its URI.
//!
//! Every activity is a room, and rooms nest (docs/architecture/ACTIVITY-ROOM-PATTERNS.md, "The
//! room tree"): a benchmark run inside `academy/benchmarks`, a card's room inside its project's.
//! Following GRID-ADDRESSING-AND-ROUTING.md, a room is addressed as a URI whose authority is the
//! room and whose path segments are the rooms inside it: `airc://room:academy/benchmarks/run-1`.
//! This type is that path. The airc channel behind a room is an internal key derived from it
//! ([`RoomPath::channel_key`]), never a name anyone types.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// One path in the room tree: segments of `a-z 0-9 - _` joined by `/`, lower-cased.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(try_from = "String", into = "String")]
#[ts(export, export_to = "../../../protocol/typescript/experience/RoomPath.ts", type = "string")]
pub struct RoomPath(String);

/// Why a string is not a room path.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RoomPathError {
    #[error("a room path cannot be empty")]
    Empty,
    #[error("room path {path:?} has an empty segment (`a//b`)")]
    EmptySegment { path: String },
    #[error("room path {path:?} contains {found:?}: a segment holds only a-z 0-9 - _")]
    InvalidChar { path: String, found: char },
}

/// The namespace a nested room's channel key is derived in: the same path is the same room on
/// every node.
const ROOM_TREE_NAMESPACE: uuid::Uuid = uuid::Uuid::from_u128(0x5c0e_7a1d_95b4_4f6e_9d21_0a7b_3c8e_d104);

impl RoomPath {
    /// Parse a path (`academy/benchmarks/run-1`, with or without `airc://room:` or surrounding
    /// `/`), lower-cased.
    pub fn parse(text: &str) -> Result<Self, RoomPathError> {
        let text = text.trim();
        let text = text.strip_prefix("airc://room:").unwrap_or(text); // unwrap_or: a bare path has no scheme to strip
        let path = text.trim_matches('/').to_ascii_lowercase();
        if path.is_empty() {
            return Err(RoomPathError::Empty);
        }
        for segment in path.split('/') {
            if segment.is_empty() {
                return Err(RoomPathError::EmptySegment { path: path.clone() });
            }
            if let Some(found) = segment.chars().find(|c| !(c.is_ascii_alphanumeric() || *c == '-' || *c == '_')) {
                return Err(RoomPathError::InvalidChar { path: path.clone(), found });
            }
        }
        Ok(Self(path))
    }

    /// The room named `segment` inside this one.
    pub fn child(&self, segment: &str) -> Result<Self, RoomPathError> {
        Self::parse(&format!("{}/{}", self.0, segment.trim_matches('/')))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The rooms this one sits inside, outermost first (`academy`, `academy/benchmarks` for
    /// `academy/benchmarks/run-1`); empty for a top-level room.
    pub fn ancestors(&self) -> Vec<RoomPath> {
        let segments: Vec<&str> = self.0.split('/').collect();
        (1..segments.len()).map(|depth| Self(segments[..depth].join("/"))).collect()
    }

    /// The room's URI, per GRID-ADDRESSING-AND-ROUTING.md.
    pub fn uri(&self) -> String {
        format!("airc://room:{}", self.0)
    }

    /// The airc channel a nested room had before the room tree: its last segment alone. `None`
    /// for a top-level room, whose channel never changed. Read only to adopt a run opened before
    /// the tree, so opening it again resumes it instead of making a second, empty room.
    pub fn pre_tree_channel(&self) -> Option<&str> {
        self.0.rsplit_once('/').map(|(_, leaf)| leaf)
    }

    /// The airc channel name behind this room. A top-level room is its own name (`academy` is
    /// the room it always was); a nested room is its last segment and a key derived from the
    /// whole path, so two rooms with the same last segment in different places never meet.
    pub fn channel_key(&self) -> String {
        match self.0.rsplit_once('/') {
            None => self.0.clone(),
            Some((_, leaf)) => {
                let key = uuid::Uuid::new_v5(&ROOM_TREE_NAMESPACE, self.0.as_bytes()).simple().to_string();
                format!("{leaf}-{}", &key[..KEY_HEX])
            }
        }
    }
}

/// Hex digits of the path key in a nested room's channel name: 48 bits, so a collision needs
/// millions of rooms sharing one last segment.
// derived-or-floor: a floor on key length (48 bits against same-leaf collisions); not a tuned size.
const KEY_HEX: usize = 12;

impl TryFrom<String> for RoomPath {
    type Error = RoomPathError;
    fn try_from(text: String) -> Result<Self, RoomPathError> {
        Self::parse(&text)
    }
}

impl From<RoomPath> for String {
    fn from(path: RoomPath) -> Self {
        path.0
    }
}

impl std::fmt::Display for RoomPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a room path that is not a URI path, ancestors out of order, a nested room
    // whose channel collides with another place's, or a top-level room renamed under it.
    #[test]
    fn a_path_parses_walks_its_ancestors_and_keys_its_channel() {
        let run = RoomPath::parse("airc://room:Academy/Benchmarks/run-1/").expect("a path");
        assert_eq!(run.as_str(), "academy/benchmarks/run-1");
        assert_eq!(run.uri(), "airc://room:academy/benchmarks/run-1");
        assert_eq!(run.ancestors().iter().map(RoomPath::as_str).collect::<Vec<_>>(), vec!["academy", "academy/benchmarks"]);
        assert_eq!(RoomPath::parse("academy").expect("a path").channel_key(), "academy", "a top-level room keeps its name");
        let elsewhere = RoomPath::parse("projects/demo/run-1").expect("a path");
        assert!(run.channel_key().starts_with("run-1-") && run.channel_key() != elsewhere.channel_key());
        assert_eq!(run.channel_key(), RoomPath::parse("academy/benchmarks/run-1").expect("a path").channel_key(), "the same path is the same room");
        assert_eq!(RoomPath::parse("a//b"), Err(RoomPathError::EmptySegment { path: "a//b".into() }));
        assert!(matches!(RoomPath::parse("a/b.c"), Err(RoomPathError::InvalidChar { found: '.', .. })));
        assert_eq!(RoomPath::parse("/"), Err(RoomPathError::Empty));
    }
}
