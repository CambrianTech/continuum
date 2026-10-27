//! How far this node trusts a caller: the one scale every command gate reads (the grid ACL,
//! a persona's tool surface, an activity's `who_may`). A session fact, so it lives in the
//! kernel beside `CallerIdentity` and `AuthPolicy`; the grid module only records which
//! node holds which level.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Trust level for a remote node.
/// Determines what commands the node is allowed to execute on us,
/// and what commands we're willing to send to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/grid/TrustLevel.ts")]
#[serde(rename_all = "lowercase")]
pub enum TrustLevel {
    /// Unknown node — not in our known_nodes list.
    /// No remote commands allowed.
    Blocked = 0,
    /// First contact — limited to read-only info commands.
    Provisional = 1,
    /// Established history of successful interactions.
    /// Can run compute commands (inference, training).
    Trusted = 2,
    /// Our own nodes (e.g., laptop + home tower).
    /// Full access to all non-local-only commands.
    Owner = 3,
}

impl Default for TrustLevel {
    fn default() -> Self {
        Self::Blocked
    }
}

impl std::str::FromStr for TrustLevel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "blocked" => Ok(Self::Blocked),
            "provisional" => Ok(Self::Provisional),
            "trusted" => Ok(Self::Trusted),
            "owner" => Ok(Self::Owner),
            _ => Err(format!("Invalid trust level: {s}. Use: blocked, provisional, trusted, owner")),
        }
    }
}
