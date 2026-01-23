//! Universal Handle System
//!
//! Handle is the universal correlation primitive - same as entity IDs, file descriptors,
//! texture IDs. A UUID that identifies and correlates everything.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Universal correlation handle.
///
/// Used everywhere, in and out:
/// - Start operation → returns handle
/// - Events → tagged with handle
/// - Cancel/status/resume → use handle
///
/// Same concept as entity IDs in data system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Handle(Uuid);

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
}
