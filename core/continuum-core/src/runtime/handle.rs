//! Universal Handle System
//!
//! `Handle` is the universal correlation primitive — same idea as entity IDs,
//! file descriptors, texture IDs: a UUIDv4 that identifies and correlates one
//! piece of live work everywhere it travels.
//!
//! It lives in `runtime/` (not `live/`) because it is a substrate primitive,
//! not a media one — it sits beside [`crate::runtime::cell_shapes::HandleRef`]
//! (which is a `Handle` PLUS routing metadata: owning module, type tag, TTL)
//! and rides the [`crate::runtime::command_envelope::CommandRequest`] envelope.
//!
//! The two roles, kept distinct on purpose:
//! - `Handle` — the bare correlation TAG. Minted by a producer, echoed by
//!   events, passed back on cancel/status/resume. This is the uuid reused
//!   over and over, in and out.
//! - `HandleRef` — a `Handle` wrapped with the metadata the kernel needs to
//!   route a follow-up call back to the module that owns the live state.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

/// Universal correlation handle.
///
/// Used everywhere, in and out:
/// - Start operation → returns handle
/// - Events → tagged with handle
/// - Cancel/status/resume → use handle
///
/// Same concept as entity IDs in the data system. Wire format is the inner
/// UUID's canonical string serialization, so ts-rs sees it as `string`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/runtime/Handle.ts")]
pub struct Handle(#[ts(type = "string")] Uuid);

impl Handle {
    /// Create a new handle (generates UUIDv4)
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create from existing UUID (for caller-provided correlation)
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Get the underlying UUID
    pub fn as_uuid(&self) -> Uuid {
        self.0
    }

    /// Short form for logging (first 8 chars)
    pub fn short(&self) -> String {
        self.0.to_string()[..8].to_string()
    }
}

impl Default for Handle {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for Handle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Uuid> for Handle {
    fn from(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl From<Handle> for Uuid {
    fn from(handle: Handle) -> Self {
        handle.0
    }
}

/// Parse handle from string
impl std::str::FromStr for Handle {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handle_creation() {
        let h1 = Handle::new();
        let h2 = Handle::new();
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_handle_from_uuid() {
        let uuid = Uuid::new_v4();
        let handle = Handle::from_uuid(uuid);
        assert_eq!(handle.as_uuid(), uuid);
    }

    #[test]
    fn test_handle_short() {
        let handle = Handle::new();
        assert_eq!(handle.short().len(), 8);
    }

    // what this catches: the wire round-trip the correlation contract depends
    // on — a Handle must serialize as the plain uuid string (so it tags events
    // and rides envelopes interchangeably with a bare Uuid), not as a wrapped
    // object. Regression guard for the ts-rs `string` projection.
    #[test]
    fn serializes_as_bare_uuid_string() {
        let uuid = Uuid::new_v4();
        let handle = Handle::from_uuid(uuid);
        let json = serde_json::to_string(&handle).unwrap();
        assert_eq!(json, format!("\"{uuid}\""));
        let back: Handle = serde_json::from_str(&json).unwrap();
        assert_eq!(back, handle);
    }
}
