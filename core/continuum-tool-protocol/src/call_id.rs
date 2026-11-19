//! [`ToolCallId`]: the id a tool call and its result share, never empty by construction.
//!
//! The failure this ends (the M5, 2026-11-19 06:21-06:41Z): a model template that names no call ids
//! (Qwen's) had its calls stored with empty ids by one parse path, and every act after refused the
//! stored receipt as "missing, duplicate, or mismatched native call/result IDs", on every build. The
//! id was a `String`, so an empty one was a valid value; each parse path decided on its own what a
//! missing id became (empty, a position, a random one) and a validator three layers down found out.
//! Now an id is minted where a call enters the system ([`ToolCallId::from_wire`] or
//! [`ToolCallId::mint`]), and a stored empty id fails to load rather than poisoning her history.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// The id of one tool call, shared by its result. Never empty: an id that crossed a wire empty is
/// refused at that wire ([`ToolCallId::from_wire`]), and one that never existed is minted.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, TS, schemars::JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/ai/ToolCallId.ts")]
pub struct ToolCallId(String);

/// An id that crossed a wire empty: the caller mints one or refuses the call, never stores it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmptyToolCallId;

impl std::fmt::Display for EmptyToolCallId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a tool call id is empty")
    }
}

impl std::error::Error for EmptyToolCallId {}

impl ToolCallId {
    /// A fresh id for a call whose source named none (a model template without call ids, a call the
    /// substrate composes): `call_` and a random UUID, as llama-server minted for its `/v1` reply.
    pub fn mint() -> Self {
        Self(format!("call_{}", uuid::Uuid::new_v4().simple()))
    }

    /// The id a wire carried, or `None` when it carried an empty one.
    pub fn from_wire(id: impl Into<String>) -> Option<Self> {
        let id = id.into();
        (!id.is_empty()).then_some(Self(id))
    }

    /// The id a wire carried, or a minted one when it carried none: the one rule every parse path
    /// applies, so no path decides it differently.
    pub fn from_wire_or_mint(id: Option<String>) -> Self {
        id.and_then(Self::from_wire).unwrap_or_else(Self::mint) // unwrap_or_else: a call the source named no id for gets a fresh one
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A test's named id (`ToolCallId::fixture("call-2")`), so a test can assert on the id it set.
/// Gated: production code mints or reads ids from a wire, never names them.
#[cfg(any(test, feature = "test-fixtures"))]
impl ToolCallId {
    #[allow(clippy::expect_used)] // a fixture's empty name is the test's own bug, refused at the test
    pub fn fixture(id: &str) -> Self {
        Self::from_wire(id).expect("test fixture: a tool call id is never empty")
    }
}

impl TryFrom<String> for ToolCallId {
    type Error = EmptyToolCallId;

    fn try_from(id: String) -> Result<Self, Self::Error> {
        Self::from_wire(id).ok_or(EmptyToolCallId)
    }
}

/// On the wire an id is its string.
impl Serialize for ToolCallId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

/// A stored or received empty id fails to load: it is never a valid id (see the module doc).
impl<'de> Deserialize<'de> for ToolCallId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let id = String::deserialize(deserializer)?;
        Self::try_from(id).map_err(serde::de::Error::custom)
    }
}

impl From<ToolCallId> for String {
    fn from(id: ToolCallId) -> Self {
        id.0
    }
}

impl std::fmt::Display for ToolCallId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// An id compares with the text a wire or a test names it by.
impl PartialEq<str> for ToolCallId {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

impl PartialEq<&str> for ToolCallId {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

impl PartialEq<String> for ToolCallId {
    fn eq(&self, other: &String) -> bool {
        &self.0 == other
    }
}

impl AsRef<str> for ToolCallId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: an empty call id stored and refused on every act after (the M5,
    // 2026-11-19). An empty id cannot be built, cannot be loaded, and a missing one is minted.
    #[test]
    fn an_empty_call_id_is_unrepresentable() {
        assert_eq!(ToolCallId::from_wire(""), None);
        assert!(serde_json::from_str::<ToolCallId>("\"\"").is_err(), "a stored empty id fails to load");
        let named = ToolCallId::from_wire_or_mint(Some("toolu_01".into()));
        assert_eq!(named.as_str(), "toolu_01", "a named id is kept");
        let minted = ToolCallId::from_wire_or_mint(Some(String::new()));
        assert!(minted.as_str().starts_with("call_") && minted.as_str().len() > 5);
        assert_ne!(ToolCallId::mint(), ToolCallId::mint(), "minted ids are unique");
        assert_eq!(serde_json::to_string(&named).ok().as_deref(), Some("\"toolu_01\""), "the wire is a plain string");
    }
}
